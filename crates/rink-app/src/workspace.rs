//! Root view: lays out canvas, inspector and timeline, owns keyboard actions
//! and drives playback.

use std::time::{Duration, Instant};

use std::path::PathBuf;

use gpui::{
    actions, deferred, div, prelude::*, px, rgb, App, Context, Entity, FocusHandle, Focusable, KeyBinding, Menu,
    MenuItem, PromptLevel, SharedString, SystemMenuType, Task, Window,
};

use crate::inspector::InspectorView;
use crate::library::{self, Entry, TEMPLATES};
use crate::rink_canvas::RinkCanvas;
use crate::shortcuts;
use rink_core::session::{self, AppState};
use rink_core::templates::build;
use rink_core::{io, EntityId, Vec2};

use crate::state::{snap, PlayState, Status, SNAP_MS};
use crate::theme::{self, button};
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
        ReplaySuggestion,
        StopReviewReplay,
        ResetZoom,
        Undo,
        Redo,
        JumpStart,
        JumpEnd,
        OpenLibrary,
        PickerUp,
        PickerDown,
        PickerConfirm,
        CloseDialog,
        ToggleShortcuts,
        AddDecisionPoint,
        ToggleAllLanes,
        CopyPosition,
        PastePosition,
        NextPlayer,
        PrevPlayer,
        NudgeLeft,
        NudgeRight,
        NudgeUp,
        NudgeDown,
        ToggleBackward,
        CyclePace,
        Hide,
        Quit
    ]
);

const CONTEXT: &str = "Workspace";
/// How far Alt-arrow moves the selected player, in meters.
const NUDGE_M: f32 = 0.5;
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
        KeyBinding::new("r", ReplaySuggestion, Some(CONTEXT)),
        KeyBinding::new("shift-r", StopReviewReplay, Some(CONTEXT)),
        KeyBinding::new("0", ResetZoom, Some(CONTEXT)),
        KeyBinding::new("cmd-z", Undo, Some(CONTEXT)),
        KeyBinding::new("ctrl-z", Undo, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-z", Redo, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-z", Redo, Some(CONTEXT)),
        KeyBinding::new("ctrl-y", Redo, Some(CONTEXT)),
        KeyBinding::new("cmd-0", ResetZoom, Some(CONTEXT)),
        KeyBinding::new("ctrl-s", Save, Some(CONTEXT)),
        KeyBinding::new("cmd-s", Save, Some(CONTEXT)),
        KeyBinding::new("delete", DeleteKeyframe, Some(CONTEXT)),
        KeyBinding::new("backspace", DeleteKeyframe, Some(CONTEXT)),
        KeyBinding::new("home", JumpStart, Some(CONTEXT)),
        KeyBinding::new("end", JumpEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-left", JumpStart, Some(CONTEXT)),
        KeyBinding::new("cmd-right", JumpEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-o", OpenLibrary, Some(CONTEXT)),
        KeyBinding::new("ctrl-o", OpenLibrary, Some(CONTEXT)),
        KeyBinding::new("up", PickerUp, Some(CONTEXT)),
        KeyBinding::new("down", PickerDown, Some(CONTEXT)),
        KeyBinding::new("enter", PickerConfirm, Some(CONTEXT)),
        KeyBinding::new("escape", CloseDialog, Some(CONTEXT)),
        KeyBinding::new("d", AddDecisionPoint, Some(CONTEXT)),
        KeyBinding::new("b", ToggleBackward, Some(CONTEXT)),
        KeyBinding::new("p", CyclePace, Some(CONTEXT)),
        KeyBinding::new("a", ToggleAllLanes, Some(CONTEXT)),
        KeyBinding::new("cmd-c", CopyPosition, Some(CONTEXT)),
        KeyBinding::new("ctrl-c", CopyPosition, Some(CONTEXT)),
        KeyBinding::new("cmd-v", PastePosition, Some(CONTEXT)),
        KeyBinding::new("ctrl-v", PastePosition, Some(CONTEXT)),
        KeyBinding::new("tab", NextPlayer, Some(CONTEXT)),
        KeyBinding::new("shift-tab", PrevPlayer, Some(CONTEXT)),
        KeyBinding::new("alt-left", NudgeLeft, Some(CONTEXT)),
        KeyBinding::new("alt-right", NudgeRight, Some(CONTEXT)),
        KeyBinding::new("alt-up", NudgeUp, Some(CONTEXT)),
        KeyBinding::new("alt-down", NudgeDown, Some(CONTEXT)),
        KeyBinding::new("?", ToggleShortcuts, Some(CONTEXT)),
        KeyBinding::new("shift-/", ToggleShortcuts, Some(CONTEXT)),
        KeyBinding::new("cmd-/", ToggleShortcuts, Some(CONTEXT)),
        KeyBinding::new("ctrl-/", ToggleShortcuts, Some(CONTEXT)),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &Quit, cx| {
        for window in cx.windows() {
            if let Some(ws) = window.downcast::<Workspace>() {
                let ok = ws.update(cx, |ws, window, cx| ws.save_before_closing(window, cx, true));
                if !matches!(ok, Ok(true)) {
                    return;
                }
            }
        }
        session::clear_state(std::process::id());
        cx.quit()
    });
}

