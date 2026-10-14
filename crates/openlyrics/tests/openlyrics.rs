use std::fs;
use std::path::Path;

use asaf_core::sample_song;
use asaf_openlyrics::{parse, read_file, read_folder, to_xml, write_folder};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn reads_openlyrics_0_9() {
    let s = read_file(&fixture("v09.xml")).unwrap();
    assert_eq!(s.title, "Cuán Grande Es Él");
    assert_eq!(s.alt_title.as_deref(), Some("How Great Thou Art"));
    assert_eq!(s.authors, ["Carl Boberg", "Arturo W. Hotton"]);
    assert_eq!(s.copyright.as_deref(), Some("Public domain"));
    assert_eq!(s.ccli_number.as_deref(), Some("14181"));
    assert_eq!(s.verse_order, ["v1", "c", "v2", "c"]);
    assert_eq!(
        s.verses[0].lines,
        [
            "Señor, mi Dios, al contemplar los cielos,",
            "el firmamento y las estrellas mil,",
            "al oír tu voz en los potentes truenos",
            "[---]",
            "y ver brillar al sol en su cenit",
        ]
    );
    assert_eq!(s.verses[1].lines, ["Mi corazón entona la canción:", "¡Cuán grande es Él! & más"]);
    assert_eq!(s.verses[2].lines, ["Al recorrer los montes y los valles"]);
    let labels: Vec<_> = s.slides().into_iter().map(|s| s.label).collect();
    assert_eq!(labels, ["V1.1", "V1.2", "C", "V2", "C"]);
}

#[test]
fn reads_openlyrics_0_8() {
    let s = read_file(&fixture("v08.xml")).unwrap();
    assert_eq!(s.title, "Amazing Grace");
    assert_eq!(s.verses.len(), 2);
    assert_eq!(s.verses[0].lines, ["Amazing grace! How sweet the sound", "That saved a wretch like me!"]);
    assert!(s.verse_order.is_empty());
}

#[test]
fn export_round_trips() {
    for song in [sample_song(), read_file(&fixture("v09.xml")).unwrap()] {
        let xml = to_xml(&song);
        assert_eq!(parse(&xml).unwrap(), song, "{xml}");
    }
}

#[test]
fn rejects_bad_files_with_a_reason() {
    assert!(parse("not xml").unwrap_err().to_string().contains("XML"));
    assert!(parse("<foo/>").unwrap_err().to_string().contains("OpenLyrics"));
    let no_lyrics = r#"<song xmlns="http://openlyrics.info/namespace/2009/song"><properties><titles><title>X</title></titles></properties><lyrics/></song>"#;
    assert!(parse(no_lyrics).unwrap_err().to_string().contains("no lyrics"));
}

#[test]
fn folder_round_trip_with_name_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = sample_song();
    let b = sample_song();
    a.title = "Blessed/Assurance?".into();
    let written = write_folder([&a, &b, &b], dir.path()).unwrap();
    let names: Vec<_> = written.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["Blessed_Assurance?.xml", "Blessed Assurance.xml", "Blessed Assurance (2).xml"]);

    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/broken.xml"), "<song>").unwrap();
    fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
    let read = read_folder(dir.path()).unwrap();
    assert_eq!(read.len(), 4);
    assert_eq!(read.iter().filter(|(_, r)| r.is_ok()).count(), 3);
}
