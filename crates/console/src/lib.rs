//! The operator console: a GPUI window that drives the core store.
//! It never draws the projected output itself.

use std::sync::Arc;

use anyhow::Result;
use asaf_core::{Blank, LiveSnapshot, Store};
use asaf_projector::Projector;
use gpui::{
    App, AppContext, Bounds, Context, FocusHandle, Focusable, FontWeight, Hsla,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowHandle, WindowOptions, actions, div, px, rgb, size,
};

actions!(asaf_console, [Next, Previous, BlankBlack, BlankBackground, Unblank]);

const KEY_CONTEXT: &str = "Console";

pub fn bind_keys(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("right", Next, ctx),
        KeyBinding::new("down", Next, ctx),
        KeyBinding::new("space", Next, ctx),
        KeyBinding::new("pagedown", Next, ctx),
        KeyBinding::new("left", Previous, ctx),
        KeyBinding::new("up", Previous, ctx),
        KeyBinding::new("pageup", Previous, ctx),
        KeyBinding::new("b", BlankBlack, ctx),
        KeyBinding::new("shift-b", BlankBackground, ctx),
        KeyBinding::new("escape", Unblank, ctx),
    ]);
}

pub struct Console {
    store: Arc<Store>,
    display_url: SharedString,
    focus: FocusHandle,
    _projector_changes: Subscription,
}

impl Console {
    pub fn open(store: Arc<Store>, display_url: String, cx: &mut App) -> Result<WindowHandle<Self>> {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(980.), px(680.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Asaf".into()),
                ..Default::default()
            }),
            window_min_size: Some(size(px(640.), px(420.))),
            ..Default::default()
        };
        let handle = cx.open_window(options, |window, cx| {
            let console = cx.new(|cx| Console {
                store,
                display_url: display_url.into(),
                focus: cx.focus_handle(),
                _projector_changes: cx.observe_global::<Projector>(|_, cx| cx.notify()),
            });
            window.focus(&console.read(cx).focus);
            console
        })?;
        handle.update(cx, |_, window, _| window.activate_window())?;
        Ok(handle)
    }

    fn act(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&Store)) {
        f(&self.store);
        cx.notify();
    }
}

impl Focusable for Console {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

// ---- palette ------------------------------------------------------------------

fn bg() -> Hsla { rgb(0x16161b).into() }
fn panel() -> Hsla { rgb(0x1f1f26).into() }
fn border() -> Hsla { rgb(0x2e2e38).into() }
fn text() -> Hsla { rgb(0xe8e8ee).into() }
fn muted() -> Hsla { rgb(0x8a8a99).into() }
fn accent() -> Hsla { rgb(0x3b6cff).into() }
fn danger() -> Hsla { rgb(0xd9433b).into() }

impl Render for Console {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let live = self.store.snapshot();

        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Next, _, cx| this.act(cx, Store::next)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.act(cx, Store::previous)))
            .on_action(cx.listener(|this, _: &BlankBlack, _, cx| {
                this.act(cx, |s| s.toggle_blank(Blank::Black))
            }))
            .on_action(cx.listener(|this, _: &BlankBackground, _, cx| {
                this.act(cx, |s| s.toggle_blank(Blank::Background))
            }))
            .on_action(cx.listener(|this, _: &Unblank, _, cx| {
                this.act(cx, |s| s.set_blank(Blank::None))
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(bg())
            .text_color(text())
            .text_sm()
            .child(self.header(&live))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.slide_list(&live, cx))
                    .child(self.side_panel(&live, cx)),
            )
            .child(self.controls(&live, cx))
    }
}

