//! Migration from OpenLP: reads the songs database (`songs/songs.sqlite`) and
//! every Bible database (`bibles/*.sqlite`) under an OpenLP data folder, in
//! one step. OpenLP's files are opened read-only and never changed.
//!
//! The schemas are OpenLP's (SQLAlchemy models in
//! `openlp/plugins/songs/lib/db.py` and `openlp/plugins/bibles/lib/db.py`):
//!
//! - songs: `songs` (title, alternate_title, lyrics, verse_order, copyright,
//!   ccli_number, `temporary` since schema 3), `authors` (display_name) and
//!   `authors_songs` (author_id, song_id, `author_type` since schema 6).
//!   `lyrics` is OpenLP's own XML: `<song version="1.0"><lyrics><verse
//!   type="v" label="1"><![CDATA[…]]></verse>…`, lines separated by newlines,
//!   `[---]` lines as optional splits, `{tag}…{/tag}` formatting and, since
//!   OpenLP 2.4, chords inline as `[G]`.
//! - Bibles, one file per translation: `metadata` (key/value: `name`,
//!   `copyright`, `language_id`…), `book` (book_reference_id 1–66 in
//!   Protestant order, 67+ deuterocanonical; the translation's book `name`)
//!   and `verse` (book_id, chapter, verse, text).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use asaf_bible::ImportedBible;
use asaf_core::bible::BOOKS;
use asaf_core::markup;
use asaf_core::{Bible, BibleVerse, SPLIT_MARKER, Song, Verse};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::{BibleImport, ImportReport, Library};

/// What an OpenLP data folder holds.
#[derive(Debug, Default, PartialEq)]
pub struct OpenLpFiles {
    pub songs: Option<PathBuf>,
    pub bibles: Vec<PathBuf>,
}

/// What importing an OpenLP folder did.
#[derive(Debug, Default)]
pub struct OpenLpReport {
    pub files: OpenLpFiles,
    pub songs: ImportReport,
    /// One result per Bible file.
    pub bibles: Vec<(PathBuf, Result<BibleImport, String>)>,
}

impl OpenLpReport {
    pub fn summary(&self) -> String {
        let mut s = match &self.files.songs {
            Some(_) => self.songs.summary().trim_end_matches('.').to_string(),
            None => "No OpenLP song database".into(),
        };
        let ok: Vec<&str> = self.bibles.iter().filter_map(|(_, r)| r.as_ref().ok()).map(|b| b.bible.abbreviation.as_str()).collect();
        let failed = self.bibles.len() - ok.len();
        match ok.len() {
            0 => s.push_str("; no Bibles imported"),
            n => s.push_str(&format!("; imported {n} Bible{} ({})", if n == 1 { "" } else { "s" }, ok.join(", "))),
        }
        if failed > 0 {
            s.push_str(&format!(", {failed} Bible{} not imported", if failed == 1 { "" } else { "s" }));
        }
        s.push('.');
        s
    }

    /// Every problem, one line each.
    pub fn problems(&self) -> Vec<String> {
        let mut out: Vec<String> = self.songs.failed.iter().map(|(p, e)| format!("{}: {e}", p.display())).collect();
        for (path, result) in &self.bibles {
            match result {
                Err(e) => out.push(format!("{}: {e}", file_name(path))),
                Ok(b) if !b.skipped_books.is_empty() => {
                    out.push(format!("{}: skipped books outside the 66: {}", b.bible.abbreviation, b.skipped_books.join(", ")))
                }
                Ok(_) => {}
            }
        }
        out
    }
}

/// Finds OpenLP databases in `dir`: the data folder itself (with `songs/` and
/// `bibles/`), its parent, or one of those folders. Looks three levels deep.
pub fn find_openlp_files(dir: &Path) -> OpenLpFiles {
    let mut files = OpenLpFiles::default();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    let mut sqlite = Vec::new();
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && depth < 3 {
                stack.push((path, depth + 1));
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("sqlite")) {
                sqlite.push(path);
            }
        }
    }
    sqlite.sort();
    for path in sqlite {
        let Ok(conn) = open_read_only(&path) else { continue };
        let tables = table_names(&conn);
        if tables.contains("songs") && tables.contains("authors") && files.songs.is_none() {
            files.songs = Some(path);
        } else if tables.contains("verse") && tables.contains("book") && tables.contains("metadata") {
            files.bibles.push(path);
        }
    }
    files
}

