//! Asaf: wires the library, store, display server, projector and console.
//!
//! ```text
//! asaf                        run the app
//! asaf import <file|dir>...   import OpenLyrics files into the library
//! asaf export <dir>           export every song as OpenLyrics
//! asaf import-bible <file>...  import OSIS or Zefania Bibles
//! asaf bibles                  list installed Bibles
//! asaf remove-bible <abbrev>   remove an installed Bible
//! ```
//!
//! Environment:
//! - `ASAF_DATA_DIR`: where `asaf.sqlite` lives (default: the OS data dir + `Asaf`).
//! - `ASAF_PORT`: server port (default 4510).
//! - `ASAF_PROJECTOR_ON_MAIN=1`: allow a fullscreen projector on the main
//!   display (for testing with a single screen).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use asaf_console::Console;
use asaf_core::{Song, Store, Theme, sample_song};
use asaf_library::Library;
use asaf_projector::Projector;
use gpui::component::theme::{Theme as UiTheme, ThemeMode};
use gpui::{App, Menu, MenuItem, actions};

actions!(asaf, [Quit]);

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
        _ => {
            eprintln!(
                "usage: asaf [import <file|dir>... | export <dir> | import-bible <file>... | bibles | remove-bible <abbrev>]"
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

fn run_app(library: Library) -> ExitCode {
    let port = std::env::var("ASAF_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(asaf_server::DEFAULT_PORT);
    let projector_on_main = std::env::var("ASAF_PROJECTOR_ON_MAIN").is_ok_and(|v| v == "1");

    // Nothing is live until the operator picks a song.
    let store = Store::new(Song::default(), Theme::default());
    let server = match asaf_server::start(store.clone(), port) {
        Ok(server) => server,
        Err(err) => {
            eprintln!("asaf: cannot listen on 127.0.0.1:{port}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let url = server.display_url();
    println!("asaf: library at {}", data_dir().join("asaf.sqlite").display());
    println!("asaf: display page at {url} (add ?debug for latency overlay)");
    let library = Arc::new(Mutex::new(library));

    gpui::application().with_assets(gpui::assets::Assets).run(move |cx: &mut App| {
        gpui::init(cx);
        UiTheme::change(ThemeMode::Dark, None, cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([gpui::KeyBinding::new("cmd-q", Quit, None)]);
        cx.set_menus(vec![Menu::new("Asaf").items([MenuItem::action("Quit Asaf", Quit)])]);
        asaf_console::init(cx);

        Projector::init(url.clone(), projector_on_main, cx);
        match Console::open(store, library, url, cx) {
            // Closing the console ends the app; the projector can't be operated without it.
            Ok(console) => cx.observe_release(&console, |_, cx| cx.quit()).detach(),
            Err(err) => {
                eprintln!("asaf: could not open console: {err:#}");
                cx.quit();
            }
        }
    });
    ExitCode::SUCCESS
}
