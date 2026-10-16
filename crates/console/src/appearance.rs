//! The console's appearance: light, dark, or following macOS. Every view takes
//! its colors from gpui-component's theme (`cx.theme()`), so switching is a
//! matter of changing the theme's mode. The windows' title bars are macOS's
//! own, so a forced Light or Dark is also set as the app's appearance.

use asaf_core::Appearance;
use gpui::component::theme::{Theme, ThemeMode};
use gpui::{App, Global, Window, WindowAppearance};

/// The appearance setting, app-wide.
pub struct UiAppearance(pub Appearance);

impl Global for UiAppearance {}

pub fn appearance(cx: &App) -> Appearance {
    cx.try_global::<UiAppearance>().map(|a| a.0).unwrap_or_default()
}

/// Switches the console to `appearance` and redraws every window.
pub fn set_appearance(appearance: Appearance, cx: &mut App) {
    cx.set_global(UiAppearance(appearance));
    // First, so System below reads macOS's appearance, not a forced one.
    cx.set_window_appearance(native_appearance(appearance));
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

/// What the app's windows (title bars included) are told to look like;
/// `None` follows macOS.
fn native_appearance(appearance: Appearance) -> Option<WindowAppearance> {
    match appearance {
        Appearance::System => None,
        Appearance::Light => Some(WindowAppearance::Light),
        Appearance::Dark => Some(WindowAppearance::Dark),
    }
}

fn change_mode(mode: ThemeMode, cx: &mut App) {
    if Theme::global(cx).mode != mode {
        Theme::change(mode, None, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_appearances_reach_the_title_bars() {
        assert_eq!(native_appearance(Appearance::System), None, "follows macOS");
        assert_eq!(native_appearance(Appearance::Light), Some(WindowAppearance::Light));
        assert_eq!(native_appearance(Appearance::Dark), Some(WindowAppearance::Dark));
    }
}
