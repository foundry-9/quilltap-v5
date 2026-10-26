//! `inventory_unequip` — set down objects the character carries in this chat
//! (a kit sets down all its members) (v4 `lib/tools/handlers/
//! inventory-unequip-handler.ts` + `inventory-unequip-tool.ts`, `e1872c94f`;
//! P4.D328). Never fails on state.

use serde::Serialize;
use serde_json::Value;

use crate::inventory::context::load_inventory_context;
use crate::inventory::equip::{assert_participant, unequip_items_holding_lock, UnequipResult};
use crate::inventory::refs::{pick_for_gesture, Gesture, ResolveInventoryRefOptions};
use crate::inventory::types::InventoryFields;

use super::inventory_equip::{load_tool_chat, CHAT_NOT_FOUND_MESSAGE, INVALID_REFS_MESSAGE};
use super::inventory_shared::{
    error_message_of, format_summary, input_object, req_refs, resolve_tool_ref, summarize,
    InventoryObjectSummary, InventoryToolLog, InventoryToolLogLevel,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// v4 `InventoryUnequipToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryUnequipToolOutput {
    pub success: bool,
    pub unequipped: Vec<InventoryObjectSummary>,
    pub not_equipped: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: String) -> InventoryUnequipToolOutput {
    InventoryUnequipToolOutput {
        success: false,
        unequipped: Vec::new(),
        not_equipped: Vec::new(),
        error: Some(error),
    }
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryUnequipToolOutput> {
    let Some(refs) = input_object(args).and_then(|o| req_refs(o, "refs").ok()) else {
        return InventoryHandled::logged(
            failure(INVALID_REFS_MESSAGE.into()),
            InventoryToolLog::outcome("invalid input"),
            InventoryToolLogLevel::Info,
        );
    };
    let failed = |error: String| {
        InventoryHandled::logged(
            failure(error.clone()),
            InventoryToolLog {
                refs: Some(refs.clone()),
                error: Some(error),
                ..InventoryToolLog::outcome("failed")
            },
            InventoryToolLogLevel::Info,
        )
    };
    let ctx = match load_inventory_context(cx.main, cx.mount, cx.character_id, Some(cx.chat_id)) {
        Ok(ctx) => ctx,
        Err(e) => return failed(error_message_of(&e.into())),
    };
    let Some(chat) = load_tool_chat(cx.main, cx.chat_id) else {
        return InventoryHandled::quiet(failure(CHAT_NOT_FOUND_MESSAGE.into()));
    };
    let mut ids: Vec<String> = Vec::new();
    let mut not_equipped: Vec<String> = Vec::new();
    for reference in &refs {
        let matches = resolve_tool_ref(&ctx, reference, ResolveInventoryRefOptions::default());
        // The first equipped match, else the first kit (it sets down its
        // members).
        let pick = pick_for_gesture(&matches, Gesture::Unequip)
            .or_else(|| matches.iter().find(|m| m.item.is_kit()));
        match pick {
            Some(pick) => {
                if !ids.iter().any(|id| id == pick.item.id()) {
                    ids.push(pick.item.id().to_string());
                }
            }
            None => not_equipped.push(reference.clone()),
        }
    }
    let (result, change) = if ids.is_empty() {
        (UnequipResult::default(), None)
    } else {
        // v4 `unequipItems` asserts the seat before taking the lock.
        let outcome = assert_participant(&chat, cx.character_id).and_then(|()| {
            unequip_items_holding_lock(cx.main, cx.mount, &chat, cx.character_id, &ids)
        });
        match outcome {
            Ok(outcome) => outcome,
            Err(e) => return failed(error_message_of(&e)),
        }
    };
    let output = InventoryUnequipToolOutput {
        success: true,
        unequipped: result
            .unequipped
            .iter()
            .map(|item| summarize(item, ctx.pool.owns(item.id()), false))
            .collect(),
        not_equipped: not_equipped
            .into_iter()
            .chain(result.not_equipped.iter().cloned())
            .collect(),
        error: None,
    };
    let log = InventoryToolLog {
        refs: Some(refs.clone()),
        resolved: Some(ids),
        unequipped: Some(output.unequipped.iter().map(|s| s.id.clone()).collect()),
        ..InventoryToolLog::outcome("unequipped")
    };
    InventoryHandled {
        output,
        changes: change
            .map(|c| vec![(cx.chat_id.to_string(), c)])
            .unwrap_or_default(),
        publishes: Vec::new(),
        log: Some((log, InventoryToolLogLevel::Debug)),
    }
}

/// v4 `formatInventoryUnequipResults`.
pub fn format(output: &InventoryUnequipToolOutput) -> String {
    if !output.success {
        return format!(
            "Inventory Error: {}",
            output
                .error
                .as_deref()
                .filter(|e| !e.is_empty())
                .unwrap_or("Unknown error")
        );
    }
    let mut lines: Vec<String> = output
        .unequipped
        .iter()
        .map(|s| format!("Set down {}", format_summary(s)))
        .collect();
    for reference in &output.not_equipped {
        lines.push(format!("Not carried: \"{reference}\""));
    }
    if lines.is_empty() {
        "Nothing changed.".to_string()
    } else {
        lines.join("\n")
    }
}
