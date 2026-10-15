// Asaf stage view: the current and next slide and a clock, from the same
// DisplayState the projector gets (/ws). No build step, no framework.
(() => {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const current = $("current").querySelector(".text");
  const next = $("next").querySelector(".text");
  let state = null;

  // Largest size in [min, max] (px) at which `text` fits its box. Lyric
  // lines stay whole when they can; prose (Bible verses) always wraps.
  function fit(text, min, max) {
    const box = text.parentElement;
    if (!text.firstChild) return;
    const fits = (px) => {
      text.style.fontSize = `${px}px`;
      return text.offsetWidth <= box.clientWidth && text.offsetHeight <= box.clientHeight;
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
    text.style.fontSize = `${size ?? min}px`;
  }

  function fill(text, lines, prose) {
    text.replaceChildren(
      ...lines.map((line) => {
        const div = document.createElement("div");
        div.textContent = line;
        return div;
      }),
    );
    text.classList.toggle("prose", prose);
  }

  function layout() {
    const vh = window.innerHeight / 100;
    fit(current, 3 * vh, 14 * vh);
    fit(next, 2.5 * vh, 7 * vh);
  }

  const BLANKS = { Black: "BLACK", Background: "TEXT HIDDEN", Logo: "LOGO" };

  function render(st) {
    state = st;
    const slide = st.slide;
    fill(current, slide ? slide.lines : [], !!slide?.wrap);
    let head = "";
    if (st.next) {
      fill(next, st.next.lines, !!st.next.wrap);
      head = `Next · ${st.next.label}`;
    } else {
      fill(next, st.up_next ? [st.up_next] : [], false);
      head = slide ? (st.up_next ? "Up next" : "End") : "";
    }
    $("next-head").textContent = head;
    // "V1 · Blessed Assurance · …", or just "John 3:16 (KJV)" for a passage.
    const footer = slide?.footer ?? "";
    $("label").textContent = footer.startsWith(slide?.label) ? footer : [slide?.label, footer].filter(Boolean).join(" · ");
    const blank = BLANKS[st.blank];
    $("blank").hidden = !blank;
    $("blank").textContent = blank ?? "";
    document.body.classList.toggle("blanked", !!blank);
    layout();
  }

  window.addEventListener("resize", () => state && layout());
  if (document.fonts) document.fonts.ready.then(() => state && layout());

  function tick() {
    const now = new Date();
    $("clock").textContent = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    setTimeout(tick, 1000 - now.getMilliseconds());
  }
  tick();

  let retryMs = 250;
  function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws`);
    ws.onopen = () => {
      $("dot").classList.add("on");
      retryMs = 250;
    };
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.type === "state") render(msg.state);
    };
    ws.onclose = () => {
      $("dot").classList.remove("on");
      setTimeout(connect, retryMs);
      retryMs = Math.min(retryMs * 2, 2000);
    };
    ws.onerror = () => ws.close();
  }
  connect();
})();
