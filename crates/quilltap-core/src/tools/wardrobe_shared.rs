//! DB-touching wardrobe helpers the seven wardrobe tool handlers compose (v4
//! `lib/tools/handlers/wardrobe-handler-shared.ts` + the persisted variants of
//! `lib/wardrobe/outfit-displacement.ts` + `lib/wardrobe/resolve-equipped.ts` +
//! `lib/mount-index/tiered-mount-pool.ts`'s `resolveProjectMountPointIdsForChat`).
//!
//! The pure leaves (union/describe/expand/effect/sentinel) live in
//! [`crate::wardrobe`]; this module holds the parts that read/write the database:
//! item resolution across tiers, the equipped-outfit displacement primitives (over
//! [`ChatOutfitsRepository`]), and the coverage-summary composition (over the
//! seeded read overlay).
//!
//! Every function takes raw connections — `main: &Connection` (the `chats` +
//! `characters` reads/writes) and a [`DocMountDocumentsRepository`] over the
//! mount-index connection (the vault reads) — so the handlers run inside a single
//! writer closure that holds both (the `wardrobe_transfers` precedent), or, in the
//! differential, against two directly-opened `Writer`s.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::chats_read;
use crate::db::project_doc_mount_links::ProjectDocMountLinksRepository;
use crate::db::DbError;
use crate::wardrobe::{Slots, WARDROBE_SLOT_TYPES};

/// v4 `resolveProjectMountPointIds(projectId)` — the project tier for a project
/// id the caller already holds: its linked document stores. `[]` for a
/// project-less caller or a failed lookup. `mount` reads
/// `project_doc_mount_links`.
///
/// P4.149 (P4.D245 item 14's second half): the REACHABLE v4 line on a failed
/// read is the base `findByFilter`'s `Error finding entities by filter
/// {collection: project_doc_mount_links}` — `findByProjectId`'s own `safeQuery`
/// (`Error finding links by project ID`) wraps that fallback and can never
/// fire, and so can never the helper's `Project mount lookup failed` catch
/// (measured through `tiered_mount_pool_equivalence`). No new home literal.
///
/// The sibling of [`resolve_project_mount_point_ids_for_chat`], which is this
/// function plus the chat→project lookup. Chat-start outfit resolution reaches
/// for this one: the create / add-participant / merge paths all hold the project
/// id already.
pub fn resolve_project_mount_point_ids(
    mount: &Connection,
    project_id: Option<&str>,
) -> Vec<String> {
    let Some(project_id) = project_id.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    crate::db::fallback::find_by_filter_or_empty("project_doc_mount_links", || {
        ProjectDocMountLinksRepository::new(mount).find_by_project_id(project_id)
    })
}

/// v4 `resolveProjectMountPointIdsForChat(chatId)` — the project tier for a chat:
/// load the chat, then its project's linked stores. `[]` for a project-less chat
/// or a failed lookup. `main` reads `chats`, `mount` reads
/// `project_doc_mount_links`.
///
/// P4.149 (P4.D245 item 18): v4's `repos.chats.findById` is the fallback
/// `_findById` — a failed read logs `Error finding entity by ID {collection:
/// chats, id}` and answers `null` → `[]`; the helper's own `Project mount
/// lookup for chat failed` catch is unreachable. v5 had swallowed the `Err`
/// silently.
pub fn resolve_project_mount_point_ids_for_chat(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
) -> Vec<String> {
    let Some(chat) = crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        chats_read::find_by_id(main, chat_id)
    }) else {
        return Vec::new();
    };
    resolve_project_mount_point_ids(mount, chat.get("projectId").and_then(Value::as_str))
}

/// Every slot an item is equipped in (v4 `findEquippedSlots`, in the list/read
/// handlers): walk [`WARDROBE_SLOT_TYPES`] collecting the slots whose id array
/// contains `item_id`.
pub fn find_equipped_slots(item_id: &str, equipped: Option<&Value>) -> Vec<String> {
    let Some(eq) = equipped else {
        return Vec::new();
    };
    let mut slots = Vec::new();
    for slot in WARDROBE_SLOT_TYPES {
        let present = eq
            .get(slot)
            .and_then(Value::as_array)
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(item_id)));
        if present {
            slots.push(slot.to_string());
        }
    }
    slots
}

/// v4 `formatEquippedSlotLines(state)` (`wardrobe-handler-shared.ts:120-130`,
/// "so the slot presentation … can never drift") — `  <slot>: <ids>` per slot
/// in `WARDROBE_SLOT_TYPES` order, `(empty)` for a slot with no ids, and an
/// empty unreported-if-blank slot (hair) omitted entirely: the model must
/// never read that as baldness. The emptiness test is v4's `ids.length === 0`
/// over the id LIST, not the joined text.
pub fn format_equipped_slot_lines(state: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for slot in crate::wardrobe::WARDROBE_SLOT_TYPES {
        let ids: Vec<&str> = state
            .get(slot)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if ids.is_empty() && !crate::wardrobe::is_slot_reported_when_empty(slot) {
            continue;
        }
        let label = if ids.is_empty() {
            "(empty)".to_string()
        } else {
            ids.join(", ")
        };
        lines.push(format!("  {slot}: {label}"));
    }
    lines
}

