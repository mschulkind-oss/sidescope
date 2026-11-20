//! Capture directories: the preflight, exclusive owner-only writes and the
//! manifest `--from-capture` reads back. Raw stdout reaches disk only here.

use super::report::ReportError;
use crate::chart::schema::CAPTURE_SCHEMA_ID;
use crate::chart::time::parse_instant;
use serde::{Deserialize, Serialize};
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Largest file `replay` or `--from-capture` reads: the stdout bound.
pub const READ_LIMIT: u64 = 1_048_576;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestWindow {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestClocks {
    pub request_at: Option<String>,
    pub model_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSource {
    pub id: String,
    pub window: Option<ManifestWindow>,
    pub timezone: String,
    pub clocks: ManifestClocks,
    pub bytes: usize,
    pub captured: bool,
    /// `ok`, or the source error id.
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub complete: bool,
    pub invocation_at: String,
    pub sources: Vec<ManifestSource>,
}

/// Reads a regular file of at most `READ_LIMIT` bytes.
pub fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.kind().to_string())?;
    if !metadata.is_file() {
        return Err("not a regular file".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.kind().to_string())?
        .take(READ_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.kind().to_string())?;
    if bytes.len() as u64 > READ_LIMIT {
        return Err(format!("larger than {READ_LIMIT} bytes"));
    }
    Ok(bytes)
}

fn writable_directory(path: &Path) -> bool {
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c_path` is a valid NUL-terminated string for the call.
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

fn stdout_name(source: &str) -> String {
    format!("{source}.stdout")
}

/// Everything checked before the first spawn. Writes nothing.
pub fn preflight(dir: &Path, source_ids: &[&str]) -> Result<(), ReportError> {
    let refuse = |text: String| Err(ReportError::new("capture-path", text));
    if dir.exists() || dir.symlink_metadata().is_ok() {
        if !dir.is_dir() {
            return refuse(format!("{} exists and is not a directory", dir.display()));
        }
        if !writable_directory(dir) {
            return refuse(format!("{} is not writable", dir.display()));
        }
        let mut names = vec!["manifest.json".to_string()];
        names.extend(source_ids.iter().map(|id| stdout_name(id)));
        for name in names {
            if dir.join(&name).symlink_metadata().is_ok() {
                return refuse(format!("{} already holds {name}", dir.display()));
            }
        }
        return Ok(());
    }
    let parent = match dir.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if !parent.is_dir() {
        return refuse(format!(
            "the parent of {} is not a directory",
            dir.display()
        ));
    }
    if !writable_directory(parent) {
        return refuse(format!("the parent of {} is not writable", dir.display()));
    }
    Ok(())
}

/// Writes capture files for one run; remembers what could not be written.
pub struct CaptureWriter {
    dir: PathBuf,
    pub sources: Vec<ManifestSource>,
    pub failures: Vec<ReportError>,
}

impl CaptureWriter {
    /// Creates the directory owner-only when it does not exist yet.
    pub fn create(dir: &Path) -> Result<Self, ReportError> {
        if !dir.is_dir() {
            DirBuilder::new().mode(0o700).create(dir).map_err(|error| {
                ReportError::new(
                    "capture-path",
                    format!("cannot create {}: {}", dir.display(), error.kind()),
                )
            })?;
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            sources: Vec::new(),
            failures: Vec::new(),
        })
    }

    fn create_new(&self, name: &str) -> std::io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.dir.join(name))
    }

    /// Writes a source's exit-0 stdout; returns whether the file was kept.
    pub fn write_stdout(&mut self, source: &str, bytes: &[u8]) -> bool {
        let result = self
            .create_new(&stdout_name(source))
            .and_then(|mut file| file.write_all(bytes).and_then(|()| file.flush()));
        match result {
            Ok(()) => true,
            Err(error) => {
                let mut failure = ReportError::new(
                    "capture-write",
                    format!("cannot write {}: {}", stdout_name(source), error.kind()),
                );
                failure.source = Some(source.to_string());
                self.failures.push(failure);
                false
            }
        }
    }

    /// Written once, last. A failure is recorded, nothing is retried.
    pub fn write_manifest(&mut self, invocation_at: String) {
        let manifest = Manifest {
            schema: CAPTURE_SCHEMA_ID.to_string(),
            complete: self.failures.is_empty(),
            invocation_at,
            sources: self.sources.clone(),
        };
        let mut text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
        text.push('\n');
        let result = self
            .create_new("manifest.json")
            .and_then(|mut file| file.write_all(text.as_bytes()).and_then(|()| file.flush()));
        if let Err(error) = result {
            self.failures.push(ReportError::new(
                "capture-write",
                format!("cannot write manifest.json: {}", error.kind()),
            ));
        }
    }
}

/// Reads and checks a capture manifest; any fault is `data-unreadable`.
pub fn load_manifest(dir: &Path) -> Result<Manifest, String> {
    let bytes = read_bounded(&dir.join("manifest.json"))
        .map_err(|why| format!("{}/manifest.json: {why}", dir.display()))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}/manifest.json: {error}", dir.display()))?;
    let bad = |why: &str| Err(format!("{}/manifest.json: {why}", dir.display()));
    if manifest.schema != CAPTURE_SCHEMA_ID {
        return bad("unknown capture schema");
    }
    if parse_instant(&manifest.invocation_at).is_err() {
        return bad("invocation_at is not an instant");
    }
    let mut seen = Vec::new();
    for source in &manifest.sources {
        if seen.contains(&source.id.as_str()) {
            return bad("a source appears twice");
        }
        seen.push(source.id.as_str());
        if parse_instant(&source.clocks.model_at).is_err()
            || source
                .clocks
                .request_at
                .as_deref()
                .is_some_and(|at| parse_instant(at).is_err())
        {
            return bad("a recorded clock is not an instant");
        }
        if let Some(window) = &source.window
            && (parse_instant(&window.from).is_err() || parse_instant(&window.to).is_err())
        {
            return bad("a recorded window is not an instant");
        }
    }
    Ok(manifest)
}

pub fn stdout_path(dir: &Path, source: &str) -> PathBuf {
    dir.join(stdout_name(source))
}
