# Asaf

A fast lyrics and Bible-verse projector: a Rust app with a GPUI operator
console and a web-based display. See `Rust Lyrics Projector — PRD.md`.

**Status: Phase 5 (remote and extra outputs).** A song library in SQLite
with full-text search as you type, a lyrics editor with live slide preview,
OpenLyrics import and export, and Bibles (OSIS and Zefania) looked up by
reference in English or Spanish. Songs and passages go into a service list
(reorder by keyboard or drag, notes and a theme per item) that is saved
as an `.asaf-service` file and autosaved continuously, so a crash or restart
brings it back. Separate preview and live panes; a whole service runs from
the keyboard: ⏎ puts the cued item live and cues the next, arrows step
through slides, B / ⇧B / L blank to black, background or logo. Themes (font,
size range, weight, colours, outline, shadow, alignment, margins, background
colour or image, transition) are edited in the console with a preview and
stored in the library; output uses fonts bundled into the app, so it looks
the same on every machine. An OpenLP data folder (songs and Bibles) imports
in one step. Settings (projector display, port, default theme, English or
Spanish console) are saved; `packaging/macos/package.sh` builds `Asaf.app`.
A phone can run the service from the same server's remote page (service
list, slides, next/previous/blank, verse lookup; optional PIN; LAN access is
opt-in), and two more outputs render from the same state: a stage view for
the worship team (current and next slide, clock) and an OBS lower-thirds
page with a transparent background.

Licensed GPL-3.0-or-later. Provided as-is, with no support.

## Run

```sh
cargo run                              # console + projector (second display if present)
cargo run -- import ~/Songs/OpenLyrics # import OpenLyrics files or folders
cargo run -- export ~/Desktop/songs    # export every song as OpenLyrics
cargo run -- import-bible kjv.osis.xml rv1909.xml   # import OSIS / Zefania Bibles
cargo run -- bibles                    # list installed Bibles
cargo run -- remove-bible RV1909       # remove one (by abbreviation or name)
cargo run -- import-openlp ~/Library/Application\ Support/openlp/Data   # migrate from OpenLP
cargo run -- themes                    # list themes (the default is marked)
cargo test --workspace                 # all tests, including headless console UI tests
cargo run -p asaf-server --example remote_demo -- 4611 [pin]   # remote/stage/OBS pages with a sample service, no GUI
packaging/macos/package.sh [--zip]     # release build of Asaf.app (see "Packaging")
```

- The library lives at `~/Library/Application Support/Asaf/asaf.sqlite` on
  macOS (the OS data dir elsewhere), with imported theme images in `media/`
  next to it. `ASAF_DATA_DIR=/some/dir` uses another folder. A new library
  starts with one public-domain hymn and the Default theme.
- The current service is autosaved next to it as `autosave.asaf-service` and
  restored on every start (a damaged one is renamed `….corrupt`, or
  `….corrupt-2` and so on, and reported, never deleted).
- Display page: <http://127.0.0.1:4510/display> (add `?debug` for an overlay
  with socket state, revision and change-to-paint latency). Works as an OBS
  browser source.
- Stage view: <http://127.0.0.1:4510/stage>. OBS lower thirds:
  <http://127.0.0.1:4510/obs>. Remote: <http://127.0.0.1:4510/remote> (from
  a phone, the LAN address the console shows; see "Remote, stage view and
  OBS").
