//! `inventory_read` — one object the character carries or can reach, in full
//! (v4 `lib/tools/handlers/inventory-read-handler.ts` + `inventory-read-tool.ts`,
//! `e1872c94f` + `102a23648`'s closed-shelf answer; P4.D328).

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::inventory::context::load_inventory_context;
use crate::inventory::refs::{InventoryState, ResolveInventoryRefOptions};
use crate::inventory::types::{InventoryFields, ResolvedInventoryItem};

use super::inventory_shared::{
    closed_shelf_message, display_name_of, error_message_of, input_object,
    inventory_not_found_message, kind_of, req_ref, resolve_tool_ref, state_for, tier_of,
    trimmed_title, InventoryObjectKind, InventoryToolLog, InventoryToolLogLevel, InventoryToolTier,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// A kit member as `inventory_read` lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryReadMember {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub state: Option<InventoryState>,
}

/// The object `inventory_read` answers (v4 `InventoryReadToolOutput`'s success
/// keys, in the handler's literal order).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryReadObject {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub state: Option<InventoryState>,
    pub kind: InventoryObjectKind,
    pub tier: InventoryToolTier,
    pub is_own: bool,
    pub description: Option<String>,
    pub image_description: Option<String>,
    pub emoji: Option<String>,
    pub metadata: Map<String, Value>,
    pub parent_id: Option<String>,
    pub archetype_title: Option<String>,
    /// `item.maxEquipped` as stored (absent → the key absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_equipped: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_copy: Option<Value>,
    pub has_picture: bool,
    pub folder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<InventoryReadMember>>,
}

/// v4 `InventoryReadToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryReadToolOutput {
    pub success: bool,
    #[serde(flatten)]
    pub object: Option<InventoryReadObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: String) -> InventoryReadToolOutput {
    InventoryReadToolOutput {
        success: false,
        object: None,
        error: Some(error),
    }
}

fn opt_str(item: &ResolvedInventoryItem, key: &str) -> Option<String> {
    item.str_of(key).map(str::to_string)
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryReadToolOutput> {
    let Some(reference) = input_object(args).and_then(|o| req_ref(o, "ref").ok()) else {
        return InventoryHandled::logged(
            failure("Invalid input: ref is required.".into()),
            InventoryToolLog::outcome("invalid input"),
            InventoryToolLogLevel::Info,
        );
    };
    let ctx = match load_inventory_context(cx.main, cx.mount, cx.character_id, Some(cx.chat_id)) {
        Ok(ctx) => ctx,
        Err(e) => {
            let error = error_message_of(&e.into());
            return InventoryHandled::logged(
                failure(error.clone()),
                InventoryToolLog {
                    refs: Some(vec![reference]),
                    error: Some(error),
                    ..InventoryToolLog::outcome("failed")
                },
                InventoryToolLogLevel::Info,
            );
        }
    };
    let matches = resolve_tool_ref(
        &ctx,
        &reference,
        ResolveInventoryRefOptions {
            include_archetypes: true,
            include_archived: true,
        },
    );
    let Some(m) = matches
        .iter()
        .find(|m| m.equipped)
        .or_else(|| matches.iter().find(|m| m.own))
        .or_else(|| matches.first())
    else {
        // ⟨102a⟩ the closed shelf's sentence first.
        let error = closed_shelf_message(&ctx, &reference, false)
            .unwrap_or_else(|| inventory_not_found_message(&reference));
        return InventoryHandled::logged(
            failure(error),
            InventoryToolLog {
                refs: Some(vec![reference]),
                resolved: Some(Vec::new()),
                ..InventoryToolLog::outcome("not found")
            },
            InventoryToolLogLevel::Info,
        );
    };
    let item = &m.item;
    let by_id: HashMap<&str, &ResolvedInventoryItem> =
        ctx.pool.items.iter().map(|i| (i.id(), i)).collect();
    let archetype = item.parent_id().and_then(|p| by_id.get(p));
    let members = item.is_kit().then(|| {
        item.item_ids()
            .iter()
            .map(|id| match by_id.get(id.as_str()) {
                Some(member) => InventoryReadMember {
                    id: id.clone(),
                    name: member.name().to_string(),
                    title: trimmed_title(*member),
                    state: state_for(member, ctx.pool.owns(id), ctx.equipped_ids.contains(id)),
                },
                None => InventoryReadMember {
                    id: id.clone(),
                    name: "(unreachable)".into(),
                    title: None,
                    state: None,
                },
            })
            .collect()
    });
    // `parentName ?? (archetype ? archetype.title || archetype.name : null)`.
    let archetype_title = item.parent_id().and_then(|_| {
        item.parent_name()
            .flatten()
            .map(str::to_string)
            .or_else(|| archetype.map(|a| a.title_or_name().to_string()))
    });
    let object = InventoryReadObject {
        id: item.id().to_string(),
        name: item.name().to_string(),
        title: trimmed_title(item),
        state: state_for(item, m.own, m.equipped),
        kind: kind_of(item),
        tier: tier_of(item, m.own),
        is_own: m.own,
        description: opt_str(item, "description"),
        image_description: opt_str(item, "imageDescription"),
        emoji: opt_str(item, "emoji"),
        metadata: item.metadata(),
        parent_id: item.parent_id().map(str::to_string),
        archetype_title,
        max_equipped: item.get("maxEquipped").cloned(),
        can_copy: item.get("canCopy").cloned(),
        has_picture: item.image_file_id().is_some_and(|f| !f.is_empty()),
        folder: Some(item.folder().to_string()).filter(|f| !f.is_empty()),
        members,
    };
    let resolved = vec![item.id().to_string()];
    InventoryHandled::logged(
        InventoryReadToolOutput {
            success: true,
            object: Some(object),
            error: None,
        },
        InventoryToolLog {
            refs: Some(vec![reference]),
            resolved: Some(resolved),
            ..InventoryToolLog::outcome("read")
        },
        InventoryToolLogLevel::Debug,
    )
}

/// JS `${v}` for a stored scalar (the `max_equipped` line).
fn js_text(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n
            .as_f64()
            .map(crate::pascal::js_value::number_to_string)
            .unwrap_or_else(|| n.to_string()),
        Some(other) => other.to_string(),
    }
}

