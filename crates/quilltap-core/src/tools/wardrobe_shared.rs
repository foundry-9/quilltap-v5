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
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::project_doc_mount_links::ProjectDocMountLinksRepository;
use crate::db::vault_wardrobe_public::{WardrobePublicError, NO_MOUNT_MESSAGE};
use crate::db::wardrobe_read::{find_by_character_id, find_by_ids_for_character};
use crate::db::wardrobe_wear_stats::{EquipSource, WornBundle};
use crate::db::DbError;
use crate::dissolve_bundles::{WearableLookup, WearableNode};
use crate::services::outfit_displacement::apply_displacement;
use crate::slot_ops::{ComputeDisplacedOptions, DisplacementMode};
use crate::wardrobe::{
    describe_outfit, expand_composites, OutfitSlotValues, Slots, WARDROBE_SLOT_TYPES,
};
use crate::wardrobe_tiers::SharedWardrobeTiers;

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

// LANE-REMOVE(P4.D280): v4 deleted resolveWardrobeItemAcrossTiers (wardrobe-handler-shared.ts) in 9ecebf986
/// The pre-`9ecebf986` cross-tier resolve — now `wear_ops::find_in_pool` over
/// a loaded pool: by id (archived included), then by title (own first incl.
/// archived, then the wearable shared set). The `origin` tag is stripped so
/// the pre-pool callers see the shape they always did. Callers move to
/// `load_wearable_pool` + `find_in_pool`.
pub fn resolve_wardrobe_item_across_tiers(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    item_id: Option<&str>,
    item_title: Option<&str>,
    tiers: &SharedWardrobeTiers,
) -> Result<Option<Value>, DbError> {
    // The pre-pool resolver PROPAGATED a failed own-vault read (v4's
    // `findByCharacterId` rethrows); the shim keeps that contract for its
    // remaining callers by reading the own items itself.
    let own = find_by_character_id(main, docs, character_id, true)?;
    let pool = crate::wearable_pool::load_wearable_pool(
        main,
        docs.conn(),
        character_id,
        Some(&tiers.project_mount_point_ids),
        crate::wearable_pool::LoadWearablePoolOptions {
            own_items: Some(&own),
            ..Default::default()
        },
    );
    let found = item_id
        .filter(|id| !id.is_empty())
        .and_then(|id| pool.get(id))
        .or_else(|| {
            item_title
                .filter(|t| !t.is_empty())
                .and_then(|t| pool.find_by_title(t))
        });
    Ok(found.cloned().map(|mut item| {
        if let Some(obj) = item.as_object_mut() {
            obj.remove("origin");
        }
        item
    }))
}

/// Render a [`WardrobePublicError`] to v4's thrown-error message (the handlers'
/// outer try/catch maps a thrown mount error to a failure result). `verb` is
/// `create`/`update`/`delete` — the only difference between the three no-mount
/// messages. (`NoMount` is unreachable for an own-item mutation in a vaulted
/// character; ported for fidelity.)
pub fn public_error_message(e: WardrobePublicError, verb: &str) -> String {
    match e {
        WardrobePublicError::NoMount => {
            format!("Cannot {verb} wardrobe item: {NO_MOUNT_MESSAGE}")
        }
        WardrobePublicError::Cycle(m) => m,
        WardrobePublicError::Db(d) => d.to_string(),
    }
}

