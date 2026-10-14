//! Domain types, slide generation and the in-memory app state.
//!
//! This crate deliberately knows nothing about GPUI, axum or wry so it can be
//! unit-tested and shared by every front end.

mod lyrics;
mod sample;
mod song;
mod state;
mod store;
mod theme;

pub use sample::sample_song;
pub use lyrics::{format_lyrics, parse_lyrics};
pub use song::{SPLIT_MARKER, Slide, Song, Verse};
pub use state::{Blank, DisplayState, Transition};
pub use store::{LiveSnapshot, Store};
pub use theme::Theme;
