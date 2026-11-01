//! Content checks: every reference a lesson makes resolves, and every step
//! can run on its plays. An authoring tool and the curriculum test read the
//! issues; the player assumes there are none.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::lesson::{Choice, Grade, Lesson, Spot, Step};
use crate::{Plays, Which};

const REQUIRED_SECTIONS: [&str; 2] = ["Principle", "Cue"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    /// The step at fault, or `None` for the lesson as a whole.
    pub step: Option<String>,
    pub message: String,
}

pub fn check(lesson: &Lesson, plays: &Plays) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut lesson_issue = |message: String| issues.push(Issue { step: None, message });
    for name in REQUIRED_SECTIONS.into_iter().filter(|s| !lesson.sections.contains_key(*s)) {
        lesson_issue(format!("missing the required section `## {name}`"));
    }
    if plays.main.me().is_none() {
        lesson_issue("the main play has no player marked as me".into());
    }

    let mut seen = BTreeSet::new();
    for step in &lesson.steps {
        let id = step.id();
        let mut problems = Vec::new();
        if !seen.insert(id) {
            problems.push("the id is used by an earlier step".into());
        }
        if !is_kebab_case(id) {
            problems.push("the id is not kebab-case".into());
        }
        step_problems(lesson, plays, step, &mut problems);
        issues.extend(problems.into_iter().map(|message| Issue { step: Some(id.into()), message }));
    }
    issues
}

fn step_problems(lesson: &Lesson, plays: &Plays, step: &Step, out: &mut Vec<String>) {
    let section = |name: &str, out: &mut Vec<String>| {
        if !lesson.sections.contains_key(name) {
            out.push(format!("section `{name}` is not in the Markdown"));
        }
    };
    match step {
        Step::Card { section: name, .. } => section(name, out),
        Step::Watch { from_ms, to_ms, play, .. } => {
            segment(lesson, plays, *play, *from_ms, Some(*to_ms), out);
        }
        Step::Choice(c) => choice_problems(lesson, plays, c, out),
        Step::Spot(s) => spot_problems(lesson, plays, s, out),
        Step::Contrast { at_ms, to_ms, section: name, .. } => {
            section(name, out);
            for which in [Which::Main, Which::Wrong] {
                segment(lesson, plays, which, *at_ms, *to_ms, out);
            }
        }
    }
}

/// `from_ms` before `to_ms`, both within the play.
fn segment(lesson: &Lesson, plays: &Plays, which: Which, from_ms: u32, to_ms: Option<u32>, out: &mut Vec<String>) {
    if let Some(to) = to_ms.filter(|to| from_ms >= *to) {
        out.push(format!("starts at {from_ms} ms but ends at {to} ms"));
    }
    let Some(play) = play_for(lesson, plays, which, out) else { return };
    for t in [Some(from_ms), to_ms].into_iter().flatten().filter(|t| *t > play.duration_ms) {
        out.push(format!("{t} ms is past the end of the {} play ({} ms)", name(which), play.duration_ms));
    }
}

fn play_for<'p>(lesson: &Lesson, plays: &'p Plays, which: Which, out: &mut Vec<String>) -> Option<&'p rink_core::Play> {
    let play = plays.get(which);
    if which == Which::Wrong && lesson.meta.wrong.is_none() {
        out.push("uses the wrong play, but the lesson names no `wrong` in its frontmatter".into());
    } else if play.is_none() {
        out.push("uses the wrong play, but it was not loaded".into());
    }
    play
}

fn choice_problems(lesson: &Lesson, plays: &Plays, c: &Choice, out: &mut Vec<String>) {
    segment(lesson, plays, Which::Main, c.at_ms, None, out);
    if c.options.len() < 2 {
        out.push("a choice needs at least two options".into());
    }
    let right = c.options.iter().filter(|o| o.grade == Grade::Right).count();
    if right != 1 {
        out.push(format!("a choice needs exactly one option graded 2, found {right}"));
    }
    for (i, o) in c.options.iter().enumerate() {
        if o.fb.trim().is_empty() {
            out.push(format!("option {i} has no feedback"));
        }
        if o.play == Which::Wrong {
            segment(lesson, plays, Which::Wrong, c.at_ms, None, out);
        }
    }
}

