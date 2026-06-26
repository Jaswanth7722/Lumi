//! # Voice System — Audio Types and Lip Sync (Chapter 13)
//!
//! Defines the wake word engine, STT/TTS configuration, viseme types,
//! and lip sync frame structures.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Wake Word Detection
// ---------------------------------------------------------------------------

/// Result of processing an audio chunk through the wake word engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WakeWordResult {
    /// Wake word detected with confidence score.
    Detected { confidence: f32 },
    /// No wake word detected.
    NotDetected,
}

/// Configuration for the wake word engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordConfig {
    /// Detection threshold (0.0 to 1.0, default 0.85).
    pub threshold: f32,
    /// Cooldown in milliseconds between detections (default 2000).
    pub cooldown_ms: u64,
    /// Primary wake phrase (default "Hey Lumi").
    pub primary_phrase: String,
    /// Alternative wake phrases (up to 3).
    pub alternative_phrases: Vec<String>,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            threshold: 0.85,
            cooldown_ms: 2000,
            primary_phrase: "Hey Lumi".into(),
            alternative_phrases: vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// Voice Activity Detection
// ---------------------------------------------------------------------------

/// Events emitted by the Voice Activity Detector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VADEvent {
    /// User started speaking.
    SpeechStart,
    /// User stopped speaking.
    SpeechEnd,
    /// No speech detected for extended period.
    Silence,
}

/// Configuration for the voice activity detector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VADConfig {
    /// Audio energy threshold for speech detection.
    pub energy_threshold: f32,
    /// Duration in ms of sustained energy to trigger SpeechStart.
    pub speech_start_ms: u64,
    /// Duration in ms of silence to trigger SpeechEnd.
    pub speech_end_ms: u64,
    /// Duration in ms of silence to trigger Silence event.
    pub silence_timeout_ms: u64,
}

impl Default for VADConfig {
    fn default() -> Self {
        Self {
            energy_threshold: 0.02,
            speech_start_ms: 150,
            speech_end_ms: 800,
            silence_timeout_ms: 30000,
        }
    }
}

// ---------------------------------------------------------------------------
// Speech-to-Text
// ---------------------------------------------------------------------------

/// Whisper model sizes for local STT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WhisperModelSize {
    /// 39MB, fastest, English-only available.
    Tiny,
    /// 74MB, fast, good accuracy.
    Base,
    /// 244MB, balanced (default).
    Small,
    /// 769MB, high accuracy.
    Medium,
    /// 1.5GB, best accuracy, multilingual.
    LargeV3,
}

impl WhisperModelSize {
    /// Returns the approximate model file size in MB.
    pub fn size_mb(&self) -> u32 {
        match self {
            WhisperModelSize::Tiny => 39,
            WhisperModelSize::Base => 74,
            WhisperModelSize::Small => 244,
            WhisperModelSize::Medium => 769,
            WhisperModelSize::LargeV3 => 1500,
        }
    }
}

impl Default for WhisperModelSize {
    fn default() -> Self {
        Self::Small
    }
}

/// Configuration for the Whisper STT engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperConfig {
    pub model_size: WhisperModelSize,
    pub language: Option<String>,
    pub beam_size: u32,
    pub confidence_threshold: f32,
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            model_size: WhisperModelSize::Small,
            language: Some("en".into()),
            beam_size: 5,
            confidence_threshold: 0.7,
        }
    }
}

// ---------------------------------------------------------------------------
// Text-to-Speech
// ---------------------------------------------------------------------------

/// Configuration for the TTS engine voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    /// Voice identifier (e.g., "lumi_default_en").
    pub voice_id: String,
    /// Speaking rate from 0.8 to 1.4, default 1.0.
    pub speaking_rate: f32,
    /// Pitch shift in semitones, default 0.0.
    pub pitch_shift: f32,
    /// Amplitude scaling, default 1.0.
    pub energy: f32,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            voice_id: "lumi_default_en".into(),
            speaking_rate: 1.0,
            pitch_shift: 0.0,
            energy: 1.0,
        }
    }
}