/// The macOS menu bar. Without one, the menu bar keeps showing the previous
/// app's menus while this window is active. Items dispatch the same actions as
/// the keys, and show those keys.
pub fn menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Rink Replay".into(),
            items: vec![
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide Rink Replay", Hide),
                MenuItem::action("Quit Rink Replay", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![MenuItem::action("Open Play…", OpenLibrary), MenuItem::action("Save", Save)],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", Undo),
                MenuItem::action("Redo", Redo),
                MenuItem::separator(),
                MenuItem::action("Delete Keyframe at Playhead", DeleteKeyframe),
                MenuItem::action("Add Decision Point", AddDecisionPoint),
                MenuItem::separator(),
                MenuItem::action("Skate Order Backwards or Forwards", ToggleBackward),
                MenuItem::action("Change Order Pace", CyclePace),
                MenuItem::separator(),
                MenuItem::action("Copy Position", CopyPosition),
                MenuItem::action("Paste Position", PastePosition),
                MenuItem::separator(),
                MenuItem::action("Select Next Player", NextPlayer),
                MenuItem::action("Select Previous Player", PrevPlayer),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Show or Hide Ghosts", ToggleGhosts),
                MenuItem::action("Flip Ends", FlipEnds),
                MenuItem::action("Fit Rink", ResetZoom),
                MenuItem::action("All Players in Timeline", ToggleAllLanes),
            ],
        },
        Menu {
            name: "Playback".into(),
            items: vec![
                MenuItem::action("Play or Pause", TogglePlay),
                MenuItem::action("Change Speed", CycleSpeed),
                MenuItem::separator(),
                MenuItem::action("Go to Start", JumpStart),
                MenuItem::action("Go to End", JumpEnd),
                MenuItem::action("Previous Keyframe", PrevKeyframe),
                MenuItem::action("Next Keyframe", NextKeyframe),
                MenuItem::separator(),
                MenuItem::action("Previous Suggestion", PrevFeedback),
                MenuItem::action("Next Suggestion", NextFeedback),
                MenuItem::action("Loop Suggestion", ReplaySuggestion),
                MenuItem::action("Exit Suggestion Loop", StopReviewReplay),
            ],
        },
        Menu { name: "Help".into(), items: vec![MenuItem::action("Keyboard Shortcuts", ToggleShortcuts)] },
    ]
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
    /// The open play picker, if it's showing.
    picker: Option<Picker>,
    /// The keyboard shortcut sheet is showing.
    show_shortcuts: bool,
}

struct Picker {
    entries: Vec<Entry>,
    highlighted: usize,
}

impl Workspace {
    pub fn new(mut state: PlayState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let started = session::now_ms();
        if let Some(req) = session::read_request().filter(|r| {
            r.requested_at_ms + LAUNCH_REQUEST_MS >= started && state.path.as_deref() == Some(r.path.as_path())
        }) {
            focus(&mut state, req.at_ms, req.select.as_deref());
        }
        let state = cx.new(|_| state);
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        window.on_window_should_close(cx, {
            let this = cx.weak_entity();
            move |window, cx| this.update(cx, |ws, cx| ws.save_before_closing(window, cx, false)).unwrap_or(true)
        });

        // A Claude Code agent works through the `rink` CLI while the app is
        // open: poll for reviews it writes and plays it asks us to open.
        let poll = cx.spawn(async move |this, cx| loop {
            if this.update(cx, |ws, cx| ws.poll(cx)).is_err() {
                break;
            }
            cx.background_executor().timer(POLL).await;
        });

        let focus_handle = cx.focus_handle();
        Workspace {
            canvas: cx.new(|cx| RinkCanvas::new(state.clone(), window, cx)),
            toolbar: cx.new(|cx| ToolbarView::new(state.clone(), cx)),
            timeline: cx.new(|cx| TimelineView::new(state.clone(), cx)),
            inspector: cx.new(|cx| InspectorView::new(state.clone(), focus_handle.clone(), window, cx)),
            state,
            focus_handle,
            playback: None,
            _poll: poll,
            last_request_ms: started,
            published: None,
            picker: None,
            show_shortcuts: false,
        }
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.state.update(cx, |s, cx| {
            if s.reload_review_if_changed() {
                cx.notify();
            }
            if s.autosave_due(now) {
                if let Err(e) = s.autosave() {
                    s.set_error(format!("Autosave failed: {e}"));
                }
                cx.notify();
            }
        });
        if let Some(req) = session::read_request().filter(|r| r.requested_at_ms > self.last_request_ms) {
            self.last_request_ms = req.requested_at_ms;
            self.open_path(req.path, req.at_ms, req.select, cx);
        }
        self.publish_state(cx);
    }

