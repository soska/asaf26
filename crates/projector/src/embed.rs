//! A web page embedded in a region of a GPUI window, like the projector's
//! webview but placed by GPUI layout: the console's live monitor and the
//! theme editor's previews, both the display page.
//!
//! The webview is a native view on top of the window, so it covers any GPUI
//! element that overlaps it (popovers, notifications): hide it meanwhile.
//! The page talks back over wry's IPC: "ready" when it can take scripts (the
//! last one sent is replayed then), "pointer" when a click gave it keyboard
//! focus, which goes straight back to the window.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};

use anyhow::{Result, anyhow};
use gpui::{Bounds, Pixels, Window};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebView, WebViewBuilder};

pub struct EmbeddedPage {
    inner: Rc<Inner>,
    bounds: Cell<Option<Bounds<Pixels>>>,
    visible: Cell<bool>,
}

#[derive(Default)]
struct Inner {
    webview: OnceCell<WebView>,
    ready: Cell<bool>,
    /// The last script sent, run again when the page (re)loads.
    script: RefCell<Option<String>>,
}

impl Inner {
    fn run(&self) {
        if let (Some(webview), true, Some(script)) = (self.webview.get(), self.ready.get(), &*self.script.borrow()) {
            let _ = webview.evaluate_script(script);
        }
    }
}

impl EmbeddedPage {
    /// Loads `url` in a webview inside `window`, hidden until it's placed.
    pub fn new(url: &str, window: &Window) -> Result<Self> {
        let inner = Rc::new(Inner::default());
        let weak: Weak<Inner> = Rc::downgrade(&inner);
        let webview = WebViewBuilder::new()
            .with_url(url)
            .with_bounds(rect(Bounds::default()))
            .with_visible(false)
            .with_background_color((0, 0, 0, 255))
            .with_focused(false)
            .with_ipc_handler(move |request| {
                let Some(inner) = weak.upgrade() else { return };
                match request.body().as_str() {
                    "ready" => {
                        inner.ready.set(true);
                        inner.run();
                    }
                    "pointer" => {
                        if let Some(webview) = inner.webview.get() {
                            let _ = webview.focus_parent();
                        }
                    }
                    _ => {}
                }
            })
            .build_as_child(window)
            .map_err(|e| anyhow!("{e}"))?;
        let _ = inner.webview.set(webview);
        Ok(Self { inner, bounds: Cell::new(None), visible: Cell::new(false) })
    }

    /// Places the page over `bounds` (window coordinates, as GPUI lays out).
    pub fn set_bounds(&self, bounds: Bounds<Pixels>) {
        if self.bounds.replace(Some(bounds)) == Some(bounds) {
            return;
        }
        if let Some(webview) = self.inner.webview.get() {
            let _ = webview.set_bounds(rect(bounds));
        }
    }

    /// Where the page was last placed.
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds.get()
    }

    /// Shows or hides the page (it stays loaded).
    pub fn set_visible(&self, visible: bool) {
        if self.visible.replace(visible) == visible {
            return;
        }
        if let Some(webview) = self.inner.webview.get() {
            let _ = webview.set_visible(visible);
        }
    }

    /// Runs `script` in the page once it's ready, unless it's the same as the
    /// last one.
    pub fn send(&self, script: String) {
        if self.inner.script.borrow().as_ref() == Some(&script) {
            return;
        }
        *self.inner.script.borrow_mut() = Some(script);
        self.inner.run();
    }
}

fn rect(bounds: Bounds<Pixels>) -> Rect {
    Rect {
        position: LogicalPosition::new(bounds.origin.x.to_f64(), bounds.origin.y.to_f64()).into(),
        size: LogicalSize::new(bounds.size.width.to_f64(), bounds.size.height.to_f64()).into(),
    }
}
