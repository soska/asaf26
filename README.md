# Asaf

A fast lyrics and Bible-verse projector: a Rust app with a GPUI operator
console and a web-based display. See `Rust Lyrics Projector — PRD.md`.

**Status: Phase 0 (proof of concept).** One hardcoded public-domain hymn
("Blessed Assurance"), Previous / Next / Blank, auto-fitted outlined text over
a generated background, and a projector window on the second display.

Licensed GPL-3.0-or-later. Provided as-is, with no support.

## Run

```sh
cargo run            # console + projector (second display if present)
cargo test           # core slide/store tests + server websocket tests
```

- Display page: <http://127.0.0.1:4510/display> (add `?debug` for an overlay
  with socket state, revision and change-to-paint latency). Works as an OBS
  browser source.
- `ASAF_PORT=5000 cargo run` changes the port.
- `ASAF_PROJECTOR_ON_MAIN=1 cargo run` puts the fullscreen projector on the
  main display when there is no second one (for testing; it covers the screen,
  quit with ⌘Q).

Console keys: `→` `↓` `Space` `PgDn` next · `←` `↑` `PgUp` previous ·
`B` blank to black · `Shift-B` hide text (keep background) · `Esc` unblank.

## Layout

| Crate | Role |
| --- | --- |
| `crates/core` (`asaf-core`) | Song/verse/slide types, slide generation, `DisplayState`, the `Store` (single source of truth, change listeners). No UI or IO deps. |
| `crates/server` (`asaf-server`) | axum on 127.0.0.1: display page, embedded assets, `/ws` pushing `{"type":"state","state":…}` on connect and on every change. |
| `display/` | `index.html`, `display.css`, `display.js`, `background.svg`. No build step; embedded into the binary with `include_str!`. |
| `crates/projector` (`asaf-projector`) | Projector window: a GPUI window hosting a wry webview, placed on the chosen display; watches for display hotplug. |
| `crates/console` (`asaf-console`) | GPUI operator UI. Mutates the store; never draws projected output. |
| `crates/app` (`asaf`) | Binary: wires the store, server, projector and console. |

## Decisions and deviations from the PRD

- **Why GPUI is pinned.** `gpui = "=0.2.2"` from crates.io, exactly. GPUI's
  API changes between releases without deprecation, so any upgrade should be a
  deliberate change. GPUI code lives only in `console` and `projector`.
- **`runtime_shaders` feature.** GPUI normally compiles its Metal shaders at
  build time, which needs Xcode's separately downloaded Metal Toolchain
  (`xcodebuild -downloadComponent MetalToolchain`). Compiling them at startup
  instead keeps `cargo run` working on a stock Xcode install. It can be dropped
  for release builds.
- **No tao.** GPUI and tao both want to own the macOS main event loop, but
  GPUI windows expose a raw window handle, so the projector is a GPUI window
  with a wry webview attached as a child view (`build_as_child`). It shares
  one process and one event loop; the separate-process fallback from the PRD
  wasn't needed. On macOS the window is then made borderless at window level
  25 (above the menu bar and Dock) and sized to its screen.
- **`console` depends on `projector`** (for the display picker) as well as
  `core`.
- **Display choice** is remembered by display UUID in memory, so the projector
  returns to the chosen display when it's plugged back in. Without a second
  display the projector is a normal window on the main one.

## Known gaps (Phase 0)

- Clicking the projector window gives the webview keyboard focus; operate from
  the console.
- Borderless fullscreen is implemented for macOS only; other platforms get a
  titlebar-less GPUI window on the display.
- A slide too long to fit at the minimum size wraps and then clips. Phase 1's
  slide splitting addresses this.
