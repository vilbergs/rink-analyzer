pub mod coach_review;
mod guides;
mod puck;
mod raster;

use std::{
    ffi::OsString,
    fs::{self, File},
    path::Path,
};

use super::{
    overlay::{text, WHITE},
    tracker::Point,
    video::Video,
};
use ab_glyph::FontArc;
use anyhow::{bail, ensure, Context, Result};
use image::{
    imageops::{self, FilterType},
    Rgb, RgbImage,
};
use rink_core::{
    io,
    review::{active_suggestions, load_review},
    sample,
    scene::{self, Frame},
    transform::{RinkTransform, ViewZoom, Viewport},
    Play, Suggestion, Vec2,
};
use serde_json::json;

const FPS: u32 = 15;
const SIZE: (u32, u32) = (1280, 720);
const WEB_FOOTAGE: (u32, u32) = (600, 450);
const WEB_RINK: (u32, u32) = (960, 488);
const SOURCE: Rect = Rect { x: 25, y: 115, w: 600, h: 427 };
const RINK: Rect = Rect { x: 649, y: 115, w: 606, h: 427 };
const BG: Rgb<u8> = Rgb([13, 24, 38]);
const PANEL: Rgb<u8> = Rgb([20, 35, 51]);
const MUTED: Rgb<u8> = Rgb([161, 184, 205]);
const GOLD: Rgb<u8> = Rgb([246, 187, 72]);
const GREEN: Rgb<u8> = Rgb([111, 224, 152]);
const USAGE: &str =
    "Usage: rink-track-preview --montage <frames-root> <plays-dir> <manifest.tsv> <font.ttf> <output.mp4> [guides.tsv] [web-output-dir]";

#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}
impl Rect {
    fn paint(self, image: &mut RgbImage, color: Rgb<u8>) {
        for y in self.y..self.y + self.h {
            for x in self.x..self.x + self.w {
                image.put_pixel(x, y, color);
            }
        }
    }
}

struct Segment<'a> {
    clip: &'a str,
    start_ms: u32,
    play: Play,
    suggestion: Suggestion,
    marker: [i32; 4],
    cue: [&'a str; 2],
}

fn parse<'a>(manifest: &'a str, plays: &Path) -> Result<Vec<Segment<'a>>> {
    let mut lines = manifest.lines();
    ensure!(lines.next() == Some("clip\tstart_ms\tplay\tx1\ty1\tx2\ty2\tcue\tcondition"), "invalid montage TSV header");
    let mut segments = Vec::new();
    for (index, line) in lines.enumerate() {
        let pieces = line.split('\t').collect::<Vec<_>>();
        let &[clip, start, play_file, x1, y1, x2, y2, cue, condition] = pieces.as_slice() else {
            bail!("invalid montage TSV line {}", index + 2);
        };
        ensure!(
            !clip.is_empty() && clip.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)),
            "invalid source clip name {clip}"
        );
        let play_path = plays.join(play_file);
        let play = io::load(&play_path)?;
        let review = load_review(&play_path)?.context("play needs a positioning review")?;
        let [suggestion] = review.suggestions.as_slice() else {
            bail!("montage play needs one suggestion per segment");
        };
        suggestion.check(&play)?;
        ensure!(!suggestion.is_stale(&play), "stale positioning review");
        let marker = [x1.parse()?, y1.parse()?, x2.parse()?, y2.parse()?];
        ensure!(
            marker[0] >= 0 && marker[1] >= 0 && marker[0] < marker[2] && marker[1] < marker[3],
            "invalid focus skater box"
        );
        let start_ms = start.parse::<u32>()?;
        ensure!(
            start_ms % 1000 == 0 && start_ms.checked_add(play.duration_ms).is_some(),
            "source start must be a whole second for frame alignment"
        );
        segments.push(Segment { clip, start_ms, play, suggestion: suggestion.clone(), marker, cue: [cue, condition] });
    }
    ensure!(!segments.is_empty(), "no montage segments");
    Ok(segments)
}

