//! Themes: how projected text and its background look. Stored in the library
//! as JSON, edited in the console, and sent to the display page, which applies
//! [`Theme::css_vars`] as CSS custom properties.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::SlideKind;

/// The theme every library starts with and that can't be deleted.
pub const DEFAULT_THEME_NAME: &str = "Default";

/// Visual settings for the display page. Sizes are in percent of the viewport
/// height (`vh`) so output scales with the screen; margins in percent of the
/// width and height. Missing fields (an older stored theme) take their
/// default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    /// A family name: one of the bundled fonts, or any font installed on the
    /// machine showing the output.
    pub font_family: String,
    pub font_weight: u16,
    /// Auto-fit range for the slide text.
    pub size_min_vh: f32,
    pub size_max_vh: f32,
    pub line_height: f32,
    /// Extra space between letters, in em (negative tightens).
    pub letter_spacing_em: f32,
    pub text_case: TextCase,
    /// Colours are `#rgb`, `#rrggbb` or `#rrggbbaa`.
    pub color: String,
    /// `**strong**` words in lyrics: bolder, in this colour (empty: the
    /// text colour).
    pub strong_color: String,
    /// `__em__` words in lyrics: italic, in this colour (empty: the text
    /// colour).
    pub em_color: String,
    pub outline_color: String,
    /// Visible outline width; 0 for none.
    pub outline_vh: f32,
    pub shadow_color: String,
    /// Blur radius; 0 with no offset means no shadow.
    pub shadow_blur_vh: f32,
    /// Downward offset.
    pub shadow_offset_vh: f32,
    pub align: Align,
    pub valign: VAlign,
    /// Safe-area margins in percent of width/height.
    pub margin_x_pct: f32,
    pub margin_y_pct: f32,
    pub background_color: String,
    /// Empty for a solid background; else it fades from `background_color`
    /// at the top left to this colour at the bottom right.
    pub gradient_color: String,
    /// How much the background (colour, gradient and image) is darkened,
    /// 0 to [`MAX_DIM`], so text stands out on a busy picture.
    pub background_dim: f32,
    /// URL on the local server: `/media/<file>` (imported) or `/assets/…`.
    pub background_image: Option<String>,
    /// A looping, muted video (`/media/<file>`) over the image.
    pub background_video: Option<String>,
    /// A shape behind the slide text.
    pub panel: PanelKind,
    pub panel_color: String,
    /// How far the panel reaches past the text.
    pub panel_padding_vh: f32,
    /// Corner radius of a box.
    pub panel_radius_vh: f32,
    /// The text areas get a box of the panel's colour too.
    pub panel_behind_areas: bool,
    /// Shown centred over the background when blanked to the logo.
    pub logo: Option<String>,
    pub transition: TransitionKind,
    pub transition_ms: u32,
    /// The song title, the credits (authors, copyright) and the scripture
    /// reference, around the slide text.
    pub title_area: TextArea,
    pub credits_area: TextArea,
    pub reference_area: TextArea,
    /// Where scripture slides put their text and how big; `None`: as
    /// lyrics (the fields above).
    pub scripture: Option<Layout>,
}

/// The shape behind the slide text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelKind {
    #[default]
    None,
    /// A box around the text.
    Box,
    /// A band across the whole screen, as high as the text.
    Bar,
}

impl PanelKind {
    pub const ALL: [PanelKind; 3] = [PanelKind::None, PanelKind::Box, PanelKind::Bar];
    pub fn as_str(self) -> &'static str {
        match self {
            PanelKind::None => "none",
            PanelKind::Box => "box",
            PanelKind::Bar => "bar",
        }
    }
}

/// Where a text area goes: in the top or bottom margin, or nowhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    Hidden,
    Top,
    #[default]
    Bottom,
}

impl Place {
    pub const ALL: [Place; 3] = [Place::Hidden, Place::Top, Place::Bottom];
}

/// One of the small text areas around the slide text. Areas in the same
/// place and alignment share a line, in [`AreaKind::ALL`] order, separated
/// by " · ".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextArea {
    pub place: Place,
    /// `None`: aligned like the slide text.
    pub align: Option<Align>,
    pub size_vh: f32,
    /// Empty: the text colour.
    pub color: String,
    pub text_case: TextCase,
}

impl Default for TextArea {
    /// Today's footer: one small line at the bottom, aligned like the text.
    fn default() -> Self {
        Self { place: Place::Bottom, align: None, size_vh: 2.2, color: String::new(), text_case: TextCase::Normal }
    }
}

/// Which text area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaKind {
    Title,
    Reference,
    Credits,
}

impl AreaKind {
    pub const ALL: [AreaKind; 3] = [AreaKind::Title, AreaKind::Reference, AreaKind::Credits];

    /// Its name in the slide JSON and the display page's classes.
    pub fn name(self) -> &'static str {
        match self {
            AreaKind::Title => "title",
            AreaKind::Reference => "reference",
            AreaKind::Credits => "credits",
        }
    }

    /// The area's text on `slide`.
    pub fn text(self, slide: &crate::Slide) -> Option<&str> {
        match self {
            AreaKind::Title => slide.title.as_deref(),
            AreaKind::Reference => slide.reference.as_deref(),
            AreaKind::Credits => slide.credits.as_deref(),
        }
    }
}

