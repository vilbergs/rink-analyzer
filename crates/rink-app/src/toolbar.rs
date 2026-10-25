//! Top bar: the play's title and save state, the last status message, and
//! buttons for the view toggles and Save.

use gpui::{div, prelude::*, px, rgb, Action, App, Context, Entity, FontWeight, SharedString, Window};

use crate::state::PlayState;
use crate::theme::{self, button, primary_button};
use crate::workspace::{FlipEnds, ResetZoom, Save, ToggleGhosts};

pub struct ToolbarView {
    state: Entity<PlayState>,
}

impl ToolbarView {
    pub fn new(state: Entity<PlayState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        ToolbarView { state }
    }
}

fn dispatch(action: impl Action) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx)
}

impl Render for ToolbarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let file = s
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Not saved yet".into());
        let save_state = if s.dirty { ("Unsaved changes", theme::WARN) } else { ("Saved", theme::TEXT_FAINT) };

        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(theme::TOOLBAR_HEIGHT))
            .px_4()
            .bg(rgb(theme::PANEL))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::TEXT))
                            .child(SharedString::from(s.play.title.clone())),
                    )
                    .child(div().text_xs().text_color(rgb(theme::TEXT_FAINT)).child(SharedString::from(file)))
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
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .mr_2()
                            .max_w(px(360.))
                            .overflow_hidden()
                            .text_xs()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child(SharedString::from(s.status.clone())),
                    )
                    .child(button("ghosts", "Ghosts", s.show_ghosts).on_click(dispatch(ToggleGhosts)))
                    .child(button("flip", "Flip ends", s.flipped).on_click(dispatch(FlipEnds)))
                    .child(button("fit", "Fit rink", false).on_click(dispatch(ResetZoom)))
                    .child(primary_button("save", "Save").on_click(dispatch(Save))),
            )
    }
}
