//! Local HTTP + websocket server. Serves the display page (and its stage and
//! OBS variants), the remote page, the bundled fonts and imported theme
//! images; pushes the full `DisplayState` to every connected display on
//! connect and on every change, and takes the remote's commands on a
//! separate, PIN-protected websocket (see `remote.rs`).

mod remote;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, RwLock};
use std::thread;

use asaf_core::{DisplayState, RemoteState, Store};
use axum::Router;
use axum::extract::{Path, State};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use serde::Serialize;
use tokio::sync::{Mutex as AsyncMutex, watch};

pub const DEFAULT_PORT: u16 = asaf_core::DEFAULT_PORT;

const INDEX_HTML: &str = include_str!("../../../display/index.html");
const DISPLAY_CSS: &str = include_str!("../../../display/display.css");
const DISPLAY_JS: &str = include_str!("../../../display/display.js");
const BACKGROUND_SVG: &str = include_str!("../../../display/background.svg");
const LOGO_SVG: &str = include_str!("../../../display/logo.svg");
const LOGO_DARK_SVG: &str = include_str!("../../../display/logo-dark.svg");
const OBS_HTML: &str = include_str!("../../../display/obs.html");
const OBS_CSS: &str = include_str!("../../../display/obs.css");
const STAGE_HTML: &str = include_str!("../../../display/stage.html");
const STAGE_CSS: &str = include_str!("../../../display/stage.css");
const STAGE_JS: &str = include_str!("../../../display/stage.js");
const REMOTE_HTML: &str = include_str!("../../../display/remote.html");
const REMOTE_CSS: &str = include_str!("../../../display/remote.css");
const REMOTE_JS: &str = include_str!("../../../display/remote.js");
const I18N_JS: &str = include_str!("../../../display/i18n.js");
static FONTS_CSS: LazyLock<String> = LazyLock::new(asaf_fonts::font_face_css);

/// How to serve.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub port: u16,
    /// Listen on every interface (0.0.0.0), not just 127.0.0.1, so the remote
    /// works from phones on the LAN.
    pub lan: bool,
    /// Imported theme images and videos, served at `/media/<file>`.
    pub media_dir: Option<PathBuf>,
    /// The remote's PIN; empty: none.
    pub pin: String,
    /// The pages' language: a locale from `asaf_i18n::locales` ("en",
    /// "es-MX"); empty: English.
    pub locale: String,
}

/// A running server. It lives on its own thread for the life of the process.
pub struct ServerHandle {
    pub addr: SocketAddr,
    pin: Arc<RwLock<String>>,
    locale: Arc<watch::Sender<String>>,
}

impl ServerHandle {
    /// The display page on this machine.
    pub fn display_url(&self) -> String {
        self.local_url("/display")
    }

    pub fn local_url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.addr.port())
    }

    /// Whether other machines can connect.
    pub fn is_lan(&self) -> bool {
        self.addr.ip().is_unspecified()
    }

    /// `path` at this machine's LAN address, when serving on the LAN and it
    /// has one.
    pub fn lan_url(&self, path: &str) -> Option<String> {
        let ip = lan_address().filter(|_| self.is_lan())?;
        Some(format!("http://{}{path}", SocketAddr::new(ip, self.addr.port())))
    }

    /// Changes the remote's PIN (empty: none). Remotes already connected with
    /// the old PIN are disconnected on their next command.
    pub fn set_pin(&self, pin: &str) {
        *self.pin.write().unwrap() = pin.to_string();
    }

    /// Changes the pages' language (a locale such as "es-MX"); open stage
    /// and remote pages switch at once.
    pub fn set_locale(&self, locale: &str) {
        self.locale.send_if_modified(|current| {
            let changed = current != locale;
            if changed {
                *current = locale.to_string();
            }
            changed
        });
    }
}

/// The address other machines on the LAN reach this one at: the one the OS
/// would send from to a public address (nothing is sent).
pub fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?; // TEST-NET-1, never routed anywhere real
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[derive(Clone)]
struct AppState {
    store: Arc<Store>,
    rx: watch::Receiver<DisplayState>,
    remote_rx: watch::Receiver<RemoteState>,
    pin: Arc<RwLock<String>>,
    /// Held while a wrong PIN is answered (after a delay), so guesses are
    /// slow however many connections try at once.
    pin_failures: Arc<AsyncMutex<()>>,
    /// Imported theme images and videos, served at `/media/<file>`.
    media_dir: Option<Arc<PathBuf>>,
    /// Shared with the handle, so it lives as long as the server does.
    locale: Arc<watch::Sender<String>>,
}

/// Binds 127.0.0.1:`port` synchronously (so a busy port is reported to the
/// caller) and serves on a background thread. Images and videos in `media_dir` are
/// served at `/media/<file>`.
pub fn start(store: Arc<Store>, port: u16, media_dir: Option<PathBuf>) -> std::io::Result<ServerHandle> {
    start_with(store, Config { port, media_dir, ..Config::default() })
}

