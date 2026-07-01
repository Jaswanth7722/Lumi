use serde::{Deserialize, Serialize};

/// STT model size.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum STTModel {
    Tiny,
    Base,
    #[default]
    Small,
    Medium,
    LargeV3,
}

/// Voice input/output configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VoiceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_wake_word")]
    pub wake_word: String,
    #[serde(default = "default_wake_sensitivity")]
    pub wake_word_sensitivity: f32,
    #[serde(default)]
    pub push_to_talk_key: Option<String>,
    #[serde(default)]
    pub stt_model: STTModel,
    #[serde(default = "default_tts_voice")]
    pub tts_voice: String,
    #[serde(default = "default_tts_rate")]
    pub tts_rate: f32,
    #[serde(default = "default_true")]
    pub tts_enabled: bool,
    #[serde(default)]
    pub microphone_device: Option<String>,
    #[serde(default)]
    pub audio_output_device: Option<String>,
    #[serde(default = "default_vad_start")]
    pub vad_start_threshold_ms: u32,
    #[serde(default = "default_vad_end")]
    pub vad_end_silence_ms: u32,
    #[serde(default = "default_transcription_confidence")]
    pub transcription_confidence_threshold: f32,
}

fn default_true() -> bool {
    true
}
fn default_wake_word() -> String {
    "Hey Lumi".into()
}
fn default_wake_sensitivity() -> f32 {
    0.85
}
fn default_tts_voice() -> String {
    "lumi_default_en".into()
}
fn default_tts_rate() -> f32 {
    1.0
}
fn default_vad_start() -> u32 {
    150
}
fn default_vad_end() -> u32 {
    800
}
fn default_transcription_confidence() -> f32 {
    0.70
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            wake_word: default_wake_word(),
            wake_word_sensitivity: 0.85,
            push_to_talk_key: None,
            stt_model: STTModel::default(),
            tts_voice: default_tts_voice(),
            tts_rate: 1.0,
            tts_enabled: true,
            microphone_device: None,
            audio_output_device: None,
            vad_start_threshold_ms: 150,
            vad_end_silence_ms: 800,
            transcription_confidence_threshold: 0.70,
        }
    }
}