impl Library {
    /// Imports songs and Bibles from an OpenLP data folder. Songs already in
    /// the library (same title and lyrics) and translations already installed
    /// (same name or abbreviation) are skipped, so running it twice is safe.
    pub fn import_openlp(&mut self, dir: &Path) -> Result<OpenLpReport, String> {
        let files = find_openlp_files(dir);
        if files.songs.is_none() && files.bibles.is_empty() {
            return Err(format!("No OpenLP songs.sqlite or Bible databases in {}.", dir.display()));
        }
        let mut report = OpenLpReport::default();
        if let Some(path) = &files.songs {
            let songs = read_openlp_songs(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let mut new_songs: Vec<Song> = Vec::new();
            for (label, song) in songs {
                match song {
                    Ok(song) => {
                        let dup_in_batch =
                            new_songs.iter().any(|s| s.title.eq_ignore_ascii_case(&song.title) && s.verses == song.verses);
                        if dup_in_batch || self.find_duplicate(&song).map_err(|e| e.to_string())?.is_some() {
                            report.songs.duplicates += 1;
                        } else {
                            new_songs.push(song);
                        }
                    }
                    Err(e) => report.songs.failed.push((PathBuf::from(label), e)),
                }
            }
            report.songs.imported = self.save_all(&new_songs).map_err(|e| e.to_string())?.len();
        }
        for path in &files.bibles {
            let result = read_openlp_bible(path).and_then(|mut imported| {
                imported.bible.abbreviation = self.free_abbreviation(&imported.bible)?;
                self.install_bible(imported)
            });
            report.bibles.push((path.clone(), result));
        }
        report.files = files;
        Ok(report)
    }
}

impl Library {
    /// The abbreviation made from an OpenLP Bible's name can clash with a
    /// different installed translation ("Reina Valera Antigua" and "Reina
    /// Valera Actualizada" are both RVA): number it rather than refuse
    /// the import. The same translation again (same name) keeps its
    /// abbreviation, so it is reported as already installed.
    fn free_abbreviation(&self, bible: &Bible) -> Result<String, String> {
        let taken = |key: &str| self.find_bible(key).map(|id| id.is_some()).map_err(|e| e.to_string());
        let base = bible.abbreviation.clone();
        if taken(&bible.name)? || !taken(&base)? {
            return Ok(base);
        }
        for n in 2.. {
            // RVA2, but RV1909-2.
            let sep = if base.ends_with(|c: char| c.is_ascii_digit()) { "-" } else { "" };
            let candidate = format!("{base}{sep}{n}");
            if !taken(&candidate)? {
                return Ok(candidate);
            }
        }
        unreachable!()
    }
}

fn open_read_only(path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
}

fn table_names(conn: &Connection) -> HashSet<String> {
    let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'") else { return HashSet::new() };
    stmt.query_map([], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.prepare(&format!("SELECT * FROM {table} LIMIT 0"))
        .map(|stmt| stmt.column_names().iter().any(|c| c.eq_ignore_ascii_case(column)))
        .unwrap_or(false)
}

/// id, title, alternate_title, lyrics, verse_order, copyright, ccli_number.
type SongRow = (i64, String, Option<String>, String, Option<String>, Option<String>, Option<String>);

/// Every song in an OpenLP `songs.sqlite`, labelled for error messages
/// ("song 12 “Title”"). Temporary songs (copies OpenLP made from opened
/// service files) are left out.
pub fn read_openlp_songs(path: &Path) -> rusqlite::Result<Vec<(String, Result<Song, String>)>> {
    let conn = open_read_only(path)?;
    let temporary = if has_column(&conn, "songs", "temporary") { "WHERE coalesce(temporary, 0) = 0" } else { "" };
    let mut stmt = conn.prepare(&format!(
        "SELECT id, title, alternate_title, lyrics, verse_order, copyright, ccli_number FROM songs {temporary} ORDER BY id"
    ))?;
    let mut authors = conn.prepare(
        "SELECT a.display_name FROM authors_songs s JOIN authors a ON a.id = s.author_id
         WHERE s.song_id = ?1 ORDER BY s.rowid",
    )?;
    let rows: Vec<SongRow> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get(2)?,
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (id, title, alt_title, lyrics, verse_order, copyright, ccli) in rows {
        let label = format!("song {id} “{}”", title.trim());
        let mut names: Vec<String> = Vec::new();
        for name in authors.query_map([id], |r| r.get::<_, Option<String>>(0))? {
            let name = name?.unwrap_or_default().trim().to_string();
            // One author can be listed once per role (words, music…).
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        let song = parse_openlp_lyrics(&lyrics).and_then(|verses| {
            if title.trim().is_empty() {
                return Err("the song has no title".into());
            }
            let nonempty = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            Ok(Song {
                id: None,
                title: title.trim().to_string(),
                alt_title: nonempty(alt_title),
                authors: names,
                copyright: nonempty(copyright),
                ccli_number: nonempty(ccli),
                verse_order: Song::parse_verse_order(&verse_order.unwrap_or_default().to_lowercase()),
                verses,
            })
        });
        out.push((label, song));
    }
    Ok(out)
}

/// OpenLP's lyrics XML into verses. Verse types are OpenLP's letters (`v`,
/// `c`, `b`, `p`, `i`, `e`, `o`); very old databases spelled them out.
pub fn parse_openlp_lyrics(xml: &str) -> Result<Vec<Verse>, String> {
    // OpenLP writes an XML declaration that says UTF-8 on text that's already decoded.
    let body = match xml.trim_start().strip_prefix("<?xml") {
        Some(rest) => rest.split_once("?>").map(|(_, b)| b).unwrap_or(rest),
        None => xml,
    };
    let doc = roxmltree::Document::parse(body).map_err(|e| format!("unreadable lyrics: {e}"))?;
    let mut verses: Vec<Verse> = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("verse")) {
        let kind = verse_kind(node.attribute("type").unwrap_or("v"));
        let label: String = node.attribute("label").unwrap_or("1").trim().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let mut tag = format!("{kind}{}", if label.is_empty() { "1" } else { &label }).to_ascii_lowercase();
        // Tags must be unique; a repeated one (two languages, a damaged song) gets a letter.
        if verses.iter().any(|v| v.tag == tag) {
            let base = tag.clone();
            tag = ('a'..='z').map(|c| format!("{base}{c}")).find(|t| !verses.iter().any(|v| &v.tag == t)).unwrap_or(base);
        }
        let text: String = node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
        let verse = Verse::new(&tag, &clean_lyrics(&text));
        if !verse.lines.iter().all(|l| l == SPLIT_MARKER) {
            verses.push(verse);
        }
    }
    if verses.is_empty() {
        return Err("the song has no lyrics".into());
    }
    Ok(verses)
}

fn verse_kind(kind: &str) -> char {
    let k = kind.trim().to_lowercase();
    match k.as_str() {
        "verse" => 'v',
        "chorus" => 'c',
        "bridge" => 'b',
        "pre-chorus" | "prechorus" => 'p',
        "intro" => 'i',
        "ending" => 'e',
        "other" => 'o',
        _ => k.chars().next().filter(|c| "vcbpieo".contains(*c)).unwrap_or('o'),
    }
}

/// Turns OpenLP lyrics into Asaf's: `{st}…{/st}` (strong) becomes
/// `**…**` and `{it}…{/it}` (italics) `__…__`; other formatting tags
/// (`{r}…{/r}`) and inline chords (`[G]`, `[F#m7]`, `[D/F#]`) are dropped,
/// `{br}` becomes a line break and `[---]` splits stay.
pub fn clean_lyrics(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n").replace("{br}", "\n");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(i) = rest.find(['{', '[']) {
        out.push_str(&markup::escape(&rest[..i]));
        let open = rest.as_bytes()[i] as char;
        let close = if open == '{' { '}' } else { ']' };
        let after = &rest[i + 1..];
        match after.find(close) {
            Some(j) if is_markup(open, &after[..j]) => {
                let tag = &after[..j];
                out.push_str(match tag.strip_prefix('/').unwrap_or(tag) {
                    "st" if open == '{' => "**",
                    "it" if open == '{' => "__",
                    _ => "",
                });
                rest = &after[j + 1..];
            }
            _ => {
                out.push(open);
                rest = after;
            }
        }
    }
    out.push_str(&markup::escape(rest));
    out.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n")
}

fn is_markup(open: char, inner: &str) -> bool {
    if open == '{' {
        // {r} {/r} {st} {it} {u} {p} {su} {sb}… and user-defined tags.
        let tag = inner.strip_prefix('/').unwrap_or(inner);
        return !tag.is_empty() && tag.len() <= 12 && tag.chars().all(|c| c.is_ascii_alphanumeric());
    }
    is_chord(inner)
}

/// `G`, `F#m7`, `Bbmaj7`, `Dsus4`, `D/F#`, `C#dim`: a note name, then chord
/// characters, optionally a slash bass note.
fn is_chord(s: &str) -> bool {
    let note = |s: &str| -> Option<usize> {
        let mut chars = s.chars();
        let first = chars.next()?;
        if !('A'..='G').contains(&first) {
            return None;
        }
        Some(if matches!(chars.next(), Some('#' | 'b')) { 2 } else { 1 })
    };
    let (main, bass) = match s.split_once('/') {
        Some((m, b)) => (m, Some(b)),
        None => (s, None),
    };
    let Some(n) = note(main) else { return false };
    let quality_ok = main[n..].len() <= 10 && is_chord_quality(&main[n..]);
    let bass_ok = bass.is_none_or(|b| note(b).is_some_and(|n| n == b.len()));
    quality_ok && bass_ok
}

/// `m7`, `maj7`, `sus4`, `add9`, `dim`, `7(b9)`: only chord words, digits and signs.
fn is_chord_quality(mut q: &str) -> bool {
    const WORDS: [&str; 7] = ["maj", "min", "dim", "aug", "sus", "add", "m"];
    while !q.is_empty() {
        if let Some(w) = WORDS.iter().find(|w| q.starts_with(**w)) {
            q = &q[w.len()..];
        } else if q.starts_with(|c: char| c.is_ascii_digit() || "+-#b()M°ø".contains(c)) {
            q = &q[q.chars().next().unwrap().len_utf8()..];
        } else {
            return false;
        }
    }
    true
}

/// OpenLP's language ids (`openlp/core/common/i18n.py`) for the languages
/// Asaf's references understand.
fn language_code(language_id: Option<&str>) -> Option<&'static str> {
    match language_id?.trim() {
        "30" => Some("en"),
        "110" => Some("es"),
        _ => None,
    }
}

