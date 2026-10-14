//! The operator console: song library, slide preview, live control and the
//! song editor. It drives the core store and never draws projected output.

mod console;
mod editor;
mod transfer;

use std::sync::{Arc, Mutex};

use asaf_library::Library;
use gpui::{App, KeyBinding, actions};

pub use console::Console;
pub use editor::{SongEditor, SongSaved};

pub type SharedLibrary = Arc<Mutex<Library>>;

actions!(
    asaf_console,
    [
        Next,
        Previous,
        BlankBlack,
        BlankBackground,
        Unblank,
        FocusSearch,
        SelectNextSong,
        SelectPreviousSong,
        LeaveSearch,
        NewSong,
        EditSong,
    ]
);
actions!(asaf_editor, [SaveSong]);

pub fn init(cx: &mut App) {
    // Live control: never while typing in a text field.
    let live = Some("Console && !Input");
    cx.bind_keys([
        KeyBinding::new("right", Next, live),
        KeyBinding::new("down", Next, live),
        KeyBinding::new("space", Next, live),
        KeyBinding::new("pagedown", Next, live),
        KeyBinding::new("left", Previous, live),
        KeyBinding::new("up", Previous, live),
        KeyBinding::new("pageup", Previous, live),
        KeyBinding::new("b", BlankBlack, live),
        KeyBinding::new("shift-b", BlankBackground, live),
        KeyBinding::new("escape", Unblank, live),
        KeyBinding::new("cmd-f", FocusSearch, Some("Console")),
        KeyBinding::new("cmd-l", FocusSearch, Some("Console")),
        KeyBinding::new("cmd-n", NewSong, Some("Console")),
        KeyBinding::new("cmd-e", EditSong, Some("Console")),
        // In the search box, arrows move through results and Esc goes back to live control.
        KeyBinding::new("down", SelectNextSong, Some("SongSearch > Input")),
        KeyBinding::new("up", SelectPreviousSong, Some("SongSearch > Input")),
        KeyBinding::new("escape", LeaveSearch, Some("SongSearch > Input")),
        KeyBinding::new("cmd-s", SaveSong, Some("SongEditor")),
    ]);
}
