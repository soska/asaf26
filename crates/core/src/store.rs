use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Blank, DisplayState, Slide, Song, Theme, Transition};

type Listener = Box<dyn Fn(&DisplayState) + Send + Sync>;

/// The single source of truth. Front ends mutate it; observers (the server)
/// are told about every change.
pub struct Store {
    live: Mutex<Live>,
    listeners: Mutex<Vec<Listener>>,
}

struct Live {
    song: Song,
    slides: Vec<Slide>,
    current: usize,
    blank: Blank,
    theme: Theme,
    transition: Transition,
    revision: u64,
    changed_at_ms: u64,
}

/// What the console needs to draw itself.
#[derive(Clone, Debug)]
pub struct LiveSnapshot {
    pub title: String,
    pub slides: Vec<Slide>,
    pub current: usize,
    pub blank: Blank,
    pub revision: u64,
}

impl Store {
    pub fn new(song: Song, theme: Theme) -> Arc<Self> {
        let slides = song.slides();
        Arc::new(Self {
            live: Mutex::new(Live {
                song,
                slides,
                current: 0,
                blank: Blank::None,
                theme,
                transition: Transition::default(),
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
        LiveSnapshot {
            title: live.song.title.clone(),
            slides: live.slides.clone(),
            current: live.current,
            blank: live.blank,
            revision: live.revision,
        }
    }

    pub fn next(&self) {
        self.update(|l| {
            if l.current + 1 < l.slides.len() {
                l.current += 1;
            }
        });
    }

    pub fn previous(&self) {
        self.update(|l| l.current = l.current.saturating_sub(1));
    }

    pub fn go_to(&self, index: usize) {
        self.update(|l| {
            if index < l.slides.len() {
                l.current = index;
            }
        });
    }

    /// Switches to `mode`, or back to showing text if `mode` is already active.
    pub fn toggle_blank(&self, mode: Blank) {
        self.update(|l| l.blank = if l.blank == mode { Blank::None } else { mode });
    }

    pub fn set_blank(&self, mode: Blank) {
        self.update(|l| l.blank = mode);
    }

    /// Applies `change`; bumps the revision and notifies only if something changed.
    fn update(&self, change: impl FnOnce(&mut Live)) {
        let state = {
            let mut live = self.live.lock().unwrap();
            let before = (live.current, live.blank);
            change(&mut live);
            if (live.current, live.blank) == before {
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
    fn display_state(&self) -> DisplayState {
        DisplayState {
            slide: self.slides.get(self.current).cloned(),
            blank: self.blank,
            theme: self.theme.clone(),
            transition: self.transition.clone(),
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
    fn display_state_serializes_with_current_slide() {
        let s = store();
        s.next();
        let json = serde_json::to_value(s.display_state()).unwrap();
        assert_eq!(json["slide"]["label"], "C1");
        assert_eq!(json["blank"], "None");
        assert!(json["theme"]["size_max_vh"].is_number());
    }
}
