//! # Character Engine Lifecycle
//!
//! Describes the Character Engine's own internal readiness as a software component.
//! This is NOT Lumi's behavioral state — see `lumas_state::CharacterMachine` for that.
//!
//! # Authority
//! Character Engine — internal lifecycle.
//!
//! # Does NOT
//! - Define behavioral states for the character
//! - Interact with the State Machine manager directly (that's `CharacterManager`)

use std::borrow::Cow;

/// Lifecycle of the CHARACTER ENGINE ITSELF as a software component
/// (has it loaded its profile from disk, is it ready to accept behavior queries).
/// This is NOT Lumi's behavioral state — see `lumas_state::CharacterMachine` for that.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineLifecycle {
    /// Engine has been created but not initialized.
    Created,
    /// Engine is loading the character profile from persistence.
    LoadingProfile,
    /// Engine is fully ready for behavior selection and tick updates.
    Ready,
    /// Engine is running with reduced functionality.
    Degraded {
        /// Description of the degradation.
        reason: Cow<'static, str>,
    },
    /// Engine is shutting down.
    ShuttingDown,
    /// Engine has stopped.
    Stopped,
}

impl EngineLifecycle {
    /// Whether the engine can process tick updates.
    pub fn can_tick(&self) -> bool {
        matches!(self, Self::Ready | Self::Degraded { .. })
    }

    /// Whether the engine has been initialized (loading done, regardless of Ready or Degraded).
    pub fn is_initialized(&self) -> bool {
        !matches!(self, Self::Created | Self::LoadingProfile | Self::Stopped)
    }
}
