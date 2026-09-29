//! The `chats` **equipped-outfit** ops (the conversation capstone, sub-unit 6).
//! Ports v4's outfit methods from `chats.repository.ts`
//! (`getEquippedOutfit` / `getEquippedOutfitForCharacter` / `setEquippedOutfit` /
//! `removeEquippedItemFromAllChats`).
//!
//! ## The shape
//!
//! The two reads (`get_equipped_outfit` / `get_equipped_outfit_for_character`) are
//! marshaled-row reads through [`chats_read::find_by_id`] — `chats` has no vault
//! overlay — passed through the slot normalizer on the way out (below). The two
//! writes are the familiar RMW: read the chat, mutate the `equippedOutfit` JSON
//! object in memory, and route the rewrite back through
//! [`ChatsRepository::update`], which PRESERVES the chat's `updatedAt` (these ops
//! never pass one — only a new message bumps a chat's `updatedAt`).
//!
//! ## Normalization on the way OUT (bug 78, v4 `275cd7bc`)
//!
//! The column is unconstrained JSON and slots were added over time, so a chat row
//! written before the hair slot existed carries four keys. v4 used to hand that
//! bag to its readers raw — which crashed avatar generation on
//! `slots['hair'] === undefined` — and now maps every character's entry through
//! `normalizeEquippedSlots` inside `getEquippedOutfit`. This port does the same
//! with [`normalize_equipped_outfit_state`]. It is a READ-side repair: the value
//! still goes to disk exactly as its writer shaped it (below), and the repair
//! reaches disk only where a write re-reads the state first.
//!
//! v4 otherwise treats the `equippedOutfit` value as **raw, un-validated JSON** —
//! neither write re-parses the incoming slots through `EquippedSlotsSchema`. So
//! the port stores it the same way (as a [`serde_json::Value`], never a typed
//! struct), which is what keeps it faithful to v4's exact bytes:
//!
//! - `set_equipped_outfit` is `existing ?? {}`, then `state[characterId] = slots`
//!   (the raw `slots` object the caller passed — partial / extra keys preserved
//!   verbatim, NOT materialized to the default), then write. `existing` comes
//!   through `get_equipped_outfit`, so every OTHER character's bag in the chat
//!   is repaired to all five slots on the way (see the normalization note).
//! - `remove_equipped_item_from_all_chats` iterates every chat with an
//!   `equippedOutfit`, and for each character walks the closed slot set
//!   [`WARDROBE_SLOT_TYPES`] dropping the deleted `item_id` **in place** — reading
//!   `slots[slotKey] ?? []` and rewriting that key ONLY when the item was actually
//!   present (v4's `before.includes` guard). A matched slot becomes a
//!   possibly-EMPTY array (NOT null — v4's doc-comment says "set to null" but the
//!   code `.filter`s); an absent slot stays absent (partial shape preserved). The
//!   chat is written back only when something changed (the `modified?` guard).
//!
//! ## Key order (seam CLOSED — `preserve_order` on)
//!
//! `equippedOutfit` is `EquippedOutfitState = Record<characterId, EquippedSlots>`,
//! persisted through `ChatUpdate.equipped_outfit: Value` →
//! `serde_json::to_string(Value)`. With `serde_json`'s **`preserve_order`** feature
//! enabled workspace-wide, `Value::Object` is an `IndexMap` that serializes in
//! INSERTION order — matching v4's `JSON.stringify` for both the inner slots and
//! the outer characterId map. The corpus proves it: a key-order chat appends a
//! higher-sorting characterId before a lower one, so the stored outer map is in
//! insertion (not sorted) order, byte-identical to the oracle.

use rusqlite::Connection;
use serde_json::Value;

use super::chats::{ChatUpdate, ChatsRepository};
use super::{chats_read, DbError};
use crate::wardrobe::normalize_equipped_outfit_state;

