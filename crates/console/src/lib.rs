//! The operator console: song library and Bible lookup (the sources), the
//! service list, preview and live panes, the song editor, the theme editor
//! and settings. It drives the core store and never draws projected output
//! (its previews are approximations).

mod bible;
mod console;
mod editor;
mod i18n;
mod preview;
mod remote;
mod service;
mod settings;
mod slides;
mod theme_editor;
mod transfer;

use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use asaf_library::Library;
use gpui::{App, KeyBinding, actions};

pub use console::Console;
pub use editor::{SongEditor, SongSaved};
pub use i18n::{UiLanguage, set_language};
pub use service::{AUTOSAVE_DELAY, ServiceSession};
pub use settings::{LibraryChanged, SettingsChanged, SettingsWindow};
pub use theme_editor::{ThemeEditor, ThemesChanged};

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
        OpenThemes,
        OpenSettings,
        ImportOpenLp,
    ]
);
actions!(asaf_editor, [SaveSong, SaveTheme]);

/// Makes the bundled fonts available to the console's previews.
fn load_fonts(cx: &mut App) {
    let fonts = asaf_fonts::FONTS.iter().map(|f| Cow::Borrowed(f.bytes)).collect();
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("asaf: couldn't load the bundled fonts for the console: {e:#}");
    }
}

pub fn init(cx: &mut App) {
    load_fonts(cx);
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
        KeyBinding::new("cmd-t", OpenThemes, Some("Console")),
        KeyBinding::new("cmd-,", OpenSettings, Some("Console")),
        KeyBinding::new("cmd-s", SaveSong, Some("SongEditor")),
        KeyBinding::new("cmd-s", SaveTheme, Some("ThemeEditor")),
    ]);
}
