use std::path::PathBuf;

use futures::{channel::mpsc, StreamExt};
use gpui::{div, img, prelude::*, px, rgb, Context, ExternalPaths, FontWeight, ObjectFit, PathPromptOptions, Window};
use rink_footage::session::{self, Session};

use crate::{
    theme::{self, button, primary_button},
    workspace::CloseDialog,
};

pub struct FootageView {
    root: PathBuf,
    sessions: Vec<Session>,
    selected: usize,
    status: String,
    errors: Vec<String>,
    choosing: bool,
    importing: bool,
}

impl FootageView {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            sessions: Vec::new(),
            selected: 0,
            status: String::new(),
            errors: Vec::new(),
            choosing: false,
            importing: false,
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let selected = self.sessions.get(self.selected).map(|session| session.id);
        match session::list(&self.root) {
            Ok(sessions) => {
                self.selected = sessions.iter().position(|session| Some(session.id) == selected).unwrap_or(0);
                self.sessions = sessions;
            }
            Err(error) => self.errors.push(format!("Could not read saved sessions: {error:#}")),
        }
        cx.notify();
    }

    fn choose(&mut self, cx: &mut Context<Self>) {
        if self.choosing || self.importing {
            return;
        }
        self.choosing = true;
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import Footage".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = prompt.await;
            this.update(cx, |view, cx| {
                view.choosing = false;
                match result {
                    Ok(Ok(Some(paths))) => view.import(paths, cx),
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => view.errors = vec![format!("Could not open file picker: {error}")],
                    Err(error) => view.errors = vec![format!("File picker closed: {error}")],
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn import(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.choosing || self.importing || paths.is_empty() {
            return;
        }
        self.importing = true;
        self.errors.clear();
        let total = paths.len();
        self.status = format!("Preparing {total} clips…");
        let root = self.root.clone();
        let (sender, mut progress) = mpsc::unbounded();
        let work = cx.background_executor().spawn(async move {
            session::import(&root, &paths, move |index, name| {
                let _ = sender.unbounded_send(format!("Importing {} of {total} · {name}", index + 1));
            })
        });
        cx.spawn(async move |this, cx| {
            while let Some(status) = progress.next().await {
                this.update(cx, |view, cx| {
                    view.status = status;
                    cx.notify();
                })
                .ok();
            }
            let result = work.await;
            this.update(cx, |view, cx| {
                view.importing = false;
                match result {
                    Ok(result) => {
                        view.errors = result.failures;
                        if let Some(session) = result.session {
                            let count = session.clips.len();
                            view.status = format!("Saved {count} {}", if count == 1 { "clip" } else { "clips" });
                            view.sessions.insert(0, session);
                            view.selected = 0;
                        } else {
                            view.status = "No footage was imported".into();
                        }
                    }
                    Err(error) => {
                        view.status.clear();
                        view.errors = vec![format!("Could not import footage: {error:#}")];
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

impl Render for FootageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let available = window.viewport_size();
        let busy = self.choosing || self.importing;
        let selected = self.sessions.get(self.selected);
        let sessions = self.sessions.iter().enumerate().map(|(index, session)| {
            let name = session.clips.first().map_or("Footage session", |clip| clip.name.as_str());
            div()
                .id(("footage-session", index))
                .rounded_md()
                .cursor_pointer()
                .px_2()
                .py_2()
                .when(index == self.selected, |row| row.bg(rgb(theme::RAISED)))
                .hover(|row| row.bg(rgb(theme::HOVER)))
                .child(div().text_sm().truncate().child(name.to_owned()))
                .child(div().text_xs().text_color(rgb(theme::TEXT_MUTED)).child(format!(
                    "{} {}",
                    session.clips.len(),
                    if session.clips.len() == 1 { "clip" } else { "clips" }
                )))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.selected = index;
                    cx.notify();
                }))
        });

        let root = &self.root;
        let clips = selected.into_iter().flat_map(|session| {
            session.clips.iter().map(move |clip| {
                let seconds = clip.duration_ms / 1000;
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(theme::RAISED))
                    .child(
                        img(session.preview_path(root, clip))
                            .w(px(120.))
                            .h(px(76.))
                            .flex_none()
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(div().min_w_0().child(div().text_sm().truncate().child(clip.name.clone())).child(
                        div().text_xs().text_color(rgb(theme::TEXT_MUTED)).child(format!(
                            "{}:{:02}",
                            seconds / 60,
                            seconds % 60
                        )),
                    ))
            })
        });

        div()
            .w(px(780.).min(available.width - px(32.)))
            .h(px(550.).min(available.height - px(95.)))
            .flex()
            .flex_col()
            .p_4()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(theme::BORDER_STRONG))
            .bg(rgb(theme::PANEL))
            .text_color(rgb(theme::TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Footage sessions"))
                    .child(button("close-footage", "Close", false).on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(CloseDialog), cx);
                    })),
            )
            .child(
                div()
                    .id("footage-drop")
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(theme::BORDER_STRONG))
                    .drag_over::<ExternalPaths>(|row, _, _, _| row.border_color(rgb(theme::ACCENT)))
                    .on_drop(cx.listener(|view, paths: &ExternalPaths, _, cx| {
                        view.import(paths.paths().to_vec(), cx);
                    }))
                    .child(div().text_sm().text_color(rgb(theme::TEXT_MUTED)).child(if self.status.is_empty() {
                        "Drop HEVC MOV or MP4 clips here".to_owned()
                    } else {
                        self.status.clone()
                    }))
                    .child(
                        primary_button("choose-footage", "Import Footage…")
                            .when(busy, |control| control.opacity(0.45))
                            .on_click(cx.listener(|view, _, _, cx| view.choose(cx))),
                    ),
            )
            .when(!self.errors.is_empty(), |d| {
                d.child(
                    div()
                        .id("footage-errors")
                        .max_h(px(105.))
                        .overflow_y_scroll()
                        .text_xs()
                        .text_color(rgb(theme::DANGER))
                        .children(self.errors.iter().cloned().map(|error| div().child(error))),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .child(
                        div()
                            .id("saved-footage-sessions")
                            .w(px(178.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .overflow_y_scroll()
                            .children(sessions),
                    )
                    .child(
                        div()
                            .id("session-clips")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .overflow_y_scroll()
                            .when(selected.is_none(), |list| {
                                list.child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(theme::TEXT_MUTED))
                                        .child("No saved footage sessions"),
                                )
                            })
                            .children(clips),
                    ),
            )
    }
}