fn spot_problems(lesson: &Lesson, plays: &Plays, s: &Spot, out: &mut Vec<String>) {
    segment(lesson, plays, Which::Main, s.at_ms, None, out);
    match s.target_in(&plays.main) {
        Some(t) if !plays.main.rink.contains(t) => out.push(format!("target ({}, {}) is off the ice", t.x, t.y)),
        Some(_) => {}
        None => out.push("no target, and the main play has no player marked as me to default to".into()),
    }
    if !(0.0 < s.r1_m && s.r1_m <= s.r2_m) {
        out.push(format!("radii must satisfy 0 < r1_m <= r2_m, got {} and {}", s.r1_m, s.r2_m));
    }
    if [&s.fb.right, &s.fb.close, &s.fb.wrong].iter().any(|f| f.trim().is_empty()) {
        out.push("every spot feedback (right, close, wrong) needs text".into());
    }
}

fn name(which: Which) -> &'static str {
    match which {
        Which::Main => "main",
        Which::Wrong => "wrong",
    }
}

fn is_kebab_case(id: &str) -> bool {
    !id.is_empty()
        && id
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use rink_core::io::from_json;

    use super::*;

    const MD: &str = include_str!("../tests/fixtures/breakout.md");
    const STEPS: &str = include_str!("../tests/fixtures/breakout.steps.json");

    fn plays() -> Plays {
        Plays {
            main: from_json(include_str!("../tests/fixtures/breakout.rink.json")).unwrap(),
            wrong: Some(from_json(include_str!("../tests/fixtures/breakout.wrong.rink.json")).unwrap()),
        }
    }

    fn fixture() -> Lesson {
        Lesson::parse(MD, STEPS).unwrap()
    }

    /// The fixture with step `id` replaced by `step` (JSON).
    fn with_step(id: &str, step: &str) -> Lesson {
        let mut lesson = fixture();
        let i = lesson.steps.iter().position(|s| s.id() == id).unwrap();
        lesson.steps[i] = serde_json::from_str(step).unwrap();
        lesson
    }

    fn messages(lesson: &Lesson, plays: &Plays) -> Vec<(Option<String>, String)> {
        check(lesson, plays).into_iter().map(|i| (i.step, i.message)).collect()
    }

    /// Exactly one issue, on `step`, containing `needle`.
    #[track_caller]
    fn one_issue(lesson: &Lesson, plays: &Plays, step: Option<&str>, needle: &str) {
        let found = messages(lesson, plays);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0.as_deref(), step, "{found:?}");
        assert!(found[0].1.contains(needle), "{found:?}");
    }

    #[test]
    fn the_fixture_is_clean() {
        assert_eq!(messages(&fixture(), &plays()), []);
    }

    #[test]
    fn step_ids_are_unique_and_kebab_case() {
        let mut lesson = fixture();
        lesson.steps.push(serde_json::from_str(r#"{ "id": "fx-cue", "type": "card", "section": "Cue" }"#).unwrap());
        one_issue(&lesson, &plays(), Some("fx-cue"), "earlier step");
        let lesson = with_step("fx-cue", r#"{ "id": "fx_Cue", "type": "card", "section": "Cue" }"#);
        one_issue(&lesson, &plays(), Some("fx_Cue"), "kebab-case");
    }

    #[test]
    fn required_sections_exist() {
        let mut lesson = fixture();
        lesson.sections.remove("Principle");
        let found = messages(&lesson, &plays());
        assert!(found.contains(&(None, "missing the required section `## Principle`".into())), "{found:?}");
    }

    #[test]
    fn referenced_sections_exist() {
        let lesson = with_step("fx-principle", r#"{ "id": "fx-principle", "type": "card", "section": "Principles" }"#);
        one_issue(&lesson, &plays(), Some("fx-principle"), "`Principles`");
        let lesson =
            with_step("fx-contrast", r#"{ "id": "fx-contrast", "type": "contrast", "at_ms": 0, "section": "Nope" }"#);
        one_issue(&lesson, &plays(), Some("fx-contrast"), "`Nope`");
    }

    #[test]
    fn segments_run_forward_within_their_play() {
        let lesson =
            with_step("fx-watch-1", r#"{ "id": "fx-watch-1", "type": "watch", "from_ms": 2000, "to_ms": 2000 }"#);
        one_issue(&lesson, &plays(), Some("fx-watch-1"), "ends at 2000 ms");
        let lesson = with_step("fx-watch-1", r#"{ "id": "fx-watch-1", "type": "watch", "from_ms": 0, "to_ms": 6500 }"#);
        one_issue(&lesson, &plays(), Some("fx-watch-1"), "past the end of the main play");
        let lesson = with_step(
            "fx-watch-wrong",
            r#"{ "id": "fx-watch-wrong", "type": "watch", "from_ms": 0, "to_ms": 5500, "play": "wrong" }"#,
        );
        one_issue(&lesson, &plays(), Some("fx-watch-wrong"), "past the end of the wrong play (5000 ms)");
        let lesson = with_step(
            "fx-spot-dp",
            r#"{ "id": "fx-spot-dp", "type": "spot", "at_ms": 7000, "prompt": "p", "target": [0, 0],
                 "fb": { "right": "r", "close": "c", "wrong": "w" } }"#,
        );
        one_issue(&lesson, &plays(), Some("fx-spot-dp"), "7000 ms is past the end");
    }

    #[test]
    fn me_exists_in_the_main_play() {
        let mut plays = plays();
        plays.main.entities.iter_mut().for_each(|e| e.is_me = false);
        let found = messages(&fixture(), &plays);
        assert!(found.contains(&(None, "the main play has no player marked as me".into())), "{found:?}");
        assert!(found.iter().any(|(s, m)| s.as_deref() == Some("fx-spot-dp") && m.contains("no target")), "{found:?}");
    }

    #[test]
    fn spot_targets_are_on_the_ice() {
        let lesson = with_step(
            "fx-spot-explicit",
            r#"{ "id": "fx-spot-explicit", "type": "spot", "at_ms": 0, "prompt": "p", "target": [0, 40],
                 "fb": { "right": "r", "close": "c", "wrong": "w" } }"#,
        );
        one_issue(&lesson, &plays(), Some("fx-spot-explicit"), "off the ice");
    }

    #[test]
    fn a_choice_has_one_right_option_and_feedback_on_each() {
        let lesson = with_step(
            "fx-choice-dp",
            r#"{ "id": "fx-choice-dp", "type": "choice", "at_ms": 0, "prompt": "p",
                 "options": [ { "text": "a", "grade": 2, "fb": "f" }, { "text": "b", "grade": 2, "fb": "f" } ] }"#,
        );
        one_issue(&lesson, &plays(), Some("fx-choice-dp"), "exactly one option graded 2, found 2");
        let lesson = with_step(
            "fx-choice-dp",
            r#"{ "id": "fx-choice-dp", "type": "choice", "at_ms": 0, "prompt": "p",
                 "options": [ { "text": "a", "grade": 2, "fb": "f" }, { "text": "b", "grade": 0, "fb": " " } ] }"#,
        );
        one_issue(&lesson, &plays(), Some("fx-choice-dp"), "option 1 has no feedback");
    }

    #[test]
    fn the_wrong_play_must_be_declared_and_loaded() {
        let mut lesson = fixture();
        lesson.meta.wrong = None;
        let found = messages(&lesson, &plays());
        for step in ["fx-choice-dp", "fx-watch-wrong", "fx-contrast"] {
            assert!(
                found.iter().any(|(s, m)| s.as_deref() == Some(step) && m.contains("no `wrong` in its frontmatter")),
                "{step}: {found:?}"
            );
        }
        let plays = Plays { wrong: None, ..plays() };
        let found = messages(&fixture(), &plays);
        assert!(found.iter().all(|(_, m)| m.contains("not loaded")), "{found:?}");
        assert_eq!(found.len(), 3, "{found:?}");
    }
}
