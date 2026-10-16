//! The console's approximation of the projected output for a theme: its
//! background colour and image, text in the theme's font, weight, colour,
//! alignment and margins, sized roughly like the display's auto-fit, with
//! the outline and shadow drawn as offset copies of the text. The projector
//! (the display page) is the truth; this only has to look close.

use std::sync::{Arc, LazyLock};

use asaf_core::{Align, TextCase, Theme, VAlign, markup, parse_color};
use gpui::{
    AnyElement, Background, Div, FontStyle, FontWeight, HighlightStyle, Hsla, Image, ImageFormat, ImageSource,
    IntoElement, ObjectFit, ParentElement, Pixels, Rgba, Styled, StyledImage as _, StyledText, TextAlign, div, img,
    linear_color_stop, linear_gradient, px, relative,
};

use crate::SharedLibrary;

static BACKGROUND_SVG: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../../display/background.svg").to_vec()))
});
static LOGO_SVG: LazyLock<Arc<Image>> =
    LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../../display/logo.svg").to_vec())));
static LOGO_DARK_SVG: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../../display/logo-dark.svg").to_vec()))
});

/// A theme colour for GPUI (black if it doesn't parse).
pub(crate) fn color(text: &str) -> Rgba {
    let [r, g, b, a] = parse_color(text).unwrap_or([0, 0, 0, 255]);
    gpui::rgba(u32::from_be_bytes([r, g, b, a]))
}

/// What a theme image URL shows in the console: the built-in assets, or the
/// imported file in the media folder.
pub(crate) fn image_source(url: Option<&str>, library: &SharedLibrary) -> Option<ImageSource> {
    match url? {
        "/assets/background.svg" => Some(BACKGROUND_SVG.clone().into()),
        "/assets/logo.svg" => Some(LOGO_SVG.clone().into()),
        "/assets/logo-dark.svg" => Some(LOGO_DARK_SVG.clone().into()),
        url => library.lock().unwrap().media_path(url).map(ImageSource::from),
    }
}

/// A theme's background colour, or its gradient (as display.css draws it).
pub(crate) fn background_fill(theme: &Theme) -> Background {
    let from = color(&theme.background_color);
    match parse_color(&theme.gradient_color) {
        Some(_) => linear_gradient(135., linear_color_stop(from, 0.), linear_color_stop(color(&theme.gradient_color), 1.)),
        None => from.into(),
    }
}

/// The theme's darkening over its background, if any.
pub(crate) fn dim_overlay(theme: &Theme) -> Option<Div> {
    (theme.background_dim > 0.0).then(|| div().absolute().inset_0().bg(Rgba { r: 0., g: 0., b: 0., a: theme.background_dim }))
}

/// A theme's optional colour (empty: the text's).
pub(crate) fn optional_color(text: &str) -> Option<Hsla> {
    parse_color(text).map(|_| color(text).into())
}

/// CSS `font-weight: bolder`, which the display page gives `**strong**`.
fn bolder(weight: FontWeight) -> FontWeight {
    FontWeight(match weight.0 {
        w if w < 350.0 => 400.0,
        w if w < 550.0 => 700.0,
        w => w.max(900.0),
    })
}

/// One lyric line without its markup, its `**strong**` words bolder than
/// `weight` and `__em__` ones italic, each in its colour when given (else the
/// surrounding text's).
/// `case` is the theme's (ALL CAPS changes the text, not just the look).
pub(crate) fn styled_line(
    line: &str,
    case: TextCase,
    weight: FontWeight,
    strong: Option<Hsla>,
    em: Option<Hsla>,
) -> StyledText {
    let (text, highlights) = highlighted(line, case, weight, strong, em);
    StyledText::new(text).with_highlights(highlights)
}

fn highlighted(
    line: &str,
    case: TextCase,
    weight: FontWeight,
    strong: Option<Hsla>,
    em: Option<Hsla>,
) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for run in markup::parse(line) {
        let start = text.len();
        match case {
            TextCase::Normal => text.push_str(&run.text),
            TextCase::Upper => text.push_str(&run.text.to_uppercase()),
        }
        if run.strong || run.em {
            let style = HighlightStyle {
                font_weight: run.strong.then(|| bolder(weight)),
                font_style: run.em.then_some(FontStyle::Italic),
                color: if run.em { em } else { None }.or(if run.strong { strong } else { None }),
                ..Default::default()
            };
            highlights.push((start..text.len(), style));
        }
    }
    (text, highlights)
}

/// The text size the display page's auto-fit would roughly pick, in pixels,
/// for `lines` on a `width`×`height` screen.
pub(crate) fn fitted_size(theme: &Theme, lines: &[String], width: f32, height: f32) -> f32 {
    let vh = height / 100.0;
    let safe_w = width * (1.0 - 2.0 * theme.margin_x_pct / 100.0);
    let safe_h = height * (1.0 - 2.0 * theme.margin_y_pct / 100.0);
    let longest = lines.iter().map(|l| markup::plain(l).chars().count()).max().unwrap_or(1).max(1) as f32;
    let rows = lines.len().max(1) as f32;
    // An average glyph is about half an em wide.
    let by_width = safe_w / (longest * 0.5);
    let by_height = safe_h / (rows * theme.line_height.max(0.8));
    by_width.min(by_height).clamp(theme.size_min_vh * vh, theme.size_max_vh * vh)
}

