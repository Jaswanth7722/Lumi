//! # Validation Framework
//!
//! Trait-based validation system for all config structs.
//! Collects all validation errors rather than stopping at the first failure.

use crate::error::{ConfigError, ValidationCategory, ValidationError};
use crate::schema::{
    AIConfig, AccessibilityConfig, AnimationConfig, CharacterConfig, DiagnosticsConfig,
    FeatureFlags, GeneralConfig, IPCConfig, LoggingConfig, LumiConfig, MemoryConfig,
    PerformanceConfig, PhysicsConfig, PluginConfig, PrivacyConfig, RenderingConfig, RuntimeConfig,
    SecurityConfig, StorageConfig, UpdateConfig, VoiceConfig, WorkspaceConfig,
};

/// Implemented by every config struct. Collects all validation errors
/// rather than stopping at the first failure.
pub trait Validate {
    /// Validate the struct in isolation (no cross-field dependencies on
    /// other config sections).
    fn validate(&self) -> Vec<ValidationError>;
}

/// Implemented by structs that have cross-field dependencies on other
/// sections of LumiConfig.
pub trait ValidateWith {
    fn validate_with(&self, config: &LumiConfig) -> Vec<ValidationError>;
}

// ---------------------------------------------------------------------------
// AIConfig
// ---------------------------------------------------------------------------

