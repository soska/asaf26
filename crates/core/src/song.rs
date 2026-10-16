use std::fmt;

use serde::Serialize;

/// A line on its own inside a verse that splits it across two slides.
pub const SPLIT_MARKER: &str = "[---]";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Song {
    /// Database id; `None` until the song is first saved.
    pub id: Option<i64>,
    pub title: String,
    pub alt_title: Option<String>,
    pub authors: Vec<String>,
    pub copyright: Option<String>,
    pub ccli_number: Option<String>,
    pub verses: Vec<Verse>,
    /// Verse tags in presentation order. Empty means "verses as declared".
    pub verse_order: Vec<String>,
}

/// One block of lyrics, tagged as in OpenLyrics: kind letter + number (`v1`, `c1`, `b1`…).
/// `lines` may contain [`SPLIT_MARKER`] lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verse {
    pub tag: String,
    pub lines: Vec<String>,
}

/// What a screen shows. Derived from songs and passages, never stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Slide {
    pub lines: Vec<String>,
    pub label: String,
    pub footer: Option<String>,
    /// Prose (Bible verses): lines wrap to fill the screen. Lyric lines stay
    /// whole unless they can't fit at the minimum size.
    pub wrap: bool,
}

impl Verse {
    pub fn new(tag: &str, text: &str) -> Self {
        Self {
            tag: tag.to_ascii_lowercase(),
            lines: text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
        }
    }

    /// The verse's lines grouped by split marker, dropping empty groups.
    pub fn parts(&self) -> Vec<&[String]> {
        self.lines
            .split(|l| l == SPLIT_MARKER)
            .filter(|p| !p.is_empty())
            .collect()
    }
}

/// Something [`Song::problems`] found. `Display` is English; the console
/// shows its own translation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SongProblem {
    NoTitle,
    NoLyrics,
    /// The tag is used by more than one verse.
    DuplicateTag(String),
    /// The verse with this tag has no lines.
    EmptyVerse(String),
    /// The verse order names a tag no verse has.
    UnknownInOrder(String),
}

impl fmt::Display for SongProblem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SongProblem::NoTitle => write!(f, "The song needs a title."),
            SongProblem::NoLyrics => write!(f, "The song has no lyrics."),
            SongProblem::DuplicateTag(tag) => write!(f, "Tag [{tag}] is used more than once."),
            SongProblem::EmptyVerse(tag) => write!(f, "[{tag}] is empty."),
            SongProblem::UnknownInOrder(tag) => write!(f, "Verse order mentions \"{tag}\", which has no lyrics."),
        }
    }
}

impl Song {
    /// One slide per verse (or per split part), following `verse_order`. Tags
    /// in the order that do not match a verse are skipped; matching is
    /// case-insensitive.
    pub fn slides(&self) -> Vec<Slide> {
        let footer = self.footer();
        let ordered: Vec<&Verse> = if self.verse_order.is_empty() {
            self.verses.iter().collect()
        } else {
            self.verse_order.iter().filter_map(|tag| self.verse(tag)).collect()
        };

        let mut slides = Vec::new();
        for verse in ordered {
            let parts = verse.parts();
            let label = verse.tag.to_ascii_uppercase();
            for (i, part) in parts.iter().enumerate() {
                slides.push(Slide {
                    lines: part.to_vec(),
                    label: if parts.len() > 1 { format!("{label}.{}", i + 1) } else { label.clone() },
                    footer: footer.clone(),
                    wrap: false,
                });
            }
        }
        slides
    }

    pub fn verse(&self, tag: &str) -> Option<&Verse> {
        self.verses.iter().find(|v| v.tag.eq_ignore_ascii_case(tag))
    }

    /// Verse order as typed by the operator: `v1 c1 v2 c1`.
    pub fn verse_order_text(&self) -> String {
        self.verse_order.join(" ")
    }

    pub fn parse_verse_order(text: &str) -> Vec<String> {
        text.split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()
    }

