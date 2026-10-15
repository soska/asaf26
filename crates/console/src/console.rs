use std::sync::Arc;

use anyhow::Result;
use asaf_core::{Blank, LiveSnapshot, Song, Store};
use asaf_library::SongSummary;
use asaf_projector::Projector;
use gpui::component::button::{Button, ButtonVariants as _};
use gpui::component::input::{Input, InputEvent, InputState};
use gpui::component::{ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui::component::notification::Notification;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable,
    FontWeight, InteractiveElement, IntoElement, ParentElement, Render, ScrollStrategy,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TitlebarOptions,
    UniformListScrollHandle, Window, WindowBounds, WindowOptions, div, px, size, uniform_list,
};

use crate::bible::BibleState;
use crate::editor::{SongEditor, SongSaved};
use crate::{
    BlankBackground, BlankBlack, EditSong, FocusSearch, LeaveSearch, NewSong, Next, Previous, SelectNext,
    SelectPrevious, SharedLibrary, ShowBible, ShowSongs, Unblank, transfer,
};

const RESULT_LIMIT: usize = 1000;

/// What the left column shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Songs,
    Bible,
}

pub struct Console {
    pub(crate) store: Arc<Store>,
    pub(crate) library: SharedLibrary,
    display_url: SharedString,
    pub(crate) focus: FocusHandle,
    pub(crate) mode: Mode,
    pub(crate) bible: BibleState,
    search: Entity<InputState>,
    results: Vec<SongSummary>,
    results_scroll: UniformListScrollHandle,
    selected: Option<usize>,
    /// The selected song, loaded in full: its slides fill the middle column.
    preview: Option<Song>,
    /// Open editor windows, by song id (`None` for new songs).
    editors: Vec<(Option<i64>, AnyWindowHandle)>,
    _subscriptions: Vec<Subscription>,
}

