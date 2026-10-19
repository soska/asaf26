//! Theme editor window: the library's themes on the left, the chosen one's
//! settings in the middle, a preview on the right: the display page itself in
//! webviews, or the console's approximation when there are none. Saving writes
//! to the library and emits [`ThemesChanged`], so the console can update the
//! projector at once if the theme is on screen.

use std::rc::Rc;

use anyhow::Result;
use asaf_core::{
    Align, AreaKind, Blank, DEFAULT_THEME_NAME, DisplayState, Layout, PanelKind, Place, Slide, SlideKind, TextCase, Theme,
    TransitionKind, VAlign, parse_color, sample_song,
};
use asaf_fonts::FONTS;
use asaf_library::{ThemeError, ThemeFileError, with_theme_extension};
use asaf_library::media::{IMAGE_EXTENSIONS, VIDEO_EXTENSIONS};
use asaf_projector::EmbeddedPage;
use gpui::component::button::{Button, ButtonVariants as _};
use gpui::component::input::{Input, InputEvent, InputState};
use gpui::component::notification::Notification;
use gpui::component::{ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, PathPromptOptions, PromptLevel, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledImage as _, Subscription, TestSupportExt as _, TitlebarOptions, Window, WindowBounds, WindowOptions, canvas, div, px,
    size,
};

use crate::i18n::{Lang, en, lang};
use crate::icons::IconName;
use crate::preview::{background_fill, color, image_source, screen};
use crate::{SaveTheme, SharedLibrary};

/// Emitted after themes were saved, renamed, deleted or a new default chosen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemesChanged {
    /// `(old, new)` when a theme was renamed.
    pub renamed: Option<(String, String)>,
    pub deleted: Option<String>,
}

/// The theme's files: background image and video, and logo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaSlot {
    Background,
    Video,
    Logo,
}

impl MediaSlot {
    fn extensions(self) -> &'static [&'static str] {
        if self == MediaSlot::Video { VIDEO_EXTENSIONS } else { IMAGE_EXTENSIONS }
    }

    fn url(self, t: &mut Theme) -> &mut Option<String> {
        match self {
            MediaSlot::Background => &mut t.background_image,
            MediaSlot::Video => &mut t.background_video,
            MediaSlot::Logo => &mut t.logo,
        }
    }

    /// For element ids.
    fn key(self) -> &'static str {
        match self {
            MediaSlot::Background => "bg",
            MediaSlot::Video => "video",
            MediaSlot::Logo => "logo",
        }
    }
}

/// The text fields of the form, each tied to one theme setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Name,
    Font,
    SizeMin,
    SizeMax,
    LineHeight,
    LetterSpacing,
    Color,
    StrongColor,
    EmColor,
    OutlineColor,
    Outline,
    ShadowColor,
    ShadowBlur,
    ShadowOffset,
    MarginX,
    MarginY,
    Background,
    Gradient,
    Dim,
    TransitionMs,
    TitleSize,
    TitleColor,
    ReferenceSize,
    ReferenceColor,
    CreditsSize,
    CreditsColor,
    /// Used only while the theme has a scripture layout of its own.
    ScriptureSizeMin,
    ScriptureSizeMax,
    PanelColor,
    PanelPadding,
    PanelRadius,
}

impl Field {
    pub const ALL: [Field; 31] = [
        Field::Name,
        Field::Font,
        Field::SizeMin,
        Field::SizeMax,
        Field::LineHeight,
        Field::LetterSpacing,
        Field::Color,
        Field::StrongColor,
        Field::EmColor,
        Field::OutlineColor,
        Field::Outline,
        Field::ShadowColor,
        Field::ShadowBlur,
        Field::ShadowOffset,
        Field::MarginX,
        Field::MarginY,
        Field::Background,
        Field::Gradient,
        Field::Dim,
        Field::TransitionMs,
        Field::TitleSize,
        Field::TitleColor,
        Field::ReferenceSize,
        Field::ReferenceColor,
        Field::CreditsSize,
        Field::CreditsColor,
        Field::ScriptureSizeMin,
        Field::ScriptureSizeMax,
        Field::PanelColor,
        Field::PanelPadding,
        Field::PanelRadius,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap()
    }

    /// The size and colour fields of a text area.
    fn of_area(kind: AreaKind) -> (Field, Field) {
        match kind {
            AreaKind::Title => (Field::TitleSize, Field::TitleColor),
            AreaKind::Reference => (Field::ReferenceSize, Field::ReferenceColor),
            AreaKind::Credits => (Field::CreditsSize, Field::CreditsColor),
        }
    }

    /// The text area a field belongs to.
    fn area(self) -> Option<AreaKind> {
        AreaKind::ALL.into_iter().find(|&k| {
            let (size, color) = Field::of_area(k);
            self == size || self == color
        })
    }

    /// The field's text for `t`.
    pub fn get(self, t: &Theme) -> String {
        match self {
            Field::Name => t.name.clone(),
            Field::Font => t.font_family.clone(),
            Field::SizeMin => t.size_min_vh.to_string(),
            Field::SizeMax => t.size_max_vh.to_string(),
            Field::LineHeight => t.line_height.to_string(),
            Field::LetterSpacing => t.letter_spacing_em.to_string(),
            Field::Color => t.color.clone(),
            Field::StrongColor => t.strong_color.clone(),
            Field::EmColor => t.em_color.clone(),
            Field::OutlineColor => t.outline_color.clone(),
            Field::Outline => t.outline_vh.to_string(),
            Field::ShadowColor => t.shadow_color.clone(),
            Field::ShadowBlur => t.shadow_blur_vh.to_string(),
            Field::ShadowOffset => t.shadow_offset_vh.to_string(),
            Field::MarginX => t.margin_x_pct.to_string(),
            Field::MarginY => t.margin_y_pct.to_string(),
            Field::Background => t.background_color.clone(),
            Field::Gradient => t.gradient_color.clone(),
            Field::Dim => (t.background_dim * 100.0).round().to_string(),
            Field::TransitionMs => t.transition_ms.to_string(),
            Field::TitleSize | Field::ReferenceSize | Field::CreditsSize => t.area(self.area().unwrap()).size_vh.to_string(),
            Field::TitleColor | Field::ReferenceColor | Field::CreditsColor => t.area(self.area().unwrap()).color.clone(),
            Field::ScriptureSizeMin => t.layout(SlideKind::Scripture).size_min_vh.to_string(),
            Field::ScriptureSizeMax => t.layout(SlideKind::Scripture).size_max_vh.to_string(),
            Field::PanelColor => t.panel_color.clone(),
            Field::PanelPadding => t.panel_padding_vh.to_string(),
            Field::PanelRadius => t.panel_radius_vh.to_string(),
        }
    }