// The source frame timestamps are rounded to milliseconds and the end is exclusive.
fn sequence_frames(duration_ms: u32, decision_ms: u32) -> Result<(u64, u64)> {
    let fps = u64::from(FPS);
    let count = (u64::from(duration_ms) * fps).saturating_sub(fps / 2).div_ceil(1000);
    ensure!(count > 0, "montage sequence must contain at least one frame");
    let nearest = (u64::from(decision_ms) * fps + 500) / 1000;
    Ok((count, nearest.min(count - 1)))
}

pub fn run(args: &[OsString]) -> Result<()> {
    render(args, &mut [])
}

fn render(args: &[OsString], puck_passes: &mut [coach_review::PuckPass]) -> Result<()> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let (frames, plays, manifest, font, output, options) = match args {
        [frames, plays, manifest, font, output, options @ ..] if options.len() <= 2 => {
            (frames, plays, manifest, font, output, options)
        }
        _ => bail!("{USAGE}"),
    };
    let manifest = fs::read_to_string(manifest)?;
    let segments = parse(&manifest, Path::new(plays))?;
    let guide_text = options.first().map(fs::read_to_string).transpose()?;
    let moments = guide_text.as_deref().map(guides::parse).transpose()?.unwrap_or_default();
    for &(clip, time) in moments.keys() {
        ensure!(
            segments.iter().any(|segment| {
                if segment.clip != clip || time < segment.start_ms {
                    return false;
                }
                let relative = u64::from(time - segment.start_ms);
                let frame = (relative * u64::from(FPS) + 500) / 1000;
                relative < u64::from(segment.play.duration_ms)
                    && relative == (frame * 1000 + u64::from(FPS) / 2) / u64::from(FPS)
            }),
            "coaching guide {clip} at {time} ms has no source frame"
        );
    }
    let frames = Path::new(frames);
    let font = FontArc::try_from_vec(fs::read(font)?).context("invalid font")?;
    let output = Path::new(output);
    let mut partial = output.as_os_str().to_owned();
    partial.push(".partial");
    ensure!(!output.exists(), "{} already exists", output.display());
    let mut video = Video::new(File::create_new(&partial)?, SIZE.0, SIZE.1, FPS)?;
    let web = options.get(1).map(Path::new);
    let mut web_video = if let Some(folder) = web {
        fs::create_dir_all(folder)?;
        Some(Video::new(File::create_new(folder.join("footage.mp4"))?, WEB_FOOTAGE.0, WEB_FOOTAGE.1, FPS)?)
    } else {
        None
    };
    let mut output_frame = 0u32;
    let mut chapters = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let mut viewport = Viewport::new(Vec2::ZERO, Vec2::new(RINK.w as f32, RINK.h as f32));
        let entities: Vec<_> = segment.play.entities.iter().map(|entity| entity.id.clone()).collect();
        viewport.crop = Some(scene::x_extent(&segment.play, &entities, 5.0));
        viewport.margin_px = 18.0;
        let transform = RinkTransform::fit(&segment.play.rink, &viewport, ViewZoom::default());
        let mut full_view = Viewport::new(Vec2::ZERO, Vec2::new(WEB_RINK.0 as f32, WEB_RINK.1 as f32));
        full_view.margin_px = 8.0;
        let full_transform = RinkTransform::fit(&segment.play.rink, &full_view, ViewZoom::default());
        let mut rink_frames = Vec::new();
        let (frame_count, decision_frame) = sequence_frames(segment.play.duration_ms, segment.suggestion.t_ms)?;
        let full_source = puck_passes.get(index).is_some_and(coach_review::PuckPass::has_seeds);
        let chapter_start = output_frame;
        let mut cue_holds = Vec::new();
        let title = segment.play.title.split_once(" · ").map_or(segment.play.title.as_str(), |(_, title)| title);
        for frame in 0..frame_count {
            let relative_ms = ((frame * 1000 + u64::from(FPS) / 2) / u64::from(FPS)) as u32;
            let source_ms = segment.start_ms + relative_ms;
            let source_file = frames.join(segment.clip).join(format!("{}-{source_ms:06}.jpg", segment.clip));
            let source_image =
                image::open(&source_file).with_context(|| format!("read {}", source_file.display()))?.to_rgb8();
            let puck = match puck_passes.get_mut(index) {
                Some(pass) => pass.observe(&source_image, source_ms, &segment.play.rink)?,
                None => None,
            };
            let at_decision = frame == decision_frame;
            let moment = moments.get(&(segment.clip, source_ms));
            let hold = at_decision || moment.is_some();
            let cue = moment.and_then(|moment| moment.cue).unwrap_or(segment.cue);
            let repeated = if hold { 48 } else { 1 };
            if hold {
                cue_holds.push(json!({
                    "frame": output_frame, "frames": repeated, "sourceMs": source_ms,
                    "cue": cue[0], "detail": cue[1], "decision": at_decision
                }));
            }
            if at_decision {
                let [_, _, x2, y2] = segment.marker;
                ensure!(
                    x2 < source_image.width() as i32 && y2 < source_image.height() as i32,
                    "focus marker extends outside {}",
                    source_file.display()
                );
            }
            let rink_ms = if at_decision { segment.suggestion.t_ms } else { relative_ms };
            let snapshot = sample(&segment.play, rink_ms);
            let ghosts: Vec<_> = active_suggestions(std::slice::from_ref(&segment.suggestion), &segment.play, rink_ms)
                .map(|(suggestion, spot)| (suggestion.clone(), spot))
                .collect();
            let rink_frame = Frame {
                play: &segment.play,
                snapshot: &snapshot,
                playhead_ms: rink_ms,
                selected: None,
                ghosts: &ghosts,
                hidden: &[],
                marks: &[],
            };
            let mut scene = scene::build(&rink_frame, &transform);
            if let Some(mark) = puck {
                mark.draw_on_rink(&mut scene, &transform);
            }
            let rink_image = raster::draw(&scene, &font, RINK.w, RINK.h)?;
            let mut composite = RgbImage::from_pixel(SIZE.0, SIZE.1, BG);
            let source_view = insert_source(&mut composite, &source_image, full_source);
            let marker = at_decision.then_some(segment.marker);
            let composite_marker = marker.and_then(|marker| source_view.visible_marker(marker));
            guides::draw(&mut composite, &font, source_view, moment, composite_marker, puck.map(|m| m.pixel))?;
            imageops::replace(&mut composite, &rink_image, i64::from(RINK.x), i64::from(RINK.y));
            text(&mut composite, &font, title, Point(25, 63), 40.0, WHITE);
            text(
                &mut composite,
                &font,
                &format!("FILM REVIEW    {:02} / {:02}", index + 1, segments.len()),
                Point(25, 24),
                17.0,
                MUTED,
            );
            text(
                &mut composite,
                &font,
                &format!("{}   {}", segment.clip, clock(source_ms)),
                Point(25, 103),
                21.0,
                WHITE,
            );
            text(&mut composite, &font, "RINK REPLAY", Point(649, 103), 19.0, WHITE);
            text(&mut composite, &font, "Gold   You", Point(858, 103), 19.0, GOLD);
            text(&mut composite, &font, "Green   Suggested route", Point(985, 103), 19.0, GREEN);

            Rect { x: 25, y: 563, w: 1230, h: 128 }.paint(&mut composite, PANEL);
            if hold || relative_ms >= segment.suggestion.branch_from_ms {
                Rect { x: 25, y: 563, w: 4, h: 128 }.paint(&mut composite, GOLD);
                let section = if at_decision {
                    "DECISION FRAME"
                } else if hold {
                    "WATCH THIS FRAME"
                } else {
                    "COACHING CUE"
                };
                text(&mut composite, &font, section, Point(44, 590), 17.0, GOLD);
                text(&mut composite, &font, cue[0], Point(44, 628), 31.0, WHITE);
                text(&mut composite, &font, cue[1], Point(44, 665), 23.0, MUTED);
            } else {
                text(&mut composite, &font, "WATCH THE SETUP", Point(44, 590), 17.0, MUTED);
                let caption = segment.play.captions.iter().rev().find(|caption| caption.t_ms <= relative_ms);
                if let Some(caption) = caption {
                    text(&mut composite, &font, &caption.text, Point(44, 636), 24.0, WHITE);
                }
            }
            if let Some(footage) = &mut web_video {
                let (mut image, view) = web_source(&source_image, moment, marker, full_source);
                guides::draw(&mut image, &font, view, moment, marker, puck.map(|m| m.pixel))?;
                footage.frame(&image, repeated)?;
                let mut layer = scene::build_play_layer(&rink_frame, &full_transform);
                if let Some(mark) = puck {
                    mark.draw_on_rink(&mut layer, &full_transform);
                }
                rink_frames.push(json!({ "frame": output_frame, "ms": relative_ms, "shapes": layer.shapes, "puckDetail": puck.map(|m| puck::detail(&source_image,m.pixel)) }));
            }
            video.frame(&composite, repeated)?;
            output_frame += repeated;
        }
        if web.is_some() {
            chapters.push(json!({
                "title": title, "clip": segment.clip, "frame": chapter_start, "endFrame": output_frame,
                "sourceMs": segment.start_ms, "holds": cue_holds, "durationMs": segment.play.duration_ms,
                "events": segment.play.events, "captions": segment.play.captions,
                "suggestion": { "tMs": segment.suggestion.t_ms, "principle": segment.suggestion.principle },
                "keyframes": segment.play.track(&segment.suggestion.entity).iter().map(|key| key.t_ms).collect::<Vec<_>>(),
                "rinkBase": scene::build_rink(&segment.play.rink, &full_transform).shapes,
                "rinkFrames": rink_frames,
            }));
        }
        println!("Rendered {} with decision at {}", segment.clip, clock(segment.start_ms + segment.suggestion.t_ms));
    }
    video.finish()?;
    if let Some(footage) = web_video {
        footage.finish()?;
        let folder = web.context("missing web output folder")?;
        let timing = json!({ "fps": FPS, "rinkSize": WEB_RINK, "chapters": chapters });
        fs::write(
            folder.join("timeline.js"),
            format!("window.coachingTiming = {};\n", serde_json::to_string(&timing)?),
        )?;
        fs::write(folder.join("index.html"), include_str!("web/index.html"))?;
        fs::write(folder.join("feedback.js"), "window.coachingFeedback = {};\n")?;
        fs::write(folder.join("coaching.js"), include_str!("web/coaching.js"))?;
        fs::write(folder.join("coaching.css"), include_str!("web/coaching.css"))?;
    }
    fs::hard_link(&partial, output)?;
    fs::remove_file(partial)?;
    Ok(())
}

