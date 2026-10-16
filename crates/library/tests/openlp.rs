//! Migration from an OpenLP data folder: fixture databases built from SQL
//! that follows OpenLP's own schemas (see tests/fixtures/openlp/*.sql).

use std::path::{Path, PathBuf};

use asaf_core::{Song, Verse, parse_reference};
use asaf_library::Library;
use asaf_library::openlp::{OpenLpFiles, find_openlp_files};
use rusqlite::Connection;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/openlp").join(name)).unwrap()
}

fn build(path: &Path, sql: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    Connection::open(path).unwrap().execute_batch(sql).unwrap();
}

/// A data folder laid out the way OpenLP leaves it on disk.
fn openlp_folder(root: &Path) -> PathBuf {
    let data = root.join("openlp/Data");
    build(&data.join("songs/songs.sqlite"), &fixture("songs.sql"));
    build(&data.join("bibles/King James Version.sqlite"), &fixture("kjv.sql"));
    build(&data.join("bibles/Reina-Valera 1909.sqlite"), &fixture("rv1909.sql"));
    build(&data.join("bibles/NIV.sqlite"), &fixture("web-bible.sql"));
    // OpenLP keeps this next to the Bibles; it isn't one.
    build(
        &data.join("bibles/alternative_book_names.sqlite"),
        "CREATE TABLE alternative_book_names (id INTEGER, book_reference_id INTEGER, language_id INTEGER, name VARCHAR(50));",
    );
    std::fs::write(data.join("bibles/notes.txt"), "not a database").unwrap();
    data
}

#[test]
fn finds_the_databases_from_the_data_folder_or_above() {
    let dir = tempfile::tempdir().unwrap();
    let data = openlp_folder(dir.path());
    let want = OpenLpFiles {
        songs: Some(data.join("songs/songs.sqlite")),
        bibles: vec![
            data.join("bibles/King James Version.sqlite"),
            data.join("bibles/NIV.sqlite"),
            data.join("bibles/Reina-Valera 1909.sqlite"),
        ],
    };
    assert_eq!(find_openlp_files(&data), want);
    assert_eq!(find_openlp_files(&dir.path().join("openlp")), want);
    let bibles_only = find_openlp_files(&data.join("bibles"));
    assert_eq!((bibles_only.songs, bibles_only.bibles.len()), (None, 3));
    assert_eq!(find_openlp_files(&data.join("nowhere")), OpenLpFiles::default());
}

#[test]
fn imports_songs_and_bibles_in_one_step() {
    let dir = tempfile::tempdir().unwrap();
    let data = openlp_folder(dir.path());
    let mut lib = Library::open_in_memory().unwrap();
    lib.save(&Song {
        title: "Blessed Assurance".into(),
        verses: vec![Verse::new("v1", "Blessed assurance, Jesus is mine!\nO what a foretaste of glory divine!")],
        ..Default::default()
    })
    .unwrap();

    let report = lib.import_openlp(&data).unwrap();
    assert_eq!(report.songs.imported, 3, "{:?}", report.songs);
    assert_eq!(report.songs.duplicates, 1, "Blessed Assurance is already there");
    assert_eq!(report.songs.failed.len(), 1);
    assert!(report.songs.failed[0].0.to_string_lossy().contains("Broken Song"));
    assert_eq!(
        report.summary(),
        "Imported 3 songs, skipped 1 already in the library, 1 file failed; imported 2 Bibles (KJV, RV1909), 1 Bible not imported."
    );
    let problems = report.problems().join("\n");
    assert!(problems.contains("NIV.sqlite: NIV is a web Bible"), "{problems}");
    assert!(problems.contains("KJV: skipped books outside the 66: Judith"), "{problems}");

    // Songs: chords and other formatting gone, italics kept as emphasis, splits and verse order kept, authors once each.
    let id = lib.search("amazing", 5).unwrap()[0].id;
    let amazing = lib.get(id).unwrap().unwrap();
    assert_eq!(amazing.alt_title.as_deref(), Some("New Britain"));
    assert_eq!(amazing.authors, ["John Newton"]);
    assert_eq!(amazing.ccli_number.as_deref(), Some("22025"));
    assert_eq!(amazing.verse_order, ["v1", "v2", "v1", "e1"]);
    assert_eq!(
        amazing.verses[0].lines,
        ["Amazing grace! How sweet the sound", "That saved a wretch like me!", "[---]", "I once was lost, but now am __found__;", "Was blind, but now I see."]
    );
    assert_eq!(amazing.verses[2].lines, ["Praise God, praise God!"]);
    assert_eq!(amazing.slides().len(), 6, "v1 split in two, twice");
    assert_eq!(lib.search("amazing", 5).unwrap().len(), 1, "the temporary copy isn't imported");
    assert_eq!(lib.search("now am found", 5).unwrap().len(), 1, "search ignores the markers");

    let holy = lib.get(lib.search("holy holy", 5).unwrap()[0].id).unwrap().unwrap();
    assert_eq!(holy.authors, ["Reginald Heber", "John B. Dykes"]);
    let castillo = lib.get(lib.search("castillo", 5).unwrap()[0].id).unwrap().unwrap();
    assert_eq!(castillo.verse_order, ["v1", "c1", "v2", "c1"]);
    assert_eq!(castillo.authors, ["Martín Lutero", "Juan B. Cabrera"]);
    assert_eq!(castillo.verses[1].lines, ["**Su reino es,** su reino es."]);
    assert!(lib.search("reino", 5).unwrap().iter().any(|s| s.title == "Castillo Fuerte"), "indexed for search");

    // Bibles: names, languages and book names from OpenLP; references work.
    let bibles = lib.bibles().unwrap();
    let names: Vec<_> = bibles.iter().map(|b| (b.abbreviation.as_str(), b.name.as_str(), b.language.as_str())).collect();
    assert_eq!(names, [("KJV", "King James Version", "en"), ("RV1909", "Reina-Valera 1909", "es")]);
    let rv = &bibles[1];
    assert_eq!(rv.book_names[42], "San Juan");
    assert_eq!(rv.copyright.as_deref(), Some("Dominio público"));
    let ranges = parse_reference("San Juan 3:16-17", &rv.names()).unwrap();
    let verses = lib.passage_verses(rv.id.unwrap(), &ranges).unwrap();
    assert_eq!(verses.len(), 2);
    assert!(verses[0].text.starts_with("Porque de tal manera amó Dios"));
    let kjv = &bibles[0];
    let ranges = parse_reference("Jn 3:17", &kjv.names()).unwrap();
    assert_eq!(
        lib.passage_verses(kjv.id.unwrap(), &ranges).unwrap()[0].text,
        "For God sent not his Son into the world to condemn the world; but that the world through him might be saved."
    );
    assert_eq!(lib.verse_count(kjv.id.unwrap()).unwrap(), 5, "Judith left out");

    // Running it again changes nothing.
    let again = lib.import_openlp(&data).unwrap();
    assert_eq!((again.songs.imported, again.songs.duplicates), (0, 4));
    let errors: Vec<_> = again.bibles.iter().filter_map(|(_, r)| r.as_ref().err()).collect();
    assert_eq!(errors.len(), 3);
    assert_eq!(errors.iter().filter(|e| e.contains("is already installed")).count(), 2, "{errors:?}");
    assert_eq!(lib.count().unwrap(), 4);
}

