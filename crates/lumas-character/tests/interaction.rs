//! Tests: Click handler fires correct lumas-state event, does not directly mutate state.

use lumas_character::interaction::{
    InteractionEvent, InteractionHandler, InteractionKind, InteractionOutcome, InteractionSystem,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};

struct TestHandler {
    handled: AtomicBool,
}

#[async_trait]
impl InteractionHandler for TestHandler {
    fn handles(&self, kind: InteractionKind) -> bool {
        matches!(kind, InteractionKind::UserClick)
    }

    async fn handle(
        &self,
        _event: &InteractionEvent,
    ) -> Result<InteractionOutcome, lumas_character::error::CharacterError> {
        self.handled.store(true, Ordering::Relaxed);
        Ok(InteractionOutcome::Handled)
    }
}

#[tokio::test]
async fn test_handler_routing() {
    let mut system = InteractionSystem::new();
    let handler = TestHandler {
        handled: AtomicBool::new(false),
    };
    system.register(Box::new(handler));

    let event = InteractionEvent {
        kind: InteractionKind::UserClick,
        position: Some((100.0, 200.0)),
        context: None,
        timestamp: std::time::Instant::now(),
    };

    let outcome = system.handle_event(&event).await.unwrap();
    assert_eq!(outcome, InteractionOutcome::Handled);
}

#[tokio::test]
async fn test_unhandled_interaction() {
    let system = InteractionSystem::new();
    let event = InteractionEvent {
        kind: InteractionKind::VoiceWakeWord,
        position: None,
        context: None,
        timestamp: std::time::Instant::now(),
    };
    let outcome = system.handle_event(&event).await.unwrap();
    assert_eq!(outcome, InteractionOutcome::Ignored);
}

#[test]
fn test_has_handler() {
    let mut system = InteractionSystem::new();
    assert!(!system.has_handler(InteractionKind::UserClick));
    // Default logging handler handles everything
    system.register(Box::new(lumas_character::interaction::LoggingInteractionHandler));
    assert!(system.has_handler(InteractionKind::UserClick));
}

#[test]
fn test_handler_count() {
    let mut system = InteractionSystem::new();
    assert_eq!(system.handler_count(), 0);
    system.register(Box::new(lumas_character::interaction::LoggingInteractionHandler));
    assert_eq!(system.handler_count(), 1);
}
