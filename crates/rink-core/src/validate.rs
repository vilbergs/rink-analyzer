//! Checks a play before it is saved: the editor shows these as warnings, and
//! `rink check` reports them so an AI-drafted play gets fixed, not trusted.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::model::{EntityId, EntityKind, EventKind, Play};
use crate::review::MAX_SKATE_SPEED_MPS;
use crate::sample::possessions;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Issue {
    DuplicateEntityId {
        entity: EntityId,
    },
    UnknownEntityInTrack {
        entity: EntityId,
    },
    UnknownEventActor {
        t_ms: u32,
        actor: EntityId,
    },
    KeyframesNotSorted {
        entity: EntityId,
    },
    KeyframeAfterEnd {
        entity: EntityId,
        t_ms: u32,
    },
    EventAfterEnd {
        t_ms: u32,
    },
    OffIce {
        entity: EntityId,
        t_ms: u32,
        x: f32,
        y: f32,
    },
    TooFast {
        entity: EntityId,
        from_ms: u32,
        to_ms: u32,
        mps: f32,
    },
    MissingActor {
        t_ms: u32,
        event: EventKind,
    },
    PuckAsActor {
        t_ms: u32,
    },
    /// A pass or shot by someone who doesn't have the puck at that moment.
    ReleaseWithoutPuck {
        t_ms: u32,
        actor: EntityId,
        carrier: Option<EntityId>,
    },
    ArrivalBeforeRelease {
        t_ms: u32,
        arrive_ms: u32,
    },
    /// A puck keyframe while someone holds the puck; the puck follows the carrier instead.
    PuckKeyframeIgnored {
        t_ms: u32,
        carrier: EntityId,
    },
    MultipleMe {
        entities: Vec<EntityId>,
    },
    SkaterWithoutTeam {
        entity: EntityId,
    },
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Issue::DuplicateEntityId { entity } => write!(f, "entity id {entity} is used twice"),
            Issue::UnknownEntityInTrack { entity } => write!(f, "track for unknown entity {entity}"),
            Issue::UnknownEventActor { t_ms, actor } => write!(f, "event at {t_ms} ms names unknown actor {actor}"),
            Issue::KeyframesNotSorted { entity } => write!(f, "{entity}: keyframes are not sorted by time"),
            Issue::KeyframeAfterEnd { entity, t_ms } => {
                write!(f, "{entity}: keyframe at {t_ms} ms is after the play ends")
            }
            Issue::EventAfterEnd { t_ms } => write!(f, "event at {t_ms} ms is after the play ends"),
            Issue::OffIce { entity, t_ms, x, y } => write!(f, "{entity} is off the ice at {t_ms} ms ({x:.1}, {y:.1})"),
            Issue::TooFast { entity, from_ms, to_ms, mps } => write!(
                f,
                "{entity} skates {mps:.1} m/s between {from_ms} and {to_ms} ms (max {MAX_SKATE_SPEED_MPS} m/s)"
            ),
            Issue::MissingActor { t_ms, event } => {
                write!(f, "{} at {t_ms} ms needs an actor (who does it)", event.label())
            }
            Issue::PuckAsActor { t_ms } => write!(f, "event at {t_ms} ms lists the puck as an actor"),
            Issue::ReleaseWithoutPuck { t_ms, actor, carrier } => match carrier {
                Some(c) => write!(f, "{actor} passes or shoots at {t_ms} ms, but {c} has the puck then"),
                None => write!(
                    f,
                    "{actor} passes or shoots at {t_ms} ms without the puck; add a puck_recovery or a pass to them first"
                ),
            },
            Issue::PuckKeyframeIgnored { t_ms, carrier } => write!(
                f,
                "puck keyframe at {t_ms} ms is ignored because {carrier} has the puck then; \
                 for a save or rebound use a save event, not a recovery"
            ),
            Issue::ArrivalBeforeRelease { t_ms, arrive_ms } => {
                write!(f, "pass at {t_ms} ms arrives at {arrive_ms} ms, before it is made")
            }
            Issue::MultipleMe { entities } => {
                let names: Vec<&str> = entities.iter().map(EntityId::as_str).collect();
                write!(f, "more than one entity is marked as me: {}", names.join(", "))
            }
            Issue::SkaterWithoutTeam { entity } => write!(f, "{entity} has no team"),
        }
    }
}

