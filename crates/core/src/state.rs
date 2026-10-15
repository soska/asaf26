use serde::Serialize;

use crate::{Slide, Theme};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Blank {
    #[default]
    None,
    /// Everything black, background included.
    Black,
    /// Hide the text, keep the theme background.
    Background,
    /// Hide the text and show the theme's logo over the background.
    Logo,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Transition {
    pub kind: String,
    pub duration_ms: u32,
}

impl Default for Transition {
    fn default() -> Self {
        Self::of(&Theme::default())
    }
}

impl Transition {
    /// The theme's transition.
    pub fn of(theme: &Theme) -> Self {
        Self { kind: theme.transition.as_str().into(), duration_ms: theme.transition_ms }
    }
}

/// Everything a display needs, flattened. Sent as JSON over the websocket.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DisplayState {
    pub slide: Option<Slide>,
    pub blank: Blank,
    pub theme: Theme,
    /// The theme as CSS custom properties (`[name, value]` pairs) for the page to set.
    pub css_vars: Vec<(&'static str, String)>,
    pub transition: Transition,
    /// Increments on every change.
    pub revision: u64,
    /// Unix time in ms when this revision was produced; lets the display measure latency.
    pub changed_at_ms: u64,
}
