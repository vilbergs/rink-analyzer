//! The play picker (Cmd-O): the plays in the library folder, newest first,
//! plus the templates for starting a new one.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gpui::{div, prelude::*, px, rgb, Div, FontWeight, SharedString, Stateful};
use rink_core::review::review_path;
use rink_core::templates::Template;
use rink_core::{io, EventKind};

use crate::theme::{self, badge, kbd, seconds};

pub const TEMPLATES: [(Template, &str); 3] =
    [(Template::Breakout, "Breakout"), (Template::DZoneSet, "D-zone set"), (Template::Faceoff, "Faceoff")];

pub struct Entry {
    pub path: PathBuf,
    pub title: String,
    pub duration_ms: u32,
    pub decision_points: usize,
    pub has_review: bool,
    /// A `.your-edits-<time>` copy saved before a reload.
    pub is_backup: bool,
    modified: Option<SystemTime>,
}

/// Every play in `dir`, most recently changed first. Files that don't load
/// are left out; `rink list` reports them.
pub fn scan(dir: &Path) -> Vec<Entry> {
    let mut entries: Vec<Entry> = io::list_play_files(dir)
        .into_iter()
        .filter_map(|path| {
            let play = io::load(&path).ok()?;
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some(Entry {
                title: play.title.clone(),
                duration_ms: play.duration_ms,
                decision_points: play.events.iter().filter(|e| e.kind == EventKind::DecisionPoint).count(),
                has_review: review_path(&path).exists(),
                is_backup: name.contains(".your-edits-"),
                modified: std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
                path,
            })
        })
        .collect();
    entries.sort_by(|a, b| a.is_backup.cmp(&b.is_backup).then(b.modified.cmp(&a.modified)));
    entries
}

/// The picker's rows: plays first, then templates.
pub fn row_count(entries: &[Entry]) -> usize {
    entries.len() + TEMPLATES.len()
}

fn row(id: (&'static str, usize), highlighted: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .when(highlighted, |d| d.bg(rgb(theme::HOVER)))
        .hover(|d| d.bg(rgb(theme::HOVER)))
}

/// One play row. `current` marks the play the window shows.
pub fn play_row(i: usize, e: &Entry, current: bool, highlighted: bool) -> Stateful<Div> {
    let mut meta = seconds(e.duration_ms);
    if e.decision_points > 0 {
        meta.push_str(&format!(
            " · {} decision point{}",
            e.decision_points,
            if e.decision_points == 1 { "" } else { "s" }
        ));
    }
    let file = e.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut badges = div().flex().gap_1();
    if current {
        badges = badges.child(badge("Open", theme::ACCENT));
    }
    if e.has_review {
        badges = badges.child(badge("Reviewed", theme::FEEDBACK));
    }
    if e.is_backup {
        badges = badges.child(badge("Your edits copy", theme::TEXT_MUTED));
    }
    row(("play", i), highlighted).child(
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if e.is_backup { theme::TEXT_MUTED } else { theme::TEXT }))
                            .truncate()
                            .child(SharedString::from(e.title.clone())),
                    )
                    .child(badges),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_FAINT))
                    .truncate()
                    .child(SharedString::from(format!("{meta} · {file}"))),
            ),
    )
}

pub fn template_row(i: usize, name: &'static str, highlighted: bool) -> Stateful<Div> {
    row(("template", i), highlighted)
        .child(div().text_sm().text_color(rgb(theme::TEXT)).child(format!("New play: {name}")))
}

/// `dir` with the home folder shortened to `~`.
fn display_dir(dir: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| dir.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => dir.display().to_string(),
    }
}

/// The picker's frame: a title row with the library folder, then `body`.
pub fn panel(dir: &Path, body: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(520.))
        .max_h(px(560.))
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
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(rgb(theme::BORDER))
                .child(
                    div().flex().flex_col().min_w_0().child(theme::section_label("Open a play")).child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::TEXT_FAINT))
                            .truncate()
                            .child(SharedString::from(display_dir(dir))),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(rgb(theme::TEXT_FAINT))
                        .child(kbd("↑↓"))
                        .child(kbd("Enter"))
                        .child(kbd("Esc")),
                ),
        )
        .child(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rink_core::templates::build;
    use rink_core::RinkSpec;

    #[test]
    fn backups_sort_after_plays() {
        let dir = std::env::temp_dir().join(format!("rink-library-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let play = build(Template::Faceoff, RinkSpec::iihf());
        io::save(&play, &dir.join("b.your-edits-1.rink.json")).unwrap();
        io::save(&play, &dir.join("a.rink.json")).unwrap();
        std::fs::write(dir.join("broken.rink.json"), "{").unwrap();
        let names: Vec<String> =
            scan(&dir).iter().map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["a.rink.json", "b.your-edits-1.rink.json"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
