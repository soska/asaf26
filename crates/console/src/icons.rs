//! The console's icons: [Lucide](https://lucide.dev), through gpui-kit.
//! `IconName` names the whole catalog, but gpui-kit's `Assets` embeds only the
//! icons its own components use; [`Assets`] adds the others the console uses.
//! A test checks every `IconName::…` in the console's code can be loaded.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub use gpui::assets::IconName;

// The icons the console uses that gpui-kit's `Assets` doesn't embed
// (see gpui-kit-assets' default-icons.txt).
gpui::assets::icon_assets!(ExtraIcons, [Download, FilePlus, GripVertical, Image, ListPlus, Monitor, MonitorOff, Music, Pencil, Save, Trash, Upload]);

/// The app's assets: gpui-kit's, plus the console's extra icons.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => gpui::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_used_is_embedded() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut used = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let code = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            for (at, _) in code.match_indices("IconName::") {
                let name: String = code[at + 10..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && name != "ALL" {
                    used.push(name);
                }
            }
        }
        used.sort();
        used.dedup();
        assert!(used.len() > 10, "found the icons: {used:?}");
        for name in used {
            let icon = IconName::ALL.iter().find(|i| format!("{i:?}") == name).unwrap_or_else(|| panic!("no icon {name}"));
            let path = icon.path();
            assert!(Assets.load(&path).unwrap().is_some(), "{name} ({path}) isn't embedded: add it to ExtraIcons");
        }
    }
}
