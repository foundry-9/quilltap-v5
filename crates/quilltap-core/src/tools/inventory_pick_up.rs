//! `inventory_pick_up` — an instance of an archetype into the character's own
//! vault, equipped by default (v4 `lib/tools/handlers/inventory-pick-up-
//! handler.ts` + `inventory-pick-up-tool.ts`, `e1872c94f` + `102a23648`'s
//! closed-shelf answer; P4.D328). The keystone's `pick_up_items_holding_lock`
//! checks the cap over the whole plan BEFORE it writes, so a breach creates
//! nothing.

use serde::Serialize;
use serde_json::Value;

use crate::inventory::context::load_inventory_context;
use crate::inventory::equip::{assert_participant, pick_up_items_holding_lock, PickUpOptions};
use crate::inventory::errors::InventoryRefusal;
use crate::inventory::refs::ResolveInventoryRefOptions;
use crate::inventory::resolve::inventory_label;
use crate::inventory::types::InventoryFields;

use super::inventory_equip::{character_change_context, load_tool_chat, CHAT_NOT_FOUND_MESSAGE};
use super::inventory_shared::{
    closed_shelf_message, display_name_of, error_message_of, format_summary, input_object,
    inventory_not_found_message, opt_bool, opt_string, req_ref, resolve_tool_ref, summarize,
    trimmed_title, InventoryObjectSummary, InventoryToolLog, InventoryToolLogLevel,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// The archetype an instance was made from.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryPickUpArchetype {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
}

/// v4 `InventoryPickUpToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryPickUpToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<InventoryObjectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archetype: Option<InventoryPickUpArchetype>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipped: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: String) -> InventoryPickUpToolOutput {
    InventoryPickUpToolOutput {
        success: false,
        item: None,
        archetype: None,
        equipped: None,
        error: Some(error),
    }
}

struct PickUpInput {
    reference: String,
    title: Option<String>,
    equip: Option<bool>,
}

fn parse(input: &Value) -> Option<PickUpInput> {
    let o = input_object(input)?;
    Some(PickUpInput {
        reference: req_ref(o, "ref").ok()?,
        title: opt_string(o, "title", Some(1), Some(200)).ok()?,
        equip: opt_bool(o, "equip").ok()?,
    })
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryPickUpToolOutput> {
    let Some(parsed) = parse(args) else {
        return InventoryHandled::logged(
            failure("Invalid input: ref names the archetype to pick up.".into()),
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
    let Some(chat) = load_tool_chat(cx.main, cx.chat_id) else {
        return InventoryHandled::quiet(failure(CHAT_NOT_FOUND_MESSAGE.into()));
    };
    let matches = resolve_tool_ref(
        &ctx,
        &reference,
        ResolveInventoryRefOptions {
            include_archetypes: true,
            include_archived: false,
        },
    );
    let Some(archetype) = matches
        .iter()
        .find(|m| m.item.is_archetype())
        .map(|m| m.item.clone())
    else {
        // An archetype the roster hides answers first: "magic bow" may also
        // name the character's own bow, and "not an archetype" would mislead.
        let error =
            closed_shelf_message(&ctx, &reference, true).unwrap_or_else(|| match matches.first() {
                Some(first) => {
                    InventoryRefusal::invalid_parent(
                        format!(
                            "\"{}\" is not an archetype; equip it with inventory_equip instead",
                            inventory_label(&first.item)
                        ),
                        serde_json::Map::new(),
                    )
                    .message
                }
                None => inventory_not_found_message(&reference),
            });
        return InventoryHandled::logged(
            failure(error.clone()),
            InventoryToolLog {
                refs: Some(vec![reference.clone()]),
                resolved: Some(matches.iter().map(|m| m.item.id().to_string()).collect()),
                error: Some(error),
                ..InventoryToolLog::outcome("refused")
            },
            InventoryToolLogLevel::Info,
        );
    };
    let change_ctx = character_change_context(cx.user_id);
    let opts = PickUpOptions {
        title: parsed.title.clone(),
        // v4 `opts.equip !== false`.
        equip: parsed.equip != Some(false),
    };
    let ids = vec![archetype.id().to_string()];
    let outcome = assert_participant(&chat, cx.character_id).and_then(|()| {
        pick_up_items_holding_lock(
            cx.main,
            cx.mount,
            &chat,
            cx.character_id,
            &ids,
            &change_ctx,
            &opts,
        )
    });
    let (result, change) = match outcome {
        Ok(outcome) => outcome,
        Err(e) => return refused(error_message_of(&e)),
    };
    let created = result.created.first();
    let log = InventoryToolLog {
        refs: Some(vec![reference.clone()]),
        resolved: Some(vec![archetype.id().to_string()]),
        created: created.map(|c| c.id().to_string()),
        equipped: Some(result.equipped),
        ..InventoryToolLog::outcome("picked up")
    };
    InventoryHandled {
        output: InventoryPickUpToolOutput {
            success: true,
            item: created.map(|c| summarize(c, true, result.equipped)),
            archetype: Some(InventoryPickUpArchetype {
                id: archetype.id().to_string(),
                name: archetype.name().to_string(),
                title: trimmed_title(&archetype),
            }),
            equipped: Some(result.equipped),
            error: None,
        },
        changes: change
            .map(|c| vec![(cx.chat_id.to_string(), c)])
            .unwrap_or_default(),
        publishes: Vec::new(),
        log: Some((log, InventoryToolLogLevel::Debug)),
    }
}

/// v4 `formatInventoryPickUpResults`.
pub fn format(output: &InventoryPickUpToolOutput) -> String {
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
    let archetype = output
        .archetype
        .as_ref()
        .map(|a| display_name_of(&a.name, a.title.as_deref()))
        .unwrap_or_else(|| "the archetype".to_string());
    let Some(item) = &output.item else {
        return format!("Nothing was picked up from {archetype}.");
    };
    format!(
        "Picked up {} — an instance of {archetype}, in your own vault{}.",
        format_summary(item),
        if output.equipped == Some(true) {
            ", equipped"
        } else {
            ", not equipped"
        }
    )
}
