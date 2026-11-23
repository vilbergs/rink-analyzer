//! Line up a camera frame with the rink from detected rink keypoints.
//!
//! `rink-align <keypoints.json> <detections.ndjson> <frames-dir> <out-dir>` reads HockeyRink
//! detections (one JSON line per frame: `frame`, `landmarks` of `{id, xy, confidence}`), maps each
//! keypoint id to its rink position from the keypoints table, and fits the rink-to-image
//! homography with RANSAC so a few wrong keypoints do not spoil it. It prints one JSON line per
//! frame with the fit's quality, and writes each fitted frame to `<out-dir>` with the rink drawn
//! back on it, to check by eye.
use std::{collections::HashMap, fs, path::Path};

use anyhow::{bail, Context, Result};
use image::{Rgb, RgbImage};
use serde::Deserialize;
use serde_json::json;

type Matrix = [[f64; 3]; 3];

#[derive(Deserialize)]
struct Table {
    keypoints: Vec<Keypoint>,
}

#[derive(Deserialize)]
struct Keypoint {
    id: usize,
    x: f64,
    y: f64,
}

#[derive(Deserialize)]
struct Detection {
    frame: String,
    landmarks: Vec<Landmark>,
}

#[derive(Deserialize)]
struct Landmark {
    id: usize,
    xy: [f64; 2],
}

/// Rink points and where they are in the image.
type Pair = ([f64; 2], [f64; 2]);

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [table, detections, frames, out] = args.as_slice() else {
        bail!("Usage: rink-align <keypoints.json> <detections.ndjson> <frames-dir> <out-dir>");
    };
    let table: Table = serde_json::from_slice(&fs::read(table)?)?;
    let rink: HashMap<usize, [f64; 2]> = table.keypoints.iter().map(|k| (k.id, [k.x, k.y])).collect();
    fs::create_dir_all(out)?;
    for line in fs::read_to_string(detections)?.lines() {
        let detection: Detection = serde_json::from_str(line)?;
        let pairs: Vec<Pair> =
            detection.landmarks.iter().filter_map(|l| rink.get(&l.id).map(|&ice| (ice, l.xy))).collect();
        let path = Path::new(frames).join(&detection.frame);
        let mut image = image::open(&path).with_context(|| format!("reading {}", path.display()))?.to_rgb8();
        let tolerance = f64::from(image.width()) * 0.02;
        let Some((h, inliers, rms)) = ransac(&pairs, tolerance) else {
            println!("{}", json!({ "frame": detection.frame, "keypoints": pairs.len(), "fit": false }));
            continue;
        };
        let near_below_far = project(&h, [0.0, -15.0]).zip(project(&h, [0.0, 15.0])).is_some_and(|(n, f)| n[1] > f[1]);
        draw_rink(&mut image, &h);
        for (_, [u, v]) in &pairs {
            dot(&mut image, *u, *v, Rgb([255, 214, 0]));
        }
        image.save(Path::new(out).join(&detection.frame))?;
        println!(
            "{}",
            json!({ "frame": detection.frame, "keypoints": pairs.len(), "inliers": inliers,
                    "rms_px": (rms * 10.0).round() / 10.0, "near_below_far": near_below_far, "fit": true })
        );
    }
    Ok(())
}

/// The homography most keypoints agree with: fit every 4-point subset (or a sample of them),
/// keep the one with the most points within `tolerance` pixels, then refit on those points.
fn ransac(pairs: &[Pair], tolerance: f64) -> Option<(Matrix, usize, f64)> {
    if pairs.len() < 4 {
        return None;
    }
    let mut best: Option<(Vec<usize>, Matrix)> = None;
    let mut seed = 0x2545_f491_u64;
    let n = pairs.len();
    for _ in 0..400 {
        let mut pick = [0usize; 4];
        for i in 0..4 {
            loop {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let c = (seed % n as u64) as usize;
                if !pick[..i].contains(&c) {
                    pick[i] = c;
                    break;
                }
            }
        }
        let Some(h) = solve(&pick.map(|i| pairs[i])) else { continue };
        let inliers: Vec<usize> = (0..n).filter(|&i| error(&h, &pairs[i]).is_some_and(|e| e < tolerance)).collect();
        if best.as_ref().is_none_or(|(b, _)| inliers.len() > b.len()) {
            best = Some((inliers, h));
        }
    }
    let (inliers, _) = best?;
    if inliers.len() < 4 {
        return None;
    }
    let chosen: Vec<Pair> = inliers.iter().map(|&i| pairs[i]).collect();
    let h = solve(&chosen)?;
    let rms = (chosen.iter().map(|p| error(&h, p).unwrap_or(1e6).powi(2)).sum::<f64>() / chosen.len() as f64).sqrt();
    Some((h, inliers.len(), rms))
}

