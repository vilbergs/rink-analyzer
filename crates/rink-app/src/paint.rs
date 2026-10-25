//! Drawing the rink and the entities on it. Everything is built in rink meters
//! and mapped through a [`RinkTransform`], so flipping ends or resizing the
//! window needs nothing special here.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use gpui::{point, px, rgb, rgba, App, Hsla, PathBuilder, Pixels, Point, Rgba, SharedString, TextRun, Window};
use rink_core::review::play_fingerprint;
use rink_core::{EntityId, EntityKind, Play, RinkSpec, Snapshot, Suggestion, Vec2};

use crate::transform::RinkTransform;

const ICE: u32 = 0xf3f7fa;
/// Goal frames.
const BOARDS: u32 = 0x30343b;
/// The boards' top edge, light against the dark window.
const BOARD_EDGE: u32 = 0xc4ccd6;
/// Soft shadows under the rink and the players.
const RINK_SHADOW: u32 = 0x00000066;
const PLAYER_SHADOW: u32 = 0x0000003a;
const RED: u32 = 0xc8102e;
const BLUE: u32 = 0x0033a0;
const CREASE: u32 = 0x9fd3f5;
const ME_RING: u32 = crate::theme::ME;
const SELECTED_RING: u32 = crate::theme::ACCENT;
const PUCK: u32 = 0x000000;
/// Position feedback: zone fill (RGBA), ghost outline and arrow, label text.
const ZONE_FILL: u32 = 0x22b35a40;
const GHOST: u32 = 0x1a9a4c;
const GHOST_FILL: u32 = 0xd8f5e3c0;
const GHOST_LABEL: u32 = 0x146b37;

struct GhostColors {
    zone: u32,
    line: u32,
    fill: u32,
    label: u32,
}

const FRESH: GhostColors = GhostColors { zone: ZONE_FILL, line: GHOST, fill: GHOST_FILL, label: GHOST_LABEL };
/// For suggestions written before the play's motion was edited.
const STALE: GhostColors = GhostColors { zone: 0x8a93a033, line: 0x8a93a0, fill: 0xe6e9edc0, label: 0x5b6370 };

fn colors_for(s: &Suggestion, fingerprint: &str) -> &'static GhostColors {
    if !s.play_hash.is_empty() && s.play_hash != fingerprint {
        &STALE
    } else {
        &FRESH
    }
}

pub const SKATER_RADIUS_M: f32 = 0.9;
pub const GOALIE_RADIUS_M: f32 = 1.1;
pub const PUCK_RADIUS_M: f32 = 0.35;
const CIRCLE_SEGMENTS: usize = 48;
const CORNER_SEGMENTS: usize = 16;

pub fn entity_radius(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Skater => SKATER_RADIUS_M,
        EntityKind::Goalie => GOALIE_RADIUS_M,
        EntityKind::Puck => PUCK_RADIUS_M,
    }
}

fn color(hex: u32) -> Rgba {
    rgb(hex)
}

fn fill(window: &mut Window, t: &RinkTransform, pts: &[Vec2], hex: u32) {
    let screen: Vec<_> = pts.iter().map(|p| t.to_screen(*p)).collect();
    let mut b = PathBuilder::fill();
    b.add_polygon(&screen, true);
    if let Ok(path) = b.build() {
        window.paint_path(path, color(hex));
    }
}

fn stroke(window: &mut Window, t: &RinkTransform, pts: &[Vec2], closed: bool, width_px: f32, hex: u32) {
    let screen: Vec<_> = pts.iter().map(|p| t.to_screen(*p)).collect();
    let mut b = PathBuilder::stroke(px(width_px));
    b.add_polygon(&screen, closed);
    if let Ok(path) = b.build() {
        window.paint_path(path, color(hex));
    }
}