fn clock(ms: u32) -> String {
    format!("{:02}:{:02}.{:01}", ms / 60000, ms / 1000 % 60, ms / 100 % 10)
}

#[derive(Clone, Copy)]
struct SourceView {
    bounds: Rect,
    source_size: (u32, u32),
    crop: Rect,
}

impl SourceView {
    fn visible_marker(self, [x1, y1, x2, y2]: [i32; 4]) -> Option<[i32; 4]> {
        let crop = self.crop;
        let left = x1.max(crop.x as i32);
        let top = y1.max(crop.y as i32);
        let right = x2.min((crop.x + crop.w - 1) as i32);
        self.project(Point(right, y2)).ok()?;
        (right > left && y2 > top).then_some([left, top, right, y2])
    }

    fn project(self, Point(x, y): Point) -> Result<Point> {
        let (width, height) = self.source_size;
        ensure!(x >= 0 && y >= 0 && x < width as i32 && y < height as i32, "guide outside source image");
        let b = self.bounds;
        let crop = self.crop;
        let x = x as u32;
        let y = y as u32;
        ensure!(
            (crop.x..crop.x + crop.w).contains(&x) && (crop.y..crop.y + crop.h).contains(&y),
            "guide outside displayed crop"
        );
        let project = |value, start, from: u32, to: u32| ((value - start) * to + from / 2) / from;
        let x = b.x + project(x, crop.x, crop.w, b.w).min(b.w - 1);
        let y = b.y + project(y, crop.y, crop.h, b.h).min(b.h - 1);
        Ok(Point(x as i32, y as i32))
    }

