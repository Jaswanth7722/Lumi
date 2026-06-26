//! # Emotion System — Emotion State and Sentiment (Chapter 19)
//!
//! Defines the emotion state model, emotion transition configuration,
//! sentiment analysis signals, and the AI state → emotion mapping.

use serde::{Deserialize, Serialize};
use crate::ai::AIState;
use crate::character::CrystalColor;
use crate::animation::EarPose;

// ---------------------------------------------------------------------------
// Emotion State Model
// ---------------------------------------------------------------------------

/// A complete emotion state for the Lumi character.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionState {
    /// Primary emotion being expressed.
    pub primary: Emotion,
    /// Optional secondary emotion for blended expressions.
    pub secondary: Option<Emotion>,
    /// Blend weight between primary and secondary (0.0 = all primary, 1.0 = all secondary).
    pub blend_weight: f32,
    /// Emotion intensity from 0.0 to 1.0.
    pub intensity: f32,
    /// Expected hold time in milliseconds before the emotion may change.
    pub duration_ms: u64,
}

/// Emotional states Lumi can express, mapped to genuine AI processing states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Emotion {
    /// Default resting state.
    Neutral,
    /// AI is exploring or processing an interesting problem.
    Curious,
    /// Active, attentive listening state.
    Engaged,
    /// Mid-inference processing.
    Thinking,
    /// Task completed, user expressed satisfaction.
    Happy,
    /// Potential issue detected, needs clarification.
    Concerned,
    /// Intensive task execution.
    Focused,
    /// Unexpected input or discovery.
    Surprised,
    /// Error occurred, recovery mode.
    Apologetic,
    /// Idle, resting presence.
    Calm,
    /// Notification received or attention needed.
    Alert,
    /// Significant task successfully completed.
    Proud,
}

// ---------------------------------------------------------------------------
// Emotion Transitions
// ---------------------------------------------------------------------------

/// Configuration for how an emotion transitions in and out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionTransitionConfig {
    /// Fade-in duration in milliseconds.
    pub fade_in_ms: u64,
    /// Fade-out duration in milliseconds.
    pub fade_out_ms: u64,
    /// Minimum hold time before emotion can change.
    pub min_hold_ms: u64,
}

/// Emotion transition configurations keyed by emotion.
pub fn default_emotion_configs() -> Vec<(Emotion, EmotionTransitionConfig)> {
    vec![
        (Emotion::Neutral, EmotionTransitionConfig { fade_in_ms: 300, fade_out_ms: 300, min_hold_ms: 0 }),
        (Emotion::Curious, EmotionTransitionConfig { fade_in_ms: 300, fade_out_ms: 500, min_hold_ms: 1000 }),
        (Emotion::Engaged, EmotionTransitionConfig { fade_in_ms: 200, fade_out_ms: 400, min_hold_ms: 800 }),
        (Emotion::Thinking, EmotionTransitionConfig { fade_in_ms: 400, fade_out_ms: 600, min_hold_ms: 500 }),
        (Emotion::Happy, EmotionTransitionConfig { fade_in_ms: 200, fade_out_ms: 800, min_hold_ms: 1500 }),
        (Emotion::Concerned, EmotionTransitionConfig { fade_in_ms: 400, fade_out_ms: 700, min_hold_ms: 1200 }),
        (Emotion::Focused, EmotionTransitionConfig { fade_in_ms: 200, fade_out_ms: 500, min_hold_ms: 2000 }),
        (Emotion::Surprised, EmotionTransitionConfig { fade_in_ms: 80, fade_out_ms: 400, min_hold_ms: 600 }),
        (Emotion::Apologetic, EmotionTransitionConfig { fade_in_ms: 600, fade_out_ms: 1200, min_hold_ms: 2000 }),
        (Emotion::Calm, EmotionTransitionConfig { fade_in_ms: 400, fade_out_ms: 600, min_hold_ms: 1000 }),
        (Emotion::Alert, EmotionTransitionConfig { fade_in_ms: 100, fade_out_ms: 600, min_hold_ms: 800 }),
        (Emotion::Proud, EmotionTransitionConfig { fade_in_ms: 300, fade_out_ms: 1000, min_hold_ms: 2000 }),
    ]
}

// ---------------------------------------------------------------------------
// Sentiment Analysis
// ---------------------------------------------------------------------------

/// Signals from sentiment analysis that adjust emotional state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SentimentSignal {
    /// User expressed positive feedback.
    UserPositive { strength: f32 },
    /// User expressed negative feedback.
    UserNegative { strength: f32 },
    /// User appears frustrated (repeated corrections, short responses).
    UserFrustrated,
    /// User is engaged (long detailed messages, questions).
    UserEngaged,
    /// A plan completed successfully.
    TaskSuccess,
    /// A plan failed.
    TaskFailed,
}

// ---------------------------------------------------------------------------
// AI State → Emotion Mapping
// ---------------------------------------------------------------------------

