//! Lessons over Rink Replay plays: the lesson and step model, parsed from the
//! curriculum's Markdown and JSON files, pure grading and the content checks.
//! `rink-core` knows nothing of lessons; this crate sits beside it.

mod check;
mod course;
mod grade;
mod lesson;

pub use check::{check, Issue};
pub use course::{Course, Module};
pub use grade::{grade, Answer, Graded, SpotFacts, WatchOn};
pub use lesson::{Choice, ChoiceOption, Grade, Lesson, LessonMeta, Spot, SpotFeedback, Step, SCHEMA};

use rink_core::Play;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Which of a lesson's plays a step runs over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Which {
    #[default]
    Main,
    Wrong,
}

/// A lesson's play and its contrast variant.
#[derive(Debug, Clone)]
pub struct Plays {
    pub main: Play,
    pub wrong: Option<Play>,
}

impl Plays {
    pub fn get(&self, which: Which) -> Option<&Play> {
        match which {
            Which::Main => Some(&self.main),
            Which::Wrong => self.wrong.as_ref(),
        }
    }

    /// Where a contrast ends by default: the end of the shorter play, so the
    /// toggle can keep the playhead.
    pub fn contrast_end_ms(&self) -> u32 {
        let main = self.main.duration_ms;
        self.wrong.as_ref().map_or(main, |w| w.duration_ms.min(main))
    }
}

#[derive(Debug, Error)]
pub enum LessonError {
    #[error("frontmatter: {0}")]
    Frontmatter(String),
    #[error("section `{0}` appears twice")]
    DuplicateSection(String),
    #[error("steps file: {0}")]
    Steps(#[from] serde_json::Error),
    #[error("schema {0} is not supported (expected {SCHEMA})")]
    Schema(u32),
    #[error("the steps file is for `{steps}` but the Markdown declares `{md}`")]
    LessonMismatch { steps: String, md: String },
    #[error("no step `{0}`")]
    UnknownStep(String),
    #[error("step `{0}` does not take that kind of answer")]
    AnswerMismatch(String),
    #[error("step `{step}` has no option {index}")]
    NoOption { step: String, index: usize },
    #[error("step `{0}` needs a player marked as me in the main play")]
    NoMe(String),
}
