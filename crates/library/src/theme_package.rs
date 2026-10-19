//! Theme files (`.asaf-theme`), for passing themes between computers: a zip
//! holding `theme.json` (the theme, in a small envelope naming the format)
//! and, under `media/`, the imported images and video it uses. Built-in
//! assets (`/assets/…`) aren't included; every Asaf has them.
//!
//! Importing never replaces anything: the media go into the media folder
//! under their own content-hashed names, and the theme is saved as a new
//! one ("Aurora 2" if there's already an "Aurora").

use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use asaf_core::Theme;
use serde_json::{Value, json};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Library;
use crate::media::{MAX_MEDIA_BYTES, MEDIA_URL_PREFIX, is_media_extension, is_media_file_name};

pub const THEME_EXTENSION: &str = "asaf-theme";
/// The `format` in `theme.json`.
pub const THEME_FORMAT: &str = "asaf-theme";
/// Bumped when a change would mislead an older Asaf; theme fields added
/// later don't need it (missing ones take their defaults).
pub const THEME_FILE_VERSION: u64 = 1;

const THEME_ENTRY: &str = "theme.json";
const MEDIA_DIR: &str = "media/";
/// More entries than a theme could use means it isn't one of ours.
const MAX_ENTRIES: usize = 64;
const MAX_THEME_JSON_BYTES: u64 = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeFileError {
    /// The file couldn't be read or written.
    Io(String),
    /// Not a theme file (not a zip, or no `theme.json` in our format).
    NotATheme,
    /// Made by a newer Asaf.
    Newer(u64),
    /// The theme uses a media file the package doesn't have, or one that
    /// isn't an image or video.
    MissingMedia(String),
    /// A media file larger than the media folder takes.
    TooLarge(String),
    /// The media folder or the library couldn't be updated.
    Storage(String),
}

impl fmt::Display for ThemeFileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ThemeFileError::Io(e) => write!(f, "{e}"),
            ThemeFileError::NotATheme => write!(f, "This isn't an Asaf theme file."),
            ThemeFileError::Newer(v) => write!(f, "This theme file is from a newer version of Asaf (format {v})."),
            ThemeFileError::MissingMedia(name) => write!(f, "The theme file is missing {name}."),
            ThemeFileError::TooLarge(name) => write!(f, "{name} in the theme file is too large."),
            ThemeFileError::Storage(e) => write!(f, "The library couldn't be updated: {e}"),
        }
    }
}

impl std::error::Error for ThemeFileError {}

/// `path` with the `.asaf-theme` extension (added if missing).
pub fn with_theme_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|e| e == THEME_EXTENSION) {
        path
    } else {
        let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".");
        name.push(THEME_EXTENSION);
        path.with_file_name(name)
    }
}

/// The theme's imported media, by file name (`sky-1a2b3c4d.jpg`).
fn media_names(theme: &Theme) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for url in [&theme.background_image, &theme.background_video, &theme.logo] {
        if let Some(name) = url.as_deref().and_then(|u| u.strip_prefix(MEDIA_URL_PREFIX))
            && !names.iter().any(|n| n == name)
        {
            names.push(name.to_string());
        }
    }
    names
}

fn io(path: &Path, e: impl fmt::Display) -> ThemeFileError {
    ThemeFileError::Io(format!("{}: {e}", path.display()))
}

