//! OpenLyrics XML import and export (<https://docs.openlyrics.org>).
//!
//! Reads 0.8 (`<lines><line>…</line></lines>`) and 0.9 (`<lines>a<br/>b</lines>`)
//! files; writes 0.9. Several `<lines>` in one verse become slide splits.
//! Chords, comments and instruments are dropped. Formatting tags
//! (`<tag name="…">`) that mean bold or italic, by the HTML the song's
//! `<format>` gives them or by their name, become `**strong**` and `__em__`;
//! export writes those back as `strong` and `em` tags.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use asaf_core::{SPLIT_MARKER, Song, Verse, markup};
use roxmltree::{Document, Node};

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

// ---- import --------------------------------------------------------------------

pub fn parse(xml: &str) -> Result<Song, Error> {
    let doc = Document::parse(xml).map_err(|e| err(format!("not valid XML: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "song" {
        return Err(err("not an OpenLyrics file (root element isn't <song>)"));
    }

    let properties = child(root, "properties").ok_or_else(|| err("missing <properties>"))?;
    let titles: Vec<String> = child(properties, "titles")
        .map(|t| children(t, "title").map(|n| clean(&text_of(n))).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let mut authors: Vec<String> = Vec::new();
    if let Some(a) = child(properties, "authors") {
        for name in children(a, "author").map(|n| clean(&text_of(n))) {
            if !name.is_empty() && !authors.contains(&name) {
                authors.push(name);
            }
        }
    }
    let prop = |name: &str| child(properties, name).map(|n| clean(&text_of(n))).filter(|s| !s.is_empty());

    let lyrics = child(root, "lyrics").ok_or_else(|| err("missing <lyrics>"))?;
    let marks = format_marks(root);
    let mut verses: Vec<Verse> = Vec::new();
    for verse in children(lyrics, "verse") {
        let tag = verse.attribute("name").unwrap_or("").trim().to_ascii_lowercase();
        let tag = if tag.is_empty() { format!("v{}", verses.len() + 1) } else { tag };
        let parts: Vec<Vec<String>> =
            children(verse, "lines").map(|l| lines_of(l, &marks)).filter(|p| !p.is_empty()).collect();
        let mut lines = Vec::new();
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                lines.push(SPLIT_MARKER.to_string());
            }
            lines.extend(part);
        }
        if lines.is_empty() {
            continue;
        }
        // Some files repeat a verse name (e.g. per-language variants); keep the first.
        if verses.iter().any(|v| v.tag == tag) {
            continue;
        }
        verses.push(Verse { tag, lines });
    }

    let mut titles = titles.into_iter();
    let title = titles.next().ok_or_else(|| err("song has no title"))?;
    if verses.is_empty() {
        return Err(err(format!("\"{title}\" has no lyrics")));
    }
    Ok(Song {
        id: None,
        title,
        alt_title: titles.next(),
        authors,
        copyright: prop("copyright"),
        ccli_number: prop("ccliNo"),
        verses,
        verse_order: prop("verseOrder").map(|o| Song::parse_verse_order(&o)).unwrap_or_default(),
    })
}

pub fn read_file(path: &Path) -> Result<Song, Error> {
    let bytes = fs::read(path).map_err(|e| err(format!("can't read: {e}")))?;
    let text = String::from_utf8(bytes).map_err(|_| err("not UTF-8 text"))?;
    parse(&text)
}

/// One file from [`read_folder`] and what came of parsing it.
pub type FileResult = (PathBuf, Result<Song, Error>);

/// Every `.xml` file under `dir` (recursively), parsed. Order is by path.
pub fn read_folder(dir: &Path) -> Result<Vec<FileResult>, Error> {
    let mut files = Vec::new();
    collect_xml(dir, &mut files).map_err(|e| err(format!("can't read {}: {e}", dir.display())))?;
    files.sort();
    Ok(files.into_iter().map(|p| {
        let song = read_file(&p);
        (p, song)
    }).collect())
}

fn collect_xml(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_xml(&path, out)?;
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")) {
            out.push(path);
        }
    }
    Ok(())
}

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|n| n.is_element() && n.tag_name().name() == name)
}

fn children<'a>(node: Node<'a, 'a>, name: &'a str) -> impl Iterator<Item = Node<'a, 'a>> + 'a {
    node.children().filter(move |n| n.is_element() && n.tag_name().name() == name)
}

