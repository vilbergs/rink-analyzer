mod auto;
#[cfg(feature = "coaching-montage")]
mod montage;
mod overlay;
mod tracker;
mod video;

use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    path::PathBuf,
};

use ab_glyph::FontArc;
use anyhow::{Context, Result, bail, ensure};
use image::Rgb;
use overlay::{WEAK, WHITE, bar, checkpoint, error_line, text, window};
use tracker::{Point, Prediction, Tracker};
use video::Video;

const COLORS: [Rgb<u8>; 4] = [Rgb([34, 226, 244]), Rgb([203, 158, 255]), Rgb([114, 227, 139]), Rgb([244, 204, 101])];
const USAGE: &str = "Usage: rink-track-preview <frames-dir> <clip-stem> <start-ms> <end-ms> <fps> <checkpoints.csv> <font.ttf> <output.mp4>";

struct Track {
    label: String,
    checkpoints: BTreeMap<u64, Option<Point>>,
    tracker: Option<Tracker>,
    latest_result: String,
    errors: Vec<f32>,
}

fn read_tracks(path: &PathBuf) -> Result<Vec<Track>> {
    let content = fs::read_to_string(path)?;
    let mut lines = content.lines();
    ensure!(lines.next() == Some("time_ms,label,x,y"), "expected CSV header time_ms,label,x,y");
    let mut tracks: BTreeMap<String, BTreeMap<u64, Option<Point>>> = BTreeMap::new();
    for (line_index, line) in lines.enumerate() {
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        let &[time, label, x, y] = fields.as_slice() else {
            bail!("invalid checkpoint on CSV line {}", line_index + 2);
        };
        ensure!(
            !label.is_empty() && label.len() <= 6 && label.chars().all(|ch| ch.is_ascii_alphanumeric()),
            "labels must contain one to six ASCII letters or digits"
        );
        let position = match (x, y) {
            ("", "") => None,
            _ => Some(Point(x.parse()?, y.parse()?)),
        };
        let previous = tracks.entry(label.to_owned()).or_default().insert(time.parse()?, position);
        ensure!(previous.is_none(), "duplicate checkpoint for {label} at {time}");
    }
    ensure!((1..=COLORS.len()).contains(&tracks.len()), "expected one to four tracks");
    tracks
        .into_iter()
        .map(|(label, checkpoints)| {
            ensure!(
                checkpoints.first_key_value().is_some_and(|(_, point)| point.is_some()),
                "the first checkpoint for {label} must be a seed"
            );
            Ok(Track { label, checkpoints, tracker: None, latest_result: String::new(), errors: Vec::new() })
        })
        .collect()
}

