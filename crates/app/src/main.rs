//! Asaf: wires the library, store, display server, projector and console.
//!
//! ```text
//! asaf                        run the app
//! asaf import <file|dir>...   import OpenLyrics files into the library
//! asaf export <dir>           export every song as OpenLyrics
//! asaf import-bible <file>...  import OSIS or Zefania Bibles
//! asaf bibles                  list installed Bibles
//! asaf remove-bible <abbrev>   remove an installed Bible
//! asaf import-openlp <dir>     import songs and Bibles from an OpenLP data folder
//! asaf themes                  list themes (the default is marked)
//! ```
//!
//! Environment:
//! - `ASAF_DATA_DIR`: where `asaf.sqlite`, imported images (`media/`) and
//!   the service autosave live (default: the OS data dir + `Asaf`).
//! - `ASAF_PORT`: server port, over the one in the settings (default 4510).
//! - `ASAF_PROJECTOR_ON_MAIN=1`: allow a fullscreen projector on the main
//!   display (for testing with a single screen).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use asaf_console::{Console, Quit, ServiceSession, SettingsChanged};
use asaf_core::{Song, Store, sample_song};
use asaf_library::Library;
use asaf_library::service::{AUTOSAVE_FILE, restore_autosave};
use asaf_projector::Projector;
use gpui::App;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let library = match open_library() {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("asaf: {e}");
            return ExitCode::FAILURE;
        }
    };

    match args.first().map(String::as_str) {
        None => run_app(library),
        Some("import") if args.len() > 1 => import(library, &args[1..]),
        Some("export") if args.len() == 2 => export(library, &args[1]),
        Some("import-bible") if args.len() > 1 => import_bibles(library, &args[1..]),
        Some("bibles") if args.len() == 1 => list_bibles(library),
        Some("remove-bible") if args.len() == 2 => remove_bible(library, &args[1]),
        Some("import-openlp") if args.len() == 2 => import_openlp(library, &args[1]),
        Some("themes") if args.len() == 1 => list_themes(library),
        _ => {
            eprintln!(
                "usage: asaf [import <file|dir>... | export <dir> | import-bible <file>... | bibles | remove-bible <abbrev> | import-openlp <dir> | themes]"
            );
            ExitCode::FAILURE
        }
    }
}

