//! `sample(play, t_ms)`: the single function that turns stored keyframes into
//! the state of every entity at a moment. The renderer and the `rink` CLI both
//! call it, so they can never disagree about where anyone was.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::model::{Entity, EntityId, Event, EventKind, Keyframe, Play};
use crate::spline::{hermite, hermite_velocity, lerp_angle, sample_segment};

/// How far in front of the carrier the puck sits, in meters.
pub const STICK_REACH_M: f32 = 0.6;
/// Speed of a released puck with no receiver, used to time its flight to an
/// event's `to` position.
pub const FREE_PUCK_SPEED_MPS: f32 = 25.0;
/// Typical pass speed, used to time a pass's arrival when it has no `arrive_ms`.
pub const PASS_SPEED_MPS: f32 = 15.0;
const MIN_PASS_MS: u32 = 100;
/// Below this speed, a skater with no explicit facing keeps facing their team's
/// attacking direction rather than their (noisy) direction of travel.
const MIN_TRAVEL_SPEED_MPS: f32 = 0.3;
const SPEED_WINDOW_MS: u32 = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub id: EntityId,
    pub pos: Vec2,
    /// Radians, 0 = +x.
    pub facing: f32,
    pub speed_mps: f32,
    pub has_puck: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PuckState {
    pub pos: Vec2,
    pub carrier: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub t_ms: u32,
    /// Skaters and goalies that have at least one keyframe, in play order.
    pub entities: Vec<EntityState>,
    pub puck: PuckState,
}

impl Snapshot {
    pub fn entity(&self, id: &EntityId) -> Option<&EntityState> {
        self.entities.iter().find(|e| &e.id == id)
    }
}

/// Every entity's state at `t_ms`.
pub fn sample(play: &Play, t_ms: u32) -> Snapshot {
    let possessions = possessions(play);
    let carrier = carrier_at(&possessions, t_ms);

    let entities = play
        .entities
        .iter()
        .filter(|e| !e.is_puck())
        .filter_map(|e| {
            let track = play.track(&e.id);
            let pos = position_at(track, t_ms)?;
            Some(EntityState {
                id: e.id.clone(),
                pos,
                facing: facing_at(play, e, t_ms),
                speed_mps: speed_at(track, t_ms),
                has_puck: carrier.is_some_and(|c| c.entity == e.id),
            })
        })
        .collect();

    let puck = match carrier {
        Some(c) => PuckState { pos: stick_pos(play, &c.entity, t_ms), carrier: Some(c.entity.clone()) },
        None => PuckState { pos: free_puck_pos(play, &possessions, t_ms), carrier: None },
    };

    Snapshot { t_ms, entities, puck }
}

/// Interpolated position of a track at `t_ms`; held before the first and after
/// the last keyframe. None for an empty track.
pub fn position_at(track: &[Keyframe], t_ms: u32) -> Option<Vec2> {
    let first = track.first()?;
    let last = track.last()?;
    if t_ms <= first.t_ms {
        return Some(first.pos);
    }
    if t_ms >= last.t_ms {
        return Some(last.pos);
    }
    // First keyframe strictly after t; the segment is [i - 1, i].
    let i = track.partition_point(|k| k.t_ms <= t_ms);
    let (a, b) = (&track[i - 1], &track[i]);
    let u = (t_ms - a.t_ms) as f32 / (b.t_ms - a.t_ms) as f32;
    if let Some((v0, v1)) = hermite_tangents(track, i - 1) {
        return Some(hermite(a.pos, v0, b.pos, v1, seconds(b.t_ms - a.t_ms), u));
    }
    let points: Vec<Vec2> = track.iter().map(|k| k.pos).collect();
    Some(sample_segment(&points, i - 1, u))
}

fn seconds(ms: u32) -> f32 {
    ms as f32 / 1000.0
}

