//! Skating orders: "skate there", like a right-click in a strategy game. A
//! small physics model (top speed, push-off, braking, and a cap on sideways
//! acceleration, which makes the turning radius grow with speed) skates the
//! order from the state the player is in at that moment, and the path is
//! baked into ordinary keyframes that carry velocities. Playback and every
//! other reader keep sampling keyframes; only giving an order simulates.

use std::f32::consts::PI;

use glam::Vec2;

use crate::model::{EntityId, EntityKind, Keyframe, Order, Pace, Play};
use crate::rink::RinkSpec;
use crate::sample::{facing_at, position_at, velocity_at};
use crate::spline::{hermite, hermite_velocity, lerp_angle, wrap_angle};

/// Simulation step.
const STEP_MS: u32 = 10;
/// An order that hasn't arrived by then is cut off.
const MAX_ORDER_MS: u32 = 30_000;
/// Close enough to a waypoint on the way to the next one.
const WAYPOINT_RADIUS_M: f32 = 1.5;
/// Close enough to the last one to stop.
const ARRIVE_RADIUS_M: f32 = 0.15;
/// Below this speed a skater can push off in any direction.
const PIVOT_SPEED_MPS: f32 = 1.0;
/// Speed a skater slows to before turning back toward something behind them.
const STOP_TURN_SPEED_MPS: f32 = 1.0;
/// Below this speed the body keeps its facing instead of following travel.
const FACE_TRAVEL_MPS: f32 = 0.3;
/// How far inside the boards skaters stay, as when dragging.
const BOARDS_CLEARANCE_M: f32 = 0.5;
/// How closely baked keyframes must reproduce the simulated path.
const BAKE_POS_M: f32 = 0.04;
const BAKE_VEL_MPS: f32 = 0.25;
const BAKE_FACING_RAD: f32 = 0.06;
/// Plays grow to fit an order, to the next whole second.
const DURATION_ROUND_MS: u32 = 1000;

/// What a skater can do, for one kind of skating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub top_speed: f32,
    /// m/s², pushing forward.
    pub accel: f32,
    /// m/s², stopping.
    pub brake: f32,
    /// m/s² sideways; the turning radius at speed v is v² / lateral.
    pub lateral: f32,
    /// rad/s the body can turn, e.g. pivoting to skate backwards.
    pub turn_rate: f32,
}

impl Limits {
    pub fn for_order(kind: EntityKind, order: &Order) -> Limits {
        let top_speed = match order.pace {
            Pace::Easy => 4.0,
            Pace::Skate => 6.5,
            // Just under review::MAX_SKATE_SPEED_MPS, so baked plays validate.
            Pace::Sprint => 7.8,
        };
        let accel = if order.pace == Pace::Sprint { 4.5 } else { 3.5 };
        let mut limits = Limits { top_speed, accel, brake: 7.0, lateral: 6.0, turn_rate: 7.0 };
        if order.backward {
            limits.top_speed *= 0.65;
            limits.accel *= 0.7;
            limits.lateral *= 0.8;
        }
        if kind == EntityKind::Goalie {
            limits.top_speed = limits.top_speed.min(3.0);
            limits.accel = limits.accel.min(3.0);
        }
        limits
    }
}

/// A skater's state at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Skater {
    pub pos: Vec2,
    /// m/s.
    pub vel: Vec2,
    /// Radians, 0 = +x.
    pub facing: f32,
}

/// Where `id` is, how they move and which way they face at `t_ms`, as
/// playback shows it. None if they aren't on the ice.
pub fn state_at(play: &Play, id: &EntityId, t_ms: u32) -> Option<Skater> {
    let entity = play.entity(id)?;
    let track = play.track(id);
    Some(Skater {
        pos: position_at(track, t_ms)?,
        vel: velocity_at(track, t_ms),
        facing: facing_at(play, entity, t_ms),
    })
}

