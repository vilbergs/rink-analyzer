//! Find isolated dark components on locally bright ice around a projected prediction.

use crate::tracker::Point;
use image::RgbImage;
use rink_core::{
    scene::{Scene, Shape},
    transform::RinkTransform,
    Vec2,
};

#[derive(Clone, Copy)]
pub(super) struct Mark {
    pub pixel: Point,
    pub ice: Vec2,
}

impl Mark {
    pub fn draw_on_rink(self, scene: &mut Scene, transform: &RinkTransform) {
        let center = transform.to_screen(self.ice);
        for (radius, color) in [(5., 0x83ebedff), (3.2, 0x14212aff)] {
            let pts = (0..20)
                .map(|i| {
                    let (sin, cos) = (i as f32 * std::f32::consts::TAU / 20.).sin_cos();
                    center + Vec2::new(cos, sin) * radius
                })
                .collect();
            scene.shapes.push(Shape::Fill { pts, color });
        }
    }
}

// Keep a small native-resolution crop for inspecting the evidence in the room.
// Centering and stretching the ice tones here avoids magnifying the encoded overlay.
pub(super) fn detail(image: &RgbImage, Point(x, y): Point) -> String {
    use std::fmt::Write;
    let mut samples = [0u8; 40 * 24];
    for (i, sample) in samples.iter_mut().enumerate() {
        let px = (x + (i % 40) as i32 - 20).clamp(0, image.width() as i32 - 1);
        let py = (y + (i / 40) as i32 - 12).clamp(0, image.height() as i32 - 1);
        *sample = luminance(image.get_pixel(px as u32, py as u32).0);
    }
    let mut sorted = samples;
    let upper = i32::from(*sorted.select_nth_unstable(40 * 24 * 3 / 4).1);
    let mut encoded = String::with_capacity(samples.len() * 2);
    for value in samples {
        let level = ((i32::from(value) - upper + 100) * 255 / 100).clamp(0, 255);
        write!(encoded, "{level:02x}").unwrap();
    }
    encoded
}

fn luminance(pixel: [u8; 3]) -> u8 {
    let [r, g, b] = pixel.map(u32::from);
    ((54 * r + 183 * g + 19 * b) >> 8) as u8
}

pub(super) fn locate(image: &RgbImage, prediction: Point) -> Option<Point> {
    let radius = (image.width() / 30).clamp(10, 64) as i32;
    let diameter = (image.width() / 140).clamp(4, 18) as i32;
    let (width, height) = image.dimensions();
    let left = prediction.0.saturating_sub(radius).max(0);
    let top = prediction.1.saturating_sub(radius).max(0);
    let right = prediction.0.saturating_add(radius).min(width as i32 - 1);
    let bottom = prediction.1.saturating_add(radius).min(height as i32 - 1);
    if left >= right || top >= bottom {
        return None;
    }
    let stride = (right - left + 1) as usize;
    let mut gray = Vec::new();
    for y in top..=bottom {
        for x in left..=right {
            gray.push(luminance(image.get_pixel(x as u32, y as u32).0));
        }
    }
    let mut sorted = gray.clone();
    let mid = sorted.len() / 2;
    let ice = *sorted.select_nth_unstable(mid).1;
    let threshold = ice.saturating_sub(24);
    let mut dark: Vec<_> = gray.iter().map(|&v| v < threshold).collect();
    let mut candidates = Vec::new();
    let mut stack = Vec::new();
    for i in 0..dark.len() {
        if !std::mem::take(&mut dark[i]) {
            continue;
        }
        stack.push(i);
        let (mut x1, mut y1, mut x2, mut y2) = (right, bottom, left, top);
        let mut darkest = i;
        let mut area = 0;
        while let Some(j) = stack.pop() {
            let x = left + (j % stride) as i32;
            let y = top + (j / stride) as i32;
            x1 = x1.min(x);
            x2 = x2.max(x);
            y1 = y1.min(y);
            y2 = y2.max(y);
            area += 1;
            if gray[j] < gray[darkest] {
                darkest = j;
            }
            for ny in (y - 1).max(top)..=(y + 1).min(bottom) {
                for nx in (x - 1).max(left)..=(x + 1).min(right) {
                    let n = (ny - top) as usize * stride + (nx - left) as usize;
                    if std::mem::take(&mut dark[n]) {
                        stack.push(n);
                    }
                }
            }
        }
        let (w, h) = (x2 - x1 + 1, y2 - y1 + 1);
        if x1 == left
            || x2 == right
            || y1 == top
            || y2 == bottom
            || w > diameter
            || h > diameter
            || h > w + 1
            || w > 4 * h
            || area * 2 < w * h
        {
            continue;
        }
        let point = Point((x1 + x2) / 2, (y1 + y2) / 2);
        let distance = point.distance(prediction);
        if distance > radius as f32 {
            continue;
        }
        let score = f32::from(ice - gray[darkest]).min(110.) - 2.5 * distance;
        candidates.push((score, point));
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let &(score, best) = candidates.first()?;
    (score >= 22. && candidates.get(1).is_none_or(|next| score - next.0 >= 10.)).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn isolates_puck_from_stick_and_abstains_on_occlusion_or_ambiguity() {
        let mut frame = RgbImage::from_pixel(120, 80, Rgb([183, 180, 175]));
        for y in 25..50 {
            frame.put_pixel(54, y, Rgb([40; 3]));
        }
        assert_eq!(locate(&frame, Point(60, 40)), None);
        for x in 62..65 {
            frame.put_pixel(x, 40, Rgb([95; 3]));
        }
        assert_eq!(locate(&frame, Point(60, 40)), Some(Point(63, 40)));
        for x in 56..59 {
            frame.put_pixel(x, 40, Rgb([95; 3]));
        }
        assert_eq!(locate(&frame, Point(60, 40)), None);
    }
}
