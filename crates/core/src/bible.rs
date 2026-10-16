//! Bible types, the 66-book table with English and Spanish names, and
//! verse-to-slide layout. Storage lives in `asaf-library`, parsing of
//! references in [`crate::reference`].

use std::collections::HashMap;

use crate::reference::RefRange;
use crate::{Slide, SlideKind, Theme};

/// One installed translation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bible {
    /// Database id; `None` until stored.
    pub id: Option<i64>,
    pub name: String,
    /// Short name shown next to references, e.g. "RV1909", "KJV".
    pub abbreviation: String,
    /// Language code as given by the source ("es", "spa", "en"…).
    pub language: String,
    pub copyright: Option<String>,
    /// The translation's own book names, indexed by book number - 1. Empty
    /// (or a blank entry) means "use the default name for the language".
    pub book_names: Vec<String>,
}

/// One verse of one translation. Books are numbered 1–66 in Protestant
/// canon order (Genesis = 1, Revelation = 66).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BibleVerse {
    pub book: u8,
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
}

/// What the reference parser produces: verse ranges in one translation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passage {
    pub bible_id: i64,
    pub ranges: Vec<RefRange>,
}

impl Bible {
    /// Book names for parsing and showing references in this translation.
    pub fn names(&self) -> BookNames {
        BookNames::new(&self.language, &self.book_names)
    }
}

pub struct Book {
    /// OSIS book id (`Gen`, `1Sam`, `John`…).
    pub osis: &'static str,
    pub chapters: u16,
    /// Full English name first, then common abbreviations.
    pub en: &'static [&'static str],
    /// Full Spanish name first, then common abbreviations.
    pub es: &'static [&'static str],
}

macro_rules! books {
    ($( $osis:literal $ch:literal [$($en:literal),*] [$($es:literal),*] )*) => {
        [$( Book { osis: $osis, chapters: $ch, en: &[$($en),*], es: &[$($es),*] } ),*]
    };
}

