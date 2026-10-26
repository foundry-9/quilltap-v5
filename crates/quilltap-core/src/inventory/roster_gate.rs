//! The project roster at the Inventory — the wardrobe's rule
//! (`project_roster_access`) applied to the project's `Inventory/` (v4
//! `lib/inventory/roster-gate.ts`, `4fbdad186`; P4.D324).
//!
//! The roster closes the shelf, not the hand. A character the project's roster
//! does not admit cannot, through their own tools or a custom tool's effects,
//! see, take up or pick up an object on the project's shelf. Whatever they are
//! already carrying stays theirs to see and set down — the operator may have
//! handed it to them, and the carried set is rewritten whole, so hiding a
//! carried object would drop it. The operator's own gestures, arrival defaults
//! and every reader of what is carried (the prompt section, the image
//! pipelines) are not gated, exactly as the wardrobe's are not.

use std::collections::HashSet;

use rusqlite::Connection;

use super::equip::InventoryChangeProvenance;
use super::types::{InventoryScope, ResolvedInventoryItem};
use crate::inventory::types::InventoryFields;
use crate::project_roster_access::project_roster_admits;

/// Whether a gesture is the character's own (a tool call or a custom tool),
/// and so roster-gated (v4 `isCharacterGesture`).
pub fn is_character_gesture(provenance: InventoryChangeProvenance) -> bool {
    provenance != InventoryChangeProvenance::Operator
}

/// Whether the project's shelf is open to this character's own reach: no
/// project, or the roster admits them (v4 `projectShelfOpenTo`). A failed
/// lookup closes it — in v5 inside the ONE chokepoint
/// ([`project_roster_admits`] fails closed through its fallback home, with
/// v4's lines). v4's own `catch` here (WARN `[InventoryRoster] Roster lookup
/// failed; the project shelf stays closed`) wraps a `projectRosterAdmits`
/// that never throws — its lookup is the same fallback — so the arm is
/// unreachable on both sides and has no v5 twin.
pub fn project_shelf_open_to(
    main: &Connection,
    mount: &Connection,
    project_id: Option<&str>,
    character_id: &str,
) -> bool {
    let Some(project_id) = project_id.filter(|s| !s.is_empty()) else {
        return true;
    };
    project_roster_admits(main, mount, Some(project_id), Some(character_id))
}

/// An object on the project's shelf that the character is not carrying: what
/// a closed shelf hides (v4 `isOnClosedShelf`).
pub fn is_on_closed_shelf(item: &ResolvedInventoryItem, carried_ids: &HashSet<String>) -> bool {
    item.origin()
        .is_some_and(|o| o.scope == InventoryScope::Project)
        && !carried_ids.contains(item.id())
}
