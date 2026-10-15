use std::path::Path;
use std::time::{Duration, Instant};

use asaf_core::bible::BOOKS;
use asaf_core::reference::osis_list;
use asaf_core::{Bible, BibleLayout, BibleVerse, Theme, parse_reference};
use asaf_library::Library;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../bible/tests/fixtures").join(name)
}

fn with_fixtures() -> (Library, i64, i64) {
    let mut lib = Library::open_in_memory().unwrap();
    let kjv = lib.import_bible(&fixture("kjv-excerpt.osis.xml")).unwrap();
    let rv = lib.import_bible(&fixture("rv1909-excerpt.zefania.xml")).unwrap();
    (lib, kjv.bible.id.unwrap(), rv.bible.id.unwrap())
}

#[test]
fn import_list_and_refuse_duplicates() {
    let (mut lib, kjv, rv) = with_fixtures();
    let bibles = lib.bibles().unwrap();
    let names: Vec<&str> = bibles.iter().map(|b| b.abbreviation.as_str()).collect();
    assert_eq!(names, ["KJV", "RV1909"]);
    assert_eq!(lib.bible(rv).unwrap().unwrap().book_names[42], "Juan");
    assert_eq!(lib.bible(rv).unwrap().unwrap().language, "es");
    assert_eq!(lib.find_bible("rv1909").unwrap(), Some(rv));
    assert_eq!(lib.verse_count(kjv).unwrap(), 31 + 25 + 6 + 16 + 36 + 25);

    let err = lib.import_bible(&fixture("rv1909-excerpt.zefania.xml")).unwrap_err();
    assert!(err.contains("already installed"), "{err}");
    assert!(lib.import_bible(Path::new("/nonexistent.xml")).is_err());
    assert_eq!(lib.bibles().unwrap().len(), 2);
}

#[test]
fn passages_in_both_languages() {
    let (lib, kjv, rv) = with_fixtures();
    let rv_names = lib.bible(rv).unwrap().unwrap().names();
    let kjv_names = lib.bible(kjv).unwrap().unwrap().names();

    let fetch = |bible: i64, names, reference: &str| {
        let ranges = parse_reference(reference, names).unwrap();
        lib.passage_verses(bible, &ranges).unwrap()
    };
    let keys = |vs: &[BibleVerse]| vs.iter().map(|v| format!("{}:{}", v.chapter, v.verse)).collect::<Vec<_>>();

    let v = fetch(rv, &rv_names, "Jn 3:16-18");
    assert_eq!(keys(&v), ["3:16", "3:17", "3:18"]);
    assert!(v[0].text.starts_with("Porque de tal manera amó Dios al mundo"));

    let v = fetch(rv, &rv_names, "Gn 1:30-2:3");
    assert_eq!(keys(&v), ["1:30", "1:31", "2:1", "2:2", "2:3"]);

    let v = fetch(rv, &rv_names, "Sal 23; 91:1-4");
    assert_eq!(v.len(), 6 + 4);
    assert_eq!((v[6].chapter, v[6].verse), (91, 1));

    let v = fetch(kjv, &kjv_names, "Psalm 23");
    assert_eq!(v.len(), 6);
    assert_eq!(v[0].text, "The LORD is my shepherd; I shall not want.");

    let v = fetch(kjv, &kjv_names, "John 3:16-");
    assert_eq!((v.len(), v.last().unwrap().verse), (21, 36));

    let v = fetch(kjv, &kjv_names, "Jude 24-25");
    assert_eq!(keys(&v), ["1:24", "1:25"]);
    assert!(v[1].text.starts_with("To the only wise God our Saviour"));

    // Past the end of a chapter: nothing, not an error.
    assert!(fetch(kjv, &kjv_names, "Ps 23:7-9").is_empty());
    // Same reference, other translation.
    assert_eq!(fetch(kjv, &kjv_names, "Juan 3:16").len(), 1);
}

#[test]
fn text_search_within_a_translation() {
    let (mut lib, kjv, rv) = with_fixtures();
    let hits = lib.search_bible(rv, "tal manera amo", 10).unwrap();
    assert_eq!((hits[0].book, hits[0].chapter, hits[0].verse), (43, 3, 16));
    // Accents and case are ignored; words are prefixes.
    assert_eq!(lib.search_bible(rv, "JEHOVA pastor", 10).unwrap()[0].chapter, 23);
    // "pastor" also matches "pastoreará" (Ps 23:2).
    assert_eq!(lib.search_bible(rv, "pastor", 10).unwrap().len(), 2);
    // Only the chosen translation.
    assert!(lib.search_bible(kjv, "pastor", 10).unwrap().is_empty());
    assert_eq!(lib.search_bible(kjv, "shepherd", 10).unwrap().len(), 1);
    assert!(lib.search_bible(kjv, "  ", 10).unwrap().is_empty());

    lib.delete_bible(kjv).unwrap();
    assert!(lib.search_bible(kjv, "shepherd", 10).unwrap().is_empty());
    assert_eq!(lib.verse_count(kjv).unwrap(), 0);
    assert_eq!(lib.search_bible(rv, "pastor", 10).unwrap().len(), 2);
    assert_eq!(lib.bibles().unwrap().len(), 1);
}