/// v4 `isOwnWardrobeItem` — the item belongs to THIS character (editable), vs a
/// shared archetype (`characterId === null`) or someone else's.
pub fn is_own_wardrobe_item(item: &Value, character_id: &str) -> bool {
    item.get("characterId").and_then(Value::as_str) == Some(character_id)
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

// LANE-REMOVE(P4.D280|P4.D282|P4.D283): v4 deleted hydrate-components.ts (hydrateComponentGraph) in 9ecebf986
/// The pre-`9ecebf986` level-at-a-time hydration — now ONE walk over the
/// pool's `byId` (`wearable_pool::component_graph`): every component reachable
/// from the items already in `items_by_id` is filled in from the pool, in
/// place. Callers move to `component_graph` / `pool.by_id()`.
pub(crate) fn hydrate_component_graph(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    items_by_id: &mut WearableLookup,
    tiers: &SharedWardrobeTiers,
) {
    let pool = crate::wearable_pool::load_wearable_pool(
        main,
        docs.conn(),
        character_id,
        Some(&tiers.project_mount_point_ids),
        crate::wearable_pool::LoadWearablePoolOptions::default(),
    );
    let roots: Vec<String> = items_by_id
        .values()
        .flat_map(|item| {
            item.get("componentItemIds")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect();
    for (id, item) in crate::wearable_pool::component_graph_lookup(&pool, &roots) {
        items_by_id.entry(id).or_insert(item);
    }
}

// LANE-REMOVE(P4.D280|P4.D282): v4 deleted lookupForBundle / loadBundleLookup in 9ecebf986
/// The pre-`9ecebf986` bundle lookup — now `component_graph` over the pool
/// from the composite's components. `None` ("wear this item as its own id")
/// when the item isn't a bundle or nothing resolves. Callers move to
/// `pool.by_id()` (the whole pool is the lookup).
pub(crate) fn lookup_for_bundle(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    component_item_ids: &[String],
    tiers: &SharedWardrobeTiers,
) -> Option<WearableLookup> {
    if component_item_ids.is_empty() {
        return None;
    }
    let pool = crate::wearable_pool::load_wearable_pool(
        main,
        docs.conn(),
        character_id,
        Some(&tiers.project_mount_point_ids),
        crate::wearable_pool::LoadWearablePoolOptions::default(),
    );
    let items_by_id = crate::wearable_pool::component_graph_lookup(&pool, component_item_ids);
    if items_by_id.is_empty() {
        return None;
    }
    Some(items_by_id)
}

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

// LANE-REMOVE(P4.D280): v4 moved wornBundlesFor to slot-ops.ts in 9ecebf986
/// Old `worn_bundles_for` → [`crate::slot_ops::worn_bundles_for`].
pub fn worn_bundles_for(
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
    only_slot: Option<&str>,
) -> Vec<WornBundle> {
    crate::slot_ops::worn_bundles_for(item, items_by_id, only_slot)
}

// LANE-REMOVE(P4.D280|P4.D282): v4 folded the four displacement primitives into applyDisplacement in 9ecebf986
/// Old `equip_item` → [`apply_displacement`] in `wear` mode (the item's
/// `replace` flag governs).
#[allow(clippy::too_many_arguments)]
pub fn equip_item(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    chat_id: &str,
    character_id: &str,
    id: &str,
    types: &[String],
    component_item_ids: &[String],
    replace: bool,
    tiers: &SharedWardrobeTiers,
    source: EquipSource,
) -> Result<Slots, DbError> {
    let items_by_id = lookup_for_bundle(main, docs, character_id, component_item_ids, tiers);
    let node = WearableNode::new(id, types, component_item_ids).with_replace(replace);
    apply_displacement(
        main,
        chat_id,
        character_id,
        &ComputeDisplacedOptions {
            mode: DisplacementMode::Wear,
            item: Some(&node),
            slot: None,
            item_id: None,
            items_by_id: items_by_id.as_ref(),
        },
        source,
    )
}

// LANE-REMOVE(P4.D280|P4.D282): v4 folded the four displacement primitives into applyDisplacement in 9ecebf986
/// Old `replace_item` → [`apply_displacement`] in `replace` mode.
#[allow(clippy::too_many_arguments)]
pub fn replace_item(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    chat_id: &str,
    character_id: &str,
    id: &str,
    types: &[String],
    component_item_ids: &[String],
    tiers: &SharedWardrobeTiers,
    source: EquipSource,
) -> Result<Slots, DbError> {
    let items_by_id = lookup_for_bundle(main, docs, character_id, component_item_ids, tiers);
    let node = WearableNode::new(id, types, component_item_ids);
    apply_displacement(
        main,
        chat_id,
        character_id,
        &ComputeDisplacedOptions {
            mode: DisplacementMode::Replace,
            item: Some(&node),
            slot: None,
            item_id: None,
            items_by_id: items_by_id.as_ref(),
        },
        source,
    )
}

// LANE-REMOVE(P4.D280|P4.D282): v4 folded the four displacement primitives into applyDisplacement in 9ecebf986
/// Old `add_to_slot` → [`apply_displacement`] in `add_to_slot` mode.
#[allow(clippy::too_many_arguments)]
pub fn add_to_slot(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    chat_id: &str,
    character_id: &str,
    slot: &str,
    id: &str,
    types: &[String],
    component_item_ids: &[String],
    tiers: &SharedWardrobeTiers,
    source: EquipSource,
) -> Result<Slots, DbError> {
    // The pre-`9ecebf986` `addToSlot` threw if the item couldn't occupy the
    // slot; the wear handler checks first, so this is unreachable there.
    debug_assert!(types.iter().any(|t| t == slot));
    let items_by_id = lookup_for_bundle(main, docs, character_id, component_item_ids, tiers);
    let node = WearableNode::new(id, types, component_item_ids);
    apply_displacement(
        main,
        chat_id,
        character_id,
        &ComputeDisplacedOptions {
            mode: DisplacementMode::AddToSlot,
            item: Some(&node),
            slot: Some(slot),
            item_id: None,
            items_by_id: items_by_id.as_ref(),
        },
        source,
    )
}

// LANE-REMOVE(P4.D280|P4.D282): v4 folded the four displacement primitives into applyDisplacement in 9ecebf986
/// Old `remove_from_slot` → [`apply_displacement`] in `remove_from_slot` mode
/// (always committed as `take-off`; the passed source is what v4's callers
/// pass — `ui`).
pub fn remove_from_slot(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    slot: &str,
    item_id: Option<&str>,
) -> Result<Slots, DbError> {
    apply_displacement(
        main,
        chat_id,
        character_id,
        &ComputeDisplacedOptions {
            mode: DisplacementMode::RemoveFromSlot,
            item: None,
            slot: Some(slot),
            item_id,
            items_by_id: None,
        },
        EquipSource::Ui,
    )
}

// ── resolve-equipped.ts — resolveEquippedOutfitForCharacter ──────────────────

/// v4 `resolveEquippedOutfitForCharacter` (the `outfitValues` half — the only part
/// the coverage summary needs). Loads the character's wardrobe (so composite
/// components resolve), fills missing equipped ids from the archetype tiers,
/// expands each slot's composites into leaves, and routes each leaf's title into
/// every output slot its own `types` declare (deduped across input slots).
pub fn resolve_equipped_outfit_values(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    slots: &Slots,
    tiers: &SharedWardrobeTiers,
) -> Result<OutfitSlotValues, DbError> {
    // Second pass over the raw leaf items: route each leaf's title into every
    // output slot its `types` declare.
    let leaves = resolve_equipped_outfit_leaf_values(main, docs, character_id, slots, tiers)?;
    let title_of = |item: &Value| {
        item.get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Ok(crate::wardrobe::build_outfit_slot_values(|slot| {
        leaves.slot(slot).iter().map(&title_of).collect()
    }))
}

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

    fn slot_mut(&mut self, name: &str) -> &mut Vec<Value> {
        match name {
            "top" => &mut self.top,
            "bottom" => &mut self.bottom,
            "footwear" => &mut self.footwear,
            "accessories" => &mut self.accessories,
            "hair" => &mut self.hair,
            _ => &mut self.top,
        }
    }
}

/// The shared first half of v4 `resolveEquippedOutfitForCharacter`: build the
/// `itemsById` map (own wardrobe → equipped-id fallback → composite-component
/// hydration), then expand every input slot's composites into leaves, deduped in
/// first-seen order.
///
/// v5 renders the result two ways — [`resolve_equipped_outfit_leaf_values`]
/// routes whole items per slot, [`resolve_equipped_leaf_items_by_slot`] routes
/// the flattened image-generation shape — so the ONE resolution lives here.
/// (v4 has one function with one shape; splitting the tail was v5's choice, and
/// duplicating the head was v5's bug: `8bb1a958`'s hydration loop would
/// otherwise have to be written twice and stay written twice.)
fn resolve_equipped_leaves(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    slots: &Slots,
    tiers: &SharedWardrobeTiers,
) -> Vec<Value> {
    // Unique equipped ids across every input slot (v4 `allEquippedItemIds`).
    let mut equipped_ids: Vec<String> = Vec::new();
    let mut seen_id: std::collections::HashSet<String> = std::collections::HashSet::new();
    for slot in WARDROBE_SLOT_TYPES {
        for id in slots.slot(slot) {
            if seen_id.insert(id.clone()) {
                equipped_ids.push(id.clone());
            }
        }
    }
    if equipped_ids.is_empty() {
        return Vec::new();
    }

    // Pull the character's own wardrobe (v4 try/catch → []) for composite resolution.
    let char_items = find_by_character_id(main, docs, character_id, true).unwrap_or_default();
    let mut items_by_id: WearableLookup = WearableLookup::new();
    for it in char_items {
        if let Some(id) = it.get("id").and_then(Value::as_str) {
            items_by_id.insert(id.to_string(), it);
        }
    }

    // Fill any equipped ids the character wardrobe didn't supply (archetypes, etc.).
    let missing: Vec<String> = equipped_ids
        .iter()
        .filter(|id| !items_by_id.contains_key(*id))
        .cloned()
        .collect();
    if !missing.is_empty() {
        let fallback = find_by_ids_for_character(main, docs, character_id, &missing, tiers)
            .unwrap_or_default();
        for it in fallback {
            if let Some(id) = it.get("id").and_then(Value::as_str) {
                items_by_id.insert(id.to_string(), it);
            }
        }
    }

    // A composite may bundle components the character doesn't own and that aren't
    // equipped in their own right. Hydrating them is shared with the write side
    // (`lookup_for_bundle`) so both ends see the same component graph — v4
    // extracted the same loop into `lib/wardrobe/hydrate-components.ts` at
    // `61574563` for exactly that reason.
    hydrate_component_graph(main, docs, character_id, &mut items_by_id, tiers);

    // Expand each slot's composites, dedup leaves in first-seen order.
    let mut seen_leaf: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ordered_leaves: Vec<Value> = Vec::new();
    for slot in WARDROBE_SLOT_TYPES {
        let expanded = expand_composites(slots.slot(slot), &items_by_id, None);
        for id in expanded.leaf_ids {
            if seen_leaf.contains(&id) {
                continue;
            }
            let Some(item) = items_by_id.get(&id) else {
                continue;
            };
            seen_leaf.insert(id);
            ordered_leaves.push(item.clone());
        }
    }
    ordered_leaves
}

/// v4 `resolveEquippedOutfitForCharacter` — the `leafItemsBySlot` half. Loads the
/// character's wardrobe (so composites resolve), fills missing equipped ids from
/// the archetype tiers, hydrates composite components that live in a shared tier,
/// expands each slot's composites into leaves (deduped in first-seen order), and
/// routes each leaf ITEM into every output slot its own `types` declare.
pub fn resolve_equipped_outfit_leaf_values(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    slots: &Slots,
    tiers: &SharedWardrobeTiers,
) -> Result<LeafItemsBySlot, DbError> {
    let ordered_leaves = resolve_equipped_leaves(main, docs, character_id, slots, tiers);

    // Second pass: route each leaf ITEM into every output slot its `types` declare.
    let mut out = LeafItemsBySlot::default();
    for item in &ordered_leaves {
        let types = item
            .get("types")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for t in &types {
            if let Some(slot) = t.as_str() {
                if WARDROBE_SLOT_TYPES.contains(&slot) {
                    out.slot_mut(slot).push(item.clone());
                }
            }
        }
    }

    Ok(out)
}

/// One equipped leaf item routed to an output slot (v4
/// `ResolvedEquippedOutfit.leafItemsBySlot[slot][]`). Carries the fields image
/// appearance-resolution consumes: `title`, `description`, `imagePrompt`.
#[derive(Clone, Debug, Default)]
pub struct EquippedLeafItem {
    pub slot: String,
    pub title: String,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
}

/// v4 `resolveEquippedOutfitForCharacter`'s `leafItemsBySlot` half — the sibling of
/// [`resolve_equipped_outfit_values`] that returns the per-slot leaf ITEMS (not
/// just titles), flattened in v4's `['top','bottom','footwear','accessories']` slot
/// order. Image generation's appearance resolution needs `title` +
/// `description` + `imagePrompt` per leaf, which the title-only
/// [`resolve_equipped_outfit_values`] discards.
///
/// Shares the exact resolution head with its sibling via
/// [`resolve_equipped_leaves`] (own wardrobe → equipped fallback → component
/// hydration → composite expansion), then routes each leaf into every output slot
/// its own `types` declare.
pub fn resolve_equipped_leaf_items_by_slot(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    slots: &Slots,
    tiers: &SharedWardrobeTiers,
) -> Result<Vec<EquippedLeafItem>, DbError> {
    let ordered_leaves = resolve_equipped_leaves(main, docs, character_id, slots, tiers);

    // Second pass: route each leaf into every output slot its `types` declare.
    let mut out: Vec<EquippedLeafItem> = Vec::new();
    for item in &ordered_leaves {
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let description = item
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        let image_prompt = item
            .get("imagePrompt")
            .and_then(Value::as_str)
            .map(str::to_string);
        let types = item
            .get("types")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for t in &types {
            if let Some(slot) = t.as_str() {
                if WARDROBE_SLOT_TYPES.contains(&slot) {
                    out.push(EquippedLeafItem {
                        slot: slot.to_string(),
                        title: title.clone(),
                        description: description.clone(),
                        image_prompt: image_prompt.clone(),
                    });
                }
            }
        }
    }

    Ok(out)
}

/// v4 `buildWardrobeCoverageSummaryFromState` — resolve the equipped outfit into
/// leaf titles, then render via [`describe_outfit`].
pub fn build_wardrobe_coverage_summary_from_state(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    slots: &Slots,
    tiers: &SharedWardrobeTiers,
) -> Result<String, DbError> {
    let values = resolve_equipped_outfit_values(main, docs, character_id, slots, tiers)?;
    Ok(describe_outfit(&values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_own_checks_character_id() {
        assert!(is_own_wardrobe_item(&json!({ "characterId": "c1" }), "c1"));
        assert!(!is_own_wardrobe_item(&json!({ "characterId": null }), "c1"));
        assert!(!is_own_wardrobe_item(
            &json!({ "characterId": "other" }),
            "c1"
        ));
    }

    #[test]
    fn find_equipped_slots_walks_all_slots() {
        let eq = json!({ "top": ["a", "b"], "bottom": [], "footwear": ["a"], "accessories": [] });
        assert_eq!(find_equipped_slots("a", Some(&eq)), vec!["top", "footwear"]);
        assert_eq!(find_equipped_slots("b", Some(&eq)), vec!["top"]);
        assert!(find_equipped_slots("z", Some(&eq)).is_empty());
        assert!(find_equipped_slots("a", None).is_empty());
    }
}
