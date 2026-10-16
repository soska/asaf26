//! The catalogs Asaf ships, built into the binary so an installed app reads
//! nothing from disk.

/// The locale of each UI language setting (`Settings::language` codes).
/// English is the source and has no catalog.
pub fn locale_for(language: &str) -> &'static str {
    match language {
        "es" => "es-MX",
        _ => "en",
    }
}

/// The catalog for `locale` as duckalization JSON; `None` for English and
/// locales Asaf doesn't ship.
pub fn catalog_json(locale: &str) -> Option<&'static str> {
    match locale {
        "es-MX" => Some(include_str!("../../../locales/es-MX.json")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_map_to_shipped_catalogs() {
        assert_eq!(locale_for("en"), "en");
        assert_eq!(locale_for("es"), "es-MX");
        assert!(catalog_json(locale_for("es")).is_some());
        assert!(catalog_json("en").is_none());
    }
}
