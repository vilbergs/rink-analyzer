use std::collections::BTreeMap;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use anyhow::{bail, ensure, Context, Result};
use image::{Rgb, RgbImage};
use rink_core::Vec2;
use tiny_skia::{FillRule, LineCap, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, StrokeDash, Transform};

use super::{SourceView, GOLD, GREEN};
use crate::{overlay::text, tracker::Point};

const INK: Rgb<u8> = Rgb([15, 26, 40]);
const BLUE: Rgb<u8> = Rgb([175, 222, 255]);

#[derive(Clone, Copy)]
enum Kind {
    Lane,
    Reference,
    Adjustment,
    Spacing,
    Player,
}

struct Guide<'a> {
    kind: Kind,
    from: Point,
    to: Point,
    label_at: Point,
    label: &'a str,
}

#[derive(Default)]
pub struct Moment<'a> {
    pub cue: Option<[&'a str; 2]>,
    guides: Vec<Guide<'a>>,
}

impl Moment<'_> {
    pub fn fits(&self, view: SourceView) -> bool {
        self.guides.iter().all(|guide| {
            view.project(guide.from).is_ok()
                && view.project(guide.to).is_ok()
                && (guide.label.is_empty()
                    || matches!(guide.kind, Kind::Player)
                    || view.project(guide.label_at).is_ok())
        })
    }
}

pub type Moments<'a> = BTreeMap<(&'a str, u32), Moment<'a>>;

pub fn parse(input: &str) -> Result<Moments<'_>> {
    let mut lines = input.lines();
    ensure!(
        lines.next() == Some("clip\tsource_ms\tkind\tx1\ty1\tx2\ty2\tlabel_x\tlabel_y\tlabel\tdetail"),
        "invalid coaching guides TSV header"
    );
    let mut moments = Moments::new();
    for (index, line) in lines.enumerate() {
        let fields: Vec<_> = line.split('\t').collect();
        let &[clip, time, kind, x1, y1, x2, y2, lx, ly, label, detail] = fields.as_slice() else {
            bail!("invalid coaching guide on line {}", index + 2);
        };
        let moment = moments.entry((clip, time.parse()?)).or_default();
        if kind == "hold" {
            ensure!(moment.cue.replace([label, detail]).is_none(), "repeated hold cue");
            continue;
        }
        let kind = match kind {
            "lane" => Kind::Lane,
            "reference" => Kind::Reference,
            "adjustment" => Kind::Adjustment,
            "spacing" => Kind::Spacing,
            "player" => Kind::Player,
            other => bail!("unknown guide type {other}"),
        };
        moment.guides.push(Guide {
            kind,
            from: Point(x1.parse()?, y1.parse()?),
            to: Point(x2.parse()?, y2.parse()?),
            label_at: if label.is_empty() || matches!(kind, Kind::Player) {
                Point(0, 0)
            } else {
                Point(lx.parse()?, ly.parse()?)
            },
            label,
        });
    }
    Ok(moments)
}