impl Library {
    /// Writes `theme` and the media it uses to `path`. Replaces the file
    /// only once the new one is complete.
    pub fn export_theme(&self, theme: &Theme, path: &Path) -> Result<(), ThemeFileError> {
        let mut media = Vec::new();
        for name in media_names(theme) {
            let file = self
                .media_path(&format!("{MEDIA_URL_PREFIX}{name}"))
                .filter(|p| p.is_file())
                .ok_or_else(|| ThemeFileError::MissingMedia(name.clone()))?;
            media.push((name, file));
        }
        let envelope = json!({ "format": THEME_FORMAT, "version": THEME_FILE_VERSION, "theme": theme });
        let json = serde_json::to_vec_pretty(&envelope).map_err(|e| ThemeFileError::Io(e.to_string()))?;

        let tmp = path.with_extension(format!("{THEME_EXTENSION}.tmp-{}", std::process::id()));
        let write = || -> Result<(), ThemeFileError> {
            let mut zip = ZipWriter::new(fs::File::create(&tmp).map_err(|e| io(&tmp, e))?);
            let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file(THEME_ENTRY, deflated).map_err(|e| io(path, e))?;
            zip.write_all(&json).map_err(|e| io(path, e))?;
            // Pictures and video are compressed already.
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored).large_file(true);
            for (name, file) in &media {
                zip.start_file(format!("{MEDIA_DIR}{name}"), stored).map_err(|e| io(path, e))?;
                let mut src = fs::File::open(file).map_err(|e| io(file, e))?;
                std::io::copy(&mut src, &mut zip).map_err(|e| io(file, e))?;
            }
            zip.finish().map_err(|e| io(path, e))?.sync_all().map_err(|e| io(path, e))?;
            fs::rename(&tmp, path).map_err(|e| io(path, e))
        };
        write().inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }

    /// Reads a theme file, copies its media into the media folder and saves
    /// the theme as a new one, under a name not yet used. Returns it as
    /// stored.
    pub fn import_theme(&mut self, path: &Path) -> Result<Theme, ThemeFileError> {
        let file = fs::File::open(path).map_err(|e| io(path, e))?;
        let mut zip = ZipArchive::new(file).map_err(|_| ThemeFileError::NotATheme)?;
        if zip.len() > MAX_ENTRIES {
            return Err(ThemeFileError::NotATheme);
        }
        let json = read_entry(&mut zip, THEME_ENTRY, MAX_THEME_JSON_BYTES)?.ok_or(ThemeFileError::NotATheme)?;
        let envelope: Value = serde_json::from_slice(&json).map_err(|_| ThemeFileError::NotATheme)?;
        if envelope["format"] != THEME_FORMAT {
            return Err(ThemeFileError::NotATheme);
        }
        match envelope["version"].as_u64() {
            Some(v) if v > THEME_FILE_VERSION => return Err(ThemeFileError::Newer(v)),
            Some(_) => {}
            None => return Err(ThemeFileError::NotATheme),
        }
        let mut theme: Theme =
            serde_json::from_value(envelope["theme"].clone()).map_err(|_| ThemeFileError::NotATheme)?;

        // Media first: a theme is saved only with everything it shows.
        for name in media_names(&theme) {
            let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
            if !is_media_file_name(&name) || !is_media_extension(&ext) {
                return Err(ThemeFileError::MissingMedia(name));
            }
            let bytes = read_entry(&mut zip, &format!("{MEDIA_DIR}{name}"), MAX_MEDIA_BYTES)?
                .ok_or_else(|| ThemeFileError::MissingMedia(name.clone()))?;
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
            let url = self.import_media_bytes(stem, &ext, &bytes).map_err(ThemeFileError::Storage)?;
            let old = format!("{MEDIA_URL_PREFIX}{name}");
            for slot in [&mut theme.background_image, &mut theme.background_video, &mut theme.logo] {
                if slot.as_deref() == Some(old.as_str()) {
                    *slot = Some(url.clone());
                }
            }
        }

        let theme = theme.sanitized();
        let name = self.unused_theme_name(&theme.name).map_err(|e| ThemeFileError::Storage(e.to_string()))?;
        self.save_theme(&Theme { name, ..theme }, None).map_err(|e| ThemeFileError::Storage(e.to_string()))
    }
}

/// The entry called `name`, if the zip has it; larger than `limit` is an
/// error (checked while reading, whatever the zip claims).
fn read_entry(zip: &mut ZipArchive<fs::File>, name: &str, limit: u64) -> Result<Option<Vec<u8>>, ThemeFileError> {
    let mut entry = match zip.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(_) => return Err(ThemeFileError::NotATheme),
    };
    if entry.size() > limit {
        return Err(ThemeFileError::TooLarge(name.into()));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.by_ref().take(limit + 1).read_to_end(&mut bytes).map_err(|_| ThemeFileError::NotATheme)?;
    if bytes.len() as u64 > limit {
        return Err(ThemeFileError::TooLarge(name.into()));
    }
    Ok(Some(bytes))
}
