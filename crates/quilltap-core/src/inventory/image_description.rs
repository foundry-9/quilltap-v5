//! The Inventory's image phrase — what a character carries, as rendering
//! hints for the image pipelines (v4 `lib/inventory/image-description.ts`,
//! `4fbdad186` at `322a5488e`; P4.D326).
//!
//! [`describe_equipped_inventory_for_images`] joins the equipped objects'
//! `imageDescription`s into one noun-phrase list ("carrying a longsword with a
//! jewelled pommel, a brass signet ring on the left hand"). An object with no
//! `imageDescription` is not depicted; a kit never is (its members are). The
//! phrase reaches the pictures through the appearance resolver
//! (`resolve_character_appearances`, the Lantern and the image tool) and the
//! avatar job, which hand it to the prompt builders as text — no prompt
//! builder reads the Inventory itself.
//!
//! The renderer is pure; the loader reads (never writes) and never fails.

use rusqlite::Connection;
use serde_json::Value;

use super::equipped_set::{equipped_ids_of, read_equipped_set};
use super::pool::{load_inventory_pool, InventoryPool, LoadInventoryPoolOptions};
use super::resolve::inventory_label;
use super::types::{InventoryFields, InventoryKind, ResolvedInventoryItem};
use crate::jsstr::js_trim;

/// Render the phrase from resolved objects, or `None` when none is depicted
/// (v4 `renderCarryingPhrase`). Pure.
///
/// Kits and objects whose `imageDescription` is empty after trimming drop
/// out; the rest sort by v4's STABLE `localeCompare(…, { sensitivity: 'base'
/// })` on the label (ties keep the equipped-set order); each description is
/// trimmed and loses a trailing run of `.`, `!` and `?` (v4's
/// `replace(/[.!?]+$/, '')` — so a description that is ONLY punctuation
/// becomes an empty element, v4's bytes).
pub fn render_carrying_phrase(items: &[ResolvedInventoryItem]) -> Option<String> {
    let mut depicted: Vec<&ResolvedInventoryItem> = items
        .iter()
        .filter(|item| {
            item.kind() != InventoryKind::Kit
                && item
                    .image_description()
                    .is_some_and(|d| !js_trim(d).is_empty())
        })
        .collect();
    if depicted.is_empty() {
        return None;
    }
    depicted.sort_by(|a, b| {
        crate::collation::locale_compare_base(&inventory_label(*a), &inventory_label(*b))
    });
    let phrases: Vec<&str> = depicted
        .iter()
        .map(|item| {
            js_trim(item.image_description().unwrap_or("")).trim_end_matches(['.', '!', '?'])
        })
        .collect();
    Some(format!("carrying {}", phrases.join(", ")))
}

/// The phrase for one character in a chat, against a pool already loaded (v4
/// `describeEquippedInventoryForImages`). `chat` is the chat row (`id`,
/// `equippedInventory`).
pub fn describe_equipped_inventory_for_images(
    chat: &Value,
    character_id: &str,
    pool: &InventoryPool,
) -> Option<String> {
    render_carrying_phrase(&read_equipped_set(
        chat.get("id").and_then(Value::as_str),
        chat.get("equippedInventory"),
        character_id,
        pool,
    ))
}

/// Load the pool and build the phrase for one character in a chat; `None`
/// when nothing carried is depicted (v4 `loadCarryingPhrase`). Never fails:
/// nothing stored → no pool read at all; a failed read WARNs and the picture
/// goes without it. `chat` is the `chats_read::find_by_id` row (`id`,
/// `projectId`, `equippedInventory`).
pub fn load_carrying_phrase(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
    character_id: &str,
) -> Option<String> {
    let equipped = chat.get("equippedInventory");
    if equipped_ids_of(equipped, character_id).is_empty() {
        return None;
    }
    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    let project_id = chat.get("projectId").and_then(Value::as_str);
    match load_inventory_pool(
        main,
        mount,
        LoadInventoryPoolOptions {
            character_id,
            chat_id: Some(chat_id),
            project_id,
            own_records: None,
        },
    ) {
        Ok(pool) => {
            let phrase = describe_equipped_inventory_for_images(chat, character_id, &pool);
            tracing::debug!(
                chatId = %chat_id,
                characterId = %character_id,
                present = phrase.is_some(),
                op = "image-phrase",
                context = "inventory",
                "[InventoryImages] Built the carrying phrase"
            );
            phrase
        }
        Err(error) => {
            // v4's catch is reachable only through a THROWING pool load, which
            // v4's real code never does (every tier read fails soft); v5's
            // home is the pool's `Err` from a failed main read.
            tracing::warn!(
                chatId = %chat_id,
                characterId = %character_id,
                context = "inventory",
                error = %crate::db::fallback::error_text(&error),
                "[InventoryImages] Failed to build the carrying phrase; picture goes without it"
            );
            None
        }
    }
}
