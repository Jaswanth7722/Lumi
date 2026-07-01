//! Tests: Appearance profile, accessory slot compatibility, skin compatibility.

use lumas_character::accessory::{AccessoryDefinition, AccessoryId, AccessoryRegistry, AccessorySlotKind, EquippedAccessory};
use lumas_character::appearance::{AppearanceProfile, SkinId};
use lumas_character::customization::ColorTheme;

fn make_registry() -> AccessoryRegistry {
    let mut r = AccessoryRegistry::new();
    r.register(AccessoryDefinition {
        id: AccessoryId::new("hat"),
        name: "Top Hat".into(),
        slot: AccessorySlotKind::Head,
        mesh_attachment_point: "head_bone".into(),
        compatible_skins: None,
    });
    r.register(AccessoryDefinition {
        id: AccessoryId::new("glasses"),
        name: "Glasses".into(),
        slot: AccessorySlotKind::Eyes,
        mesh_attachment_point: "face_bone".into(),
        compatible_skins: None,
    });
    r
}

#[test]
fn test_default_appearance() {
    let profile = AppearanceProfile::default();
    assert_eq!(profile.base_skin.0, "default");
    assert!(profile.equipped_accessories.is_empty());
}

#[test]
fn test_equip_accessory() {
    let mut profile = AppearanceProfile::default();
    let registry = make_registry();
    let equipped = EquippedAccessory {
        accessory_id: AccessoryId::new("hat"),
        slot: AccessorySlotKind::Head,
    };
    assert!(profile.equip_accessory(equipped, &registry).is_ok());
    assert!(profile.has_accessory(&AccessoryId::new("hat")));
}

#[test]
fn test_slot_conflict() {
    let mut profile = AppearanceProfile::default();
    let registry = make_registry();
    profile.equip_accessory(
        EquippedAccessory { accessory_id: AccessoryId::new("hat"), slot: AccessorySlotKind::Head },
        &registry,
    )
    .unwrap();
    let result = profile.equip_accessory(
        EquippedAccessory { accessory_id: AccessoryId::new("glasses"), slot: AccessorySlotKind::Head },
        &registry,
    );
    assert!(result.is_err());
}

#[test]
fn test_unequip_accessory() {
    let mut profile = AppearanceProfile::default();
    let registry = make_registry();
    profile.equip_accessory(
        EquippedAccessory { accessory_id: AccessoryId::new("hat"), slot: AccessorySlotKind::Head },
        &registry,
    )
    .unwrap();
    profile.unequip_accessory(AccessorySlotKind::Head);
    assert!(!profile.has_accessory(&AccessoryId::new("hat")));
}

#[test]
fn test_color_theme_defaults() {
    let theme = ColorTheme::default();
    assert_eq!(theme.primary, "#5BC8F5");
}

#[test]
fn test_color_theme_serde_roundtrip() {
    let theme = ColorTheme::pastel();
    let json = serde_json::to_string(&theme).unwrap();
    let deserialized: ColorTheme = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.primary, theme.primary);
}

#[test]
fn test_accessory_registry_resolve() {
    let registry = make_registry();
    let hat = registry.resolve(&AccessoryId::new("hat"));
    assert!(hat.is_ok());
    assert_eq!(hat.unwrap().slot, AccessorySlotKind::Head);
}

#[test]
fn test_accessory_not_found() {
    let registry = make_registry();
    let result = registry.resolve(&AccessoryId::new("nonexistent"));
    assert!(matches!(result, Err(lumas_character::error::CharacterError::AccessoryNotFound { .. })));
}
