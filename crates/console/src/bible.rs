//! Bible mode: pick a translation, type a reference (or words to search),
//! ⏎ shows the passage as slides in the preview, ⏎ again puts it live.

use asaf_core::bible::passage_slides;
use asaf_core::{
    Bible, BibleLayout, BibleVerse, BookNames, LiveSnapshot, Passage, RefError, RefRange, ReferencePlacement,
    ServiceItem, Slide, VerseRef, VersesPerSlide, format_reference, parse_reference,
};
use asaf_library::VerseHit;
use gpui::component::button::{Button, ButtonVariants as _};
use gpui::component::checkbox::Checkbox;
use gpui::component::input::{Input, InputState};
use gpui::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, ScrollStrategy,
    StatefulInteractiveElement, Styled, UniformListScrollHandle, Window, div, px, uniform_list,
};

use crate::console::Console;
use crate::i18n::lang;
use crate::slides::slide_card;
use crate::transfer;

const HIT_LIMIT: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Idle,
    Hint(String),
    Error(String),
}

/// A passage fetched and laid out for the preview column.
pub(crate) struct PassagePreview {
    pub passage: Passage,
    /// "Juan 3:16-18".
    pub reference: String,
    /// Abbreviation of the translation, "RV1909".
    pub translation: String,
    pub verses: Vec<BibleVerse>,
    pub slides: Vec<Slide>,
}

impl PassagePreview {
    /// What the live panel shows: "Juan 3:16-18 (RV1909)".
    pub fn title(&self) -> String {
        format!("{} ({})", self.reference, self.translation)
    }
}

pub(crate) struct BibleState {
    pub bibles: Vec<Bible>,
    pub current: Option<usize>,
    names: Option<BookNames>,
    pub input: Entity<InputState>,
    pub status: Status,
    /// Text-search hits (when the input has no digits).
    pub hits: Vec<VerseHit>,
    pub selected_hit: Option<usize>,
    hits_scroll: UniformListScrollHandle,
    pub preview: Option<PassagePreview>,
    pub layout: BibleLayout,
    /// The input text that produced `preview`: ⏎ on unchanged text goes live.
    previewed_text: Option<String>,
}

impl BibleState {
    pub fn new(window: &mut Window, cx: &mut Context<Console>) -> Self {
        Self {
            bibles: Vec::new(),
            current: None,
            names: None,
            input: cx.new(|cx| InputState::new(window, cx).placeholder("Jn 3:16-18, Sal 23; 91:1-4, or words")),
            status: Status::Idle,
            hits: Vec::new(),
            selected_hit: None,
            hits_scroll: UniformListScrollHandle::new(),
            preview: None,
            layout: BibleLayout::default(),
            previewed_text: None,
        }
    }

    pub fn bible(&self) -> Option<&Bible> {
        self.current.and_then(|i| self.bibles.get(i))
    }
}

