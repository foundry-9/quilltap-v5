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
use rusqlite::Connection;
use serde::Serialize;

use super::roster::{list_all_custom_tools, CustomToolLoadStage};

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
    let mut keys: Vec<ToolGateKeyUse> = Vec::new();
    for entry in &library.entries {
        let definition = &entry.definition;
        for (clause, gate) in [
            (GateKeyClause::AvailableWhen, &definition.available_when),
            (GateKeyClause::WithheldWhen, &definition.withheld_when),
        ] {
            let Some(gate) = gate else { continue };
            // Every group of an any-of. `inventoryAggregate` keys name a scope
            // and a field, never an object, so no title can shadow one: not
            // counted.
            for group in gate.gate_groups() {
                for (subject, refs) in [
                    (
                        GateKeySubject::Inventory,
                        group
                            .inventory
                            .iter()
                            .flatten()
                            .map(|(r, _)| r)
                            .collect::<Vec<_>>(),
                    ),
                    (
                        GateKeySubject::InventoryCount,
                        group
                            .inventory_count
                            .iter()
                            .flatten()
                            .map(|(r, _)| r)
                            .collect::<Vec<_>>(),
                    ),
                ] {
                    for reference in refs {
                        keys.push(ToolGateKeyUse {
                            reference: reference.clone(),
                            tool_name: definition.name.clone(),
                            clause,
                            subject,
                            mount_name: entry.mount_name.clone(),
                            definition_path: entry.definition_path.clone(),
                        });
                    }
                }
            }
        }
    }

    // What discovery could not look at, plus the load errors whose bytes or
    // JSON could not be read. A schema failure is never dealt, so its keys are
    // not live and it is not counted as unread.
    let mut unread: Vec<ToolGateKeyUnread> = library
        .unread
        .iter()
        .map(|u| ToolGateKeyUnread {
            mount_name: u.mount_name.clone(),
            definition_path: u.definition_path.clone(),
            reason: u.reason.clone(),
        })
        .chain(
            library
                .errors
                .iter()
                .filter(|e| {
                    matches!(
                        e.stage,
                        Some(CustomToolLoadStage::Read) | Some(CustomToolLoadStage::Json)
                    )
                })
                .map(|e| ToolGateKeyUnread {
                    mount_name: e.mount_name.clone(),
                    definition_path: e.definition_path.clone(),
                    reason: e.reason.clone(),
                }),
        )
        .collect();
    let stores_consulted = library.stores_consulted.clone();
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
