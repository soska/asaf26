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
