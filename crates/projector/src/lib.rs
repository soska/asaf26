//! The projector: a borderless GPUI window on the chosen display with a wry
//! webview inside it showing the display page.
//!
//! The webview is a child of the GPUI window rather than a tao window, so the
//! projector shares GPUI's event loop and needs no second process.

mod native;

use std::time::Duration;

use anyhow::Result;
use gpui::{
    App, AppContext, BorrowAppContext, Bounds, Context, DisplayId, Global, IntoElement, ParentElement, Pixels,
    Render, SharedString, Size, Styled, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, div, px, rgb, size,
};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebView, WebViewBuilder};

/// How often to re-check the connected displays (hotplug).
const DISPLAY_POLL: Duration = Duration::from_secs(1);
const WINDOWED_SIZE: Size<Pixels> = size(px(960.), px(540.));

/// Where the projector window should be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Borderless, covering a whole display.
    Fullscreen(DisplayId),
    /// A normal window on the primary display (no second display available).
    Windowed,
}

#[derive(Clone, Debug)]
pub struct DisplayInfo {
    pub id: DisplayId,
    pub label: SharedString,
    pub is_primary: bool,
    pub is_projector: bool,
}

/// Owns the projector window and keeps it on the right display. App-global.
pub struct Projector {
    url: String,
    /// Display the operator picked, by stable UUID so it survives replugging.
    preferred: Option<String>,
    /// Allow fullscreen on the primary display (testing with one screen).
    allow_primary_fullscreen: bool,
    window: Option<(WindowHandle<ProjectorView>, Placement)>,
    /// The operator closed the projector window; don't reopen it on our own.
    closed_by_user: bool,
    displays: Vec<DisplayId>,
}

impl Global for Projector {}

impl Projector {
    /// Opens the projector and starts watching for display changes.
    pub fn init(url: String, allow_primary_fullscreen: bool, cx: &mut App) {
        cx.set_global(Projector {
            url,
            preferred: None,
            allow_primary_fullscreen,
            window: None,
            closed_by_user: false,
            displays: Vec::new(),
        });
        reconcile(cx);
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(DISPLAY_POLL).await;
                if cx.update(reconcile).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub fn displays(cx: &App) -> Vec<DisplayInfo> {
        let primary = cx.primary_display().map(|d| d.id());
        let current = cx.global::<Projector>().current_display(cx);
        cx.displays()
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let b = d.bounds();
                let is_primary = Some(d.id()) == primary;
                DisplayInfo {
                    id: d.id(),
                    label: format!(
                        "Display {} · {}×{}{}",
                        i + 1,
                        b.size.width.to_f64() as i32,
                        b.size.height.to_f64() as i32,
                        if is_primary { " (main)" } else { "" }
                    )
                    .into(),
                    is_primary,
                    is_projector: current == Some(d.id()),
                }
            })
            .collect()
    }

    /// Operator picked a display: remember it and move the projector there.
    pub fn choose(display: DisplayId, cx: &mut App) {
        let uuid = cx
            .find_display(display)
            .and_then(|d| d.uuid().ok())
            .map(|u| u.to_string());
        cx.update_global::<Projector, _>(|p, _| {
            p.preferred = uuid;
            p.closed_by_user = false;
        });
        reconcile(cx);
    }

    fn current_display(&self, cx: &App) -> Option<DisplayId> {
        match self.window?.1 {
            Placement::Fullscreen(id) => Some(id),
            Placement::Windowed => cx.primary_display().map(|d| d.id()),
        }
    }

    /// Preferred display if connected, else the first non-primary one, else windowed.
    fn wanted_placement(&self, cx: &App) -> Placement {
        let primary = cx.primary_display().map(|d| d.id());
        let displays = cx.displays();
        let preferred = self.preferred.as_ref().and_then(|uuid| {
            displays
                .iter()
                .find(|d| d.uuid().ok().map(|u| u.to_string()).as_ref() == Some(uuid))
        });
        let chosen = preferred
            .or_else(|| displays.iter().find(|d| Some(d.id()) != primary))
            .or_else(|| displays.first().filter(|_| self.allow_primary_fullscreen));
        match chosen {
            Some(d) if Some(d.id()) != primary || self.allow_primary_fullscreen => {
                Placement::Fullscreen(d.id())
            }
            _ => Placement::Windowed,
        }
    }
}

