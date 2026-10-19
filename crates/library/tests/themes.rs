//! Themes, settings and imported images in the library.

use asaf_core::{Align, Appearance, Language, Settings, Theme, TransitionKind, builtin_themes};
use asaf_library::Library;
use std::path::PathBuf;

/// Theme names that aren't built in.
fn own_names(lib: &Library) -> Vec<String> {
    let builtin: Vec<String> = builtin_themes().into_iter().map(|t| t.name).collect();
    lib.themes().unwrap().into_iter().map(|t| t.name).filter(|n| !builtin.contains(n)).collect()
}

#[test]
fn a_new_library_has_the_default_and_builtin_themes() {
    let lib = Library::open_in_memory().unwrap();
    let mut want = builtin_themes();
    want.push(Theme::default());
    want.sort_by_key(|t| t.name.to_lowercase());
    assert_eq!(lib.themes().unwrap(), want);
    assert_eq!(lib.default_theme().unwrap(), Theme::default());
    assert_eq!(lib.settings().unwrap(), Settings::default());
}

#[test]
fn themes_save_rename_and_delete() {
    let mut lib = Library::open_in_memory().unwrap();
    let scripture = Theme {
        name: "Scripture".into(),
        font_family: "Lora".into(),
        align: Align::Left,
        transition: TransitionKind::Slide,
        transition_ms: 450,
        background_image: Some("/media/sky-0011aabb.jpg".into()),
        ..Theme::default()
    };
    assert_eq!(lib.save_theme(&scripture, None).unwrap(), scripture);
    assert_eq!(lib.theme("scripture").unwrap().unwrap(), scripture, "found case-insensitively");

    // Names are unique, whatever the case.
    let clash = Theme { name: "SCRIPTURE".into(), ..Theme::default() };
    assert_eq!(lib.save_theme(&clash, None).unwrap_err().to_string(), "There is already a theme called “Scripture”.");
    assert_eq!(lib.save_theme(&clash, Some("Default")).unwrap_err().to_string(), "There is already a theme called “Scripture”.");

    // Saved values are sanitized.
    let bad = Theme { name: "  Loud ".into(), color: "nope".into(), size_min_vh: 50.0, ..Theme::default() };
    let stored = lib.save_theme(&bad, None).unwrap();
    assert_eq!((stored.name.as_str(), stored.color.as_str(), stored.size_min_vh), ("Loud", "#ffffff", 40.0));

    // Renaming the default theme keeps it the default.
    let mut default = lib.default_theme().unwrap();
    default.name = "Sunday".into();
    default.color = "#ffeeaa".into();
    lib.save_theme(&default, Some("Default")).unwrap();
    assert_eq!(lib.default_theme().unwrap(), default);
    lib.save_theme(&Theme { name: "Sunday AM".into(), ..default.clone() }, Some("sunday")).unwrap();
    assert_eq!(lib.default_theme().unwrap().name, "Sunday AM");
    assert_eq!(lib.settings().unwrap().default_theme, "Sunday AM");
    // Renaming another theme leaves the default alone.
    lib.save_theme(&scripture, Some("Scripture")).unwrap();
    assert_eq!(lib.settings().unwrap().default_theme, "Sunday AM");
    assert_eq!(own_names(&lib), ["Loud", "Scripture", "Sunday AM"]);

    // The default can't be deleted; others can.
    assert!(lib.delete_theme("sunday am").unwrap_err().to_string().contains("is the default theme"));
    lib.delete_theme("Loud").unwrap();
    assert!(lib.theme("Loud").unwrap().is_none());
    assert_eq!(lib.unused_theme_name("Scripture").unwrap(), "Scripture 2");
    assert_eq!(lib.unused_theme_name("New theme").unwrap(), "New theme");
}

#[test]
fn themes_and_settings_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("asaf.sqlite");
    let theme = Theme { name: "Night".into(), background_color: "#000000".into(), ..Theme::default() };
    let settings = Settings {
        display_uuid: Some("A1B2C3D4-0000-1111-2222-333344445555".into()),
        port: 4600,
        default_theme: "Night".into(),
        language: Language::Spanish,
        appearance: Appearance::Dark,
        lan: true,
        remote_pin: "2468".into(),
    };
    {
        let mut lib = Library::open(&path).unwrap();
        lib.save_theme(&theme, None).unwrap();
        lib.save_settings(&settings).unwrap();
    }
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.settings().unwrap(), settings);
    assert_eq!(lib.default_theme().unwrap(), theme);
    assert_eq!(own_names(&lib), ["Default", "Night"]);
}