/// The 66 books, in order. Chapter counts follow the KJV / Reina-Valera
/// versification (Joel and Malachi as in English Bibles).
pub static BOOKS: [Book; 66] = books! {
    "Gen" 50 ["Genesis", "Gen", "Gn", "Ge"] ["Génesis", "Gn", "Gén", "Ge"]
    "Exod" 40 ["Exodus", "Exod", "Exo", "Ex"] ["Éxodo", "Éx", "Exo"]
    "Lev" 27 ["Leviticus", "Lev", "Lv", "Le"] ["Levítico", "Lv", "Lev"]
    "Num" 36 ["Numbers", "Num", "Nm", "Nb", "Nu"] ["Números", "Nm", "Núm", "Nú"]
    "Deut" 34 ["Deuteronomy", "Deut", "Dt", "De", "Deu"] ["Deuteronomio", "Dt", "Deut", "Deu"]
    "Josh" 24 ["Joshua", "Josh", "Jos", "Jsh"] ["Josué", "Jos"]
    "Judg" 21 ["Judges", "Judg", "Jdg", "Jg", "Jdgs"] ["Jueces", "Jue", "Jc"]
    "Ruth" 4 ["Ruth", "Rth", "Ru"] ["Rut", "Rt"]
    "1Sam" 31 ["1 Samuel", "1Sam", "1Sa", "1Sm", "1S"] ["1 Samuel", "1S", "1Sa", "1Sam", "1Sm"]
    "2Sam" 24 ["2 Samuel", "2Sam", "2Sa", "2Sm", "2S"] ["2 Samuel", "2S", "2Sa", "2Sam", "2Sm"]
    "1Kgs" 22 ["1 Kings", "1Kgs", "1Ki", "1Kin", "1Kg", "1K"] ["1 Reyes", "1R", "1Re", "1Rey", "1Ry"]
    "2Kgs" 25 ["2 Kings", "2Kgs", "2Ki", "2Kin", "2Kg", "2K"] ["2 Reyes", "2R", "2Re", "2Rey", "2Ry"]
    "1Chr" 29 ["1 Chronicles", "1Chr", "1Ch", "1Chron"] ["1 Crónicas", "1Cr", "1Cro", "1Crón"]
    "2Chr" 36 ["2 Chronicles", "2Chr", "2Ch", "2Chron"] ["2 Crónicas", "2Cr", "2Cro", "2Crón"]
    "Ezra" 10 ["Ezra", "Ezr"] ["Esdras", "Esd"]
    "Neh" 13 ["Nehemiah", "Neh", "Ne"] ["Nehemías", "Neh", "Ne"]
    "Esth" 10 ["Esther", "Esth", "Est", "Es"] ["Ester", "Est"]
    "Job" 42 ["Job", "Jb"] ["Job", "Jb"]
    "Ps" 150 ["Psalms", "Ps", "Psa", "Psalm", "Pss", "Psm"] ["Salmos", "Sal", "Salmo", "Sl", "Slm"]
    "Prov" 31 ["Proverbs", "Prov", "Pro", "Prv", "Pr"] ["Proverbios", "Pr", "Prov", "Pro"]
    "Eccl" 12 ["Ecclesiastes", "Eccl", "Ecc", "Ec", "Qoh"] ["Eclesiastés", "Ec", "Ecl", "Ecle", "Qo"]
    "Song" 8 ["Song of Solomon", "Song", "Song of Songs", "SOS", "Sg", "Canticles"] ["Cantares", "Cnt", "Cant", "Ct", "Cantar de los Cantares", "Cantar"]
    "Isa" 66 ["Isaiah", "Isa", "Is"] ["Isaías", "Is", "Isa"]
    "Jer" 52 ["Jeremiah", "Jer", "Je", "Jr"] ["Jeremías", "Jer", "Jr"]
    "Lam" 5 ["Lamentations", "Lam", "La"] ["Lamentaciones", "Lm", "Lam"]
    "Ezek" 48 ["Ezekiel", "Ezek", "Eze", "Ezk"] ["Ezequiel", "Ez", "Eze", "Ezeq"]
    "Dan" 12 ["Daniel", "Dan", "Da", "Dn"] ["Daniel", "Dn", "Dan"]
    "Hos" 14 ["Hosea", "Hos", "Ho"] ["Oseas", "Os"]
    "Joel" 3 ["Joel", "Jl", "Joe"] ["Joel", "Jl"]
    "Amos" 9 ["Amos", "Am"] ["Amós", "Am"]
    "Obad" 1 ["Obadiah", "Obad", "Ob"] ["Abdías", "Abd", "Ab"]
    "Jonah" 4 ["Jonah", "Jon", "Jnh"] ["Jonás", "Jon"]
    "Mic" 7 ["Micah", "Mic"] ["Miqueas", "Mi", "Miq"]
    "Nah" 3 ["Nahum", "Nah", "Na"] ["Nahúm", "Nah", "Na"]
    "Hab" 3 ["Habakkuk", "Hab", "Hb"] ["Habacuc", "Hab", "Ha"]
    "Zeph" 3 ["Zephaniah", "Zeph", "Zep", "Zp"] ["Sofonías", "Sof"]
    "Hag" 2 ["Haggai", "Hag", "Hg"] ["Hageo", "Hag", "Ag"]
    "Zech" 14 ["Zechariah", "Zech", "Zec", "Zc"] ["Zacarías", "Zac"]
    "Mal" 4 ["Malachi", "Mal", "Ml"] ["Malaquías", "Mal"]
    "Matt" 28 ["Matthew", "Matt", "Mat", "Mt"] ["Mateo", "Mt", "Mat", "San Mateo"]
    "Mark" 16 ["Mark", "Mrk", "Mk", "Mr"] ["Marcos", "Mr", "Mc", "Mar", "San Marcos"]
    "Luke" 24 ["Luke", "Luk", "Lk"] ["Lucas", "Lc", "Luc", "San Lucas"]
    "John" 21 ["John", "Jhn", "Jn", "Joh"] ["Juan", "Jn", "San Juan"]
    "Acts" 28 ["Acts", "Act", "Ac"] ["Hechos", "Hch", "Hech", "Hechos de los Apóstoles"]
    "Rom" 16 ["Romans", "Rom", "Ro", "Rm"] ["Romanos", "Ro", "Rom", "Rm"]
    "1Cor" 16 ["1 Corinthians", "1Cor", "1Co"] ["1 Corintios", "1Co", "1Cor"]
    "2Cor" 13 ["2 Corinthians", "2Cor", "2Co"] ["2 Corintios", "2Co", "2Cor"]
    "Gal" 6 ["Galatians", "Gal", "Ga"] ["Gálatas", "Gá", "Gál", "Gl"]
    "Eph" 6 ["Ephesians", "Eph", "Ephes"] ["Efesios", "Ef", "Efe"]
    "Phil" 4 ["Philippians", "Phil", "Php", "Pp"] ["Filipenses", "Fil", "Flp"]
    "Col" 4 ["Colossians", "Col"] ["Colosenses", "Col"]
    "1Thess" 5 ["1 Thessalonians", "1Thess", "1Thes", "1Th"] ["1 Tesalonicenses", "1Ts", "1Tes"]
    "2Thess" 3 ["2 Thessalonians", "2Thess", "2Thes", "2Th"] ["2 Tesalonicenses", "2Ts", "2Tes"]
    "1Tim" 6 ["1 Timothy", "1Tim", "1Ti", "1Tm"] ["1 Timoteo", "1Ti", "1Tim"]
    "2Tim" 4 ["2 Timothy", "2Tim", "2Ti", "2Tm"] ["2 Timoteo", "2Ti", "2Tim"]
    "Titus" 3 ["Titus", "Tit"] ["Tito", "Tit"]
    "Phlm" 1 ["Philemon", "Phlm", "Philem", "Phm"] ["Filemón", "Flm", "Filem"]
    "Heb" 13 ["Hebrews", "Heb"] ["Hebreos", "He", "Heb"]
    "Jas" 5 ["James", "Jas", "Jm"] ["Santiago", "Stg", "Sant", "Sgo"]
    "1Pet" 5 ["1 Peter", "1Pet", "1Pe", "1Pt", "1P"] ["1 Pedro", "1P", "1Pe", "1Ped"]
    "2Pet" 3 ["2 Peter", "2Pet", "2Pe", "2Pt", "2P"] ["2 Pedro", "2P", "2Pe", "2Ped"]
    "1John" 5 ["1 John", "1Jn", "1Jhn", "1Jo", "1Joh"] ["1 Juan", "1Jn"]
    "2John" 1 ["2 John", "2Jn", "2Jhn", "2Jo", "2Joh"] ["2 Juan", "2Jn"]
    "3John" 1 ["3 John", "3Jn", "3Jhn", "3Jo", "3Joh"] ["3 Juan", "3Jn"]
    "Jude" 1 ["Jude", "Jud", "Jd"] ["Judas", "Jud", "Jds"]
    "Rev" 22 ["Revelation", "Rev", "Re", "Rv", "Revelations", "Apocalypse"] ["Apocalipsis", "Ap", "Apoc"]
};

