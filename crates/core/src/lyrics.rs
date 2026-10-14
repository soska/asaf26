//! The plain-text lyrics format used by the editor:
//!
//! ```text
//! [v1]
//! Blessed assurance, Jesus is mine!
//! ...
//! [---]
//! (second slide of verse 1)
//!
//! [c1]
//! This is my story, this is my song,
//! ```
//!
//! Tags are short OpenLyrics names (`v1`, `c`, `b2`, `p1`, `e`) or spelled out
//! in English or Spanish (`[Verse 2]`, `[Chorus]`, `[Coro]`, `[Estrofa 3]`).

use crate::song::{SPLIT_MARKER, Verse};

/// Parses editor text into verses. Text before the first tag becomes `v1`
/// (or the next free verse number). Never fails: the editor shows
/// [`crate::Song::problems`] instead.
pub fn parse_lyrics(text: &str) -> Vec<Verse> {
    let mut verses: Vec<Verse> = Vec::new();
    let mut current: Option<Verse> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line == SPLIT_MARKER {
            let verse = current.get_or_insert_with(|| untagged_verse(&verses));
            verse.lines.push(SPLIT_MARKER.into());
        } else if let Some(tag) = parse_tag(line) {
            verses.extend(current.take());
            current = Some(Verse { tag, lines: Vec::new() });
        } else if !line.is_empty() {
            current
                .get_or_insert_with(|| untagged_verse(&verses))
                .lines
                .push(line.to_string());
        }
    }
    verses.extend(current);
    verses
}

/// Inverse of [`parse_lyrics`]: one tag line, the lines, a blank line.
pub fn format_lyrics(verses: &[Verse]) -> String {
    verses
        .iter()
        .map(|v| format!("[{}]\n{}", v.tag, v.lines.join("\n")))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn untagged_verse(existing: &[Verse]) -> Verse {
    let n = (1..).find(|n| !existing.iter().any(|v| v.tag == format!("v{n}"))).unwrap();
    Verse { tag: format!("v{n}"), lines: Vec::new() }
}

/// `[v1]`, `[C]`, `[Verse 2]`, `[Coro]` → normalized short tag.
fn parse_tag(line: &str) -> Option<String> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?.trim().to_lowercase();
    if inner.is_empty() || inner.starts_with('-') {
        return None;
    }
    // Spelled-out: "verse 2", "chorus", "pre-chorus 1", "estrofa 3".
    let (word, number) = match inner.rsplit_once(|c: char| c.is_whitespace() || c == ':') {
        Some((w, n)) if n.chars().all(|c| c.is_ascii_digit()) => (w.trim(), n),
        _ => (inner.as_str(), ""),
    };
    let kind = match word {
        "verse" | "estrofa" | "verso" => Some("v"),
        "chorus" | "coro" => Some("c"),
        "bridge" | "puente" => Some("b"),
        "pre-chorus" | "prechorus" | "pre chorus" | "precoro" | "pre-coro" => Some("p"),
        "intro" | "introducción" | "introduccion" => Some("i"),
        "ending" | "end" | "final" => Some("e"),
        "other" | "otro" => Some("o"),
        _ => None,
    };
    if let Some(kind) = kind {
        return Some(format!("{kind}{number}"));
    }
    // Short form: letters, then digits, then an optional letter (v1, c, v1a).
    let letters = inner.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let digits = inner[letters..].chars().take_while(|c| c.is_ascii_digit()).count();
    let rest = &inner[letters + digits..];
    let short = (1..=2).contains(&letters)
        && rest.len() <= 1
        && rest.chars().all(|c| c.is_ascii_alphabetic());
    short.then_some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(verses: &[Verse]) -> Vec<&str> {
        verses.iter().map(|v| v.tag.as_str()).collect()
    }

    #[test]
    fn parses_short_and_spelled_tags() {
        let v = parse_lyrics(
            "[v1]\na\n\n[Chorus]\nb\n[Verse 2]\nc\n[Coro]\nd\n[Pre-chorus 1]\ne\n[C2]\nf\n[v1a]\ng",
        );
        assert_eq!(tags(&v), ["v1", "c", "v2", "c", "p1", "c2", "v1a"]);
        assert_eq!(v[0].lines, ["a"]);
    }

    #[test]
    fn untagged_text_becomes_a_verse() {
        let v = parse_lyrics("hello\nworld\n\n[c1]\nchorus\n");
        assert_eq!(tags(&v), ["v1", "c1"]);
        assert_eq!(v[0].lines, ["hello", "world"]);
        let v = parse_lyrics("[v1]\na\n");
        assert_eq!(tags(&v), ["v1"]);
    }

    #[test]
    fn non_tags_in_brackets_are_lyrics() {
        let v = parse_lyrics("[v1]\n[Repeat twice]\n[hey hey]");
        assert_eq!(v[0].lines, ["[Repeat twice]", "[hey hey]"]);
    }

    #[test]
    fn split_markers_are_kept() {
        let v = parse_lyrics("[v1]\na\n [---] \nb");
        assert_eq!(v[0].lines, ["a", "[---]", "b"]);
    }

    #[test]
    fn format_round_trips() {
        let text = "[v1]\na\nb\n[---]\nc\n\n[c1]\nd";
        assert_eq!(format_lyrics(&parse_lyrics(text)), text);
    }
}