#[test]
fn builtin_themes_come_once_and_never_replace_a_theme() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("asaf.sqlite");
    {
        let mut lib = Library::open(&path).unwrap();
        lib.delete_theme("Aurora").unwrap();
    }
    let lib = Library::open(&path).unwrap();
    assert!(lib.theme("Aurora").unwrap().is_none(), "a deleted built-in stays deleted");
    drop(lib);

    // A library from before built-ins, with a theme of its own called Jade.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("DELETE FROM settings WHERE key = 'builtin_themes'", []).unwrap();
    conn.execute("DELETE FROM themes WHERE name <> 'Default'", []).unwrap();
    conn.execute("INSERT INTO themes (name, data) VALUES ('jade', '{\"color\":\"#00ff00\"}')", []).unwrap();
    drop(conn);
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.themes().unwrap().len(), 1 + builtin_themes().len());
    assert_eq!(lib.theme("Jade").unwrap().unwrap().color, "#00ff00", "the library's own Jade is kept");
    assert_eq!(lib.theme("Aurora").unwrap().unwrap(), builtin_themes()[0]);
    assert_eq!(lib.settings().unwrap(), Settings::default(), "the marker isn't a setting");
    drop(lib);

    // A library seeded with the first set, Aurora since deleted: an update
    // brings only the themes added after it.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("UPDATE settings SET value = '1' WHERE key = 'builtin_themes'", []).unwrap();
    conn.execute("DELETE FROM themes WHERE name IN ('Aurora', 'Panel')", []).unwrap();
    drop(conn);
    let lib = Library::open(&path).unwrap();
    assert!(lib.theme("Aurora").unwrap().is_none(), "a deleted first-set theme stays deleted");
    assert!(lib.theme("Panel").unwrap().is_some(), "the newer theme arrives");
}

#[test]
fn a_damaged_stored_theme_still_shows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("asaf.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("INSERT INTO themes (name, data) VALUES ('Broken', '{not json')", []).unwrap();
    conn.execute("INSERT INTO themes (name, data) VALUES ('Partial', '{\"color\":\"#00ff00\",\"name\":\"ignored\"}')", [])
        .unwrap();
    drop(conn);
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.theme("Broken").unwrap().unwrap(), Theme { name: "Broken".into(), ..Theme::default() });
    let partial = lib.theme("Partial").unwrap().unwrap();
    assert_eq!((partial.name.as_str(), partial.color.as_str(), partial.font_weight), ("Partial", "#00ff00", 700));
}

#[test]
fn images_are_copied_into_the_media_folder() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(&dir.path().join("asaf.sqlite")).unwrap();
    let media = dir.path().join("media");
    assert_eq!(lib.media_dir(), Some(media.as_path()));

    let src = dir.path().join("Cruz al Atardecer.PNG");
    std::fs::write(&src, b"\x89PNG fake image bytes").unwrap();
    let url = lib.import_media(&src).unwrap();
    assert!(url.starts_with("/media/cruz-al-atardecer-") && url.ends_with(".png"), "{url}");
    let copy = lib.media_path(&url).unwrap();
    assert_eq!(std::fs::read(&copy).unwrap(), b"\x89PNG fake image bytes");
    // Same file again: same copy. The original can go away.
    assert_eq!(lib.import_media(&src).unwrap(), url);
    std::fs::remove_file(&src).unwrap();
    assert!(copy.exists());
    // Different contents under the same name: a second copy.
    std::fs::write(&src, b"other").unwrap();
    assert_ne!(lib.import_media(&src).unwrap(), url);
    assert_eq!(std::fs::read_dir(&media).unwrap().count(), 2);

    std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
    assert!(lib.import_media(&dir.path().join("notes.txt")).unwrap_err().contains("isn't an image"));
    assert!(lib.import_media(&dir.path().join("missing.jpg")).unwrap_err().starts_with("Can't read"));
    assert_eq!(lib.media_path("/media/../asaf.sqlite"), None);
    assert!(Library::open_in_memory().unwrap().import_media(&src).is_err());
}

