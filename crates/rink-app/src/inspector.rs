//! Right-hand panel, top to bottom:
//!
//! - the play: its notes, and (Edit) its title, notes, team names and length;
//! - the selection: exact state at the playhead, number, "this is me", and
//!   taking a player off or putting them on the ice;
//! - position feedback;
//! - events: click one to jump there and edit it (kind, players, note), or
//!   add one at the playhead;
//! - validation warnings.

use gpui::{div, prelude::*, px, rgb, Context, Div, Entity, FocusHandle, Focusable, FontWeight, SharedString, Window};
use rink_core::{EntityId, EventKind, Play, TeamSide};

use crate::state::PlayState;
use crate::text_input::{InputEvent, TextInput};
use crate::theme::{self, badge, card, chip, role_name, seconds, section, short_name};

pub struct InspectorView {
    state: Entity<PlayState>,
    notes_expanded: bool,
    editing_play: bool,
    adding_event: bool,
    /// A field just got focus: its first change records one undo step.
    fresh: bool,
    title: Entity<TextInput>,
    notes: Entity<TextInput>,
    home: Entity<TextInput>,
    away: Entity<TextInput>,
    number: Entity<TextInput>,
    event_note: Entity<TextInput>,
}

/// A text field whose changes go straight into the play through `apply`.
fn field(
    placeholder: &'static str,
    multiline: bool,
    undo_label: &'static str,
    apply: fn(&mut PlayState, &str),
    return_focus: &FocusHandle,
    window: &mut Window,
    cx: &mut Context<InspectorView>,
) -> Entity<TextInput> {
    let input = cx.new(|cx| TextInput::new(placeholder, multiline, return_focus.clone(), window, cx));
    cx.subscribe(&input, move |this, _, ev: &InputEvent, cx| match ev {
        InputEvent::Focused => this.fresh = true,
        InputEvent::Changed(text) => {
            let first = std::mem::take(&mut this.fresh);
            this.state.update(cx, |s, cx| {
                if first {
                    s.checkpoint(undo_label);
                }
                apply(s, text);
                cx.notify();
            });
        }
    })
    .detach();
    input
}

impl InspectorView {
    pub fn new(
        state: Entity<PlayState>,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let f = &return_focus;
        InspectorView {
            state,
            notes_expanded: false,
            editing_play: false,
            adding_event: false,
            fresh: false,
            title: field("Title", false, "title edit", |s, t| s.set_title(t), f, window, cx),
            notes: field(
                "What happened, and what you want judged",
                true,
                "notes edit",
                |s, t| s.set_notes(t),
                f,
                window,
                cx,
            ),
            home: field("Us", false, "team name edit", |s, t| s.set_team_name(TeamSide::Home, t), f, window, cx),
            away: field("Them", false, "team name edit", |s, t| s.set_team_name(TeamSide::Away, t), f, window, cx),
            number: field(
                "#",
                false,
                "number change",
                |s, t| {
                    if let Some(id) = s.selected.clone() {
                        s.set_number(&id, t)
                    }
                },
                f,
                window,
                cx,
            ),
            event_note: field(
                "Note: what happened, or your question",
                true,
                "event note edit",
                |s, t| {
                    if let Some(i) = s.selected_event {
                        s.set_event_note(i, t)
                    }
                },
                f,
                window,
                cx,
            ),
        }
    }

    /// Start typing the selected event's note (after adding a decision point).
    pub fn focus_event_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.event_note.focus_handle(cx));
    }

    /// Show the play's current values in fields the user isn't typing in.
    fn sync_fields(&mut self, window: &Window, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        let title = s.play.title.clone();
        let notes = s.play.notes.clone();
        let home = s.play.teams[0].name.clone();
        let away = s.play.teams[1].name.clone();
        let number = s
            .selected
            .as_ref()
            .and_then(|id| s.play.entity(id))
            .and_then(|e| e.number)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let note = s.selected_event.and_then(|i| s.play.events.get(i)).map(|e| e.note.clone()).unwrap_or_default();
        for (input, text) in [
            (&self.title, title),
            (&self.notes, notes),
            (&self.home, home),
            (&self.away, away),
            (&self.number, number),
            (&self.event_note, note),
        ] {
            input.update(cx, |i, _| i.sync(&text, window));
        }
    }
}

