//! The remote's websocket, `/remote/ws`: the only way in for commands.
//!
//! ```text
//! server: {"type":"hello","pin":true}             on connect; pin: whether one is needed
//! client: {"type":"auth","pin":"2468"}            when it is
//! server: {"type":"auth","ok":true}               or {"type":"auth","ok":false,"error":…}, then close
//! server: {"type":"state","state":RemoteState}    once in, and on every change
//! client: {"type":"cmd","id":7,"cmd":"next"}      any asaf_core::Command; id is optional
//! server: {"type":"result","id":7,"ok":true,"outcome":"done"}
//!         {"type":"result","id":8,"ok":true,"outcome":"passage","title":…,"slides":[…]}
//!         {"type":"result","id":9,"ok":false,"error":"…"}
//! ```
//!
//! Every command goes through the core store, so the console, the displays
//! and other remotes see its effect like any key press in the console.

use std::time::Duration;

use asaf_core::{Command, Outcome, RemoteState};
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::AppState;

/// How long a wrong PIN waits for its answer. Guesses are answered one at a
/// time, so a 4-digit PIN takes hours to guess.
pub(crate) const WRONG_PIN_DELAY: Duration = Duration::from_secs(1);
/// How long a command may take (the console looking up a verse).
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientMsg {
    Auth {
        pin: String,
    },
    Cmd {
        #[serde(default)]
        id: Option<u64>,
        #[serde(flatten)]
        cmd: Command,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ServerMsg<'a> {
    Hello {
        pin: bool,
    },
    Auth {
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<&'a str>,
    },
    State {
        state: &'a RemoteState,
    },
    Result {
        id: Option<u64>,
        ok: bool,
        #[serde(flatten, skip_serializing_if = "Option::is_none")]
        outcome: Option<Outcome>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

impl ServerMsg<'_> {
    fn text(&self) -> Message {
        Message::text(serde_json::to_string(self).expect("serialize remote message"))
    }

    fn result(id: Option<u64>, result: Result<Outcome, String>) -> Message {
        match result {
            Ok(outcome) => ServerMsg::Result { id, ok: true, outcome: Some(outcome), error: None }.text(),
            Err(e) => ServerMsg::Result { id, ok: false, outcome: None, error: Some(e) }.text(),
        }
    }
}

pub(crate) async fn ws_handler(ws: WebSocketUpgrade, headers: HeaderMap, State(state): State<AppState>) -> Response {
    // A page on another site must not drive the projector through the
    // operator's browser: browsers always send Origin, so it must be us.
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |socket| session(socket, state))
}

/// No Origin (not a browser), or an Origin whose host is the one asked for
/// and is an address on this machine or the LAN: an IP, a bare name
/// (`localhost`) or a `.local` name. A public name that matches itself is a
/// DNS-rebinding page (evil.example resolved to 127.0.0.1), so it's refused.
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else { return true };
    let origin_host = origin.to_str().ok().and_then(|o| o.split_once("://")).map(|(_, host)| host);
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    origin_host.is_some() && origin_host == host && host.is_some_and(local_host)
}

/// Whether a Host header (`name[:port]`) names this machine or one on the LAN.
fn local_host(host: &str) -> bool {
    let name = match host.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    };
    let name = name.trim_start_matches('[').trim_end_matches(']');
    name.parse::<std::net::IpAddr>().is_ok() || !name.contains('.') || name.to_ascii_lowercase().ends_with(".local")
}

