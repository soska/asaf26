//! Serves the remote, stage and OBS pages with a sample service, for
//! checking them in a browser (or headless Chrome) without the GUI.
//!
//! ```text
//! cargo run -p asaf-server --example remote_demo -- [port] [pin] [blank]
//! ```
//!
//! "Blessed Assurance" is live at its chorus, with three more items in the
//! service. A stand-in for the console answers service-item and verse
//! commands (John 3:16 from the KJV, public domain) by changing the store,
//! so the pages can be driven for real. `blank` is `Black`, `Background` or
//! `Logo`.

use std::time::Duration;

use asaf_core::{Blank, Command, Outcome, Passage, RemoteView, ServiceEntry, Slide, Song, Store, Theme, Verse, sample_song};

fn main() {
    let mut args = std::env::args().skip(1);
    let port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let pin = args.next().filter(|p| p != "-").unwrap_or_default();
    let blank = args.next().and_then(|b| serde_json::from_value::<Blank>(b.into()).ok());

    let hymn = Song {
        title: "Holy, Holy, Holy".into(),
        verses: vec![Verse::new("v1", "Holy, holy, holy! Lord God Almighty!\nEarly in the morning our song shall rise to Thee")],
        ..Default::default()
    };
    let john = Slide {
        lines: vec!["¹⁶ For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.".into()],
        label: "John 3:16".into(),
        footer: Some("John 3:16 (KJV)".into()),
        wrap: true,
    };
    let entries = [("Blessed Assurance", "song"), ("John 3:16 (KJV)", "passage"), ("Holy, Holy, Holy", "song"), ("Psalm 23 (KJV)", "passage")]
        .map(|(title, kind)| ServiceEntry { title: title.into(), kind });
    let view = move |live: Option<usize>| RemoteView {
        service: "Sunday 10am".into(),
        items: entries.to_vec(),
        cued: live.map(|i| (i + 1).min(entries.len() - 1)),
        live,
        bibles: vec!["KJV".into(), "RV1909".into()],
        bible: Some("KJV".into()),
    };

    let store = Store::new(sample_song(), Theme::default());
    store.next(); // the chorus
    store.set_remote_view(view(Some(0)));
    if let Some(blank) = blank {
        store.set_blank(blank);
    }
    let s = store.clone();
    store.set_operator(move |cmd, reply| {
        let passage = || Passage { bible_id: 1, ranges: Vec::new() };
        match cmd {
            Command::Item { item: 0, slide } => s.load_song(sample_song(), slide),
            Command::Item { item: 1 | 3, slide } => s.load_passage(passage(), "John 3:16 (KJV)".into(), vec![john.clone()], slide),
            Command::Item { item: 2, slide } => s.load_song(hymn.clone(), slide),
            Command::Item { item, .. } => return reply(Err(format!("There is no item {}.", item + 1))),
            Command::Verse { live, .. } => {
                if live {
                    s.load_passage(passage(), "John 3:16 (KJV)".into(), vec![john.clone()], 0);
                    s.set_remote_view(view(None));
                }
                return reply(Ok(Outcome::Passage { title: "John 3:16 (KJV)".into(), slides: vec![john.clone()] }));
            }
            _ => unreachable!("the store runs those"),
        }
        if let Command::Item { item, .. } = cmd {
            s.set_remote_view(view(Some(item)));
        }
        reply(Ok(Outcome::Done))
    });

    let config = asaf_server::Config { port, pin, ..Default::default() };
    let server = asaf_server::start_with(store, config).expect("start server");
    for path in ["/remote", "/stage", "/obs"] {
        println!("{}", server.local_url(path));
    }
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