/// A request to synthesize speech from text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTSRequest {
    pub text: String,
    pub voice_config: VoiceConfig,
    pub priority: TTSPriority,
}

/// Priority level for TTS requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TTSPriority {
    /// Normal conversational output.
    Normal,
    /// Urgent output (e.g., warnings).
    High,
    /// Background or ambient output.
    Low,
}

// ---------------------------------------------------------------------------
// Lip Sync
// ---------------------------------------------------------------------------

/// A single viseme frame for lip-sync animation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LipSyncFrame {
    /// Timestamp in milliseconds within the audio.
    pub timestamp_ms: u64,
    /// The viseme being displayed.
    pub viseme: Viseme,
    /// Intensity from 0.0 to 1.0.
    pub intensity: f32,
}

/// Viseme phoneme categories for mouth animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Viseme {
    PP,
    FF,
    TH,
    DD,
    Kk,
    CH,
    SS,
    Nn,
    RR,
    Aa,
    Ee,
    Ih,
    Oh,
    Ou,
    Rest,
}

impl Viseme {
    /// Returns the index of this viseme (0-14).
    pub fn index(&self) -> usize {
        match self {
            Viseme::PP => 0,
            Viseme::FF => 1,
            Viseme::TH => 2,
            Viseme::DD => 3,
            Viseme::Kk => 4,
            Viseme::CH => 5,
            Viseme::SS => 6,
            Viseme::Nn => 7,
            Viseme::RR => 8,
            Viseme::Aa => 9,
            Viseme::Ee => 10,
            Viseme::Ih => 11,
            Viseme::Oh => 12,
            Viseme::Ou => 13,
            Viseme::Rest => 14,
        }
    }
}

/// Lip sync data extracted from TTS audio output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LipSyncData {
    pub frames: Vec<LipSyncFrame>,
    pub duration_ms: u64,
    pub sample_rate: u32,
}

// ---------------------------------------------------------------------------
// SSML Processing
// ---------------------------------------------------------------------------

/// Transformations applied to text before TTS synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SSMLTransformation {
    /// Spell out numbers in context-appropriate form.
    NormalizeNumbers,
    /// Speak file extensions character-by-character.
    SpellExtensions,
    /// Summarize URLs.
    SummarizeUrls,
    /// Strip markdown formatting.
    StripMarkdown,
    /// Reduce volume for parenthetical content.
    ParentheticalReduction,
}

/// Configuration for the SSML processor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSMLConfig {
    pub transformations: Vec<SSMLTransformation>,
    pub break_ms: u32,
}

impl Default for SSMLConfig {
    fn default() -> Self {
        Self {
            transformations: vec![
                SSMLTransformation::NormalizeNumbers,
                SSMLTransformation::SpellExtensions,
                SSMLTransformation::SummarizeUrls,
                SSMLTransformation::StripMarkdown,
                SSMLTransformation::ParentheticalReduction,
            ],
            break_ms: 200,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_model_sizes() {
        assert_eq!(WhisperModelSize::Tiny.size_mb(), 39);
        assert_eq!(WhisperModelSize::Small.size_mb(), 244);
        assert_eq!(WhisperModelSize::LargeV3.size_mb(), 1500);
    }

    #[test]
    fn test_viseme_index() {
        assert_eq!(Viseme::PP.index(), 0);
        assert_eq!(Viseme::Aa.index(), 9);
        assert_eq!(Viseme::Rest.index(), 14);
    }

    #[test]
    fn test_vad_config_default() {
        let config = VADConfig::default();
        assert_eq!(config.speech_start_ms, 150);
        assert_eq!(config.speech_end_ms, 800);
    }

    #[test]
    fn test_default_wake_word() {
        let config = WakeWordConfig::default();
        assert_eq!(config.primary_phrase, "Hey Lumi");
        assert!(config.alternative_phrases.is_empty());
    }
}
