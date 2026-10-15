//! Slide cards for the preview and live panes: a label and footer over a
//! small dark "screen" with the slide's text, so a list of them reads like
//! thumbnails of the output. An approximation; the projector is the truth.

use asaf_core::Slide;
use gpui::component::{ActiveTheme as _, v_flex};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, ElementId, FontWeight, InteractiveElement, ParentElement, Stateful, Styled, Div, div, px, rgb,
};
use gpui::component::h_flex;

/// A card for `slide`; `on_air` highlights the slide on screen. The caller
/// adds the click handler.
pub(crate) fn slide_card(id: impl Into<ElementId>, slide: &Slide, on_air: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
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
                .when_some(slide.footer.clone(), |d, f| {
                    d.child(div().flex_1().min_w_0().truncate().text_color(theme.muted_foreground).child(f))
                }),
        )
        .child(
            v_flex()
                .min_h(px(64.))
                .px_3()
                .py_2()
                .rounded_sm()
                .bg(rgb(0x101018))
                .text_color(rgb(0xffffff))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .items_center()
                .justify_center()
                .children(slide.lines.iter().map(|l| div().text_center().child(l.clone()))),
        )
}