/// The book with this number (1–66).
pub fn book(number: u8) -> Option<&'static Book> {
    BOOKS.get(usize::from(number).checked_sub(1)?)
}

/// Book number for an OSIS id (`"Gen"`, `"1Sam"`), case-insensitive.
pub fn book_by_osis(osis: &str) -> Option<u8> {
    BOOKS.iter().position(|b| b.osis.eq_ignore_ascii_case(osis)).map(|i| i as u8 + 1)
}

/// `"es"`, `"spa"`, `"Español"` → `"es"`; `"eng"` → `"en"`; anything else
/// lowercased.
pub fn language_code(language: &str) -> String {
    let l = fold(language);
    match l.as_str() {
        "es" | "spa" | "esp" | "spanish" | "espanol" | "castellano" => "es".into(),
        "en" | "eng" | "english" | "ingles" => "en".into(),
        _ if l.starts_with("es-") || l.starts_with("es_") => "es".into(),
        _ if l.starts_with("en-") || l.starts_with("en_") => "en".into(),
        _ => l,
    }
}

/// Book names for one translation: what references are typed with and how
/// they are shown. Every translation accepts English and Spanish names and
/// abbreviations; where the two disagree (`Ez`, `He`…) the translation's
/// language wins, and its own book names win over both.
#[derive(Clone, Debug)]
pub struct BookNames {
    display: Vec<String>,
    lookup: HashMap<String, u8>,
}

