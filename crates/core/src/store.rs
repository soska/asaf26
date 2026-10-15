use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Blank, DisplayState, Passage, Slide, Song, Theme, Transition};

type Listener = Box<dyn Fn(&DisplayState) + Send + Sync>;

/// The single source of truth. Front ends mutate it; observers (the server)
/// are told about every change.
pub struct Store {
    live: Mutex<Live>,
    listeners: Mutex<Vec<Listener>>,
}

struct Live {
    item: LiveItem,
    slides: Vec<Slide>,
    current: usize,
    blank: Blank,
    /// Used unless the live item has a theme of its own.
    default_theme: Theme,
    /// The live service item's own theme (its per-item override).
    item_theme: Option<Theme>,
    revision: u64,
    changed_at_ms: u64,
}

/// What is live: a song, or a Bible passage laid out by the console.
enum LiveItem {
    Song(Song),
    Passage { passage: Passage, title: String },
}

/// What the console needs to draw itself.
#[derive(Clone, Debug)]
pub struct LiveSnapshot {
    pub song_id: Option<i64>,
    /// The live passage, if a passage is live.
    pub passage: Option<Passage>,
    pub title: String,
    pub slides: Vec<Slide>,
    pub current: usize,
    pub blank: Blank,
    /// Name of the theme on screen.
    pub theme_name: String,
    pub revision: u64,
}

impl Store {
    pub fn new(song: Song, theme: Theme) -> Arc<Self> {
        let slides = song.slides();
        Arc::new(Self {
            live: Mutex::new(Live {
                item: LiveItem::Song(song),
                slides,
                current: 0,
                blank: Blank::None,
                default_theme: theme,
                item_theme: None,
                revision: 1,
                changed_at_ms: now_ms(),
            }),
            listeners: Mutex::new(Vec::new()),
        })
    }

    /// Calls `f` after every change, from whichever thread made it.
    pub fn subscribe(&self, f: impl Fn(&DisplayState) + Send + Sync + 'static) {
        self.listeners.lock().unwrap().push(Box::new(f));
    }

    pub fn display_state(&self) -> DisplayState {
        self.live.lock().unwrap().display_state()
    }

    pub fn snapshot(&self) -> LiveSnapshot {
        let live = self.live.lock().unwrap();
        let (song_id, passage, title) = match &live.item {
            LiveItem::Song(song) => (song.id, None, song.title.clone()),
            LiveItem::Passage { passage, title } => (None, Some(passage.clone()), title.clone()),
        };
        LiveSnapshot {
            song_id,
            passage,
            title,
            slides: live.slides.clone(),
            current: live.current,
            blank: live.blank,
            theme_name: live.theme().name.clone(),
            revision: live.revision,
        }
    }

    /// Puts `song` live, starting at slide `index` (clamped), in the default theme.
    pub fn load_song(&self, song: Song, index: usize) {
        self.load_song_themed(song, index, None);
    }

    /// Puts `song` live in `theme` (`None`: the default theme). The slide and
    /// the theme change together, in one revision.
    pub fn load_song_themed(&self, song: Song, index: usize, theme: Option<Theme>) {
        self.update(|l| {
            l.slides = song.slides();
            l.item = LiveItem::Song(song);
            l.current = index.min(l.slides.len().saturating_sub(1));
            l.item_theme = theme;
            true
        });
    }

    /// Puts a passage live, already laid out as `slides`, at slide `index`
    /// (clamped), in the default theme. `title` is its reference, for the console.
    pub fn load_passage(&self, passage: Passage, title: String, slides: Vec<Slide>, index: usize) {
        self.load_passage_themed(passage, title, slides, index, None);
    }

    pub fn load_passage_themed(
        &self,
        passage: Passage,
        title: String,
        slides: Vec<Slide>,
        index: usize,
        theme: Option<Theme>,
    ) {
        self.update(|l| {
            l.current = index.min(slides.len().saturating_sub(1));
            l.slides = slides;
            l.item = LiveItem::Passage { passage, title };
            l.item_theme = theme;
            true
        });
    }

    /// The theme on screen: the live item's own, else the default.
    pub fn theme(&self) -> Theme {
        self.live.lock().unwrap().theme().clone()
    }

