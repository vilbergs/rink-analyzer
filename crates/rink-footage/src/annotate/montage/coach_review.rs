//! Build an editable play and coaching export from tracked source footage and checked ice landmarks.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::Path,
};

use super::puck::{self, Mark};
use crate::tracker::Point;
use anyhow::{bail, ensure, Context, Result};
use rink_core::{
    io,
    review::{self, Review, Suggestion},
    Caption, Entity, EntityId, EntityKind, Event, EventKind, Keyframe, Play, RinkSpec, TeamSide, Vec2,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Recipe {
    clip: String,
    image: [u32; 2],
    windows: Vec<Window>,
}

#[derive(Deserialize)]
struct Window {
    start_ms: u32,
    end_ms: u32,
    focus_ms: u32,
    title: String,
    heads_right: bool,
    cue: String,
    condition: String,
    feedback: [String; 3],
    adjustment_m: [f32; 2],
    arrow_label: String,
    reference: Option<[f32; 3]>,
    reference_text: Option<String>,
    calibration: Vec<Camera>,
    #[serde(default)]
    puck_seeds: Vec<PuckSeed>,
}

#[derive(Deserialize)]
struct PuckSeed {
    source_ms: u32,
    xy: [u32; 2],
}

pub(super) struct PuckPass {
    size: [u32; 2],
    fits: Vec<Fit>,
    seeds: BTreeMap<u32, Point>,
    previous: Option<Vec2>,
    step: Vec2,
}

impl PuckPass {
    fn new(window: &Window, fits: Vec<Fit>, size: [u32; 2]) -> Result<Self> {
        let mut seeds = BTreeMap::new();
        for seed in &window.puck_seeds {
            let time = u64::from(seed.source_ms);
            let fps = u64::from(super::FPS);
            let frame = (time * fps + 500) / 1000;
            ensure!(
                (window.start_ms..window.end_ms).contains(&seed.source_ms) && (frame * 1000 + fps / 2) / fps == time,
                "puck seed must be on a source frame inside its window"
            );
            let [x, y] = seed.xy;
            ensure!(x < size[0] && y < size[1], "puck seed outside source image");
            ensure!(seeds.insert(seed.source_ms, Point(x as i32, y as i32)).is_none(), "duplicate puck seed");
        }
        Ok(Self { size, fits, seeds, previous: None, step: Vec2::ZERO })
    }

    pub(super) fn has_seeds(&self) -> bool {
        !self.seeds.is_empty()
    }

    pub(super) fn observe(&mut self, image: &image::RgbImage, time: u32, rink: &RinkSpec) -> Result<Option<Mark>> {
        ensure!(image.dimensions() == self.size.into(), "source frame dimensions disagree with camera calibration");
        let seed = self.seeds.get(&time).copied();
        let previous = self.previous.take();
        if seed.is_none() && previous.is_none() {
            return Ok(None);
        }
        let fit = fit_at(&self.fits, time)?;
        let pixel = if let Some(point) = seed {
            point
        } else if let Some(last) = previous {
            let predicted = last + self.step;
            let [x, y] = image_point(&fit.to_image, self.size, predicted)?;
            if !(0.0..f64::from(self.size[0])).contains(&x) || !(0.0..f64::from(self.size[1])).contains(&y) {
                return Ok(None);
            }
            let Some(point) = puck::locate(image, Point(x.round() as i32, y.round() as i32)) else {
                return Ok(None);
            };
            point
        } else {
            return Ok(None);
        };
        let ice = ice_point(&fit.to_ice, self.size, [f64::from(pixel.0), f64::from(pixel.1)])?;
        if !rink.contains(ice) {
            ensure!(seed.is_none(), "puck seed at {time} ms projects outside the rink");
            return Ok(None);
        }
        self.step = match previous {
            Some(last) if seed.is_none() => {
                let delta = ice - last;
                if delta.length() > 45. / super::FPS as f32 {
                    return Ok(None);
                }
                delta * 0.65 + self.step * 0.35
            }
            _ => Vec2::ZERO,
        };
        self.previous = Some(ice);
        Ok(Some(Mark { pixel, ice }))
    }
}

#[derive(Deserialize)]
struct Camera {
    source_ms: u32,
    /// [rink x, rink y, pixel x, pixel y] in the upright source.
    points: Vec<[f64; 4]>,
    /// [rink x, pixel x1, pixel y1, pixel x2, pixel y2] for a transverse line.
    #[serde(default)]
    lines: Vec<[f64; 5]>,
}

#[derive(Deserialize)]
struct Anchor {
    source_ms: u32,
    track_id: u32,
}

#[derive(Deserialize)]
struct Frame {
    source_ms: u32,
    width: u32,
    height: u32,
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct Track {
    id: u32,
    xyxy: [f32; 4],
    predicted: bool,
}

type Matrix = [[f64; 3]; 3];

struct Fit {
    source_ms: u32,
    to_image: Matrix,
    to_ice: Matrix,
}

pub fn run(args: &[OsString]) -> Result<()> {
    let [recipe, tracks, focus, frames, font, output] = args else {
        bail!("Usage: --coach-review <recipe.json> <tracks.ndjson> <focus.json> <frames-root> <font.ttf> <output-dir>");
    };
    let recipe: Recipe = serde_json::from_reader(BufReader::new(File::open(recipe)?))?;
    let selections: Vec<Anchor> = serde_json::from_reader(BufReader::new(File::open(focus)?))?;
    ensure!(
        !selections.is_empty() && selections.windows(2).all(|w| w[0].source_ms < w[1].source_ms),
        "select a skater before building a review"
    );
    ensure!(!recipe.windows.is_empty(), "review has no windows");
    ensure!(
        !recipe.clip.is_empty() && recipe.clip.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "invalid clip stem"
    );
    let size = recipe.image;
    ensure!(size.into_iter().all(|dimension| dimension >= 100), "invalid source dimensions");
    let mut observations = BTreeMap::new();
    for line in BufReader::new(File::open(tracks)?).lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        let anchor = selections.iter().rev().find(|a| a.source_ms <= frame.source_ms).unwrap_or(&selections[0]);
        let Some(track) = frame.tracks.iter().find(|t| t.id == anchor.track_id && !t.predicted) else {
            continue;
        };
        let bounds = track.xyxy;
        let box_in_image = [0, 1, 2, 3].map(|i| {
            let axis = i % 2;
            let tracked_size = [frame.width, frame.height][axis];
            bounds[i] * size[axis] as f32 / tracked_size as f32
        });
        if box_in_image.iter().all(|value| value.is_finite()) {
            observations.insert(frame.source_ms, box_in_image);
        }
    }
    let output = Path::new(output);
    fs::create_dir(output).context("create new review output directory")?;
    let plays = output.join("plays");
    fs::create_dir(&plays)?;
    let mut manifest = "clip\tstart_ms\tplay\tx1\ty1\tx2\ty2\tcue\tcondition\n".to_owned();
    let mut guides = "clip\tsource_ms\tkind\tx1\ty1\tx2\ty2\tlabel_x\tlabel_y\tlabel\tdetail\n".to_owned();
    let mut feedback = BTreeMap::new();
    let mut clip_index = Vec::new();
    let mut puck_passes = Vec::new();
    for (index, window) in recipe.windows.iter().enumerate() {
        let filename = format!("sequence-{}.rink.json", index + 1);
        let path = plays.join(&filename);
        let (play, review, marker, placement) = make_window(window, size, &observations)?;
        io::save(&play, &path)?;
        review::save_review(&path, &review)?;
        use std::fmt::Write;
        writeln!(
            manifest,
            "{}\t{}\t{filename}\t{}\t{}\t{}\t{}\t{}\t{}",
            recipe.clip,
            window.start_ms,
            marker[0],
            marker[1],
            marker[2],
            marker[3],
            clean(&window.cue),
            clean(&window.condition),
        )?;
        let symbols = write_guides(&mut guides, &recipe.clip, window, &placement, size)?;
        puck_passes.push(PuckPass::new(window, placement.fits, size)?);
        feedback.insert(
            window.focus_ms,
            json!([window.feedback[0], window.feedback[1], window.feedback[2], symbols, window.reference_text]),
        );
        clip_index.push(json!({"file": filename, "title": window.title}));
    }
    let manifest_path = output.join("sequences.tsv");
    let guides_path = output.join("guides.tsv");
    fs::write(&manifest_path, manifest)?;
    fs::write(&guides_path, guides)?;
    let web = output.join("room");
    fs::create_dir(&web)?;
    super::render(
        &[
            frames.clone(),
            plays.into(),
            manifest_path.into(),
            font.clone(),
            web.join("coaching-montage.mp4").into(),
            guides_path.into(),
            web.clone().into(),
        ],
        &mut puck_passes,
    )?;
    let contents = serde_json::to_string(&BTreeMap::from([(recipe.clip, feedback)]))?;
    let index = serde_json::to_string(&Value::Array(clip_index))?;
    fs::write(
        web.join("feedback.js"),
        format!("window.coachingFeedback = {contents};\nwindow.coachingExports = {index};\n"),
    )?;
    for entry in fs::read_dir(output.join("plays"))? {
        let entry = entry?;
        fs::copy(entry.path(), web.join(entry.file_name()))?;
    }
    Ok(())
}

fn clean(text: &str) -> String {
    text.replace(['\t', '\r', '\n'], " ")
}

struct Placement {
    fits: Vec<Fit>,
    map: Matrix,
    actual: Vec2,
    target: Vec2,
}

fn make_window(
    window: &Window,
    size: [u32; 2],
    observations: &BTreeMap<u32, [f32; 4]>,
) -> Result<(Play, Review, [u32; 4], Placement)> {
    let duration = window.end_ms.checked_sub(window.start_ms).context("invalid window times")?;
    let t_ms = window.focus_ms.checked_sub(window.start_ms).context("decision precedes window")?;
    ensure!(window.start_ms % 1000 == 0 && t_ms >= 600 && t_ms < duration, "invalid window or decision");
    ensure!(
        window.calibration.len() >= 2 && window.calibration.windows(2).all(|w| w[0].source_ms < w[1].source_ms),
        "need ordered camera fits"
    );
    ensure!(
        window.calibration[0].source_ms <= window.start_ms
            && window.calibration.last().unwrap().source_ms >= window.end_ms,
        "camera fits must cover the window"
    );
    let fits = window.calibration.iter().map(|camera| fit(camera, size)).collect::<Result<Vec<_>>>()?;
    let mut play = Play::new(window.title.clone(), RinkSpec::iihf());
    play.duration_ms = duration;
    play.teams[0].name = "Selected skater".into();
    play.teams[0].color = "#202936".into();
    play.teams[0].attacks_positive_x = window.heads_right;
    play.teams[1].attacks_positive_x = !window.heads_right;
    let entity = EntityId("YOU".into());
    play.entities.push(Entity {
        id: entity.clone(),
        kind: EntityKind::Skater,
        team: Some(TeamSide::Home),
        number: None,
        role: None,
        label: "Selected skater".into(),
        is_me: true,
    });
    play.notes = "Positions are estimated from the selected track's skates and checked rink landmarks. Camera-side boards appear at the bottom.".into();
    let focus_box =
        *observations.get(&window.focus_ms).context("selected skater must be detected on the decision frame")?;
    let mut keys = Vec::new();
    for (&source_ms, &b) in observations.range(window.start_ms..=window.end_ms) {
        if source_ms != window.focus_ms && source_ms % 200 != 0 {
            continue;
        }
        let point = [f64::from((b[0] + b[2]) / 2.0), f64::from(b[3])];
        let map = fit_at(&fits, source_ms)?;
        let pos = ice_point(&map.to_ice, size, point)?;
        ensure!(play.rink.contains(pos), "projected skater is off the ice at {source_ms} ms ({pos})");
        keys.push(Keyframe::at(source_ms - window.start_ms, pos.x, pos.y));
    }
    ensure!(keys.len() >= 3, "not enough detected skater positions in the window");
    ensure!(
        keys.first().unwrap().t_ms <= 200 && duration - keys.last().unwrap().t_ms <= 250,
        "selected track does not cover the window"
    );
    ensure!(keys.windows(2).all(|w| w[1].t_ms - w[0].t_ms <= 500), "selected track has a gap inside the window");
    let actual = keys.iter().find(|key| key.t_ms == t_ms).context("missing decision location")?.pos;
    let target = actual + Vec2::from_array(window.adjustment_m);
    play.tracks.insert(entity.clone(), keys);
    play.captions.push(Caption { t_ms: 0, text: window.feedback[0].clone() });
    play.captions.push(Caption { t_ms, text: window.cue.clone() });
    let mut decision = Event::new(t_ms, EventKind::DecisionPoint, &[&entity]);
    decision.note = window.cue.clone();
    decision.answer = Some(window.feedback.join(" "));
    play.events.push(decision);
    let mut review = Review::new(&play);
    let mut suggestion = Suggestion {
        id: String::new(),
        entity,
        t_ms,
        branch_from_ms: t_ms.saturating_sub(850),
        target,
        facing: None,
        zone_radius_m: 1.3,
        principle: window.title.clone(),
        reason: window.condition.clone(),
        detail: window.feedback[1].clone(),
        confidence: None,
        play_hash: String::new(),
    };
    if suggestion.check(&play).is_err() {
        suggestion.branch_from_ms = 0;
    }
    review.add(&play, suggestion).context("suggested route must remain on the ice and within skating range")?;
    let marker = [
        focus_box[0].clamp(0., size[0] as f32 - 2.) as u32,
        focus_box[1].clamp(0., size[1] as f32 - 2.) as u32,
        focus_box[2].clamp(1., size[0] as f32 - 1.) as u32,
        focus_box[3].clamp(1., size[1] as f32 - 1.) as u32,
    ];
    Ok((play, review, marker, Placement { map: fit_at(&fits, window.focus_ms)?.to_image, actual, target, fits }))
}

fn write_guides(
    output: &mut String,
    clip: &str,
    window: &Window,
    placement: &Placement,
    size: [u32; 2],
) -> Result<Vec<&'static str>> {
    use std::fmt::Write;
    let actual = image_point(&placement.map, size, placement.actual)?;
    let target = image_point(&placement.map, size, placement.target)?;
    let time = window.focus_ms;
    let mut symbols = vec!["player"];
    if let Some([x, y1, y2]) = window.reference {
        let from = image_point(&placement.map, size, Vec2::new(x, y1))?;
        let to = image_point(&placement.map, size, Vec2::new(x, y2))?;
        if let Some([from, to]) = visible_segment(from, to, size) {
            let name = if x == 0.0 { "CENTER RED LINE" } else { "RINK LINE" };
            writeln!(
                output,
                "{clip}\t{time}\treference\t{}\t{}\t{}\t{}\t{}\t{}\t{name}\t",
                from[0],
                from[1],
                to[0],
                to[1],
                (to[0] + 52).min(size[0] - 1),
                to[1],
            )?;
            symbols.push("reference");
        }
    }
    if let Some([actual, target]) = visible_segment(actual, target, size) {
        let label_x = if target[0] < actual[0] { target[0].saturating_sub(175) } else { target[0] + 16 };
        writeln!(
            output,
            "{clip}\t{time}\tadjustment\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t",
            actual[0],
            actual[1],
            target[0],
            target[1],
            label_x.min(size[0] - 1),
            (target[1] + 32).min(size[1] - 1),
            clean(&window.arrow_label)
        )?;
        symbols.push("move");
    }
    symbols.extend(["route", "ghost"]);
    Ok(symbols)
}

