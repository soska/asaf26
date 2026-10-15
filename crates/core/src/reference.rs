//! Bible reference parser: "Jn 3:16-18", "Gn 1:30-2:3", "Sal 23; 91:1-4",
//! "1 Juan 4", "Primera de Juan 4:7, 8", "Jud 3". Book names come from a
//! [`BookNames`] table, so they work in English and Spanish, with or without
//! accents, in any case, as full names, abbreviations or unambiguous prefixes.
//!
//! Grammar, informally:
//!
//! ```text
//! reference := item ((',' | ';') item)*
//! item      := [book] span
//! span      := point ['-' [point]]     a trailing '-' or "ss" means "to the end of the chapter"
//! point     := chapter [(':' | '.') verse]
//! ```
//!
//! After a `;` (or at the start) a bare number is a chapter; after a `,`
//! that follows verses it is another verse in the same chapter ("Jn 3:16,
//! 18"). In one-chapter books (Obadiah, Philemon, 2–3 John, Jude) bare
//! numbers are verses. "a", "al", "to", "hasta" work as `-`; "y", "and" as `,`
//! (or as `;` before another book).

use std::fmt;

use crate::bible::{BookError, BookNames, book};

/// End of chapter, for whole chapters and open ranges ("Jn 3:16-").
pub const VERSE_END: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VerseRef {
    pub chapter: u16,
    /// [`VERSE_END`] means the last verse of the chapter.
    pub verse: u16,
}

/// A contiguous run of verses in one book, inclusive. A whole chapter is
/// `c:1` to `c:VERSE_END`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefRange {
    pub book: u8,
    pub start: VerseRef,
    pub end: VerseRef,
}

impl RefRange {
    pub fn is_whole_chapters(&self) -> bool {
        self.start.verse == 1 && self.end.verse == VERSE_END
    }

    /// Compact OSIS-style form, for tests and logs: `John.3.16-18`,
    /// `Gen.1.30-2.3`, `Ps.23`, `Ps.23-24`, `John.3.16-end`.
    pub fn osis(&self) -> String {
        let b = book(self.book).map_or("?", |b| b.osis);
        let (s, e) = (self.start, self.end);
        if self.is_whole_chapters() {
            return if s.chapter == e.chapter { format!("{b}.{}", s.chapter) } else { format!("{b}.{}-{}", s.chapter, e.chapter) };
        }
        let end = if e.verse == VERSE_END { "end".to_string() } else { e.verse.to_string() };
        if s.chapter == e.chapter && s.verse == e.verse {
            format!("{b}.{}.{}", s.chapter, s.verse)
        } else if s.chapter == e.chapter {
            format!("{b}.{}.{}-{end}", s.chapter, s.verse)
        } else {
            format!("{b}.{}.{}-{}.{end}", s.chapter, s.verse, e.chapter)
        }
    }
}

/// Every range's [`RefRange::osis`], joined with "; ".
pub fn osis_list(ranges: &[RefRange]) -> String {
    ranges.iter().map(RefRange::osis).collect::<Vec<_>>().join("; ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefError {
    Empty,
    /// Numbers with no book before them.
    NoBook,
    UnknownBook(String),
    AmbiguousBook(String, Vec<String>),
    /// A book with no chapter after it.
    NoChapter(String),
    ChapterOutOfRange { book: String, chapter: u16, chapters: u16 },
    VerseZero,
    Backwards(String),
    Unexpected(String),
    /// Ends in the middle of a chapter:verse ("Jn 3:").
    Incomplete,
}

impl fmt::Display for RefError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RefError::Empty => write!(f, "Type a reference, e.g. Jn 3:16"),
            RefError::NoBook => write!(f, "Start with a book name, e.g. Jn 3:16"),
            RefError::UnknownBook(b) => write!(f, "Unknown book \"{b}\""),
            RefError::AmbiguousBook(b, books) => {
                let shown: Vec<&str> = books.iter().take(4).map(String::as_str).collect();
                write!(f, "\"{b}\" could be {}{}", shown.join(", "), if books.len() > 4 { "…" } else { "" })
            }
            RefError::NoChapter(b) => write!(f, "Add a chapter: {b} 1"),
            RefError::ChapterOutOfRange { book, chapter, chapters } => {
                write!(f, "{book} has {chapters} chapter{}, not {chapter}", if *chapters == 1 { "" } else { "s" })
            }
            RefError::VerseZero => write!(f, "Chapters and verses start at 1"),
            RefError::Backwards(r) => write!(f, "\"{r}\" ends before it starts"),
            RefError::Unexpected(t) => write!(f, "Unexpected \"{t}\""),
            RefError::Incomplete => write!(f, "Incomplete reference"),
        }
    }
}

