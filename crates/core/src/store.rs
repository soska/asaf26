use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Blank, Command, DisplayState, Outcome, Passage, RemoteState, RemoteView, Reply, Slide, Song, Theme, Transition};

type Listener = Box<dyn Fn(&DisplayState) + Send + Sync>;
type RemoteListener = Box<dyn Fn(&RemoteState) + Send + Sync>;
type SharedOperator = Arc<dyn Fn(Command, Reply) + Send + Sync>;

/// The single source of truth. Front ends mutate it, directly or with
/// [`Command`]s; observers (the server) are told about every change.
pub struct Store {
    live: Mutex<Live>,
    listeners: Mutex<Vec<Listener>>,
    remote_listeners: Mutex<Vec<RemoteListener>>,
    operator: Mutex<Option<SharedOperator>>,
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
    /// The service and translations as the console last published them.
    view: RemoteView,
    /// Increments on every change to `view`.
    view_revision: u64,
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
                view: RemoteView::default(),
                view_revision: 0,
            }),
            listeners: Mutex::new(Vec::new()),
            remote_listeners: Mutex::new(Vec::new()),
            operator: Mutex::new(None),
        })
    }

    /// Calls `f` after every change, from whichever thread made it.
    pub fn subscribe(&self, f: impl Fn(&DisplayState) + Send + Sync + 'static) {
        self.listeners.lock().unwrap().push(Box::new(f));
    }

    /// Calls `f` after every change to what the remote shows (what is live,
    /// or the console's [`RemoteView`]).
    pub fn subscribe_remote(&self, f: impl Fn(&RemoteState) + Send + Sync + 'static) {
        self.remote_listeners.lock().unwrap().push(Box::new(f));
    }

    pub fn display_state(&self) -> DisplayState {
        self.live.lock().unwrap().display_state()
    }

    pub fn remote_state(&self) -> RemoteState {
        self.live.lock().unwrap().remote_state()
    }

    /// The console publishes its service and translations for the remote.
    /// The stage view's "up next" follows.
    pub fn set_remote_view(&self, view: RemoteView) {
        let (display, remote) = {
            let mut live = self.live.lock().unwrap();
            if live.view == view {
                return;
            }
            let up_next_changed = live.view.up_next() != view.up_next();
            live.view = view;
            live.view_revision += 1;
            if up_next_changed {
                live.revision += 1;
                live.changed_at_ms = now_ms();
            }
            (up_next_changed.then(|| live.display_state()), live.remote_state())
        };
        if let Some(display) = display {
            for listener in self.listeners.lock().unwrap().iter() {
                listener(&display);
            }
        }
        for listener in self.remote_listeners.lock().unwrap().iter() {
            listener(&remote);
        }
    }

    /// Sets who runs the commands the store can't run alone (service items,
    /// verses): the console. Replaces any earlier operator.
    pub fn set_operator(&self, operator: impl Fn(Command, Reply) + Send + Sync + 'static) {
        *self.operator.lock().unwrap() = Some(Arc::new(operator));
    }

    /// Runs `cmd` and calls `reply` with the result once: straight away for
    /// slide and blank commands, later (maybe from another thread) for those
    /// the operator runs. Fails if there is no operator.
    pub fn command(&self, cmd: Command, reply: impl FnOnce(Result<Outcome, String>) + Send + 'static) {
        match cmd {
            Command::Next => self.next(),
            Command::Previous => self.previous(),
            Command::Goto { slide } => {
                let len = self.live.lock().unwrap().slides.len();
                if slide >= len {
                    return reply(Err(format!("There is no slide {} (the live item has {len}).", slide + 1)));
                }
                self.go_to(slide);
            }
            Command::Blank { mode } => self.set_blank(mode),
            Command::ToggleBlank { mode } => self.toggle_blank(mode),
            Command::Item { .. } | Command::Verse { .. } => {
                let operator = self.operator.lock().unwrap().clone();
                return match operator {
                    Some(operator) => operator(cmd, Box::new(reply)),
                    None => reply(Err("The console isn't running.".into())),
                };
            }
        }
        reply(Ok(Outcome::Done));
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
        let (state, remote) = {
            let mut live = self.live.lock().unwrap();
            if !change(&mut live) {
                return;
            }
            live.revision += 1;
            live.changed_at_ms = now_ms();
            (live.display_state(), live.remote_state())
        };
        for listener in self.listeners.lock().unwrap().iter() {
            listener(&state);
        }
        for listener in self.remote_listeners.lock().unwrap().iter() {
            listener(&remote);
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
            next: self.slides.get(self.current + 1).cloned(),
            up_next: self.view.up_next(),
            blank: self.blank,
            theme: theme.clone(),
            css_vars: theme.css_vars(),
            transition: Transition::of(theme),
            revision: self.revision,
            changed_at_ms: self.changed_at_ms,
        }
    }

    fn remote_state(&self) -> RemoteState {
        let title = match &self.item {
            LiveItem::Song(song) => song.title.clone(),
            LiveItem::Passage { title, .. } => title.clone(),
        };
        RemoteState {
            // Both counters only grow, so their sum does too.
            revision: self.revision + self.view_revision,
            title,
            slides: self.slides.clone(),
            current: self.current,
            blank: self.blank,
            view: self.view.clone(),
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
    use crate::SlideKind;

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
        let slide = |label: &str| Slide { lines: vec!["text".into()], label: label.into(), kind: SlideKind::Scripture, ..Default::default() };
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

    /// Runs `cmd` on `s` and returns its result (the reply comes straight back
    /// for store commands and for the synchronous test operators below).
    fn run(s: &Store, cmd: Command) -> Result<Outcome, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        s.command(cmd, move |r| tx.send(r).unwrap());
        rx.recv_timeout(std::time::Duration::from_secs(1)).expect("exactly one reply")
    }

    #[test]
    fn store_commands_move_slides_and_blank() {
        let s = store();
        // (command, expected result, current slide after, blank after)
        let table: Vec<(Command, Result<Outcome, String>, usize, Blank)> = vec![
            (Command::Next, Ok(Outcome::Done), 1, Blank::None),
            (Command::Next, Ok(Outcome::Done), 2, Blank::None),
            (Command::Previous, Ok(Outcome::Done), 1, Blank::None),
            (Command::Goto { slide: 4 }, Ok(Outcome::Done), 4, Blank::None),
            (Command::Goto { slide: 99 }, Err("There is no slide 100 (the live item has 7).".into()), 4, Blank::None),
            (Command::Blank { mode: Blank::Black }, Ok(Outcome::Done), 4, Blank::Black),
            (Command::Blank { mode: Blank::Black }, Ok(Outcome::Done), 4, Blank::Black),
            (Command::ToggleBlank { mode: Blank::Black }, Ok(Outcome::Done), 4, Blank::None),
            (Command::ToggleBlank { mode: Blank::Logo }, Ok(Outcome::Done), 4, Blank::Logo),
            (Command::Blank { mode: Blank::None }, Ok(Outcome::Done), 4, Blank::None),
            (Command::Goto { slide: 0 }, Ok(Outcome::Done), 0, Blank::None),
            (Command::Previous, Ok(Outcome::Done), 0, Blank::None),
        ];
        assert_eq!(s.snapshot().slides.len(), 7);
        for (cmd, result, current, blank) in table {
            let label = format!("{cmd:?}");
            assert_eq!(run(&s, cmd), result, "{label}");
            let snap = s.snapshot();
            assert_eq!((snap.current, snap.blank), (current, blank), "{label}");
        }
    }

    #[test]
    fn operator_commands_need_an_operator() {
        let s = store();
        let cmd = Command::Item { item: 0, slide: 0 };
        assert_eq!(run(&s, cmd.clone()), Err("The console isn't running.".into()));

        // The operator gets the command and puts things live through the store.
        let store2 = s.clone();
        s.set_operator(move |cmd, reply| match cmd {
            Command::Item { item: 0, slide } => {
                store2.load_song(crate::sample_song(), slide);
                reply(Ok(Outcome::Done));
            }
            Command::Verse { reference, .. } => reply(Ok(Outcome::Passage { title: reference, slides: Vec::new() })),
            _ => reply(Err("no such item".into())),
        });
        let rev = s.snapshot().revision;
        assert_eq!(run(&s, Command::Item { item: 0, slide: 2 }), Ok(Outcome::Done));
        assert_eq!((s.snapshot().current, s.snapshot().revision), (2, rev + 1));
        assert_eq!(run(&s, Command::Item { item: 5, slide: 0 }), Err("no such item".into()));
        let verse = Command::Verse { reference: "Jn 3:16".into(), translation: None, live: false };
        assert_eq!(run(&s, verse), Ok(Outcome::Passage { title: "Jn 3:16".into(), slides: Vec::new() }));
        // Store commands never reach the operator.
        assert_eq!(run(&s, Command::Next), Ok(Outcome::Done));
        assert_eq!(s.snapshot().current, 3);
    }

    #[test]
    fn remote_listeners_see_live_and_view_changes() {
        let s = store();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let displays = Arc::new(AtomicU64::new(0));
        {
            let seen = seen.clone();
            s.subscribe_remote(move |st| seen.lock().unwrap().push((st.revision, st.current, st.view.items.len())));
            let displays = displays.clone();
            s.subscribe(move |_| {
                displays.fetch_add(1, Ordering::SeqCst);
            });
        }
        s.next();
        let entry = |t: &str| crate::ServiceEntry { title: t.into(), kind: "song" };
        let view = RemoteView { service: "Sunday".into(), items: vec![entry("A"), entry("B")], cued: Some(0), ..Default::default() };
        s.set_remote_view(view.clone());
        s.set_remote_view(view.clone()); // unchanged: nothing
        assert_eq!(displays.load(Ordering::SeqCst), 2, "up next changed, so the stage view hears of it");
        assert_eq!(s.display_state().up_next.as_deref(), Some("A"));
        // A view change that leaves "up next" alone isn't a display change.
        s.set_remote_view(RemoteView { service: "Sunday 10am".into(), ..view });
        assert_eq!(displays.load(Ordering::SeqCst), 2);

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert!(seen.windows(2).all(|w| w[0].0 < w[1].0), "revisions only grow: {seen:?}");
        assert_eq!((seen[0].1, seen[1].2), (1, 2));
        let st = s.remote_state();
        assert_eq!((st.title.as_str(), st.view.service.as_str(), st.revision), ("Blessed Assurance", "Sunday 10am", seen[2].0));
        let json = serde_json::to_value(&st).unwrap();
        assert_eq!(json["items"][1]["title"], "B");
        assert_eq!(json["slides"][1]["label"], "C1");
    }

    #[test]
    fn display_state_carries_the_next_slide() {
        let s = store();
        let st = s.display_state();
        assert_eq!((st.slide.unwrap().label, st.next.unwrap().label), ("V1".into(), "C1".into()));
        s.go_to(s.snapshot().slides.len() - 1);
        assert_eq!(s.display_state().next, None);
    }
}
