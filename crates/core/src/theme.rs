use serde::Serialize;

/// Visual settings for the display page. Serialized to CSS variables there.
/// Sizes are in percent of the viewport height so output scales with the screen.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Theme {
    pub font_family: String,
    pub font_weight: u16,
    pub size_min_vh: f32,
    pub size_max_vh: f32,
    pub line_height: f32,
    pub color: String,
    pub outline_color: String,
    pub outline_vh: f32,
    pub shadow: String,
    pub align: String,
    /// Safe-area margins in percent of width/height.
    pub margin_x_pct: f32,
    pub margin_y_pct: f32,
    pub background_color: String,
    pub background_image: Option<String>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font_family: "\"Avenir Next\", \"Segoe UI\", \"Helvetica Neue\", Arial, sans-serif"
                .into(),
            font_weight: 700,
            size_min_vh: 4.0,
            size_max_vh: 11.0,
            line_height: 1.2,
            color: "#ffffff".into(),
            outline_color: "rgba(0, 0, 0, 0.9)".into(),
            outline_vh: 0.45,
            shadow: "0 0.4vh 1.2vh rgba(0, 0, 0, 0.85)".into(),
            align: "center".into(),
            margin_x_pct: 6.0,
            margin_y_pct: 8.0,
            background_color: "#101018".into(),
            background_image: Some("/assets/background.svg".into()),
        }
    }
}