const RINK_SCALE: f64 = 30.0;

fn fit(camera: &Camera, size: [u32; 2]) -> Result<Fit> {
    let [w, h] = size.map(f64::from);
    let mut rows = Vec::new();
    for &[x, y, px, py] in &camera.points {
        let (x, y, u, v) = (x / RINK_SCALE, y / RINK_SCALE, px / w, py / h);
        rows.push([x, y, 1., 0., 0., 0., -u * x, -u * y, u]);
        rows.push([0., 0., 0., x, y, 1., -v * x, -v * y, v]);
    }
    for &[x, x1, y1, x2, y2] in &camera.lines {
        let [u1, v1, u2, v2] = [x1 / w, y1 / h, x2 / w, y2 / h];
        let a = v1 - v2;
        let b = u2 - u1;
        let norm = a.hypot(b);
        ensure!(norm > 0.03, "short calibration line at {}", camera.source_ms);
        let (a, b, c) = (a / norm, b / norm, (u1 * v2 - u2 * v1) / norm);
        let x = x / RINK_SCALE;
        // Both ends of the rink marking must project onto the image line.
        // Give these extrapolated residuals less weight than a checked point.
        let half_width = f64::from(RinkSpec::iihf().half_width()) / RINK_SCALE;
        for y in [-half_width, half_width] {
            rows.push([a * x, a * y, a, b * x, b * y, b, c * x, c * y, -c].map(|v| v * 0.5));
        }
    }
    ensure!(rows.len() >= 8, "not enough landmark constraints at {}", camera.source_ms);
    let p = least_squares(rows).with_context(|| format!("ambiguous camera fit at {} ms", camera.source_ms))?;
    let to_image = [[p[0], p[1], p[2]], [p[3], p[4], p[5]], [p[6], p[7], 1.]];
    let to_ice = inverse(to_image)?;
    for &[x, y, u, v] in &camera.points {
        let [px, py] = apply(&to_image, [x / RINK_SCALE, y / RINK_SCALE])?;
        let error = (px * w - u).hypot(py * h - v);
        ensure!(
            error <= 9.0,
            "landmark ({x}, {y}) at {} ms misses its marked pixel by {error:.1} px",
            camera.source_ms
        );
    }
    Ok(Fit { source_ms: camera.source_ms, to_image, to_ice })
}