/// Skate `order` from `start`. Returns the state every [`STEP_MS`] from the
/// order's moment (offset 0 is `start`) until they stop at the last waypoint.
pub fn simulate(rink: &RinkSpec, limits: Limits, start: Skater, order: &Order) -> Vec<(u32, Skater)> {
    let dt = STEP_MS as f32 / 1000.0;
    let mut s = start;
    let mut out = vec![(0, s)];
    let mut next = 0;
    let mut t = 0;
    while t < MAX_ORDER_MS && next < order.to.len() {
        let target = order.to[next];
        let to_target = target - s.pos;
        let dist = to_target.length();
        let last = next + 1 == order.to.len();
        if !last && dist < WAYPOINT_RADIUS_M {
            next += 1;
            continue;
        }
        if last && dist < ARRIVE_RADIUS_M {
            s.vel = Vec2::ZERO;
            t += STEP_MS;
            out.push((t, s));
            break;
        }
        let left = remaining(s.pos, &order.to[next..]);
        step(rink, &limits, &mut s, to_target, left, order.backward, dt);
        t += STEP_MS;
        out.push((t, s));
    }
    if let Some(end) = out.last_mut() {
        end.1.vel = Vec2::ZERO;
    }
    out
}

/// Distance left to skate from `pos` through `waypoints`.
fn remaining(pos: Vec2, waypoints: &[Vec2]) -> f32 {
    let mut d = 0.0;
    let mut from = pos;
    for &w in waypoints {
        d += from.distance(w);
        from = w;
    }
    d
}

fn step(rink: &RinkSpec, l: &Limits, s: &mut Skater, to_target: Vec2, remaining: f32, backward: bool, dt: f32) {
    let dist = to_target.length();
    let dir = to_target / dist;
    let speed = s.vel.length();

    // As fast as allowed, but slow enough to stop in time at the end (with
    // half the braking, so the stop is firm rather than a slam)...
    let mut want = l.top_speed.min((l.brake * remaining).sqrt());
    if speed > PIVOT_SPEED_MPS {
        let heading = s.vel / speed;
        let (cos, sin) = (heading.dot(dir), heading.perp_dot(dir).abs());
        if cos < -0.2 {
            // ...stop and push off again for something behind...
            want = want.min(STOP_TURN_SPEED_MPS);
        } else if sin > 1e-3 {
            // ...or slow enough that the turning circle through the target fits.
            let radius = dist / (2.0 * sin);
            want = want.min((l.lateral * radius).sqrt().max(STOP_TURN_SPEED_MPS));
        }
    }

    let mut accel = (dir * want - s.vel) / dt;
    if speed > PIVOT_SPEED_MPS {
        let heading = s.vel / speed;
        let side = heading.perp();
        let along = accel.dot(heading).clamp(-l.brake, l.accel);
        let across = accel.dot(side).clamp(-l.lateral, l.lateral);
        accel = heading * along + side * across;
    } else {
        accel = accel.clamp_length_max(l.brake);
    }
    s.vel += accel * dt;
    s.pos += s.vel * dt;

    // The boards stop whatever part of the motion goes into them.
    let inside = rink.distance_inside_boards(s.pos);
    if inside < BOARDS_CLEARANCE_M {
        let normal = inward_normal(rink, s.pos);
        s.pos += normal * (BOARDS_CLEARANCE_M - inside);
        let into = s.vel.dot(normal);
        if into < 0.0 {
            s.vel -= normal * into;
        }
    }

    let speed = s.vel.length();
    if speed > FACE_TRAVEL_MPS {
        let travel = s.vel.y.atan2(s.vel.x);
        let want_facing = if backward { wrap_angle(travel + PI) } else { travel };
        let turn = wrap_angle(want_facing - s.facing);
        let max_turn = l.turn_rate * dt;
        s.facing = wrap_angle(s.facing + turn.clamp(-max_turn, max_turn));
    }
}

