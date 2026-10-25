//! Small, depth-sorted 3D meshes projected into GPUI's canvas. Play data stays on the ice plane.

use gpui::{px, rgba, App, PathBuilder, Pixels, Point, Window};
use rink_core::{EntityId, Play, Snapshot, Suggestion, Vec2};

use crate::paint::{boards_outline, entity_radius, paint_text, puck_draw_pos};
use crate::transform::RinkTransform;

const PLAYER_HEIGHT: f32 = 1.8;
const SEGMENTS: usize = 20;

#[derive(Clone, Copy)]
struct Vertex {
    pos: Vec2,
    height: f32,
}

impl Vertex {
    fn new(pos: Vec2, height: f32) -> Self {
        Self { pos, height }
    }
}

enum Shape {
    Face(Vec<Vertex>, u32),
    Line(Vec<Vertex>, u32, f32),
    Label(Vertex, String, u32, f32),
}

struct Primitive {
    depth: f32,
    shape: Shape,
}

struct Scene<'a> {
    transform: &'a RinkTransform,
    primitives: Vec<Primitive>,
}

impl<'a> Scene<'a> {
    fn new(transform: &'a RinkTransform) -> Self {
        Self { transform, primitives: Vec::new() }
    }

    fn face(&mut self, vertices: Vec<Vertex>, color: u32) {
        let depth = self.depth(&vertices);
        self.primitives.push(Primitive { depth, shape: Shape::Face(vertices, color) });
    }

    fn line(&mut self, vertices: Vec<Vertex>, color: u32, width: f32) {
        let depth = self.depth(&vertices) - 0.01;
        self.primitives.push(Primitive { depth, shape: Shape::Line(vertices, color, width) });
    }

    fn label(&mut self, at: Vertex, text: String, color: u32, size: f32) {
        let depth = self.transform.depth(at.pos, at.height) - 0.02;
        self.primitives.push(Primitive { depth, shape: Shape::Label(at, text, color, size) });
    }

    fn depth(&self, vertices: &[Vertex]) -> f32 {
        vertices.iter().map(|v| self.transform.depth(v.pos, v.height)).sum::<f32>() / vertices.len() as f32
    }

    fn cylinder(&mut self, center: Vec2, radius: f32, bottom: f32, top: f32, color: u32, alpha: u8) {
        let ring: Vec<_> = (0..SEGMENTS)
            .map(|i| center + Vec2::from_angle(i as f32 * std::f32::consts::TAU / SEGMENTS as f32) * radius)
            .collect();
        for i in 0..SEGMENTS {
            let (a, b) = (ring[i], ring[(i + 1) % SEGMENTS]);
            let normal = ((a + b) / 2.0 - center).normalize();
            let light = 0.66 + 0.22 * normal.dot(Vec2::new(-0.6, 0.8));
            self.face(
                vec![Vertex::new(a, bottom), Vertex::new(b, bottom), Vertex::new(b, top), Vertex::new(a, top)],
                (shade(color, light) << 8) | alpha as u32,
            );
        }
        let mut cap: Vec<_> = ring.iter().map(|&p| Vertex::new(p, top)).collect();
        self.face(cap.clone(), (color << 8) | alpha as u32);
        cap.push(cap[0]);
        self.line(cap, 0xffffff90, 1.0);
    }

