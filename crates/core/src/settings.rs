//! App settings: which display the projector uses, the server port, the
//! default theme and the console's language. Stored by the library as
//! key/value rows; see [`Settings::to_pairs`].

use serde::{Deserialize, Serialize};

use crate::theme::DEFAULT_THEME_NAME;

/// The server's port unless the settings or `ASAF_PORT` say otherwise.
pub const DEFAULT_PORT: u16 = 4510;

/// The console's language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    Spanish,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::Spanish];

    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Spanish => "es",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The language's own name, for the picker.
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Spanish => "Español",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The display the operator chose for the projector, by its UUID (stable
    /// across replugging and restarts). `None`: the first non-main display.
    pub display_uuid: Option<String>,
    pub port: u16,
    /// Name of the theme used when an item has none of its own.
    pub default_theme: String,
    pub language: Language,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            display_uuid: None,
            port: DEFAULT_PORT,
            default_theme: DEFAULT_THEME_NAME.into(),
            language: Language::English,
        }
    }
}

impl Settings {
    /// Every setting as a key and its text value.
    pub fn to_pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("display_uuid", self.display_uuid.clone().unwrap_or_default()),
            ("port", self.port.to_string()),
            ("default_theme", self.default_theme.clone()),
            ("language", self.language.code().into()),
        ]
    }

    /// Settings from stored pairs. Unknown keys are ignored and missing or
    /// unreadable values keep their default, so a damaged row can't stop the
    /// app from starting.
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut s = Settings::default();
        for (key, value) in pairs {
            let value = value.trim();
            match key {
                "display_uuid" => s.display_uuid = (!value.is_empty()).then(|| value.to_string()),
                "port" => {
                    if let Ok(p) = value.parse::<u16>()
                        && p != 0
                    {
                        s.port = p;
                    }
                }
                "default_theme" if !value.is_empty() => s.default_theme = value.to_string(),
                "language" => s.language = Language::from_code(value).unwrap_or_default(),
                _ => {}
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_round_trip() {
        let s = Settings {
            display_uuid: Some("6F1A-22".into()),
            port: 5000,
            default_theme: "Scripture".into(),
            language: Language::Spanish,
        };
        let pairs = s.to_pairs();
        assert_eq!(Settings::from_pairs(pairs.iter().map(|(k, v)| (*k, v.as_str()))), s);
        let d = Settings::default();
        assert_eq!(Settings::from_pairs(d.to_pairs().iter().map(|(k, v)| (*k, v.as_str()))), d);
    }

    #[test]
    fn bad_values_keep_defaults() {
        for (key, value) in [
            ("port", "0"),
            ("port", "70000"),
            ("port", "abc"),
            ("language", "fr"),
            ("default_theme", "  "),
            ("display_uuid", ""),
            ("unknown", "x"),
        ] {
            assert_eq!(Settings::from_pairs([(key, value)]), Settings::default(), "{key}={value}");
        }
        assert_eq!(Settings::from_pairs([("language", "ES")]).language, Language::Spanish);
    }
}
