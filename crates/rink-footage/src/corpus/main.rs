//! Build the detector training corpus on the GPU worker.
//!
//! `rink-corpus <command> [options] < sources.json` prints one JSON summary on stdout and logs
//! progress on stderr. Every command skips work already on disk, so reruns are safe. Layout
//! under `$RINK_CORPUS` (default `~/rink-corpus`):
//!
//!   models/<name>.pt                     detector checkpoints named in sources
//!   raw/youtube/<source>/<id>.mp4        downloaded games, with yt-dlp's .info.json beside them
//!   datasets/<name>/                     public labeled datasets, unpacked
//!   frames/<video>/<frame>.jpg           sampled frames at source resolution
//!   labels/<video>/<frame>.txt           draft boxes, YOLO format, in the model's class order
//!   manifest.jsonl                       one line per labeled frame
//!   cvat/<video>.zip                     draft boxes of a video's on-ice frames, for a CVAT task
//!   reviewed/<video>/<frame>.txt         boxes a person corrected in CVAT, in classes.txt order

use std::{
    collections::BTreeMap,
    env,
    fs::{self, File, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, ensure, Context, Result};
use rink_footage::yolo;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const USAGE: &str =
    "Usage: rink-corpus <sync|sample|prelabel|status|cvat-export|cvat-review> [--key value ...] < input";
const FORMAT: &str = "bv[height<=1080][ext=mp4][vcodec^=avc1]/b[height<=1080][ext=mp4]";
const PEOPLE: [&str; 4] = ["player", "goalie", "goaltender", "referee"];

#[derive(Deserialize, Default)]
struct Sources {
    #[serde(default)]
    youtube: Vec<Channel>,
    #[serde(default)]
    datasets: Vec<Download>,
    #[serde(default)]
    models: Vec<Download>,
}

#[derive(Deserialize)]
struct Channel {
    name: String,
    url: String,
    max_videos: Option<u32>,
    min_minutes: Option<u32>,
    /// Case-insensitive regex a title must match, for channels that mix games with other videos.
    title: Option<String>,
}

/// A dataset or model fetched once from `url`; entries without one are fetched by hand.
#[derive(Deserialize)]
struct Download {
    name: String,
    url: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    frame: String,
    labels: String,
    video: String,
    on_ice: bool,
    reviewed: bool,
    boxes: BTreeMap<String, u32>,
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().context(USAGE)?;
    let options = parse_options(args)?;
    // stdin holds the sources JSON, or for cvat-review the zip CVAT exported.
    let mut input = Vec::new();
    if !io::stdin().is_terminal() {
        io::stdin().read_to_end(&mut input)?;
    }
    let sources = || -> Result<Sources> {
        if input.trim_ascii().is_empty() {
            Ok(Sources::default())
        } else {
            serde_json::from_slice(&input).context("reading sources JSON on stdin")
        }
    };
    let root = env::var_os("RINK_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var("HOME").unwrap_or_default()).join("rink-corpus"));
    fs::create_dir_all(&root)?;
    let corpus = Corpus { root, options };
    let summary = match command.as_str() {
        "sync" => corpus.sync(&sources()?)?,
        "sample" => corpus.sample()?,
        "prelabel" => corpus.prelabel()?,
        "status" => corpus.status(&sources()?)?,
        "cvat-export" => corpus.cvat_export()?,
        "cvat-review" => corpus.cvat_review(&input)?,
        _ => bail!(USAGE),
    };
    println!("{summary}");
    Ok(())
}

fn parse_options(args: impl Iterator<Item = String>) -> Result<BTreeMap<String, String>> {
    let args: Vec<String> = args.collect();
    ensure!(args.len() % 2 == 0, USAGE);
    args.chunks(2).map(|pair| Ok((pair[0].strip_prefix("--").context(USAGE)?.to_owned(), pair[1].clone()))).collect()
}

struct Corpus {
    root: PathBuf,
    options: BTreeMap<String, String>,
}

impl Corpus {
    fn option<T: std::str::FromStr>(&self, key: &str, default: T) -> Result<T> {
        match self.options.get(key) {
            Some(value) => value.parse().ok().with_context(|| format!("invalid --{key} {value}")),
            None => Ok(default),
        }
    }

    fn sync(&self, sources: &Sources) -> Result<Value> {
        let archive = self.root.join("raw/archive.txt");
        let mut downloaded = BTreeMap::new();
        let mut failed = BTreeMap::new();
        for channel in &sources.youtube {
            let folder = self.root.join("raw/youtube").join(&channel.name);
            fs::create_dir_all(&folder)?;
            let before = stems(&folder, "mp4")?;
            let max = channel.max_videos.unwrap_or(self.option("max-videos", 10)?);
            log(format!("sync {}: {}", channel.name, channel.url));
            let output = Command::new("yt-dlp")
                .args(["--ignore-config", "--no-warnings", "--quiet", "--no-progress", "--format", FORMAT])
                .arg("--download-archive")
                .arg(&archive)
                .args(["--match-filters", &match_filter(channel)])
                .args(["--write-info-json", "--no-write-playlist-metafiles", "--max-downloads", &max.to_string()])
                .args(["--sleep-interval", "5", "--max-sleep-interval", "20", "--retries", "3", "--output"])
                .arg(folder.join("%(id)s.%(ext)s"))
                .args(["--", &channel.url])
                .output()
                .context("running yt-dlp")?;
            // 101 means --max-downloads was reached, the normal stop for a capped run.
            if !matches!(output.status.code(), Some(0 | 101)) {
                let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                log(format!("sync {} failed: {error}", channel.name));
                failed.insert(channel.name.clone(), error);
            }
            let after = stems(&folder, "mp4")?;
            downloaded.insert(channel.name.clone(), after.difference(&before).cloned().collect::<Vec<_>>());
        }
        let fetched_datasets = self.fetch_all(&sources.datasets, "datasets", true)?;
        let fetched_models = self.fetch_all(&sources.models, "models", false)?;
        Ok(
            json!({ "downloaded": downloaded, "failed": failed, "fetched_datasets": fetched_datasets, "fetched_models": fetched_models }),
        )
    }

    /// Fetch each download once. Datasets arrive as zips and are unpacked; models stay as `<name>.pt`.
    fn fetch_all(&self, downloads: &[Download], folder: &str, unpack: bool) -> Result<Vec<String>> {
        let mut fetched = Vec::new();
        for download in downloads {
            let Some(url) = &download.url else { continue };
            let target = self.root.join(folder).join(if unpack {
                download.name.clone()
            } else {
                format!("{}.pt", download.name)
            });
            if target.exists() {
                continue;
            }
            fs::create_dir_all(target.parent().expect("target is under root"))?;
            log(format!("fetch {folder}/{}", download.name));
            let partial = target.with_extension("part");
            let status = Command::new("curl").args(["-fsSL", "--retry", "3", "-o"]).arg(&partial).arg(url).status()?;
            ensure!(status.success(), "downloading {url} failed");
            if unpack {
                zip::ZipArchive::new(File::open(&partial)?)?.extract(&target)?;
                fs::remove_file(&partial)?;
            } else {
                fs::rename(&partial, &target)?;
            }
            fetched.push(download.name.clone());
        }
        Ok(fetched)
    }

    fn sample(&self) -> Result<Value> {
        let every: f64 = self.option("every", 4.0)?;
        let burst: usize = self.option("burst", 3)?;
        let min_change: f64 = self.option("min-change", 0.08)?;
        let mut sampled = BTreeMap::new();
        for video in files(&self.root.join("raw/youtube"), "mp4", 2)? {
            let stem = video.file_stem().expect("mp4 has a stem").to_string_lossy().into_owned();
            let out = self.root.join("frames").join(&stem);
            if out.exists() {
                continue;
            }
            let partial = out.with_extension("part");
            if partial.exists() {
                fs::remove_dir_all(&partial)?;
            }
            fs::create_dir_all(&partial)?;
            let step = ((fps(&video)? * every).round() as usize).max(burst + 1);
            // A burst of consecutive frames per step: puck labelers need neighbours to see it move.
            let status = Command::new("ffmpeg")
                .args(["-v", "error", "-nostdin", "-i"])
                .arg(&video)
                .args([
                    "-vf",
                    &format!("select='lt(mod(n\\,{step})\\,{burst})'"),
                    "-fps_mode",
                    "passthrough",
                    "-frame_pts",
                    "1",
                    "-q:v",
                    "2",
                ])
                .arg(partial.join("%08d.jpg"))
                .status()?;
            ensure!(status.success(), "ffmpeg failed on {}", video.display());
            let kept = drop_still_bursts(&partial, burst, min_change)?;
            fs::rename(&partial, &out)?;
            log(format!("sample {stem}: {kept} frames"));
            sampled.insert(stem, kept);
        }
        Ok(json!({ "sampled": sampled, "frames_total": files(&self.root.join("frames"), "jpg", 2)?.len() }))
    }

    fn prelabel(&self) -> Result<Value> {
        let model_path =
            self.options.get("model").map(PathBuf::from).unwrap_or_else(|| self.root.join("models/hockeyai.pt"));
        let size: u32 = self.option("size", 1280)?;
        let confidence: f32 = self.option("confidence", 0.3)?;
        let min_people: usize = self.option("min-people", 2)?;
        let model = yolo::Yolo::load(&model_path, yolo::device()?)?;
        let puck = model.names.iter().position(|n| n == "puck");
        // Pucks are tiny and rarely confident; a lower bar shows labelers more candidates.
        let bar = |class: usize| if Some(class) == puck { confidence / 2.0 } else { confidence };

        let mut manifest = OpenOptions::new().create(true).append(true).open(self.root.join("manifest.jsonl"))?;
        let mut by_class: BTreeMap<&str, u32> = model.names.iter().map(|n| (n.as_str(), 0)).collect();
        let (mut labeled, mut off_ice) = (0, 0);
        for frame in files(&self.root.join("frames"), "jpg", 2)? {
            let video = frame
                .parent()
                .and_then(Path::file_name)
                .expect("frame sits in a video folder")
                .to_string_lossy()
                .into_owned();
            let label = self
                .root
                .join("labels")
                .join(&video)
                .join(frame.with_extension("txt").file_name().expect("frame has a name"));
            if label.exists() {
                continue;
            }
            let image = image::open(&frame).with_context(|| format!("reading {}", frame.display()))?.to_rgb8();
            let (width, height) = (image.width() as f32, image.height() as f32);
            let found = model.detect(&image, size, &bar)?;
            let mut text = String::new();
            let mut boxes = BTreeMap::new();
            for d in &found {
                let [x0, y0, x1, y1] = d.xyxy;
                text += &format!(
                    "{} {:.6} {:.6} {:.6} {:.6}\n",
                    d.class,
                    (x0 + x1) / 2.0 / width,
                    (y0 + y1) / 2.0 / height,
                    (x1 - x0) / width,
                    (y1 - y0) / height
                );
                *boxes.entry(model.names[d.class].clone()).or_insert(0) += 1;
                *by_class.get_mut(model.names[d.class].as_str()).expect("class from model") += 1;
            }
            let people: u32 = PEOPLE.iter().filter_map(|p| boxes.get(*p)).sum();
            let on_ice = people as usize >= min_people;
            fs::create_dir_all(label.parent().expect("label has a folder"))?;
            fs::write(&label, text)?;
            let entry = Entry {
                frame: relative(&self.root, &frame),
                labels: relative(&self.root, &label),
                video,
                on_ice,
                reviewed: false,
                boxes,
            };
            writeln!(manifest, "{}", serde_json::to_string(&entry)?)?;
            labeled += 1;
            off_ice += usize::from(!on_ice);
            if labeled % 500 == 0 {
                log(format!("prelabel: {labeled} frames"));
            }
        }
        // classes.txt makes the label folder readable by YOLO tooling and labeling apps.
        fs::create_dir_all(self.root.join("labels"))?;
        fs::write(self.root.join("labels/classes.txt"), model.names.join("\n") + "\n")?;
        Ok(json!({ "labeled": labeled, "off_ice": off_ice, "boxes_by_class": by_class, "classes": model.names }))
    }

    fn entries(&self) -> Result<Vec<Entry>> {
        match fs::read_to_string(self.root.join("manifest.jsonl")) {
            Ok(text) => Ok(text.lines().map(serde_json::from_str).collect::<Result<_, _>>()?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn classes(&self) -> Result<Vec<String>> {
        let text = fs::read_to_string(self.root.join("labels/classes.txt"))
            .context("no labels/classes.txt; run prelabel first")?;
        Ok(text.lines().map(str::to_owned).collect())
    }

    /// Pack each video's on-ice frames and draft boxes as a YOLO 1.1 zip, the format CVAT imports.
    /// Frame paths are relative to frames/, which CVAT mounts as its shared folder.
    fn cvat_export(&self) -> Result<Value> {
        let classes = self.classes()?;
        let only: Option<Vec<&str>> = self.options.get("videos").map(|v| v.split(',').collect());
        let mut by_video: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
        for entry in self.entries()?.into_iter().filter(|e| e.on_ice) {
            if only.as_ref().is_none_or(|only| only.contains(&entry.video.as_str())) {
                by_video.entry(entry.video.clone()).or_default().push(entry);
            }
        }
        let options = zip::write::SimpleFileOptions::default();
        let mut videos = BTreeMap::new();
        fs::create_dir_all(self.root.join("cvat"))?;
        for (video, entries) in by_video {
            let path = self.root.join("cvat").join(format!("{video}.zip"));
            let mut zip = zip::ZipWriter::new(File::create(&path)?);
            zip.start_file("obj.names", options)?;
            zip.write_all((classes.join("\n") + "\n").as_bytes())?;
            zip.start_file("obj.data", options)?;
            write!(zip, "classes = {}\nnames = data/obj.names\ntrain = data/train.txt\n", classes.len())?;
            let frames: Vec<String> =
                entries.iter().map(|e| e.frame.strip_prefix("frames/").unwrap_or(&e.frame).to_owned()).collect();
            zip.start_file("train.txt", options)?;
            zip.write_all(frames.iter().map(|f| format!("data/obj_train_data/{f}\n")).collect::<String>().as_bytes())?;
            for (entry, frame) in entries.iter().zip(&frames) {
                zip.start_file(
                    format!("obj_train_data/{}", Path::new(frame).with_extension("txt").display()),
                    options,
                )?;
                zip.write_all(&fs::read(self.root.join(&entry.labels))?)?;
            }
            zip.finish()?;
            videos.insert(video, json!({ "zip": relative(&self.root, &path), "frames": frames }));
        }
        Ok(json!({ "classes": classes, "videos": videos }))
    }

    /// Store the boxes from a CVAT YOLO 1.1 export (the zip on stdin) as reviewed labels, mapping
    /// CVAT's class order onto classes.txt by name, and mark those frames reviewed in the manifest.
    fn cvat_review(&self, zip: &[u8]) -> Result<Value> {
        let classes = self.classes()?;
        let mut archive = zip::ZipArchive::new(io::Cursor::new(zip)).context("reading the CVAT export zip on stdin")?;
        let mut names = String::new();
        archive.by_name("obj.names").context("export has no obj.names")?.read_to_string(&mut names)?;
        let to_ours: Vec<usize> = names
            .lines()
            .map(|n| {
                classes.iter().position(|c| c == n).with_context(|| format!("CVAT label {n} is not in classes.txt"))
            })
            .collect::<Result<_>>()?;
        let mut reviewed = std::collections::BTreeSet::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let Some(name) =
                file.name()?.strip_prefix("obj_train_data/").filter(|n| n.ends_with(".txt")).map(str::to_owned)
            else {
                continue;
            };
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            let mut out = String::new();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let (class, rest) = line.split_once(' ').with_context(|| format!("bad line in {name}: {line}"))?;
                let class: usize = class.parse()?;
                out += &format!(
                    "{} {rest}\n",
                    to_ours.get(class).with_context(|| format!("class {class} out of range in {name}"))?
                );
            }
            let target = self.root.join("reviewed").join(&name);
            fs::create_dir_all(target.parent().expect("reviewed label has a folder"))?;
            fs::write(&target, out)?;
            reviewed.insert(format!("frames/{}", Path::new(&name).with_extension("jpg").display()));
        }
        let mut entries = self.entries()?;
        for entry in &mut entries {
            entry.reviewed |= reviewed.contains(&entry.frame);
        }
        let partial = self.root.join("manifest.jsonl.part");
        fs::write(&partial, entries.iter().map(|e| Ok(serde_json::to_string(e)? + "\n")).collect::<Result<String>>()?)?;
        fs::rename(&partial, self.root.join("manifest.jsonl"))?;
        Ok(json!({ "reviewed": reviewed.len(), "reviewed_total": entries.iter().filter(|e| e.reviewed).count() }))
    }

    fn status(&self, sources: &Sources) -> Result<Value> {
        let entries = self.entries()?;
        let mut seconds = 0.0;
        for info in files(&self.root.join("raw/youtube"), "json", 2)?
            .into_iter()
            .filter(|p| p.to_string_lossy().ends_with(".info.json"))
        {
            let info: Value = serde_json::from_str(&fs::read_to_string(info)?)?;
            seconds += info["duration"].as_f64().unwrap_or(0.0);
        }
        let datasets = match fs::read_dir(self.root.join("datasets")) {
            Ok(dir) => dir
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        };
        Ok(json!({
            "root": self.root,
            "sources": { "youtube": sources.youtube.len(), "datasets": sources.datasets.len() },
            "videos": files(&self.root.join("raw/youtube"), "mp4", 2)?.len(),
            "hours": (seconds / 36.0).round() / 100.0,
            "datasets": datasets,
            "frames": files(&self.root.join("frames"), "jpg", 2)?.len(),
            "labeled": entries.len(),
            "on_ice": entries.iter().filter(|e| e.on_ice).count(),
            "reviewed": entries.iter().filter(|e| e.reviewed).count(),
            "frames_with_puck": entries.iter().filter(|e| e.boxes.contains_key("puck")).count(),
            "bytes": disk_usage(&self.root)?,
        }))
    }
}

/// Remove each burst whose first frame looks like the last kept burst (stoppages, static shots).
fn drop_still_bursts(folder: &Path, burst: usize, min_change: f64) -> Result<usize> {
    let mut frames = files(folder, "jpg", 1)?;
    frames.sort_by_key(|p| p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u64>().ok()));
    let mut previous: Option<Vec<bool>> = None;
    let mut kept = 0;
    for group in frames.chunks(burst) {
        let print = thumbprint(&group[0])?;
        let changed = previous
            .as_ref()
            .map_or(1.0, |p| p.iter().zip(&print).filter(|(a, b)| a != b).count() as f64 / print.len() as f64);
        if changed < min_change {
            group.iter().try_for_each(fs::remove_file)?;
            continue;
        }
        previous = Some(print);
        kept += group.len();
    }
    Ok(kept)
}

fn thumbprint(path: &Path) -> Result<Vec<bool>> {
    let small = image::open(path)?.resize_exact(16, 9, image::imageops::FilterType::Triangle).to_luma8();
    let mean = small.pixels().map(|p| p.0[0] as f64).sum::<f64>() / 144.0;
    Ok(small.pixels().map(|p| p.0[0] as f64 > mean).collect())
}

fn fps(video: &Path) -> Result<f64> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=avg_frame_rate", "-of", "csv=p=0"])
        .arg(video)
        .output()?;
    ensure!(output.status.success(), "ffprobe failed on {}", video.display());
    let text = String::from_utf8(output.stdout)?;
    let (num, den) = text.trim().split_once('/').unwrap_or((text.trim(), "1"));
    let fps = num.parse::<f64>()? / den.parse::<f64>()?;
    ensure!(fps.is_finite() && fps > 0.0, "{} has no frame rate", video.display());
    Ok(fps)
}

/// Files with `extension` at exactly `depth` levels below `dir`; missing dirs are empty.
fn files(dir: &Path, extension: &str, depth: usize) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut found = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if depth > 1 && path.is_dir() && path.extension().is_none_or(|e| e != "part") {
            found.extend(files(&path, extension, depth - 1)?);
        } else if depth == 1 && path.extension().is_some_and(|e| e == extension) {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn stems(dir: &Path, extension: &str) -> Result<std::collections::BTreeSet<String>> {
    Ok(files(dir, extension, 1)?
        .iter()
        .filter_map(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .collect())
}

fn disk_usage(dir: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        total += if meta.is_dir() { disk_usage(&entry.path())? } else { meta.len() };
    }
    Ok(total)
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned()
}

fn log(message: String) {
    eprintln!("{message}");
}

fn match_filter(channel: &Channel) -> String {
    let mut filter = format!("!is_live & duration >= {}", channel.min_minutes.unwrap_or(5) * 60);
    if let Some(title) = &channel.title {
        filter += &format!(" & title ~= '(?i){}'", title.replace('\'', "\\'"));
    }
    filter
}