    /// Puts `text` into `t`, or says what's wrong with it.
    pub fn set(self, t: &mut Theme, text: &str) -> Result<(), &'static str> {
        let text = text.trim();
        let number = |lo: f32, hi: f32| -> Result<f32, &'static str> {
            text.replace(',', ".").parse::<f32>().ok().filter(|v| v.is_finite() && (lo..=hi).contains(v)).ok_or("number")
        };
        let colour = || -> Result<String, &'static str> { parse_color(text).map(|_| text.to_string()).ok_or("color") };
        // Empty: the text colour.
        let optional_colour = || if text.is_empty() { Ok(String::new()) } else { colour() };
        match self {
            Field::Name if text.is_empty() => return Err("name"),
            Field::Name => t.name = text.to_string(),
            Field::Font if text.is_empty() => return Err("font"),
            Field::Font => t.font_family = text.to_string(),
            Field::SizeMin => t.size_min_vh = number(1.0, 40.0)?,
            Field::SizeMax => t.size_max_vh = number(1.0, 40.0)?,
            Field::LineHeight => t.line_height = number(0.8, 3.0)?,
            Field::LetterSpacing => {
                let (lo, hi) = asaf_core::theme::LETTER_SPACING_RANGE_EM;
                t.letter_spacing_em = number(lo, hi)?
            }
            Field::Color => t.color = colour()?,
            Field::StrongColor => t.strong_color = optional_colour()?,
            Field::EmColor => t.em_color = optional_colour()?,
            Field::OutlineColor => t.outline_color = colour()?,
            Field::Outline => t.outline_vh = number(0.0, 3.0)?,
            Field::ShadowColor => t.shadow_color = colour()?,
            Field::ShadowBlur => t.shadow_blur_vh = number(0.0, 10.0)?,
            Field::ShadowOffset => t.shadow_offset_vh = number(0.0, 10.0)?,
            Field::MarginX => t.margin_x_pct = number(0.0, 40.0)?,
            Field::MarginY => t.margin_y_pct = number(0.0, 40.0)?,
            Field::Background => t.background_color = colour()?,
            Field::Gradient => t.gradient_color = optional_colour()?,
            Field::Dim => t.background_dim = number(0.0, asaf_core::theme::MAX_DIM * 100.0)? / 100.0,
            Field::TransitionMs => {
                t.transition_ms = text.parse::<u32>().ok().filter(|v| *v <= asaf_core::theme::MAX_TRANSITION_MS).ok_or("number")?
            }
            Field::TitleSize | Field::ReferenceSize | Field::CreditsSize => {
                let (lo, hi) = asaf_core::theme::AREA_SIZE_RANGE_VH;
                t.area_mut(self.area().unwrap()).size_vh = number(lo, hi)?
            }
            Field::TitleColor | Field::ReferenceColor | Field::CreditsColor => {
                t.area_mut(self.area().unwrap()).color = optional_colour()?
            }
            Field::ScriptureSizeMin | Field::ScriptureSizeMax => {
                let Some(l) = &mut t.scripture else { return Ok(()) };
                let v = number(1.0, 40.0)?;
                if self == Field::ScriptureSizeMin { l.size_min_vh = v } else { l.size_max_vh = v }
            }
            Field::PanelColor => t.panel_color = colour()?,
            Field::PanelPadding => t.panel_padding_vh = number(0.0, asaf_core::theme::MAX_PANEL_PADDING_VH)?,
            Field::PanelRadius => t.panel_radius_vh = number(0.0, asaf_core::theme::MAX_PANEL_RADIUS_VH)?,
        }
        Ok(())
    }

    fn label(self) -> &'static str {
        match self {
            Field::Name => en("Name"),
            Field::Font => en("Font"),
            Field::SizeMin => en("Smallest text (% of height)"),
            Field::SizeMax => en("Largest text (% of height)"),
            Field::LineHeight => en("Line spacing"),
            Field::LetterSpacing => en("Letter spacing (em)"),
            Field::Color => en("Text color"),
            Field::StrongColor => en("Strong color (**word**)"),
            Field::EmColor => en("Emphasis color (__word__)"),
            Field::OutlineColor => en("Outline color"),
            Field::Outline => en("Outline width"),
            Field::ShadowColor => en("Shadow color"),
            Field::ShadowBlur => en("Shadow blur"),
            Field::ShadowOffset => en("Shadow offset"),
            Field::MarginX => en("Side margins (%)"),
            Field::MarginY => en("Top and bottom margins (%)"),
            Field::Background => en("Background color"),
            Field::Gradient => en("Gradient to (empty: none)"),
            Field::Dim => en("Darken (%)"),
            Field::TransitionMs => en("Duration (ms)"),
            Field::TitleSize => en("Title size (%)"),
            Field::TitleColor => en("Title color"),
            Field::ReferenceSize => en("Reference size (%)"),
            Field::ReferenceColor => en("Reference color"),
            Field::CreditsSize => en("Credits size (%)"),
            Field::CreditsColor => en("Credits color"),
            Field::ScriptureSizeMin => en("Smallest scripture text (%)"),
            Field::ScriptureSizeMax => en("Largest scripture text (%)"),
            Field::PanelColor => en("Panel color"),
            Field::PanelPadding => en("Panel padding"),
            Field::PanelRadius => en("Corner radius"),
        }
    }
}

