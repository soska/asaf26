// The pages' language: `__("English text", { name })` looks the text up in
// the catalog the server serves at /locale (duckalization's format, the same
// catalogs as the console) and fills {placeholders}. Missing text stays
// English. Pages call `i18n.load()` before their first render and
// `i18n.update(locale)` when the server says the language changed.
"use strict";
(() => {
  // duckalization's messageId: cyrb53 over UTF-16 code units, in base 36.
  // Must match `asaf_i18n::id` (crates/i18n/src/id.rs).
  function cyrb53(str) {
    let h1 = 0xdeadbeef;
    let h2 = 0x41c6ce57;
    for (let i = 0; i < str.length; i++) {
      const ch = str.charCodeAt(i);
      h1 = Math.imul(h1 ^ ch, 2654435761);
      h2 = Math.imul(h2 ^ ch, 1597334677);
    }
    h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
    h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
    return 4294967296 * (2097151 & h2) + (h1 >>> 0);
  }
  const messageId = (text) => cyrb53(text).toString(36);

  let locale = "en";
  let messages = {};
  const listeners = [];

  function fill(template, args) {
    if (!args) return template;
    return template.replace(/\{([A-Za-z0-9_]+)\}/g, (whole, name) => (name in args ? String(args[name]) : whole));
  }

  window.__ = (text, args) => {
    const found = messages[messageId(text)];
    return fill(typeof found === "string" ? found : text, args);
  };

  async function load() {
    try {
      const res = await fetch("/locale", { cache: "no-store" });
      const body = await res.json();
      locale = body.locale || "en";
      messages = body.messages || {};
    } catch {
      // Offline or an older server: stay in English.
      locale = "en";
      messages = {};
    }
    document.documentElement.lang = locale;
    for (const fn of listeners) fn(locale);
  }

  window.i18n = {
    load,
    get locale() {
      return locale;
    },
    // Calls `fn` after each (re)load, so the page can redraw its text.
    onChange(fn) {
      listeners.push(fn);
    },
    // The server's current locale; reloads the catalog when it differs.
    update(next) {
      if (next && next !== locale) load();
    },
  };
})();
