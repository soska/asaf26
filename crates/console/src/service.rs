//! The service list: add songs and passages from the source panel, reorder
//! (keyboard or drag), remove, notes and a theme per item, save and open
//! `.asaf-service` files, and autosave for crash safety.
//!
//! One item is *cued*: the preview shows it and ⏎ puts it live, after which
//! the cue moves to the next item. Running a service is ⏎, arrows, ⏎…

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use asaf_core::{
    AutosaveInfo, Bible, ItemContent, Passage, SERVICE_EXTENSION, Service, ServiceItem, Slide, Song,
};
use asaf_library::service::{read_service, service_name, with_service_extension, write_autosave, write_service};
use gpui::component::button::{Button, ButtonVariants as _};
use gpui::component::input::{Input, InputEvent, InputState};
use gpui::component::notification::Notification;
use gpui::component::{ActiveTheme as _, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, PathPromptOptions,
    PromptLevel, Render, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    TestSupportExt as _, Window, div, px,
};

use crate::console::{Console, PreviewSource};
use crate::i18n::lang;
use crate::slides::slide_card;
use crate::transfer;

/// How long the service must be left alone before it is autosaved.
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);

/// The service the console starts with (usually restored from the autosave),
/// and where to autosave.
#[derive(Default)]
pub struct ServiceSession {
    pub service: Service,
    /// The file it was opened from or last saved to.
    pub path: Option<PathBuf>,
    /// Changes not yet saved to `path`.
    pub dirty: bool,
    /// Where to autosave; `None` turns autosave off.
    pub autosave: Option<PathBuf>,
    /// Shown once the console opens, e.g. a damaged autosave.
    pub notice: Option<String>,
}

pub(crate) struct ServiceState {
    pub service: Service,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    autosave: Option<PathBuf>,
    autosave_task: Option<Task<()>>,
    /// An autosave is scheduled and not yet written (flushed on quit).
    autosave_pending: Arc<AtomicBool>,
    /// The cued item: what ⏎ puts live.
    pub cursor: Option<usize>,
    /// The item on screen, when it came from the service.
    pub live_key: Option<u64>,
    /// The cued item, resolved into slides for the preview.
    pub resolved: Option<Resolved>,
    pub notes: Entity<InputState>,
    pub theme: Entity<InputState>,
    /// Which item the notes and theme fields show.
    pub(crate) inputs_key: Option<u64>,
    list_scroll: ScrollHandle,
}

/// A service item looked up in the library and laid out.
pub(crate) struct Resolved {
    pub key: u64,
    /// The song title, or "Juan 3:16-18 (RV1909)".
    pub title: String,
    /// Authors, or the translation's name.
    pub subtitle: String,
    pub slides: Vec<Slide>,
    pub target: Target,
}

pub(crate) enum Target {
    Song(Box<Song>),
    Passage(Passage),
    /// Not in this library (another machine's service, a deleted song, a
    /// translation that isn't installed): why.
    Missing(String),
}

/// A service row being dragged to a new position.
#[derive(Clone)]
pub(crate) struct DraggedItem {
    key: u64,
    title: SharedString,
}

/// A song dragged from the library onto the service.
#[derive(Clone)]
pub(crate) struct DraggedSong {
    pub id: i64,
    pub title: SharedString,
}

/// What follows the pointer while dragging.
pub(crate) struct DragGhost(pub SharedString);

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(theme.primary)
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .child(self.0.clone())
    }
}

impl ServiceState {
    pub fn new(session: &mut ServiceSession, window: &mut Window, cx: &mut Context<Console>) -> Self {
        let service = std::mem::take(&mut session.service);
        Self {
            cursor: (!service.items.is_empty()).then_some(0),
            service,
            path: session.path.take(),
            dirty: session.dirty,
            autosave: session.autosave.take(),
            autosave_task: None,
            autosave_pending: Arc::new(AtomicBool::new(false)),
            live_key: None,
            resolved: None,
            notes: cx.new(|cx| InputState::new(window, cx).placeholder(lang(cx).t("Notes for the operator"))),
            theme: cx.new(|cx| InputState::new(window, cx).placeholder(lang(cx).t("Default theme"))),
            inputs_key: None,
            list_scroll: ScrollHandle::new(),
        }
    }

    pub fn cued(&self) -> Option<&ServiceItem> {
        self.cursor.and_then(|i| self.service.items.get(i))
    }

    /// "Sunday 10am" with a dot while there are unsaved changes.
    pub fn title(&self) -> String {
        format!("{}{}", self.service.display_name(), if self.dirty { " •" } else { "" })
    }

    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<Console>) -> Vec<Subscription> {
        [self.notes.clone(), self.theme.clone()]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => this.item_fields_changed(cx),
                    InputEvent::PressEnter { .. } => this.focus.focus(window, cx),
                    _ => {}
                })
            })
            .collect()
    }

    /// Writes the autosave now if one is waiting (on quit).
    pub fn flush_autosave(&self) {
        if let Some(path) = &self.autosave
            && self.autosave_pending.swap(false, Ordering::SeqCst)
            && let Err(e) = write_autosave(path, &self.service, &self.autosave_info())
        {
            eprintln!("asaf: autosave failed: {e}");
        }
    }

    fn autosave_info(&self) -> AutosaveInfo {
        AutosaveInfo { path: self.path.clone(), dirty: self.dirty }
    }
}

/// Closing the console window drops the waiting autosave task (and quits the
/// app before `on_app_quit` could reach this view), so write it here.
impl Drop for ServiceState {
    fn drop(&mut self) {
        self.flush_autosave();
    }
}

impl Console {
    // ---- editing the list -------------------------------------------------------

    /// Something in the service changed: mark it unsaved, autosave soon, and
    /// refresh the cued item.
    pub(crate) fn service_changed(&mut self, cx: &mut Context<Self>) {
        self.service.dirty = true;
        self.schedule_autosave(cx);
        self.resolve_cue(cx);
    }