impl Console {
    pub fn open(
        store: Arc<Store>,
        library: SharedLibrary,
        display_url: String,
        cx: &mut App,
    ) -> Result<Entity<Self>> {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1240.), px(780.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions { title: Some("Asaf".into()), ..Default::default() }),
            window_min_size: Some(size(px(900.), px(520.))),
            ..Default::default()
        };
        let (handle, console) = gpui::open_window(options, cx, |window, cx| {
            cx.new(|cx| Console::new(store, library, display_url, window, cx))
        })?;
        handle.update(cx, |_, window, cx| {
            window.activate_window();
            console.read(cx).focus.clone().focus(window, cx);
        })?;
        Ok(console)
    }

    fn new(
        store: Arc<Store>,
        library: SharedLibrary,
        display_url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search title, lyrics or author"));
        let bible = BibleState::new(window, cx);
        let subscriptions = vec![
            cx.subscribe_in(&bible.input, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => this.bible_text_changed(cx),
                InputEvent::PressEnter { secondary, .. } => this.bible_enter(*secondary, window, cx),
                _ => {}
            }),
            cx.subscribe_in(&search, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => this.refresh_search(cx),
                InputEvent::PressEnter { .. } => this.go_live(0, window, cx),
                _ => {}
            }),
            cx.observe_global::<Projector>(|_, cx| cx.notify()),
        ];
        let mut console = Self {
            store,
            library,
            display_url: display_url.into(),
            focus: cx.focus_handle(),
            mode: Mode::Songs,
            bible,
            search,
            results: Vec::new(),
            results_scroll: UniformListScrollHandle::new(),
            selected: None,
            preview: None,
            editors: Vec::new(),
            _subscriptions: subscriptions,
        };
        console.refresh_search(cx);
        console.refresh_bibles(None, cx);
        console
    }

    /// Switches the left column and puts the cursor in its search box.
    pub(crate) fn show_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        let input = match mode {
            Mode::Songs => &self.search,
            Mode::Bible => &self.bible.input,
        };
        input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn library(&self) -> &SharedLibrary {
        &self.library
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

    /// Puts the previewed song live at `index`, and returns focus to live control.
    fn go_live(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(song) = &self.preview {
            self.store.load_song(song.clone(), index);
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    fn act(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&Store)) {
        f(&self.store);
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
            Err(e) => window.push_notification(Notification::error(format!("Can't open editor: {e}")), cx),
        }
    }

    fn song_saved(&mut self, song: Song, cx: &mut Context<Self>) {
        self.store.song_edited(&song);
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

impl Focusable for Console {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Console {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let live = self.store.snapshot();
        let theme = cx.theme();

        div()
            .key_context("Console")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Next, _, cx| this.act(cx, Store::next)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.act(cx, Store::previous)))
            .on_action(cx.listener(|this, _: &BlankBlack, _, cx| this.act(cx, |s| s.toggle_blank(Blank::Black))))
            .on_action(cx.listener(|this, _: &BlankBackground, _, cx| {
                this.act(cx, |s| s.toggle_blank(Blank::Background))
            }))
            .on_action(cx.listener(|this, _: &Unblank, _, cx| this.act(cx, |s| s.set_blank(Blank::None))))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| this.show_mode(this.mode, window, cx)))
            .on_action(cx.listener(|this, _: &ShowSongs, window, cx| this.show_mode(Mode::Songs, window, cx)))
            .on_action(cx.listener(|this, _: &ShowBible, window, cx| this.show_mode(Mode::Bible, window, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| match this.mode {
                Mode::Songs => this.move_selection(1, cx),
                Mode::Bible => this.move_hit(1, cx),
            }))
            .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| match this.mode {
                Mode::Songs => this.move_selection(-1, cx),
                Mode::Bible => this.move_hit(-1, cx),
            }))
            .on_action(cx.listener(|this, _: &LeaveSearch, window, cx| this.focus.focus(window, cx)))
            .on_action(cx.listener(|this, _: &NewSong, window, cx| this.edit(Song::default(), window, cx)))
            .on_action(cx.listener(|this, _: &EditSong, window, cx| {
                if let Some(song) = this.preview.clone() {
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
                    .child(match self.mode {
                        Mode::Songs => self.preview_column(&live, cx).into_any_element(),
                        Mode::Bible => self.passage_preview_column(&live, cx).into_any_element(),
                    })
                    .child(self.live_column(&live, cx)),
            )
            .child(self.controls(&live, cx))
    }
}

impl Console {
    fn left_column(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let tab = |id: &'static str, label: &'static str, mode: Mode, tip: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .small()
                .ghost()
                .label(label)
                .tooltip(tip)
                .selected(self.mode == mode)
                .on_click(cx.listener(move |this, _, window, cx| this.show_mode(mode, window, cx)))
        };
        v_flex()
            .w(px(340.))
            .flex_none()
            .h_full()
            .border_r_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_1()
                    .px_3()
                    .pt_3()
                    .child(tab("tab-songs", "Songs", Mode::Songs, "Songs (⌘1)", cx))
                    .child(tab("tab-bible", "Bible", Mode::Bible, "Bible (⌘2)", cx)),
            )
            .child(match self.mode {
                Mode::Songs => self.library_column(window, cx).into_any_element(),
                Mode::Bible => self.bible_column(cx).into_any_element(),
            })
    }

    fn library_column(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .child(div().flex_1().font_weight(FontWeight::BOLD).child("Songs"))
                            .child(
                                Button::new("new-song")
                                    .small()
                                    .label("New")
                                    .tooltip("New song (⌘N)")
                                    .on_click(cx.listener(|this, _, window, cx| this.edit(Song::default(), window, cx))),
                            )
                            .child(
                                Button::new("import")
                                    .small()
                                    .label("Import…")
                                    .tooltip("Import OpenLyrics files or folders")
                                    .on_click(cx.listener(|_, _, window, cx| transfer::import(window, cx))),
                            )
                            .child(
                                Button::new("export-all")
                                    .small()
                                    .label("Export…")
                                    .tooltip("Export every song as OpenLyrics into a folder")
                                    .on_click(cx.listener(|_, _, window, cx| transfer::export_all(window, cx))),
                            ),
                    )
                    .child(div().key_context("SongSearch").child(Input::new(&self.search).cleanable(true))),
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
                                        this.select(Some(i), cx);
                                        if ev.click_count() >= 2
                                            && let Some(song) = this.preview.clone()
                                        {
                                            this.edit(song, window, cx);
                                        }
                                    }))
                                    .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(r.title.clone()))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(excerpt),
                                    )
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
                        "{count} song{}{}  ·  ↑↓ choose, ⏎ go live, Esc back",
                        if count == 1 { "" } else { "s" },
                        if count == RESULT_LIMIT { "+" } else { "" }
                    )),
            )
    }

    fn preview_column(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let Some(song) = &self.preview else {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child(if self.results.is_empty() { "No songs found." } else { "Select a song." });
        };
        let slides = song.slides();
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
                            .child(div().truncate().text_base().font_weight(FontWeight::BOLD).child(song.title.clone()))
                            .child(div().truncate().text_xs().text_color(theme.muted_foreground).child(subtitle)),
                    )
                    .child(Button::new("export-song").small().ghost().label("Export…").on_click(cx.listener(
                        |this, _, window, cx| {
                            if let Some(song) = this.preview.clone() {
                                transfer::export_song(song, window, cx);
                            }
                        },
                    )))
                    .child(Button::new("edit-song").small().label("Edit").tooltip("Edit song (⌘E)").on_click(
                        cx.listener(|this, _, window, cx| {
                            if let Some(song) = this.preview.clone() {
                                this.edit(song, window, cx);
                            }
                        }),
                    ))
                    .child(
                        Button::new("go-live")
                            .small()
                            .primary()
                            .label("Go live")
                            .tooltip("Show the first slide (⏎ in search)")
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
                        d.child(div().text_color(theme.muted_foreground).child("This song has no slides. Check its verse order."))
                    })
                    .children(slides.iter().enumerate().map(|(i, slide)| {
                        let on_air = is_live_song && i == live.current;
                        h_flex()
                            .id(("slide", i))
                            .items_start()
                            .gap_3()
                            .p_3()
                            .rounded_md()
                            .border_1()
                            .border_color(if on_air { theme.primary } else { theme.border })
                            .bg(if on_air { theme.primary.opacity(0.15) } else { theme.secondary })
                            .cursor_pointer()
                            .hover(|s| s.border_color(theme.primary))
                            .on_click(cx.listener(move |this, _, window, cx| this.go_live(i, window, cx)))
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(44.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(if on_air { theme.foreground } else { theme.muted_foreground })
                                    .child(slide.label.clone()),
                            )
                            .child(v_flex().flex_1().children(slide.lines.iter().map(|l| div().child(l.clone()))))
                    })),
            )
    }

    fn live_column(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (blank_text, blank_color) = match live.blank {
            Blank::None => ("Showing text", theme.muted_foreground),
            Blank::Black => ("Blanked to black", theme.danger),
            Blank::Background => ("Text hidden (background only)", theme.danger),
        };
        let current = live.slides.get(live.current);

        v_flex()
            .w(px(300.))
            .flex_none()
            .h_full()
            .gap_4()
            .p_4()
            .border_l_1()
            .border_color(theme.border)
            .child(
                section("Live", cx).child(match current {
                    None => div().text_color(theme.muted_foreground).child("Nothing live yet."),
                    Some(slide) => v_flex()
                        .gap_1()
                        .child(div().font_weight(FontWeight::BOLD).truncate().child(live.title.clone()))
                        .child(format!("{} · {} of {}", slide.label, live.current + 1, live.slides.len()))
                        .child(
                            v_flex()
                                .mt_1()
                                .p_2()
                                .rounded_md()
                                .bg(theme.secondary)
                                .text_xs()
                                .children(slide.lines.iter().map(|l| div().child(l.clone()))),
                        ),
                })
                .child(div().text_color(blank_color).child(blank_text)),
            )
            .child(section("Projector display", cx).children(Projector::displays(cx).into_iter().enumerate().map(|(i, d)| {
                let id = d.id;
                h_flex()
                    .id(("display", i))
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(if d.is_projector { theme.primary } else { theme.border })
                    .cursor_pointer()
                    .hover(|s| s.border_color(theme.primary))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        Projector::choose(id, cx);
                        cx.notify();
                    }))
                    .child(div().w(px(14.)).child(if d.is_projector { "●" } else { "" }))
                    .child(d.label)
            })))
            .child(div().flex_1())
            .child(
                v_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("→ ↓ Space  next   ·   ← ↑  previous")
                    .child("B  black   ·   ⇧B  hide text   ·   Esc  unblank")
                    .child("⌘1  songs   ·   ⌘2  Bible   ·   ⌘F  search")
                    .child("⌘E  edit song   ·   ⌘N  new song")
                    .child(div().mt_1().truncate().child(self.display_url.clone())),
            )
    }

    fn controls(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .label("◀  Previous")
                    .disabled(at_start)
                    .on_click(cx.listener(|this, _, _, cx| this.act(cx, Store::previous))),
            )
            .child(
                Button::new("next")
                    .label("Next  ▶")
                    .disabled(at_end)
                    .on_click(cx.listener(|this, _, _, cx| this.act(cx, Store::next))),
            )
            .child(div().flex_1())
            .child(
                Button::new("hide-text")
                    .label("Hide text")
                    .when(live.blank == Blank::Background, |b| b.danger())
                    .on_click(cx.listener(|this, _, _, cx| this.act(cx, |s| s.toggle_blank(Blank::Background)))),
            )
            .child(
                Button::new("blank")
                    .label("Blank")
                    .when(live.blank == Blank::Black, |b| b.danger())
                    .on_click(cx.listener(|this, _, _, cx| this.act(cx, |s| s.toggle_blank(Blank::Black)))),
            )
    }
}

