//! Import one public YouTube video through the footage worker.
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
use rink_footage::session;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    task::spawn_blocking,
};
use url::Url;
use uuid::Uuid;

use super::{compatible, jobs, problem, require_origin, storage_error, stored_bytes, ApiError, App, MAX_STORED, MIB};

const MAX_VIDEO: usize = 200 * MIB;
const LEASE: u64 = 600;
const MAX_ATTEMPTS: u8 = 2;
const MAX_PENDING: usize = 8;

#[derive(Deserialize)]
struct Link {
    url: String,
}

#[derive(Deserialize, Serialize)]
struct Job {
    id: Uuid,
    video: String,
    created: u64,
    attempts: u8,
    lease: Option<Uuid>,
    until: u64,
    failure: Option<String>,
}

#[derive(Serialize)]
struct ImportStatus {
    id: Uuid,
    state: &'static str,
    error: Option<String>,
}

impl Job {
    fn status(&self) -> ImportStatus {
        let (state, error) = if let Some(failure) = &self.failure {
            ("failed", Some(failure.clone()))
        } else if self.until > jobs::now() {
            ("fetching", None)
        } else if self.attempts >= MAX_ATTEMPTS {
            ("failed", Some("The download worker stopped before finishing. Submit the link again to retry.".into()))
        } else {
            ("queued", None)
        };
        ImportStatus { id: self.id, state, error }
    }
}

#[derive(Serialize)]
struct Claimed {
    id: Uuid,
    video: String,
    lease: Uuid,
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/footage/youtube", post(enqueue).layer(DefaultBodyLimit::max(2048)))
        .route("/api/footage/youtube/{id}", get(status))
        .route("/api/footage/worker/youtube/claim", post(claim))
        .route("/api/footage/worker/youtube/{id}/video", put(publish))
        .route("/api/footage/worker/youtube/{id}/failure", post(fail))
}

fn video_id(link: &str) -> Option<String> {
    let url = Url::parse(link.trim()).ok()?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let id = match (url.host_str()?, url.path()) {
        ("youtu.be", path) => path.strip_prefix('/')?.to_owned(),
        ("youtube.com" | "www.youtube.com" | "m.youtube.com", "/watch") => {
            url.query_pairs().find(|(name, _)| name == "v")?.1.into_owned()
        }
        ("youtube.com" | "www.youtube.com" | "m.youtube.com", path) => {
            path.strip_prefix("/shorts/").or_else(|| path.strip_prefix("/live/"))?.to_owned()
        }
        _ => return None,
    };
    (id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))).then_some(id)
}

fn queue(root: &FilePath) -> PathBuf {
    root.join(".youtube")
}
fn path(root: &FilePath, id: Uuid) -> PathBuf {
    queue(root).join(format!("{id}.json"))
}
fn ready(root: &FilePath, id: Uuid) -> bool {
    root.join(id.to_string()).join("session.json").is_file()
}
fn done(id: Uuid) -> ImportStatus {
    ImportStatus { id, state: "ready", error: None }
}

fn load(root: &FilePath, id: Uuid) -> Result<Job, ApiError> {
    match fs::read(path(root, id)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(storage_error),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(problem(StatusCode::NOT_FOUND, "YouTube import was not found."))
        }
        Err(error) => Err(storage_error(error)),
    }
}

fn save(root: &FilePath, job: &Job) -> Result<(), ApiError> {
    let mut temp = tempfile::NamedTempFile::new_in(queue(root)).map_err(storage_error)?;
    serde_json::to_writer(temp.as_file_mut(), job).map_err(storage_error)?;
    temp.persist(path(root, job.id)).map_err(storage_error)?;
    Ok(())
}

fn pending(root: &FilePath) -> Result<Vec<Job>, ApiError> {
    fs::create_dir_all(queue(root)).map_err(storage_error)?;
    let mut jobs = Vec::new();
    for file in fs::read_dir(queue(root)).map_err(storage_error)? {
        let file = file.map_err(storage_error)?;
        if let Some(id) = file.path().file_stem().and_then(|stem| stem.to_str()).and_then(|stem| stem.parse().ok()) {
            let job = load(root, id)?;
            if ready(root, job.id)
                || (job.status().state == "failed" && jobs::now().saturating_sub(job.created) > 86400)
            {
                fs::remove_file(file.path()).map_err(storage_error)?;
            } else {
                jobs.push(job);
            }
        }
    }
    jobs.sort_by_key(|job| job.created);
    Ok(jobs)
}

async fn enqueue(
    State(app): State<App>,
    headers: HeaderMap,
    Json(link): Json<Link>,
) -> Result<Json<ImportStatus>, ApiError> {
    require_origin(&headers, &app)?;
    if app.worker_token.is_none() {
        return Err(problem(StatusCode::SERVICE_UNAVAILABLE, "The footage worker is unavailable."));
    }
    let video =
        video_id(&link.url).ok_or_else(|| problem(StatusCode::BAD_REQUEST, "Paste a link to one YouTube video."))?;
    let _lock = app.jobs_lock.lock().await;
    let canonical = format!("https://www.youtube.com/watch?v={video}");
    for session in session::list(&app.root).map_err(storage_error)? {
        if session.source_url.as_deref() == Some(&canonical) {
            return Ok(Json(done(session.id)));
        }
    }
    let queue = pending(&app.root)?;
    if let Some(job) = queue.iter().find(|job| job.video == video && job.status().state != "failed") {
        return Ok(Json(job.status()));
    }
    if queue.iter().filter(|job| job.status().state != "failed").count() >= MAX_PENDING {
        return Err(problem(StatusCode::TOO_MANY_REQUESTS, "The video import queue is full. Try again shortly."));
    }
    let job =
        Job { id: Uuid::new_v4(), video, created: jobs::now(), attempts: 0, lease: None, until: 0, failure: None };
    // Reuse the previous ID on a retry so the status link remains useful.
    let job = Job { id: queue.iter().find(|prior| prior.video == job.video).map_or(job.id, |prior| prior.id), ..job };
    save(&app.root, &job)?;
    Ok(Json(job.status()))
}