/// Why a book name could not be resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookError {
    Unknown,
    /// Several books start with these letters; their display names.
    Ambiguous(Vec<String>),
}

impl BookNames {
    /// `language` picks the default display names and wins ties; `custom`
    /// holds the translation's own names by book (blank = default).
    pub fn new(language: &str, custom: &[String]) -> Self {
        let spanish = language_code(language) == "es";
        let primary = |b: &Book| if spanish { b.es } else { b.en };

        let mut lookup = HashMap::new();
        // The other language first, so the translation's language wins ties.
        for primary_pass in [false, true] {
            for (i, b) in BOOKS.iter().enumerate() {
                let names = if primary_pass == spanish { b.es } else { b.en };
                for name in names {
                    lookup.insert(normalize_book_name(name), i as u8 + 1);
                }
                // The OSIS id works too ("1John", "Phlm").
                lookup.entry(normalize_book_name(b.osis)).or_insert(i as u8 + 1);
            }
        }
        let mut display: Vec<String> = BOOKS.iter().map(|b| primary(b)[0].to_string()).collect();
        for (i, name) in custom.iter().enumerate().take(66) {
            let name = name.trim();
            if !name.is_empty() {
                display[i] = name.to_string();
                lookup.insert(normalize_book_name(name), i as u8 + 1);
            }
        }
        Self { display, lookup }
    }

    /// Display name of book `number` (1–66).
    pub fn name(&self, number: u8) -> &str {
        self.display.get(usize::from(number).wrapping_sub(1)).map_or("?", String::as_str)
    }

    /// Resolves a typed book name: an exact name or abbreviation, else an
    /// unambiguous prefix of one ("Gene", "Apoc", "1 Jua").
    pub fn find(&self, typed: &str) -> Result<u8, BookError> {
        let key = normalize_book_name(typed);
        if key.is_empty() {
            return Err(BookError::Unknown);
        }
        if let Some(&n) = self.lookup.get(&key) {
            return Ok(n);
        }
        let mut books: Vec<u8> = self
            .lookup
            .iter()
            .filter(|(name, _)| name.starts_with(&key))
            .map(|(_, &n)| n)
            .collect();
        books.sort_unstable();
        books.dedup();
        match books.as_slice() {
            [] => Err(BookError::Unknown),
            [n] => Ok(*n),
            many => Err(BookError::Ambiguous(many.iter().map(|&n| self.name(n).to_string()).collect())),
        }
    }
}

