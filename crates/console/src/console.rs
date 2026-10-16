use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;
use asaf_core::{Blank, Command, LiveSnapshot, ServiceItem, Song, Store, Theme};
use asaf_library::SongSummary;
use asaf_projector::{EmbeddedPage, Projector};
use gpui::component::button::{Button, ButtonVariants as _};
use gpui::component::input::{Input, InputEvent, InputState};
use gpui::component::{ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui::component::notification::Notification;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, TestSupportExt as _, TitlebarOptions, UniformListScrollHandle, Window,
    WindowBounds,
    WindowOptions, canvas, div, px, size, uniform_list,
};

use crate::bible::BibleState;
use crate::editor::{SongEditor, SongSaved};
use crate::i18n::{UiLanguage, lang};
use crate::icons::IconName;
use crate::settings::{LibraryChanged, SettingsChanged, SettingsWindow};
use crate::theme_editor::{ThemeEditor, ThemesChanged};
use crate::service::{DragGhost, DraggedSong, ServiceSession, ServiceState, restore_problem_notice};
use crate::preview::{image_source, screen};
use crate::slides::slide_card;
use crate::{
    BlankBackground, BlankBlack, BlankLogo, EditNotes, EditSong, FocusSearch, GoLive, ImportOpenLp, LeaveSearch,
    MoveItemDown, MoveItemUp, NewService, NewSong, Next, NextItem, OpenService, OpenSettings, OpenThemes, Previous,
    PreviousItem, RemoveItem, SaveService, SaveServiceAs, SelectNext, SelectPrevious, SharedLibrary, ShowBible,
    ShowSongs, Unblank, transfer,
};

const RESULT_LIMIT: usize = 1000;

/// What the source panel (left column) shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Songs,
    Bible,
}

/// What the preview pane shows: the cued service item, or whatever is
/// chosen in the source panel while you're browsing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewSource {
    Service,
    Library,
}

pub struct Console {
    pub(crate) store: Arc<Store>,
    pub(crate) library: SharedLibrary,
    display_url: SharedString,
    /// The display page in the live pane, showing exactly what's projected,
    /// when a webview could be made.
    monitor: Option<Rc<EmbeddedPage>>,
    pub(crate) focus: FocusHandle,
    pub(crate) mode: Mode,
    pub(crate) bible: BibleState,
    pub(crate) service: ServiceState,
    pub(crate) preview_source: PreviewSource,
    search: Entity<InputState>,
    pub(crate) results: Vec<SongSummary>,
    results_scroll: UniformListScrollHandle,
    selected: Option<usize>,
    /// The song chosen in the library, loaded in full.
    pub(crate) preview: Option<Song>,
    /// Open editor windows, by song id (`None` for new songs).
    editors: Vec<(Option<i64>, AnyWindowHandle)>,
    /// The library's themes, for per-item themes.
    pub(crate) themes: Vec<Theme>,
    pub(crate) theme_editor: Option<(AnyWindowHandle, Entity<ThemeEditor>)>,
    pub(crate) settings_window: Option<AnyWindowHandle>,
    live_scroll: ScrollHandle,
    /// The live item and slide the live pane last scrolled to.
    live_scrolled_to: Option<(String, usize)>,
    /// Where phones reach the remote page, when it's open to the LAN.
    pub(crate) remote_url: Option<SharedString>,
    pub(crate) _subscriptions: Vec<Subscription>,
    /// Running remote commands, and redrawing when the store changes elsewhere.
    pub(crate) _tasks: Vec<Task<()>>,
}

