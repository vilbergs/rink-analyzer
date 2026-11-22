use std::{
    fs,
    path::{Path as FilePath, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use http_body_util::BodyExt;
use rink_footage::session;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use super::{problem, storage_error, stored_bytes, stored_session, ApiError, App, MAX_STORED, MIB};

const LEASE_SECONDS: u64 = 300;
const MAX_ATTEMPTS: u32 = 3;
const MAX_RESULT: usize = 256 * MIB;
const MAX_DATA: usize = 32 * MIB;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/footage/api/sessions/{id}/tracking", get(status))
        .route("/footage/api/sessions/{id}/clips/{clip}/tracks.mp4", get(movie))
        .route("/footage/api/sessions/{id}/clips/{clip}/tracks.ndjson", get(track_data))
        .route("/footage/worker/claim", post(claim))
        .route("/footage/worker/{id}/{clip}/heartbeat", post(heartbeat))
        .route("/footage/worker/{id}/{clip}/source", get(source))
        .route("/footage/worker/{id}/{clip}/result", put(result))
        .route("/footage/worker/{id}/{clip}/data", put(upload_data))
        .route("/footage/worker/{id}/{clip}/failure", post(failure))
}

#[derive(Serialize, Deserialize)]
struct Lease {
    token: Uuid,
    until: u64,
    attempts: u32,
    failed: bool,
}

#[derive(Serialize)]
struct Claimed {
    session: Uuid,
    clip: Uuid,
    lease: Uuid,
    split_shots: bool,
}

#[derive(Serialize)]
struct ClipStatus {
    clip: Uuid,
    tracking: &'static str,
}

pub(super) fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn state_path(source: &FilePath) -> PathBuf {
    source.with_file_name("tracking.json")
}

pub fn result_path(source: &FilePath) -> PathBuf {
    source.with_file_name("tracks.mp4")
}

pub fn data_path(source: &FilePath) -> PathBuf {
    source.with_file_name("tracks.ndjson")
}

fn load(source: &FilePath) -> Result<Option<Lease>, ApiError> {
    match fs::read(state_path(source)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(storage_error),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(storage_error(error)),
    }
}

pub(super) fn completed_run(source: &FilePath) -> Result<Option<Uuid>, ApiError> {
    if !result_path(source).is_file() {
        return Ok(None);
    }
    Ok(load(source)?.map(|lease| lease.token))
}

fn save(source: &FilePath, lease: &Lease) -> Result<(), ApiError> {
    let parent = source.parent().ok_or_else(|| storage_error("missing clip folder"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(storage_error)?;
    serde_json::to_writer(file.as_file_mut(), lease).map_err(storage_error)?;
    file.persist(state_path(source)).map_err(storage_error)?;
    Ok(())
}

pub(super) fn require_worker(app: &App, headers: &HeaderMap) -> Result<(), ApiError> {
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|token| token.to_str().ok())
        .and_then(|token| token.strip_prefix("Bearer "));
    if app.worker_token.as_deref().is_none_or(|token| Some(token) != presented) {
        return Err(problem(StatusCode::UNAUTHORIZED, "Worker credentials required."));
    }
    Ok(())
}

pub fn clip_source(app: &App, id: Uuid, clip_id: Uuid) -> Result<PathBuf, ApiError> {
    let session = stored_session(&app.root, id)?;
    let clip = session
        .clips
        .iter()
        .find(|clip| clip.id == clip_id)
        .ok_or_else(|| problem(StatusCode::NOT_FOUND, "That clip could not be found."))?;
    Ok(session.source_path(&app.root, clip))
}

fn active(source: &FilePath, headers: &HeaderMap) -> Result<Lease, ApiError> {
    let token = headers.get("x-rink-lease").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok());
    load(source)?
        .filter(|lease| Some(lease.token) == token && lease.until > now() && !lease.failed)
        .filter(|_| !result_path(source).is_file())
        .ok_or_else(|| problem(StatusCode::CONFLICT, "The tracking lease is no longer active."))
}

async fn status(State(app): State<App>, Path(id): Path<Uuid>) -> Result<Json<Vec<ClipStatus>>, ApiError> {
    let session = stored_session(&app.root, id)?;
    let mut clips = Vec::new();
    for clip in &session.clips {
        let source = session.source_path(&app.root, clip);
        let tracking = if result_path(&source).is_file() {
            "ready"
        } else if app.worker_token.is_none() {
            "disabled"
        } else {
            match load(&source)? {
                Some(lease) if lease.failed => "failed",
                Some(lease) if lease.until > now() => "processing",
                Some(lease) if lease.attempts >= MAX_ATTEMPTS => "failed",
                _ => "queued",
            }
        };
        clips.push(ClipStatus { clip: clip.id, tracking });
    }
    Ok(Json(clips))
}

async fn claim(State(app): State<App>, headers: HeaderMap) -> Result<Response, ApiError> {
    require_worker(&app, &headers)?;
    let _lock = app.jobs_lock.lock().await;
    for session in session::list(&app.root).map_err(storage_error)?.into_iter().rev() {
        for clip in &session.clips {
            let source = session.source_path(&app.root, clip);
            if result_path(&source).is_file() {
                continue;
            }
            let previous = load(&source)?;
            if previous
                .as_ref()
                .is_some_and(|lease| lease.until > now() || lease.failed || lease.attempts >= MAX_ATTEMPTS)
            {
                continue;
            }
            let lease = Lease {
                token: Uuid::new_v4(),
                until: now() + LEASE_SECONDS,
                attempts: previous.map_or(1, |lease| lease.attempts + 1),
                failed: false,
            };
            save(&source, &lease)?;
            return Ok(Json(Claimed {
                session: session.id,
                clip: clip.id,
                lease: lease.token,
                split_shots: session.source_url.is_some(),
            })
            .into_response());
        }
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn heartbeat(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    require_worker(&app, &headers)?;
    let _lock = app.jobs_lock.lock().await;
    let source = clip_source(&app, id, clip)?;
    let mut lease = active(&source, &headers)?;
    lease.until = now() + LEASE_SECONDS;
    save(&source, &lease)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn failure(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    require_worker(&app, &headers)?;
    let _lock = app.jobs_lock.lock().await;
    let source = clip_source(&app, id, clip)?;
    let mut lease = active(&source, &headers)?;
    lease.failed = true;
    save(&source, &lease)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn source(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_worker(&app, &headers)?;
    let source = clip_source(&app, id, clip)?;
    active(&source, &headers)?;
    stream(&source, "application/octet-stream", &headers).await
}

async fn movie(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let path = result_path(&clip_source(&app, id, clip)?);
    stream(&path, "video/mp4", &headers).await
}

async fn track_data(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let source = clip_source(&app, id, clip)?;
    let _lock = app.jobs_lock.lock().await;
    if !result_path(&source).is_file() {
        return Err(problem(StatusCode::NOT_FOUND, "Track data is unavailable."));
    }
    let mut response = stream(&data_path(&source), "application/x-ndjson", &headers).await?;
    if let Some(run) = completed_run(&source)? {
        response.headers_mut().insert("x-rink-tracking-run", run.to_string().parse().map_err(storage_error)?);
    }
    Ok(response)
}

async fn upload_data(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    require_worker(&app, &headers)?;
    let source = clip_source(&app, id, clip)?;
    active(&source, &headers)?;
    let (temp, _, size) = receive(&app, body, MAX_DATA).await?;
    if size == 0 {
        return Err(problem(StatusCode::BAD_REQUEST, "Expected track records."));
    }
    publish(&app, &headers, &source, temp, data_path(&source), size).await
}

async fn result(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    require_worker(&app, &headers)?;
    let source = clip_source(&app, id, clip)?;
    active(&source, &headers)?;
    let (temp, mut file, size) = receive(&app, body, MAX_RESULT).await?;
    file.seek(std::io::SeekFrom::Start(0)).await.map_err(storage_error)?;
    let mut signature = [0; 8];
    if file.read_exact(&mut signature).await.is_err() || &signature[4..] != b"ftyp" {
        return Err(problem(StatusCode::BAD_REQUEST, "Expected an MP4 tracking preview."));
    }
    drop(file);
    if !data_path(&source).is_file() {
        return Err(problem(StatusCode::CONFLICT, "Upload the track records before the preview."));
    }
    publish(&app, &headers, &source, temp, result_path(&source), size).await
}

pub(super) async fn receive(
    app: &App,
    mut body: Body,
    limit: usize,
) -> Result<(tempfile::NamedTempFile, File, usize), ApiError> {
    let temp = tempfile::NamedTempFile::new_in(&app.root).map_err(storage_error)?;
    let mut file = File::from_std(temp.reopen().map_err(storage_error)?);
    let mut size = 0;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| problem(StatusCode::BAD_REQUEST, "Incomplete tracking upload."))?;
        if let Ok(data) = frame.into_data() {
            size += data.len();
            if size > limit {
                return Err(problem(StatusCode::PAYLOAD_TOO_LARGE, "Tracking output exceeds the storage limit."));
            }
            file.write_all(&data).await.map_err(storage_error)?;
        }
    }
    file.flush().await.map_err(storage_error)?;
    Ok((temp, file, size))
}

async fn publish(
    app: &App,
    headers: &HeaderMap,
    source: &FilePath,
    temp: tempfile::NamedTempFile,
    destination: PathBuf,
    size: usize,
) -> Result<StatusCode, ApiError> {
    let _edit = app.edit_lock.lock().await;
    let _jobs = app.jobs_lock.lock().await;
    active(source, headers)?;
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

pub(super) async fn stream(path: &FilePath, mime: &str, headers: &HeaderMap) -> Result<Response, ApiError> {
    let mut file = File::open(path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            problem(StatusCode::NOT_FOUND, "Track preview is unavailable.")
        } else {
            storage_error(error)
        }
    })?;
    let length = file.metadata().await.map_err(storage_error)?.len();
    let mut response = Response::builder().header(header::CONTENT_TYPE, mime).header(header::ACCEPT_RANGES, "bytes");
    if let Some(range) = headers.get(header::RANGE) {
        let Some((start, end)) = range.to_str().ok().and_then(|value| byte_range(value, length)) else {
            return response
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{length}"))
                .body(Body::empty())
                .map_err(storage_error);
        };
        file.seek(std::io::SeekFrom::Start(start)).await.map_err(storage_error)?;
        let size = end - start + 1;
        response = response
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{length}"))
            .header(header::CONTENT_LENGTH, size);
        return response.body(Body::from_stream(ReaderStream::new(file.take(size)))).map_err(storage_error);
    }
    response
        .header(header::CONTENT_LENGTH, length)
        .body(Body::from_stream(ReaderStream::new(file)))
        .map_err(storage_error)
}

fn byte_range(value: &str, length: u64) -> Option<(u64, u64)> {
    let last = length.checked_sub(1)?;
    let (left, right) = value.strip_prefix("bytes=")?.split_once('-')?;
    if left.is_empty() {
        let suffix = right.parse::<u64>().ok().filter(|&size| size > 0)?;
        return Some((length.saturating_sub(suffix), last));
    }
    let start = left.parse().ok()?;
    let end = if right.is_empty() { last } else { right.parse::<u64>().ok()?.min(last) };
    (start <= end).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::byte_range;

    #[test]
    fn video_seek_ranges_are_bounded_by_the_file() {
        assert_eq!(byte_range("bytes=0-1", 40), Some((0, 1)));
        assert_eq!(byte_range("bytes=12-", 40), Some((12, 39)));
        assert_eq!(byte_range("bytes=-4", 40), Some((36, 39)));
        assert_eq!(byte_range("bytes=12-99", 40), Some((12, 39)));
        assert_eq!(byte_range("bytes=40-", 40), None);
        assert_eq!(byte_range("bytes=12-3", 40), None);
        assert_eq!(byte_range("bytes=-0", 40), None);
        assert_eq!(byte_range("bytes=0-1", 0), None);
    }
}
