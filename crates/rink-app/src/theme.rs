//! One palette and a few building blocks shared by every panel, so the app
//! reads as one product instead of a set of debug views.

use gpui::{div, prelude::*, px, rgb, Div, FontWeight, SharedString, Stateful};
use rink_core::{EntityId, Play, Role};

/// Window background around the rink.
pub const APP_BG: u32 = 0x0f1115;
/// Toolbar, inspector and timeline.
pub const PANEL: u32 = 0x16191f;
/// Cards and buttons on a panel.
pub const RAISED: u32 = 0x1d2128;
pub const HOVER: u32 = 0x262b34;
pub const BORDER: u32 = 0x262a32;
pub const BORDER_STRONG: u32 = 0x343a45;

pub const TEXT: u32 = 0xe8ebf0;
pub const TEXT_MUTED: u32 = 0x9ba3b0;
pub const TEXT_FAINT: u32 = 0x666e7b;

/// Selection, playhead, primary actions.
pub const ACCENT: u32 = 0x4c8dff;
/// Position feedback and ghosts.
pub const FEEDBACK: u32 = 0x35c47c;
/// Decision points and warnings.
pub const WARN: u32 = 0xf2a93b;
/// The user's own player.
pub const ME: u32 = 0xf2a93b;

pub const TOOLBAR_HEIGHT: f32 = 44.0;
pub const INSPECTOR_WIDTH: f32 = 320.0;

/// A small uppercase label above a group.
pub fn section_label(text: impl Into<SharedString>) -> Div {
    div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(rgb(TEXT_FAINT)).child(text.into().to_uppercase())
}

/// A panel section: label, then content, separated from the next by a rule.
pub fn section(label: impl Into<SharedString>) -> Div {
    div().flex().flex_col().gap_2().px_4().py_3().border_b_1().border_color(rgb(BORDER)).child(section_label(label))
}

/// A rounded card on a panel.
pub fn card() -> Div {
    div().flex().flex_col().gap_1().p_3().rounded_md().bg(rgb(RAISED)).border_1().border_color(rgb(BORDER))
}

/// A key cap, for shortcuts.
pub fn kbd(text: impl Into<SharedString>) -> Div {
    div()
        .px_1p5()
        .rounded_sm()
        .bg(rgb(RAISED))
        .border_1()
        .border_color(rgb(BORDER_STRONG))
        .text_xs()
        .text_color(rgb(TEXT_MUTED))
        .child(text.into())
}

/// A small colored pill, e.g. "You" or "Edited since".
pub fn badge(text: impl Into<SharedString>, hex: u32) -> Div {
    div()
        .px_1p5()
        .rounded_sm()
        .bg(gpui::rgba((hex << 8) | 0x26))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(hex))
        .child(text.into())
}

/// A toolbar or transport button. `active` shows a toggled-on state.
pub fn button(id: &'static str, label: impl Into<SharedString>, active: bool) -> Stateful<Div> {
    let (bg, fg) = if active { (0x1f3354, 0xa9c7ff) } else { (RAISED, TEXT) };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1p5()
        .h(px(28.))
        .px_2p5()
        .rounded_md()
        .bg(rgb(bg))
        .border_1()
        .border_color(rgb(if active { 0x2f4f80 } else { BORDER_STRONG }))
        .text_sm()
        .text_color(rgb(fg))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(if active { 0x24406a } else { HOVER })))
        .child(label.into())
}

/// The highlighted call to action, e.g. Save.
pub fn primary_button(id: &'static str, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(28.))
        .px_3()
        .rounded_md()
        .bg(rgb(ACCENT))
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(0xffffff))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x6a9fff)))
        .child(label.into())
}

pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::LW => "Left wing",
        Role::C => "Center",
        Role::RW => "Right wing",
        Role::LD => "Left D",
        Role::RD => "Right D",
        Role::G => "Goalie",
    }
}

/// How a player is named in lists: "#17 LW", or "Puck".
pub fn short_name(play: &Play, id: &EntityId) -> String {
    let Some(e) = play.entity(id) else { return id.to_string() };
    if e.is_puck() {
        return "Puck".into();
    }
    match (e.number, e.role) {
        (Some(n), Some(r)) => format!("#{n} {}", r.as_str()),
        (Some(n), None) => format!("#{n}"),
        (None, Some(r)) => r.as_str().to_string(),
        (None, None) => e.id.to_string(),
    }
}

/// Seconds with one decimal, for time readouts.
pub fn seconds(ms: u32) -> String {
    format!("{:.1}s", ms as f32 / 1000.0)
}
