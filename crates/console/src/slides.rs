//! Slide cards for the preview and live panes: a label and footer over a
//! small "screen" in the theme's background colour, with the slide's text in
//! its font and colour, so a list of them reads like thumbnails of the
//! output. An approximation; the projector is the truth.

use asaf_core::{Align, Slide, Theme};
use gpui::component::{ActiveTheme as _, v_flex};
use gpui::prelude::FluentBuilder as _;
use gpui::{App, Div, ElementId, FontWeight, InteractiveElement, ParentElement, Stateful, Styled, div, px};
use gpui::component::h_flex;

use crate::preview::{background_fill, color, optional_color, styled_line};

/// A card for `slide` in `look`; `on_air` highlights the slide on screen.
/// The caller adds the click handler.
pub(crate) fn slide_card(id: impl Into<ElementId>, slide: &Slide, on_air: bool, look: &Theme, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let layout = look.layout(slide.kind);
    v_flex()
        .id(id)
        .gap_1()
        .p_1p5()
        .rounded_md()
        .border_2()
        .border_color(if on_air { theme.primary } else { theme.border })
        .bg(if on_air { theme.primary.opacity(0.15) } else { theme.secondary })
        .cursor_pointer()
        .hover(|s| s.border_color(theme.primary))
        .child(
            h_flex()
                .gap_2()
                .px_1()
                .text_xs()
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::BOLD)
                        .text_color(if on_air { theme.foreground } else { theme.muted_foreground })
                        .child(slide.label.clone()),
                )
                .when_some(slide.footer(), |d, f| {
                    d.child(div().flex_1().min_w_0().truncate().text_color(theme.muted_foreground).child(f))
                }),
        )
        .child(
            v_flex()
                .min_h(px(64.))
                .px_3()
                .py_2()
                .rounded_sm()
                .bg(background_fill(look))
                .text_color(color(&look.color))
                .text_xs()
                .font_family(look.font_family.clone())
                .font_weight(FontWeight(f32::from(look.font_weight)))
                .justify_center()
                .map(|d| match layout.align {
                    Align::Left => d.items_start(),
                    Align::Center => d.items_center(),
                    Align::Right => d.items_end(),
                })
                .children(slide.lines.iter().map(|l| {
                    let weight = FontWeight(f32::from(look.font_weight));
                    div().child(styled_line(l, layout.text_case, weight, optional_color(&look.strong_color), optional_color(&look.em_color)))
                })),
        )
}
