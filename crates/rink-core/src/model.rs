//! The stored play: entities, keyframes and events. Snapshots are derived from
//! this by [`crate::sample`] and are never saved.

use std::collections::BTreeMap;
use std::fmt;

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rink::RinkSpec;

pub const SCHEMA_VERSION: u32 = 1;

/// Human-readable entity id such as `home_LW` or `puck`. Readable ids keep the
/// files diffable and the MCP output easy for a model to follow.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(pub String);

impl EntityId {
    pub fn new(id: impl Into<String>) -> Self {
        EntityId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for EntityId {
    fn from(s: &str) -> Self {
        EntityId(s.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamSide {
    Home,
    Away,
}

impl TeamSide {
    pub fn index(self) -> usize {
        match self {
            TeamSide::Home => 0,
            TeamSide::Away => 1,
        }
    }

    pub fn opponent(self) -> TeamSide {
        match self {
            TeamSide::Home => TeamSide::Away,
            TeamSide::Away => TeamSide::Home,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    pub side: TeamSide,
    pub name: String,
    /// `#RRGGBB`.
    pub color: String,
    /// True when this team attacks toward +x. Home does by convention.
    pub attacks_positive_x: bool,
}

impl Team {
    pub fn attack_sign(&self) -> f32 {
        if self.attacks_positive_x {
            1.0
        } else {
            -1.0
        }
    }

    /// The color as 0xRRGGBB, falling back to mid grey on a malformed value.
    pub fn color_rgb(&self) -> u32 {
        u32::from_str_radix(self.color.trim_start_matches('#'), 16).unwrap_or(0x808080)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Skater,
    Goalie,
    Puck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    LW,
    C,
    RW,
    LD,
    RD,
    G,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::LW => "LW",
            Role::C => "C",
            Role::RW => "RW",
            Role::LD => "LD",
            Role::RD => "RD",
            Role::G => "G",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<TeamSide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub is_me: bool,
}

impl Entity {
    pub fn is_puck(&self) -> bool {
        self.kind == EntityKind::Puck
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub t_ms: u32,
    /// Meters, rink coordinates.
    pub pos: Vec2,
    /// Radians, 0 = +x. None means "face the direction of travel".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<f32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_puck: bool,
}

impl Keyframe {
    pub fn at(t_ms: u32, x: f32, y: f32) -> Self {
        Keyframe { t_ms, pos: Vec2::new(x, y), facing: None, has_puck: false }
    }

    pub fn facing(mut self, radians: f32) -> Self {
        self.facing = Some(radians);
        self
    }

    pub fn with_puck(mut self) -> Self {
        self.has_puck = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Pass,
    Shot,
    PuckRecovery,
    Turnover,
    ZoneEntry,
    ZoneExit,
    Hit,
    Faceoff,
    Whistle,
    /// "I chose here; was it right?" These are what the AI is asked to judge.
    DecisionPoint,
}

impl EventKind {
    pub const ALL: [EventKind; 10] = [
        EventKind::Pass,
        EventKind::Shot,
        EventKind::PuckRecovery,
        EventKind::Turnover,
        EventKind::ZoneEntry,
        EventKind::ZoneExit,
        EventKind::Hit,
        EventKind::Faceoff,
        EventKind::Whistle,
        EventKind::DecisionPoint,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EventKind::Pass => "Pass",
            EventKind::Shot => "Shot",
            EventKind::PuckRecovery => "Puck recovery",
            EventKind::Turnover => "Turnover",
            EventKind::ZoneEntry => "Zone entry",
            EventKind::ZoneExit => "Zone exit",
            EventKind::Hit => "Hit",
            EventKind::Faceoff => "Faceoff",
            EventKind::Whistle => "Whistle",
            EventKind::DecisionPoint => "Decision point",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub t_ms: u32,
    pub kind: EventKind,
    #[serde(default)]
    pub actors: Vec<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Vec2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec2>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Play {
    pub schema_version: u32,
    pub id: Uuid,
    pub title: String,
    pub rink: RinkSpec,
    pub duration_ms: u32,
    /// Home first, then away.
    pub teams: [Team; 2],
    pub entities: Vec<Entity>,
    /// Keyed by entity id; a BTreeMap so saved files have a stable order.
    #[serde(default)]
    pub tracks: BTreeMap<EntityId, Vec<Keyframe>>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub notes: String,
}

impl Play {
    pub fn new(title: impl Into<String>, rink: RinkSpec) -> Self {
        Play {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            title: title.into(),
            rink,
            duration_ms: 10_000,
            teams: [
                Team { side: TeamSide::Home, name: "Home".into(), color: "#1f5fbf".into(), attacks_positive_x: true },
                Team { side: TeamSide::Away, name: "Away".into(), color: "#c8322d".into(), attacks_positive_x: false },
            ],
            entities: Vec::new(),
            tracks: BTreeMap::new(),
            events: Vec::new(),
            notes: String::new(),
        }
    }

    pub fn team(&self, side: TeamSide) -> &Team {
        &self.teams[side.index()]
    }

    pub fn entity(&self, id: &EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| &e.id == id)
    }

    pub fn me(&self) -> Option<&Entity> {
        self.entities.iter().find(|e| e.is_me)
    }

    pub fn puck(&self) -> Option<&Entity> {
        self.entities.iter().find(|e| e.is_puck())
    }

    pub fn track(&self, id: &EntityId) -> &[Keyframe] {
        self.tracks.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Insert or replace the keyframe at `kf.t_ms`, keeping the track sorted.
    pub fn set_keyframe(&mut self, id: &EntityId, kf: Keyframe) {
        let track = self.tracks.entry(id.clone()).or_default();
        match track.binary_search_by_key(&kf.t_ms, |k| k.t_ms) {
            Ok(i) => track[i] = kf,
            Err(i) => track.insert(i, kf),
        }
    }

    /// Remove the keyframe at exactly `t_ms`. Returns it if there was one.
    pub fn remove_keyframe(&mut self, id: &EntityId, t_ms: u32) -> Option<Keyframe> {
        let track = self.tracks.get_mut(id)?;
        let i = track.binary_search_by_key(&t_ms, |k| k.t_ms).ok()?;
        Some(track.remove(i))
    }

    /// Sort keyframes and events by time. Loading and editing call this so the
    /// rest of the code can assume sorted input.
    pub fn normalize(&mut self) {
        for track in self.tracks.values_mut() {
            track.sort_by_key(|k| k.t_ms);
            track.dedup_by_key(|k| k.t_ms);
        }
        self.events.sort_by_key(|e| e.t_ms);
    }

    /// All keyframe and event times, sorted and deduplicated.
    pub fn key_times(&self) -> Vec<u32> {
        let mut times: Vec<u32> =
            self.tracks.values().flatten().map(|k| k.t_ms).chain(self.events.iter().map(|e| e.t_ms)).collect();
        times.sort_unstable();
        times.dedup();
        times
    }
}
