use std::path::{Path, PathBuf};

use asaf_core::Song;

use crate::{Library, Result};

/// What happened when importing a set of files and folders.
#[derive(Debug, Default)]
pub struct ImportReport {
    pub imported: usize,
    /// Same title and lyrics as a song already in the library.
    pub duplicates: usize,
    pub failed: Vec<(PathBuf, String)>,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        let mut s = format!("Imported {} song{}", self.imported, if self.imported == 1 { "" } else { "s" });
        if self.duplicates > 0 {
            s.push_str(&format!(", skipped {} already in the library", self.duplicates));
        }
        if !self.failed.is_empty() {
            s.push_str(&format!(", {} file{} failed", self.failed.len(), if self.failed.len() == 1 { "" } else { "s" }));
        }
        s.push('.');
        s
    }
}

impl Library {
    /// Imports OpenLyrics files; folders are searched recursively for `.xml`.
    /// All new songs are saved in one transaction.
    pub fn import_openlyrics(&mut self, paths: &[impl AsRef<Path>]) -> Result<ImportReport> {
        let mut report = ImportReport::default();
        let mut parsed: Vec<(PathBuf, std::result::Result<Song, String>)> = Vec::new();
        for path in paths {
            let path = path.as_ref();
            if path.is_dir() {
                match asaf_openlyrics::read_folder(path) {
                    Ok(files) => parsed.extend(files.into_iter().map(|(p, r)| (p, r.map_err(|e| e.to_string())))),
                    Err(e) => report.failed.push((path.to_path_buf(), e.to_string())),
                }
            } else {
                parsed.push((path.to_path_buf(), asaf_openlyrics::read_file(path).map_err(|e| e.to_string())));
            }
        }

        let mut new_songs: Vec<Song> = Vec::new();
        for (path, result) in parsed {
            match result {
                Ok(song) => {
                    let dup_in_batch = new_songs
                        .iter()
                        .any(|s| s.title.eq_ignore_ascii_case(&song.title) && s.verses == song.verses);
                    if dup_in_batch || self.find_duplicate(&song)?.is_some() {
                        report.duplicates += 1;
                    } else {
                        new_songs.push(song);
                    }
                }
                Err(e) => report.failed.push((path, e)),
            }
        }
        report.imported = self.save_all(&new_songs)?.len();
        Ok(report)
    }
}
