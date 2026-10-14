//! Asaf: wires the store, the display server, the projector and the console.
//!
//! Environment:
//! - `ASAF_PORT`: server port (default 4510).
//! - `ASAF_PROJECTOR_ON_MAIN=1`: allow a fullscreen projector on the main
//!   display (for testing with a single screen).

use asaf_console::Console;
use asaf_core::{Store, Theme, sample_song};
use asaf_projector::Projector;
use gpui::{App, Application, Menu, MenuItem, actions};

actions!(asaf, [Quit]);

fn main() {
    let port = std::env::var("ASAF_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(asaf_server::DEFAULT_PORT);
    let projector_on_main = std::env::var("ASAF_PROJECTOR_ON_MAIN").is_ok_and(|v| v == "1");

    let store = Store::new(sample_song(), Theme::default());
    let server = match asaf_server::start(store.clone(), port) {
        Ok(server) => server,
        Err(err) => {
            eprintln!("asaf: cannot listen on 127.0.0.1:{port}: {err}");
            std::process::exit(1);
        }
    };
    let url = server.display_url();
    println!("asaf: display page at {url} (add ?debug for latency overlay)");

    Application::new().run(move |cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([gpui::KeyBinding::new("cmd-q", Quit, None)]);
        cx.set_menus(vec![Menu {
            name: "Asaf".into(),
            items: vec![MenuItem::action("Quit Asaf", Quit)],
        }]);
        asaf_console::bind_keys(cx);

        Projector::init(url.clone(), projector_on_main, cx);
        if let Err(err) = Console::open(store, url, cx) {
            eprintln!("asaf: could not open console: {err:#}");
            cx.quit();
        }

        // Closing the console ends the app; the projector can't be operated without it.
        cx.on_window_closed(|cx| {
            let console_open = cx.windows().iter().any(|w| w.downcast::<Console>().is_some());
            if !console_open {
                cx.quit();
            }
        })
        .detach();
    });
}