    /// `rink open` or the picker: show another play, or reload this one after
    /// an agent changed it. Unsaved edits are never dropped: they're saved to
    /// their own play before switching, or to a copy before a reload.
    fn open_path(&mut self, path: PathBuf, at_ms: Option<u32>, select: Option<String>, cx: &mut Context<Self>) {
        let switching = self.state.read(cx).path.as_deref() != Some(path.as_path());
        let play = match io::load(&path) {
            Ok(play) => play,
            Err(e) => {
                self.edit(cx, |s| s.set_error(format!("Couldn't open the play: {e}")));
                return;
            }
        };
        self.playback = None;
        self.state.update(cx, |s, cx| {
            let kept = match keep_edits(s) {
                Ok(note) => note,
                Err(e) => {
                    s.set_error(format!("Didn't open the play: couldn't save your edits ({e})"));
                    cx.notify();
                    return;
                }
            };
            // An unchanged file we just saved our edits into: reload that.
            let play = if !switching && kept.is_empty() { io::load(&path).unwrap_or(play) } else { play };
            s.replace_play(play, path);
            focus(s, at_ms, select.as_deref());
            s.set_status(format!("{} \"{}\".{kept}", if switching { "Opened" } else { "Reloaded" }, s.play.title));
            cx.notify();
        });
        if switching {
            self.canvas.update(cx, |c, cx| c.reset_zoom(cx));
        }
    }

    /// Start a new play from a template. Unsaved edits to the open play are
    /// saved first, as when switching plays.
    fn new_from_template(&mut self, template: rink_core::templates::Template, cx: &mut Context<Self>) {
        self.playback = None;
        self.state.update(cx, |s, cx| {
            if let Err(e) = keep_edits(s) {
                s.set_error(format!("Didn't start a new play: couldn't save your edits ({e})"));
                cx.notify();
                return;
            }
            let rink = s.play.rink.clone();
            s.new_play(build(template, rink));
            s.set_status("New play. It's saved to the library as soon as you change it.");
            cx.notify();
        });
        self.canvas.update(cx, |c, cx| c.reset_zoom(cx));
    }

    fn open_library(&mut self, _: &OpenLibrary, _: &mut Window, cx: &mut Context<Self>) {
        self.show_shortcuts = false;
        if self.picker.take().is_none() {
            let entries = library::scan(&io::library_dir());
            let current = self.state.read(cx).path.clone();
            let highlighted = entries.iter().position(|e| Some(&e.path) == current.as_ref()).unwrap_or(0);
            self.picker = Some(Picker { entries, highlighted });
        }
        cx.notify();
    }

    fn close_dialog(&mut self, _: &CloseDialog, _: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_some() || std::mem::take(&mut self.show_shortcuts) {
            cx.notify();
        }
    }

    fn toggle_shortcuts(&mut self, _: &ToggleShortcuts, _: &mut Window, cx: &mut Context<Self>) {
        self.show_shortcuts = !self.show_shortcuts;
        self.picker = None;
        cx.notify();
    }