fn data_dir() -> PathBuf {
    std::env::var_os("ASAF_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("Asaf")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn open_library() -> Result<Library, String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let path = dir.join("asaf.sqlite");
    let mut lib = Library::open(&path).map_err(|e| format!("can't open {}: {e}", path.display()))?;
    // First run: one public-domain song so there's something to try.
    if lib.count().map_err(|e| e.to_string())? == 0 {
        lib.save(&sample_song()).map_err(|e| e.to_string())?;
    }
    Ok(lib)
}

fn import(mut library: Library, paths: &[String]) -> ExitCode {
    match library.import_openlyrics(paths) {
        Ok(report) => {
            println!("{}", report.summary());
            for (path, err) in &report.failed {
                eprintln!("  {}: {err}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("asaf: import failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn export(library: Library, dir: &str) -> ExitCode {
    let result = library
        .all()
        .map_err(|e| e.to_string())
        .and_then(|songs| asaf_openlyrics::write_folder(&songs, dir.as_ref()).map_err(|e| e.to_string()));
    match result {
        Ok(written) => {
            println!("Exported {} songs to {dir}", written.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("asaf: export failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn import_bibles(mut library: Library, paths: &[String]) -> ExitCode {
    let mut ok = true;
    for path in paths {
        match library.import_bible(path.as_ref()) {
            Ok(report) => println!("{}", report.summary()),
            Err(e) => {
                eprintln!("asaf: {e}");
                ok = false;
            }
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn list_bibles(library: Library) -> ExitCode {
    match library.bibles() {
        Ok(bibles) if bibles.is_empty() => println!("No Bibles installed. Add one with: asaf import-bible <file>"),
        Ok(bibles) => {
            for b in bibles {
                let verses = b.id.and_then(|id| library.verse_count(id).ok()).unwrap_or(0);
                println!("{:<10} {} [{}], {verses} verses", b.abbreviation, b.name, b.language);
            }
        }
        Err(e) => {
            eprintln!("asaf: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn remove_bible(mut library: Library, name: &str) -> ExitCode {
    match library.find_bible(name) {
        Ok(Some(id)) => match library.delete_bible(id) {
            Ok(()) => {
                println!("Removed {name}.");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("asaf: {e}");
                ExitCode::FAILURE
            }
        },
        Ok(None) => {
            eprintln!("asaf: no Bible called {name} (see `asaf bibles`)");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("asaf: {e}");
            ExitCode::FAILURE
        }
    }
}

fn import_openlp(mut library: Library, dir: &str) -> ExitCode {
    match library.import_openlp(dir.as_ref()) {
        Ok(report) => {
            println!("{}", report.summary());
            for problem in report.problems() {
                eprintln!("  {problem}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("asaf: {e}");
            ExitCode::FAILURE
        }
    }
}

fn list_themes(library: Library) -> ExitCode {
    let result = library.themes().and_then(|themes| Ok((themes, library.default_theme()?.name)));
    match result {
        Ok((themes, default)) => {
            for t in themes {
                let mark = if t.name == default { " (default)" } else { "" };
                println!("{}{mark}: {} {}, {}–{}% of the height", t.name, t.font_family, t.font_weight, t.size_min_vh, t.size_max_vh);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("asaf: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_app(library: Library) -> ExitCode {
    let settings = library.settings().unwrap_or_default();
    let port = std::env::var("ASAF_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(settings.port);
    let projector_on_main = std::env::var("ASAF_PROJECTOR_ON_MAIN").is_ok_and(|v| v == "1");

    // Nothing is live until the operator picks a song; then it shows in the default theme.
    let store = Store::new(Song::default(), library.default_theme().unwrap_or_default());
    let config = asaf_server::Config {
        port,
        lan: settings.lan,
        media_dir: library.media_dir().map(|d| d.to_path_buf()),
        pin: settings.remote_pin.clone(),
        locale: asaf_i18n::locales::locale_for(settings.language.code()).to_string(),
    };
    let server = match asaf_server::start_with(store.clone(), config) {
        Ok(server) => Arc::new(server),
        Err(err) => {
            let host = if settings.lan { "0.0.0.0" } else { "127.0.0.1" };
            eprintln!("asaf: cannot listen on {host}:{port}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let url = server.display_url();
    println!("asaf: library at {}", data_dir().join("asaf.sqlite").display());
    println!("asaf: display page at {url} (add ?debug for latency overlay)");
    println!(
        "asaf: stage view at {}, OBS lower thirds at {}, remote at {}",
        server.local_url("/stage"),
        server.local_url("/obs"),
        server.local_url("/remote")
    );
    // Phones need the LAN address; without LAN access only this machine can connect.
    let remote_url = server.lan_url("/remote");
    match &remote_url {
        Some(lan) => println!("asaf: remote for phones on this network at {lan}"),
        None if server.is_lan() => eprintln!("asaf: LAN access is on, but this machine has no network address"),
        None => {}
    }
    let library = Arc::new(Mutex::new(library));
    let session = restore_service();

    gpui::application().with_assets(asaf_console::Assets).run(move |cx: &mut App| {
        gpui::init(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([gpui::KeyBinding::new("cmd-q", Quit, None)]);
        // In English until the console applies the language setting.
        asaf_console::set_menus(cx);
        asaf_console::init(cx);

        Projector::init(url.clone(), projector_on_main, settings.display_uuid.clone(), cx);
        match Console::open(store, library, url, session, cx) {
            Ok(console) => {
                // Closing the console ends the app; the projector can't be operated without it.
                cx.observe_release(&console, |_, cx| cx.quit()).detach();
                console.update(cx, |c, cx| c.set_remote_url(remote_url, cx));
                // A new PIN applies at once; LAN access from the next start.
                cx.subscribe(&console, move |_, SettingsChanged(s), _| {
                    server.set_pin(&s.remote_pin);
                    server.set_locale(asaf_i18n::locales::locale_for(s.language.code()));
                })
                .detach();
            }
            Err(err) => {
                eprintln!("asaf: could not open console: {err:#}");
                cx.quit();
            }
        }
    });
    ExitCode::SUCCESS
}

/// The service from the last run (crash or not), and where to keep autosaving it.
fn restore_service() -> ServiceSession {
    let autosave = data_dir().join(AUTOSAVE_FILE);
    let restored = restore_autosave(&autosave);
    if let Some(problem) = &restored.problem {
        eprintln!("asaf: couldn't restore the last service: {problem}");
    } else if !restored.service.items.is_empty() {
        println!("asaf: restored \"{}\" ({} items) from {}", restored.service.display_name(), restored.service.items.len(), autosave.display());
    }
    ServiceSession {
        service: restored.service,
        path: restored.info.path,
        dirty: restored.info.dirty,
        autosave: Some(autosave),
        restore_problem: restored.problem,
    }
}
