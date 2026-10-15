//! Bible storage: translations, verses, passage lookup and full-text search
//! within a translation.

use std::path::Path;

use asaf_core::{Bible, BibleVerse, RefRange};
use rusqlite::{OptionalExtension, Row, params};

use crate::{Library, Result, fts_query};

/// A text-search hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseHit {
    pub book: u8,
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
}

/// What importing one Bible file did.
#[derive(Clone, Debug)]
pub struct BibleImport {
    pub bible: Bible,
    pub verses: usize,
    pub skipped_books: Vec<String>,
}

impl BibleImport {
    pub fn summary(&self) -> String {
        let mut s = format!("Imported {} ({}), {} verses", self.bible.name, self.bible.abbreviation, self.verses);
        if !self.skipped_books.is_empty() {
            s.push_str(&format!("; skipped books outside the 66: {}", self.skipped_books.join(", ")));
        }
        s.push('.');
        s
    }
}

impl Library {
    /// Installed translations, by name.
    pub fn bibles(&self) -> Result<Vec<Bible>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, name, abbreviation, language, copyright, book_names FROM bibles
             ORDER BY name COLLATE NOCASE, id",
        )?;
        stmt.query_map([], bible_from_row)?.collect()
    }

    pub fn bible(&self, id: i64) -> Result<Option<Bible>> {
        self.conn
            .query_row(
                "SELECT id, name, abbreviation, language, copyright, book_names FROM bibles WHERE id = ?1",
                [id],
                bible_from_row,
            )
            .optional()
    }

    /// A translation with this abbreviation or name (case-insensitive).
    pub fn find_bible(&self, name: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM bibles WHERE abbreviation = ?1 COLLATE NOCASE OR name = ?1 COLLATE NOCASE
                 ORDER BY id LIMIT 1",
                [name.trim()],
                |r| r.get(0),
            )
            .optional()
    }

    /// Stores a new translation and its verses in one transaction, and
    /// indexes the text. Returns its id.
    pub fn save_bible(&mut self, bible: &Bible, verses: &[BibleVerse]) -> Result<i64> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO bibles (name, abbreviation, language, copyright, book_names) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                bible.name.trim(),
                bible.abbreviation.trim(),
                bible.language,
                bible.copyright.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                bible.book_names.iter().map(|n| n.replace('\n', " ")).collect::<Vec<_>>().join("\n"),
            ],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO bible_verses (bible_id, book, chapter, verse, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for v in verses {
                insert.execute(params![id, v.book, v.chapter, v.verse, v.text])?;
            }
        }
        tx.execute(
            "INSERT INTO bible_fts (rowid, text) SELECT id, text FROM bible_verses WHERE bible_id = ?1",
            [id],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn delete_bible(&mut self, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        // External-content FTS needs the old text to remove its entries.
        tx.execute(
            "INSERT INTO bible_fts (bible_fts, rowid, text)
             SELECT 'delete', id, text FROM bible_verses WHERE bible_id = ?1",
            [id],
        )?;
        tx.execute("DELETE FROM bible_verses WHERE bible_id = ?1", [id])?;
        tx.execute("DELETE FROM bibles WHERE id = ?1", [id])?;
        tx.commit()
    }

    pub fn verse_count(&self, bible_id: i64) -> Result<usize> {
        self.conn
            .query_row("SELECT count(*) FROM bible_verses WHERE bible_id = ?1", [bible_id], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
    }

    /// The verses of `ranges` in `bible_id`, range by range, in order. Verses
    /// past the end of a chapter simply aren't there.
    pub fn passage_verses(&self, bible_id: i64, ranges: &[RefRange]) -> Result<Vec<BibleVerse>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT book, chapter, verse, text FROM bible_verses
             WHERE bible_id = ?1 AND book = ?2
               AND (chapter, verse) >= (?3, ?4) AND (chapter, verse) <= (?5, ?6)
             ORDER BY chapter, verse",
        )?;
        let mut verses = Vec::new();
        for r in ranges {
            let rows = stmt.query_map(
                params![bible_id, r.book, r.start.chapter, r.start.verse, r.end.chapter, r.end.verse],
                verse_from_row,
            )?;
            for v in rows {
                verses.push(v?);
            }
        }
        Ok(verses)
    }

    /// Verses of `bible_id` containing every typed word (as a word prefix,
    /// ignoring accents and case), best matches first.
    pub fn search_bible(&self, bible_id: i64, query: &str, limit: usize) -> Result<Vec<VerseHit>> {
        let Some(fts_query) = fts_query(query) else { return Ok(Vec::new()) };
        let mut stmt = self.conn.prepare_cached(
            "SELECT v.book, v.chapter, v.verse, v.text
             FROM bible_fts
             JOIN bible_verses v ON v.id = bible_fts.rowid
             WHERE bible_fts MATCH ?1 AND v.bible_id = ?2
             ORDER BY bm25(bible_fts), v.book, v.chapter, v.verse
             LIMIT ?3",
        )?;
        stmt.query_map(params![fts_query, bible_id, limit as i64], |r| {
            let v = verse_from_row(r)?;
            Ok(VerseHit { book: v.book, chapter: v.chapter, verse: v.verse, text: v.text })
        })?
        .collect()
    }

    /// Imports an OSIS or Zefania file. Fails without changing anything if
    /// the file can't be read or a translation with the same name or
    /// abbreviation is already installed.
    pub fn import_bible(&mut self, path: &Path) -> std::result::Result<BibleImport, String> {
        let imported = asaf_bible::read_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.install_bible(imported)
    }

    /// Stores a Bible read from a file, unless a translation with the same
    /// name or abbreviation is already installed.
    pub(crate) fn install_bible(&mut self, imported: asaf_bible::ImportedBible) -> std::result::Result<BibleImport, String> {
        let mut bible = imported.bible;
        for key in [&bible.abbreviation, &bible.name] {
            if self.find_bible(key).map_err(|e| e.to_string())?.is_some() {
                return Err(format!("{} ({}) is already installed", bible.name, bible.abbreviation));
            }
        }
        bible.id = Some(self.save_bible(&bible, &imported.verses).map_err(|e| e.to_string())?);
        Ok(BibleImport { bible, verses: imported.verses.len(), skipped_books: imported.skipped_books })
    }
}

fn bible_from_row(r: &Row) -> Result<Bible> {
    let names: String = r.get(5)?;
    Ok(Bible {
        id: Some(r.get(0)?),
        name: r.get(1)?,
        abbreviation: r.get(2)?,
        language: r.get(3)?,
        copyright: r.get(4)?,
        book_names: if names.is_empty() { Vec::new() } else { names.split('\n').map(String::from).collect() },
    })
}

fn verse_from_row(r: &Row) -> Result<BibleVerse> {
    Ok(BibleVerse { book: r.get(0)?, chapter: r.get(1)?, verse: r.get(2)?, text: r.get(3)? })
}
