// Asaf display page: renders whatever DisplayState the server pushes.
// No build step, no framework. Never sends commands (the remote has its own
// endpoint). Also drives the OBS variant (/obs, <html class="obs">) and the
// console's webviews: its live monitor (/display?monitor, the real output)
// and the theme editor's previews (/display?preview, which get their state
// from the console instead of the server).
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
  const embedded = preview || params.has("monitor");
  root.classList.toggle("embedded", embedded);
  // Lower thirds: smaller text, in the band obs.css sets, whatever the theme's size range.
  const obs = root.classList.contains("obs");
  const OBS_SIZE_VH = { min: 3, max: 6.5 };
  debugEl.hidden = !debug;

  let state = null;
  let active = 0; // index into layers of the layer holding the shown slide
  let shownKey = null;
  let shownAreas = null;
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

  // A looping background video, when the theme has one. Muted, so it may
  // play without a click.
  const bg = document.getElementById("bg");
  function applyVideo(url) {
    let video = bg.querySelector("video");
    if (!url) {
      video?.remove();
      return;
    }
    if (video?.getAttribute("src") === url) return;
    if (!video) {
      video = document.createElement("video");
      Object.assign(video, { muted: true, loop: true, autoplay: true, playsInline: true });
      video.setAttribute("muted", "");
      bg.append(video);
    }
    video.src = url;
    video.play?.().catch(() => {});
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
    const sizes = layout(layer.classList.contains("scripture") ? "scripture" : "lyrics");
    const min = (obs ? OBS_SIZE_VH.min : sizes.size_min_vh) * vh;
    const max = (obs ? OBS_SIZE_VH.max : sizes.size_max_vh) * vh;
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

  // One line from its styled runs (**strong**, __em__ in the lyrics), as
  // text nodes: lyrics come from imports and are never parsed as HTML.
  function lineElement(runs) {
    const div = document.createElement("div");
    for (const run of runs) {
      let node = document.createTextNode(run.text);
      for (const [on, tag] of [[run.em, "em"], [run.strong, "strong"]]) {
        if (!on) continue;
        const el = document.createElement(tag);
        el.append(node);
        node = el;
      }
      div.append(node);
    }
    return div;
  }

  // The theme's placement and sizes for a kind of slide (Theme::layout).
  function layout(kind) {
    const theme = state.theme;
    return (kind === "scripture" && theme.scripture) || theme;
  }

  // The song title, reference and credits, each where the theme puts it:
  // a band (top or bottom margin) and a slot in it (left, center, right).
  // Areas sharing a slot share its line, in this order.
  const AREAS = ["title", "reference", "credits"];
  function fillAreas(layer, slide) {
    layer.querySelectorAll(".band").forEach((band) => band.remove());
    const align = layout(slide.kind).align;
    const slots = new Map();
    for (const name of AREAS) {
      const text = slide[name]?.trim();
      const area = state.theme[`${name}_area`];
      if (!text || !area || area.place === "hidden") continue;
      const key = `${area.place} ${area.align ?? align}`;
      if (!slots.has(key)) slots.set(key, []);
      slots.get(key).push([name, text]);
    }
    for (const [key, areas] of slots) {
      const [place, side] = key.split(" ");
      let band = layer.querySelector(`.band.${place}`);
      if (!band) {
        band = document.createElement("div");
        band.className = `band ${place}`;
        layer.append(band);
      }
      const slot = document.createElement("div");
      slot.className = `slot ${side}`;
      const line = document.createElement("span");
      line.className = "line";
      areas.forEach(([name, text], i) => {
        const span = document.createElement("span");
        span.className = `area ${name}`;
        span.textContent = i > 0 ? ` · ${text}` : text;
        line.append(span);
      });
      slot.append(line);
      band.append(slot);
    }
  }

  function fill(layer, slide) {
    layer.slide = slide;
    const text = layer.querySelector(".text");
    const runs = slide.runs ?? slide.lines.map((line) => [{ text: line }]);
    text.replaceChildren(...runs.map(lineElement));
    text.classList.toggle("prose", !!slide.wrap);
    layer.classList.toggle("scripture", slide.kind === "scripture");
    fillAreas(layer, slide);
    fit(layer);
  }

  // `live` is false for the state sent on connect, which may be old.
  function render(next, live) {
    state = next;
    // A new font, size range or margin changes what fits: refit both layers.
    if (applyTheme(state.css_vars ?? [])) refitSoon();
    applyVideo(obs ? null : state.theme.background_video);
    root.classList.toggle("panel", state.theme.panel !== "none");
    // The text areas may have moved (their places aren't CSS variables).
    const areasKey = JSON.stringify([state.theme.title_area, state.theme.reference_area, state.theme.credits_area, state.theme.scripture?.align, state.theme.align]);
    if (areasKey !== shownAreas) {
      shownAreas = areasKey;
      for (const layer of layers) if (layer.slide) fillAreas(layer, layer.slide);
    }

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
      else {
        incoming.slide = null;
        incoming.querySelector(".text").replaceChildren();
        incoming.querySelectorAll(".band").forEach((band) => band.remove());
      }
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

  // ---- in the console -------------------------------------------------------

  // The console's webviews hear "pointer" (a click took keyboard focus; give
  // it back) and, for previews, "ready" (send the state now) through wry's
  // IPC. A preview gets its state, with a draft theme, from asafPreview(state).
  const post = (msg) => window.ipc?.postMessage(msg);
  if (embedded) document.addEventListener("pointerdown", () => post("pointer"));
  function startPreview() {
    window.asafPreview = (next) => render(next, false);
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
