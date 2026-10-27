//! Tool gate keys — every literal object reference a custom tool's
//! availability gate keys on, across every enabled store (v4
//! `lib/pascal/tool-gate-keys.ts`, `7263105c6` + `a53024d3c`'s any-of walk, at
//! `322a5488e`; P4.D340).
//!
//! A gate's `inventory` / `inventoryCount` key is a reference resolved by
//! `resolve_inventory_ref` (id → title → name). The Inventory title lint
//! (`inventory::title_lint`) needs the whole set so a new title cannot land on
//! a key and shadow the object the gate meant.
//!
//! The census is instance-wide, not per invoker: a title in a shared tier is
//! reachable by many invokers, and over-counting a key only over-rejects,
//! which is visible. It reports what it could not look at instead of treating
//! that as "no keys": a failed `Tools/` listing, a file listed but missing on
//! open, a file whose bytes or JSON cannot be read, or no enabled store at
//! all. A definition that parses as JSON but fails the schema is never dealt,
//! so its keys are not live and it is not counted as unread.
//!
//! Infallible (P4.D340 R-7): every read v4's `listAllCustomTools` makes is a
//! fallback or a try/catch, so through real reads the census never throws.
//!
//! **On this lane's branch the walk is a fenced stand-in** (R-4): the gate's
//! `inventory` / `inventoryCount` records (P4.D329, C3), the any-of
//! `gate_groups` (P4.D333, C4) and the library's `unread` / `stage` /
//! `stores_consulted` (P4.D328 / P4.D332, S9) are sibling lanes' and not on
//! this branch. The stand-in counts NO keys and treats EVERY load error as
//! unread (fail closed — a schema failure cannot be told from a read / JSON
//! one without `stage`). On a store with no `Tools/` file it answers v4's
//! census byte for byte. The unifier pastes the pin walk at §S.5.

use rusqlite::Connection;
use serde::Serialize;

use super::roster::list_all_custom_tools;

/// Which clause of a definition a key sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GateKeyClause {
    AvailableWhen,
    WithheldWhen,
}

/// Which gate subject a key sits under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GateKeySubject {
    Inventory,
    InventoryCount,
}

/// One literal reference one gate keys on (v4 `ToolGateKeyUse`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolGateKeyUse {
    /// The reference as written in the definition.
    #[serde(rename = "ref")]
    pub reference: String,
    pub tool_name: String,
    pub clause: GateKeyClause,
    pub subject: GateKeySubject,
    pub mount_name: String,
    pub definition_path: String,
}

/// What the census could not read: the store, the file, and why (v4
/// `ToolGateKeyUnread`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolGateKeyUnread {
    pub mount_name: String,
    pub definition_path: String,
    pub reason: String,
}

/// v4 `ToolGateKeyCensus`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolGateKeyCensus {
    pub keys: Vec<ToolGateKeyUse>,
    /// Non-empty means the key set is incomplete; a caller that must not guess
    /// refuses.
    pub unread: Vec<ToolGateKeyUnread>,
    /// The stores looked in, so an empty answer still says where it looked.
    pub stores_consulted: Vec<String>,
}

/// Collect every gate key in every enabled store's `Tools/` folder (v4
/// `collectToolGateKeys`).
pub fn collect_tool_gate_keys(mount: &Connection) -> ToolGateKeyCensus {
    let library = list_all_custom_tools(mount);
    // === LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6): S9 / C3 / C4 — the gate records
    // (P4.D329), `gate_groups` (P4.D333) and the library's `unread` / `stage`
    // / `stores_consulted` (P4.D328 / P4.D332) are not on this branch. No key
    // is counted (a keyed definition cannot be read as keyed here), and every
    // load error is unread, fail closed. ===
    let keys: Vec<ToolGateKeyUse> = Vec::new();
    let mut unread: Vec<ToolGateKeyUnread> = library
        .errors
        .iter()
        .map(|e| ToolGateKeyUnread {
            mount_name: e.mount_name.clone(),
            definition_path: e.definition_path.clone(),
            reason: e.reason.clone(),
        })
        .collect();
    let stores_consulted = stand_in_stores_consulted(mount);
    // === end LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6) ===
    if stores_consulted.is_empty() {
        // Quilltap General is always enabled, so no store at all is a failed
        // look, not an empty house.
        unread.push(ToolGateKeyUnread {
            mount_name: "(no enabled stores)".to_string(),
            definition_path: "Tools/".to_string(),
            reason: "the store listing came back empty".to_string(),
        });
    }

    tracing::debug!(
        target: "quilltap::pascal",
        context = "custom-tools",
        storesJson = %serde_json::to_string(&stores_consulted).unwrap_or_default(),
        definitions = library.entries.len(),
        keys = keys.len(),
        unread = unread.len(),
        "[ToolGateKeys] Census taken"
    );
    ToolGateKeyCensus {
        keys,
        unread,
        stores_consulted,
    }
}

// === LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6): S9 — `CustomToolLibrary.stores_consulted`
// is P4.D328's (else P4.D332's). Until it lands, the enabled stores' names in
// v4's order, through the same read `list_all_custom_tools` makes (`roster.rs`
// — `findEnabled()`, sorted by `id.localeCompare`, code-unit order for the
// ASCII mount ids). ===
fn stand_in_stores_consulted(mount: &Connection) -> Vec<String> {
    use crate::db::doc_mount_points::DocMountPointsRepository;
    let mut mounts = crate::db::fallback::find_by_filter_or_empty("doc_mount_points", || {
        DocMountPointsRepository::new(mount).find_enabled_for_docedit()
    });
    mounts.sort_by(|a, b| a.id.cmp(&b.id));
    mounts.into_iter().map(|m| m.name).collect()
}
// === end LANE-REMOVE(P4.D340 → §S.5, v4 7263105c6) ===
