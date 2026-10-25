//! The open play plus editor state (playhead, selection, view options). Views
//! observe one `Entity<PlayState>`; everything here is plain Rust so it can be
//! tested without a window.

use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use rink_core::review::{load_review, review_path};
use rink_core::sample::{facing_at, is_baked_at, pass_arrival_ms, velocity_at};
use rink_core::skate::{apply_order, order_end_ms};
use rink_core::templates::{standard_entity, standard_slots};
use rink_core::{
    io, sample, validate, EntityId, Event, EventKind, Issue, Keyframe, Order, Pace, Play, Review, RinkSpec, Role,
    Snapshot, Suggestion, TeamSide, Vec2,
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
/// How many edits undo remembers.
const UNDO_LIMIT: usize = 200;
/// Autosave waits this long after the last edit, so a drag saves once.
pub const AUTOSAVE_IDLE: Duration = Duration::from_millis(800);

/// One undoable edit: what it was called and the play as it was on the other
/// side of it.
struct Step {
    label: String,
    play: Play,
}

pub struct PlayState {
    pub play: Play,
    pub path: Option<PathBuf>,
    pub playhead_ms: u32,
    pub playing: bool,
    pub speed: f32,
    pub selected: Option<EntityId>,
    pub flipped: bool,
    /// The play differs from the file (or was never saved). Autosave clears
    /// it soon after the last edit.
    pub dirty: bool,
    /// When the play was last edited, for autosave.
    last_edit: Option<Instant>,
    /// The file's modification time as we last loaded or saved it. A
    /// different time on disk means someone else (an agent) wrote it.
    disk_mtime: Option<SystemTime>,
    /// Set when the file changed on disk while we had unsaved edits: our
    /// edits were copied here, and autosave stops until the user picks a
    /// version.
    pub conflict: Option<PathBuf>,
    /// The play as last loaded or saved, so undoing back to it clears `dirty`.
    saved: Play,
    undo: Vec<Step>,
    redo: Vec<Step>,
    pub status: Status,
    pub issues: Vec<Issue>,
    /// Position feedback from `<play>.review.json`, reloaded when it changes.
    pub review: Option<Review>,
    review_mtime: Option<SystemTime>,
    pub show_ghosts: bool,
    /// The suggestion last jumped to; highlighted in the inspector.
    pub focused_suggestion: Option<String>,
    /// The event open for editing in the inspector, by index.
    pub selected_event: Option<usize>,
    /// A position copied with Cmd-C, to paste onto a player with Cmd-V.
    copied: Option<(Vec2, Option<f32>)>,
}

/// The last thing the app has to say: a passing note in the toolbar, or an
/// error that stays in a banner until it's dismissed.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Status {
    #[default]
    None,
    Info(String),
    Error(String),
}

