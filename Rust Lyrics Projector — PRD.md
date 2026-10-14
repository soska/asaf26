# Asaf — PRD

Oct 13, 2026 · @Armandito

## Overview

Asaf is a fast, good-looking replacement for OpenLP that only projects song lyrics and Bible verses. It is a Rust app with a GPUI operator console and a web-based display. Phase 0 is a proof of concept that tests that architecture before any real features are built.

**Problem.** OpenLP (Python + Qt) works, but it is slow, buggy and visually dated. Most of its feature surface goes unused: we only need lyrics and verses.

**Goals**

- Snappy operator console: search, select and go live with no perceptible lag.
- Projected output that looks modern: clean typography, outlined or shadowed text, smooth transitions.
- Reliable on Sunday: no crashes, no lost services, predictable second-monitor behavior.
- Easy migration from an existing OpenLP install (songs and Bibles).
- One display renderer reused for the projector, livestream (OBS), stage monitor and remote.

**Non-goals**

- Legacy song formats (EasyWorship, SongShow Plus, ProPresenter, etc.). OpenLyrics and OpenLP's own database are the only imports.
- Presentations (PowerPoint, LibreOffice), images-as-slides, media playback items, custom slides, alerts, plugins.
- Online song services (CCLI SongSelect) and printing.
- Feature parity with OpenLP in general. If it is not lyrics or verses, it is out unless explicitly added later.

## Users and core workflows

The primary user is a volunteer operator running a church service from one laptop with a projector (or TV) as a second display. Secondary consumers are the congregation (projector), the livestream (OBS), the worship team (stage monitor) and, later, someone controlling slides from a phone.

1. **Prepare a service (before the service).** Search songs by title or lyrics, add them to the service list, look up Bible passages by reference and add them too. Reorder items. Save the service.
2. **Run a service (live).** Select an item, see its slides in the preview pane, send a slide live, step with arrow keys. Blank the screen (to black or to theme background) instantly.
3. **Ad-hoc verse.** The preacher calls out a verse that is not in the service. The operator types a reference (e.g. "Jn 3:16-18") and puts it live in a few seconds.
4. **Edit a song.** Fix a typo or change the verse order, often minutes before the service. Changes must be quick and safe.
5. **Manage themes.** Pick font, size limits, colors, outline/shadow, background (color or image), alignment and margins. Assign a default theme and per-item overrides.
6. **Migrate.** Point the app at an existing OpenLP data folder and import songs and Bibles in one step.

## Architecture

A single Rust process owns all state; the operator console changes it, and a local web server pushes it to a dumb HTML display page that every screen loads.

&#91;embedded content: architecture · core, server, console and display outputs\]

The console never draws the projected output itself: it sends commands to the core, and every output renders the same page from the same websocket state.

**Key decisions**

- **Web display, not GPUI, for output.** CSS handles outlined and shadowed text, transitions, background images and (later) video. It also makes OBS, stage view and remote nearly free.
- **wry for the projector window,** not a browser in kiosk mode, so the app controls which display it lands on and it has no browser chrome.
- **GPUI only for the operator console,** where its speed matters most: search as you type, long lists, keyboard-first control.
- **Localhost only by default.** The server binds to 127.0.0.1; LAN access is opt-in when remote arrives.
- **Console previews are approximations** drawn in GPUI. Embedding small webviews for pixel-exact previews is a later experiment, not a requirement.

## Phase 0: proof of concept

Phase 0 proves the architecture end to end with one hardcoded song and two buttons. It succeeds if pressing "next" in a GPUI window changes outlined, auto-fitted lyrics on a fullscreen second monitor, with a fade, in under 100 ms. No persistence, no editing, no Bible.

**Deliverables**

- A Cargo workspace that builds with `cargo run` on the developer's OS.
- **Core:** in-memory state holding one hardcoded song (3 verses + chorus, a verse order) and a current slide index.
- **Server:** axum on `127.0.0.1` with a fixed port, serving the display page and a websocket that pushes the full display state on connect and on every change.
- **Display page:** one static HTML/CSS/JS bundle (embedded in the binary). Renders the current slide with outlined text over a background image, auto-fits the font size, fades between slides, reconnects automatically if the socket drops.
- **Projector window:** a borderless fullscreen wry webview on a chosen display (second display if present, else a normal window on the primary), loading the display page.
- **Console:** a minimal GPUI window with Previous, Next and Blank buttons, keyboard shortcuts (arrows, B), a list of the song's slides with the live one highlighted, and a display picker.

