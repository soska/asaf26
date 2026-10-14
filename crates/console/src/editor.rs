//! Song editor window: metadata fields, tagged lyrics, verse order and a live
//! slide preview. Saving writes to the library and emits [`SongSaved`].

use anyhow::Result;
use asaf_core::{Song, format_lyrics, parse_lyrics};
use gpui::component::button::{Button, ButtonVariants as _};
use gpui::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui::component::notification::Notification;
use gpui::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, InteractiveElement, IntoElement, ParentElement, PromptLevel, Render,
    StatefulInteractiveElement, Styled, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, px, size,
};

use crate::{SaveSong, SharedLibrary};

/// Emitted after a successful save, with the song as stored (id set).
pub struct SongSaved(pub Song);

pub struct SongEditor {
    library: SharedLibrary,
    /// The song as last saved; the draft is compared to it to detect changes.
    original: Song,
    title: Entity<InputState>,
    alt_title: Entity<InputState>,
    authors: Entity<InputState>,
    copyright: Entity<InputState>,
    ccli: Entity<InputState>,
    verse_order: Entity<InputState>,
    lyrics: Entity<TextareaState>,
    /// Set once the operator agreed to throw away changes, so closing proceeds.
    discard_confirmed: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SongSaved> for SongEditor {}

impl SongEditor {
    pub fn open(song: Song, library: SharedLibrary, cx: &mut App) -> Result<(AnyWindowHandle, Entity<Self>)> {
        let title = if song.id.is_some() { format!("Edit — {}", song.title) } else { "New Song".into() };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1080.), px(760.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some(title.into()), ..Default::default() }),
            window_min_size: Some(size(px(760.), px(520.))),
            ..Default::default()
        };
        let (handle, editor) = gpui::open_window(options, cx, |window, cx| {
            cx.new(|cx| SongEditor::new(song, library, window, cx))
        })?;
        handle.update(cx, |_, window, cx| {
            window.activate_window();
            editor.update(cx, |e, cx| e.title.update(cx, |s, cx| s.focus(window, cx)));
        })?;
        Ok((handle, editor))
    }

    fn new(song: Song, library: SharedLibrary, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = |value: &str, placeholder: &str, window: &mut Window, cx: &mut Context<Self>| {
            let (value, placeholder) = (value.to_string(), placeholder.to_string());
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
        };
        let title = field(&song.title, "Title", window, cx);
        let alt_title = field(song.alt_title.as_deref().unwrap_or(""), "Alternate title (optional)", window, cx);
        let authors = field(&song.authors.join("; "), "Authors, separated by ;", window, cx);
        let copyright = field(song.copyright.as_deref().unwrap_or(""), "Copyright", window, cx);
        let ccli = field(song.ccli_number.as_deref().unwrap_or(""), "CCLI number", window, cx);
        let verse_order = field(&song.verse_order_text(), "e.g. v1 c1 v2 c1 — empty: as written", window, cx);
        let initial_lyrics = if song.verses.is_empty() {
            String::new()
        } else {
            format_lyrics(&song.verses)
        };
        let lyrics = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("[v1]\nFirst verse…\n\n[c1]\nChorus…")
                .default_value(initial_lyrics)
        });

        let mut subscriptions = Vec::new();
        for input in [&title, &alt_title, &authors, &copyright, &ccli, &verse_order] {
            subscriptions.push(cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify()));
        }
        subscriptions.push(cx.subscribe(&lyrics, |_, _, _: &InputEvent, cx| cx.notify()));

        // Ask before closing with unsaved changes.
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let Some(editor) = this.upgrade() else { return true };
            let ready = {
                let e = editor.read(cx);
                e.discard_confirmed || !e.is_dirty(cx)
            };
            if !ready {
                editor.update(cx, |e, cx| e.confirm_discard(window, cx));
            }
            ready
        });

        Self {
            library,
            original: song,
            title,
            alt_title,
            authors,
            copyright,
            ccli,
            verse_order,
            lyrics,
            discard_confirmed: false,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The song as currently typed.
    fn draft(&self, cx: &App) -> Song {
        let text = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let optional = |e: &Entity<InputState>| Some(text(e)).filter(|s| !s.is_empty());
        Song {
            id: self.original.id,
            title: text(&self.title),
            alt_title: optional(&self.alt_title),
            authors: text(&self.authors)
                .split(';')
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(String::from)
                .collect(),
            copyright: optional(&self.copyright),
            ccli_number: optional(&self.ccli),
            verses: parse_lyrics(&self.lyrics.read(cx).value()),
            verse_order: Song::parse_verse_order(&text(&self.verse_order)),
        }
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.draft(cx) != self.original
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut song = self.draft(cx);
        if song.title.is_empty() || song.verses.is_empty() {
            window.push_notification(Notification::warning("A song needs a title and some lyrics."), cx);
            return;
        }
        let result = self.library.lock().unwrap().save(&song);
        match result {
            Ok(id) => {
                song.id = Some(id);
                self.original = song.clone();
                cx.emit(SongSaved(song));
                window.remove_window();
            }
            Err(e) => window.push_notification(Notification::error(format!("Couldn't save: {e}")), cx),
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_dirty(cx) {
            self.confirm_discard(window, cx);
        } else {
            window.remove_window();
        }
    }

    fn confirm_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            "Discard your changes to this song?",
            None,
            &["Discard", "Keep Editing"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(0) {
                let _ = this.update_in(cx, |this, window, _| {
                    this.discard_confirmed = true;
                    window.remove_window();
                });
            }
        })
        .detach();
    }
}

