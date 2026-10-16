//! The committed target catalogs (`locales/<lang>.json`) are complete and
//! consistent with `locales/en.json`: what `duckalize translate check` and
//! `lint` would say, without needing Node in CI.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use asaf_i18n::Message;

/// The languages the console ships, by catalog file name.
const TARGETS: [&str; 1] = ["es"];

fn locales() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales")
}

fn read(name: &str) -> BTreeMap<String, Message> {
    let path = locales().join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The `{name}` placeholders in every form of a message.
fn placeholders(message: &Message) -> BTreeSet<String> {
    let texts: Vec<&String> = match message {
        Message::Text(text) => vec![text],
        Message::Plural(forms) => forms.values().collect(),
    };
    let mut out = BTreeSet::new();
    for text in texts {
        let mut rest = text.as_str();
        while let Some(open) = rest.find('{') {
            rest = &rest[open + 1..];
            if let Some(close) = rest.find('}') {
                out.insert(rest[..close].to_string());
                rest = &rest[close + 1..];
            }
        }
    }
    out
}

#[test]
fn every_message_is_translated() {
    let source = read("en");
    for lang in TARGETS {
        let target = read(lang);
        let missing: Vec<_> = source.iter().filter(|(id, _)| !target.contains_key(*id)).map(|(_, m)| m).collect();
        assert!(missing.is_empty(), "{lang}: no translation for {missing:#?} (see \"Localization\" in the README)");
    }
}

#[test]
fn no_translations_for_messages_that_are_gone() {
    let source = read("en");
    for lang in TARGETS {
        let orphans: Vec<_> = read(lang).into_iter().filter(|(id, _)| !source.contains_key(id)).collect();
        assert!(orphans.is_empty(), "{lang}: translations of removed messages (`duckalize translate prune`): {orphans:#?}");
    }
}

#[test]
fn translations_keep_the_shape_and_placeholders() {
    let source = read("en");
    for lang in TARGETS {
        for (id, translation) in read(lang) {
            let Some(original) = source.get(&id) else { continue };
            let blank = match &translation {
                Message::Text(text) => text.trim().is_empty(),
                Message::Plural(forms) => forms.is_empty() || forms.values().any(|t| t.trim().is_empty()),
            };
            assert!(!blank, "{lang}: blank translation of {original:?}");
            assert_eq!(
                matches!(original, Message::Plural(_)),
                matches!(translation, Message::Plural(_)),
                "{lang}: {original:?} and {translation:?} aren't both plain text or both plural"
            );
            assert_eq!(placeholders(original), placeholders(&translation), "{lang}: placeholders of {original:?} vs {translation:?}");
        }
    }
}
