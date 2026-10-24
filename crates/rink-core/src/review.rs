//! Position feedback: "at that moment you should have been there". A review
//! lives next to its play as `<name>.review.json`, so whoever writes it (a
//! Claude Code agent through the `rink` CLI, or later an MCP server) and the
//! app that shows it only share files.
//!
//! Every suggestion is checked before it is stored: the spot must be on the
//! ice and reachable from where the player really was when the paths split.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::io::{IoError, PLAY_EXTENSION};
use crate::model::{EntityId, Play};
use crate::sample::position_at;

pub const REVIEW_EXTENSION: &str = ".review.json";
pub const REVIEW_SCHEMA_VERSION: u32 = 1;
/// Fastest a suggestion may ask a player to skate, averaged over the route.
/// Generous for a sprint from a near standstill; anything faster is a teleport.
pub const MAX_SKATE_SPEED_MPS: f32 = 8.0;
pub const DEFAULT_ZONE_RADIUS_M: f32 = 1.5;
/// How long the ghost stays on screen after the moment it is about.
pub const GHOST_LINGER_MS: u32 = 1000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Short id within the review, such as `s1`.
    pub id: String,
    pub entity: EntityId,
    /// The moment the feedback is about.
    pub t_ms: u32,
    /// When the better route splits off from what actually happened.
    pub branch_from_ms: u32,
    /// Where the player should have been at `t_ms`, rink meters.
    pub target: Vec2,
    /// Which way to face at `t_ms`, radians (0 = +x).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<f32>,
    /// Radius of the acceptable area around `target`.
    #[serde(default = "default_radius")]
    pub zone_radius_m: f32,
    /// The positional principle, in a few words ("weak-side support").
    pub principle: String,
    /// One or two sentences on why.
    pub reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// [`play_fingerprint`] of the play when this was written. If the play's
    /// motion changes afterwards, the suggestion may no longer fit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub play_hash: String,
}

fn default_radius() -> f32 {
    DEFAULT_ZONE_RADIUS_M
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub schema_version: u32,
    pub play_id: Uuid,
    #[serde(default)]
    pub suggestions: Vec<Suggestion>,
    /// Ids are never reused, so a removed suggestion's ghost can't be
    /// mistaken for a new one.
    #[serde(default)]
    pub next_id: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SuggestionError {
    UnknownEntity(EntityId),
    IsPuck,
    NotOnIce(EntityId),
    AfterPlayEnds { t_ms: u32, duration_ms: u32 },
    BranchNotBeforeTarget { branch_from_ms: u32, t_ms: u32 },
    OffIce { x: f32, y: f32 },
    Unreachable { distance_m: f32, seconds: f32, required_mps: f32 },
}

impl fmt::Display for SuggestionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SuggestionError::UnknownEntity(id) => write!(f, "no entity {id} in this play"),
            SuggestionError::IsPuck => write!(f, "suggestions are for players, not the puck"),
            SuggestionError::NotOnIce(id) => {
                write!(f, "{id} has no keyframes, so there is no actual route to branch from")
            }
            SuggestionError::AfterPlayEnds { t_ms, duration_ms } => {
                write!(f, "t = {t_ms} ms is after the play ends ({duration_ms} ms)")
            }
            SuggestionError::BranchNotBeforeTarget { branch_from_ms, t_ms } => {
                write!(f, "branch_from ({branch_from_ms} ms) must be before t ({t_ms} ms)")
            }
            SuggestionError::OffIce { x, y } => write!(f, "target ({x:.1}, {y:.1}) is off the ice"),
            SuggestionError::Unreachable { distance_m, seconds, required_mps } => write!(
                f,
                "unreachable: {distance_m:.1} m in {seconds:.1} s needs {required_mps:.1} m/s \
                 (max {MAX_SKATE_SPEED_MPS} m/s); branch earlier or pick a closer spot"
            ),
        }
    }
}

impl std::error::Error for SuggestionError {}