- The port comes from Settings (default 4510); `ASAF_PORT=5000 cargo run`
  overrides it.
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
4. **Live**: what's on the projector, slide by slide, its theme, the blank
   state, the projector display picker, Themes… / Settings…, and (with LAN
   access on) the address phones open for the remote.

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
| `⌘T` | anywhere | theme editor (on the cued item's theme, if it has one) |
| `⌘,` | anywhere | settings |
| `⌘S` | song editor | save and close |
| `⌘S` | theme editor | save the theme |

Live-control keys never fire while a text field has focus, so typing "b" in
search doesn't blank the screen and `⌘⌫` in a field deletes text, not a
service item. Clicking a service item cues it, double-clicking puts it live;
clicking a slide puts that item live at that slide (in the live pane: jumps
to it). Double-clicking a song opens the editor, a Bible hit goes live.

Running a service: prepare it with `⇧⏎` from the source panel, `Esc`, then
`⏎` (first item live), `→`… through its slides, `⏎` (next item), and so on.
An ad-hoc verse (`⌘2`, type, `⏎ ⏎`) doesn't disturb the cue: the next `⏎`
carries on with the service.

### Themes

A theme sets the font (one of the bundled ones, Source Sans 3, Montserrat,
Oswald and Lora, or any font installed on the machine showing the output),
the weight, the auto-fit size range and line spacing (in % of the screen
height), text colour, outline colour and width, shadow colour, blur and
offset, horizontal and vertical alignment, side and top/bottom margins, the
background colour and image, the logo shown by "blank to logo", and the
transition between slides (fade, slide up or cut) and its duration. Colours
are `#rrggbb` or `#rrggbbaa` (alpha).

The theme editor (`⌘T`, or Themes… in the live pane) lists the themes on the
left (New, Duplicate, Delete…, Make default), the settings in the middle and
an approximate preview on the right: the sample chorus on the theme's
background and image in its font, colours, alignment and margins, sized
roughly like the display's auto-fit, with the outline and shadow drawn as
offset copies of the text, plus a small "blanked to the logo" view. Fields
that can't be used turn red and block saving. `⌘S` saves; if the theme is on
screen the projector changes at once. Switching themes or closing with
unsaved changes asks first. Background and logo images are copied into the
data folder's `media/` (named by a hash of their contents, so re-importing
the same file reuses it) and served by the local server; the original file
can be moved or deleted afterwards. "Built in" goes back to the bundled
placeholder images.

The **default theme** (chosen in the editor or in Settings) is used for
everything without a theme of its own: ad-hoc songs and verses, and service
items with no theme. A service item's own theme is chosen with the chips
under its notes (or typed by name); it goes live together with the item's
first slide, in one display update (changing the theme of the item already
on screen applies the next time it goes live). Renaming a theme renames it
in the open service; deleting it puts those items back on the default. A name that
doesn't match a theme (a service from another machine) shows a warning and
uses the default. The default theme can't be deleted. The preview, live and
library slide cards use the theme's background colour, text colour, font
and alignment.

### Settings

Settings (`⌘,`, Settings… in the live pane, or the Asaf menu), all saved in
the library as soon as they change:

- **Language**: English or Español for the console. It switches at once.
- **Default theme**.
- **Projector display**: chosen in the live pane, remembered by the
  display's UUID across restarts, and the projector returns to it when it's
  plugged back in.
- **Display server port**: used from the next start (`ASAF_PORT` overrides it).
- **Remote control**: "Allow phones and tablets on this network" (LAN
  access, from the next start) and the remote's **PIN** (4 to 8 digits, or
  empty for none; applies at once, and remotes that used the old PIN must
  enter the new one). With LAN access on and no PIN, the window warns that
  anyone on the network can control the projector.
- **Migrate from OpenLP**: "Import from OpenLP…" (also in the Asaf menu).

### Remote, stage view and OBS

All three are pages served by the app's own server (no install, no build
step) and follow the same state as the projector.

- **Remote** (`/remote`), for a phone. The live item's slides (tap one to
  show it); the service (live item ●, the next one outlined; tap an item,
  then "Go live", or "Next item ▶"); Bible lookup (translation, reference,
  "Look up" previews it on the phone, "Go live" puts it on screen, ad hoc,
  like ⌘⏎ in the console); and always at the bottom ◀ / ▶ and Hide, Logo,
  Black (pressed again: show the text). With a PIN set, it asks for it
  once and remembers it on that phone. Everything it does goes through the
  same store as the console's keys, so the console, every display and other
  remotes follow; it never moves the operator's focus or preview.