impl std::error::Error for RefError {}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Num(u16),
    Colon,
    Dash,
    Comma,
    Semi,
    /// "ss", "ff": to the end of the chapter.
    ToEnd,
}

/// Tokens with whether they touch the previous token (no space between).
fn lex(input: &str) -> Result<Vec<(Tok, bool)>, RefError> {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<(Tok, bool)> = Vec::new();
    let mut glued = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let tok = if c.is_whitespace() {
            glued = false;
            i += 1;
            continue;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let digits: String = chars[start..i].iter().collect();
            let n = digits.parse::<u16>().ok().filter(|&n| n < 1000).ok_or(RefError::Unexpected(digits))?;
            out.push((Tok::Num(n), glued));
            glued = true;
            continue;
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphabetic() {
                i += 1;
            }
            out.push((Tok::Word(chars[start..i].iter().collect()), glued));
            glued = true;
            continue;
        } else {
            match c {
                ':' => Some(Tok::Colon),
                // "3.16" is chapter 3 verse 16; any other dot ends an abbreviation.
                '.' if matches!(out.last(), Some((Tok::Num(_), _)))
                    && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) =>
                {
                    Some(Tok::Colon)
                }
                '.' => None,
                '-' | '–' | '—' | '‐' | '‑' | '−' => Some(Tok::Dash),
                ',' => Some(Tok::Comma),
                ';' => Some(Tok::Semi),
                _ => return Err(RefError::Unexpected(c.to_string())),
            }
        };
        if let Some(tok) = tok {
            out.push((tok, glued));
        }
        glued = true;
        i += 1;
    }

    // Words that act as punctuation between numbers.
    let mut toks: Vec<(Tok, bool)> = Vec::with_capacity(out.len());
    for i in 0..out.len() {
        let after_num = matches!(toks.last(), Some((Tok::Num(_), _)));
        let before_num = matches!(out.get(i + 1), Some((Tok::Num(_), _)));
        let next_is_word = matches!(out.get(i + 1), Some((Tok::Word(_), _)));
        let (tok, glued) = out[i].clone();
        let Tok::Word(w) = &tok else {
            toks.push((tok, glued));
            continue;
        };
        let w = w.to_lowercase();
        let replacement = match w.as_str() {
            "a" | "al" | "to" | "hasta" | "through" | "thru" if after_num && before_num => Some(Some(Tok::Dash)),
            "y" | "e" | "and" if after_num && before_num => Some(Some(Tok::Comma)),
            // "Juan 3:16 y Romanos 8:28": a new book follows.
            "y" | "e" | "and" if after_num && next_is_word => Some(Some(Tok::Semi)),
            "v" | "vv" | "vs" | "ver" | "vers" | "verse" | "verses" | "versiculo" | "versículo" | "versiculos"
            | "versículos"
                if after_num && before_num =>
            {
                Some(Some(Tok::Colon))
            }
            "ss" | "sig" | "sigs" | "ff" | "f" if after_num && !next_is_word => Some(Some(Tok::ToEnd)),
            // Half verses ("16a", "16b"): the whole verse is shown.
            "a" | "b" | "c" if after_num && glued && !next_is_word => Some(None),
            _ => None,
        };
        match replacement {
            Some(Some(t)) => toks.push((t, glued)),
            Some(None) => {}
            None => toks.push((tok, glued)),
        }
    }
    Ok(toks)
}

