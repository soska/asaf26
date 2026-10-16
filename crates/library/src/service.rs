//! Service files on disk: atomic writes (for saves and the autosave) and
//! reading, plus finding a service's songs again in the library.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use asaf_core::{AutosaveInfo, SERVICE_EXTENSION, Service, Song, service_from_json, service_to_json};
use rusqlite::OptionalExtension;

use crate::{Library, Result};

/// Name of the autosave file inside the data folder.
pub const AUTOSAVE_FILE: &str = "autosave.asaf-service";

/// Writes `contents` to `path` so that a crash or power cut leaves either the
/// old file or the new one, never a mix: write a temporary file next to it,
/// flush it to disk, then rename it over the target.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    write_atomic_bytes(path, contents.as_bytes())
}

/// [`write_atomic`] for any bytes.
pub fn write_atomic_bytes(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    // Unique per write: the background autosave and a save or the quit-time
    // flush can overlap, and must not share a temporary file.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut tmp_name = name.to_os_string();
    tmp_name.push(format!(".tmp-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let tmp = dir.join(tmp_name);

    let result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        // Make the rename itself durable.
        #[cfg(unix)]
        File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Saves `service` as an `.asaf-service` file.
pub fn write_service(path: &Path, service: &Service) -> io::Result<()> {
    write_atomic(path, &service_to_json(service, None))
}

/// Saves the autosave copy of `service`.
pub fn write_autosave(path: &Path, service: &Service, info: &AutosaveInfo) -> io::Result<()> {
    write_atomic(path, &service_to_json(service, Some(info)))
}

/// Why a service file couldn't be read. `Display` is the English message
/// (the console words its own).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceFileError {
    /// The file couldn't be read at all.
    Unreadable { path: PathBuf, error: String },
    /// It was read but isn't a service file.
    Invalid { file: String, error: String },
}

impl std::fmt::Display for ServiceFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceFileError::Unreadable { path, error } => write!(f, "Can't read {}: {error}", path.display()),
            ServiceFileError::Invalid { file, error } => write!(f, "Can't open {file}: {error}"),
        }
    }
}

impl std::error::Error for ServiceFileError {}

/// Reads a service (or autosave) file.
pub fn read_service(path: &Path) -> std::result::Result<(Service, Option<AutosaveInfo>), ServiceFileError> {
    let text = fs::read_to_string(path)
        .map_err(|e| ServiceFileError::Unreadable { path: path.to_path_buf(), error: e.to_string() })?;
    service_from_json(&text).map_err(|e| ServiceFileError::Invalid { file: file_label(path), error: e.to_string() })
}

/// What a restart finds in the autosave file.
#[derive(Debug, Default)]
pub struct Restored {
    pub service: Service,
    pub info: AutosaveInfo,
    /// Why the autosave couldn't be used, if it existed but was unreadable.
    /// The file is kept beside it as `…corrupt` for inspection.
    pub problem: Option<RestoreProblem>,
}

/// An autosave that couldn't be read, and where it was moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreProblem {
    pub error: ServiceFileError,
    pub kept_as: PathBuf,
}

impl std::fmt::Display for RestoreProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (kept as {})", self.error, self.kept_as.display())
    }
}

/// Loads the autosave from `path`, if any. A damaged autosave is moved
/// aside (never deleted) and an empty service is returned.
pub fn restore_autosave(path: &Path) -> Restored {
    if !path.exists() {
        return Restored::default();
    }
    match read_service(path) {
        Ok((service, info)) => Restored { service, info: info.unwrap_or_default(), problem: None },
        Err(e) => {
            // Never over an earlier damaged copy.
            let aside = (1..)
                .map(|n| match n {
                    1 => path.with_extension(format!("{SERVICE_EXTENSION}.corrupt")),
                    n => path.with_extension(format!("{SERVICE_EXTENSION}.corrupt-{n}")),
                })
                .find(|p| !p.exists())
                .expect("some name is free");
            let _ = fs::rename(path, &aside);
            Restored { problem: Some(RestoreProblem { error: e, kept_as: aside }), ..Default::default() }
        }
    }
}

/// `path` with the `.asaf-service` extension (added if missing).
pub fn with_service_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|e| e == SERVICE_EXTENSION) {
        path
    } else {
        let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".");
        name.push(SERVICE_EXTENSION);
        path.with_file_name(name)
    }
}

/// The file name without its extension: the service's name.
pub fn service_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn file_label(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

impl Library {
    /// A song with exactly this title (ignoring case), the oldest first.
    pub fn find_song_by_title(&self, title: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM songs WHERE title = ?1 COLLATE NOCASE ORDER BY id LIMIT 1",
                [title.trim()],
                |r| r.get(0),
            )
            .optional()
    }

    /// The song a service item means: the one with `id` if its title still
    /// matches, else one with `title` (a service from another machine).
    /// Never a song that merely has the same id under another title: on
    /// another machine that's an unrelated song, and projecting the wrong
    /// lyrics is worse than reporting the item missing.
    pub fn find_service_song(&self, id: i64, title: &str) -> Result<Option<Song>> {
        if let Some(song) = self.get(id)?
            && song.title.trim().eq_ignore_ascii_case(title.trim())
        {
            return Ok(Some(song));
        }
        match self.find_song_by_title(title)? {
            Some(other) => self.get(other),
            None => Ok(None),
        }
    }
}
