//! Starting points for new plays, so the user only edits what differs.
//!
//! Positions assume home attacks +x. Roles are from each team's own view:
//! home's left (LW, LD) is +y, and away, attacking -x, has its left on -y.

use std::f32::consts::PI;

use crate::model::{Entity, EntityId, EntityKind, Event, EventKind, Keyframe, Play, Role, TeamSide};
use crate::rink::RinkSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// Center-ice faceoff, everyone at t = 0.
    Faceoff,
    /// Static 5v5 with home defending its own zone.
    DZoneSet,
    /// A short animated D-to-wing breakout with a decision point for home LW.
    Breakout,
}

impl Template {
    pub const ALL: [Template; 3] = [Template::Faceoff, Template::DZoneSet, Template::Breakout];

    pub fn label(self) -> &'static str {
        match self {
            Template::Faceoff => "Center-ice faceoff",
            Template::DZoneSet => "5v5 D-zone set",
            Template::Breakout => "Breakout",
        }
    }
}

const ROLES: [(Role, u8); 6] =
    [(Role::LW, 17), (Role::C, 19), (Role::RW, 27), (Role::LD, 4), (Role::RD, 44), (Role::G, 30)];

pub fn entity_id(side: TeamSide, role: Role) -> EntityId {
    let team = match side {
        TeamSide::Home => "home",
        TeamSide::Away => "away",
    };
    EntityId(format!("{team}_{}", role.as_str()))
}

pub const PUCK_ID: &str = "puck";

/// Every side and role of a standard roster, in roster order.
pub fn standard_slots() -> impl Iterator<Item = (TeamSide, Role)> {
    [TeamSide::Home, TeamSide::Away].into_iter().flat_map(|side| ROLES.map(|(role, _)| (side, role)))
}

/// The player a template puts in `side`'s `role`, with its default number.
/// Nobody is "me".
pub fn standard_entity(side: TeamSide, role: Role) -> Entity {
    let number = ROLES.iter().find(|(r, _)| *r == role).map_or(0, |(_, n)| *n);
    Entity {
        id: entity_id(side, role),
        kind: if role == Role::G { EntityKind::Goalie } else { EntityKind::Skater },
        team: Some(side),
        number: Some(if side == TeamSide::Home { number } else { number + 1 }),
        role: Some(role),
        label: String::new(),
        is_me: false,
    }
}

/// 5 skaters + a goalie per side and a puck; home LW is "me".
pub fn standard_roster(play: &mut Play) {
    for (side, role) in standard_slots() {
        let mut e = standard_entity(side, role);
        e.is_me = side == TeamSide::Home && role == Role::LW;
        play.entities.push(e);
    }
    play.entities.push(Entity {
        id: PUCK_ID.into(),
        kind: EntityKind::Puck,
        team: None,
        number: None,
        role: None,
        label: String::new(),
        is_me: false,
    });
}

pub fn build(template: Template, rink: RinkSpec) -> Play {
    let mut play = Play::new(template.label(), rink);
    standard_roster(&mut play);
    match template {
        Template::Faceoff => faceoff(&mut play),
        Template::DZoneSet => dzone_set(&mut play),
        Template::Breakout => breakout(&mut play),
    }
    play.normalize();
    play
}

fn place(play: &mut Play, side: TeamSide, role: Role, kf: Keyframe) {
    play.set_keyframe(&entity_id(side, role), kf);
}

fn goalies(play: &mut Play) {
    let x = play.rink.goal_line_x() - 1.0;
    place(play, TeamSide::Home, Role::G, Keyframe::at(0, -x, 0.0).facing(0.0));
    place(play, TeamSide::Away, Role::G, Keyframe::at(0, x, 0.0).facing(PI));
}

fn faceoff(play: &mut Play) {
    use Role::*;
    use TeamSide::*;
    play.duration_ms = 5_000;
    goalies(play);
    // (role, x for home, y for home); away mirrors through center ice.
    let spots = [(C, -0.8, 0.0), (LW, -1.5, 5.0), (RW, -1.5, -5.0), (LD, -6.5, 4.0), (RD, -6.5, -4.0)];
    for (role, x, y) in spots {
        place(play, Home, role, Keyframe::at(0, x, y).facing(0.0));
        place(play, Away, role, Keyframe::at(0, -x, -y).facing(PI));
    }
    play.set_keyframe(&PUCK_ID.into(), Keyframe::at(0, 0.0, 0.0));
    play.events.push(Event::new(0, EventKind::Faceoff, &[]));
}

