//! `inventory_archive` — put away (or restore) one of the character's OWN
//! objects (v4 `lib/tools/handlers/inventory-archive-handler.ts` +
//! `inventory-archive-tool.ts`, `e1872c94f`; P4.D328). Archiving drops the
//! object from the character's carried set in EVERY chat (so a later restore
//! does not quietly re-equip it); archiving an equipped one is announced as a
//! setting-down. NO closed-shelf sentence here (R-16).

use serde::Serialize;
use serde_json::Value;

use crate::inventory::context::load_inventory_context;
use crate::inventory::equip::drop_carried_everywhere;
use crate::inventory::errors::InventoryError;
use crate::inventory::item_mutations::set_inventory_item_archived;
use crate::inventory::location::{resolve_inventory_location, ResolveInventoryLocationOptions};
use crate::inventory::refs::ResolveInventoryRefOptions;
use crate::inventory::types::{InventoryFields, InventoryScope};
use crate::realtime::types::RealtimeTopic;
use crate::services::inventory_change_effects::InventoryChange;

use super::inventory_shared::{
    error_message_of, format_summary, input_object, inventory_not_found_message, req_ref,
    resolve_tool_ref, shared_inventory_item_read_only_message, summarize, InventoryObjectSummary,
    InventoryToolLog, InventoryToolLogLevel,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// v4 `InventoryArchiveToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryArchiveToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<InventoryObjectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: String) -> InventoryArchiveToolOutput {
    InventoryArchiveToolOutput {
        success: false,
        item: None,
        archived: None,
        changed: None,
        error: Some(error),
    }
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryArchiveToolOutput> {
    let parsed = input_object(args).and_then(|o| {
        let reference = req_ref(o, "ref").ok()?;
        match o.get("archived") {
            Some(Value::Bool(b)) => Some((reference, *b)),
            _ => None,
        }
    });
    let Some((reference, archived)) = parsed else {
        return InventoryHandled::logged(
            failure("Invalid input: ref and archived (true or false) are required.".into()),
            InventoryToolLog::outcome("invalid input"),
            InventoryToolLogLevel::Info,
        );
    };
    let refused = |error: String| {
        InventoryHandled::logged(
            failure(error.clone()),
            InventoryToolLog {
                refs: Some(vec![reference.clone()]),
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
    let matches = resolve_tool_ref(
        &ctx,
        &reference,
        ResolveInventoryRefOptions {
            include_archetypes: true,
            include_archived: true,
        },
    );
    if matches.is_empty() {
        return InventoryHandled::quiet(failure(inventory_not_found_message(&reference)));
    }
    let Some(m) = matches.iter().find(|m| m.own) else {
        return InventoryHandled::logged(
            failure(shared_inventory_item_read_only_message(
                &matches[0].item,
                "archived",
            )),
            InventoryToolLog {
                refs: Some(vec![reference.clone()]),
                resolved: Some(vec![matches[0].item.id().to_string()]),
                ..InventoryToolLog::outcome("refused: shared")
            },
            InventoryToolLogLevel::Info,
        );
    };
    let vault = match resolve_inventory_location(
        cx.main,
        cx.mount,
        InventoryScope::Character,
        Some(cx.character_id),
        cx.user_id,
        ResolveInventoryLocationOptions::default(),
    ) {
        Ok(v) => v,
        Err(e) => return refused(error_message_of(&InventoryError::Db(e))),
    };
    let record = match &vault {
        Some(vault) => match vault.find_item(cx.mount, m.item.id()) {
            Ok(r) => r,
            Err(e) => return refused(error_message_of(&InventoryError::Db(e))),
        },
        None => None,
    };
    let (Some(vault), Some(record)) = (vault, record) else {
        return InventoryHandled::quiet(failure(inventory_not_found_message(&reference)));
    };

    let changed = match set_inventory_item_archived(cx.main, cx.mount, &vault, &record, archived) {
        Ok(outcome) => outcome.changed,
        Err(e) => return refused(error_message_of(&e)),
    };
    let mut publishes = Vec::new();
    let mut changes = Vec::new();
    if changed {
        publishes.push((RealtimeTopic::Inventory, None));
        if archived
            && drop_carried_everywhere(
                cx.main,
                cx.character_id,
                record.id(),
                &[cx.chat_id.to_string()],
            ) > 0
        {
            publishes.push((RealtimeTopic::Chats, Some(cx.chat_id.to_string())));
        }
        if archived && m.equipped {
            changes.push((
                cx.chat_id.to_string(),
                InventoryChange {
                    equipped: Vec::new(),
                    unequipped: vec![m.item.clone()],
                    picked_up: None,
                    given: None,
                },
            ));
        }
    }
    let log = InventoryToolLog {
        refs: Some(vec![reference.clone()]),
        resolved: Some(vec![record.id().to_string()]),
        archived: Some(archived),
        ..InventoryToolLog::outcome(if changed { "changed" } else { "unchanged" })
    };
    InventoryHandled {
        output: InventoryArchiveToolOutput {
            success: true,
            item: Some(summarize(&m.item, true, !archived && m.equipped)),
            archived: Some(archived),
            changed: Some(changed),
            error: None,
        },
        changes,
        publishes,
        log: Some((log, InventoryToolLogLevel::Debug)),
    }
}

/// v4 `formatInventoryArchiveResults`.
pub fn format(output: &InventoryArchiveToolOutput) -> String {
    let Some(item) = output.item.as_ref().filter(|_| output.success) else {
        return format!(
            "Inventory Error: {}",
            output
                .error
                .as_deref()
                .filter(|e| !e.is_empty())
                .unwrap_or("Unknown error")
        );
    };
    let summary = format_summary(item);
    let archived = output.archived == Some(true);
    if output.changed != Some(true) {
        return format!(
            "{summary} was already {}; nothing changed.",
            if archived { "archived" } else { "in use" }
        );
    }
    if archived {
        format!("Archived {summary}. It is put away — no longer carried or listed — and can be restored.")
    } else {
        format!("Restored {summary}.")
    }
}
