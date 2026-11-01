//! Top bar: the play's title (click it to open another play) and save state, the last status message, and
//! buttons for the view toggles and Save. On macOS it is also the title bar:
//! the traffic lights sit at its left and dragging its empty part moves the
//! window.

use gpui::{
    div, prelude::*, px, rgb, Action, App, Context, Entity, FontWeight, MouseButton, SharedString, WeakEntity, Window,
};

use crate::state::PlayState;
use crate::theme::{self, button, primary_button};
use crate::workspace::{FlipEnds, OpenLibrary, Redo, ResetZoom, Save, ToggleGhosts, ToggleInspector, Undo, Workspace};

pub struct ToolbarView {
    state: Entity<PlayState>,
    workspace: WeakEntity<Workspace>,
}

impl ToolbarView {
    pub fn new(state: Entity<PlayState>, workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        ToolbarView { state, workspace }
    }
}

fn dispatch(action: impl Action) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx)
}

impl Render for ToolbarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inspector_shown = self.workspace.upgrade().is_some_and(|ws| ws.read(cx).show_inspector);
        let s = self.state.read(cx);
        let save_state = if s.conflict.is_some() {
            ("Conflict", theme::DANGER)
        } else if s.dirty {
            ("Saving…", theme::WARN)
        } else if s.path.is_none() {
            ("Saved once you edit", theme::TEXT_FAINT)
        } else {
            ("Saved", theme::TEXT_FAINT)
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(theme::TOOLBAR_HEIGHT))
            .pl(px(if cfg!(target_os = "macos") { 76. } else { 8. }))
            .pr_4()
            .gap_3()
            .bg(rgb(theme::PANEL))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .h_full()
                    .min_w_0()
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                    .child(
                        div()
                            .id("title")
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .min_w_0()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|st| st.bg(rgb(theme::HOVER)))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(dispatch(OpenLibrary))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme::TEXT))
                                    .min_w_0()
                                    .truncate()
                                    .debug_selector(|| "title".into())
                                    .child(SharedString::from(s.play.title.clone())),
                            )
                            .child(div().text_xs().text_color(rgb(theme::TEXT_FAINT)).child("▾")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .text_xs()
                            .text_color(rgb(save_state.1))
                            .child(div().size(px(6.)).rounded_full().bg(rgb(save_state.1)))
                            .child(save_state.0),
                    ),
            )
            // The status takes only the room the title and buttons leave, so it's cut short first.
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_end()
                    .h_full()
                    .min_w_0()
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                    .child(
                        div()
                            .min_w_0()
                            .debug_selector(|| "status".into())
                            .max_w(px(360.))
                            .truncate()
                            .text_xs()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .when(!s.status.is_error(), |d| d.child(SharedString::from(s.status.text().to_string()))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .child(
                        button("undo", "Undo", false).when(!s.can_undo(), |b| b.opacity(0.4)).on_click(dispatch(Undo)),
                    )
                    .child(
                        button("redo", "Redo", false)
                            .mr_2()
                            .when(!s.can_redo(), |b| b.opacity(0.4))
                            .on_click(dispatch(Redo)),
                    )
                    .child(button("ghosts", "Ghosts", s.show_ghosts).on_click(dispatch(ToggleGhosts)))
                    .child(button("flip", "Flip ends", s.flipped).on_click(dispatch(FlipEnds)))
                    .child(button("fit", "Fit rink", false).on_click(dispatch(ResetZoom)))
                    .child(button("inspector", "Inspector", inspector_shown).on_click(dispatch(ToggleInspector)))
                    .child(primary_button("save", "Save").on_click(dispatch(Save))),
            )
    }
}