    /// Writes the autosave once the service has been left alone for
    /// [`AUTOSAVE_DELAY`]. Each change restarts the wait.
    fn schedule_autosave(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.service.autosave.clone() else { return };
        let pending = self.service.autosave_pending.clone();
        pending.store(true, Ordering::SeqCst);
        self.service.autosave_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTOSAVE_DELAY).await;
            let Ok((service, info)) = this.update(cx, |this, _| (this.service.service.clone(), this.service.autosave_info()))
            else {
                return;
            };
            if !pending.swap(false, Ordering::SeqCst) {
                return;
            }
            let result = cx.background_executor().spawn(async move { write_autosave(&path, &service, &info) }).await;
            if let Err(e) = result {
                eprintln!("asaf: autosave failed: {e}");
            }
        }));
    }

    /// Appends `item`; the first item added to an empty service is cued.
    pub(crate) fn add_to_service(&mut self, item: ServiceItem, window: &mut Window, cx: &mut Context<Self>) {
        let title = item.title();
        let index = self.service.service.push(item);
        if self.service.cursor.is_none() {
            self.service.cursor = Some(index);
        }
        self.service.list_scroll.scroll_to_item(index);
        self.service_changed(cx);
        window.push_notification(Notification::success(format!("Added “{title}” to the service")), cx);
    }

    /// Adds the song chosen in the library.
    pub(crate) fn add_song_to_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(song) = &self.preview
            && let Some(id) = song.id
        {
            let item = ServiceItem::song(id, song.title.clone());
            self.add_to_service(item, window, cx);
        }
    }

    /// Moves the cue by `delta` items. From the source panel, the first press
    /// just brings the cued item back into the preview.
    pub(crate) fn move_cue(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.service.service.items.len();
        if len == 0 {
            return;
        }
        if self.preview_source == PreviewSource::Service || self.service.cursor.is_none() {
            let i = self.service.cursor.map_or(0, |i| (i as isize + delta).clamp(0, len as isize - 1) as usize);
            self.service.cursor = Some(i);
        }
        self.preview_source = PreviewSource::Service;
        self.cue_changed(cx);
    }

    pub(crate) fn cue(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.service.service.items.len() {
            self.service.cursor = Some(index);
            self.preview_source = PreviewSource::Service;
            self.cue_changed(cx);
        }
    }

    fn cue_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(i) = self.service.cursor {
            self.service.list_scroll.scroll_to_item(i);
        }
        self.resolve_cue(cx);
    }

    /// Moves the cued item up (`-1`) or down (`1`).
    pub(crate) fn move_cued_item(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(from) = self.service.cursor else { return };
        let to = from as isize + delta;
        if to < 0 || to as usize >= self.service.service.items.len() {
            return;
        }
        self.service.cursor = self.service.service.move_item(from, to as usize);
        self.preview_source = PreviewSource::Service;
        self.cue_changed(cx);
        self.service_changed(cx);
    }

    /// Drag and drop: the item with `key` takes the place of the item at `to`.
    pub(crate) fn move_item_to(&mut self, key: u64, to: usize, cx: &mut Context<Self>) {
        let Some(from) = self.service.service.position(key) else { return };
        if from == to {
            return;
        }
        let cued = self.service.cued().map(|i| i.key);
        self.service.service.move_item(from, to);
        self.service.cursor = cued.and_then(|k| self.service.service.position(k));
        self.service_changed(cx);
    }

    /// Drag and drop: a library song dropped at `index` (or the end).
    pub(crate) fn insert_song(&mut self, song: &DraggedSong, index: Option<usize>, cx: &mut Context<Self>) {
        let cued = self.service.cued().map(|i| i.key);
        let item = ServiceItem::song(song.id, song.title.to_string());
        let index = index.unwrap_or(self.service.service.items.len()).min(self.service.service.items.len());
        self.service.service.items.insert(index, item);
        self.service.cursor = cued.and_then(|k| self.service.service.position(k)).or(Some(index));
        self.service_changed(cx);
    }

    pub(crate) fn remove_cued_item(&mut self, cx: &mut Context<Self>) {
        let Some(i) = self.service.cursor else { return };
        self.service.service.remove(i);
        let len = self.service.service.items.len();
        self.service.cursor = (len > 0).then(|| i.min(len - 1));
        self.service_changed(cx);
    }

    /// The notes or theme field was edited: store it on the item it shows.
    fn item_fields_changed(&mut self, cx: &mut Context<Self>) {
        let notes = self.service.notes.read(cx).value().to_string();
        let theme = self.service.theme.read(cx).value().trim().to_string();
        let theme = (!theme.is_empty()).then_some(theme);
        let Some(item) = self.service.inputs_key.and_then(|k| self.service.service.items.iter_mut().find(|i| i.key == k))
        else {
            return;
        };
        if item.notes != notes || item.theme != theme {
            item.notes = notes;
            item.theme = theme;
            self.service.dirty = true;
            self.schedule_autosave(cx);
            cx.notify();
        }
    }

    /// Points the notes and theme fields at the cued item.
    pub(crate) fn sync_item_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cued = self.service.cued().map(|i| (i.key, i.notes.clone(), i.theme.clone().unwrap_or_default()));
        let key = cued.as_ref().map(|c| c.0);
        if key == self.service.inputs_key {
            return;
        }
        self.service.inputs_key = key;
        let (_, notes, theme) = cued.unwrap_or_default();
        self.service.notes.update(cx, |s, cx| s.set_value(notes, window, cx));
        self.service.theme.update(cx, |s, cx| s.set_value(theme, window, cx));
    }

    /// A theme chip under the cued item's fields was clicked: `None` is the default.
    pub(crate) fn choose_item_theme(&mut self, name: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_item_fields(window, cx);
        let value = name.unwrap_or_default();
        self.service.theme.update(cx, |s, cx| s.set_value(value, window, cx));
        self.item_fields_changed(cx);
    }

    pub(crate) fn edit_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.service.cued().is_some() {
            self.sync_item_fields(window, cx);
            self.service.notes.update(cx, |s, cx| s.focus(window, cx));
        }
    }

    // ---- resolving and going live -------------------------------------------------

    /// Looks the cued item up in the library and lays it out for the preview.
    pub(crate) fn resolve_cue(&mut self, cx: &mut Context<Self>) {
        self.service.resolved = self.service.cued().cloned().map(|item| self.resolve(&item));
        cx.notify();
    }

    fn resolve(&self, item: &ServiceItem) -> Resolved {
        let missing = |why: String| Resolved {
            key: item.key,
            title: item.title(),
            subtitle: String::new(),
            slides: Vec::new(),
            target: Target::Missing(why),
        };
        match &item.content {
            ItemContent::Song { song_id, title } => {
                match self.library.lock().unwrap().find_service_song(*song_id, title) {
                    Ok(Some(song)) => Resolved {
                        key: item.key,
                        title: song.title.clone(),
                        subtitle: song.authors.join(", "),
                        slides: song.slides(),
                        target: Target::Song(Box::new(song)),
                    },
                    Ok(None) => missing(format!("“{title}” isn't in the song library.")),
                    Err(e) => missing(format!("Can't read the library: {e}")),
                }
            }
            ItemContent::Passage { bible_id, translation, ranges, .. } => {
                let Some(bible) = self.find_bible(*bible_id, translation) else {
                    return missing(format!("The {translation} Bible isn't installed."));
                };
                match self.lay_out_passage(&bible, ranges.clone()) {
                    Ok(p) => Resolved {
                        key: item.key,
                        title: p.title(),
                        subtitle: bible.name.clone(),
                        slides: p.slides,
                        target: Target::Passage(p.passage),
                    },
                    Err(e) => missing(e),
                }
            }
        }
    }

    /// A translation by abbreviation (portable between machines), else by id.
    fn find_bible(&self, id: i64, abbreviation: &str) -> Option<Bible> {
        let by_name = self.bible.bibles.iter().find(|b| b.abbreviation.eq_ignore_ascii_case(abbreviation));
        by_name.or_else(|| self.bible.bibles.iter().find(|b| b.id == Some(id))).cloned()
    }

    /// Puts the cued item live at slide `index`, in its theme, and cues the
    /// next item.
    pub(crate) fn go_live_cued(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = &self.service.resolved else { return };
        let theme = self.service.service.items.iter().find(|i| i.key == r.key).and_then(|i| self.item_theme(i));
        match &r.target {
            Target::Song(song) => self.store.load_song_themed((**song).clone(), index, theme),
            Target::Passage(passage) => {
                self.store.load_passage_themed(passage.clone(), r.title.clone(), r.slides.clone(), index, theme)
            }
            Target::Missing(why) => {
                window.push_notification(Notification::error(why.clone()), cx);
                return;
            }
        }
        self.service.live_key = Some(r.key);
        if let Some(i) = self.service.cursor
            && i + 1 < self.service.service.items.len()
        {
            self.service.cursor = Some(i + 1);
        }
        self.preview_source = PreviewSource::Service;
        self.focus.focus(window, cx);
        self.cue_changed(cx);
    }

    /// ⏎ in live control. After the last item has gone live the cue stays
    /// on it, and a habitual ⏎ must not restart it from the top on screen.
    pub(crate) fn go_live_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cued = self.service.cued().map(|i| i.key);
        if cued.is_some() && cued == self.service.live_key {
            window.push_notification(Notification::info("End of the service"), cx);
            return;
        }
        self.go_live_cued(0, window, cx);
    }

    /// Something from the source panel went live: it isn't a service item,
    /// and the preview goes back to the cued item.
    pub(crate) fn went_live_ad_hoc(&mut self, cx: &mut Context<Self>) {
        self.service.live_key = None;
        if !self.service.service.items.is_empty() {
            self.preview_source = PreviewSource::Service;
        }
        cx.notify();
    }

    // ---- files ------------------------------------------------------------------

    /// Asks before throwing away unsaved changes; resolves to whether to go on.
    fn discard_ok(&self, window: &mut Window, cx: &mut Context<Self>) -> impl Future<Output = bool> + 'static {
        let prompt = self.service.dirty.then(|| {
            let message = format!("Discard unsaved changes to “{}”?", self.service.service.display_name());
            window.prompt(PromptLevel::Warning, &message, None, &["Discard Changes", "Cancel"], cx)
        });
        async move {
            match prompt {
                None => true,
                Some(answer) => answer.await == Ok(0),
            }
        }
    }

    pub(crate) fn new_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ok = self.discard_ok(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if ok.await {
                let _ = this.update(cx, |this, cx| this.set_service(Service::default(), None, cx));
            }
        })
        .detach();
    }

    pub(crate) fn open_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ok = self.discard_ok(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if !ok.await {
                return;
            }
            let Ok(paths) = this.update(cx, |_, cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Open".into()),
                })
            }) else {
                return;
            };
            let Ok(Ok(Some(mut paths))) = paths.await else { return };
            let Some(path) = paths.pop() else { return };
            let _ = this.update_in(cx, |this, window, cx| this.open_service_file(path, window, cx));
        })
        .detach();
    }

    pub(crate) fn open_service_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match read_service(&path) {
            Ok((mut service, _)) => {
                if service.name.trim().is_empty() {
                    service.name = service_name(&path);
                }
                self.set_service(service, Some(path), cx);
            }
            Err(e) => window.push_notification(Notification::error(e), cx),
        }
    }

    /// Replaces the whole service (new or opened), cueing its first item.
    fn set_service(&mut self, service: Service, path: Option<PathBuf>, cx: &mut Context<Self>) {
        let s = &mut self.service;
        s.cursor = (!service.items.is_empty()).then_some(0);
        s.service = service;
        s.path = path;
        s.dirty = false;
        s.live_key = None;
        self.preview_source = PreviewSource::Service;
        self.schedule_autosave(cx);
        self.cue_changed(cx);
    }

    /// ⌘S saves to the service's file (asking where the first time); ⇧⌘S
    /// always asks.
    pub(crate) fn save_service(&mut self, save_as: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !save_as && let Some(path) = self.service.path.clone() {
            self.write_service_file(path, window, cx);
            return;
        }
        let dir = self
            .service
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(transfer::documents_dir);
        let name = format!("{}.{SERVICE_EXTENSION}", self.service.service.display_name());
        let path = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = path.await else { return };
            let path = with_service_extension(path);
            let _ = this.update_in(cx, |this, window, cx| this.write_service_file(path, window, cx));
        })
        .detach();
    }

    pub(crate) fn write_service_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // The file names the service.
        let mut service = self.service.service.clone();
        service.name = service_name(&path);
        match write_service(&path, &service) {
            Ok(()) => {
                self.service.service.name = service.name;
                self.service.path = Some(path);
                self.service.dirty = false;
                self.schedule_autosave(cx);
                let note = format!("Saved “{}”", self.service.service.display_name());
                window.push_notification(Notification::success(note), cx);
            }
            Err(e) => {
                let note = format!("Couldn't save to {}: {e}", path.display());
                window.push_notification(Notification::error(note), cx);
            }
        }
        cx.notify();
    }

    // ---- rendering --------------------------------------------------------------

    pub(crate) fn service_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let chips = if self.service.cued().is_some() { Some(self.item_theme_chips(cx)) } else { None };
        let theme = cx.theme();
        let state = &self.service;
        let items = &state.service.items;
        let showing_cue = self.preview_source == PreviewSource::Service;

        let rows = items.iter().enumerate().map(|(i, item)| {
            let cued = state.cursor == Some(i);
            let live = state.live_key == Some(item.key);
            let title: SharedString = item.title().into();
            let ghost = DraggedItem { key: item.key, title: title.clone() };
            h_flex()
                .id(("service-item", i))
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(if cued && showing_cue { theme.primary } else { theme.transparent })
                .when(cued, |d| d.bg(theme.list_active))
                .when(!cued, |d| d.hover(|s| s.bg(theme.list_hover)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
                    this.cue(i, cx);
                    if ev.click_count() >= 2 {
                        this.go_live_cued(0, window, cx);
                    }
                }))
                .on_drag(ghost, |d, _, _, cx| cx.new(|_| DragGhost(d.title.clone())))
                .drag_over::<DraggedItem>(|s, _, _, cx| s.border_color(cx.theme().primary))
                .drag_over::<DraggedSong>(|s, _, _, cx| s.border_color(cx.theme().primary))
                .on_drop(cx.listener(move |this, d: &DraggedItem, _, cx| this.move_item_to(d.key, i, cx)))
                .on_drop(cx.listener(move |this, d: &DraggedSong, _, cx| this.insert_song(d, Some(i), cx)))
                .child(
                    div()
                        .flex_none()
                        .w(px(18.))
                        .text_xs()
                        .text_color(if live { theme.danger } else { theme.muted_foreground })
                        .child(if live { "●".to_string() } else { format!("{}", i + 1) }),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(title))
                        .when(!item.notes.is_empty() || item.theme.is_some(), |d| {
                            let mut extra = item.notes.lines().next().unwrap_or_default().to_string();
                            if let Some(t) = &item.theme {
                                extra = if extra.is_empty() { format!("{}: {t}", l.t("Theme")) } else { format!("{extra} · {t}") };
                            }
                            d.child(div().truncate().text_xs().text_color(theme.muted_foreground).child(extra))
                        }),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(if item.is_song() { l.t("Song") } else { l.t("Bible") }),
                )
                .test_support()
        });

        v_flex()
            .w(px(270.))
            .flex_none()
            .h_full()
            .border_r_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::BOLD)
                                    .child(state.title()),
                            )
                            .child(Button::new("service-new").xsmall().ghost().label(l.t("New")).tooltip(l.t("New service (⇧⌘N)")).on_click(
                                cx.listener(|this, _, window, cx| this.new_service(window, cx)),
                            ))
                            .child(Button::new("service-open").xsmall().ghost().label(l.t("Open…")).tooltip(l.t("Open a service (⌘O)")).on_click(
                                cx.listener(|this, _, window, cx| this.open_service(window, cx)),
                            ))
                            .child(
                                Button::new("service-save")
                                    .xsmall()
                                    .label(l.t("Save"))
                                    .tooltip(l.t("Save (⌘S), Save As (⇧⌘S)"))
                                    .on_click(cx.listener(|this, _, window, cx| this.save_service(false, window, cx))),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .id("service-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&state.list_scroll)
                    .px_2()
                    .gap_0p5()
                    .on_drop(cx.listener(|this, d: &DraggedSong, _, cx| this.insert_song(d, None, cx)))
                    .on_drop(cx.listener(|this, d: &DraggedItem, _, cx| {
                        let end = this.service.service.items.len().saturating_sub(1);
                        this.move_item_to(d.key, end, cx)
                    }))
                    .when(items.is_empty(), |d| {
                        d.child(
                            v_flex()
                                .gap_1()
                                .p_2()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(l.t("The service is empty."))
                                .child(l.t("Choose a song or type a reference on the left and press ⇧⏎ to add it, or drag songs here.")),
                        )
                    })
                    .children(rows),
            )
            .when(state.cued().is_some(), |d| {
                d.child(
                    v_flex()
                        .gap_1()
                        .p_3()
                        .border_t_1()
                        .border_color(theme.border)
                        .text_xs()
                        .child(div().text_color(theme.muted_foreground).child(l.t("Notes (⌘I)")))
                        .child(div().key_context("ServiceItem").child(Input::new(&state.notes).small()))
                        .child(div().text_color(theme.muted_foreground).child(l.t("Theme")))
                        .child(div().key_context("ServiceItem").child(Input::new(&state.theme).small()))
                        .children(chips),
                )
            })
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(l.t("⌘↑↓ cue  ·  ⏎ go live  ·  ⌥↑↓ move  ·  ⌘⌫ remove")),
            )
    }

    /// The themes as chips under the cued item's theme field (the default
    /// first), and a warning if the field names a theme that doesn't exist.
    fn item_theme_chips(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let l = lang(cx);
        let current = self.service.cued().and_then(|i| i.theme.clone());
        let unknown = current.as_ref().is_some_and(|name| !self.themes.iter().any(|t| t.name.eq_ignore_ascii_case(name.trim())));
        let chip = |id: (&'static str, usize), label: String, on: bool, name: Option<String>, cx: &mut Context<Self>| {
            Button::new(id)
                .xsmall()
                .ghost()
                .label(label)
                .selected(on)
                .on_click(cx.listener(move |this, _, window, cx| this.choose_item_theme(name.clone(), window, cx)))
        };
        // "Default theme", not "Default": the library's default theme is itself usually called Default.
        let mut chips = vec![chip(("item-theme", 0), l.t("Default theme").to_string(), current.is_none(), None, cx)];
        for (i, t) in self.themes.iter().enumerate() {
            let on = current.as_ref().is_some_and(|c| c.trim().eq_ignore_ascii_case(&t.name));
            chips.push(chip(("item-theme", i + 1), t.name.clone(), on, Some(t.name.clone()), cx));
        }
        v_flex()
            .gap_1()
            .child(h_flex().flex_wrap().gap_1().children(chips))
            .when(unknown, |d| {
                d.child(div().text_color(cx.theme().warning).child(l.t("There's no theme with that name; the default is used.")))
            })
            .into_any_element()
    }

    /// The preview pane for the cued service item.
    pub(crate) fn cue_preview_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let is_passage = matches!(self.service.resolved.as_ref().map(|r| &r.target), Some(Target::Passage(_)));
        let layout_controls = is_passage.then(|| self.layout_controls(cx));
        let theme = cx.theme();
        let Some(r) = &self.service.resolved else {
            return v_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .justify_center()
                .p_4()
                .text_color(theme.muted_foreground)
                .child(l.t("Nothing cued. Add songs and passages to the service."))
                .into_any_element();
        };
        let item = self.service.cued();
        let look = self.look_of(item);
        let live = self.store.snapshot();
        let is_live = self.service.live_key == Some(r.key);
        let notes = item.map(|i| i.notes.clone()).unwrap_or_default();
        let position = self.service.cursor.map(|i| {
            format!("{} {} {} {} · {} {}", l.t("Item"), i + 1, l.t("of"), self.service.service.items.len(), l.t("Theme"), look.name)
        });

        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                h_flex()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{} · {}", l.t("PREVIEW"), position.unwrap_or_default().to_uppercase())),
                            )
                            .child(div().truncate().text_base().font_weight(FontWeight::BOLD).child(r.title.clone()))
                            .child(div().truncate().text_xs().text_color(theme.muted_foreground).child(r.subtitle.clone())),
                    )
                    .child(
                        Button::new("cue-go-live")
                            .small()
                            .primary()
                            .label(l.t("Go live"))
                            .tooltip(l.t("Show the first slide and cue the next item (⏎)"))
                            .on_click(cx.listener(|this, _, window, cx| this.go_live_cued(0, window, cx))),
                    ),
            )
            .when(!notes.is_empty(), |d| {
                d.child(
                    div()
                        .mx_4()
                        .mt_2()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(theme.warning.opacity(0.15))
                        .text_xs()
                        .child(notes),
                )
            })
            .children(layout_controls)
            .child(
                v_flex()
                    .id("cue-slides")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_3()
                    .gap_2()
                    .when_some(
                        match &r.target {
                            Target::Missing(why) => Some(why.clone()),
                            _ if r.slides.is_empty() => Some(l.t("This item has no slides.").to_string()),
                            _ => None,
                        },
                        |d, why| d.child(div().text_color(theme.danger).child(why)),
                    )
                    .children(r.slides.iter().enumerate().map(|(i, slide)| {
                        slide_card(("cue-slide", i), slide, is_live && i == live.current, &look, cx)
                            .on_click(cx.listener(move |this, _, window, cx| this.go_live_cued(i, window, cx)))
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;

    use asaf_core::{Blank, Store, Verse};
    use asaf_library::Library;
    use asaf_library::service::{AUTOSAVE_FILE, restore_autosave};
    use gpui::test::TestWindowExt as _;
    use gpui::{AnyWindowHandle, TestAppContext};

    use super::*;
    use crate::console::tests::{open_console, setup_session, with_bibles, with_window};

    /// Public-domain hymns (with "Blessed Assurance" and "Cuán Grande Es Él"
    /// from the test setup, five songs), plus the KJV and RV1909 excerpts.
    fn hymns_and_bibles(lib: &mut Library) {
        let hymn = |title: &str, verses: &[(&str, &str)]| Song {
            title: title.into(),
            authors: vec!["Public domain".into()],
            verses: verses.iter().map(|(t, l)| Verse::new(t, l)).collect(),
            ..Default::default()
        };
        lib.save(&hymn(
            "Amazing Grace",
            &[
                ("v1", "Amazing grace! How sweet the sound\nThat saved a wretch like me!"),
                ("v2", "'Twas grace that taught my heart to fear,\nAnd grace my fears relieved;"),
                ("v3", "Through many dangers, toils and snares,\nI have already come;"),
            ],
        ))
        .unwrap();
        lib.save(&hymn(
            "Holy, Holy, Holy",
            &[
                ("v1", "Holy, holy, holy! Lord God Almighty!\nEarly in the morning our song shall rise to Thee;"),
                ("v2", "Holy, holy, holy! All the saints adore Thee,\nCasting down their golden crowns around the glassy sea;"),
            ],
        ))
        .unwrap();
        lib.save(&hymn(
            "Castillo Fuerte",
            &[
                ("v1", "Castillo fuerte es nuestro Dios,\ndefensa y buen escudo."),
                ("v2", "Nuestro valor es nada aquí,\ncon él todo es perdido;"),
            ],
        ))
        .unwrap();
        with_bibles(lib);
    }

    fn press(cx: &mut TestAppContext, h: AnyWindowHandle, keys: &[&str]) {
        with_window(cx, h, |w, cx| {
            for key in keys {
                w.press(key, cx);
            }
        });
    }

    /// Replaces the text of the focused field.
    fn retype(cx: &mut TestAppContext, h: AnyWindowHandle, text: &str) {
        with_window(cx, h, |w, cx| {
            w.press("cmd-a", cx);
            w.press("backspace", cx);
            w.input(text, cx);
        });
    }

    fn titles(cx: &mut TestAppContext, console: &Entity<Console>) -> Vec<String> {
        cx.update(|cx| console.read(cx).service.service.items.iter().map(ServiceItem::title).collect())
    }

    fn cursor(cx: &mut TestAppContext, console: &Entity<Console>) -> Option<usize> {
        cx.update(|cx| console.read(cx).service.cursor)
    }

    /// ⏎ puts the cued item live; then → steps to its last slide (one more
    /// → stays there). Returns how many slides it had.
    fn run_item(cx: &mut TestAppContext, h: AnyWindowHandle, store: &Store, title: &str) -> usize {
        press(cx, h, &["enter"]);
        let live = store.snapshot();
        assert_eq!((live.title.as_str(), live.current), (title, 0), "⏎ puts {title} live");
        let n = live.slides.len();
        assert!(n > 0, "{title} has slides");
        for i in 1..n {
            press(cx, h, &["right"]);
            assert_eq!(store.snapshot().current, i, "{title}: → to slide {}", i + 1);
        }
        press(cx, h, &["right"]);
        assert_eq!(store.snapshot().current, n - 1, "{title}: → at the end stays");
        n
    }

    /// PRD Phase 3 "done when": a full mock service (5 songs, 2 passages, 1
    /// ad-hoc verse) is prepared and run with key presses and typing only.
    /// Nothing here moves or clicks the mouse.
    #[gpui::test]
    fn full_mock_service_runs_from_the_keyboard(cx: &mut TestAppContext) {
        let started = Instant::now();
        let (store, console, h, _) = setup_session(cx, hymns_and_bibles, ServiceSession::default());
        console.update(cx, |c, cx| {
            let i = c.bible.bibles.iter().position(|b| b.abbreviation == "KJV");
            c.choose_bible(i, cx);
        });
        with_window(cx, h, |w, cx| w.render_frame(cx));

        // Prepare: search songs, ⇧⏎ adds the chosen one; the box keeps focus.
        press(cx, h, &["cmd-1"]);
        for words in ["amazing", "holy", "blessed", "castillo", "senor"] {
            retype(cx, h, words);
            press(cx, h, &["shift-enter"]);
        }
        // Passages: type a reference, ⇧⏎.
        press(cx, h, &["cmd-2"]);
        for reference in ["Jn 3:16-18", "Ps 23"] {
            retype(cx, h, reference);
            press(cx, h, &["shift-enter"]);
        }
        assert!(store.snapshot().slides.is_empty(), "preparing puts nothing live");
        assert_eq!(
            titles(cx, &console),
            [
                "Amazing Grace",
                "Holy, Holy, Holy",
                "Blessed Assurance",
                "Castillo Fuerte",
                "Cuán Grande Es Él",
                "John 3:16-18 (KJV)",
                "Psalms 23 (KJV)",
            ]
        );
        assert_eq!(cursor(cx, &console), Some(0), "the first item is cued");

        // Reorder: Esc to live control, cue Psalm 23 and move it up to third.
        press(cx, h, &["escape"]);
        press(cx, h, &["cmd-down"; 6]);
        assert_eq!(cursor(cx, &console), Some(6));
        press(cx, h, &["alt-up"; 4]);
        assert_eq!(titles(cx, &console)[2], "Psalms 23 (KJV)");
        assert_eq!(cursor(cx, &console), Some(2), "the cue follows the moved item");
        // A note for the first item: ⌘I, type, ⏎ back to live control.
        press(cx, h, &["cmd-up", "cmd-up"]);
        press(cx, h, &["cmd-i"]);
        with_window(cx, h, |w, cx| w.input("Slow intro", cx));
        press(cx, h, &["enter"]);
        assert_eq!(cx.update(|cx| console.read(cx).service.service.items[0].notes.clone()), "Slow intro");
        assert!(store.snapshot().slides.is_empty(), "⏎ in the notes field doesn't go live");

        // Run it: ⏎ puts the cued item live and cues the next.
        let mut slides = 0;
        for title in ["Amazing Grace", "Holy, Holy, Holy", "Psalms 23 (KJV)", "Blessed Assurance"] {
            slides += run_item(cx, h, &store, title);
        }
        // Blank, hide text, logo, back.
        press(cx, h, &["b"]);
        assert_eq!(store.snapshot().blank, Blank::Black);
        press(cx, h, &["b"]);
        assert_eq!(store.snapshot().blank, Blank::None);
        press(cx, h, &["shift-b"]);
        assert_eq!(store.snapshot().blank, Blank::Background);
        press(cx, h, &["escape"]);
        assert_eq!(store.snapshot().blank, Blank::None);

        // The preacher calls out a verse that isn't in the service.
        press(cx, h, &["cmd-2"]);
        retype(cx, h, "Jude 24-25");
        press(cx, h, &["enter", "enter"]);
        let live = store.snapshot();
        assert_eq!(live.title, "Jude 24-25 (KJV)");
        assert_eq!(cx.update(|cx| console.read(cx).service.live_key), None, "ad hoc, not a service item");
        press(cx, h, &["right"]);
        // …and the service carries on where it was.
        assert_eq!(cursor(cx, &console), Some(4));
        for title in ["Castillo Fuerte", "Cuán Grande Es Él", "John 3:16-18 (KJV)"] {
            slides += run_item(cx, h, &store, title);
        }
        press(cx, h, &["l"]);
        assert_eq!(store.snapshot().blank, Blank::Logo);
        press(cx, h, &["escape"]);
        assert_eq!(store.snapshot().blank, Blank::None);

        // The live pane knows which service item is on screen.
        let live_item = cx.update(|cx| {
            let s = &console.read(cx).service;
            s.live_key.and_then(|k| s.service.position(k))
        });
        assert_eq!(live_item, Some(6));
        eprintln!("mock service: 7 items, {slides} slides, 1 ad-hoc verse, run in {:?}", started.elapsed());
    }

    #[gpui::test]
    fn cue_moves_with_cmd_arrows_and_remove(cx: &mut TestAppContext) {
        let (store, console, h, _) = setup_session(cx, hymns_and_bibles, ServiceSession::default());
        press(cx, h, &["cmd-1"]);
        for words in ["amazing", "holy", "castillo"] {
            retype(cx, h, words);
            press(cx, h, &["shift-enter"]);
        }
        // Browsing the library shows the library song in the preview…
        assert_eq!(cx.update(|cx| console.read(cx).preview_source), PreviewSource::Library);
        // …Esc brings back the cued item.
        press(cx, h, &["escape"]);
        assert_eq!(cx.update(|cx| console.read(cx).preview_source), PreviewSource::Service);
        let resolved = |cx: &mut TestAppContext| {
            cx.update(|cx| console.read(cx).service.resolved.as_ref().map(|r| (r.title.clone(), r.slides.len())))
        };
        assert_eq!(resolved(cx), Some(("Amazing Grace".into(), 3)));

        press(cx, h, &["cmd-down", "cmd-down", "cmd-down"]);
        assert_eq!(cursor(cx, &console), Some(2), "clamped at the end");
        press(cx, h, &["cmd-up"]);
        assert_eq!(resolved(cx).unwrap().0, "Holy, Holy, Holy");

        // ⌘⌫ removes the cued item; the next one is cued.
        press(cx, h, &["cmd-backspace"]);
        assert_eq!(titles(cx, &console), ["Amazing Grace", "Castillo Fuerte"]);
        assert_eq!(resolved(cx).unwrap().0, "Castillo Fuerte");
        press(cx, h, &["cmd-backspace", "cmd-backspace"]);
        assert!(titles(cx, &console).is_empty());
        assert_eq!(cursor(cx, &console), None);
        // ⏎ with nothing cued does nothing.
        press(cx, h, &["enter"]);
        assert!(store.snapshot().slides.is_empty());
        assert!(cx.update(|cx| console.read(cx).service.dirty));
    }

    #[gpui::test]
    fn typing_never_edits_the_service(cx: &mut TestAppContext) {
        let (_, console, h, _) = setup_session(cx, hymns_and_bibles, ServiceSession::default());
        press(cx, h, &["cmd-1"]);
        retype(cx, h, "amazing");
        press(cx, h, &["shift-enter", "shift-enter"]);
        // In the search box these are text editing keys, not service commands.
        press(cx, h, &["alt-up", "cmd-backspace", "cmd-down", "l"]);
        assert_eq!(titles(cx, &console).len(), 2);
        assert_eq!(cursor(cx, &console), Some(0));
    }

    #[gpui::test]
    fn drag_to_reorder_and_drag_songs_in(cx: &mut TestAppContext) {
        let (_, console, h, _) = setup_session(cx, hymns_and_bibles, ServiceSession::default());
        press(cx, h, &["cmd-1"]);
        for words in ["amazing", "holy", "castillo"] {
            retype(cx, h, words);
            press(cx, h, &["shift-enter"]);
        }
        retype(cx, h, "");
        press(cx, h, &["escape"]);

        // Drag the first item onto the third: it takes that place.
        with_window(cx, h, |w, cx| w.drag_to(("service-item", 0usize), ("service-item", 2usize), cx));
        assert_eq!(titles(cx, &console), ["Holy, Holy, Holy", "Castillo Fuerte", "Amazing Grace"]);
        assert_eq!(cursor(cx, &console), Some(2), "the cued item stays cued");

        // Drag a library song (the list is alphabetical) onto the first item.
        let first_result = cx.update(|cx| console.read(cx).results[0].title.clone());
        with_window(cx, h, |w, cx| w.drag_to(("result", 0usize), ("service-item", 0usize), cx));
        assert_eq!(titles(cx, &console)[0], first_result);
        assert_eq!(titles(cx, &console).len(), 4);
    }

    #[gpui::test]
    fn missing_items_explain_themselves(cx: &mut TestAppContext) {
        let mut service = Service::default();
        service.push(ServiceItem::song(999, "Not Here"));
        service.push(ServiceItem::passage(77, "NVI", "Juan 3:16", Vec::new()));
        service.push(ServiceItem::song(1, "Blessed Assurance"));
        let session = ServiceSession { service, ..Default::default() };
        let (store, console, h, _) = setup_session(cx, |_| {}, session);
        with_window(cx, h, |w, cx| w.render_frame(cx));
        let why = |cx: &mut TestAppContext| {
            cx.update(|cx| match console.read(cx).service.resolved.as_ref().map(|r| &r.target) {
                Some(Target::Missing(why)) => Some(why.clone()),
                _ => None,
            })
        };
        assert_eq!(why(cx).as_deref(), Some("“Not Here” isn't in the song library."));
        press(cx, h, &["enter"]);
        assert!(store.snapshot().slides.is_empty(), "nothing goes live");
        assert_eq!(cursor(cx, &console), Some(0), "and the cue stays put");
        press(cx, h, &["cmd-down"]);
        assert_eq!(why(cx).as_deref(), Some("The NVI Bible isn't installed."));
        press(cx, h, &["cmd-down", "enter"]);
        assert_eq!(store.snapshot().title, "Blessed Assurance");
    }

    /// At the end of the service ⏎ leaves the last item where it is.
    #[gpui::test]
    fn enter_after_the_last_item_does_not_restart_it(cx: &mut TestAppContext) {
        let (store, _, h, _) = setup_session(cx, hymns_and_bibles, ServiceSession::default());
        press(cx, h, &["cmd-1"]);
        for words in ["holy", "amazing"] {
            retype(cx, h, words);
            press(cx, h, &["shift-enter"]);
        }
        press(cx, h, &["escape"]);
        press(cx, h, &["enter"]);
        run_item(cx, h, &store, "Amazing Grace");
        let at = store.snapshot();
        press(cx, h, &["enter"]);
        let now = store.snapshot();
        assert_eq!((now.title, now.current, now.revision), (at.title, at.current, at.revision));
        // Cueing an earlier item still works, and then the last one again.
        press(cx, h, &["cmd-up", "enter"]);
        assert_eq!(store.snapshot().title, "Holy, Holy, Holy");
        press(cx, h, &["enter"]);
        assert_eq!((store.snapshot().title.as_str(), store.snapshot().current), ("Amazing Grace", 0));
    }

    /// Songs are found by id and title together, so a rename in the editor
    /// must carry over to the service, while a different song under the
    /// same id is never shown.
    #[gpui::test]
    fn renamed_songs_stay_in_the_service(cx: &mut TestAppContext) {
        let (_, console, h, library) = setup_session(cx, hymns_and_bibles, ServiceSession::default());
        press(cx, h, &["cmd-1"]);
        retype(cx, h, "amazing");
        press(cx, h, &["shift-enter"]);
        press(cx, h, &["escape"]);
        let resolved = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                console.read(cx).service.resolved.as_ref().map(|r| (r.title.clone(), matches!(r.target, Target::Song(_))))
            })
        };
        let mut song = cx.update(|cx| match &console.read(cx).service.resolved.as_ref().unwrap().target {
            Target::Song(song) => (**song).clone(),
            _ => panic!("a song"),
        });
        song.title = "Amazing Grace (New Britain)".into();
        library.lock().unwrap().save(&song).unwrap();
        console.update(cx, |c, cx| c.song_saved(song.clone(), cx));
        assert_eq!(titles(cx, &console), ["Amazing Grace (New Britain)"]);
        assert_eq!(resolved(cx), Some(("Amazing Grace (New Britain)".into(), true)));

        // A service from elsewhere naming a song this library doesn't have,
        // under an id that exists here: missing, not the song with that id.
        let id = song.id.unwrap();
        console.update(cx, |c, cx| {
            c.service.service.push(ServiceItem::song(id, "Abide With Me"));
            c.service_changed(cx);
        });
        press(cx, h, &["cmd-down"]);
        assert_eq!(resolved(cx), Some(("Abide With Me".into(), false)));
    }

    #[gpui::test]
    fn save_and_open_through_the_file_dialogs(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (_, console, h, _) = setup_session(cx, hymns_and_bibles, ServiceSession::default());
        press(cx, h, &["cmd-1"]);
        retype(cx, h, "amazing");
        press(cx, h, &["shift-enter"]);
        press(cx, h, &["cmd-2"]);
        retype(cx, h, "Ps 23");
        press(cx, h, &["shift-enter", "escape", "cmd-i"]);
        with_window(cx, h, |w, cx| w.input("Read slowly", cx));
        // Theme override: the field under the notes.
        cx.update(|cx| {
            let theme = console.read(cx).service.theme.clone();
            h.update(cx, |_, window, cx| theme.update(cx, |s, cx| s.focus(window, cx))).unwrap();
        });
        with_window(cx, h, |w, cx| w.input("Scripture", cx));
        press(cx, h, &["escape"]);
        let before = cx.update(|cx| console.read(cx).service.service.clone());
        assert_eq!(before.items[0].notes, "Read slowly");
        assert_eq!(before.items[0].theme.as_deref(), Some("Scripture"));

        // ⌘S on a new service asks where; the extension is added.
        press(cx, h, &["cmd-s"]);
        cx.run_until_parked();
        let target = dir.path().join("Sunday 10am");
        cx.simulate_new_path_selection(|_| Some(target.clone()));
        cx.run_until_parked();
        let saved = dir.path().join("Sunday 10am.asaf-service");
        assert!(saved.exists());
        let state = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                let s = &console.read(cx).service;
                (s.service.display_name().to_string(), s.path.clone(), s.dirty)
            })
        };
        assert_eq!(state(cx), ("Sunday 10am".into(), Some(saved.clone()), false));

        // A change, then ⌘S saves in place without asking.
        press(cx, h, &["alt-down"]);
        assert!(state(cx).2);
        press(cx, h, &["cmd-s"]);
        cx.run_until_parked();
        assert!(!cx.did_prompt_for_new_path());
        assert!(!state(cx).2);
        let (on_disk, _) = asaf_library::service::read_service(&saved).unwrap();
        assert_eq!(on_disk.items.iter().map(ServiceItem::title).collect::<Vec<_>>(), ["Psalms 23 (KJV)", "Amazing Grace"]);
        assert_eq!(on_disk.items[1].notes, "Read slowly");
        assert_eq!(on_disk.items[1].theme.as_deref(), Some("Scripture"));
        let saved_service = cx.update(|cx| console.read(cx).service.service.clone());

        // New service (nothing unsaved, so no question), then open the file again.
        press(cx, h, &["cmd-shift-n"]);
        cx.run_until_parked();
        assert!(titles(cx, &console).is_empty());
        press(cx, h, &["cmd-o"]);
        cx.run_until_parked();
        cx.simulate_path_prompt_response(|_| Some(vec![saved.clone()]));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| console.read(cx).service.service.clone()), saved_service);
        assert_eq!(state(cx), ("Sunday 10am".into(), Some(saved.clone()), false));
        assert_eq!(cursor(cx, &console), Some(0));

        // Unsaved changes: opening asks first, and Cancel keeps them.
        press(cx, h, &["cmd-backspace", "cmd-o"]);
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert!(!cx.did_prompt_for_paths());
        assert_eq!(titles(cx, &console).len(), 1);

        // A file that isn't a service is refused and nothing changes.
        let bogus = dir.path().join("bogus.asaf-service");
        std::fs::write(&bogus, "{}").unwrap();
        cx.update(|cx| {
            h.update(cx, |_, window, cx| console.update(cx, |c, cx| c.open_service_file(bogus.clone(), window, cx)))
                .unwrap()
        });
        assert_eq!(titles(cx, &console).len(), 1);
    }

    fn autosave_on_disk(path: &Path) -> Option<Vec<String>> {
        path.exists().then(|| restore_autosave(path).service.items.iter().map(ServiceItem::title).collect())
    }

    #[gpui::test]
    fn autosave_is_debounced_and_restored_on_restart(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let autosave = dir.path().join(AUTOSAVE_FILE);
        let session = ServiceSession { autosave: Some(autosave.clone()), ..Default::default() };
        let (_, console, h, library) = setup_session(cx, hymns_and_bibles, session);

        press(cx, h, &["cmd-1"]);
        for words in ["amazing", "holy"] {
            retype(cx, h, words);
            press(cx, h, &["shift-enter"]);
        }
        cx.run_until_parked();
        assert_eq!(autosave_on_disk(&autosave), None, "not written while changes keep coming");
        cx.executor().advance_clock(AUTOSAVE_DELAY / 2);
        retype(cx, h, "castillo");
        press(cx, h, &["shift-enter"]);
        cx.executor().advance_clock(AUTOSAVE_DELAY / 2);
        cx.run_until_parked();
        assert_eq!(autosave_on_disk(&autosave), None, "each change restarts the wait");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();
        assert_eq!(
            autosave_on_disk(&autosave).unwrap(),
            ["Amazing Grace", "Holy, Holy, Holy", "Castillo Fuerte"]
        );
        // Notes are autosaved too.
        press(cx, h, &["escape", "cmd-down", "cmd-i"]);
        with_window(cx, h, |w, cx| w.input("Key of G", cx));
        cx.executor().advance_clock(AUTOSAVE_DELAY * 2);
        cx.run_until_parked();
        let restored = restore_autosave(&autosave);
        assert_eq!(restored.service.items[1].notes, "Key of G");
        assert!(restored.info.dirty, "never saved to a file");
        let before = cx.update(|cx| console.read(cx).service.service.clone());

        // "Crash": close the window and start again from what's on disk, the
        // way the app does.
        cx.update(|cx| h.update(cx, |_, window, _| window.remove_window()).unwrap());
        let restored = restore_autosave(&autosave);
        let session = ServiceSession {
            service: restored.service,
            path: restored.info.path,
            dirty: restored.info.dirty,
            autosave: Some(autosave.clone()),
            notice: restored.problem,
        };
        let (store, console, h) = open_console(cx, library, session);
        with_window(cx, h, |w, cx| w.render_frame(cx));
        let after = cx.update(|cx| console.read(cx).service.service.clone());
        assert_eq!(after, before);
        assert!(cx.update(|cx| console.read(cx).service.dirty));
        assert_eq!(cursor(cx, &console), Some(0));
        // It runs straight away.
        press(cx, h, &["enter"]);
        assert_eq!(store.snapshot().title, "Amazing Grace");
    }

    /// Closing the console (which quits the app) within the autosave delay
    /// drops the waiting autosave; the last change must still be written.
    #[gpui::test]
    fn dropping_the_console_flushes_a_pending_autosave(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let autosave = dir.path().join(AUTOSAVE_FILE);
        let (store, _, h, library) = setup_session(cx, hymns_and_bibles, ServiceSession::default());
        // A second console in the same window that nothing else holds, so
        // dropping it releases it the way closing its window does.
        let session = ServiceSession { autosave: Some(autosave.clone()), ..Default::default() };
        let console = with_window(cx, h, |window, cx| {
            cx.new(|cx| Console::new(store, library, "http://test/display".into(), session, window, cx))
        });
        with_window(cx, h, |window, cx| {
            console.update(cx, |c, cx| c.add_to_service(ServiceItem::song(1, "Amazing Grace"), window, cx))
        });
        cx.run_until_parked();
        assert_eq!(autosave_on_disk(&autosave), None, "still waiting");

        let released = std::rc::Rc::new(std::cell::Cell::new(false));
        let r = released.clone();
        cx.update(|cx| cx.observe_release(&console, move |_, _| r.set(true)).detach());
        drop(console);
        cx.update(|_| {});
        cx.run_until_parked();
        assert!(released.get());
        assert_eq!(autosave_on_disk(&autosave).unwrap(), ["Amazing Grace"]);
    }
}
