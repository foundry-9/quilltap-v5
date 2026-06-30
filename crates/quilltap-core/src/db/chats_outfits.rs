//! The `chats` **equipped-outfit** ops (the conversation capstone, sub-unit 6).
//! Ports v4's outfit methods from `chats.repository.ts`
//! (`getEquippedOutfit` / `getEquippedOutfitForCharacter` / `setEquippedOutfit` /
//! `removeEquippedItemFromAllChats`).
//!
//! ## The shape
//!
//! The two reads (`get_equipped_outfit` / `get_equipped_outfit_for_character`) are
//! plain marshaled-row reads through [`chats_read::find_by_id`] — `chats` has no
//! vault overlay. The two writes are the familiar RMW: read the chat, mutate the
//! `equippedOutfit` JSON object in memory, and route the rewrite back through
//! [`ChatsRepository::update`], which PRESERVES the chat's `updatedAt` (these ops
//! never pass one — only a new message bumps a chat's `updatedAt`).
//!
//! v4 treats the `equippedOutfit` value as **raw, un-validated JSON** — neither
//! write re-parses it through `EquippedSlotsSchema`. So the port stores it the same
//! way (as a [`serde_json::Value`], never a typed struct), which is what keeps it
//! faithful to v4's exact bytes:
//!
//! - `set_equipped_outfit` is `existing ?? {}`, then `state[characterId] = slots`
//!   (the raw `slots` object the caller passed — partial / extra keys preserved
//!   verbatim, NOT materialized to the four-slot default), then write.
//! - `remove_equipped_item_from_all_chats` iterates every chat with an
//!   `equippedOutfit`, and for each character walks the closed slot set
//!   [`WARDROBE_SLOT_TYPES`] dropping the deleted `item_id` **in place** — reading
//!   `slots[slotKey] ?? []` and rewriting that key ONLY when the item was actually
//!   present (v4's `before.includes` guard). A matched slot becomes a
//!   possibly-EMPTY array (NOT null — v4's doc-comment says "set to null" but the
//!   code `.filter`s); an absent slot stays absent (partial shape preserved). The
//!   chat is written back only when something changed (the `modified?` guard).
//!
//! ## Key-order seam (the one residual divergence)
//!
//! `equippedOutfit` is `EquippedOutfitState = Record<characterId, EquippedSlots>`,
//! persisted through `ChatUpdate.equipped_outfit: Value` →
//! `serde_json::to_string(Value)`. This workspace has **no** `serde_json`
//! `preserve_order` feature, so EVERY `Value::Object` (inner slots AND the outer
//! characterId map) serializes with **alphabetically sorted** keys, while v4 emits
//! both in JS **insertion** order. They coincide only when insertion order is
//! already sorted. The differential is kept byte-exact AND honest by constraining
//! the corpus to sorted key order (inner slot keys `accessories,bottom,footwear,top`
//! when more than one is present; outer characterId keys seeded/appended sorted) —
//! the SAME `Value`-sorts-keys / open-JSON-insertion-order seam the project already
//! tracks for `parameters` / `sillyTavernData` / etc. Close it (a `preserve_order`
//! `serde_json` or an order-preserving column writer) before writing real multi-key
//! equipped-outfit data out of sorted order through these ops. Single-key slot
//! objects (the partial-shape cases) are order-unambiguous, so they verify the
//! partial-preserving behavior with no seam exposure.

use rusqlite::Connection;
use serde_json::Value;

use super::chats::{ChatUpdate, ChatsRepository};
use super::{chats_read, DbError};

/// The closed slot set v4 walks when removing an item (`WARDROBE_SLOT_TYPES`).
const WARDROBE_SLOT_TYPES: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

/// Drop `item_id` from a single character's slots object, mutating it in place.
/// Mirrors v4 exactly: for each slot key, read `slots[slotKey] ?? []` and rewrite
/// that key ONLY when the item was present (so an absent slot stays absent — the
/// partial-shape-preserving `before.includes` guard). A matched slot is filtered
/// to a possibly-empty array. Returns whether any slot changed.
fn remove_item_from_slots(slots: &mut serde_json::Map<String, Value>, item_id: &str) -> bool {
    let mut changed = false;
    for slot_key in WARDROBE_SLOT_TYPES {
        let Some(before) = slots.get(slot_key).and_then(Value::as_array) else {
            continue; // absent (or non-array) → v4's `?? []`, never matches.
        };
        if before.iter().any(|v| v.as_str() == Some(item_id)) {
            let filtered: Vec<Value> = before
                .iter()
                .filter(|v| v.as_str() != Some(item_id))
                .cloned()
                .collect();
            slots.insert(slot_key.to_string(), Value::Array(filtered));
            changed = true;
        }
    }
    changed
}