/// Direction from the boards toward the middle of the ice at `p`.
fn inward_normal(rink: &RinkSpec, p: Vec2) -> Vec2 {
    let h = 0.01;
    let g = Vec2::new(
        rink.distance_inside_boards(p + Vec2::X * h) - rink.distance_inside_boards(p - Vec2::X * h),
        rink.distance_inside_boards(p + Vec2::Y * h) - rink.distance_inside_boards(p - Vec2::Y * h),
    );
    g.try_normalize().unwrap_or_else(|| (-p).normalize_or_zero())
}

/// The fewest keyframes (with velocity and facing) that reproduce `samples`,
/// with their offsets moved to start at `t0_ms`.
pub fn bake(samples: &[(u32, Skater)], t0_ms: u32) -> Vec<Keyframe> {
    let key =
        |&(t, s): &(u32, Skater)| Keyframe { t_ms: t0_ms + t, pos: s.pos, facing: Some(s.facing), vel: Some(s.vel) };
    let Some(first) = samples.first() else { return Vec::new() };
    let mut keys = vec![key(first)];
    let mut a = 0;
    while a + 1 < samples.len() {
        let mut b = a + 1;
        while b + 1 < samples.len() && reproduces(&samples[a..=b + 1]) {
            b += 1;
        }
        keys.push(key(&samples[b]));
        a = b;
    }
    keys
}

/// True when one keyframe at each end of `run` reproduces everything between.
fn reproduces(run: &[(u32, Skater)]) -> bool {
    let (ta, a) = run[0];
    let (tb, b) = run[run.len() - 1];
    let dt = (tb - ta) as f32 / 1000.0;
    run[1..run.len() - 1].iter().all(|&(t, s)| {
        let u = (t - ta) as f32 / (tb - ta) as f32;
        hermite(a.pos, a.vel, b.pos, b.vel, dt, u).distance(s.pos) <= BAKE_POS_M
            && hermite_velocity(a.pos, a.vel, b.pos, b.vel, dt, u).distance(s.vel) <= BAKE_VEL_MPS
            && wrap_angle(lerp_angle(a.facing, b.facing, u) - s.facing).abs() <= BAKE_FACING_RAD
    })
}

/// Give `id` an order. From `order.t_ms` on, it replaces their keyframes and
/// any later orders: they skate it from the state they're in then. The play
/// grows if they're still skating at its end. Returns when they stop, or
/// None (and changes nothing) for the puck or someone not on the ice.
pub fn apply_order(play: &mut Play, id: &EntityId, mut order: Order) -> Option<u32> {
    let entity = play.entity(id)?;
    if entity.is_puck() || order.to.is_empty() {
        return None;
    }
    // A spot past the boards can't be reached; skate to the nearest one that can.
    for w in &mut order.to {
        *w = play.rink.clamp_inside(*w, BOARDS_CLEARANCE_M);
    }
    let limits = Limits::for_order(entity.kind, &order);
    let start = state_at(play, id, order.t_ms)?;
    let samples = simulate(&play.rink, limits, start, &order);
    let keys = bake(&samples, order.t_ms);
    let end_ms = keys.last().map_or(order.t_ms, |k| k.t_ms);

    let track = play.tracks.entry(id.clone()).or_default();
    track.retain(|k| k.t_ms < order.t_ms);
    track.extend(keys);
    let orders = play.orders.entry(id.clone()).or_default();
    orders.retain(|o| o.t_ms < order.t_ms);
    orders.push(order);
    if end_ms > play.duration_ms {
        play.duration_ms = end_ms.div_ceil(DURATION_ROUND_MS) * DURATION_ROUND_MS;
    }
    Some(end_ms)
}