/// Binds (127.0.0.1, or every interface with `config.lan`) synchronously and
/// serves on a background thread.
pub fn start_with(store: Arc<Store>, config: Config) -> std::io::Result<ServerHandle> {
    let ip = if config.lan { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
    let listener = TcpListener::bind((ip, config.port))?;
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
    let (remote_tx, remote_rx) = watch::channel(store.remote_state());
    store.subscribe_remote(move |state| {
        remote_tx.send_if_modified(|current| {
            let newer = state.revision > current.revision;
            if newer {
                *current = state.clone();
            }
            newer
        });
    });
    let pin = Arc::new(RwLock::new(config.pin));
    let locale = if config.locale.is_empty() { "en".to_string() } else { config.locale };
    let locale = Arc::new(watch::Sender::new(locale));
    let shared_locale = locale.clone();

    let shared_pin = pin.clone();
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
                let state = AppState {
                    store,
                    rx,
                    remote_rx,
                    pin: shared_pin,
                    pin_failures: Arc::new(AsyncMutex::new(())),
                    media_dir: config.media_dir.map(Arc::new),
                    locale: shared_locale,
                };
                if let Err(err) = axum::serve(listener, router(state)).await {
                    eprintln!("asaf-server stopped: {err}");
                }
            })
        })?;

    Ok(ServerHandle { addr, pin, locale })
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/display") }))
        .route("/display", get(|| async { asset("text/html; charset=utf-8", INDEX_HTML) }))
        .route("/assets/display.css", get(|| async { asset("text/css; charset=utf-8", DISPLAY_CSS) }))
        .route("/assets/display.js", get(|| async { asset("text/javascript; charset=utf-8", DISPLAY_JS) }))
        .route("/assets/background.svg", get(|| async { asset("image/svg+xml", BACKGROUND_SVG) }))
        .route("/assets/logo.svg", get(|| async { asset("image/svg+xml", LOGO_SVG) }))
        .route("/assets/logo-dark.svg", get(|| async { asset("image/svg+xml", LOGO_DARK_SVG) }))
        .route("/assets/fonts.css", get(|| async { asset("text/css; charset=utf-8", FONTS_CSS.as_str()) }))
        .route("/obs", get(|| async { asset("text/html; charset=utf-8", OBS_HTML) }))
        .route("/assets/obs.css", get(|| async { asset("text/css; charset=utf-8", OBS_CSS) }))
        .route("/stage", get(|| async { asset("text/html; charset=utf-8", STAGE_HTML) }))
        .route("/assets/stage.css", get(|| async { asset("text/css; charset=utf-8", STAGE_CSS) }))
        .route("/assets/stage.js", get(|| async { asset("text/javascript; charset=utf-8", STAGE_JS) }))
        .route("/remote", get(|| async { asset("text/html; charset=utf-8", REMOTE_HTML) }))
        .route("/assets/remote.css", get(|| async { asset("text/css; charset=utf-8", REMOTE_CSS) }))
        .route("/assets/remote.js", get(|| async { asset("text/javascript; charset=utf-8", REMOTE_JS) }))
        .route("/assets/i18n.js", get(|| async { asset("text/javascript; charset=utf-8", I18N_JS) }))
        .route("/locale", get(locale))
        .route("/fonts/{file}", get(font))
        .route("/media/{file}", get(media))
        .route("/ws", get(ws_handler))
        .route("/remote/ws", get(remote::ws_handler))
        .with_state(state)
}

