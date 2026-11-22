//! The footage service: the API under /footage/api, the worker queue and exported film rooms.
//! Its pages are part of the web app in `web/`, which the same site serves at /footage.

mod coaching;
mod compatible;
mod focus;
mod identity;
mod jobs;
mod youtube;

use std::{
    env, fs,
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rink_footage::session::{self, Session};
use serde::Serialize;
use tokio::{fs::File, io::AsyncWriteExt, net::TcpListener, sync::Mutex, task::spawn_blocking};
use uuid::Uuid;

const MIB: usize = 1024 * 1024;
const MAX_BATCH: usize = 1024 * MIB;
const MAX_STORED: u64 = 3 * 1024 * MIB as u64;
const MAX_CLIPS: usize = 32;

#[derive(Clone)]
struct App {
    root: PathBuf,
    origin: String,
    edit_lock: Arc<Mutex<()>>,
    jobs_lock: Arc<Mutex<()>>,
    worker_token: Option<String>,
}

#[derive(Serialize)]
struct Imported {
    session: Option<Session>,
    failures: Vec<String>,
}

#[derive(Serialize)]
struct RecentSession {
    preview_clips: Vec<Uuid>,
    duration_ms: u64,
    id: Uuid,
    name: String,
    created_ms: u64,
    clip_count: usize,
}

#[derive(Serialize)]
struct Problem {
    error: String,
}

type ApiError = (StatusCode, Json<Problem>);

fn problem(code: StatusCode, message: impl Into<String>) -> ApiError {
    (code, Json(Problem { error: message.into() }))
}

fn storage_error(error: impl std::fmt::Display) -> ApiError {
    eprintln!("Footage storage: {error}");
    problem(StatusCode::INTERNAL_SERVER_ERROR, "Could not access the footage library.")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--attach-coaching") {
        return coaching::attach(&args[1..]);
    }
    let root = PathBuf::from(env::var_os("RINK_FOOTAGE_DIR").context("Set RINK_FOOTAGE_DIR")?);
    fs::create_dir_all(&root)?;
    let app = App {
        root,
        origin: env::var("RINK_FOOTAGE_ORIGIN").context("Set RINK_FOOTAGE_ORIGIN")?,
        edit_lock: Arc::new(Mutex::new(())),
        jobs_lock: Arc::new(Mutex::new(())),
        worker_token: env::var("RINK_FOOTAGE_WORKER_TOKEN").ok().filter(|token| !token.is_empty()),
    };
    let routes = Router::new()
        .route("/footage/api/health", get(|| async { "ok" }))
        .route(
            "/footage/api/sessions",
            get(recent).merge(post(import).layer(DefaultBodyLimit::max(MAX_BATCH + 2 * MIB))),
        )
        .route("/footage/api/sessions/{id}", get(details).delete(remove))
        .route("/footage/api/sessions/{id}/clips/{clip_id}/preview", get(preview))
        .merge(jobs::routes())
        .merge(youtube::routes())
        .merge(focus::routes())
        .merge(identity::routes())
        .merge(coaching::routes())
        .with_state(app)
        .layer(axum::middleware::map_response(|mut response: Response| async {
            let headers = response.headers_mut();
            headers.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
            headers.insert(header::REFERRER_POLICY, header::HeaderValue::from_static("no-referrer"));
            headers.insert(header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff"));
            headers.insert(
                header::CONTENT_SECURITY_POLICY,
                header::HeaderValue::from_static("default-src 'self'; frame-ancestors 'none'; base-uri 'none'"),
            );
            response
        }));
    let bind = env::var("RINK_FOOTAGE_BIND").unwrap_or_else(|_| "127.0.0.1:8794".into());
    let listener = TcpListener::bind(&bind).await?;
    println!("Footage importer listening on {bind}");
    axum::serve(listener, routes).await?;
    Ok(())
}

fn require_origin(headers: &HeaderMap, app: &App) -> Result<(), ApiError> {
    if headers.get(header::ORIGIN).and_then(|value| value.to_str().ok()) != Some(app.origin.as_str()) {
        return Err(problem(StatusCode::FORBIDDEN, "Open the footage page to make changes."));
    }
    Ok(())
}

async fn import(
    State(app): State<App>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    require_origin(&headers, &app)?;
    let lock = app
        .edit_lock
        .clone()
        .try_lock_owned()
        .map_err(|_| problem(StatusCode::CONFLICT, "Another upload is in progress. Try again when it finishes."))?;
    let root = app.root;
    let usage_root = root.clone();
    let used =
        spawn_blocking(move || stored_bytes(&usage_root)).await.map_err(storage_error)?.map_err(storage_error)?;
    let available = MAX_STORED.saturating_sub(used);
    let limit = available.min(MAX_BATCH as u64);
    let inbox = tempfile::Builder::new().prefix(".upload-").tempdir_in(&root).map_err(storage_error)?;
    let mut paths = Vec::new();
    let mut bytes = 0;
    while let Some(mut field) = multipart.next_field().await.map_err(multipart_error)? {
        let name = field
            .file_name()
            .filter(|_| field.name() == Some("clips"))
            .and_then(|name| name.rsplit(['/', '\\']).next())
            .filter(|name| !name.is_empty() && name.len() <= 180 && *name != "." && *name != "..")
            .ok_or_else(|| problem(StatusCode::BAD_REQUEST, "Each upload needs a valid clip name."))?;
        if paths.len() == MAX_CLIPS {
            return Err(problem(StatusCode::PAYLOAD_TOO_LARGE, "Import up to 32 clips in one session."));
        }
        let dir = inbox.path().join(paths.len().to_string());
        tokio::fs::create_dir(&dir).await.map_err(storage_error)?;
        let target = dir.join(name);
        let mut file = File::create(&target).await.map_err(storage_error)?;
        while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
            bytes += chunk.len() as u64;
            if bytes > limit {
                return Err(problem(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    if available < MAX_BATCH as u64 {
                        "The footage library is full. Delete a saved session to free space."
                    } else {
                        "This batch exceeds the 1 GiB upload limit."
                    },
                ));
            }
            file.write_all(&chunk).await.map_err(storage_error)?;
        }
        file.flush().await.map_err(storage_error)?;
        paths.push(target);
    }
    if paths.is_empty() {
        return Err(problem(StatusCode::BAD_REQUEST, "Choose footage to import."));
    }
    let outcome = spawn_blocking(move || {
        let (_inbox, _lock) = (inbox, lock);
        session::import_with_preview(&root, &paths, |_, _| {}, compatible::preview)
    })
    .await
    .map_err(storage_error)?
    .map_err(storage_error)?;
    let status = if outcome.session.is_some() { StatusCode::CREATED } else { StatusCode::UNPROCESSABLE_ENTITY };
    Ok((status, Json(Imported { session: outcome.session, failures: outcome.failures })))
}

