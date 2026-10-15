//! Theme editor window: the library's themes on the left, the chosen one's
//! settings in the middle, an approximate preview on the right. Saving writes
//! to the library and emits [`ThemesChanged`], so the console can update the
//! projector at once if the theme is on screen.

use anyhow::Result;
use asaf_core::{Align, DEFAULT_THEME_NAME, Theme, TransitionKind, VAlign, parse_color, sample_song};
use asaf_fonts::FONTS;
use gpui::component::button::{Button, ButtonVariants as _};
use gpui::component::input::{Input, InputEvent, InputState};
use gpui::component::notification::Notification;
use gpui::component::{ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, PathPromptOptions, PromptLevel, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledImage as _, Subscription, TestSupportExt as _, TitlebarOptions, Window, WindowBounds, WindowOptions, div, px,
    size,
};

use crate::i18n::{Lang, en, lang};
use crate::preview::{color, image_source, screen};
use crate::{SaveTheme, SharedLibrary};

/// Emitted after themes were saved, renamed, deleted or a new default chosen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemesChanged {
    /// `(old, new)` when a theme was renamed.
    pub renamed: Option<(String, String)>,
    pub deleted: Option<String>,
}

/// The text fields of the form, each tied to one theme setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Name,
    Font,
    SizeMin,
    SizeMax,
    LineHeight,
    Color,
    OutlineColor,
    Outline,
    ShadowColor,
    ShadowBlur,
    ShadowOffset,
    MarginX,
    MarginY,
    Background,
    TransitionMs,
}

impl Field {
    pub const ALL: [Field; 15] = [
        Field::Name,
        Field::Font,
        Field::SizeMin,
        Field::SizeMax,
        Field::LineHeight,
        Field::Color,
        Field::OutlineColor,
        Field::Outline,
        Field::ShadowColor,
        Field::ShadowBlur,
        Field::ShadowOffset,
        Field::MarginX,
        Field::MarginY,
        Field::Background,
        Field::TransitionMs,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap()
    }

    /// The field's text for `t`.
    pub fn get(self, t: &Theme) -> String {
        match self {
            Field::Name => t.name.clone(),
            Field::Font => t.font_family.clone(),
            Field::SizeMin => t.size_min_vh.to_string(),
            Field::SizeMax => t.size_max_vh.to_string(),
            Field::LineHeight => t.line_height.to_string(),
            Field::Color => t.color.clone(),
            Field::OutlineColor => t.outline_color.clone(),
            Field::Outline => t.outline_vh.to_string(),
            Field::ShadowColor => t.shadow_color.clone(),
            Field::ShadowBlur => t.shadow_blur_vh.to_string(),
            Field::ShadowOffset => t.shadow_offset_vh.to_string(),
            Field::MarginX => t.margin_x_pct.to_string(),
            Field::MarginY => t.margin_y_pct.to_string(),
            Field::Background => t.background_color.clone(),
            Field::TransitionMs => t.transition_ms.to_string(),
        }
    }

