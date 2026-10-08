//! The `?action=wear-history` read on every tier's single-item wardrobe GET —
//! v4 `lib/wardrobe/wear-history.ts` + the four item routes (`3ee3b1342`).
//!
//! P4.D255 (the round's keystone) lands this module with the contract-C1 §11
//! signature and the loud typed refusal body agreed in advance (R-E, CLAUDE.md's
//! no-stub rule); P4.D256 owns it from the `KEYSTONE` commit on and replaces
//! the body in this same round.

use super::types::{ErrorKind, Response, WardrobeContainerScope};
use crate::db::runtime::Db;

/// v4 `GET …/wardrobe/[itemId]?action=wear-history` → `Response::
/// WardrobeWearHistory({history, wearers, lastWornChat})` (contract C2 §3).
pub fn wardrobe_item_wear_history(
    _db: &Db,
    _scope: WardrobeContainerScope,
    _container_id: Option<&str>,
    _item_id: &str,
) -> Response {
    Response::error(
        ErrorKind::Internal,
        "The 'wardrobeItemWearHistory' action is recognized but not yet available (P4.D256).",
    )
}