fn fill_translucent(window: &mut Window, t: &RinkTransform, pts: &[Vec2], hex_rgba: u32) {
    let screen: Vec<_> = pts.iter().map(|p| t.to_screen(*p)).collect();
    let mut b = PathBuilder::fill();
    b.add_polygon(&screen, true);
    if let Ok(path) = b.build() {
        window.paint_path(path, rgba(hex_rgba));
    }
}

fn stroke_dashed(window: &mut Window, t: &RinkTransform, pts: &[Vec2], width_px: f32, hex: u32) {
    let screen: Vec<_> = pts.iter().map(|p| t.to_screen(*p)).collect();
    let mut b = PathBuilder::stroke(px(width_px)).dash_array(&[px(5.), px(4.)]);
    b.add_polygon(&screen, true);
    if let Ok(path) = b.build() {
        window.paint_path(path, color(hex));
    }
}

/// A line from `from` to `to` with an arrowhead, both in rink meters.
fn arrow(window: &mut Window, t: &RinkTransform, from: Vec2, to: Vec2, width_px: f32, hex: u32) {
    let d = to - from;
    if d.length() < 0.1 {
        return;
    }
    let dir = d.normalize();
    let head = 0.9;
    let left = to - dir.rotate(Vec2::from_angle(0.45)) * head;
    let right = to - dir.rotate(Vec2::from_angle(-0.45)) * head;
    stroke(window, t, &[from, to], false, width_px, hex);
    fill(window, t, &[to, left, right], hex);
}

fn circle(c: Vec2, r: f32) -> Vec<Vec2> {
    arc(c, r, 0.0, TAU, CIRCLE_SEGMENTS)
}

fn arc(c: Vec2, r: f32, from: f32, to: f32, segments: usize) -> Vec<Vec2> {
    (0..=segments)
        .map(|i| {
            let a = from + (to - from) * i as f32 / segments as f32;
            c + Vec2::from_angle(a) * r
        })
        .collect()
}

/// Axis-aligned rectangle centered on x, spanning y in [-half_h, half_h].
fn vertical_band(x: f32, width: f32, half_h: f32) -> [Vec2; 4] {
    let w = width / 2.0;
    [Vec2::new(x - w, -half_h), Vec2::new(x + w, -half_h), Vec2::new(x + w, half_h), Vec2::new(x - w, half_h)]
}

/// The boards outline: a rectangle with rounded corners.
pub fn boards_outline(rink: &RinkSpec) -> Vec<Vec2> {
    let r = rink.corner_radius_m;
    let cx = rink.half_length() - r;
    let cy = rink.half_width() - r;
    let mut pts = Vec::new();
    for (c, start) in [
        (Vec2::new(cx, cy), 0.0),
        (Vec2::new(-cx, cy), FRAC_PI_2),
        (Vec2::new(-cx, -cy), PI),
        (Vec2::new(cx, -cy), PI + FRAC_PI_2),
    ] {
        pts.extend(arc(c, r, start, start + FRAC_PI_2, CORNER_SEGMENTS));
    }
    pts
}

/// Half the ice width at x, accounting for the rounded corners.
pub fn half_width_at(rink: &RinkSpec, x: f32) -> f32 {
    let r = rink.corner_radius_m;
    let cx = rink.half_length() - r;
    let dx = x.abs() - cx;
    if dx <= 0.0 {
        rink.half_width()
    } else {
        rink.half_width() - r + (r * r - dx * dx).max(0.0).sqrt()
    }
}

