//! The service: songs and passages in the order they'll be shown, with notes
//! and an optional theme per item. Saved in our own JSON format
//! (`.asaf-service`); see [`service_to_json`].
//!
//! Items refer to the library rather than copying it: a song by id and title
//! (the title finds it again on another machine), a passage by translation and
//! verse ranges. The console resolves them into slides when they're shown.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::RefRange;

/// File extension for saved services, without the dot.
pub const SERVICE_EXTENSION: &str = "asaf-service";
/// The `format` field of every service file.
pub const SERVICE_FORMAT: &str = "asaf-service";
/// The newest file version this build reads and the one it writes.
pub const SERVICE_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Service {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub items: Vec<ServiceItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceItem {
    /// Tells items apart for this run of the app (selection, what's live)
    /// whatever their position. Never saved; fresh on every load.
    #[serde(skip, default = "next_key")]
    pub key: u64,
    #[serde(flatten)]
    pub content: ItemContent,
    /// Theme to use instead of the default, by name (themes live in the
    /// library). A name the library doesn't have means the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// For the operator only; never projected.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ItemContent {
    Song {
        song_id: i64,
        /// Finds the song when the id doesn't match (another machine's library).
        title: String,
    },
    Passage {
        bible_id: i64,
        /// Abbreviation of the translation ("RV1909"); found by this first,
        /// then by `bible_id`.
        translation: String,
        /// As shown when it was added: "Juan 3:16-18".
        reference: String,
        ranges: Vec<RefRange>,
    },
}

fn next_key() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Equal content; the per-run `key` is ignored.
impl PartialEq for ServiceItem {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content && self.theme == other.theme && self.notes == other.notes
    }
}

impl ServiceItem {
    pub fn new(content: ItemContent) -> Self {
        Self { key: next_key(), content, theme: None, notes: String::new() }
    }

    pub fn song(song_id: i64, title: impl Into<String>) -> Self {
        Self::new(ItemContent::Song { song_id, title: title.into() })
    }

    pub fn passage(bible_id: i64, translation: impl Into<String>, reference: impl Into<String>, ranges: Vec<RefRange>) -> Self {
        Self::new(ItemContent::Passage {
            bible_id,
            translation: translation.into(),
            reference: reference.into(),
            ranges,
        })
    }

    /// What the service list shows: the song title, or "Juan 3:16-18 (RV1909)".
    pub fn title(&self) -> String {
        match &self.content {
            ItemContent::Song { title, .. } => title.clone(),
            ItemContent::Passage { translation, reference, .. } => format!("{reference} ({translation})"),
        }
    }

    pub fn is_song(&self) -> bool {
        matches!(self.content, ItemContent::Song { .. })
    }
}

impl Service {
    /// "Untitled service" until it has a name.
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() { "Untitled service" } else { &self.name }
    }

    /// Appends `item` and returns its index.
    pub fn push(&mut self, item: ServiceItem) -> usize {
        self.items.push(item);
        self.items.len() - 1
    }

    pub fn remove(&mut self, index: usize) -> Option<ServiceItem> {
        (index < self.items.len()).then(|| self.items.remove(index))
    }

    /// Moves the item at `from` so it ends up at `to` (clamped to the end).
    /// Returns its new index, or `None` if `from` is out of range.
    pub fn move_item(&mut self, from: usize, to: usize) -> Option<usize> {
        let item = self.remove(from)?;
        let to = to.min(self.items.len());
        self.items.insert(to, item);
        Some(to)
    }

    pub fn position(&self, key: u64) -> Option<usize> {
        self.items.iter().position(|i| i.key == key)
    }

    /// Items using theme `old` now use `new` (renamed), or the default
    /// (`None`: deleted). Returns whether anything changed.
    pub fn theme_renamed(&mut self, old: &str, new: Option<&str>) -> bool {
        let mut changed = false;
        for item in &mut self.items {
            if item.theme.as_deref().is_some_and(|t| t.trim().eq_ignore_ascii_case(old.trim())) {
                item.theme = new.map(String::from);
                changed = true;
            }
        }
        changed
    }

    /// Updates the cached title of every item showing song `song_id`.
    /// Returns whether anything changed.
    pub fn song_renamed(&mut self, song_id: i64, new_title: &str) -> bool {
        let mut changed = false;
        for item in &mut self.items {
            if let ItemContent::Song { song_id: id, title } = &mut item.content
                && *id == song_id
                && title != new_title
            {
                *title = new_title.to_string();
                changed = true;
            }
        }
        changed
    }
}

