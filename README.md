# Asaf

A fast lyrics and Bible-verse projector: a Rust app with a GPUI operator
console and a web-based display. See `Rust Lyrics Projector — PRD.md`.

**Status: Phase 3 (service and live operation).** A song library in SQLite
with full-text search as you type, a lyrics editor with live slide preview,
OpenLyrics import and export, and Bibles (OSIS and Zefania) looked up by
reference in English or Spanish. Songs and passages go into a service list
(reorder by keyboard or drag, notes and a theme name per item) that is saved
as an `.asaf-service` file and autosaved continuously, so a crash or restart
brings it back. Separate preview and live panes; a whole service runs from
the keyboard: ⏎ puts the cued item live and cues the next, arrows step
through slides, B / ⇧B / L blank to black, background or logo.

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
- The current service is autosaved next to it as `autosave.asaf-service` and
  restored on every start (a damaged one is renamed `….corrupt`, or
  `….corrupt-2` and so on, and reported, never deleted).
- Display page: <http://127.0.0.1:4510/display> (add `?debug` for an overlay
  with socket state, revision and change-to-paint latency). Works as an OBS
  browser source.
- `ASAF_PORT=5000 cargo run` changes the port.
- `ASAF_PROJECTOR_ON_MAIN=1 cargo run` puts the fullscreen projector on the
  main display when there is no second one (for testing; it covers the screen,
  quit with ⌘Q).

## Console

Four columns, left to right:

1. **Source**: the song library (search, New, Import…, Export…) or the Bible
   (translation picker, Import…, reference/search box, hits). Whatever you
   choose here shows in the preview while you browse; ⇧⏎ adds it to the
   service, ⏎ still puts it live straight away (ad hoc).
2. **Service**: the list, with Open… / Save and the cued item's notes and
   theme fields. The cued item has an outline; the one on screen a red dot.
   Drag rows to reorder, drag songs in from the library.
3. **Preview**: the cued service item's slides (or the source panel's choice
   while you're in it), with its notes and, for passages, the layout options.
4. **Live**: what's on the projector, slide by slide, the blank state, and the
   projector display picker.

| Keys | Where | Does |
| --- | --- | --- |
| `⌘1` / `⌘2` | anywhere | Songs / Bible, cursor in its search box |
| `⌘F` / `⌘L` | anywhere | focus the current search box |
| `↑` `↓` | search | choose a song / a text-search hit |
| `⇧⏎` | song search, Bible box | add the chosen song / the reference or hit to the service |
| `⏎` | song search | put the chosen song live at slide 1 (ad hoc) |
| `⏎` | Bible box, reference | show the passage in the preview; `⏎` again puts it live |
| `⌘⏎` | Bible box, reference | show and put live at once |
| `⏎` | Bible box, words | put the chosen hit live |
| `Esc` | search, notes | back to live control (the preview goes back to the cued item) |
| `⏎` | live control | put the previewed (cued) item live and cue the next one (after the last item: nothing) |
| `⌘↓` / `⌘↑` | live control | cue the next / previous service item |
| `⌥↓` / `⌥↑` | live control | move the cued item down / up |
| `⌘⌫` | live control | remove the cued item |
| `→` `↓` `Space` `PgDn` / `←` `↑` `PgUp` | live control | next / previous slide |
| `B` / `⇧B` / `L` / `Esc` | live control | blank to black / hide text / show logo / show text again |
| `⌘I` | anywhere | edit the cued item's notes (`⏎` or `Esc` when done) |
| `⌘S` / `⇧⌘S` / `⌘O` / `⇧⌘N` | anywhere | save / save as / open / new service |
| `⌘E` / `⌘N` | anywhere | edit the previewed song / new song |
| `⌘S` | editor | save and close |

Live-control keys never fire while a text field has focus, so typing "b" in
search doesn't blank the screen and `⌘⌫` in a field deletes text, not a
service item. Clicking a service item cues it, double-clicking puts it live;
clicking a slide puts that item live at that slide (in the live pane: jumps
to it). Double-clicking a song opens the editor, a Bible hit goes live.

Running a service: prepare it with `⇧⏎` from the source panel, `Esc`, then
`⏎` (first item live), `→`… through its slides, `⏎` (next item), and so on.
An ad-hoc verse (`⌘2`, type, `⏎ ⏎`) doesn't disturb the cue: the next `⏎`
carries on with the service.

### Service files

`.asaf-service` files are JSON, versioned, readable in any text editor:

```json
{
  "format": "asaf-service",
  "version": 1,
  "name": "Sunday 10am",
  "items": [
    { "type": "song", "song_id": 12, "title": "Blessed Assurance", "notes": "Slow intro" },
    { "type": "passage", "bible_id": 2, "translation": "RV1909", "reference": "Juan 3:16-18",
      "theme": "Scripture",
      "ranges": [{ "book": 43, "start": { "chapter": 3, "verse": 16 }, "end": { "chapter": 3, "verse": 18 } }] }
  ]
}
```