/// Ease in and out so the ghost accelerates and settles like a skater.
fn smoothstep(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// A short hash of everything that decides where players and the puck are:
/// rink, duration, roster, keyframes and events. Title and notes don't count,
/// so editing them never makes a review stale.
pub fn play_fingerprint(play: &Play) -> String {
    let motion = (&play.rink, play.duration_ms, &play.teams, &play.entities, &play.tracks, &play.events);
    let bytes = serde_json::to_vec(&motion).expect("play data serializes");
    // FNV-1a: stable across builds and platforms, unlike std's hasher.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

impl Suggestion {
    /// True when the play's motion changed after this suggestion was written.
    /// Suggestions from before fingerprints existed are never stale.
    pub fn is_stale(&self, play: &Play) -> bool {
        !self.play_hash.is_empty() && self.play_hash != play_fingerprint(play)
    }

    /// Where the player actually was when the ghost's route splits off.
    pub fn branch_pos(&self, play: &Play) -> Option<Vec2> {
        position_at(play.track(&self.entity), self.branch_from_ms)
    }

    pub fn check(&self, play: &Play) -> Result<(), SuggestionError> {
        let entity = play.entity(&self.entity).ok_or_else(|| SuggestionError::UnknownEntity(self.entity.clone()))?;
        if entity.is_puck() {
            return Err(SuggestionError::IsPuck);
        }
        if self.t_ms > play.duration_ms {
            return Err(SuggestionError::AfterPlayEnds { t_ms: self.t_ms, duration_ms: play.duration_ms });
        }
        if self.branch_from_ms >= self.t_ms {
            return Err(SuggestionError::BranchNotBeforeTarget {
                branch_from_ms: self.branch_from_ms,
                t_ms: self.t_ms,
            });
        }
        if !play.rink.contains(self.target) {
            return Err(SuggestionError::OffIce { x: self.target.x, y: self.target.y });
        }
        let start = self.branch_pos(play).ok_or_else(|| SuggestionError::NotOnIce(self.entity.clone()))?;
        let distance_m = start.distance(self.target);
        let seconds = (self.t_ms - self.branch_from_ms) as f32 / 1000.0;
        let required_mps = distance_m / seconds;
        if required_mps > MAX_SKATE_SPEED_MPS {
            return Err(SuggestionError::Unreachable { distance_m, seconds, required_mps });
        }
        Ok(())
    }

    /// The ghost's position at `t_ms`: absent before the branch, skating to
    /// the target, then holding there briefly.
    pub fn ghost_pos(&self, play: &Play, t_ms: u32) -> Option<Vec2> {
        if t_ms < self.branch_from_ms || t_ms > self.t_ms + GHOST_LINGER_MS {
            return None;
        }
        let start = self.branch_pos(play)?;
        let u = (t_ms - self.branch_from_ms) as f32 / (self.t_ms - self.branch_from_ms).max(1) as f32;
        Some(start.lerp(self.target, smoothstep(u)))
    }

    /// Distance between where the player was and the target at `t_ms`.
    pub fn miss_distance(&self, play: &Play) -> Option<f32> {
        position_at(play.track(&self.entity), self.t_ms).map(|p| p.distance(self.target))
    }
}

impl Review {
    pub fn new(play: &Play) -> Self {
        Review { schema_version: REVIEW_SCHEMA_VERSION, play_id: play.id, suggestions: Vec::new(), next_id: 1 }
    }

    /// Check `s`, give it the next free id, and append it. Returns the id.
    pub fn add(&mut self, play: &Play, mut s: Suggestion) -> Result<String, SuggestionError> {
        s.check(play)?;
        s.play_hash = play_fingerprint(play);
        // Older files have no counter; start past every id already used.
        let highest = self.suggestions.iter().filter_map(|x| x.id.strip_prefix('s')?.parse::<u32>().ok()).max();
        let next = self.next_id.max(highest.map_or(1, |h| h + 1)).max(1);
        self.next_id = next + 1;
        s.id = format!("s{next}");
        s.confidence = s.confidence.map(|c| (c.clamp(0.0, 1.0) * 100.0).round() / 100.0);
        let id = s.id.clone();
        self.suggestions.push(s);
        self.suggestions.sort_by_key(|s| s.t_ms);
        Ok(id)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.suggestions.len();
        self.suggestions.retain(|s| s.id != id);
        self.suggestions.len() != before
    }
}

/// `plays/breakout-1a2b3c4d.rink.json` -> `plays/breakout-1a2b3c4d.review.json`.
pub fn review_path(play_path: &Path) -> PathBuf {
    let name = play_path.file_name().and_then(|n| n.to_str()).unwrap_or("play");
    let stem = name.strip_suffix(PLAY_EXTENSION).unwrap_or(name);
    play_path.with_file_name(format!("{stem}{REVIEW_EXTENSION}"))
}

/// The review for a play, or None if there isn't one yet.
pub fn load_review(play_path: &Path) -> Result<Option<Review>, IoError> {
    let path = review_path(play_path);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(IoError::Read { path, source }),
    };
    serde_json::from_str(&text).map(Some).map_err(|source| IoError::Parse { path, source })
}

pub fn save_review(play_path: &Path, review: &Review) -> Result<PathBuf, IoError> {
    let path = review_path(play_path);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(review).expect("a Review always serializes");
    fs::write(&tmp, text).map_err(|source| IoError::Write { path: tmp.clone(), source })?;
    fs::rename(&tmp, &path).map_err(|source| IoError::Write { path: path.clone(), source })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rink::RinkSpec;
    use crate::templates::{build, Template};

    fn suggestion(play: &Play, target: Vec2, branch_from_ms: u32, t_ms: u32) -> Suggestion {
        Suggestion {
            id: String::new(),
            entity: play.me().unwrap().id.clone(),
            t_ms,
            branch_from_ms,
            target,
            facing: None,
            zone_radius_m: DEFAULT_ZONE_RADIUS_M,
            principle: "test".into(),
            reason: "test".into(),
            detail: String::new(),
            confidence: None,
            play_hash: String::new(),
        }
    }

    #[test]
    fn edits_to_motion_make_suggestions_stale() {
        let mut play = build(Template::Breakout, RinkSpec::iihf());
        let mut review = Review::new(&play);
        review.add(&play, suggestion(&play, Vec2::new(-20.0, 12.8), 0, 1200)).unwrap();
        let s = review.suggestions[0].clone();
        assert!(!s.is_stale(&play));
        play.notes = "edited notes".into();
        assert!(!s.is_stale(&play));
        let me = play.me().unwrap().id.clone();
        play.set_keyframe(&me, crate::model::Keyframe::at(500, -18.0, 10.0));
        assert!(s.is_stale(&play));
    }

    #[test]
    fn accepts_reachable_spot() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        // LW is at (-18, 11) at t = 0; 2.7 m in 1.2 s is easy.
        let s = suggestion(&play, Vec2::new(-20.0, 12.8), 0, 1200);
        assert_eq!(s.check(&play), Ok(()));
    }

    #[test]
    fn rejects_teleport_and_off_ice() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        let far = suggestion(&play, Vec2::new(20.0, 0.0), 0, 1000);
        assert!(matches!(far.check(&play), Err(SuggestionError::Unreachable { .. })));
        let off = suggestion(&play, Vec2::new(0.0, 20.0), 0, 5000);
        assert!(matches!(off.check(&play), Err(SuggestionError::OffIce { .. })));
        let backwards = suggestion(&play, Vec2::new(-18.0, 11.0), 2000, 2000);
        assert!(matches!(backwards.check(&play), Err(SuggestionError::BranchNotBeforeTarget { .. })));
    }

    #[test]
    fn ghost_skates_from_branch_to_target() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        let s = suggestion(&play, Vec2::new(-20.0, 12.8), 0, 1200);
        let start = s.branch_pos(&play).unwrap();
        assert_eq!(s.ghost_pos(&play, 0), Some(start));
        assert!(s.ghost_pos(&play, 1200).unwrap().distance(s.target) < 1e-4);
        assert!(s.ghost_pos(&play, 1200 + GHOST_LINGER_MS).is_some());
        assert_eq!(s.ghost_pos(&play, 1201 + GHOST_LINGER_MS), None);
    }

    #[test]
    fn ids_are_assigned_and_sorted_by_time() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        let mut review = Review::new(&play);
        let a = review.add(&play, suggestion(&play, Vec2::new(-19.0, 12.0), 1000, 3000)).unwrap();
        let b = review.add(&play, suggestion(&play, Vec2::new(-20.0, 12.8), 0, 1200)).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("s1", "s2"));
        assert_eq!(review.suggestions[0].id, "s2");
        assert!(review.remove("s1"));
        let c = review.add(&play, suggestion(&play, Vec2::new(-19.0, 12.0), 1000, 3000)).unwrap();
        assert_eq!(c, "s3");
    }

    #[test]
    fn review_path_sits_next_to_play() {
        let p = Path::new("/lib/breakout-1a2b3c4d.rink.json");
        assert_eq!(review_path(p), PathBuf::from("/lib/breakout-1a2b3c4d.review.json"));
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join(format!("rink-review-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let play = build(Template::Breakout, RinkSpec::iihf());
        let play_path = dir.join("p.rink.json");
        assert_eq!(load_review(&play_path).unwrap(), None);
        let mut review = Review::new(&play);
        review.add(&play, suggestion(&play, Vec2::new(-20.0, 12.8), 0, 1200)).unwrap();
        save_review(&play_path, &review).unwrap();
        assert_eq!(load_review(&play_path).unwrap(), Some(review));
        fs::remove_dir_all(&dir).unwrap();
    }
}
