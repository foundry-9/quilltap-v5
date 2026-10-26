//! `inventory_give` — hand one of the character's OWN objects to another
//! character in this chat (v4 `lib/tools/handlers/inventory-give-handler.ts`
//! + `inventory-give-tool.ts`, `e1872c94f`; P4.D328).
//!
//! The order is the contract (trap 6): **1.** the move (the keystone's
//! `transfer_inventory_item` — the id kept; a refused move leaves the giver
//! carrying it everywhere), **2.** the drop from the giver's carried set in
//! every chat, **3.** the give's own Pascal line, posted at once and struck
//! from the giver's pending entry. NO closed-shelf sentence here (R-16).

use serde::Serialize;
use serde_json::Value;

use crate::db::characters_read;
use crate::inventory::context::load_inventory_context;
use crate::inventory::equip::drop_carried_everywhere;
use crate::inventory::errors::InventoryError;
use crate::inventory::location::{resolve_inventory_location, ResolveInventoryLocationOptions};
use crate::inventory::refs::ResolveInventoryRefOptions;
use crate::inventory::resolve::inventory_label;
use crate::inventory::transfers::{
    transfer_inventory_item, InventoryTransferAction, TransferInventoryItemParams,
};
use crate::inventory::types::{InventoryFields, InventoryScope};
use crate::jsstr::js_trim;
use crate::realtime::types::RealtimeTopic;
use crate::services::inventory_change_effects::{InventoryChange, InventoryGive};

use super::inventory_equip::{load_tool_chat, CHAT_NOT_FOUND_MESSAGE};
use super::inventory_shared::{
    display_name_of, error_message_of, input_object, inventory_not_found_message, req_ref,
    req_string, resolve_tool_ref, shared_inventory_item_read_only_message, summarize,
    InventoryObjectSummary, InventoryToolLog, InventoryToolLogLevel,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// Who received the object.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryRecipient {
    pub id: String,
    pub name: String,
}