/// Notes longer than this are cut off until "More" is clicked.
const NOTES_PREVIEW_CHARS: usize = 220;

fn muted(text: impl Into<SharedString>) -> Div {
    div().text_xs().text_color(rgb(theme::TEXT_MUTED)).child(text.into())
}

fn stat(label: &'static str, value: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .child(div().text_xs().text_color(rgb(theme::TEXT_FAINT)).child(label))
        .child(div().text_sm().text_color(rgb(theme::TEXT)).child(value.into()))
}

/// A round chip in the team's color with the player's number, like on the rink.
fn player_chip(text: impl Into<SharedString>, hex: u32, me: bool) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(34.))
        .rounded_full()
        .bg(rgb(hex))
        .border_2()
        .border_color(rgb(if me { theme::ME } else { 0xffffff }))
        .text_sm()
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(0xffffff))
        .child(text.into())
}

fn play_section(this: &InspectorView, s: &PlayState, cx: &Context<InspectorView>) -> Div {
    let toggle = div()
        .id("edit-play")
        .text_xs()
        .text_color(rgb(theme::ACCENT))
        .cursor_pointer()
        .child(if this.editing_play { "Done" } else { "Edit" })
        .on_click(cx.listener(|this, _, _, cx| {
            this.editing_play = !this.editing_play;
            cx.notify();
        }));
    let mut out = div()
        .flex()
        .flex_col()
        .gap_2()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(theme::BORDER))
        .child(div().flex().justify_between().child(theme::section_label("Play")).child(toggle));

    if this.editing_play {
        let step = |id: &'static str, label: &'static str, delta: i64| {
            chip(id, label, false).on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.change_duration(delta);
                    cx.notify();
                })
            }))
        };
        return out
            .child(labelled("Title", this.title.clone()))
            .child(labelled("Notes", this.notes.clone()))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(labelled("Us", this.home.clone()).flex_1())
                    .child(labelled("Them", this.away.clone()).flex_1()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().text_xs().text_color(rgb(theme::TEXT_FAINT)).child("Length"))
                    .child(step("shorter", "−0.5s", -500))
                    .child(
                        div().w(px(44.)).text_sm().text_center().child(SharedString::from(seconds(s.play.duration_ms))),
                    )
                    .child(step("longer", "+0.5s", 500)),
            );
    }

    let text = s.play.notes.trim();
    if text.is_empty() {
        return out.child(muted("No notes yet. Click Edit to describe the play and what you want judged."));
    }
    let long = text.chars().count() > NOTES_PREVIEW_CHARS;
    let shown = if long && !this.notes_expanded {
        let cut: String = text.chars().take(NOTES_PREVIEW_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        text.to_string()
    };
    out = out.child(div().text_sm().text_color(rgb(theme::TEXT_MUTED)).child(SharedString::from(shown)));
    if long {
        out = out.child(
            div()
                .id("notes-more")
                .text_xs()
                .text_color(rgb(theme::ACCENT))
                .cursor_pointer()
                .child(if this.notes_expanded { "Less" } else { "More" })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notes_expanded = !this.notes_expanded;
                    cx.notify();
                })),
        );
    }
    out
}

/// A small caption above a field.
fn labelled(label: &'static str, input: Entity<TextInput>) -> Div {
    div().flex().flex_col().gap_1().child(div().text_xs().text_color(rgb(theme::TEXT_FAINT)).child(label)).child(input)
}

/// A state update from a click.
fn on_state(
    state: &Entity<PlayState>,
    f: impl Fn(&mut PlayState) + 'static,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static {
    let state = state.clone();
    move |_, _, cx| {
        state.update(cx, |s, cx| {
            f(s);
            cx.notify();
        })
    }
}