/// Velocities at both ends of the segment from keyframe `i` to `i + 1`, when
/// either end has one: the stored velocity, or for an end without one the
/// slope through its neighbours. None means the segment is plain Catmull-Rom.
fn hermite_tangents(track: &[Keyframe], i: usize) -> Option<(Vec2, Vec2)> {
    let (a, b) = (&track[i], &track[i + 1]);
    if a.vel.is_none() && b.vel.is_none() {
        return None;
    }
    let slope = |j: usize| {
        let (lo, hi) = (&track[j.saturating_sub(1)], &track[(j + 1).min(track.len() - 1)]);
        if hi.t_ms > lo.t_ms {
            (hi.pos - lo.pos) / seconds(hi.t_ms - lo.t_ms)
        } else {
            Vec2::ZERO
        }
    };
    Some((a.vel.unwrap_or_else(|| slope(i)), b.vel.unwrap_or_else(|| slope(i + 1))))
}

/// True when `t_ms` falls in a stretch baked from a skating order: the
/// keyframe at or before it has a velocity, and so does the next one, if any.
pub fn is_baked_at(track: &[Keyframe], t_ms: u32) -> bool {
    let i = track.partition_point(|k| k.t_ms <= t_ms);
    i > 0 && track[i - 1].vel.is_some() && track.get(i).is_none_or(|k| k.vel.is_some())
}

/// Speed in m/s.
pub fn speed_at(track: &[Keyframe], t_ms: u32) -> f32 {
    velocity_at(track, t_ms).length()
}

/// Velocity in m/s: exact where keyframes carry velocities, otherwise from a
/// centered finite difference.
pub fn velocity_at(track: &[Keyframe], t_ms: u32) -> Vec2 {
    let (Some(first), Some(last)) = (track.first(), track.last()) else { return Vec2::ZERO };
    if t_ms >= first.t_ms && t_ms < last.t_ms {
        let i = track.partition_point(|k| k.t_ms <= t_ms);
        let (a, b) = (&track[i - 1], &track[i]);
        if let Some((v0, v1)) = hermite_tangents(track, i - 1) {
            let u = (t_ms - a.t_ms) as f32 / (b.t_ms - a.t_ms) as f32;
            return hermite_velocity(a.pos, v0, b.pos, v1, seconds(b.t_ms - a.t_ms), u);
        }
    } else if let Some(v) = last.vel {
        // At the last keyframe it moves as baked; after it, it is held still.
        return if t_ms == last.t_ms { v } else { Vec2::ZERO };
    }
    let t0 = t_ms.saturating_sub(SPEED_WINDOW_MS);
    let t1 = t_ms.saturating_add(SPEED_WINDOW_MS);
    match (position_at(track, t0), position_at(track, t1)) {
        (Some(a), Some(b)) => (b - a) / seconds(t1 - t0),
        _ => Vec2::ZERO,
    }
}

/// Facing from the keyframes that set one (shortest-arc between them, held at
/// the ends). With none set, face the direction of travel, else the team's
/// attacking direction. Stretches baked from skating orders turn as skated:
/// their keyframes all carry a facing, and are left out of the rest.
pub fn facing_at(play: &Play, entity: &Entity, t_ms: u32) -> f32 {
    let track = play.track(&entity.id);
    if let Some(f) = baked_facing(track, t_ms) {
        return f;
    }
    let faced: Vec<(u32, f32)> =
        track.iter().filter(|k| k.vel.is_none()).filter_map(|k| k.facing.map(|f| (k.t_ms, f))).collect();

    if let (Some(&(t_first, f_first)), Some(&(t_last, f_last))) = (faced.first(), faced.last()) {
        if t_ms <= t_first {
            return f_first;
        }
        if t_ms >= t_last {
            return f_last;
        }
        let i = faced.partition_point(|&(t, _)| t <= t_ms);
        let (ta, fa) = faced[i - 1];
        let (tb, fb) = faced[i];
        return lerp_angle(fa, fb, (t_ms - ta) as f32 / (tb - ta) as f32);
    }

    let v = velocity_at(track, t_ms);
    if v.length() > MIN_TRAVEL_SPEED_MPS {
        return v.y.atan2(v.x);
    }
    let attack_sign = entity.team.map(|s| play.team(s).attack_sign()).unwrap_or(1.0);
    if attack_sign > 0.0 {
        0.0
    } else {
        std::f32::consts::PI
    }
}

