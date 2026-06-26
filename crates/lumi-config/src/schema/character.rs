use serde::{Deserialize, Serialize};

/// Character appearance and positioning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CharacterConfig {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_size_scale")]
    pub size_scale: f64,
    #[serde(default = "default_panel_side")]
    pub default_side: String,
    #[serde(default = "default_pos_x")]
    pub position_x: i32,
    #[serde(default = "default_pos_y")]
    pub position_y: i32,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

fn default_name() -> String {
    "Lumi".into()
}
fn default_size_scale() -> f64 {
    1.0
}
fn default_panel_side() -> String {
    "right".into()
}
fn default_pos_x() -> i32 {
    1800
}
fn default_pos_y() -> i32 {
    900
}
fn default_opacity() -> f64 {
    1.0
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            size_scale: 1.0,
            default_side: default_panel_side(),
            position_x: 1800,
            position_y: 900,
            locked: false,
            opacity: 1.0,
        }
    }
}