/// v4 `buildWardrobeCoverageSummaryFromState(pool, slots)` =
/// `describeOutfit(resolveEquippedOutfitForCharacter(pool, slots).outfitValues)`
/// — pure over the pool the tool already loaded (no second read).
pub fn coverage_summary_from_state(
    pool: &crate::wearable_pool::WearablePool,
    state: &Slots,
) -> String {
    crate::wardrobe::describe_outfit(
        &crate::services::resolve_equipped::resolve_equipped_outfit_for_character(pool, state)
            .outfit_values,
    )
}

/// Zod 4's `z.number().int()` over a tool input value: the integer it carries,
/// or `None` for anything `.int()` refuses — a non-number (a string `"2"`
/// included), a fractional or non-finite value, or one outside the SAFE
/// integer range (Zod 4's `.int()` is `safeint`). An integral float (`2.0`)
/// is admitted, as `Number.isInteger(2.0)` is true.
pub fn zod_safe_integer(value: &Value) -> Option<i64> {
    let n = value.as_number()?;
    let max = crate::api::zod_issues::MAX_SAFE_INTEGER_I64;
    if let Some(i) = n.as_i64() {
        return (-max..=max).contains(&i).then_some(i);
    }
    let f = n.as_f64()?;
    (f.is_finite() && f.fract() == 0.0 && f.abs() <= crate::api::zod_issues::MAX_SAFE_INTEGER)
        .then_some(f as i64)
}

/// A fresh empty equipped state (v4 `emptyEquippedState`).
pub fn empty_equipped_state() -> Slots {
    Slots::fresh()
}

/// v4 `loadCurrentWardrobeState` — the character's equipped slots for a chat, or
/// an empty state when none is stored.
pub fn load_current_wardrobe_state(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
) -> Result<Slots, DbError> {
    let repo = ChatOutfitsRepository::new(main);
    let stored = repo.get_equipped_outfit_for_character(chat_id, character_id);
    Ok(Slots::from_value(stored.as_ref()))
}

// ── The persisted equip primitives (v4 `outfit-displacement.ts`) ─────────────
// Each loads the character's slots, mutates them, and commits them through the
// wear ledger's chokepoint ([`commit_equipped_outfit`], v4 `3ee3b1342`), which
// writes them via `ChatOutfitsRepository::set_equipped_outfit` (RMW through
// `chats.update`, which preserves the chat's `updatedAt`) and credits the
// garments newly put on. Each returns the slots it computed (v4's `commit`
// returns `nextSlots`, not the chokepoint's echo); a lost write is an `Err`
// carrying v4's `Failed to save the equipped outfit …` sentence.

// ── v4 `9ecebf986`: `wornBundlesFor` and the four persisted primitives moved ──
// `wornBundlesFor` now lives in `slot-ops.ts` ([`crate::slot_ops`]); the four
// primitives (`equipItem` / `replaceItem` / `addToSlot` / `removeFromSlot`)
// folded into ONE `applyDisplacement(repos, chatId, characterId, {mode, item,
// slot, itemId, itemsById}, source)` ([`apply_displacement`]) — load, the
// one mode → primitive table, the chokepoint, and the `[applyDisplacement]`
// log lines. The names below are shims with the OLD signatures for the tool
// handlers and the route until the re-aim lanes reach them; each builds its
// `items_by_id` exactly as before ([`lookup_for_bundle`]) and a
// `WearableNode` carrying the `replace` flag.

// ── resolve-equipped.ts — resolveEquippedOutfitForCharacter ──────────────────

/// The raw leaf items routed per output slot (v4 `resolveEquippedOutfitForCharacter`'s
/// `leafItemsBySlot`). Each slot holds the resolved wardrobe item `Value`s (with
/// `title` / `imagePrompt` / `types`) so callers can decorate them variously
/// (the coverage summary → titles; the live-clothing override → `titleOnly`).
#[derive(Debug, Clone, Default)]
pub struct LeafItemsBySlot {
    pub top: Vec<Value>,
    pub bottom: Vec<Value>,
    pub footwear: Vec<Value>,
    pub accessories: Vec<Value>,
    pub hair: Vec<Value>,
}

impl LeafItemsBySlot {
    /// Read a slot's leaf items by canonical name (unknown = `top`, unreachable
    /// — callers only walk [`WARDROBE_SLOT_TYPES`]).
    pub fn slot(&self, name: &str) -> &[Value] {
        match name {
            "top" => &self.top,
            "bottom" => &self.bottom,
            "footwear" => &self.footwear,
            "accessories" => &self.accessories,
            "hair" => &self.hair,
            _ => &self.top,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn find_equipped_slots_walks_all_slots() {
        let eq = json!({ "top": ["a", "b"], "bottom": [], "footwear": ["a"], "accessories": [] });
        assert_eq!(find_equipped_slots("a", Some(&eq)), vec!["top", "footwear"]);
        assert_eq!(find_equipped_slots("b", Some(&eq)), vec!["top"]);
        assert!(find_equipped_slots("z", Some(&eq)).is_empty());
        assert!(find_equipped_slots("a", None).is_empty());
    }
}
