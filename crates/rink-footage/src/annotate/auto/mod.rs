mod association;
#[cfg(all(feature = "auto-tracking", target_os = "macos"))]
mod vision;

use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use ab_glyph::FontArc;
use anyhow::{Context, Result, bail, ensure};
use image::{Rgb, RgbImage, imageops};
use serde::{Deserialize, Serialize};

use self::association::{Bounds, Detection, Tracks};
use super::{overlay::rect_label, tracker::Point, video::Video};

const RENDER_ARGS: &str = "<frames-dir> <clip-stem> <fps> <font.ttf> <output.mp4>";

#[derive(Deserialize)]
struct FrameDetections {
    frame: String,
    detections: Vec<DetectionBox>,
}

#[derive(Deserialize)]
struct DetectionBox {
    xyxy: [f32; 4],
}

#[cfg(all(feature = "auto-tracking", target_os = "macos"))]
pub fn vision_preview(args: &[OsString]) -> Result<()> {
    if let [flag] = args {
        if flag == "--help" || flag == "-h" {
            println!("Usage: rink-track-preview --auto {RENDER_ARGS}");
            return Ok(());
        }
    }
    track_preview(args, vision::detect, None, None)
}

pub fn detection_preview(args: &[OsString]) -> Result<()> {
    const COMMAND: &str = "Usage: rink-track-preview --detections";
    let (args, cuts) = match args {
        [args @ .., flag, path] if flag == "--shot-cuts" => {
            let cuts: BTreeSet<u64> = serde_json::from_reader(File::open(path)?)?;
            (args, Some(cuts))
        }
        _ => (args, None),
    };
    if let [flag] = args {
        if flag == "--help" || flag == "-h" {
            println!("{COMMAND} {RENDER_ARGS} <detections.ndjson> [tracks.ndjson] [--shot-cuts <cuts.json>]");
            return Ok(());
        }
    }
    let (args, input, track_path) = match args {
        [args @ .., input] if args.len() == 5 => (args, input, None),
        [args @ .., input, tracks] if args.len() == 5 => (args, input, Some(tracks)),
        _ => bail!("{COMMAND} {RENDER_ARGS} <detections.ndjson> [tracks.ndjson]"),
    };
    let tracks = track_path.map(File::create_new).transpose()?.map(BufWriter::new);
    let mut lines = BufReader::new(File::open(input)?).lines();
    track_preview(
        args,
        |path, _| {
            let line = lines.next().context("Missing detections for an extracted frame")??;
            let record: FrameDetections = serde_json::from_str(&line)?;
            ensure!(
                path.file_name().is_some_and(|name| name == record.frame.as_str()),
                "Out-of-order detections: {}",
                record.frame
            );
            Ok(record
                .detections
                .into_iter()
                .filter_map(|detection| {
                    let [x, y, right, bottom] = detection.xyxy;
                    let bounds = Bounds { x, y, w: right - x, h: bottom - y };
                    bounds.is_valid().then_some(bounds)
                })
                .collect())
        },
        tracks,
        cuts,
    )
}
#[derive(Deserialize, Serialize)]
struct PreviewFrame {
    source_ms: u64,
    width: u32,
    height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    shot: Option<u32>,
    tracks: Vec<PreviewTrack>,
}

#[derive(Deserialize, Serialize)]
struct PreviewTrack {
    id: u32,
    xyxy: [f32; 4],
    predicted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    gear: Option<[[u8; 5]; 3]>,
}

/// Rebuild only the video presentation. The original track records remain authoritative.
pub fn replay_preview(args: &[OsString]) -> Result<()> {
    const COMMAND: &str = "Usage: rink-track-preview --replay-tracks";
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{COMMAND} {RENDER_ARGS} <tracks.ndjson>");
        return Ok(());
    }
    let [args @ .., input] = args else { bail!("{COMMAND} {RENDER_ARGS} <tracks.ndjson>") };
    let mut lines = BufReader::new(File::open(input)?).lines();
    render(args, |time, _, frame, size, is_last| {
        let record: PreviewFrame = serde_json::from_str(&lines.next().context("Missing tracking frame")??)?;
        ensure!(
            record.source_ms == time && (record.width, record.height) == size,
            "Tracking frame does not align with video at {time} ms ({:?})",
            frame.dimensions()
        );
        ensure!(!is_last || lines.next().is_none(), "Tracking records extend past the extracted footage");
        Ok(record.tracks)
    })
}

fn track_preview(
    args: &[OsString],
    mut detect: impl FnMut(&Path, &RgbImage) -> Result<Vec<Bounds>>,
    mut track_file: Option<BufWriter<File>>,
    mut cuts: Option<BTreeSet<u64>>,
) -> Result<()> {
    let mut tracks = Tracks::default();
    let mut shot = cuts.as_ref().map(|_| 1);
    render(args, |time, path, frame, (width, height), is_last| {
        if let Some(cuts) = &mut cuts {
            if cuts.remove(&time) {
                tracks.end_shot();
                shot = shot.map(|shot| shot + 1);
            }
            ensure!(!is_last || cuts.is_empty(), "Some camera cuts do not align with the extracted frames");
        }
        let detections = detect(path, frame)
            .with_context(|| format!("detect people in {}", path.display()))?
            .into_iter()
            .map(|bounds| Detection::new(frame, bounds))
            .collect();
        tracks.advance(detections);
        let visible = tracks
            .visible()
            .map(|track| {
                let bounds = track.draw_bounds();
                let Bounds { x, y, w, h } = bounds;
                let predicted = track.age_since_match > 0;
                PreviewTrack {
                    id: track.id,
                    xyxy: [x, y, x + w, y + h],
                    predicted,
                    gear: (track_file.is_some() && !predicted && h >= 30.0 && w >= 12.0).then(|| gear(frame, bounds)),
                }
            })
            .collect();
        let record = PreviewFrame { source_ms: time, width, height, shot, tracks: visible };
        if let Some(writer) = &mut track_file {
            serde_json::to_writer(&mut *writer, &record)?;
            writer.write_all(b"\n")?;
            if is_last {
                writer.flush()?;
            }
        }
        Ok(record.tracks)
    })
}

