//! Message identity, matching `@duckalization/id` exactly: the same message
//! (and context) gets the same ID in Rust and in duckalization's JS tools.
//! Don't change the algorithm: every catalog ID would move.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A CLDR plural category. Declared in CLDR order, which is the order
/// plural forms are hashed and written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluralForm {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralForm {
    pub const ALL: [PluralForm; 6] =
        [PluralForm::Zero, PluralForm::One, PluralForm::Two, PluralForm::Few, PluralForm::Many, PluralForm::Other];

    pub fn as_str(self) -> &'static str {
        match self {
            PluralForm::Zero => "zero",
            PluralForm::One => "one",
            PluralForm::Two => "two",
            PluralForm::Few => "few",
            PluralForm::Many => "many",
            PluralForm::Other => "other",
        }
    }
}

/// A catalog value: plain text, or text per plural category.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Text(String),
    Plural(BTreeMap<PluralForm, String>),
}

impl From<&str> for Message {
    fn from(text: &str) -> Self {
        Message::Text(text.to_string())
    }
}

const FORM_SEP: char = '\u{1}';
const CONTEXT_SEP: char = '\u{0}';

/// The string that is hashed: the text itself, or `form\u1text` per plural
/// form in CLDR order, joined by `\u1`.
pub fn canonical(message: &Message) -> String {
    match message {
        Message::Text(text) => text.clone(),
        Message::Plural(forms) => {
            // BTreeMap iterates in `PluralForm` (CLDR) order.
            let parts: Vec<String> = forms.iter().map(|(form, text)| format!("{}{FORM_SEP}{text}", form.as_str())).collect();
            parts.join(&FORM_SEP.to_string())
        }
    }
}

/// The catalog ID of a message: cyrb53 of `context\0canonical` (or just the
/// canonical form without context), in base 36.
pub fn message_id(message: &Message, context: Option<&str>) -> String {
    let canonical = canonical(message);
    let input = match context {
        Some(c) if !c.is_empty() => format!("{c}{CONTEXT_SEP}{canonical}"),
        _ => canonical,
    };
    base36(cyrb53(&input))
}

/// ID of a plain text message.
pub fn text_id(text: &str, context: Option<&str>) -> String {
    message_id(&Message::Text(text.to_string()), context)
}

/// cyrb53 (public domain, by bryc) over UTF-16 code units, like JS's
/// `charCodeAt`. `Math.imul` is a wrapping 32-bit multiply.
fn cyrb53(text: &str) -> u64 {
    let mut h1: u32 = 0xdeadbeef;
    let mut h2: u32 = 0x41c6ce57;
    for ch in text.encode_utf16() {
        let ch = u32::from(ch);
        h1 = (h1 ^ ch).wrapping_mul(2654435761);
        h2 = (h2 ^ ch).wrapping_mul(1597334677);
    }
    h1 = (h1 ^ (h1 >> 16)).wrapping_mul(2246822507) ^ (h2 ^ (h2 >> 13)).wrapping_mul(3266489909);
    h2 = (h2 ^ (h2 >> 16)).wrapping_mul(2246822507) ^ (h1 ^ (h1 >> 13)).wrapping_mul(3266489909);
    4294967296 * u64::from(2097151 & h2) + u64::from(h1)
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("ascii digits")
}