/// Least-squares direct linear transform, with h33 = 1.
fn solve(pairs: &[Pair]) -> Option<Matrix> {
    let mut ata = [[0.0; 8]; 8];
    let mut atb = [0.0; 8];
    for ([x, y], [u, v]) in pairs {
        for (row, rhs) in
            [([*x, *y, 1., 0., 0., 0., -u * x, -u * y], *u), ([0., 0., 0., *x, *y, 1., -v * x, -v * y], *v)]
        {
            for i in 0..8 {
                atb[i] += row[i] * rhs;
                for j in 0..8 {
                    ata[i][j] += row[i] * row[j];
                }
            }
        }
    }
    // Gaussian elimination with partial pivoting.
    for col in 0..8 {
        let pivot = (col..8).max_by(|&a, &b| ata[a][col].abs().total_cmp(&ata[b][col].abs()))?;
        if ata[pivot][col].abs() < 1e-9 {
            return None;
        }
        ata.swap(col, pivot);
        atb.swap(col, pivot);
        for row in 0..8 {
            if row != col {
                let f = ata[row][col] / ata[col][col];
                for k in col..8 {
                    ata[row][k] -= f * ata[col][k];
                }
                atb[row] -= f * atb[col];
            }
        }
    }
    let p: Vec<f64> = (0..8).map(|i| atb[i] / ata[i][i]).collect();
    Some([[p[0], p[1], p[2]], [p[3], p[4], p[5]], [p[6], p[7], 1.0]])
}

fn project(h: &Matrix, [x, y]: [f64; 2]) -> Option<[f64; 2]> {
    let w = h[2][0] * x + h[2][1] * y + h[2][2];
    (w > 1e-9).then(|| [(h[0][0] * x + h[0][1] * y + h[0][2]) / w, (h[1][0] * x + h[1][1] * y + h[1][2]) / w])
}

fn error(h: &Matrix, (ice, [u, v]): &Pair) -> Option<f64> {
    project(h, *ice).map(|[pu, pv]| (pu - u).hypot(pv - v))
}

/// The rink's markings, as polylines in rink metres.
fn markings() -> Vec<(Vec<[f64; 2]>, Rgb<u8>)> {
    let (red, blue, white) = (Rgb([230, 40, 40]), Rgb([40, 90, 255]), Rgb([255, 255, 255]));
    let line = |x: f64, color| ((0..=30).map(|i| [x, -15.0 + i as f64]).collect(), color);
    let circle = |cx: f64, cy: f64, color| {
        (
            (0..=48)
                .map(|i| {
                    let a = i as f64 / 48.0 * std::f64::consts::TAU;
                    [cx + 4.5 * a.cos(), cy + 4.5 * a.sin()]
                })
                .collect(),
            color,
        )
    };
    let mut boards = Vec::new();
    for (cx, cy, from) in [(21.5, 6.5, 0.0), (-21.5, 6.5, 90.0), (-21.5, -6.5, 180.0), (21.5, -6.5, 270.0)] {
        for i in 0..=12 {
            let a = (from + i as f64 * 7.5_f64).to_radians();
            boards.push([cx + 8.5 * a.cos(), cy + 8.5 * a.sin()]);
        }
    }
    boards.push(boards[0]);
    let mut all = vec![(boards, white), line(0.0, red), line(-7.14, blue), line(7.14, blue)];
    all.push(((0..=20).map(|i| [-26.0, -13.7 + i as f64 * 1.37]).collect(), red));
    all.push(((0..=20).map(|i| [26.0, -13.7 + i as f64 * 1.37]).collect(), red));
    all.push(circle(0.0, 0.0, blue));
    for (x, y) in [(-20.0, 7.0), (-20.0, -7.0), (20.0, 7.0), (20.0, -7.0)] {
        all.push(circle(x, y, red));
    }
    all
}

fn draw_rink(image: &mut RgbImage, h: &Matrix) {
    for (points, color) in markings() {
        for pair in points.windows(2) {
            if let (Some(a), Some(b)) = (project(h, pair[0]), project(h, pair[1])) {
                segment(image, a, b, color);
            }
        }
    }
}

fn segment(image: &mut RgbImage, [x0, y0]: [f64; 2], [x1, y1]: [f64; 2], color: Rgb<u8>) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).ceil().min(5000.0) as usize;
    for s in 0..=steps {
        let t = if steps == 0 { 0.0 } else { s as f64 / steps as f64 };
        let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let (px, py) = (x as i64 + dx, y as i64 + dy);
            if px >= 0 && py >= 0 && px < i64::from(image.width()) && py < i64::from(image.height()) {
                image.put_pixel(px as u32, py as u32, color);
            }
        }
    }
}

fn dot(image: &mut RgbImage, u: f64, v: f64, color: Rgb<u8>) {
    for dy in -5..=5_i64 {
        for dx in -5..=5_i64 {
            let (x, y) = (u as i64 + dx, v as i64 + dy);
            if dx * dx + dy * dy <= 25
                && x >= 0
                && y >= 0
                && x < i64::from(image.width())
                && y < i64::from(image.height())
            {
                image.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}