#[test]
fn reads_old_openlp_2_song_databases() {
    let dir = tempfile::tempdir().unwrap();
    build(&dir.path().join("songs.sqlite"), &fixture("songs-openlp-2.0.sql"));
    let mut lib = Library::open_in_memory().unwrap();
    let report = lib.import_openlp(dir.path()).unwrap();
    assert_eq!((report.songs.imported, report.bibles.len()), (1, 0));
    let song = lib.get(lib.search("joy", 5).unwrap()[0].id).unwrap().unwrap();
    assert_eq!(song.authors, ["Isaac Watts"]);
    let tags: Vec<_> = song.verses.iter().map(|v| v.tag.as_str()).collect();
    assert_eq!(tags, ["v1", "c1"]);
    assert_eq!(song.slides().len(), 2);
}

#[test]
fn a_folder_without_openlp_data_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    let err = lib.import_openlp(dir.path()).unwrap_err();
    assert!(err.starts_with("No OpenLP songs.sqlite or Bible databases"), "{err}");
}

#[test]
fn openlp_files_are_never_changed() {
    let dir = tempfile::tempdir().unwrap();
    let data = openlp_folder(dir.path());
    let songs = data.join("songs/songs.sqlite");
    let before = std::fs::read(&songs).unwrap();
    Library::open_in_memory().unwrap().import_openlp(&data).unwrap();
    assert_eq!(std::fs::read(&songs).unwrap(), before);
    assert!(!data.join("songs/songs.sqlite-wal").exists());
}

#[test]
fn bibles_whose_names_abbreviate_alike_both_import() {
    // "Reina Valera Antigua" and "Reina Valera Actualizada" both shorten to
    // RVA; the second must not be refused as "already installed".
    let dir = tempfile::tempdir().unwrap();
    let rv = fixture("rv1909.sql");
    let named = |name: &str| rv.replace("'Reina-Valera 1909'", &format!("'{name}'"));
    assert_ne!(named("x"), rv, "the fixture names its Bible");
    build(&dir.path().join("bibles/a.sqlite"), &named("Reina Valera Antigua"));
    build(&dir.path().join("bibles/b.sqlite"), &named("Reina Valera Actualizada"));
    let mut lib = Library::open_in_memory().unwrap();
    let report = lib.import_openlp(dir.path()).unwrap();
    assert!(report.bibles.iter().all(|(_, r)| r.is_ok()), "{:?}", report.problems());
    let abbrevs: Vec<_> = lib.bibles().unwrap().into_iter().map(|b| (b.abbreviation, b.name)).collect();
    assert_eq!(
        abbrevs,
        [("RVA2".to_string(), "Reina Valera Actualizada".to_string()), ("RVA".into(), "Reina Valera Antigua".into())]
    );
    // The same translations again are still recognised as installed.
    let again = lib.import_openlp(dir.path()).unwrap();
    assert!(again.bibles.iter().all(|(_, r)| r.as_ref().is_err_and(|e| e.contains("already installed"))));
}
