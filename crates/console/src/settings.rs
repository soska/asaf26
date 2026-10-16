//! Settings window: the console's language and appearance, the default theme,
//! the server port, remote access (LAN and PIN) and migration from OpenLP.
//! Every change is saved to the library at once and announced with
//! [`SettingsChanged`].

use anyhow::Result;
use asaf_core::{Appearance, Language, Settings, valid_pin};
use asaf_projector::Projector;
use gpui::component::button::Button;
use gpui::component::checkbox::Checkbox;
use gpui::component::input::{Input, InputEvent, InputState};
use gpui::component::notification::Notification;
use gpui::component::{ActiveTheme as _, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, TitlebarOptions, Window,
    WindowBounds, WindowOptions, div, px, size,
};

use crate::appearance::set_appearance;
use crate::i18n::{UiLanguage, en, lang, set_language};
use crate::icons::IconName;
use crate::{SharedLibrary, transfer};

/// The settings were changed (and saved).
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsChanged(pub Settings);

/// Songs or Bibles were added to the library (an OpenLP import).
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryChanged;

pub struct SettingsWindow {
    library: SharedLibrary,
    pub(crate) settings: Settings,
    theme_names: Vec<String>,
    pub(crate) port: Entity<InputState>,
    port_problem: Option<&'static str>,
    pub(crate) pin: Entity<InputState>,
    pin_problem: Option<&'static str>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsChanged> for SettingsWindow {}
impl EventEmitter<LibraryChanged> for SettingsWindow {}

impl SettingsWindow {
    pub fn open(library: SharedLibrary, cx: &mut App) -> Result<(AnyWindowHandle, Entity<Self>)> {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(620.), px(640.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some(lang(cx).t("Settings").into()), ..Default::default() }),
            window_min_size: Some(size(px(520.), px(480.))),
            ..Default::default()
        };
        let (handle, view) = gpui::open_window(options, cx, |window, cx| cx.new(|cx| SettingsWindow::new(library, window, cx)))?;
        handle.update(cx, |_, window, cx| {
            window.activate_window();
            view.read(cx).focus.clone().focus(window, cx);
        })?;
        Ok((handle, view))
    }

    pub(crate) fn new(library: SharedLibrary, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (settings, theme_names) = {
            let lib = library.lock().unwrap();
            let names = lib.themes().unwrap_or_default().into_iter().map(|t| t.name).collect();
            (lib.settings().unwrap_or_default(), names)
        };
        let port_text = settings.port.to_string();
        let port = cx.new(|cx| InputState::new(window, cx).default_value(port_text));
        let pin_text = settings.remote_pin.clone();
        let pin = cx.new(|cx| InputState::new(window, cx).placeholder(lang(cx).t("No PIN")).default_value(pin_text));
        let subscriptions = vec![
            cx.subscribe_in(&pin, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } | InputEvent::Blur => this.save_pin(window, cx),
                InputEvent::Change => {
                    this.pin_problem = None;
                    cx.notify();
                }
                _ => {}
            }),
            cx.subscribe_in(&port, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } | InputEvent::Blur => this.save_port(window, cx),
                InputEvent::Change => {
                    this.port_problem = None;
                    cx.notify();
                }
                _ => {}
            }),
            // Re-word what isn't drawn each frame.
            cx.observe_global_in::<UiLanguage>(window, |this, window, cx| {
                let l = lang(cx);
                window.set_window_title(l.t("Settings"));
                this.pin.update(cx, |s, cx| s.set_placeholder(l.t("No PIN"), window, cx));
                cx.notify()
            }),
            cx.observe_global::<Projector>(|_, cx| cx.notify()),
        ];
        Self {
            library,
            settings,
            theme_names,
            port,
            port_problem: None,
            pin,
            pin_problem: None,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// Saves `change` applied to the settings, and announces it.
    fn change(&mut self, window: &mut Window, cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.library.lock().unwrap().settings().unwrap_or_else(|_| self.settings.clone());
        change(&mut settings);
        let result = self.library.lock().unwrap().save_settings(&settings);
        match result {
            Ok(()) => {
                self.settings = settings.clone();
                cx.emit(SettingsChanged(settings));
            }
            Err(e) => {
                let note = lang(cx).f("Couldn't save the settings: {error}", &[("error", &e.to_string())]);
                window.push_notification(Notification::error(note), cx)
            }
        }
        cx.notify();
    }

    pub(crate) fn set_language(&mut self, language: Language, window: &mut Window, cx: &mut Context<Self>) {
        self.change(window, cx, |s| s.language = language);
        set_language(language, cx);
    }

    pub(crate) fn set_appearance(&mut self, appearance: Appearance, window: &mut Window, cx: &mut Context<Self>) {
        self.change(window, cx, |s| s.appearance = appearance);
        set_appearance(appearance, cx);
    }

    pub(crate) fn save_port(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.port.read(cx).value().trim().to_string();
        match text.parse::<u16>() {
            Ok(port) if port >= 1024 => {
                if port != self.settings.port {
                    self.change(window, cx, |s| s.port = port);
                }
            }
            _ => {
                self.port_problem = Some(en("Use a number from 1024 to 65535."));
                cx.notify();
            }
        }
    }
}

