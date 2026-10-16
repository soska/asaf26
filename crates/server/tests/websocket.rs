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
    let server = asaf_server::start(store.clone(), 0, None).unwrap();
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

/// GET `path`: the status line, the headers (lowercased) and the body.
async fn get(port: u16, path: &str) -> (String, String, Vec<u8>) {
    get_with(port, path, "").await
}

/// `extra` is more request header lines, each ending in CRLF.
async fn get_with(port: u16, path: &str, extra: &str) -> (String, String, Vec<u8>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{extra}\r\n").as_bytes())
        .await
        .unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let (status, headers) = head.split_once("\r\n").unwrap_or((&head, ""));
    (status.to_string(), headers.to_lowercase(), buf[split + 4..].to_vec())
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_display_page_and_assets() {
    let store = Store::new(sample_song(), Theme::default());
    let port = asaf_server::start(store, 0, None).unwrap().addr.port();
    for path in [
        "/display",
        "/assets/display.css",
        "/assets/display.js",
        "/assets/background.svg",
        "/assets/logo.svg",
        "/assets/logo-dark.svg",
        "/assets/fonts.css",
    ] {
        let (status, _, _) = get(port, path).await;
        assert!(status.starts_with("HTTP/1.1 200"), "{path}: {status}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_bundled_fonts_with_their_licenses() {
    let store = Store::new(sample_song(), Theme::default());
    let port = asaf_server::start(store, 0, None).unwrap().addr.port();
    let (_, _, css) = get(port, "/assets/fonts.css").await;
    let css = String::from_utf8(css).unwrap();
    for f in asaf_fonts::FONTS {
        assert!(css.contains(&format!("font-family: \"{}\"; src: url(\"/fonts/{}\")", f.family, f.file)), "{css}");
        let (status, headers, body) = get(port, &format!("/fonts/{}", f.file)).await;
        assert!(status.ends_with("200 OK"), "{}: {status}", f.file);
        assert!(headers.contains("content-type: font/ttf"));
        assert_eq!(body, f.bytes, "{} served byte for byte", f.file);
        let (status, _, body) = get(port, &format!("/fonts/{}", f.license_file())).await;
        assert!(status.ends_with("200 OK"));
        assert!(String::from_utf8(body).unwrap().contains("SIL Open Font License"));
    }
    assert!(get(port, "/fonts/Arial.ttf").await.0.contains("404"));
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_imported_images_and_videos_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media");
    std::fs::create_dir(&media).unwrap();
    std::fs::write(media.join("sky-0011aabb.jpg"), b"jpeg bytes").unwrap();
    std::fs::write(media.join("logo-1.svg"), b"<svg/>").unwrap();
    std::fs::write(media.join("notes.txt"), b"private").unwrap();
    std::fs::write(dir.path().join("asaf.sqlite"), b"secret").unwrap();
    let store = Store::new(sample_song(), Theme::default());
    let port = asaf_server::start(store, 0, Some(media.clone())).unwrap().addr.port();

    let (status, headers, body) = get(port, "/media/sky-0011aabb.jpg").await;
    assert!(status.ends_with("200 OK"), "{status}");
    assert!(headers.contains("content-type: image/jpeg"));
    assert_eq!(body, b"jpeg bytes");
    assert!(get(port, "/media/logo-1.svg").await.1.contains("image/svg+xml"));

    // Videos, in byte ranges as WebKit asks for them.
    let video: Vec<u8> = (0..=255u8).collect();
    std::fs::write(media.join("loop-1.mp4"), &video).unwrap();
    let (status, headers, body) = get(port, "/media/loop-1.mp4").await;
    assert!(status.ends_with("200 OK") && headers.contains("content-type: video/mp4"), "{status}");
    assert!(headers.contains("accept-ranges: bytes") && body == video);
    let (status, headers, body) = get_with(port, "/media/loop-1.mp4", "Range: bytes=10-19\r\n").await;
    assert!(status.ends_with("206 Partial Content"), "{status}");
    assert!(headers.contains("content-range: bytes 10-19/256"), "{headers}");
    assert_eq!(body, &video[10..20]);
    let (_, headers, body) = get_with(port, "/media/loop-1.mp4", "Range: bytes=250-\r\n").await;
    assert!(headers.contains("content-range: bytes 250-255/256") && body == video[250..]);
    let (status, headers, _) = get_with(port, "/media/loop-1.mp4", "Range: bytes=300-\r\n").await;
    assert!(status.ends_with("416 Range Not Satisfiable") && headers.contains("content-range: bytes */256"), "{status}");
    for path in [
        "/media/notes.txt",
        "/media/missing.png",
        "/media/..%2Fasaf.sqlite",
        "/media/%2E%2E%2Fasaf.sqlite",
        "/media/../asaf.sqlite",
        "/media/.hidden.png",
    ] {
        let (status, _, body) = get(port, path).await;
        assert!(!status.ends_with("200 OK"), "{path}: {status}");
        assert_ne!(body, b"secret");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn theme_changes_reach_displays_as_css_variables() {
    let store = Store::new(sample_song(), Theme::default());
    let port = asaf_server::start(store.clone(), 0, None).unwrap().addr.port();
    let mut ws = connect(port).await;
    let st = next_state(&mut ws).await;
    assert_eq!(st["theme"]["name"], "Default");
    assert!(st["css_vars"].as_array().unwrap().contains(&serde_json::json!(["--font-family", "\"Source Sans 3\", system-ui, sans-serif"])));

    let scripture = Theme {
        name: "Scripture".into(),
        font_family: "Lora".into(),
        align: asaf_core::Align::Left,
        transition: asaf_core::TransitionKind::Cut,
        background_image: Some("/media/sky-0011aabb.jpg".into()),
        ..Theme::default()
    };
    store.load_song_themed(sample_song(), 2, Some(scripture));
    let st = next_state(&mut ws).await;
    assert_eq!(st["slide"]["label"], "V2", "the slide and its theme arrive together");
    let vars: std::collections::HashMap<String, String> = st["css_vars"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_str().unwrap().to_string()))
        .collect();
    assert_eq!(vars["--font-family"], "\"Lora\", system-ui, sans-serif");
    assert_eq!(vars["--align"], "left");
    assert_eq!(vars["--fade"], "0ms");
    assert_eq!(vars["--bg-image"], "url(\"/media/sky-0011aabb.jpg\")");
    assert_eq!(st["transition"]["kind"], "cut");

    store.set_default_theme(Theme { color: "#ff0000".into(), ..Theme::default() });
    store.load_song(sample_song(), 0);
    let st = next_state(&mut ws).await;
    assert_eq!(st["theme"]["name"], "Default");
    assert_eq!(st["theme"]["color"], "#ff0000");
}

#[tokio::test(flavor = "multi_thread")]
async fn passages_reach_the_display_as_wrapping_slides() {
    let store = Store::new(sample_song(), Theme::default());
    let port = asaf_server::start(store.clone(), 0, None).unwrap().addr.port();
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

#[tokio::test(flavor = "multi_thread")]
async fn pages_get_the_language_and_hear_when_it_changes() {
    let store = Store::new(sample_song(), Theme::default());
    let config = asaf_server::Config { locale: "es-MX".into(), ..Default::default() };
    let server = asaf_server::start_with(store, config).unwrap();
    let port = server.addr.port();

    let (status, _, js) = get(port, "/assets/i18n.js").await;
    assert!(status.contains("200"), "{status}");
    assert!(String::from_utf8_lossy(&js).contains("cyrb53"));

    let (status, headers, body) = get(port, "/locale").await;
    assert!(status.contains("200"), "{status}");
    assert!(headers.contains("application/json"), "{headers}");
    let locale: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(locale["locale"], "es-MX");
    assert_eq!(locale["messages"][asaf_i18n::id::text_id("Connect", None)], "Conectar");

    // Connected pages hear about a change, and nothing when it's the same.
    let mut ws = connect(port).await;
    next_state(&mut ws).await;
    server.set_locale("es-MX");
    server.set_locale("en");
    let msg = timeout(Duration::from_secs(2), ws.next()).await.expect("locale within 2s").unwrap().unwrap();
    let msg: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert_eq!(msg, serde_json::json!({ "type": "locale", "locale": "en" }));

    let (_, _, body) = get(port, "/locale").await;
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), serde_json::json!({ "locale": "en", "messages": {} }));
}