/// Where the slide text goes and how big, for one kind of slide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub align: Align,
    pub valign: VAlign,
    pub size_min_vh: f32,
    pub size_max_vh: f32,
    pub text_case: TextCase,
}

impl Default for Layout {
    fn default() -> Self {
        Theme::default().layout(SlideKind::Lyrics)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

/// How the slide text's letters are cased on screen (the lyrics keep theirs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextCase {
    /// As typed.
    #[default]
    Normal,
    /// ALL CAPS.
    Upper,
}

impl TextCase {
    pub const ALL: [TextCase; 2] = [TextCase::Normal, TextCase::Upper];
    pub fn css(self) -> &'static str {
        match self {
            TextCase::Normal => "none",
            TextCase::Upper => "uppercase",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// How one slide replaces the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionKind {
    /// Cross-fade.
    #[default]
    Fade,
    /// The new slide rises into place as it fades in.
    Slide,
    /// Instant.
    Cut,
}

impl Align {
    pub const ALL: [Align; 3] = [Align::Left, Align::Center, Align::Right];
    pub fn css(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }
    /// The flexbox `justify-content` value that puts the text block there.
    pub fn justify(self) -> &'static str {
        match self {
            Align::Left => "flex-start",
            Align::Center => "center",
            Align::Right => "flex-end",
        }
    }
}

impl VAlign {
    pub const ALL: [VAlign; 3] = [VAlign::Top, VAlign::Middle, VAlign::Bottom];
    /// The flexbox `align-items` value that puts the text there.
    pub fn css(self) -> &'static str {
        match self {
            VAlign::Top => "flex-start",
            VAlign::Middle => "center",
            VAlign::Bottom => "flex-end",
        }
    }
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 3] = [TransitionKind::Fade, TransitionKind::Slide, TransitionKind::Cut];
    pub fn as_str(self) -> &'static str {
        match self {
            TransitionKind::Fade => "fade",
            TransitionKind::Slide => "slide",
            TransitionKind::Cut => "cut",
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: DEFAULT_THEME_NAME.into(),
            font_family: "Source Sans 3".into(),
            font_weight: 700,
            size_min_vh: 4.0,
            size_max_vh: 11.0,
            line_height: 1.2,
            letter_spacing_em: 0.0,
            text_case: TextCase::Normal,
            color: "#ffffff".into(),
            strong_color: "#ffd166".into(),
            em_color: String::new(),
            outline_color: "#000000e6".into(),
            outline_vh: 0.45,
            shadow_color: "#000000d9".into(),
            shadow_blur_vh: 1.2,
            shadow_offset_vh: 0.4,
            align: Align::Center,
            valign: VAlign::Middle,
            margin_x_pct: 6.0,
            margin_y_pct: 8.0,
            background_color: "#101018".into(),
            gradient_color: String::new(),
            background_dim: 0.0,
            background_image: Some("/assets/background.svg".into()),
            background_video: None,
            panel: PanelKind::None,
            panel_color: "#00000099".into(),
            panel_padding_vh: 2.5,
            panel_radius_vh: 1.5,
            panel_behind_areas: false,
            logo: Some("/assets/logo.svg".into()),
            transition: TransitionKind::Fade,
            transition_ms: 300,
            title_area: TextArea::default(),
            credits_area: TextArea::default(),
            reference_area: TextArea::default(),
            scripture: None,
        }
    }
}

/// Limits the editor and [`Theme::sanitized`] enforce.
pub const SIZE_RANGE_VH: (f32, f32) = (1.0, 40.0);
pub const MAX_TRANSITION_MS: u32 = 5000;
pub const LETTER_SPACING_RANGE_EM: (f32, f32) = (-0.1, 0.5);
pub const MAX_DIM: f32 = 0.9;
pub const AREA_SIZE_RANGE_VH: (f32, f32) = (1.0, 10.0);
pub const MAX_PANEL_PADDING_VH: f32 = 10.0;
pub const MAX_PANEL_RADIUS_VH: f32 = 10.0;

/// An RGBA colour parsed from `#rgb`, `#rrggbb` or `#rrggbbaa`.
pub fn parse_color(text: &str) -> Option<[u8; 4]> {
    let hex = text.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|v| v * 17);
            Some([d(0)?, d(1)?, d(2)?, 255])
        }
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

/// `rgba(r, g, b, a)`: the display page's spelling of a colour.
fn css_color(text: &str) -> String {
    let [r, g, b, a] = parse_color(text).unwrap_or([0, 0, 0, 255]);
    if a == 255 {
        format!("rgb({r}, {g}, {b})")
    } else {
        format!("rgba({r}, {g}, {b}, {})", trim_float(f32::from(a) / 255.0, 3))
    }
}

/// An optional colour: `inherit` (the text's) when empty.
fn or_text_color(text: &str) -> String {
    if text.trim().is_empty() { "inherit".into() } else { css_color(text) }
}

/// `1.5`, `2`, `0.333`: no trailing zeros.
fn trim_float(v: f32, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.into() }
}