impl Console {
    pub fn open(
        store: Arc<Store>,
        library: SharedLibrary,
        display_url: String,
        session: ServiceSession,
        cx: &mut App,
    ) -> Result<Entity<Self>> {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1360.), px(820.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some("Asaf".into()), ..Default::default() }),
            window_min_size: Some(size(px(1080.), px(560.))),
            ..Default::default()
        };
        let (handle, console) = gpui::open_window(options, cx, |window, cx| {
            cx.new(|cx| Console::new(store, library, display_url, session, window, cx))
        })?;
        handle.update(cx, |_, window, cx| {
            window.activate_window();
            console.read(cx).focus.clone().focus(window, cx);
        })?;
        Ok(console)
    }

    pub(crate) fn new(
        store: Arc<Store>,
        library: SharedLibrary,
        display_url: String,
        mut session: ServiceSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The console speaks the language and has the appearance from the
        // settings, and the projector starts in the library's default theme.
        let settings = library.lock().unwrap().settings().unwrap_or_default();
        cx.set_global(UiLanguage(settings.language));
        crate::set_menus(cx);
        crate::set_appearance(settings.appearance, cx);
        if let Ok(theme) = library.lock().unwrap().default_theme() {
            store.set_default_theme(theme);
        }
        let l = lang(cx);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(l.t("Search title, lyrics or author")));
        let bible = BibleState::new(window, cx);
        let service = ServiceState::new(&mut session, window, cx);
        let mut subscriptions = vec![
            cx.subscribe_in(&bible.input, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.preview_source = PreviewSource::Library;
                    this.bible_text_changed(cx)
                }
                InputEvent::PressEnter { shift: true, .. } => this.add_bible_to_service(window, cx),
                InputEvent::PressEnter { secondary, .. } => this.bible_enter(*secondary, window, cx),
                _ => {}
            }),
            cx.subscribe_in(&search, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.preview_source = PreviewSource::Library;
                    this.refresh_search(cx)
                }
                InputEvent::PressEnter { shift: true, .. } => this.add_song_to_service(window, cx),
                InputEvent::PressEnter { .. } => this.go_live(0, window, cx),
                _ => {}
            }),
            cx.observe_global::<Projector>(|_, cx| cx.notify()),
            cx.observe_global_in::<UiLanguage>(window, |this, window, cx| this.language_changed(window, cx)),
            // macOS switched between light and dark.
            cx.observe_window_appearance(window, |_, window, cx| crate::appearance::system_appearance_changed(window, cx)),
            // Don't lose the last half second of changes on ⌘Q.
            cx.on_app_quit(|this, _| {
                this.service.flush_autosave();
                async {}
            }),
        ];
        subscriptions.extend(service.subscriptions(window, cx));
        // Notifications need the window's root, which exists once this view does.
        let note = match session.restore_problem.take() {
            Some(problem) => Some(Notification::warning(restore_problem_notice(l, &problem))),
            None if !service.service.items.is_empty() => Some(Notification::info(
                l.f("Restored “{name}” from the last session", &[("name", service.service.display_name())]),
            )),
            None => None,
        };
        if let Some(note) = note {
            cx.defer_in(window, move |_, window, cx| window.push_notification(note, cx));
        }
        let preview_source =
            if service.service.items.is_empty() { PreviewSource::Library } else { PreviewSource::Service };
        let monitor = match EmbeddedPage::new(&format!("{display_url}?monitor"), window) {
            Ok(page) => Some(Rc::new(page)),
            Err(e) => {
                eprintln!("asaf: no webview for the live monitor: {e:#}");
                None
            }
        };
        let mut console = Self {
            store,
            library,
            display_url: display_url.into(),
            monitor,
            focus: cx.focus_handle(),
            mode: Mode::Songs,
            bible,
            service,
            preview_source,
            search,
            results: Vec::new(),
            results_scroll: UniformListScrollHandle::new(),
            selected: None,
            preview: None,
            editors: Vec::new(),
            themes: Vec::new(),
            theme_editor: None,
            settings_window: None,
            live_scroll: ScrollHandle::new(),
            live_scrolled_to: None,
            remote_url: None,
            _subscriptions: subscriptions,
            _tasks: Vec::new(),
        };
        console.serve_remote(cx);
        console.refresh_themes(cx);
        console.refresh_search(cx);
        console.refresh_bibles(None, cx);
        console
    }

    // ---- themes and settings ------------------------------------------------------

    pub(crate) fn refresh_themes(&mut self, cx: &mut Context<Self>) {
        let lib = self.library.lock().unwrap();
        self.themes = lib.themes().unwrap_or_default();
        if let Ok(theme) = lib.default_theme() {
            self.store.set_default_theme(theme);
        }
        drop(lib);
        cx.notify();
    }

    /// A service item's own theme, if it names one the library has.
    pub(crate) fn item_theme(&self, item: &ServiceItem) -> Option<Theme> {
        let name = item.theme.as_deref()?.trim();
        self.themes.iter().find(|t| t.name.eq_ignore_ascii_case(name)).cloned()
    }

    /// How a service item will look: its own theme, else the default.
    pub(crate) fn look_of(&self, item: Option<&ServiceItem>) -> Theme {
        item.and_then(|i| self.item_theme(i)).unwrap_or_else(|| self.store.default_theme())
    }

    /// Themes were edited: follow renames and deletions into the service,
    /// and update the projector if the theme on screen changed.
    pub(crate) fn themes_changed(&mut self, change: &ThemesChanged, cx: &mut Context<Self>) {
        self.refresh_themes(cx);
        let service_changed = match (&change.renamed, &change.deleted) {
            (Some((old, new)), _) => self.service.service.theme_renamed(old, Some(new)),
            (None, Some(deleted)) => self.service.service.theme_renamed(deleted, None),
            _ => false,
        };
        if service_changed {
            self.service.inputs_key = None; // show the new name in the theme field
            self.service_changed(cx);
        }
        let live_item = self.service.live_key.and_then(|k| self.service.service.items.iter().find(|i| i.key == k)).cloned();
        self.store.set_item_theme(live_item.and_then(|i| self.item_theme(&i)));
        self.resolve_cue(cx);
    }

    pub(crate) fn open_theme_editor(&mut self, cx: &mut Context<Self>) {
        if let Some((h, _)) = &self.theme_editor
            && h.update(cx, |_, window, _| window.activate_window()).is_ok()
        {
            return;
        }
        // Start on the theme of the cued item, if it has one.
        let select = self.service.cued().and_then(|i| self.item_theme(i)).map(|t| t.name);
        match ThemeEditor::open(self.library.clone(), select, Some(&self.display_url), cx) {
            Ok((handle, editor)) => {
                self.theme_editor = Some((handle, editor.clone()));
                self._subscriptions.push(cx.subscribe(&editor, |this, _, change: &ThemesChanged, cx| {
                    this.themes_changed(change, cx)
                }));
            }
            Err(e) => eprintln!("asaf: can't open the theme editor: {e:#}"),
        }
    }

    pub(crate) fn open_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(h) = self.settings_window
            && h.update(cx, |_, window, _| window.activate_window()).is_ok()
        {
            return;
        }
        match SettingsWindow::open(self.library.clone(), cx) {
            Ok((handle, view)) => {
                self.settings_window = Some(handle);
                self._subscriptions.push(cx.subscribe(&view, |this, _, change: &SettingsChanged, cx| {
                    this.themes_changed(&ThemesChanged::default(), cx);
                    cx.emit(change.clone()); // the app applies the remote's PIN
                }));
                self._subscriptions.push(cx.subscribe(&view, |this, _, _: &LibraryChanged, cx| this.library_changed(cx)));
            }
            Err(e) => eprintln!("asaf: can't open settings: {e:#}"),
        }
    }

    /// Songs or Bibles arrived (an import): refresh everything that lists them.
    pub(crate) fn library_changed(&mut self, cx: &mut Context<Self>) {
        self.refresh_search(cx);
        self.refresh_bibles(None, cx); // also re-resolves the cued item
    }

    /// The operator picked a projector display: remember it for next time.
    fn display_chosen(&mut self, cx: &mut Context<Self>) {
        let uuid = Projector::preferred_uuid(cx);
        let mut lib = self.library.lock().unwrap();
        let result = lib.settings().and_then(|mut s| {
            if s.display_uuid == uuid {
                return Ok(());
            }
            s.display_uuid = uuid;
            lib.save_settings(&s)
        });
        if let Err(e) = result {
            eprintln!("asaf: couldn't save the display choice: {e}");
        }
    }

    /// Switches the source panel and puts the cursor in its search box.
    pub(crate) fn show_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.preview_source = PreviewSource::Library;
        let input = match mode {
            Mode::Songs => &self.search,
            Mode::Bible => &self.bible.input,
        };
        input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// Esc in a text field: back to live control, the preview back to the cue.
    fn leave_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        if !self.service.service.items.is_empty() {
            self.preview_source = PreviewSource::Service;
        }
        cx.notify();
    }

    pub(crate) fn library(&self) -> &SharedLibrary {
        &self.library
    }

    /// The console's language changed: re-word what isn't drawn each frame
    /// (placeholders, why the cued item can't be shown).
    fn language_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let l = lang(cx);
        let placeholders = [
            (&self.search, l.t("Search title, lyrics or author")),
            (&self.service.notes, l.t("Notes for the operator")),
            (&self.service.theme, l.t("Default theme")),
        ];
        for (input, text) in placeholders {
            input.update(cx, |s, cx| s.set_placeholder(text, window, cx));
        }
        self.resolve_cue(cx);
    }

    /// Re-runs the search, keeping the selected song selected if it's still listed.
    pub(crate) fn refresh_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value();
        let keep = self.selected_id();
        match self.library.lock().unwrap().search(&query, RESULT_LIMIT) {
            Ok(results) => self.results = results,
            Err(e) => eprintln!("asaf: search failed: {e}"),
        }
        let index = keep
            .and_then(|id| self.results.iter().position(|r| r.id == id))
            .or((!self.results.is_empty()).then_some(0));
        self.select(index, cx);
    }

    fn selected_id(&self) -> Option<i64> {
        self.selected.and_then(|i| self.results.get(i)).map(|r| r.id)
    }

    fn select(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.selected = index.filter(|&i| i < self.results.len());
        self.preview = self
            .selected_id()
            .and_then(|id| self.library.lock().unwrap().get(id).ok().flatten());
        if let Some(i) = self.selected {
            self.results_scroll.scroll_to_item(i, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        let i = self.selected.map_or(0, |i| i as isize + delta);
        self.select(Some(i.clamp(0, self.results.len() as isize - 1) as usize), cx);
    }

    /// Puts the song chosen in the library live at `index` (ad hoc, outside
    /// the service), and returns focus to live control.
    fn go_live(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(song) = &self.preview {
            self.store.load_song(song.clone(), index);
            self.focus.focus(window, cx);
            self.went_live_ad_hoc(cx);
        }
    }

    /// ⏎ in live control: puts whatever the preview shows live.
    fn go_live_previewed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match (self.preview_source, self.mode) {
            (PreviewSource::Service, _) => self.go_live_next(window, cx),
            (PreviewSource::Library, Mode::Songs) => self.go_live(0, window, cx),
            (PreviewSource::Library, Mode::Bible) => self.bible_go_live(0, window, cx),
        }
    }

    /// Runs a slide or blank command through the store, as the remote does.
    fn command(&mut self, cmd: Command, cx: &mut Context<Self>) {
        self.store.command(cmd, |_| {});
        cx.notify();
    }

    fn edit(&mut self, song: Song, window: &mut Window, cx: &mut Context<Self>) {
        self.editors.retain(|(_, w)| w.update(cx, |_, _, _| ()).is_ok());
        if let Some((_, w)) = self.editors.iter().find(|(id, _)| song.id.is_some() && *id == song.id) {
            let _ = w.update(cx, |_, window, _| window.activate_window());
            return;
        }
        match SongEditor::open(song.clone(), self.library.clone(), cx) {
            Ok((handle, editor)) => {
                self.editors.push((song.id, handle));
                self._subscriptions.push(cx.subscribe(&editor, |this, _, SongSaved(song), cx| {
                    this.song_saved(song.clone(), cx)
                }));
            }
            Err(e) => {
                let note = lang(cx).f("Can't open the song editor: {error}", &[("error", &format!("{e:#}"))]);
                window.push_notification(Notification::error(note), cx)
            }
        }
    }

    /// The song being edited: the cued service song when the preview shows
    /// it, else the one chosen in the library.
    fn song_to_edit(&self) -> Option<Song> {
        if self.preview_source == PreviewSource::Service
            && let Some(crate::service::Target::Song(song)) = self.service.resolved.as_ref().map(|r| &r.target)
        {
            return Some((**song).clone());
        }
        self.preview.clone()
    }

    pub(crate) fn song_saved(&mut self, song: Song, cx: &mut Context<Self>) {
        self.store.song_edited(&song);
        if let Some(id) = song.id
            && self.service.service.song_renamed(id, &song.title)
        {
            self.service_changed(cx);
        }
        self.resolve_cue(cx);
        // Select the saved song (new songs too), then refresh the list.
        if let Some(id) = song.id {
            let query = self.search.read(cx).value();
            if let Ok(results) = self.library.lock().unwrap().search(&query, RESULT_LIMIT) {
                self.results = results;
            }
            self.selected = self.results.iter().position(|r| r.id == id);
        }
        self.refresh_search(cx);
    }
}