/// All problems found in `play`. Empty means it is safe to save as-is.
pub fn validate(play: &Play) -> Vec<Issue> {
    let mut issues = Vec::new();

    let mut seen = HashSet::new();
    for e in &play.entities {
        if !seen.insert(&e.id) {
            issues.push(Issue::DuplicateEntityId { entity: e.id.clone() });
        }
        if e.kind != EntityKind::Puck && e.team.is_none() {
            issues.push(Issue::SkaterWithoutTeam { entity: e.id.clone() });
        }
    }

    let me: Vec<EntityId> = play.entities.iter().filter(|e| e.is_me).map(|e| e.id.clone()).collect();
    if me.len() > 1 {
        issues.push(Issue::MultipleMe { entities: me });
    }

    for (id, track) in &play.tracks {
        let Some(entity) = play.entity(id) else {
            issues.push(Issue::UnknownEntityInTrack { entity: id.clone() });
            continue;
        };
        if track.windows(2).any(|w| w[0].t_ms >= w[1].t_ms) {
            issues.push(Issue::KeyframesNotSorted { entity: id.clone() });
        } else if !entity.is_puck() {
            // Straight-line speed between keyframes; the real path is a bit longer.
            for w in track.windows(2) {
                let mps = w[0].pos.distance(w[1].pos) / ((w[1].t_ms - w[0].t_ms) as f32 / 1000.0);
                if mps > MAX_SKATE_SPEED_MPS {
                    issues.push(Issue::TooFast { entity: id.clone(), from_ms: w[0].t_ms, to_ms: w[1].t_ms, mps });
                }
            }
        }
        for k in track {
            if k.t_ms > play.duration_ms {
                issues.push(Issue::KeyframeAfterEnd { entity: id.clone(), t_ms: k.t_ms });
            }
            if !play.rink.contains(k.pos) {
                issues.push(Issue::OffIce { entity: id.clone(), t_ms: k.t_ms, x: k.pos.x, y: k.pos.y });
            }
        }
    }

    let ps = possessions(play);
    // Who has the puck just before `t` (a release at `t` ends possession at `t`).
    let carrier_before =
        |t: u32| ps.iter().find(|p| p.start_ms < t && p.end_ms.is_none_or(|e| e >= t)).map(|p| p.entity.clone());
    if let Some(puck) = play.puck() {
        for k in play.track(&puck.id) {
            let holder = ps.iter().find(|p| p.start_ms <= k.t_ms && p.end_ms.is_none_or(|e| k.t_ms < e));
            if let Some(p) = holder {
                issues.push(Issue::PuckKeyframeIgnored { t_ms: k.t_ms, carrier: p.entity.clone() });
            }
        }
    }

    for ev in &play.events {
        if ev.t_ms > play.duration_ms {
            issues.push(Issue::EventAfterEnd { t_ms: ev.t_ms });
        }
        for a in &ev.actors {
            match play.entity(a) {
                None => issues.push(Issue::UnknownEventActor { t_ms: ev.t_ms, actor: a.clone() }),
                Some(e) if e.is_puck() => issues.push(Issue::PuckAsActor { t_ms: ev.t_ms }),
                _ => {}
            }
        }
        let needs_actor = matches!(ev.kind, EventKind::Pass | EventKind::Shot | EventKind::PuckRecovery);
        if needs_actor && ev.actors.is_empty() {
            issues.push(Issue::MissingActor { t_ms: ev.t_ms, event: ev.kind });
        }
        if matches!(ev.kind, EventKind::Pass | EventKind::Shot) {
            if let Some(actor) = ev.actors.first() {
                let carrier = carrier_before(ev.t_ms);
                if carrier.as_ref() != Some(actor) {
                    issues.push(Issue::ReleaseWithoutPuck { t_ms: ev.t_ms, actor: actor.clone(), carrier });
                }
            }
        }
        if let Some(arrive_ms) = ev.arrive_ms.filter(|&a| a < ev.t_ms) {
            issues.push(Issue::ArrivalBeforeRelease { t_ms: ev.t_ms, arrive_ms });
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Keyframe;
    use crate::templates;

    #[test]
    fn templates_are_valid() {
        for t in templates::Template::ALL {
            let play = templates::build(t, crate::rink::RinkSpec::iihf());
            assert_eq!(validate(&play), vec![], "template {t:?}");
        }
    }

    #[test]
    fn flags_off_ice_speed_and_bad_events() {
        use crate::model::Event;
        let mut play = templates::build(templates::Template::Faceoff, crate::rink::RinkSpec::iihf());
        let ids: Vec<EntityId> = play.entities.iter().filter(|e| !e.is_puck()).map(|e| e.id.clone()).collect();
        play.set_keyframe(&ids[0], Keyframe::at(500, 40.0, 0.0));
        play.events.push(Event::new(1000, EventKind::Pass, &[&ids[1], &ids[2]]));
        play.events.push(Event::new(1200, EventKind::PuckRecovery, &[]));
        play.events.push(Event { arrive_ms: Some(1000), ..Event::new(1500, EventKind::Pass, &[&ids[2], &ids[1]]) });
        play.normalize();
        let issues = validate(&play);
        assert!(issues.iter().any(|i| matches!(i, Issue::OffIce { .. })));
        assert!(issues.iter().any(|i| matches!(i, Issue::TooFast { .. })));
        assert!(issues.iter().any(|i| matches!(i, Issue::ReleaseWithoutPuck { t_ms: 1000, carrier: None, .. })));
        assert!(issues.iter().any(|i| matches!(i, Issue::MissingActor { t_ms: 1200, .. })));
        assert!(issues.iter().any(|i| matches!(i, Issue::ArrivalBeforeRelease { t_ms: 1500, .. })));
    }
}