/// Make the window match what we want: open, move or reopen it.
fn reconcile(cx: &mut App) {
    let displays: Vec<DisplayId> = cx.displays().iter().map(|d| d.id()).collect();
    let (wanted, current, closed_by_user, url, displays_changed) = {
        let p = cx.global::<Projector>();
        (
            p.wanted_placement(cx),
            p.window,
            p.closed_by_user,
            p.url.clone(),
            p.displays != displays,
        )
    };

    // Did someone close the window?
    let current = current.filter(|(handle, _)| handle.update(cx, |_, _, _| ()).is_ok());
    let closed_by_user = closed_by_user || (current.is_none() && cx.global::<Projector>().window.is_some());

    let mut window = current;
    if !closed_by_user && current.map(|(_, placement)| placement) != Some(wanted) {
        if let Some((handle, _)) = current {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        window = match open_window(&url, wanted, cx) {
            Ok(handle) => Some((handle, wanted)),
            Err(err) => {
                eprintln!("asaf: could not open projector window: {err:#}");
                None
            }
        };
    }

    let changed = displays_changed || window != cx.global::<Projector>().window;
    if changed {
        // update_global notifies observers (the console's display picker).
        cx.update_global::<Projector, _>(|p, _| {
            p.window = window;
            p.closed_by_user = closed_by_user;
            p.displays = displays;
        });
    }
}

fn open_window(url: &str, placement: Placement, cx: &mut App) -> Result<WindowHandle<ProjectorView>> {
    let options = match placement {
        Placement::Fullscreen(id) => {
            let bounds = cx
                .find_display(id)
                .map(|d| d.bounds())
                .unwrap_or_else(|| Bounds::centered(None, WINDOWED_SIZE, cx));
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id: Some(id),
                titlebar: None,
                focus: false,
                kind: WindowKind::Normal,
                is_movable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            }
        }
        Placement::Windowed => WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, WINDOWED_SIZE, cx))),
            titlebar: Some(TitlebarOptions {
                title: Some("Asaf — Projector".into()),
                ..Default::default()
            }),
            focus: false,
            ..Default::default()
        },
    };

    let url = url.to_string();
    let handle = cx.open_window(options, move |window, cx| {
        let viewport = window.viewport_size();
        let webview = WebViewBuilder::new()
            .with_url(&url)
            .with_bounds(webview_rect(viewport))
            .with_background_color((0, 0, 0, 255))
            .with_focused(false)
            .with_accept_first_mouse(true)
            .build_as_child(&*window)
            .map_err(|e| SharedString::from(e.to_string()));
        if let Placement::Fullscreen(_) = placement {
            native::make_borderless_fullscreen(window);
        }
        cx.new(|_| ProjectorView { webview, viewport })
    })?;
    Ok(handle)
}

fn webview_rect(viewport: Size<Pixels>) -> Rect {
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(viewport.width.to_f64(), viewport.height.to_f64()).into(),
    }
}

pub struct ProjectorView {
    webview: std::result::Result<WebView, SharedString>,
    viewport: Size<Pixels>,
}

impl Render for ProjectorView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // The webview is a native child view; keep it covering the window.
        let viewport = window.viewport_size();
        if viewport != self.viewport {
            self.viewport = viewport;
            if let Ok(webview) = &self.webview {
                let _ = webview.set_bounds(webview_rect(viewport));
            }
        }

        let root = div().size_full().bg(rgb(0x000000));
        match &self.webview {
            Ok(_) => root,
            Err(err) => root
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0xff6666))
                .child(format!("Projector webview failed: {err}")),
        }
    }
}
