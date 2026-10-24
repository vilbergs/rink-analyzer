//! Checks a play before it is saved: the editor shows these as warnings, and
//! `create_play` returns them so an AI-drafted play gets fixed, not trusted.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::model::{EntityId, EntityKind, Play};
use crate::sample::possessions;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Issue {
    DuplicateEntityId { entity: EntityId },
    UnknownEntityInTrack { entity: EntityId },
    UnknownEventActor { t_ms: u32, actor: EntityId },
    KeyframesNotSorted { entity: EntityId },
    KeyframeAfterEnd { entity: EntityId, t_ms: u32 },
    EventAfterEnd { t_ms: u32 },
    OffIce { entity: EntityId, t_ms: u32, x: f32, y: f32 },
    PuckHasPossession { t_ms: u32 },
    MultipleCarriers { t_ms: u32, entities: Vec<EntityId> },
    MultipleMe { entities: Vec<EntityId> },
    SkaterWithoutTeam { entity: EntityId },
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
            Issue::PuckHasPossession { t_ms } => write!(f, "the puck's own keyframe at {t_ms} ms has has_puck set"),
            Issue::MultipleCarriers { t_ms, entities } => {
                let names: Vec<&str> = entities.iter().map(EntityId::as_str).collect();
                write!(f, "{} all pick up the puck at {t_ms} ms", names.join(", "))
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
        }
        for k in track {
            if k.t_ms > play.duration_ms {
                issues.push(Issue::KeyframeAfterEnd { entity: id.clone(), t_ms: k.t_ms });
            }
            if !play.rink.contains(k.pos) {
                issues.push(Issue::OffIce { entity: id.clone(), t_ms: k.t_ms, x: k.pos.x, y: k.pos.y });
            }
            if entity.is_puck() && k.has_puck {
                issues.push(Issue::PuckHasPossession { t_ms: k.t_ms });
            }
        }
    }

    for ev in &play.events {
        if ev.t_ms > play.duration_ms {
            issues.push(Issue::EventAfterEnd { t_ms: ev.t_ms });
        }
        for a in &ev.actors {
            if play.entity(a).is_none() {
                issues.push(Issue::UnknownEventActor { t_ms: ev.t_ms, actor: a.clone() });
            }
        }
    }

    // Two entities starting possession at the same instant is ambiguous.
    let ps = possessions(play);
    let mut i = 0;
    while i < ps.len() {
        let same: Vec<EntityId> =
            ps[i..].iter().take_while(|p| p.start_ms == ps[i].start_ms).map(|p| p.entity.clone()).collect();
        if same.len() > 1 {
            issues.push(Issue::MultipleCarriers { t_ms: ps[i].start_ms, entities: same.clone() });
        }
        i += same.len();
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
    fn flags_off_ice_and_double_pickup() {
        let mut play = templates::build(templates::Template::Faceoff, crate::rink::RinkSpec::iihf());
        let ids: Vec<EntityId> = play.entities.iter().filter(|e| !e.is_puck()).map(|e| e.id.clone()).collect();
        play.set_keyframe(&ids[0], Keyframe::at(500, 40.0, 0.0));
        play.set_keyframe(&ids[1], Keyframe::at(1000, 0.0, 0.0).with_puck());
        play.set_keyframe(&ids[2], Keyframe::at(1000, 0.0, 1.0).with_puck());
        let issues = validate(&play);
        assert!(issues.iter().any(|i| matches!(i, Issue::OffIce { .. })));
        assert!(issues.iter().any(|i| matches!(i, Issue::MultipleCarriers { t_ms: 1000, .. })));
    }
}