    /// All lyric lines without tags or split markers, for search indexing.
    pub fn plain_lyrics(&self) -> String {
        self.verses
            .iter()
            .flat_map(|v| v.lines.iter())
            .filter(|l| *l != SPLIT_MARKER)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Problems worth showing the operator before saving. Empty means fine.
    pub fn problems(&self) -> Vec<SongProblem> {
        let mut problems = Vec::new();
        if self.title.trim().is_empty() {
            problems.push(SongProblem::NoTitle);
        }
        if self.verses.is_empty() {
            problems.push(SongProblem::NoLyrics);
        }
        for (i, v) in self.verses.iter().enumerate() {
            if self.verses[..i].iter().any(|w| w.tag == v.tag) {
                problems.push(SongProblem::DuplicateTag(v.tag.clone()));
            }
            if v.parts().is_empty() {
                problems.push(SongProblem::EmptyVerse(v.tag.clone()));
            }
        }
        for tag in &self.verse_order {
            if self.verse(tag).is_none() {
                problems.push(SongProblem::UnknownInOrder(tag.clone()));
            }
        }
        problems.dedup();
        problems
    }

    fn footer(&self) -> Option<String> {
        let mut parts = vec![self.title.clone()];
        if !self.authors.is_empty() {
            parts.push(self.authors.join(", "));
        }
        if let Some(c) = &self.copyright {
            parts.push(c.clone());
        }
        Some(parts.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(order: &[&str]) -> Song {
        Song {
            title: "Test".into(),
            authors: vec!["Someone".into()],
            verses: vec![
                Verse::new("v1", "one a\none b"),
                Verse::new("c1", "chorus"),
                Verse::new("v2", "two"),
            ],
            verse_order: order.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn labels(slides: &[Slide]) -> Vec<&str> {
        slides.iter().map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn follows_verse_order_and_repeats_chorus() {
        let slides = song(&["v1", "c1", "v2", "c1"]).slides();
        assert_eq!(labels(&slides), ["V1", "C1", "V2", "C1"]);
        assert_eq!(slides[0].lines, ["one a", "one b"]);
    }

    #[test]
    fn empty_order_uses_declared_order() {
        assert_eq!(labels(&song(&[]).slides()), ["V1", "C1", "V2"]);
    }

    #[test]
    fn unknown_tags_are_skipped_and_matching_ignores_case() {
        assert_eq!(labels(&song(&["V1", "x9", "C1"]).slides()), ["V1", "C1"]);
    }

    #[test]
    fn split_marker_makes_extra_slides() {
        let mut s = song(&["v1", "c1"]);
        s.verses[0] = Verse::new("v1", "a\nb\n[---]\nc\n[---]\n[---]");
        let slides = s.slides();
        assert_eq!(labels(&slides), ["V1.1", "V1.2", "C1"]);
        assert_eq!(slides[0].lines, ["a", "b"]);
        assert_eq!(slides[1].lines, ["c"]);
    }

    #[test]
    fn verse_text_is_trimmed_and_blank_lines_dropped() {
        let v = Verse::new("V3", "  a  \n\n b\n");
        assert_eq!(v.tag, "v3");
        assert_eq!(v.lines, ["a", "b"]);
    }

    #[test]
    fn footer_has_title_and_authors() {
        let slides = song(&["v1"]).slides();
        assert_eq!(slides[0].footer.as_deref(), Some("Test · Someone"));
    }

    #[test]
    fn verse_order_parsing_accepts_spaces_and_commas() {
        assert_eq!(Song::parse_verse_order(" V1, c1  v2,,C1 "), ["v1", "c1", "v2", "c1"]);
    }

    #[test]
    fn problems_are_reported() {
        let mut s = song(&["v1", "v9"]);
        s.title = " ".into();
        s.verses.push(Verse::new("v1", "dup"));
        let p = s.problems();
        assert!(p.contains(&SongProblem::NoTitle), "{p:?}");
        assert!(p.contains(&SongProblem::UnknownInOrder("v9".into())), "{p:?}");
        assert!(p.contains(&SongProblem::DuplicateTag("v1".into())), "{p:?}");
        assert_eq!(SongProblem::DuplicateTag("v1".into()).to_string(), "Tag [v1] is used more than once.");
        assert!(song(&["v1", "c1"]).problems().is_empty());
    }

    #[test]
    fn plain_lyrics_skip_markers() {
        let mut s = song(&[]);
        s.verses[0] = Verse::new("v1", "a\n[---]\nb");
        assert_eq!(s.plain_lyrics(), "a\nb\nchorus\ntwo");
    }

    #[test]
    fn sample_song_has_expected_shape() {
        let s = crate::sample_song();
        let slides = s.slides();
        assert_eq!(slides.len(), s.verse_order.len());
        assert!(s.problems().is_empty());
    }
}