impl Status {
    pub fn text(&self) -> &str {
        match self {
            Status::None => "",
            Status::Info(text) | Status::Error(text) => text,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Status::Error(_))
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn snap(t_ms: u32) -> u32 {
    (t_ms + SNAP_MS / 2) / SNAP_MS * SNAP_MS
}

/// Pull `p` back onto the ice along the line to center ice if it's outside.
pub fn clamp_to_ice(rink: &RinkSpec, p: Vec2) -> Vec2 {
    rink.clamp_inside(p, BOARDS_CLEARANCE_M)
}

impl PlayState {
    pub fn new(play: Play, path: Option<PathBuf>) -> Self {
        let issues = validate(&play);
        let selected = play.me().map(|e| e.id.clone());
        PlayState {
            saved: play.clone(),
            last_edit: None,
            disk_mtime: path.as_deref().and_then(mtime),
            conflict: None,
            undo: Vec::new(),
            redo: Vec::new(),
            play,
            path,
            playhead_ms: 0,
            playing: false,
            speed: 1.0,
            selected,
            flipped: false,
            dirty: false,
            status: Status::default(),
            issues,
            review: None,
            review_mtime: None,
            show_ghosts: true,
            focused_suggestion: None,
            selected_event: None,
            copied: None,
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
                    self.set_status(match count {
                        1 => "1 position suggestion loaded".into(),
                        n => format!("{n} position suggestions loaded"),
                    });
                }
                self.review = review;
            }
            Err(e) => self.set_error(format!("Couldn't read the review: {e}")),
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
        self.dirty = self.play != self.saved;
        self.last_edit = Some(Instant::now());
        self.issues = validate(&self.play);
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = Status::Error(message.into());
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Status::Info(message.into());
    }

    /// The open file was written by someone else since we loaded or saved it.
    pub fn changed_on_disk(&self) -> bool {
        self.path.as_deref().is_some_and(|p| mtime(p) != self.disk_mtime)
    }

    /// Unsaved edits that have sat long enough to autosave.
    pub fn autosave_due(&self, now: Instant) -> bool {
        self.dirty && self.conflict.is_none() && self.last_edit.is_none_or(|t| now - t >= AUTOSAVE_IDLE)
    }

    /// Save unsaved edits, unless the file changed on disk since we read it:
    /// then copy them next to it instead, and stop autosaving until the user
    /// picks a version (Cmd-S keeps theirs over the file; reopening takes the
    /// file's).
    pub fn autosave(&mut self) -> Result<(), io::IoError> {
        if !self.dirty || self.conflict.is_some() {
            return Ok(());
        }
        if self.changed_on_disk() {
            let backup = self.backup_edits()?;
            self.set_error("This play changed on disk while you were editing it. Your edits are kept in a copy.");
            self.conflict = Some(backup);
            return Ok(());
        }
        self.save().map(|_| ())
    }

    /// Remember the play as it is now, before an edit called `label`. A drag
    /// calls this once, on its first move, so the whole drag is one step.
    pub fn checkpoint(&mut self, label: impl Into<String>) {
        self.undo.push(Step { label: label.into(), play: self.play.clone() });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        self.travel(true)
    }

    pub fn redo(&mut self) -> bool {
        self.travel(false)
    }

    /// Step back (or forward) one edit. Steps that changed nothing, like a
    /// click that started a drag but never moved, are skipped.
    fn travel(&mut self, back: bool) -> bool {
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        while let Some(step) = from.pop() {
            if step.play == self.play {
                continue;
            }
            let current = std::mem::replace(&mut self.play, step.play);
            to.push(Step { label: step.label.clone(), play: current });
            self.set_status(format!("{} {}", if back { "Undid" } else { "Redid" }, step.label));
            self.edited();
            return true;
        }
        self.set_status(if back { "Nothing to undo" } else { "Nothing to redo" });
        false
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
    /// current sampled position. On a path baked from an order, a new one
    /// keeps the skated velocity and facing, so the path stays smooth.
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
        let track = self.play.track(id);
        match self.play.entity(id).filter(|_| is_baked_at(track, t)) {
            Some(e) => {
                Keyframe { t_ms: t, pos, facing: Some(facing_at(&self.play, e, t)), vel: Some(velocity_at(track, t)) }
            }
            None => Keyframe { t_ms: t, pos, facing: None, vel: None },
        }
    }

    /// When an order given now starts: the snapped playhead, or while
    /// playing, the playhead as it is (to the simulation step).
    fn order_time(&mut self) -> u32 {
        if self.playing {
            self.playhead_ms / 10 * 10
        } else {
            self.playhead_ms = snap(self.playhead_ms);
            self.playhead_ms
        }
    }

    /// Right-click: the selected player skates to `to` from the playhead on,
    /// replacing what they did after it. With `queue` (Shift), `to` is added
    /// to the order they're following instead, as the new place to stop.
    pub fn order_to(&mut self, to: Vec2, queue: bool) {
        let Some(id) = self.selected.clone() else {
            self.set_status("Select a player, then right-click where they should skate");
            return;
        };
        let Some(entity) = self.play.entity(&id) else { return };
        if entity.is_puck() {
            self.set_status("The puck goes where passes and shots send it; Alt-click a player to give it to them");
            return;
        }
        if self.play.track(&id).is_empty() {
            self.set_status(format!("{} isn't on the ice", short_name(&self.play, &id)));
            return;
        }
        let to = clamp_to_ice(&self.play.rink, to);
        let t = self.order_time();
        let order = match self.play.order_at(&id, t).cloned() {
            Some(mut current) if queue => {
                current.to.push(to);
                current
            }
            // A new order skates like the last one: same pace, same direction.
            Some(previous) => Order { t_ms: t, to: vec![to], ..previous },
            None => Order::new(t, to),
        };
        self.checkpoint(format!("order for {}", short_name(&self.play, &id)));
        self.give_order(&id, order);
    }

    /// Apply `order` to `id` and say when they get there.
    fn give_order(&mut self, id: &EntityId, order: Order) {
        let duration = self.play.duration_ms;
        let Some(end) = apply_order(&mut self.play, id, order) else { return };
        let mut status = format!("{} stops at {:.1}s", short_name(&self.play, id), end as f32 / 1000.0);
        if self.play.duration_ms > duration {
            status.push_str(&format!("; the play now runs {:.0}s", self.play.duration_ms as f32 / 1000.0));
        }
        self.set_status(status);
        self.edited();
    }

    /// The order the selected player follows at the playhead.
    pub fn current_order(&self) -> Option<(EntityId, Order)> {
        let id = self.selected.as_ref()?;
        Some((id.clone(), self.play.order_at(id, self.playhead_ms)?.clone()))
    }

    /// When the selected player's current order is done (see [`order_end_ms`]).
    pub fn current_order_end(&self) -> Option<u32> {
        self.current_order().map(|(id, o)| order_end_ms(&self.play, &id, &o))
    }

    /// Change the selected player's current order and skate it again.
    fn change_order(&mut self, label: &str, f: impl FnOnce(&mut Order)) {
        let Some((id, mut order)) = self.current_order() else {
            self.set_status("No order here: select a player and right-click where they should skate");
            return;
        };
        f(&mut order);
        self.checkpoint(format!("{label} for {}", short_name(&self.play, &id)));
        self.give_order(&id, order);
    }

    pub fn set_order_pace(&mut self, pace: Pace) {
        self.change_order("pace", |o| o.pace = pace);
    }

    /// P: easy, skate, sprint, and round again.
    pub fn cycle_order_pace(&mut self) {
        if let Some((_, o)) = self.current_order() {
            self.set_order_pace(o.pace.next());
        } else {
            self.change_order("pace", |_| {});
        }
    }

    /// B: skate the current order backwards (or forwards again).
    pub fn toggle_order_backward(&mut self) {
        self.change_order("skating direction", |o| o.backward = !o.backward);
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
        self.checkpoint(format!("puck change for {}", short_name(&self.play, id)));
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
                self.set_status(format!("{} loses the puck at {:.1}s", short_name(&self.play, id), t as f32 / 1000.0));
            } else {
                self.set_status(format!(
                    "Removed how {} got the puck at {:.1}s",
                    short_name(&self.play, id),
                    t as f32 / 1000.0
                ));
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
            self.set_status(format!("{} {} at {:.1}s", event.kind.label(), who.join(" → "), t as f32 / 1000.0));
            self.play.events.push(event);
        }
        self.play.normalize();
        self.edited();
    }

