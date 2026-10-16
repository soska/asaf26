//! The remote's websocket: commands round-trip through the store, the PIN
//! keeps strangers out, and displays stay in sync with what the remote does.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use asaf_core::{Blank, Command, Outcome, RemoteView, ServiceEntry, Song, Store, Theme, Verse, sample_song};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{connect_async, tungstenite::Message};

type Stream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A websocket client that keeps messages read while waiting for another
/// kind: state pushes and command results arrive in either order.
struct Client {
    ws: Stream,
    unread: VecDeque<Value>,
}

impl From<Stream> for Client {
    fn from(ws: Stream) -> Self {
        Self { ws, unread: VecDeque::new() }
    }
}

fn server(store: &Arc<Store>, pin: &str) -> asaf_server::ServerHandle {
    let config = asaf_server::Config { port: 0, pin: pin.into(), ..Default::default() };
    asaf_server::start_with(store.clone(), config).unwrap()
}

async fn connect(port: u16, path: &str) -> Client {
    connect_async(format!("ws://127.0.0.1:{port}{path}")).await.unwrap().0.into()
}

/// The next text message, as JSON; `None` if the server closed the socket.
async fn recv(c: &mut Client) -> Option<Value> {
    if let Some(msg) = c.unread.pop_front() {
        return Some(msg);
    }
    loop {
        match timeout(Duration::from_secs(6), c.ws.next()).await.expect("a message within 6s") {
            Some(Ok(Message::Text(text))) => return Some(serde_json::from_str(&text).unwrap()),
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
            Some(Ok(_)) => {}
        }
    }
}

/// The next message of `kind`; others stay unread.
async fn recv_kind(c: &mut Client, kind: &str) -> Value {
    if let Some(i) = c.unread.iter().position(|m| m["type"] == kind) {
        return c.unread.remove(i).unwrap();
    }
    let mut skipped = Vec::new();
    let msg = loop {
        let msg = recv(c).await.unwrap_or_else(|| panic!("closed while waiting for {kind}"));
        if msg["type"] == kind {
            break msg;
        }
        skipped.push(msg);
    };
    c.unread.extend(skipped);
    msg
}

async fn send(c: &mut Client, msg: Value) {
    c.ws.send(Message::text(msg.to_string())).await.unwrap();
}

/// Sends `cmd` with `id` and returns its result.
async fn command(ws: &mut Client, id: u64, cmd: Value) -> Value {
    let mut msg = json!({"type": "cmd", "id": id});
    msg.as_object_mut().unwrap().extend(cmd.as_object().unwrap().clone());
    send(ws, msg).await;
    let result = recv_kind(ws, "result").await;
    assert_eq!(result["id"], id);
    result
}

/// A remote past the hello (and the PIN, if any) holding its first state.
async fn remote(port: u16, pin: Option<&str>) -> (Client, Value) {
    let mut ws = connect(port, "/remote/ws").await;
    let hello = recv(&mut ws).await.unwrap();
    assert_eq!(hello, json!({"type": "hello", "pin": pin.is_some()}));
    if let Some(pin) = pin {
        send(&mut ws, json!({"type": "auth", "pin": pin})).await;
        assert_eq!(recv(&mut ws).await.unwrap(), json!({"type": "auth", "ok": true}));
    }
    let state = recv_kind(&mut ws, "state").await;
    (ws, state["state"].clone())
}

async fn display_slide(display: &mut Client) -> (Value, Value) {
    let st = recv_kind(display, "state").await["state"].clone();
    (st["slide"]["label"].clone(), st["blank"].clone())
}

