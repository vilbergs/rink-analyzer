//! Score each frozen review opportunity before revealing its confirmation.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
};

use anyhow::{Context, Result, ensure};
use rink_footage::identity::{self, Example, FrameKey, View};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
struct Game {
    game: String,
    reviewed_seconds: f64,
    tracks: Vec<View>,
    decisions: Vec<Decision>,
}

#[derive(Deserialize)]
struct Decision {
    frame: FrameKey,
    player: Uuid,
    #[serde(default)]
    scenarios: BTreeSet<String>,
    #[serde(default)]
    confirmations: Vec<Confirmation>,
    #[serde(default)]
    undo: Vec<Uuid>,
    #[serde(default)]
    rejected_before: Vec<Uuid>,
    #[serde(default)]
    prompts: u32,
    #[serde(default)]
    corrections: u32,
    /// Set only for adjacent samples on a continuously adjudicated stretch.
    continuous: Option<Continuous>,
}

#[derive(Deserialize)]
struct Confirmation {
    id: Uuid,
    player: Uuid,
}

#[derive(Deserialize)]
struct Continuous {
    stretch: String,
    seconds_since_previous: f64,
}

#[derive(Default, Serialize)]
struct Scores {
    reviewed: u32,
    suggestions: u32,
    wrong_first_choices: u32,
    wrong_identity_transitions: u32,
    continuous_seconds: f64,
}

impl Scores {
    fn observe(&mut self, choice: Option<Uuid>, actual: Uuid, previous: Option<Uuid>, seconds: f64) {
        self.reviewed += 1;
        self.continuous_seconds += seconds;
        if let Some(choice) = choice {
            self.suggestions += 1;
            if choice != actual {
                self.wrong_first_choices += 1;
                if seconds > 0.0 && previous.is_some_and(|p| p != choice) {
                    self.wrong_identity_transitions += 1;
                }
            }
        }
    }

    fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "counts": self,
            "suggestion_coverage": ratio(self.suggestions, self.reviewed as f64),
            "wrong_suggestion_rate": ratio(self.wrong_first_choices, self.suggestions as f64),
            "switches_per_reviewed_player_minute": ratio(self.wrong_identity_transitions, self.continuous_seconds / 60.0),
        })
    }
}

fn ratio(count: u32, denominator: f64) -> Option<f64> {
    (denominator > 0.0).then(|| count as f64 / denominator)
}

type Comparison = [Scores; 3];

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().context("Usage: rink-identity-replay <held-out-games.json>")?;
    ensure!(args.next().is_none(), "Expected one held-out corpus");
    let games: Vec<Game> = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    ensure!(!games.is_empty(), "Expected one or more held-out games");
    let mut names = BTreeSet::new();
    let mut scores = Comparison::default();
    let mut scenarios: BTreeMap<String, Comparison> = BTreeMap::new();
    let mut prompts = 0;
    let mut corrections = 0;
    let mut reviewed_minutes = 0.0;
    for game in games {
        ensure!(names.insert(game.game.clone()), "Each held-out game must appear once");
        ensure!(game.reviewed_seconds.is_finite() && game.reviewed_seconds > 0.0, "Expected reviewed game duration");
        reviewed_minutes += game.reviewed_seconds / 60.0;
        let mut examples: Vec<(Uuid, Example)> = vec![];
        let mut confirmations = BTreeSet::new();
        let mut clips: BTreeMap<(Uuid, Uuid), Vec<View>> = BTreeMap::new();
        for view in game.tracks {
            clips.entry((view.key.clip, view.key.run)).or_default().push(view);
        }
        for views in clips.values_mut() {
            views.sort_by_key(|view| view.key.source_ms);
        }
        let mut previous: BTreeMap<String, [Option<Uuid>; 3]> = BTreeMap::new();
        let mut visited_clips = BTreeSet::new();
        for review in game.decisions {
            let clip = clips.get(&(review.frame.clip, review.frame.run)).context("Missing frozen tracking run")?;
            let query = *clip.iter().find(|view| view.key == review.frame).context("Missing frozen review frame")?;
            let all = examples.iter().map(|(_, example)| *example).collect::<Vec<_>>();
            let same_clip = all.iter().copied().filter(|e| e.view.key.clip == query.key.clip).collect::<Vec<_>>();
            let choices = [
                identity::baseline(query, clip, &all),
                identity::clear_choice(&identity::rank(query, &all, &review.rejected_before)),
                identity::clear_choice(&identity::rank(query, &same_clip, &review.rejected_before)),
            ];
            let mut groups = review.scenarios;
            let mut visited = false;
            for prior in &visited_clips {
                if *prior != query.key.clip {
                    visited = true;
                    break;
                }
            }
            if visited {
                groups.insert("later_clip".into());
            }
            visited_clips.insert(query.key.clip);
            let (last, seconds) = if let Some(stretch) = review.continuous {
                let seconds = stretch.seconds_since_previous;
                ensure!(seconds >= 0.0 && seconds.is_finite(), "Invalid continuous interval");
                let prior = previous.insert(stretch.stretch, choices);
                ensure!(seconds == 0.0 || prior.is_some(), "Continuous intervals require a preceding sample");
                (prior.unwrap_or_default(), seconds)
            } else {
                ([None; 3], 0.0)
            };
            let score = |comparison: &mut Comparison| {
                for (i, metric) in comparison.iter_mut().enumerate() {
                    metric.observe(choices[i], review.player, last[i], seconds);
                }
            };
            score(&mut scores);
            for scenario in groups {
                score(scenarios.entry(scenario).or_default());
            }
            prompts += review.prompts;
            corrections += review.corrections;
            for id in review.undo {
                let index = examples.iter().position(|(active, _)| *active == id).context("Undo target is inactive")?;
                examples.remove(index);
            }
            for confirmation in review.confirmations {
                ensure!(confirmations.insert(confirmation.id), "Duplicate confirmation ID");
                examples.push((confirmation.id, Example { player: confirmation.player, view: query }));
            }
        }
    }
    let report = |comparison: &Comparison| {
        serde_json::json!({
            "legacy": comparison[0].report(),
            "learned": comparison[1].report(),
            "learned_without_earlier_clips": comparison[2].report(),
        })
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "games": names,
            "overall": report(&scores),
            "scenarios": scenarios.iter().map(|(name,comparison)| (name,report(comparison))).collect::<BTreeMap<_,_>>(),
            "reviewer_effort": {
                "reviewed_minutes": reviewed_minutes,
                "corrections_per_reviewed_minute": ratio(corrections, reviewed_minutes),
                "prompts_per_reviewed_minute": ratio(prompts, reviewed_minutes),
            }
        }))?
    );
    Ok(())
}