Items point into the library rather than copying it: a song is found by id
when its title still matches, else by title (a service made on another
machine), never by id alone (on another machine that id is some other song);
renaming a song in the editor updates the open service. A passage is found by
translation abbreviation, then id, and is laid out with the current passage
layout when shown. An item
that can't be found stays in the list and says why in the preview
("“X” isn't in the song library", "The NVI Bible isn't installed"); ⏎ on it
does nothing. Unknown fields are ignored, so later versions can add fields;
a file from a newer format version is refused with a message saying so. The
autosave is the same format plus an `autosave` field (the file it belongs to
and whether it had unsaved changes).

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
| `crates/core` (`asaf-core`) | Song/verse/slide types, lyrics text format, slide generation and splitting, Bible types and the 66-book table with English/Spanish names, the reference parser, verse-to-slide layout, the service model and its JSON file format (`service.rs`), `DisplayState`, the `Store` (single source of truth, change listeners). No UI or IO deps (serde/serde_json only). |
| `crates/library` (`asaf-library`) | SQLite storage (rusqlite, bundled SQLite), migrations via `PRAGMA user_version`, FTS5 search indexes (songs, Bible verses), OpenLyrics and Bible import with duplicate skipping, passage lookup. Service files on disk (`service.rs`): atomic writes, reading, autosave restore, finding a service's songs. |
| `crates/bible` (`asaf-bible`) | OSIS and Zefania XML readers (parsing only; depends on `core`). Test fixtures are public-domain excerpts (KJV, Reina-Valera 1909). |
| `crates/openlyrics` (`asaf-openlyrics`) | OpenLyrics XML read (0.8 and 0.9) and write (0.9), single files and folders. |
| `crates/server` (`asaf-server`) | axum on 127.0.0.1: display page, embedded assets, `/ws` pushing `{"type":"state","state":…}` on connect and on every change. |
| `display/` | `index.html`, `display.css`, `display.js`, `background.svg`, `logo.svg` (placeholder). No build step; embedded into the binary with `include_str!`. |
| `crates/projector` (`asaf-projector`) | Projector window: a GPUI window hosting a wry webview, placed on the chosen display; watches for display hotplug. |
| `crates/console` (`asaf-console`) | GPUI operator UI: song library and Bible mode (`bible.rs`) as sources, the service list with files and autosave (`service.rs`), preview and live panes with slide cards (`slides.rs`), song editor, import/export. Never draws projected output. |
| `crates/app` (`asaf`) | Binary: opens the library, restores the autosaved service, wires store, server, projector and console; `import`/`export`/`import-bible`/`bibles`/`remove-bible` subcommands. |

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
- **Preview and live.** The preview shows the cued service item, or the
  source panel's choice while you're browsing it (typing, ↑↓, ⌘1/⌘2); `Esc`
  or putting anything live brings it back to the cue. ⏎ in live control puts
  whatever the preview shows live. After a service item goes live the cue
  moves to the next item, so ⏎ always means "next thing in the service";
  once the last item is live, ⏎ does nothing rather than restart it. The
  service lives in the console (not the core store): only what's live is
  shared state; the remote (Phase 5) can move it into the store.
  Editing the live song updates the projector in place and keeps the current
  slide; renaming a song updates its title in the service.
- **Service files are our own JSON**, not SQLite: one small human-readable
  file per service that can be emailed or kept in a shared folder. Items refer
  to the library (see "Service files") rather than embedding lyrics, so an
  edit made minutes before the service shows up without re-adding the song.
  The file name names the service.
- **Autosave**: every change to the service (items, order, notes, theme)
  schedules a write 500 ms after the last change; the write goes to a
  temporary file that is fsynced and renamed over `autosave.asaf-service`, so
  a crash mid-write leaves the previous autosave intact. A pending write is
  flushed on ⌘Q and when the console window closes. On start the autosave is always restored (crash or clean
  quit), with the file it belongs to and its unsaved-changes flag; the
  console says "Restored … from the last session". New/Open ask before
  discarding unsaved changes.
- **Blank to logo** is a third blank mode (`Blank::Logo`): the theme's logo
  centred over its background, with the text's drop shadow. Until themes
  arrive (Phase 4) the logo is a placeholder (`display/logo.svg`).
- **Per-item theme** is stored (a theme name, free text) and saved, but not
  applied until themes exist (Phase 4).
- **Notes** are for the operator only: shown under the item in the service
  list and above its slides in the preview, never projected.
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

- Removing a service item can't be undone (re-add it; the autosave doesn't
  help because it follows the change).
- The passage layout (verses per slide etc.) is one setting for all passages,
  not per service item, and isn't saved in the service file.
- Only songs can be dragged into the service; passages are added with ⇧⏎ or
  the Add button. There's no way to edit a passage item's reference in
  place (remove and re-add).
- Service items don't embed lyrics: a service opened on a machine without the
  same songs (by title) or translations (by abbreviation) shows those items as
  missing. So does a song renamed while a saved service that uses it wasn't
  open (re-add it).
- Quitting with unsaved changes doesn't ask; the autosave brings them back on
  the next start.
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
