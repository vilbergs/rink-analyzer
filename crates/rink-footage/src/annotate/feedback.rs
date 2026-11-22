//! One feedback video from a few clips the user is marked in.
//!
//! `rink-track-preview --feedback <work-dir> <font.ttf>` reads, for each clip in `job.json`,
//! `<clip>/frames/*.jpg` (15 fps, named by source time), `<clip>/tracks.ndjson` and
//! `<clip>/focus.json` (the "this is me" anchors). It shows Claude sampled frames with the user
//! boxed, gets back a few moments with cues, checks them against the clips, and writes
//! `out/feedback.json` and `out/video.mp4`. Claude is reached through the API with
//! `ANTHROPIC_API_KEY`, or through the Claude Code CLI at `RINK_FEEDBACK_CLAUDE`;
//! `RINK_FEEDBACK_PLAN` names a saved answer to render instead.
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs::{self, File},
    io::{BufRead, BufReader, Cursor, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use anyhow::{bail, ensure, Context, Result};
use base64::Engine;
use image::{imageops::FilterType, Rgb, RgbImage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    overlay::{bar, rect_label, text, WHITE},
    tracker::Point,
    video::Video,
};

const USAGE: &str = "Usage: rink-track-preview --feedback <work-dir> <font.ttf>";
const FPS: u32 = 15;
const YOU: Rgb<u8> = Rgb([255, 214, 0]);
/// Images sent to Claude across all clips; frames are 720 px wide, about 400 tokens each.
const MAX_SAMPLES: usize = 90;
const HOLD_SECONDS: u32 = 5;
const INTRO_SECONDS: u32 = 4;

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
    let moments = render(&plan, moments, &job, &clips, &font, &out.join("video.mp4"))?;
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
    Ok(Clip { name: name.to_owned(), frames, you, duration_ms })
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
        "{ask}Pick the 3 to 6 moments most worth learning from, spread over the clips where it makes sense. \
For each, give the clip number, a window of 4 to 12 seconds (start_s, end_s, in clip seconds), the second \
to freeze on (freeze_s, the key decision, inside the window), a title of at most 6 words, what I did \
(observation) and what to try next time (suggestion), each one or two sentences. Then a two-sentence \
summary of the main pattern across the clips."
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

/// Frames scaled and centered onto the output size, with the transform for track boxes.
struct Canvas {
    width: u32,
    height: u32,
}

impl Canvas {
    fn fit(&self, path: &Path) -> Result<(RgbImage, impl Fn([f32; 4]) -> [f32; 4])> {
        let image = image::open(path)?.to_rgb8();
        let scale = (self.width as f32 / image.width() as f32).min(self.height as f32 / image.height() as f32);
        let (w, h) = ((image.width() as f32 * scale) as u32, (image.height() as f32 * scale) as u32);
        let (dx, dy) = ((self.width - w) / 2, (self.height - h) / 2);
        let map = move |[x0, y0, x1, y1]: [f32; 4]| {
            let (dx, dy) = (dx as f32, dy as f32);
            [x0 * scale + dx, y0 * scale + dy, x1 * scale + dx, y1 * scale + dy]
        };
        if (w, h) == (self.width, self.height) {
            return Ok((image, map));
        }
        let mut canvas = RgbImage::new(self.width, self.height);
        let resized = image::imageops::resize(&image, w, h, FilterType::Triangle);
        image::imageops::replace(&mut canvas, &resized, i64::from(dx), i64::from(dy));
        Ok((canvas, map))
    }
}

fn render(
    plan: &Plan,
    windows: Vec<Window>,
    job: &Job,
    clips: &[Clip],
    font: &FontArc,
    output: &Path,
) -> Result<Vec<Moment>> {
    let opening = &clips[windows[0].clip].frames[0].0;
    let (width, height) = image::image_dimensions(opening)?;
    let canvas = Canvas { width: width & !1, height: height & !1 };
    let mut reel = Reel { video: Video::new(File::create(output)?, canvas.width, canvas.height, FPS)?, frames: 0 };

    // Intro: the summary over the opening frame.
    let (mut intro, _) = canvas.fit(opening)?;
    bar(&mut intro, 0, canvas.height);
    text(&mut intro, font, "Feedback", Point(24, 48), 30.0, YOU);
    paragraph(&mut intro, font, &plan.summary, Point(24, 90), canvas.width - 48, 22.0, WHITE);
    reel.write(&intro, INTRO_SECONDS * FPS)?;

    let total = windows.len();
    let mut out = Vec::new();
    for (n, window) in windows.into_iter().enumerate() {
        let clip = &clips[window.clip];
        let m = &plan.moments[window.planned];
        let video_ms = reel.frames * 1000 / u64::from(FPS);
        let mut frozen = false;
        for (path, ms) in clip.frames.iter().filter(|(_, ms)| (window.start..=window.end).contains(ms)) {
            let (mut image, map) = canvas.fit(path)?;
            let you = clip.you.get(ms).map(|&xyxy| map(xyxy));
            if let Some(xyxy) = you {
                outline(&mut image, xyxy, YOU, 3, Some(font));
            }
            bar(&mut image, 0, 40);
            let header = format!("{}/{total} · {} · {}", n + 1, m.title, clip.name);
            text(&mut image, font, &header, Point(14, 28), 20.0, WHITE);
            reel.write(&image, 1)?;
            if !frozen && *ms >= window.freeze {
                frozen = true;
                // The cue goes in the half of the frame away from the player.
                let you_low = you.is_some_and(|[_, y0, _, y1]| (y0 + y1) / 2.0 > canvas.height as f32 / 2.0);
                caption(&mut image, font, m, you_low);
                reel.write(&image, HOLD_SECONDS * FPS)?;
            }
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
    reel.video.finish()?;
    Ok(out)
}

/// The output video and how many frames it holds, for each moment's start time.
struct Reel {
    video: Video,
    frames: u64,
}

impl Reel {
    fn write(&mut self, image: &RgbImage, repetitions: u32) -> Result<()> {
        self.video.frame(image, repetitions)?;
        self.frames += u64::from(repetitions);
        Ok(())
    }
}

fn caption(image: &mut RgbImage, font: &FontArc, m: &PlannedMoment, at_top: bool) {
    let (width, height) = image.dimensions();
    let (top, bottom) = if at_top { (40, height * 9 / 20 + 40) } else { (height * 11 / 20, height) };
    bar(image, top, bottom);
    text(image, font, &m.title, Point(20, top as i32 + 32), 24.0, YOU);
    let y = paragraph(image, font, &m.observation, Point(20, top as i32 + 62), width - 40, 18.0, WHITE);
    paragraph(image, font, &format!("Try: {}", m.suggestion), Point(20, y + 8), width - 40, 18.0, YOU);
}

/// Draw text wrapped to `width` from the baseline at `at`; returns the baseline after the last line.
fn paragraph(
    image: &mut RgbImage,
    font: &FontArc,
    value: &str,
    at: Point,
    width: u32,
    size: f32,
    color: Rgb<u8>,
) -> i32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let measure = |s: &str| s.chars().map(|c| scaled.h_advance(scaled.glyph_id(c))).sum::<f32>();
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in value.split_whitespace() {
        let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if measure(&candidate) > width as f32 && !line.is_empty() {
            lines.push(std::mem::replace(&mut line, word.to_owned()));
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    let step = (size * 1.35) as i32;
    for (i, line) in lines.iter().enumerate() {
        text(image, font, line, Point(at.0, at.1 + i as i32 * step), size, color);
    }
    at.1 + (lines.len() as i32 - 1).max(0) * step + step
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
