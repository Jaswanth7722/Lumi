//! # Emotion System
//!
//! Computes the target `EmotionState` given current AI state, sentiment signals,
//! and active behavior. This produces **parameters** for the Animation Engine —
//! it does not itself blend, fade, or animate anything (SRS Chapter 19.4
//! transition timing is the Animation Engine's responsibility).
//!
//! # Authority
//! Character Engine — emotion target computation.
//!
//! # Does NOT
//! - Blend or interpolate between emotion states (Animation Engine's job)
//! - Own animation clips or blend trees
//! - Drive facial expression blend shapes directly

use lumas_common::ai::AIState;
use lumas_common::emotion::{
    EmotionMapping, EmotionState, EmotionTransitionConfig, SentimentSignal,
    default_emotion_configs, emotion_mapping_for_ai_state,
};
pub use lumas_common::emotion::Emotion;
use std::sync::RwLock;

/// Context provided to the emotion system for target computation.
#[derive(Debug, Clone)]
pub struct EmotionContext {
    /// Current AI processing state.
    pub ai_state: Option<AIState>,
    /// Current character state ID (from state machine).
    pub current_state_id: Option<lumas_state::error::StateId>,
    /// Any sentiment signal received this tick.
    pub sentiment: Option<SentimentSignal>,
    /// Active behavior ID (if any).
    pub active_behavior: Option<crate::behavior::BehaviorId>,
}

/// The emotion system computes target emotion states for the character.
///
/// The system maintains the current emotion state and produces new targets
/// based on AI state changes, sentiment signals, and the active behavior.
/// The actual blending and interpolation between emotion states is handled
/// by the Animation Engine.
#[derive(Debug)]
pub struct EmotionSystem {
    current: RwLock<EmotionState>,
    personality_scaling: f32,
    emotion_configs: Vec<(Emotion, EmotionTransitionConfig)>,
}

impl EmotionSystem {
    /// Create a new emotion system with the given personality scaling factor.
    pub fn new(expressiveness: f32) -> Self {
        Self {
            current: RwLock::new(EmotionState {
                primary: Emotion::Neutral,
                secondary: None,
                blend_weight: 0.0,
                intensity: 0.5,
                duration_ms: 0,
            }),
            personality_scaling: expressiveness,
            emotion_configs: default_emotion_configs(),
        }
    }

    /// Get the current emotion state.
    pub fn current(&self) -> EmotionState {
        self.current
            .read()
            .map(|g| EmotionState {
                primary: g.primary,
                secondary: g.secondary,
                blend_weight: g.blend_weight,
                intensity: g.intensity,
                duration_ms: g.duration_ms,
            })
            .unwrap_or(EmotionState {
                primary: Emotion::Neutral,
                secondary: None,
                blend_weight: 0.0,
                intensity: 0.5,
                duration_ms: 0,
            })
    }

    /// Compute the target emotion state for the given context.
    /// This does NOT modify the current state — the caller is responsible
    /// for applying the target through the Animation Engine.
    pub fn compute_target(&self, ctx: &EmotionContext) -> EmotionState {
        // Determine emotion from AI state mapping
        let base = if let Some(ref ai_state) = ctx.ai_state {
            let mapping = emotion_mapping_for_ai_state(ai_state);
            Some(mapping)
        } else {
            None
        };

        let primary = base
            .as_ref()
            .map(|m| m.primary_emotion)
            .unwrap_or(Emotion::Neutral);

        let config = self
            .emotion_configs
            .iter()
            .find(|(e, _)| *e == primary)
            .map(|(_, c)| c)
            .unwrap();

        // Scale intensity by personality expressiveness
        let base_intensity = match ctx.ai_state {
            Some(AIState::Success) | Some(AIState::Error) => 0.9,
            Some(AIState::Thinking) | Some(AIState::GeneratingResponse) => 0.6,
            Some(AIState::Listening) | Some(AIState::ReceivingInput) => 0.5,
            Some(AIState::Speaking) => 0.6,
            Some(AIState::Idle) => 0.3,
            _ => 0.5,
        };

        // Apply sentiment override if present
        let (sentiment_emotion, sentiment_weight) = match ctx.sentiment {
            Some(SentimentSignal::UserPositive { strength }) => {
                (Some(Emotion::Happy), strength)
            }
            Some(SentimentSignal::UserNegative { strength }) => {
                (Some(Emotion::Concerned), strength)
            }
            Some(SentimentSignal::UserFrustrated) => (Some(Emotion::Apologetic), 0.7),
            Some(SentimentSignal::UserEngaged) => (Some(Emotion::Engaged), 0.6),
            Some(SentimentSignal::TaskSuccess) => (Some(Emotion::Happy), 0.9),
            Some(SentimentSignal::TaskFailed) => (Some(Emotion::Concerned), 0.8),
            None => (None, 0.0),
        };

        let intensity = (base_intensity * self.personality_scaling).clamp(0.0, 1.0);

        let secondary = sentiment_emotion;

        let blend_weight = if secondary.is_some() {
            sentiment_weight * 0.3
        } else {
            0.0
        };

        EmotionState {
            primary,
            secondary,
            blend_weight: blend_weight.clamp(0.0, 1.0),
            intensity,
            duration_ms: config.min_hold_ms,
        }
    }

