//! The stored play: entities, keyframes and events. Snapshots are derived from
//! this by [`crate::sample`] and are never saved.

use std::collections::BTreeMap;
use std::fmt;

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rink::RinkSpec;

/// v2: possession comes from events; keyframes no longer carry `has_puck`.
/// v3: keyframes can carry a velocity, and plays can hold skating orders.
pub const SCHEMA_VERSION: u32 = 3;

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
    /// Meters per second. Keyframes baked from a skating [`Order`] carry it,
    /// so the path between two of them is the one skated, and a new order
    /// can pick up from any moment with the speed the skater had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vel: Option<Vec2>,
}

impl Keyframe {
    pub fn at(t_ms: u32, x: f32, y: f32) -> Self {
        Keyframe { t_ms, pos: Vec2::new(x, y), facing: None, vel: None }
    }

    pub fn facing(mut self, radians: f32) -> Self {
        self.facing = Some(radians);
        self
    }
}

/// How hard a skater goes on an [`Order`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pace {
    Easy,
    #[default]
    Skate,
    Sprint,
}

impl Pace {
    pub const ALL: [Pace; 3] = [Pace::Easy, Pace::Skate, Pace::Sprint];

    pub fn label(self) -> &'static str {
        match self {
            Pace::Easy => "Easy",
            Pace::Skate => "Skate",
            Pace::Sprint => "Sprint",
        }
    }

    pub fn next(self) -> Pace {
        match self {
            Pace::Easy => Pace::Skate,
            Pace::Skate => Pace::Sprint,
            Pace::Sprint => Pace::Easy,
        }
    }

    fn is_default(&self) -> bool {
        *self == Pace::default()
    }
}

/// "Skate there": given at a moment, like a right-click in a strategy game.
/// [`crate::skate`] simulates the skating and bakes it into the entity's
/// keyframes from `t_ms` on; the order is kept so it can be shown and changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub t_ms: u32,
    /// Where to skate, in turn, in rink meters. They stop at the last one.
    pub to: Vec<Vec2>,
    #[serde(default, skip_serializing_if = "Pace::is_default")]
    pub pace: Pace,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub backward: bool,
}

impl Order {
    pub fn new(t_ms: u32, to: Vec2) -> Self {
        Order { t_ms, to: vec![to], pace: Pace::default(), backward: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Pass,
    Shot,
    /// The goalie stops a shot without holding it; the puck stays loose.
    Save,
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
    pub const ALL: [EventKind; 11] = [
        EventKind::Pass,
        EventKind::Shot,
        EventKind::Save,
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
            EventKind::Save => "Save",
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

/// Something that happens at a moment. Possession comes only from events:
///
/// | kind | possession |
/// | --- | --- |
/// | `puck_recovery` | `actors[0]` gets the puck |
/// | `pass` | `actors[0]` releases it; `actors[1]` (if any) gets it on arrival |
/// | `shot`, `whistle` | whoever has it loses it; the puck is loose |
/// | `turnover` | whoever has it loses it; `actors[1]` (if any) gets it |
/// | `faceoff` | `actors[0]` (if any) comes out with it |
/// | others, including `save` | no change; labels for the reviewer |
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
    /// For a pass: when the receiver gets it. Without it, arrival is worked
    /// out from the distance at a typical pass speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrive_ms: Option<u32>,
    #[serde(default)]
    pub note: String,
    /// For a decision point: what to do there, hidden until the user asks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

impl Event {
    pub fn new(t_ms: u32, kind: EventKind, actors: &[&EntityId]) -> Self {
        Event {
            t_ms,
            kind,
            actors: actors.iter().map(|&a| a.clone()).collect(),
            from: None,
            to: None,
            arrive_ms: None,
            note: String::new(),
            answer: None,
        }
    }
}

/// One line of the play's transcript: what's going on from `t_ms` until the
/// next caption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub t_ms: u32,
    pub text: String,
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
    /// Skating orders by entity id, sorted by time. Each one's path is baked
    /// into the track from its time until the next order.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub orders: BTreeMap<EntityId, Vec<Order>>,
    #[serde(default)]
    pub events: Vec<Event>,
    /// The transcript, sorted by time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub captions: Vec<Caption>,
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
            orders: BTreeMap::new(),
            events: Vec::new(),
            captions: Vec::new(),
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

    pub fn orders(&self, id: &EntityId) -> &[Order] {
        self.orders.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The order `id` is following at `t_ms`: the last one given at or before it.
    pub fn order_at(&self, id: &EntityId, t_ms: u32) -> Option<&Order> {
        self.orders(id).iter().rev().find(|o| o.t_ms <= t_ms)
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

    /// The caption showing at `t_ms`: the last one at or before it.
    pub fn caption_at(&self, t_ms: u32) -> Option<&Caption> {
        let after = self.captions.partition_point(|c| c.t_ms <= t_ms);
        after.checked_sub(1).map(|i| &self.captions[i])
    }

    /// Insert or replace the caption at `t_ms`, keeping them sorted. Returns its index.
    pub fn set_caption(&mut self, t_ms: u32, text: impl Into<String>) -> usize {
        let caption = Caption { t_ms, text: text.into() };
        match self.captions.binary_search_by_key(&t_ms, |c| c.t_ms) {
            Ok(i) => {
                self.captions[i] = caption;
                i
            }
            Err(i) => {
                self.captions.insert(i, caption);
                i
            }
        }
    }

    /// Remove the caption at exactly `t_ms`. Returns it if there was one.
    pub fn remove_caption(&mut self, t_ms: u32) -> Option<Caption> {
        let i = self.captions.binary_search_by_key(&t_ms, |c| c.t_ms).ok()?;
        Some(self.captions.remove(i))
    }

    /// Sort keyframes, events and captions by time. Loading and editing call this so the
    /// rest of the code can assume sorted input.
    pub fn normalize(&mut self) {
        for track in self.tracks.values_mut() {
            track.sort_by_key(|k| k.t_ms);
            track.dedup_by_key(|k| k.t_ms);
        }
        for orders in self.orders.values_mut() {
            orders.sort_by_key(|o| o.t_ms);
            orders.dedup_by_key(|o| o.t_ms);
        }
        self.orders.retain(|_, orders| !orders.is_empty());
        self.events.sort_by_key(|e| e.t_ms);
        self.captions.sort_by_key(|c| c.t_ms);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_at_is_the_latest_at_or_before() {
        let mut play = Play::new("t", RinkSpec::iihf());
        play.set_caption(2500, "second");
        play.set_caption(1000, "first");
        let at = |t| play.caption_at(t).map(|c| c.text.as_str());
        assert_eq!(at(0), None, "before the first");
        assert_eq!(at(1000), Some("first"), "exact hit");
        assert_eq!(at(2499), Some("first"), "between");
        assert_eq!(at(2500), Some("second"));
        assert_eq!(at(9000), Some("second"), "after the last");
    }

    #[test]
    fn set_caption_replaces_at_the_same_time() {
        let mut play = Play::new("t", RinkSpec::iihf());
        play.set_caption(1000, "a");
        assert_eq!(play.set_caption(1000, "b"), 0);
        assert_eq!(play.captions, vec![Caption { t_ms: 1000, text: "b".into() }]);
        assert!(play.remove_caption(1000).is_some());
        assert!(play.remove_caption(1000).is_none());
    }
}