/// All text below `node`, skipping comments.
fn text_of(node: Node) -> String {
    let mut s = String::new();
    for d in node.descendants() {
        if d.is_text() && !inside_comment(d, node) {
            s.push_str(d.text().unwrap_or(""));
        }
    }
    s
}

fn inside_comment(n: Node, stop: Node) -> bool {
    n.ancestors()
        .take_while(|a| *a != stop)
        .any(|a| a.is_element() && a.tag_name().name() == "comment")
}

/// Emphasis a formatting tag stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Strong,
    Em,
}

impl Mark {
    fn marker(self) -> &'static str {
        match self {
            Mark::Strong => "**",
            Mark::Em => "__",
        }
    }

    /// By a tag's name, for tags the song doesn't define (or defines as
    /// something else entirely).
    fn by_name(name: &str) -> Option<Mark> {
        match name.to_ascii_lowercase().as_str() {
            "strong" | "st" | "b" | "bold" => Some(Mark::Strong),
            "em" | "it" | "i" | "italic" | "emphasis" => Some(Mark::Em),
            _ => None,
        }
    }

    /// By the HTML a `<format>` definition opens it with: `<strong>`,
    /// `<b>`, `<em>`, `<i>` or a `font-weight`/`font-style` style.
    fn by_html(open: &str) -> Option<Mark> {
        let open = open.to_ascii_lowercase().replace(' ', "");
        let starts = |tag: &str| open.starts_with(&format!("<{tag}>")) || open.starts_with(&format!("<{tag}/")) || open.starts_with(&format!("<{tag} "));
        if starts("strong") || starts("b") || open.contains("font-weight:bold") || open.contains("font-weight:700") {
            Some(Mark::Strong)
        } else if starts("em") || starts("i") || open.contains("font-style:italic") {
            Some(Mark::Em)
        } else {
            None
        }
    }
}

/// The song's own formatting tags (`<format><tags><tag name="…"><open>`)
/// that mean bold or italic.
fn format_marks(root: Node) -> HashMap<String, Mark> {
    let mut marks = HashMap::new();
    for tags in child(root, "format").into_iter().flat_map(|f| children(f, "tags")) {
        for tag in children(tags, "tag") {
            let (Some(name), Some(open)) = (tag.attribute("name"), child(tag, "open")) else { continue };
            if let Some(mark) = Mark::by_html(&text_of(open)) {
                marks.insert(name.to_string(), mark);
            }
        }
    }
    marks
}

/// Lines of one `<lines>` element: `<br/>`-separated text (0.9) or `<line>`
/// children (0.8), as markup.
fn lines_of(lines: Node, marks: &HashMap<String, Mark>) -> Vec<String> {
    let line_children: Vec<Node> = children(lines, "line").collect();
    let raw: Vec<String> = if !line_children.is_empty() {
        line_children
            .into_iter()
            .map(|line| {
                let mut out = vec![String::new()];
                walk_lines(line, marks, &mut Vec::new(), &mut out);
                out.join(" ")
            })
            .collect()
    } else {
        let mut out = vec![String::new()];
        walk_lines(lines, marks, &mut Vec::new(), &mut out);
        out
    };
    raw.iter().map(|l| clean(l)).filter(|l| !l.is_empty()).collect()
}

/// Appends `node`'s text to the last of `out` (escaped, with emphasis
/// markers), starting a line at each `<br/>`. `open` are the marks around
/// it, closed at a line break and opened again after.
fn walk_lines(node: Node, marks: &HashMap<String, Mark>, open: &mut Vec<Mark>, out: &mut Vec<String>) {
    for c in node.children() {
        if c.is_text() {
            out.last_mut().unwrap().push_str(&markup::escape(c.text().unwrap_or("")));
        } else if c.is_element() {
            match c.tag_name().name() {
                "br" => {
                    let line = out.last_mut().unwrap();
                    open.iter().rev().for_each(|m| line.push_str(m.marker()));
                    out.push(open.iter().map(|m| m.marker()).collect());
                }
                "comment" | "instrument" => {}
                "tag" => {
                    let name = c.attribute("name").unwrap_or("");
                    // Inside the same emphasis already: nothing to add.
                    match marks.get(name).copied().or_else(|| Mark::by_name(name)).filter(|m| !open.contains(m)) {
                        Some(mark) => {
                            out.last_mut().unwrap().push_str(mark.marker());
                            open.push(mark);
                            walk_lines(c, marks, open, out);
                            open.pop();
                            out.last_mut().unwrap().push_str(mark.marker());
                        }
                        None => walk_lines(c, marks, open, out),
                    }
                }
                _ => walk_lines(c, marks, open, out), // chord, …: keep their text
            }
        }
    }
}

