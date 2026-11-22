use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use anyhow::{Context, Result};
use image::{Rgb, RgbImage};
use rink_core::{
    scene::{Scene, Shape},
    Vec2,
};
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Stroke, StrokeDash, Transform};

use super::super::{overlay::text, tracker::Point};

fn paint(color: u32) -> Paint<'static> {
    let [r, g, b, a] = color.to_be_bytes();
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;
    paint
}

fn path(points: &[Vec2], closed: bool) -> Option<Path> {
    let (first, rest) = points.split_first()?;
    let mut path = PathBuilder::new();
    path.move_to(first.x, first.y);
    for point in rest {
        path.line_to(point.x, point.y);
    }
    if closed {
        path.close();
    }
    path.finish()
}

pub fn draw(scene: &Scene, font: &FontArc, width: u32, height: u32) -> Result<RgbImage> {
    let mut pixels = Pixmap::new(width, height).context("invalid rink viewport")?;
    pixels.fill(tiny_skia::Color::from_rgba8(20, 35, 51, 255));
    for shape in &scene.shapes {
        match shape {
            Shape::Fill { pts, color } => {
                if let Some(path) = path(pts, true) {
                    pixels.fill_path(&path, &paint(*color), FillRule::Winding, Transform::identity(), None);
                }
            }
            Shape::Stroke { pts, closed, width_px, dash, color } => {
                if let Some(path) = path(pts, *closed) {
                    let stroke = Stroke {
                        width: *width_px,
                        dash: dash.and_then(|pair| StrokeDash::new(pair.to_vec(), 0.0)),
                        ..Stroke::default()
                    };
                    pixels.stroke_path(&path, &paint(*color), &stroke, Transform::identity(), None);
                }
            }
            Shape::Text { .. } => {}
        }
    }
    let rgb: Vec<u8> = pixels.data().as_chunks::<4>().0.iter().flat_map(|rgba| [rgba[0], rgba[1], rgba[2]]).collect();
    let mut image = RgbImage::from_raw(width, height, rgb).context("invalid rink pixels")?;
    for shape in &scene.shapes {
        if let Shape::Text { at, text: label, size_px, color } = shape {
            let scaled = font.as_scaled(PxScale::from(*size_px));
            let width: f32 = label.chars().map(|ch| scaled.h_advance(scaled.glyph_id(ch))).sum();
            let [r, g, b, _] = color.to_be_bytes();
            text(
                &mut image,
                font,
                label,
                Point((at.x - width / 2.0).round() as i32, (at.y + size_px * 0.30).round() as i32),
                *size_px,
                Rgb([r, g, b]),
            );
        }
    }
    Ok(image)
}
