//! `inventory_equip` — take up objects (or kits) in this chat (v4
//! `lib/tools/handlers/inventory-equip-handler.ts` + `inventory-equip-tool.ts`,
//! `e1872c94f` + `102a23648`'s closed-shelf answer; P4.D328).
//!
//! Every ref resolves through the ONE rule (archetypes included, so an
//! archetype can be pointed at `inventory_pick_up`); the objects to carry go
//! through ONE call of the keystone's `equip_items_holding_lock`, which plans,
//! checks the cap and the roster shelf, writes, and RETURNS the change.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;

use crate::inventory::context::load_inventory_context;
use crate::inventory::equip::{equip_items_holding_lock, EquipChat, EquipOptions, SkipReason};
use crate::inventory::refs::{pick_for_gesture, Gesture, ResolveInventoryRefOptions};
use crate::inventory::resolve::inventory_label;
use crate::inventory::types::InventoryFields;
use crate::services::inventory_change_effects::{
    InventoryChangeContext, InventoryChangeProvenance,
};

use super::inventory_shared::{
    closed_shelf_message, error_message_of, format_summary, input_object,
    inventory_not_found_message, req_refs, resolve_tool_ref, summarize, summarize_match,
    InventoryObjectSummary, InventoryToolLog, InventoryToolLogLevel,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// A ref that was not carried, and why.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventorySkipped {
    #[serde(rename = "ref")]
    pub reference: String,
    pub reason: String,
}

/// v4 `InventoryEquipToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryEquipToolOutput {
    pub success: bool,
    pub equipped: Vec<InventoryObjectSummary>,
    pub created: Vec<InventoryObjectSummary>,
    pub already_equipped: Vec<InventoryObjectSummary>,
    pub skipped: Vec<InventorySkipped>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub const INVALID_REFS_MESSAGE: &str =
    "Invalid input: refs must be a list of one to eight object references.";
pub const CHAT_NOT_FOUND_MESSAGE: &str = "This chat could not be found.";

fn failure(error: String) -> InventoryEquipToolOutput {
    InventoryEquipToolOutput {
        success: false,
        equipped: Vec::new(),
        created: Vec::new(),
        already_equipped: Vec::new(),
        skipped: Vec::new(),
        error: Some(error),
    }
}

/// v4 `loadToolChat` — `repos.chats.findById` (the fallback read) as an
/// `EquipChat`, or `None`.
pub fn load_tool_chat(main: &rusqlite::Connection, chat_id: &str) -> Option<EquipChat> {
    crate::db::chats_read::find_by_id_or_none(main, chat_id)
        .map(|row| EquipChat::from_chat_row(&row))
}

/// v4 `characterChangeContext` for the in-write gesture: a character's own,
/// announced. (The turn's map rides the after-commit half — module header of
/// `services::inventory_change_effects`.)
pub fn character_change_context(user_id: &str) -> InventoryChangeContext<'_> {
    InventoryChangeContext {
        user_id,
        provenance: InventoryChangeProvenance::Character,
        announce: true,
        pending: None,
    }
}

