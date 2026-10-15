//! End-to-end: store change -> websocket -> client, for several clients.

use std::time::{Duration, Instant};

use asaf_core::{Blank, Store, Theme, sample_song};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

type Client = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(port: u16) -> Client {
    connect_async(format!("ws://127.0.0.1:{port}/ws")).await.unwrap().0
}

async fn next_state(ws: &mut Client) -> Value {
    loop {
        let msg = timeout(Duration::from_secs(2), ws.next()).await.expect("state within 2s");
        if let Some(Ok(Message::Text(text))) = msg {
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["type"], "state");
            return v["state"].clone();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn clients_get_state_on_connect_and_every_change() {
    let store = Store::new(sample_song(), Theme::default());
    let server = asaf_server::start(store.clone(), 0).unwrap();
    let port = server.addr.port();

    let mut a = connect(port).await;
    let mut b = connect(port).await;
    for ws in [&mut a, &mut b] {
        let st = next_state(ws).await;
        assert_eq!(st["slide"]["label"], "V1");
    }

    let mut worst = Duration::ZERO;
    for expected in ["C1", "V2", "C1"] {
        let t0 = Instant::now();
        store.next();
        for ws in [&mut a, &mut b] {
            assert_eq!(next_state(ws).await["slide"]["label"], expected);
        }
        worst = worst.max(t0.elapsed());
    }
    store.toggle_blank(Blank::Logo);
    let st = next_state(&mut a).await;
    assert_eq!(st["blank"], "Logo");
    assert_eq!(st["theme"]["logo"], "/assets/logo.svg");
    store.toggle_blank(Blank::Black);
    assert_eq!(next_state(&mut a).await["blank"], "Black");
    assert!(worst < Duration::from_millis(100), "slowest change took {worst:?}");
    eprintln!("slowest change delivered to both clients in {worst:?}");

    // A client that reconnects gets the current state straight away.
    drop(a);
    let mut c = connect(port).await;
    let st = next_state(&mut c).await;
    assert_eq!(st["slide"]["label"], "C1");
    assert_eq!(st["blank"], "Black");
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_display_page_and_assets() {
    let store = Store::new(sample_song(), Theme::default());
    let port = asaf_server::start(store, 0).unwrap().addr.port();
    for path in ["/display", "/assets/display.css", "/assets/display.js", "/assets/background.svg", "/assets/logo.svg"] {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut buf = String::new();
        stream.read_to_string(&mut buf).await.unwrap();
        assert!(buf.starts_with("HTTP/1.1 200"), "{path}: {}", buf.lines().next().unwrap_or(""));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn passages_reach_the_display_as_wrapping_slides() {
    let store = Store::new(sample_song(), Theme::default());
    let port = asaf_server::start(store.clone(), 0).unwrap().addr.port();
    let mut ws = connect(port).await;
    assert_eq!(next_state(&mut ws).await["slide"]["wrap"], false);

    let names = asaf_core::BookNames::new("en", &[]);
    let ranges = asaf_core::parse_reference("Jn 11:35", &names).unwrap();
    let verses = [asaf_core::BibleVerse { book: 43, chapter: 11, verse: 35, text: "Jesus wept.".into() }];
    let slides = asaf_core::bible::passage_slides(
        &verses,
        &ranges,
        &names,
        "KJV",
        &asaf_core::BibleLayout::default(),
        &Theme::default(),
    );
    let passage = asaf_core::Passage { bible_id: 1, ranges };
    store.load_passage(passage, "John 11:35 (KJV)".into(), slides, 0);
    let st = next_state(&mut ws).await;
    assert_eq!(st["slide"]["lines"][0], "³⁵ Jesus wept.");
    assert_eq!(st["slide"]["footer"], "John 11:35 (KJV)");
    assert_eq!(st["slide"]["wrap"], true);
}
