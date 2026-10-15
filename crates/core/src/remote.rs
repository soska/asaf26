//! The command API shared by every front end (the console's keys and the
//! remote page), and what the remote is shown.
//!
//! Slide and blank commands only touch what is live, so the [`Store`]
//! applies them itself. Service items and verses need the library and the
//! console's service, so the store hands those to its *operator* (the
//! console), which puts the result live through the store like any key
//! press would.
//!
//! [`Store`]: crate::Store

use serde::{Deserialize, Serialize};

use crate::{Blank, Slide};

/// Something a front end asks for. In the remote protocol it is the body of
/// a `{"type":"cmd", "cmd": …}` message, e.g. `{"type":"cmd","cmd":"goto","slide":3}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Next,
    Previous,
    /// Jump to a slide of the live item (0-based).
    Goto { slide: usize },
    /// Set the blank mode (`"None"` shows the text again).
    Blank { mode: Blank },
    /// Switch to `mode`, or back to the text if it's already on (the console's B, ⇧B, L).
    ToggleBlank { mode: Blank },
    /// Put service item `item` (0-based) live at `slide`, in its theme; the
    /// cue moves to the item after it, as with ⏎.
    Item {
        item: usize,
        #[serde(default)]
        slide: usize,
    },
    /// Look up a Bible reference in `translation` (an abbreviation; `None`:
    /// the console's current one) with the console's passage layout; with
    /// `live`, also put it live (ad hoc, like ⌘⏎ in the Bible box).
    Verse {
        reference: String,
        #[serde(default)]
        translation: Option<String>,
        #[serde(default)]
        live: bool,
    },
}

impl Command {
    /// Whether the store can apply this alone, or it needs the operator.
    pub fn needs_operator(&self) -> bool {
        matches!(self, Command::Item { .. } | Command::Verse { .. })
    }
}

/// What a command produced, sent back to whoever asked.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Done,
    /// A looked-up passage: "Juan 3:16-18 (RV1909)" and its slides.
    Passage { title: String, slides: Vec<Slide> },
}

/// Called once with a command's result, possibly from another thread.
pub type Reply = Box<dyn FnOnce(Result<Outcome, String>) + Send>;

/// Runs the commands the store can't: set by the console with [`Store::set_operator`].
///
/// [`Store::set_operator`]: crate::Store::set_operator
pub type Operator = Box<dyn Fn(Command, Reply) + Send + Sync>;

/// One row of the service list, as the remote shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ServiceEntry {
    pub title: String,
    /// `"song"` or `"passage"`.
    pub kind: &'static str,
}

/// The console's side of the remote's view, published by the console
/// whenever it changes ([`Store::set_remote_view`]).
///
/// [`Store::set_remote_view`]: crate::Store::set_remote_view
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RemoteView {
    /// The service's name.
    pub service: String,
    pub items: Vec<ServiceEntry>,
    /// The cued item: what ⏎ (or "next item") puts live.
    pub cued: Option<usize>,
    /// The item on screen, when it came from the service.
    pub live: Option<usize>,
    /// Installed translations, by abbreviation.
    pub bibles: Vec<String>,
    /// The console's current translation.
    pub bible: Option<String>,
}

impl RemoteView {
    /// The title of what comes after the live item: the cued item, unless
    /// it is the one already live (the end of the service).
    pub fn up_next(&self) -> Option<String> {
        let cued = self.cued?;
        (self.live != Some(cued)).then(|| self.items.get(cued).map(|i| i.title.clone())).flatten()
    }
}

/// Everything the remote page shows, sent as `{"type":"state","state":…}`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteState {
    /// Increments on every change to what is live or to the console's view.
    pub revision: u64,
    /// The live item's title ("" when nothing is live).
    pub title: String,
    pub slides: Vec<Slide>,
    pub current: usize,
    pub blank: Blank,
    #[serde(flatten)]
    pub view: RemoteView,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The wire format of commands, as the remote page sends them.
    #[test]
    fn commands_parse_from_json() {
        for (text, cmd) in [
            (json!({"cmd": "next"}), Command::Next),
            (json!({"cmd": "previous"}), Command::Previous),
            (json!({"cmd": "goto", "slide": 3}), Command::Goto { slide: 3 }),
            (json!({"cmd": "blank", "mode": "Black"}), Command::Blank { mode: Blank::Black }),
            (json!({"cmd": "blank", "mode": "None"}), Command::Blank { mode: Blank::None }),
            (json!({"cmd": "toggle_blank", "mode": "Logo"}), Command::ToggleBlank { mode: Blank::Logo }),
            (json!({"cmd": "item", "item": 2}), Command::Item { item: 2, slide: 0 }),
            (json!({"cmd": "item", "item": 0, "slide": 4}), Command::Item { item: 0, slide: 4 }),
            (
                json!({"cmd": "verse", "reference": "Jn 3:16"}),
                Command::Verse { reference: "Jn 3:16".into(), translation: None, live: false },
            ),
            (
                json!({"cmd": "verse", "reference": "Sal 23", "translation": "RV1909", "live": true, "id": 9}),
                Command::Verse { reference: "Sal 23".into(), translation: Some("RV1909".into()), live: true },
            ),
        ] {
            assert_eq!(serde_json::from_value::<Command>(text.clone()).unwrap(), cmd, "{text}");
            // And back: the remote and tests can build them the same way.
            assert_eq!(serde_json::from_value::<Command>(serde_json::to_value(&cmd).unwrap()).unwrap(), cmd);
        }
        for bad in [
            json!({"cmd": "explode"}),
            json!({"cmd": "goto"}),
            json!({"cmd": "goto", "slide": -1}),
            json!({"cmd": "blank", "mode": "Purple"}),
            json!({"cmd": "item"}),
            json!({"slide": 1}),
        ] {
            assert!(serde_json::from_value::<Command>(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn up_next_is_the_cued_item_unless_it_is_live() {
        let entry = |t: &str| ServiceEntry { title: t.into(), kind: "song" };
        let mut view = RemoteView { items: vec![entry("A"), entry("B")], cued: Some(1), live: Some(0), ..Default::default() };
        assert_eq!(view.up_next().as_deref(), Some("B"));
        view.live = Some(1);
        assert_eq!(view.up_next(), None, "the end of the service");
        view.live = None;
        view.cued = None;
        assert_eq!(view.up_next(), None);
    }
}