    fn boards(&mut self, play: &Play) {
        let outline = boards_outline(&play.rink);
        for (a, b) in outline.iter().copied().zip(outline.iter().copied().cycle().skip(1)) {
            // Subdivide the long straight boards for reliable depth sorting at oblique angles.
            let steps = (a.distance(b) / 1.5).ceil().max(1.0) as usize;
            for i in 0..steps {
                let a1 = a.lerp(b, i as f32 / steps as f32);
                let b1 = a.lerp(b, (i + 1) as f32 / steps as f32);
                let near = self.transform.depth((a1 + b1) / 2.0, 0.0) < self.transform.depth(Vec2::ZERO, 0.0);
                if near {
                    self.face(vertical_quad(a1, b1, -0.65, 0.0), 0x303a4aff);
                }
                self.face(vertical_quad(a1, b1, 0.0, 0.18), if near { 0xe6b84990 } else { 0xe6b849ff });
                self.face(vertical_quad(a1, b1, 0.18, 1.1), if near { 0xd6e4ed55 } else { 0xd6e4edff });
                self.line(vec![Vertex::new(a1, 1.1), Vertex::new(b1, 1.1)], 0xe8f0f8d0, 2.0);
                if !near {
                    self.face(vertical_quad(a1, b1, 1.1, 2.5), 0x91b9cf20);
                    self.line(vec![Vertex::new(a1, 2.5), Vertex::new(b1, 2.5)], 0x96b4ca70, 1.0);
                }
            }
        }
    }

    fn goals(&mut self, play: &Play) {
        let rink = &play.rink;
        let half = rink.goal_width_m / 2.0;
        for side in [-1.0, 1.0] {
            let front = side * rink.goal_line_x();
            let back = side * (rink.goal_line_x() + rink.goal_depth_m);
            let a = Vec2::new(front, -half);
            let b = Vec2::new(front, half);
            let c = Vec2::new(back, half);
            let d = Vec2::new(back, -half);
            self.face(vertical_quad(c, d, 0.0, 1.05), 0xffffff55);
            self.face(
                vec![Vertex::new(a, 1.22), Vertex::new(b, 1.22), Vertex::new(c, 1.05), Vertex::new(d, 1.05)],
                0xffffff45,
            );
            for (p, q) in [(a, d), (b, c)] {
                self.face(
                    vec![Vertex::new(p, 0.0), Vertex::new(q, 0.0), Vertex::new(q, 1.05), Vertex::new(p, 1.22)],
                    0xffffff40,
                );
                self.line(vec![Vertex::new(p, 1.22), Vertex::new(q, 1.05), Vertex::new(q, 0.0)], 0xb42238ff, 2.0);
            }
            self.line(
                vec![Vertex::new(a, 0.0), Vertex::new(a, 1.22), Vertex::new(b, 1.22), Vertex::new(b, 0.0)],
                0xd32440ff,
                3.0,
            );
            self.line(vec![Vertex::new(d, 1.05), Vertex::new(c, 1.05)], 0xb42238ff, 2.0);
            for i in 1..8 {
                let u = i as f32 / 8.0;
                let front_pos = a.lerp(b, u);
                let back_pos = d.lerp(c, u);
                self.line(
                    vec![Vertex::new(front_pos, 1.22), Vertex::new(back_pos, 1.05), Vertex::new(back_pos, 0.0)],
                    0xb2beca90,
                    0.8,
                );
            }
            for i in 1..5 {
                let height = i as f32 * 1.05 / 5.0;
                self.line(vec![Vertex::new(d, height), Vertex::new(c, height)], 0xb2beca90, 0.8);
            }
        }
    }

    fn ring(&mut self, center: Vec2, radius: f32, color: u32, width: f32) {
        let points = (0..=SEGMENTS)
            .map(|i| {
                Vertex::new(
                    center + Vec2::from_angle(i as f32 * std::f32::consts::TAU / SEGMENTS as f32) * radius,
                    0.03,
                )
            })
            .collect();
        self.line(points, color, width);
    }
}

fn vertical_quad(a: Vec2, b: Vec2, bottom: f32, top: f32) -> Vec<Vertex> {
    vec![Vertex::new(a, bottom), Vertex::new(b, bottom), Vertex::new(b, top), Vertex::new(a, top)]
}