/// When the order `id` follows at `t_ms` is done: when the next order
/// takes over, or when they stop.
pub fn order_end_ms(play: &Play, id: &EntityId, order: &Order) -> u32 {
    let next = play.orders(id).iter().find(|o| o.t_ms > order.t_ms).map(|o| o.t_ms);
    next.unwrap_or_else(|| play.track(id).last().map_or(order.t_ms, |k| k.t_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entity, Role, TeamSide};
    use crate::sample::{sample, speed_at};
    use crate::validate::validate;

    fn play_with_skater(at: Vec2, facing: f32) -> (Play, EntityId) {
        let mut play = Play::new("orders", RinkSpec::iihf());
        let id: EntityId = "home_LW".into();
        play.entities.push(Entity {
            id: id.clone(),
            kind: EntityKind::Skater,
            team: Some(TeamSide::Home),
            number: Some(17),
            role: Some(Role::LW),
            label: String::new(),
            is_me: true,
        });
        play.set_keyframe(&id, Keyframe::at(0, at.x, at.y).facing(facing));
        (play, id)
    }

    #[test]
    fn skates_to_the_target_and_stops() {
        let (mut play, id) = play_with_skater(Vec2::new(-10.0, 0.0), 0.0);
        let end = apply_order(&mut play, &id, Order::new(0, Vec2::new(10.0, 5.0))).unwrap();
        let track = play.track(&id);
        assert!(track.last().unwrap().pos.distance(Vec2::new(10.0, 5.0)) < ARRIVE_RADIUS_M + 0.01);
        // ~21 m from a standstill at up to 6.5 m/s takes a few seconds.
        assert!((3000..8000).contains(&end), "arrived at {end} ms");
        assert!(speed_at(track, end / 2) > 4.0);
        assert_eq!(speed_at(track, end + 500), 0.0);
        // Far fewer keyframes than simulation steps.
        assert!(track.len() < 25, "{} keyframes", track.len());
        assert_eq!(validate(&play), vec![]);
    }

    #[test]
    fn keyframes_reproduce_the_simulation() {
        let (play, id) = play_with_skater(Vec2::new(-10.0, 0.0), 0.0);
        let order = Order { to: vec![Vec2::new(5.0, 10.0), Vec2::new(15.0, -5.0)], ..Order::new(0, Vec2::ZERO) };
        let start = state_at(&play, &id, 0).unwrap();
        let samples = simulate(&play.rink, Limits::for_order(EntityKind::Skater, &order), start, &order);
        let mut baked = play.clone();
        apply_order(&mut baked, &id, order).unwrap();
        let track = baked.track(&id);
        for &(t, s) in &samples {
            let pos = position_at(track, t).unwrap();
            assert!(pos.distance(s.pos) < BAKE_POS_M + 1e-3, "at {t} ms: {pos} vs {}", s.pos);
        }
    }

    #[test]
    fn faster_means_wider_turns() {
        // Flying along +x at 7 m/s, then told to go to a spot up and behind:
        // the turn swings wider than the same order given at 2 m/s.
        let widest = |speed: f32| {
            let (mut play, id) = play_with_skater(Vec2::new(-20.0, -8.0), 0.0);
            play.tracks.get_mut(&id).unwrap()[0].vel = Some(Vec2::new(speed, 0.0));
            apply_order(&mut play, &id, Order::new(0, Vec2::new(-20.0, 0.0))).unwrap();
            play.track(&id).iter().map(|k| k.pos.x).fold(f32::MIN, f32::max)
        };
        let (fast, slow) = (widest(7.0), widest(2.0));
        assert!(fast > slow + 2.0, "fast swings to x={fast}, slow to x={slow}");
    }

    #[test]
    fn new_order_continues_from_the_current_motion() {
        let (mut play, id) = play_with_skater(Vec2::new(-20.0, 0.0), 0.0);
        apply_order(&mut play, &id, Order::new(0, Vec2::new(20.0, 0.0))).unwrap();
        let before = state_at(&play, &id, 2000).unwrap();
        assert!(before.vel.x > 4.0);
        apply_order(&mut play, &id, Order::new(2000, Vec2::new(before.pos.x, 10.0))).unwrap();
        let after = state_at(&play, &id, 2000).unwrap();
        assert!(after.pos.distance(before.pos) < 1e-3);
        assert!(after.vel.distance(before.vel) < 1e-3);
        // Still carried forward a little after the new order.
        let later = position_at(play.track(&id), 2300).unwrap();
        assert!(later.x > before.pos.x + 0.5);
        assert_eq!(play.orders(&id).len(), 2);

        // An order before the second one replaces it.
        apply_order(&mut play, &id, Order::new(1000, Vec2::new(-20.0, 0.0))).unwrap();
        assert_eq!(play.orders(&id).iter().map(|o| o.t_ms).collect::<Vec<_>>(), vec![0, 1000]);
    }

    #[test]
    fn backward_skating_faces_away_from_travel() {
        let (mut play, id) = play_with_skater(Vec2::new(10.0, 0.0), 0.0);
        let order = Order { backward: true, ..Order::new(0, Vec2::new(-10.0, 0.0)) };
        let end = apply_order(&mut play, &id, order).unwrap();
        let entity = play.entity(&id).unwrap().clone();
        let mid = end / 2;
        assert!(velocity_at(play.track(&id), mid).x < -2.0);
        assert!(facing_at(&play, &entity, mid).abs() < 0.2, "faces +x while going -x");
    }

    #[test]
    fn stays_inside_the_boards() {
        let (mut play, id) = play_with_skater(Vec2::new(20.0, 10.0), 0.0);
        play.tracks.get_mut(&id).unwrap()[0].vel = Some(Vec2::new(7.0, 0.0));
        apply_order(&mut play, &id, Order::new(0, Vec2::new(20.0, -10.0))).unwrap();
        for t in (0..play.duration_ms).step_by(50) {
            let p = sample(&play, t).entity(&id).unwrap().pos;
            assert!(play.rink.contains(p), "off the ice at {t} ms: {p}");
        }
    }

    #[test]
    fn spots_past_the_boards_are_pulled_onto_the_ice() {
        let (mut play, id) = play_with_skater(Vec2::new(-10.0, 0.0), 0.0);
        let order = Order { to: vec![Vec2::new(5.0, 20.0), Vec2::new(10.0, 0.0)], ..Order::new(0, Vec2::ZERO) };
        let end = apply_order(&mut play, &id, order).unwrap();
        assert!(end < 10_000, "doesn't grind along the boards until the cut-off: {end} ms");
        assert!(play.orders(&id)[0].to.iter().all(|&w| play.rink.contains(w)));
    }

    #[test]
    fn play_grows_to_fit_a_late_order() {
        let (mut play, id) = play_with_skater(Vec2::new(-25.0, 0.0), 0.0);
        let end = apply_order(&mut play, &id, Order::new(9000, Vec2::new(25.0, 0.0))).unwrap();
        assert!(end > 10_000);
        assert!(play.duration_ms >= end && play.duration_ms % 1000 == 0);
    }

    #[test]
    fn puck_takes_no_orders() {
        let (mut play, _) = play_with_skater(Vec2::ZERO, 0.0);
        play.entities.push(Entity {
            id: "puck".into(),
            kind: EntityKind::Puck,
            team: None,
            number: None,
            role: None,
            label: String::new(),
            is_me: false,
        });
        assert_eq!(apply_order(&mut play, &"puck".into(), Order::new(0, Vec2::X)), None);
    }

    #[test]
    fn orders_survive_a_save() {
        let (mut play, id) = play_with_skater(Vec2::new(-10.0, 0.0), 0.0);
        apply_order(&mut play, &id, Order { pace: Pace::Sprint, ..Order::new(0, Vec2::new(10.0, 0.0)) }).unwrap();
        let back = crate::io::from_json(&crate::io::to_json(&play)).unwrap();
        assert_eq!(back, play);
    }
}
