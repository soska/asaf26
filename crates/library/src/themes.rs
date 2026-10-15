//! Themes and settings. Themes are stored as JSON (`asaf_core::Theme`), so
//! fields added later read back with their defaults; the `name` column is the
//! key and wins over the name inside the JSON.

use asaf_core::{DEFAULT_THEME_NAME, Settings, Theme};
use rusqlite::{OptionalExtension, params};

use crate::{Library, Result};

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
    pub fn save_theme(&mut self, theme: &Theme, previous_name: Option<&str>) -> std::result::Result<Theme, String> {
        let theme = theme.clone().sanitized();
        let json = serde_json::to_string(&theme).map_err(|e| e.to_string())?;
        let default_name = self.settings().map_err(|e| e.to_string())?.default_theme;
        let renames_default = previous_name.is_some_and(|p| p.trim().eq_ignore_ascii_case(&default_name));
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        let taken: Option<String> = tx
            .query_row("SELECT name FROM themes WHERE name = ?1", [&theme.name], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        if let Some(taken) = &taken
            && !previous_name.is_some_and(|p| same(p, taken))
        {
            return Err(format!("There is already a theme called “{taken}”."));
        }
        let updated = match previous_name {
            Some(prev) => tx
                .execute(
                    "UPDATE themes SET name = ?1, data = ?2, updated_at = unixepoch() WHERE name = ?3",
                    params![theme.name, json, prev.trim()],
                )
                .map_err(|e| e.to_string())?,
            None => 0,
        };
        if updated == 0 {
            tx.execute("INSERT INTO themes (name, data) VALUES (?1, ?2)", params![theme.name, json])
                .map_err(|e| e.to_string())?;
        }
        if renames_default {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES ('default_theme', ?1)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [&theme.name],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(theme)
    }

    /// Deletes a theme. The default theme can't be deleted (choose another
    /// default first), so there's always one left.
    pub fn delete_theme(&mut self, name: &str) -> std::result::Result<(), String> {
        let default = self.default_theme().map_err(|e| e.to_string())?;
        if default.name.eq_ignore_ascii_case(name.trim()) {
            return Err(format!("“{}” is the default theme; choose another default first.", default.name));
        }
        self.conn.execute("DELETE FROM themes WHERE name = ?1", [name.trim()]).map_err(|e| e.to_string())?;
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

    /// A new library has the built-in default theme.
    pub(crate) fn ensure_a_theme(&mut self) -> Result<()> {
        let count: i64 = self.conn.query_row("SELECT count(*) FROM themes", [], |r| r.get(0))?;
        if count == 0 {
            let json = serde_json::to_string(&Theme::default()).expect("serialize theme");
            self.conn.execute("INSERT INTO themes (name, data) VALUES (?1, ?2)", params![DEFAULT_THEME_NAME, json])?;
        }
        Ok(())
    }
}

/// A stored theme; unreadable JSON gives the default look under its name.
fn theme_from(name: String, json: &str) -> Theme {
    let theme: Theme = serde_json::from_str(json).unwrap_or_default();
    Theme { name, ..theme }.sanitized()
}
