//! A session's film room: one feedback film from the clips the user is marked in.
//!
//! `rink-track-preview --feedback <work-dir> <font.ttf>` reads, for each clip in `job.json`,
//! `<clip>/source.mp4` (the upload), `<clip>/frames/*.jpg` (the 15 fps tracking frames, named by
//! source time), `<clip>/tracks.ndjson` and `<clip>/focus.json` (the "this is me" anchors). It shows
//! Claude sampled frames with the user boxed, gets back a few moments with cues, and checks them
//! against the clips. The film carries annotations only: each moment is cut from the upload at up to
//! 1920 px and 30 fps, with the user boxed, and holds on its key frame. The cues go to
//! `out/feedback.json` for the page beside the film, and the film to `out/video.mp4`. Claude is
//! reached through the API with `ANTHROPIC_API_KEY`, or through the Claude Code CLI at
//! `RINK_FEEDBACK_CLAUDE`;
//! `RINK_FEEDBACK_PLAN` names a saved answer to render instead.
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs::{self, File},
    io::{BufRead, BufReader, Cursor, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    time::Duration,
};

use ab_glyph::FontArc;
use anyhow::{bail, ensure, Context, Result};
use base64::Engine;
use image::{Rgb, RgbImage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{overlay::rect_label, tracker::Point};

const USAGE: &str = "Usage: rink-track-preview --feedback <work-dir> <font.ttf>";
const FPS: u32 = 15;
const YOU: Rgb<u8> = Rgb([255, 214, 0]);
/// Images sent to Claude across all clips; frames are 720 px wide, about 400 tokens each.
const MAX_SAMPLES: usize = 120;
/// The film: long edge in pixels, frame rate, and how long it holds on each moment's key frame.
const LONG_EDGE: u32 = 1920;
const FILM_FPS: u32 = 30;
const HOLD_SECONDS: u32 = 2;

const SYSTEM: &str = "You are an ice hockey coach reviewing an adult recreational player's game footage, \
filmed on a phone from the stands. The player you are coaching is boxed in yellow in every image. \
Their league plays without set team tactics, so judge how well they read what the other skaters \
actually do: where they are relative to the puck, the puck carrier, teammates and opponents; whether \
they support, give an outlet, close a gap or open a lane; how they skate to space. Use plain language \
and explain any hockey term you use. Comment only on what the frames show. If the view is blocked or \
unclear at a moment, skip it rather than guess. If the player's note asks about something, focus on it.";

#[derive(Deserialize)]
struct Job {
    #[serde(default)]
    note: String,
    clips: Vec<JobClip>,
}

#[derive(Deserialize)]
struct JobClip {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Frame {
    source_ms: u64,
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct Track {
    id: u32,
    xyxy: [f32; 4],
}

#[derive(Deserialize)]
struct Anchor {
    source_ms: u64,
    track_id: u32,
}

struct Clip {
    name: String,
    /// The original upload, cut for the film at full quality.
    source: PathBuf,
    /// Width and height of the tracking frames, which the track boxes are measured in.
    tracked: (u32, u32),
    /// Frame path and source time, in time order.
    frames: Vec<(PathBuf, u64)>,
    you: BTreeMap<u64, [f32; 4]>,
    duration_ms: u64,
}

#[derive(Deserialize)]
struct Plan {
    summary: String,
    moments: Vec<PlannedMoment>,
}

#[derive(Deserialize)]
struct PlannedMoment {
    clip: usize,
    start_s: f64,
    end_s: f64,
    freeze_s: f64,
    title: String,
    observation: String,
    suggestion: String,
}

#[derive(Serialize)]
struct Moment {
    clip: String,
    clip_name: String,
    start_ms: u64,
    end_ms: u64,
    freeze_ms: u64,
    title: String,
    observation: String,
    suggestion: String,
    /// Where the moment starts in the feedback video, for seeking.
    video_ms: u64,
}

pub fn run(args: &[OsString]) -> Result<()> {
    let [work, font] = args else { bail!(USAGE) };
    let work = PathBuf::from(work);
    let font = FontArc::try_from_vec(fs::read(font)?).context("invalid font")?;
    let job: Job = serde_json::from_slice(&fs::read(work.join("job.json"))?).context("reading job.json")?;
    ensure!(!job.clips.is_empty(), "the job has no clips");
    let clips = job.clips.iter().map(|clip| load(&work.join(&clip.id), &clip.name)).collect::<Result<Vec<_>>>()?;
    for clip in &clips {
        ensure!(!clip.you.is_empty(), "you are not in view anywhere in {}", clip.name);
    }

    // RINK_FEEDBACK_PLAN renders a saved plan (Claude's JSON answer) again without calling the API.
    let plan = match env::var_os("RINK_FEEDBACK_PLAN") {
        Some(path) => serde_json::from_slice(&fs::read(path)?).context("reading the saved plan")?,
        None => ask_claude(&job.note, &clips, &work)?,
    };
    let moments = check(&plan, &clips)?;
    let out = work.join("out");
    fs::create_dir_all(&out)?;
    let moments = render(moments, &plan, &job, &clips, &font, &work, &out.join("video.mp4"))?;
    let feedback = json!({ "summary": plan.summary, "moments": moments });
    fs::write(out.join("feedback.json"), serde_json::to_vec_pretty(&feedback)?)?;
    println!("Feedback video with {} moments", feedback["moments"].as_array().map_or(0, Vec::len));
    Ok(())
}

fn load(dir: &Path, name: &str) -> Result<Clip> {
    let mut frames: Vec<(PathBuf, u64)> = fs::read_dir(dir.join("frames"))
        .with_context(|| format!("reading frames of {name}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "jpg"))
        .filter_map(|path| {
            let ms = path.file_stem()?.to_str()?.rsplit('-').next()?.parse().ok()?;
            Some((path, ms))
        })
        .collect();
    frames.sort_by_key(|(_, ms)| *ms);
    ensure!(!frames.is_empty(), "{name} has no frames");

    let mut anchors: Vec<Anchor> =
        serde_json::from_slice(&fs::read(dir.join("focus.json"))?).context("reading focus.json")?;
    anchors.sort_by_key(|anchor| anchor.source_ms);
    ensure!(!anchors.is_empty(), "you are not marked in {name}");
    let mut you = BTreeMap::new();
    for line in BufReader::new(File::open(dir.join("tracks.ndjson"))?).lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        // An anchor says which track is you from its frame on; before the first one, use the first.
        let track = anchors.iter().rev().find(|a| a.source_ms <= frame.source_ms).unwrap_or(&anchors[0]).track_id;
        if let Some(found) = frame.tracks.iter().find(|t| t.id == track) {
            you.insert(frame.source_ms, found.xyxy);
        }
    }
    let duration_ms = frames.last().map_or(0, |(_, ms)| *ms);
    let source = dir.join("source.mp4");
    ensure!(source.is_file(), "{name} has no source video");
    let tracked = image::image_dimensions(&frames[0].0)?;
    Ok(Clip { name: name.to_owned(), source, tracked, frames, you, duration_ms })
}

/// Evenly spaced frames where the user is in view, at most `MAX_SAMPLES` across all clips.
fn samples(clips: &[Clip]) -> Vec<(usize, &(PathBuf, u64))> {
    let visible: Vec<(usize, &(PathBuf, u64))> = clips
        .iter()
        .enumerate()
        .flat_map(|(i, clip)| clip.frames.iter().filter(|(_, ms)| clip.you.contains_key(ms)).map(move |f| (i, f)))
        .collect();
    let seconds = visible.len() / FPS as usize;
    let every = (seconds.div_ceil(MAX_SAMPLES)).max(1) * FPS as usize;
    visible.into_iter().step_by(every).collect()
}

/// What Claude is shown: text and boxed frames, in order.
enum Part {
    Text(String),
    Image(Vec<u8>),
}

fn prompt(note: &str, clips: &[Clip]) -> Result<Vec<Part>> {
    let listing: String = clips
        .iter()
        .enumerate()
        .map(|(i, clip)| format!("Clip {i}: {} ({:.1} s long)\n", clip.name, clip.duration_ms as f64 / 1000.0))
        .collect();
    let mut parts = vec![Part::Text(format!(
        "These are frames from {} clip(s), sampled about once a second where I am in view. I am boxed in yellow.\n{listing}",
        clips.len()
    ))];
    for (i, (path, ms)) in samples(clips) {
        let mut image = image::open(path)?.to_rgb8();
        if let Some(&xyxy) = clips[i].you.get(ms) {
            outline(&mut image, xyxy, YOU, 3, None);
        }
        let mut jpeg = Vec::new();
        image::DynamicImage::ImageRgb8(image).write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)?;
        parts.push(Part::Text(format!("Clip {i} at {:.1} s:", *ms as f64 / 1000.0)));
        parts.push(Part::Image(jpeg));
    }
    let ask = if note.is_empty() { String::new() } else { format!("What I want feedback on: {note}\n\n") };
    parts.push(Part::Text(format!(
        "{ask}Pick the 4 to 8 moments most worth learning from. Cover as many different clips as have something worth \
learning, rather than several moments from one clip. For each, give the clip number, a window of 4 to 12 seconds (start_s, end_s, in clip seconds), the second \
to freeze on (freeze_s, the key decision, inside the window), a title of at most 6 words, what I did \
(observation) and what to try next time (suggestion), each one or two sentences. Then a two-sentence \
summary of the main pattern across the clips. In the text, name clips by their file name \
(like IMG_2911), never by clip number."
    )));
    Ok(parts)
}

fn schema() -> Value {
    let moment = json!({
        "type": "object",
        "properties": {
            "clip": { "type": "integer" }, "start_s": { "type": "number" }, "end_s": { "type": "number" },
            "freeze_s": { "type": "number" }, "title": { "type": "string" },
            "observation": { "type": "string" }, "suggestion": { "type": "string" }
        },
        "required": ["clip", "start_s", "end_s", "freeze_s", "title", "observation", "suggestion"],
        "additionalProperties": false
    });
    json!({
        "type": "object",
        "properties": { "summary": { "type": "string" }, "moments": { "type": "array", "items": moment } },
        "required": ["summary", "moments"],
        "additionalProperties": false
    })
}

/// Ask Claude through the API when `ANTHROPIC_API_KEY` is set, otherwise through the Claude Code
/// CLI at `RINK_FEEDBACK_CLAUDE`, which uses that machine's own Claude login.
fn ask_claude(note: &str, clips: &[Clip], work: &Path) -> Result<Plan> {
    let parts = prompt(note, clips)?;
    if let Ok(key) = env::var("ANTHROPIC_API_KEY") {
        return ask_api(&parts, &key);
    }
    if let Some(cli) = env::var_os("RINK_FEEDBACK_CLAUDE") {
        return ask_claude_code(&parts, Path::new(&cli), work);
    }
    bail!("Set ANTHROPIC_API_KEY or RINK_FEEDBACK_CLAUDE on the worker")
}

fn ask_api(parts: &[Part], key: &str) -> Result<Plan> {
    let model = env::var("RINK_FEEDBACK_MODEL").unwrap_or_else(|_| "claude-opus-5-5".into());
    let content: Vec<Value> = parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => json!({ "type": "text", "text": text }),
            Part::Image(jpeg) => json!({ "type": "image", "source": {
                "type": "base64", "media_type": "image/jpeg",
                "data": base64::engine::general_purpose::STANDARD.encode(jpeg)
            }}),
        })
        .collect();
    let body = json!({
        "model": model,
        "max_tokens": 16000,
        "system": SYSTEM,
        "output_config": { "effort": "high", "format": { "type": "json_schema", "schema": schema() } },
        "fallbacks": "default",
        "messages": [{ "role": "user", "content": content }]
    });
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(600)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2026-10-21")
        .header("anthropic-beta", "server-side-fallback-2026-07-01")
        .send_json(&body)
        .context("calling the Claude API")?;
    let status = response.status();
    let reply: Value = response.body_mut().read_json().context("reading the Claude API reply")?;
    if !status.is_success() {
        bail!("Claude API returned {status}: {}", reply["error"]["message"].as_str().unwrap_or("no message"));
    }
    match reply["stop_reason"].as_str() {
        Some("refusal") => bail!("Claude declined to review these clips"),
        Some("max_tokens") => bail!("Claude's answer was cut off"),
        _ => {}
    }
    let text = reply["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
        .and_then(|block| block["text"].as_str())
        .context("Claude's reply has no text")?;
    serde_json::from_str(text).context("Claude's reply is not the expected JSON")
}

/// Run Claude Code headless in the work folder. The frames are written to `samples/` and the
/// prompt names them in order; Claude Code may only read files.
fn ask_claude_code(parts: &[Part], cli: &Path, work: &Path) -> Result<Plan> {
    let samples = work.join("samples");
    fs::create_dir_all(&samples)?;
    let mut text = format!("{SYSTEM}\n\nRead every image file named below, in order, before answering.\n\n");
    let mut count = 0;
    for part in parts {
        match part {
            Part::Text(line) => text += &format!("{line}\n"),
            Part::Image(jpeg) => {
                let name = format!("samples/{count:03}.jpg");
                fs::write(work.join(&name), jpeg)?;
                text += &format!("{name}\n");
                count += 1;
            }
        }
    }
    let mut child = Command::new(cli)
        .current_dir(work)
        .args(["-p", "--output-format", "json", "--model", "opus", "--effort", "high", "--strict-mcp-config"])
        .args(["--allowedTools", "Read", "--disallowedTools", "Bash,Edit,Write,WebFetch,WebSearch"])
        .arg("--json-schema")
        .arg(schema().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {}", cli.display()))?;
    child.stdin.take().context("no stdin for Claude Code")?.write_all(text.as_bytes())?;
    let output = child.wait_with_output()?;
    let reply: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!("Claude Code did not answer in JSON: {}", String::from_utf8_lossy(&output.stderr).trim())
    })?;
    if reply["is_error"].as_bool() == Some(true) || !output.status.success() {
        bail!("Claude Code failed: {}", reply["result"].as_str().unwrap_or("no message"));
    }
    let structured = &reply["structured_output"];
    if structured.is_object() {
        return serde_json::from_value(structured.clone()).context("Claude Code's answer is not the expected JSON");
    }
    let result = reply["result"].as_str().context("Claude Code's reply has no result")?;
    let json = &result[result.find('{').unwrap_or(0)..=result.rfind('}').unwrap_or(result.len() - 1)];
    serde_json::from_str(json).context("Claude Code's answer is not the expected JSON")
}

/// A moment that fits its clip, with times in clip milliseconds.
struct Window {
    clip: usize,
    start: u64,
    end: u64,
    freeze: u64,
    /// Index into the plan's moments.
    planned: usize,
}

/// Keep moments that fit their clip, with the window clamped to 4–15 s and the freeze inside it.
fn check(plan: &Plan, clips: &[Clip]) -> Result<Vec<Window>> {
    let mut kept = Vec::new();
    for (planned, m) in plan.moments.iter().enumerate() {
        let Some(clip) = clips.get(m.clip) else { continue };
        let ms = |s: f64| (s.max(0.0) * 1000.0).round() as u64;
        let start = ms(m.start_s).min(clip.duration_ms.saturating_sub(4000));
        let end = ms(m.end_s).clamp(start + 4000, start + 15_000).min(clip.duration_ms);
        let freeze = ms(m.freeze_s).clamp(start, end);
        if end > start {
            kept.push(Window { clip: m.clip, start, end, freeze, planned });
        }
    }
    ensure!(!kept.is_empty(), "Claude found no moments that fit the clips");
    Ok(kept)
}

/// The film's frame size: the shape most moments were filmed in, at up to `LONG_EDGE` pixels.
struct Canvas {
    width: u32,
    height: u32,
}

impl Canvas {
    fn new(windows: &[Window], clips: &[Clip]) -> Result<Self> {
        let mut shapes: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for window in windows {
            *shapes.entry(clips[window.clip].tracked).or_default() += 1;
        }
        let (w, h) = shapes.into_iter().max_by_key(|(_, n)| *n).map(|(shape, _)| shape).context("no moments")?;
        let scale = LONG_EDGE as f32 / w.max(h) as f32;
        Ok(Self { width: (w as f32 * scale) as u32 & !1, height: (h as f32 * scale) as u32 & !1 })
    }

    /// Center a frame that already fits inside the canvas.
    fn place(&self, frame: &RgbImage) -> (RgbImage, (f32, f32)) {
        if frame.dimensions() == (self.width, self.height) {
            return (frame.clone(), (0.0, 0.0));
        }
        let (dx, dy) = ((self.width - frame.width()) / 2, (self.height - frame.height()) / 2);
        let mut canvas = RgbImage::new(self.width, self.height);
        image::imageops::replace(&mut canvas, frame, i64::from(dx), i64::from(dy));
        (canvas, (dx as f32, dy as f32))
    }
}

/// Where the user is at `ms`, moved smoothly between the 15 fps tracked frames.
fn you_at(clip: &Clip, ms: f64) -> Option<[f32; 4]> {
    let before = clip.you.range(..=ms as u64).next_back();
    let after = clip.you.range(ms.ceil() as u64..).next();
    match (before, after) {
        (Some((&t0, a)), Some((&t1, b))) if t1 - t0 <= 200 => {
            let f = if t1 == t0 { 0.0 } else { ((ms - t0 as f64) / (t1 - t0) as f64) as f32 };
            Some(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f))
        }
        (Some((&t, a)), _) | (_, Some((&t, a))) if (t as f64 - ms).abs() <= 100.0 => Some(*a),
        _ => None,
    }
}

