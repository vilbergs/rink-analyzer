//! Grading: an answer to a step in, a grade with its feedback and what to
//! show on the rink out. Pure, so the browser, a test and the desktop app
//! grade the same way.

use rink_core::facts::{entity_facts, EntityFacts, Hypothetical};
use rink_core::scene::{opaque, Mark, GHOST, SELECTED_RING, SKATER_RADIUS_M, ZONE_FILL};
use rink_core::{sample, Vec2};
use serde::{Deserialize, Serialize};

use crate::lesson::{Choice, Grade, Lesson, Spot, Step};
use crate::{LessonError, Plays, Which};

/// The outer, "close" zone: the same green as the inner one, fainter.
const CLOSE_ZONE: u32 = (ZONE_FILL & !0xff) | 0x20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Answer {
    Choice {
        index: usize,
    },
    /// A tap on the rink, in rink meters.
    Spot {
        at: Vec2,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Graded {
    pub grade: Grade,
    pub feedback: String,
    /// For a choice: the play to watch on, and from when. `None` for a spot.
    pub watch: Option<WatchOn>,
    /// Annotations for the rink after answering. Empty for a choice.
    pub marks: Vec<Mark>,
    /// For a spot: the facts at the tap and at the target.
    pub facts: Option<SpotFacts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WatchOn {
    pub play: Which,
    pub from_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpotFacts {
    /// The user's player as if they stood at the tap.
    pub actual: EntityFacts,
    pub target: EntityFacts,
    pub distance_m: f32,
}

pub fn grade(lesson: &Lesson, plays: &Plays, step_id: &str, answer: &Answer) -> Result<Graded, LessonError> {
    match (lesson.step(step_id)?, answer) {
        (Step::Choice(c), Answer::Choice { index }) => grade_choice(c, *index),
        (Step::Spot(s), Answer::Spot { at }) => grade_spot(plays, s, *at),
        _ => Err(LessonError::AnswerMismatch(step_id.into())),
    }
}

fn grade_choice(c: &Choice, index: usize) -> Result<Graded, LessonError> {
    let option = c.options.get(index).ok_or_else(|| LessonError::NoOption { step: c.id.clone(), index })?;
    Ok(Graded {
        grade: option.grade,
        feedback: option.fb.clone(),
        watch: Some(WatchOn { play: option.play, from_ms: c.at_ms }),
        marks: Vec::new(),
        facts: None,
    })
}

fn grade_spot(plays: &Plays, s: &Spot, tap: Vec2) -> Result<Graded, LessonError> {
    let play = &plays.main;
    let no_me = || LessonError::NoMe(s.id.clone());
    let me = play.me().ok_or_else(no_me)?;
    let target = s.target_in(play).ok_or_else(no_me)?;
    let snap = sample(play, s.at_ms);
    let facts_at = |pos| entity_facts(play, &snap, &me.id, Some(Hypothetical { pos, facing: None })).ok_or_else(no_me);

    let distance_m = tap.distance(target);
    let (grade, feedback) = if distance_m <= s.r1_m {
        (Grade::Right, &s.fb.right)
    } else if distance_m <= s.r2_m {
        (Grade::Close, &s.fb.close)
    } else {
        (Grade::Wrong, &s.fb.wrong)
    };

    let mut marks = vec![
        Mark::Zone { at: target, r_m: s.r2_m, color: CLOSE_ZONE },
        Mark::Zone { at: target, r_m: s.r1_m, color: ZONE_FILL },
    ];
    let r = SKATER_RADIUS_M;
    if distance_m > 2.0 * r {
        let dir = (target - tap).normalize();
        marks.push(Mark::Arrow { from: tap + dir * r, to: target - dir * r, color: opaque(GHOST) });
    }
    marks.push(Mark::Dot { at: target, label: me.mark(), color: opaque(GHOST) });
    marks.push(Mark::Dot { at: tap, label: "you".into(), color: opaque(SELECTED_RING) });

    Ok(Graded {
        grade,
        feedback: feedback.clone(),
        watch: None,
        marks,
        facts: Some(SpotFacts { actual: facts_at(tap)?, target: facts_at(target)?, distance_m }),
    })
}

#[cfg(test)]
mod tests {
    use rink_core::io::from_json;

    use super::*;

    fn fixture() -> (Lesson, Plays) {
        let lesson = Lesson::parse(
            include_str!("../tests/fixtures/breakout.md"),
            include_str!("../tests/fixtures/breakout.steps.json"),
        )
        .unwrap();
        let plays = Plays {
            main: from_json(include_str!("../tests/fixtures/breakout.rink.json")).unwrap(),
            wrong: Some(from_json(include_str!("../tests/fixtures/breakout.wrong.rink.json")).unwrap()),
        };
        (lesson, plays)
    }

    fn spot(step: &str, x: f32, y: f32) -> Graded {
        let (lesson, plays) = fixture();
        grade(&lesson, &plays, step, &Answer::Spot { at: Vec2::new(x, y) }).unwrap()
    }

    #[test]
    fn a_choice_returns_the_options_grade_feedback_and_play() {
        let (lesson, plays) = fixture();
        let answers: Vec<_> = (0..3)
            .map(|index| grade(&lesson, &plays, "fx-choice-dp", &Answer::Choice { index }).unwrap())
            .map(|g| (g.grade, g.feedback, g.watch, g.marks.len(), g.facts.is_none()))
            .collect();
        let watch = |play| Some(WatchOn { play, from_ms: 2000 });
        assert_eq!(
            answers,
            [
                (Grade::Right, "Right: the D has an outlet.".into(), watch(Which::Main), 0, true),
                (
                    Grade::Close,
                    "Close: open, but the pass goes through the middle.".into(),
                    watch(Which::Main),
                    0,
                    true
                ),
                (Grade::Wrong, "Wrong: two of us in the corner and no outlet.".into(), watch(Which::Wrong), 0, true),
            ]
        );
    }

    #[test]
    fn a_spot_defaults_to_where_me_is_and_grades_by_two_radii() {
        // home_LW is at (-19, 12) at 2.0 s in the breakout; radii 2 and 4 m.
        assert_eq!(spot("fx-spot-dp", -19.0, 10.5).grade, Grade::Right);
        assert_eq!(spot("fx-spot-dp", -19.0, 10.5).feedback, "Right: on the wall.");
        assert_eq!(spot("fx-spot-dp", -19.0, 10.0).grade, Grade::Right, "exactly r1 is right");
        assert_eq!(spot("fx-spot-dp", -16.0, 12.0).grade, Grade::Close);
        assert_eq!(spot("fx-spot-dp", -16.0, 12.0).feedback, "Close: a few strides off.");
        assert_eq!(spot("fx-spot-dp", -19.0, 7.0).grade, Grade::Wrong);
        assert_eq!(spot("fx-spot-dp", -19.0, 7.0).feedback, "Wrong: no outlet there.");
    }

    #[test]
    fn a_spot_uses_its_explicit_target_and_radii() {
        // Target (-12, 11.5), radii 1 and 3 m.
        assert_eq!(spot("fx-spot-explicit", -12.0, 12.0).grade, Grade::Right);
        assert_eq!(spot("fx-spot-explicit", -12.0, 13.5).grade, Grade::Close);
        assert_eq!(spot("fx-spot-explicit", -12.0, 15.0).grade, Grade::Wrong);
    }

    #[test]
    fn a_spot_answer_shows_both_zones_the_target_the_tap_and_their_facts() {
        let g = spot("fx-spot-dp", -10.0, 0.0);
        let target = Vec2::new(-19.0, 12.0);
        let tap = Vec2::new(-10.0, 0.0);
        assert_eq!(g.watch, None);
        assert_eq!(g.marks[0], Mark::Zone { at: target, r_m: 4.0, color: CLOSE_ZONE });
        assert_eq!(g.marks[1], Mark::Zone { at: target, r_m: 2.0, color: ZONE_FILL });
        assert!(
            matches!(g.marks[2], Mark::Arrow { from, to, .. } if from.distance(tap) < 1.0 && to.distance(target) < 1.0)
        );
        assert_eq!(g.marks[3], Mark::Dot { at: target, label: "LW".into(), color: opaque(GHOST) });
        assert_eq!(g.marks[4], Mark::Dot { at: tap, label: "you".into(), color: opaque(SELECTED_RING) });
        let facts = g.facts.unwrap();
        assert_eq!((facts.actual.pos, facts.target.pos), (tap, target));
        assert_eq!(facts.distance_m, 15.0);

        let near = spot("fx-spot-dp", -19.0, 11.0);
        assert!(!near.marks.iter().any(|m| matches!(m, Mark::Arrow { .. })), "no arrow when the dots overlap");
    }

    #[test]
    fn rejects_answers_that_do_not_fit_the_step() {
        let (lesson, plays) = fixture();
        let err = |step, answer| grade(&lesson, &plays, step, &answer).unwrap_err();
        assert!(matches!(err("nope", Answer::Choice { index: 0 }), LessonError::UnknownStep(_)));
        assert!(matches!(err("fx-choice-dp", Answer::Spot { at: Vec2::ZERO }), LessonError::AnswerMismatch(_)));
        assert!(matches!(err("fx-principle", Answer::Choice { index: 0 }), LessonError::AnswerMismatch(_)));
        assert!(matches!(err("fx-choice-dp", Answer::Choice { index: 3 }), LessonError::NoOption { index: 3, .. }));
    }

    #[test]
    fn answers_and_grades_cross_the_boundary_as_plain_json() {
        let answer: Answer = serde_json::from_str(r#"{ "kind": "spot", "at": [-19.0, 10.5] }"#).unwrap();
        let (lesson, plays) = fixture();
        let json = serde_json::to_value(grade(&lesson, &plays, "fx-spot-dp", &answer).unwrap()).unwrap();
        assert_eq!(json["grade"], 2);
        assert_eq!(json["marks"][0]["kind"], "zone");
        assert_eq!(json["facts"]["target"]["pos"], serde_json::json!([-19.0, 12.0]));
        let answer: Answer = serde_json::from_str(r#"{ "kind": "choice", "index": 2 }"#).unwrap();
        let json = serde_json::to_value(grade(&lesson, &plays, "fx-choice-dp", &answer).unwrap()).unwrap();
        assert_eq!(json["watch"], serde_json::json!({ "play": "wrong", "from_ms": 2000 }));
        assert_eq!(json["facts"], serde_json::Value::Null);
    }
}
