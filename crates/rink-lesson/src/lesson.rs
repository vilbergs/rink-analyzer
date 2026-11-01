//! A lesson on disk is two files with one stem: `<stem>.md` holds the prose
//! (frontmatter, then `## ` sections) and `<stem>.steps.json` holds the steps
//! the player runs over the lesson's play.

use std::collections::BTreeMap;

use rink_core::{sample, Play, Vec2};
use serde::{Deserialize, Serialize};

use crate::{LessonError, Plays, Which};

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LessonMeta {
    pub id: String,
    pub module: String,
    pub title: String,
    /// File stem under `curriculum/plays/`, without `.rink.json`.
    pub play: String,
    /// The contrast variant's file stem, when the lesson has one.
    pub wrong: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lesson {
    pub meta: LessonMeta,
    /// Section bodies by heading, trimmed.
    pub sections: BTreeMap<String, String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Shows a Markdown section of the lesson.
    Card {
        id: String,
        section: String,
    },
    /// Plays a segment and stops.
    Watch {
        id: String,
        from_ms: u32,
        to_ms: u32,
        #[serde(default)]
        play: Which,
        #[serde(default = "half_speed")]
        speed: f32,
    },
    Choice(Choice),
    Spot(Spot),
    /// Flips between the play and its wrong variant from `at_ms`.
    Contrast {
        id: String,
        at_ms: u32,
        section: String,
        /// Defaults to the end of the shorter of the two plays.
        #[serde(default)]
        to_ms: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: String,
    pub at_ms: u32,
    pub prompt: String,
    pub options: Vec<ChoiceOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceOption {
    pub text: String,
    pub grade: Grade,
    pub fb: String,
    /// Which play runs on from the decision point after this answer.
    #[serde(default)]
    pub play: Which,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spot {
    pub id: String,
    pub at_ms: u32,
    pub prompt: String,
    /// Defaults to where the user's player is at `at_ms` in the main play.
    #[serde(default)]
    pub target: Option<Vec2>,
    /// Within this distance of the target is right.
    #[serde(default = "default_r1")]
    pub r1_m: f32,
    /// Within this distance is close.
    #[serde(default = "default_r2")]
    pub r2_m: f32,
    pub fb: SpotFeedback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpotFeedback {
    pub right: String,
    pub close: String,
    pub wrong: String,
}

/// 0 wrong, 1 close, 2 right. A number in the files and across the wasm boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum Grade {
    Wrong,
    Close,
    Right,
}

impl TryFrom<u8> for Grade {
    type Error = String;

    fn try_from(n: u8) -> Result<Self, Self::Error> {
        match n {
            0 => Ok(Grade::Wrong),
            1 => Ok(Grade::Close),
            2 => Ok(Grade::Right),
            _ => Err(format!("grade {n} is not 0, 1 or 2")),
        }
    }
}

impl From<Grade> for u8 {
    fn from(g: Grade) -> u8 {
        g as u8
    }
}

fn half_speed() -> f32 {
    0.5
}

fn default_r1() -> f32 {
    2.0
}

fn default_r2() -> f32 {
    4.0
}

impl Step {
    pub fn id(&self) -> &str {
        match self {
            Step::Card { id, .. } | Step::Watch { id, .. } | Step::Contrast { id, .. } => id,
            Step::Choice(c) => &c.id,
            Step::Spot(s) => &s.id,
        }
    }
}

impl Spot {
    /// The explicit target, else the user's player at `at_ms` in `main`.
    pub fn target_in(&self, main: &Play) -> Option<Vec2> {
        self.target.or_else(|| {
            let me = main.me()?;
            sample(main, self.at_ms).entity(&me.id).map(|e| e.pos)
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepsFile {
    schema: u32,
    lesson: String,
    steps: Vec<Step>,
}

impl Lesson {
    /// Parse a lesson's Markdown and its steps file. The steps file must name
    /// the lesson the Markdown declares.
    pub fn parse(md: &str, steps_json: &str) -> Result<Lesson, LessonError> {
        let (meta, body) = parse_frontmatter(md)?;
        let sections = parse_sections(body)?;
        let file: StepsFile = serde_json::from_str(steps_json)?;
        if file.schema != SCHEMA {
            return Err(LessonError::Schema(file.schema));
        }
        if file.lesson != meta.id {
            return Err(LessonError::LessonMismatch { steps: file.lesson, md: meta.id });
        }
        Ok(Lesson { meta, sections, steps: file.steps })
    }

    pub fn step(&self, id: &str) -> Result<&Step, LessonError> {
        self.steps.iter().find(|s| s.id() == id).ok_or_else(|| LessonError::UnknownStep(id.into()))
    }

    /// The steps with every play-dependent default filled in: a spot's target
    /// and a contrast's end. A default that can't be resolved stays empty, and
    /// [`check`](crate::check) reports it.
    pub fn resolved_steps(&self, plays: &Plays) -> Vec<Step> {
        self.steps
            .iter()
            .cloned()
            .map(|step| match step {
                Step::Spot(s) => Step::Spot(Spot { target: s.target_in(&plays.main), ..s }),
                Step::Contrast { id, at_ms, section, to_ms } => {
                    Step::Contrast { id, at_ms, section, to_ms: to_ms.or(Some(plays.contrast_end_ms())) }
                }
                other => other,
            })
            .collect()
    }
}

fn parse_frontmatter(md: &str) -> Result<(LessonMeta, &str), LessonError> {
    let rest = md.strip_prefix("---").and_then(|r| r.strip_prefix('\n').or_else(|| r.strip_prefix("\r\n")));
    let rest = rest.ok_or(LessonError::Frontmatter("the file must start with a `---` line".into()))?;
    let (front, body) = match rest.find("\n---") {
        Some(end) => (&rest[..end], rest[end + 4..].trim_start_matches(['\r', '\n'])),
        None => return Err(LessonError::Frontmatter("no closing `---` line".into())),
    };

    let mut fields = BTreeMap::new();
    for line in front.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (key, value) =
            line.split_once(':').ok_or_else(|| LessonError::Frontmatter(format!("`{line}` is not `key: value`")))?;
        let key = key.trim();
        if !["id", "module", "title", "play", "wrong"].contains(&key) {
            return Err(LessonError::Frontmatter(format!("unknown key `{key}`")));
        }
        let value = value.trim();
        let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
        if fields.insert(key, value.to_string()).is_some() {
            return Err(LessonError::Frontmatter(format!("`{key}` given twice")));
        }
    }
    let mut take =
        |key: &'static str| fields.remove(key).ok_or_else(|| LessonError::Frontmatter(format!("missing `{key}`")));
    let meta = LessonMeta {
        id: take("id")?,
        module: take("module")?,
        title: take("title")?,
        play: take("play")?,
        wrong: None,
    };
    Ok((LessonMeta { wrong: fields.remove("wrong"), ..meta }, body))
}

/// Each `## ` heading and the text under it, up to the next `## `. Text
/// before the first heading is not a section.
fn parse_sections(body: &str) -> Result<BTreeMap<String, String>, LessonError> {
    let mut sections = BTreeMap::new();
    let mut current: Option<(String, Vec<&str>)> = None;
    let mut finish = |current: Option<(String, Vec<&str>)>| match current {
        Some((heading, lines)) => {
            let text = lines.join("\n").trim().to_string();
            match sections.insert(heading.clone(), text) {
                Some(_) => Err(LessonError::DuplicateSection(heading)),
                None => Ok(()),
            }
        }
        None => Ok(()),
    };
    for line in body.lines() {
        match line.strip_prefix("## ") {
            Some(heading) => finish(current.replace((heading.trim().to_string(), Vec::new())))?,
            None => {
                if let Some((_, lines)) = current.as_mut() {
                    lines.push(line);
                }
            }
        }
    }
    finish(current)?;
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = include_str!("../tests/fixtures/breakout.md");
    const STEPS: &str = include_str!("../tests/fixtures/breakout.steps.json");

    #[test]
    fn parses_frontmatter_sections_and_steps_with_defaults() {
        let lesson = Lesson::parse(MD, STEPS).unwrap();
        assert_eq!(
            lesson.meta,
            LessonMeta {
                id: "fx-breakout-weak-side".into(),
                module: "breakouts".into(),
                title: "Breakout, weak side".into(),
                play: "breakout".into(),
                wrong: Some("breakout.wrong".into()),
            }
        );
        assert_eq!(
            lesson.sections.keys().collect::<Vec<_>>(),
            ["Cue", "Principle", "Self-check", "What wrong looks like"]
        );
        assert_eq!(lesson.sections["Principle"], "The weak-side winger gives the D an outlet on the wall.");
        assert_eq!(lesson.steps.len(), 8);
        assert_eq!(
            lesson.steps[1],
            Step::Watch { id: "fx-watch-1".into(), from_ms: 0, to_ms: 2000, play: Which::Main, speed: 0.5 }
        );
        let Step::Spot(spot) = lesson.step("fx-spot-dp").unwrap() else { panic!("a spot step") };
        assert_eq!((spot.target, spot.r1_m, spot.r2_m), (None, 2.0, 4.0));
        let Step::Choice(choice) = lesson.step("fx-choice-dp").unwrap() else { panic!("a choice step") };
        assert_eq!(choice.options.iter().map(|o| o.play).collect::<Vec<_>>(), [Which::Main, Which::Main, Which::Wrong]);
    }

    #[test]
    fn optional_wrong_and_quoted_values() {
        let md = "---\nid: a\nmodule: m\ntitle: \"Puck: in my corner\"\nplay: p\n---\n## Principle\nx\n";
        let lesson = Lesson::parse(md, r#"{ "schema": 1, "lesson": "a", "steps": [] }"#).unwrap();
        assert_eq!(lesson.meta.title, "Puck: in my corner");
        assert_eq!(lesson.meta.wrong, None);
    }

    #[test]
    fn rejects_malformed_files() {
        let steps = r#"{ "schema": 1, "lesson": "fx-breakout-weak-side", "steps": [] }"#;
        let err = |md: &str, steps: &str| Lesson::parse(md, steps).unwrap_err();
        assert!(matches!(err("## Principle\nx", steps), LessonError::Frontmatter(_)));
        assert!(matches!(err("---\nid: a\nmodule: m\ntitle: t\n", steps), LessonError::Frontmatter(_)));
        assert!(matches!(err("---\nid: a\nmodule: m\ntitle: t\n---\n", steps), LessonError::Frontmatter(_)));
        assert!(matches!(
            err("---\nid: a\nmodule: m\ntitle: t\nplay: p\nauthor: x\n---\n", steps),
            LessonError::Frontmatter(_)
        ));
        assert!(matches!(
            err("---\nid: a\nmodule: m\ntitle: t\nplay: p\n---\n## A\n## A\n", steps),
            LessonError::DuplicateSection(_)
        ));
        assert!(matches!(
            err(MD, r#"{ "schema": 2, "lesson": "fx-breakout-weak-side", "steps": [] }"#),
            LessonError::Schema(2)
        ));
        assert!(matches!(
            err(MD, r#"{ "schema": 1, "lesson": "other", "steps": [] }"#),
            LessonError::LessonMismatch { .. }
        ));
        let bad_steps = [
            r#"{ "id": "s", "type": "pick", "at_ms": 0 }"#,
            r#"{ "id": "s", "type": "card", "section": "Cue", "sectoin": "x" }"#,
            r#"{ "id": "s", "type": "choice", "at_ms": 0, "prompt": "p", "options": [ { "text": "t", "grade": 3, "fb": "f" } ] }"#,
        ];
        for step in bad_steps {
            let steps = format!(r#"{{ "schema": 1, "lesson": "fx-breakout-weak-side", "steps": [{step}] }}"#);
            assert!(matches!(err(MD, &steps), LessonError::Steps(_)), "{step}");
        }
    }
}
