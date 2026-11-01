//! The rink view as a display list. [`build`] draws the rink, orders,
//! suggestion zones, players and ghosts for one frame in rink meters, maps
//! every point through a [`RinkTransform`], and returns plain shapes in
//! screen pixels, so the GPUI canvas and a browser canvas paint the same
//! picture with a dumb loop.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::model::{EntityId, EntityKind, Play};
use crate::review::{play_fingerprint, Suggestion};
use crate::rink::RinkSpec;
use crate::sample::{position_at, Snapshot};
use crate::skate::order_end_ms;
use crate::transform::RinkTransform;

/// 0xRRGGBBAA.
pub type Color = u32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    /// Closed polygon, screen pixels.
    Fill { pts: Vec<Vec2>, color: Color },
    /// Polyline, screen pixels. `dash` is [on, off] in px.
    Stroke { pts: Vec<Vec2>, closed: bool, width_px: f32, dash: Option<[f32; 2]>, color: Color },
    /// One line of text centered on `at` (screen pixels).
    Text { at: Vec2, text: String, size_px: f32, color: Color },
}

/// Back-to-front display list for one frame. Painters iterate it in order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub shapes: Vec<Shape>,
}

pub struct Frame<'a> {
    pub play: &'a Play,
    pub snapshot: &'a Snapshot,
    pub playhead_ms: u32,
    pub selected: Option<&'a EntityId>,
    /// Active suggestions with the ghost's position at the playhead.
    pub ghosts: &'a [(Suggestion, Vec2)],
    /// Entities not drawn, with their orders. Their snapshot state still
    /// exists, so hit testing and facts work.
    pub hidden: &'a [EntityId],
    /// Annotations drawn above everything, in rink meters.
    pub marks: &'a [Mark],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mark {
    /// Translucent disc, such as an acceptable zone.
    Zone {
        at: Vec2,
        r_m: f32,
        color: Color,
    },
    /// A labelled point: a player-sized disc with the text centered on it.
    Dot {
        at: Vec2,
        label: String,
        color: Color,
    },
    Arrow {
        from: Vec2,
        to: Vec2,
        color: Color,
    },
}

// Colors below are 0xRRGGBB unless named as RGBA.
const ICE: u32 = 0xf3f7fa;
/// Goal frames.
const BOARDS: u32 = 0x30343b;
/// The boards' top edge, light against the dark window.
const BOARD_EDGE: u32 = 0xc4ccd6;
/// Soft shadows under the rink and the players, RGBA.
const RINK_SHADOW: u32 = 0x00000080;
const PLAYER_SHADOW: u32 = 0x0000003a;
const RED: u32 = 0xc8102e;
const BLUE: u32 = 0x0033a0;
const CREASE: u32 = 0x9fd3f5;
/// The user's own player. The app's theme uses it too.
pub const ME_RING: u32 = 0xf2a93b;
/// Selection. The app's theme uses it as its accent.
pub const SELECTED_RING: u32 = 0x4c8dff;
const PUCK: u32 = 0x000000;
const WHITE: u32 = 0xffffff;
/// Players without a team.
const NO_TEAM: u32 = 0x808080;
/// Position feedback: zone fill (RGBA), ghost outline and arrow, ghost fill (RGBA), label text.
pub const ZONE_FILL: u32 = 0x22b35a40;
pub const GHOST: u32 = 0x1a9a4c;
const GHOST_FILL: u32 = 0xd8f5e3c0;
const GHOST_LABEL: u32 = 0x146b37;
const DASH: [f32; 2] = [5.0, 4.0];
const DOT: [f32; 2] = [2.0, 5.0];

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
/// Extra grab distance around an entity, in meters.
const HIT_SLOP_M: f32 = 0.3;
const CIRCLE_SEGMENTS: usize = 48;
const CORNER_SEGMENTS: usize = 16;
/// How finely an order's path is drawn.
const PATH_STEP_MS: u32 = 50;

pub fn entity_radius(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Skater => SKATER_RADIUS_M,
        EntityKind::Goalie => GOALIE_RADIUS_M,
        EntityKind::Puck => PUCK_RADIUS_M,
    }
}