async fn session(mut socket: WebSocket, state: AppState) {
    let mut pin = state.pin.read().unwrap().clone();
    if socket.send(ServerMsg::Hello { pin: !pin.is_empty() }.text()).await.is_err() {
        return;
    }
    if !pin.is_empty() {
        match authenticate(&mut socket, &state).await {
            Some(accepted) => pin = accepted,
            None => return,
        }
    }

    let mut rx = state.remote_rx.clone();
    rx.mark_changed(); // send the current state immediately
    let (results_tx, mut results) = mpsc::unbounded_channel::<Message>();
    // This remote's commands run one at a time, in the order sent: "Go live"
    // then "next" must not have the next applied to the item before.
    let (commands, queue) = mpsc::unbounded_channel::<(Option<u64>, Command)>();
    tokio::spawn(run_in_order(state.clone(), queue, results_tx.clone()));
    // A PIN set or changed since this remote got in: it must ask again.
    let pin_changed = || *state.pin.read().unwrap() != pin;
    loop {
        tokio::select! {
            changed = rx.changed() => {
                if changed.is_err() {
                    return;
                }
                if pin_changed() {
                    let _ = socket.send(ServerMsg::Auth { ok: false, error: Some("The PIN has changed.") }.text()).await;
                    return;
                }
                let msg = ServerMsg::State { state: &rx.borrow_and_update() }.text();
                if socket.send(msg).await.is_err() {
                    return;
                }
            }
            Some(msg) = results.recv() => {
                if socket.send(msg).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if pin_changed() {
                        let _ = socket.send(ServerMsg::Auth { ok: false, error: Some("The PIN has changed.") }.text()).await;
                        return;
                    }
                    match serde_json::from_str::<ClientMsg>(&text) {
                        Ok(ClientMsg::Cmd { id, cmd }) => {
                            let _ = commands.send((id, cmd));
                        }
                        Ok(ClientMsg::Auth { .. }) => {} // already in
                        Err(e) => {
                            let id = serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| v["id"].as_u64());
                            let _ = results_tx.send(ServerMsg::result(id, Err(format!("Not a command: {e}"))));
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Waits for the right PIN. Returns the PIN accepted, or `None` once the
/// remote has been told it's wrong (or went away).
async fn authenticate(socket: &mut WebSocket, state: &AppState) -> Option<String> {
    loop {
        let text = match socket.recv().await? {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(_) => continue,
        };
        let refusal = match serde_json::from_str::<ClientMsg>(&text) {
            Ok(ClientMsg::Auth { pin }) => {
                let expected = state.pin.read().unwrap().clone();
                if pin.trim() == expected || expected.is_empty() {
                    let _ = socket.send(ServerMsg::Auth { ok: true, error: None }.text()).await;
                    return Some(expected);
                }
                "Wrong PIN."
            }
            Ok(ClientMsg::Cmd { .. }) | Err(_) => "Enter the PIN first.",
        };
        let _slow_down = state.pin_failures.lock().await;
        tokio::time::sleep(WRONG_PIN_DELAY).await;
        let _ = socket.send(ServerMsg::Auth { ok: false, error: Some(refusal) }.text()).await;
        let _ = socket.send(Message::Close(None)).await;
        return None;
    }
}

/// Runs one remote's commands through the store, each after the one before
/// has finished, and sends each result. Ends when the remote goes away.
async fn run_in_order(
    state: AppState,
    mut commands: mpsc::UnboundedReceiver<(Option<u64>, Command)>,
    results: mpsc::UnboundedSender<Message>,
) {
    while let Some((id, cmd)) = commands.recv().await {
        let (tx, rx) = oneshot::channel();
        state.store.command(cmd, move |result| {
            let _ = tx.send(result);
        });
        let result = match tokio::time::timeout(COMMAND_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("The console dropped the command.".into()),
            Err(_) => Err("The console didn't answer in time.".into()),
        };
        if results.send(ServerMsg::result(id, result)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn only_same_origin_browsers_may_connect() {
        for (origin, host, ok) in [
            (None, Some("127.0.0.1:4510"), true),
            (Some("http://127.0.0.1:4510"), Some("127.0.0.1:4510"), true),
            (Some("http://192.168.1.20:4510"), Some("192.168.1.20:4510"), true),
            (Some("https://evil.example"), Some("127.0.0.1:4510"), false),
            (Some("http://127.0.0.1:4510.evil.example"), Some("127.0.0.1:4510"), false),
            (Some("http://localhost:4510"), Some("127.0.0.1:4510"), false),
            (Some("null"), Some("127.0.0.1:4510"), false),
            (Some("http://127.0.0.1:4510"), None, false),
            (Some("http://localhost:4510"), Some("localhost:4510"), true),
            (Some("http://asaf-mac.local:4510"), Some("asaf-mac.local:4510"), true),
            (Some("http://[::1]:4510"), Some("[::1]:4510"), true),
            // DNS rebinding: a public name that now resolves to this machine.
            (Some("http://evil.example:4510"), Some("evil.example:4510"), false),
            (Some("http://evil.example"), Some("evil.example"), false),
        ] {
            let mut headers = HeaderMap::new();
            if let Some(o) = origin {
                headers.insert(header::ORIGIN, HeaderValue::from_static(o));
            }
            if let Some(h) = host {
                headers.insert(header::HOST, HeaderValue::from_static(h));
            }
            assert_eq!(same_origin(&headers), ok, "{origin:?} for {host:?}");
        }
    }

    #[test]
    fn client_messages_parse() {
        let msg: ClientMsg = serde_json::from_str(r#"{"type":"cmd","id":7,"cmd":"goto","slide":2}"#).unwrap();
        assert!(matches!(msg, ClientMsg::Cmd { id: Some(7), cmd: Command::Goto { slide: 2 } }));
        let msg: ClientMsg = serde_json::from_str(r#"{"type":"cmd","cmd":"next"}"#).unwrap();
        assert!(matches!(msg, ClientMsg::Cmd { id: None, cmd: Command::Next }));
        let msg: ClientMsg = serde_json::from_str(r#"{"type":"auth","pin":"1234"}"#).unwrap();
        assert!(matches!(msg, ClientMsg::Auth { pin } if pin == "1234"));
        assert!(serde_json::from_str::<ClientMsg>(r#"{"type":"cmd","cmd":"self_destruct"}"#).is_err());
    }

    #[test]
    fn results_are_flat_json() {
        let text = |m: Message| serde_json::from_str::<serde_json::Value>(m.to_text().unwrap()).unwrap();
        let done = text(ServerMsg::result(Some(3), Ok(Outcome::Done)));
        assert_eq!(done, serde_json::json!({"type": "result", "id": 3, "ok": true, "outcome": "done"}));
        let err = text(ServerMsg::result(None, Err("nope".into())));
        assert_eq!(err, serde_json::json!({"type": "result", "id": null, "ok": false, "error": "nope"}));
        let passage = text(ServerMsg::result(Some(4), Ok(Outcome::Passage { title: "Jn 3:16".into(), slides: Vec::new() })));
        assert_eq!((&passage["outcome"], &passage["title"]), (&"passage".into(), &"Jn 3:16".into()));
    }
}
