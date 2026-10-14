# Asaf

A fast lyrics and Bible-verse projector: a Rust app with a GPUI operator
console and a web-based display. See `Rust Lyrics Projector — PRD.md`.

**Status: Phase 1 (songs).** A song library in SQLite with full-text search
as you type, a lyrics editor with live slide preview, OpenLyrics import and
export, and live control of a projector window on the second display.

Licensed GPL-3.0-or-later. Provided as-is, with no support.

## Run

```sh
cargo run                              # console + projector (second display if present)
cargo run -- import ~/Songs/OpenLyrics # import OpenLyrics files or folders
cargo run -- export ~/Desktop/songs    # export every song as OpenLyrics
cargo test                             # all tests, including headless console UI tests
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

Three columns: the song library (search, New, Import…, Export…), the selected
song's slides, and the live panel (what's on screen, blank state, projector
display picker).

| Keys | Where | Does |
| --- | --- | --- |
| `⌘F` / `⌘L` | anywhere | focus search |
| `↑` `↓` | search | choose a song |
| `⏎` | search | put the chosen song live at slide 1 |
| `Esc` | search | back to live control |
| `→` `↓` `Space` `PgDn` / `←` `↑` `PgUp` | live control | next / previous slide |
| `B` / `⇧B` / `Esc` | live control | blank to black / hide text / unblank |
| `⌘E` / `⌘N` | anywhere | edit the chosen song / new song |
| `⌘S` | editor | save and close |

Live-control keys never fire while a text field has focus, so typing "b" in
search doesn't blank the screen. Clicking a slide puts that song live at that
slide; double-clicking a song opens the editor.

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
| `crates/core` (`asaf-core`) | Song/verse/slide types, lyrics text format, slide generation and splitting, `DisplayState`, the `Store` (single source of truth, change listeners). No UI or IO deps. |
| `crates/library` (`asaf-library`) | SQLite storage (rusqlite, bundled SQLite), migrations via `PRAGMA user_version`, FTS5 search index, OpenLyrics import with duplicate skipping. |
| `crates/openlyrics` (`asaf-openlyrics`) | OpenLyrics XML read (0.8 and 0.9) and write (0.9), single files and folders. |
| `crates/server` (`asaf-server`) | axum on 127.0.0.1: display page, embedded assets, `/ws` pushing `{"type":"state","state":…}` on connect and on every change. |
| `display/` | `index.html`, `display.css`, `display.js`, `background.svg`. No build step; embedded into the binary with `include_str!`. |
| `crates/projector` (`asaf-projector`) | Projector window: a GPUI window hosting a wry webview, placed on the chosen display; watches for display hotplug. |
| `crates/console` (`asaf-console`) | GPUI operator UI: library, preview, live control, song editor, import/export. Never draws projected output. |
| `crates/app` (`asaf`) | Binary: opens the library, wires store, server, projector and console; `import`/`export` subcommands. |

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

## Known gaps

- No delete-song button yet.
- Clicking the projector window gives the webview keyboard focus; operate from
  the console.
- Borderless fullscreen is implemented for macOS only; other platforms get a
  titlebar-less GPUI window on the display.
- A slide too long to fit at the minimum size wraps and then clips; split it
  with `[---]`.