/// Parses `input` into verse ranges, validating chapter numbers against the
/// book table. Verse numbers beyond the end of a chapter are not errors
/// here; they simply match no verses when fetched.
pub fn parse_reference(input: &str, names: &BookNames) -> Result<Vec<RefRange>, RefError> {
    let toks = lex(input)?;
    if toks.is_empty() {
        return Err(RefError::Empty);
    }
    let mut p = Parser { toks: &toks, pos: 0, names };
    let mut ranges = Vec::new();
    let mut current_book: Option<(u8, String)> = None;
    let mut chapter: Option<u16> = None;
    let mut after_verses = false; // the previous item named verses
    let mut sep = Tok::Semi;

    loop {
        if let Some((n, typed)) = p.book()? {
            current_book = Some((n, typed));
            chapter = None;
            after_verses = false;
            sep = Tok::Semi;
        }
        let Some((book_no, typed)) = current_book.clone() else {
            return Err(RefError::NoBook);
        };
        let info = book(book_no).expect("valid book number");
        let name = p.names.name(book_no).to_string();
        let single = info.chapters == 1;

        let Some(a) = p.num() else {
            return Err(match p.peek() {
                None | Some(Tok::Comma | Tok::Semi) => RefError::NoChapter(typed),
                Some(t) => RefError::Unexpected(tok_text(t)),
            });
        };
        // First point: chapter[:verse]
        let (start, start_has_verse) = if p.eat(&Tok::Colon) {
            let v = p.num().ok_or_else(|| p.unexpected())?;
            (VerseRef { chapter: a, verse: v }, true)
        } else if single {
            (VerseRef { chapter: 1, verse: a }, true)
        } else if sep == Tok::Comma && after_verses {
            (VerseRef { chapter: chapter.unwrap_or(1), verse: a }, true)
        } else {
            (VerseRef { chapter: a, verse: 1 }, false)
        };

        let end = if p.eat(&Tok::ToEnd) {
            VerseRef { chapter: start.chapter, verse: VERSE_END }
        } else if p.eat(&Tok::Dash) {
            if p.eat(&Tok::ToEnd) {
                VerseRef { chapter: start.chapter, verse: VERSE_END }
            } else if let Some(b) = p.num() {
                if p.eat(&Tok::Colon) {
                    let v = p.num().ok_or_else(|| p.unexpected())?;
                    VerseRef { chapter: b, verse: v }
                } else if start_has_verse {
                    VerseRef { chapter: start.chapter, verse: b }
                } else {
                    VerseRef { chapter: b, verse: VERSE_END }
                }
            } else {
                // "Jn 3:16-": to the end of the chapter.
                VerseRef { chapter: start.chapter, verse: VERSE_END }
            }
        } else if start_has_verse {
            start
        } else {
            VerseRef { chapter: start.chapter, verse: VERSE_END }
        };
        for c in [start.chapter, end.chapter] {
            if c == 0 {
                return Err(RefError::VerseZero);
            }
            if c > info.chapters {
                return Err(RefError::ChapterOutOfRange { book: name.clone(), chapter: c, chapters: info.chapters });
            }
        }
        if start.verse == 0 || end.verse == 0 {
            return Err(RefError::VerseZero);
        }
        let range = RefRange { book: book_no, start, end };
        if end < start {
            return Err(RefError::Backwards(format_reference(&[range], p.names, |_, _| None)));
        }
        ranges.push(range);
        chapter = Some(end.chapter);
        after_verses = start_has_verse || end.verse != VERSE_END;

        match p.next() {
            None => break,
            Some(t @ (Tok::Comma | Tok::Semi)) => {
                sep = t.clone();
                // Tolerate a trailing separator while typing: "Jn 3:16,".
                if p.peek().is_none() {
                    break;
                }
            }
            Some(t) => return Err(RefError::Unexpected(tok_text(t))),
        }
    }
    Ok(ranges)
}