/// v4 `InventoryGiveToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryGiveToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<InventoryObjectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<InventoryRecipient>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The receiver among the chat's live participants, by id or name; or why
/// not (v4 `resolveReceiver`). R-18: v4 loads the cast with `Promise.all` and
/// takes the FIRST candidate that matches; here the characters are read one
/// by one in participant order and the search stops at the first match — the
/// same character, the same sentence. An id match reads only that one.
fn resolve_receiver(
    main: &rusqlite::Connection,
    participants: &[crate::inventory::equip::EquipParticipant],
    giver_id: &str,
    to: &str,
) -> Result<InventoryRecipient, String> {
    let wanted = js_trim(to).to_lowercase();
    let seated: Vec<&str> = participants
        .iter()
        .filter(|p| p.status.as_deref() != Some("removed"))
        .filter_map(|p| p.character_id.as_deref().filter(|id| !id.is_empty()))
        .collect();
    let by_id = seated
        .iter()
        .copied()
        .find(|id| id.to_lowercase() == wanted);
    let candidates: Vec<&str> = match by_id {
        Some(id) => vec![id],
        None => seated,
    };
    for id in candidates {
        // `repos.characters.findByIdRaw` — the fallback raw read.
        let Some(character) = characters_read::find_by_id_raw_or_none(main, id) else {
            continue;
        };
        let name = character.get("name").and_then(Value::as_str).unwrap_or("");
        if by_id.is_none() && js_trim(name).to_lowercase() != wanted {
            continue;
        }
        if id == giver_id {
            return Err("You cannot give an object to yourself.".into());
        }
        if crate::inventory::types::js_truthy(character.get("archivedAt")) {
            return Err(format!("{name} is archived and cannot receive anything."));
        }
        return Ok(InventoryRecipient {
            id: id.to_string(),
            name: name.to_string(),
        });
    }
    Err(format!(
        "No one named \"{to}\" is in this chat to receive it."
    ))
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryGiveToolOutput> {
    let parsed = input_object(args).and_then(|o| {
        Some((
            req_ref(o, "ref").ok()?,
            req_string(o, "to", Some(1), Some(200)).ok()?,
        ))
    });
    let Some((reference, to)) = parsed else {
        return InventoryHandled::logged(
            InventoryGiveToolOutput {
                success: false,
                item: None,
                recipient: None,
                error: Some(
                    "Invalid input: ref (the object) and to (the receiver) are required.".into(),
                ),
            },
            InventoryToolLog::outcome("invalid input"),
            InventoryToolLogLevel::Info,
        );
    };
    // v4 `refuse(error, resolved)` — the info line, then the failure.
    let refuse = |error: String, resolved: Vec<String>| InventoryHandled {
        output: InventoryGiveToolOutput {
            success: false,
            item: None,
            recipient: None,
            error: Some(error.clone()),
        },
        changes: Vec::new(),
        publishes: Vec::new(),
        log: Some((
            InventoryToolLog {
                refs: Some(vec![reference.clone()]),
                resolved: Some(resolved),
                to: Some(to.clone()),
                error: Some(error),
                ..InventoryToolLog::outcome("refused")
            },
            InventoryToolLogLevel::Info,
        )),
    };
    let ctx = match load_inventory_context(cx.main, cx.mount, cx.character_id, Some(cx.chat_id)) {
        Ok(ctx) => ctx,
        Err(e) => return refuse(error_message_of(&e.into()), Vec::new()),
    };
    let Some(chat) = load_tool_chat(cx.main, cx.chat_id) else {
        return refuse(CHAT_NOT_FOUND_MESSAGE.into(), Vec::new());
    };
    let matches = resolve_tool_ref(&ctx, &reference, ResolveInventoryRefOptions::default());
    if matches.is_empty() {
        return refuse(inventory_not_found_message(&reference), Vec::new());
    }
    let Some(m) = matches.iter().find(|m| m.own) else {
        return refuse(
            shared_inventory_item_read_only_message(&matches[0].item, "given"),
            vec![matches[0].item.id().to_string()],
        );
    };
    let item = &m.item;
    let item_id = item.id().to_string();
    if item.is_kit() {
        return refuse(
            format!(
                "\"{}\" is a kit; give its objects one by one.",
                inventory_label(item)
            ),
            vec![item_id],
        );
    }
    let receiver = match resolve_receiver(cx.main, &chat.participants, cx.character_id, &to) {
        Ok(r) => r,
        Err(error) => return refuse(error, vec![item_id]),
    };
    let locate = |id: &str, ensure: bool| {
        resolve_inventory_location(
            cx.main,
            cx.mount,
            InventoryScope::Character,
            Some(id),
            cx.user_id,
            ResolveInventoryLocationOptions { ensure },
        )
    };
    let source = match locate(cx.character_id, false) {
        Ok(s) => s,
        Err(e) => return refuse(error_message_of(&InventoryError::Db(e)), Vec::new()),
    };
    let destination = match locate(&receiver.id, true) {
        Ok(d) => d,
        Err(e) => return refuse(error_message_of(&InventoryError::Db(e)), Vec::new()),
    };
    let record = match &source {
        Some(source) => match source.find_item(cx.mount, &item_id) {
            Ok(r) => r,
            Err(e) => return refuse(error_message_of(&InventoryError::Db(e)), Vec::new()),
        },
        None => None,
    };
    let (Some(source), Some(record)) = (source, record) else {
        return refuse(inventory_not_found_message(&reference), vec![item_id]);
    };
    let Some(destination) = destination else {
        return refuse(
            format!("{} has no vault to receive it.", receiver.name),
            vec![item_id],
        );
    };

    // 1. The move.
    let moved = match transfer_inventory_item(
        cx.main,
        cx.mount,
        TransferInventoryItemParams {
            action: InventoryTransferAction::Move,
            source: &source,
            destination: &destination,
            record: &record,
            user_id: cx.user_id,
        },
        cx.blob_webp,
    ) {
        Ok(t) => t.item,
        // v4's catch: `refuse(errorMessageOf(e, …))` with no resolved ids.
        Err(e) => return refuse(error_message_of(&e), Vec::new()),
    };
    // 2. The drop, in every chat the giver carries it (this one always).
    drop_carried_everywhere(
        cx.main,
        cx.character_id,
        &item_id,
        &[cx.chat_id.to_string()],
    );
    // 3. The give's own line (after commit).
    let change = InventoryChange {
        equipped: Vec::new(),
        unequipped: Vec::new(),
        picked_up: None,
        given: Some(InventoryGive {
            item: moved.clone(),
            to_name: receiver.name.clone(),
        }),
    };
    let log = InventoryToolLog {
        refs: Some(vec![reference.clone()]),
        resolved: Some(vec![item_id]),
        to: Some(receiver.id.clone()),
        ..InventoryToolLog::outcome("given")
    };
    InventoryHandled {
        output: InventoryGiveToolOutput {
            success: true,
            // The receiver's now: carried by no one until they equip it.
            item: Some(summarize(&moved, true, false)),
            recipient: Some(receiver),
            error: None,
        },
        changes: vec![(cx.chat_id.to_string(), change)],
        publishes: vec![
            (RealtimeTopic::Inventory, None),
            (RealtimeTopic::Chats, Some(cx.chat_id.to_string())),
        ],
        log: Some((log, InventoryToolLogLevel::Debug)),
    }
}

/// v4 `formatInventoryGiveResults`.
pub fn format(output: &InventoryGiveToolOutput) -> String {
    let (Some(item), Some(recipient)) = (
        output.item.as_ref().filter(|_| output.success),
        output.recipient.as_ref(),
    ) else {
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
        "Handed {} [{}] to {}. It is in their vault now; they may equip it.",
        display_name_of(&item.name, item.title.as_deref()),
        item.id,
        recipient.name
    )
}
