//! Themes: how projected text and its background look. Stored in the library
//! as JSON, edited in the console, and sent to the display page, which applies
//! [`Theme::css_vars`] as CSS custom properties.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

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
    /// Colours are `#rgb`, `#rrggbb` or `#rrggbbaa`.
    pub color: String,
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
    /// URL on the local server: `/media/<file>` (imported) or `/assets/…`.
    pub background_image: Option<String>,
    /// Shown centred over the background when blanked to the logo.
    pub logo: Option<String>,
    pub transition: TransitionKind,
    pub transition_ms: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
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
            color: "#ffffff".into(),
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
            background_image: Some("/assets/background.svg".into()),
            logo: Some("/assets/logo.svg".into()),
            transition: TransitionKind::Fade,
            transition_ms: 300,
        }
    }
}

/// Limits the editor and [`Theme::sanitized`] enforce.
pub const SIZE_RANGE_VH: (f32, f32) = (1.0, 40.0);
pub const MAX_TRANSITION_MS: u32 = 5000;

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
        vec![
            // A cut is a transition that takes no time.
            ("--fade", format!("{}ms", if self.transition == TransitionKind::Cut { 0 } else { self.transition_ms })),
            // How far an incoming slide travels (and an outgoing one leaves).
            ("--shift", if self.transition == TransitionKind::Slide { "6vh" } else { "0vh" }.into()),
            ("--font-family", format!("{}, system-ui, sans-serif", css_string(&self.font_family))),
            ("--font-weight", self.font_weight.to_string()),
            ("--line-height", trim_float(self.line_height, 2)),
            ("--color", css_color(&self.color)),
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
            ("--logo", css_url(self.logo.as_deref())),
        ]
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
        ] {
            *value = match parse_color(value) {
                Some(_) => value.trim().to_ascii_lowercase(),
                None => default.clone(),
            };
        }
        for url in [&mut self.background_image, &mut self.logo] {
            *url = url.take().map(|u| u.trim().to_string()).filter(|u| is_local_url(u));
        }
        self
    }
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
            outline_vh: 99.0,
            shadow_blur_vh: -1.0,
            margin_x_pct: 80.0,
            transition_ms: 60_000,
            background_image: Some("https://example.com/x.jpg".into()),
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
        assert_eq!((t.outline_vh, t.shadow_blur_vh, t.margin_x_pct), (3.0, 0.0, 40.0));
        assert_eq!(t.transition_ms, MAX_TRANSITION_MS);
        assert_eq!((t.background_image, t.logo), (None, None));
        assert_eq!(Theme::default().sanitized(), Theme::default());
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
    }
}