/// Lowercase, without accents, spaces or dots, with a leading ordinal as a
/// digit: "1ra. de Juan", "Primera Juan", "I Juan" and "1 Jn" all start
/// with "1".
pub fn normalize_book_name(name: &str) -> String {
    let folded = fold(name);
    let mut words: Vec<&str> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    // "1ra" may arrive as one word; split the digit from its suffix.
    if let Some(&first) = words.first()
        && let Some(rest) = first.strip_prefix(['1', '2', '3'])
        && ORDINAL_SUFFIXES.contains(&rest)
    {
        words.splice(0..1, [&first[..1], rest]);
    }
    if words.len() > 1 {
        let ordinal = match words[0] {
            "1" | "i" | "primera" | "primer" | "primero" | "first" => Some("1"),
            "2" | "ii" | "segunda" | "segundo" | "second" => Some("2"),
            "3" | "iii" | "tercera" | "tercer" | "tercero" | "third" => Some("3"),
            _ => None,
        };
        if let Some(digit) = ordinal {
            words[0] = digit;
            if words.len() > 2 && ORDINAL_SUFFIXES.contains(&words[1]) && words[0].len() == 1 {
                words.remove(1);
            }
            if words.len() > 2 && matches!(words[1], "de" | "of" | "del") {
                words.remove(1);
            }
        }
    }
    words.concat()
}

const ORDINAL_SUFFIXES: &[&str] = &["a", "ra", "era", "o", "er", "ro", "do", "da", "st", "nd", "rd"];

/// Lowercase and strip the accents used in Spanish (and a few more).
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'ª' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'º' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

