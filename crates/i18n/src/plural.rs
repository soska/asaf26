//! CLDR plural rules for whole numbers (counts), for the languages Asaf
//! ships. Any other language uses English's rule.

use crate::PluralForm;

/// The plural category of `n` in `locale` (`"es"`, `"es-MX"`, `"en"`, …).
pub fn plural_form(locale: &str, n: u64) -> PluralForm {
    let language = locale.split(['-', '_']).next().unwrap_or(locale);
    match language {
        // `many` is for exact millions: "1 millón de canciones".
        "es" if n == 1 => PluralForm::One,
        "es" if n != 0 && n % 1_000_000 == 0 => PluralForm::Many,
        "es" => PluralForm::Other,
        _ if n == 1 => PluralForm::One,
        _ => PluralForm::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PluralForm::*;

    #[test]
    fn spanish_matches_cldr() {
        // From `new Intl.PluralRules('es-MX').select(n)`.
        for (n, form) in [(0, Other), (1, One), (2, Other), (1_000_000, Many), (2_000_000, Many), (1_500_000, Other)] {
            assert_eq!(plural_form("es-MX", n), form, "{n}");
            assert_eq!(plural_form("es", n), form, "{n}");
        }
    }

    #[test]
    fn english_and_others_have_one_and_other() {
        for locale in ["en", "fr"] {
            assert_eq!(plural_form(locale, 0), Other);
            assert_eq!(plural_form(locale, 1), One);
            assert_eq!(plural_form(locale, 1_000_000), Other);
        }
    }
}
