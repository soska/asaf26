//! Inline emphasis in lyrics: `**strong**` and `__em__`, nothing else. Lines
//! keep the markup as typed (songs, the editor, the library); slides send the
//! parsed [`Run`]s to the display pages, where themes style `<strong>` and
//! `<em>`. A backslash makes the next `*`, `_` or `\` literal, and a marker
//! with no partner on its line is just text.
//!
//! Unlike CommonMark, `__text__` is emphasis (italic), not bold.

use serde::Serialize;

/// A stretch of a line in one style.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub strong: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub em: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

enum Token {
    Char(char),
    Strong,
    Em,
}

/// `line` as styled runs, without the markers. Empty runs are dropped and
/// neighbours in the same style merged.
pub fn parse(line: &str) -> Vec<Run> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\\', Some(&n @ ('*' | '_' | '\\'))) => {
                chars.next();
                tokens.push(Token::Char(n));
            }
            ('*', Some('*')) => {
                chars.next();
                tokens.push(Token::Strong);
            }
            ('_', Some('_')) => {
                chars.next();
                tokens.push(Token::Em);
            }
            _ => tokens.push(Token::Char(c)),
        }
    }
    // A marker left over after pairing them up in order is literal.
    let unpaired = |is: fn(&Token) -> bool| {
        let all: Vec<usize> = tokens.iter().enumerate().filter(|(_, t)| is(t)).map(|(i, _)| i).collect();
        (all.len() % 2 == 1).then(|| *all.last().unwrap())
    };
    let lone = [unpaired(|t| matches!(t, Token::Strong)), unpaired(|t| matches!(t, Token::Em))];

    let mut runs: Vec<Run> = Vec::new();
    let (mut strong, mut em) = (false, false);
    let mut push = |text: &str, strong: bool, em: bool| match runs.last_mut() {
        Some(last) if last.strong == strong && last.em == em => last.text.push_str(text),
        _ if !text.is_empty() => runs.push(Run { text: text.into(), strong, em }),
        _ => {}
    };
    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::Char(c) => push(c.encode_utf8(&mut [0; 4]), strong, em),
            Token::Strong if lone.contains(&Some(i)) => push("**", strong, em),
            Token::Em if lone.contains(&Some(i)) => push("__", strong, em),
            Token::Strong => strong = !strong,
            Token::Em => em = !em,
        }
    }
    runs
}

/// The text `line` shows, without markup: for search, fitting and plain
/// outputs.
pub fn plain(line: &str) -> String {
    parse(line).into_iter().map(|r| r.text).collect()
}

/// Whether `line` has any emphasis.
pub fn is_styled(line: &str) -> bool {
    parse(line).iter().any(|r| r.strong || r.em)
}

/// Markup that shows `text` exactly as it is: escapes what [`parse`] would
/// take for a marker or an escape.
pub fn escape(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        let special = match c {
            '*' | '_' => prev == Some(c) || next == Some(c),
            '\\' => matches!(next, Some('*' | '_' | '\\')),
            _ => false,
        };
        if special {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Markup for `runs`: the inverse of [`parse`].
pub fn markup(runs: &[Run]) -> String {
    let mut out = String::new();
    let (mut strong, mut em) = (false, false);
    for run in runs {
        // Close inner first, then open, so markers stay properly nested.
        if em && !run.em {
            out.push_str("__");
            em = false;
        }
        if strong != run.strong {
            if em {
                out.push_str("__");
                em = false;
            }
            out.push_str("**");
            strong = run.strong;
        }
        if em != run.em {
            out.push_str("__");
            em = run.em;
        }
        out.push_str(&escape(&run.text));
    }
    if em {
        out.push_str("__");
    }
    if strong {
        out.push_str("**");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, strong: bool, em: bool) -> Run {
        Run { text: text.into(), strong, em }
    }

    #[test]
    fn parse_table() {
        let plain_run = |t: &str| vec![run(t, false, false)];
        let cases: Vec<(&str, Vec<Run>)> = vec![
            ("Holy, holy", plain_run("Holy, holy")),
            ("**Santo**, santo", vec![run("Santo", true, false), run(", santo", false, false)]),
            ("a __b__ c", vec![run("a ", false, false), run("b", false, true), run(" c", false, false)]),
            ("**__both__**", vec![run("both", true, true)]),
            ("**a __b** c__", vec![run("a ", true, false), run("b", true, true), run(" c", false, true)]),
            ("2 ** 3", plain_run("2 ** 3")),
            ("**a** **b", vec![run("a", true, false), run(" **b", false, false)]),
            ("snake_case * one", plain_run("snake_case * one")),
            ("\\**not\\**", plain_run("**not**")),
            ("a\\\\b \\x", plain_run("a\\b \\x")),
            ("****", vec![]),
            ("", vec![]),
            ("***x***", vec![run("*x", true, false), run("*", false, false)]),
        ];
        for (line, want) in cases {
            assert_eq!(parse(line), want, "{line:?}");
        }
        assert_eq!(plain("**Santo**, __santo__"), "Santo, santo");
        assert!(is_styled("a **b**") && !is_styled("a ** b"));
    }

    #[test]
    fn escape_and_markup_round_trip() {
        for text in ["plain", "2 ** 3", "__init__", "a\\*b", "back\\\\slash", "***", "a_b*c", "\\"] {
            assert_eq!(parse(&escape(text)), if text.is_empty() { vec![] } else { vec![run(text, false, false)] }, "{text:?}");
        }
        assert_eq!(escape("snake_case and 2*3"), "snake_case and 2*3");
        for line in ["**Santo**, santo", "a __b__ c", "**__both__** and **a __b** c__", "x \\** y", "**2 \\*\\* 3**"] {
            let runs = parse(line);
            assert_eq!(parse(&markup(&runs)), runs, "{line:?} -> {:?}", markup(&runs));
        }
        assert_eq!(markup(&parse("**a __b** c__")), "**a __b__**__ c__");
    }
}
