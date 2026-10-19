//! Background and logo images and background videos, copied into the data
//! folder's `media` directory so a theme never depends on where the original
//! file was. The server serves them at `/media/<file>`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::Library;

/// Image types the display page can show, by extension.
pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "svg"];

/// Video types the display page can loop, by extension.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "webm"];

/// Larger files aren't imported: they'd be read into memory whole.
pub const MAX_MEDIA_BYTES: u64 = 1 << 30;

pub(crate) fn is_media_extension(ext: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext) || VIDEO_EXTENSIONS.contains(&ext)
}

/// URL prefix the server serves media under.
pub const MEDIA_URL_PREFIX: &str = "/media/";

impl Library {
    /// Where imported images go: `media` next to the database (none for an
    /// in-memory library unless set).
    pub fn media_dir(&self) -> Option<&Path> {
        self.media_dir.as_deref()
    }

    pub fn set_media_dir(&mut self, dir: Option<PathBuf>) {
        self.media_dir = dir;
    }

    /// Copies an image or video into the media folder and returns its URL
    /// (`/media/sunrise-3f9a1c0b.jpg`). The name carries a hash of the
    /// contents, so importing the same file twice reuses one copy and two
    /// different files never overwrite each other.
    pub fn import_media(&self, path: &Path) -> Result<String, String> {
        self.media_dir().ok_or("This library has no media folder.")?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|e| is_media_extension(e))
            .ok_or_else(|| {
                let types = [IMAGE_EXTENSIONS, VIDEO_EXTENSIONS].concat().join(", ");
                format!("{} isn't an image or video Asaf can show (use {types}).", path.display())
            })?;
        let size = fs::metadata(path).map_err(|e| format!("Can't read {}: {e}", path.display()))?.len();
        if size > MAX_MEDIA_BYTES {
            return Err(format!("{} is larger than {} MB.", path.display(), MAX_MEDIA_BYTES >> 20));
        }
        let bytes = fs::read(path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        self.import_media_bytes(stem, &ext, &bytes)
    }

    /// [`Library::import_media`] for a file's contents: `stem` names it
    /// (made a slug), `ext` is its (lowercase, known) extension.
    pub(crate) fn import_media_bytes(&self, stem: &str, ext: &str, bytes: &[u8]) -> Result<String, String> {
        let dir = self.media_dir().ok_or("This library has no media folder.")?;
        let stem = slug(stem);
        // A name we gave before ("sky-1a2b3c4d") gets a fresh hash, not a second one.
        let stem = match stem.rsplit_once('-') {
            Some((head, hash)) if hash.len() == 8 && hash.chars().all(|c| c.is_ascii_hexdigit()) => head.to_string(),
            _ => stem,
        };
        let stem = if stem.is_empty() { "media".to_string() } else { stem };
        let name = format!("{stem}-{:08x}.{ext}", fnv1a(bytes) as u32);
        fs::create_dir_all(dir).map_err(|e| format!("Can't create {}: {e}", dir.display()))?;
        let target = dir.join(&name);
        if !target.exists() {
            crate::service::write_atomic_bytes(&target, bytes).map_err(|e| format!("Can't copy into {}: {e}", dir.display()))?;
        }
        Ok(format!("{MEDIA_URL_PREFIX}{name}"))
    }

    /// The file behind a `/media/…` URL, if it's one of ours.
    pub fn media_path(&self, url: &str) -> Option<PathBuf> {
        let name = url.strip_prefix(MEDIA_URL_PREFIX)?;
        is_media_file_name(name).then(|| self.media_dir().map(|d| d.join(name)))?
    }
}

/// A name the server may serve from the media folder: no path separators,
/// no hidden files, a known image or video extension.
pub fn is_media_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && name.rsplit_once('.').is_some_and(|(_, ext)| is_media_extension(&ext.to_ascii_lowercase()))
}

/// Lowercase ASCII letters, digits and dashes: "Cruz al Atardecer (2)" → "cruz-al-atardecer-2".
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = fold_accent(c);
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(40).collect()
}

fn fold_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
        'ñ' | 'Ñ' => 'n',
        c => c,
    }
}

/// FNV-1a, 64-bit: a stable content hash with no dependency.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_slugs_and_only_safe_names_are_served() {
        assert_eq!(slug("Cruz al Atardecer (2)"), "cruz-al-atardecer-2");
        assert_eq!(slug("  ¡Señor!  "), "senor");
        for (name, ok) in [
            ("sky-1a2b3c4d.jpg", true),
            ("A_B.PNG", true),
            ("../asaf.sqlite", false),
            ("x/y.png", false),
            (".hidden.png", false),
            ("notes.txt", false),
            ("loop-00ff.MP4", true),
            ("clip.webm", true),
            ("", false),
            ("a b.jpg", false),
        ] {
            assert_eq!(is_media_file_name(name), ok, "{name}");
        }
    }
}