    /// Puts `text` into `t`, or says what's wrong with it.
    pub fn set(self, t: &mut Theme, text: &str) -> Result<(), &'static str> {
        let text = text.trim();
        let number = |lo: f32, hi: f32| -> Result<f32, &'static str> {
            text.replace(',', ".").parse::<f32>().ok().filter(|v| v.is_finite() && (lo..=hi).contains(v)).ok_or("number")
        };
        let colour = || -> Result<String, &'static str> { parse_color(text).map(|_| text.to_string()).ok_or("color") };
        match self {
            Field::Name if text.is_empty() => return Err("name"),
            Field::Name => t.name = text.to_string(),
            Field::Font if text.is_empty() => return Err("font"),
            Field::Font => t.font_family = text.to_string(),
            Field::SizeMin => t.size_min_vh = number(1.0, 40.0)?,
            Field::SizeMax => t.size_max_vh = number(1.0, 40.0)?,
            Field::LineHeight => t.line_height = number(0.8, 3.0)?,
            Field::Color => t.color = colour()?,
            Field::OutlineColor => t.outline_color = colour()?,
            Field::Outline => t.outline_vh = number(0.0, 3.0)?,
            Field::ShadowColor => t.shadow_color = colour()?,
            Field::ShadowBlur => t.shadow_blur_vh = number(0.0, 10.0)?,
            Field::ShadowOffset => t.shadow_offset_vh = number(0.0, 10.0)?,
            Field::MarginX => t.margin_x_pct = number(0.0, 40.0)?,
            Field::MarginY => t.margin_y_pct = number(0.0, 40.0)?,
            Field::Background => t.background_color = colour()?,
            Field::TransitionMs => {
                t.transition_ms = text.parse::<u32>().ok().filter(|v| *v <= asaf_core::theme::MAX_TRANSITION_MS).ok_or("number")?
            }
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
            Field::Color => en("Text color"),
            Field::OutlineColor => en("Outline color"),
            Field::Outline => en("Outline width"),
            Field::ShadowColor => en("Shadow color"),
            Field::ShadowBlur => en("Shadow blur"),
            Field::ShadowOffset => en("Shadow offset"),
            Field::MarginX => en("Side margins (%)"),
            Field::MarginY => en("Top and bottom margins (%)"),
            Field::Background => en("Background color"),
            Field::TransitionMs => en("Duration (ms)"),
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
    discard_confirmed: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ThemesChanged> for ThemeEditor {}

const SAMPLE_SLIDE: usize = 1; // the chorus of the sample hymn: four lines

impl ThemeEditor {
    /// Opens the editor on the theme called `select` (else the default).
    pub fn open(library: SharedLibrary, select: Option<String>, cx: &mut App) -> Result<(AnyWindowHandle, Entity<Self>)> {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1240.), px(780.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some(lang(cx).t("Themes").into()), ..Default::default() }),
            window_min_size: Some(size(px(1000.), px(600.))),
            ..Default::default()
        };
        let (handle, editor) = gpui::open_window(options, cx, |window, cx| {
            cx.new(|cx| ThemeEditor::new(library, select, window, cx))
        })?;
        handle.update(cx, |_, window, cx| {
            window.activate_window();
            editor.read(cx).focus.clone().focus(window, cx);
        })?;
        Ok((handle, editor))
    }

    pub(crate) fn new(library: SharedLibrary, select: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        subscriptions.push(cx.observe_global::<crate::i18n::UiLanguage>(|_, cx| cx.notify()));
        let mut editor = Self {
            library,
            themes: Vec::new(),
            default_name: DEFAULT_THEME_NAME.into(),
            selected: 0,
            choice: Theme::default(),
            inputs,
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
            let note = format!("{} {}", l.t("Fix these first:"), names.join(", "));
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
                window.push_notification(Notification::success(format!("{} “{}”", l.t("Saved"), saved.name)), cx);
            }
            Err(e) => window.push_notification(Notification::error(e), cx),
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
            let stem = if copy { format!("{} {}", base.name, l.t("copy")) } else { l.t("New theme").to_string() };
            lib.unused_theme_name(&stem)
                .map_err(|e| e.to_string())
                .and_then(|name| lib.save_theme(&Theme { name, ..base }, None))
        };
        match result {
            Ok(theme) => {
                self.reload(Some(&theme.name), window, cx);
                cx.emit(ThemesChanged::default());
                self.input(Field::Name).update(cx, |s, cx| s.focus(window, cx));
            }
            Err(e) => window.push_notification(Notification::error(e), cx),
        }
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let l = lang(cx);
        let name = self.stored().name;
        let message = format!("{} “{name}”?", l.t("Delete the theme"));
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
                    Err(e) => window.push_notification(Notification::error(e), cx),
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
            Err(e) => window.push_notification(Notification::error(e.to_string()), cx),
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
        let message = format!("{} “{}”?", l.t("Discard your changes to"), self.stored().name);
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

    /// Imports an image into the media folder for the background or the logo.
    fn choose_image(&mut self, logo: bool, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(lang(cx).t("Choose").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = paths.await else { return };
            let Some(path) = paths.pop() else { return };
            let _ = this.update_in(cx, |this, window, cx| this.set_image_file(logo, &path, window, cx));
        })
        .detach();
    }

    pub(crate) fn set_image_file(&mut self, logo: bool, path: &std::path::Path, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.library.lock().unwrap().import_media(path);
        match result {
            Ok(url) => {
                if logo {
                    self.choice.logo = Some(url);
                } else {
                    self.choice.background_image = Some(url);
                }
                cx.notify();
            }
            Err(e) => window.push_notification(Notification::error(e), cx),
        }
    }

    fn set_choice(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Theme)) {
        f(&mut self.choice);
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
        Some("/assets/background.svg") | Some("/assets/logo.svg") => l.t("Built in").to_string(),
        Some(u) => u.rsplit('/').next().unwrap_or(u).to_string(),
    }
}