#[tokio::test(flavor = "multi_thread")]
async fn commands_round_trip_and_displays_follow() {
    let store = Store::new(sample_song(), Theme::default());
    let port = server(&store, "").addr.port();
    let mut display = connect(port, "/ws").await;
    assert_eq!(display_slide(&mut display).await.0, "V1");
    let (mut ws, state) = remote(port, None).await;
    assert_eq!((state["title"].clone(), state["current"].clone()), (json!("Blessed Assurance"), json!(0)));
    assert_eq!(state["slides"].as_array().unwrap().len(), 7);

    // (command, the display's slide label and blank mode after it)
    let table = [
        (json!({"cmd": "next"}), "C1", "None"),
        (json!({"cmd": "next"}), "V2", "None"),
        (json!({"cmd": "previous"}), "C1", "None"),
        (json!({"cmd": "goto", "slide": 4}), "V3", "None"),
        (json!({"cmd": "blank", "mode": "Black"}), "V3", "Black"),
        (json!({"cmd": "blank", "mode": "Logo"}), "V3", "Logo"),
        (json!({"cmd": "toggle_blank", "mode": "Logo"}), "V3", "None"),
        (json!({"cmd": "goto", "slide": 0}), "V1", "None"),
    ];
    let mut worst = Duration::ZERO;
    for (id, (cmd, label, blank)) in table.into_iter().enumerate() {
        let t0 = Instant::now();
        let result = command(&mut ws, id as u64, cmd.clone()).await;
        assert_eq!(result, json!({"type": "result", "id": id, "ok": true, "outcome": "done"}), "{cmd}");
        assert_eq!(display_slide(&mut display).await, (json!(label), json!(blank)), "{cmd}");
        worst = worst.max(t0.elapsed());
        // The remote hears about it too, and so does the store everyone shares.
        let st = recv_kind(&mut ws, "state").await["state"].clone();
        assert_eq!(st["slides"][st["current"].as_u64().unwrap() as usize]["label"], label);
        assert_eq!(store.snapshot().blank, serde_json::from_value::<Blank>(json!(blank)).unwrap());
    }
    eprintln!("slowest remote command → display update: {worst:?}");
    assert!(worst < Duration::from_millis(100), "{worst:?}");

    // Changes made elsewhere (the console) reach the remote.
    store.next();
    assert_eq!(recv_kind(&mut ws, "state").await["state"]["current"], 1);

    // Mistakes are answered, not fatal.
    let r = command(&mut ws, 50, json!({"cmd": "goto", "slide": 99})).await;
    assert_eq!((r["ok"].clone(), r["error"].clone()), (json!(false), json!("There is no slide 100 (the live item has 7).")));
    let r = command(&mut ws, 51, json!({"cmd": "launch"})).await;
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().starts_with("Not a command"), "{r}");
    // Service items and verses need the console.
    let r = command(&mut ws, 52, json!({"cmd": "item", "item": 0})).await;
    assert_eq!(r["error"], "The console isn't running.");
    let r = command(&mut ws, 53, json!({"cmd": "next"})).await;
    assert_eq!(r["ok"], true, "still connected after errors");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pin_keeps_strangers_out() {
    let store = Store::new(sample_song(), Theme::default());
    let server = server(&store, "2468");
    let port = server.addr.port();

    // A command before the PIN: refused and disconnected, nothing changes.
    let mut ws = connect(port, "/remote/ws").await;
    assert_eq!(recv(&mut ws).await.unwrap(), json!({"type": "hello", "pin": true}));
    send(&mut ws, json!({"type": "cmd", "id": 1, "cmd": "next"})).await;
    assert_eq!(recv(&mut ws).await.unwrap(), json!({"type": "auth", "ok": false, "error": "Enter the PIN first."}));
    assert!(recv(&mut ws).await.is_none(), "disconnected");
    assert_eq!(store.snapshot().current, 0);

    // A wrong PIN: refused slowly, and no state is ever sent.
    let mut ws = connect(port, "/remote/ws").await;
    recv(&mut ws).await.unwrap();
    let t0 = Instant::now();
    send(&mut ws, json!({"type": "auth", "pin": "1111"})).await;
    assert_eq!(recv(&mut ws).await.unwrap(), json!({"type": "auth", "ok": false, "error": "Wrong PIN."}));
    assert!(t0.elapsed() >= Duration::from_millis(900), "{:?}", t0.elapsed());
    assert!(recv(&mut ws).await.is_none());

    // Guesses on parallel connections are answered one after another.
    let t0 = Instant::now();
    let guesses = (0..3).map(|i| async move {
        let mut ws = connect(port, "/remote/ws").await;
        recv(&mut ws).await.unwrap();
        send(&mut ws, json!({"type": "auth", "pin": format!("000{i}")})).await;
        recv(&mut ws).await.unwrap()
    });
    for reply in futures_util::future::join_all(guesses).await {
        assert_eq!(reply["ok"], false);
    }
    assert!(t0.elapsed() >= Duration::from_millis(2700), "3 guesses took {:?}", t0.elapsed());

    // The right PIN: in, and commands work.
    let (mut ws, _) = remote(port, Some("2468")).await;
    assert_eq!(command(&mut ws, 2, json!({"cmd": "next"})).await["ok"], true);
    assert_eq!(store.snapshot().current, 1);

    // A new PIN (set in Settings) locks out remotes that used the old one.
    server.set_pin("1357");
    send(&mut ws, json!({"type": "cmd", "id": 3, "cmd": "next"})).await;
    assert_eq!(recv_kind(&mut ws, "auth").await, json!({"type": "auth", "ok": false, "error": "The PIN has changed."}));
    while let Some(msg) = recv(&mut ws).await {
        assert_eq!(msg["type"], "state", "only state from before the change: {msg}");
    }
    assert_eq!(store.snapshot().current, 1);
    let (mut ws, _) = remote(port, Some("1357")).await;
    assert_eq!(command(&mut ws, 4, json!({"cmd": "previous"})).await["ok"], true);

    // No PIN any more: straight in.
    server.set_pin("");
    let (mut ws, _) = remote(port, None).await;
    assert_eq!(command(&mut ws, 5, json!({"cmd": "next"})).await["ok"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn other_sites_cannot_open_the_remote_socket() {
    let store = Store::new(sample_song(), Theme::default());
    let port = server(&store, "").addr.port();
    let mut request = format!("ws://127.0.0.1:{port}/remote/ws").into_client_request().unwrap();
    request.headers_mut().insert("Origin", "https://evil.example".parse().unwrap());
    assert!(connect_async(request).await.is_err(), "cross-site upgrade refused");
    let mut request = format!("ws://127.0.0.1:{port}/remote/ws").into_client_request().unwrap();
    request.headers_mut().insert("Origin", format!("http://127.0.0.1:{port}").parse().unwrap());
    let mut ws: Client = connect_async(request).await.unwrap().0.into();
    assert_eq!(recv(&mut ws).await.unwrap()["type"], "hello");
}

/// The operator (the console in the app) runs item and verse commands; here a
/// stand-in that does what the console does: put things live through the store.
#[tokio::test(flavor = "multi_thread")]
async fn service_items_and_verses_go_through_the_operator() {
    let store = Store::new(Song::default(), Theme::default());
    let hymn = Song { id: Some(2), title: "Holy, Holy, Holy".into(), verses: vec![Verse::new("v1", "Holy, holy, holy!")], ..Default::default() };
    let service = [sample_song(), hymn];
    let entries: Vec<ServiceEntry> = service.iter().map(|s| ServiceEntry { title: s.title.clone(), kind: "song" }).collect();
    store.set_remote_view(RemoteView { service: "Sunday".into(), items: entries.clone(), cued: Some(0), bibles: vec!["KJV".into()], bible: Some("KJV".into()), ..Default::default() });
    {
        let store2 = store.clone();
        store.set_operator(move |cmd, reply| match cmd {
            Command::Item { item, slide } => match service.get(item) {
                Some(song) => {
                    store2.load_song(song.clone(), slide);
                    store2.set_remote_view(RemoteView { cued: Some(item + 1).filter(|&i| i < 2).or(Some(item)), live: Some(item), items: entries.clone(), ..Default::default() });
                    reply(Ok(Outcome::Done))
                }
                None => reply(Err(format!("There is no item {}.", item + 1))),
            },
            Command::Verse { reference, live, .. } => {
                let slide = asaf_core::Slide { lines: vec!["Jesus wept.".into()], label: reference.clone(), kind: asaf_core::SlideKind::Scripture, ..Default::default() };
                if live {
                    let passage = asaf_core::Passage { bible_id: 1, ranges: Vec::new() };
                    store2.load_passage(passage, reference.clone(), vec![slide.clone()], 0);
                }
                // Answered from another thread, like the console's.
                std::thread::spawn(move || reply(Ok(Outcome::Passage { title: reference, slides: vec![slide] })));
            }
            _ => unreachable!("the store runs those"),
        });
    }
    let port = server(&store, "").addr.port();
    let mut display = connect(port, "/ws").await;
    let first = recv_kind(&mut display, "state").await["state"].clone();
    assert_eq!((first["slide"].clone(), first["up_next"].clone()), (Value::Null, json!("Blessed Assurance")));

    let (mut ws, state) = remote(port, None).await;
    assert_eq!(state["service"], "Sunday");
    assert_eq!(state["items"][1], json!({"title": "Holy, Holy, Holy", "kind": "song"}));
    assert_eq!((state["cued"].clone(), state["live"].clone(), state["bibles"].clone()), (json!(0), Value::Null, json!(["KJV"])));

    assert_eq!(command(&mut ws, 1, json!({"cmd": "item", "item": 1})).await["ok"], true);
    let st = recv_kind(&mut display, "state").await["state"].clone();
    assert_eq!(st["slide"]["lines"][0], "Holy, holy, holy!");
    let st = recv_kind(&mut ws, "state").await["state"].clone();
    assert_eq!((st["title"].clone(), st["live"].clone()), (json!("Holy, Holy, Holy"), json!(1)));
    let r = command(&mut ws, 2, json!({"cmd": "item", "item": 7})).await;
    assert_eq!((r["ok"].clone(), r["error"].clone()), (json!(false), json!("There is no item 8.")));

    // A lookup answers with the slides and leaves the screen alone…
    let rev = store.snapshot().revision;
    let r = command(&mut ws, 3, json!({"cmd": "verse", "reference": "Jn 11:35"})).await;
    assert_eq!((r["outcome"].clone(), r["title"].clone()), (json!("passage"), json!("Jn 11:35")));
    assert_eq!(r["slides"][0]["lines"][0], "Jesus wept.");
    assert_eq!(store.snapshot().revision, rev);
    // …and with "live" it goes on screen.
    let r = command(&mut ws, 4, json!({"cmd": "verse", "reference": "Jn 11:35", "translation": "KJV", "live": true})).await;
    assert_eq!(r["ok"], true);
    let st = loop {
        let st = recv_kind(&mut display, "state").await["state"].clone();
        if st["slide"]["label"] == "Jn 11:35" {
            break st;
        }
    };
    assert_eq!(st["slide"]["wrap"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_operator_must_answer_in_time() {
    let store = Store::new(sample_song(), Theme::default());
    // An operator that never replies (a stuck console): dropping the reply is an error.
    store.set_operator(|_, reply| drop(reply));
    let port = server(&store, "").addr.port();
    let (mut ws, _) = remote(port, None).await;
    let r = command(&mut ws, 1, json!({"cmd": "item", "item": 0})).await;
    assert_eq!(r["error"], "The console dropped the command.");
}

/// GET `path`: the status line and body.
async fn get(port: u16, path: &str) -> (String, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    (head.lines().next().unwrap().to_string(), body.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_remote_stage_and_obs_pages() {
    let store = Store::new(sample_song(), Theme::default());
    let port = server(&store, "").addr.port();
    for (path, contains) in [
        ("/remote", "/assets/remote.js"),
        ("/assets/remote.css", "footer"),
        ("/assets/remote.js", "/remote/ws"),
        ("/stage", "/assets/stage.js"),
        ("/assets/stage.css", "#clock"),
        ("/assets/stage.js", "up_next"),
        ("/obs", "class=\"obs\""),
        ("/assets/obs.css", "transparent"),
    ] {
        let (status, body) = get(port, path).await;
        assert!(status.ends_with("200 OK"), "{path}: {status}");
        assert!(body.contains(contains), "{path} should contain {contains}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lan_access_is_opt_in() {
    let store = Store::new(sample_song(), Theme::default());
    let local = server(&store, "");
    assert!(local.addr.ip().is_loopback());
    assert!(!local.is_lan());
    assert_eq!(local.lan_url("/remote"), None);
    assert_eq!(local.display_url(), format!("http://127.0.0.1:{}/display", local.addr.port()));

    let lan = asaf_server::start_with(store, asaf_server::Config { lan: true, ..Default::default() }).unwrap();
    assert!(lan.addr.ip().is_unspecified(), "{}", lan.addr);
    // Still reachable here, and the URLs to share use 127.0.0.1 locally and
    // the LAN address (if this machine has one) for phones.
    let (status, _) = get(lan.addr.port(), "/remote").await;
    assert!(status.ends_with("200 OK"));
    assert!(lan.display_url().starts_with("http://127.0.0.1:"));
    match asaf_server::lan_address() {
        Some(ip) => assert_eq!(lan.lan_url("/remote"), Some(format!("http://{ip}:{}/remote", lan.addr.port()))),
        None => assert_eq!(lan.lan_url("/remote"), None),
    }
}

/// A remote's commands apply in the order it sent them, even when the first
/// waits on the console: "Go live" then a quick "next" lands on slide 2 of
/// the new item, not on the old one before it is replaced.
#[tokio::test(flavor = "multi_thread")]
async fn a_remotes_commands_apply_in_order() {
    let store = Store::new(sample_song(), Theme::default());
    let hymn = Song { id: Some(2), title: "Holy, Holy, Holy".into(), verses: vec![Verse::new("v1", "Holy"), Verse::new("v2", "Holy again")], ..Default::default() };
    {
        let store2 = store.clone();
        // A slow console, answering from its own thread.
        store.set_operator(move |_, reply| {
            let (store, hymn) = (store2.clone(), hymn.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(150));
                store.load_song(hymn, 0);
                reply(Ok(Outcome::Done));
            });
        });
    }
    let port = server(&store, "").addr.port();
    let (mut ws, _) = remote(port, None).await;
    send(&mut ws, json!({"type": "cmd", "id": 1, "cmd": "item", "item": 0})).await;
    send(&mut ws, json!({"type": "cmd", "id": 2, "cmd": "next"})).await;
    let first = recv_kind(&mut ws, "result").await;
    let second = recv_kind(&mut ws, "result").await;
    assert_eq!((first["id"].clone(), second["id"].clone()), (json!(1), json!(2)), "results in order");
    let live = store.snapshot();
    assert_eq!((live.title.as_str(), live.current), ("Holy, Holy, Holy", 1));
}

/// A remote that got in with the old PIN hears nothing more after a change,
/// even if it never sends a command.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_pin_cuts_off_idle_remotes() {
    let store = Store::new(sample_song(), Theme::default());
    let server = server(&store, "2468");
    let (mut ws, _) = remote(server.addr.port(), Some("2468")).await;
    server.set_pin("1357");
    store.next();
    assert_eq!(recv(&mut ws).await.unwrap(), json!({"type": "auth", "ok": false, "error": "The PIN has changed."}));
    assert!(recv(&mut ws).await.is_none(), "disconnected");
}

/// A page on a public name that resolves to this machine (DNS rebinding)
/// can't open the remote socket, even though its Origin matches its Host.
#[tokio::test(flavor = "multi_thread")]
async fn rebound_names_cannot_open_the_remote_socket() {
    let store = Store::new(sample_song(), Theme::default());
    let port = server(&store, "").addr.port();
    let mut request = format!("ws://127.0.0.1:{port}/remote/ws").into_client_request().unwrap();
    request.headers_mut().insert("Host", format!("evil.example:{port}").parse().unwrap());
    request.headers_mut().insert("Origin", format!("http://evil.example:{port}").parse().unwrap());
    assert!(connect_async(request).await.is_err());
    let mut request = format!("ws://127.0.0.1:{port}/remote/ws").into_client_request().unwrap();
    request.headers_mut().insert("Host", format!("localhost:{port}").parse().unwrap());
    request.headers_mut().insert("Origin", format!("http://localhost:{port}").parse().unwrap());
    assert!(connect_async(request).await.is_ok());
}