    fn clamp_label(self, Point(x, y): Point, width: i32) -> Result<Point> {
        let b = self.bounds;
        ensure!(width <= b.w as i32, "guide label is wider than the displayed footage");
        let min_x = b.x as i32;
        let max_x = (b.x + b.w) as i32 - width;
        Ok(Point(x.clamp(min_x, max_x), y.clamp(b.y as i32 + 16, (b.y + b.h) as i32 - 7)))
    }
}

fn insert_source(destination: &mut RgbImage, original: &RgbImage, full_source: bool) -> SourceView {
    SOURCE.paint(destination, PANEL);
    let portrait = !full_source && original.width() < original.height();
    let top = if portrait { original.height() / 4 } else { 0 };
    let height = if portrait { original.height() * 52 / 100 } else { original.height() };
    let crop = imageops::crop_imm(original, 0, top, original.width(), height).to_image();
    let scaled = image::DynamicImage::ImageRgb8(crop).resize(SOURCE.w, SOURCE.h, FilterType::Triangle).to_rgb8();
    let x = SOURCE.x + (SOURCE.w - scaled.width()) / 2;
    let y = SOURCE.y + (SOURCE.h - scaled.height()) / 2;
    imageops::replace(destination, &scaled, i64::from(x), i64::from(y));
    SourceView {
        bounds: Rect { x, y, w: scaled.width(), h: scaled.height() },
        source_size: original.dimensions(),
        crop: Rect { x: 0, y: top, w: original.width(), h: height },
    }
}

