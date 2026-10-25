//! Root view: lays out canvas, inspector and timeline, owns keyboard actions
//! and drives playback.

use std::time::{Duration, Instant};

use gpui::{actions, div, prelude::*, rgb, App, Context, Entity, FocusHandle, Focusable, KeyBinding, Task, Window};

use crate::inspector::InspectorView;
use crate::rink_canvas::RinkCanvas;
use rink_core::session::{self, AppState, OpenRequest};
use rink_core::{io, EntityId};

use crate::state::{snap, PlayState, SNAP_MS};
use crate::theme;
use crate::timeline::TimelineView;
use crate::toolbar::ToolbarView;

actions!(
    rink_replay,
    [
        TogglePlay,
        StepForward,
        StepBack,
        NextKeyframe,
        PrevKeyframe,
        CycleSpeed,
        FlipEnds,
        Save,
        DeleteKeyframe,
        NextFeedback,
        PrevFeedback,
        ToggleGhosts,
        ResetZoom,
        Quit
    ]
);

const CONTEXT: &str = "Workspace";
const FRAME: Duration = Duration::from_millis(16);
/// How often to look for a new or updated review file and for `rink open`
/// requests.
const POLL: Duration = Duration::from_millis(750);
/// A request this recent naming the play we were launched with is the one
/// that launched us: apply its moment and selection.
const LAUNCH_REQUEST_MS: u64 = 10_000;

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("space", TogglePlay, Some(CONTEXT)),
        KeyBinding::new("right", StepForward, Some(CONTEXT)),
        KeyBinding::new("left", StepBack, Some(CONTEXT)),
        KeyBinding::new("shift-right", NextKeyframe, Some(CONTEXT)),
        KeyBinding::new("shift-left", PrevKeyframe, Some(CONTEXT)),
        KeyBinding::new("]", CycleSpeed, Some(CONTEXT)),
        KeyBinding::new("f", FlipEnds, Some(CONTEXT)),
        KeyBinding::new("n", NextFeedback, Some(CONTEXT)),
        KeyBinding::new("shift-n", PrevFeedback, Some(CONTEXT)),
        KeyBinding::new("g", ToggleGhosts, Some(CONTEXT)),
        KeyBinding::new("0", ResetZoom, Some(CONTEXT)),
        KeyBinding::new("cmd-0", ResetZoom, Some(CONTEXT)),
        KeyBinding::new("ctrl-s", Save, Some(CONTEXT)),
        KeyBinding::new("cmd-s", Save, Some(CONTEXT)),
        KeyBinding::new("delete", DeleteKeyframe, Some(CONTEXT)),
        KeyBinding::new("backspace", DeleteKeyframe, Some(CONTEXT)),
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    cx.on_action(|_: &Quit, cx| {
        session::clear_state(std::process::id());
        cx.quit()
    });
}

pub struct Workspace {
    state: Entity<PlayState>,
    canvas: Entity<RinkCanvas>,
    toolbar: Entity<ToolbarView>,
    timeline: Entity<TimelineView>,
    inspector: Entity<InspectorView>,
    focus_handle: FocusHandle,
    playback: Option<Task<()>>,
    _poll: Task<()>,
    /// The newest `rink open` request already handled.
    last_request_ms: u64,
    /// What we last wrote for `rink current`.
    published: Option<AppState>,
}

