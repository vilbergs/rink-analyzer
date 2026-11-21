use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use image::{Rgb, RgbImage};

use crate::tracker::Point;

pub const WHITE: Rgb<u8> = Rgb([251, 252, 255]);
pub const DARK: Rgb<u8> = Rgb([14, 24, 43]);
pub const WEAK: Rgb<u8> = Rgb([255, 178, 56]);

pub fn bar(image: &mut RgbImage, top: u32, bottom: u32) {
    let width = image.width();
    for y in top..bottom.min(image.height()) {
        for x in 0..width {
            let background = image.get_pixel_mut(x, y);
            for (channel, panel) in background.0.iter_mut().zip(DARK.0) {
                *channel = ((u16::from(*channel) + 7 * u16::from(panel)) / 8) as u8;
            }
        }
    }
}

fn pixel(image: &mut RgbImage, x: i32, y: i32, color: Rgb<u8>) {
    if x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32 {
        image.put_pixel(x as u32, y as u32, color);
    }
}

pub fn text(image: &mut RgbImage, font: &FontArc, value: &str, at: Point, size: f32, color: Rgb<u8>) {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut x = at.0 as f32;
    let mut previous = None;
    for ch in value.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(previous) = previous {
            x += scaled.kern(previous, id);
        }
        let glyph = id.with_scale_and_position(size, point(x, at.1 as f32));
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|dx, dy, alpha| {
                let x = bounds.min.x as i32 + dx as i32;
                let y = bounds.min.y as i32 + dy as i32;
                if x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32 {
                    let destination = image.get_pixel_mut(x as u32, y as u32);
                    for (channel, source) in destination.0.iter_mut().zip(color.0) {
                        *channel = (f32::from(*channel) * (1.0 - alpha) + f32::from(source) * alpha) as u8;
                    }
                }
            });
        }
        x += scaled.h_advance(id);
        previous = Some(id);
    }
}

pub fn window(image: &mut RgbImage, Point(x, y): Point, color: Rgb<u8>) {
    const SIZE: i32 = 16;
    for offset in -SIZE..=SIZE {
        for edge in [-SIZE, SIZE] {
            pixel(image, x + edge, y + offset, DARK);
            pixel(image, x + offset, y + edge, DARK);
        }
    }
    for offset in -(SIZE - 1)..(SIZE - 1) {
        for edge in [-(SIZE - 1), SIZE - 1] {
            pixel(image, x + edge, y + offset, color);
            pixel(image, x + offset, y + edge, color);
        }
    }
    for offset in -2..=2 {
        pixel(image, x + offset, y, color);
        pixel(image, x, y + offset, color);
    }
}

pub fn checkpoint(image: &mut RgbImage, Point(x, y): Point) {
    for offset in -9..=9 {
        for edge in -2..=2 {
            pixel(image, x + edge, y + offset, DARK);
            pixel(image, x + offset, y + edge, DARK);
        }
    }
    for offset in -8..=8 {
        pixel(image, x, y + offset, WHITE);
        pixel(image, x + offset, y, WHITE);
    }
}

pub fn error_line(image: &mut RgbImage, from: Point, to: Point, color: Rgb<u8>) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let length = dx.abs().max(dy.abs());
    for step in 0..length {
        if step % 7 < 4 {
            pixel(image, from.0 + dx * step / length, from.1 + dy * step / length, color);
        }
    }
}

pub fn rect_label(
    image: &mut RgbImage,
    font: &FontArc,
    label: &str,
    Point(left, top): Point,
    Point(right, bottom): Point,
    color: Rgb<u8>,
    predicted: bool,
) {
    let left = left.max(0);
    let top = top.max(0);
    let right = right.min(image.width() as i32 - 1);
    let bottom = bottom.min(image.height() as i32 - 1);
    if right <= left || bottom <= top {
        return;
    }
    for x in left..=right {
        if !predicted || (x - left) % 8 < 4 {
            for y in [top, bottom] {
                pixel(image, x, y, DARK);
                pixel(image, x, y + 1, color);
            }
        }
    }
    for y in top..=bottom {
        if !predicted || (y - top) % 8 < 4 {
            for x in [left + 1, right] {
                pixel(image, x, y, DARK);
                pixel(image, x - 1, y, color);
            }
        }
    }
    let x = left.min(image.width() as i32 - 48);
    let y = (top - 18).max(54);
    let width = (label.len() as i32 * 10 + 6).max(32);
    for row in y..y + 17 {
        for col in x..x + width {
            pixel(image, col, row, DARK);
        }
    }
    text(image, font, label, Point(x + 3, y + 13), 17.0, color);
}
