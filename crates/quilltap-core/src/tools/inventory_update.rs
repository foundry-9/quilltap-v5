//! `inventory_update` — change one of the character's OWN objects (v4
//! `lib/tools/handlers/inventory-update-handler.ts` + `inventory-update-tool.ts`,
//! `e1872c94f`; P4.D328). A shared object is read-only; NO closed-shelf
//! sentence here (R-16 — the plain rule answers). The object reported back is
//! the one a SECOND context load in the same write reads (trap 5), so the
//! summary reflects what was written.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::inventory::context::load_inventory_context;
use crate::inventory::item_mutations::update_inventory_item;
use crate::inventory::pool::LoadPoolContext;
use crate::inventory::refs::ResolveInventoryRefOptions;
use crate::inventory::types::{InventoryFields, ResolvedInventoryItem};
use crate::jsstr::js_trim;

use super::inventory_shared::{
    error_message_of, format_summary, input_object, inventory_not_found_message, load_own_record,
    object_fields, opt_string, req_ref, resolve_tool_ref, shared_inventory_item_read_only_message,
    summarize, InventoryObjectFields, InventoryObjectSummary, InventoryToolLog,
    InventoryToolLogLevel,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// v4 `InventoryUpdateToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryUpdateToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<InventoryObjectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: String) -> InventoryUpdateToolOutput {
    InventoryUpdateToolOutput {
        success: false,
        item: None,
        fields: None,
        error: Some(error),
    }
}

struct UpdateInput {
    reference: String,
    name: Option<String>,
    fields: InventoryObjectFields,
}

/// v4 `inventoryUpdateToolInputSchema` — the object, then its `.refine`: at
/// least one key other than `ref` with a defined value (the refine's message
/// never reaches the model — `validate*Input` answers null, trap 31).
fn parse(input: &Value) -> Option<UpdateInput> {
    let o = input_object(input)?;
    let parsed = UpdateInput {
        reference: req_ref(o, "ref").ok()?,
        name: opt_string(o, "name", Some(1), Some(200)).ok()?,
        fields: object_fields(o).ok()?,
    };
    let f = &parsed.fields;
    let any = parsed.name.is_some()
        || f.title.is_some()
        || f.description.is_some()
        || f.image_description.is_some()
        || f.emoji.is_some()
        || f.max_equipped.is_some()
        || f.metadata.is_some();
    any.then_some(parsed)
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryUpdateToolOutput> {
    let Some(parsed) = parse(args) else {
        return InventoryHandled::logged(
            failure("Invalid input: ref and at least one field to change are required.".into()),
            InventoryToolLog::outcome("invalid input"),
            InventoryToolLogLevel::Info,
        );
    };
    let reference = parsed.reference.clone();
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
                "changed",
            )),
            InventoryToolLog {
                refs: Some(vec![reference.clone()]),
                resolved: Some(vec![matches[0].item.id().to_string()]),
                ..InventoryToolLog::outcome("refused: shared")
            },
            InventoryToolLogLevel::Info,
        );
    };
    let (vault, record) = match load_own_record(&cx, m.item.id()) {
        Ok(Some(own)) => own,
        Ok(None) => {
            return InventoryHandled::quiet(failure(inventory_not_found_message(&reference)))
        }
        Err(e) => return refused(error_message_of(&e)),
    };

    // The patch, in field order; `metadata` merged key by key into the
    // RECORD's own metadata (an instance's overrides merge over its own file,
    // never the archetype view), a `null` deleting (trap 30).
    let f = &parsed.fields;
    let mut patch = Map::new();
    if let Some(name) = &parsed.name {
        patch.insert("name".into(), Value::String(js_trim(name).to_string()));
    }
    if let Some(title) = &f.title {
        let t = js_trim(title);
        patch.insert(
            "title".into(),
            if t.is_empty() {
                Value::Null
            } else {
                Value::String(t.to_string())
            },
        );
    }
    if let Some(v) = &f.description {
        patch.insert("description".into(), Value::String(v.clone()));
    }
    if let Some(v) = &f.image_description {
        patch.insert("imageDescription".into(), Value::String(v.clone()));
    }
    if let Some(v) = &f.emoji {
        patch.insert("emoji".into(), Value::String(v.clone()));
    }
    if let Some(v) = f.max_equipped {
        patch.insert("maxEquipped".into(), Value::from(v));
    }
    if let Some(metadata) = &f.metadata {
        let mut merged: Map<String, Value> = record
            .get("metadata")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (key, value) in metadata {
            if value.is_null() {
                merged.shift_remove(key);
            } else {
                merged.insert(key.clone(), value.clone());
            }
        }
        patch.insert("metadata".into(), Value::Object(merged));
    }

    let updated = match update_inventory_item(
        cx.main,
        cx.mount,
        &vault,
        &record,
        &patch,
        LoadPoolContext {
            chat_id: Some(cx.chat_id),
            project_id: None,
        },
    ) {
        Ok(updated) => updated,
        Err(e) => return refused(error_message_of(&e)),
    };
    let Some(updated) = updated else {
        return InventoryHandled::quiet(failure(format!("Failed to change \"{reference}\".")));
    };
    // Resolved against the pool already in hand: the edit changed this
    // object, not its archetype (v4 `0276d10d9` — `serverPool.resolve({
    // ...updated, origin: vault.origin, mountPointId: vault.mountPointId })`).
    let after: ResolvedInventoryItem = resolve_in_hand(&ctx.server_pool, &updated, &vault);
    let fields: Vec<String> = patch.keys().cloned().collect();
    let log = InventoryToolLog {
        refs: Some(vec![reference.clone()]),
        resolved: Some(vec![record.id().to_string()]),
        fields: Some(fields.clone()),
        ..InventoryToolLog::outcome("updated")
    };
    InventoryHandled {
        output: InventoryUpdateToolOutput {
            success: true,
            item: Some(summarize(&after, true, m.equipped)),
            fields: Some(fields),
            error: None,
        },
        changes: Vec::new(),
        publishes: Vec::new(),
        log: Some((log, InventoryToolLogLevel::Debug)),
    }
}

/// v4 `formatInventoryUpdateResults`.
pub fn format(output: &InventoryUpdateToolOutput) -> String {
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
    format!(
        "Changed {} on {}.",
        output.fields.as_deref().unwrap_or_default().join(", "),
        format_summary(item)
    )
}

/// `pool.resolve({ ...updated, origin: vault.origin, mountPointId:
/// vault.mountPointId })` — the updated record tagged with its vault (the
/// spread keeps the record's key order; the two keys land last, or in place
/// when the record already carries them).
fn resolve_in_hand(
    pool: &crate::inventory::pool::InventoryPool,
    updated: &crate::inventory::types::InventoryRecord,
    vault: &crate::inventory::location::InventoryLocation,
) -> ResolvedInventoryItem {
    let mut tagged = updated.clone();
    tagged.set("origin", vault.origin.to_value());
    tagged.set("mountPointId", Value::String(vault.mount_point_id.clone()));
    pool.resolve(&tagged)
}