// Fold each normalized landmark equation into the triangular factor with
// Givens rotations. Forming AᵀA would square the condition number of the fit.
fn least_squares(rows: Vec<[f64; 9]>) -> Option<[f64; 8]> {
    let mut triangular = [[0_f64; 9]; 8];
    for mut row in rows {
        for (col, pivot) in triangular.iter_mut().enumerate() {
            if row[col] == 0. {
                continue;
            }
            let norm = pivot[col].hypot(row[col]);
            let (cos, sin) = (pivot[col] / norm, row[col] / norm);
            for (upper, lower) in pivot[col..].iter_mut().zip(&mut row[col..]) {
                (*upper, *lower) = (cos * *upper + sin * *lower, cos * *lower - sin * *upper);
            }
        }
    }
    let scale = triangular.iter().enumerate().map(|(i, row)| row[i].abs()).fold(0., f64::max);
    let mut solution = [0.; 8];
    for i in (0..8).rev() {
        let row = &triangular[i];
        if row[i].abs() <= scale * 1e-7 {
            return None;
        }
        let known = (i + 1..8).map(|j| row[j] * solution[j]).sum::<f64>();
        solution[i] = (row[8] - known) / row[i];
    }
    Some(solution)
}

fn fit_at(fits: &[Fit], time: u32) -> Result<Fit> {
    let i = fits.partition_point(|fit| fit.source_ms <= time);
    let a = &fits[i.saturating_sub(1)];
    let b = fits.get(i).unwrap_or(a);
    let t = if b.source_ms > a.source_ms {
        f64::from(time - a.source_ms) / f64::from(b.source_ms - a.source_ms)
    } else {
        0.
    };
    let to_image = std::array::from_fn(|row| {
        std::array::from_fn(|col| a.to_image[row][col] * (1. - t) + b.to_image[row][col] * t)
    });
    Ok(Fit { source_ms: time, to_image, to_ice: inverse(to_image)? })
}