    pub fn default_theme(&self) -> Theme {
        self.live.lock().unwrap().default_theme.clone()
    }

    /// A new or edited default theme. Displays update only if it's the one
    /// on screen.
    pub fn set_default_theme(&self, theme: Theme) {
        self.update(|l| {
            let shown = l.item_theme.is_none() && l.default_theme != theme;
            l.default_theme = theme;
            shown
        });
    }

    /// Changes the live item's own theme (`None`: back to the default), e.g.
    /// after that theme was edited.
    pub fn set_item_theme(&self, theme: Option<Theme>) {
        self.update(|l| {
            let before = l.theme().clone();
            l.item_theme = theme;
            *l.theme() != before
        });
    }

    /// The live song was edited: show the new text, keeping the position.
    /// Does nothing if `song` isn't the live song.
    pub fn song_edited(&self, song: &Song) {
        self.update(|l| {
            let LiveItem::Song(live_song) = &l.item else { return false };
            if song.id.is_none() || live_song.id != song.id {
                return false;
            }
            l.slides = song.slides();
            l.item = LiveItem::Song(song.clone());
            l.current = l.current.min(l.slides.len().saturating_sub(1));
            true
        });
    }

    pub fn next(&self) {
        self.update(|l| {
            let ok = l.current + 1 < l.slides.len();
            if ok {
                l.current += 1;
            }
            ok
        });
    }

    pub fn previous(&self) {
        self.update(|l| {
            let ok = l.current > 0;
            if ok {
                l.current -= 1;
            }
            ok
        });
    }

    pub fn go_to(&self, index: usize) {
        self.update(|l| {
            let ok = index < l.slides.len() && index != l.current;
            if ok {
                l.current = index;
            }
            ok
        });
    }

    /// Switches to `mode`, or back to showing text if `mode` is already active.
    pub fn toggle_blank(&self, mode: Blank) {
        self.update(|l| {
            l.blank = if l.blank == mode { Blank::None } else { mode };
            true
        });
    }

    pub fn set_blank(&self, mode: Blank) {
        self.update(|l| std::mem::replace(&mut l.blank, mode) != mode);
    }

    /// Applies `change`, which returns whether it changed anything; if so,
    /// bumps the revision and notifies listeners.
    fn update(&self, change: impl FnOnce(&mut Live) -> bool) {
        let state = {
            let mut live = self.live.lock().unwrap();
            if !change(&mut live) {
                return;
            }
            live.revision += 1;
            live.changed_at_ms = now_ms();
            live.display_state()
        };
        for listener in self.listeners.lock().unwrap().iter() {
            listener(&state);
        }
    }
}

impl Live {
    fn theme(&self) -> &Theme {
        self.item_theme.as_ref().unwrap_or(&self.default_theme)
    }