    /// Delete the selected entity's keyframe at the playhead.
    pub fn delete_keyframe(&mut self) -> bool {
        let Some(id) = self.selected.clone() else { return false };
        let t = snap(self.playhead_ms);
        if !self.play.track(&id).iter().any(|k| k.t_ms == t) {
            return false;
        }
        self.checkpoint(format!("keyframe delete for {}", short_name(&self.play, &id)));
        self.play.remove_keyframe(&id, t);
        self.edited();
        true
    }

    /// Timeline drag: move `id`'s keyframe at `from_ms` to `to_ms`, snapped
    /// and kept between its neighbours. Returns where it landed.
    pub fn retime_keyframe(&mut self, id: &EntityId, from_ms: u32, to_ms: u32) -> u32 {
        let track = self.play.track(id);
        let Some(i) = track.iter().position(|k| k.t_ms == from_ms) else { return from_ms };
        let lo = if i > 0 { track[i - 1].t_ms + SNAP_MS } else { 0 };
        let hi = track.get(i + 1).map_or(self.play.duration_ms, |k| k.t_ms.saturating_sub(SNAP_MS));
        let to = snap(to_ms).clamp(lo, hi.max(lo));
        if to != from_ms {
            if let Some(mut kf) = self.play.remove_keyframe(id, from_ms) {
                kf.t_ms = to;
                self.play.set_keyframe(id, kf);
            }
            self.edited();
        }
        self.playhead_ms = to;
        to
    }

    /// Timeline drag: move event `index` to `to_ms`. A pass keeps its flight
    /// time. Returns the event's index after re-sorting.
    pub fn retime_event(&mut self, index: usize, to_ms: u32) -> usize {
        let Some(ev) = self.play.events.get_mut(index) else { return index };
        let to = snap(to_ms).min(self.play.duration_ms);
        if to != ev.t_ms {
            if let Some(arrive) = ev.arrive_ms {
                ev.arrive_ms = Some((arrive as i64 + to as i64 - ev.t_ms as i64).max(to as i64) as u32);
            }
            ev.t_ms = to;
            let moved = ev.clone();
            self.play.normalize();
            self.edited();
            self.playhead_ms = to;
            let new_index = self.play.events.iter().position(|e| *e == moved).unwrap_or(index);
            if self.selected_event == Some(index) {
                self.selected_event = Some(new_index);
            }
            return new_index;
        }
        self.playhead_ms = to;
        index
    }

    pub fn delete_event(&mut self, index: usize) {
        let Some(ev) = self.play.events.get(index) else { return };
        let (label, t) = (ev.kind.label().to_lowercase(), ev.t_ms);
        self.checkpoint(format!("delete of a {label}"));
        self.play.events.remove(index);
        self.selected_event = match self.selected_event {
            Some(i) if i == index => None,
            Some(i) if i > index => Some(i - 1),
            other => other,
        };
        self.set_status(format!("Deleted the {label} at {:.1}s", t as f32 / 1000.0));
        self.edited();
    }