fn shade(color: u32, light: f32) -> u32 {
    let channel = |shift: u32| (((color >> shift) & 255_u32) as f32 * light).clamp(0.0, 255.0) as u32;
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn scene(
    t: &RinkTransform,
    play: &Play,
    snapshot: &Snapshot,
    selected: Option<&EntityId>,
    active: &[(Suggestion, Vec2)],
) -> Vec<Primitive> {
    let mut scene = Scene::new(t);
    scene.boards(play);
    scene.goals(play);
    for state in &snapshot.entities {
        let Some(entity) = play.entity(&state.id) else { continue };
        let radius = entity_radius(entity.kind);
        let color = entity.team.map(|side| play.team(side).color_rgb()).unwrap_or(0x808080);
        let shadow: Vec<_> = (0..SEGMENTS)
            .map(|i| {
                Vertex::new(
                    state.pos
                        + Vec2::new(0.3, -0.3)
                        + Vec2::from_angle(i as f32 * std::f32::consts::TAU / SEGMENTS as f32) * radius * 1.1,
                    0.01,
                )
            })
            .collect();
        scene.face(shadow, 0x14213635);
        if entity.is_me {
            scene.ring(state.pos, radius + 0.3, (crate::theme::ME << 8) | 255, 3.0);
        }
        if selected == Some(&state.id) {
            scene.ring(state.pos, radius + 0.6, (crate::theme::ACCENT << 8) | 255, 3.0);
        }
        scene.cylinder(state.pos, radius, 0.08, PLAYER_HEIGHT, color, 255);
        let direction = Vec2::from_angle(state.facing);
        let blade = state.pos + direction * (radius + 1.0);
        scene.line(
            vec![
                Vertex::new(state.pos + direction * radius, 0.9),
                Vertex::new(blade, 0.06),
                Vertex::new(blade + Vec2::new(-direction.y, direction.x) * 0.5, 0.06),
            ],
            0x26354aff,
            2.5,
        );
        let label = entity
            .number
            .map(|n| n.to_string())
            .or_else(|| entity.role.map(|r| r.as_str().to_string()))
            .unwrap_or_default();
        scene.label(
            Vertex::new(state.pos, PLAYER_HEIGHT + 0.04),
            label,
            0xffffff,
            (t.scale_at(state.pos, PLAYER_HEIGHT) * radius * 1.15).clamp(10.0, 26.0),
        );
    }
    let puck = puck_draw_pos(play, snapshot);
    scene.cylinder(puck, 0.35_f32.max(3.5 / t.scale_at(puck, 0.0)), 0.02, 0.18, 0x17202b, 255);
    if selected.is_some_and(|id| play.entity(id).is_some_and(|e| e.is_puck())) {
        scene.ring(puck, 0.8, (crate::theme::ACCENT << 8) | 255, 2.0);
    }
    for (suggestion, pos) in active {
        let Some(entity) = play.entity(&suggestion.entity) else { continue };
        let radius = entity_radius(entity.kind);
        let color = if suggestion.is_stale(play) { 0x8a93a0 } else { 0x35c47c };
        scene.cylinder(*pos, radius, 0.08, PLAYER_HEIGHT, color, 115);
        scene.ring(*pos, radius + 0.2, (color << 8) | 200, 2.0);
        let label = entity.number.map(|n| format!("{n}′")).unwrap_or_else(|| "′".into());
        scene.label(
            Vertex::new(*pos, PLAYER_HEIGHT + 0.04),
            label,
            0x124931,
            (t.scale_at(*pos, PLAYER_HEIGHT) * radius * 1.1).clamp(10.0, 26.0),
        );
    }
    scene.primitives.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    scene.primitives
}

pub fn paint_scene(
    window: &mut Window,
    cx: &mut App,
    t: &RinkTransform,
    play: &Play,
    snapshot: &Snapshot,
    selected: Option<&EntityId>,
    active: &[(Suggestion, Vec2)],
) {
    for primitive in scene(t, play, snapshot, selected, active) {
        match primitive.shape {
            Shape::Face(vertices, color) => {
                let points: Vec<_> = vertices.iter().map(|v| t.to_screen_at(v.pos, v.height)).collect();
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&points, true);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, rgba(color));
                }
            }
            Shape::Line(vertices, color, width) => {
                let points: Vec<_> = vertices.iter().map(|v| t.to_screen_at(v.pos, v.height)).collect();
                let mut builder = PathBuilder::stroke(px(width));
                builder.add_polygon(&points, false);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, rgba(color));
                }
            }
            Shape::Label(at, label, color, size) => {
                paint_text(window, cx, t.to_screen_at(at.pos, at.height), &label, px(size), color)
            }
        }
    }
}

