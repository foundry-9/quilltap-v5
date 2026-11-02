//! The ONE way a caller builds an [`InventoryChangeContext`] (v4
//! `lib/inventory/equip-effects.ts` `inventoryChangeContext`, `0276d10d9`).
//!
//! The types live with the chokepoints in [`super::equip`]; this module is
//! v4's builder: announced unless said otherwise, and joined to the turn's
//! pending announcements when there is a turn.

use super::equip::PendingInventoryMap;
pub use super::equip::{InventoryChangeContext, InventoryChangeProvenance};

/// v4 `inventoryChangeContext(userId, provenance, { announce?,
/// pendingInventoryAnnouncements? })` — `announce ?? true`.
pub fn inventory_change_context<'a>(
    user_id: &'a str,
    provenance: InventoryChangeProvenance,
    announce: Option<bool>,
    pending: Option<&'a mut PendingInventoryMap>,
) -> InventoryChangeContext<'a> {
    InventoryChangeContext {
        user_id,
        provenance,
        announce: announce.unwrap_or(true),
        pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 `chokepoints.test.ts` "announces by default" / "carries announce:
    /// false and the turn's pending map when given".
    #[test]
    fn announces_unless_told_otherwise() {
        let ctx = inventory_change_context("u", InventoryChangeProvenance::Operator, None, None);
        assert!(ctx.announce);
        assert!(ctx.pending.is_none());
        assert_eq!(ctx.user_id, "u");
        let mut map = PendingInventoryMap::default();
        let ctx = inventory_change_context(
            "u",
            InventoryChangeProvenance::Pascal,
            Some(false),
            Some(&mut map),
        );
        assert!(!ctx.announce);
        assert!(ctx.pending.is_some());
        assert_eq!(ctx.provenance, InventoryChangeProvenance::Pascal);
    }
}
