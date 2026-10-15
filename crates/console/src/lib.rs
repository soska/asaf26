//! The operator console: song library and Bible lookup (the sources), the
//! service list, preview and live panes, and the song editor. It drives the
//! core store and never draws projected output.

mod bible;
mod console;
mod editor;
mod service;
mod slides;
mod transfer;

use std::sync::{Arc, Mutex};

use asaf_library::Library;
use gpui::{App, KeyBinding, actions};

pub use console::Console;
pub use editor::{SongEditor, SongSaved};
pub use service::{AUTOSAVE_DELAY, ServiceSession};

pub type SharedLibrary = Arc<Mutex<Library>>;

actions!(
    asaf_console,
    [
        Next,
        Previous,
        BlankBlack,
        BlankBackground,
        BlankLogo,
        Unblank,
        GoLive,
        NextItem,
        PreviousItem,
        MoveItemUp,
        MoveItemDown,
        RemoveItem,
        EditNotes,
        NewService,
        OpenService,
        SaveService,
        SaveServiceAs,
        FocusSearch,
        ShowSongs,
        ShowBible,
        SelectNext,
        SelectPrevious,
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
        KeyBinding::new("l", BlankLogo, live),
        KeyBinding::new("escape", Unblank, live),
        // The service: ⏎ puts the cued item live and cues the next one.
        KeyBinding::new("enter", GoLive, live),
        KeyBinding::new("cmd-down", NextItem, live),
        KeyBinding::new("cmd-up", PreviousItem, live),
        KeyBinding::new("alt-down", MoveItemDown, live),
        KeyBinding::new("alt-up", MoveItemUp, live),
        KeyBinding::new("cmd-backspace", RemoveItem, live),
        KeyBinding::new("cmd-i", EditNotes, Some("Console")),
        KeyBinding::new("cmd-shift-n", NewService, Some("Console")),
        KeyBinding::new("cmd-o", OpenService, Some("Console")),
        KeyBinding::new("cmd-s", SaveService, Some("Console")),
        KeyBinding::new("cmd-shift-s", SaveServiceAs, Some("Console")),
        KeyBinding::new("cmd-f", FocusSearch, Some("Console")),
        KeyBinding::new("cmd-l", FocusSearch, Some("Console")),
        KeyBinding::new("cmd-n", NewSong, Some("Console")),
        KeyBinding::new("cmd-e", EditSong, Some("Console")),
        KeyBinding::new("cmd-1", ShowSongs, Some("Console")),
        KeyBinding::new("cmd-2", ShowBible, Some("Console")),
        // In a search box, arrows move through results and Esc goes back to live control.
        KeyBinding::new("down", SelectNext, Some("SongSearch > Input")),
        KeyBinding::new("up", SelectPrevious, Some("SongSearch > Input")),
        KeyBinding::new("escape", LeaveSearch, Some("SongSearch > Input")),
        KeyBinding::new("down", SelectNext, Some("BibleSearch > Input")),
        KeyBinding::new("up", SelectPrevious, Some("BibleSearch > Input")),
        KeyBinding::new("escape", LeaveSearch, Some("BibleSearch > Input")),
        KeyBinding::new("escape", LeaveSearch, Some("ServiceItem > Input")),
        KeyBinding::new("cmd-s", SaveSong, Some("SongEditor")),
    ]);
}
