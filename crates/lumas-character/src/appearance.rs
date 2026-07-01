//! # Appearance Profile
//!
//! Describes *what* appearance items are equipped — never *how* they're rendered.
//! The Render Engine reads this profile (via IPC or shared state) and resolves
//! mesh/asset references against its own catalog.
//!
//! # Authority
//! Character Engine — "what is true" about appearance.
//!
//! # Does NOT
//! - Contain mesh handles, GPU resources, or `MeshId` from `lumas-render`
//! - Define rendering logic or material properties (Render Engine's job)
//! - Own animation clips or blend shapes

use crate::accessory::{AccessoryId, EquippedAccessory};
use crate::customization::ColorTheme;
use crate::error::CharacterError;
use serde::{Deserialize, Serialize};

/// Identifier for a base skin type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkinId(pub String);

impl SkinId {
    /// Create a new skin ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for SkinId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Skin({})", self.0)
    }
}

/// Default skin ID.
pub const DEFAULT_SKIN_ID: &str = "default";

/// Seasonal theme identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeasonalThemeId(pub String);

impl SeasonalThemeId {
    /// Create a new seasonal theme ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// The complete appearance profile for the character.
///
/// This is **data only** — it describes what is equipped, never how it's rendered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceProfile {
    /// Base skin variant.
    pub base_skin: SkinId,
    /// Color theme for UI elements and accent colors.
    pub color_theme: ColorTheme,
    /// Currently equipped accessories.
    pub equipped_accessories: Vec<EquippedAccessory>,
    /// Optional seasonal theme override.
    pub seasonal_theme: Option<SeasonalThemeId>,
}

impl Default for AppearanceProfile {
    fn default() -> Self {
        Self {
            base_skin: SkinId::new(DEFAULT_SKIN_ID),
            color_theme: ColorTheme::default(),
            equipped_accessories: Vec::new(),
            seasonal_theme: None,
        }
    }
}

impl AppearanceProfile {
    /// Create a new appearance profile with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Change the base skin.
    pub fn set_skin(&mut self, skin: SkinId) {
        self.base_skin = skin;
    }

    /// Set the color theme.
    pub fn set_color_theme(&mut self, theme: ColorTheme) {
        self.color_theme = theme;
    }

    /// Equip an accessory, returning an error if the slot is already occupied.
    pub fn equip_accessory(
        &mut self,
        accessory: EquippedAccessory,
        registry: &crate::accessory::AccessoryRegistry,
    ) -> Result<(), CharacterError> {
        // Validate that the accessory exists in the registry
        registry.resolve(&accessory.accessory_id)?;

        // Check slot compatibility
        if let Some(existing) = self.equipped_accessories.iter().find(|a| a.slot == accessory.slot) {
            return Err(CharacterError::AccessorySlotIncompatible {
                accessory: existing.accessory_id.clone(),
                slot: accessory.slot,
            });
        }

        self.equipped_accessories.push(accessory);
        Ok(())
    }

    /// Unequip an accessory by slot.
    pub fn unequip_accessory(&mut self, slot: crate::accessory::AccessorySlotKind) {
        self.equipped_accessories.retain(|a| a.slot != slot);
    }

    /// Check if an accessory is currently equipped.
    pub fn has_accessory(&self, id: &AccessoryId) -> bool {
        self.equipped_accessories.iter().any(|a| a.accessory_id == *id)
    }

    /// Get the accessory in a specific slot.
    pub fn accessory_in_slot(
        &self,
        slot: crate::accessory::AccessorySlotKind,
    ) -> Option<&EquippedAccessory> {
        self.equipped_accessories.iter().find(|a| a.slot == slot)
    }

    /// Set the seasonal theme.
    pub fn set_seasonal_theme(&mut self, theme: Option<SeasonalThemeId>) {
        self.seasonal_theme = theme;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accessory::{AccessoryDefinition, AccessoryRegistry, AccessorySlotKind};

    fn make_registry() -> AccessoryRegistry {
        let mut registry = AccessoryRegistry::new();
        registry.register(AccessoryDefinition {
            id: AccessoryId::new("hat"),
            name: "Top Hat".into(),
            slot: AccessorySlotKind::Head,
            mesh_attachment_point: "head_bone".into(),
            compatible_skins: None,
        });
        registry
    }

    #[test]
    fn test_default_appearance() {
        let profile = AppearanceProfile::default();
        assert_eq!(profile.base_skin.0, DEFAULT_SKIN_ID);
        assert!(profile.equipped_accessories.is_empty());
        assert!(profile.seasonal_theme.is_none());
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

        let hat1 = EquippedAccessory {
            accessory_id: AccessoryId::new("hat"),
            slot: AccessorySlotKind::Head,
        };
        let hat2 = EquippedAccessory {
            accessory_id: AccessoryId::new("crown"),
            slot: AccessorySlotKind::Head,
        };

        profile.equip_accessory(hat1, &registry).unwrap();
        let result = profile.equip_accessory(hat2, &registry);
        assert!(result.is_err()); // Slot conflict
    }

    #[test]
    fn test_unequip_accessory() {
        let mut profile = AppearanceProfile::default();
        let registry = make_registry();

        let equipped = EquippedAccessory {
            accessory_id: AccessoryId::new("hat"),
            slot: AccessorySlotKind::Head,
        };
        profile.equip_accessory(equipped, &registry).unwrap();
        profile.unequip_accessory(AccessorySlotKind::Head);
        assert!(!profile.has_accessory(&AccessoryId::new("hat")));
    }
}
