//! Service files on disk: save/load round trip, atomic writes, autosave
//! restore, and finding a service's songs again.

use std::fs;

use asaf_core::{AutosaveInfo, RefRange, Service, ServiceItem, Song, Verse, VerseRef, sample_song};
use asaf_library::Library;
use asaf_library::service::{
    AUTOSAVE_FILE, read_service, restore_autosave, service_name, with_service_extension, write_atomic,
    write_autosave, write_service,
};

fn service() -> Service {
    let mut s = Service { name: "Sunday".into(), items: Vec::new() };
    s.push(ServiceItem::song(1, "Blessed Assurance"));
    let mut item = ServiceItem::passage(
        2,
        "KJV",
        "John 3:16-18",
        vec![RefRange { book: 43, start: VerseRef { chapter: 3, verse: 16 }, end: VerseRef { chapter: 3, verse: 18 } }],
    );
    item.notes = "Pastor reads this one".into();
    item.theme = Some("Scripture".into());
    s.push(item);
    s
}

#[test]
fn save_and_load_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Sunday.asaf-service");
    write_service(&path, &service()).unwrap();
    let (back, autosave) = read_service(&path).unwrap();
    assert_eq!(back, service());
    assert_eq!(autosave, None);
    // Nothing but the file itself is left behind.
    let names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["Sunday.asaf-service"]);
}

#[test]
fn atomic_write_replaces_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.asaf-service");
    write_atomic(&path, &"a".repeat(10_000)).unwrap();
    write_atomic(&path, "short").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "short");
    // A target in a missing folder fails cleanly without leaving a temp file.
    assert!(write_atomic(&dir.path().join("missing/x.asaf-service"), "x").is_err());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

/// The debounced autosave runs on a background thread while a save or the
/// quit-time flush may write the same file: every write must still land
/// whole, each through its own temporary file.
#[test]
fn overlapping_writes_to_one_file_never_mix() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.asaf-service");
    let contents: Vec<String> = (0..8).map(|t| t.to_string().repeat(20_000 + t * 1000)).collect();
    std::thread::scope(|s| {
        for text in &contents {
            let path = &path;
            s.spawn(move || {
                for _ in 0..25 {
                    write_atomic(path, text).unwrap();
                }
            });
        }
    });
    let on_disk = fs::read_to_string(&path).unwrap();
    assert!(contents.contains(&on_disk), "a whole write, not a mix");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary files left");
}

#[test]
fn autosave_restores_service_path_and_dirty_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(AUTOSAVE_FILE);
    assert_eq!(restore_autosave(&path).service, Service::default(), "no autosave yet");

    let info = AutosaveInfo { path: Some(dir.path().join("Sunday.asaf-service")), dirty: true };
    write_autosave(&path, &service(), &info).unwrap();
    let restored = restore_autosave(&path);
    assert_eq!(restored.service, service());
    assert_eq!(restored.info, info);
    assert!(restored.problem.is_none());
}

#[test]
fn damaged_autosave_is_kept_aside_not_lost() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(AUTOSAVE_FILE);
    fs::write(&path, "{\"format\":\"asaf-service\",\"version\":1,\"items\":[{\"type\":").unwrap();
    let restored = restore_autosave(&path);
    assert!(restored.service.items.is_empty());
    let problem = restored.problem.expect("reported");
    assert!(problem.contains("isn't an Asaf service"), "{problem}");
    assert!(!path.exists());
    assert!(dir.path().join("autosave.asaf-service.corrupt").exists());

    // Damaged again on a later start: the first damaged copy is kept too.
    fs::write(&path, "second").unwrap();
    let problem = restore_autosave(&path).problem.expect("reported");
    assert!(problem.contains("autosave.asaf-service.corrupt-2"), "{problem}");
    assert_eq!(fs::read_to_string(dir.path().join("autosave.asaf-service.corrupt-2")).unwrap(), "second");
    assert!(fs::read_to_string(dir.path().join("autosave.asaf-service.corrupt")).unwrap().contains("format"));
}

#[test]
fn reading_reports_problems_in_plain_words() {
    let dir = tempfile::tempdir().unwrap();
    let missing = read_service(&dir.path().join("nope.asaf-service")).unwrap_err();
    assert!(missing.starts_with("Can't read"), "{missing}");
    let newer = dir.path().join("newer.asaf-service");
    fs::write(&newer, r#"{"format":"asaf-service","version":9}"#).unwrap();
    let err = read_service(&newer).unwrap_err();
    assert!(err.contains("newer.asaf-service") && err.contains("newer Asaf"), "{err}");
}

#[test]
fn file_names() {
    let cases = [
        ("/a/Sunday", "/a/Sunday.asaf-service"),
        ("/a/Sunday.asaf-service", "/a/Sunday.asaf-service"),
        ("/a/Sunday 10.30", "/a/Sunday 10.30.asaf-service"),
    ];
    for (given, expected) in cases {
        assert_eq!(with_service_extension(given.into()), std::path::PathBuf::from(expected), "{given}");
    }
    assert_eq!(service_name("/a/Sunday 10.30.asaf-service".as_ref()), "Sunday 10.30");
}

#[test]
fn service_songs_are_found_by_id_then_title() {
    let mut lib = Library::open_in_memory().unwrap();
    let blessed = lib.save(&sample_song()).unwrap();
    let other = lib
        .save(&Song { title: "Cuán Grande Es Él".into(), verses: vec![Verse::new("v1", "Señor")], ..Default::default() })
        .unwrap();
    let title = |s: Option<Song>| s.map(|s| s.title);

    // Same machine: id and title agree.
    assert_eq!(title(lib.find_service_song(blessed, "Blessed Assurance").unwrap()).as_deref(), Some("Blessed Assurance"));
    // Another machine: the id points elsewhere, the title wins.
    assert_eq!(title(lib.find_service_song(other, "blessed assurance").unwrap()).as_deref(), Some("Blessed Assurance"));
    // Another machine's song this library doesn't have: missing, never
    // whatever song happens to have the same id here.
    assert!(lib.find_service_song(other, "How Great Thou Art").unwrap().is_none());
    // Neither.
    assert!(lib.find_service_song(99, "Nope").unwrap().is_none());
}
