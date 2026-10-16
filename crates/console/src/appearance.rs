//! The console's appearance: light, dark, or following macOS. Every view takes
//! its colors from gpui-component's theme (`cx.theme()`), so switching is a
//! matter of changing the theme's mode.

use asaf_core::Appearance;
use gpui::component::theme::{Theme, ThemeMode};
use gpui::{App, Global, Window};

/// The appearance setting, app-wide.
pub struct UiAppearance(pub Appearance);

impl Global for UiAppearance {}

pub fn appearance(cx: &App) -> Appearance {
    cx.try_global::<UiAppearance>().map(|a| a.0).unwrap_or_default()
}

/// Switches the console to `appearance` and redraws every window.
pub fn set_appearance(appearance: Appearance, cx: &mut App) {
    cx.set_global(UiAppearance(appearance));
    let mode = match appearance {
        Appearance::System => ThemeMode::from(cx.window_appearance()),
        Appearance::Light => ThemeMode::Light,
        Appearance::Dark => ThemeMode::Dark,
    };
    change_mode(mode, cx);
}

/// Follows macOS when it switches between light and dark, if the setting is
/// System. Called from the console window's appearance observer.
pub fn system_appearance_changed(window: &mut Window, cx: &mut App) {
    if appearance(cx) == Appearance::System {
        change_mode(window.appearance().into(), cx);
    }
}

fn change_mode(mode: ThemeMode, cx: &mut App) {
    if Theme::global(cx).mode != mode {
        Theme::change(mode, None, cx);
    }
}
