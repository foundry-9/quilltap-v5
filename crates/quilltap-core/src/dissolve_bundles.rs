//! SHIM — the old home of bundle dissolution (v4 `lib/wardrobe/dissolve-
//! bundles.ts`, `61574563` → `3ee3b1342`), kept so its callers compile
//! unchanged while v4's rebuild lands.
//!
//! v4 `9ecebf986` deleted `dissolve-bundles.ts` and rebuilt it as
//! `slot-ops.ts` — "bundle" became "composite", `dissolveBundlesInSlots` and
//! `…WithCredit` folded into ONE `dissolveCompositesInSlots` returning
//! `{ slots, wornBundles }` (with an `only` filter), and the two
//! `[dissolveBundleToLeaves]` WARNs moved to the persisted
//! `applyDisplacement` as `[applyDisplacement]` lines (so the pure math is
//! silent). The port of that module is [`crate::slot_ops`]; every name here
//! is a thin wrapper over it with the OLD signature, each marked
//! `LANE-REMOVE(P4.D282)` for the lane that re-aims the remaining callers
//! (`api::chat_outfits`, `services::outfit_selections`) and deletes this
//! file. Nothing here has behaviour of its own.

pub use crate::slot_ops::{DissolvedLeaf, WearableLookup, WearableNode};
use crate::wardrobe::Slots;

// LANE-REMOVE(P4.D282): v4 deleted dissolve-bundles.ts in 9ecebf986
/// Old `slotsCoveredBy(item)` → [`crate::slot_ops::slots_of`].
pub fn slots_covered_by(item: &WearableNode) -> Vec<String> {
    crate::slot_ops::slots_of(item)
}

// LANE-REMOVE(P4.D282): v4 deleted dissolve-bundles.ts in 9ecebf986
/// Old `isBundle(item)` → [`crate::wardrobe::is_composite`] over the node's
/// components.
pub fn is_bundle(item: &WearableNode) -> bool {
    crate::wardrobe::is_composite(&item.component_item_ids)
}

// LANE-REMOVE(P4.D282): v4 deleted dissolve-bundles.ts in 9ecebf986
/// Old `dissolveBundleToLeaves` → [`crate::slot_ops::dissolve_composite_to_leaves`]
/// (SILENT now — the WARNs live in `apply_displacement`).
pub fn dissolve_bundle_to_leaves(
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
) -> Option<Vec<DissolvedLeaf>> {
    crate::slot_ops::dissolve_composite_to_leaves(item, items_by_id)
}

// LANE-REMOVE(P4.D282): v4 deleted dissolve-bundles.ts in 9ecebf986
/// Old `layLeavesIntoSlots` → [`crate::slot_ops::lay_leaves_into_slots`].
pub fn lay_leaves_into_slots(
    current_slots: &Slots,
    bundle: &WearableNode,
    leaves: &[DissolvedLeaf],
    clear_covered_slots: bool,
) -> Slots {
    crate::slot_ops::lay_leaves_into_slots(current_slots, bundle, leaves, clear_covered_slots)
}

// LANE-REMOVE(P4.D282): v4 deleted dissolve-bundles.ts in 9ecebf986
/// Old `dissolveBundlesInSlots` → the slots half of
/// [`crate::slot_ops::dissolve_composites_in_slots`] with no `only` filter.
pub fn dissolve_bundles_in_slots(current_slots: &Slots, items_by_id: &WearableLookup) -> Slots {
    crate::slot_ops::dissolve_composites_in_slots(current_slots, items_by_id, None).0
}

// LANE-REMOVE(P4.D282): v4 deleted dissolve-bundles.ts in 9ecebf986
/// Old `{ slots, wornBundles }` answer of `dissolveBundlesInSlotsWithCredit`.
#[derive(Debug, Clone, PartialEq)]
pub struct DissolvedWithCredit {
    pub slots: Slots,
    /// Every bundle dissolved, with the leaves it contributed — in the order
    /// the bundles were met (slot order, first seen).
    pub worn_bundles: Vec<crate::db::wardrobe_wear_stats::WornBundle>,
}

// LANE-REMOVE(P4.D282): v4 deleted dissolve-bundles.ts in 9ecebf986
/// Old `dissolveBundlesInSlotsWithCredit` →
/// [`crate::slot_ops::dissolve_composites_in_slots`] with no `only` filter.
pub fn dissolve_bundles_in_slots_with_credit(
    current_slots: &Slots,
    items_by_id: &WearableLookup,
) -> DissolvedWithCredit {
    let (slots, worn_bundles) =
        crate::slot_ops::dissolve_composites_in_slots(current_slots, items_by_id, None);
    DissolvedWithCredit {
        slots,
        worn_bundles,
    }
}
