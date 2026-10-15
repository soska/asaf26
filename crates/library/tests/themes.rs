//! Themes, settings and imported images in the library.

use asaf_core::{Align, Language, Settings, Theme, TransitionKind};
use asaf_library::Library;

#[test]
fn a_new_library_has_the_default_theme() {
    let lib = Library::open_in_memory().unwrap();
    assert_eq!(lib.themes().unwrap(), [Theme::default()]);
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
    assert_eq!(lib.save_theme(&clash, None).unwrap_err(), "There is already a theme called “Scripture”.");
    assert_eq!(lib.save_theme(&clash, Some("Default")).unwrap_err(), "There is already a theme called “Scripture”.");

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
    let names: Vec<_> = lib.themes().unwrap().into_iter().map(|t| t.name).collect();
    assert_eq!(names, ["Loud", "Scripture", "Sunday AM"]);

    // The default can't be deleted; others can.
    assert!(lib.delete_theme("sunday am").unwrap_err().contains("is the default theme"));
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
    assert_eq!(lib.themes().unwrap().len(), 2);
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
