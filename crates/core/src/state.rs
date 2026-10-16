use serde::{Deserialize, Serialize};

use crate::{Slide, Theme};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The slide after it in the live item, for the stage view.
    pub next: Option<Slide>,
    /// The title of the service item after the live one, for the stage view.
    pub up_next: Option<String>,
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

impl DisplayState {
    /// What a display page shows for `slide` (or no slide) in `theme`, apart
    /// from the live output: the theme editor's preview.
    pub fn preview(theme: &Theme, slide: Option<Slide>, blank: Blank) -> Self {
        Self {
            slide,
            next: None,
            up_next: None,
            blank,
            theme: theme.clone(),
            css_vars: theme.css_vars(),
            transition: Transition::of(theme),
            revision: 0,
            changed_at_ms: 0,
        }
    }
}
