//! The extractor: finds the console's messages in the source and writes
//! `locales/en.json` and `locales/en.meta.json` exactly as duckalization's
//! own `extract` would, so `duckalize translate|glossary|review` work on them.
//!
//! Sources are tokenized (comments, strings and char literals understood),
//! not parsed: the calls in [`crate`]'s table are recognised by their token
//! shape. Everything from a `#[cfg(test)]` line to the end of a Rust file is
//! skipped, so test strings never reach the catalog.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::id::{Message, PluralForm, canonical, message_id};

/// Where the catalog is written, relative to the repo root.
pub const CATALOG: &str = "locales/en.json";
pub const META: &str = "locales/en.meta.json";

/// Rust source directories (scanned recursively) and the folder of web pages
/// (its `.js` files only), relative to the repo root.
const RUST_DIRS: [&str; 2] = ["crates/console/src", "crates/app/src"];
const JS_DIR: &str = "display";

/// One catalog entry's sidecar: everything a translator needs for context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EntryMeta {
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// `path:line:column` call sites, sorted, without repeats.
    pub refs: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Extracted {
    pub catalog: BTreeMap<String, Message>,
    pub meta: BTreeMap<String, EntryMeta>,
    pub files: usize,
    pub calls: usize,
}

/// A problem at a call site; any one of them stops the catalog being written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}: {}", self.file, self.line, self.column, self.message)
    }
}

/// One message call found in a file.
#[derive(Debug)]
struct Call {
    message: Message,
    context: Option<String>,
    line: usize,
    column: usize,
}

/// Scans the repo's sources.
pub fn extract(root: &Path) -> Result<Extracted, Vec<Diagnostic>> {
    let io_error = |path: &Path, e: std::io::Error| {
        vec![Diagnostic { file: relative(root, path), line: 1, column: 1, message: e.to_string() }]
    };
    let mut files = Vec::new();
    for dir in RUST_DIRS {
        collect(&root.join(dir), "rs", true, &mut files).map_err(|e| io_error(&root.join(dir), e))?;
    }
    collect(&root.join(JS_DIR), "js", false, &mut files).map_err(|e| io_error(&root.join(JS_DIR), e))?;
    files.sort();
    let mut sources = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).map_err(|e| io_error(path, e))?;
        sources.push((relative(root, path), text));
    }
    extract_sources(&sources)
}

/// Scans `(path, text)` pairs; the extension of `path` picks Rust or JS.
pub fn extract_sources(sources: &[(String, String)]) -> Result<Extracted, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    // (path, id, call), in source order.
    let mut found: Vec<(&String, String, Call)> = Vec::new();
    for (path, text) in sources {
        let calls = if path.ends_with(".js") { js_calls(text) } else { rust_calls(text) };
        for call in calls {
            let call = match call {
                Ok(call) => call,
                Err((line, column, message)) => {
                    diagnostics.push(Diagnostic { file: path.clone(), line, column, message });
                    continue;
                }
            };
            found.push((path, message_id(&call.message, call.context.as_deref()), call));
        }
    }

    let mut extracted = Extracted { files: sources.len(), calls: found.len(), ..Default::default() };
    let mut by_id: BTreeMap<&str, Vec<&(&String, String, Call)>> = BTreeMap::new();
    for entry in &found {
        by_id.entry(&entry.1).or_default().push(entry);
    }
    for (id, group) in by_id {
        let (path, _, first) = group[0];
        let identity = |call: &Call| (call.context.clone(), canonical(&call.message));
        let mut refs: Vec<String> = group.iter().map(|(path, _, c)| format!("{path}:{}:{}", c.line, c.column)).collect();
        refs.sort();
        refs.dedup();
        if group.iter().any(|(_, _, c)| identity(c) != identity(first)) {
            let sites: Vec<String> = refs.iter().map(|r| format!("    at {r}")).collect();
            diagnostics.push(Diagnostic {
                file: path.to_string(),
                line: first.line,
                column: first.column,
                message: format!("ID \"{id}\" maps to different messages:\n{}\n  Disambiguate with a context (`.tc`).", sites.join("\n")),
            });
            continue;
        }
        extracted.catalog.insert(id.to_string(), first.message.clone());
        extracted.meta.insert(id.to_string(), EntryMeta { message: first.message.clone(), context: first.context.clone(), refs });
    }

    if diagnostics.is_empty() { Ok(extracted) } else { Err(diagnostics) }
}

