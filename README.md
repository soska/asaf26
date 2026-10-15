# Asaf

A fast lyrics and Bible-verse projector: a Rust app with a GPUI operator
console and a web-based display. See `Rust Lyrics Projector — PRD.md`.

**Status: Phase 2 (Bibles).** A song library in SQLite with full-text search
as you type, a lyrics editor with live slide preview, OpenLyrics import and
export, and live control of a projector window on the second display. Bibles
(OSIS and Zefania) live in the same database: type a reference in English or
Spanish, press ⏎ and the verses are in the preview; ⏎ again puts them live.
Words without numbers search the translation's text.

Licensed GPL-3.0-or-later. Provided as-is, with no support.

## Run

```sh
cargo run                              # console + projector (second display if present)
cargo run -- import ~/Songs/OpenLyrics # import OpenLyrics files or folders
cargo run -- export ~/Desktop/songs    # export every song as OpenLyrics
cargo run -- import-bible kjv.osis.xml rv1909.xml   # import OSIS / Zefania Bibles
cargo run -- bibles                    # list installed Bibles
cargo run -- remove-bible RV1909       # remove one (by abbreviation or name)
cargo test --workspace                 # all tests, including headless console UI tests
```

- The library lives at `~/Library/Application Support/Asaf/asaf.sqlite` on
  macOS (the OS data dir elsewhere). `ASAF_DATA_DIR=/some/dir` uses another
  folder. A new library starts with one public-domain hymn.
- Display page: <http://127.0.0.1:4510/display> (add `?debug` for an overlay
  with socket state, revision and change-to-paint latency). Works as an OBS
  browser source.
- `ASAF_PORT=5000 cargo run` changes the port.
- `ASAF_PROJECTOR_ON_MAIN=1 cargo run` puts the fullscreen projector on the
  main display when there is no second one (for testing; it covers the screen,
  quit with ⌘Q).

## Console

Three columns: on the left the song library (search, New, Import…, Export…)
or the Bible (translation picker, Import…, reference/search box, hits), in
the middle the selected song's or passage's slides, and on the right the live
panel (what's on screen, blank state, projector display picker).

| Keys | Where | Does |
| --- | --- | --- |
| `⌘1` / `⌘2` | anywhere | Songs / Bible, cursor in its search box |
| `⌘F` / `⌘L` | anywhere | focus the current search box |
| `↑` `↓` | search | choose a song / a text-search hit |
| `⏎` | song search | put the chosen song live at slide 1 |
| `⏎` | Bible box, reference | show the passage in the preview; `⏎` again puts it live |
| `⌘⏎` | Bible box, reference | show and put live at once |
| `⏎` | Bible box, words | put the chosen hit live |
| `Esc` | search | back to live control |
| `→` `↓` `Space` `PgDn` / `←` `↑` `PgUp` | live control | next / previous slide |
| `B` / `⇧B` / `Esc` | live control | blank to black / hide text / unblank |
| `⌘E` / `⌘N` | anywhere | edit the chosen song / new song |
| `⌘S` | editor | save and close |

Live-control keys never fire while a text field has focus, so typing "b" in
search doesn't blank the screen. Clicking a slide puts that song or passage
live at that slide; double-clicking a song opens the editor, double-clicking a
Bible hit puts it live.

### Bible references

Book names work in English and Spanish whatever the translation, as full
names, common abbreviations or any unambiguous prefix, with or without
accents and dots, in any case (`Jn`, `Juan`, `San Juan`, `1 Jn`, `1Jn`,
`1ra de Juan`, `Primera Juan`, `I John`, `Génesis`, `genesis`, `Apoc`).

| Typed | Means |
| --- | --- |
| `Jn 3:16` / `Juan 3.16` | one verse |
| `Jn 3:16-18` / `Juan 3:16 al 18` | a range |
| `Gn 1:30-2:3` | a range across chapters |
| `Sal 23` / `Ps 23-24` | whole chapters |
| `Sal 23; 91:1-4` | a list (`;` starts a new chapter, a book name a new book) |
| `Juan 3:16 y Romanos 8:28` | `y` / `and` before a book name works like `;` |
| `Jn 3:16, 18` / `Juan 3:16 y 18` | more verses in the same chapter |
| `Jn 3:16-` / `Juan 3:16ss` | to the end of the chapter |
| `Jud 3`, `Flm 4-7` | one-chapter books take verses directly |

A translation's own book names (Zefania `bname`, OSIS book titles' `short`)
are used for display and also accepted when typing. The parser lives in
`asaf-core` (`reference.rs`) with table-driven tests over 440 real
references in both languages plus every name and abbreviation in the book
table (`bible.rs`).

