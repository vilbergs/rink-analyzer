//! Spatial facts derived from a snapshot. Language models reason poorly about
//! raw coordinates but well about "weak side, above the puck, 3.5 m from the
//! nearest opponent", so reviews read these instead of positions alone.
//!
//! Distances are rounded to 0.5 m: positions are entered by hand, and finer
//! numbers would claim a precision the input doesn't have.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::model::{EntityId, EntityKind, Play, Role, TeamSide};
use crate::rink::Zone;
use crate::sample::Snapshot;

/// An opponent closer than this to the line between carrier and receiver
/// counts as covering the pass lane.
pub const LANE_CLEARANCE_M: f32 = 1.0;
/// Opponents this close to the passer are pressure on the carrier (reported as
/// the carrier's nearest opponent), not a blocker of every lane.
pub const PASSER_PRESSURE_M: f32 = 1.5;
/// Within this many meters of the puck's y (or x), a player counts as level
/// with it rather than on one side.
const LEVEL_BAND_M: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Same side of the ice as the puck.
    Strong,
    Weak,
    /// The puck is in the middle lane, so there is no strong side.
    Middle,
}

/// Position along the rink relative to the puck, read from the end the puck
/// is in: above = closer to center ice than the puck, below = closer to the
/// end boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelativeToPuck {
    Above,
    Level,
    Below,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nearest {
    pub id: EntityId,
    pub dist_m: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityFacts {
    pub id: EntityId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<TeamSide>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_me: bool,
    pub pos: Vec2,
    pub facing_deg: f32,
    pub speed_mps: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub has_puck: bool,
    /// From this player's team's point of view.
    pub zone: Zone,
    pub side: Side,
    pub relative_to_puck: RelativeToPuck,
    pub dist_to_puck_m: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearest_opponent: Option<Nearest>,
    /// For teammates of the carrier: whether a pass from the carrier to this
    /// player has no opponent within [`LANE_CLEARANCE_M`] of its line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_pass_lane_from_carrier: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PuckFacts {
    pub pos: Vec2,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carrier: Option<EntityId>,
    /// Zone from the home team's point of view.
    pub zone_for_home: Zone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactSnapshot {
    pub t_ms: u32,
    pub puck: PuckFacts,
    pub entities: Vec<EntityFacts>,
}

pub fn round_half(v: f32) -> f32 {
    (v * 2.0).round() / 2.0
}

fn round_vec(v: Vec2) -> Vec2 {
    Vec2::new(round_half(v.x), round_half(v.y))
}

/// Whether an opponent at `o` sits in the lane from passer `a` to receiver `b`.
fn blocks_lane(o: Vec2, a: Vec2, b: Vec2) -> bool {
    o.distance(a) > PASSER_PRESSURE_M && distance_to_segment(o, a, b) <= LANE_CLEARANCE_M
}

fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn side_of(puck: Vec2, pos: Vec2) -> Side {
    if puck.y.abs() < LEVEL_BAND_M {
        Side::Middle
    } else if pos.y * puck.y > 0.0 {
        Side::Strong
    } else {
        Side::Weak
    }
}

fn relative_to(puck: Vec2, pos: Vec2) -> RelativeToPuck {
    // Toward the end boards of the end the puck is in.
    let toward_end = if puck.x >= 0.0 { 1.0 } else { -1.0 };
    let d = (pos.x - puck.x) * toward_end;
    if d < -LEVEL_BAND_M {
        RelativeToPuck::Above
    } else if d > LEVEL_BAND_M {
        RelativeToPuck::Below
    } else {
        RelativeToPuck::Level
    }
}

/// Facts for one entity in `snap`. With `pos_override`, the entity is treated
/// as if it stood there instead (everyone else stays put), which is how a
/// suggested position is compared with the real one.
pub fn entity_facts(play: &Play, snap: &Snapshot, id: &EntityId, pos_override: Option<Vec2>) -> Option<EntityFacts> {
    let entity = play.entity(id)?;
    if entity.kind == EntityKind::Puck {
        return None;
    }
    let state = snap.entity(id)?;
    let pos = pos_override.unwrap_or(state.pos);
    let team = entity.team;
    let attack_sign = team.map(|t| play.team(t).attack_sign()).unwrap_or(1.0);
    let puck = snap.puck.pos;

    let opponents = snap.entities.iter().filter(|o| {
        let other = play.entity(&o.id);
        other.and_then(|e| e.team).is_some_and(|t| Some(t) != team)
    });
    let nearest_opponent = opponents
        .clone()
        .map(|o| (o.pos.distance(pos), &o.id))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(d, oid)| Nearest { id: oid.clone(), dist_m: round_half(d) });

    let carrier = snap.puck.carrier.as_ref().and_then(|c| snap.entity(c));
    let carrier_team = carrier.and_then(|c| play.entity(&c.id)).and_then(|e| e.team);
    let open_lane = match carrier {
        Some(c) if c.id != *id && carrier_team.is_some() && carrier_team == team => {
            Some(!opponents.clone().any(|o| blocks_lane(o.pos, c.pos, pos)))
        }
        _ => None,
    };

    Some(EntityFacts {
        id: id.clone(),
        team,
        role: entity.role,
        is_me: entity.is_me,
        pos: round_vec(pos),
        facing_deg: state.facing.to_degrees().round(),
        speed_mps: round_half(state.speed_mps),
        has_puck: state.has_puck && pos_override.is_none(),
        zone: play.rink.zone_at(pos.x, attack_sign),
        side: side_of(puck, pos),
        relative_to_puck: relative_to(puck, pos),
        dist_to_puck_m: round_half(pos.distance(puck)),
        nearest_opponent,
        open_pass_lane_from_carrier: open_lane,
    })
}

pub fn facts(play: &Play, snap: &Snapshot) -> FactSnapshot {
    let home_sign = play.team(TeamSide::Home).attack_sign();
    FactSnapshot {
        t_ms: snap.t_ms,
        puck: PuckFacts {
            pos: round_vec(snap.puck.pos),
            carrier: snap.puck.carrier.clone(),
            zone_for_home: play.rink.zone_at(snap.puck.pos.x, home_sign),
        },
        entities: snap.entities.iter().filter_map(|e| entity_facts(play, snap, &e.id, None)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rink::RinkSpec;
    use crate::sample::sample;
    use crate::templates::{build, entity_id, Template};

    #[test]
    fn breakout_facts_at_recovery() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        let snap = sample(&play, 0);
        let f = facts(&play, &snap);
        assert_eq!(f.puck.zone_for_home, Zone::Defensive);
        assert_eq!(f.puck.carrier, Some(entity_id(TeamSide::Home, Role::LD)));

        let lw = f.entities.iter().find(|e| e.is_me).unwrap();
        assert_eq!(lw.zone, Zone::Defensive);
        // Puck is behind the net on the +y side, so LW on +y is strong side and above it.
        assert_eq!(lw.side, Side::Strong);
        assert_eq!(lw.relative_to_puck, RelativeToPuck::Above);
        assert!(lw.open_pass_lane_from_carrier.is_some());

        let away_c = f.entities.iter().find(|e| e.id == entity_id(TeamSide::Away, Role::C)).unwrap();
        assert_eq!(away_c.zone, Zone::Offensive);
        assert_eq!(away_c.open_pass_lane_from_carrier, None);
    }

    #[test]
    fn override_moves_only_that_player() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        let snap = sample(&play, 0);
        let me = play.me().unwrap().id.clone();
        let moved = entity_facts(&play, &snap, &me, Some(Vec2::new(-10.0, -10.0))).unwrap();
        assert_eq!(moved.pos, Vec2::new(-10.0, -10.0));
        assert_eq!(moved.side, Side::Weak);
    }

    #[test]
    fn lane_blocked_by_opponent_on_the_line() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        assert!(distance_to_segment(Vec2::new(5.0, 0.5), a, b) < LANE_CLEARANCE_M);
        assert!(distance_to_segment(Vec2::new(5.0, 3.0), a, b) > LANE_CLEARANCE_M);
        assert!((distance_to_segment(Vec2::new(-3.0, 4.0), a, b) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn pressure_on_passer_does_not_block_lane() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        assert!(!blocks_lane(Vec2::new(0.5, 0.3), a, b));
        assert!(blocks_lane(Vec2::new(5.0, 0.5), a, b));
    }

    #[test]
    fn rounds_to_half_meters() {
        assert_eq!(round_half(3.26), 3.5);
        assert_eq!(round_half(3.24), 3.0);
    }
}
