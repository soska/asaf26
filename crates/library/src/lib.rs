//! Song and Bible storage: SQLite with FTS5 indexes over songs (title,
//! alternate title, authors, lyrics) and Bible verse text.

mod bible;
mod import;
pub mod media;
mod migrations;
pub mod openlp;
pub mod service;
mod themes;

use std::path::{Path, PathBuf};
use std::time::Duration;

use asaf_core::{Song, format_lyrics, parse_lyrics};
use rusqlite::{Connection, OptionalExtension, Row, params};

pub use bible::{BibleImport, VerseHit};
pub use themes::ThemeError;
pub use import::ImportReport;
pub use rusqlite::Error;
pub type Result<T> = rusqlite::Result<T>;

/// One search hit, enough for a results list.
#[derive(Clone, Debug, PartialEq)]
pub struct SongSummary {
    pub id: i64,
    pub title: String,
    pub authors: String,
    /// Matching lyric text for lyric hits, else the first line.
    pub excerpt: String,
}

pub struct Library {
    conn: Connection,
    media_dir: Option<PathBuf>,
}

impl Library {
    /// Opens (creating if needed) the database at `path` and migrates it.
    /// Imported images go in `media` next to it.
    pub fn open(path: &Path) -> Result<Self> {
        let media = path.parent().map(|d| d.join("media"));
        Self::init(Connection::open(path)?, media)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(conn: Connection, media_dir: Option<PathBuf>) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // FULL: a saved edit is on disk before save() returns.
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut lib = Self { conn, media_dir };
        migrations::run(&mut lib.conn)?;
        lib.ensure_a_theme()?;
        Ok(lib)
    }

    pub fn count(&self) -> Result<usize> {
        self.conn
            .query_row("SELECT count(*) FROM songs", [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
    }

    pub fn get(&self, id: i64) -> Result<Option<Song>> {
        self.conn
            .query_row(
                "SELECT id, title, alt_title, authors, copyright, ccli_number, lyrics, verse_order
                 FROM songs WHERE id = ?1",
                [id],
                song_from_row,
            )
            .optional()
    }

    /// Inserts a new song (`id == None`) or updates an existing one, and
    /// returns its id. The search index is updated in the same transaction.
    pub fn save(&mut self, song: &Song) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let id = save_in(&tx, song)?;
        tx.commit()?;
        Ok(id)
    }

    /// Saves many songs in one transaction (fast bulk import).
    pub fn save_all<'a>(&mut self, songs: impl IntoIterator<Item = &'a Song>) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let ids = songs.into_iter().map(|s| save_in(&tx, s)).collect::<Result<_>>()?;
        tx.commit()?;
        Ok(ids)
    }

    pub fn delete(&mut self, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM songs_fts WHERE rowid = ?1", [id])?;
        tx.execute("DELETE FROM songs WHERE id = ?1", [id])?;
        tx.commit()
    }

    /// An existing song with the same title and lyrics, if any. Used to skip
    /// duplicates when importing the same files twice.
    pub fn find_duplicate(&self, song: &Song) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM songs WHERE title = ?1 COLLATE NOCASE AND lyrics = ?2 LIMIT 1",
                params![song.title.trim(), format_lyrics(&song.verses)],
                |r| r.get(0),
            )
            .optional()
    }

    /// Search as you type. Every word must match the start of a word in the
    /// title, alternate title, authors or lyrics; accents and case are
    /// ignored. Title matches rank first. An empty query lists all songs by
    /// title.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SongSummary>> {
        let Some(fts_query) = fts_query(query) else {
            let mut stmt = self.conn.prepare_cached(
                "SELECT id, title, authors, lyrics FROM songs
                 ORDER BY title COLLATE NOCASE, id LIMIT ?1",
            )?;
            return stmt
                .query_map([limit as i64], |r| {
                    Ok(SongSummary {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        authors: joined_authors(&r.get::<_, String>(2)?),
                        excerpt: first_lyric_line(&r.get::<_, String>(3)?),
                    })
                })?
                .collect();
        };

        let mut stmt = self.conn.prepare_cached(
            "SELECT s.id, s.title, s.authors,
                    snippet(songs_fts, 3, '', '', '…', 10),
                    s.lyrics
             FROM songs_fts
             JOIN songs s ON s.id = songs_fts.rowid
             WHERE songs_fts MATCH ?1
             ORDER BY bm25(songs_fts, 10.0, 6.0, 3.0, 1.0), s.title COLLATE NOCASE
             LIMIT ?2",
        )?;
        stmt.query_map(params![fts_query, limit as i64], |r| {
            let snippet: String = r.get(3)?;
            Ok(SongSummary {
                id: r.get(0)?,
                title: r.get(1)?,
                authors: joined_authors(&r.get::<_, String>(2)?),
                excerpt: if snippet.trim().is_empty() {
                    first_lyric_line(&r.get::<_, String>(4)?)
                } else {
                    snippet.replace('\n', " / ")
                },
            })
        })?
        .collect()
    }

    /// Every song, for export.
    pub fn all(&self) -> Result<Vec<Song>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, alt_title, authors, copyright, ccli_number, lyrics, verse_order
             FROM songs ORDER BY title COLLATE NOCASE, id",
        )?;
        stmt.query_map([], song_from_row)?.collect()
    }
}