/// `en.json` and `en.meta.json` as duckalization writes them: keys sorted,
/// two-space indent, trailing newline.
pub fn render(extracted: &Extracted) -> (String, String) {
    (pretty(&extracted.catalog), pretty(&extracted.meta))
}

/// Extracts and writes both files. Nothing is written on errors.
pub fn write(root: &Path) -> Result<Extracted, String> {
    let extracted = extract(root).map_err(report)?;
    let (catalog, meta) = render(&extracted);
    let dir = root.join(CATALOG).parent().expect("locales dir").to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (path, text) in [(CATALOG, catalog), (META, meta)] {
        std::fs::write(root.join(path), text).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(extracted)
}

/// Fails if the committed files differ from what the source produces now.
pub fn check(root: &Path) -> Result<Extracted, String> {
    let extracted = extract(root).map_err(report)?;
    let (catalog, meta) = render(&extracted);
    for (path, text) in [(CATALOG, catalog), (META, meta)] {
        if std::fs::read_to_string(root.join(path)).ok().as_deref() != Some(text.as_str()) {
            return Err(format!("{path} is out of date: run `cargo run -p asaf-i18n --bin i18n-extract`"));
        }
    }
    Ok(extracted)
}

fn report(diagnostics: Vec<Diagnostic>) -> String {
    diagnostics.iter().map(|d| format!("error: {d}")).collect::<Vec<_>>().join("\n")
}

fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

fn collect(dir: &Path, ext: &str, recurse: bool, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if recurse {
                collect(&path, ext, recurse, out)?;
            }
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
    Ok(())
}

/// serde_json's pretty printer matches `JSON.stringify(value, null, 2)` for
/// these shapes (ASCII keys, strings, nested objects and arrays).
fn pretty(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).expect("catalog serializes") + "\n"
}

// ---- Tokens -------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Tok {
    Ident(String),
    /// A string literal, unescaped; `template` for JS backtick strings.
    Str { value: String, template: bool },
    Punct(char),
}

#[derive(Debug)]
struct Token {
    tok: Tok,
    line: usize,
    column: usize,
}

struct Lexer {
    chars: Vec<char>,
    at: usize,
    line: usize,
    column: usize,
    js: bool,
}

impl Lexer {
    fn new(src: &str, js: bool) -> Self {
        Lexer { chars: src.chars().collect(), at: 0, line: 1, column: 1, js }
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.at).copied()?;
        self.at += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn tokens(mut self) -> Vec<Token> {
        let mut out = Vec::new();
        while let Some(c) = self.peek(0) {
            let (line, column) = (self.line, self.column);
            let tok = if c.is_whitespace() {
                self.bump();
                continue;
            } else if c == '/' && self.peek(1) == Some('/') {
                while self.peek(0).is_some_and(|c| c != '\n') {
                    self.bump();
                }
                continue;
            } else if c == '/' && self.peek(1) == Some('*') {
                self.block_comment();
                continue;
            } else if c == '"' || (self.js && (c == '\'' || c == '`')) {
                self.bump();
                Tok::Str { value: self.string(c), template: c == '`' }
            } else if !self.js && c == 'r' && matches!((self.peek(1), self.peek(2)), (Some('"'), _) | (Some('#'), Some('"' | '#'))) {
                self.bump();
                Tok::Str { value: self.raw_string(), template: false }
            } else if !self.js && c == 'b' && self.peek(1) == Some('"') {
                self.bump();
                self.bump();
                Tok::Str { value: self.string('"'), template: false }
            } else if !self.js && c == '\'' {
                // A char literal ('x', '\n', '\u{..}'), or a lifetime ('a).
                if self.peek(1) == Some('\\') || self.peek(2) == Some('\'') {
                    self.bump();
                    self.string('\'');
                    continue;
                }
                self.bump();
                Tok::Punct('\'')
            } else if c.is_alphanumeric() || c == '_' || (self.js && c == '$') {
                let mut ident = String::new();
                while let Some(c) = self.peek(0).filter(|c| c.is_alphanumeric() || *c == '_' || (self.js && *c == '$')) {
                    ident.push(c);
                    self.bump();
                }
                Tok::Ident(ident)
            } else {
                self.bump();
                Tok::Punct(c)
            };
            out.push(Token { tok, line, column });
        }
        out
    }