const COLORS: [Rgb<u8>; 8] = [
    Rgb([40, 225, 244]),
    Rgb([223, 171, 255]),
    Rgb([149, 244, 121]),
    Rgb([255, 211, 77]),
    Rgb([117, 182, 255]),
    Rgb([255, 159, 114]),
    Rgb([255, 166, 211]),
    Rgb([167, 240, 202]),
];

fn render(
    args: &[OsString],
    mut boxes: impl FnMut(u64, &Path, &RgbImage, (u32, u32), bool) -> Result<Vec<PreviewTrack>>,
) -> Result<()> {
    let [frames, stem, fps, font, output] = args else { bail!("Expected {RENDER_ARGS}") };
    let dir = Path::new(frames);
    let stem = stem.to_str().context("invalid clip stem")?;
    let prefix = format!("{stem}-");
    let fps: u32 = fps.to_str().context("missing frame rate")?.parse()?;
    ensure!((1..=60).contains(&fps), "fps must be between 1 and 60");
    let output = Path::new(output);
    let font = FontArc::try_from_vec(fs::read(font)?).context("invalid font")?;
    let mut files = Vec::new();
    for file in fs::read_dir(dir)? {
        let path = file?.path();
        let timestamp = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix(&prefix))
            .and_then(|name| name.strip_suffix(".jpg"))
            .and_then(|name| name.parse::<u64>().ok());
        if let Some(time) = timestamp {
            files.push((time, path));
        }
    }
    files.sort_unstable();
    let (start, first_path) = files.first().context("no matching frames")?;
    for (index, (time, _)) in files.iter().enumerate() {
        let expected = start + (index as u64 * 1000 + u64::from(fps) / 2) / u64::from(fps);
        ensure!(*time == expected, "expected a frame at {expected} ms, found {time} ms");
    }

    let (width, height) = image::image_dimensions(first_path)?;
    let size = (width & !1, height & !1);
    ensure!(size.0 >= 160 && size.1 >= 100, "preview needs frames at least 160 × 100 pixels");
    let partial = partial_path(output);
    ensure!(!output.exists(), "{} already exists", output.display());
    let file = File::create_new(&partial).with_context(|| format!("create {}", partial.display()))?;
    let mut video = Video::new(file, size.0, size.1, fps)?;
    let mut peak = 0;
    let count = files.len();
    for (i, (time, path)) in files.into_iter().enumerate() {
        let mut frame = image::open(&path)?.to_rgb8();
        ensure!(frame.dimensions() == (width, height), "frame dimensions changed: {}", path.display());
        let boxes = boxes(time, &path, &frame, size, i + 1 == count)?;
        peak = peak.max(boxes.len());
        for track in boxes {
            let [left, top, right, bottom] = track.xyxy;
            let color = COLORS[track.id.saturating_sub(1) as usize % COLORS.len()];
            rect_label(
                &mut frame,
                &font,
                &format!("S{:02}", track.id),
                Point(left.round() as i32, top.round() as i32),
                Point(right.round() as i32, bottom.round() as i32),
                color,
                track.predicted,
            );
        }
        if size != frame.dimensions() {
            frame = imageops::crop(&mut frame, 0, 0, size.0, size.1).to_image();
        }
        video.frame(&frame, 1)?;
        if (i + 1) % 150 == 0 {
            eprintln!("{stem}: {}/{count} frames", i + 1);
        }
    }
    video.finish()?;
    // Publish the finished file without replacing a file another render created.
    fs::hard_link(&partial, output)?;
    fs::remove_file(partial)?;
    println!("{stem}: {count} frames, peak {peak} concurrent IDs · {}", output.display());
    Ok(())
}

/// Coarse color counts for helmet, upper jersey, and lower legs.
fn gear(frame: &RgbImage, bounds: Bounds) -> [[u8; 5]; 3] {
    let mut profile = [[0; 5]; 3];
    for (zone, (top, bottom)) in profile.iter_mut().zip([(0.03, 0.18), (0.26, 0.48), (0.73, 0.90)]) {
        for row in 0..4 {
            for column in 0..5 {
                let x = bounds.x + bounds.w * (0.28 + column as f32 * 0.11);
                let y = bounds.y + bounds.h * (top + (bottom - top) * row as f32 / 3.0);
                let x = x.clamp(0.0, (frame.width() - 1) as f32) as u32;
                let y = y.clamp(0.0, (frame.height() - 1) as f32) as u32;
                let [r, g, b] = frame.get_pixel(x, y).0.map(i32::from);
                let lightness = (2 * r + 5 * g + b) / 8;
                let color = if r > g + 26 && r > b + 26 {
                    0
                } else if b > r + 15 && b > g + 7 {
                    1
                } else if lightness < 75 {
                    2
                } else if lightness > 172 && r.max(g).max(b) - r.min(g).min(b) < 44 {
                    3
                } else {
                    4
                };
                zone[color] += 1;
            }
        }
    }
    profile
}

fn partial_path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_owned();
    path.push(".partial");
    PathBuf::from(path)
}
