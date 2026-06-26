//! # Audio System — Sound Playback and Channel Management (Chapter 37)

use lumi_common::audio::{AudioChannel, AudioConfig, SoundId};
use tracing::debug;

/// Manages audio output including TTS and sound effects playback.
pub struct AudioEngine {
    config: AudioConfig,
    focus_mode: bool,
    muted: bool,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            config: AudioConfig::default(),
            focus_mode: false,
            muted: false,
        }
    }

    pub fn play_sfx(&self, sound_id: SoundId) {
        if !self.config.sfx_enabled || self.focus_mode || self.muted {
            return;
        }
        debug!(
            "Playing SFX: {:?} (volume: {})",
            sound_id, self.config.sfx_volume
        );
    }

    pub fn play_tts(&self) {
        if self.muted || self.focus_mode {
            return;
        }
        debug!("Playing TTS audio");
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.config.master_volume = volume.clamp(0.0, 1.0);
    }

    pub fn set_sfx_volume(&mut self, volume: f32) {
        self.config.sfx_volume = volume.clamp(0.0, 1.0);
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        debug!("Audio {}", if muted { "muted" } else { "unmuted" });
    }

    pub fn set_focus_mode(&mut self, active: bool) {
        self.focus_mode = active;
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn master_volume(&self) -> f32 {
        self.config.master_volume
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_volume() {
        let engine = AudioEngine::new();
        assert!((engine.master_volume() - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mute() {
        let mut engine = AudioEngine::new();
        assert!(!engine.is_muted());
        engine.set_muted(true);
        assert!(engine.is_muted());
        engine.play_sfx(SoundId::Success); // Should not panic
    }

    #[test]
    fn test_focus_mode() {
        let mut engine = AudioEngine::new();
        engine.set_focus_mode(true);
        engine.play_sfx(SoundId::Success); // Should be no-op
    }
}
