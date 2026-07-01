//! # Accessory System
//!
//! Defines accessory types, slots, and registry for the character's equippable items.
//! Each accessory has a mesh attachment point that the Render Engine resolves against
//! its own asset catalog.
//!
//! # Authority
//! Character Engine — what accessories are equipped.
//!
//! # Does NOT
//! - Contain mesh handles or GPU resources (Render Engine)
//! - Define rendering logic for accessories

use crate::appearance::SkinId;
use crate::error::{CharacterError, CharacterResult};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

/// Unique identifier for an accessory type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccessoryId(pub String);

impl AccessoryId {
    /// Create a new accessory ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for AccessoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Accessory({})", self.0)
    }
}

/// The slot on the character where an accessory can be attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccessorySlotKind {
    /// On the head (hats, crowns, etc.).
    Head,
    /// Around the eyes (glasses, goggles, etc.).
    Eyes,
    /// On the back (backpacks, capes, etc.).
    Back,
    /// Held in paws/hands (items, tools, etc.).
    Held,
    /// Around the neck (scarves, collars, etc.).
    Neck,
}

/// A concrete equipped accessory instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquippedAccessory {
    /// The type of accessory.
    pub accessory_id: AccessoryId,
    /// Which slot this accessory occupies.
    pub slot: AccessorySlotKind,
}

/// Definition of an accessory type available in the catalog.
#[derive(Debug, Clone)]
pub struct AccessoryDefinition {
    /// Unique identifier.
    pub id: AccessoryId,
    /// Display name.
    pub name: Cow<'static, str>,
    /// Which slot this accessory fits in.
    pub slot: AccessorySlotKind,
    /// Bone name the Render Engine uses for attachment.
    pub mesh_attachment_point: Cow<'static, str>,
    /// Skins this accessory is compatible with (None = all skins).
    pub compatible_skins: Option<Vec<SkinId>>,
}

/// Registry of all available accessories.
#[derive(Debug, Clone)]
pub struct AccessoryRegistry {
    catalog: HashMap<AccessoryId, AccessoryDefinition>,
}

impl AccessoryRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            catalog: HashMap::new(),
        }
    }

    /// Register a new accessory definition.
    pub fn register(&mut self, definition: AccessoryDefinition) {
        self.catalog.insert(definition.id.clone(), definition);
    }

    /// Resolve an accessory by ID, returning its definition.
    pub fn resolve(&self, id: &AccessoryId) -> Result<&AccessoryDefinition, CharacterError> {
        self.catalog
            .get(id)
            .ok_or_else(|| CharacterError::AccessoryNotFound { id: id.clone() })
    }

    /// Check if an accessory exists in the registry.
    pub fn contains(&self, id: &AccessoryId) -> bool {
        self.catalog.contains_key(id)
    }

    /// Get all registered accessories.
    pub fn all(&self) -> impl Iterator<Item = &AccessoryDefinition> {
        self.catalog.values()
    }

    /// Get accessories compatible with a specific slot.
    pub fn for_slot(&self, slot: AccessorySlotKind) -> Vec<&AccessoryDefinition> {
        self.catalog
            .values()
            .filter(|def| def.slot == slot)
            .collect()
    }

    /// Get accessories compatible with a specific skin.
    pub fn for_skin(&self, skin: &SkinId) -> Vec<&AccessoryDefinition> {
        self.catalog
            .values()
            .filter(|def| {
                def.compatible_skins
                    .as_ref()
                    .map(|skins| skins.contains(skin))
                    .unwrap_or(true)
            })
            .collect()
    }

    /// Number of registered accessories.
    pub fn count(&self) -> usize {
        self.catalog.len()
    }

    /// Check if an accessory is compatible with a given slot.
    pub fn is_compatible_with_slot(
        &self,
        id: &AccessoryId,
        slot: AccessorySlotKind,
    ) -> CharacterResult<bool> {
        let def = self.resolve(id)?;
        Ok(def.slot == slot)
    }
}

impl Default for AccessoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Register the built-in accessory catalog.
pub fn register_builtin_accessories(registry: &mut AccessoryRegistry) {
    registry.register(AccessoryDefinition {
        id: AccessoryId::new("default_hat"),
        name: "Default Hat".into(),
        slot: AccessorySlotKind::Head,
        mesh_attachment_point: "head_top_joint".into(),
        compatible_skins: None,
    });
    registry.register(AccessoryDefinition {
        id: AccessoryId::new("glasses_round"),
        name: "Round Glasses".into(),
        slot: AccessorySlotKind::Eyes,
        mesh_attachment_point: "face_joint".into(),
        compatible_skins: None,
    });
    registry.register(AccessoryDefinition {
        id: AccessoryId::new("scarf_red"),
        name: "Red Scarf".into(),
        slot: AccessorySlotKind::Neck,
        mesh_attachment_point: "neck_joint".into(),
        compatible_skins: None,
    });
    registry.register(AccessoryDefinition {
        id: AccessoryId::new("backpack_small"),
        name: "Small Backpack".into(),
        slot: AccessorySlotKind::Back,
        mesh_attachment_point: "spine_joint".into(),
        compatible_skins: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> AccessoryRegistry {
        let mut r = AccessoryRegistry::new();
        register_builtin_accessories(&mut r);
        r
    }

    #[test]
    fn test_accessory_registry_resolve() {
        let registry = make_registry();
        let def = registry.resolve(&AccessoryId::new("glasses_round"));
        assert!(def.is_ok());
        assert_eq!(def.unwrap().slot, AccessorySlotKind::Eyes);
    }

    #[test]
    fn test_accessory_not_found() {
        let registry = make_registry();
        let result = registry.resolve(&AccessoryId::new("nonexistent"));
        assert!(matches!(result, Err(CharacterError::AccessoryNotFound { .. })));
    }

    #[test]
    fn test_for_slot_filtering() {
        let registry = make_registry();
        let head_accessories = registry.for_slot(AccessorySlotKind::Head);
        assert_eq!(head_accessories.len(), 1);
        assert_eq!(head_accessories[0].id, AccessoryId::new("default_hat"));
    }

    #[test]
    fn test_compatible_with_slot() {
        let registry = make_registry();
        let result = registry
            .is_compatible_with_slot(&AccessoryId::new("default_hat"), AccessorySlotKind::Head);
        assert!(result.unwrap());
    }

    #[test]
    fn test_incompatible_with_slot() {
        let registry = make_registry();
        let result = registry
            .is_compatible_with_slot(&AccessoryId::new("default_hat"), AccessorySlotKind::Eyes);
        assert!(!result.unwrap());
    }

    #[test]
    fn test_builtin_accessories_count() {
        let registry = make_registry();
        assert_eq!(registry.count(), 4);
    }
}