// ---- file format ------------------------------------------------------------

/// Extra state kept only in the autosave file, so a restored service knows
/// where it was saved and whether it had unsaved changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutosaveInfo {
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceFileError {
    /// Not JSON, or JSON that isn't a service.
    NotAService,
    /// Written by a newer Asaf.
    TooNew(u32),
    /// The right format and version, but something inside is wrong.
    Invalid(String),
}

impl fmt::Display for ServiceFileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServiceFileError::NotAService => write!(f, "this isn't an Asaf service file"),
            ServiceFileError::TooNew(v) => {
                write!(f, "this service was saved by a newer Asaf (format version {v}); update Asaf to open it")
            }
            ServiceFileError::Invalid(e) => write!(f, "the service file is damaged: {e}"),
        }
    }
}

impl std::error::Error for ServiceFileError {}

#[derive(Serialize)]
struct FileOut<'a> {
    format: &'static str,
    version: u32,
    #[serde(flatten)]
    service: &'a Service,
    #[serde(skip_serializing_if = "Option::is_none")]
    autosave: Option<&'a AutosaveInfo>,
}

#[derive(Deserialize)]
struct FileIn {
    #[serde(flatten)]
    service: Service,
    #[serde(default)]
    autosave: Option<AutosaveInfo>,
}

/// A service as pretty-printed JSON:
///
/// ```json
/// {
///   "format": "asaf-service",
///   "version": 1,
///   "name": "Sunday 10am",
///   "items": [
///     { "type": "song", "song_id": 12, "title": "Blessed Assurance", "notes": "Skip v3" },
///     { "type": "passage", "bible_id": 2, "translation": "RV1909", "reference": "Juan 3:16-18",
///       "ranges": [{ "book": 43, "start": { "chapter": 3, "verse": 16 }, "end": { "chapter": 3, "verse": 18 } }] }
///   ]
/// }
/// ```
///
/// `autosave` is only given for the autosave file. Readers ignore fields
/// they don't know, so later versions can add fields without bumping
/// `version`; a bump means older builds can't read the file.
pub fn service_to_json(service: &Service, autosave: Option<&AutosaveInfo>) -> String {
    let file = FileOut { format: SERVICE_FORMAT, version: SERVICE_VERSION, service, autosave };
    serde_json::to_string_pretty(&file).expect("a service always serializes")
}

