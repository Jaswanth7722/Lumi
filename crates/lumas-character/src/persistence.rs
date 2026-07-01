//! # Persistence Layer
//!
//! Implements exactly the narrow persistence scope: character identity, appearance,
//! and behavior preferences. **Runtime behavioral state is never persisted** — on
//! restart, Lumas always re-initializes to `Idle.Watching` and re-derives behavior
//! from current context.
//!
//! # Authority
//! Character Engine — persisting identity-durable data.
//!
//! # Does NOT
//! - Persist runtime behavioral state (what Lumas was doing)
//! - Own the file system or database (delegates to `lumas-storage`)
//! - Cache or memoize profile data (that's the `CharacterManager`'s role)

use crate::accessory::AccessoryId;
use crate::appearance::{AppearanceProfile, SkinId};
use crate::identity::{CharacterId, CharacterIdentity};
use crate::position::{MonitorInfo, PersistedPosition, revalidate_position};
use crate::error::{CharacterError, CharacterResult};
use async_trait::async_trait;
use lumas_common::position::PositionTarget;
use serde::{Deserialize, Serialize};

/// Behavior preference flags that can be toggled by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorPreferences {
    /// Disable celebration animations (e.g., bounce on task success).
    pub disable_celebrations: bool,
    /// Disable idle exploration.
    pub disable_exploration: bool,
    /// Reduce movement frequency.
    pub reduced_movement: bool,
    /// Enable quiet mode (suppress sounds, reduce animations).
    pub quiet_mode: bool,
}

impl Default for BehaviorPreferences {
    fn default() -> Self {
        Self {
            disable_celebrations: false,
            disable_exploration: false,
            reduced_movement: false,
            quiet_mode: false,
        }
    }
}

/// The complete persisted character profile.
///
/// Only identity-durable data survives a restart:
/// - Character identity (name, personality profile)
/// - Appearance (skin, color theme, accessories)
/// - Last known position (as a hint, re-validated on load)
/// - Behavior preferences (user config toggles)
///
/// # What is NOT persisted
/// - In-progress behavior (always re-derives from context on restart)
/// - Current emotion state
/// - Current state machine state
/// - Any runtime-only caches or timers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedCharacterProfile {
    /// Schema version for migration support.
    pub schema_version: u16,
    /// Character identity.
    pub character: CharacterIdentity,
    /// Appearance profile.
    pub appearance: AppearanceProfile,
    /// Last known position hint — re-validated on load.
    pub last_known_position: Option<PersistedPosition>,
    /// User-configurable behavior preferences.
    pub behavior_preferences: BehaviorPreferences,
    /// Equipped accessory IDs.
    pub equipped_accessories: Vec<AccessoryId>,
}

impl PersistedCharacterProfile {
    /// Current schema version.
    pub const CURRENT_SCHEMA_VERSION: u16 = 1;

    /// Create a new profile with the given identity.
    pub fn new(character: CharacterIdentity) -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            character,
            appearance: AppearanceProfile::default(),
            last_known_position: None,
            behavior_preferences: BehaviorPreferences::default(),
            equipped_accessories: Vec::new(),
        }
    }

    /// Validate the persisted position hint against current monitor configuration.
    /// Returns a corrected position if the original is now off-screen or on a
    /// disconnected monitor; returns the original if still valid.
    pub fn revalidate_position(
        &self,
        current_monitors: &[MonitorInfo],
    ) -> Option<PositionTarget> {
        self.last_known_position
            .as_ref()
            .map(|hint| revalidate_position(hint, current_monitors))
    }
}

/// Trait for persistence backends.
#[async_trait]
pub trait CharacterPersistence: Send + Sync {
    /// Load the character profile from persistence.
    async fn load_profile(&self) -> CharacterResult<PersistedCharacterProfile>;

    /// Save the character profile to persistence.
    async fn save_profile(&self, profile: &PersistedCharacterProfile) -> CharacterResult<()>;

    /// Check if a profile exists.
    async fn profile_exists(&self) -> bool;

    /// Delete the character profile.
    async fn delete_profile(&self) -> CharacterResult<()>;
}

/// An in-memory persistence backend for testing.
#[derive(Debug, Default)]
pub struct InMemoryPersistence {
    profile: std::sync::RwLock<Option<PersistedCharacterProfile>>,
}

impl InMemoryPersistence {
    /// Create a new in-memory persistence with the given profile.
    pub fn with_profile(profile: PersistedCharacterProfile) -> Self {
        Self {
            profile: std::sync::RwLock::new(Some(profile)),
        }
    }
}

#[async_trait]
impl CharacterPersistence for InMemoryPersistence {
    async fn load_profile(&self) -> CharacterResult<PersistedCharacterProfile> {
        self.profile
            .read()
            .map_err(|e| CharacterError::ProfileLoadFailed {
                cause: e.to_string(),
            })?
            .clone()
            .ok_or_else(|| CharacterError::ProfileLoadFailed {
                cause: "No profile stored".into(),
            })
    }

    async fn save_profile(&self, profile: &PersistedCharacterProfile) -> CharacterResult<()> {
        self.profile
            .write()
            .map_err(|e| CharacterError::ProfileLoadFailed {
                cause: e.to_string(),
            })
            .map(|mut p| *p = Some(profile.clone()))
    }

    async fn profile_exists(&self) -> bool {
        self.profile
            .read()
            .map(|p| p.is_some())
            .unwrap_or(false)
    }

    async fn delete_profile(&self) -> CharacterResult<()> {
        self.profile
            .write()
            .map(|mut p| *p = None)
            .map_err(|e| CharacterError::ProfileLoadFailed {
                cause: e.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::CharacterIdentity;

    fn make_profile() -> PersistedCharacterProfile {
        let identity = CharacterIdentity::new("Lumas".into());
        PersistedCharacterProfile::new(identity)
    }

    #[tokio::test]
    async fn test_save_load_roundtrip() {
        let persistence = InMemoryPersistence::with_profile(make_profile());
        let loaded = persistence.load_profile().await.unwrap();
        assert_eq!(loaded.character.name, "Lumas");
        assert_eq!(loaded.schema_version, PersistedCharacterProfile::CURRENT_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_save_updates_profile() {
        let mut profile = make_profile();
        let persistence = InMemoryPersistence::with_profile(make_profile());

        profile.character.name = "Lumas 2.0".into();
        persistence.save_profile(&profile).await.unwrap();

        let loaded = persistence.load_profile().await.unwrap();
        assert_eq!(loaded.character.name, "Lumas 2.0");
    }

    #[tokio::test]
    async fn test_profile_exists() {
        let persistence = InMemoryPersistence::with_profile(make_profile());
        assert!(persistence.profile_exists().await);

        let empty_persistence = InMemoryPersistence::default();
        assert!(!empty_persistence.profile_exists().await);
    }

    #[tokio::test]
    async fn test_delete_profile() {
        let persistence = InMemoryPersistence::with_profile(make_profile());
        assert!(persistence.profile_exists().await);
        persistence.delete_profile().await.unwrap();
        assert!(!persistence.profile_exists().await);
    }

    #[test]
    fn test_profile_revalidate_position_no_hint() {
        let profile = make_profile();
        let monitors = vec![MonitorInfo {
            index: 0,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            is_primary: true,
        }];
        assert!(profile.revalidate_position(&monitors).is_none());
    }
}