Passage layout (in the preview header): verses per slide (1, 2, 3 or Fit,
which packs whole verses up to an estimate from the theme's size range),
verse numbers on/off (as superscripts), and the reference either in each
slide's footer (`Juan 3:16-17 (RV1909)`) or as the last line of the last
slide. Changing the layout of the live passage updates the projector in place
and keeps the verse that was showing.

### Lyrics format

```text
[v1]
Blessed assurance, Jesus is mine!
O what a foretaste of glory divine!
[---]
Heir of salvation, purchase of God,
Born of His Spirit, washed in His blood.

[c1]
This is my story, this is my song,
```

Section tags are OpenLyrics names (`v1`, `c1`, `b1`, `p1`, `e1`, `c`…) or
spelled out in English or Spanish (`[Verse 2]`, `[Chorus]`, `[Coro]`,
`[Estrofa 3]`, `[Puente]`). `[---]` splits a section across slides (shown as
`V1.1`, `V1.2`). The verse order field takes tags separated by spaces
(`v1 c1 v2 c1`); empty means "as written". Text before the first tag becomes
a verse. The editor lists problems (unknown tags in the order, duplicate
tags) next to the preview; a song needs a title and lyrics to be saved.

## Layout

| Crate | Role |
| --- | --- |
| `crates/core` (`asaf-core`) | Song/verse/slide types, lyrics text format, slide generation and splitting, Bible types and the 66-book table with English/Spanish names, the reference parser, verse-to-slide layout, `DisplayState`, the `Store` (single source of truth, change listeners). No UI or IO deps. |
| `crates/library` (`asaf-library`) | SQLite storage (rusqlite, bundled SQLite), migrations via `PRAGMA user_version`, FTS5 search indexes (songs, Bible verses), OpenLyrics and Bible import with duplicate skipping, passage lookup. |
| `crates/bible` (`asaf-bible`) | OSIS and Zefania XML readers (parsing only; depends on `core`). Test fixtures are public-domain excerpts (KJV, Reina-Valera 1909). |
| `crates/openlyrics` (`asaf-openlyrics`) | OpenLyrics XML read (0.8 and 0.9) and write (0.9), single files and folders. |
| `crates/server` (`asaf-server`) | axum on 127.0.0.1: display page, embedded assets, `/ws` pushing `{"type":"state","state":…}` on connect and on every change. |
| `display/` | `index.html`, `display.css`, `display.js`, `background.svg`. No build step; embedded into the binary with `include_str!`. |
| `crates/projector` (`asaf-projector`) | Projector window: a GPUI window hosting a wry webview, placed on the chosen display; watches for display hotplug. |
| `crates/console` (`asaf-console`) | GPUI operator UI: song library, Bible mode (`bible.rs`), preview, live control, song editor, import/export. Never draws projected output. |
| `crates/app` (`asaf`) | Binary: opens the library, wires store, server, projector and console; `import`/`export`/`import-bible`/`bibles`/`remove-bible` subcommands. |

## Decisions and deviations from the PRD

- **GPUI Kit, and why it's pinned.** The console uses
  [gpui-component](https://gpui-kit.com) (text inputs, textarea, buttons,
  notifications) through `gpui-kit =0.7.3`, which bundles its own GPUI snapshot
  (`gpui-pre 0.3.9`). It's aliased as `gpui` in `Cargo.toml` so GPUI's derive
  macros resolve. GPUI's API changes between releases without deprecation, and
  gpui-component must match the GPUI it was built against, so both move only
  together, deliberately. GPUI code lives only in `console` and `projector`.
  Phase 0 used `gpui 0.2.2` from crates.io; this snapshot needs no Metal
  Toolchain download, so the `runtime_shaders` workaround is gone.
- **`unicode-properties` is held at 0.1.3** in `Cargo.lock`: GPUI Kit's test
  support requires exactly that version. Don't `cargo update` it.
- **SQLite via rusqlite**, with SQLite compiled in (`bundled`, which includes
  FTS5). Writes use WAL with `synchronous=FULL`, so a saved edit is on disk
  before the save returns. Search uses `unicode61 remove_diacritics 2`, so
  "senor" finds "Señor"; each typed word is a prefix match and all must
  match; title hits rank above author and lyric hits.
- **Lyrics are stored in the editor's text format** (see above), so what you
  type is what's saved. The search index holds plain lyrics without tags.
- **No tao.** GPUI and tao both want to own the macOS main event loop, but
  GPUI windows expose a raw window handle, so the projector is a GPUI window
  with a wry webview attached as a child view (`build_as_child`). It shares
  one process and one event loop. On macOS the window is then made borderless
  at window level 25 (above the menu bar and Dock) and sized to its screen.
- **`console` depends on `projector`** (for the display picker) as well as
  `core` and `library`.
- **Selecting vs. going live.** Phase 3 adds proper preview/live panes; until
  then the middle column previews the chosen song and nothing goes live until
  you press ⏎ or click a slide. Editing the live song updates the projector in
  place and keeps the current slide.
- **Display choice** is remembered by display UUID in memory, so the projector
  returns to the chosen display when it's plugged back in. Without a second
  display the projector is a normal window on the main one.
- **Bibles share the song database** (migration 2): `bibles` (one row per
  translation; its book names one per line) and `bible_verses`, unique on
  `(bible_id, book, chapter, verse)` with an integer `id` (so VACUUM and
  dump/restore keep the rowids), and an external-content FTS5 index
  (`bible_fts`, same accent-folding tokenizer as songs). A ~31,000-verse
  translation imports in about a second; importing a translation whose name
  or abbreviation is already installed is refused.
- **Books are numbered 1–66 in Protestant order**, with KJV/Reina-Valera
  chapter counts, which the parser uses to reject `Juan 22`. Verse numbers
  aren't validated by the parser; a verse past the end of a chapter just
  isn't found ("Juan 3:99 isn't in RV1909"). Deuterocanonical books in a file
  are skipped and reported.
- **`BibleVerse` has no `bible_id`** (the PRD lists one): verses are only
  ever handled per translation, so the id lives in `Passage` and the table.
- **⏎ previews, ⏎ again goes live** for references, unlike songs (⏎ goes
  live), so a mistyped reference never reaches the screen; `⌘⏎` skips the
  check. Text-search hits preview as you move through them, like songs.
- **`Slide.wrap`** marks prose: the display page always wraps Bible slides
  and auto-fits them, where lyric lines are kept whole when possible.
  Verse numbers are Unicode superscripts in the text, so every output shows
  them without extra markup.
- **The store's live item is a song or a passage.** A passage goes live
  already laid out (`Store::load_passage`); the snapshot carries the live
  `Passage` so the preview can highlight the slide on air.

## Known gaps

- No delete-song button yet; Bibles are removed from the command line only
  (`asaf remove-bible`).
- The chosen translation and passage layout settings live in memory and reset
  on restart (settings arrive in Phase 4). Fit-to-screen estimates from the
  default theme.
- One versification (KJV-style chapter counts); a translation that numbers
  Joel or Malachi differently can't be addressed past those counts.
- References are parsed in English and Spanish only; the console UI is
  English. The European `Jn 3,16` (comma for colon) style isn't recognised:
  it reads as chapters 3 and 16.
- Bible files are read as UTF-8, UTF-16 (with a byte-order mark) or, failing
  those, Latin-1; other encodings come out garbled.
- Verse ranges inside one item can't cross books (`Mal 4 - Mt 1`).
- Clicking the projector window gives the webview keyboard focus; operate from
  the console.
- Borderless fullscreen is implemented for macOS only; other platforms get a
  titlebar-less GPUI window on the display.
- A slide too long to fit at the minimum size wraps and then clips; split it
  with `[---]`.