    fn picker_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.picker {
            let n = library::row_count(&p.entries) as isize;
            p.highlighted = (p.highlighted as isize + delta).rem_euclid(n) as usize;
            cx.notify();
        }
    }

    fn picker_up(&mut self, _: &PickerUp, _: &mut Window, cx: &mut Context<Self>) {
        self.picker_move(-1, cx);
    }

    fn picker_down(&mut self, _: &PickerDown, _: &mut Window, cx: &mut Context<Self>) {
        self.picker_move(1, cx);
    }

    fn picker_confirm(&mut self, _: &PickerConfirm, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &self.picker {
            let row = p.highlighted;
            self.pick(row, cx);
        }
    }

    /// Open picker row `row`: a play, or a template after the plays.
    fn pick(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(p) = self.picker.take() else { return };
        match p.entries.get(row) {
            Some(e) => self.open_path(e.path.clone(), None, None, cx),
            None => {
                if let Some((template, _)) = TEMPLATES.get(row - p.entries.len()) {
                    self.new_from_template(*template, cx);
                }
            }
        }
        cx.notify();
    }

    fn picker_overlay(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let p = self.picker.as_ref()?;
        let current = self.state.read(cx).path.clone();
        let mut list = div().id("picker-list").flex().flex_col().p_2().overflow_y_scroll();
        if p.entries.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child("No plays in the library yet. Start one from a template below."),
            );
        }
        for (i, e) in p.entries.iter().enumerate() {
            list = list.child(
                library::play_row(i, e, Some(&e.path) == current.as_ref(), i == p.highlighted)
                    .on_click(cx.listener(move |ws, _, _, cx| ws.pick(i, cx))),
            );
        }
        list = list.child(div().mx_3().my_1().h(px(1.)).bg(rgb(theme::BORDER)));
        for (j, (_, name)) in TEMPLATES.iter().enumerate() {
            let row = p.entries.len() + j;
            list = list.child(
                library::template_row(j, name, row == p.highlighted)
                    .on_click(cx.listener(move |ws, _, _, cx| ws.pick(row, cx))),
            );
        }
        let panel = library::panel(
            &io::library_dir(),
            div().flex().flex_col().flex_1().min_h_0().child(list.flex_1().min_h_0()),
        );
        Some(self.dialog(panel, cx))
    }

    /// A dialog over the window: `panel` near the top, on a dimmed backdrop
    /// that closes it when clicked.
    fn dialog(&self, panel: gpui::Div, cx: &mut Context<Self>) -> impl IntoElement {
        let backdrop = div()
            .id("dialog-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(theme::TOOLBAR_HEIGHT + 24.))
            .bg(gpui::rgba(0x0000_0066))
            .on_click(cx.listener(|ws, _, _, cx| {
                ws.picker = None;
                ws.show_shortcuts = false;
                cx.notify();
            }))
            .child(panel.id("dialog").on_click(|_, _, cx| cx.stop_propagation()));
        deferred(backdrop).with_priority(1)
    }

    /// D: a decision point at the playhead for the selected player, then
    /// straight into typing its question.
    fn add_decision_point(&mut self, _: &AddDecisionPoint, window: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.add_event(rink_core::EventKind::DecisionPoint);
        });
        self.inspector.update(cx, |i, cx| i.focus_event_note(window, cx));
    }

    fn toggle_backward(&mut self, _: &ToggleBackward, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| s.toggle_order_backward());
    }

    fn cycle_pace(&mut self, _: &CyclePace, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| s.cycle_order_pace());
    }

    fn toggle_all_lanes(&mut self, _: &ToggleAllLanes, _: &mut Window, cx: &mut Context<Self>) {
        self.timeline.update(cx, |t, cx| {
            t.all_players = !t.all_players;
            cx.notify();
        });
    }

    fn copy_position(&mut self, _: &CopyPosition, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, PlayState::copy_position);
    }

    fn paste_position(&mut self, _: &PastePosition, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, PlayState::paste_position);
    }

    fn next_player(&mut self, _: &NextPlayer, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| s.cycle_selection(1));
    }

    fn prev_player(&mut self, _: &PrevPlayer, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| s.cycle_selection(-1));
    }

    fn nudge(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.nudge(Vec2::new(dx, dy));
        });
    }

    fn nudge_left(&mut self, _: &NudgeLeft, _: &mut Window, cx: &mut Context<Self>) {
        let d = self.screen_step(cx);
        self.nudge(-d.0, -d.1, cx);
    }

    fn nudge_right(&mut self, _: &NudgeRight, _: &mut Window, cx: &mut Context<Self>) {
        let d = self.screen_step(cx);
        self.nudge(d.0, d.1, cx);
    }

    fn nudge_up(&mut self, _: &NudgeUp, _: &mut Window, cx: &mut Context<Self>) {
        let d = self.screen_step(cx);
        self.nudge(-d.1, d.0, cx);
    }

    fn nudge_down(&mut self, _: &NudgeDown, _: &mut Window, cx: &mut Context<Self>) {
        let d = self.screen_step(cx);
        self.nudge(d.1, -d.0, cx);
    }

    /// One nudge to the right on screen, in rink meters: +x, or −x when the
    /// view is flipped. Up on screen is +y (−y flipped).
    fn screen_step(&self, cx: &App) -> (f32, f32) {
        let sign = if self.state.read(cx).flipped { -1.0 } else { 1.0 };
        (NUDGE_M * sign, 0.0)
    }

    fn jump_start(&mut self, _: &JumpStart, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.set_playhead(0);
        });
    }

    fn jump_end(&mut self, _: &JumpEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.set_playhead(s.play.duration_ms);
        });
    }

    /// Save before the window closes or the app quits. Returns true when it's
    /// fine to go ahead; otherwise asks, and quits or closes on "anyway".
    pub fn save_before_closing(&mut self, window: &mut Window, cx: &mut Context<Self>, quitting: bool) -> bool {
        let result = self.state.update(cx, |s, _| keep_edits(s));
        let Err(e) = result else { return true };
        let detail = format!("Your edits couldn't be saved: {e}");
        let anyway = if quitting { "Quit Anyway" } else { "Close Anyway" };
        let answer =
            window.prompt(PromptLevel::Warning, "Lose your unsaved edits?", Some(&detail), &[anyway, "Cancel"], cx);
        cx.spawn_in(window, async move |_, cx| {
            if answer.await == Ok(0) {
                cx.update(|window, cx| {
                    if quitting {
                        session::clear_state(std::process::id());
                        cx.quit();
                    } else {
                        window.remove_window();
                    }
                })
                .ok();
            }
        })
        .detach();
        false
    }

    /// Keep the edits that are in the window over a file that changed on
    /// disk.
    fn keep_mine(&mut self, cx: &mut Context<Self>) {
        self.edit(cx, |s| match s.save() {
            Ok(_) => s.set_status("Saved your version over the file. The copy of your edits is still there."),
            Err(e) => s.set_error(format!("Save failed: {e}")),
        });
    }

    /// Drop the window's edits (they're in a copy) and load the file.
    fn use_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.state.read(cx).path.clone() else { return };
        self.state.update(cx, |s, _| s.dirty = false);
        self.open_path(path, None, None, cx);
    }

    /// The red banner under the toolbar: a conflict to resolve, or an error.
    fn banner(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let s = self.state.read(cx);
        let base = || {
            div()
                .flex()
                .items_center()
                .gap_3()
                .px_4()
                .py_2()
                .bg(gpui::rgba((theme::DANGER << 8) | 0x1f))
                .border_b_1()
                .border_color(rgb(theme::BORDER))
                .text_sm()
                .text_color(rgb(theme::TEXT))
        };
        if let Some(copy) = &s.conflict {
            let name = copy.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            return Some(
                base()
                    .child(div().flex_1().child(SharedString::from(format!(
                        "This play changed on disk while you were editing it, probably by Claude. Your edits are kept in {name}."
                    ))))
                    .child(button("keep-mine", "Keep my version", false).on_click(cx.listener(|ws, _, _, cx| ws.keep_mine(cx))))
                    .child(button("use-file", "Use the file's", false).on_click(cx.listener(|ws, _, _, cx| ws.use_file(cx)))),
            );
        }
        if !s.status.is_error() {
            return None;
        }
        Some(
            base().child(div().flex_1().child(SharedString::from(s.status.text().to_string()))).child(
                button("dismiss", "Dismiss", false)
                    .on_click(cx.listener(|ws, _, _, cx| ws.edit(cx, |s| s.status = Status::None))),
            ),
        )
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
            if !s.playing && s.review_loop.is_none() {
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
                    let playing = s.advance_playback(elapsed);
                    cx.notify();
                    playing
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
                s.set_status("No keyframe here for the selected player");
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

    fn replay_suggestion(&mut self, _: &ReplaySuggestion, window: &mut Window, cx: &mut Context<Self>) {
        let ready = self.state.update(cx, |s, cx| {
            let ready = s.prepare_review_replay();
            cx.notify();
            ready
        });
        if ready {
            self.toggle_play(&TogglePlay, window, cx);
        }
    }

    fn stop_review_replay(&mut self, _: &StopReviewReplay, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, PlayState::stop_review_loop);
    }

    /// Editing context stays next to the rink, even when the inspector is scrolled.
    fn rink_context(&self, cx: &Context<Self>) -> gpui::Div {
        let s = self.state.read(cx);
        let selected = s.selected.as_ref().map(|id| crate::theme::short_name(&s.play, id));
        let heading = match selected {
            Some(who) => format!(
                "{} · {} · {}",
                if s.playing { "Playing" } else { "Editing" },
                who,
                theme::seconds(s.playhead_ms)
            ),
            None => format!("{} · Select a player to edit", theme::seconds(s.playhead_ms)),
        };
        let mut out = div()
            .flex()
            .flex_col()
            .flex_none()
            .gap_1()
            .px_4()
            .py_2()
            .bg(rgb(theme::PANEL))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .child(div().text_sm().child(heading));
        if let Some(id) = &s.selected {
            let hint = if s.play.entity(id).is_some_and(|e| e.is_puck()) {
                "Drag to set a puck position here. Possession follows pass and recovery events."
            } else {
                "Drag: position here · Right-click: replace movement from here · Shift-right-click: extend current order"
            };
            out = out.child(div().text_xs().text_color(rgb(theme::TEXT_MUTED)).child(hint));
        }
        out
    }

    /// Persistent review transport: it never scrolls out of reach with the cards.
    fn review_controls(&self, cx: &Context<Self>) -> Option<gpui::Div> {
        let s = self.state.read(cx);
        let list = s.suggestions();
        if list.is_empty() {
            return None;
        }
        let focused = list.iter().position(|sug| Some(&sug.id) == s.focused_suggestion.as_ref());
        let label = focused.map_or_else(
            || format!("{} suggestions", list.len()),
            |i| format!("Suggestion {} of {}", i + 1, list.len()),
        );
        let mut controls = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_2()
            .child(div().text_sm().mr_2().child(label))
            .child(
                button("previous-suggestion", "← Previous", false)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(PrevFeedback), cx)),
            )
            .child(
                button("next-suggestion", "Next →", false)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(NextFeedback), cx)),
            )
            .child(
                button("replay-suggestion", "↻ Replay moment", false)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ReplaySuggestion), cx)),
            );
        if s.review_loop.is_some() {
            controls = controls.child(
                button("exit-review-loop", "Exit loop", true)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(StopReviewReplay), cx)),
            );
        }
        for (id, label, compare) in [("view-actual", "Actual", false), ("view-compare", "Compare", true)] {
            let state = self.state.clone();
            controls = controls.child(button(id, label, s.show_ghosts == compare).on_click(move |_, _, cx| {
                state.update(cx, |s, cx| {
                    s.show_ghosts = compare;
                    cx.notify();
                });
            }));
        }
        let mut details = match s.review_bounds() {
            Some((start, end)) => format!(
                "{} {}–{} · Space to {} · ",
                if s.playing { "Looping" } else { "Loop paused" },
                theme::seconds(start),
                theme::seconds(end),
                if s.playing { "pause" } else { "resume" }
            ),
            None => String::new(),
        };
        details.push_str(if s.show_ghosts { "Actual play with suggested route overlaid" } else { "Actual play only" });
        let stale = focused.is_some_and(|i| list[i].is_stale(&s.play));
        if stale {
            details.push_str(" · Play changed; ask your agent for a fresh review");
        }
        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_1p5()
                .px_4()
                .py_2()
                .bg(rgb(theme::PANEL))
                .border_b_1()
                .border_color(rgb(theme::BORDER))
                .child(controls)
                .child(
                    div().text_xs().text_color(rgb(if stale { theme::WARN } else { theme::TEXT_MUTED })).child(details),
                ),
        )
    }

    fn edit_notice(&self, cx: &Context<Self>) -> Option<gpui::Div> {
        let text = self.state.read(cx).edit_notice.clone()?;
        let state = self.state.clone();
        Some(
            div()
                .flex()
                .items_center()
                .flex_none()
                .gap_3()
                .px_4()
                .py_2()
                .bg(rgb(theme::RAISED))
                .border_b_1()
                .border_color(rgb(theme::BORDER))
                .child(div().flex_1().text_sm().child(text))
                .child(
                    button("undo-notice", "Undo", false)
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(Undo), cx)),
                )
                .child(button("dismiss-notice", "Dismiss", false).on_click(move |_, _, cx| {
                    state.update(cx, |s, cx| {
                        s.edit_notice = None;
                        cx.notify();
                    });
                })),
        )
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.undo();
        });
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| {
            s.playing = false;
            s.redo();
        });
    }

    fn reset_zoom(&mut self, _: &ResetZoom, _: &mut Window, cx: &mut Context<Self>) {
        self.canvas.update(cx, |c, cx| c.reset_zoom(cx));
    }

    fn save(&mut self, _: &Save, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |s| match s.save() {
            Ok(_) => s.set_status("Saved"),
            Err(e) => s.set_error(format!("Save failed: {e}")),
        });
    }
}

