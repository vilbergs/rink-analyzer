//! An imported session owns the original media and upright preview frames.
//! A session becomes visible to the app after its manifest and media are ready.

use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tempfile::Builder;
use uuid::Uuid;

use crate::{Selection, extract};

const VERSION: u32 = 1;
const MANIFEST: &str = "session.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    pub name: String,
    pub duration_ms: u64,
    pub preview_ms: u64,
    container: Container,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Container {
    Mov,
    Mp4,
}

impl Container {
    fn for_path(path: &Path) -> Result<Self> {
        let extension = path.extension().and_then(OsStr::to_str).map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("mov") => Ok(Self::Mov),
            Some("mp4") => Ok(Self::Mp4),
            _ => anyhow::bail!("Choose a HEVC MOV or MP4 file."),
        }
    }

    fn filename(&self) -> &'static str {
        match self {
            Self::Mov => "source.mov",
            Self::Mp4 => "source.mp4",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    version: u32,
    pub id: Uuid,
    pub created_ms: u64,
    pub clips: Vec<Clip>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

impl Session {
    pub fn source_path(&self, root: &Path, clip: &Clip) -> PathBuf {
        self.clip_dir(root, clip).join(clip.container.filename())
    }

    pub fn preview_path(&self, root: &Path, clip: &Clip) -> PathBuf {
        self.clip_dir(root, clip).join("preview.jpg")
    }

    fn clip_dir(&self, root: &Path, clip: &Clip) -> PathBuf {
        root.join(self.id.to_string()).join(clip.id.to_string())
    }

    pub fn load(root: &Path, id: Uuid) -> Result<Self> {
        let path = root.join(id.to_string()).join(MANIFEST);
        let session: Self = serde_json::from_reader(BufReader::new(
            File::open(&path).with_context(|| format!("Open {}", path.display()))?,
        ))
        .with_context(|| format!("Read {}", path.display()))?;
        ensure!(session.id == id && session.version == VERSION, "Invalid session manifest at {}", path.display());
        Ok(session)
    }
}

pub struct ImportResult {
    pub session: Option<Session>,
    pub failures: Vec<String>,
}

pub fn import(root: &Path, paths: &[PathBuf], on_clip: impl FnMut(usize, &str)) -> Result<ImportResult> {
    import_with_preview(root, paths, on_clip, extract_preview)
}

/// Import a batch with a platform-specific preview decoder. The decoder writes
/// `preview.jpg` in the clip folder and returns its duration and preview time.
pub fn import_with_preview(
    root: &Path,
    paths: &[PathBuf],
    on_clip: impl FnMut(usize, &str),
    preview: impl FnMut(&Path, &Path) -> Result<(u64, u64)>,
) -> Result<ImportResult> {
    import_with_source(root, paths, Uuid::new_v4(), None, on_clip, preview)
}

pub fn import_with_source(
    root: &Path,
    paths: &[PathBuf],
    id: Uuid,
    source_url: Option<String>,
    mut on_clip: impl FnMut(usize, &str),
    mut preview: impl FnMut(&Path, &Path) -> Result<(u64, u64)>,
) -> Result<ImportResult> {
    fs::create_dir_all(root).with_context(|| format!("Create footage library at {}", root.display()))?;
    let stage = Builder::new().prefix(".import-").tempdir_in(root)?;
    let mut session = Session {
        version: VERSION,
        id,
        source_url,
        created_ms: u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?,
        clips: Vec::new(),
    };
    let mut failures = Vec::new();
    for (index, input) in paths.iter().enumerate() {
        let name = input.file_name().unwrap_or(input.as_os_str()).to_string_lossy();
        on_clip(index, &name);
        match import_clip(stage.path(), input, name.to_string(), &mut preview) {
            Ok(clip) => session.clips.push(clip),
            Err(error) => failures.push(format!("{name}: {error:#}")),
        }
    }
    if session.clips.is_empty() {
        return Ok(ImportResult { session: None, failures });
    }

    let mut writer = BufWriter::new(File::create(stage.path().join(MANIFEST))?);
    serde_json::to_writer_pretty(&mut writer, &session)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    drop(writer);
    fs::rename(stage.path(), root.join(session.id.to_string())).context("Save footage session")?;
    Ok(ImportResult { session: Some(session), failures })
}

fn import_clip(
    stage: &Path,
    input: &Path,
    name: String,
    preview: &mut impl FnMut(&Path, &Path) -> Result<(u64, u64)>,
) -> Result<Clip> {
    let container = Container::for_path(input)?;
    ensure!(input.is_file(), "The selected path is not a file.");
    let id = Uuid::new_v4();
    let clip_dir = Builder::new().prefix(".clip-").tempdir_in(stage)?;
    let source = clip_dir.path().join(container.filename());
    fs::copy(input, &source).context("Copy source footage")?;
    let (duration_ms, preview_ms) = preview(&source, clip_dir.path())?;
    fs::rename(clip_dir.path(), stage.join(id.to_string()))?;
    Ok(Clip { id, name, duration_ms, preview_ms, container })
}

pub fn extract_preview(source: &Path, folder: &Path) -> Result<(u64, u64)> {
    let mut preview = None;
    let duration_ms = extract(source, folder, 480, Selection::FirstFrame, |frame| preview = Some(frame))
        .context("Read the first frame as HEVC video")?;
    let frame = preview.context("No preview frame was returned")?;
    fs::rename(frame.path, folder.join("preview.jpg"))?;
    Ok((duration_ms, frame.requested_ms))
}

/// List saved sessions newest first. Unpublished imports are skipped.
pub fn list(root: &Path) -> Result<Vec<Session>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut sessions = Vec::new();
    for entry in entries {
        let entry = entry?;
        let id = entry.file_name().to_str().and_then(|name| Uuid::parse_str(name).ok());
        let Some(id) = id.filter(|_| entry.file_type().is_ok_and(|kind| kind.is_dir())) else { continue };
        sessions.push(Session::load(root, id)?);
    }
    sessions.sort_by_key(|session| std::cmp::Reverse((session.created_ms, session.id)));
    Ok(sessions)
}
