//! `.rink.json` files: one human-readable play per file in a library folder.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use crate::model::{Play, SCHEMA_VERSION};

pub const PLAY_EXTENSION: &str = ".rink.json";
pub const LIBRARY_ENV: &str = "RINK_REPLAY_LIBRARY";

#[derive(Debug, Error)]
pub enum IoError {
    #[error("reading {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("writing {path}: {source}")]
    Write { path: PathBuf, source: std::io::Error },
    #[error("parsing {path}: {source}")]
    Parse { path: PathBuf, source: serde_json::Error },
    #[error("{path} has schema version {found}, newer than this build supports ({SCHEMA_VERSION})")]
    TooNew { path: PathBuf, found: u32 },
}

/// The library folder: `$RINK_REPLAY_LIBRARY` if set, else `plays/` under the
/// platform data dir. The app and the MCP server both resolve it this way.
pub fn library_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(LIBRARY_ENV) {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("", "", "rink-replay")
        .map(|d| d.data_dir().join("plays"))
        .unwrap_or_else(|| PathBuf::from("plays"))
}

/// Parse a play from JSON text, migrating older schema versions.
pub fn from_json(text: &str) -> Result<Play, serde_json::Error> {
    let mut value: Value = serde_json::from_str(text)?;
    migrate(&mut value);
    let mut play: Play = serde_json::from_value(value)?;
    play.normalize();
    Ok(play)
}

pub fn to_json(play: &Play) -> String {
    serde_json::to_string_pretty(play).expect("a Play always serializes")
}

/// Bring a raw play document up to [`SCHEMA_VERSION`]. Each future version
/// adds one step here; v1 is the first, so there is nothing to do yet.
fn migrate(value: &mut Value) {
    let version = value.get("schema_version").and_then(Value::as_u64).unwrap_or(1);
    if version < 1 {
        unreachable!("schema versions start at 1");
    }
}

pub fn load(path: &Path) -> Result<Play, IoError> {
    let text = fs::read_to_string(path).map_err(|source| IoError::Read { path: path.into(), source })?;
    let version = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v.get("schema_version").and_then(Value::as_u64))
        .unwrap_or(1) as u32;
    if version > SCHEMA_VERSION {
        return Err(IoError::TooNew { path: path.into(), found: version });
    }
    from_json(&text).map_err(|source| IoError::Parse { path: path.into(), source })
}

/// Write atomically (temp file + rename) so a crash mid-save never leaves a
/// half-written play for the MCP server to read.
pub fn save(play: &Play, path: &Path) -> Result<(), IoError> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, to_json(play)).map_err(|source| IoError::Write { path: tmp.clone(), source })?;
    fs::rename(&tmp, path).map_err(|source| IoError::Write { path: path.into(), source })
}

/// File name for a play: a slug of its title plus the start of its id, so
/// titles can repeat without clobbering each other.
pub fn file_name(play: &Play) -> String {
    let slug: String = play
        .title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let short_id = &play.id.simple().to_string()[..8];
    if slug.is_empty() {
        format!("{short_id}{PLAY_EXTENSION}")
    } else {
        format!("{slug}-{short_id}{PLAY_EXTENSION}")
    }
}

/// All play files in `dir`, sorted by path. Missing dir = empty library.
pub fn list_play_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(PLAY_EXTENSION)))
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rink::RinkSpec;
    use crate::templates::{build, Template};

    #[test]
    fn round_trips() {
        let play = build(Template::Breakout, RinkSpec::iihf());
        let back = from_json(&to_json(&play)).unwrap();
        assert_eq!(play, back);
    }

    #[test]
    fn file_name_is_slugged() {
        let mut play = Play::new("D-zone breakout: 2nd period!", RinkSpec::iihf());
        play.id = uuid::Uuid::nil();
        assert_eq!(file_name(&play), "d-zone-breakout-2nd-period-00000000.rink.json");
    }

    #[test]
    fn save_and_list() {
        let dir = std::env::temp_dir().join(format!("rink-io-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let play = build(Template::Faceoff, RinkSpec::iihf());
        let path = dir.join(file_name(&play));
        save(&play, &path).unwrap();
        assert_eq!(list_play_files(&dir), vec![path.clone()]);
        assert_eq!(load(&path).unwrap(), play);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_newer_schema() {
        let dir = std::env::temp_dir().join(format!("rink-io-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let mut value = serde_json::to_value(build(Template::Faceoff, RinkSpec::iihf())).unwrap();
        value["schema_version"] = (SCHEMA_VERSION + 1).into();
        let path = dir.join("future.rink.json");
        fs::write(&path, value.to_string()).unwrap();
        assert!(matches!(load(&path), Err(IoError::TooNew { .. })));
        fs::remove_dir_all(&dir).unwrap();
    }
}
