//! `wardrobe_archive` handler (v4 `lib/tools/handlers/wardrobe-archive-handler.ts`
//! `executeWardrobeArchiveTool` + `formatWardrobeArchiveResults`).
//!
//! Soft-retires the character's OWN item (`pool.owns` — shared items are
//! read-only) through the keystone's ONE `set_item_archived`. Idempotent (v4
//! `7a8d6dacc`, bug 188): an item already archived is written NOTHING — its
//! original `archivedAt` stands — and the tool answers `already_archived:
//! true` (carried to the model by the formatted text only; the executor's
//! payload projection never names it). When a FRESH archive retires an
//! equipped item, the character id comes back as an outfit change — the
//! executor fires the outfit-change effects for it (v4's
//! `notifyWardrobeChanged`) once the write commits.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::vault_wardrobe_public::WardrobeScope;
use crate::db::wardrobe_location::{resolve_wardrobe_location, ResolveWardrobeLocationOptions};
use crate::services::wardrobe_item_mutations::set_item_archived;
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe::normalize_no_item_sentinel;
use crate::wear_ops::{find_in_pool, WearableRef};

use super::wardrobe_read::{not_found_message, validate_locate, LocateInput};
use super::wardrobe_shared::find_equipped_slots;
use super::wardrobe_tool_table::{load_tool_pool, shared_wardrobe_item_read_only_message};

/// v4 `WardrobeArchiveToolOutput`.
#[derive(Debug, Serialize)]
pub struct WardrobeArchiveToolOutput {
    pub success: bool,
    pub item_id: String,
    pub title: String,
    /// Always `"archived"`.
    pub action: String,
    /// True when the item was already archived — nothing was written and its
    /// original `archivedAt` stands (v4 `7a8d6dacc`, bug 188). Absent
    /// otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub already_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: impl Into<String>) -> WardrobeArchiveToolOutput {
    WardrobeArchiveToolOutput {
        success: false,
        item_id: String::new(),
        title: String::new(),
        action: "archived".to_string(),
        already_archived: None,
        error: Some(error.into()),
    }
}

/// v4's `context` on every line the handler logs.
const LOG_CONTEXT: &str = "wardrobe-archive-handler";

/// Execute `wardrobe_archive` (v4 `executeWardrobeArchiveTool`). Returns the output
/// plus the characters whose outfit changed (non-empty only when a fresh
/// archive retired an equipped item) — the executor's `notifyWardrobeChanged`.
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> (WardrobeArchiveToolOutput, Vec<String>) {
    let Some(input) = validate_locate(args) else {
        tracing::warn!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            characterId = character_id,
            inputJson = %args,
            "Wardrobe archive tool validation failed",
        );
        return (
            failure("Invalid input: item_id or item_title is required."),
            Vec::new(),
        );
    };
    match run(main, mount, user_id, chat_id, character_id, &input) {
        Ok(pair) => pair,
        // v4's catch: a thrown writer error (an archived owner) or a failed
        // location read.
        Err(message) => {
            tracing::error!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                characterId = character_id,
                error = %message,
                "Wardrobe archive tool execution failed",
            );
            (failure(message), Vec::new())
        }
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    input: &LocateInput,
) -> Result<(WardrobeArchiveToolOutput, Vec<String>), String> {
    let pool = load_tool_pool(main, mount, chat_id, character_id);
    let item_id = normalize_no_item_sentinel(input.item_id.as_deref());
    let item_title = normalize_no_item_sentinel(input.item_title.as_deref());
    let Some(item) = find_in_pool(
        &pool,
        WearableRef {
            item_id: item_id.as_deref(),
            item_title: item_title.as_deref(),
        },
    ) else {
        return Ok((
            failure(not_found_message(&input.item_id, &input.item_title)),
            Vec::new(),
        ));
    };
    let current = WardrobeItem::from_read_value(item);
    let (id, title) = (current.id.clone(), current.title.clone());

    if !pool.owns(item) {
        return Ok((
            failure(shared_wardrobe_item_read_only_message(&title, "archived")),
            Vec::new(),
        ));
    }

    // The location comes BEFORE the equipped read (v4 `9ecebf986`).
    let location = resolve_wardrobe_location(
        main,
        mount,
        WardrobeScope::Character,
        Some(character_id),
        user_id,
        ResolveWardrobeLocationOptions::default(),
    )
    .map_err(|e| crate::db::fallback::error_text(&e))?;
    let Some(location) = location else {
        return Ok((
            failure(format!("Failed to archive wardrobe item \"{title}\"")),
            Vec::new(),
        ));
    };

    // Is the item currently equipped? (Archive doesn't clear equipped slots,
    // but an equipped-then-archived item warrants a visible refresh.)
    let equipped =
        ChatOutfitsRepository::new(main).get_equipped_outfit_for_character(chat_id, character_id);
    let was_equipped = !find_equipped_slots(&id, equipped.as_ref()).is_empty();

    let outcome =
        set_item_archived(main, mount, &location, &current, true).map_err(|e| e.message())?;
    if !outcome.changed {
        if outcome.item.is_some() {
            tracing::debug!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                chatId = chat_id,
                characterId = character_id,
                itemId = %id,
                archivedAt = %current.archived_at.clone().flatten().unwrap_or_default(),
                "Wardrobe item already archived; keeping its original date",
            );
            return Ok((
                WardrobeArchiveToolOutput {
                    success: true,
                    item_id: id,
                    title,
                    action: "archived".to_string(),
                    already_archived: Some(true),
                    error: None,
                },
                Vec::new(),
            ));
        }
        // The item vanished under the write.
        return Ok((
            failure(format!("Failed to archive wardrobe item \"{title}\"")),
            Vec::new(),
        ));
    }

    // Equipped-then-archived: the outfit changed (v4's `notifyWardrobeChanged`,
    // fired by the executor once the write commits).
    let changed = if was_equipped {
        vec![character_id.to_string()]
    } else {
        Vec::new()
    };

    tracing::info!(
        target: "quilltap::tools",
        context = LOG_CONTEXT,
        userId = user_id,
        chatId = chat_id,
        characterId = character_id,
        itemId = %id,
        itemTitle = %title,
        wasEquipped = was_equipped,
        "Wardrobe item archived",
    );

    Ok((
        WardrobeArchiveToolOutput {
            success: true,
            item_id: id,
            title,
            action: "archived".to_string(),
            already_archived: None,
            error: None,
        },
        changed,
    ))
}

/// v4 `formatWardrobeArchiveResults`.
pub fn format(output: &WardrobeArchiveToolOutput) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    if output.already_archived == Some(true) {
        return format!(
            "\"{}\" ({}) was already archived; nothing changed.",
            output.title, output.item_id
        );
    }
    format!(
        "Archived \"{}\" ({}). It's hidden from listings and can't be worn; a human can restore it.",
        output.title, output.item_id
    )
}
