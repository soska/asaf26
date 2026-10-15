//! Bible importers: OSIS XML (<https://crosswire.org/osis/>) and Zefania XML
//! (<https://sourceforge.net/projects/zefania-sharp/>). Parsing only; the
//! library stores what these return.
//!
//! Both formats are read leniently: container and milestone verses in OSIS,
//! notes, cross-references and headings dropped, words inside markup
//! (`<w>`, `<transChange>`, `<STYLE>`, `<gr>`) kept. Books outside the
//! 66-book Protestant canon are skipped and counted.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use asaf_core::bible::{book_by_osis, language_code};
use asaf_core::{Bible, BibleVerse};
use roxmltree::{Document, Node, ParsingOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn err(msg: impl Into<String>) -> Error {
    Error(msg.into())
}

/// A parsed translation, ready to store.
#[derive(Debug, Clone)]
pub struct ImportedBible {
    pub bible: Bible,
    /// In canonical order, one per (book, chapter, verse).
    pub verses: Vec<BibleVerse>,
    /// Books in the file that aren't among the 66 (deuterocanonical books, appendices).
    pub skipped_books: Vec<String>,
}

/// Reads an OSIS or Zefania file. UTF-16 files are recognised by their
/// byte-order mark; other files that aren't UTF-8 are read as Latin-1, which
/// is what older Zefania files use.
pub fn read_file(path: &Path) -> Result<ImportedBible, Error> {
    let bytes = std::fs::read(path).map_err(|e| err(format!("can't read {}: {e}", path.display())))?;
    let xml = match bytes.as_slice() {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes)?,
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes)?,
        _ => match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
        },
    };
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    parse(&xml, &stem)
}

fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> Result<String, Error> {
    let units = bytes.as_chunks::<2>().0.iter().map(|&b| unit(b));
    char::decode_utf16(units).collect::<Result<String, _>>().map_err(|_| err("invalid UTF-16 text"))
}

/// Parses OSIS or Zefania XML, picked by the root element. `fallback_name`
/// names the translation when the file doesn't (usually the file name).
pub fn parse(xml: &str, fallback_name: &str) -> Result<ImportedBible, Error> {
    let xml = xml.trim_start_matches('\u{feff}');
    let options = ParsingOptions { allow_dtd: true, ..ParsingOptions::default() };
    let doc = Document::parse_with_options(xml, options).map_err(|e| err(format!("not valid XML: {e}")))?;
    let root = doc.root_element();
    let mut imported = match root.tag_name().name() {
        "osis" => parse_osis(root)?,
        "XMLBIBLE" => parse_zefania(root)?,
        other => return Err(err(format!("not an OSIS or Zefania Bible (root element is <{other}>)"))),
    };
    if imported.verses.is_empty() {
        return Err(err("no verses found"));
    }
    let bible = &mut imported.bible;
    if bible.name.is_empty() {
        bible.name = fallback_name.trim().to_string();
    }
    if bible.name.is_empty() {
        bible.name = "Bible".into();
    }
    if bible.abbreviation.is_empty() {
        bible.abbreviation = initials(&bible.name);
    }
    Ok(imported)
}

type Key = (u8, u16, u16);

/// Verse text collected while walking the document, merged per verse.
#[derive(Default)]
struct Collector {
    verses: BTreeMap<Key, String>,
    current: Option<Key>,
    skipped_books: Vec<String>,
}

impl Collector {
    fn text(&mut self, text: &str) {
        if let Some(key) = self.current {
            self.verses.entry(key).or_default().push_str(text);
        }
    }

    fn space(&mut self) {
        self.text(" ");
    }

    fn skip_book(&mut self, name: &str) {
        if !self.skipped_books.iter().any(|b| b == name) {
            self.skipped_books.push(name.to_string());
        }
    }