pub fn draw(
    image: &mut RgbImage,
    font: &FontArc,
    view: SourceView,
    moment: Option<&Moment<'_>>,
    marker: Option<[i32; 4]>,
    puck: Option<Point>,
) -> Result<()> {
    let guides = moment.map_or(&[][..], |moment| &moment.guides);
    if guides.is_empty() && marker.is_none() && puck.is_none() {
        return Ok(());
    }

    let mut ink = Pixmap::new(image.width(), image.height()).context("invalid annotation canvas")?;
    let mut labels = Vec::new();
    let mut occupied = Vec::new();
    for guide in guides {
        let from = vector(view.project(guide.from)?);
        let to = vector(view.project(guide.to)?);
        let color = match guide.kind {
            Kind::Reference => BLUE,
            Kind::Adjustment => GREEN,
            _ => GOLD,
        };
        if matches!(guide.kind, Kind::Player) {
            occupied.push([from.x as i32, from.y as i32, to.x as i32, to.y as i32]);
            labels.push((
                guide.label,
                player(&mut ink, from, to)?,
                Point(((from.x + to.x) / 2.) as i32, from.y as i32),
                GOLD,
            ));
            continue;
        }
        let Some(axis) = (to - from).try_normalize() else {
            continue;
        };
        let path = polyline(&[from, to], false)?;
        match guide.kind {
            Kind::Reference => outlined(&mut ink, &path, BLUE, 2.5, Some([9.0, 7.0])),
            Kind::Lane => {
                stroke(&mut ink, &path, GOLD, 17.0, 78, None);
                outlined(&mut ink, &path, GOLD, 2.25, None);
            }
            Kind::Adjustment => {
                let length = from.distance(to);
                let head = (length * 0.36).clamp(3.0, 16.0);
                let base = to - axis * head;
                let cross = axis.perp();
                let arrow = polyline(
                    &[
                        from - cross * 3.0,
                        base - cross * 3.0,
                        base - cross * 8.5,
                        to,
                        base + cross * 8.5,
                        base + cross * 3.0,
                        from + cross * 3.0,
                    ],
                    true,
                )?;
                stroke(&mut ink, &arrow, INK, 2.5, 200, None);
                ink.fill_path(&arrow, &paint(GREEN, 255), FillRule::Winding, Transform::identity(), None);
            }
            Kind::Spacing => {
                outlined(&mut ink, &path, GOLD, 2.75, None);
                for (end, inward) in [(from, axis), (to, -axis)] {
                    let base = end + inward * 9.0;
                    let wing = inward.perp() * 5.0;
                    let head = polyline(&[base + wing, end, base - wing], false)?;
                    outlined(&mut ink, &head, GOLD, 2.75, None);
                }
            }
            Kind::Player => unreachable!(),
        }
        if !guide.label.is_empty() {
            labels.push((guide.label, view.project(guide.label_at)?, Point(to.x as i32, to.y as i32), color));
        }
    }
    if let Some([x1, y1, x2, y2]) = marker {
        let from = vector(view.project(Point(x1, y1))?);
        let to = vector(view.project(Point(x2, y2))?);
        occupied.push([from.x as i32, from.y as i32, to.x as i32, to.y as i32]);
        labels.push(("YOU", player(&mut ink, from, to)?, Point(((from.x + to.x) / 2.) as i32, from.y as i32), GOLD));
    }
    if let Some(center) = puck.and_then(|p| view.project(p).ok()) {
        let x = center.0 as f32;
        let y = center.1 as f32;
        // Open corners leave the source pixels around the puck visible.
        for dx in [-1., 1.] {
            for dy in [-1., 1.] {
                let corner = polyline(
                    &[
                        Vec2::new(x + dx * 3., y + dy * 8.),
                        Vec2::new(x + dx * 8., y + dy * 8.),
                        Vec2::new(x + dx * 8., y + dy * 3.),
                    ],
                    false,
                )?;
                outlined(&mut ink, &corner, Rgb([131, 235, 237]), 1.8, None);
            }
        }
        occupied.push([center.0 - 10, center.1 - 10, center.0 + 10, center.1 + 10]);
    }
    let mut captions = Vec::new();
    for (label, anchor, feature, color) in labels {
        if label.is_empty() {
            continue;
        }
        let size = 15.;
        let scaled = font.as_scaled(PxScale::from(size));
        let width = label.chars().map(|ch| scaled.h_advance(scaled.glyph_id(ch))).sum::<f32>().ceil() as i32 + 23;
        let mut best = None;
        // Try nearby rows on either side of the feature before covering a skater or another label.
        for dy in [0, -26, 26, -52, 52, -78, 78] {
            for dx in [0, -width] {
                let Point(x, y) = view.clamp_label(Point(anchor.0 + dx, anchor.1 + dy), width)?;
                let overlap: i64 = occupied
                    .iter()
                    .map(|&[l, t, r, b]| {
                        i64::from(((x + width).min(r + 3) - x.max(l - 3)).max(0))
                            * i64::from(((y + 5).min(b + 3) - (y - 16).max(t - 3)).max(0))
                    })
                    .sum();
                let cost = overlap * 1000 + i64::from(dy.abs() + dx.abs() / 4);
                if best.is_none_or(|(prior, _)| cost < prior) {
                    best = Some((cost, Point(x, y)));
                }
            }
        }
        let (_, Point(x, y)) = best.unwrap();
        occupied.push([x, y - 16, x + width, y + 5]);
        let attach = Point(feature.0.clamp(x + 5, x + width - 5), feature.1.clamp(y - 14, y + 3));
        if attach.distance(feature) > 12. {
            let line = polyline(&[vector(attach), vector(feature)], false)?;
            stroke(&mut ink, &line, INK, 2.5, 150, None);
            stroke(&mut ink, &line, color, 1., 220, None);
        }
        let chip = rounded_chip(x as f32, (y - 16) as f32, width as f32)?;
        ink.fill_path(&chip, &paint(INK, 225), FillRule::Winding, Transform::identity(), None);
        stroke(&mut ink, &chip, color, 1., 185, None);
        let dot = PathBuilder::from_circle(x as f32 + 9., y as f32 - 5., 2.3).context("invalid label dot")?;
        ink.fill_path(&dot, &paint(color, 255), FillRule::Winding, Transform::identity(), None);
        captions.push((label, Point(x + 16, y), size));
    }
    for (pixel, overlay) in image.pixels_mut().zip(ink.pixels()) {
        let alpha = u16::from(overlay.alpha());
        if alpha > 0 {
            for (channel, top) in pixel.0.iter_mut().zip([overlay.red(), overlay.green(), overlay.blue()]) {
                *channel = (u16::from(top) + (u16::from(*channel) * (255 - alpha) + 127) / 255) as u8;
            }
        }
    }
    for (label, at, size) in captions {
        text(image, font, label, at, size, crate::overlay::WHITE);
    }
    Ok(())
}

