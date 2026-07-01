//! # Character Engine Errors
//!
//! All errors produced by the Character Engine. Each error has a stable
//! error code, severity, and diagnostic context.

use std::time::Duration;
use thiserror::Error;

/// Result alias for Character Engine operations.
pub type CharacterResult<T> = Result<T, CharacterError>;

/// Errors produced by the Character Engine.
///
/// # Authority
/// This type belongs to the Character Engine layer (per the three-layer model).
///
/// # Does NOT
/// Wrap `lumas_state::StateError` — state machine errors are propagated
/// separately via the observer pattern, not through the Character Engine's
/// result types.
#[derive(Debug, Clone, Error)]
pub enum CharacterError {
    /// Failed to load the character profile from persistence.
    #[error("Profile load failed: {cause}")]
    ProfileLoadFailed { cause: String },

    /// Profile data is corrupted or has an invalid schema version.
    #[error("Profile corrupted: {field}")]
    ProfileCorrupted { field: std::borrow::Cow<'static, str> },

    /// No behavior is applicable in the current state context.
    #[error("No behavior applicable for state {current_state:?}")]
    NoBehaviorApplicable { current_state: lumas_state::error::StateId },

    /// The state machine is not reachable (shut down or not registered).
    #[error("State machine unreachable")]
    StateMachineUnreachable,

    /// Navigation planner could not compute a valid destination.
    #[error("Navigation failed: {reason}")]
    NavigationFailed { reason: std::borrow::Cow<'static, str> },

    /// The requested accessory was not found in the registry.
    #[error("Accessory not found: {id}")]
    AccessoryNotFound { id: crate::accessory::AccessoryId },

    /// An accessory is incompatible with the requested slot.
    #[error("Accessory {accessory} incompatible with slot {slot:?}")]
    AccessorySlotIncompatible {
        accessory: crate::accessory::AccessoryId,
        slot: crate::accessory::AccessorySlotKind,
    },

    /// Personality weight value is out of the valid range.
    #[error("Invalid personality weight '{field}': {value}")]
    InvalidPersonalityWeight {
        field: &'static str,
        value: f32,
    },

    /// Position revalidation against current monitor configuration failed.
    #[error("Position revalidation failed: {cause}")]
    PositionRevalidationFailed { cause: String },

    /// A tick cycle exceeded its deadline.
    #[error("Tick timeout: elapsed={elapsed:?}")]
    TickTimeout { elapsed: Duration },

    /// Internal engine error.
    #[error("Internal error: {0}")]
    Internal(String),
}
