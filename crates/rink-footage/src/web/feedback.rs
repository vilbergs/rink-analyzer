//! Feedback videos: the user picks clips they are marked in, and the GPU worker turns them into
//! one video of a few moments with coaching cues. A request lives in the session folder at
//! `feedback/<id>/`: `request.json` holds the clips, the note and the worker's lease;
//! `feedback.json` (the moments and cues) and `video.mp4` are the result, written in that order.

use std::{
    fs,
    path::{Path as FilePath, PathBuf},
};

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use uuid::Uuid;

use super::{
    jobs::{self, now, receive, require_worker, stream},
    problem, require_origin, storage_error, stored_bytes, stored_session, ApiError, App, MAX_STORED, MIB,
};

const LEASE_SECONDS: u64 = 300;
const MAX_ATTEMPTS: u32 = 2;
const MAX_CLIPS: usize = 6;
const MAX_NOTE: usize = 500;
const MAX_VIDEO: usize = 256 * MIB;
const MAX_FEEDBACK: usize = MIB;

#[derive(Serialize, Deserialize)]
struct Request {
    id: Uuid,
    created_ms: u64,
    clips: Vec<Uuid>,
    note: String,
    lease: Option<Lease>,
    error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Lease {
    token: Uuid,
    until: u64,
    attempts: u32,
}

#[derive(Deserialize)]
struct NewRequest {
    clips: Vec<Uuid>,
    #[serde(default)]
    note: String,
}

#[derive(Serialize)]
struct Summary {
    id: Uuid,
    created_ms: u64,
    clips: Vec<Uuid>,
    note: String,
    state: &'static str,
    error: Option<String>,
    feedback: Option<Value>,
}

#[derive(Serialize)]
struct ClaimedClip {
    id: Uuid,
    name: String,
}

#[derive(Serialize)]
struct Claimed {
    session: Uuid,
    request: Uuid,
    lease: Uuid,
    note: String,
    clips: Vec<ClaimedClip>,
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/footage/sessions/{id}/feedback", get(list).merge(post(create).layer(DefaultBodyLimit::max(4096))))
        .route("/api/footage/sessions/{id}/feedback/{request}/video.mp4", get(video))
        .route("/api/footage/worker/feedback/claim", post(claim))
        .route("/api/footage/worker/feedback/{id}/{request}/heartbeat", post(heartbeat))
        .route("/api/footage/worker/feedback/{id}/{request}/feedback", put(upload_feedback))
        .route("/api/footage/worker/feedback/{id}/{request}/video", put(upload_video))
        .route("/api/footage/worker/feedback/{id}/{request}/failure", post(failure).layer(DefaultBodyLimit::max(4096)))
}

fn folder(root: &FilePath, session: Uuid) -> PathBuf {
    root.join(session.to_string()).join("feedback")
}

fn load(dir: &FilePath) -> Result<Request, ApiError> {
    let bytes = fs::read(dir.join("request.json")).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            problem(StatusCode::NOT_FOUND, "That feedback request could not be found.")
        } else {
            storage_error(error)
        }
    })?;
    serde_json::from_slice(&bytes).map_err(storage_error)
}

fn save(dir: &FilePath, request: &Request) -> Result<(), ApiError> {
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(storage_error)?;
    serde_json::to_writer(file.as_file_mut(), request).map_err(storage_error)?;
    file.persist(dir.join("request.json")).map_err(storage_error)?;
    Ok(())
}

fn requests(root: &FilePath, session: Uuid) -> Result<Vec<(PathBuf, Request)>, ApiError> {
    let entries = match fs::read_dir(folder(root, session)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(storage_error(error)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let dir = entry.map_err(storage_error)?.path();
        if dir.join("request.json").is_file() {
            let request = load(&dir)?;
            found.push((dir, request));
        }
    }
    found.sort_by_key(|(_, request)| request.created_ms);
    Ok(found)
}

fn state(dir: &FilePath, request: &Request) -> &'static str {
    if dir.join("video.mp4").is_file() {
        "ready"
    } else if request.error.is_some() {
        "failed"
    } else {
        match &request.lease {
            Some(lease) if lease.until > now() => "processing",
            Some(lease) if lease.attempts >= MAX_ATTEMPTS => "failed",
            _ => "queued",
        }
    }
}

