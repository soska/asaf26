use std::sync::Arc;

use anyhow::Result;
use asaf_core::{Blank, LiveSnapshot, Song, Store};
use asaf_library::SongSummary;
use asaf_projector::Projector;
use gpui::component::button::{Button, ButtonVariants as _};
use gpui::component::input::{Input, InputEvent, InputState};
use gpui::component::{ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui::component::notification::Notification;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable,
    FontWeight, InteractiveElement, IntoElement, ParentElement, Render, ScrollStrategy,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TitlebarOptions,
    UniformListScrollHandle, Window, WindowBounds, WindowOptions, div, px, size, uniform_list,
};

use crate::editor::{SongEditor, SongSaved};
use crate::{
    BlankBackground, BlankBlack, EditSong, FocusSearch, LeaveSearch, NewSong, Next, Previous,
    SelectNextSong, SelectPreviousSong, SharedLibrary, Unblank, transfer,
};

const RESULT_LIMIT: usize = 1000;

pub struct Console {
    store: Arc<Store>,
    library: SharedLibrary,
    display_url: SharedString,
    focus: FocusHandle,
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
        let subscriptions = vec![
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
            search,
            results: Vec::new(),
            results_scroll: UniformListScrollHandle::new(),
            selected: None,
            preview: None,
            editors: Vec::new(),
            _subscriptions: subscriptions,
        };
        console.refresh_search(cx);
        console
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
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search.update(cx, |s, cx| s.focus(window, cx));
            }))
            .on_action(cx.listener(|this, _: &SelectNextSong, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &SelectPreviousSong, _, cx| this.move_selection(-1, cx)))
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
                    .child(self.library_column(window, cx))
                    .child(self.preview_column(&live, cx))
                    .child(self.live_column(&live, cx)),
            )
            .child(self.controls(&live, cx))
    }
}

impl Console {
    fn library_column(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let count = self.results.len();
        v_flex()
            .w(px(340.))
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
                    .child("⌘F  search   ·   ⌘E  edit   ·   ⌘N  new song")
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
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let mut lib = Library::open_in_memory().unwrap();
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
}

