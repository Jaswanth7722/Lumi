use crate::secret::Secret;
use serde::{Deserialize, Serialize};

/// AI inference mode preference.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InferenceMode {
    AlwaysLocal,
    AlwaysCloud,
    #[default]
    PreferLocal,
    PreferCloud,
}

/// Cloud AI provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudProvider {
    #[default]
    Anthropic,
    OpenAICompatible,
}

/// AI inference configuration matching SRS Chapter 27.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AIConfig {
    /// Inference mode determining whether local or cloud LLM is preferred.
    /// Default: PreferLocal (best privacy with cloud fallback)
    #[serde(default)]
    pub inference_mode: InferenceMode,

    /// Cloud provider for inference when cloud mode is active.
    #[serde(default)]
    pub cloud_provider: CloudProvider,

    /// API key for Anthropic Claude API. Required when cloud_provider = Anthropic
    /// and inference_mode is AlwaysCloud or PreferCloud.
    /// Never logged. Stored in OS keychain at runtime; config file is optional.
    #[serde(default, skip_serializing)]
    pub anthropic_api_key: Option<Secret<String>>,

    /// API key for OpenAI-compatible endpoint.
    #[serde(default, skip_serializing)]
    pub openai_api_key: Option<Secret<String>>,

    /// API key for OpenAI-compatible endpoint (legacy field).
    #[serde(default, skip_serializing)]
    pub openai_compatible_api_key: Option<Secret<String>>,

    /// Base URL for OpenAI-compatible endpoint.
    #[serde(default)]
    pub openai_compatible_url: Option<String>,

    /// GGUF model filename within the models directory.
    /// Required when inference_mode = AlwaysLocal.
    #[serde(default)]
    pub local_model: String,

    /// Sampling temperature. Valid range: 0.0–2.0. Default: 0.7
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Maximum tokens in a single response. Valid range: 64–8192. Default: 1024
    #[serde(default = "default_max_tokens")]
    pub max_response_tokens: u32,

    /// Maximum tokens for conversation history in context window.
    /// Valid range: 512–128000. Default: 8192
    #[serde(default = "default_history_tokens")]
    pub max_history_tokens: u32,

    /// Timeout in ms for a single inference request. Valid range: 1000–300000.
    #[serde(default = "default_timeout_ms")]
    pub inference_timeout_ms: u64,
}

fn default_temperature() -> f32 {
    0.7
}
fn default_max_tokens() -> u32 {
    1024
}
fn default_history_tokens() -> u32 {
    8192
}
fn default_timeout_ms() -> u64 {
    60000
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            inference_mode: InferenceMode::default(),
            cloud_provider: CloudProvider::default(),
            anthropic_api_key: None,
            openai_api_key: None,
            openai_compatible_api_key: None,
            openai_compatible_url: None,
            local_model: String::new(),
            temperature: 0.7,
            max_response_tokens: 1024,
            max_history_tokens: 8192,
            inference_timeout_ms: 60000,
        }
    }
}