/// `lines` shown on a `width`-wide 16:9 screen in `theme`.
pub(crate) fn screen(theme: &Theme, lines: &[String], background: Option<ImageSource>, width: Pixels) -> Div {
    let w = f32::from(width);
    let h = w * 9.0 / 16.0;
    let vh = h / 100.0;
    let size = fitted_size(theme, lines, w, h);
    let weight = FontWeight(f32::from(theme.font_weight));
    // Emphasis colours are for the fill; the outline and shadow copies are
    // all their own colour.
    let text = |c: Rgba, fill: bool| -> AnyElement {
        let (strong, em) =
            if fill { (optional_color(&theme.strong_color), optional_color(&theme.em_color)) } else { (None, None) };
        div()
            .text_color(c)
            .font_family(theme.font_family.clone())
            .font_weight(weight)
            .text_size(px(size))
            .line_height(relative(theme.line_height))
            .text_align(match theme.align {
                Align::Left => TextAlign::Left,
                Align::Center => TextAlign::Center,
                Align::Right => TextAlign::Right,
            })
            .children(lines.iter().map(|l| div().child(styled_line(l, theme.text_case, weight, strong, em))))
            .into_any_element()
    };
    // The safe area, placed like display.css places it.
    let area = |dx: f32, dy: f32, child: AnyElement| {
        let mut d = div()
            .absolute()
            .top(px(h * theme.margin_y_pct / 100.0 + dy))
            .bottom(px(h * theme.margin_y_pct / 100.0 - dy))
            .left(px(w * theme.margin_x_pct / 100.0 + dx))
            .right(px(w * theme.margin_x_pct / 100.0 - dx))
            .flex();
        d = match theme.align {
            Align::Left => d.justify_start(),
            Align::Center => d.justify_center(),
            Align::Right => d.justify_end(),
        };
        d = match theme.valign {
            VAlign::Top => d.items_start(),
            VAlign::Middle => d.items_center(),
            VAlign::Bottom => d.items_end(),
        };
        d.child(child)
    };

    let mut layers: Vec<AnyElement> = Vec::new();
    if theme.shadow_blur_vh > 0.0 || theme.shadow_offset_vh > 0.0 {
        let mut c = color(&theme.shadow_color);
        c.a *= 0.6;
        let dy = (theme.shadow_offset_vh + theme.shadow_blur_vh * 0.3) * vh;
        layers.push(area(0.0, dy, text(c, false)).into_any_element());
    }
    let o = theme.outline_vh * vh;
    if o > 0.0 {
        for (dx, dy) in [(-o, 0.0), (o, 0.0), (0.0, -o), (0.0, o), (-o, -o), (o, o), (-o, o), (o, -o)] {
            layers.push(area(dx, dy, text(color(&theme.outline_color), false)).into_any_element());
        }
    }
    layers.push(area(0.0, 0.0, text(color(&theme.color), true)).into_any_element());

    div()
        .relative()
        .flex_none()
        .w(width)
        .h(px(h))
        .overflow_hidden()
        .rounded_md()
        .bg(background_fill(theme))
        .children(background.map(|src| img(src).absolute().size_full().object_fit(ObjectFit::Cover)))
        .children(dim_overlay(theme))
        .children(layers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_line_drops_markers_and_marks_runs() {
        let gold: Hsla = color("#ffd166").into();
        let (text, marks) = highlighted("**Santo**, __santo__", TextCase::Normal, FontWeight(700.), Some(gold), None);
        assert_eq!(text, "Santo, santo");
        assert_eq!(marks.len(), 2);
        assert_eq!((marks[0].0.clone(), marks[0].1.font_weight, marks[0].1.color), (0..5, Some(FontWeight(900.)), Some(gold)));
        assert_eq!((marks[1].0.clone(), marks[1].1.font_style, marks[1].1.color), (7..12, Some(FontStyle::Italic), None));
        assert_eq!(bolder(FontWeight(700.)), FontWeight(900.));
        assert_eq!(bolder(FontWeight(300.)), FontWeight(400.));
        assert_eq!(optional_color(""), None);
        let (upper, marks) = highlighted("**straße** x", TextCase::Upper, FontWeight(400.), None, None);
        assert_eq!((upper.as_str(), marks[0].0.clone()), ("STRASSE X", 0..7));
    }

    #[test]
    fn fitted_size_stays_in_the_theme_range() {
        let theme = Theme::default();
        let (w, h) = (1600.0, 900.0);
        let one_word = ["Amen.".to_string()];
        assert_eq!(fitted_size(&theme, &one_word, w, h), theme.size_max_vh * 9.0, "short slides get the maximum");
        let long = ["x".repeat(400)];
        assert_eq!(fitted_size(&theme, &long, w, h), theme.size_min_vh * 9.0, "never below the minimum");
        let four = vec!["This is my story, this is my song,".to_string(); 4];
        let marked = vec!["**This is my story**, this is my song,".to_string(); 4];
        assert_eq!(fitted_size(&theme, &marked, w, h), fitted_size(&theme, &four, w, h), "markers take no room");
        let s = fitted_size(&theme, &four, w, h);
        assert!(s > theme.size_min_vh * 9.0 && s < theme.size_max_vh * 9.0, "{s}");
    }

    #[test]
    fn colors_convert() {
        let c = color("#ff800080");
        assert_eq!((c.r, c.g, c.b), (1.0, 128.0 / 255.0, 0.0));
        assert!((c.a - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(color("nonsense").a, 1.0);
    }
}
