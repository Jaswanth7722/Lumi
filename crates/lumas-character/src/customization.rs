//! # Customization
//!
//! Color themes and visual customization options for the character.
//! These are appearance-specific settings that affect the character's
//! visual presentation, not behavior.
//!
//! # Authority
//! Character Engine — customization data.
//!
//! # Does NOT
//! - Contain GPU resources or shader parameters
//! - Define rendering pipeline logic

use serde::{Deserialize, Serialize};

/// A color theme for the character's UI elements and accent colors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorTheme {
    /// Primary accent color (RGB hex).
    pub primary: String,
    /// Secondary accent color (RGB hex).
    pub secondary: String,
    /// Background/fill color (RGB hex).
    pub background: String,
    /// Text/foreground color (RGB hex).
    pub foreground: String,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            primary: "#5BC8F5".into(),       // Blue default
            secondary: "#F5A623".into(),     // Amber accent
            background: "#1A1A2E".into(),    // Dark background
            foreground: "#F0F4F8".into(),    // Light foreground
        }
    }
}

impl ColorTheme {
    /// Create a new color theme with the given colors.
    pub fn new(primary: String, secondary: String, background: String, foreground: String) -> Self {
        Self {
            primary,
            secondary,
            background,
            foreground,
        }
    }

    /// Create a light theme variant.
    pub fn light() -> Self {
        Self {
            primary: "#5BC8F5".into(),
            secondary: "#F5A623".into(),
            background: "#FFFFFF".into(),
            foreground: "#1A1A2E".into(),
        }
    }

    /// Create a dark theme variant (default).
    pub fn dark() -> Self {
        Self::default()
    }

    /// Create a pastel theme variant.
    pub fn pastel() -> Self {
        Self {
            primary: "#A8D8EA".into(),
            secondary: "#F3B0C3".into(),
            background: "#FEF9EF".into(),
            foreground: "#4A4A6A".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_theme() {
        let theme = ColorTheme::default();
        assert_eq!(theme.primary, "#5BC8F5");
    }

    #[test]
    fn test_light_theme() {
        let theme = ColorTheme::light();
        assert_eq!(theme.background, "#FFFFFF");
    }

    #[test]
    fn test_dark_theme() {
        let theme = ColorTheme::dark();
        assert_eq!(theme.background, "#1A1A2E");
    }

    #[test]
    fn test_pastel_theme() {
        let theme = ColorTheme::pastel();
        assert_eq!(theme.primary, "#A8D8EA");
    }

    #[test]
    fn test_theme_serde_roundtrip() {
        let theme = ColorTheme::pastel();
        let json = serde_json::to_string(&theme).unwrap();
        let deserialized: ColorTheme = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.primary, theme.primary);
        assert_eq!(deserialized.secondary, theme.secondary);
    }
}
