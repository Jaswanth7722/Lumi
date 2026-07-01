//! Tests: Save/load roundtrip, revalidate_position correctly relocates off-screen hints.

use lumas_character::identity::CharacterIdentity;
use lumas_character::persistence::{CharacterPersistence, InMemoryPersistence, PersistedCharacterProfile, BehaviorPreferences};
use lumas_character::position::{MonitorInfo, PersistedPosition, ScreenIndex, revalidate_position};
use lumas_common::position::PositionTarget;

fn make_profile() -> PersistedCharacterProfile {
    let identity = CharacterIdentity::new("Lumas");
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
    let persistence = InMemoryPersistence::with_profile(make_profile());
    let mut profile = make_profile();
    profile.character.name = "Lumas 2.0".into();
    persistence.save_profile(&profile).await.unwrap();
    let loaded = persistence.load_profile().await.unwrap();
    assert_eq!(loaded.character.name, "Lumas 2.0");
}

#[tokio::test]
async fn test_profile_exists() {
    let persistence = InMemoryPersistence::with_profile(make_profile());
    assert!(persistence.profile_exists().await);
    let empty = InMemoryPersistence::default();
    assert!(!empty.profile_exists().await);
}

#[tokio::test]
async fn test_delete_profile() {
    let persistence = InMemoryPersistence::with_profile(make_profile());
    assert!(persistence.profile_exists().await);
    persistence.delete_profile().await.unwrap();
    assert!(!persistence.profile_exists().await);
}

#[test]
fn test_valid_position_kept() {
    let hint = PersistedPosition {
        x: 100.0,
        y: 200.0,
        screen_index: ScreenIndex(0),
        screen_width: 1920,
        screen_height: 1080,
    };
    let monitors = vec![MonitorInfo {
        index: 0, x: 0, y: 0, width: 1920, height: 1080, is_primary: true,
    }];
    let target = revalidate_position(&hint, &monitors);
    assert!(matches!(target, PositionTarget::Absolute { x, y } if x == 100.0 && y == 200.0));
}

#[test]
fn test_monitor_disconnected_moves_to_primary() {
    let hint = PersistedPosition {
        x: 100.0,
        y: 200.0,
        screen_index: ScreenIndex(5), // Doesn't exist
        screen_width: 1920,
        screen_height: 1080,
    };
    let monitors = vec![MonitorInfo {
        index: 0, x: 0, y: 0, width: 1920, height: 1080, is_primary: true,
    }];
    let target = revalidate_position(&hint, &monitors);
    assert!(matches!(target, PositionTarget::Absolute { x, y } if x > 0.0 && y > 0.0));
}

#[test]
fn test_resolution_change_recenters() {
    let hint = PersistedPosition {
        x: 100.0,
        y: 200.0,
        screen_index: ScreenIndex(0),
        screen_width: 3840, // Was 4K
        screen_height: 2160,
    };
    let monitors = vec![MonitorInfo {
        index: 0, x: 0, y: 0, width: 1920, height: 1080, is_primary: true, // Now 1080p
    }];
    let target = revalidate_position(&hint, &monitors);
    assert!(matches!(target, PositionTarget::Absolute { .. }));
}

#[test]
fn test_no_monitors_returns_preserve() {
    let hint = PersistedPosition {
        x: 100.0,
        y: 200.0,
        screen_index: ScreenIndex(0),
        screen_width: 1920,
        screen_height: 1080,
    };
    let target = revalidate_position(&hint, &[]);
    assert_eq!(target, PositionTarget::Preserve);
}