pub struct ThemeEditor {
    library: SharedLibrary,
    /// The library's themes, by name.
    pub(crate) themes: Vec<Theme>,
    pub(crate) default_name: String,
    /// Index into `themes` of the theme being edited.
    pub(crate) selected: usize,
    /// The edited theme's settings that aren't text fields (weight,
    /// alignment, transition, images); text fields are read from the inputs.
    choice: Theme,
    inputs: Vec<Entity<InputState>>,
    /// The display page showing the sample slide and the logo screen, when
    /// webviews could be made.
    pages: Option<[Rc<EmbeddedPage>; 2]>,
    /// Which sample slide the preview shows.
    pub(crate) preview_kind: SlideKind,
    discard_confirmed: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ThemesChanged> for ThemeEditor {}

const SAMPLE_SLIDE: usize = 1; // the chorus of the sample hymn: four lines

/// What the preview shows for `kind`: the sample hymn's chorus, with
/// emphasis to show off its colours, or a verse.
fn sample_slide(kind: SlideKind) -> Slide {
    match kind {
        SlideKind::Lyrics => {
            let mut slide = sample_song().slides().swap_remove(SAMPLE_SLIDE);
            slide.lines[0] = "**This is my story**, this is my song,".into();
            slide.lines[1] = "Praising my __Savior__ all the day long;".into();
            slide
        }
        SlideKind::Scripture => Slide {
            lines: vec!["¹⁶ For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.".into()],
            label: "John 3:16".into(),
            kind,
            reference: Some("John 3:16 (KJV)".into()),
            ..Default::default()
        },
    }
}

impl ThemeEditor {
    /// Opens the editor on the theme called `select` (else the default).
    /// `display_url` is the display page to preview with; without it the
    /// preview is the console's approximation.
    pub fn open(
        library: SharedLibrary,
        select: Option<String>,
        display_url: Option<&str>,
        cx: &mut App,
    ) -> Result<(AnyWindowHandle, Entity<Self>)> {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1240.), px(780.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some(lang(cx).t("Themes").into()), ..Default::default() }),
            window_min_size: Some(size(px(1000.), px(600.))),
            ..Default::default()
        };
        let (handle, editor) = gpui::open_window(options, cx, |window, cx| {
            cx.new(|cx| ThemeEditor::new(library, select, display_url, window, cx))
        })?;
        handle.update(cx, |_, window, cx| {
            window.activate_window();
            editor.read(cx).focus.clone().focus(window, cx);
        })?;
        Ok((handle, editor))
    }

    pub(crate) fn new(
        library: SharedLibrary,
        select: Option<String>,
        display_url: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let pages = display_url.and_then(|url| {
            let url = format!("{url}?preview");
            let page = || EmbeddedPage::new(&url, window).map(Rc::new);
            match page().and_then(|slide| Ok([slide, page()?])) {
                Ok(pages) => Some(pages),
                Err(e) => {
                    eprintln!("asaf: no webview for the theme preview, showing an approximation: {e:#}");
                    None
                }
            }
        });
        let inputs: Vec<Entity<InputState>> = Field::ALL.iter().map(|_| cx.new(|cx| InputState::new(window, cx))).collect();
        let mut subscriptions: Vec<Subscription> =
            inputs.iter().map(|i| cx.subscribe(i, |_, _, _: &InputEvent, cx| cx.notify())).collect();
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let Some(editor) = this.upgrade() else { return true };
            let ready = {
                let e = editor.read(cx);
                e.discard_confirmed || !e.is_dirty(cx)
            };
            if !ready {
                editor.update(cx, |e, cx| e.confirm_discard(None, window, cx));
            }
            ready
        });
        subscriptions.push(cx.observe_global_in::<crate::i18n::UiLanguage>(window, |_, window, cx| {
            window.set_window_title(lang(cx).t("Themes"));
            cx.notify()
        }));
        let mut editor = Self {
            library,
            themes: Vec::new(),
            default_name: DEFAULT_THEME_NAME.into(),
            selected: 0,
            choice: Theme::default(),
            inputs,
            pages,
            preview_kind: SlideKind::Lyrics,
            discard_confirmed: false,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        editor.reload(select.as_deref(), window, cx);
        editor
    }

    pub(crate) fn input(&self, field: Field) -> &Entity<InputState> {
        &self.inputs[field.index()]
    }

    /// Re-reads the themes and shows the one called `select` (or the same
    /// one as before, or the default).
    fn reload(&mut self, select: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let (themes, default_name) = {
            let lib = self.library.lock().unwrap();
            (lib.themes().unwrap_or_default(), lib.default_theme().map(|t| t.name).unwrap_or_default())
        };
        let current = self.themes.get(self.selected).map(|t| t.name.clone());
        let want = select.map(String::from).or(current).unwrap_or_else(|| default_name.clone());
        self.selected = themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(&want))
            .or_else(|| themes.iter().position(|t| t.name == default_name))
            .unwrap_or(0);
        self.themes = themes;
        self.default_name = default_name;
        self.load_selected(window, cx);
    }

    fn stored(&self) -> Theme {
        self.themes.get(self.selected).cloned().unwrap_or_default()
    }

    fn load_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let theme = self.stored();
        for (field, input) in Field::ALL.iter().zip(&self.inputs) {
            let value = field.get(&theme);
            input.update(cx, |s, cx| s.set_value(value, window, cx));
        }
        self.choice = theme;
        cx.notify();
    }

    /// The theme as currently edited, and which fields can't be used (their
    /// stored value stands in for them meanwhile).
    pub(crate) fn draft(&self, cx: &App) -> (Theme, Vec<Field>) {
        let mut theme = self.choice.clone();
        let mut bad = Vec::new();
        for (field, input) in Field::ALL.iter().zip(&self.inputs) {
            if field.set(&mut theme, &input.read(cx).value()).is_err() {
                bad.push(*field);
            }
        }
        if theme.size_max_vh < theme.size_min_vh {
            bad.push(Field::SizeMax);
        }
        if theme.scripture.as_ref().is_some_and(|l| l.size_max_vh < l.size_min_vh) {
            bad.push(Field::ScriptureSizeMax);
        }
        (theme, bad)
    }

    pub(crate) fn is_dirty(&self, cx: &App) -> bool {
        self.draft(cx).0 != self.stored()
    }

    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let l = lang(cx);
        let (theme, bad) = self.draft(cx);
        if !bad.is_empty() {
            let names: Vec<&str> = bad.iter().map(|f| l.t(f.label())).collect();
            let note = l.f("Fix these first: {fields}", &[("fields", &names.join(", "))]);
            window.push_notification(Notification::warning(note), cx);
            return;
        }
        let previous = self.stored().name;
        let result = self.library.lock().unwrap().save_theme(&theme, Some(&previous));
        match result {
            Ok(saved) => {
                let renamed = (!saved.name.eq(&previous)).then(|| (previous, saved.name.clone()));
                self.reload(Some(&saved.name), window, cx);
                cx.emit(ThemesChanged { renamed, deleted: None });
                window.push_notification(Notification::success(l.f("Saved “{name}”", &[("name", &saved.name)])), cx);
            }
            Err(e) => window.push_notification(Notification::error(theme_error(l, &e)), cx),
        }
    }

    /// A new theme: a copy of the edited one (`copy`) or of the defaults.
    fn add(&mut self, copy: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_dirty(cx) {
            return self.confirm_discard(Some(Box::new(move |this, w, cx| this.add(copy, w, cx))), window, cx);
        }
        let l = lang(cx);
        let base = if copy { self.stored() } else { Theme::default() };
        let result = {
            let mut lib = self.library.lock().unwrap();
            let stem = if copy { l.f("{name} copy", &[("name", &base.name)]) } else { l.t("New theme").to_string() };
            lib.unused_theme_name(&stem)
                .map_err(|e| ThemeError::Storage(e.to_string()))
                .and_then(|name| lib.save_theme(&Theme { name, ..base }, None))
        };
        match result {
            Ok(theme) => {
                self.reload(Some(&theme.name), window, cx);
                cx.emit(ThemesChanged::default());
                self.input(Field::Name).update(cx, |s, cx| s.focus(window, cx));
            }
            Err(e) => window.push_notification(Notification::error(theme_error(l, &e)), cx),
        }
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let l = lang(cx);
        let name = self.stored().name;
        let message = l.f("Delete the theme “{name}”?", &[("name", &name)]);
        let answer = window.prompt(PromptLevel::Warning, &message, None, &[l.t("Delete"), l.t("Cancel")], cx);
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                let result = this.library.lock().unwrap().delete_theme(&name);
                match result {
                    Ok(()) => {
                        this.reload(None, window, cx);
                        cx.emit(ThemesChanged { renamed: None, deleted: Some(name) });
                    }
                    Err(e) => window.push_notification(Notification::error(theme_error(lang(cx), &e)), cx),
                }
            });
        })
        .detach();
    }

    fn make_default(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.stored().name;
        let result = {
            let mut lib = self.library.lock().unwrap();
            lib.settings().and_then(|mut s| {
                s.default_theme = name.clone();
                lib.save_settings(&s)
            })
        };
        match result {
            Ok(()) => {
                self.default_name = name;
                cx.emit(ThemesChanged::default());
                cx.notify();
            }
            Err(e) => {
                let note = lang(cx).f("Couldn't save the settings: {error}", &[("error", &e.to_string())]);
                window.push_notification(Notification::error(note), cx)
            }
        }
    }

    pub(crate) fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.selected || index >= self.themes.len() {
            return;
        }
        if self.is_dirty(cx) {
            return self.confirm_discard(Some(Box::new(move |this, w, cx| this.select(index, w, cx))), window, cx);
        }
        self.selected = index;
        self.load_selected(window, cx);
    }

    /// Asks before throwing away changes; then closes the window (`then`
    /// is `None`) or goes on with `then`.
    fn confirm_discard(&mut self, then: Option<Box<AfterDiscard>>, window: &mut Window, cx: &mut Context<Self>) {
        let l = lang(cx);
        let message = l.f("Discard your changes to “{name}”?", &[("name", &self.stored().name)]);
        let answer = window.prompt(PromptLevel::Warning, &message, None, &[l.t("Discard"), l.t("Keep Editing")], cx);
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| match then {
                None => {
                    this.discard_confirmed = true;
                    window.remove_window();
                }
                Some(then) => {
                    this.load_selected(window, cx);
                    then(this, window, cx);
                }
            });
        })
        .detach();
    }

    /// Saves the theme (as stored) and its media to a file the operator
    /// picks, for another computer.
    fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let theme = self.stored();
        let name = format!("{}.{}", file_stem(&theme.name), asaf_library::THEME_EXTENSION);
        let path = cx.prompt_for_new_path(&crate::transfer::documents_dir(), Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = path.await else { return };
            let _ = this.update_in(cx, |this, window, cx| this.export_to(&with_theme_extension(path), window, cx));
        })
        .detach();
    }

    pub(crate) fn export_to(&mut self, path: &std::path::Path, window: &mut Window, cx: &mut Context<Self>) {
        let theme = self.stored();
        let result = self.library.lock().unwrap().export_theme(&theme, path);
        let l = lang(cx);
        let note = match result {
            Ok(()) => Notification::success(l.f("Exported “{name}”", &[("name", &theme.name)])),
            Err(e) => Notification::error(theme_file_error(l, &e)),
        };
        window.push_notification(note, cx);
    }

    /// Adds the theme in a file the operator picks, as a new theme.
    fn import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_dirty(cx) {
            return self.confirm_discard(Some(Box::new(|this, w, cx| this.import(w, cx))), window, cx);
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(lang(cx).t("Import").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = paths.await else { return };
            let Some(path) = paths.pop() else { return };
            let _ = this.update_in(cx, |this, window, cx| this.import_from(&path, window, cx));
        })
        .detach();
    }

    pub(crate) fn import_from(&mut self, path: &std::path::Path, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.library.lock().unwrap().import_theme(path);
        let l = lang(cx);
        match result {
            Ok(theme) => {
                self.reload(Some(&theme.name), window, cx);
                cx.emit(ThemesChanged::default());
                window.push_notification(Notification::success(l.f("Imported “{name}”", &[("name", &theme.name)])), cx);
            }
            Err(e) => window.push_notification(Notification::error(theme_file_error(l, &e)), cx),
        }
    }

    /// Imports an image or video into the media folder for `slot`.
    fn choose_media(&mut self, slot: MediaSlot, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(lang(cx).t("Choose").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = paths.await else { return };
            let Some(path) = paths.pop() else { return };
            let _ = this.update_in(cx, |this, window, cx| this.set_media_file(slot, &path, window, cx));
        })
        .detach();
    }

    pub(crate) fn set_media_file(&mut self, slot: MediaSlot, path: &std::path::Path, window: &mut Window, cx: &mut Context<Self>) {
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        if !slot.extensions().contains(&ext.as_str()) {
            let (file, types) = (path.display().to_string(), slot.extensions().join(", "));
            let note = if slot == MediaSlot::Video {
                lang(cx).f("{file} isn't a video Asaf can play (use {types}).", &[("file", &file), ("types", &types)])
            } else {
                lang(cx).f("{file} isn't an image Asaf can show (use {types}).", &[("file", &file), ("types", &types)])
            };
            return window.push_notification(Notification::error(note), cx);
        }
        let result = self.library.lock().unwrap().import_media(path);
        match result {
            Ok(url) => {
                *slot.url(&mut self.choice) = Some(url);
                cx.notify();
            }
            Err(e) => window.push_notification(Notification::error(e), cx),
        }
    }

    fn set_choice(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Theme)) {
        f(&mut self.choice);
        cx.notify();
    }

    fn set_scripture(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Layout)) {
        if let Some(layout) = &mut self.choice.scripture {
            f(layout);
            cx.notify();
        }
    }

    /// Gives scripture a layout of its own, starting from the lyrics' as
    /// edited, or (`own` false) puts it back to the lyrics'.
    pub(crate) fn set_own_scripture_layout(&mut self, own: bool, window: &mut Window, cx: &mut Context<Self>) {
        if own == self.choice.scripture.is_some() {
            return;
        }
        let mut draft = self.draft(cx).0;
        draft.scripture = own.then(|| draft.layout(SlideKind::Lyrics));
        self.choice.scripture = draft.scripture.clone();
        for field in [Field::ScriptureSizeMin, Field::ScriptureSizeMax] {
            let value = field.get(&draft);
            self.input(field).clone().update(cx, |s, cx| s.set_value(value, window, cx));
        }
        self.preview_kind = SlideKind::Scripture;
        cx.notify();
    }
}

