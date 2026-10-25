//! The open play plus editor state (playhead, selection, view options). Views
//! observe one `Entity<PlayState>`; everything here is plain Rust so it can be
//! tested without a window.

use std::path::PathBuf;
use std::time::SystemTime;

use rink_core::review::{load_review, review_path};
use rink_core::sample::pass_arrival_ms;
use rink_core::{
    io, sample, validate, EntityId, Event, EventKind, Issue, Keyframe, Play, Review, RinkSpec, Snapshot, Suggestion,
    Vec2,
};

use crate::paint::{entity_radius, puck_draw_pos};
use crate::theme::short_name;

/// Playhead and keyframe times snap to this grid when editing.
pub const SNAP_MS: u32 = 100;
/// Extra grab distance around an entity, in meters.
const HIT_SLOP_M: f32 = 0.3;
/// How far inside the boards a dragged entity is kept, in meters.
const BOARDS_CLEARANCE_M: f32 = 0.5;

pub const SPEEDS: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

pub struct PlayState {
    pub play: Play,
    pub path: Option<PathBuf>,
    pub playhead_ms: u32,
    pub playing: bool,
    pub speed: f32,
    pub selected: Option<EntityId>,
    pub flipped: bool,
    pub dirty: bool,
    pub status: String,
    pub issues: Vec<Issue>,
    /// Position feedback from `<play>.review.json`, reloaded when it changes.
    pub review: Option<Review>,
    review_mtime: Option<SystemTime>,
    pub show_ghosts: bool,
    /// The suggestion last jumped to; highlighted in the inspector.
    pub focused_suggestion: Option<String>,
}

pub fn snap(t_ms: u32) -> u32 {
    (t_ms + SNAP_MS / 2) / SNAP_MS * SNAP_MS
}