fn truthy(v: Option<&Value>) -> bool {
    crate::inventory::types::js_truthy(v)
}

/// v4 `formatInventoryReadResults`.
pub fn format(output: &InventoryReadToolOutput) -> String {
    let Some(o) = output.object.as_ref().filter(|_| output.success) else {
        return format!(
            "Inventory Error: {}",
            output
                .error
                .as_deref()
                .filter(|e| !e.is_empty())
                .unwrap_or("Unknown error")
        );
    };
    let emoji = o
        .emoji
        .as_deref()
        .filter(|e| !e.is_empty())
        .map(|e| format!("{e} "))
        .unwrap_or_default();
    let state = if o.kind == InventoryObjectKind::Archetype {
        "archetype (pick up an instance with inventory_pick_up)"
    } else {
        o.state.map_or("null", InventoryState::as_str)
    };
    let mut lines = vec![
        format!(
            "{emoji}{} [{}]",
            display_name_of(&o.name, o.title.as_deref()),
            o.id
        ),
        format!(
            "State: {state} · {}{}",
            o.tier.as_str(),
            if o.is_own {
                ""
            } else {
                " [shared — read-only]"
            }
        ),
    ];
    if let Some(t) = o.archetype_title.as_deref().filter(|t| !t.is_empty()) {
        lines.push(format!("Instance of: {t}"));
    }
    if let Some(d) = o.description.as_deref().filter(|d| !d.is_empty()) {
        lines.push(format!("Description: {d}"));
    }
    if let Some(d) = o.image_description.as_deref().filter(|d| !d.is_empty()) {
        lines.push(format!("Looks: {d}"));
    }
    lines.push(format!(
        "At most {} carried at a time{}{}",
        js_text(o.max_equipped.as_ref()),
        if truthy(o.can_copy.as_ref()) {
            ""
        } else {
            " · cannot be copied"
        },
        if o.has_picture {
            " · has a picture"
        } else {
            ""
        }
    ));
    if let Some(f) = o.folder.as_deref() {
        lines.push(format!("Folder: {f}"));
    }
    if !o.metadata.is_empty() {
        lines.push(format!(
            "Metadata: {}",
            serde_json::to_string(&o.metadata).unwrap_or_default()
        ));
    }
    if let Some(members) = &o.members {
        lines.push("Members:".into());
        for m in members {
            lines.push(format!(
                "  {} [{}] ({})",
                display_name_of(&m.name, m.title.as_deref()),
                m.id,
                m.state.map_or("archetype", InventoryState::as_str)
            ));
        }
    }
    lines.join("\n")
}