impl Console {
    fn header(&self, live: &LiveSnapshot) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(border())
            .child(div().font_weight(FontWeight::BOLD).text_base().child(live.title.clone()))
            .child(div().text_color(muted()).child(format!("{} slides", live.slides.len())))
            .child(div().flex_1())
            .child(div().text_xs().text_color(muted()).child(self.display_url.clone()))
    }

    fn slide_list(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("slides")
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .children(live.slides.iter().enumerate().map(|(i, slide)| {
                let is_live = i == live.current;
                div()
                    .id(("slide", i))
                    .flex()
                    .gap_3()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(if is_live { accent() } else { border() })
                    .bg(if is_live { accent().opacity(0.18) } else { panel() })
                    .cursor_pointer()
                    .hover(|s| s.border_color(accent()))
                    .on_click(cx.listener(move |this, _, _, cx| this.act(cx, |s| s.go_to(i))))
                    .child(
                        div()
                            .flex_none()
                            .w(px(40.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if is_live { text() } else { muted() })
                            .child(slide.label.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .children(slide.lines.iter().map(|l| div().child(l.clone()))),
                    )
            }))
    }

    fn side_panel(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let status = match live.blank {
            Blank::None => "Showing text".to_string(),
            Blank::Black => "Blanked to black".to_string(),
            Blank::Background => "Text hidden (background only)".to_string(),
        };
        let label = live.slides.get(live.current).map(|s| s.label.clone()).unwrap_or_default();

        div()
            .w(px(280.))
            .flex_none()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .border_l_1()
            .border_color(border())
            .child(
                section("Live").child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::BOLD)
                                .child(format!("{label} · {} of {}", live.current + 1, live.slides.len())),
                        )
                        .child(
                            div()
                                .text_color(if live.blank == Blank::None { muted() } else { danger() })
                                .child(status),
                        ),
                ),
            )
            .child(
                section("Projector display").children(Projector::displays(cx).into_iter().map(|d| {
                    let id = d.id;
                    div()
                        .id(SharedString::from(format!("display-{}", u32::from(d.id))))
                        .flex()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .border_1()
                        .border_color(if d.is_projector { accent() } else { border() })
                        .bg(panel())
                        .cursor_pointer()
                        .hover(|s| s.border_color(accent()))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            Projector::choose(id, cx);
                            cx.notify();
                        }))
                        .child(div().w(px(14.)).child(if d.is_projector { "●" } else { "" }))
                        .child(d.label)
                })),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(muted())
                    .child("← → / Space: previous / next · B: black · Shift-B: hide text · Esc: unblank"),
            )
    }

    fn controls(&self, live: &LiveSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let at_start = live.current == 0;
        let at_end = live.current + 1 >= live.slides.len();
        div()
            .flex()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(border())
            .child(
                button("prev", "◀  Previous", false, at_start)
                    .on_click(cx.listener(|this, _, _, cx| this.act(cx, Store::previous))),
            )
            .child(
                button("next", "Next  ▶", false, at_end)
                    .on_click(cx.listener(|this, _, _, cx| this.act(cx, Store::next))),
            )
            .child(div().flex_1())
            .child(
                button("hide-text", "Hide text", live.blank == Blank::Background, false).on_click(
                    cx.listener(|this, _, _, cx| this.act(cx, |s| s.toggle_blank(Blank::Background))),
                ),
            )
            .child(
                button("blank", "Blank", live.blank == Blank::Black, false).on_click(
                    cx.listener(|this, _, _, cx| this.act(cx, |s| s.toggle_blank(Blank::Black))),
                ),
            )
    }
}

fn section(title: &'static str) -> gpui::Div {
    div().flex().flex_col().gap_2().child(
        div()
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(muted())
            .child(title.to_uppercase()),
    )
}

fn button(id: &'static str, label: &'static str, active: bool, disabled: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_4()
        .py_2()
        .rounded_md()
        .border_1()
        .font_weight(FontWeight::SEMIBOLD)
        .border_color(if active { danger() } else { border() })
        .bg(if active { danger() } else { panel() })
        .text_color(if disabled { muted() } else { text() })
        .when(!disabled, |b| b.cursor_pointer().hover(|s| s.border_color(accent())))
        .child(label)
}

use gpui::prelude::FluentBuilder as _;