/// A CSS string literal.
fn css_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => {
                let _ = write!(out, "\\{:x} ", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `url("…")` for a server path, or `none`.
fn css_url(url: Option<&str>) -> String {
    match url.map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => format!("url({})", css_string(u)),
        None => "none".into(),
    }
}

impl Theme {
    /// Placement and sizes for `kind` of slide.
    pub fn layout(&self, kind: SlideKind) -> Layout {
        match (kind, &self.scripture) {
            (SlideKind::Scripture, Some(layout)) => layout.clone(),
            _ => Layout {
                align: self.align,
                valign: self.valign,
                size_min_vh: self.size_min_vh,
                size_max_vh: self.size_max_vh,
                text_case: self.text_case,
            },
        }
    }

    pub fn area(&self, kind: AreaKind) -> &TextArea {
        match kind {
            AreaKind::Title => &self.title_area,
            AreaKind::Reference => &self.reference_area,
            AreaKind::Credits => &self.credits_area,
        }
    }

    pub fn area_mut(&mut self, kind: AreaKind) -> &mut TextArea {
        match kind {
            AreaKind::Title => &mut self.title_area,
            AreaKind::Reference => &mut self.reference_area,
            AreaKind::Credits => &mut self.credits_area,
        }
    }

    /// The theme as CSS custom properties for the display page, in a fixed
    /// order. The page sets each on `:root`; `display.css` uses them.
    pub fn css_vars(&self) -> Vec<(&'static str, String)> {
        let shadow = if self.shadow_blur_vh <= 0.0 && self.shadow_offset_vh <= 0.0 {
            "none".to_string()
        } else {
            format!(
                "0 {}vh {}vh {}",
                trim_float(self.shadow_offset_vh, 2),
                trim_float(self.shadow_blur_vh, 2),
                css_color(&self.shadow_color)
            )
        };
        let mut vars = vec![
            // A cut is a transition that takes no time.
            ("--fade", format!("{}ms", if self.transition == TransitionKind::Cut { 0 } else { self.transition_ms })),
            // How far an incoming slide travels (and an outgoing one leaves).
            ("--shift", if self.transition == TransitionKind::Slide { "6vh" } else { "0vh" }.into()),
            ("--font-family", format!("{}, system-ui, sans-serif", css_string(&self.font_family))),
            ("--font-weight", self.font_weight.to_string()),
            ("--line-height", trim_float(self.line_height, 2)),
            ("--letter-spacing", format!("{}em", trim_float(self.letter_spacing_em, 3))),
            ("--text-transform", self.text_case.css().into()),
            ("--color", css_color(&self.color)),
            ("--strong-color", or_text_color(&self.strong_color)),
            ("--em-color", or_text_color(&self.em_color)),
            ("--outline-color", css_color(&self.outline_color)),
            // The stroke is centred on the glyph edge and painted under the
            // fill, so only half of it shows: draw twice the visible width.
            ("--outline-width", format!("{}vh", trim_float(self.outline_vh.max(0.0) * 2.0, 2))),
            ("--shadow", shadow),
            ("--align", self.align.css().into()),
            ("--justify", self.align.justify().into()),
            ("--valign", self.valign.css().into()),
            ("--mx", format!("{}vw", trim_float(self.margin_x_pct, 2))),
            ("--my", format!("{}vh", trim_float(self.margin_y_pct, 2))),
            ("--bg-color", css_color(&self.background_color)),
            ("--bg-image", css_url(self.background_image.as_deref())),
            ("--bg-gradient", match parse_color(&self.gradient_color) {
                Some(_) => format!(
                    "linear-gradient(135deg, {}, {})",
                    css_color(&self.background_color),
                    css_color(&self.gradient_color)
                ),
                None => "none".into(),
            }),
            ("--bg-dim", trim_float(self.background_dim, 2)),
            ("--logo", css_url(self.logo.as_deref())),
            // The panel: a box reaches --panel-pad past the text on every
            // side, a bar past the screen's sides.
            ("--panel-display", if self.panel == PanelKind::None { "none" } else { "block" }.into()),
            ("--panel-color", css_color(&self.panel_color)),
            ("--panel-pad", format!("{}vh", trim_float(self.panel_padding_vh, 2))),
            ("--panel-x", match self.panel {
                PanelKind::Bar => "100vw".into(),
                _ => format!("{}vh", trim_float(self.panel_padding_vh, 2)),
            }),
            ("--panel-radius", match self.panel {
                PanelKind::Box => format!("{}vh", trim_float(self.panel_radius_vh, 2)),
                _ => "0".into(),
            }),
            ("--areas-panel", if self.panel_behind_areas { css_color(&self.panel_color) } else { "transparent".into() }),
        ];
        // Scripture slides' layer redefines --align, --justify, --valign
        // and --text-transform from these.
        let scripture = self.layout(SlideKind::Scripture);
        vars.extend([
            ("--scripture-align", scripture.align.css().into()),
            ("--scripture-justify", scripture.align.justify().into()),
            ("--scripture-valign", scripture.valign.css().into()),
            ("--scripture-transform", scripture.text_case.css().into()),
        ]);
        for kind in AreaKind::ALL {
            let area = self.area(kind);
            let [size, color, transform] = match kind {
                AreaKind::Title => ["--title-size", "--title-color", "--title-transform"],
                AreaKind::Reference => ["--reference-size", "--reference-color", "--reference-transform"],
                AreaKind::Credits => ["--credits-size", "--credits-color", "--credits-transform"],
            };
            vars.extend([
                (size, format!("{}vh", trim_float(area.size_vh, 2))),
                (color, or_text_color(&area.color)),
                (transform, area.text_case.css().into()),
            ]);
        }
        vars
    }

    /// The same theme with every value in range and every colour valid
    /// (invalid ones take the default's), so a hand-edited or damaged stored
    /// theme can never break the output.
    pub fn sanitized(mut self) -> Self {
        let d = Theme::default();
        let clamp = |v: f32, lo: f32, hi: f32, def: f32| if v.is_finite() { v.clamp(lo, hi) } else { def };
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            self.name = "Untitled".into();
        }
        self.font_family = self.font_family.trim().to_string();
        if self.font_family.is_empty() {
            self.font_family = d.font_family.clone();
        }
        self.font_weight = (self.font_weight / 100 * 100).clamp(100, 900);
        self.size_min_vh = clamp(self.size_min_vh, SIZE_RANGE_VH.0, SIZE_RANGE_VH.1, d.size_min_vh);
        self.size_max_vh = clamp(self.size_max_vh, self.size_min_vh, SIZE_RANGE_VH.1, d.size_max_vh.max(self.size_min_vh));
        self.line_height = clamp(self.line_height, 0.8, 3.0, d.line_height);
        self.letter_spacing_em =
            clamp(self.letter_spacing_em, LETTER_SPACING_RANGE_EM.0, LETTER_SPACING_RANGE_EM.1, d.letter_spacing_em);
        self.background_dim = clamp(self.background_dim, 0.0, MAX_DIM, d.background_dim);
        self.panel_padding_vh = clamp(self.panel_padding_vh, 0.0, MAX_PANEL_PADDING_VH, d.panel_padding_vh);
        self.panel_radius_vh = clamp(self.panel_radius_vh, 0.0, MAX_PANEL_RADIUS_VH, d.panel_radius_vh);
        self.outline_vh = clamp(self.outline_vh, 0.0, 3.0, d.outline_vh);
        self.shadow_blur_vh = clamp(self.shadow_blur_vh, 0.0, 10.0, d.shadow_blur_vh);
        self.shadow_offset_vh = clamp(self.shadow_offset_vh, 0.0, 10.0, d.shadow_offset_vh);
        self.margin_x_pct = clamp(self.margin_x_pct, 0.0, 40.0, d.margin_x_pct);
        self.margin_y_pct = clamp(self.margin_y_pct, 0.0, 40.0, d.margin_y_pct);
        self.transition_ms = self.transition_ms.min(MAX_TRANSITION_MS);
        for (value, default) in [
            (&mut self.color, &d.color),
            (&mut self.outline_color, &d.outline_color),
            (&mut self.shadow_color, &d.shadow_color),
            (&mut self.background_color, &d.background_color),
            (&mut self.panel_color, &d.panel_color),
        ] {
            *value = match parse_color(value) {
                Some(_) => value.trim().to_ascii_lowercase(),
                None => default.clone(),
            };
        }
        for (value, default) in [
            (&mut self.strong_color, &d.strong_color),
            (&mut self.em_color, &d.em_color),
            (&mut self.gradient_color, &d.gradient_color),
        ] {
            *value = match parse_color(value) {
                Some(_) => value.trim().to_ascii_lowercase(),
                None if value.trim().is_empty() => String::new(),
                None => default.clone(),
            };
        }
        for url in [&mut self.background_image, &mut self.background_video, &mut self.logo] {
            *url = url.take().map(|u| u.trim().to_string()).filter(|u| is_local_url(u));
        }
        for kind in AreaKind::ALL {
            let area = self.area_mut(kind);
            area.size_vh = clamp(area.size_vh, AREA_SIZE_RANGE_VH.0, AREA_SIZE_RANGE_VH.1, TextArea::default().size_vh);
            area.color = match parse_color(&area.color) {
                Some(_) => area.color.trim().to_ascii_lowercase(),
                None => String::new(),
            };
        }
        if let Some(l) = &mut self.scripture {
            l.size_min_vh = clamp(l.size_min_vh, SIZE_RANGE_VH.0, SIZE_RANGE_VH.1, d.size_min_vh);
            l.size_max_vh = clamp(l.size_max_vh, l.size_min_vh, SIZE_RANGE_VH.1, d.size_max_vh.max(l.size_min_vh));
        }
        self
    }
}