impl Focusable for SongEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SongEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.draft(cx);
        let problems = draft.problems();
        let slides = draft.slides();
        let dirty = draft != self.original;
        let theme = cx.theme();

        let label = |text: &'static str| {
            div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child(text)
        };

        v_flex()
            .key_context("SongEditor")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SaveSong, window, cx| this.save(window, cx)))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .gap_3()
                            .p_4()
                            .child(
                                h_flex()
                                    .gap_3()
                                    .child(v_flex().flex_1().gap_1().child(label("TITLE")).child(Input::new(&self.title)))
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .gap_1()
                                            .child(label("ALTERNATE TITLE"))
                                            .child(Input::new(&self.alt_title)),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_3()
                                    .child(v_flex().flex_1().gap_1().child(label("AUTHORS")).child(Input::new(&self.authors)))
                                    .child(
                                        v_flex().flex_1().gap_1().child(label("COPYRIGHT")).child(Input::new(&self.copyright)),
                                    )
                                    .child(v_flex().w(px(120.)).gap_1().child(label("CCLI #")).child(Input::new(&self.ccli))),
                            )
                            .child(v_flex().gap_1().child(label("VERSE ORDER")).child(Input::new(&self.verse_order)))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_h_0()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .child(div().flex_1().child(label("LYRICS")))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child("[v1] [c1] [b1] [p1] [e1] start a section · [---] splits a slide"),
                                            ),
                                    )
                                    .child(div().flex_1().min_h_0().child(Textarea::new(&self.lyrics).h_full())),
                            ),
                    )
                    .child(
                        v_flex()
                            .id("editor-preview")
                            .w(px(340.))
                            .flex_none()
                            .h_full()
                            .overflow_y_scroll()
                            .gap_2()
                            .p_4()
                            .border_l_1()
                            .border_color(theme.border)
                            .child(label("PREVIEW"))
                            .children(problems.iter().map(|p| {
                                div()
                                    .p_2()
                                    .rounded_md()
                                    .bg(theme.danger.opacity(0.15))
                                    .text_color(theme.danger)
                                    .text_xs()
                                    .child(p.clone())
                            }))
                            .when(slides.is_empty() && problems.is_empty(), |d| {
                                d.child(div().text_color(theme.muted_foreground).child("No slides yet."))
                            })
                            .children(slides.iter().map(|slide| {
                                v_flex()
                                    .p_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.secondary)
                                    .text_xs()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.muted_foreground)
                                            .child(slide.label.clone()),
                                    )
                                    .children(slide.lines.iter().map(|l| div().child(l.clone())))
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{} slide{}{}",
                                slides.len(),
                                if slides.len() == 1 { "" } else { "s" },
                                if dirty { " · unsaved changes" } else { "" }
                            )),
                    )
                    .child(Button::new("cancel").label("Cancel").on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))))
                    .child(
                        Button::new("save")
                            .primary()
                            .label("Save  ⌘S")
                            .disabled(draft.title.is_empty() || draft.verses.is_empty())
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use asaf_core::sample_song;
    use asaf_library::Library;
    use gpui::TestAppContext;
    use gpui::test::TestWindowExt as _;

    use super::*;

    fn open(cx: &mut TestAppContext, song: Song) -> (SharedLibrary, Entity<SongEditor>, AnyWindowHandle, Rc<RefCell<Vec<Song>>>) {
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let library: SharedLibrary = Arc::new(Mutex::new(Library::open_in_memory().unwrap()));
        let lib = library.clone();
        let (h, editor) = cx.update(|cx| SongEditor::open(song, lib, cx).unwrap());
        let saved = Rc::new(RefCell::new(Vec::new()));
        let sink = saved.clone();
        cx.update(|cx| {
            cx.subscribe(&editor, move |_, SongSaved(s), _| sink.borrow_mut().push(s.clone())).detach()
        });
        (library, editor, h, saved)
    }

    fn set(cx: &mut TestAppContext, h: AnyWindowHandle, f: impl FnOnce(&mut Window, &mut App)) {
        cx.update(|cx| h.update(cx, |_, w, cx| f(w, cx)).unwrap());
    }

    #[gpui::test]
    fn new_song_is_saved_with_cmd_s(cx: &mut TestAppContext) {
        let (library, editor, h, saved) = open(cx, Song::default());
        set(cx, h, |w, cx| {
            w.render_frame(cx);
            // The title field has focus on open.
            w.input("Mi Canción", cx);
            editor.update(cx, |e, cx| {
                e.lyrics.update(cx, |s, cx| s.set_value("[v1]\nUno\n[---]\nDos\n\n[c1]\nCoro", w, cx));
                e.verse_order.update(cx, |s, cx| s.set_value("v1 c1 v1", w, cx));
            });
            w.press("cmd-s", cx);
        });
        let saved = saved.borrow();
        assert_eq!(saved.len(), 1);
        let id = saved[0].id.expect("id assigned");
        let stored = library.lock().unwrap().get(id).unwrap().unwrap();
        assert_eq!(stored.title, "Mi Canción");
        let labels: Vec<_> = stored.slides().into_iter().map(|s| s.label).collect();
        assert_eq!(labels, ["V1.1", "V1.2", "C1", "V1.1", "V1.2"]);
    }

    #[gpui::test]
    fn save_needs_title_and_lyrics(cx: &mut TestAppContext) {
        let (library, _, h, saved) = open(cx, Song::default());
        set(cx, h, |w, cx| {
            w.render_frame(cx);
            w.input("Only a title", cx);
            w.press("cmd-s", cx);
        });
        assert!(saved.borrow().is_empty());
        assert_eq!(library.lock().unwrap().count().unwrap(), 0);
    }

    #[gpui::test]
    fn unchanged_song_is_not_dirty(cx: &mut TestAppContext) {
        let mut song = sample_song();
        song.id = Some(1);
        let (_, editor, h, _) = open(cx, song);
        set(cx, h, |w, cx| w.render_frame(cx));
        assert!(!cx.update(|cx| editor.read(cx).is_dirty(cx)));
        set(cx, h, |w, cx| w.input("!", cx));
        assert!(cx.update(|cx| editor.read(cx).is_dirty(cx)));
    }
}