fn render(
    windows: Vec<Window>,
    plan: &Plan,
    job: &Job,
    clips: &[Clip],
    font: &FontArc,
    work: &Path,
    output: &Path,
) -> Result<Vec<Moment>> {
    let canvas = Canvas::new(&windows, clips)?;
    let mut reel = Reel::start(output, &canvas)?;
    let cut = work.join("cut");
    let mut out = Vec::new();
    for window in windows {
        let clip = &clips[window.clip];
        let m = &plan.moments[window.planned];
        let video_ms = reel.frames * 1000 / u64::from(FILM_FPS);
        // Cut the moment from the original clip, scaled to fit the canvas at 30 fps.
        if cut.exists() {
            fs::remove_dir_all(&cut)?;
        }
        fs::create_dir_all(&cut)?;
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-ss", &format!("{:.3}", window.start as f64 / 1000.0), "-i"])
            .arg(&clip.source)
            .args(["-t", &format!("{:.3}", (window.end - window.start) as f64 / 1000.0)])
            .args([
                "-vf",
                &format!(
                    "fps={FILM_FPS},scale={}:{}:force_original_aspect_ratio=decrease:force_divisible_by=2",
                    canvas.width, canvas.height
                ),
            ])
            .args(["-q:v", "2"])
            .arg(cut.join("%06d.jpg"))
            .status()
            .context("running ffmpeg to cut a moment")?;
        ensure!(status.success(), "ffmpeg could not cut {} at {} ms", clip.name, window.start);
        let mut frames: Vec<PathBuf> = fs::read_dir(&cut)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        frames.sort();
        let mut frozen = false;
        let mut last = None;
        for (k, path) in frames.iter().enumerate() {
            let ms = window.start as f64 + k as f64 * 1000.0 / f64::from(FILM_FPS);
            let frame = image::open(path)?.to_rgb8();
            // Track boxes are in the tracking frames' pixels; this frame is the same picture, larger.
            let scale = frame.width() as f32 / clip.tracked.0 as f32;
            let (mut image, (dx, dy)) = canvas.place(&frame);
            if let Some([x0, y0, x1, y1]) = you_at(clip, ms) {
                let xyxy = [x0 * scale + dx, y0 * scale + dy, x1 * scale + dx, y1 * scale + dy];
                outline(&mut image, xyxy, YOU, (canvas.width / 400).max(3) as i32, Some(font));
            }
            reel.write(&image, 1)?;
            if !frozen && ms >= window.freeze as f64 {
                frozen = true;
                reel.write(&image, HOLD_SECONDS * FILM_FPS)?;
            }
            last = Some(image);
        }
        // A key frame at the very end of the window falls between frames; hold the last one.
        if let (false, Some(image)) = (frozen, &last) {
            reel.write(image, HOLD_SECONDS * FILM_FPS)?;
        }
        out.push(Moment {
            clip: job.clips[window.clip].id.clone(),
            clip_name: clip.name.clone(),
            start_ms: window.start,
            end_ms: window.end,
            freeze_ms: window.freeze,
            title: m.title.clone(),
            observation: m.observation.clone(),
            suggestion: m.suggestion.clone(),
            video_ms,
        });
    }
    reel.finish()?;
    Ok(out)
}

