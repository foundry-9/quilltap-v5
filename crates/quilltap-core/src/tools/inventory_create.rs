//! `inventory_create` — a new plain object of the character's own, in their
//! vault, optionally equipped (v4 `lib/tools/handlers/inventory-create-
//! handler.ts` + `inventory-create-tool.ts`, `e1872c94f`; P4.D328).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::api::zod_issues::zod_error_message;
use crate::inventory::equip::{equip_items_holding_lock, EquipOptions};
use crate::inventory::errors::InventoryError;
use crate::inventory::item_mutations::{create_inventory_item, CreateInventoryItemOptions};
use crate::inventory::location::{resolve_inventory_location, ResolveInventoryLocationOptions};
use crate::inventory::resolve::resolve_item;
use crate::inventory::types::{parse_create_inventory_body, InventoryFields, InventoryScope};
use crate::jsstr::js_trim;
use crate::realtime::types::RealtimeTopic;

use super::inventory_equip::{character_change_context, load_tool_chat};
use super::inventory_shared::{
    error_message_of, format_summary, input_object, object_fields, opt_bool, req_string, summarize,
    InventoryObjectFields, InventoryObjectSummary, InventoryToolLog, InventoryToolLogLevel,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// v4 `InventoryCreateToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryCreateToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<InventoryObjectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipped: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: String) -> InventoryCreateToolOutput {
    InventoryCreateToolOutput {
        success: false,
        item: None,
        equipped: None,
        error: Some(error),
    }
}

struct CreateInput {
    name: String,
    fields: InventoryObjectFields,
    equip: Option<bool>,
}

fn parse(input: &Value) -> Option<CreateInput> {
    let o = input_object(input)?;
    Some(CreateInput {
        name: req_string(o, "name", Some(1), Some(200)).ok()?,
        fields: object_fields(o).ok()?,
        equip: opt_bool(o, "equip").ok()?,
    })
}

/// The create body v4 hands `createInventorySchema.parse` (its key order).
fn body_of(parsed: &CreateInput) -> Value {
    let f = &parsed.fields;
    let mut m = Map::new();
    m.insert("kind".into(), Value::String("item".into()));
    m.insert(
        "name".into(),
        Value::String(js_trim(&parsed.name).to_string()),
    );
    if let Some(title) = &f.title {
        let t = js_trim(title);
        m.insert(
            "title".into(),
            if t.is_empty() {
                Value::Null
            } else {
                Value::String(t.to_string())
            },
        );
    }
    if let Some(v) = &f.description {
        m.insert("description".into(), Value::String(v.clone()));
    }
    if let Some(v) = &f.image_description {
        m.insert("imageDescription".into(), Value::String(v.clone()));
    }
    if let Some(v) = &f.emoji {
        m.insert("emoji".into(), Value::String(v.clone()));
    }
    if let Some(v) = f.max_equipped {
        m.insert("maxEquipped".into(), Value::from(v));
    }
    if let Some(v) = &f.metadata {
        m.insert("metadata".into(), Value::Object(v.clone()));
    }
    m.insert("isArchetype".into(), Value::Bool(false));
    m.insert("canCopy".into(), Value::Bool(false));
    m.insert("equippedByDefault".into(), Value::Bool(false));
    Value::Object(m)
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryCreateToolOutput> {
    let Some(parsed) = parse(args) else {
        return InventoryHandled::logged(
            failure(
                "Invalid input: name is required; see the tool description for the other fields."
                    .into(),
            ),
            InventoryToolLog::outcome("invalid input"),
            InventoryToolLogLevel::Info,
        );
    };
    let refused = |error: String| {
        InventoryHandled::logged(
            failure(error.clone()),
            InventoryToolLog {
                refs: Some(vec![parsed.name.clone()]),
                error: Some(error),
                ..InventoryToolLog::outcome("refused")
            },
            InventoryToolLogLevel::Info,
        )
    };
    let vault = match resolve_inventory_location(
        cx.main,
        cx.mount,
        InventoryScope::Character,
        Some(cx.character_id),
        cx.user_id,
        ResolveInventoryLocationOptions::default(),
    ) {
        Ok(Some(vault)) => vault,
        Ok(None) => {
            return InventoryHandled::quiet(failure(
                "You have no vault to keep a new object in.".into(),
            ))
        }
        Err(e) => return refused(error_message_of(&InventoryError::Db(e))),
    };
    // `createInventorySchema.parse(...)` — a ZodError is thrown, and the catch
    // renders its message.
    let body = match parse_create_inventory_body(&body_of(&parsed)) {
        Ok(body) => body,
        Err(issues) => return refused(zod_error_message(&issues)),
    };
    let record = match create_inventory_item(
        cx.main,
        cx.mount,
        &vault,
        &body,
        CreateInventoryItemOptions::default(),
    ) {
        Ok(record) => record,
        Err(e) => return refused(error_message_of(&e)),
    };
    let mut tagged = record.clone();
    tagged.set("origin", vault.origin.to_value());
    tagged.set("mountPointId", Value::String(vault.mount_point_id.clone()));
    let item = resolve_item(&tagged, |_| None);

    let mut equipped = false;
    let mut changes = Vec::new();
    if parsed.equip == Some(true) {
        if let Some(chat) = load_tool_chat(cx.main, cx.chat_id) {
            let change_ctx = character_change_context(cx.user_id);
            match equip_items_holding_lock(
                cx.main,
                cx.mount,
                &chat,
                cx.character_id,
                &[item.id().to_string()],
                &change_ctx,
                // The file this call wrote, looked up after the pool (v4's
                // `known` — the pool reads it anyway on the writer, trap 4).
                &EquipOptions {
                    lenient: false,
                    known: vec![item.clone()],
                },
            ) {
                Ok((result, change)) => {
                    equipped = result.equipped.iter().any(|e| e.id() == item.id())
                        || result.already_equipped.iter().any(|id| id == item.id());
                    if let Some(change) = change {
                        changes.push((cx.chat_id.to_string(), change));
                    }
                }
                Err(e) => {
                    let mut handled = refused(error_message_of(&e));
                    handled.publishes = vec![(RealtimeTopic::Inventory, None)];
                    return handled;
                }
            }
        }
    }
    let log = InventoryToolLog {
        refs: Some(vec![parsed.name.clone()]),
        resolved: Some(vec![item.id().to_string()]),
        equipped: Some(equipped),
        ..InventoryToolLog::outcome("created")
    };
    InventoryHandled {
        output: InventoryCreateToolOutput {
            success: true,
            item: Some(summarize(&item, true, equipped)),
            equipped: Some(equipped),
            error: None,
        },
        changes,
        publishes: vec![(RealtimeTopic::Inventory, None)],
        log: Some((log, InventoryToolLogLevel::Debug)),
    }
}

/// v4 `formatInventoryCreateResults`.
pub fn format(output: &InventoryCreateToolOutput) -> String {
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
        "Created {} in your own vault{}.",
        format_summary(item),
        if output.equipped == Some(true) {
            ", equipped"
        } else {
            ""
        }
    )
}