fn image_point(matrix: &Matrix, size: [u32; 2], pos: Vec2) -> Result<[f64; 2]> {
    let [u, v] = apply(matrix, [f64::from(pos.x) / RINK_SCALE, f64::from(pos.y) / RINK_SCALE])?;
    Ok([u * f64::from(size[0]), v * f64::from(size[1])])
}

// Clip along the line itself so a guide crossing a frame edge retains its direction.
fn visible_segment(a: [f64; 2], b: [f64; 2], size: [u32; 2]) -> Option<[[u32; 2]; 2]> {
    let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
    for axis in 0..2 {
        let limit = f64::from(size[axis] - 1);
        let step = b[axis] - a[axis];
        if step.abs() < 1e-10 {
            if !(0.0..=limit).contains(&a[axis]) {
                return None;
            }
        } else {
            let t0 = -a[axis] / step;
            let t1 = (limit - a[axis]) / step;
            enter = enter.max(t0.min(t1));
            leave = leave.min(t0.max(t1));
        }
    }
    (enter < leave).then(|| [enter, leave].map(|t| std::array::from_fn(|i| (a[i] + t * (b[i] - a[i])).round() as u32)))
}

fn ice_point(matrix: &Matrix, size: [u32; 2], point: [f64; 2]) -> Result<Vec2> {
    let [x, y] = apply(matrix, [point[0] / f64::from(size[0]), point[1] / f64::from(size[1])])?;
    Ok(Vec2::new((x * RINK_SCALE) as f32, (y * RINK_SCALE) as f32))
}

