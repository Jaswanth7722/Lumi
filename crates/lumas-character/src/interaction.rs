//! # Interaction System
//!
//! Handles user interactions with the character. Interaction handlers fire
//! state machine events and/or select behaviors — they never directly
//! manipulate render state or animation.
//!
//! # Authority
//! Character Engine — interaction response selection.
//!
//! # Does NOT
//! - Directly mutate render state or animation (fires events instead)
//! - Force state transitions (fires events, state machine guards evaluate them)

use crate::error::{CharacterError, CharacterResult};
use async_trait::async_trait;

/// Types of interactions the character can respond to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    /// Left-click on the character.
    UserClick,
    /// Right-click on the character.
    UserRightClick,
    /// User started dragging the character.
    UserDragStart,
    /// User stopped dragging the character.
    UserDragEnd,
    /// Voice wake word detected.
    VoiceWakeWord,
    /// A notification was received from another application.
    NotificationReceived,
    /// The active foreground window changed.
    WindowFocusChanged,
}

/// An interaction event with associated data.
#[derive(Debug, Clone)]
pub struct InteractionEvent {
    /// The kind of interaction.
    pub kind: InteractionKind,
    /// Screen-space position of the interaction (if applicable).
    pub position: Option<(f32, f32)>,
    /// Additional context (application name, notification content, etc.).
    pub context: Option<String>,
    /// Timestamp of the interaction.
    pub timestamp: std::time::Instant,
}

/// Outcome of handling an interaction.
#[derive(Debug, Clone)]
pub enum InteractionOutcome {
    /// Interaction was handled successfully.
    Handled,
    /// Interaction was ignored (no handler registered for this kind).
    Ignored,
    /// Interaction handler deferred processing (will complete asynchronously).
    Deferred,
}

/// Trait for handling character interactions.
///
/// Implementations register with the `InteractionSystem` and respond to
/// interaction events by firing state machine events and/or selecting behaviors.
#[async_trait]
pub trait InteractionHandler: Send + Sync + std::fmt::Debug {
    /// Whether this handler can process the given interaction kind.
    fn handles(&self, kind: InteractionKind) -> bool;

    /// Handle the interaction event.
    async fn handle(
        &self,
        event: &InteractionEvent,
    ) -> CharacterResult<InteractionOutcome>;
}

/// System that routes interaction events to registered handlers.
#[derive(Debug, Default)]
pub struct InteractionSystem {
    handlers: Vec<Box<dyn InteractionHandler>>,
}

impl InteractionSystem {
    /// Create a new interaction system.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an interaction handler.
    pub fn register(&mut self, handler: Box<dyn InteractionHandler>) {
        self.handlers.push(handler);
    }

    /// Route an interaction event to the appropriate handler.
    pub async fn handle_event(&self, event: &InteractionEvent) -> CharacterResult<InteractionOutcome> {
        for handler in &self.handlers {
            if handler.handles(event.kind) {
                return handler.handle(event).await;
            }
        }
        Ok(InteractionOutcome::Ignored)
    }

    /// Check if there is a handler for the given interaction kind.
    pub fn has_handler(&self, kind: InteractionKind) -> bool {
        self.handlers.iter().any(|h| h.handles(kind))
    }

    /// Number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }
}

/// A simple interaction handler that logs interactions (useful as a default).
#[derive(Debug)]
pub struct LoggingInteractionHandler;

#[async_trait]
impl InteractionHandler for LoggingInteractionHandler {
    fn handles(&self, _kind: InteractionKind) -> bool {
        true
    }

    async fn handle(
        &self,
        _event: &InteractionEvent,
    ) -> CharacterResult<InteractionOutcome> {
        // In production, this would emit a log or diagnostic event
        Ok(InteractionOutcome::Handled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestClickHandler {
        handled: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl InteractionHandler for TestClickHandler {
        fn handles(&self, kind: InteractionKind) -> bool {
            matches!(kind, InteractionKind::UserClick)
        }

        async fn handle(
            &self,
            _event: &InteractionEvent,
        ) -> CharacterResult<InteractionOutcome> {
            self.handled.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(InteractionOutcome::Handled)
        }
    }

    #[tokio::test]
    async fn test_handler_routing() {
        let mut system = InteractionSystem::new();
        let handler = TestClickHandler {
            handled: std::sync::atomic::AtomicBool::new(false),
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
            kind: InteractionKind::UserClick,
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
        assert!(!system.has_handler(InteractionKind::VoiceWakeWord));

        system.register(Box::new(LoggingInteractionHandler));
        assert!(system.has_handler(InteractionKind::VoiceWakeWord));
    }
}