    fn finish(self) -> (Vec<BibleVerse>, Vec<String>) {
        let verses = self
            .verses
            .into_iter()
            .map(|((book, chapter, verse), text)| BibleVerse { book, chapter, verse, text: clean(&text) })
            .filter(|v| !v.text.is_empty())
            .collect();
        (verses, self.skipped_books)
    }
}

// ---- OSIS ----------------------------------------------------------------------

fn parse_osis(root: Node) -> Result<ImportedBible, Error> {
    let text = child(root, "osisText").ok_or_else(|| err("OSIS file has no <osisText>"))?;
    let work_id = text.attribute("osisIDWork").unwrap_or("");
    let header = child(text, "header");
    let work = header.and_then(|h| {
        let mut works = children(h, "work");
        works.clone().find(|w| w.attribute("osisWork") == Some(work_id)).or_else(|| works.next())
    });
    let field = |name: &str| work.and_then(|w| child(w, name)).map(|n| clean(&text_of(n))).filter(|s| !s.is_empty());

    let language = text
        .attribute(("http://www.w3.org/XML/1998/namespace", "lang"))
        .map(String::from)
        .or_else(|| field("language"))
        .unwrap_or_default();
    // "Bible.en.kjv" → "KJV".
    let abbreviation = work_id.rsplit('.').next().unwrap_or("").trim();
    let abbreviation =
        if abbreviation.chars().any(char::is_uppercase) { abbreviation.to_string() } else { abbreviation.to_uppercase() };

    let mut book_names = vec![String::new(); 66];
    let mut c = Collector::default();
    walk_osis(text, &mut c, &mut book_names);
    let (verses, skipped_books) = c.finish();
    Ok(ImportedBible {
        bible: Bible {
            id: None,
            name: field("title").unwrap_or_default(),
            abbreviation,
            language: language_code(&language),
            copyright: field("rights"),
            book_names,
        },
        verses,
        skipped_books,
    })
}

fn walk_osis(node: Node, c: &mut Collector, book_names: &mut [String]) {
    for n in node.children() {
        if n.is_text() {
            c.text(n.text().unwrap_or(""));
            continue;
        }
        if !n.is_element() {
            continue;
        }
        match n.tag_name().name() {
            "header" | "note" | "title" | "rdg" | "catchWord" | "figure" => {}
            "div" if n.attribute("type") == Some("book") => {
                let id = n.attribute("osisID").unwrap_or("");
                match book_by_osis(id) {
                    Some(book) => {
                        if let Some(short) = children(n, "title").find_map(|t| t.attribute("short")) {
                            book_names[usize::from(book) - 1] = clean(short);
                        }
                        walk_osis(n, c, book_names);
                    }
                    None => c.skip_book(id),
                }
            }
            "verse" => {
                if n.attribute("eID").is_some() {
                    c.current = None;
                } else {
                    let key = n.attribute("osisID").or(n.attribute("sID")).and_then(|id| osis_key(id, c));
                    c.current = key;
                    if n.attribute("sID").is_none() {
                        // Container form: <verse osisID="Gen.1.1">text</verse>.
                        walk_osis(n, c, book_names);
                        c.current = None;
                    }
                }
            }
            "lb" | "l" | "lg" | "p" => {
                c.space();
                walk_osis(n, c, book_names);
                c.space();
            }
            _ => walk_osis(n, c, book_names),
        }
    }
}

/// "Gen.1.1" (or "Gen.1.1 Gen.1.2", or an sID like "Gen.1.1.seID.00002") → key.
fn osis_key(id: &str, c: &mut Collector) -> Option<Key> {
    let first = id.split_whitespace().next()?;
    let mut parts = first.split('.');
    let (book, chapter, verse) = (parts.next()?, parts.next()?, parts.next()?);
    let Some(book_no) = book_by_osis(book) else {
        c.skip_book(book);
        return None;
    };
    Some((book_no, chapter.parse().ok()?, leading_number(verse)?))
}

// ---- Zefania -------------------------------------------------------------------

