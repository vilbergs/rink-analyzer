//! Right-hand panel: the selected entity's exact state at the playhead, the
//! play's events, validation warnings, and the shortcut list.

use gpui::{div, prelude::*, px, rgb, Context, Entity, SharedString, Window};

use crate::state::PlayState;

const BG: u32 = 0x23272e;
const TEXT: u32 = 0xd5dae1;
const MUTED: u32 = 0x8a93a0;
const WARN: u32 = 0xffb300;
const PASSED: u32 = 0x5fb3ff;

pub struct InspectorView {
    state: Entity<PlayState>,
}

impl InspectorView {
    pub fn new(state: Entity<PlayState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        InspectorView { state }
    }
}

fn heading(text: impl Into<SharedString>) -> gpui::Div {
    div().pt_3().pb_1().text_color(rgb(MUTED)).text_xs().child(text.into())
}

fn line(text: impl Into<SharedString>) -> gpui::Div {
    div().child(text.into())
}

const SHORTCUTS: [(&str, &str); 10] = [
    ("Drag", "move (auto-keyframe)"),
    ("Shift-drag", "turn facing"),
    ("Alt-click", "toggle puck"),
    ("Space", "play / pause"),
    ("← →", "step 100 ms"),
    ("Shift ← →", "jump keyframe"),
    ("Delete", "remove keyframe here"),
    ("]", "playback speed"),
    ("F", "flip ends"),
    ("Ctrl-S", "save"),
];

impl Render for InspectorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let snap = s.snapshot();
        let play = &s.play;

        let file = match &s.path {
            Some(p) => p.display().to_string(),
            None => "not saved yet".into(),
        };
        let mut panel = div()
            .flex()
            .flex_col()
            .w(px(300.))
            .h_full()
            .px_4()
            .py_3()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_sm()
            .overflow_hidden()
            .child(div().text_base().child(SharedString::from(format!(
                "{}{}",
                play.title,
                if s.dirty { " •" } else { "" }
            ))))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(SharedString::from(format!("{} · {file}", play.rink.name))),
            );

        panel = panel.child(heading("SELECTED"));
        match s.selected.as_ref().and_then(|id| play.entity(id)) {
            None => panel = panel.child(line("Nothing. Click a player or the puck.")),
            Some(e) => {
                let who = match (e.number, e.role, e.team) {
                    (Some(n), Some(r), Some(t)) => format!("#{n} {} · {} ({:?})", r.as_str(), play.team(t).name, t),
                    _ => e.id.to_string(),
                };
                panel = panel.child(line(format!("{who}{}", if e.is_me { " · me" } else { "" })));
                panel = panel.child(div().text_xs().text_color(rgb(MUTED)).child(SharedString::from(e.id.to_string())));
                let state_line = if e.is_puck() {
                    let carrier = snap.puck.carrier.as_ref().map(|c| c.to_string()).unwrap_or_else(|| "loose".into());
                    format!("x {:.1}  y {:.1}  · {carrier}", snap.puck.pos.x, snap.puck.pos.y)
                } else if let Some(es) = snap.entity(&e.id) {
                    format!(
                        "x {:.1}  y {:.1}  facing {:.0}°  {:.1} m/s{}",
                        es.pos.x,
                        es.pos.y,
                        es.facing.to_degrees(),
                        es.speed_mps,
                        if es.has_puck { "  · puck" } else { "" }
                    )
                } else {
                    "not on the ice yet (no keyframes)".into()
                };
                panel = panel.child(line(state_line));
                let keys = play.track(&e.id);
                let here = keys.iter().any(|k| k.t_ms == s.playhead_ms);
                panel = panel.child(div().text_xs().text_color(rgb(MUTED)).child(SharedString::from(format!(
                    "{} keyframes{}",
                    keys.len(),
                    if here { " · one at playhead" } else { "" }
                ))));
            }
        }

        panel = panel.child(heading("EVENTS"));
        if play.events.is_empty() {
            panel = panel.child(line("None yet."));
        }
        for ev in &play.events {
            let actors: Vec<&str> = ev.actors.iter().map(|a| a.as_str()).collect();
            let mut text = format!("{:.1}s  {}  {}", ev.t_ms as f32 / 1000.0, ev.kind.label(), actors.join(" → "));
            if !ev.note.is_empty() {
                text.push_str(&format!(" — {}", ev.note));
            }
            let color = if ev.t_ms <= s.playhead_ms { PASSED } else { TEXT };
            panel = panel.child(div().text_xs().text_color(rgb(color)).child(SharedString::from(text)));
        }

        if !s.issues.is_empty() {
            panel = panel.child(heading(format!("WARNINGS ({})", s.issues.len())));
            for issue in s.issues.iter().take(6) {
                panel = panel.child(div().text_xs().text_color(rgb(WARN)).child(SharedString::from(issue.to_string())));
            }
        }

        if !s.status.is_empty() {
            panel = panel.child(heading("STATUS")).child(div().text_xs().child(SharedString::from(s.status.clone())));
        }

        panel = panel.child(heading("SHORTCUTS"));
        for (key, what) in SHORTCUTS {
            panel = panel.child(
                div()
                    .flex()
                    .text_xs()
                    .child(div().w(px(90.)).text_color(rgb(MUTED)).child(key))
                    .child(div().child(what)),
            );
        }
        panel
    }
}