/// Reads one OpenLP Bible database.
pub fn read_openlp_bible(path: &Path) -> Result<ImportedBible, String> {
    let conn = open_read_only(path).map_err(|e| e.to_string())?;
    let meta = |key: &str| -> Option<String> {
        conn.query_row("SELECT value FROM metadata WHERE key = ?1", [key], |r| r.get::<_, Option<String>>(0))
            .optional()
            .ok()
            .flatten()
            .flatten()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = meta("name").unwrap_or(stem);
    if meta("download_source").is_some() {
        return Err(format!("{name} is a web Bible (downloaded on demand); its text isn't stored in OpenLP."));
    }

    let mut books = conn
        .prepare("SELECT id, book_reference_id, name FROM book ORDER BY book_reference_id, id")
        .map_err(|e| e.to_string())?;
    let books: Vec<(i64, i64, String)> = books
        .query_map([], |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0), r.get::<_, Option<String>>(2)?.unwrap_or_default())))
        .and_then(|rows| rows.collect())
        .map_err(|e| e.to_string())?;
    let mut book_names = vec![String::new(); BOOKS.len()];
    let mut skipped_books = Vec::new();
    let mut verses = Vec::new();
    let mut stmt = conn
        .prepare("SELECT chapter, verse, text FROM verse WHERE book_id = ?1 ORDER BY chapter, verse, id")
        .map_err(|e| e.to_string())?;
    for (id, reference, book_name) in books {
        if !(1..=BOOKS.len() as i64).contains(&reference) {
            skipped_books.push(book_name);
            continue;
        }
        book_names[reference as usize - 1] = book_name.trim().to_string();
        let rows = stmt
            .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default())))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (chapter, verse, text) = row.map_err(|e| e.to_string())?;
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() || !(1..=i64::from(u16::MAX)).contains(&chapter) || !(1..=i64::from(u16::MAX)).contains(&verse) {
                continue;
            }
            verses.push(BibleVerse { book: reference as u8, chapter: chapter as u16, verse: verse as u16, text });
        }
    }
    if verses.is_empty() {
        return Err(format!("{name} has no verses."));
    }
    let language = language_code(meta("language_id").as_deref())
        .or_else(|| guess_language(&book_names))
        .unwrap_or_default()
        .to_string();
    let bible = Bible {
        id: None,
        abbreviation: abbreviate(&name),
        name,
        language,
        copyright: meta("copyright"),
        book_names,
    };
    Ok(ImportedBible { bible, verses, skipped_books })
}