type AfterDiscard = dyn FnOnce(&mut ThemeEditor, &mut Window, &mut Context<ThemeEditor>);

impl Focusable for ThemeEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// What a theme image URL looks like in the form: its file name.
fn image_name(url: &Option<String>, l: Lang) -> String {
    match url.as_deref() {
        None => l.t("None").to_string(),
        Some("/assets/background.svg" | "/assets/logo.svg" | "/assets/logo-dark.svg") => l.t("Built in").to_string(),
        Some(u) => u.rsplit('/').next().unwrap_or(u).to_string(),
    }
}

/// The call that shows `slide` (or `blank`) in `theme` on a preview page.
fn preview_script(theme: &Theme, slide: Option<Slide>, blank: Blank) -> String {
    let state = DisplayState::preview(theme, slide, blank);
    format!("asafPreview({})", serde_json::to_string(&state).expect("serialize a display state"))
}

impl Render for ThemeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let (draft, bad) = self.draft(cx);
        let dirty = draft != self.stored();
        let sample = sample_slide(self.preview_kind);
        let background = image_source(draft.background_image.as_deref(), &self.library);
        let logo = image_source(draft.logo.as_deref(), &self.library);
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;

        let field = |f: Field, width: f32| {
            v_flex()
                .w(px(width))
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(if bad.contains(&f) { theme.danger } else { muted })
                        .child(l.t(f.label())),
                )
                .child(Input::new(&self.inputs[f.index()]).small())
        };
        let or_text = |c: &str| if c.trim().is_empty() { draft.color.clone() } else { c.to_string() };
        let or_bg = |c: &str| if c.trim().is_empty() { draft.background_color.clone() } else { c.to_string() };
        let swatch = |c: &str| {
            div()
                .flex_none()
                .size(px(26.))
                .mt(px(18.))
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .bg(color(c))
        };
        let heading = |text: &'static str| {
            div().pt_2().text_xs().font_weight(FontWeight::BOLD).text_color(muted).child(l.t(text).to_uppercase())
        };
        let choice = |id: SharedString, label: &str, on: bool| Button::new(id).xsmall().label(label.to_string()).selected(on);

        let list = v_flex()
            .id("theme-list")
            .w(px(210.))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .gap_0p5()
            .p_2()
            .border_r_1()
            .border_color(theme.border)
            .children(self.themes.iter().enumerate().map(|(i, t)| {
                let is_default = t.name == self.default_name;
                h_flex()
                    .id(("theme", i))
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .when(i == self.selected, |d| d.bg(theme.list_active))
                    .when(i != self.selected, |d| d.hover(|s| s.bg(theme.list_hover)))
                    .on_click(cx.listener(move |this, _, window, cx| this.select(i, window, cx)))
                    .child(div().size(px(14.)).flex_none().rounded_sm().bg(background_fill(t)))
                    .child(div().flex_1().min_w_0().truncate().child(t.name.clone()))
                    .when(is_default, |d| d.child(div().text_xs().text_color(muted).child(l.t("default"))))
            }));

        let fonts = h_flex().flex_wrap().gap_1().children(FONTS.iter().enumerate().map(|(i, f)| {
            let family = f.family;
            choice(format!("font-{i}").into(), family, draft.font_family == family).on_click(cx.listener(move |this, _, window, cx| {
                    this.inputs[Field::Font.index()].update(cx, |s, cx| s.set_value(family, window, cx));
                }))
        }));
        let weights = h_flex().gap_1().children([300u16, 400, 500, 600, 700, 800, 900].into_iter().map(|w| {
            choice(format!("weight-{w}").into(), &w.to_string(), draft.font_weight == w)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.font_weight = w)))
        }));
        let cases = h_flex().gap_1().children(TextCase::ALL.into_iter().map(|c| {
            let label = match c {
                TextCase::Normal => en("As typed"),
                TextCase::Upper => en("ALL CAPS"),
            };
            choice(format!("case-{}", c.css()).into(), l.t(label), draft.text_case == c)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.text_case = c)))
        }));
        let aligns = h_flex().gap_1().children(Align::ALL.into_iter().map(|a| {
            let label = match a {
                Align::Left => en("Left"),
                Align::Center => en("Center"),
                Align::Right => en("Right"),
            };
            choice(format!("align-{}", a.css()).into(), l.t(label), draft.align == a)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.align = a)))
        }));
        let valigns = h_flex().gap_1().children(VAlign::ALL.into_iter().map(|a| {
            let label = match a {
                VAlign::Top => en("Top"),
                VAlign::Middle => en("Middle"),
                VAlign::Bottom => en("Bottom"),
            };
            choice(format!("valign-{label}").into(), l.t(label), draft.valign == a)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.valign = a)))
        }));
        let transitions = h_flex().gap_1().children(TransitionKind::ALL.into_iter().map(|k| {
            let label = match k {
                TransitionKind::Fade => en("Fade"),
                TransitionKind::Slide => en("Slide up"),
                TransitionKind::Cut => en("Cut"),
            };
            choice(format!("transition-{}", k.as_str()).into(), l.t(label), draft.transition == k)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.transition = k)))
        }));
        let image_row = |slot: MediaSlot, url: &Option<String>, cx: &mut Context<Self>| {
            let (label, builtin) = match slot {
                MediaSlot::Background => (en("Background image"), Some("/assets/background.svg")),
                MediaSlot::Video => (en("Background video"), None),
                MediaSlot::Logo => (en("Logo (blank to logo)"), Some("/assets/logo.svg")),
            };
            let key = slot.key();
            h_flex()
                .gap_1()
                .child(div().w(px(120.)).text_xs().text_color(muted).child(l.t(label)))
                .child(div().flex_1().min_w_0().truncate().text_xs().child(image_name(url, l)))
                .child(Button::new(SharedString::from(format!("{key}-choose")))
                        .xsmall()
                        .label(l.t("Choose…"))
                        .on_click(cx.listener(move |this, _, window, cx| this.choose_media(slot, window, cx))))
                .children(builtin.map(|builtin| {
                    Button::new(SharedString::from(format!("{key}-builtin"))).xsmall().ghost().label(l.t("Built in")).on_click(cx.listener(move |this, _, _, cx| {
                        this.set_choice(cx, |t| *slot.url(t) = Some(builtin.to_string()))
                    }))
                }))
                .child(Button::new(SharedString::from(format!("{key}-clear"))).xsmall().ghost().label(l.t("None")).on_click(cx.listener(move |this, _, _, cx| {
                        this.set_choice(cx, |t| *slot.url(t) = None)
                    })))
        };
        let small_label = |text: &'static str, width: f32| div().w(px(width)).flex_none().text_xs().text_color(muted).child(l.t(text));
        let area_block = |kind: AreaKind, cx: &mut Context<Self>| {
            let a = draft.area(kind).clone();
            let name = kind.name();
            let (size, colour) = Field::of_area(kind);
            let places = h_flex().gap_1().children(Place::ALL.into_iter().map(|p| {
                let (label, key) = match p {
                    Place::Hidden => (en("Hidden"), "hidden"),
                    Place::Top => (en("Top"), "top"),
                    Place::Bottom => (en("Bottom"), "bottom"),
                };
                choice(format!("area-{name}-{key}").into(), l.t(label), a.place == p)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.area_mut(kind).place = p)))
            }));
            let aligns = h_flex().gap_1().children([None, Some(Align::Left), Some(Align::Center), Some(Align::Right)].map(|al| {
                let (label, key) = match al {
                    None => (en("Like the text"), "text"),
                    Some(Align::Left) => (en("Left"), "left"),
                    Some(Align::Center) => (en("Center"), "center"),
                    Some(Align::Right) => (en("Right"), "right"),
                };
                choice(format!("area-{name}-align-{key}").into(), l.t(label), a.align == al)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.area_mut(kind).align = al)))
            }));
            let cases = h_flex().gap_1().children(TextCase::ALL.into_iter().map(|c| {
                let label = match c {
                    TextCase::Normal => en("As typed"),
                    TextCase::Upper => en("ALL CAPS"),
                };
                choice(format!("area-{name}-case-{}", c.css()).into(), l.t(label), a.text_case == c)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.area_mut(kind).text_case = c)))
            }));
            let title = match kind {
                AreaKind::Title => en("Song title"),
                AreaKind::Reference => en("Scripture reference"),
                AreaKind::Credits => en("Credits (authors, copyright)"),
            };
            v_flex()
                .gap_1()
                .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child(l.t(title)))
                .child(h_flex().gap_3().flex_wrap().child(places).when(a.place != Place::Hidden, |d| d.child(aligns)))
                .when(a.place != Place::Hidden, |d| {
                    d.child(
                        h_flex()
                            .gap_2()
                            .items_end()
                            .child(field(size, 120.))
                            .child(field(colour, 120.))
                            .child(swatch(&or_text(&a.color)))
                            .child(div().pb_1().child(cases)),
                    )
                })
        };
        let areas: Vec<_> = AreaKind::ALL.into_iter().map(|k| area_block(k, cx)).collect();

        let own = draft.scripture.clone();
        let scripture_modes = h_flex().gap_1().children([(false, en("Like lyrics")), (true, en("Its own layout"))].map(|(on, label)| {
            choice(if on { "scripture-own" } else { "scripture-same" }.into(), l.t(label), own.is_some() == on)
                .on_click(cx.listener(move |this, _, window, cx| this.set_own_scripture_layout(on, window, cx)))
        }));
        let scripture_layout = own.map(|layout| {
            let aligns = h_flex().gap_1().children(Align::ALL.into_iter().map(|a| {
                let label = match a {
                    Align::Left => en("Left"),
                    Align::Center => en("Center"),
                    Align::Right => en("Right"),
                };
                choice(format!("scripture-align-{}", a.css()).into(), l.t(label), layout.align == a)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_scripture(cx, |l| l.align = a)))
            }));
            let valigns = h_flex().gap_1().children(VAlign::ALL.into_iter().map(|a| {
                let (label, key) = match a {
                    VAlign::Top => (en("Top"), "top"),
                    VAlign::Middle => (en("Middle"), "middle"),
                    VAlign::Bottom => (en("Bottom"), "bottom"),
                };
                choice(format!("scripture-valign-{key}").into(), l.t(label), layout.valign == a)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_scripture(cx, |l| l.valign = a)))
            }));
            let cases = h_flex().gap_1().children(TextCase::ALL.into_iter().map(|c| {
                let label = match c {
                    TextCase::Normal => en("As typed"),
                    TextCase::Upper => en("ALL CAPS"),
                };
                choice(format!("scripture-case-{}", c.css()).into(), l.t(label), layout.text_case == c)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_scripture(cx, |l| l.text_case = c)))
            }));
            v_flex()
                .gap_2()
                .child(h_flex().gap_2().child(small_label(en("Across"), 70.)).child(aligns))
                .child(h_flex().gap_2().child(small_label(en("Down"), 70.)).child(valigns))
                .child(h_flex().gap_2().child(small_label(en("Letters"), 70.)).child(cases))
                .child(h_flex().gap_3().child(field(Field::ScriptureSizeMin, 190.)).child(field(Field::ScriptureSizeMax, 190.)))
        });

        let bg_row = image_row(MediaSlot::Background, &draft.background_image, cx);
        let video_row = image_row(MediaSlot::Video, &draft.background_video, cx);
        let logo_row = image_row(MediaSlot::Logo, &draft.logo, cx);

        let panels = h_flex().gap_1().children(PanelKind::ALL.into_iter().map(|k| {
            let label = match k {
                PanelKind::None => en("None"),
                PanelKind::Box => en("Box"),
                PanelKind::Bar => en("Bar across"),
            };
            choice(format!("panel-{}", k.as_str()).into(), l.t(label), draft.panel == k)
                .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.panel = k)))
        }));
        let panel_areas = h_flex().gap_1().children([(false, en("Text only")), (true, en("Text and small lines"))].map(|(on, label)| {
            choice(if on { "panel-areas-on" } else { "panel-areas-off" }.into(), l.t(label), draft.panel_behind_areas == on)
                .on_click(cx.listener(move |this, _, _, cx| this.set_choice(cx, |t| t.panel_behind_areas = on)))
        }));
        let panel_settings = (draft.panel != PanelKind::None).then(|| {
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .child(field(Field::PanelColor, 120.))
                        .child(swatch(&draft.panel_color))
                        .child(field(Field::PanelPadding, 110.))
                        .when(draft.panel == PanelKind::Box, |d| d.child(field(Field::PanelRadius, 110.))),
                )
                .child(h_flex().gap_2().child(small_label(en("Behind"), 70.)).child(panel_areas))
        });

        let form = v_flex()
            .id("theme-form")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .gap_2()
            .p_4()
            .child(h_flex().gap_3().child(field(Field::Name, 260.)))
            .child(heading(en("Text")))
            .child(h_flex().gap_3().items_end().child(field(Field::Font, 200.)).child(fonts))
            .child(h_flex().gap_2().child(div().text_xs().text_color(muted).child(l.t("Weight"))).child(weights))
            .child(
                h_flex()
                    .gap_3()
                    .child(field(Field::SizeMin, 150.))
                    .child(field(Field::SizeMax, 150.))
                    .child(field(Field::LineHeight, 110.)),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(field(Field::LetterSpacing, 150.))
                    .child(h_flex().gap_2().pb_1().child(div().text_xs().text_color(muted).child(l.t("Letters"))).child(cases)),
            )
            .child(h_flex().gap_2().child(field(Field::Color, 110.)).child(swatch(&draft.color)))
            .child(
                h_flex()
                    .gap_2()
                    .child(field(Field::StrongColor, 160.))
                    .child(swatch(&or_text(&draft.strong_color)))
                    .child(field(Field::EmColor, 160.))
                    .child(swatch(&or_text(&draft.em_color))),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(field(Field::OutlineColor, 110.))
                    .child(swatch(&draft.outline_color))
                    .child(field(Field::Outline, 100.)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(field(Field::ShadowColor, 110.))
                    .child(swatch(&draft.shadow_color))
                    .child(field(Field::ShadowBlur, 100.))
                    .child(field(Field::ShadowOffset, 100.)),
            )
            .child(div().text_xs().text_color(muted).child(l.t("Sizes and widths are in % of the screen height; colors are #rrggbb or #rrggbbaa. Leave a strong or emphasis color empty to use the text color.")))
            .child(heading(en("Position")))
            .child(h_flex().gap_2().child(div().w(px(70.)).text_xs().text_color(muted).child(l.t("Across"))).child(aligns))
            .child(h_flex().gap_2().child(div().w(px(70.)).text_xs().text_color(muted).child(l.t("Down"))).child(valigns))
            .child(h_flex().gap_3().child(field(Field::MarginX, 150.)).child(field(Field::MarginY, 190.)))
            .child(heading(en("Panel")))
            .child(h_flex().gap_2().child(small_label(en("Shape"), 70.)).child(panels))
            .children(panel_settings)
            .child(heading(en("Scripture")))
            .child(h_flex().gap_2().child(small_label(en("Placement and size"), 120.)).child(scripture_modes))
            .children(scripture_layout)
            .child(heading(en("Title, reference and credits")))
            .child(div().text_xs().text_color(muted).child(l.t("Small lines in the top or bottom margin. Ones in the same place and alignment share a line.")))
            .children(areas)
            .child(heading(en("Background")))
            .child(
                h_flex()
                    .gap_2()
                    .child(field(Field::Background, 110.))
                    .child(swatch(&draft.background_color))
                    .child(field(Field::Gradient, 170.))
                    .child(swatch(&or_bg(&draft.gradient_color)))
                    .child(field(Field::Dim, 90.)),
            )
            .child(bg_row)
            .child(video_row)
            .child(logo_row)
            .child(heading(en("Transition")))
            .child(h_flex().gap_3().items_end().child(transitions).child(field(Field::TransitionMs, 110.)));

        // The webviews cover whatever GPUI draws over them: step aside for
        // notifications and dialogs (the approximation shows meanwhile).
        let covered = !window.notifications(cx).is_empty() || window.has_active_dialog(cx);
        if let Some([slide, logo]) = &self.pages {
            slide.send(preview_script(&draft, Some(sample.clone()), Blank::None));
            logo.send(preview_script(&draft, None, Blank::Logo));
            slide.set_visible(!covered);
            logo.set_visible(!covered);
        }
        let pages = self.pages.clone();
        let page = |i: usize| {
            pages.as_ref().map(|p| {
                let page = p[i].clone();
                canvas(move |bounds, _, _| page.set_bounds(bounds), |_, _, _, _| {}).absolute().inset_0()
            })
        };
        let caption = if self.pages.is_some() {
            l.t("The display page itself, as the projector shows it.")
        } else {
            l.t("An approximation: the projector shows the real thing.")
        };

        let is_default = self.stored().name == self.default_name;
        let preview = v_flex()
            .w(px(500.))
            .flex_none()
            .h_full()
            .gap_2()
            .p_4()
            .border_l_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().text_xs().font_weight(FontWeight::BOLD).text_color(muted).child(l.t("PREVIEW")))
                    .children([(SlideKind::Lyrics, en("Song")), (SlideKind::Scripture, en("Scripture"))].map(|(kind, label)| {
                        let id = if kind == SlideKind::Lyrics { "preview-song" } else { "preview-scripture" };
                        choice(id.into(), l.t(label), self.preview_kind == kind).on_click(cx.listener(move |this, _, _, cx| {
                            this.preview_kind = kind;
                            cx.notify()
                        }))
                    })),
            )
            .child(div().relative().w(px(468.)).child(screen(&draft, Some(&sample), background.clone(), px(468.))).children(page(0)))
            .child(div().text_xs().text_color(muted).child(l.t("Blanked to the logo:")))
            .child(
                div()
                    .relative()
                    .w(px(234.))
                    .child(screen(&draft, None, background, px(234.)))
                    .children(logo.map(|src| {
                        gpui::img(src)
                            .absolute()
                            .top(px(234. * 9. / 16. * 0.3))
                            .left_0()
                            .w(px(234.))
                            .h(px(234. * 9. / 16. * 0.4))
                            .object_fit(gpui::ObjectFit::Contain)
                    }))
                    .children(page(1)),
            )
            .child(div().text_xs().text_color(muted).child(caption));

        v_flex()
            .key_context("ThemeEditor")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SaveTheme, window, cx| this.save(window, cx)))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .child(h_flex().flex_1().min_h_0().child(list).child(form.test_support()).child(preview))
            .child(
                h_flex()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(Button::new("theme-new").icon(IconName::Plus).small().label(l.t("New")).on_click(cx.listener(|this, _, w, cx| this.add(false, w, cx))))
                    .child(Button::new("theme-duplicate")
                            .icon(IconName::Copy)
                            .small()
                            .label(l.t("Duplicate"))
                            .on_click(cx.listener(|this, _, w, cx| this.add(true, w, cx))))
                    .child(Button::new("theme-delete")
                            .icon(IconName::Trash)
                            .small()
                            .ghost()
                            .label(l.t("Delete…"))
                            .disabled(is_default)
                            .on_click(cx.listener(|this, _, w, cx| this.delete(w, cx))))
                    .child(Button::new("theme-import")
                            .icon(IconName::Download)
                            .small()
                            .ghost()
                            .label(l.t("Import…"))
                            .on_click(cx.listener(|this, _, w, cx| this.import(w, cx))))
                    .child(Button::new("theme-export")
                            .icon(IconName::Upload)
                            .small()
                            .ghost()
                            .label(l.t("Export…"))
                            .disabled(dirty)
                            .on_click(cx.listener(|this, _, w, cx| this.export(w, cx))))
                    .child(Button::new("theme-default")
                            .icon(IconName::Star)
                            .small()
                            .ghost()
                            .label(if is_default { l.t("The default theme") } else { l.t("Make default") })
                            .disabled(is_default || dirty)
                            .on_click(cx.listener(|this, _, w, cx| this.make_default(w, cx))))
                    .child(div().flex_1().text_xs().text_color(muted).when(dirty, |d| d.child(l.t("Unsaved changes"))))
                    .child(Button::new("theme-save")
                            .icon(IconName::Save)
                            .small()
                            .primary()
                            .label(l.t("Save  ⌘S"))
                            .disabled(!dirty)
                            .on_click(cx.listener(|this, _, w, cx| this.save(w, cx)))),
            )
    }
}


