//! Themes and settings. Themes are stored as JSON (`asaf_core::Theme`), so
//! fields added later read back with their defaults; the `name` column is the
//! key and wins over the name inside the JSON.

use asaf_core::theme::{BUILTIN_THEMES_VERSION, builtin_theme_version};
use asaf_core::{DEFAULT_THEME_NAME, Settings, Theme, builtin_themes};
use rusqlite::{OptionalExtension, params};

use crate::{Library, Result};

/// Why a theme couldn't be saved or deleted. `Display` is the English
/// message (the console words its own).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeError {
    /// Another theme already has this name.
    NameTaken(String),
    /// The default theme can't be deleted.
    IsDefault(String),
    /// Reading or writing the library failed.
    Storage(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeError::NameTaken(name) => write!(f, "There is already a theme called “{name}”."),
            ThemeError::IsDefault(name) => write!(f, "“{name}” is the default theme; choose another default first."),
            ThemeError::Storage(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for ThemeError {}

fn storage(e: impl ToString) -> ThemeError {
    ThemeError::Storage(e.to_string())
}

impl Library {
    /// Every theme, by name. There is always at least one.
    pub fn themes(&self) -> Result<Vec<Theme>> {
        let mut stmt = self.conn.prepare_cached("SELECT name, data FROM themes ORDER BY name COLLATE NOCASE")?;
        stmt.query_map([], |r| Ok(theme_from(r.get(0)?, &r.get::<_, String>(1)?)))?.collect()
    }

    /// The theme called `name` (case-insensitive).
    pub fn theme(&self, name: &str) -> Result<Option<Theme>> {
        self.conn
            .query_row("SELECT name, data FROM themes WHERE name = ?1", [name.trim()], |r| {
                Ok(theme_from(r.get(0)?, &r.get::<_, String>(1)?))
            })
            .optional()
    }

    /// The default theme from the settings, or failing that any theme.
    pub fn default_theme(&self) -> Result<Theme> {
        let name = self.settings()?.default_theme;
        if let Some(t) = self.theme(&name)? {
            return Ok(t);
        }
        Ok(self.themes()?.into_iter().next().unwrap_or_default())
    }

    /// Saves `theme` (sanitized; see [`Theme::sanitized`]), as a new theme or
    /// over the one called `previous_name`, which renames it. Renaming the
    /// default theme keeps it the default. Fails, changing nothing, if
    /// another theme already has the name. Returns the theme as stored.
    pub fn save_theme(&mut self, theme: &Theme, previous_name: Option<&str>) -> std::result::Result<Theme, ThemeError> {
        let theme = theme.clone().sanitized();
        let json = serde_json::to_string(&theme).map_err(storage)?;
        let default_name = self.settings().map_err(storage)?.default_theme;
        let renames_default = previous_name.is_some_and(|p| p.trim().eq_ignore_ascii_case(&default_name));
        let tx = self.conn.transaction().map_err(storage)?;
        let taken: Option<String> = tx
            .query_row("SELECT name FROM themes WHERE name = ?1", [&theme.name], |r| r.get(0))
            .optional()
            .map_err(storage)?;
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        if let Some(taken) = &taken
            && !previous_name.is_some_and(|p| same(p, taken))
        {
            return Err(ThemeError::NameTaken(taken.clone()));
        }
        let updated = match previous_name {
            Some(prev) => tx
                .execute(
                    "UPDATE themes SET name = ?1, data = ?2, updated_at = unixepoch() WHERE name = ?3",
                    params![theme.name, json, prev.trim()],
                )
                .map_err(storage)?,
            None => 0,
        };
        if updated == 0 {
            tx.execute("INSERT INTO themes (name, data) VALUES (?1, ?2)", params![theme.name, json])
                .map_err(storage)?;
        }
        if renames_default {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES ('default_theme', ?1)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [&theme.name],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(theme)
    }

    /// Deletes a theme. The default theme can't be deleted (choose another
    /// default first), so there's always one left.
    pub fn delete_theme(&mut self, name: &str) -> std::result::Result<(), ThemeError> {
        let default = self.default_theme().map_err(storage)?;
        if default.name.eq_ignore_ascii_case(name.trim()) {
            return Err(ThemeError::IsDefault(default.name));
        }
        self.conn.execute("DELETE FROM themes WHERE name = ?1", [name.trim()]).map_err(storage)?;
        Ok(())
    }

    /// A theme name not yet used: `base`, else `base 2`, `base 3`…
    pub fn unused_theme_name(&self, base: &str) -> Result<String> {
        let base = base.trim();
        let mut n = 1;
        loop {
            let name = if n == 1 { base.to_string() } else { format!("{base} {n}") };
            if self.theme(&name)?.is_none() {
                return Ok(name);
            }
            n += 1;
        }
    }

    pub fn settings(&self) -> Result<Settings> {
        let mut stmt = self.conn.prepare_cached("SELECT key, value FROM settings")?;
        let rows: Vec<(String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_>>()?;
        Ok(Settings::from_pairs(rows.iter().map(|(k, v)| (k.as_str(), v.as_str()))))
    }

    pub fn save_settings(&mut self, settings: &Settings) -> Result<()> {
        let tx = self.conn.transaction()?;
        for (key, value) in settings.to_pairs() {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        tx.commit()
    }

    /// A new library has the default theme; every library gets the
    /// built-in themes added since it was last seeded ([`BUILTIN_THEMES_VERSION`]), once:
    /// a deleted one doesn't come back, and a theme already using a
    /// built-in's name is left alone.
    pub(crate) fn seed_themes(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        let count: i64 = tx.query_row("SELECT count(*) FROM themes", [], |r| r.get(0))?;
        if count == 0 {
            let json = serde_json::to_string(&Theme::default()).expect("serialize theme");
            tx.execute("INSERT INTO themes (name, data) VALUES (?1, ?2)", params![DEFAULT_THEME_NAME, json])?;
        }
        let seeded: u32 = tx
            .query_row("SELECT value FROM settings WHERE key = 'builtin_themes'", [], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if seeded < BUILTIN_THEMES_VERSION {
            for theme in builtin_themes().into_iter().filter(|t| builtin_theme_version(&t.name) > seeded) {
                let json = serde_json::to_string(&theme).expect("serialize theme");
                tx.execute("INSERT OR IGNORE INTO themes (name, data) VALUES (?1, ?2)", params![theme.name, json])?;
            }
            tx.execute(
                "INSERT INTO settings (key, value) VALUES ('builtin_themes', ?1)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [BUILTIN_THEMES_VERSION.to_string()],
            )?;
        }
        tx.commit()
    }
}

/// A stored theme; unreadable JSON gives the default look under its name.
fn theme_from(name: String, json: &str) -> Theme {
    let theme: Theme = serde_json::from_str(json).unwrap_or_default();
    Theme { name, ..theme }.sanitized()
}
