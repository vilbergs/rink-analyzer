use std::{fs, path::Path, process::Command};

use anyhow::{ensure, Context, Result};
use rink_footage::session;
use serde::Deserialize;

#[derive(Deserialize)]
struct Probe {
    streams: Vec<Video>,
    format: Format,
}

#[derive(Deserialize)]
struct Video {
    codec_name: String,
    duration: Option<String>,
}

#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
}

pub fn preview(source: &Path, folder: &Path) -> Result<(u64, u64)> {
    // Keep the existing extractor for ordinary HEVC, and fall back for phone exports.
    if let Ok(result) = session::extract_preview(source, folder) {
        return Ok(result);
    }
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-f",
            "mov",
            "-protocol_whitelist",
            "file,pipe",
            "-enable_drefs",
            "0",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,duration:format=duration",
            "-of",
            "json",
        ])
        .arg(source)
        .output()
        .context("Could not start the compatible video reader")?;
    ensure!(probe.status.success(), "This file could not be read as MOV or MP4.");
    let metadata: Probe = serde_json::from_slice(&probe.stdout)?;
    let video = metadata.streams.first().context("No video track was found.")?;
    ensure!(matches!(video.codec_name.as_str(), "h264" | "hevc"), "Choose a MOV or MP4 with H.264 or HEVC video.");
    let duration = [video.duration.as_deref(), metadata.format.duration.as_deref()]
        .into_iter()
        .flatten()
        .find_map(|value| value.parse::<f64>().ok().filter(|value| value.is_finite() && *value > 0.0))
        .context("This clip has no usable duration.")?;
    let output = folder.join("preview.jpg");
    let status = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "mov",
            "-protocol_whitelist",
            "file,pipe",
            "-enable_drefs",
            "0",
            "-threads",
            "2",
            "-i",
        ])
        .arg(source)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=480:480:force_original_aspect_ratio=decrease,setsar=1",
            "-q:v",
            "3",
            "-f",
            "image2",
            "-update",
            "1",
        ])
        .arg(&output)
        .output()
        .context("Could not start the compatible video decoder")?;
    ensure!(
        status.status.success() && fs::metadata(output).is_ok_and(|file| file.len() > 0),
        "This clip could not be decoded."
    );
    Ok(((duration * 1000.0).round() as u64, 0))
}
