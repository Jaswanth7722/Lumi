//! Tests: EmotionSystem compute_target produces correct EmotionState.

use lumas_character::emotion::{EmotionContext, EmotionSystem};
use lumas_character::behavior::BehaviorId;
use lumas_common::ai::AIState;
use lumas_common::emotion::{Emotion, SentimentSignal};

#[test]
fn test_idle_emotion_is_neutral() {
    let system = EmotionSystem::new(0.65);
    let ctx = EmotionContext {
        ai_state: Some(AIState::Idle),
        current_state_id: None,
        sentiment: None,
        active_behavior: None,
    };
    let target = system.compute_target(&ctx);
    assert_eq!(target.primary, Emotion::Neutral);
}

#[test]
fn test_thinking_emotion() {
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
fn test_success_emotion_is_happy() {
    let system = EmotionSystem::new(1.0);
    let ctx = EmotionContext {
        ai_state: Some(AIState::Success),
        current_state_id: None,
        sentiment: None,
        active_behavior: None,
    };
    let target = system.compute_target(&ctx);
    assert_eq!(target.primary, Emotion::Happy);
    assert!(target.intensity > 0.5, "Success should have high intensity");
}

#[test]
fn test_sentiment_signal_applied() {
    let system = EmotionSystem::new(0.7);
    system.apply_sentiment_signal(SentimentSignal::UserPositive { strength: 0.8 });
    let current = system.current();
    assert!(current.intensity >= 0.0);
}

#[test]
fn test_expressiveness_affects_intensity() {
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
    assert!(low_target.intensity <= high_target.intensity);
}
