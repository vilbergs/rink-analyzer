//! The keyboard shortcut sheet (? or Cmd-/): every shortcut and mouse
//! gesture, grouped, in a dialog over the window.

use gpui::{div, prelude::*, px, rgb, Div, SharedString};

use crate::theme::{self, kbd};

type Group = (&'static str, &'static [(&'static str, &'static str)]);

const LEFT: [Group; 4] = [
    (
        "Editing",
        &[
            ("Drag", "Move; keyframes at the playhead"),
            ("Shift-drag", "Turn facing"),
            ("Alt-click", "Give the puck (again to undo)"),
            ("D", "Decision point here, then type it"),
            ("Alt-arrows", "Nudge the selected player 0.5 m"),
            ("Cmd-C  Cmd-V", "Copy / paste a position"),
            ("Drag a timeline ◆", "Retime a keyframe or event"),
            ("Delete", "Remove the keyframe here"),
            ("Cmd-Z", "Undo"),
            ("Shift-Cmd-Z", "Redo"),
        ],
    ),
    ("Plays", &[("Cmd-O", "Open a play or start a new one"), ("Cmd-S", "Save")]),
    ("Selection", &[("Tab  Shift-Tab", "Next / previous player"), ("Click a lane name", "Select that player")]),
    ("Help", &[("?  Cmd-/", "This sheet"), ("Esc", "Close a dialog or field")]),
];

const RIGHT: [Group; 3] = [
    (
        "Playback",
        &[
            ("Space", "Play / pause"),
            ("← →", "Step 100 ms"),
            ("Shift-← →", "Previous / next keyframe"),
            ("Home  End", "Start / end"),
            ("]", "Playback speed"),
        ],
    ),
    ("Feedback", &[("N  Shift-N", "Next / previous suggestion"), ("G", "Show / hide ghosts")]),
    (
        "View",
        &[
            ("Pinch, wheel", "Zoom"),
            ("Two-finger scroll", "Pan"),
            ("0", "Fit the rink"),
            ("F", "Flip ends"),
            ("A", "All players in the timeline"),
        ],
    ),
];

fn column(groups: &[Group]) -> Div {
    let mut col = div().flex().flex_col().flex_1().gap_4();
    for (title, rows) in groups {
        let mut group = div().flex().flex_col().gap_1p5().child(theme::section_label(*title));
        for (key, what) in rows.iter() {
            group = group.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .text_sm()
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child(SharedString::from(*what))
                    .child(kbd(*key)),
            );
        }
        col = col.child(group);
    }
    col
}

/// The sheet's panel; the caller puts it in an overlay.
pub fn sheet() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(640.))
        .rounded_lg()
        .bg(rgb(theme::PANEL))
        .border_1()
        .border_color(rgb(theme::BORDER_STRONG))
        .shadow_lg()
        .text_color(rgb(theme::TEXT))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_5()
                .py_3()
                .border_b_1()
                .border_color(rgb(theme::BORDER))
                .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child("Keyboard shortcuts"))
                .child(kbd("Esc")),
        )
        .child(div().flex().gap_8().px_5().py_4().child(column(&LEFT)).child(column(&RIGHT)))
        .child(
            div()
                .px_5()
                .py_2()
                .border_t_1()
                .border_color(rgb(theme::BORDER))
                .text_xs()
                .text_color(rgb(theme::TEXT_FAINT))
                .child("Ctrl works in place of Cmd. The menu bar shows these too; docs/MANUAL.md has the rest."),
        )
}