**Acceptance criteria**

- [ ] `cargo run` opens the console and the projector window on the second monitor.
- [ ] Next/Previous/Blank update the projector within 100 ms, with a visible fade.
- [ ] Long and short slides both fill the safe area without overflow or tiny text (auto-fit works).
- [ ] Text stays readable over a busy background image (outline and shadow).
- [ ] Opening `http://127.0.0.1:<port>/display` in a normal browser or an OBS browser source shows the same output and stays in sync.
- [ ] Killing and reopening the browser tab reconnects and shows the current slide.
- [ ] Unplugging the second display does not crash the app.

**Explicitly out of Phase 0:** database, song editor, search, Bible, service list, themes UI, settings, packaging.

## Later phases

Each phase ends with something usable; Phase 3 is the first version good enough to run a real service. Phases 1 and 2 can run in parallel once Phase 0 is done.

### Phase 1: songs

- SQLite storage (rusqlite or sqlx) with migrations; FTS5 index over title, alternate title, author and lyrics.
- Song model with tagged verses (v1, c1, b1, p1, e1…) and an explicit verse order.
- Lyrics editor in GPUI: plain-text editing with `[v1]`-style tags, verse order field, live slide preview.
- Slide splitting: one verse per slide by default, optional manual split marker for long verses.
- Import and export OpenLyrics XML (single file and folder).
- **Done when:** 500 songs import, search returns as you type (under 50 ms), and edits survive restart.

### Phase 2: Bibles

- Bible storage in the same database (or one file per translation): book, chapter, verse, text.
- Importers: OSIS XML and Zefania XML.
- Reference parser: Spanish and English book names and common abbreviations, ranges (`Jn 3:16-18`), cross-chapter ranges (`Gn 1:30-2:3`), lists (`Sal 23; 91:1-4`), whole chapters. Configurable book-name table per translation.
- Verse-to-slide layout: N verses per slide or fit-to-screen, verse numbers optional, reference shown on the last slide or in a footer.
- Text search within a translation.
- **Done when:** typing a reference and pressing Enter shows the correct verses in preview in under a second, for both languages.

### Phase 3: service and live operation

- Service list: add songs and passages, reorder by drag and keyboard, per-item theme override, notes.
- Save and load services (a simple JSON or SQLite file, our own format).
- Preview and live panes with slide thumbnails, keyboard-first operation, blank to black / to background, "show logo" optional.
- Crash safety: autosave the current service; on restart, restore it.
- **Done when:** a full mock service (5 songs, 2 passages, 1 ad-hoc verse) runs without touching the mouse.

### Phase 4: themes and migration

- Theme model and editor: font family, size range, weight, color, outline, shadow, alignment, margins, background color or image, transition type and duration. Themes are serialized to CSS variables for the display page.
- Bundled fonts served by the local server so output looks the same on every machine.
- OpenLP importer: read `songs.sqlite` and Bible `.sqlite` files from an OpenLP data folder.
- Settings: display selection, port, default theme, language (Spanish and English UI).
- Packaging for the target OS.

### Phase 5: remote and extra outputs (optional)

- Remote web page served by the same axum server: service list, slide list, next/previous/blank, quick verse lookup. Optional PIN.
- Stage view: current and next slide, clock.
- OBS-friendly variant of the display page with a transparent background (lower-thirds).
- Stretch: mimic OpenLP's remote API so existing OpenLP remote apps can connect. Check that API's license and documentation first.

## Data model

The core types are deliberately small; everything the display needs is derived into a flat `DisplayState` that the server sends as JSON. Field lists are a starting point, not a contract.

| Type | Key fields | Notes |
| --- | --- | --- |
| `Song` | id, title, alt\_title, authors, copyright, ccli\_number, verses, verse\_order, theme\_id? | Stored in SQLite; lyrics also indexed in FTS5 |
| `Verse` | tag (`v1`, `c1`, `b1`…), lines | Tag = kind letter + number, as in OpenLyrics |
| `Bible` | id, name, abbreviation, language, book\_names | One row per installed translation |
| `BibleVerse` | bible\_id, book (1–66), chapter, verse, text | Primary key on (bible\_id, book, chapter, verse) |
| `Passage` | bible\_id, ranges: Vec<(start, end)> | Output of the reference parser |
| `Theme` | font, size\_min/max, color, outline, shadow, align, margins, background, transition | Serialized to CSS variables |
| `Service` | id, name, items | Saved as our own file format |
| `ServiceItem` | Song(id) or Passage, theme\_override?, notes |  |
| `Slide` | lines, label (e.g. "V1", "Jn 3:16"), footer? | Derived, never stored |
| `DisplayState` | slide?, blank: None/Black/Background, theme, transition, revision | What the websocket sends; `revision` increments on every change |

