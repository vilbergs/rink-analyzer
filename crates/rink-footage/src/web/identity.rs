use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader},
    path::Path as FilePath,
};

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use rink_footage::identity::{self, Candidate, Example, FrameKey, View};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{focus, jobs, problem, require_origin, storage_error, stored_session, ApiError, App};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Action {
    Assign { frame: FrameKey, player: Uuid, name: Option<String>, details: Option<PlayerDetails>, shown: Vec<Uuid> },
    Reject { frame: FrameKey, player: Uuid, shown: Vec<Uuid> },
    Rename { frame: FrameKey, player: Uuid, name: String, details: Option<PlayerDetails> },
    Undo { target: Uuid },
    SaveMoment { frame: FrameKey, player: Uuid, compare: Option<FrameKey>, text: String },
    RemoveMoment { target: Uuid },
    SavePlay { play: Play },
    RemovePlay { target: Uuid },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct PlayerDetails {
    team: String,
    jersey: String,
}

impl PlayerDetails {
    fn valid(&self) -> bool {
        self.team.chars().count() <= 40
            && !self.team.chars().any(char::is_control)
            && self.jersey.len() <= 2
            && self.jersey.bytes().all(|byte| byte.is_ascii_digit())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Edit {
    id: Uuid,
    revision: usize,
    #[serde(flatten)]
    action: Action,
}

#[derive(Deserialize, Serialize)]
struct Entry {
    edit: Edit,
    view: Option<View>,
    compare: Option<Compared>,
    kind: String,
    baseline: Option<Uuid>,
    learned: Option<Uuid>,
}

#[derive(Clone, Copy, Serialize)]
struct Assignment {
    edit: Uuid,
    player: Uuid,
    view: View,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
struct Compared {
    player: Option<Uuid>,
    view: View,
}

#[derive(Serialize)]
struct Moment {
    id: Uuid,
    player: Uuid,
    view: View,
    compare: Option<Compared>,
    text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Play {
    id: Uuid,
    clip: Uuid,
    run: Uuid,
    start_ms: u64,
    decision_ms: u64,
    end_ms: u64,
    primary: String,
    secondary: Option<String>,
    cue: String,
    track: Option<u32>,
}

#[derive(Serialize)]
struct SavedPlay {
    #[serde(flatten)]
    play: Play,
    player: Option<Compared>,
}

#[derive(Serialize)]
struct PlayLabel {
    id: &'static str,
    name: &'static str,
    description: &'static str,
}

const PLAY_LABELS: &[PlayLabel] = &[
    PlayLabel {
        id: "get_open",
        name: "Get open",
        description: "A player without the puck changes position or speed to become a passing option.",
    },
    PlayLabel {
        id: "draw_a_defender",
        name: "Draw a defender",
        description: "A player pulls a defender out of position, opening space for a teammate.",
    },
    PlayLabel {
        id: "support_the_puck",
        name: "Support the puck",
        description: "A teammate moves into an outlet position the puck carrier can use under pressure.",
    },
    PlayLabel {
        id: "switch_the_play",
        name: "Switch the play",
        description: "The puck moves to the opposite side to exploit open ice.",
    },
    PlayLabel {
        id: "attack_the_seam",
        name: "Attack the seam",
        description: "A pass or carry goes through a gap between defenders.",
    },
    PlayLabel {
        id: "screen_the_goalie",
        name: "Screen the goalie",
        description: "An attacker blocks the goalie’s view of the shot.",
    },
    PlayLabel {
        id: "win_the_rebound",
        name: "Win the rebound",
        description: "An attacker beats a defender to a loose puck after the initial shot.",
    },
];

#[derive(Default, Serialize)]
struct Review {
    revision: usize,
    runs: BTreeMap<Uuid, Uuid>,
    players: BTreeMap<Uuid, String>,
    player_details: BTreeMap<Uuid, PlayerDetails>,
    assignments: Vec<Assignment>,
    moments: Vec<Moment>,
    plays: Vec<SavedPlay>,
    play_labels: &'static [PlayLabel],
    undo: Option<Uuid>,
}

impl Review {
    fn assignment(&self, key: FrameKey) -> Option<Assignment> {
        self.assignments
            .iter()
            .filter(|a| {
                let other = a.view.key;
                other.clip == key.clip
                    && other.run == key.run
                    && other.track_id == key.track_id
                    && other.source_ms <= key.source_ms
            })
            .max_by_key(|a| a.view.key.source_ms)
            .copied()
    }

    fn examples(&self) -> Vec<Example> {
        self.assignments
            .iter()
            .filter(|a| self.assignment(a.view.key).is_some_and(|b| a.edit == b.edit))
            .map(|a| Example { player: a.player, view: a.view })
            .collect()
    }
}

#[derive(Serialize)]
struct Suggestions {
    revision: usize,
    clear: Option<Uuid>,
    candidates: Vec<Candidate>,
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/footage/api/sessions/{id}/identity", get(read).post(write))
        .route("/footage/api/sessions/{id}/identity/suggestions", post(suggestions))
        .layer(DefaultBodyLimit::max(8192))
}

fn invalid(message: &'static str) -> ApiError {
    problem(StatusCode::UNPROCESSABLE_ENTITY, message)
}

fn conflict(message: &'static str) -> ApiError {
    problem(StatusCode::CONFLICT, message)
}

fn load(path: &FilePath) -> Result<Vec<Entry>, ApiError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(storage_error),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(error) => Err(storage_error(error)),
    }
}

fn current(ledger: &[Entry], runs: BTreeMap<Uuid, Uuid>) -> Review {
    let reversed: BTreeSet<_> = ledger
        .iter()
        .filter_map(|entry| match entry.edit.action {
            Action::Undo { target } | Action::RemoveMoment { target } => Some(target),
            _ => None,
        })
        .collect();
    let mut state = Review { revision: ledger.len(), runs, play_labels: PLAY_LABELS, ..Review::default() };
    let mut plays = BTreeMap::new();
    for entry in ledger.iter().filter(|e| !reversed.contains(&e.edit.id)) {
        match &entry.edit.action {
            Action::SavePlay { play } => {
                plays.insert(play.id, SavedPlay { play: play.clone(), player: entry.compare });
            }
            Action::RemovePlay { target } => {
                plays.remove(target);
            }
            _ => {}
        }
        let Some(view) = entry.view else { continue };
        let active = state.runs.get(&view.key.clip) == Some(&view.key.run);
        match &entry.edit.action {
            Action::Assign { player, name, details, .. } => {
                if let Some(name) = name {
                    state.players.insert(*player, name.trim().into());
                }
                if let Some(details) = details {
                    state.player_details.insert(*player, details.clone());
                }
                if active {
                    state.assignments.push(Assignment { edit: entry.edit.id, player: *player, view });
                    state.undo = Some(entry.edit.id);
                }
            }
            Action::Rename { player, name, details, .. } => {
                state.players.insert(*player, name.trim().into());
                if let Some(details) = details {
                    state.player_details.insert(*player, details.clone());
                }
                if active {
                    state.undo = Some(entry.edit.id);
                }
            }
            Action::Reject { .. } if active => state.undo = Some(entry.edit.id),
            Action::SaveMoment { player, text, .. } => state.moments.push(Moment {
                id: entry.edit.id,
                player: *player,
                view,
                compare: entry.compare,
                text: text.trim().into(),
            }),
            _ => {}
        }
    }
    state.plays = plays.into_values().collect();
    state.plays.sort_by_key(|saved| (saved.play.clip, saved.play.decision_ms));
    state.players.retain(|player, _| {
        state.assignments.iter().any(|a| a.player == *player)
            || state
                .moments
                .iter()
                .any(|m| m.player == *player || m.compare.is_some_and(|other| other.player == Some(*player)))
            || state.plays.iter().any(|p| p.player.is_some_and(|other| other.player == Some(*player)))
    });
    state.player_details.retain(|player, _| state.players.contains_key(player));
    state
}

fn review(app: &App, id: Uuid, ledger: &[Entry]) -> Result<Review, ApiError> {
    let session = stored_session(&app.root, id)?;
    let mut runs = BTreeMap::new();
    for clip in &session.clips {
        if let Some(run) = jobs::completed_run(&session.source_path(&app.root, clip))? {
            runs.insert(clip.id, run);
        }
    }
    Ok(current(ledger, runs))
}

fn view(app: &App, id: Uuid, state: &Review, key: FrameKey) -> Result<View, ApiError> {
    if state.runs.get(&key.clip) != Some(&key.run) {
        return Err(conflict("Tracking has changed. Reopen the clip before editing."));
    }
    let source = jobs::clip_source(app, id, key.clip)?;
    let (size, track) = focus::detected_track(&jobs::data_path(&source), key.source_ms, key.track_id)?;
    Ok(View { key, size, xyxy: track.xyxy, gear: track.gear })
}

fn clip_views(app: &App, id: Uuid, key: FrameKey) -> Result<Vec<View>, ApiError> {
    let source = jobs::clip_source(app, id, key.clip)?;
    let records = BufReader::new(fs::File::open(jobs::data_path(&source)).map_err(storage_error)?);
    let mut views = Vec::new();
    for line in records.lines() {
        let frame: focus::Frame = serde_json::from_str(&line.map_err(storage_error)?).map_err(storage_error)?;
        views.extend(frame.tracks.into_iter().filter(|track| !track.predicted && track.gear.is_some()).map(|track| {
            View {
                key: FrameKey { source_ms: frame.source_ms, track_id: track.id, ..key },
                size: [frame.width, frame.height],
                xyxy: track.xyxy,
                gear: track.gear,
            }
        }));
    }
    Ok(views)
}

fn rejected(ledger: &[Entry], frame: FrameKey) -> Vec<Uuid> {
    ledger
        .iter()
        .filter_map(|entry| {
            let Action::Reject { frame: target, player, .. } = entry.edit.action else { return None };
            (target == frame
                && !ledger.iter().any(|e| matches!(e.edit.action, Action::Undo { target } if target == entry.edit.id)))
            .then_some(player)
        })
        .collect()
}

fn journal(app: &App, id: Uuid) -> std::path::PathBuf {
    app.root.join(id.to_string()).join("identity.json")
}

pub(super) fn affects_coaching(
    app: &App,
    session: Uuid,
    clip: Uuid,
    selections: &[(u64, u32)],
) -> Result<bool, ApiError> {
    let Some((_, first_track)) = selections.first() else { return Ok(false) };
    let ledger = load(&journal(app, session))?;
    if ledger.is_empty() {
        return Ok(false);
    }
    let state = review(app, session, &ledger)?;
    let Some(&run) = state.runs.get(&clip) else { return Ok(false) };
    let selected_here = |key: FrameKey| {
        selections.iter().rev().find(|(time, _)| *time <= key.source_ms).map_or(*first_track, |(_, track)| *track)
            == key.track_id
    };
    let mut seen = BTreeSet::new();
    for &(source_ms, track_id) in selections {
        let key = FrameKey { clip, run, source_ms, track_id };
        if let Some(assignment) = state.assignment(key) {
            seen.insert(assignment.player);
        }
    }
    for assignment in state.assignments.iter().filter(|a| a.view.key.clip == clip) {
        if selected_here(assignment.view.key)
            && state.assignment(assignment.view.key).is_some_and(|latest| latest.edit == assignment.edit)
        {
            seen.insert(assignment.player);
        }
    }
    Ok(seen.len() > 1)
}

async fn read(State(app): State<App>, Path(id): Path<Uuid>) -> Result<Json<Review>, ApiError> {
    let lock = app.jobs_lock.clone().lock_owned().await;
    tokio::task::spawn_blocking(move || {
        let _lock = lock;
        Ok(Json(review(&app, id, &load(&journal(&app, id))?)?))
    })
    .await
    .map_err(storage_error)?
}

async fn suggestions(
    State(app): State<App>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(frame): Json<FrameKey>,
) -> Result<Json<Suggestions>, ApiError> {
    require_origin(&headers, &app)?;
    let lock = app.jobs_lock.clone().lock_owned().await;
    tokio::task::spawn_blocking(move || {
        let _lock = lock;
        let ledger = load(&journal(&app, id))?;
        let state = review(&app, id, &ledger)?;
        let query = view(&app, id, &state, frame)?;
        let mut candidates = identity::rank(query, &state.examples(), &rejected(&ledger, frame));
        let clear = identity::clear_choice(&candidates);
        candidates.truncate(offer_count(&candidates));
        Ok(Json(Suggestions { revision: state.revision, clear, candidates }))
    })
    .await
    .map_err(storage_error)?
}

async fn write(
    State(app): State<App>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(edit): Json<Edit>,
) -> Result<Json<Review>, ApiError> {
    require_origin(&headers, &app)?;
    let lock = app.jobs_lock.clone().lock_owned().await;
    tokio::task::spawn_blocking(move || {
        let _lock = lock;
        let path = journal(&app, id);
        let mut ledger = load(&path)?;
        let state = review(&app, id, &ledger)?;
        if let Some(previous) = ledger.iter().find(|e| e.edit.id == edit.id) {
            return if previous.edit == edit {
                Ok(Json(state))
            } else {
                Err(conflict("That edit ID is already in use."))
            };
        }
        if edit.revision != state.revision {
            return Err(conflict("Player study changed in another tab. Review the latest assignment before saving."));
        }
        let entry = validate(&app, id, &state, &ledger, edit)?;
        ledger.push(entry);
        let mut file = tempfile::NamedTempFile::new_in(app.root.join(id.to_string())).map_err(storage_error)?;
        serde_json::to_writer(file.as_file_mut(), &ledger).map_err(storage_error)?;
        file.as_file().sync_all().map_err(storage_error)?;
        file.persist(path).map_err(storage_error)?;
        Ok(Json(current(&ledger, state.runs)))
    })
    .await
    .map_err(storage_error)?
}

fn validate(app: &App, id: Uuid, state: &Review, ledger: &[Entry], edit: Edit) -> Result<Entry, ApiError> {
    let mut entry = Entry { edit, view: None, compare: None, kind: String::new(), baseline: None, learned: None };
    let valid_name = |name: &str| !name.trim().is_empty() && name.trim().chars().count() <= 40;
    let known = |player: &Uuid| state.players.contains_key(player);
    entry.kind = match &entry.edit.action {
        Action::Assign { frame, player, name, details, shown } => {
            if details.as_ref().is_some_and(|details| name.is_none() || !details.valid()) {
                return Err(invalid("Enter a team up to 40 characters and a jersey number of up to two digits."));
            }
            if let Some(name) = name {
                if !valid_name(name) || ledger.iter().any(|prior| {
                    matches!(prior.edit.action, Action::Assign { player: existing, .. } if existing == *player)
                }) {
                    return Err(invalid("Enter a player name or jersey number and use a new player ID."));
                }
            } else if !known(player) {
                return Err(invalid("Choose an existing player."));
            }
            let query = view(app, id, state, *frame)?;
            entry.view = Some(query);
            let examples = state.examples();
            let excluded = rejected(ledger, *frame);
            let learned = identity::rank(query, &examples, &excluded);
            if examples.iter().any(|example| example.view.key.clip != frame.clip && example.view.gear.is_some()) {
                entry.baseline = identity::baseline(query, &clip_views(app, id, *frame)?, &examples);
            }
            entry.learned = identity::clear_choice(&learned);
            validate_shown(shown, &learned)?;
            if state.assignment(*frame).is_some_and(|a| a.player != *player) {
                "correction"
            } else if shown.contains(player) {
                "accept"
            } else if name.is_some() {
                "first_label"
            } else {
                "link"
            }
        }
        Action::Reject { frame, player, shown } => {
            let query = view(app, id, state, *frame)?;
            let ranking = identity::rank(query, &state.examples(), &rejected(ledger, *frame));
            validate_shown(shown, &ranking)?;
            if !shown.contains(player) {
                return Err(invalid("Choose a suggested player to reject."));
            }
            entry.view = Some(query);
            "rejection"
        }
        Action::Rename { frame, player, name, details } => {
            if details.as_ref().is_some_and(|details| !details.valid()) {
                return Err(invalid("Enter a team up to 40 characters and a jersey number of up to two digits."));
            }
            if !valid_name(name) || state.assignment(*frame).is_none_or(|a| a.player != *player) {
                return Err(invalid("Choose a confirmed player and enter a label."));
            }
            entry.view = Some(view(app, id, state, *frame)?);
            "rename"
        }
        Action::Undo { target } if state.undo == Some(*target) => "undo",
        Action::SaveMoment { frame, player, compare, text } => {
            if text.trim().is_empty()
                || text.trim().chars().count() > 600
                || state.assignment(*frame).is_none_or(|a| a.player != *player)
            {
                return Err(invalid("Choose a confirmed player on this frame and enter an observation."));
            }
            entry.view = Some(view(app, id, state, *frame)?);
            if let Some(other) = compare {
                if (other.clip, other.run, other.source_ms) != (frame.clip, frame.run, frame.source_ms) {
                    return Err(invalid("Choose a comparison on the same frame."));
                }
                let compared = state.assignment(*other).map(|a| a.player);
                if compared == Some(*player) {
                    return Err(invalid("Choose a different player for the comparison."));
                }
                entry.compare = Some(Compared { player: compared, view: view(app, id, state, *other)? });
            }
            "moment"
        }
        Action::RemoveMoment { target } if state.moments.iter().any(|m| m.id == *target) => "remove_moment",
        Action::SavePlay { play } => {
            validate_play(app, id, state, play)?;
            if let Some(track_id) = play.track {
                let key = FrameKey { clip: play.clip, run: play.run, track_id, source_ms: play.decision_ms };
                entry.compare = Some(Compared {
                    view: view(app, id, state, key)?,
                    player: state.assignment(key).map(|a| a.player),
                });
            }
            "play"
        }
        Action::RemovePlay { target } if state.plays.iter().any(|saved| saved.play.id == *target) => "remove_play",
        _ => return Err(conflict("That edit is no longer available.")),
    }
    .into();
    Ok(entry)
}

fn validate_play(app: &App, id: Uuid, state: &Review, play: &Play) -> Result<(), ApiError> {
    if state.runs.get(&play.clip) != Some(&play.run) {
        return Err(conflict("Tracking has changed. Reopen the clip before saving the play."));
    }
    if state.plays.iter().any(|saved| saved.play.id == play.id && saved.play.clip != play.clip) {
        return Err(conflict("This play belongs to another clip."));
    }
    let known = |key: &str| PLAY_LABELS.iter().any(|label| label.id == key);
    if !known(&play.primary) || play.secondary.as_deref().is_some_and(|key| !known(key) || key == play.primary) {
        return Err(invalid("Choose a primary label and a different optional secondary label."));
    }
    if play.cue.trim().chars().count() > 600
        || !(play.start_ms < play.end_ms && play.start_ms <= play.decision_ms && play.decision_ms <= play.end_ms)
    {
        return Err(invalid("Set a start and end that include the decision frame."));
    }
    let source = jobs::clip_source(app, id, play.clip)?;
    let data = BufReader::new(fs::File::open(jobs::data_path(&source)).map_err(storage_error)?);
    let mut missing = BTreeSet::from([play.start_ms, play.decision_ms, play.end_ms]);
    for line in data.lines() {
        let frame: focus::Frame = serde_json::from_str(&line.map_err(storage_error)?).map_err(storage_error)?;
        missing.remove(&frame.source_ms);
        if missing.is_empty() {
            return Ok(());
        }
        if frame.source_ms > play.end_ms {
            break;
        }
    }
    Err(invalid("Choose frames that are present in this tracking run."))
}

fn validate_shown(shown: &[Uuid], ranking: &[Candidate]) -> Result<(), ApiError> {
    let limit = offer_count(ranking);
    if !shown.is_empty() && !ranking.iter().take(limit).map(|c| c.player).eq(shown.iter().copied()) {
        return Err(conflict("The suggestions changed. Check the latest candidates."));
    }
    Ok(())
}

fn offer_count(ranking: &[Candidate]) -> usize {
    if identity::clear_choice(ranking).is_some() {
        1
    } else if ranking.first().is_some_and(|candidate| candidate.distance <= identity::MAX_DISTANCE) {
        3
    } else {
        0
    }
}
