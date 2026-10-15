//! Background and logo images, copied into the data folder's `media`
//! directory so a theme never depends on where the original file was. The
//! server serves them at `/media/<file>`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::Library;

/// Image types the display page can show, by extension.
pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "svg"];

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

    /// Copies an image into the media folder and returns its URL
    /// (`/media/sunrise-3f9a1c0b.jpg`). The name carries a hash of the
    /// contents, so importing the same file twice reuses one copy and two
    /// different files never overwrite each other.
    pub fn import_media(&self, path: &Path) -> Result<String, String> {
        let dir = self.media_dir().ok_or("This library has no media folder.")?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
            .ok_or_else(|| format!("{} isn't an image Asaf can show (use {}).", path.display(), IMAGE_EXTENSIONS.join(", ")))?;
        let bytes = fs::read(path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).map(slug).unwrap_or_default();
        let stem = if stem.is_empty() { "image".to_string() } else { stem };
        let name = format!("{stem}-{:08x}.{ext}", fnv1a(&bytes) as u32);
        fs::create_dir_all(dir).map_err(|e| format!("Can't create {}: {e}", dir.display()))?;
        let target = dir.join(&name);
        if !target.exists() {
            crate::service::write_atomic_bytes(&target, &bytes).map_err(|e| format!("Can't copy into {}: {e}", dir.display()))?;
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
/// no hidden files, a known image extension.
pub fn is_media_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && name.rsplit_once('.').is_some_and(|(_, ext)| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
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
            ("", false),
            ("a b.jpg", false),
        ] {
            assert_eq!(is_media_file_name(name), ok, "{name}");
        }
    }
}
