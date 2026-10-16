//! Localization in duckalization's format
//! (<https://github.com/soska/duckalization>): the English text is the
//! message, and its ID is a hash of that text. `locales/en.json` and
//! `en.meta.json` are written by the extractor; target catalogs
//! (`locales/es.json`) are made with `duckalize translate` and read here at
//! runtime. No UI deps.
//!
//! The calls the extractor finds and the console's `Lang` provides (string
//! literals only, so they can be extracted):
//!
//! | Call | Catalog message |
//! |---|---|
//! | `l.t("Songs")` | `"Songs"` |
//! | `en("Background image")` | `"Background image"`, translated later through a variable (`l.t(label)`) |
//! | `l.f("Added {title} to the service", &[("title", &name)])` | `"Added {title} to the service"`; `{name}` is filled in after translating |
//! | `l.n(count, "{count} song", "{count} songs")` | `{ "one": "{count} song", "other": "{count} songs" }`; `{count}` is filled in |
//! | `l.tc("Book", "verb")` | `"Book"` with context `"verb"` |
//! | `__("Connect")` in `display/*.js` | `"Connect"` |

pub mod catalog;
pub mod extract;
pub mod id;
pub mod plural;

pub use id::{Message, PluralForm, message_id};
