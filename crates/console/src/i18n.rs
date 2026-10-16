//! The console's language. Code writes the English text
//! (`lang(cx).t("Songs")`) and it's looked up in the language's catalog,
//! `locales/<lang>.json` in duckalization's format (see "Localization" in the
//! README). `asaf-i18n`'s tests check the catalogs are complete. Messages
//! built with `format!` from other parts (reference errors, import reports)
//! stay English.

use std::sync::LazyLock;

use asaf_core::Language;
use asaf_i18n::catalog::{Catalog, english_plural, fill};
use gpui::{App, Global};

/// The language the console shows, app-wide.
pub struct UiLanguage(pub Language);

impl Global for UiLanguage {}

/// Translates for one language; cheap to copy into render code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lang(pub Language);

pub fn lang(cx: &App) -> Lang {
    Lang(cx.try_global::<UiLanguage>().map(|l| l.0).unwrap_or_default())
}

/// Switches the console's language and redraws every window.
pub fn set_language(language: Language, cx: &mut App) {
    cx.set_global(UiLanguage(language));
    crate::set_menus(cx);
    cx.refresh_windows();
}

/// Marks English text that is translated later, through a variable
/// (`lang.t(label)`), so the extractor can find it.
pub const fn en(text: &'static str) -> &'static str {
    text
}

static SPANISH_CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    let json = asaf_i18n::locales::catalog_json("es-MX").expect("es-MX is shipped");
    Catalog::from_json("es-MX", json).expect("locales/es-MX.json is a valid catalog")
});

impl Lang {
    /// The catalog for this language; `None` for English, the source.
    fn catalog(self) -> Option<&'static Catalog> {
        match self.0 {
            Language::English => None,
            Language::Spanish => Some(&SPANISH_CATALOG),
        }
    }

    /// `en` in this language (itself if there's no translation).
    pub fn t(self, en: &'static str) -> &'static str {
        self.catalog().map_or(en, |c| c.text(en, None))
    }

    /// `en` in the sense given by `context` (`l.tc("Book", "verb")`), for
    /// English text that needs different translations in different places.
    #[allow(dead_code, reason = "no text needs a context yet")]
    pub fn tc(self, en: &'static str, context: &'static str) -> &'static str {
        self.catalog().map_or(en, |c| c.text(en, Some(context)))
    }

    /// `en` translated, then each `{name}` filled with its value:
    /// `l.f("Added {title} to the service", &[("title", &name)])`.
    pub fn f(self, en: &'static str, args: &[(&str, &str)]) -> String {
        fill(self.t(en), args)
    }

    /// The plural form for `count`, with `{count}` filled:
    /// `l.n(count, "{count} song", "{count} songs")`.
    pub fn n(self, count: u64, one: &'static str, other: &'static str) -> String {
        let form = self.catalog().map_or_else(|| english_plural(one, other, count), |c| c.plural(one, other, count));
        fill(form, &[("count", &count.to_string())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_identity_and_spanish_translates() {
        assert_eq!(Lang(Language::English).t("Songs"), "Songs");
        assert_eq!(Lang(Language::Spanish).t("Songs"), "Canciones");
        assert_eq!(Lang(Language::Spanish).t("not in the table"), "not in the table");
    }

    #[test]
    fn placeholders_are_filled_after_translating() {
        let title = [("title", "Sublime gracia")];
        assert_eq!(Lang(Language::English).f("Added {title} to the service", &title), "Added Sublime gracia to the service");
        // Not in the catalog: the English template, filled.
        assert_eq!(Lang(Language::Spanish).f("Added {title} to the service", &title), "Added Sublime gracia to the service");
        assert_eq!(Lang(Language::Spanish).f("Songs", &title), "Canciones");
    }

    #[test]
    fn counts_pick_the_plural_form() {
        assert_eq!(Lang(Language::English).n(1, "{count} song", "{count} songs"), "1 song");
        assert_eq!(Lang(Language::English).n(3, "{count} song", "{count} songs"), "3 songs");
        assert_eq!(Lang(Language::Spanish).n(1, "{count} song", "{count} songs"), "1 canción");
        assert_eq!(Lang(Language::Spanish).n(0, "{count} song", "{count} songs"), "0 canciones");
        assert_eq!(Lang(Language::Spanish).n(1_000_000, "{count} song", "{count} songs"), "1000000 de canciones");
        // A plural that isn't in the catalog: the English forms.
        assert_eq!(Lang(Language::Spanish).n(2, "{count} hymn", "{count} hymns"), "2 hymns");
    }

    #[test]
    fn context_without_a_translation_is_the_english_text() {
        assert_eq!(Lang(Language::English).tc("Book", "verb"), "Book");
        assert_eq!(Lang(Language::Spanish).tc("Songs", "verb"), "Songs");
    }
}