/// Input with a digit is a reference; anything else is a text search.
fn is_reference(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

impl Console {
    /// Reloads the installed translations, keeping (or switching to) `select`.
    pub(crate) fn refresh_bibles(&mut self, select: Option<i64>, cx: &mut Context<Self>) {
        let keep = select.or_else(|| self.bible.bible().and_then(|b| b.id));
        match self.library.lock().unwrap().bibles() {
            Ok(bibles) => self.bible.bibles = bibles,
            Err(e) => eprintln!("asaf: can't list Bibles: {e}"),
        }
        let index = keep
            .and_then(|id| self.bible.bibles.iter().position(|b| b.id == Some(id)))
            .or((!self.bible.bibles.is_empty()).then_some(0));
        self.choose_bible(index, cx);
        // A service passage may have been waiting for this translation.
        self.resolve_cue(cx);
    }

    /// Switches translation. The previewed passage and the search follow.
    pub(crate) fn choose_bible(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.bible.current = index.filter(|&i| i < self.bible.bibles.len());
        self.bible.names = self.bible.bible().map(Bible::names);
        if let Some(ranges) = self.bible.preview.as_ref().map(|p| p.passage.ranges.clone()) {
            self.preview_ranges(ranges, cx);
        }
        if !is_reference(&self.bible.input.read(cx).value()) {
            self.bible_text_changed(cx);
        }
        self.publish_remote_view();
        cx.notify();
    }

    pub(crate) fn bible_text_changed(&mut self, cx: &mut Context<Self>) {
        let text = self.bible.input.read(cx).value().trim().to_string();
        self.bible.hits.clear();
        self.bible.selected_hit = None;
        let Some(names) = &self.bible.names else {
            self.bible.status = Status::Error("Import a Bible first (Import… above).".into());
            cx.notify();
            return;
        };
        if text.is_empty() {
            self.bible.status = Status::Idle;
        } else if is_reference(&text) {
            self.bible.status = match parse_reference(&text, names) {
                Ok(ranges) => Status::Hint(format!("⏎ to show {}", format_reference(&ranges, names, |_, _| None))),
                Err(RefError::Incomplete) => Status::Idle,
                Err(e) => Status::Error(e.to_string()),
            };
        } else {
            let id = self.bible.bible().and_then(|b| b.id).unwrap_or_default();
            match self.library.lock().unwrap().search_bible(id, &text, HIT_LIMIT) {
                Ok(hits) => self.bible.hits = hits,
                Err(e) => eprintln!("asaf: Bible search failed: {e}"),
            }
            let n = self.bible.hits.len();
            self.bible.status = if n == 0 {
                Status::Error("No verses contain those words.".into())
            } else {
                Status::Hint(format!("{n}{} verse{}", if n == HIT_LIMIT { "+" } else { "" }, if n == 1 { "" } else { "s" }))
            };
            self.select_hit((n > 0).then_some(0), cx);
        }
        cx.notify();
    }

    /// ⏎ in the Bible box. A reference is shown in the preview; ⏎ again (or
    /// ⌘⏎ straight away) puts it live. With search hits, ⏎ goes live.
    pub(crate) fn bible_enter(&mut self, go_live: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.bible.input.read(cx).value().trim().to_string();
        if !is_reference(&text) {
            if self.bible.selected_hit.is_some() {
                self.bible_go_live(0, window, cx);
            }
            return;
        }
        let shown = self.bible.preview.is_some() && self.bible.previewed_text.as_deref() == Some(text.as_str());
        if shown || self.show_reference(&text, cx) && go_live {
            self.bible_go_live(0, window, cx);
        }
    }

    /// Parses `text`, fetches its verses and previews them. Returns whether
    /// there is something to show.
    pub(crate) fn show_reference(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(names) = &self.bible.names else {
            self.bible.status = Status::Error("Import a Bible first (Import… above).".into());
            return false;
        };
        let ok = match parse_reference(text, names) {
            Ok(ranges) => self.preview_ranges(ranges, cx),
            Err(e) => {
                self.bible.status = Status::Error(e.to_string());
                false
            }
        };
        self.bible.previewed_text = ok.then(|| text.to_string());
        cx.notify();
        ok
    }

    /// Fetches `ranges` from the current translation into the preview.
    fn preview_ranges(&mut self, ranges: Vec<RefRange>, cx: &mut Context<Self>) -> bool {
        let Some(bible) = self.bible.bible().cloned() else { return false };
        match self.lay_out_passage(&bible, ranges) {
            Ok(preview) => {
                let (verses, slides) = (preview.verses.len(), preview.slides.len());
                self.bible.status = Status::Hint(format!(
                    "{verses} verse{} on {slides} slide{} · ⏎ again to go live, ⇧⏎ to add to the service",
                    if verses == 1 { "" } else { "s" },
                    if slides == 1 { "" } else { "s" },
                ));
                self.bible.preview = Some(preview);
                cx.notify();
                true
            }
            Err(e) => {
                self.bible.status = Status::Error(e);
                self.bible.preview = None;
                cx.notify();
                false
            }
        }
    }

    /// Fetches `ranges` from `bible` and lays them out with the current
    /// layout. Errors are worded for the operator.
    pub(crate) fn lay_out_passage(&self, bible: &Bible, ranges: Vec<RefRange>) -> Result<PassagePreview, String> {
        let names = bible.names();
        let bible_id = bible.id.unwrap_or_default();
        let verses = self
            .library
            .lock()
            .unwrap()
            .passage_verses(bible_id, &ranges)
            .map_err(|e| format!("Can't read the Bible: {e}"))?;
        let reference = format_reference(&ranges, &names, |b, c| asaf_core::bible::last_verse(&verses, b, c));
        if verses.is_empty() {
            return Err(format!("{reference} isn't in {}.", bible.abbreviation));
        }
        let slides = passage_slides(&verses, &ranges, &names, &bible.abbreviation, &self.bible.layout, &self.store.default_theme());
        Ok(PassagePreview {
            passage: Passage { bible_id, ranges },
            reference,
            translation: bible.abbreviation.clone(),
            verses,
            slides,
        })
    }

    /// ⇧⏎ in the Bible box: adds the typed reference (or the chosen search
    /// hit) to the service without showing it.
    pub(crate) fn add_bible_to_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.bible.input.read(cx).value().trim().to_string();
        let ok = if is_reference(&text) {
            self.show_reference(&text, cx)
        } else {
            self.bible.selected_hit.is_some() && self.bible.preview.is_some()
        };
        if !ok {
            return;
        }
        if let Some(p) = &self.bible.preview {
            let item = ServiceItem::passage(p.passage.bible_id, p.translation.clone(), p.reference.clone(), p.passage.ranges.clone());
            self.add_to_service(item, window, cx);
        }
    }

    pub(crate) fn select_hit(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.bible.selected_hit = index.filter(|&i| i < self.bible.hits.len());
        if let Some(i) = self.bible.selected_hit {
            let h = &self.bible.hits[i];
            let at = VerseRef { chapter: h.chapter, verse: h.verse };
            let range = RefRange { book: h.book, start: at, end: at };
            self.bible.hits_scroll.scroll_to_item(i, ScrollStrategy::Nearest);
            self.preview_ranges(vec![range], cx);
            self.bible.previewed_text = None;
        }
        cx.notify();
    }

    pub(crate) fn move_hit(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.bible.hits.is_empty() {
            return;
        }
        let i = self.bible.selected_hit.map_or(0, |i| i as isize + delta);
        self.select_hit(Some(i.clamp(0, self.bible.hits.len() as isize - 1) as usize), cx);
    }

    /// Changes how passages are laid out: the preview, the cued service item
    /// and a live passage all follow. A live passage keeps the verse that was
    /// on screen.
    pub(crate) fn set_bible_layout(&mut self, layout: BibleLayout, cx: &mut Context<Self>) {
        self.bible.layout = layout;
        let default_theme = self.store.default_theme();
        if let (Some(p), Some(names)) = (&mut self.bible.preview, &self.bible.names) {
            p.slides = passage_slides(&p.verses, &p.passage.ranges, names, &p.translation, &layout, &default_theme);
        }
        self.resolve_cue(cx);

        let live = self.store.snapshot();
        let Some(passage) = live.passage.clone() else { return };
        let Some(bible) = self.bible.bibles.iter().find(|b| b.id == Some(passage.bible_id)).cloned() else { return };
        if let Ok(p) = self.lay_out_passage(&bible, passage.ranges) {
            let index = live
                .slides
                .get(live.current)
                .and_then(|s| slide_with_verse_of(&p.slides, &s.label))
                .unwrap_or(0);
            self.store.load_passage(p.passage, live.title, p.slides, index);
        }
        cx.notify();
    }

    pub(crate) fn bible_go_live(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &self.bible.preview {
            self.store.load_passage(p.passage.clone(), p.title(), p.slides.clone(), index);
            self.focus.focus(window, cx);
            self.went_live_ad_hoc(cx);
        }
    }

    // ---- rendering --------------------------------------------------------------

    pub(crate) fn bible_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let theme = cx.theme();
        let state = &self.bible;
        let count = state.hits.len();
        let (status, status_color) = match &state.status {
            Status::Idle => (String::new(), theme.muted_foreground),
            Status::Hint(h) => (h.clone(), theme.muted_foreground),
            Status::Error(e) => (e.clone(), theme.danger),
        };

        v_flex()
            .size_full()
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().flex_1().font_weight(FontWeight::BOLD).child(l.t("Bible")))
                            .child(
                                Button::new("import-bible")
                                    .small()
                                    .label(l.t("Import…"))
                                    .tooltip(l.t("Import an OSIS or Zefania Bible"))
                                    .on_click(cx.listener(|_, _, window, cx| transfer::import_bible(window, cx))),
                            ),
                    )
                    .child(
                        h_flex().flex_wrap().gap_1().children(state.bibles.iter().enumerate().map(|(i, b)| {
                            Button::new(("bible", i))
                                .small()
                                .label(b.abbreviation.clone())
                                .tooltip(b.name.clone())
                                .selected(state.current == Some(i))
                                .when(state.current == Some(i), |b| b.primary())
                                .on_click(cx.listener(move |this, _, _, cx| this.choose_bible(Some(i), cx)))
                        })),
                    )
                    .child(div().key_context("BibleSearch").child(Input::new(&state.input).cleanable(true)))
                    .child(div().min_h(px(16.)).text_xs().text_color(status_color).child(status)),
            )
            .when(count > 0, |d| {
                d.child(
                uniform_list(
                    "bible-hits",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                        let theme = cx.theme();
                        let names = this.bible.names.as_ref();
                        range
                            .map(|i| {
                                let h = &this.bible.hits[i];
                                let selected = this.bible.selected_hit == Some(i);
                                let book = names.map_or("", |n| n.name(h.book));
                                v_flex()
                                    .id(("hit", i))
                                    .px_3()
                                    .py_1p5()
                                    .h(px(48.))
                                    .justify_center()
                                    .cursor_pointer()
                                    .when(selected, |d| d.bg(theme.list_active))
                                    .when(!selected, |d| d.hover(|s| s.bg(theme.list_hover)))
                                    .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
                                        this.select_hit(Some(i), cx);
                                        if ev.click_count() >= 2 {
                                            this.bible_go_live(0, window, cx);
                                        }
                                    }))
                                    .child(
                                        div()
                                            .truncate()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(format!("{book} {}:{}", h.chapter, h.verse)),
                                    )
                                    .child(div().truncate().text_xs().text_color(theme.muted_foreground).child(h.text.clone()))
                            })
                            .collect()
                    }),
                )
                .track_scroll(&state.hits_scroll)
                .flex_1(),
                )
            })
            .when(count == 0, |d| {
                d.child(
                    v_flex()
                        .flex_1()
                        .px_3()
                        .gap_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Jn 3:16-18  ·  Gn 1:30-2:3  ·  Sal 23; 91:1-4")
                        .child("1 Juan 4  ·  Ro 8:28, 38-39  ·  Jud 24-25")
                        .child(l.t("Words without numbers search the text.")),
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
                    .child(l.t("⏎ show, ⏎ again go live  ·  ⌘⏎ go live  ·  ⇧⏎ add to service  ·  Esc back")),
            )
    }

    pub(crate) fn passage_preview_column(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let look = self.store.default_theme();
        let l = lang(cx);
        let theme = cx.theme();
        let Some(p) = &self.bible.preview else {
            return v_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child(l.t("Type a reference and press ⏎."))
                .into_any_element();
        };
        let is_live = live.passage.as_ref() == Some(&p.passage);
        let name = self.bible.bible().map(|b| b.name.clone()).unwrap_or_default();

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
                                    .child(l.t("PREVIEW · BIBLE")),
                            )
                            .child(div().truncate().text_base().font_weight(FontWeight::BOLD).child(p.reference.clone()))
                            .child(div().truncate().text_xs().text_color(theme.muted_foreground).child(name)),
                    )
                    .child(
                        Button::new("add-passage")
                            .small()
                            .label(l.t("Add"))
                            .tooltip(l.t("Add to the service (⇧⏎)"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(p) = &this.bible.preview {
                                    let item = ServiceItem::passage(
                                        p.passage.bible_id,
                                        p.translation.clone(),
                                        p.reference.clone(),
                                        p.passage.ranges.clone(),
                                    );
                                    this.add_to_service(item, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("go-live")
                            .small()
                            .primary()
                            .label(l.t("Go live"))
                            .tooltip(l.t("Show the first slide (⏎ again, or ⌘⏎)"))
                            .on_click(cx.listener(|this, _, window, cx| this.bible_go_live(0, window, cx))),
                    ),
            )
            .child(self.layout_controls(cx))
            .child(
                v_flex()
                    .id("passage-slides")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_3()
                    .gap_2()
                    .children(p.slides.iter().enumerate().map(|(i, slide)| {
                        slide_card(("passage-slide", i), slide, is_live && i == live.current, &look, cx)
                            .on_click(cx.listener(move |this, _, window, cx| this.bible_go_live(i, window, cx)))
                    })),
            )
            .into_any_element()
    }

    /// Verses per slide, verse numbers and reference placement.
    pub(crate) fn layout_controls(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let l = lang(cx);
        let theme = cx.theme();
        let layout = self.bible.layout;
        let per_slide = |label: &'static str, value: VersesPerSlide, cx: &Context<Self>| {
            Button::new(label)
                .xsmall()
                .label(label)
                .selected(layout.verses_per_slide == value)
                .when(layout.verses_per_slide == value, |b| b.primary())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_bible_layout(BibleLayout { verses_per_slide: value, ..this.bible.layout }, cx)
                }))
        };
        h_flex()
            .flex_wrap()
            .gap_3()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .text_xs()
            .child(
                h_flex()
                    .gap_1()
                    .child(div().text_color(theme.muted_foreground).child(l.t("Verses per slide")))
                    .child(per_slide("1", VersesPerSlide::Count(1), cx))
                    .child(per_slide("2", VersesPerSlide::Count(2), cx))
                    .child(per_slide("3", VersesPerSlide::Count(3), cx))
                    .child(per_slide("Fit", VersesPerSlide::Fit, cx).label(l.t("Fit"))),
            )
            .child(
                Checkbox::new("verse-numbers").label(l.t("Verse numbers")).checked(layout.verse_numbers).on_click(
                    cx.listener(|this, checked: &bool, _, cx| {
                        this.set_bible_layout(BibleLayout { verse_numbers: *checked, ..this.bible.layout }, cx)
                    }),
                ),
            )
            .child(
                Checkbox::new("reference-last")
                    .label(l.t("Reference on last slide"))
                    .checked(layout.reference == ReferencePlacement::LastSlide)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let reference = if *checked { ReferencePlacement::LastSlide } else { ReferencePlacement::Footer };
                        this.set_bible_layout(BibleLayout { reference, ..this.bible.layout }, cx)
                    })),
            )
            .into_any_element()
    }
}