fn run() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    #[cfg(all(feature = "auto-tracking", target_os = "macos"))]
    if args.first().is_some_and(|arg| arg == "--auto") {
        return auto::vision_preview(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "--detections") {
        return auto::detection_preview(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "--replay-tracks") {
        return auto::replay_preview(&args[1..]);
    }
    #[cfg(feature = "coaching-montage")]
    if args.first().is_some_and(|arg| arg == "--coach-review") {
        return montage::coach_review::run(&args[1..]);
    }
    #[cfg(feature = "coaching-montage")]
    if args.first().is_some_and(|arg| arg == "--montage") {
        return montage::run(&args[1..]);
    }
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let [directory, stem, start, end, fps, references, font, output] = args.as_slice() else {
        bail!("{USAGE}");
    };
    let start: u64 = start.to_str().context(USAGE)?.parse()?;
    let end: u64 = end.to_str().context(USAGE)?.parse()?;
    let fps: u32 = fps.to_str().context(USAGE)?.parse()?;
    ensure!(end > start && (1..=60).contains(&fps), "invalid clip interval or frame rate");
    let stem = stem.to_str().context("invalid clip stem")?;
    let directory = PathBuf::from(directory);
    let output = PathBuf::from(output);
    let font = FontArc::try_from_vec(fs::read(font)?).context("invalid font")?;
    let mut tracks = read_tracks(&PathBuf::from(references))?;
    let fps64 = u64::from(fps);
    let frame_times: Vec<_> =
        (0..).map(|index| start + (index * 1000 + fps64 / 2) / fps64).take_while(|&time| time < end).collect();
    for track in &tracks {
        for time in track.checkpoints.keys() {
            ensure!(
                frame_times.binary_search(time).is_ok(),
                "{} checkpoint {time} ms has no sampled frame",
                track.label
            );
        }
    }

    let load = |time| {
        let path = directory.join(format!("{stem}-{time:06}.jpg"));
        Ok::<_, anyhow::Error>(image::open(&path).with_context(|| format!("read {}", path.display()))?.to_rgb8())
    };
    let first = load(start)?;
    ensure!(first.width() >= 420 && first.height() >= 240, "preview needs frames at least 420 × 240 pixels");
    for track in &tracks {
        for (time, &point) in &track.checkpoints {
            if let Some(Point(x, y)) = point {
                ensure!(
                    (0..first.width() as i32).contains(&x) && (0..first.height() as i32).contains(&y),
                    "{} checkpoint {time} ms is outside the image",
                    track.label
                );
            }
        }
    }
    let output_file = File::create_new(&output).with_context(|| format!("create {}", output.display()))?;
    let mut video = Video::new(output_file, first.width(), first.height(), fps)?;
    for time in frame_times {
        let mut image = load(time)?;
        let mut predictions = Vec::new();
        let mut is_check = false;
        for track in &mut tracks {
            let reference = track.checkpoints.get(&time).copied();
            let prediction = if let Some(tracker) = &mut track.tracker {
                let prediction = tracker.advance(&image);
                match reference {
                    Some(Some(point)) => {
                        let error = prediction.center.distance(point);
                        track.errors.push(error);
                        track.latest_result = format!("{} px at {:.1}s", error.round(), time as f64 / 1000.0);
                        println!("{} {time} ms: {:.1} px center error", track.label, error);
                        is_check = true;
                    }
                    Some(None) => {
                        track.latest_result = format!("off screen at {:.1}s", time as f64 / 1000.0);
                        println!("{} {time} ms: marked off screen", track.label);
                        is_check = true;
                    }
                    None => {}
                }
                Some(prediction)
            } else if let Some(Some(point)) = reference {
                track.tracker = Some(Tracker::seed(&image, point).context("seed too close to image edge")?);
                track.latest_result = "manual seed".into();
                Some(Prediction { center: point, weak_match: false })
            } else {
                None
            };
            predictions.push((prediction, reference.flatten()));
        }

        for (index, (prediction, reference)) in predictions.into_iter().enumerate() {
            if let Some(prediction) = prediction {
                let color = if prediction.weak_match { WEAK } else { COLORS[index] };
                if let Some(reference) = reference {
                    error_line(&mut image, prediction.center, reference, color);
                }
                window(&mut image, prediction.center, color);
                let Point(x, y) = prediction.center;
                text(&mut image, &font, &tracks[index].label, Point(x + 19, y - 10), 22.0, color);
            }
            if let Some(reference) = reference {
                checkpoint(&mut image, reference);
            }
        }

        let panel_height = 93 + 23 * tracks.len() as u32;
        bar(&mut image, 0, panel_height);
        text(&mut image, &font, "SKATER TRACKING CHECK", Point(15, 29), 26.0, WHITE);
        let clock =
            format!("{stem}   {:.2}s{}", time as f64 / 1000.0, if is_check { "   |   checkpoint hold" } else { "" });
        text(&mut image, &font, &clock, Point(16, 54), 20.0, WHITE);
        text(&mut image, &font, "Box: CV estimate   Cross: manual check", Point(16, 78), 19.0, WHITE);
        for (index, track) in tracks.iter().enumerate() {
            let label = if track.latest_result.is_empty() { "awaiting seed" } else { &track.latest_result };
            text(
                &mut image,
                &font,
                &format!("{}   {label}", track.label),
                Point(16, 102 + 23 * index as i32),
                20.0,
                COLORS[index],
            );
        }
        let height = image.height();
        bar(&mut image, height - 36, height);
        text(
            &mut image,
            &font,
            "Amber = weak match  |  Seeds are never reset",
            Point(15, height as i32 - 11),
            18.0,
            WHITE,
        );
        video.frame(&image, if is_check { (fps / 3).max(1) } else { 1 })?;
    }
    video.finish()?;
    for track in &mut tracks {
        track.errors.sort_by(f32::total_cmp);
        let count = track.errors.len();
        if count == 0 {
            continue;
        }
        let median = (track.errors[(count - 1) / 2] + track.errors[count / 2]) / 2.0;
        let within = track.errors.iter().filter(|&&error| error <= 25.0).count();
        println!("{}: {within}/{count} within 25 px, median {median:.1} px", track.label);
    }
    println!("{}", output.display());
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
