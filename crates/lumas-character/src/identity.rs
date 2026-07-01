//! # Character Identity
//!
//! The persistent identity of the Lumas character — name, personality profile,
//! and creation metadata. This is NOT a duplicate of the AI Core's system prompt
//! personality concept (SRS Chapter 8.5); it is a narrow behavior-scoring-only
//! profile that biases the `BehaviorSelector` and `EmotionSystem`.
//!
//! # Authority
//! Character Engine — owns "who Lumas is" across sessions.
//!
//! # Does NOT
//! - Define the AI Core's system prompt or inference personality
//! - Control animation clips or render state
//! - Define behavioral states (see `lumas_state::CharacterMachine`)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Unique identifier for a character instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterId(pub u64);

impl CharacterId {
    /// Create a new unique character ID.
    pub fn new() -> Self {
        Self(rand::random())
    }
}

impl Default for CharacterId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CharacterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Character({})", self.0)
    }
}

/// The persistent identity of the Lumas character.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterIdentity {
    /// Unique character identifier.
    pub id: CharacterId,
    /// User-customizable display name (default "Lumas").
    pub name: String,
    /// Semantic version of the identity schema.
    pub version: semver::Version,
    /// Personality profile that biases behavior scoring.
    pub personality_profile: PersonalityProfile,
    /// When this character was first created.
    pub created_at: DateTime<Utc>,
}

impl CharacterIdentity {
    /// Create a new character identity with defaults.
    pub fn new(name: String) -> Self {
        Self {
            id: CharacterId::new(),
            name,
            version: semver::Version::new(1, 0, 0),
            personality_profile: PersonalityProfile::default(),
            created_at: Utc::now(),
        }
    }
}

/// Personality is a set of weights that bias behavior scoring — not a separate
/// AI personality system (that's the AI Core's system prompt, see SRS Chapter 8.5).
/// This is purely a behavioral-tuning knob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityProfile {
    /// 0.0–1.0: biases BehaviorSelector scoring toward playful/exploratory behaviors.
    pub playfulness: f32,
    /// 0.0–1.0: biases hysteresis `min_run_time` — higher = longer before interrupting.
    pub patience: f32,
    /// 0.0–1.0: biases EmotionSystem intensity scaling.
    pub expressiveness: f32,
}

impl PersonalityProfile {
    /// Validate that all weights are in [0.0, 1.0].
    pub fn validate(&self) -> Result<(), crate::error::CharacterError> {
        if !(0.0..=1.0).contains(&self.playfulness) {
            return Err(crate::error::CharacterError::InvalidPersonalityWeight {
                field: "playfulness",
                value: self.playfulness,
            });
        }
        if !(0.0..=1.0).contains(&self.patience) {
            return Err(crate::error::CharacterError::InvalidPersonalityWeight {
                field: "patience",
                value: self.patience,
            });
        }
        if !(0.0..=1.0).contains(&self.expressiveness) {
            return Err(crate::error::CharacterError::InvalidPersonalityWeight {
                field: "expressiveness",
                value: self.expressiveness,
            });
        }
        Ok(())
    }
}

impl Default for PersonalityProfile {
    fn default() -> Self {
        Self {
            playfulness: 0.6,
            patience: 0.7,
            expressiveness: 0.65,
        }
    }
}
