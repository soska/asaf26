// Asaf display page: renders whatever DisplayState the server pushes.
// No build step, no framework. Never sends commands (the remote has its own
// endpoint). Also drives the OBS variant (/obs, <html class="obs">) and the
// console's previews (/display?preview), which get their state from the
// console instead of the server.
(() => {
  "use strict";

  const root = document.documentElement;
  const layers = [document.getElementById("layer-a"), document.getElementById("layer-b")];
  const black = document.getElementById("black");
  const logo = document.getElementById("logo");
  const debugEl = document.getElementById("debug");
  const params = new URLSearchParams(location.search);
  const debug = params.has("debug");
  const preview = params.has("preview");
  root.classList.toggle("preview", preview);
  // Lower thirds: smaller text, in the band obs.css sets, whatever the theme's size range.
  const obs = root.classList.contains("obs");
  const OBS_SIZE_VH = { min: 3, max: 6.5 };
  debugEl.hidden = !debug;

  let state = null;
  let active = 0; // index into layers of the layer holding the shown slide
  let shownKey = null;
  let connected = false;
  let lastLatency = null;

  // ---- theme ----------------------------------------------------------------

  // The server sends the theme already turned into CSS custom properties
  // (asaf_core::Theme::css_vars); display.css does the rest.
  let appliedVars = null;
  function applyTheme(cssVars) {
    const key = JSON.stringify(cssVars);
    if (key === appliedVars) return false;
    appliedVars = key;
    for (const [k, v] of cssVars) root.style.setProperty(k, v);
    return true;
  }

  // ---- auto-fit -------------------------------------------------------------

  // Largest font size in [min, max] at which the text fits the safe area.
  // Lyric lines are kept whole when possible and wrap only at the minimum
  // size; prose (Bible verses, `slide.wrap`) always wraps.
  function fit(layer) {
    if (!state) return;
    const text = layer.querySelector(".text");
    const safe = layer.querySelector(".safe");
    if (!text.firstChild) return;
    const vh = window.innerHeight / 100;
    const min = (obs ? OBS_SIZE_VH.min : state.theme.size_min_vh) * vh;
    const max = (obs ? OBS_SIZE_VH.max : state.theme.size_max_vh) * vh;
    const w = safe.clientWidth;
    const h = safe.clientHeight;
    const fits = (px) => {
      text.style.fontSize = `${px}px`;
      return text.offsetWidth <= w && text.offsetHeight <= h;
    };
    const search = () => {
      let lo = min, hi = max;
      if (fits(hi)) return hi;
      if (!fits(lo)) return null;
      while (hi - lo > 0.5) {
        const mid = (lo + hi) / 2;
        if (fits(mid)) lo = mid; else hi = mid;
      }
      return lo;
    };

    const prose = text.classList.contains("prose");
    text.classList.toggle("wrap", prose);
    let size = search();
    if (size === null && !prose) {
      text.classList.add("wrap");
      size = search();
    }
    text.style.fontSize = `${size ?? min}px`; // still too long: minimum size, overflow clipped
  }

  let refitQueued = false;
  function refitSoon() {
    if (refitQueued) return;
    refitQueued = true;
    requestAnimationFrame(() => {
      refitQueued = false;
      layers.forEach(fit);
    });
  }
  window.addEventListener("resize", refitSoon);
  if (document.fonts) {
    document.fonts.ready.then(refitSoon);
    document.fonts.addEventListener?.("loadingdone", refitSoon);
  }

  // ---- render ---------------------------------------------------------------

  function fill(layer, slide) {
    const text = layer.querySelector(".text");
    text.replaceChildren(
      ...slide.lines.map((line) => {
        const div = document.createElement("div");
        div.textContent = line;
        return div;
      }),
    );
    text.classList.toggle("prose", !!slide.wrap);
    layer.querySelector(".footer").textContent = slide.footer ?? "";
    fit(layer);
  }

  // `live` is false for the state sent on connect, which may be old.
  function render(next, live) {
    state = next;
    // A new font, size range or margin changes what fits: refit both layers.
    if (applyTheme(state.css_vars ?? [])) refitSoon();

    const slide = state.slide;
    const key = slide ? JSON.stringify(slide) : null;
    if (key !== shownKey) {
      // Prepare the hidden layer in its waiting place (no animation), then
      // transition to it while the other leaves.
      active = 1 - active;
      const incoming = layers[active];
      incoming.classList.add("instant");
      incoming.classList.remove("on", "out");
      if (slide) fill(incoming, slide);
      else incoming.querySelector(".text").replaceChildren();
      void incoming.offsetWidth; // commit the waiting position before animating
      incoming.classList.remove("instant");
      layers[1 - active].classList.add("out");
      shownKey = key;
    }

    const showText = !!slide && state.blank === "None";
    layers[active].classList.toggle("on", showText);
    layers[1 - active].classList.remove("on");
    black.classList.toggle("on", state.blank === "Black");
    logo.classList.toggle("on", state.blank === "Logo");

    if (!live) {
      updateDebug();
      return;
    }
    // Server and display share a clock on the projector machine, so this is
    // core change -> websocket -> next painted frame.
    requestAnimationFrame(() => {
      lastLatency = Date.now() - state.changed_at_ms;
      if (debug) console.log(`asaf: revision ${state.revision} painted after ${lastLatency} ms`);
      updateDebug();
    });
  }

  function updateDebug() {
    if (!debug) return;
    debugEl.textContent = [
      `socket: ${connected ? "connected" : "reconnecting…"}`,
      `revision: ${state?.revision ?? "-"}`,
      `slide: ${state?.slide?.label ?? "-"}  blank: ${state?.blank ?? "-"}`,
      `last change to paint: ${lastLatency ?? "-"} ms`,
    ].join("\n");
  }

  // ---- socket ---------------------------------------------------------------

  // ---- preview --------------------------------------------------------------

  // The console's webview calls asafPreview(state) with a draft theme, and
  // hears "ready" (send the state now) and "pointer" (a click took keyboard
  // focus; give it back) through wry's IPC.
  function startPreview() {
    const post = (msg) => window.ipc?.postMessage(msg);
    window.asafPreview = (next) => render(next, false);
    document.addEventListener("pointerdown", () => post("pointer"));
    post("ready");
  }

  let retryMs = 250;
  function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws`);
    let first = true;
    ws.onopen = () => {
      connected = true;
      retryMs = 250;
      updateDebug();
    };
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.type === "state") render(msg.state, !first);
      first = false;
    };
    ws.onclose = () => {
      // Keep showing the last slide while we try again.
      connected = false;
      updateDebug();
      setTimeout(connect, retryMs);
      retryMs = Math.min(retryMs * 2, 2000);
    };
    ws.onerror = () => ws.close();
  }
  if (preview) startPreview();
  else connect();
})();