/// The closed slot set v4 walks when removing an item — read from the ONE
/// registry (`4423ad10`'s consolidation).
const WARDROBE_SLOT_TYPES: [&str; 5] = crate::wardrobe::WARDROBE_SLOT_TYPES;

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
    ///
    /// **A failed chat read answers `None`, never an error** — v4's method is a
    /// fallback `safeQuery` whose only fallible step is `this.findById`, itself
    /// the fallback `_findById` (`base.repository.ts:247-257`): a DB failure
    /// logs `Error finding entity by ID` and yields `null`, so `getEquippedOutfit`
    /// answers `null` with no line of its own (its `Failed to get equipped
    /// outfit` arm can only see a throw, and nothing left inside it throws).
    /// Every v4 caller — the wardrobe tools, the outfit routes, the arriving-
    /// character refresh — therefore reads "no outfit" on a broken read, which
    /// is what the `Option` (not `Result`) return type says (the `97b25fc53`
    /// unification review; P4.D238 had pinned the unreachable inner line).
    ///
    /// **The coercion point** (v4 `275cd7bc`, bug 78): the stored value is raw
    /// JSON, and a row written before a slot existed carries fewer keys than the
    /// slot list declares. Every character's entry is normalized on the way out
    /// ([`normalize_equipped_outfit_state`]), so no consumer — this repository's
    /// own `setEquippedOutfit` included — ever sees a short bag.
    pub fn get_equipped_outfit(&self, chat_id: &str) -> Option<Value> {
        let chat = chats_read::find_by_id_or_none(self.conn, chat_id)?;
        match chat.get("equippedOutfit") {
            Some(v) if !v.is_null() => Some(normalize_equipped_outfit_state(v)),
            _ => None,
        }
    }

    /// `getEquippedOutfitForCharacter` — the slots object for one character in a
    /// chat (`state?.[characterId] ?? null`).
    pub fn get_equipped_outfit_for_character(
        &self,
        chat_id: &str,
        character_id: &str,
    ) -> Option<Value> {
        let state = self.get_equipped_outfit(chat_id)?;
        match state.get(character_id) {
            Some(v) if !v.is_null() => Some(v.clone()),
            _ => None,
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

        // existing ?? {} — v4 reads it through `getEquippedOutfit`, so every
        // OTHER character's bag in this chat is normalized on the way in and
        // written back repaired (`275cd7bc`). Only the character being set
        // keeps the caller's raw `slots` object.
        let mut state = match chat.get("equippedOutfit") {
            Some(v) if !v.is_null() => match normalize_equipped_outfit_state(v) {
                Value::Object(m) => m,
                _ => serde_json::Map::new(),
            },
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

    /// v4's `getEquippedOutfit` is a fallback `safeQuery` whose only fallible
    /// step is the fallback `_findById`: a broken chat read logs `Error finding
    /// entity by ID` `{collection, id, error}` and the read answers "no outfit"
    /// — no `Failed to get equipped outfit` line, no error (the `97b25fc53`
    /// unification review).
    #[test]
    fn a_failed_chat_read_answers_no_outfit_with_v4s_repository_error_line() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // No `chats` table at all: every read fails.
        let repo = ChatOutfitsRepository::new(&conn);
        let (whole, lines) =
            crate::test_support::captured_with(|| repo.get_equipped_outfit("chat-1"));
        assert_eq!(whole, None);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        let l = &lines[0];
        assert!(l.starts_with("ERROR "), "{l}");
        assert!(l.contains("Error finding entity by ID"), "{l}");
        assert!(l.contains("collection=chats"), "{l}");
        assert!(l.contains("id=chat-1"), "{l}");
        assert!(!l.contains("context="), "{l}");
        assert!(!l.contains("Failed to get equipped outfit"), "{l}");
        let (one, lines) = crate::test_support::captured_with(|| {
            repo.get_equipped_outfit_for_character("chat-1", "char-1")
        });
        assert_eq!(one, None);
        assert_eq!(lines.len(), 1, "{lines:#?}");
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
