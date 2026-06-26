//! # Speech-to-Text Engine (Chapter 13.5)
//!
//! Local speech transcription using Whisper.cpp.
//! Processes audio buffers from Voice Activity Detection and returns
//! transcribed text with confidence scores.

use lumi_common::voice::WhisperModelSize;

/// Configuration for the STT engine.
pub struct STTConfig {
    pub model_size: WhisperModelSize,
    pub language: Option<String>,
    pub beam_size: u32,
    pub confidence_threshold: f32,
}

impl Default for STTConfig {
    fn default() -> Self {
        Self {
            model_size: WhisperModelSize::Small,
            language: Some("en".into()),
            beam_size: 5,
            confidence_threshold: 0.7,
        }
    }
}

/// The result of a transcription operation.
pub struct TranscriptionResult {
    pub text: String,
    pub confidence: f32,
    pub duration_ms: u64,
    pub language: String,
}

/// Speech-to-Text engine using Whisper.cpp.
pub struct STTEngine {
    /// Whether the Whisper model is loaded.
    model_loaded: bool,
    /// Current engine configuration.
    config: STTConfig,
}

impl STTEngine {
    pub fn new() -> Self {
        Self {
            model_loaded: false,
            config: STTConfig::default(),
        }
    }

    /// Load the Whisper model (in production, loads via whisper.cpp FFI).
    pub fn load_model(&mut self, _size: WhisperModelSize) {
        // In production: load the GGML model file and initialize Whisper context
        self.model_loaded = true;
    }

    /// Transcribe audio samples to text.
    pub fn transcribe(&self, _samples: &[f32], _sample_rate: u32) -> Option<TranscriptionResult> {
        if !self.model_loaded {
            return None;
        }

        // In production: run Whisper inference via whisper.cpp FFI
        Some(TranscriptionResult {
            text: String::new(),
            confidence: 0.0,
            duration_ms: 0,
            language: self.config.language.clone().unwrap_or("en".into()),
        })
    }

    /// Check if the model is loaded.
    pub fn is_loaded(&self) -> bool {
        self.model_loaded
    }

    /// Get the engine configuration.
    pub fn config(&self) -> &STTConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = STTConfig::default();
        assert_eq!(config.model_size, WhisperModelSize::Small);
        assert_eq!(config.language, Some("en".into()));
        assert_eq!(config.beam_size, 5);
    }

    #[test]
    fn test_model_loading() {
        let mut engine = STTEngine::new();
        assert!(!engine.is_loaded());
        engine.load_model(WhisperModelSize::Base);
        assert!(engine.is_loaded());
    }

    #[test]
    fn test_transcription_without_model() {
        let engine = STTEngine::new();
        assert!(engine.transcribe(&[0.0; 16000], 16000).is_none());
    }
}
