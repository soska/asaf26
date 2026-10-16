use std::collections::BTreeMap;
use std::path::Path;

use asaf_i18n::extract::{self, Extracted, extract_sources};
use asaf_i18n::id::text_id;
use asaf_i18n::{Message, PluralForm, message_id};

fn rust(src: &str) -> Extracted {
    extract_sources(&[("crates/console/src/x.rs".into(), src.into())]).unwrap()
}

fn messages(x: &Extracted) -> Vec<&str> {
    let mut out: Vec<&str> = x
        .catalog
        .values()
        .filter_map(|m| match m {
            Message::Text(t) => Some(t.as_str()),
            Message::Plural(_) => None,
        })
        .collect();
    out.sort();
    out
}

fn errors(path: &str, src: &str) -> Vec<String> {
    extract_sources(&[(path.into(), src.into())]).unwrap_err().iter().map(ToString::to_string).collect()
}

#[test]
fn finds_each_call_shape() {
    let x = rust(
        r#"
        fn view(l: Lang, n: usize, name: &str) {
            l.t("Songs");
            l.t(if logo { en("Logo") } else { en("Background image") });
            l.f("Added {title} to the service", &[("title", name)]);
            l.n(items.len(), "{count} song", "{count} songs");
            l.tc("Book", "verb");
        }
        "#,
    );
    assert_eq!(messages(&x), ["Added {title} to the service", "Background image", "Book", "Logo", "Songs"]);
    let plural = Message::Plural(BTreeMap::from([
        (PluralForm::One, "{count} song".into()),
        (PluralForm::Other, "{count} songs".into()),
    ]));
    assert_eq!(x.catalog[&message_id(&plural, None)], plural);
    let book = &x.meta[&text_id("Book", Some("verb"))];
    assert_eq!(book.context.as_deref(), Some("verb"));
    assert_eq!(x.calls, 6);
}

#[test]
fn skips_dynamic_text_comments_and_other_functions() {
    let x = rust(
        r#"
        //! Docs mention `l.t("In a doc comment")`.
        pub const fn en(text: &'static str) -> &'static str { text }
        fn f(l: Lang) {
            l.t(label); // `.t` of a variable: marked with en() elsewhere
            /* l.t("In a block comment") */
            open("not a message"); then("nor this"); x.len();
            let c = '"'; l.t("After a quote char");
            fn generic<'a>(s: &'a str) -> &'a str { s }
        }
        "#,
    );
    assert_eq!(messages(&x), ["After a quote char"]);
}

#[test]
fn stops_at_the_test_module() {
    let x = rust("fn a(l: Lang) { l.t(\"Kept\"); }\n\n#[cfg(test)]\nmod tests { fn b(l: Lang) { l.t(\"not in the table\"); } }\n");
    assert_eq!(messages(&x), ["Kept"]);
}

#[test]
fn unescapes_literals() {
    let x = rust(
        "l.t(\"Say \\\"hi\\\" \\\\ now\");\n\
         l.t(\"Tab\\there\");\n\
         l.t(\"Caf\\u{e9}\");\n\
         l.t(\"One long \\\n              line\");\n\
         l.t(r#\"Raw \"quoted\"\"#);\n",
    );
    assert_eq!(messages(&x), ["Café", "One long line", "Raw \"quoted\"", "Say \"hi\" \\ now", "Tab\there"]);
}

#[test]
fn reads_js_calls() {
    let src = "// __('In a comment')\nconst a = __(\"Connect\");\nb.textContent = __('Go live');\nc = __('It\\'s \\u00e9');\n";
    let x = extract_sources(&[("display/remote.js".into(), src.into())]).unwrap();
    assert_eq!(messages(&x), ["Connect", "Go live", "It's é"]);
    assert_eq!(x.meta[&text_id("Connect", None)].refs, ["display/remote.js:2:11"]);
}

#[test]
fn merges_repeats_into_sorted_refs() {
    let x = extract_sources(&[
        ("crates/console/src/b.rs".into(), "l.t(\"Songs\");\nl.t(\"Songs\");".into()),
        ("crates/console/src/a.rs".into(), "  l.t(\"Songs\");".into()),
    ])
    .unwrap();
    assert_eq!(x.catalog.len(), 1);
    assert_eq!(
        x.meta[&text_id("Songs", None)].refs,
        ["crates/console/src/a.rs:1:5", "crates/console/src/b.rs:1:3", "crates/console/src/b.rs:2:3"]
    );
}

#[test]
fn reports_bad_calls_with_their_place() {
    assert_eq!(errors("crates/console/src/x.rs", "\n  l.t(\"\");"), ["crates/console/src/x.rs:2:5: empty message"]);
    let bad = [
        "l.f(name, &[]);",
        "l.tc(\"Book\", ctx);",
        "l.n(count, \"{count} song\");",
        "l.n(count, one, \"{count} songs\");",
        "l.n(count, \"a\", \"b\", \"c\");",
    ];
    for src in bad {
        let errs = errors("crates/console/src/x.rs", src);
        assert_eq!(errs.len(), 1, "{src}: {errs:?}");
        assert!(errs[0].starts_with("crates/console/src/x.rs:1:3: "), "{src}: {errs:?}");
    }
    assert_eq!(errors("display/x.js", "__(`Hi ${name}`)").len(), 1);
    assert_eq!(errors("display/x.js", "__(label)").len(), 1);
}

#[test]
fn renders_like_duckalization() {
    let x = rust("l.tc(\"Book\", \"verb\"); l.t(\"Songs\");");
    let (catalog, meta) = extract::render(&x);
    let songs = text_id("Songs", None);
    let book = text_id("Book", Some("verb"));
    let (first, second) = if book < songs { (&book, &songs) } else { (&songs, &book) };
    let value = |id: &String| if *id == songs { "Songs" } else { "Book" };
    assert_eq!(catalog, format!("{{\n  \"{first}\": \"{}\",\n  \"{second}\": \"{}\"\n}}\n", value(first), value(second)));
    assert!(meta.contains(&format!(
        "  \"{book}\": {{\n    \"message\": \"Book\",\n    \"context\": \"verb\",\n    \"refs\": [\n      \"crates/console/src/x.rs:1:3\"\n    ]\n  }}"
    )));
    assert!(meta.ends_with("}\n"));
}

/// The committed catalog matches the source: run
/// `cargo run -p asaf-i18n --bin i18n-extract` after changing any message.
#[test]
fn committed_catalog_is_up_to_date() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let x = extract::check(&root).unwrap_or_else(|e| panic!("{e}"));
    assert!(x.catalog.len() > 100, "found {} messages", x.catalog.len());
}