fn baked_facing(track: &[Keyframe], t_ms: u32) -> Option<f32> {
    if !is_baked_at(track, t_ms) {
        return None;
    }
    let i = track.partition_point(|k| k.t_ms <= t_ms);
    let a = &track[i - 1];
    let Some(b) = track.get(i) else { return a.facing };
    let u = (t_ms - a.t_ms) as f32 / (b.t_ms - a.t_ms) as f32;
    Some(lerp_angle(a.facing?, b.facing?, u))
}

/// One stretch of time during which a single entity holds the puck.
#[derive(Debug, Clone, PartialEq)]
pub struct Possession {
    pub entity: EntityId,
    pub start_ms: u32,
    /// None = holds it to the end of the play.
    pub end_ms: Option<u32>,
}

impl Possession {
    fn covers(&self, t_ms: u32) -> bool {
        self.start_ms <= t_ms && self.end_ms.is_none_or(|e| t_ms < e)
    }
}

/// Events after which the puck is loose (until someone gains it).
fn releases_puck(kind: EventKind) -> bool {
    matches!(kind, EventKind::Pass | EventKind::Shot | EventKind::Turnover | EventKind::Whistle)
}

/// When a pass reaches its receiver: `arrive_ms` if set, otherwise the
/// distance at [`PASS_SPEED_MPS`]. None for anything but a pass to someone.
pub fn pass_arrival_ms(play: &Play, ev: &Event) -> Option<u32> {
    if ev.kind != EventKind::Pass {
        return None;
    }
    let receiver = ev.actors.get(1)?;
    if let Some(a) = ev.arrive_ms {
        return Some(a.max(ev.t_ms));
    }
    let from = ev.from.or_else(|| ev.actors.first().map(|p| stick_pos(play, p, ev.t_ms)))?;
    let to = position_at(play.track(receiver), ev.t_ms)?;
    let flight_ms = (from.distance(to) / PASS_SPEED_MPS * 1000.0) as u32;
    Some(ev.t_ms + flight_ms.max(MIN_PASS_MS))
}

enum Change {
    Gain(EntityId),
    /// A pass reaching its receiver; void if anyone gained the puck since it
    /// was released (an interception).
    Arrive {
        receiver: EntityId,
        released_ms: u32,
    },
    Release,
}

/// Possession intervals, sorted by start, derived from the events (see
/// [`Event`] for what each kind does).
pub fn possessions(play: &Play) -> Vec<Possession> {
    // (time, order within the same time, change): releases go before gains.
    let mut changes: Vec<(u32, u8, Change)> = Vec::new();
    for ev in &play.events {
        let first = ev.actors.first().cloned();
        let second = ev.actors.get(1).cloned();
        match ev.kind {
            EventKind::PuckRecovery | EventKind::Faceoff => {
                if let Some(a) = first {
                    changes.push((ev.t_ms, 1, Change::Gain(a)));
                }
            }
            EventKind::Pass => {
                changes.push((ev.t_ms, 0, Change::Release));
                if let (Some(receiver), Some(arrive)) = (second, pass_arrival_ms(play, ev)) {
                    changes.push((arrive, 1, Change::Arrive { receiver, released_ms: ev.t_ms }));
                }
            }
            EventKind::Shot | EventKind::Whistle => changes.push((ev.t_ms, 0, Change::Release)),
            EventKind::Turnover => {
                changes.push((ev.t_ms, 0, Change::Release));
                if let Some(b) = second {
                    changes.push((ev.t_ms, 1, Change::Gain(b)));
                }
            }
            _ => {}
        }
    }
    changes.sort_by_key(|c| (c.0, c.1));

    let mut out: Vec<Possession> = Vec::new();
    let mut last_gain_ms: Option<u32> = None;
    let end_current = |out: &mut Vec<Possession>, t: u32| {
        if let Some(p) = out.last_mut().filter(|p| p.end_ms.is_none()) {
            p.end_ms = Some(t);
        }
    };
    for (t, _, change) in changes {
        let gainer = match change {
            Change::Release => {
                end_current(&mut out, t);
                None
            }
            Change::Gain(e) => Some(e),
            Change::Arrive { receiver, released_ms } => {
                last_gain_ms.is_none_or(|g| g < released_ms).then_some(receiver)
            }
        };
        if let Some(e) = gainer {
            if out.last().is_some_and(|p| p.end_ms.is_none() && p.entity == e) {
                continue;
            }
            end_current(&mut out, t);
            out.push(Possession { entity: e, start_ms: t, end_ms: None });
            last_gain_ms = Some(t);
        }
    }
    out.retain(|p| p.end_ms.is_none_or(|e| e > p.start_ms));
    out
}