impl Render for ThemeEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let l = lang(cx);
        let (draft, bad) = self.draft(cx);
        let dirty = draft != self.stored();
        let sample = sample_song().slides().swap_remove(SAMPLE_SLIDE).lines;
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
                    .child(div().size(px(14.)).flex_none().rounded_sm().bg(color(&t.background_color)))
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
        let image_row = |logo: bool, url: &Option<String>, cx: &mut Context<Self>| {
            let builtin = if logo { "/assets/logo.svg" } else { "/assets/background.svg" };
            h_flex()
                .gap_1()
                .child(
                    div()
                        .w(px(120.))
                        .text_xs()
                        .text_color(muted)
                        .child(l.t(if logo { en("Logo (blank to logo)") } else { en("Background image") })),
                )
                .child(div().flex_1().min_w_0().truncate().text_xs().child(image_name(url, l)))
                .child(Button::new(if logo { "logo-choose" } else { "bg-choose" })
                        .xsmall()
                        .label(l.t("Choose…"))
                        .on_click(cx.listener(move |this, _, window, cx| this.choose_image(logo, window, cx))))
                .child(Button::new(if logo { "logo-builtin" } else { "bg-builtin" }).xsmall().ghost().label(l.t("Built in")).on_click(cx.listener(move |this, _, _, cx| {
                        this.set_choice(cx, |t| {
                            let url = Some(builtin.to_string());
                            if logo { t.logo = url } else { t.background_image = url }
                        })
                    })))
                .child(Button::new(if logo { "logo-clear" } else { "bg-clear" }).xsmall().ghost().label(l.t("None")).on_click(cx.listener(move |this, _, _, cx| {
                        this.set_choice(cx, |t| if logo { t.logo = None } else { t.background_image = None })
                    })))
        };
        let bg_row = image_row(false, &draft.background_image, cx);
        let logo_row = image_row(true, &draft.logo, cx);

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
            .child(h_flex().gap_2().child(field(Field::Color, 110.)).child(swatch(&draft.color)))
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
            .child(div().text_xs().text_color(muted).child(l.t("Sizes and widths are in % of the screen height; colors are #rrggbb or #rrggbbaa.")))
            .child(heading(en("Position")))
            .child(h_flex().gap_2().child(div().w(px(70.)).text_xs().text_color(muted).child(l.t("Across"))).child(aligns))
            .child(h_flex().gap_2().child(div().w(px(70.)).text_xs().text_color(muted).child(l.t("Down"))).child(valigns))
            .child(h_flex().gap_3().child(field(Field::MarginX, 150.)).child(field(Field::MarginY, 190.)))
            .child(heading(en("Background")))
            .child(h_flex().gap_2().child(field(Field::Background, 110.)).child(swatch(&draft.background_color)))
            .child(bg_row)
            .child(logo_row)
            .child(heading(en("Transition")))
            .child(h_flex().gap_3().items_end().child(transitions).child(field(Field::TransitionMs, 110.)));

        let is_default = self.stored().name == self.default_name;
        let preview = v_flex()
            .w(px(500.))
            .flex_none()
            .h_full()
            .gap_2()
            .p_4()
            .border_l_1()
            .border_color(theme.border)
            .child(div().text_xs().font_weight(FontWeight::BOLD).text_color(muted).child(l.t("PREVIEW")))
            .child(screen(&draft, &sample, background.clone(), px(468.)))
            .child(div().text_xs().text_color(muted).child(l.t("Blanked to the logo:")))
            .child(
                div()
                    .relative()
                    .child(screen(&draft, &[], background, px(234.)))
                    .children(logo.map(|src| {
                        gpui::img(src)
                            .absolute()
                            .top(px(234. * 9. / 16. * 0.3))
                            .left_0()
                            .w(px(234.))
                            .h(px(234. * 9. / 16. * 0.4))
                            .object_fit(gpui::ObjectFit::Contain)
                    })),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(l.t("An approximation: the projector shows the real thing.")),
            );

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
                    .child(Button::new("theme-new").small().label(l.t("New")).on_click(cx.listener(|this, _, w, cx| this.add(false, w, cx))))
                    .child(Button::new("theme-duplicate")
                            .small()
                            .label(l.t("Duplicate"))
                            .on_click(cx.listener(|this, _, w, cx| this.add(true, w, cx))))
                    .child(Button::new("theme-delete")
                            .small()
                            .ghost()
                            .label(l.t("Delete…"))
                            .disabled(is_default)
                            .on_click(cx.listener(|this, _, w, cx| this.delete(w, cx))))
                    .child(Button::new("theme-default")
                            .small()
                            .ghost()
                            .label(if is_default { l.t("The default theme") } else { l.t("Make default") })
                            .disabled(is_default || dirty)
                            .on_click(cx.listener(|this, _, w, cx| this.make_default(w, cx))))
                    .child(div().flex_1().text_xs().text_color(muted).when(dirty, |d| d.child(l.t("Unsaved changes"))))
                    .child(Button::new("theme-save")
                            .small()
                            .primary()
                            .label(l.t("Save  ⌘S"))
                            .disabled(!dirty)
                            .on_click(cx.listener(|this, _, w, cx| this.save(w, cx)))),
            )
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
        let (h, editor) = cx.update(|cx| ThemeEditor::open(lib, None, cx).unwrap());
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
        w.render_frame(cx);
        w.scroll("theme-form", gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-2000.))), cx);
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
        let t = Theme { size_min_vh: 3.5, outline_vh: 0.25, transition_ms: 450, ..Theme::default() };
        for f in Field::ALL {
            let mut back = Theme { name: "x".into(), font_family: "y".into(), ..Theme::default() };
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
            (Field::Name, "  ", false),
            (Field::Font, "", false),
            (Field::TransitionMs, "-1", false),
            (Field::TransitionMs, "99999", false),
            (Field::LineHeight, "1.4", true),
        ] {
            assert_eq!(f.set(&mut t, text).is_ok(), ok, "{f:?} {text:?}");
        }
        assert_eq!((t.size_min_vh, t.color.as_str(), t.line_height), (4.5, "#ff0", 1.4));
    }

    #[gpui::test]
    fn editing_and_saving_a_theme(cx: &mut TestAppContext) {
        let (library, editor, h, events) = open(cx, Library::open_in_memory().unwrap());
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
            scroll_form_down(w, cx);
            w.click("align-left", cx);
            w.click("valign-Bottom", cx);
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
    fn new_duplicate_default_and_delete(cx: &mut TestAppContext) {
        let (library, editor, h, events) = open(cx, Library::open_in_memory().unwrap());
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
        let mut lib = Library::open_in_memory().unwrap();
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