pub fn paint_rink(window: &mut Window, t: &RinkTransform, rink: &RinkSpec) {
    // A soft shadow: the outline grown by the board thickness, twice.
    for grow in [1.4f32, 0.7] {
        let mut shadow = rink.clone();
        shadow.length_m += 2.0 * grow;
        shadow.width_m += 2.0 * grow;
        shadow.corner_radius_m += grow;
        fill_translucent(window, t, &boards_outline(&shadow), RINK_SHADOW);
    }
    let outline = boards_outline(rink);
    fill(window, t, &outline, ICE);

    let hw = rink.half_width();
    let min_w = 1.5 / t.scale();

    // Goal creases under the lines.
    let gx = rink.goal_line_x();
    for sign in [-1.0f32, 1.0] {
        let c = Vec2::new(sign * gx, 0.0);
        let (from, to) = if sign > 0.0 { (FRAC_PI_2, PI + FRAC_PI_2) } else { (-FRAC_PI_2, FRAC_PI_2) };
        let mut crease = arc(c, rink.crease_radius_m, from, to, 24);
        crease.push(c);
        fill(window, t, &crease, CREASE);
        stroke(window, t, &crease, true, 1.5, RED);
    }

    // Center red line, blue lines, goal lines.
    fill(window, t, &vertical_band(0.0, rink.red_line_width_m.max(min_w), hw), RED);
    let bx = rink.blue_line_x();
    for x in [-bx, bx] {
        fill(window, t, &vertical_band(x, rink.blue_line_width_m.max(min_w), hw), BLUE);
    }
    let goal_half = half_width_at(rink, gx);
    for x in [-gx, gx] {
        fill(window, t, &vertical_band(x, rink.line_width_m.max(min_w), goal_half), RED);
    }

    // Circles and dots.
    stroke(window, t, &circle(Vec2::ZERO, rink.center_circle_radius_m), true, 2.0, BLUE);
    fill(window, t, &circle(Vec2::ZERO, 0.15), BLUE);
    for dot in rink.end_dots() {
        stroke(window, t, &circle(dot, rink.faceoff_circle_radius_m), true, 2.0, RED);
        fill(window, t, &circle(dot, 0.3), RED);
    }
    for dot in rink.neutral_dots() {
        fill(window, t, &circle(dot, 0.3), RED);
    }

    // Goal frames behind the goal lines.
    let gw = rink.goal_width_m / 2.0;
    for sign in [-1.0f32, 1.0] {
        let x0 = sign * gx;
        let x1 = sign * (gx + rink.goal_depth_m);
        let frame = [Vec2::new(x0, -gw), Vec2::new(x1, -gw), Vec2::new(x1, gw), Vec2::new(x0, gw)];
        stroke(window, t, &frame, false, 2.0, BOARDS);
    }

    stroke(window, t, &outline, true, 3.0, BOARD_EDGE);
}

pub struct EntityPaint<'a> {
    pub play: &'a Play,
    pub snapshot: &'a Snapshot,
    pub selected: Option<&'a EntityId>,
}

pub fn paint_entities(window: &mut Window, cx: &mut App, t: &RinkTransform, p: EntityPaint<'_>) {
    for state in &p.snapshot.entities {
        let Some(entity) = p.play.entity(&state.id) else { continue };
        let r = entity_radius(entity.kind);
        let team_color = entity.team.map(|s| p.play.team(s).color_rgb()).unwrap_or(0x808080);

        fill_translucent(window, t, &circle(state.pos + Vec2::new(0.12, -0.18), r * 1.05), PLAYER_SHADOW);
        if p.selected == Some(&state.id) {
            fill_translucent(window, t, &circle(state.pos, r + 0.9), (SELECTED_RING << 8) | 0x2e);
            stroke(window, t, &circle(state.pos, r + 0.9), true, 2.0, SELECTED_RING);
        }
        if entity.is_me {
            stroke(window, t, &circle(state.pos, r + 0.4), true, 3.0, ME_RING);
        }

        let tip = state.pos + Vec2::from_angle(state.facing) * (r + 0.75);
        stroke(window, t, &[state.pos, tip], false, 3.5, team_color);
        fill(window, t, &circle(state.pos, r), team_color);
        stroke(window, t, &circle(state.pos, r), true, 2.0, 0xffffff);

        let label = entity
            .number
            .map(|n| n.to_string())
            .or_else(|| entity.role.map(|r| r.as_str().to_string()))
            .unwrap_or_default();
        paint_centered_text(window, cx, t, state.pos, &label, r * 1.1);
    }

    let puck_r = PUCK_RADIUS_M.max(3.5 / t.scale());
    fill(window, t, &circle(p.snapshot.puck.pos, puck_r + 1.5 / t.scale()), 0xffffff);
    fill(window, t, &circle(p.snapshot.puck.pos, puck_r), PUCK);
    if p.selected.is_some_and(|id| p.play.entity(id).is_some_and(|e| e.is_puck())) {
        stroke(window, t, &circle(p.snapshot.puck.pos, puck_r + 0.4), true, 2.0, SELECTED_RING);
    }
}

