//! Root view: lays out canvas, inspector and timeline, owns keyboard actions
//! and drives playback.

use std::time::{Duration, Instant};

use gpui::{actions, div, prelude::*, rgb, App, Context, Entity, FocusHandle, Focusable, KeyBinding, Task, Window};

use crate::inspector::InspectorView;
use crate::rink_canvas::RinkCanvas;
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
/// How often to look for a new or updated review file.
const REVIEW_POLL: Duration = Duration::from_millis(750);

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
    cx.on_action(|_: &Quit, cx| cx.quit());
}

pub struct Workspace {
    state: Entity<PlayState>,
    canvas: Entity<RinkCanvas>,
    toolbar: Entity<ToolbarView>,
    timeline: Entity<TimelineView>,
    inspector: Entity<InspectorView>,
    focus_handle: FocusHandle,
    playback: Option<Task<()>>,
    _review_watch: Task<()>,
}

impl Workspace {
    pub fn new(state: PlayState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| state);

        // A Claude Code agent writes reviews with the `rink` CLI while the app
        // is open; poll the review file so feedback shows up as it lands.
        let watched = state.clone();
        let review_watch = cx.spawn(async move |_, cx| loop {
            let alive = watched.update(cx, |s, cx| {
                if s.reload_review_if_changed() {
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
            cx.background_executor().timer(REVIEW_POLL).await;
        });

        Workspace {
            canvas: cx.new(|cx| RinkCanvas::new(state.clone(), window, cx)),
            toolbar: cx.new(|cx| ToolbarView::new(state.clone(), cx)),
            timeline: cx.new(|cx| TimelineView::new(state.clone(), cx)),
            inspector: cx.new(|cx| InspectorView::new(state.clone(), cx)),
            state,
            focus_handle: cx.focus_handle(),
            playback: None,
            _review_watch: review_watch,
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
                s.status = "No keyframe for the selection at the playhead.".into();
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
                Ok(path) => format!("Saved {}", path.display()),
                Err(e) => format!("Save failed: {e}"),
            };
        });
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