#[test]
fn songs_database_from_phase_1_migrates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("asaf.sqlite");
    {
        // A Phase 1 database: songs only, user_version 1.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE songs (id INTEGER PRIMARY KEY, title TEXT NOT NULL, alt_title TEXT,
                authors TEXT NOT NULL DEFAULT '', copyright TEXT, ccli_number TEXT, lyrics TEXT NOT NULL,
                verse_order TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE INDEX songs_title ON songs (title COLLATE NOCASE);
             CREATE VIRTUAL TABLE songs_fts USING fts5(title, alt_title, authors, lyrics,
                tokenize = 'unicode61 remove_diacritics 2');
             INSERT INTO songs (title, lyrics) VALUES ('Old song', '[v1]\nhello');
             INSERT INTO songs_fts (rowid, title, alt_title, authors, lyrics) VALUES (1, 'Old song', NULL, '', 'hello');
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let mut lib = Library::open(&path).unwrap();
    assert_eq!(lib.search("old", 5).unwrap()[0].title, "Old song");
    lib.import_bible(&fixture("kjv-excerpt.osis.xml")).unwrap();
    drop(lib);
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.bibles().unwrap().len(), 1);
    assert_eq!(lib.count().unwrap(), 1);
}

/// A full-size synthetic translation: every chapter of all 66 books, 26
/// verses each (~31,000 verses), with made-up text.
fn synthetic_bible(language: &str, abbreviation: &str) -> (Bible, Vec<BibleVerse>) {
    let words = ["lux", "aqua", "terra", "caelum", "verbum", "vita", "pax", "via", "spes", "fides", "gloria", "lumen"];
    let mut verses = Vec::new();
    for (i, b) in BOOKS.iter().enumerate() {
        for chapter in 1..=b.chapters {
            for verse in 1..=26u16 {
                let n = i * 7919 + usize::from(chapter) * 104729 + usize::from(verse) * 31;
                let text: Vec<&str> = (0..24).map(|k| words[(n / (k + 1) + k * 5) % words.len()]).collect();
                verses.push(BibleVerse { book: i as u8 + 1, chapter, verse, text: text.join(" ") });
            }
        }
    }
    let bible = Bible {
        name: format!("Synthetic {abbreviation}"),
        abbreviation: abbreviation.into(),
        language: language.into(),
        ..Default::default()
    };
    (bible, verses)
}

/// The Phase 2 bar: a typed reference shows its verses in well under a
/// second. Measures parse + fetch + layout on full-size translations.
#[test]
fn reference_lookup_on_full_bibles_is_fast() {
    let mut lib = Library::open_in_memory().unwrap();
    let t = Instant::now();
    let (es, es_verses) = synthetic_bible("es", "SES");
    let (en, en_verses) = synthetic_bible("en", "SEN");
    let es_id = lib.save_bible(&es, &es_verses).unwrap();
    let en_id = lib.save_bible(&en, &en_verses).unwrap();
    eprintln!("stored 2 × {} verses in {:?}", es_verses.len(), t.elapsed());

    let cases = [
        (es_id, &es, "Jn 3:16-18", "John.3.16-18", 3),
        (es_id, &es, "Gn 1:25-2:3", "Gen.1.25-2.3", 5),
        (es_id, &es, "Sal 23; 91:1-4", "Ps.23; Ps.91.1-4", 26 + 4),
        (es_id, &es, "Primera de Juan 4", "1John.4", 26),
        (es_id, &es, "Ap 21-22", "Rev.21-22", 52),
        (es_id, &es, "Sal 119", "Ps.119", 26),
        (en_id, &en, "John 3:16-18", "John.3.16-18", 3),
        (en_id, &en, "Gen 1:25-2:3", "Gen.1.25-2.3", 5),
        (en_id, &en, "Ps 23; 91:1-4", "Ps.23; Ps.91.1-4", 30),
        (en_id, &en, "1 Cor 13", "1Cor.13", 26),
        (en_id, &en, "Rom 8:18, 24-25", "Rom.8.18; Rom.8.24-25", 3),
    ];
    let mut worst = Duration::ZERO;
    for (id, bible, input, expected, count) in cases {
        let t = Instant::now();
        let names = bible.names();
        let ranges = parse_reference(input, &names).unwrap();
        let verses = lib.passage_verses(id, &ranges).unwrap();
        let slides = asaf_core::bible::passage_slides(
            &verses,
            &ranges,
            &names,
            &bible.abbreviation,
            &BibleLayout::default(),
            &Theme::default(),
        );
        let elapsed = t.elapsed();
        worst = worst.max(elapsed);
        assert_eq!(osis_list(&ranges), expected, "{input}");
        assert_eq!(verses.len(), count, "{input}");
        assert!(!slides.is_empty());
    }
    eprintln!("slowest reference → slides on a full Bible: {worst:?}");
    assert!(worst < Duration::from_millis(100), "{worst:?}");

    let t = Instant::now();
    let hits = lib.search_bible(es_id, "verbum gloria", 50).unwrap();
    let search = t.elapsed();
    eprintln!("text search over a full Bible: {search:?} ({} hits)", hits.len());
    assert!(!hits.is_empty());
    assert!(search < Duration::from_millis(500), "{search:?}");
}

/// The full-text index points at verse rows by rowid, so rowids must survive
/// a VACUUM (they only do for an explicit INTEGER PRIMARY KEY).
#[test]
fn search_survives_vacuum_after_a_delete() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("asaf.sqlite");
    let rv = {
        let mut lib = Library::open(&path).unwrap();
        let kjv = lib.import_bible(&fixture("kjv-excerpt.osis.xml")).unwrap().bible.id.unwrap();
        let rv = lib.import_bible(&fixture("rv1909-excerpt.zefania.xml")).unwrap().bible.id.unwrap();
        lib.delete_bible(kjv).unwrap();
        rv
    };
    rusqlite::Connection::open(&path).unwrap().execute_batch("VACUUM").unwrap();
    let lib = Library::open(&path).unwrap();
    let hits = lib.search_bible(rv, "tal manera amo", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].book, hits[0].chapter, hits[0].verse), (43, 3, 16));
    assert!(hits[0].text.starts_with("Porque de tal manera"), "{}", hits[0].text);
}
