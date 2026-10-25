//! Right-hand panel: the selected entity's exact state at the playhead,
//! position feedback, the play's events, validation warnings, and the
//! shortcut list.

use gpui::{div, prelude::*, px, rgb, Context, Div, Entity, FontWeight, SharedString, Window};
use rink_core::{EventKind, Play};

use crate::state::PlayState;
use crate::theme::{self, badge, card, kbd, role_name, seconds, section, short_name};

pub struct InspectorView {
    state: Entity<PlayState>,
    show_shortcuts: bool,
}

impl InspectorView {
    pub fn new(state: Entity<PlayState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        InspectorView { state, show_shortcuts: false }
    }
}

const SHORTCUTS: [(&str, &str); 14] = [
    ("Drag", "Move; keyframes at the playhead"),
    ("Shift-drag", "Turn facing"),
    ("Alt-click", "Give the puck"),
    ("Space", "Play / pause"),
    ("← →", "Step 100 ms"),
    ("Shift ← →", "Previous / next keyframe"),
    ("Delete", "Remove keyframe here"),
    ("]", "Playback speed"),
    ("F", "Flip ends"),
    ("N", "Next feedback (Shift: previous)"),
    ("G", "Show / hide ghosts"),
    ("Pinch, scroll", "Zoom / pan"),
    ("0", "Fit rink"),
    ("Ctrl-S", "Save"),
];

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

fn selection(s: &PlayState) -> Div {
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
        }
        None => body = body.child(muted("Not on the ice yet. Drag them onto the rink to add a keyframe.")),
    }
    section("Selection").child(body)
}

fn feedback(s: &PlayState) -> Option<Div> {
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
        let mut item =
            card().border_l_2().border_color(rgb(if on { theme::FEEDBACK } else { theme::BORDER })).child(head).child(
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

fn events(s: &PlayState) -> Div {
    let play: &Play = &s.play;
    let mut out = section("Events");
    if play.events.is_empty() {
        return out.child(muted("No events yet. Alt-click a player to give them the puck."));
    }
    for ev in &play.events {
        let past = ev.t_ms <= s.playhead_ms;
        let dot = if ev.kind == EventKind::DecisionPoint { theme::WARN } else { theme::ACCENT };
        let actors: Vec<String> = ev.actors.iter().map(|a| short_name(play, a)).collect();
        let mut text = div().flex().flex_col().flex_1().child(
            div()
                .flex()
                .gap_1p5()
                .text_sm()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(if past { theme::TEXT } else { theme::TEXT_MUTED }))
                        .child(ev.kind.label()),
                )
                .child(
                    div()
                        .text_color(rgb(if past { theme::TEXT_MUTED } else { theme::TEXT_FAINT }))
                        .child(SharedString::from(actors.join(" → "))),
                ),
        );
        if !ev.note.is_empty() {
            text = text
                .child(div().text_xs().text_color(rgb(theme::TEXT_FAINT)).child(SharedString::from(ev.note.clone())));
        }
        out = out.child(
            div()
                .flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .w(px(34.))
                        .pt(px(2.))
                        .text_xs()
                        .text_color(rgb(theme::TEXT_FAINT))
                        .child(SharedString::from(seconds(ev.t_ms))),
                )
                .child(div().flex_none().mt(px(7.)).size(px(6.)).rounded_full().bg(rgb(dot)).opacity(if past {
                    1.0
                } else {
                    0.45
                }))
                .child(text),
        );
    }
    out
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(selection(s));
        if let Some(f) = feedback(s) {
            panel = panel.child(f);
        }
        panel = panel.child(events(s));
        if let Some(w) = warnings(s) {
            panel = panel.child(w);
        }

        let open = self.show_shortcuts;
        let mut shortcuts = div().flex().flex_col().gap_2().px_4().py_3().child(
            div()
                .id("shortcuts-toggle")
                .flex()
                .justify_between()
                .cursor_pointer()
                .child(theme::section_label("Keyboard shortcuts"))
                .child(div().text_xs().text_color(rgb(theme::TEXT_FAINT)).child(if open { "Hide" } else { "Show" }))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_shortcuts = !this.show_shortcuts;
                    cx.notify();
                })),
        );
        if open {
            for (key, what) in SHORTCUTS {
                shortcuts = shortcuts.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_xs()
                        .text_color(rgb(theme::TEXT_MUTED))
                        .child(what)
                        .child(kbd(key)),
                );
            }
        }
        panel.child(shortcuts)
    }
}