/// Under the players: the acceptable zone around each active suggestion's
/// target, labelled with its id and how far off the real position was.
pub fn paint_suggestion_zones(
    window: &mut Window,
    cx: &mut App,
    t: &RinkTransform,
    play: &Play,
    active: &[(Suggestion, Vec2)],
) {
    let fingerprint = play_fingerprint(play);
    for (s, _) in active {
        let c = colors_for(s, &fingerprint);
        fill_translucent(window, t, &circle(s.target, s.zone_radius_m), c.zone);
        let label = match s.miss_distance(play) {
            Some(m) => format!("{} · {:.1} m", s.id, m),
            None => s.id.clone(),
        };
        let below = s.target - Vec2::new(0.0, s.zone_radius_m + 0.8);
        paint_text(window, cx, t.to_screen(below), &label, px(11.0), c.label);
    }
}

/// Over the players: the ghost skating the suggested route, see-through so the
/// real player stays visible, with an arrow from the real position to it.
pub fn paint_ghosts(
    window: &mut Window,
    cx: &mut App,
    t: &RinkTransform,
    play: &Play,
    snapshot: &Snapshot,
    active: &[(Suggestion, Vec2)],
) {
    let fingerprint = play_fingerprint(play);
    for (s, ghost) in active {
        let c = colors_for(s, &fingerprint);
        let Some(entity) = play.entity(&s.entity) else { continue };
        let r = entity_radius(entity.kind);
        if let Some(actual) = snapshot.entity(&s.entity).map(|e| e.pos) {
            let gap = ghost.distance(actual);
            if gap > r * 1.2 {
                let dir = (*ghost - actual).normalize();
                arrow(window, t, actual + dir * r, *ghost - dir * (r * 0.6), 2.5, c.line);
            }
        }
        if let Some(facing) = s.facing {
            let tip = *ghost + Vec2::from_angle(facing) * (r + 0.7);
            stroke(window, t, &[*ghost, tip], false, 3.0, c.line);
        }
        fill_translucent(window, t, &circle(*ghost, r), c.fill);
        stroke_dashed(window, t, &circle(*ghost, r), 2.0, c.line);
        if let Some(n) = entity.number {
            let font = px((r * 1.1 * t.scale()).clamp(8.0, 28.0));
            paint_text(window, cx, t.to_screen(*ghost), &n.to_string(), font, c.label);
        }
    }
}

fn paint_centered_text(window: &mut Window, cx: &mut App, t: &RinkTransform, at: Vec2, text: &str, height_m: f32) {
    let font_size = px((height_m * t.scale()).clamp(8.0, 28.0));
    paint_text(window, cx, t.to_screen(at), text, font_size, 0xffffff);
}

/// Paint one line of text centered on `center` (window pixels).
pub fn paint_text(window: &mut Window, cx: &mut App, center: Point<Pixels>, text: &str, font_size: Pixels, hex: u32) {
    if text.is_empty() {
        return;
    }
    let text_color: Hsla = rgb(hex).into();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_line_is_shorter_than_center_line() {
        let rink = RinkSpec::iihf();
        let goal = half_width_at(&rink, rink.goal_line_x());
        assert!(goal < rink.half_width());
        assert!((half_width_at(&rink, 0.0) - rink.half_width()).abs() < 1e-5);
        // Outline points all sit on the boards.
        for p in boards_outline(&rink) {
            assert!(rink.distance_inside_boards(p).abs() < 1e-3, "{p}");
        }
    }
}