fn parse_zefania(root: Node) -> Result<ImportedBible, Error> {
    let info = child(root, "INFORMATION");
    let field = |name: &str| info.and_then(|i| child(i, name)).map(|n| clean(&text_of(n))).filter(|s| !s.is_empty());
    let name = root
        .attribute("biblename")
        .map(clean)
        .filter(|s| !s.is_empty())
        .or_else(|| field("title"))
        .unwrap_or_default();

    let mut book_names = vec![String::new(); 66];
    let mut c = Collector::default();
    for book in children(root, "BIBLEBOOK") {
        let bname = book.attribute("bname").map(clean).unwrap_or_default();
        let number = book.attribute("bnumber").and_then(|n| n.trim().parse::<u8>().ok());
        let Some(number) = number.filter(|n| (1..=66).contains(n)) else {
            c.skip_book(if bname.is_empty() { book.attribute("bnumber").unwrap_or("?") } else { &bname });
            continue;
        };
        if !bname.is_empty() {
            book_names[usize::from(number) - 1] = bname;
        }
        for chapter in children(book, "CHAPTER") {
            let Some(ch) = chapter.attribute("cnumber").and_then(|n| n.trim().parse::<u16>().ok()) else {
                continue;
            };
            for verse in children(chapter, "VERS") {
                let Some(v) = verse.attribute("vnumber").and_then(leading_number) else { continue };
                c.current = Some((number, ch, v));
                walk_zefania(verse, &mut c);
                c.space();
                c.current = None;
            }
        }
    }
    let (verses, skipped_books) = c.finish();
    Ok(ImportedBible {
        bible: Bible {
            id: None,
            name,
            abbreviation: field("identifier").unwrap_or_default(),
            language: language_code(&field("language").unwrap_or_default()),
            copyright: field("rights"),
            book_names,
        },
        verses,
        skipped_books,
    })
}

fn walk_zefania(node: Node, c: &mut Collector) {
    for n in node.children() {
        if n.is_text() {
            c.text(n.text().unwrap_or(""));
        } else if n.is_element() {
            match n.tag_name().name() {
                "NOTE" | "XREF" | "MEDIA" | "DIV" | "CAPTION" => {}
                "BR" => c.space(),
                _ => walk_zefania(n, c),
            }
        }
    }
}

// ---- helpers -------------------------------------------------------------------

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    children(node, name).next()
}

fn children<'a, 'n>(node: Node<'a, 'a>, name: &'n str) -> impl Iterator<Item = Node<'a, 'a>> + Clone + 'n
where
    'a: 'n,
{
    node.children().filter(move |n| n.is_element() && n.tag_name().name() == name)
}

fn text_of(node: Node) -> String {
    node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect()
}

/// Collapses runs of whitespace and drops spaces before closing punctuation.
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split_whitespace() {
        let glue = word.starts_with([',', '.', ';', ':', '!', '?', ')', '’', '”']);
        if !out.is_empty() && !glue {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// "16", "16a", "16-17" → 16.
fn leading_number(s: &str) -> Option<u16> {
    let digits: String = s.trim().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|&n| n > 0)
}

/// "Reina-Valera 1909" → "RV1909"; "American Standard Version" → "ASV".
fn initials(name: &str) -> String {
    name.split(|c: char| c.is_whitespace() || c == '-')
        .filter_map(|w| if w.chars().all(|c| c.is_ascii_digit()) { Some(w.to_string()) } else { w.chars().next().map(|c| c.to_uppercase().to_string()) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleaning() {
        assert_eq!(clean("  darkness\n was \n upon , the deep . "), "darkness was upon, the deep.");
        assert_eq!(leading_number("16a"), Some(16));
        assert_eq!(leading_number("0"), None);
        assert_eq!(initials("Reina-Valera 1909"), "RV1909");
        assert_eq!(initials("American Standard Version"), "ASV");
    }
}
