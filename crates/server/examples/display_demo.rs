//! Serves the display page with one slide in a given theme, for checking the
//! output in a browser (or headless Chrome) without the GUI.
//!
//! ```text
//! cargo run -p asaf-server --example display_demo -- [theme.json] [media dir] [port]
//! ```
//!
//! `theme.json` is an `asaf_core::Theme` (missing fields take the default).
//! The page then shows "Blessed Assurance" chorus at /display until killed.

use std::time::Duration;

use asaf_core::{Store, Theme, sample_song};

fn main() {
    let mut args = std::env::args().skip(1);
    let theme = match args.next().filter(|a| !a.is_empty() && a != "-") {
        Some(path) => {
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            serde_json::from_str::<Theme>(&text).unwrap_or_else(|e| panic!("{path}: {e}")).sanitized()
        }
        None => Theme::default(),
    };
    let media = args.next().filter(|a| !a.is_empty() && a != "-").map(Into::into);
    let port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let store = Store::new(sample_song(), theme);
    store.next(); // the chorus: four lines
    let server = asaf_server::start(store, port, media).expect("start server");
    println!("{}", server.display_url());
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