/// Where the puck is drawn and grabbed. A carried puck sits on the rim of the
/// carrier's disc, on the side it's sampled at (their stick side), so it
/// doesn't cover their mark.
pub fn puck_draw_pos(play: &Play, snapshot: &Snapshot) -> Vec2 {
    let puck = snapshot.puck.pos;
    let Some(carrier) = snapshot.puck.carrier.as_ref().and_then(|id| snapshot.entity(id)) else { return puck };
    let r = play.entity(&carrier.id).map(|e| entity_radius(e.kind)).unwrap_or(SKATER_RADIUS_M);
    let offset = puck - carrier.pos;
    let dir = if offset.length() > 0.05 { offset.normalize() } else { Vec2::from_angle(carrier.facing) };
    carrier.pos + dir * (r + PUCK_RADIUS_M * 0.5)
}

/// The entity under `at` (rink meters): the puck first, else the nearest
/// player whose disc, plus a little slop, contains it.
pub fn hit_test(play: &Play, snapshot: &Snapshot, at: Vec2) -> Option<EntityId> {
    if let Some(puck) = play.puck() {
        if puck_draw_pos(play, snapshot).distance(at) <= entity_radius(puck.kind) + HIT_SLOP_M {
            return Some(puck.id.clone());
        }
    }
    snapshot
        .entities
        .iter()
        .filter_map(|s| {
            let e = play.entity(&s.id)?;
            let d = s.pos.distance(at);
            (d <= entity_radius(e.kind) + HIT_SLOP_M).then_some((d, s.id.clone()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
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

pub fn opaque(rgb: u32) -> Color {
    (rgb << 8) | 0xff
}

/// Collects shapes in rink meters and stores them in screen pixels.
struct Builder<'t> {
    t: &'t RinkTransform,
    shapes: Vec<Shape>,
}

impl Builder<'_> {
    fn screen(&self, pts: &[Vec2]) -> Vec<Vec2> {
        pts.iter().map(|p| self.t.to_screen(*p)).collect()
    }

    fn fill(&mut self, pts: &[Vec2], rgb: u32) {
        self.fill_translucent(pts, opaque(rgb));
    }

    fn fill_translucent(&mut self, pts: &[Vec2], color: Color) {
        self.shapes.push(Shape::Fill { pts: self.screen(pts), color });
    }

    fn stroke(&mut self, pts: &[Vec2], closed: bool, width_px: f32, rgb: u32) {
        self.line(pts, closed, width_px, None, opaque(rgb));
    }

    fn line(&mut self, pts: &[Vec2], closed: bool, width_px: f32, dash: Option<[f32; 2]>, color: Color) {
        self.shapes.push(Shape::Stroke { pts: self.screen(pts), closed, width_px, dash, color });
    }

    /// A line from `from` to `to` with an arrowhead, both in rink meters.
    fn arrow(&mut self, from: Vec2, to: Vec2, width_px: f32, color: Color) {
        let d = to - from;
        if d.length() < 0.1 {
            return;
        }
        let dir = d.normalize();
        let head = 0.9;
        let left = to - dir.rotate(Vec2::from_angle(0.45)) * head;
        let right = to - dir.rotate(Vec2::from_angle(-0.45)) * head;
        self.line(&[from, to], false, width_px, None, color);
        self.fill_translucent(&[to, left, right], color);
    }

    fn text(&mut self, at: Vec2, text: String, size_px: f32, rgb: u32) {
        self.shapes.push(Shape::Text { at: self.t.to_screen(at), text, size_px, color: opaque(rgb) });
    }

    /// Text `height_m` tall at the current zoom, within readable bounds.
    fn label_size(&self, height_m: f32) -> f32 {
        (height_m * self.t.scale()).clamp(8.0, 28.0)
    }
}

pub fn build(frame: &Frame<'_>, t: &RinkTransform) -> Scene {
    let mut b = Builder { t, shapes: Vec::new() };
    rink(&mut b, &frame.play.rink);
    suggestion_zones(&mut b, frame.play, frame.ghosts);
    orders(&mut b, frame);
    entities(&mut b, frame);
    ghosts(&mut b, frame.play, frame.snapshot, frame.ghosts);
    marks(&mut b, frame.marks);
    Scene { shapes: b.shapes }
}

fn rink(b: &mut Builder<'_>, rink: &RinkSpec) {
    let scale = b.t.scale();
    // A thin shadow just outside the boards.
    let mut shadow = rink.clone();
    let grow = 4.0 / scale;
    shadow.length_m += 2.0 * grow;
    shadow.width_m += 2.0 * grow;
    shadow.corner_radius_m += grow;
    b.fill_translucent(&boards_outline(&shadow), RINK_SHADOW);
    let outline = boards_outline(rink);
    b.fill(&outline, ICE);

    let hw = rink.half_width();
    let min_w = 1.5 / scale;

    // Goal creases under the lines.
    let gx = rink.goal_line_x();
    for sign in [-1.0f32, 1.0] {
        let c = Vec2::new(sign * gx, 0.0);
        let (from, to) = if sign > 0.0 { (FRAC_PI_2, PI + FRAC_PI_2) } else { (-FRAC_PI_2, FRAC_PI_2) };
        let mut crease = arc(c, rink.crease_radius_m, from, to, 24);
        crease.push(c);
        b.fill(&crease, CREASE);
        b.stroke(&crease, true, 1.5, RED);
    }

    // Center red line, blue lines, goal lines.
    b.fill(&vertical_band(0.0, rink.red_line_width_m.max(min_w), hw), RED);
    let bx = rink.blue_line_x();
    for x in [-bx, bx] {
        b.fill(&vertical_band(x, rink.blue_line_width_m.max(min_w), hw), BLUE);
    }
    let goal_half = half_width_at(rink, gx);
    for x in [-gx, gx] {
        b.fill(&vertical_band(x, rink.line_width_m.max(min_w), goal_half), RED);
    }

    // Circles and dots.
    b.stroke(&circle(Vec2::ZERO, rink.center_circle_radius_m), true, 2.0, BLUE);
    b.fill(&circle(Vec2::ZERO, 0.15), BLUE);
    for dot in rink.end_dots() {
        b.stroke(&circle(dot, rink.faceoff_circle_radius_m), true, 2.0, RED);
        b.fill(&circle(dot, 0.3), RED);
    }
    for dot in rink.neutral_dots() {
        b.fill(&circle(dot, 0.3), RED);
    }

    // Goal frames behind the goal lines.
    let gw = rink.goal_width_m / 2.0;
    for sign in [-1.0f32, 1.0] {
        let x0 = sign * gx;
        let x1 = sign * (gx + rink.goal_depth_m);
        let frame = [Vec2::new(x0, -gw), Vec2::new(x1, -gw), Vec2::new(x1, gw), Vec2::new(x0, gw)];
        b.stroke(&frame, false, 2.0, BOARDS);
    }

    b.stroke(&outline, true, 3.0, BOARD_EDGE);
}

/// Under the players: the acceptable zone around each active suggestion's
/// target, labelled with its id and how far off the real position was.
fn suggestion_zones(b: &mut Builder<'_>, play: &Play, active: &[(Suggestion, Vec2)]) {
    let fingerprint = play_fingerprint(play);
    for (s, _) in active {
        let c = colors_for(s, &fingerprint);
        b.fill_translucent(&circle(s.target, s.zone_radius_m), c.zone);
        let label = match s.miss_distance(play) {
            Some(m) => format!("{} · {:.1} m", s.id, m),
            None => s.id.clone(),
        };
        let below = s.target - Vec2::new(0.0, s.zone_radius_m + 0.8);
        b.text(below, label, 11.0, c.label);
    }
}

/// Under the players: where everyone following an order is headed from the
/// playhead on, dotted along the path they'll skate, with a dashed ring
/// where they stop and dots at the places they pass through. The selected
/// player's is drawn strongest.
fn orders(b: &mut Builder<'_>, frame: &Frame<'_>) {
    let Frame { play, playhead_ms, selected, hidden, .. } = *frame;
    for (id, orders) in play.orders.iter().filter(|(id, _)| !hidden.contains(id)) {
        let Some(order) = orders.iter().rev().find(|o| o.t_ms <= playhead_ms) else { continue };
        let end = order_end_ms(play, id, order);
        let Some(entity) = play.entity(id).filter(|_| end > playhead_ms) else { continue };
        let track = play.track(id);
        let hex = entity.team.map(|s| play.team(s).color_rgb()).unwrap_or(NO_TEAM);
        let (alpha, width) = if selected == Some(id) { (0xff, 2.5) } else { (0x80, 1.5) };
        let line = (hex << 8) | alpha;

        let pts: Vec<Vec2> = (playhead_ms..end)
            .step_by(PATH_STEP_MS as usize)
            .chain([end])
            .filter_map(|ms| position_at(track, ms))
            .collect();
        b.line(&pts, false, width, Some(DOT), line);
        for &w in &order.to[..order.to.len().saturating_sub(1)] {
            b.fill_translucent(&circle(w, 0.3), line);
        }
        if let Some(stop) = position_at(track, end) {
            let r = entity_radius(entity.kind);
            b.fill_translucent(&circle(stop, r), (hex << 8) | 0x24);
            b.line(&circle(stop, r), true, width, Some(DASH), line);
        }
    }
}

fn entities(b: &mut Builder<'_>, frame: &Frame<'_>) {
    let Frame { play, snapshot, selected, hidden, .. } = *frame;
    for state in snapshot.entities.iter().filter(|s| !hidden.contains(&s.id)) {
        let Some(entity) = play.entity(&state.id) else { continue };
        let r = entity_radius(entity.kind);
        let team_color = entity.team.map(|s| play.team(s).color_rgb()).unwrap_or(NO_TEAM);

        b.fill_translucent(&circle(state.pos + Vec2::new(0.12, -0.18), r * 1.05), PLAYER_SHADOW);
        if selected == Some(&state.id) {
            b.fill_translucent(&circle(state.pos, r + 0.9), (SELECTED_RING << 8) | 0x2e);
            b.stroke(&circle(state.pos, r + 0.9), true, 2.0, SELECTED_RING);
        }
        if entity.is_me {
            b.stroke(&circle(state.pos, r + 0.4), true, 3.0, ME_RING);
        }

        let tip = state.pos + Vec2::from_angle(state.facing) * (r + 0.75);
        b.stroke(&[state.pos, tip], false, 3.5, team_color);
        b.fill(&circle(state.pos, r), team_color);
        b.stroke(&circle(state.pos, r), true, 2.0, WHITE);
        let size = b.label_size(r * 1.1);
        b.text(state.pos, entity.mark(), size, WHITE);
    }

    if play.puck().is_some_and(|p| hidden.contains(&p.id)) {
        return;
    }
    let scale = b.t.scale();
    let puck_r = PUCK_RADIUS_M.max(3.5 / scale);
    let puck = puck_draw_pos(play, snapshot);
    b.fill(&circle(puck, puck_r + 1.5 / scale), WHITE);
    b.fill(&circle(puck, puck_r), PUCK);
    if selected.is_some_and(|id| play.entity(id).is_some_and(|e| e.is_puck())) {
        b.stroke(&circle(puck, puck_r + 0.4), true, 2.0, SELECTED_RING);
    }
}

/// Over the players: the ghost skating the suggested route, see-through so the
/// real player stays visible, with an arrow from the real position to it.
fn ghosts(b: &mut Builder<'_>, play: &Play, snapshot: &Snapshot, active: &[(Suggestion, Vec2)]) {
    let fingerprint = play_fingerprint(play);
    for (s, ghost) in active {
        let c = colors_for(s, &fingerprint);
        let Some(entity) = play.entity(&s.entity) else { continue };
        let r = entity_radius(entity.kind);
        if let Some(actual) = snapshot.entity(&s.entity).map(|e| e.pos) {
            let gap = ghost.distance(actual);
            if gap > r * 1.2 {
                let dir = (*ghost - actual).normalize();
                b.arrow(actual + dir * r, *ghost - dir * (r * 0.6), 2.5, opaque(c.line));
            }
        }
        if let Some(facing) = s.facing {
            let tip = *ghost + Vec2::from_angle(facing) * (r + 0.7);
            b.stroke(&[*ghost, tip], false, 3.0, c.line);
        }
        b.fill_translucent(&circle(*ghost, r), c.fill);
        b.line(&circle(*ghost, r), true, 2.0, Some(DASH), opaque(c.line));
        let size = b.label_size(r * 1.1);
        b.text(*ghost, entity.mark(), size, c.label);
    }
}

fn marks(b: &mut Builder<'_>, marks: &[Mark]) {
    for mark in marks {
        match mark {
            Mark::Zone { at, r_m, color } => b.fill_translucent(&circle(*at, *r_m), *color),
            Mark::Dot { at, label, color } => {
                let r = SKATER_RADIUS_M;
                b.fill_translucent(&circle(*at, r), *color);
                b.stroke(&circle(*at, r), true, 2.0, WHITE);
                let size = b.label_size(r * 0.9);
                b.text(*at, label.clone(), size, WHITE);
            }
            Mark::Arrow { from, to, color } => b.arrow(*from, *to, 2.5, *color),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::active_suggestions;
    use crate::sample::sample;
    use crate::templates::{build as template, Template};
    use crate::transform::ViewZoom;

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

    #[test]
    fn hit_test_finds_the_puck_and_the_nearest_player() {
        let play = template(Template::Breakout, RinkSpec::iihf());
        let snap = sample(&play, 0);
        let me = play.me().unwrap();
        let pos = play.track(&me.id)[0].pos;
        assert_eq!(hit_test(&play, &snap, pos + Vec2::new(0.3, 0.0)), Some(me.id.clone()));
        assert_eq!(hit_test(&play, &snap, Vec2::new(0.0, 14.0)), None);
        assert_eq!(hit_test(&play, &snap, puck_draw_pos(&play, &snap)), Some("puck".into()));
    }

    #[test]
    fn scene_draws_rink_then_players_with_their_marks_in_screen_pixels() {
        let play = template(Template::Breakout, RinkSpec::iihf());
        let snap = sample(&play, 0);
        let me = play.me().unwrap();
        let t = RinkTransform::fit(Vec2::ZERO, Vec2::new(1248.0, 648.0), &play.rink, false, ViewZoom::default());
        let frame = Frame {
            play: &play,
            snapshot: &snap,
            playhead_ms: 0,
            selected: Some(&me.id),
            ghosts: &[],
            hidden: &[],
            marks: &[],
        };
        let scene = build(&frame, &t);

        let Shape::Fill { color, .. } = &scene.shapes[1] else { panic!("ice comes second") };
        assert_eq!(*color, opaque(ICE));
        let text = |s: &Shape| match s {
            Shape::Text { at, text, .. } => Some((*at, text.clone())),
            _ => None,
        };
        let labels: Vec<_> = scene.shapes.iter().filter_map(text).collect();
        assert_eq!(labels.len(), snap.entities.len());
        let (at, mark) = labels.iter().find(|(_, m)| *m == me.mark()).unwrap();
        assert!(at.distance(t.to_screen(play.track(&me.id)[0].pos)) < 1e-3, "{mark} sits on its player");
        assert!(scene
            .shapes
            .iter()
            .any(|s| matches!(s, Shape::Stroke { color, .. } if *color == opaque(SELECTED_RING))));
    }

    #[test]
    fn ghosts_are_drawn_over_players() {
        let play = template(Template::Breakout, RinkSpec::iihf());
        let me = play.me().unwrap().id.clone();
        let s = Suggestion {
            id: "s1".into(),
            entity: me,
            t_ms: 1200,
            branch_from_ms: 0,
            target: Vec2::new(-20.0, 12.8),
            facing: None,
            zone_radius_m: 1.5,
            principle: "test".into(),
            reason: "test".into(),
            detail: String::new(),
            confidence: None,
            play_hash: String::new(),
        };
        let suggestions = [s];
        let ghosts: Vec<_> = active_suggestions(&suggestions, &play, 600).map(|(s, g)| (s.clone(), g)).collect();
        assert_eq!(ghosts.len(), 1);
        let snap = sample(&play, 600);
        let t = RinkTransform::fit(Vec2::ZERO, Vec2::new(1248.0, 648.0), &play.rink, false, ViewZoom::default());
        let frame = Frame {
            play: &play,
            snapshot: &snap,
            playhead_ms: 600,
            selected: None,
            ghosts: &ghosts,
            hidden: &[],
            marks: &[],
        };
        let scene = build(&frame, &t);
        let last = scene.shapes.last().unwrap();
        assert!(matches!(last, Shape::Text { color, .. } if *color == opaque(GHOST_LABEL)), "{last:?}");
        let zone_label =
            scene.shapes.iter().position(|s| matches!(s, Shape::Text { text, .. } if text.starts_with("s1")));
        let first_player =
            scene.shapes.iter().position(|s| matches!(s, Shape::Fill { color, .. } if *color == PLAYER_SHADOW));
        assert!(zone_label < first_player, "zones sit under the players");
    }

    #[test]
    fn hidden_entities_are_not_drawn() {
        let play = template(Template::Breakout, RinkSpec::iihf());
        let snap = sample(&play, 0);
        let me = play.me().unwrap();
        let t = RinkTransform::fit(Vec2::ZERO, Vec2::new(1248.0, 648.0), &play.rink, false, ViewZoom::default());
        let hidden = [me.id.clone()];
        let frame = Frame {
            play: &play,
            snapshot: &snap,
            playhead_ms: 0,
            selected: None,
            ghosts: &[],
            hidden: &hidden,
            marks: &[],
        };
        let scene = build(&frame, &t);
        let labels: Vec<_> = scene
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), snap.entities.len() - 1);
        let me_at = t.to_screen(snap.entity(&me.id).unwrap().pos);
        assert!(labels.iter().all(|at| at.distance(me_at) > 1.0), "no label on the hidden player");
        assert!(!scene.shapes.iter().any(|s| matches!(s, Shape::Stroke { color, .. } if *color == opaque(ME_RING))));
    }

    #[test]
    fn marks_are_drawn_over_everything_else() {
        let play = template(Template::Breakout, RinkSpec::iihf());
        let snap = sample(&play, 0);
        let t = RinkTransform::fit(Vec2::ZERO, Vec2::new(1248.0, 648.0), &play.rink, false, ViewZoom::default());
        let frame = |marks| Frame {
            play: &play,
            snapshot: &snap,
            playhead_ms: 0,
            selected: None,
            ghosts: &[],
            hidden: &[],
            marks,
        };
        let plain = build(&frame(&[]), &t);
        let you = Vec2::new(-10.0, 5.0);
        let marks = [
            Mark::Zone { at: Vec2::new(-12.0, 6.0), r_m: 2.0, color: ZONE_FILL },
            Mark::Dot { at: you, label: "you".into(), color: opaque(SELECTED_RING) },
            Mark::Arrow { from: you, to: Vec2::new(-12.0, 6.0), color: opaque(GHOST) },
        ];
        let marked = build(&frame(&marks), &t);

        assert_eq!(marked.shapes[..plain.shapes.len()], plain.shapes[..]);
        let added = &marked.shapes[plain.shapes.len()..];
        assert!(matches!(&added[0], Shape::Fill { color, .. } if *color == ZONE_FILL), "{:?}", added[0]);
        assert!(added.iter().any(
            |s| matches!(s, Shape::Text { at, text, .. } if text == "you" && at.distance(t.to_screen(you)) < 1e-3)
        ));
        assert!(matches!(added.last(), Some(Shape::Fill { color, .. }) if *color == opaque(GHOST)), "arrowhead last");
    }
}