fn selection(this: &InspectorView, s: &PlayState) -> Div {
    let play = &s.play;
    let snap = s.snapshot();
    let Some(e) = s.selected.as_ref().and_then(|id| play.entity(id)) else {
        return section("Selection").child(muted("Click a player or the puck to inspect it."));
    };

    if e.is_puck() {
        let carrier = match &snap.puck.carrier {
            Some(id) => format!("Carried by {}", short_name(play, id)),
            None => "Loose".into(),
        };
        return section("Selection").child(
            card()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(player_chip("", 0x111111, false))
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("Puck")),
                )
                .child(
                    div()
                        .flex()
                        .pt_2()
                        .child(stat("Position", format!("{:.1}, {:.1} m", snap.puck.pos.x, snap.puck.pos.y)))
                        .child(stat("State", carrier)),
                ),
        );
    }

    let team = e.team.map(|t| play.team(t));
    let color = team.map(|t| t.color_rgb()).unwrap_or(0x808080);
    let title = match (e.number, e.role) {
        (Some(n), Some(r)) => format!("#{n} · {}", role_name(r)),
        _ => short_name(play, &e.id),
    };
    let mut header = div()
        .flex()
        .items_center()
        .gap_3()
        .child(player_chip(e.number.map(|n| n.to_string()).unwrap_or_default(), color, e.is_me))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(SharedString::from(title)))
                .child(muted(team.map(|t| t.name.clone()).unwrap_or_default())),
        );
    if e.is_me {
        header = header.child(badge("You", theme::ME));
    }

    let keys = play.track(&e.id);
    let here = keys.iter().any(|k| k.t_ms == s.playhead_ms);
    let id = e.id.clone();
    let mut actions = div().flex().items_center().gap_2().pt_2().child(div().w(px(52.)).child(this.number.clone()));
    if !e.is_me {
        let id = id.clone();
        actions =
            actions.child(chip("make-me", "This is me", false).on_click(on_state(&this.state, move |s| s.set_me(&id))));
    }
    let mut body = card().child(header);
    match snap.entity(&e.id) {
        Some(es) => {
            body = body
                .child(
                    div()
                        .flex()
                        .pt_2()
                        .child(stat("Position", format!("{:.1}, {:.1} m", es.pos.x, es.pos.y)))
                        .child(stat("Speed", format!("{:.1} m/s", es.speed_mps))),
                )
                .child(
                    div()
                        .flex()
                        .pt_1()
                        .child(stat("Facing", format!("{:.0}°", es.facing.to_degrees())))
                        .child(stat("Keyframes", format!("{}{}", keys.len(), if here { " · one here" } else { "" }))),
                );
            if es.has_puck {
                body = body.child(div().pt_1().child(badge("Has the puck", theme::TEXT_MUTED)));
            }
            actions = actions.child(
                chip("off-ice", "Take off the ice", false)
                    .on_click(on_state(&this.state, move |s| s.take_off_ice(&id))),
            );
        }
        None => {
            body = body.child(div().pt_1().child(muted("Not on the ice.")));
            if let (Some(side), Some(role)) = (e.team, e.role) {
                actions = actions.child(
                    chip("on-ice", "Put on the ice", false)
                        .on_click(on_state(&this.state, move |s| s.put_on_ice(side, role))),
                );
            }
        }
    }
    section("Selection").child(body.child(actions))
}

/// Roster spots nobody fills on the ice; click one to put them on.
fn off_ice(this: &InspectorView, s: &PlayState) -> Option<Div> {
    let spots = s.off_ice();
    if spots.is_empty() {
        return None;
    }
    let mut chips = div().flex().flex_wrap().gap_1p5();
    for (i, (side, role, id)) in spots.into_iter().enumerate() {
        let name = match s.play.entity(&id) {
            Some(_) => short_name(&s.play, &id),
            None => role.as_str().to_string(),
        };
        let team = if side == TeamSide::Home { "Us" } else { "Them" };
        chips = chips.child(
            chip(("put-on", i), format!("{team} {name}"), false)
                .on_click(on_state(&this.state, move |s| s.put_on_ice(side, role))),
        );
    }
    Some(section("Off the ice").child(muted("Click to put them on the ice near the puck.")).child(chips))
}