struct Parser<'a> {
    toks: &'a [(Tok, bool)],
    pos: usize,
    names: &'a BookNames,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn peek_at(&self, offset: usize) -> Option<&Tok> {
        self.toks.get(self.pos + offset).map(|(t, _)| t)
    }

    fn next(&mut self) -> Option<&Tok> {
        let t = self.toks.get(self.pos).map(|(t, _)| t);
        self.pos += 1;
        t
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        let yes = self.peek() == Some(tok);
        if yes {
            self.pos += 1;
        }
        yes
    }

    fn num(&mut self) -> Option<u16> {
        match self.peek() {
            Some(Tok::Num(n)) => {
                let n = *n;
                self.pos += 1;
                Some(n)
            }
            _ => None,
        }
    }

    fn unexpected(&self) -> RefError {
        self.peek().map_or(RefError::Incomplete, |t| RefError::Unexpected(tok_text(t)))
    }

    /// A book name at the current position: words, optionally after an
    /// ordinal number ("1 Juan", "2Co"). Returns the book and the name as typed.
    fn book(&mut self) -> Result<Option<(u8, String)>, RefError> {
        let mut parts: Vec<String> = Vec::new();
        let mut pos = self.pos;
        if let (Some(Tok::Num(n)), Some(Tok::Word(_))) = (self.peek(), self.peek_at(1)) {
            parts.push(n.to_string());
            pos += 1;
        }
        while let Some((Tok::Word(w), _)) = self.toks.get(pos) {
            parts.push(w.clone());
            pos += 1;
        }
        if parts.is_empty() || !parts.iter().any(|p| p.chars().any(char::is_alphabetic)) {
            return Ok(None);
        }
        let typed = parts.join(" ");
        self.pos = pos;
        match self.names.find(&typed) {
            Ok(n) => Ok(Some((n, typed))),
            Err(BookError::Unknown) => Err(RefError::UnknownBook(typed)),
            Err(BookError::Ambiguous(books)) => Err(RefError::AmbiguousBook(typed, books)),
        }
    }
}

fn tok_text(t: &Tok) -> String {
    match t {
        Tok::Word(w) => w.clone(),
        Tok::Num(n) => n.to_string(),
        Tok::Colon => ":".into(),
        Tok::Dash => "-".into(),
        Tok::Comma => ",".into(),
        Tok::Semi => ";".into(),
        Tok::ToEnd => "ss".into(),
    }
}

/// Human form with book names from `names`: "Juan 3:16-18", "Salmos 23;
/// 91:1-4", "Génesis 1:30-2:3", "Juan 3:16, 18". `last_verse(book, chapter)`
/// fills in open ends ("Jn 3:16-" → "Juan 3:16-36") when known.
pub fn format_reference(
    ranges: &[RefRange],
    names: &BookNames,
    last_verse: impl Fn(u8, u16) -> Option<u16>,
) -> String {
    let mut out = String::new();
    let mut prev: Option<&RefRange> = None;
    for r in ranges {
        let (s, e) = (r.start, r.end);
        let whole = r.is_whole_chapters();
        let end_verse = if e.verse == VERSE_END {
            last_verse(r.book, e.chapter).map_or(String::new(), |v| v.to_string())
        } else {
            e.verse.to_string()
        };
        let same_book = prev.is_some_and(|p| p.book == r.book);
        let same_chapter_verses =
            same_book && !whole && prev.is_some_and(|p| !p.is_whole_chapters() && p.end.chapter == s.chapter);

        let span = if whole {
            if s.chapter == e.chapter { s.chapter.to_string() } else { format!("{}-{}", s.chapter, e.chapter) }
        } else if s.chapter != e.chapter {
            format!("{}:{}-{}:{end_verse}", s.chapter, s.verse, e.chapter)
        } else {
            let verses = if s == e { s.verse.to_string() } else { format!("{}-{end_verse}", s.verse) };
            // One-chapter books are cited by verse alone: "Judas 3".
            let single = book(r.book).is_some_and(|b| b.chapters == 1);
            if same_chapter_verses || single { verses } else { format!("{}:{verses}", s.chapter) }
        };

        if prev.is_some() {
            out.push_str(if same_chapter_verses && s.chapter == e.chapter { ", " } else { "; " });
        }
        if !same_book {
            out.push_str(names.name(r.book));
            out.push(' ');
        }
        out.push_str(&span);
        prev = Some(r);
    }
    out
}

#[cfg(test)]
mod tests;
