//! # Character — Public API
//!
//! `Character` is the primary public API for interacting with the character.
//! It wraps `CharacterEngine` and provides a simplified interface for other
//! subsystems (runtime, AI Core, Desktop Awareness) to use.
//!
//! # Authority
//! Character Engine — public-facing API.
//!
//! # Does NOT
//! - Duplicate `CharacterEngine` functionality
//! - Provide direct access to internal subsystems

use crate::accessory::AccessoryRegistry;
use crate::appearance::AppearanceProfile;
use crate::config::CharacterConfig;
use crate::diagnostics::EngineDiagnostics;
use crate::error::CharacterResult;
use crate::identity::CharacterIdentity;
use crate::interaction::InteractionEvent;
use crate::manager::{CharacterEngine, TickContext};
use crate::metrics::CharacterMetrics;
use crate::movement::{MovementIntent, MovementPlanner};
use crate::persistence::CharacterPersistence;
use crate::position::MonitorInfo;
use lumas_common::ai::{AIState, AIStateEvent};
use lumas_common::desktop::DesktopSnapshot;
use lumas_common::emotion::{EmotionState, SentimentSignal};
use lumas_common::position::PositionTarget;
use lumas_state::error::StateId;
use lumas_state::manager::StateMachineManager;
use std::sync::Arc;

/// The primary public API for the Lumas character.
///
/// Provides high-level methods for subsystems to interact with the character.
/// Internal behavior selection, emotion computation, and expression target
/// computation happen in `CharacterEngine::tick()`.
#[derive(Clone)]
pub struct Character {
    engine: Arc<CharacterEngine>,
}

impl Character {
    /// Create and start a new character.
    pub async fn start(
        config: CharacterConfig,
        state_machine: Arc<StateMachineManager>,
        persistence: Arc<dyn CharacterPersistence>,
    ) -> CharacterResult<Self> {
        let engine = CharacterEngine::start(config, state_machine, persistence).await?;
        Ok(Self { engine })
    }

    /// Run one tick of the character engine.
    pub async fn tick(&self, ctx: &TickContext) -> CharacterResult<()> {
        self.engine.tick(ctx).await
    }

    /// Notify the character of an AI state change.
    pub fn on_ai_state_event(&self, event: AIStateEvent) {
        self.engine.on_ai_state_event(event);
    }

    /// Apply a sentiment signal (positive/negative feedback, task success/failure).
    pub fn apply_sentiment(&self, signal: SentimentSignal) {
        self.engine.apply_sentiment_signal(signal);
    }

    /// Handle a user interaction event.
    pub async fn handle_interaction(&self, event: &InteractionEvent) -> CharacterResult<()> {
        self.engine.handle_interaction(event).await
    }

    /// Set a new movement intent for the character.
    pub fn move_to(&self, intent: MovementIntent) {
        self.engine.set_movement_intent(intent);
    }

    /// Take the current movement intent (consumes it).
    /// Called by the Desktop Engine when picking up the intent.
    pub fn take_movement_intent(&self) -> Option<MovementIntent> {
        self.engine.take_movement_intent()
    }

    /// Get the character's current identity information.
    pub fn identity(&self) -> CharacterIdentity {
        self.engine.identity()
    }

    /// Get the character's current appearance profile.
    pub fn appearance(&self) -> AppearanceProfile {
        self.engine.appearance()
    }

    /// Get the accessory registry (for querying available accessories).
    pub fn accessories(&self) -> &AccessoryRegistry {
        self.engine.accessory_registry()
    }

    /// Get current emotion state for the Animation Engine.
    pub fn current_emotion(&self) -> EmotionState {
        self.engine.current_emotion_state()
    }

    /// Get the metrics collector.
    pub fn metrics(&self) -> &Arc<CharacterMetrics> {
        self.engine.metrics()
    }

    /// Get engine diagnostics snapshot.
    pub fn diagnostics(&self) -> EngineDiagnostics {
        self.engine.diagnostics()
    }

    /// Save the character profile to persistence.
    pub async fn save(&self) -> CharacterResult<()> {
        self.engine.save_profile().await
    }

    /// Shut down the character gracefully.
    pub async fn shutdown(&self) -> CharacterResult<()> {
        self.engine.shutdown().await
    }

    /// Get a reference to the inner engine (for advanced use cases).
    pub fn engine(&self) -> &CharacterEngine {
        &self.engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::InMemoryPersistence;
    use lumas_state::config::StateMachineConfig;

    #[tokio::test]
    async fn test_character_create_and_shutdown() {
        let config = CharacterConfig::default();
        let sm_config = StateMachineConfig::default();
        let state_machine = StateMachineManager::start(sm_config).await.unwrap();
        let persistence: Arc<dyn CharacterPersistence> = Arc::new(InMemoryPersistence::default());

        let character = Character::start(config, Arc::new(state_machine), persistence)
            .await
            .unwrap();

        let identity = character.identity();
        assert_eq!(identity.name, "Lumas");

        character.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_character_tick() {
        let config = CharacterConfig::default();
        let sm_config = StateMachineConfig::default();
        let state_machine = StateMachineManager::start(sm_config).await.unwrap();
        let persistence: Arc<dyn CharacterPersistence> = Arc::new(InMemoryPersistence::default());

        let character = Character::start(config, Arc::new(state_machine), persistence)
            .await
            .unwrap();

        let ctx = TickContext::new(
            StateId::new(1100), // Idle.Watching
            None,
            Some(AIState::Idle),
        );

        character.tick(&ctx).await.unwrap();
        character.shutdown().await.unwrap();
    }
}
