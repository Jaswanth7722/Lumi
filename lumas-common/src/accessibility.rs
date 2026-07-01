//! # Accessibility — Visual, Motor, and Cognitive Accommodations (Chapter 26)
//!
//! Defines accessibility modes, contrast settings, text scaling,
//! and assistive technology integration points.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Accessibility Modes
// ---------------------------------------------------------------------------

/// Visual accessibility settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualAccessibility {
    /// High contrast mode for workspace panels.
    pub high_contrast: bool,
    /// Font scale factor (1.0 = normal, 1.5 = large).
    pub text_scale: f32,
    /// Reduce non-essential motion.
    pub reduce_motion: bool,
    /// Color-blind friendly mode (shapes + patterns beyond hue).
    pub color_blind_mode: bool,
    /// Screen reader / assistive technology support enabled.
    pub screen_reader_support: bool,
}

impl Default for VisualAccessibility {
    fn default() -> Self {
        Self {
            high_contrast: false,
            text_scale: 1.0,
            reduce_motion: false,
            color_blind_mode: false,
            screen_reader_support: true,
        }
    }
}

/// Motor accessibility settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotorAccessibility {
    /// Keyboard-only navigation mode.
    pub keyboard_navigation: bool,
    /// Sticky keys compatibility (multi-key hotkeys work with sticky keys).
    pub sticky_keys_compatible: bool,
    /// Large click targets.
    pub large_click_targets: bool,
    /// Voice-only mode (complete control via voice).
    pub voice_only_mode: bool,
    /// Dwell activation time in milliseconds (0 = disabled).
    pub dwell_activation_ms: u64,
}

impl Default for MotorAccessibility {
    fn default() -> Self {
        Self {
            keyboard_navigation: true,
            sticky_keys_compatible: true,
            large_click_targets: false,
            voice_only_mode: false,
            dwell_activation_ms: 0,
        }
    }
}

/// Cognitive accessibility settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveAccessibility {
    /// Consistent behavior mode (no unexpected changes).
    pub consistent_behavior: bool,
    /// Plain language mode (~8th grade reading level).
    pub plain_language: bool,
    /// Focus mode compatibility (respect OS focus assist).
    pub focus_mode_compatible: bool,
    /// Offer undo immediately after every action.
    pub offer_undo: bool,
}

impl Default for CognitiveAccessibility {
    fn default() -> Self {
        Self {
            consistent_behavior: true,
            plain_language: false,
            focus_mode_compatible: true,
            offer_undo: true,
        }
    }
}

/// Complete accessibility configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccessibilityConfig {
    pub visual: VisualAccessibility,
    pub motor: MotorAccessibility,
    pub cognitive: CognitiveAccessibility,
}

// ---------------------------------------------------------------------------
// Contrast and Color
// ---------------------------------------------------------------------------

/// WCAG contrast ratio requirement.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ContrastRequirement {
    pub ratio: f32,
    pub level: ContrastLevel,
}

/// WCAG compliance level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContrastLevel {
    AA,
    AAA,
}

impl Default for ContrastRequirement {
    fn default() -> Self {
        Self {
            ratio: 4.5,
            level: ContrastLevel::AA,
        }
    }
}

/// Check if a contrast ratio meets WCAG AA standards.
pub fn meets_wcag_aa(ratio: f32) -> bool {
    ratio >= 4.5
}

/// Check if a contrast ratio meets WCAG AAA standards.
pub fn meets_wcag_aaa(ratio: f32) -> bool {
    ratio >= 7.0
}

// ---------------------------------------------------------------------------
// Screen Reader Integration
// ---------------------------------------------------------------------------

/// Roles for accessible UI elements (ARIA-equivalent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessibleRole {
    Application,
    Character,
    Panel,
    Button,
    Status,
    Alert,
    Dialog,
}

/// An accessible element registered with the OS accessibility system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibleElement {
    pub role: AccessibleRole,
    pub label: String,
    pub description: Option<String>,
    pub bounds: (f32, f32, f32, f32),
    pub parent: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_defaults() {
        let config = AccessibilityConfig::default();
        assert!(!config.visual.high_contrast);
        assert!(config.visual.screen_reader_support);
        assert!(config.motor.keyboard_navigation);
        assert!(!config.motor.voice_only_mode);
        assert!(config.cognitive.offer_undo);
    }

    #[test]
    fn test_wcag_contrast() {
        assert!(meets_wcag_aa(4.5));
        assert!(!meets_wcag_aa(3.0));
        assert!(meets_wcag_aaa(7.0));
        assert!(!meets_wcag_aaa(5.0));
    }

    #[test]
    fn test_contrast_requirement_default() {
        let req = ContrastRequirement::default();
        assert!((req.ratio - 4.5).abs() < f32::EPSILON);
        assert_eq!(req.level, ContrastLevel::AA);
    }

    #[test]
    fn test_accessible_role_variants() {
        let roles = vec![
            AccessibleRole::Application,
            AccessibleRole::Character,
            AccessibleRole::Panel,
            AccessibleRole::Button,
        ];
        for role in roles {
            let json = serde_json::to_value(&role).unwrap();
            let back: AccessibleRole = serde_json::from_value(json).unwrap();
            assert_eq!(format!("{role:?}"), format!("{back:?}"));
        }
    }
}