fn carrier_at(possessions: &[Possession], t_ms: u32) -> Option<&Possession> {
    possessions.iter().filter(|p| p.covers(t_ms)).max_by_key(|p| p.start_ms)
}

/// Where the puck sits on `id`'s stick at `t_ms`.
fn stick_pos(play: &Play, id: &EntityId, t_ms: u32) -> Vec2 {
    let Some(entity) = play.entity(id) else { return Vec2::ZERO };
    let pos = position_at(play.track(id), t_ms).unwrap_or(Vec2::ZERO);
    let facing = facing_at(play, entity, t_ms);
    pos + Vec2::from_angle(facing) * STICK_REACH_M
}

/// Puck position with no carrier: linear between the last release, any puck
/// keyframes, and the next pickup.
fn free_puck_pos(play: &Play, possessions: &[Possession], t_ms: u32) -> Vec2 {
    let prev = possessions.iter().filter(|p| p.end_ms.is_some_and(|e| e <= t_ms)).max_by_key(|p| p.end_ms);
    let next = possessions.iter().filter(|p| p.start_ms > t_ms).min_by_key(|p| p.start_ms);
    let window_start = prev.and_then(|p| p.end_ms).unwrap_or(0);
    let window_end = next.map(|p| p.start_ms).unwrap_or(u32::MAX);

    let mut anchors: Vec<(u32, Vec2)> = Vec::new();
    let mut release_to = None;
    if let Some(p) = prev {
        let t = p.end_ms.unwrap_or(0);
        let event = play.events.iter().find(|ev| releases_puck(ev.kind) && ev.t_ms == t);
        let from = event.and_then(|ev| ev.from).unwrap_or_else(|| stick_pos(play, &p.entity, t));
        anchors.push((t, from));
        release_to = event.and_then(|ev| ev.to);
    }
    if let Some(puck) = play.puck() {
        anchors.extend(
            play.track(&puck.id)
                .iter()
                .filter(|k| k.t_ms > window_start && k.t_ms < window_end)
                .map(|k| (k.t_ms, k.pos)),
        );
        if prev.is_none() {
            // Before anyone has held it, puck keyframes at or before t count too.
            anchors.extend(play.track(&puck.id).iter().filter(|k| k.t_ms <= window_start).map(|k| (k.t_ms, k.pos)));
        }
    }
    match next {
        Some(p) => anchors.push((p.start_ms, stick_pos(play, &p.entity, p.start_ms))),
        // A release's `to` only applies when nothing else says where the puck went.
        None if anchors.len() == 1 => {
            if let (Some(to), Some(&(t, from))) = (release_to, anchors.first()) {
                let flight_ms = (from.distance(to) / FREE_PUCK_SPEED_MPS * 1000.0) as u32;
                anchors.push((t.saturating_add(flight_ms.max(1)), to));
            }
        }
        None => {}
    }
    anchors.sort_by_key(|a| a.0);
    anchors.dedup_by_key(|a| a.0);

    let Some(&(t_first, p_first)) = anchors.first() else { return Vec2::ZERO };
    if t_ms <= t_first {
        return p_first;
    }
    for w in anchors.windows(2) {
        let ((ta, pa), (tb, pb)) = (w[0], w[1]);
        if t_ms <= tb {
            return pa.lerp(pb, (t_ms - ta) as f32 / (tb - ta) as f32);
        }
    }
    anchors.last().map(|a| a.1).unwrap_or(Vec2::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EntityKind, Role, TeamSide};
    use crate::rink::RinkSpec;

    fn skater(id: &str, team: TeamSide) -> Entity {
        Entity {
            id: id.into(),
            kind: EntityKind::Skater,
            team: Some(team),
            number: Some(1),
            role: Some(Role::C),
            label: String::new(),
            is_me: false,
        }
    }

    fn two_skater_play() -> Play {
        let mut play = Play::new("test", RinkSpec::iihf());
        play.entities.push(skater("a", TeamSide::Home));
        play.entities.push(skater("b", TeamSide::Home));
        play.entities.push(Entity {
            id: "puck".into(),
            kind: EntityKind::Puck,
            team: None,
            number: None,
            role: None,
            label: String::new(),
            is_me: false,
        });
        play
    }

    #[test]
    fn holds_before_and_after_keyframes() {
        let mut play = two_skater_play();
        play.set_keyframe(&"a".into(), Keyframe::at(1000, 0.0, 0.0));
        play.set_keyframe(&"a".into(), Keyframe::at(2000, 10.0, 0.0));
        let s0 = sample(&play, 0);
        let s3 = sample(&play, 3000);
        assert_eq!(s0.entity(&"a".into()).unwrap().pos, Vec2::new(0.0, 0.0));
        assert_eq!(s3.entity(&"a".into()).unwrap().pos, Vec2::new(10.0, 0.0));
        // b has no keyframes, so it isn't on the ice.
        assert!(s0.entity(&"b".into()).is_none());
    }

    #[test]
    fn speed_matches_straight_skate() {
        let mut play = two_skater_play();
        play.set_keyframe(&"a".into(), Keyframe::at(0, 0.0, 0.0));
        play.set_keyframe(&"a".into(), Keyframe::at(2000, 10.0, 0.0));
        let s = sample(&play, 1000);
        let speed = s.entity(&"a".into()).unwrap().speed_mps;
        assert!((speed - 5.0).abs() < 0.5, "speed {speed}");
    }

    #[test]
    fn facing_follows_travel_without_keyframed_facing() {
        let mut play = two_skater_play();
        play.set_keyframe(&"a".into(), Keyframe::at(0, 0.0, 0.0));
        play.set_keyframe(&"a".into(), Keyframe::at(1000, 0.0, 5.0));
        let f = sample(&play, 500).entity(&"a".into()).unwrap().facing;
        assert!((f - std::f32::consts::FRAC_PI_2).abs() < 0.05);
    }

    fn ev(t_ms: u32, kind: EventKind, actors: &[&str]) -> Event {
        let ids: Vec<EntityId> = actors.iter().map(|&a| a.into()).collect();
        let refs: Vec<&EntityId> = ids.iter().collect();
        Event::new(t_ms, kind, &refs)
    }

    #[test]
    fn pass_moves_puck_between_carriers() {
        let mut play = two_skater_play();
        play.set_keyframe(&"a".into(), Keyframe::at(0, 0.0, 0.0).facing(0.0));
        play.set_keyframe(&"b".into(), Keyframe::at(0, 10.0, 0.0).facing(std::f32::consts::PI));
        play.events.push(ev(0, EventKind::PuckRecovery, &["a"]));
        play.events.push(Event { arrive_ms: Some(2000), ..ev(1000, EventKind::Pass, &["a", "b"]) });

        let s = sample(&play, 500);
        assert_eq!(s.puck.carrier, Some("a".into()));
        assert!(s.entity(&"a".into()).unwrap().has_puck);

        let mid = sample(&play, 1500);
        assert_eq!(mid.puck.carrier, None);
        assert!(mid.puck.pos.x > 0.6 && mid.puck.pos.x < 9.4, "puck in flight at {}", mid.puck.pos);

        let s = sample(&play, 2500);
        assert_eq!(s.puck.carrier, Some("b".into()));
    }

    #[test]
    fn pass_arrival_is_timed_from_distance() {
        let mut play = two_skater_play();
        play.set_keyframe(&"a".into(), Keyframe::at(0, 0.0, 0.0).facing(0.0));
        play.set_keyframe(&"b".into(), Keyframe::at(0, 15.6, 0.0));
        play.events.push(ev(0, EventKind::PuckRecovery, &["a"]));
        play.events.push(ev(1000, EventKind::Pass, &["a", "b"]));
        // 15 m from stick to receiver at 15 m/s = 1 s.
        assert_eq!(pass_arrival_ms(&play, &play.events[1]), Some(2000));
        assert_eq!(sample(&play, 1900).puck.carrier, None);
        assert_eq!(sample(&play, 2000).puck.carrier, Some("b".into()));
    }

    #[test]
    fn interception_cancels_the_pass() {
        let mut play = two_skater_play();
        play.entities.push(skater("x", TeamSide::Away));
        play.set_keyframe(&"a".into(), Keyframe::at(0, 0.0, 0.0));
        play.set_keyframe(&"b".into(), Keyframe::at(0, 10.0, 0.0));
        play.set_keyframe(&"x".into(), Keyframe::at(0, 5.0, 0.5));
        play.events.push(ev(0, EventKind::PuckRecovery, &["a"]));
        play.events.push(Event { arrive_ms: Some(2000), ..ev(1000, EventKind::Pass, &["a", "b"]) });
        play.events.push(ev(1300, EventKind::PuckRecovery, &["x"]));
        assert_eq!(sample(&play, 2500).puck.carrier, Some("x".into()));
    }

    #[test]
    fn recovery_by_other_ends_possession() {
        let mut play = two_skater_play();
        play.set_keyframe(&"a".into(), Keyframe::at(0, 0.0, 0.0));
        play.set_keyframe(&"b".into(), Keyframe::at(0, 1.0, 0.0));
        play.events.push(ev(0, EventKind::PuckRecovery, &["a"]));
        play.events.push(ev(1000, EventKind::Turnover, &["a", "b"]));
        assert_eq!(sample(&play, 1500).puck.carrier, Some("b".into()));
        let p = possessions(&play);
        assert_eq!(p[0].end_ms, Some(1000));
        assert_eq!(p[1].start_ms, 1000);
    }

    #[test]
    fn shot_flies_to_target_and_stops() {
        let mut play = two_skater_play();
        play.set_keyframe(&"a".into(), Keyframe::at(0, 20.0, 0.0).facing(0.0));
        play.events.push(ev(0, EventKind::PuckRecovery, &["a"]));
        play.events.push(Event { to: Some(Vec2::new(26.0, 0.0)), ..ev(1000, EventKind::Shot, &["a"]) });
        let end = sample(&play, 5000);
        assert_eq!(end.puck.carrier, None);
        assert!(end.puck.pos.distance(Vec2::new(26.0, 0.0)) < 1e-3);
    }

    #[test]
    fn puck_keyframes_win_over_shot_target() {
        let mut play = two_skater_play();
        play.set_keyframe(&"a".into(), Keyframe::at(0, 20.0, 0.0).facing(0.0));
        play.events.push(ev(0, EventKind::PuckRecovery, &["a"]));
        play.events.push(Event { to: Some(Vec2::new(26.0, 0.0)), ..ev(1000, EventKind::Shot, &["a"]) });
        // Rebound off the pad into the corner.
        play.set_keyframe(&"puck".into(), Keyframe::at(1300, 25.0, 0.5));
        play.set_keyframe(&"puck".into(), Keyframe::at(2300, 26.0, 10.0));
        assert!(sample(&play, 1300).puck.pos.distance(Vec2::new(25.0, 0.5)) < 1e-3);
        assert!(sample(&play, 4000).puck.pos.distance(Vec2::new(26.0, 10.0)) < 1e-3);
    }

    #[test]
    fn loose_puck_follows_its_own_track() {
        let mut play = two_skater_play();
        play.set_keyframe(&"puck".into(), Keyframe::at(0, 0.0, 0.0));
        play.set_keyframe(&"puck".into(), Keyframe::at(1000, 4.0, 0.0));
        let s = sample(&play, 500);
        assert!((s.puck.pos.x - 2.0).abs() < 1e-3);
    }
}
