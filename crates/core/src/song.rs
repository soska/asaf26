use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    pub id: u64,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verse {
    pub tag: String,
    pub lines: Vec<String>,
}

/// What a screen shows. Derived from songs (and later passages), never stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Slide {
    pub lines: Vec<String>,
    pub label: String,
    pub footer: Option<String>,
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
}

impl Song {
    /// One slide per verse, following `verse_order`. Tags in the order that do
    /// not match a verse are skipped; matching is case-insensitive.
    pub fn slides(&self) -> Vec<Slide> {
        let footer = self.footer();
        let make = |verse: &Verse| Slide {
            lines: verse.lines.clone(),
            label: verse.tag.to_ascii_uppercase(),
            footer: footer.clone(),
        };

        if self.verse_order.is_empty() {
            return self.verses.iter().map(make).collect();
        }
        self.verse_order
            .iter()
            .filter_map(|tag| self.verses.iter().find(|v| v.tag.eq_ignore_ascii_case(tag)))
            .map(make)
            .collect()
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
            id: 1,
            title: "Test".into(),
            alt_title: None,
            authors: vec!["Someone".into()],
            copyright: None,
            ccli_number: None,
            verses: vec![
                Verse::new("v1", "one a\none b"),
                Verse::new("c1", "chorus"),
                Verse::new("v2", "two"),
            ],
            verse_order: order.iter().map(|s| s.to_string()).collect(),
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
    fn sample_song_has_expected_shape() {
        let s = crate::sample_song();
        let slides = s.slides();
        assert_eq!(slides.len(), s.verse_order.len());
        assert!(slides.iter().all(|s| !s.lines.is_empty()));
    }
}
