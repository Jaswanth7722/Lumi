//! # Configuration System — User Preferences and Settings (Chapter 27)
//!
//! Defines the full configuration schema, validation, and platform-specific
//! config file paths for the Lumi platform.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Configuration Schema (TOML-based)
// ---------------------------------------------------------------------------

/// Root configuration structure matching config.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LumiConfig {
    pub general: GeneralConfig,
    pub character: CharacterConfig,
    pub ai: AIConfig,
    pub voice: VoiceConfig,
    pub memory: MemoryConfig,
    pub privacy: PrivacyConfig,
    pub desktop_awareness: DesktopAwarenessConfig,
    pub performance: PerformanceConfig,
    pub behavior: BehaviorConfig,
    pub hotkeys: HotkeysConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub language: String,
    pub start_on_login: bool,
    pub check_updates: bool,
    pub update_channel: UpdateChannel,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            language: "en".into(),
            start_on_login: true,
            check_updates: true,
            update_channel: UpdateChannel::Stable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterConfig {
    pub name: String,
    pub size_scale: f32,
    pub position_x: f32,
    pub position_y: f32,
    pub default_side: PanelSide,
    pub greeting_message: String,
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            name: "Lumi".into(),
            size_scale: 1.0,
            position_x: 1800.0,
            position_y: 900.0,
            default_side: PanelSide::Right,
            greeting_message: "Hello! How can I help you today?".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelSide {
    Right,
    Left,
    Above,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub inference_mode: InferenceModeStr,
    pub cloud_provider: CloudProvider,
    pub local_model: String,
    pub temperature: f32,
    pub max_response_tokens: u32,
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            inference_mode: InferenceModeStr::PreferLocal,
            cloud_provider: CloudProvider::Anthropic,
            local_model: "phi-3-mini".into(),
            temperature: 0.7,
            max_response_tokens: 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceModeStr {
    AlwaysLocal,
    AlwaysCloud,
    PreferLocal,
    PreferCloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudProvider {
    Anthropic,
    OpenAICompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub wake_word: String,
    pub wake_word_sensitivity: f32,
    pub push_to_talk_key: String,
    pub stt_model: String,
    pub tts_voice: String,
    pub tts_rate: f32,
    pub tts_enabled: bool,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            wake_word: "Hey Lumi".into(),
            wake_word_sensitivity: 0.85,
            push_to_talk_key: "ctrl+shift+space".into(),
            stt_model: "small".into(),
            tts_voice: "lumi_default_en".into(),
            tts_rate: 1.0,
            tts_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub retention_days_default: u32,
    pub auto_extract: bool,
    pub require_confirmation_for_observations: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days_default: 365,
            auto_extract: true,
            require_confirmation_for_observations: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub screen_capture_enabled: bool,
    pub clipboard_access: ClipboardAccessLevel,
    pub telemetry_enabled: bool,
    pub crash_reports_enabled: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            screen_capture_enabled: false,
            clipboard_access: ClipboardAccessLevel::OnRequest,
            telemetry_enabled: false,
            crash_reports_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardAccessLevel {
    Never,
    OnRequest,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopAwarenessConfig {
    pub active_window_tracking: bool,
    pub notification_awareness: bool,
    pub focus_mode_detection: bool,
    pub idle_sleep_minutes: u32,
}

impl Default for DesktopAwarenessConfig {
    fn default() -> Self {
        Self {
            active_window_tracking: true,
            notification_awareness: true,
            focus_mode_detection: true,
            idle_sleep_minutes: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub render_fps_cap: u32,
    pub render_quality: RenderQuality,
    pub gpu_memory_limit_mb: u32,
    pub animation_quality: AnimationQuality,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            render_fps_cap: 60,
            render_quality: RenderQuality::Auto,
            gpu_memory_limit_mb: 1200,
            animation_quality: AnimationQuality::Full,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderQuality {
    Auto,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationQuality {
    Full,
    Reduced,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub proactive_suggestions: bool,
    pub celebration_animations: bool,
    pub idle_exploration: bool,
    pub no_disturb_during_fullscreen: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            proactive_suggestions: true,
            celebration_animations: true,
            idle_exploration: true,
            no_disturb_during_fullscreen: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeysConfig {
    pub toggle_conversation: String,
    pub toggle_voice: String,
    pub toggle_visibility: String,
    pub toggle_focus_mode: String,
}

impl Default for HotkeysConfig {
    fn default() -> Self {
        Self {
            toggle_conversation: "ctrl+shift+l".into(),
            toggle_voice: "ctrl+shift+m".into(),
            toggle_visibility: "ctrl+shift+h".into(),
            toggle_focus_mode: "ctrl+shift+f".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration Validation
// ---------------------------------------------------------------------------

/// Result of configuration validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

/// A single validation error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub value: Option<String>,
}

/// Validates a `LumiConfig` against range constraints.
pub fn validate_config(config: &LumiConfig) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // General
    if config.general.language.is_empty() {
        errors.push(ValidationError {
            field: "general.language".into(),
            message: "Language must not be empty".into(),
            value: None,
        });
    }

    // Character
    if config.character.size_scale < 0.5 || config.character.size_scale > 2.0 {
        errors.push(ValidationError {
            field: "character.size_scale".into(),
            message: "Size scale must be between 0.5 and 2.0".into(),
            value: Some(config.character.size_scale.to_string()),
        });
    }

    // AI
    if config.ai.temperature < 0.0 || config.ai.temperature > 2.0 {
        warnings.push(format!(
            "AI temperature {} is outside recommended range (0.0-2.0)",
            config.ai.temperature
        ));
    }

    // Voice
    if config.voice.wake_word_sensitivity < 0.0 || config.voice.wake_word_sensitivity > 1.0 {
        errors.push(ValidationError {
            field: "voice.wake_word_sensitivity".into(),
            message: "Wake word sensitivity must be between 0.0 and 1.0".into(),
            value: Some(config.voice.wake_word_sensitivity.to_string()),
        });
    }

    // Performance
    if config.performance.render_fps_cap < 15 || config.performance.render_fps_cap > 240 {
        warnings.push(format!(
            "Render FPS cap {} is outside recommended range (15-240)",
            config.performance.render_fps_cap
        ));
    }

    // Desktop Awareness
    if config.desktop_awareness.idle_sleep_minutes < 1
        || config.desktop_awareness.idle_sleep_minutes > 120
    {
        warnings.push(format!(
            "Idle sleep time {} minutes is outside recommended range (1-120)",
            config.desktop_awareness.idle_sleep_minutes
        ));
    }

    ValidationResult {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

// ---------------------------------------------------------------------------
// Platform Config Paths
// ---------------------------------------------------------------------------

/// Get the platform-appropriate config directory path.
pub fn config_directory() -> String {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        format!("{home}/Library/Application Support/Lumi")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Roaming".into());
        format!("{appdata}\\Lumi")
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        format!("{home}/.config/lumi")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "/tmp/lumi-config".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = LumiConfig::default();
        let result = validate_config(&config);
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_invalid_size_scale() {
        let mut config = LumiConfig::default();
        config.character.size_scale = 3.0;
        let result = validate_config(&config);
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.field == "character.size_scale")
        );
    }

    #[test]
    fn test_invalid_wake_word_sensitivity() {
        let mut config = LumiConfig::default();
        config.voice.wake_word_sensitivity = 1.5;
        let result = validate_config(&config);
        assert!(!result.valid);
    }

    #[test]
    fn test_config_directory_not_empty() {
        let dir = config_directory();
        assert!(!dir.is_empty());
    }

    #[test]
    fn test_update_channel_default() {
        assert_eq!(UpdateChannel::Stable, UpdateChannel::Stable);
    }

    #[test]
    fn test_hotkeys_config_default() {
        let hotkeys = HotkeysConfig::default();
        assert_eq!(hotkeys.toggle_conversation, "ctrl+shift+l");
        assert_eq!(hotkeys.toggle_focus_mode, "ctrl+shift+f");
    }
}