    /// Inspector click: go to event `index` and select who did it.
    pub fn jump_to_event(&mut self, index: usize) {
        let Some(ev) = self.play.events.get(index) else { return };
        self.playing = false;
        self.selected_event = Some(index);
        self.playhead_ms = ev.t_ms;
        if let Some(actor) = ev.actors.first() {
            self.selected = Some(actor.clone());
        }
    }

    /// Inspector click: go to a suggestion's moment and select its player.
    pub fn focus_suggestion(&mut self, id: &str) {
        let Some(s) = self.suggestions().iter().find(|s| s.id == id).cloned() else { return };
        self.playing = false;
        self.playhead_ms = s.t_ms;
        self.selected = Some(s.entity);
        self.focused_suggestion = Some(s.id);
    }

    // Play details, edited in the inspector. Text edits don't record undo
    // steps themselves: the inspector records one when a field gets its
    // first change.

    pub fn set_title(&mut self, title: &str) {
        if self.play.title != title {
            self.play.title = title.to_string();
            self.edited();
        }
    }

    pub fn set_notes(&mut self, notes: &str) {
        if self.play.notes != notes {
            self.play.notes = notes.to_string();
            self.edited();
        }
    }

    pub fn set_team_name(&mut self, side: TeamSide, name: &str) {
        let team = &mut self.play.teams[side.index()];
        if team.name != name {
            team.name = name.to_string();
            self.edited();
        }
    }

    /// A jersey number typed into the inspector: digits only, at most two;
    /// empty clears it. Anything else is ignored.
    pub fn set_number(&mut self, id: &EntityId, text: &str) {
        let number = match text.trim() {
            "" => None,
            t if t.len() <= 2 && t.chars().all(|c| c.is_ascii_digit()) => t.parse().ok(),
            _ => return,
        };
        if let Some(e) = self.play.entities.iter_mut().find(|e| &e.id == id) {
            if e.number != number {
                e.number = number;
                self.edited();
            }
        }
    }

    pub fn set_me(&mut self, id: &EntityId) {
        self.checkpoint(format!("making {} you", short_name(&self.play, id)));
        for e in &mut self.play.entities {
            e.is_me = &e.id == id;
        }
        self.set_status(format!("You're {} now", short_name(&self.play, id)));
        self.edited();
    }

    /// The shortest the play can be: up to its last keyframe or event.
    pub fn min_duration_ms(&self) -> u32 {
        self.play.key_times().last().copied().unwrap_or(0).max(SNAP_MS)
    }

    pub fn change_duration(&mut self, delta_ms: i64) {
        let to = snap((self.play.duration_ms as i64 + delta_ms).clamp(0, 600_000) as u32).max(self.min_duration_ms());
        if to == self.play.duration_ms {
            self.set_status("The play can't be shorter than its last keyframe or event");
            return;
        }
        self.checkpoint("length change");
        self.play.duration_ms = to;
        self.playhead_ms = self.playhead_ms.min(to);
        self.edited();
    }

    /// Standard roster spots with nobody on the ice: players without
    /// keyframes, and roles the play doesn't have at all.
    pub fn off_ice(&self) -> Vec<(TeamSide, Role, EntityId)> {
        standard_slots()
            .filter_map(|(side, role)| {
                let id = standard_entity(side, role).id;
                let on_ice = self
                    .play
                    .entities
                    .iter()
                    .find(|e| e.team == Some(side) && e.role == Some(role))
                    .is_some_and(|e| !self.play.track(&e.id).is_empty());
                (!on_ice).then_some((side, role, id))
            })
            .collect()
    }

    /// Put a roster spot on the ice near the puck at the playhead, adding
    /// the player if the play doesn't have them, and select them.
    pub fn put_on_ice(&mut self, side: TeamSide, role: Role) {
        self.checkpoint(format!("putting {} on the ice", role.as_str()));
        let id = match self.play.entities.iter().find(|e| e.team == Some(side) && e.role == Some(role)) {
            Some(e) => e.id.clone(),
            None => {
                let e = standard_entity(side, role);
                let id = e.id.clone();
                self.play.entities.push(e);
                id
            }
        };
        let sign = self.play.team(side).attack_sign();
        let near = self.snapshot().puck.pos - Vec2::new(3.0 * sign, 0.0);
        let t = snap(self.playhead_ms);
        self.playhead_ms = t;
        self.play
            .set_keyframe(&id, Keyframe { t_ms: t, pos: clamp_to_ice(&self.play.rink, near), facing: None, vel: None });
        self.selected = Some(id.clone());
        self.set_status(format!("{} is on the ice. Drag them where they were.", short_name(&self.play, &id)));
        self.edited();
    }