- **Stage view** (`/stage`), for the worship team's monitor: the current
  slide large, the next one under it (at the end of an item, the next
  service item's title, or "End"), the slide label and a clock. It's plain
  white on black whatever the theme, and keeps showing the text while the
  projector is blanked, with a BLACK / TEXT HIDDEN / LOGO badge.
- **OBS** (`/obs`), as a browser source over the camera: the text as lower
  thirds in the theme's font, colours, outline and shadow, on a transparent
  page with a soft shade behind it; no background, and any blank simply
  hides the text. Set the source to the stream's size (e.g. 1920×1080).

**LAN access is off by default**: the server listens on 127.0.0.1 only, so
nothing on the network can reach it. Settings → "Allow phones and tablets on
this network" makes it listen on every interface (0.0.0.0) from the next
start; the live pane then shows the address to open on a phone
("Remote for phones: http://192.168.1.8:4510/remote") and so does the
terminal. All the pages (display, stage, OBS, theme images) are then
visible on the network; only the remote's socket can change anything, and
that is what the PIN guards.

Protocol, for other clients: the remote talks to `/remote/ws`, separate
from the displays' `/ws`.

```text
server: {"type":"hello","pin":true}                 pin: whether one is needed
client: {"type":"auth","pin":"2468"}
server: {"type":"auth","ok":true}                   or ok:false + error, then the socket closes
server: {"type":"state","state":{…}}                on connect (once in) and on every change
client: {"type":"cmd","id":7,"cmd":"next"}
server: {"type":"result","id":7,"ok":true,"outcome":"done"}
```

Commands: `next`, `previous`, `goto` (`slide`), `blank` (`mode`: `None`,
`Black`, `Background`, `Logo`), `toggle_blank` (`mode`), `item` (`item`,
optional `slide`, 0-based), `verse` (`reference`, optional `translation`
abbreviation and `live`); a `verse` result carries `"outcome":"passage"`,
`title` and `slides`. A failed command answers `"ok":false` with an
`error` and the socket stays open. The state is the live item (`title`,
`slides`, `current`, `blank`) and the console's view (`service`, `items`
with `title` and `kind`, `cued`, `live`, `bibles`, `bible`), with a
`revision` that only grows. The display state (`/ws`) also carries `next`
(the next slide) and `up_next` (the next service item's title), for the
stage view.

### Migrating from OpenLP

Point Asaf at an OpenLP data folder (on macOS
`~/Library/Application Support/openlp/Data`, on Windows `%APPDATA%\openlp\data`)
with "Import from OpenLP…" or `asaf import-openlp <dir>`; the folder above it,
or the `songs` or `bibles` folder alone, works too. Every song in
`songs/songs.sqlite` and every Bible in `bibles/*.sqlite` is imported in one
step; OpenLP's files are opened read-only and never changed.

- Songs keep title, alternate title, authors (each once, whatever their
  role), copyright, CCLI number, verse order and verse tags (`v1`, `c1`,
  `b1`, `p1`, `i1`, `e1`, `o1`); `[---]` optional splits stay slide splits.
  OpenLP formatting tags (`{r}…{/r}`, `{br}`) and inline chords (`[G]`,
  `[D/F#]`) are removed. Temporary songs (copies OpenLP makes from opened
  service files) are skipped, and so is a song already in the library with
  the same title and lyrics, so running the import twice is harmless. A song
  whose lyrics can't be read is reported and the rest still import.
- Bibles keep their name, copyright and book names (which then also work as
  typed references, e.g. `San Juan 3:16`); the abbreviation is made from the
  name (King James Version → KJV, Reina-Valera 1909 → RV1909) since OpenLP
  doesn't store one, and numbered if another translation already uses it
  (Reina Valera Antigua → RVA, then Reina Valera Actualizada → RVA2); the language comes from OpenLP's language id or the
  book names. Deuterocanonical books are skipped and reported, as are web
  Bibles (OpenLP downloads their text on demand, so it isn't in the file)
  and translations already installed.
- Not imported: song books and numbers, topics, comments, OpenLP themes,
  media files and services.

The importer follows OpenLP's schemas (`openlp/plugins/songs/lib/db.py`,
`openlp/plugins/bibles/lib/db.py`): the tests build databases from SQL
mirroring them, for OpenLP 3 and for OpenLP 2.0 song databases (before the
`temporary` column and author roles), with public-domain hymns and KJV /
Reina-Valera 1909 verses (`crates/library/tests/fixtures/openlp`).

### Packaging

`packaging/macos/package.sh` builds a release binary and wraps it in
`target/release/Asaf.app` (Info.plist from `packaging/macos/Info.plist.in`
with the workspace version, an `.icns` made from the placeholder
`packaging/macos/icon.png` (its source is `icon.svg`), the fonts' OFL
licenses in `Contents/Resources/Licenses`), signs it ad hoc so it runs on
the machine that built it, and with `--zip` also packs
`target/release/Asaf-<version>-macos.zip`. The bundle identifier is
`local.asaf.Asaf` unless `ASAF_BUNDLE_ID` says otherwise.

Out of scope: Developer ID signing, notarization and a DMG. Copied to another
Mac, the ad-hoc signed app is blocked by Gatekeeper until it's allowed under
System Settings → Privacy & Security (or with `xattr -dr
com.apple.quarantine Asaf.app`). Signing for distribution needs an Apple
Developer account: `codesign --options runtime --sign "Developer ID
Application: …"`, then `xcrun notarytool submit` and `xcrun stapler staple`.
Other platforms aren't packaged yet.

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

## Localization

The console's text is English in the code and translated from catalogs in
[duckalization](https://github.com/soska/duckalization)'s format: the English
text is the message, and its ID is a hash of that text.

| Call | For |
| --- | --- |
| `l.t("Songs")` | plain text |
| `en("Background image")` | text translated later through a variable (`l.t(label)`) |
| `l.f("Added {title} to the service", &[("title", &name)])` | text with `{placeholders}`, filled after translating |
| `l.n(count, "{count} song", "{count} songs")` | plurals (`{count}` is filled) |
| `l.tc("Book", "verb")` | the same text with different meanings |
| `__("Connect")` in `display/*.js` | the web pages (not wired up yet) |

Arguments must be string literals so the extractor can find them.

- `locales/en.json` and `en.meta.json` (every message, with where it's used)
  are written by `cargo run -p asaf-i18n --bin i18n-extract`. A test fails
  when they're out of date. **Don't run `duckalize extract`** here: it only
  reads JS/TS and would empty the catalog.
- `locales/es-MX.json` is the Spanish (Mexico) catalog, built into the
  binary. Tests in `asaf-i18n` fail on a missing translation, a translation
  of a message that no longer exists, or changed placeholders. A missing one
  shows English.
- `locales/glossary.json` fixes how Asaf's terms are translated (servicio,
  proyectar, diapositiva…) and which names stay as they are (OpenLP, PIN…);
  `locales/style/es-MX.md` is the style guide (Mexican Spanish, tú, short
  labels). Briefs include both, and `translate lint` warns when a
  translation doesn't use a glossary term.

After changing the console's text, from the repo root (Node is only needed
for translating):

```sh
cargo run -p asaf-i18n --bin i18n-extract
npx @duckalization/cli@0.1.0 translate status
npx @duckalization/cli@0.1.0 translate brief       # → locales/.work/es-MX.brief.json
# an agent translates the brief (see duckalization's llms.txt) into es-MX.out.json
npx @duckalization/cli@0.1.0 translate apply locales/.work/es-MX.out.json --by claude
npx @duckalization/cli@0.1.0 translate lint        # glossary, placeholders
npx @duckalization/cli@0.1.0 translate prune       # drop translations of removed text
npx @duckalization/cli@0.1.0 review status         # machine / approved / edited
```

Settings are in `duckalization.config.json` (its `include` is empty on
purpose, since the Rust extractor writes the catalog).

## Layout

| Crate | Role |
| --- | --- |
| `crates/core` (`asaf-core`) | Song/verse/slide types, lyrics text format, slide generation and splitting, Bible types and the 66-book table with English/Spanish names, the reference parser, verse-to-slide layout, the service model and its JSON file format (`service.rs`), the `Theme` model and its CSS variables (`theme.rs`), `Settings` (`settings.rs`), `DisplayState`, the command API and the remote's view (`remote.rs`: `Command`, `Outcome`, `RemoteView`, `RemoteState`), the `Store` (single source of truth, default and per-item theme, commands and their operator, change listeners for displays and remotes). No UI or IO deps (serde/serde_json only). |
| `crates/library` (`asaf-library`) | SQLite storage (rusqlite, bundled SQLite), migrations via `PRAGMA user_version`, FTS5 search indexes (songs, Bible verses), OpenLyrics and Bible import with duplicate skipping, passage lookup. Themes and settings (`themes.rs`), theme images in the media folder (`media.rs`), the OpenLP importer (`openlp.rs`). Service files on disk (`service.rs`): atomic writes, reading, autosave restore, finding a service's songs. |
| `crates/fonts` (`asaf-fonts`) | The bundled OFL fonts (Source Sans 3, Montserrat, Oswald, Lora; Google Fonts' variable TTFs) and their licenses, embedded with `include_bytes!`, plus their `@font-face` CSS. |
| `crates/bible` (`asaf-bible`) | OSIS and Zefania XML readers (parsing only; depends on `core`). Test fixtures are public-domain excerpts (KJV, Reina-Valera 1909). |
| `crates/openlyrics` (`asaf-openlyrics`) | OpenLyrics XML read (0.8 and 0.9) and write (0.9), single files and folders. |
| `crates/server` (`asaf-server`) | axum on 127.0.0.1 (or 0.0.0.0 with LAN access): display, OBS, stage and remote pages, embedded assets, bundled fonts (`/fonts/…`, `/assets/fonts.css`), imported theme images (`/media/…`), `/ws` pushing `{"type":"state","state":…}` on connect and on every change, and the remote's PIN-protected command socket `/remote/ws` (`remote.rs`). `examples/display_demo.rs` serves one slide in a theme from a JSON file, `examples/remote_demo.rs` the remote, stage and OBS pages with a sample service (and a stand-in console), for checking output in a browser. |
| `display/` | `index.html`, `display.css`, `display.js`, `background.svg`, `logo.svg` (placeholder); the OBS variant (`obs.html`, `obs.css`, same `display.js`); the stage view (`stage.html`, `stage.css`, `stage.js`); the remote (`remote.html`, `remote.css`, `remote.js`). No build step, no framework; embedded into the binary with `include_str!`. |
| `crates/i18n` (`asaf-i18n`) | Localization in duckalization's format: message IDs (`id.rs`, the same hash as `@duckalization/id`), target catalogs and lookups (`catalog.rs`), plural rules (`plural.rs`), and the extractor that writes `locales/en.json` + `en.meta.json` (`extract.rs`, `bin/i18n-extract.rs`). No UI deps. |
| `crates/projector` (`asaf-projector`) | Projector window: a GPUI window hosting a wry webview, placed on the chosen display; watches for display hotplug. |
| `crates/console` (`asaf-console`) | GPUI operator UI: song library and Bible mode (`bible.rs`) as sources, the service list with files and autosave (`service.rs`), preview and live panes with slide cards (`slides.rs`), song editor, theme editor (`theme_editor.rs`) and its approximate preview (`preview.rs`), settings (`settings.rs`), the console's language (`i18n.rs`: `Lang` over `asaf-i18n` catalogs), import/export and OpenLP migration (`transfer.rs`), and the remote's operator (`remote.rs`: runs service-item and verse commands, publishes the service). Never draws projected output. |
| `crates/app` (`asaf`) | Binary: opens the library, restores the autosaved service, wires store, server (port, LAN access, PIN and media folder from the settings; a new PIN reaches it at once), projector (remembered display) and console (the LAN address of the remote); `import`/`export`/`import-bible`/`bibles`/`remove-bible`/`import-openlp`/`themes` subcommands. |
| `packaging/macos` | `package.sh` (builds `Asaf.app`), `Info.plist.in`, the placeholder icon. |

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
  service lives in the console (not the core store); the console publishes
  a summary of it to the store for the remote (see "Commands" below).
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
  centred over its background, with the text's drop shadow. The default
  logo is a placeholder (`display/logo.svg`); themes can use any image.
- **Themes are JSON in SQLite** (migration 3: `themes`, keyed by a
  case-insensitive unique name, and `settings`, key/value rows). A stored
  theme missing fields (written by an older version) or with values out of
  range is read with defaults / clamped (`Theme::sanitized`), so a damaged
  row can't break the output. Service items refer to themes by name, as the
  service file already did.
- **The theme reaches the display as CSS variables**: `Theme::css_vars` in
  `asaf-core` turns it into `--color`, `--shadow`, `--valign`… (table-driven
  tests), `DisplayState.css_vars` carries them and `display.js` only sets
  them on `:root`. A cut is a `0ms` transition; "slide up" sets `--shift`,
  the distance a new slide rises (the old one leaves upward). Fonts are
  served by the app, never fetched from the internet; theme images must be
  local URLs (`/media/…` or `/assets/…`).
- **Default and per-item theme live in the `Store`.** `load_song_themed` /
  `load_passage_themed` change the slide and the theme in one revision, so
  the projector never shows the new slide in the old theme; editing the
  default theme only notifies displays if it's the one on screen.
- **Bundled fonts**: four families from Google Fonts' OFL set, as variable
  TTFs (1.8 MB in the binary), chosen for Latin/Spanish coverage and to cover
  sans, condensed and serif looks. The console registers them with GPUI so
  its previews use the same fonts.
- **Settings are applied when they change** and need no Save button; the
  port is the exception (it applies from the next start).
- **The console language** uses duckalization's catalogs (keyed by a hash
  of the English text, `lang(cx).t("Songs")`) with a Rust extractor and
  lookup instead of its JS tooling, which can't read Rust; its CLI only
  reads the JSON files, so translating, glossary and review work unchanged.
  See "Localization".
- **resvg is optimised in dev builds** (`[profile.dev.package.resvg]` and its
  rendering crates): unoptimised, the console's SVG background previews took
  seconds per frame.
- **Notes** are for the operator only: shown under the item in the service
  list and above its slides in the preview, never projected.
- **Display choice** is remembered by display UUID (and saved in the
  settings), so the projector returns to the chosen display when it's
  plugged back in or the app restarts. Without a second display the
  projector is a normal window on the main one.
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

- **Commands** (`asaf_core::Command`) are the one way front ends change
  what's live: the console's live-control keys and buttons and the remote
  both call `Store::command`. Slide and blank commands only touch what is
  live, so the store applies them itself (from whichever thread, the
  server's included). A service item or a verse needs the library and the
  service, which live in the console, so the store hands those to its
  *operator*: the console registers one at start, and it forwards each
  command over a channel to a task on the console's thread, which puts the
  result live through the store like ⏎ would (without moving the
  operator's focus or preview) and sends the answer back. No operator (the
  console closed) is an error, not a hang; the server also gives up after
  5 s. The console publishes a `RemoteView` (service name and items, cue,
  live item, translations) whenever any of it may have changed, and redraws
  on every store change it didn't make (coalesced, one redraw per burst).
- **The remote has its own socket** (`/remote/ws`), never the displays'
  (`/ws`, which stays read-only), as the PRD's protocol says. Its state is a
  separate `RemoteState` with its own listener list, so service changes
  don't wake the displays, except when the stage view's "up next" changes
  (then the display revision bumps too, as a separate revision after the
  slide: the slide and its theme still arrive together).
- **The PIN** is checked on the remote socket only: the first message must
  be `auth` with the right PIN, or the socket is told why and closed. Wrong
  PINs are answered after 1 s, one at a time across all connections, so a
  4-digit PIN takes hours to guess. A changed PIN disconnects remotes that
  used the old one at their next command or state update. The socket also
  refuses browser connections whose `Origin` isn't the page's own host, or
  whose host is a public name rather than an IP, a bare name (`localhost`)
  or a `.local` name, so another web site open in the operator's browser
  can't drive the projector through 127.0.0.1, even by DNS rebinding.
  Remotes remember the PIN in the phone's `localStorage`.
- **Each remote's commands run one at a time, in order**: a command waits
  for the one before it to finish, so "Go live" followed quickly by "Next"
  moves within the new item, not the old one. Commands from different
  remotes and the console aren't ordered against each other.
- **LAN access is opt-in and applies at start** (the PRD's "localhost only
  by default"): the bind address is chosen once. The address shown is the
  one the OS would use to reach the internet (a UDP socket "connected" to a
  TEST-NET address; nothing is sent), so with several networks it shows one.
- **The stage view ignores the theme**: it's for reading, white on black,
  and fits text with the display's algorithm in its own boxes. **The OBS
  page keeps the theme's look but not its size range** (3–6.5% of the
  height, lower-third band, theme margins left and right) and ignores
  vertical alignment and the background; the band and the shade are in
  `obs.css`.
- **The remote shows the live item's slides from the store** and the
  service as titles only; it can't preview a service item before it goes
  live (tap → "Go live" is the safety step instead).

## Known gaps

- **OpenLP remote API (the PRD's stretch goal) is not done**: mimicking it
  so OpenLP's remote apps can connect needs a license and documentation
  review of that API first.
- The remote page is English only (the console's language setting doesn't
  reach it), and it can't edit the service (add, reorder, notes, themes) or
  search Bible text by words: references only.
- The remote is plain HTTP/websocket: on the LAN, the PIN and commands are
  not encrypted. The PIN is stored in the library as plain text. Use it on a
  trusted network.
- The stage clock is the clock of the device showing the page, in its
  locale's format.

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
  on restart (they aren't part of the Phase 4 settings). Fit-to-screen
  estimates from the default theme, even for a passage with its own theme.
- The theme preview is an approximation: GPUI has no text stroke or blur, so
  the outline is drawn as eight offset copies of the text and the shadow as
  one translucent copy, and the size follows a character-width estimate,
  not the display's measured auto-fit.
- Spanish covers the console's labels, buttons, hints and the theme and
  settings windows; the song editor is still English. Messages built from
  other parts (Bible reference errors and hints, import reports, file dialogs' questions, notifications such as
  "Added … to the service") and the Asaf menu are still English; input
  placeholders switch on the next start.
- The OpenLP importer doesn't bring song books, topics, comments, media,
  OpenLP themes or services, and web Bibles can't be imported.
- Theme images are never removed from `media/`, even when no theme uses them.
- Packaging is macOS only, signed ad hoc (see "Packaging").
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