fn multipart_error(error: axum::extract::multipart::MultipartError) -> ApiError {
    let status = error.status();
    let message = if status == StatusCode::PAYLOAD_TOO_LARGE {
        "This batch exceeds the 1 GiB upload limit."
    } else {
        "The upload was interrupted or unreadable."
    };
    problem(status, message)
}

fn stored_bytes(root: &FilePath) -> Result<u64> {
    let mut total = 0;
    for session in session::list(root)? {
        for clip in &session.clips {
            total += fs::metadata(session.source_path(root, clip))?.len();
            total += fs::metadata(session.preview_path(root, clip))?.len();
            let source = session.source_path(root, clip);
            for output in [jobs::result_path(&source), jobs::data_path(&source)] {
                match fs::metadata(output) {
                    Ok(metadata) => total += metadata.len(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    Ok(total)
}

fn stored_session(root: &FilePath, id: Uuid) -> Result<Session, ApiError> {
    Session::load(root, id).map_err(|error| {
        if error.downcast_ref::<std::io::Error>().is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) {
            problem(StatusCode::NOT_FOUND, "That footage session could not be found.")
        } else {
            storage_error(error)
        }
    })
}

async fn details(State(app): State<App>, Path(id): Path<Uuid>) -> Result<Json<Session>, ApiError> {
    Ok(Json(stored_session(&app.root, id)?))
}

async fn recent(State(app): State<App>) -> Result<Json<Vec<RecentSession>>, ApiError> {
    let sessions =
        spawn_blocking(move || session::list(&app.root)).await.map_err(storage_error)?.map_err(storage_error)?;
    Ok(Json(
        sessions
            .into_iter()
            .filter_map(|session| {
                Some(RecentSession {
                    preview_clips: session.clips.iter().take(3).map(|clip| clip.id).collect(),
                    duration_ms: session.clips.iter().map(|clip| clip.duration_ms).sum(),
                    id: session.id,
                    name: session.clips.first()?.name.clone(),
                    created_ms: session.created_ms,
                    clip_count: session.clips.len(),
                })
            })
            .collect(),
    ))
}

async fn preview(
    State(app): State<App>,
    Path((id, clip_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let session = stored_session(&app.root, id)?;
    let clip = session
        .clips
        .iter()
        .find(|clip| clip.id == clip_id)
        .ok_or_else(|| problem(StatusCode::NOT_FOUND, "That clip could not be found."))?;
    let bytes = tokio::fs::read(session.preview_path(&app.root, clip)).await.map_err(storage_error)?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg")], bytes))
}

async fn remove(State(app): State<App>, Path(id): Path<Uuid>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    require_origin(&headers, &app)?;
    let _lock = app
        .edit_lock
        .try_lock()
        .map_err(|_| problem(StatusCode::CONFLICT, "Wait for the upload to finish before deleting a session."))?;
    let _jobs = app.jobs_lock.lock().await;
    stored_session(&app.root, id)?;
    tokio::fs::remove_dir_all(app.root.join(id.to_string())).await.map_err(storage_error)?;
    Ok(StatusCode::NO_CONTENT)
}