#[test]
fn themes_travel_in_files_with_their_media() {
    let here = tempfile::tempdir().unwrap();
    let lib = Library::open(&here.path().join("asaf.sqlite")).unwrap();
    let still = here.path().join("Amanecer.jpg");
    let clip = here.path().join("Olas.mp4");
    std::fs::write(&still, b"jpeg bytes").unwrap();
    std::fs::write(&clip, vec![7u8; 300_000]).unwrap();
    let theme = Theme {
        name: "Sunday".into(),
        background_image: Some(lib.import_media(&still).unwrap()),
        background_video: Some(lib.import_media(&clip).unwrap()),
        logo: Some("/assets/logo.svg".into()),
        color: "#ffee00".into(),
        ..Theme::default()
    };
    let file = asaf_library::with_theme_extension(here.path().join("Sunday"));
    let missing = Theme { logo: Some("/media/gone-00000000.png".into()), ..theme.clone() };
    assert_eq!(lib.export_theme(&missing, &file), Err(asaf_library::ThemeFileError::MissingMedia("gone-00000000.png".into())));
    assert!(!file.exists(), "nothing written");
    let theme = Theme { logo: theme.background_image.clone(), ..theme };
    assert!(file.ends_with("Sunday.asaf-theme"));
    lib.export_theme(&theme, &file).unwrap();

    // Another computer: a fresh library gets the theme and copies of its media.
    let there = tempfile::tempdir().unwrap();
    let mut other = Library::open(&there.path().join("asaf.sqlite")).unwrap();
    let got = other.import_theme(&file).unwrap();
    assert_eq!(got, Theme { background_image: got.background_image.clone(), background_video: got.background_video.clone(), ..theme.clone() });
    for (url, bytes) in [(&got.background_image, b"jpeg bytes".to_vec()), (&got.background_video, vec![7u8; 300_000])] {
        let path = other.media_path(url.as_deref().unwrap()).unwrap();
        assert!(path.starts_with(there.path()), "{path:?}");
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(got.background_image.as_deref().unwrap().starts_with("/media/amanecer-"));
    assert_eq!(other.theme("Sunday").unwrap(), Some(got.clone()));

    // Again: a copy beside it, never a replacement; the media are shared.
    let again = other.import_theme(&file).unwrap();
    assert_eq!((again.name.as_str(), &again.background_video), ("Sunday 2", &got.background_video));
    assert_eq!(std::fs::read_dir(there.path().join("media")).unwrap().count(), 2);
}

#[test]
fn bad_theme_files_are_refused_and_change_nothing() {
    use asaf_library::ThemeFileError;
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(&dir.path().join("asaf.sqlite")).unwrap();
    let before = lib.themes().unwrap();
    let zip_with = |name: &str, entries: &[(&str, &[u8])]| {
        let path = dir.path().join(name);
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (entry, bytes) in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    };
    let envelope = |version: u32, theme: &str| format!(r#"{{"format":"asaf-theme","version":{version},"theme":{theme}}}"#);
    std::fs::write(dir.path().join("plain.asaf-theme"), "not a zip").unwrap();
    let cases: Vec<(PathBuf, ThemeFileError)> = vec![
        (dir.path().join("plain.asaf-theme"), ThemeFileError::NotATheme),
        (zip_with("empty.asaf-theme", &[("readme.txt", b"hi")]), ThemeFileError::NotATheme),
        (zip_with("other.asaf-theme", &[("theme.json", br#"{"format":"something","version":1,"theme":{}}"#)]), ThemeFileError::NotATheme),
        (zip_with("newer.asaf-theme", &[("theme.json", envelope(9, "{}").as_bytes())]), ThemeFileError::Newer(9)),
        (
            zip_with("missing.asaf-theme", &[("theme.json", envelope(1, r#"{"name":"X","background_image":"/media/sky-1.jpg"}"#).as_bytes())]),
            ThemeFileError::MissingMedia("sky-1.jpg".into()),
        ),
        (
            zip_with(
                "sneaky.asaf-theme",
                &[("theme.json", envelope(1, r#"{"name":"X","logo":"/media/notes.txt"}"#).as_bytes()), ("media/notes.txt", b"x")],
            ),
            ThemeFileError::MissingMedia("notes.txt".into()),
        ),
    ];
    for (path, want) in cases {
        assert_eq!(lib.import_theme(&path), Err(want), "{path:?}");
    }
    assert!(matches!(lib.import_theme(&dir.path().join("nowhere.asaf-theme")), Err(ThemeFileError::Io(_))));
    assert_eq!(lib.themes().unwrap(), before);

    // A theme with no media of its own, and an old one missing newer fields.
    let ok = zip_with("ok.asaf-theme", &[("theme.json", envelope(1, r##"{"name":"Plain","color":"#00ff00"}"##).as_bytes())]);
    let got = lib.import_theme(&ok).unwrap();
    assert_eq!((got.name.as_str(), got.color.as_str(), got.font_weight), ("Plain", "#00ff00", 700));
}
