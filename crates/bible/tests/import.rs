//! Fixtures are short excerpts of public-domain translations: the King James
//! Version (OSIS, milestone verses, from the open-bibles project) and the
//! Reina-Valera 1909 (Zefania), plus synthetic markup.

use std::path::Path;

use asaf_bible::{parse, read_file};
use asaf_core::BibleVerse;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn verse(verses: &[BibleVerse], book: u8, chapter: u16, verse: u16) -> &str {
    &verses.iter().find(|v| (v.book, v.chapter, v.verse) == (book, chapter, verse)).expect("verse present").text
}

#[test]
fn osis_milestone_verses() {
    let b = read_file(&fixture("kjv-excerpt.osis.xml")).unwrap();
    assert_eq!(b.bible.name, "King James Version (excerpt)");
    assert_eq!(b.bible.abbreviation, "KJV");
    assert_eq!(b.bible.language, "en");
    assert_eq!(b.bible.copyright.as_deref(), Some("Public domain"));
    assert_eq!(b.bible.book_names[0], "Genesis");
    assert_eq!(b.skipped_books, ["Tob"]);

    let v = &b.verses;
    assert_eq!(verse(v, 1, 1, 1), "In the beginning God created the heaven and the earth.");
    // <transChange> words are kept, line breaks inside the markup collapsed.
    assert_eq!(
        verse(v, 1, 1, 2),
        "And the earth was without form, and void; and darkness was upon the face of the deep. \
         And the Spirit of God moved upon the face of the waters."
    );
    // The psalm title ("A Psalm of David.") is a heading, not verse text.
    assert_eq!(verse(v, 19, 23, 1), "The LORD is my shepherd; I shall not want.");
    assert!(verse(v, 43, 3, 16).starts_with("For God so loved the world, that he gave his only begotten Son"));
    assert_eq!(v.iter().filter(|x| (x.book, x.chapter) == (19, 23)).count(), 6);
    assert_eq!(v.iter().filter(|x| (x.book, x.chapter) == (43, 3)).count(), 36);
    assert_eq!(v.iter().filter(|x| x.book == 65).count(), 25);
    // Canonical order, no duplicates.
    assert!(v.windows(2).all(|w| (w[0].book, w[0].chapter, w[0].verse) < (w[1].book, w[1].chapter, w[1].verse)));
}

#[test]
fn osis_container_verses_and_notes() {
    let xml = r#"<?xml version="1.0"?>
        <osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
          <osisText osisIDWork="KJV" xml:lang="en">
            <header><work osisWork="KJV"><title>KJV</title></work></header>
            <div type="book" osisID="John">
              <chapter osisID="John.11">
                <title type="chapter">Chapter 11</title>
                <verse osisID="John.11.35">Jesus wept.<note type="study">A synthetic note.</note></verse>
              </chapter>
              <chapter osisID="John.3">
                <verse osisID="John.3.17">For God sent not his Son into the world to condemn the world;
                  but that the world through him might be saved.</verse>
              </chapter>
            </div>
          </osisText>
        </osis>"#;
    let b = parse(xml, "fallback").unwrap();
    assert_eq!(b.bible.abbreviation, "KJV");
    assert_eq!(b.verses.len(), 2);
    assert_eq!(b.verses[0].text, "For God sent not his Son into the world to condemn the world; but that the world through him might be saved.");
    assert_eq!((b.verses[1].chapter, b.verses[1].text.as_str()), (11, "Jesus wept."));
}

#[test]
fn zefania_with_markup() {
    let b = read_file(&fixture("rv1909-excerpt.zefania.xml")).unwrap();
    assert_eq!(b.bible.name, "Reina-Valera 1909 (extracto)");
    assert_eq!(b.bible.abbreviation, "RV1909");
    assert_eq!(b.bible.language, "es");
    assert_eq!(b.bible.book_names[42], "Juan");
    assert_eq!(b.bible.book_names[18], "Salmos");
    assert_eq!(b.skipped_books, ["Apéndice"]);

    let v = &b.verses;
    // <gr> and <STYLE> keep their words, <NOTE> is dropped, <BR/> is a space.
    assert_eq!(
        verse(v, 43, 3, 16),
        "Porque de tal manera amó Dios al mundo, que ha dado á su Hijo unigénito, para que todo aquel \
         que en él cree, no se pierda, mas tenga vida eterna."
    );
    // This source keeps the psalm's title inside verse 1.
    assert_eq!(verse(v, 19, 23, 1), "Salmo de David. JEHOVÁ es mi pastor; nada me faltará.");
    assert_eq!(verse(v, 1, 2, 3).split_whitespace().next(), Some("Y"));
    assert_eq!(v.iter().filter(|x| (x.book, x.chapter) == (1, 2)).count(), 25);
}

#[test]
fn latin1_files_and_errors() {
    let dir = std::env::temp_dir().join(format!("asaf-bible-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("latin1.xml");
    let xml = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><XMLBIBLE biblename=\"Prueba\">\
               <BIBLEBOOK bnumber=\"43\"><CHAPTER cnumber=\"1\"><VERS vnumber=\"1\">En el principio era el Verbo, y el Verbo era con Dios.</VERS>\
               <VERS vnumber=\"2\">Él</VERS></CHAPTER></BIBLEBOOK></XMLBIBLE>";
    let bytes: Vec<u8> = xml.chars().map(|c| c as u32 as u8).collect();
    std::fs::write(&path, bytes).unwrap();
    let b = read_file(&path).unwrap();
    assert_eq!(b.verses[1].text, "Él");
    assert_eq!(b.bible.abbreviation, "P");

    // UTF-16 with a byte-order mark, both byte orders.
    let xml16 = xml.replace("ISO-8859-1", "UTF-16");
    for big_endian in [false, true] {
        let mut bytes: Vec<u8> = if big_endian { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
        for u in xml16.encode_utf16() {
            bytes.extend(if big_endian { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        std::fs::write(&path, bytes).unwrap();
        let b = read_file(&path).unwrap();
        assert_eq!(b.verses[1].text, "Él", "big endian: {big_endian}");
    }
    std::fs::remove_dir_all(&dir).ok();

    assert!(parse("<song/>", "x").unwrap_err().to_string().contains("not an OSIS or Zefania"));
    assert!(parse("<XMLBIBLE/>", "x").unwrap_err().to_string().contains("no verses"));
    assert!(parse("<nope", "x").unwrap_err().to_string().contains("not valid XML"));
    assert!(read_file(Path::new("/nonexistent/bible.xml")).is_err());
}