/// Collapses whitespace runs (including source newlines) to single spaces.
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---- export --------------------------------------------------------------------

const NAMESPACE: &str = "http://openlyrics.info/namespace/2009/song";

pub fn to_xml(song: &Song) -> String {
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str(&format!(
        "<song xmlns=\"{NAMESPACE}\" version=\"0.9\" createdIn=\"Asaf {v}\" modifiedIn=\"Asaf {v}\">\n",
        v = env!("CARGO_PKG_VERSION")
    ));
    x.push_str("  <properties>\n    <titles>\n");
    x.push_str(&format!("      <title>{}</title>\n", esc(&song.title)));
    if let Some(alt) = &song.alt_title {
        x.push_str(&format!("      <title>{}</title>\n", esc(alt)));
    }
    x.push_str("    </titles>\n");
    if !song.authors.is_empty() {
        x.push_str("    <authors>\n");
        for a in &song.authors {
            x.push_str(&format!("      <author>{}</author>\n", esc(a)));
        }
        x.push_str("    </authors>\n");
    }
    if let Some(c) = &song.copyright {
        x.push_str(&format!("    <copyright>{}</copyright>\n", esc(c)));
    }
    if let Some(c) = &song.ccli_number {
        x.push_str(&format!("    <ccliNo>{}</ccliNo>\n", esc(c)));
    }
    if !song.verse_order.is_empty() {
        x.push_str(&format!("    <verseOrder>{}</verseOrder>\n", esc(&song.verse_order_text())));
    }
    x.push_str("  </properties>\n");
    if song.verses.iter().flat_map(|v| &v.lines).any(|l| markup::is_styled(l)) {
        x.push_str("  <format>\n    <tags application=\"Asaf\">\n");
        for name in ["strong", "em"] {
            x.push_str(&format!(
                "      <tag name=\"{name}\">\n        <open>&lt;{name}&gt;</open>\n        <close>&lt;/{name}&gt;</close>\n      </tag>\n"
            ));
        }
        x.push_str("    </tags>\n  </format>\n");
    }
    x.push_str("  <lyrics>\n");
    for verse in &song.verses {
        x.push_str(&format!("    <verse name=\"{}\">\n", esc(&verse.tag)));
        for part in verse.parts() {
            let lines: Vec<String> = part.iter().map(|l| line_xml(l)).collect();
            x.push_str(&format!("      <lines>{}</lines>\n", lines.join("<br/>")));
        }
        x.push_str("    </verse>\n");
    }
    x.push_str("  </lyrics>\n</song>\n");
    x
}

/// One line's text, with its emphasis as `strong` and `em` tags.
fn line_xml(line: &str) -> String {
    let mut x = String::new();
    for run in markup::parse(line) {
        let tags: Vec<&str> = [(run.strong, "strong"), (run.em, "em")].into_iter().filter(|(on, _)| *on).map(|(_, t)| t).collect();
        tags.iter().for_each(|t| x.push_str(&format!("<tag name=\"{t}\">")));
        x.push_str(&esc(&run.text));
        tags.iter().rev().for_each(|_| x.push_str("</tag>"));
    }
    x
}

/// Writes one file per song into `dir`, named after the title. Returns the paths.
pub fn write_folder<'a>(songs: impl IntoIterator<Item = &'a Song>, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    fs::create_dir_all(dir)?;
    let mut used = HashSet::new();
    let mut written = Vec::new();
    for song in songs {
        let base = file_stem(&song.title);
        let mut name = format!("{base}.xml");
        let mut n = 2;
        while !used.insert(name.to_lowercase()) || dir.join(&name).exists() {
            name = format!("{base} ({n}).xml");
            n += 1;
        }
        let path = dir.join(name);
        fs::write(&path, to_xml(song))?;
        written.push(path);
    }
    Ok(written)
}

/// A safe file name for a song title.
pub fn file_stem(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_'(),.!¡¿?".contains(c) { c } else { '_' })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "Untitled".into() } else { s.chars().take(100).collect() }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
