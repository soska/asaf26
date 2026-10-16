//! A target-language catalog (`locales/es.json`) and lookups by English text.
//! A message that isn't in the catalog comes back in English.

use std::collections::{BTreeMap, HashMap};

use crate::id::{message_id, text_id};
use crate::plural::plural_form;
use crate::{Message, PluralForm};

/// Translations for one language, by message ID.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    locale: String,
    messages: HashMap<String, Message>,
}

impl Catalog {
    /// Reads a duckalization catalog: `{ "<id>": "text" | { "one": …, "other": … } }`.
    pub fn from_json(locale: &str, json: &str) -> serde_json::Result<Self> {
        Ok(Catalog { locale: locale.into(), messages: serde_json::from_str(json)? })
    }

    /// Builds a catalog from English → translation pairs (plain text only).
    pub fn from_pairs(locale: &str, pairs: &[(&str, &str)]) -> Self {
        let messages = pairs.iter().map(|(en, tr)| (text_id(en, None), Message::from(*tr))).collect();
        Catalog { locale: locale.into(), messages }
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// `en` (with an optional context) in this language, or `en` itself.
    pub fn text<'a>(&'a self, en: &'a str, context: Option<&str>) -> &'a str {
        match self.messages.get(&text_id(en, context)) {
            Some(Message::Text(tr)) => tr,
            _ => en,
        }
    }

    /// The form for `n` of the English plural `one`/`other`: the language's
    /// own form for `n`, else its `other`, else the English form.
    pub fn plural<'a>(&'a self, one: &'a str, other: &'a str, n: u64) -> &'a str {
        let source = Message::Plural(BTreeMap::from([(PluralForm::One, one.to_string()), (PluralForm::Other, other.to_string())]));
        if let Some(Message::Plural(forms)) = self.messages.get(&message_id(&source, None)) {
            if let Some(tr) = forms.get(&plural_form(&self.locale, n)).or_else(|| forms.get(&PluralForm::Other)) {
                return tr;
            }
        }
        english_plural(one, other, n)
    }
}

/// English's form for `n`.
pub fn english_plural<'a>(one: &'a str, other: &'a str, n: u64) -> &'a str {
    if plural_form("en", n) == PluralForm::One { one } else { other }
}

/// Replaces each `{name}` in `template` with its value. Placeholders without
/// a value are left as they are.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter().find(|(k, _)| *k == name).map(|(_, v)| (*v, close))
        });
        match value {
            Some((value, close)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ES: &str = r#"{
        "qqr9u9goft": "Canciones",
        "UNUSED": { "one": "x", "other": "y" }
    }"#;

    fn plural_catalog() -> Catalog {
        let source = Message::Plural(BTreeMap::from([
            (PluralForm::One, "{count} song".to_string()),
            (PluralForm::Other, "{count} songs".to_string()),
        ]));
        let id = message_id(&source, None);
        let json = format!(r#"{{ "{id}": {{ "one": "{{count}} canción", "many": "{{count}} de canciones", "other": "{{count}} canciones" }} }}"#);
        Catalog::from_json("es", &json).unwrap()
    }

    #[test]
    fn reads_text_and_plural_entries() {
        let c = Catalog::from_json("es", ES).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c.text("Songs", None), "Canciones");
    }

    #[test]
    fn missing_text_falls_back_to_english() {
        let c = Catalog::from_json("es", ES).unwrap();
        assert_eq!(c.text("Bible", None), "Bible");
        assert_eq!(Catalog::default().text("Songs", None), "Songs");
    }

    #[test]
    fn context_separates_identical_text() {
        let c = Catalog::from_json(
            "es",
            &format!(r#"{{ "{}": "Libro", "{}": "Reservar" }}"#, text_id("Book", None), text_id("Book", Some("verb"))),
        )
        .unwrap();
        assert_eq!(c.text("Book", None), "Libro");
        assert_eq!(c.text("Book", Some("verb")), "Reservar");
        assert_eq!(c.text("Book", Some("noun")), "Book");
    }

    #[test]
    fn pairs_are_keyed_by_the_english_text() {
        let c = Catalog::from_pairs("es", &[("Songs", "Canciones")]);
        assert_eq!(c.text("Songs", None), "Canciones");
    }

    #[test]
    fn plurals_use_the_language_rule() {
        let c = plural_catalog();
        assert_eq!(c.plural("{count} song", "{count} songs", 1), "{count} canción");
        assert_eq!(c.plural("{count} song", "{count} songs", 0), "{count} canciones");
        assert_eq!(c.plural("{count} song", "{count} songs", 2_000_000), "{count} de canciones");
    }

    #[test]
    fn plurals_fall_back_to_other_then_english() {
        let source = Message::Plural(BTreeMap::from([
            (PluralForm::One, "{count} song".to_string()),
            (PluralForm::Other, "{count} songs".to_string()),
        ]));
        let json = format!(r#"{{ "{}": {{ "other": "{{count}} canciones" }} }}"#, message_id(&source, None));
        let c = Catalog::from_json("es", &json).unwrap();
        assert_eq!(c.plural("{count} song", "{count} songs", 1), "{count} canciones");
        assert_eq!(c.plural("{count} verse", "{count} verses", 1), "{count} verse");
        assert_eq!(c.plural("{count} verse", "{count} verses", 3), "{count} verses");
    }

    #[test]
    fn fill_replaces_known_placeholders_only() {
        assert_eq!(fill("Added {title} to the service", &[("title", "Amazing Grace")]), "Added Amazing Grace to the service");
        assert_eq!(fill("{a} and {b}", &[("a", "1")]), "1 and {b}");
        assert_eq!(fill("{count} {count}", &[("count", "2")]), "2 2");
        assert_eq!(fill("open { brace", &[("x", "y")]), "open { brace");
        assert_eq!(fill("no placeholders", &[]), "no placeholders");
    }
}