/// Bytes the session's feedback results take, for the shared storage limit.
pub(super) fn stored(root: &FilePath, session: Uuid) -> std::io::Result<u64> {
    let mut total = 0;
    let entries = match fs::read_dir(folder(root, session)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let dir = entry?.path();
        for name in ["video.mp4", "feedback.json"] {
            match fs::metadata(dir.join(name)) {
                Ok(metadata) => total += metadata.len(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(total)
}

async fn list(State(app): State<App>, Path(id): Path<Uuid>) -> Result<Json<Vec<Summary>>, ApiError> {
    stored_session(&app.root, id)?;
    let mut out = Vec::new();
    for (dir, request) in requests(&app.root, id)?.into_iter().rev() {
        let state = state(&dir, &request);
        let feedback = if state == "ready" {
            Some(
                serde_json::from_slice(&fs::read(dir.join("feedback.json")).map_err(storage_error)?)
                    .map_err(storage_error)?,
            )
        } else {
            None
        };
        let error = match (state, request.error) {
            ("failed", None) => Some("The worker could not finish this video.".into()),
            (_, error) => error,
        };
        out.push(Summary {
            id: request.id,
            created_ms: request.created_ms,
            clips: request.clips,
            note: request.note,
            state,
            error,
            feedback,
        });
    }
    Ok(Json(out))
}

async fn create(
    State(app): State<App>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(new): Json<NewRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_origin(&headers, &app)?;
    let note = new.note.trim().to_owned();
    if new.clips.is_empty() || new.clips.len() > MAX_CLIPS {
        return Err(problem(StatusCode::BAD_REQUEST, format!("Pick between 1 and {MAX_CLIPS} clips.")));
    }
    if note.chars().count() > MAX_NOTE {
        return Err(problem(StatusCode::BAD_REQUEST, format!("Keep the note under {MAX_NOTE} characters.")));
    }
    let _edit = app.edit_lock.lock().await;
    let session = stored_session(&app.root, id)?;
    let mut clips = Vec::new();
    for clip_id in new.clips {
        if clips.contains(&clip_id) {
            continue;
        }
        let clip = session
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| problem(StatusCode::NOT_FOUND, "That clip could not be found."))?;
        let source = session.source_path(&app.root, clip);
        if !jobs::result_path(&source).is_file() {
            return Err(problem(StatusCode::CONFLICT, format!("{} is not tracked yet.", clip.name)));
        }
        let marked = fs::read(source.with_file_name("focus.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<Value>>(&bytes).ok())
            .is_some_and(|anchors| !anchors.is_empty());
        if !marked {
            return Err(problem(StatusCode::CONFLICT, format!("Mark yourself in {} first.", clip.name)));
        }
        clips.push(clip_id);
    }
    let request = Request { id: Uuid::new_v4(), created_ms: now() * 1000, clips, note, lease: None, error: None };
    let dir = folder(&app.root, id).join(request.id.to_string());
    fs::create_dir_all(&dir).map_err(storage_error)?;
    save(&dir, &request)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": request.id, "state": "queued" }))))
}

async fn video(
    State(app): State<App>,
    Path((id, request)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    stream(&folder(&app.root, id).join(request.to_string()).join("video.mp4"), "video/mp4", &headers).await
}

async fn claim(State(app): State<App>, headers: HeaderMap) -> Result<Response, ApiError> {
    require_worker(&app, &headers)?;
    let _lock = app.jobs_lock.lock().await;
    let mut oldest: Option<(PathBuf, Request, Uuid)> = None;
    for session in rink_footage::session::list(&app.root).map_err(storage_error)? {
        for (dir, request) in requests(&app.root, session.id)? {
            if state(&dir, &request) == "queued"
                && oldest.as_ref().is_none_or(|(_, o, _)| request.created_ms < o.created_ms)
            {
                oldest = Some((dir, request, session.id));
            }
        }
    }
    let Some((dir, mut request, session_id)) = oldest else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let session = stored_session(&app.root, session_id)?;
    let token = Uuid::new_v4();
    let attempts = request.lease.as_ref().map_or(1, |lease| lease.attempts + 1);
    request.lease = Some(Lease { token, until: now() + LEASE_SECONDS, attempts });
    save(&dir, &request)?;
    let clips = request
        .clips
        .iter()
        .filter_map(|id| session.clips.iter().find(|clip| clip.id == *id))
        .map(|clip| ClaimedClip { id: clip.id, name: clip.name.clone() })
        .collect();
    Ok(Json(Claimed { session: session_id, request: request.id, lease: token, note: request.note, clips })
        .into_response())
}

/// The request with an active lease matching the worker's `X-Rink-Lease` header.
fn active(app: &App, id: Uuid, request: Uuid, headers: &HeaderMap) -> Result<(PathBuf, Request), ApiError> {
    let dir = folder(&app.root, id).join(request.to_string());
    let request = load(&dir)?;
    let token = headers.get("x-rink-lease").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok());
    let live = request.lease.as_ref().is_some_and(|lease| Some(lease.token) == token && lease.until > now());
    if !live || request.error.is_some() || dir.join("video.mp4").is_file() {
        return Err(problem(StatusCode::CONFLICT, "The feedback lease is no longer active."));
    }
    Ok((dir, request))
}

async fn heartbeat(
    State(app): State<App>,
    Path((id, request)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    require_worker(&app, &headers)?;
    let _lock = app.jobs_lock.lock().await;
    let (dir, mut request) = active(&app, id, request, &headers)?;
    if let Some(lease) = &mut request.lease {
        lease.until = now() + LEASE_SECONDS;
    }
    save(&dir, &request)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn failure(
    State(app): State<App>,
    Path((id, request)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    reason: String,
) -> Result<StatusCode, ApiError> {
    require_worker(&app, &headers)?;
    let _lock = app.jobs_lock.lock().await;
    let (dir, mut request) = active(&app, id, request, &headers)?;
    let reason: String = reason.trim().chars().take(300).collect();
    request.error = Some(if reason.is_empty() { "The worker could not finish this video.".into() } else { reason });
    save(&dir, &request)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_feedback(
    State(app): State<App>,
    Path((id, request)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    require_worker(&app, &headers)?;
    let (dir, _) = active(&app, id, request, &headers)?;
    let (temp, _, size) = receive(&app, body, MAX_FEEDBACK).await?;
    let parsed: Result<Value, _> = serde_json::from_slice(&fs::read(temp.path()).map_err(storage_error)?);
    if size == 0 || !parsed.is_ok_and(|value| value["moments"].is_array()) {
        return Err(problem(StatusCode::BAD_REQUEST, "Expected feedback JSON with moments."));
    }
    finish(&app, id, request, &headers, temp, dir.join("feedback.json"), size).await
}

async fn upload_video(
    State(app): State<App>,
    Path((id, request)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    require_worker(&app, &headers)?;
    let (dir, _) = active(&app, id, request, &headers)?;
    let (temp, mut file, size) = receive(&app, body, MAX_VIDEO).await?;
    file.seek(std::io::SeekFrom::Start(0)).await.map_err(storage_error)?;
    let mut signature = [0; 8];
    if file.read_exact(&mut signature).await.is_err() || &signature[4..] != b"ftyp" {
        return Err(problem(StatusCode::BAD_REQUEST, "Expected an MP4 feedback video."));
    }
    drop(file);
    if !dir.join("feedback.json").is_file() {
        return Err(problem(StatusCode::CONFLICT, "Upload the feedback before the video."));
    }
    finish(&app, id, request, &headers, temp, dir.join("video.mp4"), size).await
}

async fn finish(
    app: &App,
    id: Uuid,
    request: Uuid,
    headers: &HeaderMap,
    temp: tempfile::NamedTempFile,
    destination: PathBuf,
    size: usize,
) -> Result<StatusCode, ApiError> {
    let _edit = app.edit_lock.lock().await;
    let _jobs = app.jobs_lock.lock().await;
    active(app, id, request, headers)?;
    let root = app.root.clone();
    let used = tokio::task::spawn_blocking(move || stored_bytes(&root))
        .await
        .map_err(storage_error)?
        .map_err(storage_error)?;
    let previous = fs::metadata(&destination).map_or(0, |file| file.len());
    if used.saturating_sub(previous).saturating_add(size as u64) > MAX_STORED {
        return Err(problem(StatusCode::INSUFFICIENT_STORAGE, "Footage storage is full."));
    }
    temp.persist(destination).map_err(storage_error)?;
    Ok(StatusCode::NO_CONTENT)
}