/// Bump when [`builtin_themes`] gains a theme, so libraries get it (once).
pub const BUILTIN_THEMES_VERSION: u32 = 2;

/// The [`BUILTIN_THEMES_VERSION`] that added a built-in theme, so a library
/// seeded before gets only the newer ones.
pub fn builtin_theme_version(name: &str) -> u32 {
    match name {
        "Panel" => 2,
        _ => 1,
    }
}

/// The built-in logo in dark ink, for light backgrounds.
const DARK_LOGO: &str = "/assets/logo-dark.svg";

/// The curated themes every library gets, besides "Default": four dark
/// gradients, two light ones (for bright rooms), one for scripture, one for
/// big, bold lyrics and (since version 2) one with a bar behind the text,
/// for busy pictures and lower thirds. Names read the same in English and
/// Spanish.
pub fn builtin_themes() -> Vec<Theme> {
    let base = Theme { background_image: None, ..Theme::default() };
    let shadow = |color: &str, blur: f32, offset: f32| (color.to_string(), blur, offset);
    let themed = |name: &str, f: &dyn Fn(&mut Theme)| {
        let mut t = Theme { name: name.into(), ..base.clone() };
        f(&mut t);
        t
    };
    // The song title in capitals at the top, in an accent colour.
    let top_title = |color: &str, size_vh: f32| TextArea {
        place: Place::Top,
        size_vh,
        color: color.into(),
        text_case: TextCase::Upper,
        ..TextArea::default()
    };
    let set_shadow = |t: &mut Theme, (color, blur, offset): (String, f32, f32)| {
        (t.shadow_color, t.shadow_blur_vh, t.shadow_offset_vh) = (color, blur, offset);
    };
    vec![
        themed("Aurora", &|t| {
            (t.font_family, t.font_weight) = ("Montserrat".into(), 600);
            (t.size_max_vh, t.line_height, t.letter_spacing_em) = (10.0, 1.25, 0.01);
            (t.strong_color, t.outline_vh) = ("#fcd34d".into(), 0.0);
            set_shadow(t, shadow("#0000008c", 1.5, 0.3));
            (t.background_color, t.gradient_color) = ("#0b1026".into(), "#5b21b6".into());
            t.transition_ms = 400;
            t.title_area = top_title("#c4b5fd", 2.4);
        }),
        themed("Jade", &|t| {
            t.font_weight = 600;
            (t.color, t.strong_color, t.outline_vh) = ("#f0fdf4".into(), "#6ee7b7".into(), 0.0);
            set_shadow(t, shadow("#00000080", 1.2, 0.3));
            (t.background_color, t.gradient_color) = ("#022c22".into(), "#0f766e".into());
        }),
        themed("Coral", &|t| {
            (t.font_family, t.font_weight, t.text_case) = ("Montserrat".into(), 700, TextCase::Upper);
            (t.size_min_vh, t.size_max_vh, t.line_height, t.letter_spacing_em) = (3.5, 8.5, 1.3, 0.05);
            (t.color, t.strong_color, t.outline_vh) = ("#fff7ed".into(), "#fdba74".into(), 0.0);
            set_shadow(t, shadow("#00000066", 1.0, 0.2));
            (t.background_color, t.gradient_color) = ("#3f0d1f".into(), "#be123c".into());
            (t.transition, t.transition_ms) = (TransitionKind::Slide, 450);
            t.title_area = top_title("#fdba74", 2.4);
            t.scripture =
                Some(Layout { size_min_vh: 3.5, size_max_vh: 7.5, text_case: TextCase::Normal, ..t.layout(SlideKind::Lyrics) });
        }),
        themed("Sepia", &|t| {
            (t.font_family, t.font_weight) = ("Lora".into(), 500);
            (t.size_min_vh, t.size_max_vh, t.line_height) = (3.5, 8.0, 1.4);
            (t.color, t.strong_color, t.outline_vh) = ("#f5ead6".into(), "#e9b872".into(), 0.0);
            set_shadow(t, shadow("#00000099", 1.0, 0.2));
            (t.align, t.margin_x_pct, t.margin_y_pct) = (Align::Left, 10.0, 12.0);
            (t.background_color, t.gradient_color) = ("#1a120b".into(), "#3d2b1f".into());
            t.transition_ms = 500;
            // Made for scripture: the reference heads the passage.
            t.reference_area = TextArea { place: Place::Top, size_vh: 3.2, color: "#e9b872".into(), ..TextArea::default() };
        }),
        themed("Lava", &|t| {
            (t.font_family, t.font_weight, t.text_case) = ("Oswald".into(), 600, TextCase::Upper);
            (t.size_min_vh, t.size_max_vh, t.line_height, t.letter_spacing_em) = (5.0, 13.0, 1.1, 0.03);
            (t.strong_color, t.outline_color, t.outline_vh) = ("#ff8a3d".into(), "#000000".into(), 0.3);
            set_shadow(t, shadow("#000000cc", 1.5, 0.4));
            (t.valign, t.margin_y_pct) = (VAlign::Bottom, 12.0);
            (t.background_color, t.gradient_color) = ("#0a0a0a".into(), "#3b0d02".into());
            t.transition = TransitionKind::Slide;
            t.title_area = TextArea { align: Some(Align::Left), ..top_title("#ff8a3d", 2.6) };
            // Verses read better smaller, as typed, in the middle.
            t.scripture = Some(Layout {
                valign: VAlign::Middle,
                size_min_vh: 4.0,
                size_max_vh: 8.0,
                text_case: TextCase::Normal,
                ..t.layout(SlideKind::Lyrics)
            });
        }),
        themed("Oasis", &|t| {
            (t.font_family, t.font_weight, t.line_height) = ("Lora".into(), 600, 1.3);
            (t.color, t.strong_color, t.outline_vh) = ("#1f2937".into(), "#b45309".into(), 0.0);
            set_shadow(t, shadow("#00000000", 0.0, 0.0));
            (t.background_color, t.gradient_color) = ("#fbf7ef".into(), "#eadcc2".into());
            t.logo = Some(DARK_LOGO.into());
            t.transition_ms = 350;
        }),
        themed("Polar", &|t| {
            t.font_weight = 600;
            (t.color, t.strong_color, t.outline_vh) = ("#0f172a".into(), "#0369a1".into(), 0.0);
            set_shadow(t, shadow("#00000000", 0.0, 0.0));
            (t.background_color, t.gradient_color) = ("#f8fafc".into(), "#dbeafe".into());
            t.logo = Some(DARK_LOGO.into());
        }),
        themed("Gala", &|t| {
            (t.font_family, t.font_weight, t.text_case) = ("Montserrat".into(), 400, TextCase::Upper);
            (t.size_min_vh, t.size_max_vh, t.line_height, t.letter_spacing_em) = (3.5, 8.0, 1.5, 0.12);
            (t.color, t.strong_color, t.outline_vh) = ("#f5f5f5".into(), "#d4af37".into(), 0.0);
            set_shadow(t, shadow("#000000b3", 1.0, 0.0));
            (t.background_color, t.gradient_color) = ("#050505".into(), "#1c1917".into());
            t.transition_ms = 700;
            t.title_area = top_title("#d4af37", 2.2);
            t.scripture = Some(Layout { text_case: TextCase::Normal, ..t.layout(SlideKind::Lyrics) });
        }),
        themed("Panel", &|t| {
            // The default picture, under a dark bar across the bottom.
            t.background_image = Some("/assets/background.svg".into());
            (t.font_weight, t.size_min_vh, t.size_max_vh, t.outline_vh) = (600, 3.5, 7.5, 0.0);
            set_shadow(t, shadow("#00000000", 0.0, 0.0));
            (t.valign, t.margin_y_pct) = (VAlign::Bottom, 12.0);
            (t.panel, t.panel_color, t.panel_padding_vh) = (PanelKind::Bar, "#000000b3".into(), 3.0);
            t.panel_behind_areas = true;
            (t.background_color, t.gradient_color) = ("#101018".into(), "#312e81".into());
        }),
    ]
}