/// A bundled font, or its license (`/fonts/Lora.ttf`, `/fonts/Lora-OFL.txt`).
async fn font(Path(file): Path<String>) -> Response {
    for f in asaf_fonts::FONTS {
        if f.file == file {
            let headers = [(header::CONTENT_TYPE, "font/ttf"), (header::CACHE_CONTROL, "max-age=86400")];
            return (headers, f.bytes).into_response();
        }
        if f.license_file() == file {
            return asset("text/plain; charset=utf-8", f.license);
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

/// An imported image or video. Names carry a hash of the contents, so they
/// never change. Byte ranges are served too: WebKit plays video only from a
/// server that does.
async fn media(State(state): State<AppState>, Path(file): Path<String>, headers: HeaderMap) -> Response {
    let Some(dir) = state.media_dir else { return StatusCode::NOT_FOUND.into_response() };
    let Some(content_type) = media_type(&file) else { return StatusCode::NOT_FOUND.into_response() };
    let path = dir.join(&file);
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).map(String::from);
    let read = tokio::task::spawn_blocking(move || read_media(&path, range.as_deref())).await;
    let cache = (header::CACHE_CONTROL, "max-age=31536000, immutable");
    let ranges = (header::ACCEPT_RANGES, "bytes");
    match read {
        Ok(Ok(MediaRead::Whole(bytes))) => ([(header::CONTENT_TYPE, content_type), cache, ranges], bytes).into_response(),
        Ok(Ok(MediaRead::Part { bytes, start, total })) => {
            let content_range = format!("bytes {start}-{}/{total}", start + bytes.len() as u64 - 1);
            (
                StatusCode::PARTIAL_CONTENT,
                [(header::CONTENT_TYPE, content_type), cache, ranges],
                [(header::CONTENT_RANGE, content_range)],
                bytes,
            )
                .into_response()
        }
        Ok(Ok(MediaRead::Unsatisfiable(total))) => {
            (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{total}"))]).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

enum MediaRead {
    Whole(Vec<u8>),
    Part { bytes: Vec<u8>, start: u64, total: u64 },
    Unsatisfiable(u64),
}

/// Open-ended ranges (`bytes=100-`) get at most this much, so a video
/// streams in pieces.
const MAX_RANGE_BYTES: u64 = 4 << 20;

fn read_media(path: &std::path::Path, range: Option<&str>) -> std::io::Result<MediaRead> {
    use std::io::{Read, Seek, SeekFrom};
    let Some(range) = range else { return std::fs::read(path).map(MediaRead::Whole) };
    let mut file = std::fs::File::open(path)?;
    let total = file.metadata()?.len();
    let Some((start, end)) = parse_range(range, total) else { return Ok(MediaRead::Unsatisfiable(total)) };
    let end = end.min(start + MAX_RANGE_BYTES - 1);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity((end - start + 1) as usize);
    file.take(end - start + 1).read_to_end(&mut bytes)?;
    Ok(MediaRead::Part { bytes, start, total })
}

/// The inclusive byte range a single-range `Range` header asks of a file
/// of `total` bytes: `bytes=0-99`, `bytes=100-` or `bytes=-100` (the last
/// 100). `None` if it can't be served.
fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    let (from, to) = spec.split_once('-')?;
    let (from, to) = (from.trim(), to.trim());
    let (start, end) = match (from.is_empty(), to.is_empty()) {
        (false, true) => (from.parse().ok()?, total.checked_sub(1)?),
        (false, false) => (from.parse().ok()?, to.parse::<u64>().ok()?.min(total.checked_sub(1)?)),
        (true, false) => {
            let n: u64 = to.parse().ok()?;
            (total.checked_sub(n.min(total))?, total.checked_sub(1)?)
        }
        (true, true) => return None,
    };
    (start <= end && start < total).then_some((start, end))
}

/// The content type of an image or video the media folder may serve, or `None` for
/// any other name (path separators, hidden files, other extensions).
fn media_type(file: &str) -> Option<&'static str> {
    let safe = !file.starts_with('.') && file.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    let ext = file.rsplit_once('.')?.1.to_ascii_lowercase();
    let content_type = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        _ => return None,
    };
    safe.then_some(content_type)
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    (
        [(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache")],
        body,
    )
        .into_response()
}

/// The pages' language and its catalog: `{"locale": "es-MX", "messages":
/// {…}}` (no messages for English). Pages translate with `display/i18n.js`.
async fn locale(State(state): State<AppState>) -> Response {
    let locale = state.locale.borrow().clone();
    let messages = asaf_i18n::locales::catalog_json(&locale).unwrap_or("{}");
    let body = format!(r#"{{"locale":{},"messages":{messages}}}"#, serde_json::to_string(&locale).expect("serialize locale"));
    ([(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    let (rx, locale) = (state.rx, state.locale.subscribe());
    ws.on_upgrade(move |socket| push_state(socket, rx, locale))
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ServerMsg<'a> {
    State { state: &'a DisplayState },
    /// The pages' language changed: fetch `/locale` again.
    Locale { locale: &'a str },
}

/// Sends every state, and the pages' language when it changes (pages fetch
/// `/locale` themselves when they connect).
async fn push_state(mut socket: WebSocket, mut rx: watch::Receiver<DisplayState>, mut locale: watch::Receiver<String>) {
    rx.mark_changed(); // send the current state immediately
    loop {
        tokio::select! {
            changed = locale.changed() => {
                if changed.is_err() {
                    return;
                }
                let json = serde_json::to_string(&ServerMsg::Locale { locale: &locale.borrow_and_update() }).expect("serialize locale");
                if socket.send(Message::text(json)).await.is_err() {
                    return;
                }
            }
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
                // Displays never send commands (the remote has its own
                // endpoint); just notice when they go away.
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_ranges() {
        for (header, want) in [
            ("bytes=0-99", Some((0, 99))),
            ("bytes=100-", Some((100, 999))),
            ("bytes=-100", Some((900, 999))),
            ("bytes=-5000", Some((0, 999))),
            ("bytes=990-5000", Some((990, 999))),
            ("bytes=1000-", None),
            ("bytes=5-2", None),
            ("bytes=-", None),
            ("bytes=0-1,5-6", None),
            ("items=0-1", None),
        ] {
            assert_eq!(parse_range(header, 1000), want, "{header}");
        }
        assert_eq!(parse_range("bytes=0-", 0), None);
    }
}