fn apply(m: &Matrix, [x, y]: [f64; 2]) -> Result<[f64; 2]> {
    let [u, v, s] = m.map(|row| row[0] * x + row[1] * y + row[2]);
    ensure!(s.abs() > 1e-9, "point is on the camera horizon");
    Ok([u / s, v / s])
}

fn inverse([[a, b, c], [d, e, f], [g, h, i]]: Matrix) -> Result<Matrix> {
    let adj = [
        [e * i - f * h, c * h - b * i, b * f - c * e],
        [f * g - d * i, a * i - c * g, c * d - a * f],
        [d * h - e * g, b * g - a * h, a * e - b * d],
    ];
    let det = a * adj[0][0] + b * adj[1][0] + c * adj[2][0];
    ensure!(det.abs() > 1e-9, "degenerate camera transform");
    Ok(adj.map(|row| row.map(|value| value / det)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_crossings_preserve_the_projected_line() {
        assert_eq!(visible_segment([-10., 0.], [10., 20.], [100, 100]), Some([[0, 10], [10, 20]]));
        assert_eq!(visible_segment([-10., 0.], [-5., 20.], [100, 100]), None);
    }

    #[test]
    fn stationary_puck_follows_camera_pan_and_can_be_reseeded_after_loss() {
        use image::{Rgb, RgbImage};
        let size = [200, 100];
        let fits = [(0, 0.4), (67, 0.6)].map(|(source_ms, offset)| {
            let to_image = [[0.5, 0., offset], [0., 0.5, 0.5], [0., 0., 1.]];
            Fit { source_ms, to_image, to_ice: inverse(to_image).unwrap() }
        });
        let mut pass = PuckPass {
            size,
            fits: fits.into(),
            seeds: BTreeMap::from([(0, Point(80, 50)), (267, Point(120, 50))]),
            previous: None,
            step: Vec2::ZERO,
        };
        let rink = RinkSpec::iihf();
        let blank = RgbImage::from_pixel(200, 100, Rgb([200; 3]));
        pass.observe(&blank, 0, &rink).unwrap().unwrap();
        let mut visible = blank.clone();
        visible.put_pixel(120, 50, Rgb([80; 3]));
        assert!(pass.observe(&visible, 67, &rink).unwrap().unwrap().ice.length() < 0.001);
        assert!(pass.observe(&blank, 133, &rink).unwrap().is_none());
        assert!(pass.observe(&visible, 200, &rink).unwrap().is_none());
        assert!(pass.observe(&visible, 267, &rink).unwrap().is_some());
    }

    #[test]
    fn camera_fit_preserves_near_side_and_left_to_right_coordinates() {
        let reference = [[0.4, 0.08, 0.5], [0.02, -0.38, 0.53], [0.06, 0.28, 1.0]];
        let size = [900, 506];
        let points = [(-7.14, 15.), (0., 15.), (7.14, 15.), (-5.64, -7.), (5.64, -7.)]
            .into_iter()
            .map(|(x, y)| {
                let [u, v] = apply(&reference, [x / RINK_SCALE, y / RINK_SCALE]).unwrap();
                [x, y, u * f64::from(size[0]), v * f64::from(size[1])]
            })
            .collect();
        let fit = fit(&Camera { source_ms: 0, points, lines: vec![] }, size).unwrap();
        let expected = Vec2::new(-3.25, -6.5);
        let [u, v] =
            apply(&reference, [f64::from(expected.x) / RINK_SCALE, f64::from(expected.y) / RINK_SCALE]).unwrap();
        let mapped = ice_point(&fit.to_ice, size, [u * f64::from(size[0]), v * f64::from(size[1])]).unwrap();
        assert!(mapped.distance(expected) < 0.01);
    }

    #[test]
    fn transverse_line_directions_constrain_the_camera_fit() {
        let size = [900, 500];
        let image = |x: f64, y: f64| [450. + 10. * x + 3. * y, 250. + x - 8. * y];
        let points = [(-10., 6.), (0., -4.), (9., 7.)].map(|(x, y)| {
            let [u, v] = image(x, y);
            [x, y, u, v]
        });
        let lines = [-7.14].map(|x| {
            let [u1, v1] = image(x, -15.);
            let [u2, v2] = image(x, 15.);
            [x, u1, v1, u2, v2]
        });
        let fit = fit(&Camera { source_ms: 0, points: points.into(), lines: lines.into() }, size).unwrap();
        let mapped = ice_point(&fit.to_ice, size, image(12., -9.)).unwrap();
        assert!(mapped.distance(Vec2::new(12., -9.)) < 0.01);
    }

    #[test]
    fn collinear_marks_cannot_calibrate_the_ice() {
        let points = [-7.14, 0., 7.14, 10.].map(|x| [x, 15., 450. + 20. * x, 140.]).to_vec();
        assert!(fit(&Camera { source_ms: 0, points, lines: vec![] }, [900, 506]).is_err());
    }
}
