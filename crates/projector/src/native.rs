//! Platform window tweaks GPUI doesn't expose.

use gpui::Window;

/// Strips the frame and raises the window above the menu bar and dock so it
/// covers its whole display, like a presentation.
#[cfg(target_os = "macos")]
pub fn make_borderless_fullscreen(window: &Window) {
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior, NSWindowStyleMask};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else { return };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return };
    // SAFETY: GPUI hands out its live NSView; we're on the main thread.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    let Some(ns_window) = view.window() else { return };
    let Some(screen) = ns_window.screen() else { return };

    ns_window.setStyleMask(NSWindowStyleMask::Borderless);
    // kCGMainMenuWindowLevel (24) + 1: above the menu bar and the dock.
    ns_window.setLevel(25);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    ns_window.setHidesOnDeactivate(false);
    ns_window.setFrame_display(screen.frame(), true);
}

#[cfg(not(target_os = "macos"))]
pub fn make_borderless_fullscreen(_window: &Window) {
    // TODO: Windows/Linux. GPUI's titlebar-less window on the display is close enough for now.
}