    /// Remove a player's keyframes, so they're not on the ice. Players in
    /// events stay, since the events need them.
    pub fn take_off_ice(&mut self, id: &EntityId) {
        let n = self.play.events.iter().filter(|e| e.actors.contains(id)).count();
        if n > 0 {
            self.set_status(format!(
                "{} is in {n} event{}; remove them from those first",
                short_name(&self.play, id),
                if n == 1 { "" } else { "s" }
            ));
            return;
        }
        self.checkpoint(format!("taking {} off the ice", short_name(&self.play, id)));
        self.play.tracks.remove(id);
        self.play.orders.remove(id);
        if self.selected.as_ref() == Some(id) {
            self.selected = None;
        }
        self.edited();
    }

    /// Add a `kind` event at the playhead, done by the selected player, and
    /// open it for editing.
    pub fn add_event(&mut self, kind: EventKind) -> usize {
        self.checkpoint(format!("new {}", kind.label().to_lowercase()));
        let t = snap(self.playhead_ms);
        self.playhead_ms = t;
        let actor = self.selected.clone().filter(|id| self.play.entity(id).is_some_and(|e| !e.is_puck()));
        let event = Event::new(t, kind, &actor.iter().collect::<Vec<_>>());
        self.play.events.push(event.clone());
        self.play.normalize();
        let i = self.play.events.iter().rposition(|e| *e == event).unwrap_or(0);
        self.selected_event = Some(i);
        self.set_status(format!("Added a {} at {:.1}s", kind.label().to_lowercase(), t as f32 / 1000.0));
        self.edited();
        i
    }

    pub fn set_event_kind(&mut self, index: usize, kind: EventKind) {
        let Some(ev) = self.play.events.get(index) else { return };
        if ev.kind == kind {
            return;
        }
        self.checkpoint(format!("change to a {}", kind.label().to_lowercase()));
        self.play.events[index].kind = kind;
        self.edited();
    }

    pub fn set_event_note(&mut self, index: usize, note: &str) {
        if let Some(ev) = self.play.events.get_mut(index) {
            if ev.note != note {
                ev.note = note.to_string();
                self.edited();
            }
        }
    }

    pub fn add_event_actor(&mut self, index: usize, id: &EntityId) {
        let Some(ev) = self.play.events.get(index) else { return };
        if ev.actors.contains(id) {
            return;
        }
        self.checkpoint(format!("adding {} to an event", short_name(&self.play, id)));
        self.play.events[index].actors.push(id.clone());
        self.edited();
    }

    pub fn remove_event_actor(&mut self, index: usize, actor: usize) {
        let Some(id) = self.play.events.get(index).and_then(|e| e.actors.get(actor)).cloned() else { return };
        self.checkpoint(format!("removing {} from an event", short_name(&self.play, &id)));
        self.play.events[index].actors.remove(actor);
        self.edited();
    }

    /// Cmd-C: remember where the selected player (or puck) is at the
    /// playhead, and which way they face.
    pub fn copy_position(&mut self) {
        let Some(id) = self.selected.clone() else { return };
        let snap = self.snapshot();
        let copied = if self.play.entity(&id).is_some_and(|e| e.is_puck()) {
            Some((snap.puck.pos, None))
        } else {
            snap.entity(&id).map(|e| (e.pos, Some(e.facing)))
        };
        if let Some(c) = copied {
            self.copied = Some(c);
            self.set_status(format!(
                "Copied where {} is at {:.1}s",
                short_name(&self.play, &id),
                self.playhead_ms as f32 / 1000.0
            ));
        }
    }

    /// Cmd-V: put the selected player at the copied spot, as a keyframe at
    /// the playhead. Pasting onto the same player later holds them there.
    pub fn paste_position(&mut self) {
        let (Some(id), Some((pos, facing))) = (self.selected.clone(), self.copied) else {
            self.set_status("Nothing to paste: select a player and press Cmd-C first");
            return;
        };
        self.checkpoint(format!("paste onto {}", short_name(&self.play, &id)));
        let mut kf = self.keyframe_at_playhead(&id);
        kf.pos = pos;
        if !self.play.entity(&id).is_some_and(|e| e.is_puck()) {
            kf.facing = facing;
        }
        self.play.set_keyframe(&id, kf);
        self.set_status(format!("Pasted onto {} at {:.1}s", short_name(&self.play, &id), kf.t_ms as f32 / 1000.0));
        self.edited();
    }

    /// Tab: select the next (dir > 0) or previous player on the ice, in
    /// roster order, starting from you.
    pub fn cycle_selection(&mut self, dir: i32) {
        let snap = self.snapshot();
        let mut on_ice: Vec<EntityId> = self
            .play
            .entities
            .iter()
            .filter(|e| !e.is_puck() && snap.entity(&e.id).is_some())
            .map(|e| e.id.clone())
            .collect();
        if let Some(me) = on_ice.iter().position(|id| self.play.entity(id).is_some_and(|e| e.is_me)) {
            on_ice.rotate_left(me);
        }
        if on_ice.is_empty() {
            return;
        }
        let n = on_ice.len() as i32;
        let next = match self.selected.as_ref().and_then(|id| on_ice.iter().position(|x| x == id)) {
            Some(i) => (i as i32 + dir).rem_euclid(n) as usize,
            None => 0,
        };
        self.selected = Some(on_ice[next].clone());
    }