    fn block_comment(&mut self) {
        self.bump();
        self.bump();
        let mut depth = 1;
        while depth > 0 && self.peek(0).is_some() {
            if self.peek(0) == Some('/') && self.peek(1) == Some('*') && !self.js {
                depth += 1;
                self.bump();
            } else if self.peek(0) == Some('*') && self.peek(1) == Some('/') {
                depth -= 1;
                self.bump();
            }
            self.bump();
        }
    }

    /// The rest of a quoted literal after its opening `quote`, unescaped.
    fn string(&mut self, quote: char) -> String {
        let mut out = String::new();
        while let Some(c) = self.bump() {
            if c == quote {
                break;
            }
            if c != '\\' {
                out.push(c);
                continue;
            }
            let Some(e) = self.bump() else { break };
            match e {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '0' => out.push('\0'),
                'x' => {
                    let hex: String = [self.bump(), self.bump()].into_iter().flatten().collect();
                    out.extend(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32));
                }
                'u' if self.peek(0) == Some('{') => {
                    self.bump();
                    let mut hex = String::new();
                    while let Some(c) = self.bump().filter(|c| *c != '}') {
                        hex.push(c);
                    }
                    out.extend(u32::from_str_radix(&hex.replace('_', ""), 16).ok().and_then(char::from_u32));
                }
                'u' if self.js => {
                    let hex: String = (0..4).filter_map(|_| self.bump()).collect();
                    let unit = u16::from_str_radix(&hex, 16).unwrap_or(0xfffd);
                    // A surrogate pair is two `\uXXXX` escapes in a row.
                    if (0xd800..0xdc00).contains(&unit) && self.peek(0) == Some('\\') && self.peek(1) == Some('u') {
                        self.bump();
                        self.bump();
                        let low: String = (0..4).filter_map(|_| self.bump()).collect();
                        let low = u16::from_str_radix(&low, 16).unwrap_or(0xfffd);
                        out.extend(char::decode_utf16([unit, low]).map(|r| r.unwrap_or('\u{fffd}')));
                    } else {
                        out.extend(char::decode_utf16([unit]).map(|r| r.unwrap_or('\u{fffd}')));
                    }
                }
                '\n' | '\r' => {
                    if e == '\r' && self.peek(0) == Some('\n') {
                        self.bump();
                    }
                    // Rust's continuation also drops the next line's indent; JS's
                    // keeps it.
                    if !self.js {
                        while self.peek(0).is_some_and(char::is_whitespace) {
                            self.bump();
                        }
                    }
                }
                other => out.push(other),
            }
        }
        out
    }

    /// `r"…"`, `r#"…"#`: after the `r`.
    fn raw_string(&mut self) -> String {
        let mut hashes = 0;
        while self.peek(0) == Some('#') {
            hashes += 1;
            self.bump();
        }
        self.bump(); // the opening quote
        let mut out = String::new();
        while let Some(c) = self.bump() {
            if c == '"' && (0..hashes).all(|i| self.peek(i) == Some('#')) {
                for _ in 0..hashes {
                    self.bump();
                }
                break;
            }
            out.push(c);
        }
        out
    }
}

// ---- Calls --------------------------------------------------------------

type Found = Result<Call, (usize, usize, String)>;

/// The source before its `#[cfg(test)]` line, if any.
fn without_tests(src: &str) -> &str {
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        if line.trim_start().starts_with("#[cfg(test)]") {
            return &src[..offset];
        }
        offset += line.len();
    }
    src
}