/// Repository over a borrowed MAIN-db connection (held by the [`super::Writer`]).
pub struct ChatOutfitsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatOutfitsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `getEquippedOutfit` — the full `EquippedOutfitState` for a chat, or `None`
    /// when the chat is absent OR has no `equippedOutfit` (v4: `!chat ||
    /// !chat.equippedOutfit`). The marshaler omits a NULL `equippedOutfit` cell, so
    /// "absent key" is the no-outfit case.
    pub fn get_equipped_outfit(&self, chat_id: &str) -> Result<Option<Value>, DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(None);
        };
        match chat.get("equippedOutfit") {
            Some(v) if !v.is_null() => Ok(Some(v.clone())),
            _ => Ok(None),
        }
    }

    /// `getEquippedOutfitForCharacter` — the slots object for one character in a
    /// chat (`state?.[characterId] ?? null`).
    pub fn get_equipped_outfit_for_character(
        &self,
        chat_id: &str,
        character_id: &str,
    ) -> Result<Option<Value>, DbError> {
        let Some(state) = self.get_equipped_outfit(chat_id)? else {
            return Ok(None);
        };
        match state.get(character_id) {
            Some(v) if !v.is_null() => Ok(Some(v.clone())),
            _ => Ok(None),
        }
    }

    /// `setEquippedOutfit` — `existing ?? {}`, then `state[characterId] = slots`,
    /// then write through `update` (preserving the chat's `updatedAt`). Returns
    /// `Ok(true)` when the chat existed (the write ran), `Ok(false)` when absent.
    ///
    /// `slots` is stored VERBATIM (v4 does NOT re-validate it through Zod here), so
    /// a partial or extra-key slots object is preserved as-passed; on-disk key order
    /// follows the writer's `Value` sort (see the module-header seam note).
    pub fn set_equipped_outfit(
        &self,
        chat_id: &str,
        character_id: &str,
        slots: &Value,
    ) -> Result<bool, DbError> {
        // The chat must exist for the write to land (v4's `update` is a no-op on a
        // missing row; mirror that by short-circuiting to false).
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(false);
        };

        // existing ?? {}  — the marshaler omits a NULL cell, so absent = {}.
        let mut state = match chat.get("equippedOutfit") {
            Some(Value::Object(m)) => m.clone(),
            _ => serde_json::Map::new(),
        };

        // state[characterId] = slots (stored raw, v4-faithful).
        state.insert(character_id.to_string(), slots.clone());

        let update = ChatUpdate {
            equipped_outfit: Some(Some(Value::Object(state))),
            ..Default::default()
        };
        ChatsRepository::new(self.conn).update(chat_id, &update)
    }

    /// `removeEquippedItemFromAllChats` — across every chat with an
    /// `equippedOutfit`, drop `item_id` from each character's slots in place (the
    /// v4 per-slot `.filter`), writing a chat back only when it actually changed.
    /// Returns the number of chats modified (v4's `modifiedCount`).
    pub fn remove_equipped_item_from_all_chats(&self, item_id: &str) -> Result<usize, DbError> {
        let chats = chats_read::find_all(self.conn)?;
        let mut modified_count = 0usize;

        for chat in &chats {
            let Some(Value::Object(state_obj)) = chat.get("equippedOutfit") else {
                continue;
            };
            let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
                continue;
            };

            // Mutate each character's slots in place (preserving partial shape);
            // track whether the chat changed.
            let mut state = state_obj.clone();
            let mut chat_modified = false;
            for (_character_id, slots_val) in state.iter_mut() {
                let Some(slots) = slots_val.as_object_mut() else {
                    continue;
                };
                if remove_item_from_slots(slots, item_id) {
                    chat_modified = true;
                }
            }

            if chat_modified {
                let update = ChatUpdate {
                    equipped_outfit: Some(Some(Value::Object(state))),
                    ..Default::default()
                };
                ChatsRepository::new(self.conn).update(chat_id, &update)?;
                modified_count += 1;
            }
        }

        Ok(modified_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn remove_item_filters_present_slots_and_preserves_partial_shape() {
        // A PARTIAL slots object — only `top` + `bottom` present. Removing an item
        // in `top` filters `top` to an empty-or-shorter array and leaves the absent
        // `footwear`/`accessories` ABSENT (v4's `before.includes` guard never
        // materializes a slot the item was not in).
        let mut slots = json!({ "top": ["x", "item", "y"], "bottom": ["item"] })
            .as_object()
            .unwrap()
            .clone();
        assert!(remove_item_from_slots(&mut slots, "item"));
        assert_eq!(slots["top"], json!(["x", "y"]));
        assert_eq!(slots["bottom"], json!([])); // filtered to empty, NOT null
        assert!(!slots.contains_key("footwear")); // absent stays absent
        assert!(!slots.contains_key("accessories"));
        // A second pass finds nothing → no change.
        assert!(!remove_item_from_slots(&mut slots, "item"));
    }

    #[test]
    fn remove_item_absent_is_noop() {
        let mut slots = json!({ "top": ["a"], "bottom": [], "footwear": [], "accessories": [] })
            .as_object()
            .unwrap()
            .clone();
        assert!(!remove_item_from_slots(&mut slots, "missing"));
        // Unchanged.
        assert_eq!(slots["top"], json!(["a"]));
    }
}