/// Pick the raised marker silhouette rather than its position on the ice. The nearest marker wins.
pub fn hit_test(t: &RinkTransform, play: &Play, snapshot: &Snapshot, at: Point<Pixels>) -> Option<EntityId> {
    let pointer = Vec2::new(f32::from(at.x), f32::from(at.y));
    let screen = |p: Vec2, height| {
        let p = t.to_screen_at(p, height);
        Vec2::new(f32::from(p.x), f32::from(p.y))
    };
    if let Some(puck) = play.puck() {
        let pos = puck_draw_pos(play, snapshot);
        if pointer.distance(screen(pos, 0.1)) <= (0.35 * t.scale_at(pos, 0.0)).max(4.0) + 3.0 {
            return Some(puck.id.clone());
        }
    }
    snapshot
        .entities
        .iter()
        .filter_map(|s| {
            let entity = play.entity(&s.id)?;
            let (bottom, top) = (screen(s.pos, 0.0), screen(s.pos, PLAYER_HEIGHT));
            let axis = top - bottom;
            let along = ((pointer - bottom).dot(axis) / axis.length_squared().max(0.001)).clamp(0.0, 1.0);
            let distance = pointer.distance(bottom + along * axis);
            let radius = entity_radius(entity.kind) * t.scale_at(s.pos, PLAYER_HEIGHT) + 4.0;
            (distance <= radius).then_some((t.depth(s.pos, PLAYER_HEIGHT / 2.0), s.id.clone()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::{Camera3d, ViewZoom};
    use gpui::{point, size, Bounds};
    use rink_core::{
        sample,
        templates::{build, Template},
        RinkSpec,
    };

    fn transform(play: &Play, camera: Camera3d) -> RinkTransform {
        RinkTransform::fit_3d(
            Bounds::new(point(px(0.0), px(0.0)), size(px(1200.0), px(700.0))),
            &play.rink,
            false,
            ViewZoom::default(),
            camera,
        )
    }

    #[test]
    fn raised_markers_can_be_picked_at_their_tops() {
        let play = build(Template::Faceoff, RinkSpec::iihf());
        let snapshot = sample(&play, 0);
        for yaw in [0.0_f32, 90.0, 180.0, 270.0] {
            let t = transform(&play, Camera3d { yaw: yaw.to_radians(), elevation: 50.0_f32.to_radians() });
            for entity in &snapshot.entities {
                // Isolate the marker to verify its whole projected height, independently of occlusion.
                let mut isolated = snapshot.clone();
                isolated.entities.retain(|e| e.id == entity.id);
                isolated.puck.carrier = None;
                isolated.puck.pos = Vec2::new(29.0, 0.0);
                assert_eq!(
                    hit_test(&t, &play, &isolated, t.to_screen_at(entity.pos, PLAYER_HEIGHT)),
                    Some(entity.id.clone())
                );
            }
        }
    }

    #[test]
    fn meshes_are_finite_and_sorted_back_to_front() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        let snapshot = sample(&play, 3000);
        for camera in [Camera3d::default(), Camera3d { yaw: 2.0, elevation: 25.0_f32.to_radians() }] {
            let t = transform(&play, camera);
            let primitives = scene(&t, &play, &snapshot, None, &[]);
            assert!(primitives.len() > 500);
            assert!(primitives.windows(2).all(|p| p[0].depth >= p[1].depth));
            for p in primitives {
                assert!(p.depth.is_finite() && p.depth > 0.0);
                let vertices = match p.shape {
                    Shape::Face(v, _) | Shape::Line(v, _, _) => v,
                    Shape::Label(v, _, _, _) => vec![v],
                };
                for v in vertices {
                    let at = t.to_screen_at(v.pos, v.height);
                    assert!(f32::from(at.x).is_finite() && f32::from(at.y).is_finite());
                }
            }
        }
    }
}