/// Reads what [`service_to_json`] wrote, checking format and version first
/// so a file from a newer Asaf gets a clear message.
pub fn service_from_json(text: &str) -> Result<(Service, Option<AutosaveInfo>), ServiceFileError> {
    let value: Value = serde_json::from_str(text).map_err(|_| ServiceFileError::NotAService)?;
    if value.get("format").and_then(Value::as_str) != Some(SERVICE_FORMAT) {
        return Err(ServiceFileError::NotAService);
    }
    let version = value.get("version").and_then(Value::as_u64).ok_or(ServiceFileError::NotAService)?;
    if version > u64::from(SERVICE_VERSION) {
        return Err(ServiceFileError::TooNew(version.min(u64::from(u32::MAX)) as u32));
    }
    let file: FileIn = serde_json::from_value(value).map_err(|e| ServiceFileError::Invalid(e.to_string()))?;
    Ok((file.service, file.autosave))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VerseRef;

    fn sample() -> Service {
        let mut s = Service { name: "Sunday 10am".into(), items: Vec::new() };
        let mut song = ServiceItem::song(12, "Blessed Assurance");
        song.notes = "Skip verse 3; key of D".into();
        s.push(song);
        let range = RefRange {
            book: 43,
            start: VerseRef { chapter: 3, verse: 16 },
            end: VerseRef { chapter: 3, verse: 18 },
        };
        let mut passage = ServiceItem::passage(2, "RV1909", "Juan 3:16-18", vec![range]);
        passage.theme = Some("Scripture".into());
        s.push(passage);
        let whole = RefRange {
            book: 19,
            start: VerseRef { chapter: 23, verse: 1 },
            end: VerseRef { chapter: 23, verse: crate::reference::VERSE_END },
        };
        s.push(ServiceItem::passage(1, "KJV", "Psalms 23", vec![whole]));
        s
    }

    #[test]
    fn round_trips_through_json() {
        let service = sample();
        let json = service_to_json(&service, None);
        let (back, autosave) = service_from_json(&json).unwrap();
        assert_eq!(back, service);
        assert_eq!(autosave, None);
        // Keys are per run: loading gives fresh ones.
        assert!(back.items.iter().zip(&service.items).all(|(a, b)| a.key != b.key));

        let info = AutosaveInfo { path: Some("/tmp/Sunday 10am.asaf-service".into()), dirty: true };
        let (back, autosave) = service_from_json(&service_to_json(&service, Some(&info))).unwrap();
        assert_eq!((back, autosave), (service, Some(info)));
    }

    #[test]
    fn file_layout_is_stable() {
        let v: Value = serde_json::from_str(&service_to_json(&sample(), None)).unwrap();
        assert_eq!(v["format"], "asaf-service");
        assert_eq!(v["version"], 1);
        assert_eq!(v["items"][0]["type"], "song");
        assert_eq!(v["items"][0]["song_id"], 12);
        assert_eq!(v["items"][0]["notes"], "Skip verse 3; key of D");
        assert!(v["items"][0].get("theme").is_none(), "empty fields are left out");
        assert!(v["items"][0].get("key").is_none(), "keys are never saved");
        assert_eq!(v["items"][1]["type"], "passage");
        assert_eq!(v["items"][1]["ranges"][0]["start"]["verse"], 16);
        assert_eq!(v["items"][1]["theme"], "Scripture");
        assert!(v.get("autosave").is_none());
    }

    #[test]
    fn reads_minimal_and_future_compatible_files() {
        let (s, _) = service_from_json(r#"{"format":"asaf-service","version":1}"#).unwrap();
        assert_eq!(s, Service::default());
        // Unknown fields (from a later minor change) are ignored.
        let text = r#"{"format":"asaf-service","version":1,"name":"x","color":"red",
            "items":[{"type":"song","song_id":1,"title":"A","rating":5}]}"#;
        let (s, _) = service_from_json(text).unwrap();
        assert_eq!(s.items, vec![ServiceItem::song(1, "A")]);
    }

    #[test]
    fn rejects_other_files_clearly() {
        let cases: &[(&str, ServiceFileError)] = &[
            ("", ServiceFileError::NotAService),
            ("not json", ServiceFileError::NotAService),
            ("[1, 2]", ServiceFileError::NotAService),
            (r#"{"format":"openlp","version":1}"#, ServiceFileError::NotAService),
            (r#"{"format":"asaf-service"}"#, ServiceFileError::NotAService),
            (r#"{"format":"asaf-service","version":2,"items":"whatever"}"#, ServiceFileError::TooNew(2)),
        ];
        for (text, expected) in cases {
            assert_eq!(service_from_json(text).unwrap_err(), *expected, "{text:?}");
        }
        let err = service_from_json(r#"{"format":"asaf-service","version":1,"items":[{"type":"video"}]}"#).unwrap_err();
        assert!(matches!(err, ServiceFileError::Invalid(_)), "{err:?}");
        assert!(ServiceFileError::TooNew(2).to_string().contains("newer Asaf"));
    }

    #[test]
    fn reorder_remove_and_rename() {
        let mut s = sample();
        let keys: Vec<u64> = s.items.iter().map(|i| i.key).collect();
        assert_eq!(s.move_item(0, 2), Some(2));
        assert_eq!(s.items.iter().map(|i| i.key).collect::<Vec<_>>(), [keys[1], keys[2], keys[0]]);
        assert_eq!(s.move_item(2, 0), Some(0));
        assert_eq!(s.move_item(1, 99), Some(2), "clamped to the end");
        assert_eq!(s.move_item(5, 0), None);
        assert_eq!(s.position(keys[0]), Some(0));

        assert!(s.song_renamed(12, "Blessed Assurance (Crosby)"));
        assert!(!s.song_renamed(12, "Blessed Assurance (Crosby)"));
        assert_eq!(s.items[0].title(), "Blessed Assurance (Crosby)");
        assert_eq!(s.items[1].title(), "Psalms 23 (KJV)");

        assert!(s.remove(0).is_some());
        assert!(s.remove(5).is_none());
        assert_eq!(s.items.len(), 2);
        assert_eq!(Service::default().display_name(), "Untitled service");
    }

    #[test]
    fn theme_renames_follow_into_items() {
        let mut s = sample();
        s.items[0].theme = Some("Scripture".into());
        s.items[1].theme = Some("Other".into());
        assert!(s.theme_renamed("scripture", Some("Bible")));
        assert_eq!(s.items[0].theme.as_deref(), Some("Bible"));
        assert!(!s.theme_renamed("Scripture", Some("x")));
        assert!(s.theme_renamed("Other", None));
        assert_eq!(s.items[1].theme, None);
    }
}