/// "Juan 3:16-18" → ("Juan 3", 16, 18); "Juan 3:16" → ("Juan 3", 16, 16).
fn label_span(label: &str) -> Option<(&str, u16, u16)> {
    let (chapter, verses) = label.rsplit_once(':')?;
    let (a, b) = verses.split_once('-').unwrap_or((verses, verses));
    Some((chapter, a.parse().ok()?, b.parse().ok()?))
}

/// The slide in `slides` showing the first verse of `old_label`.
fn slide_with_verse_of(slides: &[Slide], old_label: &str) -> Option<usize> {
    let (chapter, verse, _) = label_span(old_label)?;
    slides.iter().position(|s| {
        label_span(&s.label).is_some_and(|(c, a, b)| c == chapter && (a..=b).contains(&verse))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_slide_holding_a_verse() {
        let slide = |label: &str| Slide { lines: vec![], label: label.into(), footer: None, wrap: true };
        let slides = [slide("Juan 3:16-17"), slide("Juan 3:18"), slide("Juan 4:1-3")];
        assert_eq!(slide_with_verse_of(&slides, "Juan 3:17"), Some(0));
        assert_eq!(slide_with_verse_of(&slides, "Juan 3:18-20"), Some(1));
        assert_eq!(slide_with_verse_of(&slides, "Juan 4:2"), Some(2));
        assert_eq!(slide_with_verse_of(&slides, "Juan 5:1"), None);
        assert_eq!(label_span("1 Juan 4:7-8"), Some(("1 Juan 4", 7, 8)));
    }
}