    /// Alt-arrow: move the selected player by `delta` meters at the
    /// playhead, as a keyframe.
    pub fn nudge(&mut self, delta: Vec2) {
        let Some(id) = self.selected.clone() else { return };
        let snap = self.snapshot();
        let here = if self.play.entity(&id).is_some_and(|e| e.is_puck()) {
            Some(snap.puck.pos)
        } else {
            snap.entity(&id).map(|e| e.pos)
        };
        let Some(here) = here else { return };
        self.checkpoint(format!("nudge of {}", short_name(&self.play, &id)));
        self.move_entity(&id, here + delta);
    }

    /// Start an unsaved play (e.g. from a template). View settings stay.
    pub fn new_play(&mut self, play: Play) {
        let mut next = PlayState::new(play, None);
        next.flipped = self.flipped;
        next.speed = self.speed;
        next.show_ghosts = self.show_ghosts;
        *self = next;
    }

    pub fn cycle_speed(&mut self) {
        let i = SPEEDS.iter().position(|&s| s == self.speed).unwrap_or(2);
        self.speed = SPEEDS[(i + 1) % SPEEDS.len()];
    }

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

    /// Save to the open file, or a new file in the library folder. This
    /// overwrites the file even if it changed on disk; autosave doesn't.
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
        self.disk_mtime = mtime(&path);
        self.path = Some(path.clone());
        self.saved = self.play.clone();
        self.dirty = false;
        self.conflict = None;
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