/// Settings changed in the settings window, passed on for the app.
impl EventEmitter<SettingsChanged> for Console {}

impl Focusable for Console {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Console {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_item_fields(window, cx);
        let live = self.store.snapshot();
        let theme = cx.theme();

        div()
            .key_context("Console")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Next, _, cx| this.command(Command::Next, cx)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.command(Command::Previous, cx)))
            .on_action(cx.listener(|this, _: &BlankBlack, _, cx| this.command(Command::ToggleBlank { mode: Blank::Black }, cx)))
            .on_action(cx.listener(|this, _: &BlankBackground, _, cx| this.command(Command::ToggleBlank { mode: Blank::Background }, cx)))
            .on_action(cx.listener(|this, _: &BlankLogo, _, cx| this.command(Command::ToggleBlank { mode: Blank::Logo }, cx)))
            .on_action(cx.listener(|this, _: &Unblank, _, cx| this.command(Command::Blank { mode: Blank::None }, cx)))
            .on_action(cx.listener(|this, _: &GoLive, window, cx| this.go_live_previewed(window, cx)))
            .on_action(cx.listener(|this, _: &NextItem, _, cx| this.move_cue(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousItem, _, cx| this.move_cue(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveItemDown, _, cx| this.move_cued_item(1, cx)))
            .on_action(cx.listener(|this, _: &MoveItemUp, _, cx| this.move_cued_item(-1, cx)))
            .on_action(cx.listener(|this, _: &RemoveItem, _, cx| this.remove_cued_item(cx)))
            .on_action(cx.listener(|this, _: &EditNotes, window, cx| this.edit_notes(window, cx)))
            .on_action(cx.listener(|this, _: &NewService, window, cx| this.new_service(window, cx)))
            .on_action(cx.listener(|this, _: &OpenService, window, cx| this.open_service(window, cx)))
            .on_action(cx.listener(|this, _: &SaveService, window, cx| this.save_service(false, window, cx)))
            .on_action(cx.listener(|this, _: &SaveServiceAs, window, cx| this.save_service(true, window, cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| this.show_mode(this.mode, window, cx)))
            .on_action(cx.listener(|this, _: &ShowSongs, window, cx| this.show_mode(Mode::Songs, window, cx)))
            .on_action(cx.listener(|this, _: &ShowBible, window, cx| this.show_mode(Mode::Bible, window, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| {
                this.preview_source = PreviewSource::Library;
                match this.mode {
                    Mode::Songs => this.move_selection(1, cx),
                    Mode::Bible => this.move_hit(1, cx),
                }
            }))
            .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| {
                this.preview_source = PreviewSource::Library;
                match this.mode {
                    Mode::Songs => this.move_selection(-1, cx),
                    Mode::Bible => this.move_hit(-1, cx),
                }
            }))
            .on_action(cx.listener(|this, _: &LeaveSearch, window, cx| this.leave_search(window, cx)))
            .on_action(cx.listener(|this, _: &NewSong, window, cx| this.edit(Song::default(), window, cx)))
            .on_action(cx.listener(|this, _: &OpenThemes, _, cx| this.open_theme_editor(cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| this.open_settings(cx)))
            .on_action(cx.listener(|this, _: &ImportOpenLp, window, cx| {
                transfer::import_openlp(this.library.clone(), window, cx, |this, cx| this.library_changed(cx))
            }))
            .on_action(cx.listener(|this, _: &EditSong, window, cx| {
                if let Some(song) = this.song_to_edit() {
                    this.edit(song, window, cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.left_column(window, cx))
                    .child(self.service_column(cx))
                    .child(match (self.preview_source, self.mode) {
                        (PreviewSource::Service, _) => self.cue_preview_column(cx).into_any_element(),
                        (PreviewSource::Library, Mode::Songs) => self.preview_column(&live, cx).into_any_element(),
                        (PreviewSource::Library, Mode::Bible) => {
                            self.passage_preview_column(&live, cx).into_any_element()
                        }
                    })
                    .child(self.live_column(&live, window, cx)),
            )
            .child(self.controls(&live, cx))
    }
}

impl Console {
    fn left_column(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let theme = cx.theme();
        let tab = |id: &'static str, icon: IconName, label: &'static str, mode: Mode, tip: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .icon(icon)
                .small()
                .ghost()
                .label(label)
                .tooltip(tip)
                .selected(self.mode == mode)
                .on_click(cx.listener(move |this, _, window, cx| this.show_mode(mode, window, cx)))
        };
        v_flex()
            .w(px(300.))
            .flex_none()
            .h_full()
            .border_r_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_1()
                    .px_3()
                    .pt_3()
                    .child(tab("tab-songs", IconName::Music, l.t("Songs"), Mode::Songs, l.t("Songs (⌘1)"), cx))
                    .child(tab("tab-bible", IconName::BookOpen, l.t("Bible"), Mode::Bible, l.t("Bible (⌘2)"), cx)),
            )
            .child(match self.mode {
                Mode::Songs => self.library_column(window, cx).into_any_element(),
                Mode::Bible => self.bible_column(cx).into_any_element(),
            })
    }

    fn library_column(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let theme = cx.theme();
        let count = self.results.len();
        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().flex_1().font_weight(FontWeight::BOLD).child(l.t("Songs")))
                            .child(
                                Button::new("new-song")
                                    .icon(IconName::Plus)
                                    .small()
                                    .label(l.t("New"))
                                    .tooltip(l.t("New song (⌘N)"))
                                    .on_click(cx.listener(|this, _, window, cx| this.edit(Song::default(), window, cx))),
                            )
                            .child(
                                Button::new("import")
                                    .icon(IconName::Download)
                                    .small()
                                    .tooltip(l.t("Import OpenLyrics files or folders"))
                                    .on_click(cx.listener(|_, _, window, cx| transfer::import(window, cx))),
                            )
                            .child(
                                Button::new("export-all")
                                    .icon(IconName::Upload)
                                    .small()
                                    .tooltip(l.t("Export every song as OpenLyrics into a folder"))
                                    .on_click(cx.listener(|_, _, window, cx| transfer::export_all(window, cx))),
                            ),
                    )
                    .child(div().key_context("SongSearch").child(Input::new(&self.search).prefix(Icon::new(IconName::Search).text_color(theme.muted_foreground)).cleanable(true))),
            )
            .child(
                uniform_list(
                    "results",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                        let theme = cx.theme();
                        range
                            .map(|i| {
                                let r = &this.results[i];
                                let selected = this.selected == Some(i);
                                let excerpt = if r.authors.is_empty() {
                                    r.excerpt.clone()
                                } else {
                                    format!("{} · {}", r.authors, r.excerpt)
                                };
                                let dragged = DraggedSong { id: r.id, title: r.title.clone().into() };
                                v_flex()
                                    .id(("result", i))
                                    .px_3()
                                    .py_1p5()
                                    .h(px(48.))
                                    .justify_center()
                                    .cursor_pointer()
                                    .when(selected, |d| d.bg(theme.list_active))
                                    .when(!selected, |d| d.hover(|s| s.bg(theme.list_hover)))
                                    .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
                                        this.preview_source = PreviewSource::Library;
                                        this.select(Some(i), cx);
                                        if ev.click_count() >= 2
                                            && let Some(song) = this.preview.clone()
                                        {
                                            this.edit(song, window, cx);
                                        }
                                    }))
                                    .on_drag(dragged, |d, _, _, cx| cx.new(|_| DragGhost(d.title.clone())))
                                    .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(r.title.clone()))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(excerpt),
                                    )
                                    .test_support()
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.results_scroll)
                .flex_1(),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{}  ·  {}",
                        if count == RESULT_LIMIT {
                            l.f("{count}+ songs", &[("count", &count.to_string())])
                        } else {
                            l.n(count as u64, "{count} song", "{count} songs")
                        },
                        l.t("⏎ go live, ⇧⏎ add to service")
                    )),
            )
    }

    fn preview_column(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let theme = cx.theme();
        let Some(song) = &self.preview else {
            return v_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child(if self.results.is_empty() { l.t("No songs found.") } else { l.t("Select a song.") })
                .into_any_element();
        };
        let slides = song.slides();
        let look = self.store.default_theme();
        let is_live_song = song.id.is_some() && live.song_id == song.id;
        let subtitle = song.authors.join(", ");

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
                                    .child(l.t("PREVIEW · LIBRARY")),
                            )
                            .child(div().truncate().text_base().font_weight(FontWeight::BOLD).child(song.title.clone()))
                            .child(div().truncate().text_xs().text_color(theme.muted_foreground).child(subtitle)),
                    )
                    .child(Button::new("export-song").icon(IconName::Upload).small().ghost().label(l.t("Export…")).on_click(cx.listener(
                        |this, _, window, cx| {
                            if let Some(song) = this.preview.clone() {
                                transfer::export_song(song, window, cx);
                            }
                        },
                    )))
                    .child(Button::new("edit-song").icon(IconName::Pencil).small().ghost().label(l.t("Edit")).tooltip(l.t("Edit song (⌘E)")).on_click(
                        cx.listener(|this, _, window, cx| {
                            if let Some(song) = this.preview.clone() {
                                this.edit(song, window, cx);
                            }
                        }),
                    ))
                    .child(
                        Button::new("add-song")
                            .icon(IconName::ListPlus)
                            .small()
                            .label(l.t("Add"))
                            .tooltip(l.t("Add to the service (⇧⏎ in search)"))
                            .on_click(cx.listener(|this, _, window, cx| this.add_song_to_service(window, cx))),
                    )
                    .child(
                        Button::new("go-live")
                            .icon(IconName::Play)
                            .small()
                            .primary()
                            .label(l.t("Go live"))
                            .tooltip(l.t("Show the first slide (⏎ in search)"))
                            .on_click(cx.listener(|this, _, window, cx| this.go_live(0, window, cx))),
                    ),
            )
            .child(
                v_flex()
                    .id("preview-slides")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_3()
                    .gap_2()
                    .when(slides.is_empty(), |d| {
                        d.child(div().text_color(theme.muted_foreground).child(l.t("This song has no slides. Check its verse order.")))
                    })
                    .children(slides.iter().enumerate().map(|(i, slide)| {
                        slide_card(("slide", i), slide, is_live_song && i == live.current, &look, cx)
                            .on_click(cx.listener(move |this, _, window, cx| this.go_live(i, window, cx)))
                    })),
            )
            .into_any_element()
    }

    /// The live monitor: the display page, connected like the projector. It
    /// covers whatever GPUI draws over it, so it steps aside for
    /// notifications and dialogs, and the console's approximation of the
    /// output shows meanwhile.
    fn live_monitor(&self, live: &LiveSnapshot, look: &Theme, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let page = self.monitor.clone()?;
        let covered = !window.notifications(cx).is_empty() || window.has_active_dialog(cx);
        page.set_visible(!covered);
        let stand_in = page.bounds().filter(|_| covered).map(|b| {
            let slide = live.slides.get(live.current).filter(|_| live.blank == Blank::None);
            let background = image_source(look.background_image.as_deref(), &self.library);
            match live.blank {
                Blank::Black => div().size_full().into_any_element(),
                _ => screen(look, slide, background, b.size.width).into_any_element(),
            }
        });
        Some(
            div()
                .px_3()
                .pt_3()
                .child(
                    div()
                        .relative()
                        .w_full()
                        .aspect_ratio(16. / 9.)
                        .bg(gpui::black())
                        .children(stand_in)
                        .child(canvas(move |bounds, _, _| page.set_bounds(bounds), |_, _, _, _| {}).absolute().inset_0()),
                )
                .into_any_element(),
        )
    }

    fn live_column(&mut self, live: &LiveSnapshot, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keep the slide on screen in view as the operator steps through.
        let at = (live.title.clone(), live.current);
        if self.live_scrolled_to.as_ref() != Some(&at) {
            self.live_scroll.scroll_to_item(live.current);
            self.live_scrolled_to = Some(at);
        }
        let look = self.store.theme();
        let monitor = self.live_monitor(live, &look, window, cx);
        let l = lang(cx);
        let theme = cx.theme();
        let (blank_text, blank_color) = match live.blank {
            Blank::None => (l.t("Showing text"), theme.success),
            Blank::Black => (l.t("Blanked to black (B)"), theme.danger),
            Blank::Background => (l.t("Text hidden, background only (⇧B)"), theme.danger),
            Blank::Logo => (l.t("Showing the logo (L)"), theme.danger),
        };
        let from_service = self
            .service
            .live_key
            .and_then(|k| self.service.service.position(k))
            .map(|i| {
                let (number, count) = ((i + 1).to_string(), self.service.service.items.len().to_string());
                format!("{} · ", l.f("Item {number} of {count}", &[("number", &number), ("count", &count)]))
            })
            .unwrap_or_else(|| if live.slides.is_empty() { String::new() } else { format!("{} · ", l.t("Not in the service")) });

        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .border_l_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .gap_0p5()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .child(div().text_color(theme.danger).child(l.t("● LIVE")))
                            .child(div().flex_1())
                            .child(div().text_color(blank_color).child(blank_text)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .child(if live.slides.is_empty() { l.t("Nothing live yet.").to_string() } else { live.title.clone() }),
                    )
                    .child(div().truncate().text_xs().text_color(theme.muted_foreground).child(
                        match live.slides.get(live.current) {
                            Some(slide) => {
                                let (number, count) = ((live.current + 1).to_string(), live.slides.len().to_string());
                                format!(
                                    "{from_service}{} · {} · {}",
                                    slide.label,
                                    l.f("{number} of {count}", &[("number", &number), ("count", &count)]),
                                    l.f("Theme {theme}", &[("theme", &live.theme_name)])
                                )
                            }
                            None => String::new(),
                        },
                    )),
            )
            .children(monitor)
            .child(
                v_flex()
                    .id("live-slides")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.live_scroll)
                    .p_3()
                    .gap_2()
                    .children(live.slides.iter().enumerate().map(|(i, slide)| {
                        slide_card(("live-slide", i), slide, i == live.current, &look, cx)
                            .on_click(cx.listener(move |this, _, _, cx| this.command(Command::Goto { slide: i }, cx)))
                    })),
            )
            .child(
                v_flex()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(section(l.t("Projector display"), cx).child(h_flex().flex_wrap().gap_1().children(
                        Projector::displays(cx).into_iter().enumerate().map(|(i, d)| {
                            let id = d.id;
                            Button::new(("display", i))
                                .xsmall()
                                .label(d.label)
                                .selected(d.is_projector)
                                .when(d.is_projector, |b| b.primary())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    Projector::choose(id, cx);
                                    this.display_chosen(cx);
                                    cx.notify();
                                }))
                        }),
                    )))
                    .child(
                        v_flex()
                            .gap_0p5()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(l.t("→ ↓ Space next  ·  ← ↑ previous  ·  ⏎ cued item live"))
                            .child(l.t("B black  ·  ⇧B hide text  ·  L logo  ·  Esc show text"))
                            .child(l.t("⌘1 songs  ·  ⌘2 Bible  ·  ⌘E edit  ·  ⌘I notes  ·  ⌘S save"))
                            .when_some(self.remote_url.clone(), |d, url| {
                                d.child(div().id("remote-url").truncate().child(format!("{} {url}", l.t("Remote for phones:"))).test_support())
                            })
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(div().flex_1().min_w_0().truncate().child(self.display_url.clone()))
                                    .child(
                                        Button::new("open-themes")
                                            .icon(IconName::Palette)
                                            .xsmall()
                                            .ghost()
                                            .label(l.t("Themes…"))
                                            .tooltip(l.t("Edit themes (⌘T)"))
                                            .on_click(cx.listener(|this, _, _, cx| this.open_theme_editor(cx))),
                                    )
                                    .child(
                                        Button::new("open-settings")
                                            .icon(IconName::Settings)
                                            .xsmall()
                                            .ghost()
                                            .label(l.t("Settings…"))
                                            .tooltip(l.t("Settings (⌘,)"))
                                            .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx))),
                                    ),
                            ),
                    ),
            )
    }

    fn controls(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let theme = cx.theme();
        let at_start = live.current == 0;
        let at_end = live.current + 1 >= live.slides.len();
        h_flex()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(theme.border)
            .child(
                Button::new("prev")
                    .icon(IconName::ChevronLeft)
                    .label(l.t("Previous"))
                    .disabled(at_start)
                    .on_click(cx.listener(|this, _, _, cx| this.command(Command::Previous, cx))),
            )
            .child(
                Button::new("next")
                    .label(l.t("Next"))
                    .child(Icon::new(IconName::ChevronRight))
                    .disabled(at_end)
                    .on_click(cx.listener(|this, _, _, cx| this.command(Command::Next, cx))),
            )
            .child(div().flex_1())
            .child(
                Button::new("hide-text")
                    .icon(IconName::EyeOff)
                    .label(l.t("Hide text"))
                    .when(live.blank == Blank::Background, |b| b.danger())
                    .on_click(cx.listener(|this, _, _, cx| this.command(Command::ToggleBlank { mode: Blank::Background }, cx))),
            )
            .child(
                Button::new("logo")
                    .icon(IconName::Image)
                    .label(l.t("Logo"))
                    .when(live.blank == Blank::Logo, |b| b.danger())
                    .on_click(cx.listener(|this, _, _, cx| this.command(Command::ToggleBlank { mode: Blank::Logo }, cx))),
            )
            .child(
                Button::new("blank")
                    .icon(IconName::MonitorOff)
                    .label(l.t("Blank"))
                    .when(live.blank == Blank::Black, |b| b.danger())
                    .on_click(cx.listener(|this, _, _, cx| this.command(Command::ToggleBlank { mode: Blank::Black }, cx))),
            )
    }
}

