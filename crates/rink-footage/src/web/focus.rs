use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::Path as FilePath,
};

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{jobs, problem, require_origin, storage_error, stored_session, ApiError, App};

const MAX_ANCHORS: usize = 256;

#[derive(Deserialize, Serialize)]
struct Anchor {
    source_ms: u64,
    track_id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gear: Option<[[u8; 5]; 3]>,
}

#[derive(Deserialize)]
struct Edit {
    source_ms: u64,
    track_id: Option<u32>,
}

#[derive(Deserialize)]
struct Frame {
    source_ms: u64,
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct Track {
    id: u32,
    predicted: bool,
    gear: Option<[[u8; 5]; 3]>,
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/footage/api/sessions/{id}/focus", get(selections))
        .route("/footage/api/sessions/{id}/clips/{clip}/focus", put(edit).layer(DefaultBodyLimit::max(1024)))
}

fn load(source: &FilePath) -> Result<Vec<Anchor>, ApiError> {
    match fs::read(source.with_file_name("focus.json")) {
        Ok(data) => serde_json::from_slice(&data).map_err(storage_error),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(storage_error(error)),
    }
}

async fn selections(
    State(app): State<App>,
    Path(id): Path<Uuid>,
) -> Result<Json<BTreeMap<Uuid, Vec<Anchor>>>, ApiError> {
    let all = tokio::task::spawn_blocking(move || {
        let session = stored_session(&app.root, id)?;
        session
            .clips
            .iter()
            .map(|clip| Ok((clip.id, load(&session.source_path(&app.root, clip))?)))
            .collect::<Result<_, ApiError>>()
    })
    .await
    .map_err(storage_error)??;
    Ok(Json(all))
}

async fn edit(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(edit): Json<Edit>,
) -> Result<Json<Vec<Anchor>>, ApiError> {
    require_origin(&headers, &app)?;
    let lock = app.jobs_lock.clone().lock_owned().await;
    let anchors = tokio::task::spawn_blocking(move || {
        let _lock = lock;
        let source = jobs::clip_source(&app, id, clip)?;
        if !jobs::result_path(&source).is_file() {
            return Err(problem(StatusCode::CONFLICT, "Wait for tracking to finish."));
        }
        let mut anchors = load(&source)?;
        let position = anchors.binary_search_by_key(&edit.source_ms, |anchor| anchor.source_ms);
        if let Some(track_id) = edit.track_id {
            let track = detected_track(&jobs::data_path(&source), edit.source_ms, track_id)?;
            let anchor = Anchor { source_ms: edit.source_ms, track_id, gear: track.gear };
            match position {
                Ok(index) => anchors[index] = anchor,
                Err(index) if anchors.len() < MAX_ANCHORS => anchors.insert(index, anchor),
                Err(_) => return Err(problem(StatusCode::CONFLICT, "Remove a correction before adding another.")),
            }
        } else if let Ok(index) = position {
            anchors.remove(index);
        } else {
            return Ok(anchors);
        }
        let parent = source.parent().ok_or_else(|| storage_error("missing clip folder"))?;
        let mut output = tempfile::NamedTempFile::new_in(parent).map_err(storage_error)?;
        serde_json::to_writer(output.as_file_mut(), &anchors).map_err(storage_error)?;
        output.persist(source.with_file_name("focus.json")).map_err(storage_error)?;
        Ok(anchors)
    })
    .await
    .map_err(storage_error)??;
    Ok(Json(anchors))
}

fn detected_track(path: &FilePath, source_ms: u64, id: u32) -> Result<Track, ApiError> {
    let reader = BufReader::new(File::open(path).map_err(storage_error)?);
    for line in reader.lines() {
        let frame: Frame = serde_json::from_str(&line.map_err(storage_error)?).map_err(storage_error)?;
        if frame.source_ms < source_ms {
            continue;
        }
        if frame.source_ms == source_ms {
            if let Some(track) = frame.tracks.into_iter().find(|track| track.id == id && !track.predicted) {
                return Ok(track);
            }
        }
        break;
    }
    Err(problem(StatusCode::UNPROCESSABLE_ENTITY, "Choose a detected skater on a clear frame."))
}
