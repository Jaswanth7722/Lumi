//! # Emotion System — Emotional State Processing (Chapter 19)
//!
//! Translates AI processing state, conversation sentiment, and task outcomes
//! into emotional expression parameters for the character.

use lumas_common::ai::AIState;
use lumas_common::character::CrystalColor;
use lumas_common::emotion::{
    BodyPosture, Emotion, EmotionMapping, EmotionState, EmotionTransitionConfig, SentimentSignal,
    default_emotion_configs, emotion_mapping_for_ai_state,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::debug;

/// The Emotion System manages Lumi's emotional expression state.
pub struct EmotionSystem {
    /// Current emotion state.
    current: EmotionState,
    /// Previous emotion state (for transitions).
    previous: Option<EmotionState>,
    /// Transition configurations by emotion.
    transition_configs: HashMap<Emotion, EmotionTransitionConfig>,
    /// When the current emotion started.
    current_since: Instant,
    /// Timestamp of last user frustration signal (to suppress proactive behavior).
    last_frustration: Option<Instant>,
    /// Duration to suppress proactive behavior after frustration.
    frustration_cooldown: Duration,
}

impl EmotionSystem {
    pub fn new() -> Self {
        let mut configs = HashMap::new();
        for (emotion, config) in default_emotion_configs() {
            configs.insert(emotion, config);
        }

        Self {
            current: EmotionState {
                primary: Emotion::Neutral,
                secondary: None,
                blend_weight: 0.0,
                intensity: 0.5,
                duration_ms: 0,
            },
            previous: None,
            transition_configs: configs,
            current_since: Instant::now(),
            last_frustration: None,
            frustration_cooldown: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Transition to a new primary emotion with given intensity.
    pub fn transition_to(&mut self, emotion: Emotion, intensity: f32) {
        let config =
            self.transition_configs
                .get(&emotion)
                .cloned()
                .unwrap_or(EmotionTransitionConfig {
                    fade_in_ms: 300,
                    fade_out_ms: 500,
                    min_hold_ms: 500,
                });

        // Check minimum hold time
        let elapsed = self.current_since.elapsed().as_millis() as u64;
        let min_hold = self
            .transition_configs
            .get(&self.current.primary)
            .map(|c| c.min_hold_ms)
            .unwrap_or(0);

        if elapsed < min_hold && self.current.primary == emotion {
            return; // Still in minimum hold time for current emotion
        }

        self.previous = Some(self.current.clone());
        self.current = EmotionState {
            primary: emotion,
            secondary: None,
            blend_weight: 0.0,
            intensity: intensity.clamp(0.0, 1.0),
            duration_ms: config.fade_in_ms + config.min_hold_ms,
        };
        self.current_since = Instant::now();

        debug!(
            "Emotion transitioned to {:?} (intensity: {})",
            emotion, intensity
        );
    }

    /// Apply a sentiment signal from conversation analysis or task results.
    pub fn apply_signal(&mut self, signal: SentimentSignal) {
        match signal {
            SentimentSignal::UserPositive { strength } => {
                self.transition_to(Emotion::Happy, strength.clamp(0.3, 1.0));
            }
            SentimentSignal::UserNegative { strength: _ } => {
                self.transition_to(Emotion::Concerned, 0.6);
            }
            SentimentSignal::UserFrustrated => {
                self.transition_to(Emotion::Apologetic, 0.8);
                self.last_frustration = Some(Instant::now());
            }
            SentimentSignal::UserEngaged => {
                self.transition_to(Emotion::Engaged, 0.7);
            }
            SentimentSignal::TaskSuccess => {
                self.transition_to(Emotion::Happy, 0.9);
            }
            SentimentSignal::TaskFailed => {
                self.transition_to(Emotion::Concerned, 0.7);
            }
        }
    }

    /// Update emotion based on AI state (called on AI state changes).
    pub fn update_from_ai_state(&mut self, ai_state: &AIState) {
        let mapping = emotion_mapping_for_ai_state(ai_state);
        self.transition_to(mapping.primary_emotion, 0.7);
    }

    /// Check if proactive behavior should be suppressed (user frustration cooldown).
    pub fn should_suppress_proactive(&self) -> bool {
        if let Some(frustration_time) = self.last_frustration {
            frustration_time.elapsed() < self.frustration_cooldown
        } else {
            false
        }
    }

    /// Get the current emotion state.
    pub fn current_emotion(&self) -> &EmotionState {
        &self.current
    }

    /// Get the emotion mapping for the current state.
    pub fn current_mapping(&self) -> EmotionMapping {
        EmotionMapping {
            ai_state: AIState::Idle,
            primary_emotion: self.current.primary.clone(),
            crystal_color: self.crystal_color_for_emotion(&self.current.primary),
            ear_pose: lumas_common::animation::ear_pose_for_ai_state(&AIState::Idle),
            body_posture: BodyPosture::Relaxed,
        }
    }

    /// Map emotion to crystal color.
    fn crystal_color_for_emotion(&self, emotion: &Emotion) -> CrystalColor {
        match emotion {
            Emotion::Happy | Emotion::Proud => CrystalColor::GreenSuccess,
            Emotion::Apologetic | Emotion::Concerned => CrystalColor::RedError,
            Emotion::Alert => CrystalColor::AmberWarning,
            Emotion::Curious => CrystalColor::PurpleMemory,
            _ => CrystalColor::BlueDefault,
        }
    }

    /// Get the frustration cooldown remaining in seconds.
    pub fn frustration_cooldown_remaining(&self) -> Option<u64> {
        self.last_frustration.map(|t| {
            let elapsed = t.elapsed();
            if elapsed >= self.frustration_cooldown {
                0
            } else {
                (self.frustration_cooldown - elapsed).as_secs()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_emotion() {
        let system = EmotionSystem::new();
        assert_eq!(system.current_emotion().primary, Emotion::Neutral);
    }

    #[test]
    fn test_transition_to_happy() {
        let mut system = EmotionSystem::new();
        system.apply_signal(SentimentSignal::TaskSuccess);
        assert_eq!(system.current_emotion().primary, Emotion::Happy);
    }

    #[test]
    fn test_transition_to_apologetic() {
        let mut system = EmotionSystem::new();
        system.apply_signal(SentimentSignal::UserFrustrated);
        assert_eq!(system.current_emotion().primary, Emotion::Apologetic);
    }

    #[test]
    fn test_user_frustration_suppresses_proactive() {
        let mut system = EmotionSystem::new();
        assert!(!system.should_suppress_proactive());
        system.apply_signal(SentimentSignal::UserFrustrated);
        assert!(system.should_suppress_proactive());
    }

    #[test]
    fn test_crystal_color_mapping() {
        let system = EmotionSystem::new();
        assert_eq!(
            system.crystal_color_for_emotion(&Emotion::Happy),
            CrystalColor::GreenSuccess
        );
        assert_eq!(
            system.crystal_color_for_emotion(&Emotion::Neutral),
            CrystalColor::BlueDefault
        );
    }

    #[test]
    fn test_ai_state_update() {
        let mut system = EmotionSystem::new();
        system.update_from_ai_state(&AIState::Success);
        assert_eq!(system.current_emotion().primary, Emotion::Happy);
    }
}
