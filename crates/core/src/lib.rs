//! Domain types, slide generation and the in-memory app state.
//!
//! This crate deliberately knows nothing about GPUI, axum or wry so it can be
//! unit-tested and shared by every front end.

pub mod bible;
mod lyrics;
pub mod reference;
mod sample;
mod song;
mod state;
mod store;
mod theme;

pub use bible::{Bible, BibleLayout, BibleVerse, BookNames, Passage, ReferencePlacement, VersesPerSlide};
pub use lyrics::{format_lyrics, parse_lyrics};
pub use reference::{RefError, RefRange, VerseRef, format_reference, parse_reference};
pub use sample::sample_song;
pub use song::{SPLIT_MARKER, Slide, Song, Verse};
pub use state::{Blank, DisplayState, Transition};
pub use store::{LiveSnapshot, Store};
pub use theme::Theme;
