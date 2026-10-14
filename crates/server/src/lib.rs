//! Local HTTP + websocket server. Serves the display page and pushes the full
//! `DisplayState` to every connected display on connect and on every change.

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::thread;

use asaf_core::{DisplayState, Store};
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::header;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use serde::Serialize;
use tokio::sync::watch;

pub const DEFAULT_PORT: u16 = 4510;

const INDEX_HTML: &str = include_str!("../../../display/index.html");
const DISPLAY_CSS: &str = include_str!("../../../display/display.css");
const DISPLAY_JS: &str = include_str!("../../../display/display.js");
const BACKGROUND_SVG: &str = include_str!("../../../display/background.svg");

/// A running server. It lives on its own thread for the life of the process.
pub struct ServerHandle {
    pub addr: SocketAddr,
}

impl ServerHandle {
    pub fn display_url(&self) -> String {
        format!("http://{}/display", self.addr)
    }
}

/// Binds 127.0.0.1:`port` synchronously (so a busy port is reported to the
/// caller) and serves on a background thread.
pub fn start(store: Arc<Store>, port: u16) -> std::io::Result<ServerHandle> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;

    let (tx, rx) = watch::channel(store.display_state());
    store.subscribe(move |state| {
        // Listeners may run concurrently; never let an older revision win.
        tx.send_if_modified(|current| {
            let newer = state.revision > current.revision;
            if newer {
                *current = state.clone();
            }
            newer
        });
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    thread::Builder::new()
        .name("asaf-server".into())
        .spawn(move || {
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("convert std listener to tokio");
                if let Err(err) = axum::serve(listener, router(rx)).await {
                    eprintln!("asaf-server stopped: {err}");
                }
            })
        })?;

    Ok(ServerHandle { addr })
}

fn router(rx: watch::Receiver<DisplayState>) -> Router {
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/display") }))
        .route("/display", get(|| async { asset("text/html; charset=utf-8", INDEX_HTML) }))
        .route("/assets/display.css", get(|| async { asset("text/css; charset=utf-8", DISPLAY_CSS) }))
        .route("/assets/display.js", get(|| async { asset("text/javascript; charset=utf-8", DISPLAY_JS) }))
        .route("/assets/background.svg", get(|| async { asset("image/svg+xml", BACKGROUND_SVG) }))
        .route("/ws", get(ws_handler))
        .with_state(rx)
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    (
        [(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache")],
        body,
    )
        .into_response()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(rx): State<watch::Receiver<DisplayState>>,
) -> Response {
    ws.on_upgrade(move |socket| push_state(socket, rx))
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ServerMsg<'a> {
    State { state: &'a DisplayState },
}

async fn push_state(mut socket: WebSocket, mut rx: watch::Receiver<DisplayState>) {
    rx.mark_changed(); // send the current state immediately
    loop {
        tokio::select! {
            changed = rx.changed() => {
                if changed.is_err() {
                    return;
                }
                let json = {
                    let state = rx.borrow_and_update();
                    serde_json::to_string(&ServerMsg::State { state: &state }).expect("serialize state")
                };
                if socket.send(Message::text(json)).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => match incoming {
                // Displays never send commands in v0; just notice when they go away.
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
        }
    }
}