/// The film, encoded by ffmpeg from raw frames, and how many frames it holds so far.
struct Reel {
    child: Child,
    input: Option<ChildStdin>,
    frames: u64,
}

impl Reel {
    fn start(output: &Path, canvas: &Canvas) -> Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24"])
            .args(["-s", &format!("{}x{}", canvas.width, canvas.height), "-r", &FILM_FPS.to_string(), "-i", "-"])
            .args([
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "20",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(output)
            .stdin(Stdio::piped())
            .spawn()
            .context("starting ffmpeg to encode the film")?;
        let input = child.stdin.take();
        Ok(Self { child, input, frames: 0 })
    }

    fn write(&mut self, image: &RgbImage, repetitions: u32) -> Result<()> {
        let input = self.input.as_mut().context("the encoder is closed")?;
        for _ in 0..repetitions {
            input.write_all(image.as_raw()).context("ffmpeg stopped taking frames")?;
        }
        self.frames += u64::from(repetitions);
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        drop(self.input.take());
        ensure!(self.child.wait()?.success(), "ffmpeg could not encode the film");
        Ok(())
    }
}

fn outline(image: &mut RgbImage, [x0, y0, x1, y1]: [f32; 4], color: Rgb<u8>, width: i32, font: Option<&FontArc>) {
    if let Some(font) = font {
        rect_label(image, font, "You", Point(x0 as i32, y0 as i32), Point(x1 as i32, y1 as i32), color, false);
    }
    let (w, h) = (image.width() as i32, image.height() as i32);
    let (x0, y0, x1, y1) = (x0 as i32 - width, y0 as i32 - width, x1 as i32 + width, y1 as i32 + width);
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            if x - x0 < width || x1 - x < width || y - y0 < width || y1 - y < width {
                image.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}