    #[test]
    fn undo_and_redo_walk_edits_and_track_saved_state() {
        let path = PathBuf::from("/plays/a.rink.json");
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), Some(path));
        let me = s.play.me().unwrap().id.clone();
        let start = s.play.clone();

        // A drag is one step however many moves it has.
        s.checkpoint("move of me");
        s.move_entity(&me, Vec2::new(1.0, 1.0));
        s.move_entity(&me, Vec2::new(2.0, 2.0));
        let moved = s.play.clone();
        assert!(s.dirty);

        assert!(s.undo());
        assert_eq!(s.play, start);
        assert!(!s.dirty, "back at the saved play");
        assert!(s.redo());
        assert_eq!(s.play, moved);
        assert!(s.dirty);
        assert!(!s.redo());

        // A step that changed nothing is skipped, and a new edit clears redo.
        s.checkpoint("click without a move");
        assert!(s.undo());
        assert_eq!(s.play, start);
        s.checkpoint("move of me");
        s.move_entity(&me, Vec2::new(3.0, 3.0));
        assert!(!s.can_redo());
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rink-{name}-{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn autosave_writes_after_idle() {
        let dir = temp_dir("autosave");
        let path = dir.join("a.rink.json");
        io::save(&build(Template::Breakout, RinkSpec::iihf()), &path).unwrap();
        let mut s = PlayState::new(io::load(&path).unwrap(), Some(path.clone()));
        let me = s.play.me().unwrap().id.clone();
        s.move_entity(&me, Vec2::new(3.0, 3.0));
        let now = Instant::now();
        assert!(!s.autosave_due(now), "waits for the edit to settle");
        assert!(s.autosave_due(now + AUTOSAVE_IDLE));
        s.autosave().unwrap();
        assert!(!s.dirty);
        assert_eq!(io::load(&path).unwrap(), s.play);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn autosave_never_overwrites_a_file_changed_on_disk() {
        let dir = temp_dir("conflict");
        let path = dir.join("a.rink.json");
        io::save(&build(Template::Breakout, RinkSpec::iihf()), &path).unwrap();
        let mut s = PlayState::new(io::load(&path).unwrap(), Some(path.clone()));
        let me = s.play.me().unwrap().id.clone();
        s.move_entity(&me, Vec2::new(3.0, 3.0));

        // An agent rewrites the file meanwhile.
        std::thread::sleep(Duration::from_millis(20));
        let mut theirs = io::load(&path).unwrap();
        theirs.title = "Agent's version".into();
        io::save(&theirs, &path).unwrap();

        s.autosave().unwrap();
        assert_eq!(io::load(&path).unwrap().title, "Agent's version", "file untouched");
        let copy = s.conflict.clone().expect("edits went to a copy");
        assert_eq!(io::load(&copy).unwrap(), s.play);
        assert!(s.status.is_error());
        assert!(!s.autosave_due(Instant::now() + AUTOSAVE_IDLE), "autosave stops until resolved");

        // Keep mine: an explicit save overwrites and clears the conflict.
        s.save().unwrap();
        assert!(s.conflict.is_none());
        assert_eq!(io::load(&path).unwrap(), s.play);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn new_play_is_clean_until_edited() {
        let mut s = state();
        s.new_play(build(Template::Faceoff, RinkSpec::iihf()));
        assert!(!s.dirty);
        let me = s.play.me().unwrap().id.clone();
        s.move_entity(&me, Vec2::new(1.0, 1.0));
        assert!(s.dirty);
    }

    #[test]
    fn edits_play_details_and_roster() {
        use rink_core::{Role, TeamSide};
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let c = EntityId::from("home_C");

        s.set_number(&c, "91");
        assert_eq!(s.play.entity(&c).unwrap().number, Some(91));
        s.set_number(&c, "9x");
        assert_eq!(s.play.entity(&c).unwrap().number, Some(91), "junk is ignored");
        s.set_number(&c, "");
        assert_eq!(s.play.entity(&c).unwrap().number, None);

        s.set_me(&c);
        assert_eq!(s.play.me().unwrap().id, c);
        assert_eq!(s.play.entities.iter().filter(|e| e.is_me).count(), 1);

        let min = s.min_duration_ms();
        s.change_duration(-100_000);
        assert_eq!(s.play.duration_ms, min, "can't cut off keyframes or events");
        s.change_duration(500);
        assert_eq!(s.play.duration_ms, min + 500);

        // A player in events stays on the ice; one who isn't comes off and back.
        let rd = EntityId::from("away_RD");
        s.take_off_ice(&rd);
        assert!(s.play.track(&rd).is_empty());
        assert!(s.off_ice().iter().any(|(_, _, id)| *id == rd));
        s.set_playhead(1000);
        s.put_on_ice(TeamSide::Away, Role::RD);
        assert_eq!(s.play.track(&rd).len(), 1);
        assert_eq!(s.selected, Some(rd.clone()));
        let actor = s.play.events.iter().find_map(|e| e.actors.first()).cloned().unwrap();
        let keys = s.play.track(&actor).len();
        s.take_off_ice(&actor);
        assert_eq!(s.play.track(&actor).len(), keys, "players in events stay on");

        // A role the play never had is added.
        s.play.entities.retain(|e| e.id.as_str() != "away_G");
        s.play.tracks.remove(&EntityId::from("away_G"));
        s.put_on_ice(TeamSide::Away, Role::G);
        assert!(s.play.entity(&EntityId::from("away_G")).is_some());
        s.undo();
        assert!(s.play.entity(&EntityId::from("away_G")).is_none(), "one undo step");
    }

    #[test]
    fn adds_and_edits_events() {
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let me = s.play.me().unwrap().id.clone();
        let c = EntityId::from("home_C");
        s.selected = Some(me.clone());
        s.set_playhead(2250);
        let i = s.add_event(EventKind::DecisionPoint);
        assert_eq!(s.selected_event, Some(i));
        let ev = &s.play.events[i];
        assert_eq!((ev.t_ms, ev.kind, ev.actors.clone()), (2300, EventKind::DecisionPoint, vec![me.clone()]));

        s.set_event_note(i, "Where should I be?");
        s.set_event_kind(i, EventKind::Shot);
        s.add_event_actor(i, &c);
        assert_eq!(s.play.events[i].actors, vec![me.clone(), c.clone()]);
        s.remove_event_actor(i, 0);
        assert_eq!(s.play.events[i].actors, vec![c]);
        assert_eq!(s.play.events[i].note, "Where should I be?");

        s.delete_event(i);
        assert_eq!(s.selected_event, None);
    }

    #[test]
    fn copy_paste_cycle_and_nudge() {
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let me = s.play.me().unwrap().id.clone();
        let c = EntityId::from("home_C");

        s.selected = Some(c.clone());
        s.set_playhead(1000);
        s.copy_position();
        let spot = s.snapshot().entity(&c).unwrap().pos;
        s.selected = Some(me.clone());
        s.set_playhead(3000);
        s.paste_position();
        assert!(s.snapshot().entity(&me).unwrap().pos.distance(spot) < 1e-3);
        s.undo();
        assert!(s.snapshot().entity(&me).unwrap().pos.distance(spot) > 0.1);

        s.selected = None;
        s.cycle_selection(1);
        assert_eq!(s.selected, Some(me.clone()), "starts from you");
        s.cycle_selection(1);
        s.cycle_selection(-1);
        assert_eq!(s.selected, Some(me.clone()));
        s.cycle_selection(-1);
        assert_ne!(s.selected, Some(me.clone()), "wraps around");

        s.selected = Some(me.clone());
        s.set_playhead(1500);
        let before = s.snapshot().entity(&me).unwrap().pos;
        s.nudge(Vec2::new(0.5, 0.0));
        let after = s.snapshot().entity(&me).unwrap().pos;
        assert!((after - before - Vec2::new(0.5, 0.0)).length() < 1e-3);
    }

    #[test]
    fn orders_skate_and_change() {
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let me = s.play.me().unwrap().id.clone();
        s.selected = Some(me.clone());
        s.set_playhead(1040);
        let target = Vec2::new(10.0, 5.0);
        s.order_to(target, false);
        assert_eq!(s.playhead_ms, 1000, "snapped while paused");
        let (_, order) = s.current_order().unwrap();
        assert_eq!((order.t_ms, order.to.clone()), (1000, vec![target]));
        let end = s.current_order_end().unwrap();
        assert!(s.play.track(&me).iter().filter(|k| k.t_ms >= 1000).all(|k| k.vel.is_some()));
        assert!(s.play.track(&me).last().unwrap().pos.distance(target) < 0.2);
        assert!(s.dirty);

        s.set_order_pace(Pace::Sprint);
        assert!(s.current_order_end().unwrap() < end, "sprinting gets there sooner");
        assert_eq!(s.play.orders(&me).len(), 1);
        s.toggle_order_backward();
        assert!(s.current_order().unwrap().1.backward);

        // Shift-right-click: one more stop on the same order.
        s.order_to(Vec2::new(0.0, -10.0), true);
        assert_eq!(s.current_order().unwrap().1.to.len(), 2);

        // A drag on the baked path keeps it a baked path.
        s.set_playhead(1500);
        let here = s.snapshot().entity(&me).unwrap().pos;
        s.move_entity(&me, here + Vec2::new(0.3, 0.0));
        assert!(s.play.track(&me).iter().find(|k| k.t_ms == 1500).unwrap().vel.is_some());

        for _ in 0..4 {
            s.undo();
        }
        assert!(s.play.orders.is_empty(), "each order change is one undo step");

        s.selected = s.play.puck().map(|p| p.id.clone());
        s.order_to(target, false);
        assert!(s.play.orders.is_empty() && s.status.text().contains("puck"));
    }

    #[test]
    fn retime_keyframe_stays_between_neighbours() {
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let me = s.play.me().unwrap().id.clone();
        let times = |s: &PlayState| s.play.track(&me).iter().map(|k| k.t_ms).collect::<Vec<_>>();
        assert_eq!(&times(&s)[..3], &[0, 2000, 3500]);
        let pos = s.play.track(&me)[1].pos;

        assert_eq!(s.retime_keyframe(&me, 2000, 2440), 2400);
        assert_eq!(&times(&s)[..3], &[0, 2400, 3500]);
        assert_eq!(s.play.track(&me)[1].pos, pos, "only the time moves");
        assert_eq!(s.playhead_ms, 2400);
        assert_eq!(s.retime_keyframe(&me, 2400, 9000), 3400, "stops short of the next keyframe");
        assert_eq!(s.retime_keyframe(&me, 3400, 0), 100);
    }

    #[test]
    fn retime_event_resorts_and_keeps_pass_flight() {
        let mut s = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let mut pass = Event::new(3000, EventKind::Pass, &[&EntityId::from("home_LW"), &EntityId::from("home_C")]);
        pass.arrive_ms = Some(3600);
        s.play.events.push(pass);
        s.play.normalize();
        let i = s.play.events.iter().position(|e| e.kind == EventKind::Pass && e.t_ms == 3000).unwrap();
        let j = s.retime_event(i, 0);
        assert!(j < i, "re-sorted forward");
        let moved = &s.play.events[j];
        assert_eq!((moved.kind, moved.t_ms, moved.arrive_ms), (EventKind::Pass, 0, Some(600)));
        assert!(s.play.events.windows(2).all(|w| w[0].t_ms <= w[1].t_ms));

        let n = s.play.events.len();
        s.delete_event(j);
        assert_eq!(s.play.events.len(), n - 1);
        s.undo();
        assert_eq!(s.play.events.len(), n);
    }

    #[test]
    fn puck_and_keyframe_edits_are_undoable() {
        let mut s = state();
        let c = EntityId::from("home_C");
        let before = s.play.clone();
        s.give_puck(&c);
        assert_ne!(s.play, before);
        s.undo();
        assert_eq!(s.play, before);

        s.selected = Some(c.clone());
        s.set_playhead(0);
        let had = s.play.track(&c).len();
        assert!(s.delete_keyframe());
        assert_eq!(s.play.track(&c).len(), had - 1);
        s.undo();
        assert_eq!(s.play.track(&c).len(), had);
        s.set_playhead(1300);
        assert!(!s.delete_keyframe(), "no keyframe here");
        assert!(s.can_redo(), "a delete that did nothing leaves redo alone");
    }
}