fn feedback(s: &PlayState, cx: &Context<InspectorView>) -> Option<Div> {
    let play = &s.play;
    let suggestions = s.suggestions();
    if suggestions.is_empty() {
        return None;
    }
    let active: Vec<&str> = s.active_suggestions().map(|(sug, _)| sug.id.as_str()).collect();
    let mut out = section(format!("Position feedback · {}", suggestions.len()));
    if !s.show_ghosts {
        out = out.child(muted("Ghosts are hidden. Press G to show them."));
    }
    for sug in suggestions {
        let on = active.contains(&sug.id.as_str()) || s.focused_suggestion.as_deref() == Some(sug.id.as_str());
        let mut meta = format!("{} · {}", seconds(sug.t_ms), short_name(play, &sug.entity));
        if let Some(m) = sug.miss_distance(play) {
            meta.push_str(&format!(" · {m:.1} m off"));
        }
        let mut head = div().flex().items_center().justify_between().child(muted(meta));
        if sug.is_stale(play) {
            head = head.child(badge("Play edited since", theme::TEXT_MUTED));
        }
        let id = sug.id.clone();
        let mut item = card()
            .id(SharedString::from(format!("suggestion-{}", sug.id)))
            .cursor_pointer()
            .hover(|st| st.bg(rgb(theme::HOVER)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.focus_suggestion(&id);
                    cx.notify();
                })
            }))
            .border_l_2()
            .border_color(rgb(if on { theme::FEEDBACK } else { theme::BORDER }))
            .child(head)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(if on { theme::TEXT } else { theme::TEXT_MUTED }))
                    .child(SharedString::from(capitalize(&sug.principle))),
            );
        if on {
            item =
                item.child(div().text_sm().text_color(rgb(theme::TEXT)).child(SharedString::from(sug.reason.clone())));
            if !sug.detail.is_empty() {
                item = item.child(muted(sug.detail.clone()));
            }
        }
        out = out.child(item);
    }
    Some(out)
}

fn events(this: &InspectorView, s: &PlayState, cx: &Context<InspectorView>) -> Div {
    let play: &Play = &s.play;
    let add = div()
        .id("add-event")
        .text_xs()
        .text_color(rgb(theme::ACCENT))
        .cursor_pointer()
        .child(if this.adding_event { "Cancel" } else { "+ Add" })
        .on_click(cx.listener(|this, _, _, cx| {
            this.adding_event = !this.adding_event;
            cx.notify();
        }));
    let mut out = div()
        .flex()
        .flex_col()
        .gap_2()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(theme::BORDER))
        .child(div().flex().justify_between().child(theme::section_label("Events")).child(add));
    if this.adding_event {
        let who = s
            .selected
            .as_ref()
            .filter(|id| play.entity(id).is_some_and(|e| !e.is_puck()))
            .map(|id| format!(", by {}", short_name(play, id)))
            .unwrap_or_default();
        let mut kinds = div().flex().flex_wrap().gap_1p5();
        for (i, kind) in EventKind::ALL.into_iter().enumerate() {
            kinds = kinds.child(chip(("add-kind", i), kind.label(), false).on_click(cx.listener(
                move |this, _, window, cx| {
                    this.adding_event = false;
                    this.state.update(cx, |s, cx| {
                        s.add_event(kind);
                        cx.notify();
                    });
                    if kind == EventKind::DecisionPoint {
                        this.focus_event_note(window, cx);
                    }
                },
            )));
        }
        out = out.child(muted(format!("At {}{who}:", seconds(s.playhead_ms)))).child(kinds);
    }
    if play.events.is_empty() {
        return out.child(muted("No events yet. Alt-click a player to give them the puck, or add one above."));
    }
    for (i, ev) in play.events.iter().enumerate() {
        let past = ev.t_ms <= s.playhead_ms;
        let here = ev.t_ms == s.playhead_ms;
        let dot = if ev.kind == EventKind::DecisionPoint { theme::WARN } else { theme::ACCENT };
        let actors: Vec<String> = ev.actors.iter().map(|a| short_name(play, a)).collect();
        let delete = div()
            .id(("delete-event", i))
            .flex_none()
            .px_1()
            .rounded_sm()
            .text_xs()
            .text_color(rgb(theme::TEXT_FAINT))
            .cursor_pointer()
            .hover(|st| st.text_color(rgb(theme::DANGER)).bg(rgb(theme::RAISED)))
            .child("✕")
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.state.update(cx, |s, cx| {
                    s.delete_event(i);
                    cx.notify();
                })
            }));
        let head = div()
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .child(
                div()
                    .flex_none()
                    .w(px(30.))
                    .text_xs()
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child(SharedString::from(seconds(ev.t_ms))),
            )
            .child(div().flex_none().size(px(6.)).rounded_full().bg(rgb(dot)).opacity(if past { 1.0 } else { 0.45 }))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if past { theme::TEXT } else { theme::TEXT_MUTED }))
                    .child(ev.kind.label()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if past { theme::TEXT_MUTED } else { theme::TEXT_FAINT }))
                    .child(SharedString::from(actors.join(" → "))),
            )
            .child(delete);
        let mut item = div()
            .id(("event", i))
            .flex()
            .flex_col()
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .when(here, |d| d.bg(rgb(theme::RAISED)))
            .hover(|st| st.bg(rgb(theme::HOVER)))
            .when(s.selected_event == Some(i), |d| {
                d.bg(rgb(theme::RAISED)).border_1().border_color(rgb(theme::BORDER_STRONG))
            })
            .on_click(on_state(&this.state, move |s| {
                if s.selected_event == Some(i) {
                    s.selected_event = None;
                } else {
                    s.jump_to_event(i);
                }
            }))
            .child(head);
        if s.selected_event == Some(i) {
            out = out.child(item).child(event_editor(this, s, i));
            continue;
        }
        if !ev.note.is_empty() {
            item = item.child(
                div()
                    .w_full()
                    .pl(px(52.))
                    .text_xs()
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child(SharedString::from(ev.note.clone())),
            );
        }
        out = out.child(item);
    }
    out
}