**Websocket protocol (v0):** the server sends `{"type":"state","state":DisplayState}` on connect and on every change. The display page never sends commands in Phase 0. In Phase 5 the remote sends `{"type":"cmd", ...}` messages on a separate authenticated endpoint.

## Risks, open questions and licensing

The biggest risks are GPUI's maturity and running a GPUI app and a wry webview in one process; Phase 0 exists to retire both early.

| Risk | Impact | Mitigation |
| --- | --- | --- |
| GPUI API churn; distributed via unofficial standalone or snapshot crates | Upgrades break the build | Pin an exact version or git rev; isolate GPUI code in the console crate |
| GPUI and tao/wry event loops in one process | Projector window may not run alongside GPUI | Try in Phase 0. Fallbacks: run the projector as a child process, or launch a browser in kiosk mode |
| Webview quality on Linux (WebKitGTK) | Jank on transitions or video | Keep the display page light; test on target hardware early |
| No stock GPUI widgets (text input, lists) | Editor work takes longer | Evaluate gpui-component in Phase 1 before building our own |
| Bible reference parsing edge cases | Wrong verses shown live | Table-driven tests with hundreds of real references in both languages |
| Second-display hotplug | Projector lands on the wrong screen or crashes | Re-enumerate displays on change; remember the chosen display by name |

**Open questions**

- Target OS for Phase 0 and first release (Windows, macOS, Linux)?
- Publish the source publicly, or keep the repo private and share source only with people who receive a copy?
- Which Bible translations will the church actually use, and in which formats are they available?
- Do we need a confidence/stage monitor before remote?

**Licensing**

- Decision: Asaf is licensed GPLv3 or later, like OpenLP. Reading and adapting OpenLP's code (reference parser, OpenLyrics handling) is allowed; credit OpenLP where code is adapted. Using Asaf privately creates no obligations; anyone who receives a copy must also be offered the source. If published, the repo is source-available with no support promise: issues and pull requests off, README says it is provided as-is.
- Reading OpenLP's data files to import them is fine either way.
- Many Spanish and English translations (Reina-Valera 1960, NVI, NTV and others) are copyrighted. Ship importers, not Bible texts, unless a translation is clearly public domain.
- Song lyrics are the church's own data and are never bundled.

## Notes for Claude Code

The app is called Asaf: name the binary asaf and prefix every crate with asaf- (asaf-core, asaf-server, asaf-console, and so on). Start with Phase 0 only, and stop to report after each acceptance criterion is met or blocked. Suggested workspace layout:

| Crate | Role | Depends on |
| --- | --- | --- |
| `core` | Domain types, slide generation, `DisplayState`, app state + change notifications | none (no UI, no IO frameworks) |
| `server` | axum HTTP + websocket, embedded display assets | `core` |
| `display/` (not a crate) | `index.html`, `display.css`, `display.js`; no build step, no framework | — |
| `projector` | tao + wry fullscreen window on a chosen display | `server` (URL only) |
| `console` | GPUI operator UI | `core` |
| `app` | Binary: wires state, starts server, opens projector and console | all of the above |

**Guidelines**

- Keep `core` free of GPUI, axum and wry so it can be unit-tested and reused by the remote.
- One source of truth: the console mutates state in `core`; the server observes it and broadcasts. Nothing else talks to the display.
- Display page: auto-fit by binary-searching `font-size` between the theme's min and max until the text fits the safe area; recompute on resize and font load. Outline with `-webkit-text-stroke` plus `paint-order: stroke fill`; fade with CSS opacity transitions on two stacked layers.
- Pin the GPUI dependency to an exact version or git revision and record why in the README.
- If GPUI and tao/wry cannot share a process, fall back to spawning the projector as a separate process of the same binary (e.g. `app --projector --url ...`) before considering anything else.
- Write tests for slide generation and, from Phase 2, for the reference parser.
- Phase 0 uses a hardcoded public-domain hymn (for example "Amazing Grace") as test content and a generated gradient as the background image.