fn dzone_set(play: &mut Play) {
    use Role::*;
    use TeamSide::*;
    play.duration_ms = 5_000;
    goalies(play);
    let home = [
        (LD, -24.0, 2.5, 0.0),
        (RD, -24.0, -8.0, -0.6),
        (C, -20.0, -3.0, -0.3),
        (LW, -14.0, 7.0, PI),
        (RW, -15.0, -7.0, PI),
    ];
    for (role, x, y, f) in home {
        place(play, Home, role, Keyframe::at(0, x, y).facing(f));
    }
    let away = [
        (LW, -26.0, -10.0, 0.5),
        (C, -22.0, -6.0, PI),
        (RW, -24.0, 3.0, PI),
        (LD, -9.0, -7.0, PI),
        (RD, -9.0, 8.0, PI),
    ];
    for (role, x, y, f) in away {
        place(play, Away, role, Keyframe::at(0, x, y).facing(f));
    }
    play.events.push(Event::new(0, EventKind::PuckRecovery, &[&entity_id(Away, LW)]));
}

/// (t_ms, x, y)
type TimedSpot = (u32, f32, f32);

fn breakout(play: &mut Play) {
    use Role::*;
    use TeamSide::*;
    play.duration_ms = 6_000;
    play.notes = "Example: LD recovers behind the net and rims it to me on the strong-side wall.".into();
    goalies(play);

    let tracks: [(TeamSide, Role, &[TimedSpot]); 10] = [
        (Home, LD, &[(0, -27.5, 3.0), (1500, -27.0, 6.0), (3500, -24.0, 5.0)]),
        (
            Home,
            LW,
            &[(0, -18.0, 11.0), (2000, -19.0, 12.0), (3500, -12.0, 11.5), (5000, -5.0, 10.0), (6000, -1.0, 9.0)],
        ),
        (Home, C, &[(0, -22.0, 0.0), (2500, -16.0, 3.0), (5000, -6.0, 4.0), (6000, -2.0, 3.0)]),
        (Home, RW, &[(0, -18.0, -10.0), (3000, -12.0, -9.5), (5000, -6.0, -9.0), (6000, -2.0, -8.0)]),
        (Home, RD, &[(0, -24.0, -6.0), (2500, -20.0, -4.0), (5000, -12.0, -3.0)]),
        (Away, C, &[(0, -20.0, 2.0), (1500, -25.0, 5.0), (3000, -21.0, 9.0), (4500, -15.0, 9.0)]),
        (Away, LW, &[(0, -15.0, -6.0), (3000, -17.0, -3.0), (5000, -14.0, -2.0)]),
        (Away, RW, &[(0, -14.0, 8.0), (3000, -15.0, 11.0), (4000, -11.0, 10.5), (5000, -7.0, 9.5)]),
        (Away, RD, &[(0, -6.0, 7.0), (3500, -8.0, 9.0), (5500, -4.0, 8.0)]),
        (Away, LD, &[(0, -6.0, -5.0), (5000, -5.0, -4.0)]),
    ];
    for (side, role, points) in tracks {
        for &(t, x, y) in points {
            place(play, side, role, Keyframe::at(t, x, y));
        }
    }

    let ld = entity_id(Home, LD);
    let lw = entity_id(Home, LW);
    play.events = vec![
        Event::new(0, EventKind::PuckRecovery, &[&ld]),
        Event { arrive_ms: Some(2000), ..Event::new(1500, EventKind::Pass, &[&ld, &lw]) },
        Event {
            note: "Chip it past the pinching D or carry it out?".into(),
            ..Event::new(3000, EventKind::DecisionPoint, &[&lw])
        },
        Event::new(4600, EventKind::ZoneExit, &[&lw]),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::sample;

    #[test]
    fn roster_is_ten_skaters_two_goalies_and_a_puck() {
        let play = build(Template::Faceoff, RinkSpec::iihf());
        let count = |k: EntityKind| play.entities.iter().filter(|e| e.kind == k).count();
        assert_eq!(count(EntityKind::Skater), 10);
        assert_eq!(count(EntityKind::Goalie), 2);
        assert_eq!(count(EntityKind::Puck), 1);
        assert_eq!(play.me().unwrap().id, entity_id(TeamSide::Home, Role::LW));
    }

    #[test]
    fn roles_sit_on_their_own_teams_side() {
        // At t = 0 every left-side role is on its team's left: +y for home, -y for away.
        for t in Template::ALL {
            let play = build(t, RinkSpec::iihf());
            for e in play.entities.iter().filter(|e| matches!(e.role, Some(Role::LW | Role::LD))) {
                let y = play.track(&e.id)[0].pos.y;
                let left = if e.team == Some(TeamSide::Home) { y > 0.0 } else { y < 0.0 };
                assert!(left, "{t:?}: {} at y = {y}", e.id);
            }
        }
    }

    #[test]
    fn breakout_hands_puck_to_me() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        let me = play.me().unwrap().id.clone();
        assert_eq!(sample(&play, 1000).puck.carrier, Some(entity_id(TeamSide::Home, Role::LD)));
        assert_eq!(sample(&play, 1700).puck.carrier, None);
        assert_eq!(sample(&play, 3000).puck.carrier, Some(me.clone()));
        assert_eq!(sample(&play, 5000).puck.carrier, Some(me));
    }
}