/// The open event: its kind, its players, and its note.
fn event_editor(this: &InspectorView, s: &PlayState, i: usize) -> Div {
    let play = &s.play;
    let ev = &play.events[i];
    let mut kinds = div().flex().flex_wrap().gap_1p5();
    for (k, kind) in EventKind::ALL.into_iter().enumerate() {
        kinds = kinds.child(
            chip(("kind", k), kind.label(), kind == ev.kind)
                .on_click(on_state(&this.state, move |s| s.set_event_kind(i, kind))),
        );
    }
    let mut players = div().flex().flex_wrap().items_center().gap_1p5();
    for (a, actor) in ev.actors.iter().enumerate() {
        players = players.child(
            chip(("actor", a), format!("{}  ✕", short_name(play, actor)), true)
                .on_click(on_state(&this.state, move |s| s.remove_event_actor(i, a))),
        );
    }
    let addable: Option<EntityId> =
        s.selected.clone().filter(|id| !ev.actors.contains(id) && play.entity(id).is_some_and(|e| !e.is_puck()));
    if let Some(id) = addable {
        let label = format!("+ {}", short_name(play, &id));
        players = players
            .child(chip("add-actor", label, false).on_click(on_state(&this.state, move |s| s.add_event_actor(i, &id))));
    } else if ev.actors.is_empty() {
        players = players.child(muted("Select a player on the rink to add them."));
    }
    let order = match ev.kind {
        EventKind::Pass => "First the passer, then the receiver.",
        EventKind::Turnover => "First who loses it, then who gets it.",
        _ => "",
    };
    let mut out = div()
        .flex()
        .flex_col()
        .gap_2()
        .ml_2()
        .pl_3()
        .border_l_2()
        .border_color(rgb(theme::BORDER_STRONG))
        .child(kinds)
        .child(players);
    if !order.is_empty() {
        out = out.child(muted(order));
    }
    out.child(this.event_note.clone()).child(muted("Drag its ◆ in the timeline to change when it happens."))
}

fn warnings(s: &PlayState) -> Option<Div> {
    if s.issues.is_empty() {
        return None;
    }
    let mut out = section(format!("Warnings · {}", s.issues.len()));
    for issue in s.issues.iter().take(6) {
        out = out.child(
            div()
                .flex()
                .gap_2()
                .text_xs()
                .text_color(rgb(theme::WARN))
                .child("⚠")
                .child(div().flex_1().child(SharedString::from(issue.to_string()))),
        );
    }
    if s.issues.len() > 6 {
        out = out.child(muted(format!("{} more. Run `rink check` for the full list.", s.issues.len() - 6)));
    }
    Some(out)
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl Render for InspectorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_fields(window, cx);
        let s = self.state.read(cx);
        let mut panel = div()
            .id("inspector")
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(theme::INSPECTOR_WIDTH))
            .h_full()
            .bg(rgb(theme::PANEL))
            .border_l_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .child(play_section(self, s, cx))
            .child(selection(self, s))
            .children(off_ice(self, s));
        if let Some(f) = feedback(s, cx) {
            panel = panel.child(f);
        }
        panel = panel.child(events(self, s, cx));
        if let Some(w) = warnings(s) {
            panel = panel.child(w);
        }
        panel
    }
}
