//! Paints a [`Scene`] built by `rink_core::scene` with GPUI, plus the text
//! helper the timeline shares.

use gpui::{point, px, rgb, rgba, App, Hsla, PathBuilder, Pixels, Point, SharedString, TextRun, Window};
use rink_core::scene::{Scene, Shape};
use rink_core::Vec2;

fn screen(pts: &[Vec2]) -> Vec<Point<Pixels>> {
    pts.iter().map(|p| point(px(p.x), px(p.y))).collect()
}

pub fn paint_scene(window: &mut Window, cx: &mut App, scene: &Scene) {
    for shape in &scene.shapes {
        match shape {
            Shape::Fill { pts, color } => {
                let mut b = PathBuilder::fill();
                b.add_polygon(&screen(pts), true);
                if let Ok(path) = b.build() {
                    window.paint_path(path, rgba(*color));
                }
            }
            Shape::Stroke { pts, closed, width_px, dash, color } => {
                let mut b = PathBuilder::stroke(px(*width_px));
                if let Some([on, off]) = dash {
                    b = b.dash_array(&[px(*on), px(*off)]);
                }
                b.add_polygon(&screen(pts), *closed);
                if let Ok(path) = b.build() {
                    window.paint_path(path, rgba(*color));
                }
            }
            Shape::Text { at, text, size_px, color } => {
                paint_text_rgba(window, cx, point(px(at.x), px(at.y)), text, px(*size_px), rgba(*color).into())
            }
        }
    }
}

/// Paint one line of text centered on `center` (window pixels).
pub fn paint_text(window: &mut Window, cx: &mut App, center: Point<Pixels>, text: &str, font_size: Pixels, hex: u32) {
    paint_text_rgba(window, cx, center, text, font_size, rgb(hex).into());
}

fn paint_text_rgba(
    window: &mut Window,
    cx: &mut App,
    center: Point<Pixels>,
    text: &str,
    font_size: Pixels,
    text_color: Hsla,
) {
    if text.is_empty() {
        return;
    }
    let run = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: text_color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(SharedString::from(text.to_string()), font_size, &[run], None);
    let line_height = font_size * 1.2;
    let origin = point(center.x - line.width / 2.0, center.y - line_height / 2.0);
    line.paint(origin, line_height, window, cx).ok();
}