// ---- verse-to-slide layout ------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersesPerSlide {
    /// Exactly this many verses per slide (at least 1).
    Count(u8),
    /// As many whole verses as fit at a comfortable size (estimated from the
    /// theme); a single long verse still gets a slide of its own.
    Fit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferencePlacement {
    /// Each slide's reference (a text area of the theme) names its own
    /// verses and the translation.
    Footer,
    /// The whole passage's reference is the last line of the last slide.
    LastSlide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BibleLayout {
    pub verses_per_slide: VersesPerSlide,
    pub verse_numbers: bool,
    pub reference: ReferencePlacement,
}

impl Default for BibleLayout {
    fn default() -> Self {
        Self { verses_per_slide: VersesPerSlide::Fit, verse_numbers: true, reference: ReferencePlacement::Footer }
    }
}

/// Lays out `verses` (in passage order, as fetched for `ranges`) as slides.
/// Slides never mix chapters or non-consecutive verses, so each slide's
/// label is a simple reference ("Juan 3:16-17").
pub fn passage_slides(
    verses: &[BibleVerse],
    ranges: &[RefRange],
    names: &BookNames,
    translation: &str,
    layout: &BibleLayout,
    theme: &Theme,
) -> Vec<Slide> {
    let budget = fit_chars(theme);
    let mut groups: Vec<&[BibleVerse]> = Vec::new();
    for run in verses.chunk_by(|a, b| a.book == b.book && a.chapter == b.chapter && b.verse == a.verse + 1) {
        let mut rest = run;
        while !rest.is_empty() {
            let n = match layout.verses_per_slide {
                VersesPerSlide::Count(n) => usize::from(n.max(1)),
                VersesPerSlide::Fit => {
                    let mut chars = 0;
                    rest.iter()
                        .take_while(|v| {
                            chars += v.text.chars().count() + 4;
                            chars <= budget
                        })
                        .count()
                        .max(1)
                }
            };
            let (group, tail) = rest.split_at(n.min(rest.len()));
            groups.push(group);
            rest = tail;
        }
    }

    let mut slides: Vec<Slide> = groups
        .iter()
        .map(|group| {
            let (first, last) = (&group[0], &group[group.len() - 1]);
            let label = if first.verse == last.verse {
                format!("{} {}:{}", names.name(first.book), first.chapter, first.verse)
            } else {
                format!("{} {}:{}-{}", names.name(first.book), first.chapter, first.verse, last.verse)
            };
            let lines = group
                .iter()
                .map(|v| if layout.verse_numbers { format!("{} {}", superscript(v.verse), v.text) } else { v.text.clone() })
                .collect();
            let reference = match layout.reference {
                ReferencePlacement::Footer => Some(with_translation(&label, translation)),
                ReferencePlacement::LastSlide => None,
            };
            Slide { lines, label, kind: SlideKind::Scripture, reference, ..Default::default() }
        })
        .collect();

    if layout.reference == ReferencePlacement::LastSlide
        && let Some(last) = slides.last_mut()
    {
        let reference = crate::reference::format_reference(ranges, names, |b, c| last_verse(verses, b, c));
        last.lines.push(with_translation(&reference, translation));
    }
    slides
}

/// The highest verse number of `book` `chapter` among `verses`.
pub fn last_verse(verses: &[BibleVerse], book: u8, chapter: u16) -> Option<u16> {
    verses.iter().filter(|v| v.book == book && v.chapter == chapter).map(|v| v.verse).max()
}

fn with_translation(reference: &str, translation: &str) -> String {
    if translation.is_empty() { reference.to_string() } else { format!("{reference} ({translation})") }
}

/// Characters of verse text that fit on one slide at a comfortable size:
/// the midpoint of the theme's size range for scripture on a 16:9 screen,
/// assuming an average glyph width of 0.55 em and some waste at line ends.
pub fn fit_chars(theme: &Theme) -> usize {
    let layout = theme.layout(SlideKind::Scripture);
    let size = ((layout.size_min_vh + layout.size_max_vh) / 2.0).max(1.0);
    let width_vh = (100.0 - 2.0 * theme.margin_x_pct) / 100.0 * (100.0 * 16.0 / 9.0);
    let height_vh = 100.0 - 2.0 * theme.margin_y_pct;
    let lines = (height_vh / (size * theme.line_height.max(1.0))).floor().max(1.0);
    let per_line = width_vh / (size * 0.55);
    (lines * per_line * 0.85) as usize
}

/// 16 → "¹⁶".
pub fn superscript(n: u16) -> String {
    n.to_string()
        .chars()
        .map(|d| ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'][d as usize - '0' as usize])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::parse_reference;

    #[test]
    fn book_table_is_consistent() {
        assert_eq!(BOOKS.iter().map(|b| b.chapters as u32).sum::<u32>(), 1189);
        assert_eq!(book_by_osis("1sam"), Some(9));
        assert_eq!(book(66).unwrap().osis, "Rev");
        assert!(book(0).is_none() && book(67).is_none());
        for (i, b) in BOOKS.iter().enumerate() {
            assert!(!b.en.is_empty() && !b.es.is_empty(), "{}", b.osis);
            assert_eq!(book_by_osis(b.osis), Some(i as u8 + 1));
        }
    }

    #[test]
    fn no_abbreviation_points_at_two_books_within_a_language() {
        for lang in ["en", "es"] {
            let mut seen: HashMap<String, &str> = HashMap::new();
            for b in &BOOKS {
                let names = if lang == "es" { b.es } else { b.en };
                for n in names {
                    if let Some(other) = seen.insert(normalize_book_name(n), b.osis) {
                        assert_eq!(other, b.osis, "{lang}: {n:?} is both {other} and {}", b.osis);
                    }
                }
            }
        }
    }

    #[test]
    fn normalizes_ordinals_accents_and_spacing() {
        for (input, expected) in [
            ("1 Juan", "1juan"),
            ("1Jn", "1jn"),
            ("I Juan", "1juan"),
            ("1ra. Juan", "1juan"),
            ("1ra de Juan", "1juan"),
            ("Primera de Juan", "1juan"),
            ("Primera Juan", "1juan"),
            ("II Reyes", "2reyes"),
            ("2do Reyes", "2reyes"),
            ("Segunda de Reyes", "2reyes"),
            ("3rd John", "3john"),
            ("First Corinthians", "1corinthians"),
            ("Génesis", "genesis"),
            ("ÉXODO", "exodo"),
            ("Cantar de los Cantares", "cantardeloscantares"),
            ("S. Juan", "sjuan"),
            ("Is", "is"),
        ] {
            assert_eq!(normalize_book_name(input), expected, "{input}");
        }
    }

    #[test]
    fn language_breaks_ties_and_custom_names_win() {
        let es = BookNames::new("spa", &[]);
        let en = BookNames::new("en", &[]);
        assert_eq!(es.find("Ez"), Ok(26));
        assert_eq!(es.name(43), "Juan");
        assert_eq!(en.name(43), "John");
        assert_eq!(en.find("Juan"), Ok(43));
        assert_eq!(es.find("John"), Ok(43));
        assert!(matches!(es.find("Ju"), Err(BookError::Ambiguous(_))));
        assert_eq!(es.find("xyz"), Err(BookError::Unknown));

        let mut custom = vec![String::new(); 66];
        custom[42] = "San Juan".into();
        custom[18] = "Psalmen".into();
        let names = BookNames::new("es", &custom);
        assert_eq!(names.name(43), "San Juan");
        assert_eq!(names.find("psalmen"), Ok(19));
        assert_eq!(names.name(1), "Génesis");
    }

    fn verses(book: u8, chapter: u16, range: std::ops::RangeInclusive<u16>, len: usize) -> Vec<BibleVerse> {
        range
            .map(|v| BibleVerse { book, chapter, verse: v, text: format!("{v}{}", "x".repeat(len)) })
            .collect()
    }

    #[test]
    fn n_verses_per_slide_breaks_at_chapters_and_gaps() {
        let names = BookNames::new("es", &[]);
        let mut vs = verses(1, 1, 30..=31, 10);
        vs.extend(verses(1, 2, 1..=3, 10));
        vs.extend(verses(1, 2, 7..=7, 10));
        let ranges = parse_reference("Gn 1:30-2:3, 7", &names).unwrap();
        let layout = BibleLayout { verses_per_slide: VersesPerSlide::Count(2), ..Default::default() };
        let slides = passage_slides(&vs, &ranges, &names, "RV", &layout, &Theme::default());
        let labels: Vec<&str> = slides.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Génesis 1:30-31", "Génesis 2:1-2", "Génesis 2:3", "Génesis 2:7"]);
        assert_eq!(slides[0].lines[0], format!("³⁰ 30{}", "x".repeat(10)));
        assert_eq!(slides[1].reference.as_deref(), Some("Génesis 2:1-2 (RV)"));
        assert!(slides.iter().all(|s| s.wrap()));
    }

    #[test]
    fn reference_on_last_slide_and_no_numbers() {
        let names = BookNames::new("en", &[]);
        let vs = verses(43, 3, 16..=18, 20);
        let ranges = parse_reference("Jn 3:16-18", &names).unwrap();
        let layout = BibleLayout {
            verses_per_slide: VersesPerSlide::Count(1),
            verse_numbers: false,
            reference: ReferencePlacement::LastSlide,
        };
        let slides = passage_slides(&vs, &ranges, &names, "KJV", &layout, &Theme::default());
        assert_eq!(slides.len(), 3);
        assert!(slides.iter().all(|s| s.reference.is_none()));
        assert_eq!(slides[0].lines, [format!("16{}", "x".repeat(20))]);
        assert_eq!(slides[2].lines.last().unwrap(), "John 3:16-18 (KJV)");
    }

    #[test]
    fn fit_packs_short_verses_and_isolates_long_ones() {
        let names = BookNames::new("en", &[]);
        let theme = Theme::default();
        let budget = fit_chars(&theme);
        assert!((150..600).contains(&budget), "{budget}");
        let short = verses(19, 117, 1..=2, 40);
        let slides = passage_slides(&short, &[], &names, "", &BibleLayout::default(), &theme);
        assert_eq!(slides.len(), 1, "Psalm 117 fits on one slide");

        let long = verses(17, 8, 9..=10, budget);
        let slides = passage_slides(&long, &[], &names, "", &BibleLayout::default(), &theme);
        assert_eq!(slides.len(), 2);
    }

    #[test]
    fn superscripts() {
        assert_eq!(superscript(1), "¹");
        assert_eq!(superscript(176), "¹⁷⁶");
    }
}
