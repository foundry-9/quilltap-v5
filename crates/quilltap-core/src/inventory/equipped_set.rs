//! The equipped set — the objects a character carries in one chat, read (v4
//! `lib/inventory/equipped-set.ts`, `4fbdad186`; P4.D324).
//!
//! `chats.equippedInventory[characterId]` holds ids; this resolves them
//! against the character's pool and drops any that no longer resolve to
//! something carriable (missing, archived, an archetype or a kit), each logged
//! at `debug`. Nothing is written: a dropped id leaves the column at the next
//! equip write (`equip.rs` prunes it).

use serde_json::Value;

use super::pool::InventoryPool;
use super::resolve::is_equippable;
use super::types::{InventoryFields, ResolvedInventoryItem};
use crate::db::chats_inventory::normalize_equipped_inventory;

pub use super::resolve::kit_state;

/// The raw ids a chat stores for one character (v4 `equippedIdsOf`); `[]`
/// when it has no entry.
pub fn equipped_ids_of(equipped_inventory: Option<&Value>, character_id: &str) -> Vec<String> {
    normalize_equipped_inventory(equipped_inventory)
        .into_iter()
        .find(|(k, _)| k == character_id)
        .map(|(_, ids)| ids)
        .unwrap_or_default()
}

/// True when the chat has an entry (even an empty one) for the character (v4
/// `hasEquippedEntry`).
pub fn has_equipped_entry(equipped_inventory: Option<&Value>, character_id: &str) -> bool {
    normalize_equipped_inventory(equipped_inventory)
        .iter()
        .any(|(k, _)| k == character_id)
}

/// What a character carries in a chat, resolved; unresolvable ids dropped (v4
/// `readEquippedSet`). Each drop DEBUGs `[InventoryEquippedSet] Dropped an id
/// that no longer resolves to a carriable object` with the reason in v4's
/// precedence: missing → archived → archetype → kit.
pub fn read_equipped_set(
    chat_id: Option<&str>,
    equipped_inventory: Option<&Value>,
    character_id: &str,
    pool: &InventoryPool,
) -> Vec<ResolvedInventoryItem> {
    let mut out = Vec::new();
    for id in equipped_ids_of(equipped_inventory, character_id) {
        let item = pool.resolve_id(&id);
        match item {
            Some(item) if is_equippable(&item) => out.push(item),
            other => {
                let reason = match &other {
                    None => "missing",
                    Some(i) if i.is_archived() => "archived",
                    Some(i) if i.is_archetype() => "archetype",
                    Some(_) => "kit",
                };
                // v4 `chatId: chat.id` — `undefined` (omitted) when the chat
                // object carries none.
                tracing::debug!(
                    chatId = chat_id,
                    characterId = %character_id,
                    itemId = %id,
                    reason,
                    context = "inventory",
                    "[InventoryEquippedSet] Dropped an id that no longer resolves to a carriable object"
                );
            }
        }
    }
    out
}