fn rust_calls(src: &str) -> Vec<Found> {
    let toks = Lexer::new(without_tests(src), false).tokens();
    let mut out = Vec::new();
    for i in 0..toks.len() {
        let Tok::Ident(name) = &toks[i].tok else { continue };
        let after_dot = i > 0 && toks[i - 1].tok == Tok::Punct('.');
        if toks.get(i + 1).map(|t| &t.tok) != Some(&Tok::Punct('(')) {
            continue;
        }
        let (line, column) = (toks[i].line, toks[i].column);
        let args = &toks[i + 2..];
        let call = match (name.as_str(), after_dot) {
            // A variable argument is text marked with `en("…")` elsewhere.
            ("t", true) | ("en", false) => match literal(args, 0) {
                Some(text) => text_call(text, None),
                None => continue,
            },
            ("f", true) => match literal(args, 0) {
                Some(text) => text_call(text, None),
                None => Err("`.f(…)` needs a string literal message".into()),
            },
            ("tc", true) => match (literal(args, 0), args.get(1).map(|t| &t.tok), literal(args, 2)) {
                (Some(text), Some(Tok::Punct(',')), Some(context)) => text_call(text, Some(context)),
                _ => Err("`.tc(…)` needs a string literal message and context".into()),
            },
            ("n", true) => plural_call(args),
            _ => continue,
        };
        out.push(call.map(|(message, context)| Call { message, context, line, column }).map_err(|m| (line, column, m)));
    }
    out
}

fn js_calls(src: &str) -> Vec<Found> {
    let toks = Lexer::new(src, true).tokens();
    let mut out = Vec::new();
    for i in 0..toks.len() {
        if toks[i].tok != Tok::Ident("__".into()) || toks.get(i + 1).map(|t| &t.tok) != Some(&Tok::Punct('(')) {
            continue;
        }
        if i > 0 && matches!(toks[i - 1].tok, Tok::Punct('.')) {
            continue;
        }
        let (line, column) = (toks[i].line, toks[i].column);
        let call = match toks.get(i + 2).map(|t| &t.tok) {
            Some(Tok::Str { template: true, .. }) => Err("template literals can't be extracted; use a {placeholder}".to_string()),
            Some(Tok::Str { value, .. }) => text_call(value, None),
            _ => Err("`__(…)` needs a string literal message".to_string()),
        };
        out.push(call.map(|(message, context)| Call { message, context, line, column }).map_err(|m| (line, column, m)));
    }
    out
}

/// The string literal at `args[at]`, if it's a plain one.
fn literal(args: &[Token], at: usize) -> Option<&str> {
    match args.get(at).map(|t| &t.tok) {
        Some(Tok::Str { value, template: false }) => Some(value),
        _ => None,
    }
}

fn text_call(text: &str, context: Option<&str>) -> Result<(Message, Option<String>), String> {
    if text.is_empty() {
        return Err("empty message".into());
    }
    Ok((Message::Text(text.into()), context.map(Into::into)))
}

/// `.n(<count>, "one", "other")`: skips the count expression to its
/// top-level comma.
fn plural_call(args: &[Token]) -> Result<(Message, Option<String>), String> {
    let malformed = || "`.n(count, \"one\", \"other\")` needs two string literal forms".to_string();
    let mut depth = 0;
    let mut at = 0;
    loop {
        match args.get(at).map(|t| &t.tok) {
            None => return Err(malformed()),
            Some(Tok::Punct('(' | '[' | '{')) => depth += 1,
            Some(Tok::Punct(')' | ']' | '}')) if depth == 0 => return Err(malformed()),
            Some(Tok::Punct(')' | ']' | '}')) => depth -= 1,
            Some(Tok::Punct(',')) if depth == 0 => break,
            _ => {}
        }
        at += 1;
    }
    if at == 0 {
        return Err(malformed());
    }
    let (Some(one), Some(Tok::Punct(',')), Some(other)) =
        (literal(args, at + 1), args.get(at + 2).map(|t| &t.tok), literal(args, at + 3))
    else {
        return Err(malformed());
    };
    let close = match args.get(at + 4).map(|t| &t.tok) {
        Some(Tok::Punct(',')) => args.get(at + 5),
        _ => args.get(at + 4),
    };
    if close.map(|t| &t.tok) != Some(&Tok::Punct(')')) {
        return Err(malformed());
    }
    if one.is_empty() || other.is_empty() {
        return Err("empty plural form".into());
    }
    let forms = BTreeMap::from([(PluralForm::One, one.to_string()), (PluralForm::Other, other.to_string())]);
    Ok((Message::Plural(forms), None))
}