fn save_in(tx: &rusqlite::Transaction, song: &Song) -> Result<i64> {
    let lyrics = format_lyrics(&song.verses);
    let authors = song.authors.join("\n");
    let values = params![
        song.title.trim(),
        song.alt_title.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        authors,
        song.copyright.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        song.ccli_number.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        lyrics,
        song.verse_order_text(),
    ];
    let id = match song.id {
        Some(id) => {
            let updated = tx.execute(
                "UPDATE songs SET title = ?1, alt_title = ?2, authors = ?3, copyright = ?4,
                        ccli_number = ?5, lyrics = ?6, verse_order = ?7, updated_at = unixepoch()
                 WHERE id = ?8",
                rusqlite::params_from_iter(values.iter().copied().chain([&id as &dyn rusqlite::ToSql])),
            )?;
            if updated == 0 {
                return Err(Error::QueryReturnedNoRows);
            }
            tx.execute("DELETE FROM songs_fts WHERE rowid = ?1", [id])?;
            id
        }
        None => {
            tx.execute(
                "INSERT INTO songs (title, alt_title, authors, copyright, ccli_number, lyrics, verse_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                values,
            )?;
            tx.last_insert_rowid()
        }
    };
    tx.execute(
        "INSERT INTO songs_fts (rowid, title, alt_title, authors, lyrics) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, song.title.trim(), song.alt_title, authors, song.plain_lyrics()],
    )?;
    Ok(id)
}

fn song_from_row(r: &Row) -> Result<Song> {
    let authors: String = r.get(3)?;
    Ok(Song {
        id: Some(r.get(0)?),
        title: r.get(1)?,
        alt_title: r.get(2)?,
        authors: authors.lines().map(String::from).filter(|a| !a.is_empty()).collect(),
        copyright: r.get(4)?,
        ccli_number: r.get(5)?,
        verses: parse_lyrics(&r.get::<_, String>(6)?),
        verse_order: Song::parse_verse_order(&r.get::<_, String>(7)?),
    })
}

/// User text → FTS5 query: each word becomes a quoted prefix term, all required.
pub(crate) fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn joined_authors(stored: &str) -> String {
    stored.lines().collect::<Vec<_>>().join(", ")
}

fn first_lyric_line(lyrics: &str) -> String {
    lyrics
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('['))
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_quotes_words_as_prefixes() {
        assert_eq!(fts_query("  señor, ¡ven! "), Some("\"señor\"* \"ven\"*".into()));
        assert_eq!(fts_query(" \" * - "), None);
    }
}
