//! The console as the store's operator: it runs the remote's commands that
//! need the library or the service (a service item, a verse), publishes the
//! service for the remote to show, and redraws when the remote changes what
//! is live.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use asaf_core::{Command, Outcome, RemoteView, Reply, ServiceEntry, parse_reference};
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::{Context, SharedString};

use crate::console::Console;
use crate::i18n::lang;

impl Console {
    /// Becomes the store's operator, and starts listening for changes made
    /// elsewhere (the remote's next/previous/blank go straight to the store).
    pub(crate) fn serve_remote(&mut self, cx: &mut Context<Self>) {
        // Commands arrive from the server's threads; they run here, on the
        // console's, one at a time and in order.
        let (tx, mut commands) = mpsc::unbounded::<(Command, Reply)>();
        self.store.set_operator(move |cmd, reply| {
            if let Err(e) = tx.unbounded_send((cmd, reply)) {
                let (_, reply) = e.into_inner();
                reply(Err("The console has closed.".into()));
            }
        });
        self._tasks.push(cx.spawn(async move |this, cx| {
            while let Some((cmd, reply)) = commands.next().await {
                let result = this.update(cx, |this, cx| this.run_command(cmd, cx));
                reply(result.unwrap_or_else(|_| Err("The console has closed.".into())));
            }
        }));

        // One redraw per burst of changes, however many arrive meanwhile.
        let (tx, mut changes) = mpsc::unbounded::<()>();
        let waiting = Arc::new(AtomicBool::new(false));
        let flag = waiting.clone();
        self.store.subscribe_remote(move |_| {
            if !flag.swap(true, Ordering::SeqCst) {
                let _ = tx.unbounded_send(());
            }
        });
        self._tasks.push(cx.spawn(async move |this, cx| {
            while changes.next().await.is_some() {
                waiting.store(false, Ordering::SeqCst);
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        }));
    }

    /// Runs a command from the remote. Unlike the same thing done from the
    /// keyboard, it leaves the operator's focus and preview alone.
    pub(crate) fn run_command(&mut self, cmd: Command, cx: &mut Context<Self>) -> Result<Outcome, String> {
        // Replies are shown on the phone, in the console's language.
        let l = lang(cx);
        match cmd {
            Command::Item { item, slide } => {
                let len = self.service.service.items.len();
                if item >= len {
                    let (number, count) = ((item + 1).to_string(), len.to_string());
                    return Err(l.f("There is no item {number} (the service has {count}).", &[("number", &number), ("count", &count)]));
                }
                let cued = self.service.cursor.replace(item);
                self.resolve_cue(cx);
                let result = self.put_cued_live(slide, cx);
                if result.is_err() {
                    // Nothing went live: leave the cue where the operator had it.
                    self.service.cursor = cued;
                    self.resolve_cue(cx);
                }
                result.map(|()| Outcome::Done)
            }
            Command::Verse { reference, translation, live } => {
                let bible = match &translation {
                    Some(abbr) => self.bible.bibles.iter().find(|b| b.abbreviation.eq_ignore_ascii_case(abbr)).cloned(),
                    None => self.bible.bible().cloned(),
                };
                let Some(bible) = bible else {
                    return Err(match translation {
                        Some(abbr) => l.f("The {bible} Bible isn't installed.", &[("bible", &abbr)]),
                        None => l.t("Import a Bible first.").into(),
                    });
                };
                let ranges = parse_reference(&reference, &bible.names()).map_err(|e| e.to_string())?;
                let p = self.lay_out_passage(&bible, ranges)?;
                if live {
                    self.store.load_passage(p.passage.clone(), p.title(), p.slides.clone(), 0);
                    self.went_live_ad_hoc(cx);
                }
                Ok(Outcome::Passage { title: p.title(), slides: p.slides })
            }
            // Slide and blank commands never need the console.
            cmd => {
                let (tx, rx) = std::sync::mpsc::channel();
                self.store.command(cmd, move |r| {
                    let _ = tx.send(r);
                });
                rx.try_recv().unwrap_or_else(|_| Err(l.t("No result.").into()))
            }
        }
    }

    /// Tells the store (and so the remote) about the service, the cue and the
    /// translations. Called whenever any of them may have changed.
    pub(crate) fn publish_remote_view(&self) {
        let s = &self.service;
        let kind = |song: bool| if song { "song" } else { "passage" };
        self.store.set_remote_view(RemoteView {
            service: s.service.display_name().to_string(),
            items: s.service.items.iter().map(|i| ServiceEntry { title: i.title(), kind: kind(i.is_song()) }).collect(),
            cued: s.cursor,
            live: s.live_key.and_then(|k| s.service.position(k)),
            bibles: self.bible.bibles.iter().map(|b| b.abbreviation.clone()).collect(),
            bible: self.bible.bible().map(|b| b.abbreviation.clone()),
        });
    }

    /// Where phones reach the remote page (shown in the live pane), when the
    /// server is open to the LAN.
    pub fn set_remote_url(&mut self, url: Option<String>, cx: &mut Context<Self>) {
        self.remote_url = url.map(SharedString::from);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use asaf_core::{Blank, BookNames, Service, ServiceItem, Song, Store, Verse, parse_reference, sample_song};
    use asaf_library::Library;
    use gpui::test::TestWindowExt as _;
    use gpui::{AnyWindowHandle, Entity, TestAppContext};

    use super::*;
    use crate::console::PreviewSource;
    use crate::console::tests::{open_console, with_bibles, with_window};
    use crate::service::ServiceSession;

    /// A console whose service is Blessed Assurance, John 3:16-17 (KJV) and
    /// Cuán Grande Es Él, with the KJV and RV1909 excerpts installed.
    fn setup(cx: &mut TestAppContext) -> (Arc<Store>, Entity<Console>, AnyWindowHandle) {
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let mut lib = Library::open_in_memory().unwrap();
        with_bibles(&mut lib);
        lib.save(&sample_song()).unwrap();
        lib.save(&Song { title: "Cuán Grande Es Él".into(), verses: vec![Verse::new("v1", "Señor mi Dios")], ..Default::default() })
            .unwrap();
        let kjv = lib.bibles().unwrap().into_iter().find(|b| b.abbreviation == "KJV").unwrap();
        let ranges = parse_reference("Jn 3:16-17", &BookNames::new("en", &[])).unwrap();
        let mut service = Service { name: "Sunday".into(), ..Default::default() };
        service.push(ServiceItem::song(1, "Blessed Assurance"));
        service.push(ServiceItem::passage(kjv.id.unwrap(), "KJV", "John 3:16-17", ranges));
        service.push(ServiceItem::song(2, "Cuán Grande Es Él"));
        let (store, console, h) = open_console(cx, Arc::new(Mutex::new(lib)), ServiceSession { service, ..Default::default() });
        with_window(cx, h, |w, cx| w.render_frame(cx));
        (store, console, h)
    }

    /// Sends `cmd` the way the server does and waits for the console's answer.
    fn run(cx: &mut TestAppContext, store: &Store, cmd: Command) -> Result<Outcome, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        store.command(cmd, move |r| tx.send(r).unwrap());
        cx.run_until_parked();
        rx.recv_timeout(Duration::from_secs(1)).expect("one reply")
    }

    #[gpui::test]
    fn the_console_publishes_its_service_for_the_remote(cx: &mut TestAppContext) {
        let (store, console, h) = setup(cx);
        let view = store.remote_state().view;
        assert_eq!(view.service, "Sunday");
        let titles: Vec<_> = view.items.iter().map(|i| (i.title.as_str(), i.kind)).collect();
        assert_eq!(titles, [("Blessed Assurance", "song"), ("John 3:16-17 (KJV)", "passage"), ("Cuán Grande Es Él", "song")]);
        assert_eq!((view.cued, view.live), (Some(0), None));
        assert_eq!((view.bibles, view.bible.as_deref()), (vec!["KJV".to_string(), "RV1909".into()], Some("KJV")));
        assert_eq!(store.display_state().up_next.as_deref(), Some("Blessed Assurance"));

        // ⏎ in the console moves the cue; the remote and the stage view follow.
        with_window(cx, h, |w, cx| w.press("enter", cx));
        let view = store.remote_state().view;
        assert_eq!((view.cued, view.live), (Some(1), Some(0)));
        assert_eq!(store.display_state().up_next.as_deref(), Some("John 3:16-17 (KJV)"));
        // Reordering and removing too.
        with_window(cx, h, |w, cx| w.press("alt-down", cx));
        assert_eq!(store.remote_state().view.items[2].title, "John 3:16-17 (KJV)");
        with_window(cx, h, |w, cx| w.press("cmd-backspace", cx));
        assert_eq!(store.remote_state().view.items.len(), 2);
        // Another translation chosen in the console.
        console.update(cx, |c, cx| c.choose_bible(Some(1), cx));
        assert_eq!(store.remote_state().view.bible.as_deref(), Some("RV1909"));
    }

    #[gpui::test]
    fn remote_commands_run_through_the_console(cx: &mut TestAppContext) {
        let (store, console, h) = setup(cx);
        // The operator is typing in song search while someone uses the remote.
        with_window(cx, h, |w, cx| {
            w.press("cmd-f", cx);
            w.input("cu", cx);
        });

        // A service item: live at the slide asked for, the cue moves on, as with ⏎.
        assert_eq!(run(cx, &store, Command::Item { item: 1, slide: 0 }), Ok(Outcome::Done));
        assert_eq!(store.snapshot().title, "John 3:16-17 (KJV)");
        assert_eq!(run(cx, &store, Command::Item { item: 0, slide: 2 }), Ok(Outcome::Done));
        let live = store.snapshot();
        assert_eq!((live.title.as_str(), live.current), ("Blessed Assurance", 2));
        let view = store.remote_state().view;
        assert_eq!((view.live, view.cued), (Some(0), Some(1)));
        // Slide commands go straight to the store; the console shows them.
        assert_eq!(run(cx, &store, Command::Previous), Ok(Outcome::Done));
        assert_eq!(run(cx, &store, Command::Blank { mode: Blank::Black }), Ok(Outcome::Done));
        assert_eq!((store.snapshot().current, store.snapshot().blank), (1, Blank::Black));
        assert_eq!(run(cx, &store, Command::Blank { mode: Blank::None }), Ok(Outcome::Done));

        // The operator's typing, focus and preview were left alone: "b"
        // still goes into the search box, not to the projector.
        with_window(cx, h, |w, cx| w.input("b", cx));
        assert_eq!(store.snapshot().blank, Blank::None);
        assert_eq!(cx.update(|cx| console.read(cx).preview_source), PreviewSource::Library);

        // Mistakes come back as messages.
        for (cmd, error) in [
            (Command::Item { item: 9, slide: 0 }, "There is no item 10 (the service has 3)."),
            (
                Command::Verse { reference: "Juan 3:16".into(), translation: Some("NVI".into()), live: false },
                "The NVI Bible isn't installed.",
            ),
            (Command::Verse { reference: "Juan 22".into(), translation: Some("RV1909".into()), live: false }, "Juan has 21 chapters, not 22"),
            (Command::Verse { reference: "Juan 3:99".into(), translation: None, live: false }, "John 3:99 isn't in KJV."),
        ] {
            assert_eq!(run(cx, &store, cmd.clone()), Err(error.to_string()), "{cmd:?}");
        }
        // A missing song says why.
        console.update(cx, |c, cx| {
            c.service.service.push(ServiceItem::song(999, "Castillo Fuerte"));
            c.service_changed(cx);
        });
        assert_eq!(run(cx, &store, Command::Item { item: 3, slide: 0 }), Err("“Castillo Fuerte” isn't in the song library.".into()));
        assert_eq!(store.remote_state().view.cued, Some(1), "the cue stays put");

        // A verse lookup shows the slides without touching the screen…
        let rev = store.snapshot().revision;
        let lookup = Command::Verse { reference: "Jn 3:16".into(), translation: Some("rv1909".into()), live: false };
        let Ok(Outcome::Passage { title, slides }) = run(cx, &store, lookup) else { panic!("a passage") };
        assert_eq!(title, "Juan 3:16 (RV1909)");
        assert!(slides[0].lines.join(" ").contains("Porque de tal manera amó Dios al mundo"), "{slides:?}");
        assert_eq!(store.snapshot().revision, rev);
        // …and "live" puts it on screen, ad hoc: the service cue stays.
        let live = Command::Verse { reference: "Jn 3:16".into(), translation: None, live: true };
        assert!(matches!(run(cx, &store, live), Ok(Outcome::Passage { .. })));
        assert_eq!(store.snapshot().title, "John 3:16 (KJV)");
        let view = store.remote_state().view;
        assert_eq!((view.live, view.cued), (None, Some(1)));
    }

    #[gpui::test]
    fn changes_from_other_threads_redraw_the_console(cx: &mut TestAppContext) {
        let (store, console, _) = setup(cx);
        cx.executor().allow_parking(); // woken from another thread, on purpose
        let redraws = Arc::new(Mutex::new(0));
        let count = redraws.clone();
        cx.update(|cx| cx.observe(&console, move |_, _| *count.lock().unwrap() += 1).detach());
        // The remote's "next" reaches the store from the server's thread.
        let s = store.clone();
        std::thread::spawn(move || {
            s.load_song(sample_song(), 0);
            s.next();
            s.next();
        })
        .join()
        .unwrap();
        cx.run_until_parked();
        let n = *redraws.lock().unwrap();
        assert!(n >= 1, "the console redrew");
        assert!(n <= 3, "bursts are coalesced: {n} redraws");
    }

    #[gpui::test]
    fn the_live_pane_shows_where_phones_connect(cx: &mut TestAppContext) {
        let (_, console, h) = setup(cx);
        assert!(with_window(cx, h, |w, _| w.try_find("remote-url")).is_none(), "nothing without LAN access");
        console.update(cx, |c, cx| c.set_remote_url(Some("http://192.168.1.20:4510/remote".into()), cx));
        with_window(cx, h, |w, cx| w.render_frame(cx));
        assert!(with_window(cx, h, |w, _| w.try_find("remote-url")).is_some());
        assert_eq!(cx.update(|cx| console.read(cx).remote_url.clone()).as_deref(), Some("http://192.168.1.20:4510/remote"));
    }

    #[gpui::test]
    fn a_closed_console_answers_instead_of_hanging(cx: &mut TestAppContext) {
        let (store, console, h) = setup(cx);
        drop(console);
        cx.update(|cx| h.update(cx, |_, w, _| w.remove_window()).unwrap());
        cx.run_until_parked();
        let reply = run(cx, &store, Command::Item { item: 0, slide: 0 });
        assert_eq!(reply, Err("The console has closed.".into()));
    }

    /// End to end: a phone's commands over the real server reach this console
    /// (a service item, a verse) and every display follows.
    #[gpui::test]
    fn a_remote_over_the_network_drives_the_console(cx: &mut TestAppContext) {
        use futures::SinkExt as _;
        use serde_json::{Value, json};
        use tokio_tungstenite::tungstenite::Message;

        let (store, _console, _) = setup(cx);
        // The server's threads wake the console's tasks, as in the app.
        cx.executor().allow_parking();
        let config = asaf_server::Config { pin: "2468".into(), ..Default::default() };
        let port = asaf_server::start_with(store.clone(), config).unwrap().addr.port();

        // The phone, on its own thread: PIN, then each command; sends back
        // each result and the display's slide after it.
        let commands = [
            json!({"type": "cmd", "id": 1, "cmd": "item", "item": 2}),
            json!({"type": "cmd", "id": 2, "cmd": "verse", "reference": "Sal 23:1", "translation": "RV1909", "live": true}),
            json!({"type": "cmd", "id": 3, "cmd": "item", "item": 0, "slide": 1}),
        ];
        let (tx, rx) = std::sync::mpsc::channel::<(Value, Value)>();
        let phone = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let url = |path: &str| format!("ws://127.0.0.1:{port}{path}");
                let (mut remote, _) = tokio_tungstenite::connect_async(url("/remote/ws")).await.unwrap();
                let (mut display, _) = tokio_tungstenite::connect_async(url("/ws")).await.unwrap();
                async fn next_of(ws: &mut (impl futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin), kind: &str) -> Value {
                    loop {
                        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
                        if let Message::Text(text) = msg {
                            let v: Value = serde_json::from_str(&text).unwrap();
                            if v["type"] == kind {
                                return v;
                            }
                        }
                    }
                }
                next_of(&mut remote, "hello").await;
                remote.send(Message::text(json!({"type": "auth", "pin": "2468"}).to_string())).await.unwrap();
                assert_eq!(next_of(&mut remote, "auth").await["ok"], true);
                let mut slide = next_of(&mut display, "state").await["state"]["slide"].clone();
                for cmd in commands {
                    remote.send(Message::text(cmd.to_string())).await.unwrap();
                    let result = next_of(&mut remote, "result").await;
                    // The display gets the new slide (skipping "up next" updates).
                    loop {
                        let next = next_of(&mut display, "state").await["state"]["slide"].clone();
                        if next != slide || !result["ok"].as_bool().unwrap() {
                            slide = next;
                            break;
                        }
                    }
                    tx.send((result, slide.clone())).unwrap();
                }
            });
        });

        // Meanwhile this thread runs the console, as the app's main thread does.
        let mut got = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while got.len() < 3 && std::time::Instant::now() < deadline {
            cx.run_until_parked();
            got.extend(rx.try_iter());
            std::thread::sleep(Duration::from_millis(5));
        }
        phone.join().unwrap();
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!(got[0].0["ok"], true);
        assert_eq!(got[0].1["lines"][0], "Señor mi Dios");
        assert_eq!((got[1].0["outcome"].clone(), got[1].0["title"].clone()), (json!("passage"), json!("Salmos 23:1 (RV1909)")));
        assert!(got[1].1["lines"][0].as_str().unwrap().contains("JEHOVÁ es mi pastor"), "{}", got[1].1);
        assert_eq!(got[2].1["label"], "C1");
        let view = store.remote_state().view;
        assert_eq!((view.live, view.cued), (Some(0), Some(1)));
    }
}
