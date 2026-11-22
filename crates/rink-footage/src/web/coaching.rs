//! Serve a reviewed export inside the session it was built from.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path as FilePath, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};
use rink_footage::session::Session;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ApiError, App, identity, jobs, problem, storage_error, stored_session};

const REQUIRED: [&str; 7] =
    ["index.html", "timeline.js", "feedback.js", "coaching.js", "coaching.css", "footage.mp4", "coaching-montage.mp4"];

#[derive(Deserialize, Serialize, PartialEq, Eq)]
struct Selection {
    source_ms: u32,
    track_id: u32,
}

#[derive(Serialize)]
struct Room {
    state: &'static str,
    url: Option<String>,
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/footage/api/sessions/{id}/coaching", get(list))
        .route("/footage/room/{id}/{clip}/", get(index))
        .route("/footage/room/{id}/{clip}/{file}", get(asset))
}

fn folder(source: &FilePath) -> PathBuf {
    source.with_file_name("coaching")
}

fn selections(path: &FilePath) -> Result<Vec<Selection>> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn state(app: &App, session: Uuid, clip: Uuid, source: &FilePath) -> Result<&'static str, ApiError> {
    let selected = selections(&source.with_file_name("focus.json")).map_err(storage_error)?;
    if selected.is_empty() {
        return Ok("choose");
    }
    let room = folder(source);
    if !room.join("index.html").is_file() {
        return Ok("review");
    }
    let used = selections(&room.join("selections.json")).map_err(storage_error)?;
    if selected != used {
        return Ok("changed");
    }
    let anchors = selected.iter().map(|item| (u64::from(item.source_ms), item.track_id)).collect::<Vec<_>>();
    if identity::affects_coaching(app, session, clip, &anchors)? {
        return Ok("identity_changed");
    }
    Ok("ready")
}

async fn list(State(app): State<App>, Path(id): Path<Uuid>) -> Result<Json<BTreeMap<Uuid, Room>>, ApiError> {
    let session = stored_session(&app.root, id)?;
    let mut rooms = BTreeMap::new();
    for clip in &session.clips {
        let state = state(&app, id, clip.id, &session.source_path(&app.root, clip))?;
        let url = (state == "ready").then(|| format!("/footage/room/{}/{}/", id, clip.id));
        rooms.insert(clip.id, Room { state, url });
    }
    Ok(Json(rooms))
}

async fn index(
    State(app): State<App>,
    Path((id, clip)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    file(&app, id, clip, "index.html", &headers).await
}

async fn asset(
    State(app): State<App>,
    Path((id, clip, name)): Path<(Uuid, Uuid, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    file(&app, id, clip, &name, &headers).await
}

async fn file(app: &App, id: Uuid, clip: Uuid, name: &str, headers: &HeaderMap) -> Result<Response, ApiError> {
    let kind = content_type(name).ok_or_else(|| problem(StatusCode::NOT_FOUND, "Review file not found."))?;
    let source = jobs::clip_source(app, id, clip)?;
    if state(app, id, clip, &source)? != "ready" {
        return Err(problem(StatusCode::NOT_FOUND, "The current skater selection does not have a coaching review."));
    }
    jobs::stream(&folder(&source).join(name), kind, headers).await
}

fn content_type(name: &str) -> Option<&'static str> {
    match name {
        "index.html" => Some("text/html; charset=utf-8"),
        "coaching.css" => Some("text/css; charset=utf-8"),
        "coaching.js" | "timeline.js" | "feedback.js" => Some("text/javascript; charset=utf-8"),
        "footage.mp4" | "coaching-montage.mp4" => Some("video/mp4"),
        other => {
            let (index, extension) = other.strip_prefix("sequence-")?.split_once('.')?;
            (!index.is_empty()
                && index.bytes().all(|byte| byte.is_ascii_digit())
                && matches!(extension, "rink.json" | "review.json"))
            .then_some("application/json")
        }
    }
}

/// Local administration command. The export and the selection snapshot must
/// come from the same run of the coaching renderer.
pub fn attach(args: &[OsString]) -> Result<()> {
    let (args, replace) = match args {
        [args @ .., flag] if flag == "--replace" => (args, true),
        args => (args, false),
    };
    let [root, id, clip, export, snapshot] = args else {
        bail!(
            "Usage: --attach-coaching <session-root> <session-id> <clip-id> <room-dir> <focus-snapshot.json> [--replace]"
        );
    };
    let root = FilePath::new(root);
    let id = id.to_str().context("invalid session ID")?.parse()?;
    let clip_id: Uuid = clip.to_str().context("invalid clip ID")?.parse()?;
    let session = Session::load(root, id)?;
    let clip = session.clips.iter().find(|clip| clip.id == clip_id).context("clip is not in this session")?;
    let source = session.source_path(root, clip);
    let used = selections(FilePath::new(snapshot))?;
    ensure!(
        !used.is_empty() && used == selections(&source.with_file_name("focus.json"))?,
        "skater selections changed since rendering"
    );
    let target = folder(&source);
    ensure!(replace || !target.exists(), "a review already exists for this clip");
    let export = FilePath::new(export);
    for name in REQUIRED {
        ensure!(export.join(name).is_file(), "missing export file: {name}");
    }
    let stage = tempfile::tempdir_in(source.parent().context("missing clip folder")?)?;
    for entry in fs::read_dir(export)? {
        let entry = entry?;
        let name = entry.file_name();
        if !entry.file_type()?.is_file() || name.to_str().and_then(content_type).is_none() {
            continue;
        }
        fs::copy(entry.path(), stage.path().join(name))?;
    }
    fs::write(stage.path().join("selections.json"), serde_json::to_vec(&used)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The clip directory itself remains private. Allow its service owner
        // to read the room when this management command is run with sudo.
        fs::set_permissions(stage.path(), fs::Permissions::from_mode(0o755))?;
        for entry in fs::read_dir(stage.path())? {
            fs::set_permissions(entry?.path(), fs::Permissions::from_mode(0o644))?;
        }
    }
    ensure!(used == selections(&source.with_file_name("focus.json"))?, "skater selections changed during publication");
    if target.exists() && replace {
        let backup = tempfile::tempdir_in(source.parent().context("missing clip folder")?)?;
        let previous = backup.path().join("previous");
        fs::rename(&target, &previous)?;
        if let Err(error) = fs::rename(stage.path(), &target) {
            if let Err(restore) = fs::rename(&previous, &target) {
                let location = backup.keep();
                bail!(
                    "Could not publish ({error}) or restore ({restore}); previous review is in {}",
                    location.display()
                );
            }
            return Err(error.into());
        }
    } else {
        fs::rename(stage.path(), target)?;
    }
    println!("/footage/room/{id}/{clip_id}/");
    Ok(())
}
