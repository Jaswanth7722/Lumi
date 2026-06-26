//! # Audio System — Sound Effects and Ambient Audio (Chapter 37)
//!
//! Defines sound identifiers, audio channels, volume control,
//! and the procedural sound synthesis system.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Sound IDs
// ---------------------------------------------------------------------------

/// Identifiers for all Lumi sound effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SoundId {
    /// Wake word detected — soft ascending chime.
    Wake,
    /// Inference start — subtle crystal resonance.
    Thinking,
    /// Task complete — warm ascending tone.
    Success,
    /// Error state — soft descending minor interval.
    Error,
    /// Desktop notification received — gentle bell.
    Notification,
    /// Sleep state entry — quiet fading tone.
    Sleep,
    /// Sleep state exit — soft ascending crystal tone.
    WakeUp,
}

impl SoundId {
    /// Description of the sound effect.
    pub fn description(&self) -> &'static str {
        match self {
            SoundId::Wake => "Soft ascending chime on wake word detection",
            SoundId::Thinking => "Subtle crystal resonance during inference",
            SoundId::Success => "Warm ascending tone on task completion",
            SoundId::Error => "Soft descending minor interval on error",
            SoundId::Notification => "Gentle bell on desktop notification",
            SoundId::Sleep => "Quiet fading tone on sleep entry",
            SoundId::WakeUp => "Soft ascending crystal tone on wake",
        }
    }

    /// Whether this sound loops.
    pub fn looping(&self) -> bool {
        matches!(self, SoundId::Thinking)
    }
}

// ---------------------------------------------------------------------------
// Audio Configuration
// ---------------------------------------------------------------------------

/// Configuration for the audio engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Master volume (0.0 to 1.0).
    pub master_volume: f32,
    /// Sound effects volume (0.0 to 1.0).
    pub sfx_volume: f32,
    /// Whether sound effects are enabled.
    pub sfx_enabled: bool,
    /// Whether sounds play during focus mode.
    pub play_in_focus_mode: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            master_volume: 0.7,
            sfx_volume: 0.5,
            sfx_enabled: true,
            play_in_focus_mode: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Audio Channels
// ---------------------------------------------------------------------------

/// Audio output channel for managing separate volume and mixing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioChannel {
    /// TTS voice output.
    Voice,
    /// Sound effects.
    Sfx,
    /// Ambient/background audio.
    Ambient,
}

/// A request to play a sound effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaySoundRequest {
    pub sound_id: SoundId,
    pub volume: f32,
    pub channel: AudioChannel,
    pub loop_count: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_descriptions() {
        assert!(SoundId::Wake.description().contains("chime"));
        assert!(SoundId::Error.description().contains("descending"));
        assert!(SoundId::Success.description().contains("ascending"));
    }

    #[test]
    fn test_looping_sounds() {
        assert!(SoundId::Thinking.looping());
        assert!(!SoundId::Wake.looping());
        assert!(!SoundId::Success.looping());
    }

    #[test]
    fn test_audio_config_default() {
        let config = AudioConfig::default();
        assert!((config.master_volume - 0.7).abs() < f32::EPSILON);
        assert!(config.sfx_enabled);
        assert!(!config.play_in_focus_mode);
    }

    #[test]
    fn test_play_sound_request() {
        let req = PlaySoundRequest {
            sound_id: SoundId::Success,
            volume: 0.8,
            channel: AudioChannel::Sfx,
            loop_count: None,
        };
        assert_eq!(req.sound_id, SoundId::Success);
        assert_eq!(req.channel, AudioChannel::Sfx);
    }

    #[test]
    fn test_sound_id_serialization() {
        let ids = vec![
            SoundId::Wake,
            SoundId::Thinking,
            SoundId::Success,
            SoundId::Error,
            SoundId::Notification,
            SoundId::Sleep,
            SoundId::WakeUp,
        ];
        for id in ids {
            let json = serde_json::to_value(&id).unwrap();
            let back: SoundId = serde_json::from_value(json).unwrap();
            assert_eq!(format!("{id:?}"), format!("{back:?}"));
        }
    }
}
