//! The console's approximation of the projected output for a theme: its
//! background colour and image, text in the theme's font, weight, colour,
//! alignment and margins, sized roughly like the display's auto-fit, with
//! the outline and shadow drawn as offset copies of the text. The projector
//! (the display page) is the truth; this only has to look close.

use std::sync::{Arc, LazyLock};

use asaf_core::{Align, Theme, VAlign, parse_color};
use gpui::{
    AnyElement, Div, FontWeight, Image, ImageFormat, ImageSource, IntoElement, ObjectFit, ParentElement, Pixels, Rgba,
    Styled, StyledImage as _, TextAlign, div, img, px, relative,
};

use crate::SharedLibrary;

static BACKGROUND_SVG: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../../display/background.svg").to_vec()))
});
static LOGO_SVG: LazyLock<Arc<Image>> =
    LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../../display/logo.svg").to_vec())));

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
        url => library.lock().unwrap().media_path(url).map(ImageSource::from),
    }
}

/// The text size the display page's auto-fit would roughly pick, in pixels,
/// for `lines` on a `width`×`height` screen.
pub(crate) fn fitted_size(theme: &Theme, lines: &[String], width: f32, height: f32) -> f32 {
    let vh = height / 100.0;
    let safe_w = width * (1.0 - 2.0 * theme.margin_x_pct / 100.0);
    let safe_h = height * (1.0 - 2.0 * theme.margin_y_pct / 100.0);
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f32;
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
    let text = |c: Rgba| -> AnyElement {
        div()
            .text_color(c)
            .font_family(theme.font_family.clone())
            .font_weight(FontWeight(f32::from(theme.font_weight)))
            .text_size(px(size))
            .line_height(relative(theme.line_height))
            .text_align(match theme.align {
                Align::Left => TextAlign::Left,
                Align::Center => TextAlign::Center,
                Align::Right => TextAlign::Right,
            })
            .children(lines.iter().map(|l| div().child(l.clone())))
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
        layers.push(area(0.0, dy, text(c)).into_any_element());
    }
    let o = theme.outline_vh * vh;
    if o > 0.0 {
        for (dx, dy) in [(-o, 0.0), (o, 0.0), (0.0, -o), (0.0, o), (-o, -o), (o, o), (-o, o), (o, -o)] {
            layers.push(area(dx, dy, text(color(&theme.outline_color))).into_any_element());
        }
    }
    layers.push(area(0.0, 0.0, text(color(&theme.color))).into_any_element());

    div()
        .relative()
        .flex_none()
        .w(width)
        .h(px(h))
        .overflow_hidden()
        .rounded_md()
        .bg(color(&theme.background_color))
        .children(background.map(|src| img(src).absolute().size_full().object_fit(ObjectFit::Cover)))
        .children(layers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_size_stays_in_the_theme_range() {
        let theme = Theme::default();
        let (w, h) = (1600.0, 900.0);
        let one_word = ["Amen.".to_string()];
        assert_eq!(fitted_size(&theme, &one_word, w, h), theme.size_max_vh * 9.0, "short slides get the maximum");
        let long = ["x".repeat(400)];
        assert_eq!(fitted_size(&theme, &long, w, h), theme.size_min_vh * 9.0, "never below the minimum");
        let four = vec!["This is my story, this is my song,".to_string(); 4];
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
