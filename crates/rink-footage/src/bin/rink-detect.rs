//! Detect skaters, goalies and referees with a hockey YOLOv8 checkpoint (HockeyAI by default),
//! writing one JSON line per frame in the format `detect-yolox.py` writes, so the tracker can
//! take either detector.
//!
//! `detections` holds the people the tracker follows: every box there becomes a track
//! candidate. Pucks and rink marks go in `other`, which the tracker does not read.
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, ensure, Context, Result};
use rink_footage::yolo::{self, Yolo};
use serde_json::json;

const USAGE: &str =
    "Usage: rink-detect <model.pt> <frames-dir|frame.jpg>... [--confidence 0.25] [--size 1280]\n       \
                     rink-detect draw <frames-dir> <detections.ndjson> <out-dir>";
/// Class names that are people on the ice, across HockeyAI's and similar checkpoints' spellings.
const PEOPLE: [&str; 4] = ["player", "goalie", "goaltender", "referee"];

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next().context(USAGE)?;
    if first == "draw" {
        let rest: Vec<String> = args.collect();
        let [frames, detections, out] = rest.as_slice() else { bail!(USAGE) };
        return draw(Path::new(frames), Path::new(detections), Path::new(out));
    }
    let model_path = PathBuf::from(first);
    let (mut inputs, mut confidence, mut size) = (Vec::new(), 0.25_f32, 1280_u32);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--confidence" => confidence = args.next().context(USAGE)?.parse()?,
            "--size" => size = args.next().context(USAGE)?.parse()?,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            flag if flag.starts_with("--") => bail!("unknown option {flag}\n{USAGE}"),
            path => inputs.push(PathBuf::from(path)),
        }
    }
    ensure!(!inputs.is_empty(), USAGE);
    ensure!(confidence > 0.0 && confidence <= 1.0, "--confidence must be greater than 0 and at most 1");
    ensure!(size >= 32 && size % 32 == 0, "--size must be a multiple of 32");

    let device = yolo::device()?;
    let provider = if device.is_cuda() { "candle-cuda" } else { "candle-cpu" };
    let model = Yolo::load(&model_path, device)?;
    let puck = model.names.iter().position(|n| n == "puck");
    // Pucks are tiny and rarely confident, so they get half the bar, as in the corpus draft boxes.
    let bar = |class: usize| if Some(class) == puck { confidence / 2.0 } else { confidence };

    let mut out = io::stdout().lock();
    for frame in frames(&inputs)? {
        let image = image::open(&frame).with_context(|| format!("reading {}", frame.display()))?.to_rgb8();
        let (mut people, mut other) = (Vec::new(), Vec::new());
        for d in model.detect(&image, size, &bar)? {
            let name = &model.names[d.class];
            let xyxy: Vec<f64> = d.xyxy.iter().map(|v| (f64::from(*v) * 100.0).round() / 100.0).collect();
            let entry = json!({ "xyxy": xyxy, "confidence": (f64::from(d.score) * 1e4).round() / 1e4, "class": name });
            if PEOPLE.contains(&name.as_str()) {
                people.push(entry)
            } else {
                other.push(entry)
            }
        }
        let name = frame.file_name().context("frame has a name")?.to_string_lossy();
        writeln!(out, "{}", json!({ "frame": name, "provider": provider, "detections": people, "other": other }))?;
        out.flush()?;
    }
    Ok(())
}

/// Draw every box of a detections file (this tool's or `detect-yolox.py`'s) onto its frames,
/// colored by class, for looking at a detector's raw output before tracking.
fn draw(frames: &Path, detections: &Path, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let text = std::fs::read_to_string(detections).with_context(|| format!("reading {}", detections.display()))?;
    for line in text.lines() {
        let record: serde_json::Value = serde_json::from_str(line)?;
        let name = record["frame"].as_str().context("detections line has no frame")?;
        let mut image = image::open(frames.join(name)).with_context(|| format!("reading frame {name}"))?.to_rgb8();
        let boxes = ["detections", "other"].iter().filter_map(|k| record[*k].as_array()).flatten();
        for b in boxes {
            let class = b["class"].as_str().unwrap_or("person");
            let Some(color) = color(class) else { continue };
            let v: Vec<f32> = b["xyxy"]
                .as_array()
                .context("box has no xyxy")?
                .iter()
                .filter_map(|x| x.as_f64())
                .map(|x| x as f32)
                .collect();
            ensure!(v.len() == 4, "box in {name} is not xyxy");
            // A puck is a few pixels wide; pad its box so it can be seen at playback size.
            let pad = if class == "puck" { 6.0 } else { 0.0 };
            outline(&mut image, [v[0] - pad, v[1] - pad, v[2] + pad, v[3] + pad], color, 2);
        }
        image.save(out.join(name))?;
    }
    Ok(())
}

/// Class colors; rink marks (faceoff dots, center ice, goals) are left out to keep the view clear.
fn color(class: &str) -> Option<[u8; 3]> {
    Some(match class {
        "person" => [255, 255, 255],
        "player" => [40, 140, 255],
        "goalie" | "goaltender" => [255, 210, 0],
        "referee" => [60, 220, 90],
        "puck" => [255, 30, 30],
        _ => return None,
    })
}

fn outline(image: &mut image::RgbImage, [x0, y0, x1, y1]: [f32; 4], color: [u8; 3], width: i64) {
    let (w, h) = (i64::from(image.width()), i64::from(image.height()));
    let (x0, y0, x1, y1) = (x0.round() as i64, y0.round() as i64, x1.round() as i64, y1.round() as i64);
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let edge = x - x0 < width || x1 - x < width || y - y0 < width || y1 - y < width;
            if edge {
                image.put_pixel(x as u32, y as u32, image::Rgb(color));
            }
        }
    }
}

/// Frames in the order the tracker expects: a directory's JPEGs sorted by the number after the
/// last `-` in their names (the frame time), as `detect-yolox.py` sorts them.
fn frames(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut all = Vec::new();
    for input in inputs {
        if !input.is_dir() {
            all.push(input.clone());
            continue;
        }
        let mut found: Vec<PathBuf> = std::fs::read_dir(input)?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
            .collect();
        let key = |p: &Path| -> Result<u64> {
            let stem = p.file_stem().and_then(|s| s.to_str()).context("frame name is not UTF-8")?;
            stem.rsplit('-')
                .next()
                .unwrap_or(stem)
                .parse()
                .with_context(|| format!("frame {stem} has no trailing number"))
        };
        let mut keyed = found.drain(..).map(|p| Ok((key(&p)?, p))).collect::<Result<Vec<_>>>()?;
        keyed.sort();
        all.extend(keyed.into_iter().map(|(_, p)| p));
    }
    Ok(all)
}
