//! How an agent drives the open app window. Two small JSON files in the
//! library folder, so the CLI and the app need no socket:
//!
//! - `.open-request.json`, written by `rink open`: the play the window should
//!   show, and optionally the moment and player to focus. The app polls it and
//!   acts on each new request once. Asking for the play that's already open
//!   reloads it, so an agent runs `rink open` once when it's done editing.
//! - `.app-state.json`, written by the app: what the window shows right now,
//!   read by `rink current`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::io::{library_dir, IoError};

const REQUEST_FILE: &str = ".open-request.json";
const STATE_FILE: &str = ".app-state.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenRequest {
    /// When the request was made, in ms since the Unix epoch. The app acts on
    /// requests newer than the last one it handled (or its own start).
    pub requested_at_ms: u64,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    pub pid: u32,
    /// None for a play that hasn't been saved yet.
    pub path: Option<PathBuf>,
    pub title: String,
    pub playhead_ms: u32,
    pub selected: Option<String>,
    /// Unsaved edits in the window.
    pub dirty: bool,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn request_path() -> PathBuf {
    library_dir().join(REQUEST_FILE)
}

pub fn state_path() -> PathBuf {
    library_dir().join(STATE_FILE)
}

fn write_json<T: Serialize>(value: &T, path: &Path) -> Result<(), IoError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|source| IoError::Write { path: dir.into(), source })?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value).expect("session files always serialize");
    fs::write(&tmp, text).map_err(|source| IoError::Write { path: tmp.clone(), source })?;
    fs::rename(&tmp, path).map_err(|source| IoError::Write { path: path.into(), source })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

pub fn write_request(request: &OpenRequest) -> Result<(), IoError> {
    write_json(request, &request_path())
}

pub fn read_request() -> Option<OpenRequest> {
    read_json(&request_path())
}

pub fn write_state(state: &AppState) -> Result<(), IoError> {
    write_json(state, &state_path())
}

pub fn read_state() -> Option<AppState> {
    read_json(&state_path())
}

/// Remove the state file if it still describes this process, so `rink
/// current` doesn't report a window that has closed.
pub fn clear_state(pid: u32) {
    if read_state().is_some_and(|s| s.pid == pid) {
        let _ = fs::remove_file(state_path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_without_optional_fields() {
        let r = OpenRequest { requested_at_ms: 5, path: "a.rink.json".into(), at_ms: None, select: None };
        let text = serde_json::to_string(&r).unwrap();
        assert!(!text.contains("\"at_ms\""));
        assert_eq!(serde_json::from_str::<OpenRequest>(&text).unwrap(), r);
    }
}