fn section(title: &str, cx: &App) -> gpui::Div {
    v_flex().gap_2().child(
        div()
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(cx.theme().muted_foreground)
            .child(title.to_uppercase()),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use asaf_core::{Theme, Verse, sample_song};
    use asaf_library::Library;
    use gpui::test::TestWindowExt as _;
    use gpui::TestAppContext;

    use super::*;

    pub(crate) fn setup(cx: &mut TestAppContext) -> (Arc<Store>, Entity<Console>, AnyWindowHandle) {
        setup_with(cx, |_| {})
    }

    pub(crate) fn setup_with(
        cx: &mut TestAppContext,
        prepare: impl FnOnce(&mut Library),
    ) -> (Arc<Store>, Entity<Console>, AnyWindowHandle) {
        let (store, console, h, _) = setup_session(cx, prepare, ServiceSession::default());
        (store, console, h)
    }

    /// A console on an in-memory library holding "Blessed Assurance", "Cuán
    /// Grande Es Él" and whatever `prepare` adds, starting with `session`.
    pub(crate) fn setup_session(
        cx: &mut TestAppContext,
        prepare: impl FnOnce(&mut Library),
        session: ServiceSession,
    ) -> (Arc<Store>, Entity<Console>, AnyWindowHandle, SharedLibrary) {
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let mut lib = crate::test_library();
        prepare(&mut lib);
        lib.save(&sample_song()).unwrap();
        lib.save(&Song {
            title: "Cuán Grande Es Él".into(),
            verses: vec![Verse::new("v1", "Señor mi Dios")],
            ..Default::default()
        })
        .unwrap();
        let library = Arc::new(Mutex::new(lib));
        let (store, console, handle) = open_console(cx, library.clone(), session);
        (store, console, handle, library)
    }

    pub(crate) fn open_console(
        cx: &mut TestAppContext,
        library: SharedLibrary,
        session: ServiceSession,
    ) -> (Arc<Store>, Entity<Console>, AnyWindowHandle) {
        let store = Store::new(Song::default(), Theme::default());
        let (handle, console) = cx.update(|cx| {
            let store = store.clone();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1360.), px(820.)), cx))),
                ..Default::default()
            };
            gpui::open_window(options, cx, move |window, cx| {
                cx.new(|cx| Console::new(store, library, "http://test/display".into(), session, window, cx))
            })
            .unwrap()
        });
        cx.update(|cx| {
            handle
                .update(cx, |_, window, cx| console.read(cx).focus.clone().focus(window, cx))
                .unwrap()
        });
        (store, console, handle)
    }

    pub(crate) fn with_window<R>(cx: &mut TestAppContext, h: AnyWindowHandle, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        cx.update(|cx| h.update(cx, |_, window, cx| f(window, cx)).unwrap())
    }

    #[gpui::test]
    fn search_go_live_and_shortcuts(cx: &mut TestAppContext) {
        let (store, console, h) = setup(cx);
        let search = cx.update(|cx| console.read(cx).search.clone());

        with_window(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("cmd-f", cx);
            w.input("bless", cx);
            w.press("enter", cx);
        });
        let live = store.snapshot();
        assert_eq!(live.title, "Blessed Assurance");
        assert_eq!(live.current, 0);

        // Enter returned focus to live control: arrows and B drive the projector.
        with_window(cx, h, |w, cx| {
            w.press("right", cx);
            w.press("space", cx);
        });
        assert_eq!(store.snapshot().current, 2);
        with_window(cx, h, |w, cx| w.press("b", cx));
        assert_eq!(store.snapshot().blank, Blank::Black);
        with_window(cx, h, |w, cx| w.press("escape", cx));
        assert_eq!(store.snapshot().blank, Blank::None);

        // While typing in search, letters and arrows don't touch the live output.
        with_window(cx, h, |w, cx| {
            w.press("cmd-f", cx);
            w.press("cmd-a", cx);
            w.input("b", cx);
            w.press("right", cx);
        });
        assert_eq!(store.snapshot().blank, Blank::None);
        assert_eq!(store.snapshot().current, 2);
        assert_eq!(cx.update(|cx| search.read(cx).value()), "b");

        // Esc leaves search; arrows drive slides again.
        with_window(cx, h, |w, cx| {
            w.press("escape", cx);
            w.press("right", cx);
        });
        assert_eq!(store.snapshot().current, 3);
    }

    #[gpui::test]
    fn arrows_in_search_move_through_results(cx: &mut TestAppContext) {
        let (_, console, h) = setup(cx);
        let titles = |cx: &mut TestAppContext| {
            cx.update(|cx| console.read(cx).preview.as_ref().map(|s| s.title.clone()))
        };
        with_window(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("cmd-f", cx);
        });
        assert_eq!(titles(cx).as_deref(), Some("Blessed Assurance"));
        with_window(cx, h, |w, cx| w.press("down", cx));
        assert_eq!(titles(cx).as_deref(), Some("Cuán Grande Es Él"));
        with_window(cx, h, |w, cx| w.input("senor", cx));
        assert_eq!(titles(cx).as_deref(), Some("Cuán Grande Es Él"));
        assert_eq!(cx.update(|cx| console.read(cx).results.len()), 1);
    }

    #[gpui::test]
    fn editing_the_live_song_updates_the_projector(cx: &mut TestAppContext) {
        let (store, console, h) = setup(cx);
        with_window(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("cmd-f", cx);
            w.input("bless", cx);
            w.press("enter", cx);
        });
        with_window(cx, h, |w, cx| w.press("right", cx));
        assert_eq!(store.snapshot().current, 1);

        let mut song = cx.update(|cx| console.read(cx).preview.clone().unwrap());
        song.verses[1].lines[0] = "This is my story (edited)".into();
        console.update(cx, |c, cx| c.song_saved(song.clone(), cx));

        let live = store.snapshot();
        assert_eq!(live.current, 1, "position kept");
        assert_eq!(live.slides[1].lines[0], "This is my story (edited)");
    }

    // ---- Bible mode -------------------------------------------------------------

    use std::time::{Duration, Instant};

    use asaf_core::bible::BOOKS;
    use asaf_core::{Bible, BibleLayout, BibleVerse, VersesPerSlide};

    use crate::bible::Status;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bible/tests/fixtures").join(name)
    }

    /// Public-domain excerpts: KJV (English, OSIS) and RV1909 (Spanish, Zefania).
    pub(crate) fn with_bibles(lib: &mut Library) {
        lib.import_bible(&fixture("kjv-excerpt.osis.xml")).unwrap();
        lib.import_bible(&fixture("rv1909-excerpt.zefania.xml")).unwrap();
    }

    fn choose(cx: &mut TestAppContext, console: &Entity<Console>, abbreviation: &str) {
        console.update(cx, |c, cx| {
            let i = c.bible.bibles.iter().position(|b| b.abbreviation == abbreviation);
            c.choose_bible(i, cx);
        });
    }

    /// The preview's reference and its slides' text, one string per slide.
    fn preview(cx: &mut TestAppContext, console: &Entity<Console>) -> Option<(String, Vec<String>)> {
        cx.update(|cx| {
            console.read(cx).bible.preview.as_ref().map(|p| {
                (p.reference.clone(), p.slides.iter().map(|s| s.lines.join(" ")).collect())
            })
        })
    }

    fn status(cx: &mut TestAppContext, console: &Entity<Console>) -> Status {
        cx.update(|cx| console.read(cx).bible.status.clone())
    }

    /// Types `text` into the (focused, emptied) Bible box and presses ⏎;
    /// returns how long ⏎ took to fill the preview.
    fn look_up(cx: &mut TestAppContext, h: AnyWindowHandle, text: &str) -> Duration {
        with_window(cx, h, |w, cx| {
            w.press("cmd-2", cx);
            w.press("cmd-a", cx);
            w.press("backspace", cx);
            w.input(text, cx);
        });
        let t = Instant::now();
        with_window(cx, h, |w, cx| w.press("enter", cx));
        let elapsed = t.elapsed();
        with_window(cx, h, |w, cx| w.render_frame(cx));
        elapsed
    }

    #[gpui::test]
    fn reference_and_enter_shows_verses_then_goes_live(cx: &mut TestAppContext) {
        let (store, console, h) = setup_with(cx, with_bibles);
        with_window(cx, h, |w, cx| w.render_frame(cx));
        choose(cx, &console, "RV1909");
        console.update(cx, |c, cx| {
            c.set_bible_layout(BibleLayout { verses_per_slide: VersesPerSlide::Count(1), ..c.bible.layout }, cx)
        });

        // Spanish.
        let took = look_up(cx, h, "Jn 3:16-18");
        let (reference, slides) = preview(cx, &console).expect("verses in preview");
        assert_eq!(reference, "Juan 3:16-18");
        assert_eq!(slides.len(), 3);
        assert!(slides[0].starts_with("¹⁶ Porque de tal manera amó Dios al mundo"), "{}", slides[0]);
        assert!(slides[2].starts_with("¹⁸ El que en él cree, no es condenado"), "{}", slides[2]);
        assert!(took < Duration::from_secs(1), "{took:?}");
        eprintln!("Jn 3:16-18 (RV1909): ⏎ to preview in {took:?}");
        // Previewing doesn't touch the projector.
        assert!(store.snapshot().slides.is_empty());

        // ⏎ again: live, and focus is back on live control.
        with_window(cx, h, |w, cx| w.press("enter", cx));
        let live = store.snapshot();
        assert_eq!(live.title, "Juan 3:16-18 (RV1909)");
        assert_eq!((live.current, live.slides.len()), (0, 3));
        assert_eq!(live.slides[0].reference.as_deref(), Some("Juan 3:16 (RV1909)"));
        with_window(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("right", cx);
        });
        assert_eq!(store.snapshot().current, 1);

        // Cross-chapter ranges and lists.
        look_up(cx, h, "Gn 1:30-2:3");
        let (reference, slides) = preview(cx, &console).unwrap();
        assert_eq!((reference.as_str(), slides.len()), ("Génesis 1:30-2:3", 5));
        look_up(cx, h, "Sal 23; 91:1-4");
        let (reference, slides) = preview(cx, &console).unwrap();
        assert_eq!((reference.as_str(), slides.len()), ("Salmos 23; 91:1-4", 10));
        assert!(slides[6].starts_with("¹ EL que habita al abrigo del Altísimo"), "{}", slides[6]);

        // English: switching translation re-fetches the same passage.
        choose(cx, &console, "KJV");
        let (reference, slides) = preview(cx, &console).unwrap();
        assert_eq!(reference, "Psalms 23; 91:1-4");
        assert!(slides[0].starts_with("¹ The LORD is my shepherd"), "{}", slides[0]);
        let took = look_up(cx, h, "John 3:16");
        let (reference, slides) = preview(cx, &console).unwrap();
        assert_eq!(reference, "John 3:16");
        assert!(slides[0].starts_with("¹⁶ For God so loved the world"), "{}", slides[0]);
        assert!(took < Duration::from_secs(1), "{took:?}");
        eprintln!("John 3:16 (KJV): ⏎ to preview in {took:?}");

        // ⌘⏎ goes live straight away.
        look_up(cx, h, "Jude 24-25");
        with_window(cx, h, |w, cx| {
            w.press("cmd-2", cx);
            w.press("cmd-enter", cx);
        });
        assert_eq!(store.snapshot().title, "Jude 24-25 (KJV)");
    }

    #[gpui::test]
    fn typing_in_the_bible_box_never_drives_the_projector(cx: &mut TestAppContext) {
        let (store, _console, h) = setup_with(cx, with_bibles);
        with_window(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("cmd-1", cx);
            w.input("bless", cx);
            w.press("enter", cx);
        });
        assert_eq!(store.snapshot().title, "Blessed Assurance");
        with_window(cx, h, |w, cx| {
            w.press("cmd-2", cx);
            w.input("b", cx);
            w.press("right", cx);
            w.press("space", cx);
        });
        assert_eq!(store.snapshot().blank, Blank::None);
        assert_eq!(store.snapshot().current, 0);
        // Esc leaves the box; keys drive the projector again.
        with_window(cx, h, |w, cx| {
            w.press("escape", cx);
            w.press("right", cx);
            w.press("b", cx);
        });
        assert_eq!(store.snapshot().current, 1);
        assert_eq!(store.snapshot().blank, Blank::Black);
    }

    #[gpui::test]
    fn search_by_words_lists_verses(cx: &mut TestAppContext) {
        let (store, console, h) = setup_with(cx, with_bibles);
        choose(cx, &console, "RV1909");
        with_window(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("cmd-2", cx);
            w.input("senor", cx);
            w.render_frame(cx);
        });
        let hits = cx.update(|cx| console.read(cx).bible.hits.len());
        assert!(hits > 1, "{hits}");
        let first = preview(cx, &console).unwrap().0;
        with_window(cx, h, |w, cx| w.press("down", cx));
        let second = preview(cx, &console).unwrap().0;
        assert_ne!(first, second);

        with_window(cx, h, |w, cx| {
            w.press("cmd-a", cx);
            w.input("tal manera amo", cx);
        });
        assert_eq!(preview(cx, &console).unwrap().0, "Juan 3:16");
        with_window(cx, h, |w, cx| w.press("enter", cx));
        assert_eq!(store.snapshot().title, "Juan 3:16 (RV1909)");

        with_window(cx, h, |w, cx| {
            w.press("cmd-2", cx);
            w.press("cmd-a", cx);
            w.input("zzzz", cx);
        });
        assert!(matches!(status(cx, &console), Status::Error(_)));
        // Nothing found: ⏎ doesn't put the previous hit live.
        let before = store.snapshot().revision;
        with_window(cx, h, |w, cx| w.press("enter", cx));
        assert_eq!(store.snapshot().revision, before);
    }

    #[gpui::test]
    fn bad_references_explain_themselves(cx: &mut TestAppContext) {
        let (_, console, h) = setup_with(cx, with_bibles);
        choose(cx, &console, "RV1909");
        with_window(cx, h, |w, cx| {
            w.press("cmd-2", cx);
            w.input("Juan 22", cx);
        });
        assert_eq!(status(cx, &console), Status::Error("Juan has 21 chapters, not 22".into()));
        look_up(cx, h, "Juan 3:99");
        assert_eq!(status(cx, &console), Status::Error("Juan 3:99 isn't in RV1909.".into()));
        assert!(preview(cx, &console).is_none());
        look_up(cx, h, "Juan 3:16");
        assert!(matches!(status(cx, &console), Status::Hint(_)));
    }

    #[gpui::test]
    fn no_bibles_installed(cx: &mut TestAppContext) {
        let (store, console, h) = setup(cx);
        look_up(cx, h, "Jn 3:16");
        assert!(matches!(status(cx, &console), Status::Error(e) if e.contains("Import a Bible")));
        assert!(store.snapshot().slides.is_empty());
    }

    #[gpui::test]
    fn layout_changes_update_a_live_passage_in_place(cx: &mut TestAppContext) {
        let (store, console, h) = setup_with(cx, with_bibles);
        choose(cx, &console, "KJV");
        console.update(cx, |c, cx| {
            c.set_bible_layout(BibleLayout { verses_per_slide: VersesPerSlide::Count(1), ..c.bible.layout }, cx)
        });
        look_up(cx, h, "Ps 23");
        with_window(cx, h, |w, cx| w.press("enter", cx));
        with_window(cx, h, |w, cx| {
            w.press("right", cx);
            w.press("right", cx);
        });
        assert_eq!(store.snapshot().slides[store.snapshot().current].label, "Psalms 23:3");

        console.update(cx, |c, cx| {
            c.set_bible_layout(
                BibleLayout {
                    verses_per_slide: VersesPerSlide::Count(2),
                    verse_numbers: false,
                    reference: asaf_core::ReferencePlacement::LastSlide,
                },
                cx,
            )
        });
        let live = store.snapshot();
        assert_eq!(live.slides.len(), 3);
        assert_eq!(live.slides[live.current].label, "Psalms 23:3-4", "verse 3 stays on screen");
        assert_eq!(live.slides[2].lines.last().unwrap(), "Psalms 23 (KJV)");
        assert!(!live.slides[0].lines[0].starts_with('¹'));
        with_window(cx, h, |w, cx| w.render_frame(cx));
    }

    /// A made-up translation the size of a real Bible (~31,000 verses).
    fn full_size_bible(language: &str, abbreviation: &str) -> (Bible, Vec<BibleVerse>) {
        let words = ["lux", "aqua", "terra", "caelum", "verbum", "vita", "pax", "via", "spes", "fides"];
        let mut verses = Vec::new();
        for (i, b) in BOOKS.iter().enumerate() {
            for chapter in 1..=b.chapters {
                for verse in 1..=26u16 {
                    let text: Vec<&str> = (0..24).map(|k| words[(i + usize::from(chapter) + usize::from(verse) * k) % 10]).collect();
                    verses.push(BibleVerse { book: i as u8 + 1, chapter, verse, text: text.join(" ") });
                }
            }
        }
        let bible = Bible {
            name: format!("Synthetic {abbreviation}"),
            abbreviation: abbreviation.into(),
            language: language.into(),
            ..Default::default()
        };
        (bible, verses)
    }

    /// Phase 2 "done when": typing a reference and pressing ⏎ shows the right
    /// verses in the preview in under a second, in both languages.
    #[gpui::test]
    fn enter_to_preview_under_a_second_on_full_size_bibles(cx: &mut TestAppContext) {
        let (_, console, h) = setup_with(cx, |lib| {
            for (lang, abbr) in [("es", "SES"), ("en", "SEN")] {
                let (bible, verses) = full_size_bible(lang, abbr);
                lib.save_bible(&bible, &verses).unwrap();
            }
        });
        let mut worst = Duration::ZERO;
        for (abbr, typed, reference, verses) in [
            ("SES", "Jn 3:16-18", "Juan 3:16-18", 3),
            ("SES", "Gn 1:25-2:3", "Génesis 1:25-2:3", 5),
            ("SES", "Sal 23; 91:1-4", "Salmos 23; 91:1-4", 30),
            ("SES", "1 Juan 4", "1 Juan 4", 26),
            ("SES", "Apocalipsis 21-22", "Apocalipsis 21-22", 52),
            ("SEN", "John 3:16-18", "John 3:16-18", 3),
            ("SEN", "Gen 1:25-2:3", "Genesis 1:25-2:3", 5),
            ("SEN", "Ps 119", "Psalms 119", 26),
            ("SEN", "1 Cor 13:4-7", "1 Corinthians 13:4-7", 4),
            ("SEN", "Rev 21-22", "Revelation 21-22", 52),
        ] {
            choose(cx, &console, abbr);
            let took = look_up(cx, h, typed);
            worst = worst.max(took);
            let n = cx.update(|cx| console.read(cx).bible.preview.as_ref().map(|p| (p.reference.clone(), p.verses.len())));
            assert_eq!(n, Some((reference.to_string(), verses)), "{typed}");
        }
        eprintln!("slowest ⏎ → preview on a full-size Bible: {worst:?}");
        assert!(worst < Duration::from_secs(1), "{worst:?}");
    }

    // ---- themes, settings, migration ----------------------------------------------

    use asaf_core::{Language, ServiceItem};

    use crate::service::ServiceSession;
    use crate::theme_editor::ThemesChanged;

    fn theme_named(name: &str, color: &str) -> Theme {
        Theme { name: name.into(), color: color.into(), ..Theme::default() }
    }

    /// A service of three songs; the second has its own theme.
    fn themed_service(lib: &mut Library) -> ServiceSession {
        lib.save_theme(&theme_named("Scripture", "#ffee00"), None).unwrap();
        let mut service = asaf_core::Service::default();
        service.push(ServiceItem::song(1, "Blessed Assurance"));
        let mut themed = ServiceItem::song(2, "Cuán Grande Es Él");
        themed.theme = Some("scripture".into());
        service.push(themed);
        service.push(ServiceItem::song(1, "Blessed Assurance"));
        ServiceSession { service, ..Default::default() }
    }

    fn setup_themed(cx: &mut TestAppContext) -> (Arc<Store>, Entity<Console>, AnyWindowHandle, SharedLibrary) {
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let mut lib = crate::test_library();
        lib.save(&sample_song()).unwrap();
        lib.save(&Song { title: "Cuán Grande Es Él".into(), verses: vec![Verse::new("v1", "Señor mi Dios")], ..Default::default() })
            .unwrap();
        let session = themed_service(&mut lib);
        let library = Arc::new(Mutex::new(lib));
        let (store, console, h) = open_console(cx, library.clone(), session);
        with_window(cx, h, |w, cx| w.render_frame(cx));
        (store, console, h, library)
    }

    /// Per-item themes reach the projector together with the item's first
    /// slide; items without one (and ad-hoc songs) use the default.
    #[gpui::test]
    fn service_items_go_live_in_their_own_theme(cx: &mut TestAppContext) {
        let (store, _console, h, _) = setup_themed(cx);
        let shown = |store: &Store| {
            let st = store.display_state();
            (st.theme.name.clone(), st.theme.color.clone(), st.slide.map(|s| s.lines[0].clone()))
        };
        with_window(cx, h, |w, cx| w.press("enter", cx));
        assert_eq!(shown(&store), ("Default".into(), "#ffffff".into(), Some("Blessed assurance, Jesus is mine!".into())));
        // Every state the displays get: the new slide never shows in the old theme.
        let sent = Arc::new(Mutex::new(Vec::new()));
        let sink = sent.clone();
        store.subscribe(move |st| sink.lock().unwrap().push((st.theme.name.clone(), st.slide.as_ref().map(|s| s.lines[0].clone()))));
        with_window(cx, h, |w, cx| w.press("enter", cx));
        assert_eq!(shown(&store), ("Scripture".into(), "#ffee00".into(), Some("Señor mi Dios".into())));
        let sent = sent.lock().unwrap().clone();
        assert!(!sent.is_empty());
        for (theme, line) in sent {
            assert_eq!((theme.as_str(), line.as_deref()), ("Scripture", Some("Señor mi Dios")), "the slide and its theme change in one revision");
        }
        assert_eq!(store.snapshot().theme_name, "Scripture");
        with_window(cx, h, |w, cx| w.press("enter", cx));
        assert_eq!(shown(&store).0, "Default");

        // Ad hoc from the library: the default theme.
        with_window(cx, h, |w, cx| w.press("cmd-up", cx));
        with_window(cx, h, |w, cx| w.press("enter", cx));
        assert_eq!(shown(&store).0, "Scripture");
        with_window(cx, h, |w, cx| {
            w.press("cmd-f", cx);
            w.input("bless", cx);
            w.press("enter", cx);
        });
        assert_eq!(shown(&store).0, "Default");
    }

    #[gpui::test]
    fn theme_chips_set_the_cued_items_theme(cx: &mut TestAppContext) {
        let (store, console, h, _) = setup_themed(cx);
        // The first item is cued; its chips are the default and each theme.
        with_window(cx, h, |w, cx| w.click(("item-theme", 2usize), cx));
        let theme_of = |cx: &mut TestAppContext, i: usize| cx.update(|cx| console.read(cx).service.service.items[i].theme.clone());
        assert_eq!(theme_of(cx, 0).as_deref(), Some("Scripture"));
        assert!(cx.update(|cx| console.read(cx).service.dirty));
        with_window(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("enter", cx);
        });
        assert_eq!(store.display_state().theme.name, "Scripture");
        with_window(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("cmd-up", cx);
            w.render_frame(cx);
            w.click(("item-theme", 0usize), cx);
        });
        assert_eq!(theme_of(cx, 0), None, "back to the default");
        // The cue preview's slide cards and header follow the item's theme.
        let look = cx.update(|cx| {
            let c = console.read(cx);
            c.look_of(c.service.cued()).name
        });
        assert_eq!(look, "Default");
    }

    #[gpui::test]
    fn editing_themes_updates_the_service_and_the_projector(cx: &mut TestAppContext) {
        let (store, console, h, library) = setup_themed(cx);
        with_window(cx, h, |w, cx| w.press("enter", cx));
        with_window(cx, h, |w, cx| w.press("enter", cx));
        assert_eq!(store.display_state().theme.name, "Scripture");

        // The theme on screen is edited and renamed: the projector shows the
        // new look at once and the service follows the new name.
        library.lock().unwrap().save_theme(&theme_named("Bible text", "#00ffff"), Some("Scripture")).unwrap();
        console.update(cx, |c, cx| {
            c.themes_changed(&ThemesChanged { renamed: Some(("Scripture".into(), "Bible text".into())), deleted: None }, cx)
        });
        let st = store.display_state();
        assert_eq!((st.theme.name.as_str(), st.theme.color.as_str()), ("Bible text", "#00ffff"));
        assert_eq!(cx.update(|cx| console.read(cx).service.service.items[1].theme.clone()).as_deref(), Some("Bible text"));

        // Deleted: back to the default, on screen and in the service.
        library.lock().unwrap().delete_theme("Bible text").unwrap();
        console.update(cx, |c, cx| c.themes_changed(&ThemesChanged { renamed: None, deleted: Some("Bible text".into()) }, cx));
        assert_eq!(store.display_state().theme.name, "Default");
        assert_eq!(cx.update(|cx| console.read(cx).service.service.items[1].theme.clone()), None);

        // A new default theme shows straight away on an item that has none.
        let mut lib = library.lock().unwrap();
        lib.save_theme(&theme_named("Night", "#cccccc"), None).unwrap();
        let mut settings = lib.settings().unwrap();
        settings.default_theme = "Night".into();
        lib.save_settings(&settings).unwrap();
        drop(lib);
        console.update(cx, |c, cx| c.themes_changed(&ThemesChanged::default(), cx));
        assert_eq!(store.display_state().theme.name, "Night");
        with_window(cx, h, |w, cx| w.render_frame(cx));
    }

    #[gpui::test]
    fn cmd_t_and_cmd_comma_open_the_theme_editor_and_settings(cx: &mut TestAppContext) {
        let (store, console, h, library) = setup_themed(cx);
        let windows = |cx: &mut TestAppContext| cx.update(|cx| cx.windows().len());
        assert_eq!(windows(cx), 1);
        // ⌘T on the themed item opens the editor on its theme.
        with_window(cx, h, |w, cx| w.press("cmd-down", cx));
        with_window(cx, h, |w, cx| w.press("cmd-t", cx));
        assert_eq!(windows(cx), 2);
        let (editor, editor_view) = cx.update(|cx| console.read(cx).theme_editor.clone().unwrap());
        assert_eq!(cx.update(|cx| editor_view.read(cx).themes[editor_view.read(cx).selected].name.clone()), "Scripture");
        // Pressed again: the same window comes forward.
        with_window(cx, h, |w, cx| w.press("cmd-t", cx));
        assert_eq!(windows(cx), 2);

        // Saving in the editor reaches the console (and the projector).
        with_window(cx, h, |w, cx| w.press("enter", cx));
        assert_eq!(store.display_state().theme.name, "Scripture");
        cx.update(|cx| {
            editor
                .update(cx, |_, window, cx| {
                    editor_view.update(cx, |e, cx| {
                        e.input(crate::theme_editor::Field::Color).update(cx, |s, cx| s.set_value("#ff00ff", window, cx));
                        e.save(window, cx);
                    })
                })
                .unwrap()
        });
        assert_eq!(store.display_state().theme.color, "#ff00ff");

        // ⌘, opens settings; switching language there switches the console.
        with_window(cx, h, |w, cx| {
            console.read(cx).focus.clone().focus(w, cx);
            w.press("cmd-,", cx);
        });
        assert_eq!(windows(cx), 3);
        let settings = cx.update(|cx| console.read(cx).settings_window.unwrap());
        cx.update(|cx| {
            settings
                .update(cx, |_, w, cx| {
                    w.render_frame(cx);
                    w.click("lang-es", cx);
                })
                .unwrap()
        });
        assert_eq!(cx.update(|cx| crate::i18n::lang(cx)), crate::i18n::Lang(Language::Spanish));
        assert_eq!(library.lock().unwrap().settings().unwrap().language, Language::Spanish);
        with_window(cx, h, |w, cx| w.render_frame(cx));
    }

    /// The console's "Import from OpenLP…" action: pick the folder, and the
    /// songs and Bibles show up in the console.
    #[gpui::test]
    fn importing_from_openlp_fills_the_library(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/tests/fixtures/openlp");
        for (file, sql) in [("songs/songs.sqlite", "songs.sql"), ("bibles/KJV.sqlite", "kjv.sql")] {
            let path = dir.path().join("Data").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let sql = std::fs::read_to_string(fixtures.join(sql)).unwrap();
            rusqlite::Connection::open(&path).unwrap().execute_batch(&sql).unwrap();
        }
        // A service from a machine that already has OpenLP's songs: its item is
        // missing here until the import, then shows without re-cueing.
        let mut service = asaf_core::Service::default();
        service.push(ServiceItem::song(999, "Castillo Fuerte"));
        let session = ServiceSession { service, ..Default::default() };
        let (_, console, h, _) = setup_session(cx, |_| {}, session);
        let cued_is_missing = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                let c = console.read(cx);
                matches!(c.service.resolved.as_ref().map(|r| &r.target), Some(crate::service::Target::Missing(_)))
            })
        };
        cx.update(|cx| {
            console.update(cx, |c, cx| {
                c.service.cursor = Some(0);
                c.resolve_cue(cx);
            })
        });
        assert!(cued_is_missing(cx));
        let before = cx.update(|cx| console.read(cx).results.len());
        with_window(cx, h, |w, cx| w.dispatch_action(Box::new(crate::ImportOpenLp), cx));
        cx.run_until_parked();
        cx.simulate_path_prompt_response(|_| Some(vec![dir.path().join("Data")]));
        cx.run_until_parked();
        let (songs, bibles) = cx.update(|cx| {
            let c = console.read(cx);
            (c.results.iter().map(|r| r.title.clone()).collect::<Vec<_>>(), c.bible.bibles.len())
        });
        assert_eq!(songs.len(), before + 4, "{songs:?}"); // its Blessed Assurance has different lyrics
        assert!(songs.contains(&"Castillo Fuerte".to_string()));
        assert_eq!(bibles, 1);
        assert!(!cued_is_missing(cx), "the cued item resolved once its song arrived");
    }
}