/// Pull `p` back onto the ice along the line to center ice if it's outside.
pub fn clamp_to_ice(rink: &RinkSpec, p: Vec2) -> Vec2 {
    if rink.distance_inside_boards(p) >= BOARDS_CLEARANCE_M {
        return p;
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if rink.distance_inside_boards(p * mid) >= BOARDS_CLEARANCE_M {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    p * lo
}

impl PlayState {
    pub fn new(play: Play, path: Option<PathBuf>) -> Self {
        let issues = validate(&play);
        let selected = play.me().map(|e| e.id.clone());
        PlayState {
            play,
            path,
            playhead_ms: 0,
            playing: false,
            speed: 1.0,
            selected,
            flipped: false,
            dirty: false,
            status: String::new(),
            issues,
            review: None,
            review_mtime: None,
            show_ghosts: true,
            focused_suggestion: None,
        }
    }

    /// Load the review file if it appeared or changed since the last check.
    /// Returns true when the review was (re)loaded or removed.
    pub fn reload_review_if_changed(&mut self) -> bool {
        let Some(path) = &self.path else { return false };
        let mtime = std::fs::metadata(review_path(path)).and_then(|m| m.modified()).ok();
        if mtime == self.review_mtime {
            return false;
        }
        self.review_mtime = mtime;
        match load_review(path) {
            Ok(review) => {
                let count = review.as_ref().map_or(0, |r| r.suggestions.len());
                if review.is_some() {
                    self.status = match count {
                        1 => "1 position suggestion loaded".into(),
                        n => format!("{n} position suggestions loaded"),
                    };
                }
                self.review = review;
            }
            Err(e) => self.status = format!("Couldn't read review: {e}"),
        }
        true
    }

    pub fn suggestions(&self) -> &[Suggestion] {
        self.review.as_ref().map_or(&[], |r| r.suggestions.as_slice())
    }

    /// Suggestions whose ghost is on the ice at the playhead.
    pub fn active_suggestions(&self) -> impl Iterator<Item = (&Suggestion, Vec2)> {
        let t = self.playhead_ms;
        self.suggestions().iter().filter_map(move |s| s.ghost_pos(&self.play, t).map(|g| (s, g)))
    }

    /// Jump the playhead to the next (dir > 0) or previous suggestion's moment
    /// and select the player it is about.
    pub fn jump_suggestion(&mut self, dir: i32) {
        let t = self.playhead_ms;
        let list = self.suggestions();
        let found = if dir > 0 {
            list.iter().find(|s| s.t_ms > t).or_else(|| list.first())
        } else {
            list.iter().rev().find(|s| s.t_ms < t).or_else(|| list.last())
        };
        if let Some(s) = found.cloned() {
            self.playhead_ms = s.t_ms;
            self.selected = Some(s.entity.clone());
            self.focused_suggestion = Some(s.id);
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        sample(&self.play, self.playhead_ms)
    }

    fn edited(&mut self) {
        self.dirty = true;
        self.issues = validate(&self.play);
    }

    pub fn set_playhead(&mut self, t_ms: u32) {
        self.playhead_ms = t_ms.min(self.play.duration_ms);
    }

    pub fn step(&mut self, delta_ms: i64) {
        let t = (snap(self.playhead_ms) as i64 + delta_ms).clamp(0, self.play.duration_ms as i64);
        self.playhead_ms = t as u32;
    }

    /// Jump to the next (dir > 0) or previous keyframe or event time. Uses the
    /// selected entity's keyframes when one is selected.
    pub fn jump_keyframe(&mut self, dir: i32) {
        let times: Vec<u32> = match &self.selected {
            Some(id) => self.play.track(id).iter().map(|k| k.t_ms).collect(),
            None => self.play.key_times(),
        };
        let t = self.playhead_ms;
        let target =
            if dir > 0 { times.iter().copied().find(|&k| k > t) } else { times.iter().rev().copied().find(|&k| k < t) };
        if let Some(target) = target {
            self.playhead_ms = target;
        }
    }

    pub fn hit_test(&self, at: Vec2) -> Option<EntityId> {
        let snap = self.snapshot();
        if let Some(puck) = self.play.puck() {
            if puck_draw_pos(&self.play, &snap).distance(at) <= entity_radius(puck.kind) + HIT_SLOP_M {
                return Some(puck.id.clone());
            }
        }
        snap.entities
            .iter()
            .filter_map(|s| {
                let e = self.play.entity(&s.id)?;
                let d = s.pos.distance(at);
                (d <= entity_radius(e.kind) + HIT_SLOP_M).then_some((d, s.id.clone()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }

    /// The keyframe at the snapped playhead, or a new one at the entity's
    /// current sampled position.
    fn keyframe_at_playhead(&mut self, id: &EntityId) -> Keyframe {
        let t = snap(self.playhead_ms);
        self.playhead_ms = t;
        if let Some(k) = self.play.track(id).iter().find(|k| k.t_ms == t) {
            return *k;
        }
        let snapshot = self.snapshot();
        let pos = if self.play.entity(id).is_some_and(|e| e.is_puck()) {
            snapshot.puck.pos
        } else {
            snapshot.entity(id).map(|s| s.pos).unwrap_or(Vec2::ZERO)
        };
        Keyframe { t_ms: t, pos, facing: None }
    }

    /// Drag: move `id` to `to` at the playhead, auto-creating a keyframe.
    pub fn move_entity(&mut self, id: &EntityId, to: Vec2) {
        let mut kf = self.keyframe_at_playhead(id);
        kf.pos = clamp_to_ice(&self.play.rink, to);
        self.play.set_keyframe(id, kf);
        self.edited();
    }

    /// Shift-drag: face `id` toward `toward` at the playhead.
    pub fn rotate_entity(&mut self, id: &EntityId, toward: Vec2) {
        if self.play.entity(id).is_some_and(|e| e.is_puck()) {
            return;
        }
        let mut kf = self.keyframe_at_playhead(id);
        let d = toward - kf.pos;
        if d.length() < 0.2 {
            return;
        }
        kf.facing = Some(d.y.atan2(d.x));
        self.play.set_keyframe(id, kf);
        self.edited();
    }

    /// Alt-click: `id` gets the puck at the playhead. Possession comes from
    /// events, so this adds the event that fits: a pass from a teammate who
    /// has it, a turnover from an opponent who has it, or a recovery of a loose
    /// puck. Alt-clicking the player who just got it here undoes that.
    pub fn give_puck(&mut self, id: &EntityId) {
        let Some(entity) = self.play.entity(id).cloned() else { return };
        if entity.is_puck() {
            return;
        }
        let t = snap(self.playhead_ms);
        self.playhead_ms = t;
        let carrier = self.snapshot().puck.carrier;

        if carrier.as_ref() == Some(id) {
            let before = self.play.events.len();
            let arrivals: Vec<Option<u32>> =
                self.play.events.iter().map(|ev| pass_arrival_ms(&self.play, ev)).collect();
            let mut i = 0;
            self.play.events.retain(|ev| {
                let arrival = arrivals[i];
                i += 1;
                let gave_here = match ev.kind {
                    EventKind::PuckRecovery | EventKind::Faceoff => ev.t_ms == t && ev.actors.first() == Some(id),
                    EventKind::Turnover => ev.t_ms == t && ev.actors.get(1) == Some(id),
                    EventKind::Pass => arrival == Some(t) && ev.actors.get(1) == Some(id),
                    _ => false,
                };
                !gave_here
            });
            if self.play.events.len() == before {
                self.play.events.push(Event::new(t, EventKind::Turnover, &[id]));
                self.status = format!("{} loses the puck at {:.1}s", short_name(&self.play, id), t as f32 / 1000.0);
            } else {
                self.status =
                    format!("Removed how {} got the puck at {:.1}s", short_name(&self.play, id), t as f32 / 1000.0);
            }
        } else {
            let event = match carrier {
                Some(c) if self.play.entity(&c).and_then(|e| e.team) == entity.team => {
                    Event::new(t, EventKind::Pass, &[&c, id])
                }
                Some(c) => Event::new(t, EventKind::Turnover, &[&c, id]),
                None => Event::new(t, EventKind::PuckRecovery, &[id]),
            };
            let who: Vec<String> = event.actors.iter().map(|a| short_name(&self.play, a)).collect();
            self.status = format!("{} {} at {:.1}s", event.kind.label(), who.join(" → "), t as f32 / 1000.0);
            self.play.events.push(event);
        }
        self.play.normalize();
        self.edited();
    }

    /// Delete the selected entity's keyframe at the playhead.
    pub fn delete_keyframe(&mut self) -> bool {
        let Some(id) = self.selected.clone() else { return false };
        let removed = self.play.remove_keyframe(&id, snap(self.playhead_ms)).is_some();
        if removed {
            self.edited();
        }
        removed
    }

    pub fn cycle_speed(&mut self) {
        let i = SPEEDS.iter().position(|&s| s == self.speed).unwrap_or(2);
        self.speed = SPEEDS[(i + 1) % SPEEDS.len()];
    }

    /// Save to the open file, or a new file in the library folder.
    /// Show `play` from `path` in place of the current one: another play, or a
    /// fresh copy of this one after an agent changed the file. View settings
    /// stay; on a reload the playhead and selection stay too, if they still fit.
    pub fn replace_play(&mut self, play: Play, path: PathBuf) {
        let reload = self.path.as_deref() == Some(path.as_path());
        let mut next = PlayState::new(play, Some(path));
        next.flipped = self.flipped;
        next.speed = self.speed;
        next.show_ghosts = self.show_ghosts;
        if reload {
            next.playhead_ms = self.playhead_ms.min(next.play.duration_ms);
            if let Some(id) = self.selected.take().filter(|id| next.play.entity(id).is_some()) {
                next.selected = Some(id);
            }
        }
        next.reload_review_if_changed();
        *self = next;
    }

    /// Save unsaved edits next to the play as `<name>.your-edits-<time>.rink.json`,
    /// leaving the play file itself alone. Used before reloading it.
    pub fn backup_edits(&self) -> Result<PathBuf, io::IoError> {
        let original = self.path.clone().unwrap_or_else(|| io::library_dir().join(io::file_name(&self.play)));
        let name = original.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let stem = name.strip_suffix(io::PLAY_EXTENSION).unwrap_or(&name);
        let backup = original.with_file_name(format!(
            "{stem}.your-edits-{}{}",
            rink_core::session::now_ms() / 1000,
            io::PLAY_EXTENSION
        ));
        io::save(&self.play, &backup)?;
        Ok(backup)
    }

    pub fn save(&mut self) -> Result<PathBuf, io::IoError> {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => {
                let dir = io::library_dir();
                std::fs::create_dir_all(&dir).map_err(|source| io::IoError::Write { path: dir.clone(), source })?;
                dir.join(io::file_name(&self.play))
            }
        };
        io::save(&self.play, &path)?;
        self.path = Some(path.clone());
        self.dirty = false;
        self.reload_review_if_changed();
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rink_core::templates::{build, Template};

    fn state() -> PlayState {
        PlayState::new(build(Template::Faceoff, RinkSpec::iihf()), None)
    }

    #[test]
    fn snaps_to_grid() {
        assert_eq!(snap(149), 100);
        assert_eq!(snap(150), 200);
        assert_eq!(snap(0), 0);
    }

    #[test]
    fn drag_creates_keyframe_at_snapped_playhead() {
        let mut s = state();
        let me = s.play.me().unwrap().id.clone();
        s.set_playhead(1234);
        s.move_entity(&me, Vec2::new(5.0, 5.0));
        let track = s.play.track(&me);
        assert_eq!(track.len(), 2);
        assert_eq!(track[1].t_ms, 1200);
        assert_eq!(track[1].pos, Vec2::new(5.0, 5.0));
        assert!(s.dirty);
    }

    #[test]
    fn drag_keeps_existing_facing() {
        let mut s = state();
        let me = s.play.me().unwrap().id.clone();
        s.move_entity(&me, Vec2::new(-3.0, 4.0));
        assert_eq!(s.play.track(&me)[0].facing, Some(0.0));
    }

    #[test]
    fn drag_off_ice_is_clamped() {
        let mut s = state();
        let me = s.play.me().unwrap().id.clone();
        s.move_entity(&me, Vec2::new(0.0, 40.0));
        let p = s.play.track(&me)[0].pos;
        assert!(s.play.rink.contains(p));
        assert!(p.y > 13.0);
    }

    #[test]
    fn drag_keeps_carrier_in_possession() {
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let me = s.play.me().unwrap().id.clone();
        s.set_playhead(2700);
        s.move_entity(&me, Vec2::new(-16.0, 12.0));
        assert_eq!(s.snapshot().puck.carrier, Some(me));
    }

    #[test]
    fn give_puck_picks_the_right_event() {
        use rink_core::templates::entity_id;
        use rink_core::{Role, TeamSide};

        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let me = s.play.me().unwrap().id.clone();
        let c = entity_id(TeamSide::Home, Role::C);
        let away_rw = entity_id(TeamSide::Away, Role::RW);

        // Teammate: a pass from me to C.
        s.set_playhead(3000);
        s.give_puck(&c);
        let pass = s.play.events.iter().find(|e| e.kind == EventKind::Pass && e.t_ms == 3000).unwrap();
        assert_eq!(pass.actors, vec![me.clone(), c.clone()]);

        // Opponent: a turnover to their RW, who has it at once.
        s.set_playhead(2500);
        s.give_puck(&away_rw);
        assert_eq!(s.snapshot().puck.carrier, Some(away_rw.clone()));

        // Alt-click the same player again: undone.
        s.give_puck(&away_rw);
        assert_eq!(s.snapshot().puck.carrier, Some(me.clone()));
    }

    #[test]
    fn give_loose_puck_is_a_recovery() {
        let mut s = state();
        let me = s.play.me().unwrap().id.clone();
        s.set_playhead(1000);
        s.give_puck(&me);
        assert!(s.play.events.iter().any(|e| e.kind == EventKind::PuckRecovery && e.t_ms == 1000));
        assert_eq!(s.snapshot().puck.carrier, Some(me));
    }

    #[test]
    fn loads_review_and_jumps_between_suggestions() {
        use rink_core::review::{save_review, DEFAULT_ZONE_RADIUS_M};

        let dir = std::env::temp_dir().join(format!("rink-state-test-{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        let play = build(Template::Breakout, RinkSpec::iihf());
        let path = dir.join("p.rink.json");
        io::save(&play, &path).unwrap();
        let mut s = PlayState::new(play.clone(), Some(path.clone()));
        assert!(!s.reload_review_if_changed());
        assert!(s.review.is_none());

        let me = play.me().unwrap().id.clone();
        let mut review = Review::new(&play);
        for (t, x) in [(1500, -21.0), (4000, -9.0)] {
            let sug = Suggestion {
                id: String::new(),
                entity: me.clone(),
                t_ms: t,
                branch_from_ms: t - 1000,
                target: Vec2::new(x, 12.0),
                facing: None,
                zone_radius_m: DEFAULT_ZONE_RADIUS_M,
                principle: "p".into(),
                reason: "r".into(),
                detail: String::new(),
                confidence: None,
                play_hash: String::new(),
            };
            review.add(&play, sug).unwrap();
        }
        save_review(&path, &review).unwrap();
        assert!(s.reload_review_if_changed());
        assert_eq!(s.suggestions().len(), 2);

        s.selected = None;
        s.jump_suggestion(1);
        assert_eq!((s.playhead_ms, s.selected.clone()), (1500, Some(me.clone())));
        assert_eq!(s.active_suggestions().count(), 1);
        s.jump_suggestion(1);
        assert_eq!(s.playhead_ms, 4000);
        s.jump_suggestion(-1);
        assert_eq!(s.playhead_ms, 1500);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn uuid_like() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn hit_test_finds_nearest() {
        let s = state();
        let me = s.play.me().unwrap();
        let pos = s.play.track(&me.id)[0].pos;
        assert_eq!(s.hit_test(pos + Vec2::new(0.3, 0.0)), Some(me.id.clone()));
        assert_eq!(s.hit_test(Vec2::new(0.0, 14.0)), None);
        assert_eq!(s.hit_test(Vec2::ZERO), Some("puck".into()));
    }

    #[test]
    fn rotate_sets_facing() {
        let mut s = state();
        let me = s.play.me().unwrap().id.clone();
        let pos = s.play.track(&me)[0].pos;
        s.rotate_entity(&me, pos + Vec2::new(0.0, 3.0));
        let f = s.play.track(&me)[0].facing.unwrap();
        assert!((f - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn jump_between_keyframes() {
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        s.jump_keyframe(1);
        assert_eq!(s.playhead_ms, 2000);
        s.jump_keyframe(1);
        assert_eq!(s.playhead_ms, 3500);
        s.jump_keyframe(-1);
        assert_eq!(s.playhead_ms, 2000);
    }

    #[test]
    fn reload_keeps_playhead_and_selection_but_switching_resets_them() {
        let path = PathBuf::from("/plays/a.rink.json");
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), Some(path.clone()));
        s.flipped = true;
        s.set_playhead(3000);
        s.selected = Some(EntityId::from("home_C"));
        s.move_entity(&EntityId::from("home_C"), Vec2::new(1.0, 1.0));
        assert!(s.dirty);

        s.replace_play(build(Template::Breakout, RinkSpec::iihf()), path);
        assert_eq!((s.playhead_ms, s.selected.as_ref().map(|i| i.as_str())), (3000, Some("home_C")));
        assert!(!s.dirty);
        assert!(s.flipped, "view settings carry over");

        s.replace_play(build(Template::Faceoff, RinkSpec::iihf()), PathBuf::from("/plays/b.rink.json"));
        assert_eq!(s.playhead_ms, 0);
        assert_eq!(s.selected, s.play.me().map(|e| e.id.clone()));
    }

    #[test]
    fn backup_leaves_the_play_file_alone() {
        let dir = std::env::temp_dir().join(format!("rink-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rush.rink.json");
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), Some(path.clone()));
        s.save().unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        s.play.title = "Edited".into();

        let backup = s.backup_edits().unwrap();
        let name = backup.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("rush.your-edits-") && name.ends_with(io::PLAY_EXTENSION), "{name}");
        assert_eq!(io::load(&backup).unwrap().title, "Edited");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
        std::fs::remove_dir_all(&dir).ok();
    }
}