async fn status(State(app): State<App>, Path(id): Path<Uuid>) -> Result<Json<ImportStatus>, ApiError> {
    let _lock = app.jobs_lock.lock().await;
    Ok(Json(if ready(&app.root, id) { done(id) } else { load(&app.root, id)?.status() }))
}

async fn claim(State(app): State<App>, headers: HeaderMap) -> Result<Response, ApiError> {
    jobs::require_worker(&app, &headers)?;
    let _lock = app.jobs_lock.lock().await;
    let Some(mut job) = pending(&app.root)?.into_iter().find(|job| job.status().state == "queued") else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let lease = Uuid::new_v4();
    job.lease = Some(lease);
    job.until = jobs::now() + LEASE;
    job.attempts += 1;
    save(&app.root, &job)?;
    Ok(Json(Claimed { id: job.id, video: job.video, lease }).into_response())
}

fn active(root: &FilePath, id: Uuid, headers: &HeaderMap) -> Result<Job, ApiError> {
    let lease = headers.get("x-rink-lease").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok());
    let job = load(root, id)?;
    if lease.is_none() || job.lease != lease || job.status().state != "fetching" {
        return Err(problem(StatusCode::CONFLICT, "The YouTube import lease has expired."));
    }
    Ok(job)
}

async fn fail(State(app): State<App>, Path(id): Path<Uuid>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    jobs::require_worker(&app, &headers)?;
    let _lock = app.jobs_lock.lock().await;
    if !ready(&app.root, id) {
        let mut job = active(&app.root, id, &headers)?;
        job.failure =
            Some("Could not import this YouTube video. Check that it is public and try the link again.".into());
        save(&app.root, &job)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn publish(
    State(app): State<App>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    jobs::require_worker(&app, &headers)?;
    {
        let _lock = app.jobs_lock.lock().await;
        if ready(&app.root, id) {
            return Ok(StatusCode::NO_CONTENT);
        }
        active(&app.root, id, &headers)?;
    }
    let (temp, mut file, size) = jobs::receive(&app, body, MAX_VIDEO).await?;
    file.seek(std::io::SeekFrom::Start(0)).await.map_err(storage_error)?;
    let mut signature = [0; 8];
    if file.read_exact(&mut signature).await.is_err() || &signature[4..] != b"ftyp" {
        return Err(problem(StatusCode::BAD_REQUEST, "Expected a YouTube MP4 download."));
    }
    drop(file);
    let edit_lock = app.edit_lock.clone().lock_owned().await;
    let job_lock = app.jobs_lock.clone().lock_owned().await;
    spawn_blocking(move || {
        let (_edit, _jobs) = (edit_lock, job_lock);
        if ready(&app.root, id) {
            return Ok(StatusCode::NO_CONTENT);
        }
        let job = active(&app.root, id, &headers)?;
        if stored_bytes(&app.root).map_err(storage_error)?.saturating_add(size as u64) > MAX_STORED {
            return Err(problem(StatusCode::INSUFFICIENT_STORAGE, "Footage storage is full."));
        }
        let folder = tempfile::tempdir_in(&app.root).map_err(storage_error)?;
        let mut title = headers
            .get("x-rink-title")
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                url::form_urlencoded::parse(format!("title={value}").as_bytes())
                    .next()
                    .map(|(_, title)| {
                        let mut name = String::new();
                        for character in title.chars().filter(|c| !c.is_control() && !matches!(c, '/' | '\\')) {
                            if name.len() + character.len_utf8() > 170 {
                                break;
                            }
                            name.push(character);
                        }
                        name
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        title = title.trim().to_owned();
        if title.is_empty() {
            title = format!("YouTube {}", job.video);
        }
        let video = folder.path().join(format!("{title}.mp4"));
        temp.persist(&video).map_err(storage_error)?;
        let url = format!("https://www.youtube.com/watch?v={}", job.video);
        let outcome = session::import_with_source(&app.root, &[video], id, Some(url), |_, _| {}, compatible::preview)
            .map_err(storage_error)?;
        if outcome.session.is_none() {
            return Err(problem(StatusCode::UNPROCESSABLE_ENTITY, "Could not decode the YouTube video."));
        }
        if let Err(error) = fs::remove_file(path(&app.root, id)) {
            eprintln!("Remove completed YouTube request {id}: {error}");
        }
        Ok(StatusCode::CREATED)
    })
    .await
    .map_err(storage_error)?
}

#[cfg(test)]
mod tests {
    use super::video_id;

    #[test]
    fn imports_an_individual_video_from_youtube_urls() {
        for url in [
            "https://www.youtube.com/watch?v=k-QG45Z4Ssc&list=ignored",
            "https://youtu.be/k-QG45Z4Ssc?t=3",
            "https://m.youtube.com/shorts/k-QG45Z4Ssc",
        ] {
            assert_eq!(video_id(url).as_deref(), Some("k-QG45Z4Ssc"));
        }
        for url in [
            "https://youtube.com.evil.test/watch?v=k-QG45Z4Ssc",
            "https://youtube.com/playlist?list=k-QG45Z4Ssc",
            "https://youtu.be/k-QG45Z4Ssc/anything",
            "https://other@youtube.com/watch?v=k-QG45Z4Ssc",
        ] {
            assert!(video_id(url).is_none(), "{url}");
        }
    }
}