    fn display_state(&self) -> DisplayState {
        let theme = self.theme();
        DisplayState {
            slide: self.slides.get(self.current).cloned(),
            blank: self.blank,
            theme: theme.clone(),
            css_vars: theme.css_vars(),
            transition: Transition::of(theme),
            revision: self.revision,
            changed_at_ms: self.changed_at_ms,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn store() -> Arc<Store> {
        Store::new(crate::sample_song(), Theme::default())
    }

    #[test]
    fn navigation_is_clamped() {
        let s = store();
        s.previous();
        assert_eq!(s.snapshot().current, 0);
        for _ in 0..50 {
            s.next();
        }
        assert_eq!(s.snapshot().current, s.snapshot().slides.len() - 1);
        s.go_to(999);
        assert_eq!(s.snapshot().current, s.snapshot().slides.len() - 1);
        s.go_to(2);
        assert_eq!(s.snapshot().current, 2);
    }

    #[test]
    fn listeners_see_each_change_once_with_increasing_revision() {
        let s = store();
        let last = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicU64::new(0));
        {
            let (last, calls) = (last.clone(), calls.clone());
            s.subscribe(move |st| {
                assert!(st.revision > last.swap(st.revision, Ordering::SeqCst));
                calls.fetch_add(1, Ordering::SeqCst);
            });
        }
        s.next();
        s.previous();
        s.previous(); // no-op at the start: no notification
        s.toggle_blank(Blank::Black);
        s.toggle_blank(Blank::Black);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(s.display_state().blank, Blank::None);
    }

    #[test]
    fn load_and_edit_live_song() {
        let s = store();
        let mut song = crate::sample_song();
        song.id = Some(7);
        s.load_song(song.clone(), 3);
        assert_eq!(s.snapshot().current, 3);
        assert_eq!(s.snapshot().song_id, Some(7));

        // Editing the live song keeps the position but clamps it.
        song.verse_order = vec!["v1".into(), "c1".into()];
        s.song_edited(&song);
        assert_eq!(s.snapshot().slides.len(), 2);
        assert_eq!(s.snapshot().current, 1);

        // Edits to other songs are ignored.
        let rev = s.snapshot().revision;
        let mut other = song.clone();
        other.id = Some(8);
        s.song_edited(&other);
        assert_eq!(s.snapshot().revision, rev);
    }

    #[test]
    fn passages_go_live_like_songs() {
        let s = store();
        let passage = Passage { bible_id: 1, ranges: Vec::new() };
        let slide = |label: &str| Slide { lines: vec!["text".into()], label: label.into(), footer: None, wrap: true };
        s.load_passage(passage.clone(), "Juan 3:16-18".into(), vec![slide("a"), slide("b")], 5);
        let snap = s.snapshot();
        assert_eq!((snap.current, snap.song_id, snap.passage.as_ref()), (1, None, Some(&passage)));
        assert_eq!(snap.title, "Juan 3:16-18");
        s.previous();
        assert_eq!(s.display_state().slide.unwrap().label, "a");

        // A song edit doesn't touch a live passage.
        let rev = s.snapshot().revision;
        let mut song = crate::sample_song();
        song.id = Some(1);
        s.song_edited(&song);
        assert_eq!(s.snapshot().revision, rev);
    }

    #[test]
    fn display_state_serializes_with_current_slide() {
        let s = store();
        s.next();
        let json = serde_json::to_value(s.display_state()).unwrap();
        assert_eq!(json["slide"]["label"], "C1");
        assert_eq!(json["blank"], "None");
        assert!(json["theme"]["size_max_vh"].is_number());
        assert_eq!(json["css_vars"][0], serde_json::json!(["--fade", "300ms"]));
        assert_eq!(json["transition"]["kind"], "fade");
    }

    #[test]
    fn item_themes_override_the_default_and_change_with_the_slide() {
        let s = store();
        let revisions = Arc::new(AtomicU64::new(0));
        {
            let revisions = revisions.clone();
            s.subscribe(move |_| {
                revisions.fetch_add(1, Ordering::SeqCst);
            });
        }
        let scripture = Theme { name: "Scripture".into(), color: "#ffee00".into(), ..Theme::default() };
        s.load_song_themed(crate::sample_song(), 1, Some(scripture.clone()));
        assert_eq!(revisions.load(Ordering::SeqCst), 1, "slide and theme in one revision");
        let st = s.display_state();
        assert_eq!((st.theme.name.as_str(), st.slide.unwrap().label.as_str()), ("Scripture", "C1"));
        assert!(st.css_vars.contains(&("--color", "rgb(255, 238, 0)".into())));
        assert_eq!(s.snapshot().theme_name, "Scripture");

        // Editing the default doesn't touch an item with its own theme…
        let dark = Theme { background_color: "#000000".into(), transition_ms: 0, ..Theme::default() };
        s.set_default_theme(dark.clone());
        assert_eq!(revisions.load(Ordering::SeqCst), 1);
        // …until something in the default theme goes live.
        s.load_song(crate::sample_song(), 0);
        let st = s.display_state();
        assert_eq!((st.theme, st.transition.duration_ms), (dark.clone(), 0));
        s.set_default_theme(Theme { background_color: "#202020".into(), ..dark });
        assert_eq!(revisions.load(Ordering::SeqCst), 3);
        assert_eq!(s.theme().background_color, "#202020");

        // An item theme set on the live item applies straight away; the same one again is no change.
        s.set_item_theme(Some(scripture.clone()));
        s.set_item_theme(Some(scripture));
        assert_eq!(revisions.load(Ordering::SeqCst), 4);
        s.set_item_theme(None);
        assert_eq!(s.theme().name, "Default");
    }
}