impl SettingsWindow {
    /// Saves the remote's PIN: 4 to 8 digits, or empty for none. It applies
    /// at once (remotes using the old one must enter the new one).
    pub(crate) fn save_pin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pin = self.pin.read(cx).value().trim().to_string();
        if !valid_pin(&pin) {
            self.pin_problem = Some(en("Use 4 to 8 digits, or leave it empty for no PIN."));
            cx.notify();
        } else if pin != self.settings.remote_pin {
            self.change(window, cx, |s| s.remote_pin = pin);
        }
    }
}

impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Themes may have been added, renamed or deleted (or a new default
        // made) in the theme editor since this window opened.
        if let Ok(lib) = self.library.lock() {
            if let Ok(themes) = lib.themes() {
                self.theme_names = themes.into_iter().map(|t| t.name).collect();
            }
            if let Ok(settings) = lib.settings() {
                self.settings = settings;
            }
        }
        let l = lang(cx);
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let section = |title: &'static str| {
            v_flex()
                .gap_2()
                .pb_3()
                .border_b_1()
                .border_color(theme.border)
                .child(div().text_xs().font_weight(FontWeight::BOLD).text_color(muted).child(l.t(title).to_uppercase()))
        };
        let note = |text: &'static str| div().text_xs().text_color(muted).child(l.t(text));
        let display = Projector::displays(cx).into_iter().find(|d| d.is_projector).map(|d| d.label.to_string());

        v_flex()
            .key_context("Settings")
            .track_focus(&self.focus)
            .id("settings")
            .size_full()
            .overflow_y_scroll()
            .gap_3()
            .p_5()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .child(
                section(en("Language")).child(h_flex().gap_1().children(Language::ALL.into_iter().map(|language| {
                    Button::new(format!("lang-{}", language.code()))
                            .small()
                            .label(language.name())
                            .selected(self.settings.language == language)
                            .on_click(cx.listener(move |this, _, window, cx| this.set_language(language, window, cx)))
                }))),
            )
            .child(
                section(en("Appearance"))
                    .child(h_flex().gap_1().children(Appearance::ALL.into_iter().map(|appearance| {
                        let label = match appearance {
                            Appearance::System => en("System"),
                            Appearance::Light => en("Light"),
                            Appearance::Dark => en("Dark"),
                        };
                        let icon = match appearance {
                            Appearance::System => IconName::Monitor,
                            Appearance::Light => IconName::Sun,
                            Appearance::Dark => IconName::Moon,
                        };
                        Button::new(format!("appearance-{}", appearance.code()))
                            .icon(icon)
                            .small()
                            .label(l.t(label))
                            .selected(self.settings.appearance == appearance)
                            .on_click(cx.listener(move |this, _, window, cx| this.set_appearance(appearance, window, cx)))
                    })))
                    .child(note(en("System follows macOS and switches when it does."))),
            )
            .child(
                section(en("Default theme"))
                    .child(h_flex().flex_wrap().gap_1().children(self.theme_names.iter().enumerate().map(|(i, name)| {
                        let n = name.clone();
                        Button::new(format!("default-theme-{i}"))
                                .small()
                                .label(name.clone())
                                .selected(self.settings.default_theme.eq_ignore_ascii_case(name))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let n = n.clone();
                                    this.change(window, cx, |s| s.default_theme = n)
                                }))
                    })))
                    .child(note(en("Used for everything that has no theme of its own. Edit themes with ⌘T."))),
            )
            .child(
                section(en("Projector display"))
                    .child(div().child(display.unwrap_or_else(|| l.t("No second display").to_string())))
                    .child(note(en("Choose it in the live pane; Asaf remembers it and goes back to it when it's plugged in."))),
            )
            .child(
                section(en("Display server port"))
                    .child(h_flex().gap_2().child(div().w(px(120.)).child(Input::new(&self.port).small())).children(
                        self.port_problem.map(|p| div().text_xs().text_color(theme.danger).child(l.t(p))),
                    ))
                    .child(note(en("Takes effect the next time Asaf starts. ASAF_PORT overrides it."))),
            )
            .child(
                section(en("Remote control"))
                    .child(
                        Checkbox::new("remote-lan")
                            .label(l.t("Allow phones and tablets on this network"))
                            .checked(self.settings.lan)
                            .on_click(cx.listener(|this, checked: &bool, window, cx| {
                                let lan = *checked;
                                this.change(window, cx, |s| s.lan = lan)
                            })),
                    )
                    .child(note(en("Takes effect the next time Asaf starts; the live pane then shows the address to open on a phone. Off: only this computer can reach the display and remote pages.")))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().child(l.t("PIN")))
                            .child(div().w(px(120.)).child(Input::new(&self.pin).small()))
                            .children(self.pin_problem.map(|p| div().text_xs().text_color(theme.danger).child(l.t(p)))),
                    )
                    .child(note(en("The remote asks for it before it can control anything. Changing it applies at once.")))
                    .children((self.settings.lan && self.settings.remote_pin.is_empty()).then(|| {
                        div()
                            .text_xs()
                            .text_color(theme.danger)
                            .child(l.t("Without a PIN, anyone on the network can control the projector."))
                    })),
            )
            .child(
                section(en("Migrate from OpenLP"))
                    .child(note(en("Imports every song and Bible from an OpenLP data folder in one step. OpenLP's files aren't changed; songs and Bibles already here are skipped.")))
                    .child(h_flex().child(Button::new("openlp-import").icon(IconName::Download).small().label(l.t("Import from OpenLP…")).on_click(cx.listener(|this, _, window, cx| {
                            transfer::import_openlp(this.library.clone(), window, cx, |_, cx| cx.emit(LibraryChanged))
                        })))),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use gpui::TestAppContext;
    use gpui::test::TestWindowExt as _;

    use super::*;

    #[gpui::test]
    fn changes_are_saved_and_announced(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let mut lib = crate::test_library();
        lib.save_theme(&asaf_core::Theme { name: "Night".into(), ..Default::default() }, None).unwrap();
        let library: SharedLibrary = Arc::new(Mutex::new(lib));
        let (h, view) = cx.update(|cx| SettingsWindow::open(library.clone(), cx).unwrap());
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|cx| cx.subscribe(&view, move |_, e: &SettingsChanged, _| sink.borrow_mut().push(e.0.clone())).detach());
        let with = |cx: &mut TestAppContext, f: &dyn Fn(&mut Window, &mut App)| cx.update(|cx| h.update(cx, |_, w, cx| f(w, cx)).unwrap());

        with(cx, &|w, cx| {
            w.render_frame(cx);
            w.click("lang-es", cx);
        });
        assert_eq!(library.lock().unwrap().settings().unwrap().language, Language::Spanish);
        assert_eq!(cx.update(|cx| lang(cx).t("Songs")), "Canciones", "the whole console switches");

        with(cx, &|w, cx| {
            w.render_frame(cx);
            w.click("default-theme-1", cx);
        });
        assert_eq!(library.lock().unwrap().default_theme().unwrap().name, "Night");

        // A bad port is refused; a good one is saved on ⏎.
        let port = cx.update(|cx| view.read(cx).port.clone());
        for (text, saved) in [("80", 4510), ("99999", 4510), ("5050", 5050)] {
            with(cx, &|w, cx| {
                port.update(cx, |s, cx| s.focus(w, cx));
                w.press("cmd-a", cx);
                w.input(text, cx);
                w.press("enter", cx);
            });
            assert_eq!(library.lock().unwrap().settings().unwrap().port, saved, "{text}");
        }
        assert!(cx.update(|cx| view.read(cx).port_problem.is_none()));
        // A theme made (in the theme editor) while the window is open can be
        // chosen without reopening it.
        library.lock().unwrap().save_theme(&asaf_core::Theme { name: "Zion".into(), ..Default::default() }, None).unwrap();
        with(cx, &|w, cx| {
            w.render_frame(cx);
            w.click("default-theme-2", cx);
        });
        assert_eq!(library.lock().unwrap().default_theme().unwrap().name, "Zion");
        let events = events.borrow();
        assert_eq!(events.len(), 4);
        assert_eq!(events[2].port, 5050);
        assert_eq!((events[2].language, events[2].default_theme.as_str()), (Language::Spanish, "Night"));
    }

    #[gpui::test]
    fn appearance_applies_at_once(cx: &mut TestAppContext) {
        use gpui::component::theme::{Theme, ThemeMode};

        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let library: SharedLibrary = Arc::new(Mutex::new(crate::test_library()));
        let (h, _) = cx.update(|cx| SettingsWindow::open(library.clone(), cx).unwrap());
        let mode = |cx: &mut TestAppContext| cx.update(|cx| Theme::global(cx).mode);
        assert_eq!(library.lock().unwrap().settings().unwrap().appearance, Appearance::System);
        for (button, appearance, expected) in [
            ("appearance-dark", Appearance::Dark, ThemeMode::Dark),
            ("appearance-light", Appearance::Light, ThemeMode::Light),
        ] {
            cx.update(|cx| {
                h.update(cx, |_, w, cx| {
                    w.render_frame(cx);
                    w.click(button, cx);
                })
                .unwrap()
            });
            assert_eq!(library.lock().unwrap().settings().unwrap().appearance, appearance);
            assert_eq!(mode(cx), expected, "{button}");
        }
        // System: whatever macOS uses (the test platform's appearance).
        cx.update(|cx| {
            h.update(cx, |_, w, cx| {
                w.render_frame(cx);
                w.click("appearance-system", cx);
            })
            .unwrap()
        });
        let system = cx.update(|cx| ThemeMode::from(cx.window_appearance()));
        assert_eq!(mode(cx), system);
    }

    #[gpui::test]
    fn remote_access_lan_and_pin(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui::init(cx);
            crate::init(cx);
        });
        let library: SharedLibrary = Arc::new(Mutex::new(crate::test_library()));
        let (h, view) = cx.update(|cx| SettingsWindow::open(library.clone(), cx).unwrap());
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|cx| cx.subscribe(&view, move |_, e: &SettingsChanged, _| sink.borrow_mut().push(e.0.clone())).detach());
        let with = |cx: &mut TestAppContext, f: &dyn Fn(&mut Window, &mut App)| cx.update(|cx| h.update(cx, |_, w, cx| f(w, cx)).unwrap());
        let saved = || library.lock().unwrap().settings().unwrap();

        // LAN access is off until switched on (it applies at the next start).
        assert!(!saved().lan);
        with(cx, &|w, cx| {
            w.render_frame(cx);
            w.click("remote-lan", cx);
        });
        assert!(saved().lan);

        // PINs: 4 to 8 digits, or empty for none; bad ones aren't saved.
        let pin = cx.update(|cx| view.read(cx).pin.clone());
        for (text, stored, problem) in [("12", "", true), ("12ab", "", true), ("2468", "2468", false), ("123456789", "2468", true), ("", "", false)] {
            with(cx, &|w, cx| {
                pin.update(cx, |s, cx| s.focus(w, cx));
                w.press("cmd-a", cx);
                w.press("backspace", cx);
                w.input(text, cx);
                w.press("enter", cx);
            });
            assert_eq!(saved().remote_pin, stored, "{text:?}");
            assert_eq!(cx.update(|cx| view.read(cx).pin_problem.is_some()), problem, "{text:?}");
        }
        with(cx, &|w, cx| {
            w.render_frame(cx);
            w.click("remote-lan", cx);
        });
        assert!(!saved().lan);
        // Each change was announced (the app applies a new PIN at once).
        let events = events.borrow();
        let pins: Vec<_> = events.iter().map(|e| (e.lan, e.remote_pin.as_str())).collect();
        assert_eq!(pins, [(true, ""), (true, "2468"), (true, ""), (false, "")]);
    }
}