impl Validate for AIConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Temperature range check
        if self.temperature < 0.0 || self.temperature > 2.0 {
            errors.push(
                ValidationError::new(
                    "ai.temperature",
                    ValidationCategory::OutOfRange,
                    format!(
                        "Temperature {} is outside valid range 0.0–2.0",
                        self.temperature
                    ),
                )
                .with_expected("0.0–2.0")
                .with_actual(self.temperature.to_string())
                .with_suggestion("Set temperature to a value between 0.0 and 2.0"),
            );
        }

        // Max response tokens
        if self.max_response_tokens < 64 || self.max_response_tokens > 8192 {
            errors.push(
                ValidationError::new(
                    "ai.max_response_tokens",
                    ValidationCategory::OutOfRange,
                    format!(
                        "max_response_tokens {} is outside valid range 64–8192",
                        self.max_response_tokens
                    ),
                )
                .with_expected("64–8192")
                .with_actual(self.max_response_tokens.to_string()),
            );
        }

        // Inference timeout
        if self.inference_timeout_ms < 1000 || self.inference_timeout_ms > 300_000 {
            errors.push(
                ValidationError::new(
                    "ai.inference_timeout_ms",
                    ValidationCategory::OutOfRange,
                    format!(
                        "inference_timeout_ms {} is outside valid range 1000–300000",
                        self.inference_timeout_ms
                    ),
                )
                .with_expected("1000–300000")
                .with_actual(self.inference_timeout_ms.to_string()),
            );
        }

        // Local model required when AlwaysLocal
        if self.inference_mode == crate::schema::InferenceMode::AlwaysLocal
            && self.local_model.is_empty()
        {
            errors.push(
                ValidationError::new(
                    "ai.local_model",
                    ValidationCategory::RequiredFieldMissing,
                    "local_model must be set when inference_mode is AlwaysLocal",
                )
                .with_suggestion(
                    "Set ai.local_model to a GGUF model filename in the models directory",
                ),
            );
        }

        // API key required for cloud
        if self.inference_mode == crate::schema::InferenceMode::AlwaysCloud
            && self.cloud_provider == crate::schema::CloudProvider::Anthropic
            && self.anthropic_api_key.is_none()
        {
            errors.push(
                ValidationError::new("ai.anthropic_api_key", ValidationCategory::RequiredFieldMissing,
                    "Anthropic API key is required when inference_mode is AlwaysCloud and cloud_provider is Anthropic")
                    .with_suggestion("Set LUMI_AI_ANTHROPIC_API_KEY environment variable or add anthropic_api_key to config"),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// VoiceConfig
// ---------------------------------------------------------------------------

impl Validate for VoiceConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.wake_word_sensitivity < 0.0 || self.wake_word_sensitivity > 1.0 {
            errors.push(
                ValidationError::new(
                    "voice.wake_word_sensitivity",
                    ValidationCategory::OutOfRange,
                    format!(
                        "Wake word sensitivity {} is outside valid range 0.0–1.0",
                        self.wake_word_sensitivity
                    ),
                )
                .with_expected("0.0–1.0")
                .with_actual(self.wake_word_sensitivity.to_string()),
            );
        }

        if self.tts_rate < 0.5 || self.tts_rate > 2.0 {
            errors.push(
                ValidationError::new(
                    "voice.tts_rate",
                    ValidationCategory::OutOfRange,
                    format!("TTS rate {} is outside valid range 0.5–2.0", self.tts_rate),
                )
                .with_expected("0.5–2.0")
                .with_actual(self.tts_rate.to_string()),
            );
        }

        if self.vad_start_threshold_ms < 50 || self.vad_start_threshold_ms > 500 {
            errors.push(
                ValidationError::new(
                    "voice.vad_start_threshold_ms",
                    ValidationCategory::OutOfRange,
                    format!(
                        "VAD start threshold {} is outside valid range 50–500",
                        self.vad_start_threshold_ms
                    ),
                )
                .with_expected("50–500")
                .with_actual(self.vad_start_threshold_ms.to_string()),
            );
        }

        if self.vad_end_silence_ms < 200 || self.vad_end_silence_ms > 2000 {
            errors.push(
                ValidationError::new(
                    "voice.vad_end_silence_ms",
                    ValidationCategory::OutOfRange,
                    format!(
                        "VAD end silence {} is outside valid range 200–2000",
                        self.vad_end_silence_ms
                    ),
                )
                .with_expected("200–2000")
                .with_actual(self.vad_end_silence_ms.to_string()),
            );
        }

        if self.transcription_confidence_threshold < 0.0
            || self.transcription_confidence_threshold > 1.0
        {
            errors.push(
                ValidationError::new(
                    "voice.transcription_confidence_threshold",
                    ValidationCategory::OutOfRange,
                    format!(
                        "Transcription confidence {} is outside valid range 0.0–1.0",
                        self.transcription_confidence_threshold
                    ),
                )
                .with_expected("0.0–1.0")
                .with_actual(self.transcription_confidence_threshold.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// PerformanceConfig
// ---------------------------------------------------------------------------

impl Validate for PerformanceConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.render_fps_cap < 30 || self.render_fps_cap > 360 {
            errors.push(
                ValidationError::new(
                    "performance.render_fps_cap",
                    ValidationCategory::OutOfRange,
                    format!(
                        "Render FPS cap {} is outside valid range 30–360",
                        self.render_fps_cap
                    ),
                )
                .with_expected("30–360")
                .with_actual(self.render_fps_cap.to_string()),
            );
        }

        if self.gpu_memory_limit_mb < 256 || self.gpu_memory_limit_mb > 32768 {
            errors.push(
                ValidationError::new(
                    "performance.gpu_memory_limit_mb",
                    ValidationCategory::OutOfRange,
                    format!(
                        "GPU memory limit {} is outside valid range 256–32768",
                        self.gpu_memory_limit_mb
                    ),
                )
                .with_expected("256–32768")
                .with_actual(self.gpu_memory_limit_mb.to_string()),
            );
        }

        errors
    }
}

impl ValidateWith for PerformanceConfig {
    fn validate_with(&self, _config: &LumiConfig) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // High quality requires sufficient VRAM
        if self.render_quality == "high" && self.gpu_memory_limit_mb < 1024 {
            errors.push(
                ValidationError::new(
                    "performance.gpu_memory_limit_mb",
                    ValidationCategory::IncompatibleOptions,
                    "GPU memory must be at least 1024 MB when render_quality is 'high'",
                )
                .with_expected("≥ 1024 MB")
                .with_actual(self.gpu_memory_limit_mb.to_string())
                .with_suggestion(
                    "Increase gpu_memory_limit_mb to at least 1024 or set render_quality to 'auto'",
                ),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// StorageConfig
// ---------------------------------------------------------------------------

impl Validate for StorageConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.max_cache_size_mb < 64 || self.max_cache_size_mb > 102400 {
            errors.push(
                ValidationError::new(
                    "storage.max_cache_size_mb",
                    ValidationCategory::OutOfRange,
                    format!(
                        "Max cache size {} MB is outside valid range 64–102400",
                        self.max_cache_size_mb
                    ),
                )
                .with_expected("64–102400")
                .with_actual(self.max_cache_size_mb.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// GeneralConfig
// ---------------------------------------------------------------------------

impl Validate for GeneralConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let valid_channels = ["stable", "beta", "nightly"];
        if !valid_channels.contains(&self.update_channel.as_str()) {
            errors.push(
                ValidationError::new(
                    "general.update_channel",
                    ValidationCategory::InvalidEnumVariant,
                    format!(
                        "Invalid update channel '{}'. Expected one of: stable, beta, nightly",
                        self.update_channel
                    ),
                )
                .with_expected("stable, beta, or nightly")
                .with_actual(self.update_channel.clone()),
            );
        }

        let valid_themes = ["auto", "light", "dark", "system"];
        if !valid_themes.contains(&self.theme.as_str()) {
            errors.push(
                ValidationError::new(
                    "general.theme",
                    ValidationCategory::InvalidEnumVariant,
                    format!(
                        "Invalid theme '{}'. Expected one of: auto, light, dark, system",
                        self.theme
                    ),
                )
                .with_expected("auto, light, dark, or system")
                .with_actual(self.theme.clone()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// RuntimeConfig
// ---------------------------------------------------------------------------

impl Validate for RuntimeConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.log_level.as_str()) {
            errors.push(
                ValidationError::new(
                    "runtime.log_level",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid log level '{}'", self.log_level),
                )
                .with_expected("trace, debug, info, warn, or error")
                .with_actual(self.log_level.clone()),
            );
        }

        if self.max_concurrent_tasks == 0 || self.max_concurrent_tasks > 1024 {
            errors.push(
                ValidationError::new(
                    "runtime.max_concurrent_tasks",
                    ValidationCategory::OutOfRange,
                    format!(
                        "max_concurrent_tasks {} is outside valid range 1–1024",
                        self.max_concurrent_tasks
                    ),
                )
                .with_expected("1–1024")
                .with_actual(self.max_concurrent_tasks.to_string()),
            );
        }

        if self.shutdown_timeout_secs < 5 || self.shutdown_timeout_secs > 300 {
            errors.push(
                ValidationError::new(
                    "runtime.shutdown_timeout_secs",
                    ValidationCategory::OutOfRange,
                    format!(
                        "shutdown_timeout_secs {} is outside valid range 5–300",
                        self.shutdown_timeout_secs
                    ),
                )
                .with_expected("5–300")
                .with_actual(self.shutdown_timeout_secs.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// CharacterConfig
// ---------------------------------------------------------------------------

impl Validate for CharacterConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.name.is_empty() {
            errors.push(
                ValidationError::new(
                    "character.name",
                    ValidationCategory::RequiredFieldMissing,
                    "Character name must not be empty",
                )
                .with_suggestion("Set character.name to the name of your AI companion"),
            );
        }

        if self.size_scale < 0.1 || self.size_scale > 5.0 {
            errors.push(
                ValidationError::new(
                    "character.size_scale",
                    ValidationCategory::OutOfRange,
                    format!(
                        "size_scale {} is outside valid range 0.1–5.0",
                        self.size_scale
                    ),
                )
                .with_expected("0.1–5.0")
                .with_actual(self.size_scale.to_string()),
            );
        }

        if self.opacity < 0.0 || self.opacity > 1.0 {
            errors.push(
                ValidationError::new(
                    "character.opacity",
                    ValidationCategory::OutOfRange,
                    format!("Opacity {} is outside valid range 0.0–1.0", self.opacity),
                )
                .with_expected("0.0–1.0")
                .with_actual(self.opacity.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// MemoryConfig
// ---------------------------------------------------------------------------

impl Validate for MemoryConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.retention_days_default < 1 || self.retention_days_default > 36500 {
            errors.push(
                ValidationError::new(
                    "memory.retention_days_default",
                    ValidationCategory::OutOfRange,
                    format!(
                        "retention_days_default {} is outside valid range 1–36500",
                        self.retention_days_default
                    ),
                )
                .with_expected("1–36500")
                .with_actual(self.retention_days_default.to_string()),
            );
        }

        if self.max_active_memories == 0 || self.max_active_memories > 10000 {
            errors.push(
                ValidationError::new(
                    "memory.max_active_memories",
                    ValidationCategory::OutOfRange,
                    format!(
                        "max_active_memories {} is outside valid range 1–10000",
                        self.max_active_memories
                    ),
                )
                .with_expected("1–10000")
                .with_actual(self.max_active_memories.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// RenderingConfig
// ---------------------------------------------------------------------------

impl Validate for RenderingConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let valid_backends = ["auto", "opengl", "vulkan", "directx", "metal"];
        if !valid_backends.contains(&self.backend.as_str()) {
            errors.push(
                ValidationError::new(
                    "rendering.backend",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid rendering backend '{}'", self.backend),
                )
                .with_expected("auto, opengl, vulkan, directx, or metal")
                .with_actual(self.backend.clone()),
            );
        }

        let valid_msaa = [1, 2, 4, 8];
        if !valid_msaa.contains(&self.msaa_samples) {
            errors.push(
                ValidationError::new(
                    "rendering.msaa_samples",
                    ValidationCategory::OutOfRange,
                    format!(
                        "msaa_samples {} is not a valid MSAA level",
                        self.msaa_samples
                    ),
                )
                .with_expected("1, 2, 4, or 8")
                .with_actual(self.msaa_samples.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// PhysicsConfig
// ---------------------------------------------------------------------------

impl Validate for PhysicsConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.physics_fps < 15 || self.physics_fps > 240 {
            errors.push(
                ValidationError::new(
                    "physics.physics_fps",
                    ValidationCategory::OutOfRange,
                    format!(
                        "physics_fps {} is outside valid range 15–240",
                        self.physics_fps
                    ),
                )
                .with_expected("15–240")
                .with_actual(self.physics_fps.to_string()),
            );
        }

        if self.gravity_multiplier < 0.0 || self.gravity_multiplier > 10.0 {
            errors.push(
                ValidationError::new(
                    "physics.gravity_multiplier",
                    ValidationCategory::OutOfRange,
                    format!(
                        "gravity_multiplier {} is outside valid range 0.0–10.0",
                        self.gravity_multiplier
                    ),
                )
                .with_expected("0.0–10.0")
                .with_actual(self.gravity_multiplier.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// AnimationConfig
// ---------------------------------------------------------------------------

impl Validate for AnimationConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let valid_qualities = ["full", "reduced", "minimal"];
        if !valid_qualities.contains(&self.quality.as_str()) {
            errors.push(
                ValidationError::new(
                    "animation.quality",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid animation quality '{}'", self.quality),
                )
                .with_expected("full, reduced, or minimal")
                .with_actual(self.quality.clone()),
            );
        }

        if self.max_animation_fps < 15 || self.max_animation_fps > 240 {
            errors.push(
                ValidationError::new(
                    "animation.max_animation_fps",
                    ValidationCategory::OutOfRange,
                    format!(
                        "max_animation_fps {} is outside valid range 15–240",
                        self.max_animation_fps
                    ),
                )
                .with_expected("15–240")
                .with_actual(self.max_animation_fps.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// WorkspaceConfig
// ---------------------------------------------------------------------------

impl Validate for WorkspaceConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let valid_sides = ["left", "right", "top", "bottom"];
        if !valid_sides.contains(&self.default_side.as_str()) {
            errors.push(
                ValidationError::new(
                    "workspace.default_side",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid default side '{}'", self.default_side),
                )
                .with_expected("left, right, top, or bottom")
                .with_actual(self.default_side.clone()),
            );
        }

        if self.default_width < 200 || self.default_width > 4096 {
            errors.push(
                ValidationError::new(
                    "workspace.default_width",
                    ValidationCategory::OutOfRange,
                    format!(
                        "default_width {} is outside valid range 200–4096",
                        self.default_width
                    ),
                )
                .with_expected("200–4096")
                .with_actual(self.default_width.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// PluginConfig
// ---------------------------------------------------------------------------

impl Validate for PluginConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.max_active_plugins == 0 || self.max_active_plugins > 200 {
            errors.push(
                ValidationError::new(
                    "plugin.max_active_plugins",
                    ValidationCategory::OutOfRange,
                    format!(
                        "max_active_plugins {} is outside valid range 1–200",
                        self.max_active_plugins
                    ),
                )
                .with_expected("1–200")
                .with_actual(self.max_active_plugins.to_string()),
            );
        }

        let valid_sandbox = ["none", "isolated", "strict"];
        if !valid_sandbox.contains(&self.sandbox_level.as_str()) {
            errors.push(
                ValidationError::new(
                    "plugin.sandbox_level",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid sandbox_level '{}'", self.sandbox_level),
                )
                .with_expected("none, isolated, or strict")
                .with_actual(self.sandbox_level.clone()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// IPCConfig
// ---------------------------------------------------------------------------

impl Validate for IPCConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.port > 0 && self.port < 1024 {
            errors.push(
                ValidationError::new(
                    "ipc.port",
                    ValidationCategory::InvalidPort,
                    format!("Port {} is a privileged port (< 1024)", self.port),
                )
                .with_expected("≥ 1024 or 0 (auto-assign)")
                .with_actual(self.port.to_string())
                .with_suggestion("Use a port ≥ 1024 or set port to 0 for auto-assignment"),
            );
        }

        if self.connection_timeout_ms < 100 || self.connection_timeout_ms > 60000 {
            errors.push(
                ValidationError::new(
                    "ipc.connection_timeout_ms",
                    ValidationCategory::OutOfRange,
                    format!(
                        "connection_timeout_ms {} is outside valid range 100–60000",
                        self.connection_timeout_ms
                    ),
                )
                .with_expected("100–60000")
                .with_actual(self.connection_timeout_ms.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// SecurityConfig
// ---------------------------------------------------------------------------

impl Validate for SecurityConfig {
    fn validate(&self) -> Vec<ValidationError> {
        // No specific field validations needed for SecurityConfig beyond
        // what's enforced by its Option type and serde skip_serializing
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// PrivacyConfig
// ---------------------------------------------------------------------------

impl Validate for PrivacyConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let valid_clipboard = ["always", "on_request", "never"];
        if !valid_clipboard.contains(&self.clipboard_access.as_str()) {
            errors.push(
                ValidationError::new(
                    "privacy.clipboard_access",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid clipboard_access '{}'", self.clipboard_access),
                )
                .with_expected("always, on_request, or never")
                .with_actual(self.clipboard_access.clone()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// AccessibilityConfig
// ---------------------------------------------------------------------------

impl Validate for AccessibilityConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.font_size_scale < 0.5 || self.font_size_scale > 3.0 {
            errors.push(
                ValidationError::new(
                    "accessibility.font_size_scale",
                    ValidationCategory::OutOfRange,
                    format!(
                        "font_size_scale {} is outside valid range 0.5–3.0",
                        self.font_size_scale
                    ),
                )
                .with_expected("0.5–3.0")
                .with_actual(self.font_size_scale.to_string()),
            );
        }

        if self.tooltip_delay_ms > 5000 {
            errors.push(
                ValidationError::new(
                    "accessibility.tooltip_delay_ms",
                    ValidationCategory::OutOfRange,
                    format!(
                        "tooltip_delay_ms {} is outside valid range 0–5000",
                        self.tooltip_delay_ms
                    ),
                )
                .with_expected("0–5000")
                .with_actual(self.tooltip_delay_ms.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// LoggingConfig
// ---------------------------------------------------------------------------

impl Validate for LoggingConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.level.as_str()) {
            errors.push(
                ValidationError::new(
                    "logging.level",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid log level '{}'", self.level),
                )
                .with_expected("trace, debug, info, warn, or error")
                .with_actual(self.level.clone()),
            );
        }

        let valid_formats = ["json", "text", "compact"];
        if !valid_formats.contains(&self.format.as_str()) {
            errors.push(
                ValidationError::new(
                    "logging.format",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid log format '{}'", self.format),
                )
                .with_expected("json, text, or compact")
                .with_actual(self.format.clone()),
            );
        }

        if self.max_log_files == 0 || self.max_log_files > 100 {
            errors.push(
                ValidationError::new(
                    "logging.max_log_files",
                    ValidationCategory::OutOfRange,
                    format!(
                        "max_log_files {} is outside valid range 1–100",
                        self.max_log_files
                    ),
                )
                .with_expected("1–100")
                .with_actual(self.max_log_files.to_string()),
            );
        }

        if self.max_log_file_size_mb < 1 || self.max_log_file_size_mb > 1024 {
            errors.push(
                ValidationError::new(
                    "logging.max_log_file_size_mb",
                    ValidationCategory::OutOfRange,
                    format!(
                        "max_log_file_size_mb {} is outside valid range 1–1024",
                        self.max_log_file_size_mb
                    ),
                )
                .with_expected("1–1024")
                .with_actual(self.max_log_file_size_mb.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// UpdateConfig
// ---------------------------------------------------------------------------

impl Validate for UpdateConfig {
    fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let valid_channels = ["stable", "beta", "nightly"];
        if !valid_channels.contains(&self.channel.as_str()) {
            errors.push(
                ValidationError::new(
                    "update.channel",
                    ValidationCategory::InvalidEnumVariant,
                    format!("Invalid update channel '{}'", self.channel),
                )
                .with_expected("stable, beta, or nightly")
                .with_actual(self.channel.clone()),
            );
        }

        if self.check_interval_hours == 0 || self.check_interval_hours > 8760 {
            errors.push(
                ValidationError::new(
                    "update.check_interval_hours",
                    ValidationCategory::OutOfRange,
                    format!(
                        "check_interval_hours {} is outside valid range 1–8760",
                        self.check_interval_hours
                    ),
                )
                .with_expected("1–8760")
                .with_actual(self.check_interval_hours.to_string()),
            );
        }

        errors
    }
}

// ---------------------------------------------------------------------------
// DiagnosticsConfig
// ---------------------------------------------------------------------------

impl Validate for DiagnosticsConfig {
    fn validate(&self) -> Vec<ValidationError> {
        // DiagnosticsConfig has no constrained fields beyond bool/option types
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// FeatureFlags
// ---------------------------------------------------------------------------

impl Validate for FeatureFlags {
    fn validate(&self) -> Vec<ValidationError> {
        if let Err(msg) = self.check_consistency() {
            vec![ValidationError::new(
                "feature_flags",
                ValidationCategory::IncompatibleOptions,
                msg,
            )]
        } else {
            Vec::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Full Config Validation
// ---------------------------------------------------------------------------

/// Validates the complete LumiConfig by:
/// 1. Calling `validate()` on every subsystem config
/// 2. Calling `validate_with(config)` on structs implementing `ValidateWith`
/// 3. Collecting all errors into a single Vec<ValidationError>
/// 4. Returning ConfigError::ValidationFailed if any errors exist
pub fn validate_config(config: &LumiConfig) -> Result<(), ConfigError> {
    let mut all_errors: Vec<ValidationError> = Vec::new();

    // Validate each subsystem config
    all_errors.extend(config.general.validate());
    all_errors.extend(config.runtime.validate());
    all_errors.extend(config.character.validate());
    all_errors.extend(config.ai.validate());
    all_errors.extend(config.voice.validate());
    all_errors.extend(config.memory.validate());
    all_errors.extend(config.rendering.validate());
    all_errors.extend(config.physics.validate());
    all_errors.extend(config.animation.validate());
    all_errors.extend(config.workspace.validate());
    all_errors.extend(config.plugin.validate());
    all_errors.extend(config.ipc.validate());
    all_errors.extend(config.storage.validate());
    all_errors.extend(config.security.validate());
    all_errors.extend(config.privacy.validate());
    all_errors.extend(config.performance.validate());
    all_errors.extend(config.accessibility.validate());
    all_errors.extend(config.logging.validate());
    all_errors.extend(config.update.validate());
    all_errors.extend(config.diagnostics.validate());
    all_errors.extend(config.feature_flags.validate());

    // Cross-field validations
    all_errors.extend(config.performance.validate_with(config));

    if all_errors.is_empty() {
        Ok(())
    } else {
        let error_msgs: Vec<String> = all_errors.iter().map(|e| e.to_string()).collect();
        Err(ConfigError::ValidationFailed {
            count: all_errors.len(),
            errors: error_msgs.join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config_produces_no_errors() {
        let config = LumiConfig::default();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_temperature_out_of_range() {
        let mut config = LumiConfig::default();
        config.ai.temperature = 3.0;
        let result = validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_always_local_without_model() {
        let mut config = LumiConfig::default();
        config.ai.inference_mode = crate::schema::InferenceMode::AlwaysLocal;
        config.ai.local_model = String::new();
        let result = validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_collects_all_errors() {
        let mut config = LumiConfig::default();
        config.ai.temperature = 3.0;
        config.performance.render_fps_cap = 500;
        let result = validate_config(&config);
        match result {
            Err(ConfigError::ValidationFailed { count, .. }) => {
                assert!(
                    count >= 2,
                    "Expected at least 2 validation errors, got {count}"
                );
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }

    #[test]
    fn test_render_fps_cap_below_minimum() {
        let mut config = LumiConfig::default();
        config.performance.render_fps_cap = 15;
        let result = validate_config(&config);
        assert!(result.is_err());
    }
}