fn section(title: &'static str, cx: &App) -> gpui::Div {
    v_flex().gap_2().child(
        div()
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(cx.theme().muted_foreground)
            .child(title.to_uppercase()),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use asaf_core::{Theme, Verse, sample_song};
    use asaf_library::Library;
    use gpui::test::TestWindowExt as _;
    use gpui::TestAppContext;

    use super::*;

    fn setup(cx: &mut TestAppContext) -> (Arc<Store>, Entity<Console>, AnyWindowHandle) {
        setup_with(cx, |_| {})
    }

    fn setup_with(
        cx: &mut TestAppContext,
        prepare: impl FnOnce(&mut Library),
    ) -> (Arc<Store>, Entity<Console>, AnyWindowHandle) {
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let mut lib = Library::open_in_memory().unwrap();
        prepare(&mut lib);
        lib.save(&sample_song()).unwrap();
        lib.save(&Song {
            title: "Cuán Grande Es Él".into(),
            verses: vec![Verse::new("v1", "Señor mi Dios")],
            ..Default::default()
        })
        .unwrap();
        let library = Arc::new(Mutex::new(lib));
        let store = Store::new(Song::default(), Theme::default());
        let (handle, console) = cx.update(|cx| {
            let store = store.clone();
            gpui::open_window(WindowOptions::default(), cx, move |window, cx| {
                cx.new(|cx| Console::new(store, library, "http://test/display".into(), window, cx))
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

    fn with_window<R>(cx: &mut TestAppContext, h: AnyWindowHandle, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
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
    fn with_bibles(lib: &mut Library) {
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
        assert_eq!(live.slides[0].footer.as_deref(), Some("Juan 3:16 (RV1909)"));
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
}
