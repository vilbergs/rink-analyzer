//! How an agent drives the open app window. Two small JSON files in the
//! library folder, so the CLI and the app need no socket:
//!
//! - `.open-request.json`, written by `rink open`: the play the window should
//!   show, and optionally the moment and player to focus. The app polls it and
//!   acts on each new request once. Asking for the play that's already open
//!   reloads it, so an agent runs `rink open` once when it's done editing.
//! - `.app-state.json`, written by the app: what the window shows right now,
//!   read by `rink current`.
//!
//! A `rink://open` link in a note reaches the installed app as an open-URL
//! event; the app turns it into the same request (`parse_open_url`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::io::{library_dir, IoError, PLAY_EXTENSION};

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

#[derive(Debug, PartialEq, Error)]
pub enum UrlError {
    #[error("{0:?} isn't a rink://open?... link")]
    NotOpenUrl(String),
    #[error("the link names no play; add play=<name or absolute path>")]
    NoPlay,
    #[error("play={0:?} is neither a library play name nor an absolute path")]
    BadPlay(String),
    #[error("at={0:?} isn't a time in ms")]
    BadAt(String),
    #[error("{0:?} has a broken %-escape")]
    BadEscape(String),
}

/// Parse `rink://open?play=<name or absolute path>&at=<ms>&select=<entity id>`.
/// A name is a play in the library folder, without `.rink.json`. Values are
/// percent-decoded; unknown keys are ignored.
pub fn parse_open_url(url: &str) -> Result<OpenRequest, UrlError> {
    let query = url
        .strip_prefix("rink://open")
        .map(|rest| rest.strip_prefix('/').unwrap_or(rest))
        .and_then(|rest| rest.strip_prefix('?'))
        .ok_or_else(|| UrlError::NotOpenUrl(url.into()))?;
    let (mut play, mut at_ms, mut select) = (None, None, None);
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, raw) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(raw).ok_or_else(|| UrlError::BadEscape(raw.into()))?;
        match key {
            "play" => play = Some(value),
            "at" => at_ms = Some(value.parse().map_err(|_| UrlError::BadAt(value))?),
            "select" => select = Some(value),
            _ => {}
        }
    }
    let play = play.filter(|p| !p.is_empty()).ok_or(UrlError::NoPlay)?;
    let path = if Path::new(&play).is_absolute() {
        PathBuf::from(play)
    } else if play.contains('/') {
        return Err(UrlError::BadPlay(play));
    } else {
        library_dir().join(format!("{play}{PLAY_EXTENSION}"))
    };
    Ok(OpenRequest { requested_at_ms: now_ms(), path, at_ms, select: select.filter(|s| !s.is_empty()) })
}

fn percent_decode(s: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(s.len());
    let mut it = s.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let hex = [it.next()?, it.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_names_a_library_play_with_moment_and_player() {
        let r = parse_open_url("rink://open?play=01-dzone-strong-side-wall&at=2500&select=home_LW").unwrap();
        assert_eq!(r.path, library_dir().join("01-dzone-strong-side-wall.rink.json"));
        assert_eq!(r.at_ms, Some(2500));
        assert_eq!(r.select.as_deref(), Some("home_LW"));
    }

    #[test]
    fn url_takes_a_percent_encoded_absolute_path() {
        let r = parse_open_url("rink://open/?play=%2FUsers%2Fme%2FApplication%20Support%2Fa.rink.json&at=0").unwrap();
        assert_eq!(r.path, PathBuf::from("/Users/me/Application Support/a.rink.json"));
        assert_eq!(r.at_ms, Some(0));
    }

    #[test]
    fn url_moment_and_player_are_optional() {
        let r = parse_open_url("rink://open?play=breakout").unwrap();
        assert_eq!((r.at_ms, r.select), (None, None));
        let r = parse_open_url("rink://open?play=breakout&select=").unwrap();
        assert_eq!(r.select, None);
    }

    #[test]
    fn url_rejects_other_schemes_and_actions() {
        for url in ["http://open?play=a", "rink://edit?play=a", "rink://opened?play=a", "rink://open"] {
            assert_eq!(parse_open_url(url), Err(UrlError::NotOpenUrl(url.into())), "{url}");
        }
    }

    #[test]
    fn url_rejects_a_bad_moment() {
        assert_eq!(parse_open_url("rink://open?play=a&at=soon"), Err(UrlError::BadAt("soon".into())));
        assert_eq!(parse_open_url("rink://open?play=a&at=-5"), Err(UrlError::BadAt("-5".into())));
    }

    #[test]
    fn url_rejects_missing_play_relative_paths_and_broken_escapes() {
        assert_eq!(parse_open_url("rink://open?at=5"), Err(UrlError::NoPlay));
        assert_eq!(parse_open_url("rink://open?play=plays/a"), Err(UrlError::BadPlay("plays/a".into())));
        assert_eq!(parse_open_url("rink://open?play=a%2"), Err(UrlError::BadEscape("a%2".into())));
    }

    #[test]
    fn request_round_trips_without_optional_fields() {
        let r = OpenRequest { requested_at_ms: 5, path: "a.rink.json".into(), at_ms: None, select: None };
        let text = serde_json::to_string(&r).unwrap();
        assert!(!text.contains("\"at_ms\""));
        assert_eq!(serde_json::from_str::<OpenRequest>(&text).unwrap(), r);
    }
}
