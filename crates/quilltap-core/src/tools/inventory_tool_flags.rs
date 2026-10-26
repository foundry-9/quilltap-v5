//! The `inventory_*` family's per-tool enable flags, in one place (v4
//! `lib/tools/inventory-tool-flags.ts`, `e1872c94f`; P4.D328).
//!
//! Every surface that offers tools (the native slate in
//! `services::tool_build`, the simple-JSON and text-block prompts) carries the
//! nine flags; they are all on or all off, gated by the character's
//! `canManageInventory` (`null` reads as on). Brahma and the Scenario Builder
//! pass off.
//!
//! v4 spreads the record into each option object (`extends
//! InventoryToolFlags`); v5's option structs carry the nine fields by name
//! (`inventory_list` … `inventory_give`, camelCase on the wire), and this module
//! holds the one rule that sets them and the one offer order that reads them.

/// The nine flags (v4 `Record<InventoryToolFlag, boolean>`), in offer order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InventoryToolFlags {
    pub list: bool,
    pub read: bool,
    pub equip: bool,
    pub unequip: bool,
    pub pick_up: bool,
    pub create: bool,
    pub update: bool,
    pub archive: bool,
    pub give: bool,
}

/// The catalog keys the nine flags enable, in v4's offer order
/// (`INVENTORY_TOOL_DEFINITIONS`' key order — `inventory-tool-flags.ts:22-32`).
pub const INVENTORY_TOOL_CATALOG_KEYS: [&str; 9] = [
    "inventoryList",
    "inventoryRead",
    "inventoryEquip",
    "inventoryUnequip",
    "inventoryPickUp",
    "inventoryCreate",
    "inventoryUpdate",
    "inventoryArchive",
    "inventoryGive",
];

impl InventoryToolFlags {
    /// Every flag set to `enabled`.
    pub const fn all(enabled: bool) -> Self {
        InventoryToolFlags {
            list: enabled,
            read: enabled,
            equip: enabled,
            unequip: enabled,
            pick_up: enabled,
            create: enabled,
            update: enabled,
            archive: enabled,
            give: enabled,
        }
    }

    /// The nine in offer order (aligned with [`INVENTORY_TOOL_CATALOG_KEYS`]).
    pub const fn in_offer_order(&self) -> [bool; 9] {
        [
            self.list,
            self.read,
            self.equip,
            self.unequip,
            self.pick_up,
            self.create,
            self.update,
            self.archive,
            self.give,
        ]
    }
}

/// All nine flags from the character's `canManageInventory` (v4
/// `inventoryToolFlags`): `null` / absent / `true` read ON, only `false` turns
/// the family off — the `!== false` doctrine the two wardrobe flags follow.
pub const fn inventory_tool_flags(can_manage_inventory: Option<bool>) -> InventoryToolFlags {
    InventoryToolFlags::all(!matches!(can_manage_inventory, Some(false)))
}

/// The enabled catalog keys, in offer order (v4
/// `enabledInventoryToolDefinitions` — filter, then map).
pub fn enabled_inventory_tool_keys(flags: &InventoryToolFlags) -> Vec<&'static str> {
    INVENTORY_TOOL_CATALOG_KEYS
        .iter()
        .zip(flags.in_offer_order())
        .filter(|(_, on)| *on)
        .map(|(key, _)| *key)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::definitions::definition_json_by_key;

    #[test]
    fn null_and_true_read_on_and_only_false_turns_the_family_off() {
        assert_eq!(inventory_tool_flags(None), InventoryToolFlags::all(true));
        assert_eq!(
            inventory_tool_flags(Some(true)),
            InventoryToolFlags::all(true)
        );
        assert_eq!(
            inventory_tool_flags(Some(false)),
            InventoryToolFlags::all(false)
        );
    }

    #[test]
    fn the_enabled_keys_follow_the_offer_order_and_resolve_in_the_catalog() {
        assert_eq!(
            enabled_inventory_tool_keys(&InventoryToolFlags::all(true)),
            INVENTORY_TOOL_CATALOG_KEYS.to_vec()
        );
        assert!(enabled_inventory_tool_keys(&InventoryToolFlags::all(false)).is_empty());
        let some = InventoryToolFlags {
            give: true,
            list: true,
            ..InventoryToolFlags::default()
        };
        assert_eq!(
            enabled_inventory_tool_keys(&some),
            vec!["inventoryList", "inventoryGive"]
        );
        for key in INVENTORY_TOOL_CATALOG_KEYS {
            assert!(
                definition_json_by_key(key).is_some(),
                "{key} is not in the catalog"
            );
        }
    }
}