fn rounded_chip(x: f32, y: f32, w: f32) -> Result<Path> {
    let (r, h) = (8., 21.);
    let mut path = PathBuilder::new();
    path.move_to(x + r, y);
    path.line_to(x + w - r, y);
    path.quad_to(x + w, y, x + w, y + r);
    path.line_to(x + w, y + h - r);
    path.quad_to(x + w, y + h, x + w - r, y + h);
    path.line_to(x + r, y + h);
    path.quad_to(x, y + h, x, y + h - r);
    path.line_to(x, y + r);
    path.quad_to(x, y, x + r, y);
    path.close();
    path.finish().context("invalid annotation label")
}

fn vector(Point(x, y): Point) -> Vec2 {
    Vec2::new(x as f32, y as f32)
}

fn player(ink: &mut Pixmap, from: Vec2, to: Vec2) -> Result<Point> {
    let center = Vec2::new((from.x + to.x) / 2.0, to.y - 4.0);
    let radius = ((to.x - from.x) * 0.36).clamp(14.0, 33.0);
    let depth = (radius * 0.26).clamp(4.0, 8.0);
    let bounds = Rect::from_ltrb(center.x - radius, center.y - depth, center.x + radius, center.y + depth)
        .context("invalid player spotlight")?;
    let ring = PathBuilder::from_oval(bounds).context("invalid player spotlight")?;
    ink.fill_path(&ring, &paint(GOLD, 45), FillRule::Winding, Transform::identity(), None);
    outlined(ink, &ring, GOLD, 2.2, None);
    Ok(Point((center.x - radius).round() as i32, (from.y - 10.0).round() as i32))
}

fn polyline(points: &[Vec2], close: bool) -> Result<Path> {
    let mut path = PathBuilder::new();
    path.move_to(points[0].x, points[0].y);
    for point in &points[1..] {
        path.line_to(point.x, point.y);
    }
    if close {
        path.close();
    }
    path.finish().context("invalid annotation path")
}

fn paint(color: Rgb<u8>, alpha: u8) -> Paint<'static> {
    let [r, g, b] = color.0;
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, alpha);
    paint.anti_alias = true;
    paint
}

fn stroke(ink: &mut Pixmap, path: &Path, color: Rgb<u8>, width: f32, alpha: u8, dash: Option<[f32; 2]>) {
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        dash: dash.and_then(|segments| StrokeDash::new(segments.to_vec(), 0.0)),
        ..Stroke::default()
    };
    ink.stroke_path(path, &paint(color, alpha), &stroke, Transform::identity(), None);
}

fn outlined(ink: &mut Pixmap, path: &Path, color: Rgb<u8>, width: f32, dash: Option<[f32; 2]>) {
    stroke(ink, path, INK, width + 2.4, 185, dash);
    stroke(ink, path, color, width, 255, dash);
}
