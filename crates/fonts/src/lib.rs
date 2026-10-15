//! Fonts bundled into the binary, so the projected output looks the same on
//! every machine: the server serves them to the display page and the console
//! loads them for its theme preview. All are SIL Open Font License 1.1
//! (Google Fonts' variable versions); each license travels with its font.

/// One bundled font family (a variable font covering a weight range).
pub struct BundledFont {
    /// CSS / GPUI family name, as themes store it.
    pub family: &'static str,
    /// File name it is served under: `/fonts/<file>`.
    pub file: &'static str,
    pub bytes: &'static [u8],
    /// Lightest and heaviest weight the variable font covers.
    pub weights: (u16, u16),
    /// Generic CSS family to fall back on while it loads.
    pub generic: &'static str,
    /// The font's OFL license text, served at `/fonts/<family file stem>-OFL.txt`.
    pub license: &'static str,
}

pub const FONTS: &[BundledFont] = &[
    BundledFont {
        family: "Source Sans 3",
        file: "SourceSans3.ttf",
        bytes: include_bytes!("../files/SourceSans3.ttf"),
        weights: (200, 900),
        generic: "sans-serif",
        license: include_str!("../files/SourceSans3-OFL.txt"),
    },
    BundledFont {
        family: "Montserrat",
        file: "Montserrat.ttf",
        bytes: include_bytes!("../files/Montserrat.ttf"),
        weights: (100, 900),
        generic: "sans-serif",
        license: include_str!("../files/Montserrat-OFL.txt"),
    },
    BundledFont {
        family: "Oswald",
        file: "Oswald.ttf",
        bytes: include_bytes!("../files/Oswald.ttf"),
        weights: (200, 700),
        generic: "sans-serif",
        license: include_str!("../files/Oswald-OFL.txt"),
    },
    BundledFont {
        family: "Lora",
        file: "Lora.ttf",
        bytes: include_bytes!("../files/Lora.ttf"),
        weights: (400, 700),
        generic: "serif",
        license: include_str!("../files/Lora-OFL.txt"),
    },
];

/// The bundled font with this family name (case-insensitive).
pub fn find(family: &str) -> Option<&'static BundledFont> {
    FONTS.iter().find(|f| f.family.eq_ignore_ascii_case(family.trim()))
}

/// `@font-face` rules for every bundled font, for the display page.
pub fn font_face_css() -> String {
    FONTS
        .iter()
        .map(|f| {
            format!(
                "@font-face {{ font-family: \"{}\"; src: url(\"/fonts/{}\") format(\"truetype\"); \
                 font-weight: {} {}; font-style: normal; font-display: block; }}\n",
                f.family, f.file, f.weights.0, f.weights.1
            )
        })
        .collect()
}

impl BundledFont {
    /// Where its license is served: `SourceSans3-OFL.txt`.
    pub fn license_file(&self) -> String {
        format!("{}-OFL.txt", self.file.trim_end_matches(".ttf"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_font_is_a_truetype_file_with_its_license() {
        for f in FONTS {
            assert_eq!(&f.bytes[..4], &[0, 1, 0, 0], "{} is TrueType", f.family);
            assert!(f.license.contains("SIL Open Font License, Version 1.1"), "{}", f.family);
            assert!(f.weights.0 <= 400 && f.weights.1 >= 700, "{} covers regular and bold", f.family);
        }
        assert_eq!(find("montserrat").map(|f| f.file), Some("Montserrat.ttf"));
        assert!(find("Comic Sans").is_none());
        assert_eq!(font_face_css().matches("@font-face").count(), FONTS.len());
    }
}
