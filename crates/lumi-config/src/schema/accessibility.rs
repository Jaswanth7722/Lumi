use serde::{Deserialize, Serialize};

/// Accessibility and UI configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AccessibilityConfig {
    #[serde(default)]
    pub reduce_motion: bool,
    #[serde(default = "default_font_size")]
    pub font_size_scale: f64,
    #[serde(default)]
    pub high_contrast_mode: bool,
    #[serde(default)]
    pub screen_reader_support: bool,
    #[serde(default)]
    pub colorblind_mode: String,
    #[serde(default = "default_tooltip_delay")]
    pub tooltip_delay_ms: u32,
}

fn default_font_size() -> f64 {
    1.0
}
fn default_tooltip_delay() -> u32 {
    500
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            reduce_motion: false,
            font_size_scale: 1.0,
            high_contrast_mode: false,
            screen_reader_support: false,
            colorblind_mode: "none".into(),
            tooltip_delay_ms: 500,
        }
    }
}