fn web_source(
    original: &RgbImage,
    moment: Option<&guides::Moment<'_>>,
    marker: Option<[i32; 4]>,
    full_source: bool,
) -> (RgbImage, SourceView) {
    let (width, height) = original.dimensions();
    let (w, h) = WEB_FOOTAGE;
    let crop_w = width.min((height * w / h).max(1));
    let crop_h = height.min((width * h / w).max(1));
    let centered = SourceView {
        bounds: Rect { x: 0, y: 0, w, h },
        source_size: (width, height),
        crop: Rect { x: (width - crop_w) / 2, y: (height - crop_h) / 2, w: crop_w, h: crop_h },
    };
    let shows_marker = marker.is_none_or(|[x1, y1, x2, y2]| {
        centered.project(Point(x1, y1 - 15)).is_ok() && centered.project(Point(x2, y2)).is_ok()
    });
    if shows_marker && !full_source && moment.is_none_or(|moment| moment.fits(centered)) {
        let c = centered.crop;
        let crop = imageops::crop_imm(original, c.x, c.y, c.w, c.h).to_image();
        return (imageops::resize(&crop, w, h, FilterType::Triangle), centered);
    }
    let scaled = image::DynamicImage::ImageRgb8(original.clone()).resize(w, h, FilterType::Triangle).to_rgb8();
    let (sw, sh) = scaled.dimensions();
    let (x, y) = ((w - sw) / 2, (h - sh) / 2);
    let mut image = RgbImage::from_pixel(w, h, PANEL);
    imageops::replace(&mut image, &scaled, i64::from(x), i64::from(y));
    let view = SourceView {
        bounds: Rect { x, y, w: sw, h: sh },
        source_size: (width, height),
        crop: Rect { x: 0, y: 0, w: width, h: height },
    };
    (image, view)
}

#[cfg(test)]
mod tests {
    use super::sequence_frames;

    #[test]
    fn end_decision_uses_the_last_available_source_frame() {
        assert_eq!(sequence_frames(1000, 1000).unwrap(), (15, 14));
        assert_eq!(sequence_frames(67, 66).unwrap(), (1, 0));
        assert_eq!(sequence_frames(68, 67).unwrap(), (2, 1));
        assert!(sequence_frames(0, 0).is_err());
    }
}
