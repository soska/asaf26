//! Schema migrations, tracked with `PRAGMA user_version`. Append only: never
//! edit a migration that has shipped.

use rusqlite::{Connection, Result};

const MIGRATIONS: &[&str] = &[
    // 1: songs + full-text index.
    "CREATE TABLE songs (
        id          INTEGER PRIMARY KEY,
        title       TEXT NOT NULL,
        alt_title   TEXT,
        authors     TEXT NOT NULL DEFAULT '',   -- one per line
        copyright   TEXT,
        ccli_number TEXT,
        lyrics      TEXT NOT NULL,              -- editor format, see asaf_core::format_lyrics
        verse_order TEXT NOT NULL DEFAULT '',   -- 'v1 c1 v2'
        created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
        updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
     );
     CREATE INDEX songs_title ON songs (title COLLATE NOCASE);
     -- Maintained by asaf-library on every save (plain lyrics, no tags).
     CREATE VIRTUAL TABLE songs_fts USING fts5(
        title, alt_title, authors, lyrics,
        tokenize = 'unicode61 remove_diacritics 2'
     );",
    // 2: Bibles. Verses have an explicit integer id (a rowid that VACUUM and
    // dump/restore keep) so the full-text index can point at them.
    "CREATE TABLE bibles (
        id           INTEGER PRIMARY KEY,
        name         TEXT NOT NULL,
        abbreviation TEXT NOT NULL,
        language     TEXT NOT NULL DEFAULT '',  -- 'es', 'en'…
        copyright    TEXT,
        book_names   TEXT NOT NULL DEFAULT '',  -- 66 lines; a blank line means the default name
        created_at   INTEGER NOT NULL DEFAULT (unixepoch())
     );
     CREATE TABLE bible_verses (
        id       INTEGER PRIMARY KEY,
        bible_id INTEGER NOT NULL REFERENCES bibles (id) ON DELETE CASCADE,
        book     INTEGER NOT NULL,              -- 1..66, Protestant canon order
        chapter  INTEGER NOT NULL,
        verse    INTEGER NOT NULL,
        text     TEXT NOT NULL,
        UNIQUE (bible_id, book, chapter, verse)
     );
     -- External content: maintained by asaf-library on import and delete.
     CREATE VIRTUAL TABLE bible_fts USING fts5(
        text,
        content = 'bible_verses',
        content_rowid = 'id',
        tokenize = 'unicode61 remove_diacritics 2'
     );",
    // 3: themes (the theme as asaf_core::Theme JSON; its name is the key) and
    // settings (asaf_core::Settings, one row per key).
    "CREATE TABLE themes (
        id         INTEGER PRIMARY KEY,
        name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
        data       TEXT NOT NULL,
        updated_at INTEGER NOT NULL DEFAULT (unixepoch())
     );
     CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
     );",
];

pub fn run(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}