/// A file name for a theme: its name without characters macOS or other
/// systems refuse.
fn file_stem(name: &str) -> String {
    let stem: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect();
    let stem = stem.trim().trim_start_matches('.').to_string();
    if stem.is_empty() { "theme".into() } else { stem }
}

/// Why a theme file couldn't be written or read, in the console's language.
fn theme_file_error(l: Lang, e: &ThemeFileError) -> String {
    match e {
        ThemeFileError::Io(error) => l.f("The file couldn't be used: {error}", &[("error", error)]),
        ThemeFileError::NotATheme => l.t("This isn't an Asaf theme file.").to_string(),
        ThemeFileError::Newer(_) => l.t("This theme file is from a newer version of Asaf.").to_string(),
        ThemeFileError::MissingMedia(name) => l.f("The theme file is missing {file}.", &[("file", name)]),
        ThemeFileError::TooLarge(name) => l.f("{file} in the theme file is too large.", &[("file", name)]),
        ThemeFileError::Storage(error) => l.f("The library couldn't be updated: {error}", &[("error", error)]),
    }
}

/// Why a theme couldn't be saved or deleted, in the console's language.
fn theme_error(l: Lang, e: &ThemeError) -> String {
    match e {
        ThemeError::NameTaken(name) => l.f("There is already a theme called “{name}”.", &[("name", name)]),
        ThemeError::IsDefault(name) => {
            l.f("“{name}” is the default theme; choose another default first.", &[("name", name)])
        }
        ThemeError::Storage(error) => l.f("The library couldn't be updated: {error}", &[("error", error)]),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use asaf_library::Library;
    use gpui::TestAppContext;
    use gpui::test::TestWindowExt as _;

    use super::*;

    type Events = Rc<RefCell<Vec<ThemesChanged>>>;

    fn open(cx: &mut TestAppContext, lib: Library) -> (SharedLibrary, Entity<ThemeEditor>, AnyWindowHandle, Events) {
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let library: SharedLibrary = Arc::new(Mutex::new(lib));
        let lib = library.clone();
        let (h, editor) = cx.update(|cx| ThemeEditor::open(lib, None, None, cx).unwrap());
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|cx| cx.subscribe(&editor, move |_, e: &ThemesChanged, _| sink.borrow_mut().push(e.clone())).detach());
        (library, editor, h, events)
    }

    fn with(cx: &mut TestAppContext, h: AnyWindowHandle, f: impl FnOnce(&mut Window, &mut App)) {
        cx.update(|cx| h.update(cx, |_, w, cx| f(w, cx)).unwrap());
    }

    /// Scrolls the form to its end, where the background and transition are.
    fn scroll_form_down(w: &mut Window, cx: &mut App) {
        scroll_form(w, cx, 4000.);
    }

    /// Scrolls the form down by `by` pixels.
    fn scroll_form(w: &mut Window, cx: &mut App, by: f32) {
        w.render_frame(cx);
        w.scroll("theme-form", gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-by))), cx);
        w.render_frame(cx);
    }

    fn set_field(cx: &mut TestAppContext, h: AnyWindowHandle, editor: &Entity<ThemeEditor>, field: Field, text: &str) {
        let input = cx.update(|cx| editor.read(cx).input(field).clone());
        with(cx, h, |w, cx| {
            input.update(cx, |s, cx| s.focus(w, cx));
            w.press("cmd-a", cx);
            w.press("backspace", cx);
            w.input(text, cx);
        });
    }

    #[test]
    fn fields_round_trip_and_reject_bad_text() {
        let t = Theme {
            size_min_vh: 3.5,
            outline_vh: 0.25,
            transition_ms: 450,
            letter_spacing_em: 0.05,
            gradient_color: "#5b21b6".into(),
            background_dim: 0.35,
            scripture: Some(Layout { size_min_vh: 3.5, size_max_vh: 7.0, ..Layout::default() }),
            ..Theme::default()
        };
        let mut t = t;
        (t.title_area.size_vh, t.credits_area.color) = (3.5, "#aabbcc".into());
        for f in Field::ALL {
            let mut back =
                Theme { name: "x".into(), font_family: "y".into(), scripture: Some(Layout::default()), ..Theme::default() };
            f.set(&mut back, &f.get(&t)).unwrap();
            assert_eq!(f.get(&back), f.get(&t), "{f:?}");
        }
        let mut t = Theme::default();
        for (f, text, ok) in [
            (Field::SizeMin, "4,5", true),
            (Field::SizeMin, "0", false),
            (Field::SizeMax, "abc", false),
            (Field::Color, "#ff0", true),
            (Field::Color, "red", false),
            (Field::StrongColor, "", true),
            (Field::EmColor, "blue", false),
            (Field::EmColor, "#00f", true),
            (Field::Name, "  ", false),
            (Field::Font, "", false),
            (Field::TransitionMs, "-1", false),
            (Field::TransitionMs, "99999", false),
            (Field::LineHeight, "1.4", true),
            (Field::LetterSpacing, "0.9", false),
            (Field::Dim, "95", false),
            (Field::Dim, "40", true),
            (Field::Gradient, "", true),
            (Field::TitleSize, "12", false),
            (Field::ReferenceColor, "", true),
            (Field::CreditsColor, "grey", false),
            // Ignored while scripture is laid out like lyrics.
            (Field::ScriptureSizeMin, "nonsense", true),
        ] {
            assert_eq!(f.set(&mut t, text).is_ok(), ok, "{f:?} {text:?}");
        }
        assert_eq!((t.size_min_vh, t.color.as_str(), t.line_height), (4.5, "#ff0", 1.4));
        assert_eq!((t.strong_color.as_str(), t.em_color.as_str()), ("", "#00f"));
    }

    #[gpui::test]
    fn editing_and_saving_a_theme(cx: &mut TestAppContext) {
        let (library, editor, h, events) = open(cx, crate::test_library());
        with(cx, h, |w, cx| w.render_frame(cx));
        assert!(!cx.update(|cx| editor.read(cx).is_dirty(cx)));

        // Text fields, buttons and the font picker.
        set_field(cx, h, &editor, Field::Name, "Sunday");
        set_field(cx, h, &editor, Field::Color, "#ffee00");
        set_field(cx, h, &editor, Field::SizeMax, "9");
        with(cx, h, |w, cx| {
            w.render_frame(cx);
            w.click("font-3", cx);
            w.click("weight-400", cx);
            scroll_form(w, cx, 300.);
            w.click("align-left", cx);
            w.click("valign-Bottom", cx);
            scroll_form_down(w, cx);
            w.click("transition-slide", cx);
            w.click("bg-clear", cx);
        });
        let (draft, bad) = cx.update(|cx| editor.read(cx).draft(cx));
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(
            (draft.align, draft.valign, draft.transition, draft.font_family.as_str(), draft.font_weight),
            (Align::Left, VAlign::Bottom, TransitionKind::Slide, "Lora", 400)
        );
        assert_eq!(draft.background_image, None);

        // A bad value blocks saving and is flagged.
        set_field(cx, h, &editor, Field::Outline, "lots");
        with(cx, h, |w, cx| w.press("cmd-s", cx));
        assert_eq!(cx.update(|cx| editor.read(cx).draft(cx).1), [Field::Outline]);
        assert_eq!(library.lock().unwrap().themes().unwrap()[0].name, "Default", "nothing saved");

        set_field(cx, h, &editor, Field::Outline, "0.3");
        with(cx, h, |w, cx| w.press("cmd-s", cx));
        let stored = library.lock().unwrap().themes().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0], Theme { outline_vh: 0.3, ..draft });
        assert_eq!(library.lock().unwrap().default_theme().unwrap().name, "Sunday", "the renamed default stays default");
        assert_eq!(
            events.borrow().as_slice(),
            [ThemesChanged { renamed: Some(("Default".into(), "Sunday".into())), deleted: None }]
        );
        assert!(!cx.update(|cx| editor.read(cx).is_dirty(cx)));
    }

    #[gpui::test]
    fn text_areas_and_the_scripture_layout(cx: &mut TestAppContext) {
        let (library, editor, h, _) = open(cx, crate::test_library());
        with(cx, h, |w, cx| {
            scroll_form(w, cx, 500.);
            w.click("scripture-own", cx);
            w.render_frame(cx);
            w.click("scripture-align-left", cx);
            w.click("scripture-valign-top", cx);
        });
        assert_eq!(cx.update(|cx| editor.read(cx).preview_kind), SlideKind::Scripture, "the preview follows");
        set_field(cx, h, &editor, Field::ScriptureSizeMax, "7");
        with(cx, h, |w, cx| {
            scroll_form(w, cx, 300.);
            w.click("area-title-top", cx);
            w.render_frame(cx);
            w.click("area-title-align-right", cx);
            w.click("area-title-case-uppercase", cx);
            w.click("area-credits-hidden", cx);
        });
        set_field(cx, h, &editor, Field::TitleColor, "#ffcc00");
        let (draft, bad) = cx.update(|cx| editor.read(cx).draft(cx));
        assert!(bad.is_empty(), "{bad:?}");
        let lyrics = Theme::default().layout(SlideKind::Lyrics);
        assert_eq!(
            draft.scripture,
            Some(Layout { align: Align::Left, valign: VAlign::Top, size_max_vh: 7.0, ..lyrics.clone() })
        );
        assert_eq!(
            draft.title_area,
            asaf_core::TextArea {
                place: Place::Top,
                align: Some(Align::Right),
                color: "#ffcc00".into(),
                text_case: TextCase::Upper,
                ..Default::default()
            }
        );
        assert_eq!((draft.credits_area.place, draft.reference_area.place), (Place::Hidden, Place::Bottom));

        // Scripture sizes below their minimum are flagged.
        set_field(cx, h, &editor, Field::ScriptureSizeMax, "2");
        assert_eq!(cx.update(|cx| editor.read(cx).draft(cx).1), [Field::ScriptureSizeMax]);
        set_field(cx, h, &editor, Field::ScriptureSizeMax, "7");
        with(cx, h, |w, cx| w.press("cmd-s", cx));
        assert_eq!(library.lock().unwrap().default_theme().unwrap(), draft);

        // Back to the lyrics' layout.
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.set_own_scripture_layout(false, w, cx)));
        assert_eq!(cx.update(|cx| editor.read(cx).draft(cx).0.scripture.clone()), None);
    }

    #[gpui::test]
    fn panels_and_background_videos(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("asaf.sqlite")).unwrap();
        let (library, editor, h, _) = open(cx, lib);
        with(cx, h, |w, cx| {
            scroll_form(w, cx, 500.);
            w.click("panel-box", cx);
            w.render_frame(cx);
            w.click("panel-areas-on", cx);
        });
        set_field(cx, h, &editor, Field::PanelRadius, "3");
        set_field(cx, h, &editor, Field::PanelColor, "#1e3a8acc");
        let (draft, bad) = cx.update(|cx| editor.read(cx).draft(cx));
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(
            (draft.panel, draft.panel_behind_areas, draft.panel_radius_vh, draft.panel_color.as_str()),
            (PanelKind::Box, true, 3.0, "#1e3a8acc")
        );
        set_field(cx, h, &editor, Field::PanelPadding, "40");
        assert_eq!(cx.update(|cx| editor.read(cx).draft(cx).1), [Field::PanelPadding]);
        set_field(cx, h, &editor, Field::PanelPadding, "2");

        // A video goes in the video slot; an image doesn't.
        let still = dir.path().join("still.png");
        let clip = dir.path().join("Olas del mar.mp4");
        std::fs::write(&still, b"png").unwrap();
        std::fs::write(&clip, b"mp4 bytes").unwrap();
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.set_media_file(MediaSlot::Video, &still, w, cx)));
        assert_eq!(cx.update(|cx| editor.read(cx).draft(cx).0.background_video), None);
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.set_media_file(MediaSlot::Video, &clip, w, cx)));
        let video = cx.update(|cx| editor.read(cx).draft(cx).0.background_video).unwrap();
        assert!(video.starts_with("/media/olas-del-mar-") && video.ends_with(".mp4"), "{video}");
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.set_media_file(MediaSlot::Background, &clip, w, cx)));
        assert_eq!(cx.update(|cx| editor.read(cx).draft(cx).0.background_image).as_deref(), Some("/assets/background.svg"));

        with(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("cmd-s", cx);
        });
        let saved = library.lock().unwrap().default_theme().unwrap();
        assert_eq!((saved.background_video, saved.panel), (Some(video), PanelKind::Box));
        with(cx, h, |w, cx| {
            scroll_form_down(w, cx);
            w.click("video-clear", cx);
        });
        assert_eq!(cx.update(|cx| editor.read(cx).draft(cx).0.background_video), None);
    }

    #[gpui::test]
    fn exporting_and_importing_theme_files(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(&dir.path().join("asaf.sqlite")).unwrap();
        lib.save_theme(&Theme { name: "Night: Blue".into(), color: "#123456".into(), ..Theme::default() }, None).unwrap();
        let (library, editor, h, events) = open(cx, lib);
        let night = cx.update(|cx| editor.read(cx).themes.iter().position(|t| t.name == "Night: Blue").unwrap());
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.select(night, w, cx)));

        // The button asks where; the answer goes to export_to.
        with(cx, h, |w, cx| {
            w.render_frame(cx);
            w.click("theme-export", cx);
        });
        cx.run_until_parked();
        assert!(cx.did_prompt_for_new_path());
        let file = dir.path().join("night.asaf-theme");
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.export_to(&file, w, cx)));
        assert!(file.exists());
        assert_eq!(file_stem("Night: Blue"), "Night- Blue");
        assert_eq!((file_stem("../x"), file_stem("  ")), ("-x".to_string(), "theme".to_string()));

        events.borrow_mut().clear();
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.import_from(&file, w, cx)));
        let names: Vec<String> = library.lock().unwrap().themes().unwrap().into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"Night: Blue 2".to_string()), "{names:?}");
        let selected = cx.update(|cx| editor.read(cx).stored().name);
        assert_eq!(selected, "Night: Blue 2", "the imported theme is shown");
        assert_eq!(events.borrow().len(), 1);

        // Not a theme file: nothing changes.
        let junk = dir.path().join("junk.asaf-theme");
        std::fs::write(&junk, "nope").unwrap();
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.import_from(&junk, w, cx)));
        assert_eq!(library.lock().unwrap().themes().unwrap().len(), names.len());
        assert_eq!(theme_file_error(crate::i18n::Lang(asaf_core::Language::English), &ThemeFileError::NotATheme), "This isn't an Asaf theme file.");
    }

    #[gpui::test]
    fn new_duplicate_default_and_delete(cx: &mut TestAppContext) {
        let (library, editor, h, events) = open(cx, crate::test_library());
        with(cx, h, |w, cx| {
            w.render_frame(cx);
            w.click("theme-duplicate", cx);
        });
        let names = |cx: &mut TestAppContext| cx.update(|cx| editor.read(cx).themes.iter().map(|t| t.name.clone()).collect::<Vec<_>>());
        assert_eq!(names(cx), ["Default", "Default copy"]);
        assert_eq!(cx.update(|cx| editor.read(cx).selected), 1, "the copy is selected");
        with(cx, h, |w, cx| {
            w.render_frame(cx);
            w.click("theme-new", cx);
        });
        assert_eq!(names(cx), ["Default", "Default copy", "New theme"]);

        // Make the new one the default; then the old default can be deleted.
        with(cx, h, |w, cx| {
            w.render_frame(cx);
            w.click("theme-default", cx);
        });
        assert_eq!(library.lock().unwrap().settings().unwrap().default_theme, "New theme");
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.select(0, w, cx)));
        with(cx, h, |w, cx| {
            w.render_frame(cx);
            w.click("theme-delete", cx);
        });
        cx.run_until_parked();
        cx.simulate_prompt_answer("Delete");
        cx.run_until_parked();
        assert_eq!(names(cx), ["Default copy", "New theme"]);
        assert_eq!(events.borrow().last().unwrap().deleted.as_deref(), Some("Default"));
    }

    #[gpui::test]
    fn switching_themes_with_unsaved_changes_asks(cx: &mut TestAppContext) {
        let mut lib = crate::test_library();
        lib.save_theme(&Theme { name: "Night".into(), ..Theme::default() }, None).unwrap();
        let (_, editor, h, _) = open(cx, lib);
        with(cx, h, |w, cx| w.render_frame(cx));
        set_field(cx, h, &editor, Field::Color, "#123456");
        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.select(1, w, cx)));
        cx.run_until_parked();
        cx.simulate_prompt_answer("Keep Editing");
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| editor.read(cx).selected), 0);
        assert!(cx.update(|cx| editor.read(cx).is_dirty(cx)));

        with(cx, h, |w, cx| editor.update(cx, |e, cx| e.select(1, w, cx)));
        cx.run_until_parked();
        cx.simulate_prompt_answer("Discard");
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| editor.read(cx).selected), 1);
        assert_eq!(cx.update(|cx| editor.read(cx).input(Field::Name).read(cx).value()), "Night");
        assert!(!cx.update(|cx| editor.read(cx).is_dirty(cx)));
    }

    #[gpui::test]
    fn background_images_are_imported_into_the_media_folder(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("asaf.sqlite")).unwrap();
        let (library, editor, h, _) = open(cx, lib);
        let src = dir.path().join("Amanecer.svg");
        std::fs::write(&src, include_bytes!("../../../display/background.svg")).unwrap();
        with(cx, h, |w, cx| {
            scroll_form_down(w, cx);
            w.click("bg-choose", cx);
        });
        cx.run_until_parked();
        cx.simulate_path_prompt_response(|_| Some(vec![src.clone()]));
        cx.run_until_parked();
        let (draft, _) = cx.update(|cx| editor.read(cx).draft(cx));
        let url = draft.background_image.clone().unwrap();
        assert!(url.starts_with("/media/amanecer-") && url.ends_with(".svg"), "{url}");
        assert!(library.lock().unwrap().media_path(&url).unwrap().exists());
        with(cx, h, |w, cx| {
            w.render_frame(cx);
            w.press("cmd-s", cx);
        });
        assert_eq!(library.lock().unwrap().default_theme().unwrap().background_image, Some(url));
    }
}