/// Before the window shows something else: save unsaved edits to their
/// play, or, if the file changed on disk meanwhile, to a copy next to it.
/// Returns a note for the status line when the edits went to a copy.
fn keep_edits(s: &mut PlayState) -> Result<String, io::IoError> {
    if s.dirty {
        s.autosave()?;
    }
    Ok(match &s.conflict {
        Some(copy) => {
            let name = copy.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            format!(" Your edits are in {name}.")
        }
        None => String::new(),
    })
}

/// Apply a request's moment and selection.
fn focus(s: &mut PlayState, at_ms: Option<u32>, select: Option<&str>) {
    if let Some(t) = at_ms {
        s.playing = false;
        s.set_playhead(t.min(s.play.duration_ms));
    }
    if let Some(id) = select.map(EntityId::from).filter(|id| s.play.entity(id).is_some()) {
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
        let banner = self.banner(cx);
        let picker = self.picker_overlay(cx).map(IntoElement::into_any_element);
        let shortcuts = self.show_shortcuts.then(|| self.dialog(shortcuts::sheet(), cx).into_any_element());
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
            .on_action(cx.listener(Self::replay_suggestion))
            .on_action(cx.listener(Self::stop_review_replay))
            .on_action(cx.listener(Self::reset_zoom))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::jump_start))
            .on_action(cx.listener(Self::jump_end))
            .on_action(cx.listener(Self::open_library))
            .on_action(cx.listener(Self::close_dialog))
            .on_action(cx.listener(Self::toggle_shortcuts))
            .on_action(cx.listener(Self::add_decision_point))
            .on_action(cx.listener(Self::toggle_backward))
            .on_action(cx.listener(Self::cycle_pace))
            .on_action(cx.listener(Self::toggle_all_lanes))
            .on_action(cx.listener(Self::copy_position))
            .on_action(cx.listener(Self::paste_position))
            .on_action(cx.listener(Self::next_player))
            .on_action(cx.listener(Self::prev_player))
            .on_action(cx.listener(Self::nudge_left))
            .on_action(cx.listener(Self::nudge_right))
            .on_action(cx.listener(Self::nudge_up))
            .on_action(cx.listener(Self::nudge_down))
            .on_action(cx.listener(Self::picker_up))
            .on_action(cx.listener(Self::picker_down))
            .on_action(cx.listener(Self::picker_confirm))
            // Clicking anywhere outside a text field ends editing in it.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|ws, _, window, _| {
                    if !ws.focus_handle.is_focused(window) {
                        window.focus(&ws.focus_handle);
                    }
                }),
            )
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme::APP_BG))
            .text_color(rgb(theme::TEXT))
            .child(self.toolbar.clone())
            .children(banner)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.rink_context(cx))
                            .children(self.review_controls(cx))
                            .children(self.edit_notice(cx))
                            .child(div().flex_1().min_h_0().child(self.canvas.clone())),
                    )
                    .child(self.inspector.clone()),
            )
            .child(self.timeline.clone())
            .children(picker)
            .children(shortcuts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use rink_core::templates::Template;
    use rink_core::{EventKind, RinkSpec};

    fn setup(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
        // The workspace reads and writes session files in the library folder.
        let dir = std::env::temp_dir().join(format!("rink-ui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var(io::LIBRARY_ENV, &dir);
        cx.update(|cx| {
            bind_keys(cx);
            crate::text_input::bind_keys(cx);
        });
        let state = PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None);
        let (ws, cx) = cx.add_window_view(|window, cx| Workspace::new(state, window, cx));
        cx.update(|window, cx| window.focus(&ws.read(cx).focus_handle));
        (ws, cx)
    }

    fn play_state(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<PlayState> {
        cx.update(|_, cx| ws.read(cx).state.clone())
    }

    fn click_button(cx: &mut VisualTestContext, selector: &'static str) {
        cx.run_until_parked();
        let bounds = cx.debug_bounds(selector).unwrap_or_else(|| panic!("missing button: {selector}"));
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    }

    #[gpui::test]
    fn review_buttons_and_shortcuts_control_playback_without_edits(cx: &mut TestAppContext) {
        let (ws, cx) = setup(cx);
        let state = play_state(&ws, cx);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/breakout.rink.json");
        let play = io::load(&path).unwrap();
        let original = play.clone();
        let review = rink_core::review::load_review(&path).unwrap();
        cx.update(|_, cx| {
            state.update(cx, |s, cx| {
                *s = PlayState::new(play, None);
                s.review = review;
                cx.notify();
            })
        });
        cx.simulate_resize(gpui::size(px(1280.), px(800.)));

        click_button(cx, "next-suggestion");
        let first = cx.update(|_, cx| state.read(cx).focused_suggestion.clone());
        assert!(first.is_some());
        click_button(cx, "replay-suggestion");
        assert!(cx.update(|_, cx| {
            let s = state.read(cx);
            s.playing && s.review_loop == first
        }));
        // Keep every control reachable when the review bar wraps on a smaller window.
        for width in [1024.0, 1280.0] {
            cx.simulate_resize(gpui::size(px(width), px(800.)));
            cx.run_until_parked();
            for selector in [
                "previous-suggestion",
                "next-suggestion",
                "replay-suggestion",
                "exit-review-loop",
                "view-actual",
                "view-compare",
            ] {
                let bounds = cx.debug_bounds(selector).expect("review control rendered");
                assert!(
                    bounds.left() >= px(0.) && bounds.right() <= px(width - theme::INSPECTOR_WIDTH),
                    "{selector} must fit beside the inspector at {width}px: {bounds:?}"
                );
            }
        }
        click_button(cx, "view-actual");
        assert!(cx.update(|_, cx| {
            let s = state.read(cx);
            !s.show_ghosts && s.playing
        }));
        click_button(cx, "view-compare");
        assert!(cx.update(|_, cx| state.read(cx).show_ghosts));

        cx.simulate_keystrokes("space");
        assert!(cx.update(|_, cx| {
            let s = state.read(cx);
            !s.playing && s.review_loop.is_some()
        }));
        cx.simulate_keystrokes("space");
        assert!(cx.update(|_, cx| state.read(cx).playing));
        click_button(cx, "exit-review-loop");
        assert!(cx.update(|_, cx| {
            let s = state.read(cx);
            !s.playing && s.review_loop.is_none()
        }));

        cx.simulate_keystrokes("r");
        assert!(cx.update(|_, cx| state.read(cx).playing));
        cx.simulate_keystrokes("shift-r");
        assert!(cx.update(|_, cx| state.read(cx).review_loop.is_none()));
        click_button(cx, "next-suggestion");
        assert_ne!(cx.update(|_, cx| state.read(cx).focused_suggestion.clone()), first);
        click_button(cx, "previous-suggestion");
        assert_eq!(cx.update(|_, cx| state.read(cx).focused_suggestion.clone()), first);
        cx.update(|_, cx| {
            let s = state.read(cx);
            assert_eq!(s.play, original);
            assert!(!s.dirty && !s.can_undo());
        });
    }

    #[gpui::test]
    fn route_replacement_notice_undoes_the_whole_order(cx: &mut TestAppContext) {
        let (ws, cx) = setup(cx);
        let state = play_state(&ws, cx);
        let original = cx.update(|_, cx| state.read(cx).play.clone());
        cx.update(|_, cx| {
            state.update(cx, |s, cx| {
                s.set_playhead(1000);
                s.order_to(Vec2::new(10.0, 5.0), false);
                cx.notify();
            })
        });
        click_button(cx, "undo-notice");
        cx.update(|_, cx| {
            let s = state.read(cx);
            assert_eq!(s.play, original);
            assert!(s.edit_notice.is_none());
        });
    }

    #[gpui::test]
    fn typing_in_a_field_types_instead_of_running_shortcuts(cx: &mut TestAppContext) {
        let (ws, cx) = setup(cx);
        let state = play_state(&ws, cx);

        // D: a decision point at the playhead, and its note takes the typing.
        cx.simulate_keystrokes("d");
        cx.simulate_input("fgn0");
        let (note, flipped, ghosts) = cx.update(|_, cx| {
            let s = state.read(cx);
            let i = s.selected_event.unwrap();
            assert_eq!(s.play.events[i].kind, EventKind::DecisionPoint);
            (s.play.events[i].note.clone(), s.flipped, s.show_ghosts)
        });
        assert_eq!(note, "fgn0");
        assert!(!flipped && ghosts, "F and G typed, not run");

        // Cmd-Enter ends editing; then F is a shortcut again.
        cx.simulate_keystrokes("cmd-enter f");
        assert!(cx.update(|_, cx| state.read(cx).flipped));

        // The note is one undo step, the new event another.
        let before = cx.update(|_, cx| state.read(cx).play.events.len());
        cx.simulate_keystrokes("cmd-z");
        let notes = cx.update(|_, cx| {
            let s = state.read(cx);
            s.play.events.iter().filter(|e| e.kind == EventKind::DecisionPoint && e.note.is_empty()).count()
        });
        assert_eq!(cx.update(|_, cx| state.read(cx).play.events.len()), before, "the event stays");
        assert!(notes >= 1, "its note is gone");
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(cx.update(|_, cx| state.read(cx).play.events.len()), before - 1);
    }

    #[gpui::test]
    fn escape_restores_the_field(cx: &mut TestAppContext) {
        let (ws, cx) = setup(cx);
        let state = play_state(&ws, cx);
        cx.simulate_keystrokes("d");
        cx.simulate_input("oops");
        cx.simulate_keystrokes("escape");
        let note = cx.update(|_, cx| {
            let s = state.read(cx);
            s.play.events[s.selected_event.unwrap()].note.clone()
        });
        assert_eq!(note, "");
    }

    #[gpui::test]
    fn dialogs_open_and_close_from_the_keyboard(cx: &mut TestAppContext) {
        let (ws, cx) = setup(cx);
        cx.simulate_keystrokes("cmd-o");
        assert!(cx.update(|_, cx| ws.read(cx).picker.is_some()));
        cx.simulate_keystrokes("escape");
        assert!(cx.update(|_, cx| ws.read(cx).picker.is_none()));
        cx.simulate_keystrokes("cmd-/");
        assert!(cx.update(|_, cx| ws.read(cx).show_shortcuts));
        cx.simulate_keystrokes("escape");
        assert!(!cx.update(|_, cx| ws.read(cx).show_shortcuts));
    }

    #[gpui::test]
    fn right_click_gives_an_order_and_keys_change_it(cx: &mut TestAppContext) {
        let (ws, cx) = setup(cx);
        let state = play_state(&ws, cx);
        let me = cx.update(|_, cx| state.read(cx).play.me().unwrap().id.clone());
        cx.run_until_parked();
        cx.simulate_mouse_down(gpui::point(px(300.), px(250.)), gpui::MouseButton::Right, gpui::Modifiers::none());
        let order = |cx: &mut VisualTestContext| cx.update(|_, cx| state.read(cx).play.orders(&me).first().cloned());
        let given = order(cx).expect("right-click gives the selected player an order");
        let on_ice = cx.update(|_, cx| state.read(cx).play.rink.contains(given.to[0]));
        assert!(on_ice, "{:?}", given.to);
        cx.simulate_keystrokes("p b");
        let changed = order(cx).unwrap();
        assert_eq!((changed.pace, changed.backward), (rink_core::Pace::Sprint, true));
    }

    #[gpui::test]
    fn keyboard_editing_moves_the_selection(cx: &mut TestAppContext) {
        let (ws, cx) = setup(cx);
        let state = play_state(&ws, cx);
        let me = cx.update(|_, cx| state.read(cx).play.me().unwrap().id.clone());
        let at = |cx: &mut VisualTestContext| cx.update(|_, cx| state.read(cx).snapshot().entity(&me).unwrap().pos);
        let before = at(cx);
        cx.simulate_keystrokes("alt-right alt-up");
        let moved = at(cx) - before;
        assert!((moved - Vec2::new(NUDGE_M, NUDGE_M)).length() < 1e-3, "{moved}");
        cx.simulate_keystrokes("tab");
        assert_ne!(cx.update(|_, cx| state.read(cx).selected.clone()), Some(me));
    }
}
