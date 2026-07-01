//! # Character Engine Events
//!
//! Events that the Character Engine publishes about itself for diagnostics,
//! observability, and cross-subsystem coordination. These are DISTINCT from
//! the `lumas_state::StateEvent` events that drive state machine transitions.
//!
//! # Authority
//! Character Engine — published by `CharacterManager` for external observers.
//!
//! # Does NOT
//! - Trigger state machine transitions (use `lumas_state::StateEvent` for that)
//! - Duplicate `lumas_state::TransitionEvent`

use crate::accessory::AccessorySlotKind;
use crate::behavior::BehaviorId;
use crate::identity::CharacterId;
use crate::movement::MovementReason;
use lumas_common::emotion::Emotion as CommonEmotion;
use std::borrow::Cow;
use std::time::Duration;

/// Events emitted by the Character Engine for diagnostics and observability.
#[derive(Debug, Clone)]
pub enum CharacterEvent {
    /// Character profile was loaded from persistence.
    ProfileLoaded { character_id: CharacterId },

    /// A behavior started execution.
    BehaviorStarted {
        behavior_id: BehaviorId,
        reason: Cow<'static, str>,
    },

    /// A behavior completed execution.
    BehaviorCompleted {
        behavior_id: BehaviorId,
        duration: Duration,
    },

    /// A behavior was interrupted by another behavior.
    BehaviorInterrupted {
        behavior_id: BehaviorId,
        interrupted_by: BehaviorId,
    },

    /// The character's primary emotion changed.
    EmotionChanged {
        from: CommonEmotion,
        to: CommonEmotion,
    },

    /// An appearance field was modified.
    AppearanceChanged { field: Cow<'static, str> },

    /// An accessory was equipped or unequipped.
    AccessoryEquipped {
        accessory_id: crate::accessory::AccessoryId,
        slot: AccessorySlotKind,
    },

    /// Movement intent was updated.
    MovementIntentChanged { reason: MovementReason },
}