/// Theme images must come from the local server (`/media/…`, `/assets/…`),
/// so output never depends on the network.
fn is_local_url(url: &str) -> bool {
    url.starts_with('/') && !url.starts_with("//") && !url.contains("..") && !url.contains(['"', '\\', '\n'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_in_three_lengths() {
        for (text, want) in [
            ("#fff", Some([255, 255, 255, 255])),
            ("#1E90ff", Some([30, 144, 255, 255])),
            (" #00000080 ", Some([0, 0, 0, 128])),
            ("#12", None),
            ("123456", None),
            ("#ggg", None),
            ("#ffffffff0", None),
            ("", None),
        ] {
            assert_eq!(parse_color(text), want, "{text:?}");
        }
    }

    fn var(t: &Theme, name: &str) -> String {
        t.css_vars().into_iter().find(|(k, _)| *k == name).unwrap().1
    }

    #[test]
    fn css_vars_table() {
        let base = Theme::default();
        type Case = (&'static str, fn(&mut Theme), &'static str);
        let cases: &[Case] = &[
            ("--font-family", |_| {}, "\"Source Sans 3\", system-ui, sans-serif"),
            ("--font-family", |t| t.font_family = "Bad\"Font".into(), "\"Bad\\\"Font\", system-ui, sans-serif"),
            ("--font-weight", |t| t.font_weight = 400, "400"),
            ("--color", |_| {}, "rgb(255, 255, 255)"),
            ("--color", |t| t.color = "#ff000080".into(), "rgba(255, 0, 0, 0.502)"),
            ("--outline-color", |_| {}, "rgba(0, 0, 0, 0.902)"),
            ("--outline-width", |_| {}, "0.9vh"),
            ("--outline-width", |t| t.outline_vh = 0.0, "0vh"),
            ("--shadow", |_| {}, "0 0.4vh 1.2vh rgba(0, 0, 0, 0.851)"),
            ("--shadow", |t| (t.shadow_blur_vh, t.shadow_offset_vh) = (0.0, 0.0), "none"),
            ("--align", |t| t.align = Align::Left, "left"),
            ("--valign", |t| t.valign = VAlign::Bottom, "flex-end"),
            ("--valign", |_| {}, "center"),
            ("--mx", |t| t.margin_x_pct = 10.5, "10.5vw"),
            ("--my", |_| {}, "8vh"),
            ("--line-height", |t| t.line_height = 1.25, "1.25"),
            ("--bg-color", |t| t.background_color = "#000".into(), "rgb(0, 0, 0)"),
            ("--bg-image", |_| {}, "url(\"/assets/background.svg\")"),
            ("--bg-image", |t| t.background_image = None, "none"),
            ("--bg-image", |t| t.background_image = Some("/media/a b.jpg".into()), "url(\"/media/a b.jpg\")"),
            ("--logo", |t| t.logo = Some("  ".into()), "none"),
            ("--fade", |t| t.transition_ms = 750, "750ms"),
            ("--fade", |t| t.transition = TransitionKind::Cut, "0ms"),
            ("--shift", |_| {}, "0vh"),
            ("--shift", |t| t.transition = TransitionKind::Slide, "6vh"),
            ("--justify", |t| t.align = Align::Right, "flex-end"),
            ("--letter-spacing", |_| {}, "0em"),
            ("--letter-spacing", |t| t.letter_spacing_em = 0.08, "0.08em"),
            ("--text-transform", |_| {}, "none"),
            ("--text-transform", |t| t.text_case = TextCase::Upper, "uppercase"),
            ("--bg-gradient", |_| {}, "none"),
            ("--bg-gradient", |t| t.gradient_color = "#4c1d95".into(), "linear-gradient(135deg, rgb(16, 16, 24), rgb(76, 29, 149))"),
            ("--bg-dim", |_| {}, "0"),
            ("--bg-dim", |t| t.background_dim = 0.35, "0.35"),
            ("--strong-color", |_| {}, "rgb(255, 209, 102)"),
            ("--strong-color", |t| t.strong_color = " ".into(), "inherit"),
            ("--em-color", |_| {}, "inherit"),
            ("--em-color", |t| t.em_color = "#0f0".into(), "rgb(0, 255, 0)"),
            ("--title-size", |_| {}, "2.2vh"),
            ("--panel-display", |_| {}, "none"),
            ("--panel-display", |t| t.panel = PanelKind::Box, "block"),
            ("--panel-color", |_| {}, "rgba(0, 0, 0, 0.6)"),
            ("--panel-pad", |t| t.panel_padding_vh = 4.0, "4vh"),
            ("--panel-x", |t| t.panel = PanelKind::Box, "2.5vh"),
            ("--panel-x", |t| t.panel = PanelKind::Bar, "100vw"),
            ("--panel-radius", |t| t.panel = PanelKind::Box, "1.5vh"),
            ("--panel-radius", |t| t.panel = PanelKind::Bar, "0"),
            ("--areas-panel", |_| {}, "transparent"),
            ("--areas-panel", |t| t.panel_behind_areas = true, "rgba(0, 0, 0, 0.6)"),
            ("--title-color", |_| {}, "inherit"),
            ("--credits-color", |t| t.credits_area.color = "#aabbcc".into(), "rgb(170, 187, 204)"),
            ("--reference-transform", |t| t.reference_area.text_case = TextCase::Upper, "uppercase"),
            ("--reference-size", |t| t.reference_area.size_vh = 3.5, "3.5vh"),
            ("--scripture-align", |t| t.align = Align::Right, "right"),
            ("--scripture-justify", |t| t.scripture = Some(Layout { align: Align::Left, ..Layout::default() }), "flex-start"),
            ("--scripture-valign", |t| t.scripture = Some(Layout { valign: VAlign::Top, ..Layout::default() }), "flex-start"),
            ("--scripture-transform", |t| t.text_case = TextCase::Upper, "uppercase"),
            ("--scripture-transform", |t| {
                t.text_case = TextCase::Upper;
                t.scripture = Some(Layout::default());
            }, "none"),
        ];
        for (name, change, want) in cases {
            let mut t = base.clone();
            change(&mut t);
            assert_eq!(var(&t, name), *want, "{name}");
        }
        // Every variable display.css uses is set, once.
        let names: Vec<_> = base.css_vars().into_iter().map(|(k, _)| k).collect();
        let css = include_str!("../../../display/display.css");
        for n in &names {
            assert!(css.contains(&format!("var({n})")), "display.css uses {n}");
            assert_eq!(names.iter().filter(|m| *m == n).count(), 1);
        }
    }

    #[test]
    fn sanitized_repairs_anything_out_of_range() {
        let t = Theme {
            name: "  ".into(),
            font_family: "".into(),
            font_weight: 1234,
            size_min_vh: -3.0,
            size_max_vh: f32::NAN,
            line_height: 0.0,
            color: "red".into(),
            outline_color: "#ABCDEF".into(),
            strong_color: "gold".into(),
            letter_spacing_em: 2.0,
            background_dim: -1.0,
            gradient_color: "nope".into(),
            em_color: " ".into(),
            outline_vh: 99.0,
            shadow_blur_vh: -1.0,
            margin_x_pct: 80.0,
            transition_ms: 60_000,
            background_image: Some("https://example.com/x.jpg".into()),
            background_video: Some("//cdn.example.com/loop.mp4".into()),
            panel_color: "nope".into(),
            panel_padding_vh: -2.0,
            panel_radius_vh: 99.0,
            logo: Some("/media/../../etc/passwd".into()),
            ..Theme::default()
        }
        .sanitized();
        assert_eq!(t.name, "Untitled");
        assert_eq!(t.font_family, "Source Sans 3");
        assert_eq!(t.font_weight, 900);
        assert_eq!((t.size_min_vh, t.size_max_vh), (1.0, 11.0));
        assert_eq!(t.line_height, 0.8);
        assert_eq!(t.color, "#ffffff");
        assert_eq!(t.outline_color, "#abcdef");
        assert_eq!((t.strong_color.as_str(), t.em_color.as_str()), ("#ffd166", ""));
        assert_eq!((t.letter_spacing_em, t.background_dim, t.gradient_color.as_str()), (0.5, 0.0, ""));
        assert_eq!((t.outline_vh, t.shadow_blur_vh, t.margin_x_pct), (3.0, 0.0, 40.0));
        assert_eq!(t.transition_ms, MAX_TRANSITION_MS);
        assert_eq!((t.background_image, t.background_video, t.logo), (None, None, None));
        assert_eq!((t.panel_color.as_str(), t.panel_padding_vh, t.panel_radius_vh), ("#00000099", 0.0, MAX_PANEL_RADIUS_VH));
        assert_eq!(Theme::default().sanitized(), Theme::default());
    }

    #[test]
    fn scripture_has_its_own_layout_or_the_lyrics_one() {
        let t = Theme { align: Align::Left, size_max_vh: 9.0, ..Theme::default() };
        let lyrics = t.layout(SlideKind::Lyrics);
        assert_eq!((lyrics.align, lyrics.size_max_vh), (Align::Left, 9.0));
        assert_eq!(t.layout(SlideKind::Scripture), lyrics);
        let own = Layout { valign: VAlign::Top, size_min_vh: 3.0, ..Layout::default() };
        let t = Theme { scripture: Some(own.clone()), ..t };
        assert_eq!((t.layout(SlideKind::Scripture), t.layout(SlideKind::Lyrics)), (own, lyrics));
        assert_eq!(Layout::default(), Theme::default().layout(SlideKind::Lyrics));
    }

    #[test]
    fn sanitized_repairs_text_areas_and_the_scripture_layout() {
        let mut t = Theme {
            title_area: TextArea { size_vh: 50.0, color: "gold".into(), ..TextArea::default() },
            ..Theme::default()
        };
        t.credits_area.size_vh = f32::NAN;
        t.reference_area.color = " #ABC ".into();
        t.scripture = Some(Layout { size_min_vh: 12.0, size_max_vh: 5.0, ..Layout::default() });
        let t = t.sanitized();
        assert_eq!((t.title_area.size_vh, t.title_area.color.as_str()), (AREA_SIZE_RANGE_VH.1, ""));
        assert_eq!((t.credits_area.size_vh, t.reference_area.color.as_str()), (2.2, "#abc"));
        let s = t.scripture.unwrap();
        assert_eq!((s.size_min_vh, s.size_max_vh), (12.0, 12.0));
    }

    #[test]
    fn builtin_themes_are_valid_and_distinct() {
        let themes = builtin_themes();
        assert_eq!(themes.len(), 9);
        for t in &themes {
            assert_eq!(&t.clone().sanitized(), t, "{} is already in range", t.name);
            assert!(crate::theme::parse_color(&t.gradient_color).is_some(), "{} has a gradient", t.name);
            assert_ne!(t.name, DEFAULT_THEME_NAME);
            let light = parse_color(&t.background_color).unwrap().iter().take(3).map(|&c| u32::from(c)).sum::<u32>() > 600;
            assert_eq!(t.logo.as_deref() == Some(DARK_LOGO), light, "{}: the logo shows on its background", t.name);
        }
        assert!(themes.iter().all(|t| (1..=BUILTIN_THEMES_VERSION).contains(&builtin_theme_version(&t.name))));
        assert!(themes.iter().any(|t| builtin_theme_version(&t.name) == BUILTIN_THEMES_VERSION), "the version is bumped");
        let mut names: Vec<_> = themes.iter().map(|t| t.name.to_lowercase()).collect();
        names.dedup();
        assert_eq!(names.len(), themes.len());
    }

    #[test]
    fn stored_json_round_trips_and_old_json_gets_defaults() {
        let mut t = Theme { name: "Scripture".into(), align: Align::Left, transition: TransitionKind::Slide, ..Theme::default() };
        t.background_image = Some("/media/sky-1a2b.jpg".into());
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"align\":\"left\"") && json.contains("\"transition\":\"slide\""), "{json}");
        assert_eq!(serde_json::from_str::<Theme>(&json).unwrap(), t);
        let old: Theme = serde_json::from_str(r##"{"name":"Old","color":"#ff0"}"##).unwrap();
        assert_eq!((old.name.as_str(), old.color.as_str(), old.font_weight), ("Old", "#ff0", 700));
        // Older themes had one footer line at the bottom: every area goes there.
        assert!(AreaKind::ALL.iter().all(|&k| *old.area(k) == TextArea::default()) && old.scripture.is_none());
        let t = Theme {
            title_area: TextArea { place: Place::Top, align: Some(Align::Right), ..TextArea::default() },
            scripture: Some(Layout { valign: VAlign::Top, ..Layout::default() }),
            ..Theme::default()
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains(r#""title_area":{"place":"top","align":"right""#), "{json}");
        assert_eq!(serde_json::from_str::<Theme>(&json).unwrap(), t);
    }
}