fn skip_reason_text(reason: SkipReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryEquipToolOutput> {
    let Some(refs) = input_object(args).and_then(|o| req_refs(o, "refs").ok()) else {
        return InventoryHandled::logged(
            failure(INVALID_REFS_MESSAGE.into()),
            InventoryToolLog::outcome("invalid input"),
            InventoryToolLogLevel::Info,
        );
    };
    let refused = |error: String| {
        InventoryHandled::logged(
            failure(error.clone()),
            InventoryToolLog {
                refs: Some(refs.clone()),
                error: Some(error),
                ..InventoryToolLog::outcome("refused")
            },
            InventoryToolLogLevel::Info,
        )
    };
    let ctx = match load_inventory_context(cx.main, cx.mount, cx.character_id, Some(cx.chat_id)) {
        Ok(ctx) => ctx,
        Err(e) => return refused(error_message_of(&e.into())),
    };
    let Some(chat) = load_tool_chat(cx.main, cx.chat_id) else {
        return InventoryHandled::quiet(failure(CHAT_NOT_FOUND_MESSAGE.into()));
    };

    let mut output = InventoryEquipToolOutput {
        success: true,
        equipped: Vec::new(),
        created: Vec::new(),
        already_equipped: Vec::new(),
        skipped: Vec::new(),
        error: None,
    };
    let mut ids: Vec<String> = Vec::new();
    for reference in &refs {
        let matches = resolve_tool_ref(
            &ctx,
            reference,
            ResolveInventoryRefOptions {
                include_archetypes: true,
                include_archived: false,
            },
        );
        if matches.is_empty() {
            output.skipped.push(InventorySkipped {
                reference: reference.clone(),
                reason: closed_shelf_message(&ctx, reference, false)
                    .unwrap_or_else(|| inventory_not_found_message(reference)),
            });
            continue;
        }
        if let Some(pick) = pick_for_gesture(&matches, Gesture::Equip) {
            if !ids.iter().any(|id| id == pick.item.id()) {
                ids.push(pick.item.id().to_string());
            }
            continue;
        }
        if let Some(carried) = matches.iter().find(|m| m.equipped) {
            output.already_equipped.push(summarize_match(carried));
            continue;
        }
        let reason = match matches.iter().find(|m| m.item.is_archetype()) {
            Some(archetype) => format!(
                "\"{}\" is an archetype — pick up an instance of it with inventory_pick_up.",
                inventory_label(&archetype.item)
            ),
            None => format!("\"{reference}\" cannot be carried."),
        };
        output.skipped.push(InventorySkipped {
            reference: reference.clone(),
            reason,
        });
    }

    if ids.is_empty() {
        let log = InventoryToolLog {
            refs: Some(refs.clone()),
            resolved: Some(Vec::new()),
            skipped: Some(serde_json::to_value(&output.skipped).unwrap_or(Value::Null)),
            ..InventoryToolLog::outcome("nothing to equip")
        };
        if !output.already_equipped.is_empty() {
            return InventoryHandled::logged(output, log, InventoryToolLogLevel::Info);
        }
        let reasons: Vec<&str> = output.skipped.iter().map(|s| s.reason.as_str()).collect();
        let joined = reasons.join(" ");
        let mut out = failure(if joined.is_empty() {
            "Nothing to equip.".into()
        } else {
            joined
        });
        out.skipped = output.skipped;
        return InventoryHandled::logged(out, log, InventoryToolLogLevel::Info);
    }

    let change_ctx = character_change_context(cx.user_id);
    let (result, change) = match equip_items_holding_lock(
        cx.main,
        cx.mount,
        &chat,
        cx.character_id,
        &ids,
        &change_ctx,
        &EquipOptions::default(),
    ) {
        Ok(outcome) => outcome,
        Err(e) => return refused(error_message_of(&e)),
    };
    let created_ids: HashSet<&str> = result.created.iter().map(|i| i.id()).collect();
    output.equipped = result
        .equipped
        .iter()
        .map(|item| {
            summarize(
                item,
                created_ids.contains(item.id()) || ctx.pool.owns(item.id()),
                true,
            )
        })
        .collect();
    output.created = result
        .created
        .iter()
        .map(|item| summarize(item, true, true))
        .collect();
    for id in &result.already_equipped {
        if let Some(item) = ctx.pool.items.iter().find(|i| i.id() == id.as_str()) {
            output
                .already_equipped
                .push(summarize(item, ctx.pool.owns(id), true));
        }
    }
    for skip in &result.skipped {
        output.skipped.push(InventorySkipped {
            reference: skip.id.clone(),
            reason: format!(
                "a kit member could not be carried ({})",
                skip_reason_text(skip.reason)
            ),
        });
    }
    let log = InventoryToolLog {
        refs: Some(refs.clone()),
        resolved: Some(ids),
        equipped_ids: Some(output.equipped.iter().map(|s| s.id.clone()).collect()),
        created_ids: Some(output.created.iter().map(|s| s.id.clone()).collect()),
        ..InventoryToolLog::outcome("equipped")
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

/// v4 `formatInventoryEquipResults`.
pub fn format(output: &InventoryEquipToolOutput) -> String {
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
    let mut lines: Vec<String> = Vec::new();
    for s in &output.equipped {
        lines.push(format!("Equipped {}", format_summary(s)));
    }
    for s in &output.created {
        lines.push(format!("  (a new instance was made for a kit: {})", s.id));
    }
    for s in &output.already_equipped {
        lines.push(format!("Already equipped: {}", format_summary(s)));
    }
    for s in &output.skipped {
        lines.push(format!("Skipped \"{}\": {}", s.reference, s.reason));
    }
    if lines.is_empty() {
        "Nothing changed.".to_string()
    } else {
        lines.join("\n")
    }
}