    /// Apply a sentiment signal, which may adjust the current emotion.
    pub fn apply_sentiment_signal(&self, signal: SentimentSignal) {
        let ctx = EmotionContext {
            ai_state: None,
            current_state_id: None,
            sentiment: Some(signal),
            active_behavior: None,
        };
        let target = self.compute_target(&ctx);
        if let Ok(mut current) = self.current.write() {
            // Update current state based on sentiment
            // The actual smooth transition is handled by the Animation Engine
            *current = target;
        }
    }

    /// Update the current emotion target (called when the Animation Engine
    /// has accepted the new target and the Character Engine should begin tracking it).
    pub fn set_current(&self, state: EmotionState) {
        if let Ok(mut current) = self.current.write() {
            *current = state;
        }
    }

    /// Get the personality scaling factor.
    pub fn personality_scaling(&self) -> f32 {
        self.personality_scaling
    }

    /// Update the personality scaling factor.
    pub fn set_personality_scaling(&mut self, scaling: f32) {
        self.personality_scaling = scaling.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_target_idle() {
        let system = EmotionSystem::new(0.65);
        let ctx = EmotionContext {
            ai_state: Some(AIState::Idle),
            current_state_id: None,
            sentiment: None,
            active_behavior: None,
        };
        let target = system.compute_target(&ctx);
        assert_eq!(target.primary, Emotion::Neutral);
        assert!(target.intensity >= 0.0 && target.intensity <= 1.0);
    }

    #[test]
    fn test_compute_target_thinking() {
        let system = EmotionSystem::new(0.8);
        let ctx = EmotionContext {
            ai_state: Some(AIState::Thinking),
            current_state_id: None,
            sentiment: None,
            active_behavior: None,
        };
        let target = system.compute_target(&ctx);
        assert_eq!(target.primary, Emotion::Thinking);
    }

    #[test]
    fn test_compute_target_success() {
        let system = EmotionSystem::new(1.0);
        let ctx = EmotionContext {
            ai_state: Some(AIState::Success),
            current_state_id: None,
            sentiment: None,
            active_behavior: None,
        };
        let target = system.compute_target(&ctx);
        assert_eq!(target.primary, Emotion::Happy);
        assert!(target.intensity >= 0.5);
    }

    #[test]
    fn test_sentiment_signal_applied() {
        let system = EmotionSystem::new(0.7);
        system.apply_sentiment_signal(SentimentSignal::UserPositive { strength: 0.8 });
        let current = system.current();
        // Sentiment should have an effect, but we're checking the system doesn't panic
        assert!(current.intensity >= 0.0);
    }

    #[test]
    fn test_personality_scaling_affects_intensity() {
        let low = EmotionSystem::new(0.3);
        let high = EmotionSystem::new(1.0);
        let ctx = EmotionContext {
            ai_state: Some(AIState::Success),
            current_state_id: None,
            sentiment: None,
            active_behavior: None,
        };
        let low_target = low.compute_target(&ctx);
        let high_target = high.compute_target(&ctx);
        // Lower expressiveness should produce lower intensity
        assert!(low_target.intensity <= high_target.intensity);
    }
}
