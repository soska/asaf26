use std::time::Instant;

use asaf_core::{Song, Verse, sample_song};
use asaf_library::Library;

fn song(title: &str, lyrics: &[(&str, &str)]) -> Song {
    Song {
        title: title.into(),
        authors: vec!["Anon".into()],
        verses: lyrics.iter().map(|(t, l)| Verse::new(t, l)).collect(),
        ..Default::default()
    }
}

#[test]
fn save_get_update_delete() {
    let mut lib = Library::open_in_memory().unwrap();
    let mut s = sample_song();
    let id = lib.save(&s).unwrap();
    s.id = Some(id);
    assert_eq!(lib.get(id).unwrap().unwrap(), s);

    s.title = "Blessed Assurance (edited)".into();
    s.verse_order = vec!["c1".into()];
    assert_eq!(lib.save(&s).unwrap(), id);
    assert_eq!(lib.get(id).unwrap().unwrap(), s);
    assert_eq!(lib.count().unwrap(), 1);
    assert_eq!(lib.search("edited", 10).unwrap().len(), 1);

    lib.delete(id).unwrap();
    assert!(lib.get(id).unwrap().is_none());
    assert!(lib.search("blessed", 10).unwrap().is_empty());
}

#[test]
fn search_by_title_lyrics_author_prefix_and_accents() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.save(&sample_song()).unwrap();
    lib.save(&song("Cuán Grande Es Él", &[("v1", "Señor mi Dios, al contemplar los cielos")])).unwrap();
    lib.save(&song("Story Time", &[("v1", "nothing to see")])).unwrap();

    let titles = |q: &str| -> Vec<String> {
        lib.search(q, 10).unwrap().into_iter().map(|s| s.title).collect()
    };
    assert_eq!(titles("cuan grande"), ["Cuán Grande Es Él"]);
    assert_eq!(titles("senor contem"), ["Cuán Grande Es Él"]);
    assert_eq!(titles("crosby"), ["Blessed Assurance"]);
    // Title hits rank above lyric hits.
    assert_eq!(titles("story"), ["Story Time", "Blessed Assurance"]);
    assert_eq!(titles(""), ["Blessed Assurance", "Cuán Grande Es Él", "Story Time"]);

    let hit = &lib.search("rapture", 10).unwrap()[0];
    assert!(hit.excerpt.contains("rapture"), "{hit:?}");
}

#[test]
fn search_and_excerpts_ignore_emphasis_markup() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.save(&song("Santo", &[("v1", "**¡Santo, santo!** Señor __omnipotente__")])).unwrap();
    assert_eq!(lib.search("", 10).unwrap()[0].excerpt, "¡Santo, santo! Señor omnipotente");
    let hit = &lib.search("santo omnipotente", 10).unwrap()[0];
    assert!(!hit.excerpt.contains('*') && !hit.excerpt.contains('_'), "{hit:?}");
}

#[test]
fn edits_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("asaf.sqlite");
    let mut s = sample_song();
    {
        let mut lib = Library::open(&path).unwrap();
        s.id = Some(lib.save(&s).unwrap());
        s.verses[0].lines[0] = "Blessed assurance, Jesus is mine! (fixed typo)".into();
        lib.save(&s).unwrap();
    }
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.get(s.id.unwrap()).unwrap().unwrap(), s);
    assert_eq!(lib.search("typo", 5).unwrap().len(), 1);
}

#[test]
fn duplicates_are_detected() {
    let mut lib = Library::open_in_memory().unwrap();
    let s = sample_song();
    let id = lib.save(&s).unwrap();
    assert_eq!(lib.find_duplicate(&s).unwrap(), Some(id));
    let mut other = s.clone();
    other.verses.pop();
    assert_eq!(lib.find_duplicate(&other).unwrap(), None);
}

#[test]
fn five_hundred_songs_search_under_50ms() {
    let mut lib = Library::open_in_memory().unwrap();
    let words = ["grace", "glory", "mercy", "light", "river", "mountain", "praise", "holy", "king", "love"];
    let songs: Vec<Song> = (0..500)
        .map(|i| {
            let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
            song(
                &format!("Song {i} of {}", w(0)),
                &[
                    ("v1", &format!("{} and {} forever\nwe sing of {} {i}\nthe {} is near", w(1), w(2), w(3), w(4))),
                    ("c1", &format!("{} {} {}\n{} {} {}", w(5), w(6), w(7), w(8), w(9), w(1))),
                    ("v2", &format!("second verse of {} {}\nand the {} of {}", w(2), w(4), w(6), w(8))),
                ],
            )
        })
        .collect();
    lib.save_all(&songs).unwrap();
    assert_eq!(lib.count().unwrap(), 500);

    // Simulate typing "mountain river" one key at a time.
    let typed = "mountain river";
    let mut worst = std::time::Duration::ZERO;
    for n in 1..=typed.len() {
        let t = Instant::now();
        let hits = lib.search(&typed[..n], 100).unwrap();
        worst = worst.max(t.elapsed());
        if n == typed.len() {
            assert!(!hits.is_empty());
        }
    }
    eprintln!("slowest keystroke search over 500 songs: {worst:?}");
    assert!(worst.as_millis() < 50, "{worst:?}");
}

#[test]
fn import_folder_skips_duplicates_and_reports_failures() {
    let dir = tempfile::tempdir().unwrap();
    let mut other = sample_song();
    other.title = "Another".into();
    asaf_openlyrics::write_folder([&sample_song(), &other], dir.path()).unwrap();
    std::fs::write(dir.path().join("bad.xml"), "<nope").unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    let r = lib.import_openlyrics(&[dir.path()]).unwrap();
    assert_eq!((r.imported, r.duplicates, r.failed.len()), (2, 0, 1), "{r:?}");
    let r = lib.import_openlyrics(&[dir.path()]).unwrap();
    assert_eq!((r.imported, r.duplicates), (0, 2));
    assert_eq!(lib.count().unwrap(), 2);
    assert!(r.summary().contains("skipped 2"));
}
