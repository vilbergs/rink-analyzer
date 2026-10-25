//! Rink geometry as data. Every line position comes from a [`RinkSpec`], so the
//! renderer, the validator and the MCP server all agree on where the ice is.
//!
//! Coordinates are meters with the origin at center ice. x runs along the
//! length (+x is the home team's attacking direction), y across the width.

use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RinkSpec {
    pub name: String,
    pub length_m: f32,
    pub width_m: f32,
    pub corner_radius_m: f32,
    pub goal_line_from_end_m: f32,
    pub blue_line_from_end_m: f32,
    pub line_width_m: f32,
    pub blue_line_width_m: f32,
    pub red_line_width_m: f32,
    pub center_circle_radius_m: f32,
    pub faceoff_circle_radius_m: f32,
    pub end_dot_from_goal_line_m: f32,
    pub end_dot_lateral_m: f32,
    pub neutral_dot_from_blue_line_m: f32,
    pub neutral_dot_lateral_m: f32,
    pub crease_radius_m: f32,
    pub goal_width_m: f32,
    pub goal_depth_m: f32,
}

/// Which preset a play uses. Plays store the full spec too, so a file stays
/// readable even if a preset changes later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RinkPreset {
    Iihf,
    Nhl,
}

// IIHF figures checked against the 2026-27 rule book (rules 1.2, 1.5, 1.9):
// 60 m long, corner radius 7.0-8.5 m, goal lines 4.0 m from the end boards,
// 4.5 m circles, neutral-zone spots 1.5 m from the blue lines and 14 m apart.
// The blue lines' 22.86 m from the end boards is quoted from the rule's text
// by secondary sources; the online rule book page leaves it out.
const IIHF_JSON: &str = include_str!("../../../rinks/iihf.json");
const NHL_JSON: &str = include_str!("../../../rinks/nhl.json");

/// Zones relative to one team's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Defensive,
    Neutral,
    Offensive,
}

impl RinkSpec {
    pub fn preset(preset: RinkPreset) -> RinkSpec {
        let json = match preset {
            RinkPreset::Iihf => IIHF_JSON,
            RinkPreset::Nhl => NHL_JSON,
        };
        serde_json::from_str(json).expect("bundled rink preset is valid JSON")
    }

    pub fn iihf() -> RinkSpec {
        Self::preset(RinkPreset::Iihf)
    }

    pub fn nhl() -> RinkSpec {
        Self::preset(RinkPreset::Nhl)
    }

    pub fn half_length(&self) -> f32 {
        self.length_m / 2.0
    }

    pub fn half_width(&self) -> f32 {
        self.width_m / 2.0
    }

    /// |x| of each goal line.
    pub fn goal_line_x(&self) -> f32 {
        self.half_length() - self.goal_line_from_end_m
    }

    /// |x| of each blue line (its center).
    pub fn blue_line_x(&self) -> f32 {
        self.half_length() - self.blue_line_from_end_m
    }

    /// The four end-zone faceoff dots.
    pub fn end_dots(&self) -> [Vec2; 4] {
        let x = self.goal_line_x() - self.end_dot_from_goal_line_m;
        let y = self.end_dot_lateral_m;
        [Vec2::new(-x, -y), Vec2::new(-x, y), Vec2::new(x, -y), Vec2::new(x, y)]
    }

    /// The four neutral-zone faceoff dots.
    pub fn neutral_dots(&self) -> [Vec2; 4] {
        let x = self.blue_line_x() - self.neutral_dot_from_blue_line_m;
        let y = self.neutral_dot_lateral_m;
        [Vec2::new(-x, -y), Vec2::new(-x, y), Vec2::new(x, -y), Vec2::new(x, y)]
    }

    /// `p`, pulled back toward center ice until it is at least `clearance_m`
    /// inside the boards.
    pub fn clamp_inside(&self, p: Vec2, clearance_m: f32) -> Vec2 {
        if self.distance_inside_boards(p) >= clearance_m {
            return p;
        }
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if self.distance_inside_boards(p * mid) >= clearance_m {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        p * lo
    }

    /// True when `p` is on the ice surface (inside the rounded boards).
    pub fn contains(&self, p: Vec2) -> bool {
        self.distance_inside_boards(p) >= 0.0
    }

    /// Signed distance from `p` to the boards: positive inside, negative outside.
    pub fn distance_inside_boards(&self, p: Vec2) -> f32 {
        let r = self.corner_radius_m;
        let hx = self.half_length();
        let hy = self.half_width();
        let ax = p.x.abs();
        let ay = p.y.abs();
        let cx = hx - r;
        let cy = hy - r;
        if ax > cx && ay > cy {
            r - Vec2::new(ax - cx, ay - cy).length()
        } else {
            (hx - ax).min(hy - ay)
        }
    }

    /// Zone at `x` for a team attacking toward `attack_sign` (+1 or -1).
    pub fn zone_at(&self, x: f32, attack_sign: f32) -> Zone {
        let bx = self.blue_line_x();
        let rel = x * attack_sign;
        if rel > bx {
            Zone::Offensive
        } else if rel < -bx {
            Zone::Defensive
        } else {
            Zone::Neutral
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_load() {
        let iihf = RinkSpec::iihf();
        assert_eq!(iihf.length_m, 60.0);
        assert_eq!(iihf.width_m, 30.0);
        let nhl = RinkSpec::nhl();
        assert!(nhl.width_m < iihf.width_m);
    }

    #[test]
    fn line_positions() {
        let r = RinkSpec::iihf();
        assert!((r.goal_line_x() - 26.0).abs() < 1e-4);
        assert!((r.blue_line_x() - 7.14).abs() < 1e-3);
    }

    #[test]
    fn contains_respects_corners() {
        let r = RinkSpec::iihf();
        assert!(r.contains(Vec2::ZERO));
        assert!(r.contains(Vec2::new(29.9, 0.0)));
        assert!(!r.contains(Vec2::new(30.1, 0.0)));
        // The extreme corner is cut off by the 8.5 m radius.
        assert!(!r.contains(Vec2::new(29.5, 14.5)));
        assert!(r.contains(Vec2::new(25.0, 10.0)));
    }

    #[test]
    fn zones() {
        let r = RinkSpec::iihf();
        assert_eq!(r.zone_at(20.0, 1.0), Zone::Offensive);
        assert_eq!(r.zone_at(20.0, -1.0), Zone::Defensive);
        assert_eq!(r.zone_at(0.0, 1.0), Zone::Neutral);
    }
}
