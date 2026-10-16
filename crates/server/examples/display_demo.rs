//! Serves the display page with one slide in a given theme, for checking the
//! output in a browser (or headless Chrome) without the GUI.
//!
//! ```text
//! cargo run -p asaf-server --example display_demo -- [theme.json] [media dir] [port] [scripture]
//! ```
//!
//! `theme.json` is an `asaf_core::Theme` (missing fields take the default).
//! The page then shows "Blessed Assurance" chorus (or, with `scripture`,
//! John 3:16) at /display until killed.

use std::time::Duration;

use asaf_core::{Passage, Slide, SlideKind, Store, Theme, sample_song};

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
    if args.next().as_deref() == Some("scripture") {
        let john = Slide {
            lines: vec!["¹⁶ For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.".into()],
            label: "John 3:16".into(),
            kind: SlideKind::Scripture,
            reference: Some("John 3:16 (KJV)".into()),
            ..Default::default()
        };
        store.load_passage(Passage { bible_id: 1, ranges: Vec::new() }, "John 3:16 (KJV)".into(), vec![john], 0);
    }
    let server = asaf_server::start(store, port, media).expect("start server");
    println!("{}", server.display_url());
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