impl Workspace {
    pub fn new(mut state: PlayState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let started = session::now_ms();
        if let Some(req) = session::read_request().filter(|r| {
            r.requested_at_ms + LAUNCH_REQUEST_MS >= started && state.path.as_deref() == Some(r.path.as_path())
        }) {
            focus(&mut state, &req);
        }
        let state = cx.new(|_| state);

        // A Claude Code agent works through the `rink` CLI while the app is
        // open: poll for reviews it writes and plays it asks us to open.
        let poll = cx.spawn(async move |this, cx| loop {
            if this.update(cx, |ws, cx| ws.poll(cx)).is_err() {
                break;
            }
            cx.background_executor().timer(POLL).await;
        });

        Workspace {
            canvas: cx.new(|cx| RinkCanvas::new(state.clone(), window, cx)),
            toolbar: cx.new(|cx| ToolbarView::new(state.clone(), cx)),
            timeline: cx.new(|cx| TimelineView::new(state.clone(), cx)),
            inspector: cx.new(|cx| InspectorView::new(state.clone(), cx)),
            state,
            focus_handle: cx.focus_handle(),
            playback: None,
            _poll: poll,
            last_request_ms: started,
            published: None,
        }
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            if s.reload_review_if_changed() {
                cx.notify();
            }
        });
        if let Some(req) = session::read_request().filter(|r| r.requested_at_ms > self.last_request_ms) {
            self.last_request_ms = req.requested_at_ms;
            self.open_requested(req, cx);
        }
        self.publish_state(cx);
    }

    /// `rink open`: show another play, or reload this one after an agent
    /// changed it. Unsaved edits are never dropped: they're saved to their own
    /// play before switching, or to a copy before a reload.
    fn open_requested(&mut self, req: OpenRequest, cx: &mut Context<Self>) {
        let switching = self.state.read(cx).path.as_deref() != Some(req.path.as_path());
        let play = match io::load(&req.path) {
            Ok(play) => play,
            Err(e) => {
                self.edit(cx, |s| s.status = format!("Couldn't open the requested play: {e}"));
                return;
            }
        };
        self.playback = None;
        self.state.update(cx, |s, cx| {
            let mut kept = String::new();
            if s.dirty {
                let saved = if switching {
                    s.save().map(|_| " Your edits to the previous play were saved.".to_string())
                } else {
                    s.backup_edits().map(|p| {
                        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        format!(" Your unsaved edits are in {name}.")
                    })
                };
                match saved {
                    Ok(note) => kept = note,
                    Err(e) => {
                        s.status = format!("Didn't open the requested play: couldn't save your edits ({e})");
                        cx.notify();
                        return;
                    }
                }
            }
            s.replace_play(play, req.path.clone());
            focus(s, &req);
            s.status = format!("{} \"{}\".{kept}", if switching { "Opened" } else { "Reloaded" }, s.play.title);
            cx.notify();
        });
        if switching {
            self.canvas.update(cx, |c, cx| c.reset_zoom(cx));
        }
    }

    /// Tell `rink current` what the window shows, when that changes.
    fn publish_state(&mut self, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        let now = AppState {
            pid: std::process::id(),
            path: s.path.clone(),
            title: s.play.title.clone(),
            playhead_ms: s.playhead_ms,
            selected: s.selected.as_ref().map(|id| id.to_string()),
            dirty: s.dirty,
        };
        if self.published.as_ref() != Some(&now) && session::write_state(&now).is_ok() {
            self.published = Some(now);
        }
    }

    fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut PlayState)) {
        self.state.update(cx, |s, cx| {
            f(s);
            cx.notify();
        });
    }

    fn toggle_play(&mut self, _: &TogglePlay, window: &mut Window, cx: &mut Context<Self>) {
        let playing = self.state.update(cx, |s, cx| {
            if !s.playing && s.playhead_ms >= s.play.duration_ms {
                s.playhead_ms = 0;
            }
            s.playing = !s.playing;
            if !s.playing {
                s.playhead_ms = snap(s.playhead_ms);
            }
            cx.notify();
            s.playing
        });
        if !playing {
            self.playback = None;
            return;
        }

        let state = self.state.clone();
        self.playback = Some(cx.spawn_in(window, async move |_, cx| {
            let mut last = Instant::now();
            loop {
                cx.background_executor().timer(FRAME).await;
                let now = Instant::now();
                let elapsed = now - last;
                last = now;
                let keep_going = state.update(cx, |s, cx| {
                    if !s.playing {
                        return false;
                    }
                    let advance = (elapsed.as_secs_f32() * 1000.0 * s.speed) as u32;
                    s.playhead_ms = (s.playhead_ms + advance).min(s.play.duration_ms);
                    if s.playhead_ms >= s.play.duration_ms {
                        s.playing = false;
                    }
                    cx.notify();
                    s.playing
                });
                if !matches!(keep_going, Ok(true)) {
                    break;
                }
            }
        }));
    }

    fn step_forward(&mut self, _: &StepForward, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.step(SNAP_MS as i64);
        });
    }

    fn step_back(&mut self, _: &StepBack, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.step(-(SNAP_MS as i64));
        });
    }

    fn next_keyframe(&mut self, _: &NextKeyframe, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.jump_keyframe(1);
        });
    }

    fn prev_keyframe(&mut self, _: &PrevKeyframe, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.jump_keyframe(-1);
        });
    }

    fn cycle_speed(&mut self, _: &CycleSpeed, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, PlayState::cycle_speed);
    }

    fn flip_ends(&mut self, _: &FlipEnds, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| s.flipped = !s.flipped);
    }

    fn delete_keyframe(&mut self, _: &DeleteKeyframe, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            if !s.delete_keyframe() {
                s.status = "No keyframe here for the selected player".into();
            }
        });
    }

    fn next_feedback(&mut self, _: &NextFeedback, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.jump_suggestion(1);
        });
    }

    fn prev_feedback(&mut self, _: &PrevFeedback, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.jump_suggestion(-1);
        });
    }

    fn toggle_ghosts(&mut self, _: &ToggleGhosts, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| s.show_ghosts = !s.show_ghosts);
    }

    fn reset_zoom(&mut self, _: &ResetZoom, _: &mut Window, cx: &mut Context<Self>) {
        self.canvas.update(cx, |c, cx| c.reset_zoom(cx));
    }

    fn save(&mut self, _: &Save, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.status = match s.save() {
                Ok(_) => "Saved".into(),
                Err(e) => format!("Save failed: {e}"),
            };
        });
    }
}

/// Apply a request's moment and selection.
fn focus(s: &mut PlayState, req: &OpenRequest) {
    if let Some(t) = req.at_ms {
        s.playing = false;
        s.set_playhead(t.min(s.play.duration_ms));
    }
    if let Some(id) = req.select.as_deref().map(EntityId::from).filter(|id| s.play.entity(id).is_some()) {
        s.selected = Some(id);
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::toggle_play))
            .on_action(cx.listener(Self::step_forward))
            .on_action(cx.listener(Self::step_back))
            .on_action(cx.listener(Self::next_keyframe))
            .on_action(cx.listener(Self::prev_keyframe))
            .on_action(cx.listener(Self::cycle_speed))
            .on_action(cx.listener(Self::flip_ends))
            .on_action(cx.listener(Self::delete_keyframe))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::next_feedback))
            .on_action(cx.listener(Self::prev_feedback))
            .on_action(cx.listener(Self::toggle_ghosts))
            .on_action(cx.listener(Self::reset_zoom))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme::APP_BG))
            .text_color(rgb(theme::TEXT))
            .child(self.toolbar.clone())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex_1().h_full().child(self.canvas.clone()))
                    .child(self.inspector.clone()),
            )
            .child(self.timeline.clone())
    }
}