/// Maps AI processing states to the character's visual emotion expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionMapping {
    pub ai_state: AIState,
    pub primary_emotion: Emotion,
    pub crystal_color: CrystalColor,
    pub ear_pose: EarPose,
    pub body_posture: BodyPosture,
}

/// High-level body posture descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyPosture {
    Relaxed,
    Upright,
    LeaningForward,
    HeadTilt,
    Bounce,
    Tense,
    Droop,
    LyingDown,
    Attentive,
    Natural,
}

/// Returns the emotion mapping for a given AI state.
pub fn emotion_mapping_for_ai_state(ai_state: &AIState) -> EmotionMapping {
    match ai_state {
        AIState::Idle => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Neutral,
            crystal_color: CrystalColor::BlueDefault,
            ear_pose: EarPose::neutral(),
            body_posture: BodyPosture::Relaxed,
        },
        AIState::Thinking => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Thinking,
            crystal_color: CrystalColor::BlueDefault,
            ear_pose: EarPose { forward_back: 0.3, up_down: 0.5 },
            body_posture: BodyPosture::HeadTilt,
        },
        AIState::Planning => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Focused,
            crystal_color: CrystalColor::BlueDefault,
            ear_pose: EarPose { forward_back: 0.5, up_down: 0.6 },
            body_posture: BodyPosture::Upright,
        },
        AIState::ExecutingTool => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Focused,
            crystal_color: CrystalColor::BlueDefault,
            ear_pose: EarPose { forward_back: 0.5, up_down: 0.6 },
            body_posture: BodyPosture::LeaningForward,
        },
        AIState::Listening | AIState::ReceivingInput => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Engaged,
            crystal_color: CrystalColor::BlueDefault,
            ear_pose: EarPose { forward_back: 1.0, up_down: 0.8 },
            body_posture: BodyPosture::Attentive,
        },
        AIState::Speaking => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Engaged,
            crystal_color: CrystalColor::BlueDefault,
            ear_pose: EarPose { forward_back: 0.3, up_down: 0.3 },
            body_posture: BodyPosture::Natural,
        },
        AIState::Success => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Happy,
            crystal_color: CrystalColor::GreenSuccess,
            ear_pose: EarPose { forward_back: 0.6, up_down: 1.0 },
            body_posture: BodyPosture::Bounce,
        },
        AIState::Error => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Apologetic,
            crystal_color: CrystalColor::RedError,
            ear_pose: EarPose { forward_back: -0.5, up_down: -0.3 },
            body_posture: BodyPosture::Droop,
        },
        AIState::RetrievingMemory => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Curious,
            crystal_color: CrystalColor::PurpleMemory,
            ear_pose: EarPose { forward_back: 0.0, up_down: 0.0 },
            body_posture: BodyPosture::HeadTilt,
        },
        AIState::AwaitingConfirmation => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Curious,
            crystal_color: CrystalColor::BlueDefault,
            ear_pose: EarPose { forward_back: 0.7, up_down: 0.4 },
            body_posture: BodyPosture::Attentive,
        },
        AIState::GeneratingResponse => EmotionMapping {
            ai_state: ai_state.clone(),
            primary_emotion: Emotion::Thinking,
            crystal_color: CrystalColor::BlueDefault,
            ear_pose: EarPose { forward_back: 0.3, up_down: 0.5 },
            body_posture: BodyPosture::HeadTilt,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_ai_states_have_mappings() {
        let states = vec![
            AIState::Idle,
            AIState::Thinking,
            AIState::Planning,
            AIState::ExecutingTool,
            AIState::Listening,
            AIState::Speaking,
            AIState::Success,
            AIState::Error,
            AIState::RetrievingMemory,
            AIState::AwaitingConfirmation,
            AIState::GeneratingResponse,
            AIState::ReceivingInput,
        ];
        for state in states {
            let mapping = emotion_mapping_for_ai_state(&state);
            assert_eq!(mapping.ai_state, state);
        }
    }

    #[test]
    fn test_emotion_transition_configs() {
        let configs = default_emotion_configs();
        let emotions: std::collections::HashSet<_> = configs.iter().map(|(e, _)| e).collect();
        assert!(emotions.contains(&Emotion::Happy));
        assert!(emotions.contains(&Emotion::Thinking));
        assert!(emotions.contains(&Emotion::Alert));
        // 12 unique emotions should be present
        assert_eq!(emotions.len(), 12);
    }

    #[test]
    fn test_success_mapping() {
        let mapping = emotion_mapping_for_ai_state(&AIState::Success);
        assert_eq!(mapping.primary_emotion, Emotion::Happy);
        assert_eq!(mapping.crystal_color, CrystalColor::GreenSuccess);
        assert_eq!(mapping.body_posture, BodyPosture::Bounce);
    }

    #[test]
    fn test_error_mapping() {
        let mapping = emotion_mapping_for_ai_state(&AIState::Error);
        assert_eq!(mapping.primary_emotion, Emotion::Apologetic);
        assert_eq!(mapping.crystal_color, CrystalColor::RedError);
        assert_eq!(mapping.body_posture, BodyPosture::Droop);
    }
}
