// Asaf remote: service list, live slides, next/previous/blank and quick
// verse lookup from a phone. Talks to /remote/ws (see
// crates/server/src/remote.rs for the protocol); every command goes through
// the app's store like a key press in the console. No build step, no framework.
(() => {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const PIN_KEY = "asaf-remote-pin";
  const store = {
    get: (k) => { try { return localStorage.getItem(k); } catch { return null; } },
    set: (k, v) => { try { localStorage.setItem(k, v); } catch { /* private mode */ } },
    del: (k) => { try { localStorage.removeItem(k); } catch { /* private mode */ } },
  };

  let ws = null;
  let state = null;
  let nextId = 1;
  const pending = new Map(); // command id -> callback(result)
  let chosenItem = null; // service row tapped once, waiting for "Go live"
  let scrolledTo = -1; // the live slide the list last scrolled to

  // ---- commands ---------------------------------------------------------------

  function send(cmd, done) {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      toast("Not connected.");
      return;
    }
    const id = nextId++;
    if (done) pending.set(id, done);
    ws.send(JSON.stringify({ type: "cmd", id, ...cmd }));
  }

  function result(msg) {
    const done = pending.get(msg.id);
    pending.delete(msg.id);
    if (!msg.ok) toast(msg.error || "That didn't work.");
    if (done) done(msg);
  }

  let toastTimer = null;
  function toast(text) {
    const el = $("toast");
    el.textContent = text;
    el.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { el.hidden = true; }, 3500);
  }

  // ---- rendering --------------------------------------------------------------

  function card(slide, on) {
    const li = document.createElement("li");
    li.classList.toggle("on", on);
    const label = document.createElement("div");
    label.className = "label";
    label.textContent = slide.label; // the title is in the header
    const lines = document.createElement("div");
    lines.className = "lines";
    lines.textContent = slide.lines.join("\n");
    li.append(label, lines);
    return li;
  }

  const BLANKS = { None: "Showing text", Black: "Blanked to black", Background: "Text hidden", Logo: "Showing the logo" };

  function render() {
    const st = state;
    const live = st.slides.length > 0;
    $("title").textContent = live ? st.title : "Nothing live yet.";
    const slide = st.slides[st.current];
    const of = st.live != null ? ` · item ${st.live + 1} of ${st.items.length}` : "";
    $("position").textContent = slide ? `${slide.label} · ${st.current + 1} of ${st.slides.length}${of}` : "";
    $("blank-state").textContent = BLANKS[st.blank] ?? st.blank;
    $("blank-state").classList.toggle("on", st.blank !== "None");
    document.querySelectorAll("[data-blank]").forEach((b) => b.classList.toggle("on", b.dataset.blank === st.blank));
    $("prev").disabled = !live || st.current === 0;
    $("next").disabled = !live || st.current + 1 >= st.slides.length;

    // Slides: tap one to show it.
    const slides = $("slides");
    slides.replaceChildren(
      ...st.slides.map((s, i) => {
        const li = card(s, i === st.current);
        li.onclick = () => send({ cmd: "goto", slide: i });
        return li;
      }),
    );
    $("no-slides").hidden = live;
    const on = slides.children[st.current];
    if (on && scrolledTo !== st.current) on.scrollIntoView({ block: "nearest" });
    scrolledTo = st.current;

    // Service: tap once to choose, "Go live" to show it.
    $("service-name").textContent = st.service || "Service";
    const nextIndex = st.cued != null && st.cued !== st.live ? st.cued : null;
    $("next-item").disabled = nextIndex == null;
    if (chosenItem != null && chosenItem >= st.items.length) chosenItem = null;
    $("items").replaceChildren(
      ...st.items.map((item, i) => {
        const li = document.createElement("li");
        li.classList.toggle("live", i === st.live);
        li.classList.toggle("cued", i === nextIndex);
        li.classList.toggle("chosen", i === chosenItem);
        const n = document.createElement("span");
        n.className = "n";
        n.textContent = i === st.live ? "●" : String(i + 1);
        const what = document.createElement("div");
        what.className = "what";
        const t = document.createElement("div");
        t.className = "t";
        t.textContent = item.title;
        const k = document.createElement("div");
        k.className = "k";
        k.textContent = [item.kind === "song" ? "Song" : "Bible", i === nextIndex ? "next" : ""].filter(Boolean).join(" · ");
        what.append(t, k);
        const go = document.createElement("button");
        go.className = "go primary";
        go.textContent = "Go live";
        go.onclick = (ev) => {
          ev.stopPropagation();
          chosenItem = null;
          send({ cmd: "item", item: i }, (r) => { if (r.ok) showTab("slides"); });
        };
        li.onclick = () => {
          chosenItem = chosenItem === i ? null : i;
          render();
        };
        li.append(n, what, go);
        return li;
      }),
    );
    $("no-items").hidden = st.items.length > 0;

    // Translations, keeping the viewer's choice.
    const select = $("bible");
    const chosen = select.value || st.bible || "";
    const names = st.bibles.join("\n");
    if (select.dataset.names !== names) {
      select.dataset.names = names;
      select.replaceChildren(
        ...st.bibles.map((b) => {
          const o = document.createElement("option");
          o.value = o.textContent = b;
          return o;
        }),
      );
    }
    if (st.bibles.includes(chosen)) select.value = chosen;
  }

  // ---- Bible ------------------------------------------------------------------

  function lookUp(live) {
    const reference = $("reference").value.trim();
    if (!reference) return;
    const translation = $("bible").value || null;
    $("reference").blur(); // drop the phone's keyboard
    send({ cmd: "verse", reference, translation, live }, (r) => {
      if (!r.ok) {
        $("verse-title").textContent = r.error;
        $("verse-slides").replaceChildren();
        $("verse-live").disabled = true;
        return;
      }
      $("verse-title").textContent = live ? `${r.title}: live` : r.title;
      $("verse-slides").replaceChildren(...r.slides.map((s) => card(s, false)));
      $("verse-live").disabled = false;
      if (live) showTab("slides");
    });
  }

  $("verse").addEventListener("submit", (ev) => {
    ev.preventDefault();
    lookUp(false);
  });
  $("verse-live").addEventListener("click", () => lookUp(true));
  $("reference").addEventListener("input", () => { $("verse-live").disabled = !$("reference").value.trim(); });

  // ---- controls ---------------------------------------------------------------

  $("prev").addEventListener("click", () => send({ cmd: "previous" }));
  $("next").addEventListener("click", () => send({ cmd: "next" }));
  document.querySelectorAll("[data-blank]").forEach((b) =>
    b.addEventListener("click", () => {
      // Pressing the blank that's on shows the text again.
      const mode = state && state.blank === b.dataset.blank ? "None" : b.dataset.blank;
      send({ cmd: "blank", mode });
    }),
  );
  $("next-item").addEventListener("click", () => {
    if (state && state.cued != null) send({ cmd: "item", item: state.cued });
  });

  // The tab is in the address (#service, #bible), so a reload or bookmark keeps it.
  const TABS = ["slides", "service", "bible"];
  function showTab(name) {
    if (!TABS.includes(name)) name = "slides";
    document.querySelectorAll("nav button").forEach((b) => b.classList.toggle("on", b.dataset.tab === name));
    for (const tab of TABS) $(`tab-${tab}`).hidden = tab !== name;
    history.replaceState(null, "", name === "slides" ? location.pathname : `#${name}`);
  }
  document.querySelectorAll("nav button").forEach((b) => b.addEventListener("click", () => showTab(b.dataset.tab)));
  showTab(location.hash.slice(1));

  // ---- PIN --------------------------------------------------------------------

  function askPin(error) {
    $("app").hidden = true;
    $("pin").hidden = false;
    $("pin-error").textContent = error || "";
    $("pin-input").value = "";
    $("pin-input").focus();
  }

  let typedPin = null;
  $("pin").addEventListener("submit", (ev) => {
    ev.preventDefault();
    typedPin = $("pin-input").value.trim();
    $("pin-error").textContent = "Checking…";
    connect();
  });

  // ---- socket -----------------------------------------------------------------

  let retryMs = 250;
  let retryTimer = null;
  let refused = false; // wrong PIN: wait for a new one instead of reconnecting

  function connect() {
    clearTimeout(retryTimer);
    if (ws) {
      ws.onclose = null;
      ws.close();
    }
    refused = false;
    const socket = new WebSocket(`ws://${location.host}/remote/ws`);
    ws = socket;
    socket.onmessage = (ev) => {
      const msg = JSON.parse(ev.data);
      switch (msg.type) {
        case "hello":
          if (msg.pin) {
            const pin = typedPin ?? store.get(PIN_KEY);
            if (pin) socket.send(JSON.stringify({ type: "auth", pin }));
            else {
              refused = true;
              askPin();
            }
          }
          break;
        case "auth":
          if (msg.ok) {
            if (typedPin) store.set(PIN_KEY, typedPin);
            typedPin = null;
          } else {
            refused = true;
            store.del(PIN_KEY);
            typedPin = null;
            askPin(msg.error);
          }
          break;
        case "state":
          retryMs = 250;
          state = msg.state;
          $("pin").hidden = true;
          $("app").hidden = false;
          $("dot").classList.add("on");
          render();
          break;
        case "result":
          result(msg);
          break;
      }
    };
    socket.onclose = () => {
      $("dot").classList.remove("on");
      for (const done of pending.values()) done({ ok: false, error: "Disconnected." });
      pending.clear();
      if (refused) return;
      retryTimer = setTimeout(connect, retryMs);
      retryMs = Math.min(retryMs * 2, 2000);
    };
    socket.onerror = () => socket.close();
  }
  connect();
})();