/// English or Spanish from the name of John's Gospel, when the database
/// doesn't say.
fn guess_language(book_names: &[String]) -> Option<&'static str> {
    let john = book_names.get(42)?.to_lowercase();
    if john.contains("juan") {
        Some("es")
    } else if john.contains("john") {
        Some("en")
    } else {
        None
    }
}

/// OpenLP Bibles have a name but no abbreviation: "King James Version" →
/// "KJV", "Reina-Valera 1909" → "RV1909". Short names are kept as they are.
pub fn abbreviate(name: &str) -> String {
    let name = name.trim();
    if name.chars().count() <= 8 && !name.contains(char::is_whitespace) {
        return name.to_string();
    }
    let mut out = String::new();
    for word in name.split(|c: char| c.is_whitespace() || c == '-' || c == '(' || c == ')' || c == ',') {
        if word.chars().all(|c| c.is_ascii_digit()) {
            out.push_str(word);
        } else if let Some(c) = word.chars().next().filter(|c| c.is_alphanumeric())
            && !matches!(word.to_lowercase().as_str(), "de" | "del" | "the" | "of" | "y" | "and")
        {
            out.extend(c.to_uppercase());
        }
    }
    if out.is_empty() { name.chars().take(8).collect() } else { out }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lyrics_cleanup_table() {
        for (raw, want) in [
            ("Amazing grace", "Amazing grace"),
            ("{r}Amazing{/r} grace", "Amazing grace"),
            ("[G]Amazing [C]grace, how [G]sweet", "Amazing grace, how sweet"),
            ("[D/F#]Holy, [Bbmaj7]holy [F#m7]holy", "Holy, holy holy"),
            ("line one{br}line two", "line one\nline two"),
            ("first\n[---]\nsecond", "first\n[---]\nsecond"),
            ("[Coro] stays: not a chord", "[Coro] stays: not a chord"),
            ("[x] [Hm] {not a tag}", "[x] [Hm] {not a tag}"),
            ("  spaced   out  ", "spaced out"),
            ("unclosed [G", "unclosed [G"),
            ("{st}Señor{/st}, mi Dios", "**Señor**, mi Dios"),
            ("{it}{st}Santo{/st}{/it} {u}santo{/u}", "__**Santo**__ santo"),
            ("2 ** 3 {r}__init__{/r}", "2 \\*\\* 3 \\_\\_init\\_\\_"),
        ] {
            assert_eq!(clean_lyrics(raw), want, "{raw:?}");
        }
    }

    #[test]
    fn verse_types_table() {
        for (t, want) in [
            ("v", 'v'),
            ("C", 'c'),
            ("Verse", 'v'),
            ("Pre-Chorus", 'p'),
            ("Intro", 'i'),
            ("e", 'e'),
            ("x", 'o'),
            ("", 'o'),
        ] {
            assert_eq!(verse_kind(t), want, "{t:?}");
        }
    }

    #[test]
    fn lyrics_xml_into_verses() {
        let xml = "<?xml version='1.0' encoding='UTF-8'?>\n<song version=\"1.0\"><lyrics language=\"en\">\
            <verse type=\"v\" label=\"1\"><![CDATA[Line one\nLine two\n[---]\nLine three]]></verse>\
            <verse type=\"c\" label=\"1\"><![CDATA[Chorus]]></verse>\
            <verse type=\"c\" label=\"1\"><![CDATA[Coro]]></verse>\
            <verse type=\"o\" label=\"\"><![CDATA[]]></verse>\
            </lyrics></song>";
        let verses = parse_openlp_lyrics(xml).unwrap();
        let tags: Vec<_> = verses.iter().map(|v| v.tag.as_str()).collect();
        assert_eq!(tags, ["v1", "c1", "c1a"]);
        assert_eq!(verses[0].lines, ["Line one", "Line two", "[---]", "Line three"]);
        assert!(parse_openlp_lyrics("<song><lyrics/></song>").is_err());
        assert!(parse_openlp_lyrics("not xml").unwrap_err().starts_with("unreadable lyrics"));
    }

    #[test]
    fn abbreviations_table() {
        for (name, want) in [
            ("King James Version", "KJV"),
            ("Reina-Valera 1909", "RV1909"),
            ("Reina Valera (1909)", "RV1909"),
            ("La Biblia de las Américas", "LBLA"),
            ("KJV", "KJV"),
            ("RVR1960", "RVR1960"),
            ("World English Bible", "WEB"),
        ] {
            assert_eq!(abbreviate(name), want, "{name}");
        }
    }
}
