//! The public wardrobe **read** path (v4 `WardrobeRepository.findByCharacterId`
//! + `readSharedTiers` + `getOverlaidWardrobeItems`, at `4a38222e6`).
//!
//! Post-cutover a character's wardrobe lives only in its document vault, so the
//! public reads are vault overlays, not table scans:
//!
//!   - [`find_by_character_id`] (v4 `findByCharacterId` → `getOverlaidWardrobeItems`)
//!     resolves the character's `characterDocumentMountPointId`, reads its
//!     `Wardrobe/**` recursively through the read overlay, coerces every item's
//!     `characterId` to the lookup id, and applies the archived filter. Any
//!     missing character / unlinked vault / unreadable folder degrades to `[]`
//!     (v4 logs and returns empty).
//!   - [`read_shared_tiers`] (v4 `readSharedTiers`, `9ecebf986`) reads the
//!     `Wardrobe/` folder of each shared mount in order, a later mount's item
//!     shadowing an earlier one with the same id — so callers pass mounts
//!     weakest-tier-first — and tags each item with `originOf(mountPointId)`.
//!
//! **The shrink (v4 `9ecebf986`).** v4's `WardrobeRepository` lost
//! `findByIdForCharacter`, `findByIdsForCharacter`, `findWearablePoolForCharacter`,
//! `findArchetypes*`, `findAll`, `findByCharacterIdRaw`, `archive` and
//! `unarchive`: every "what can this character reach" question is now a walk
//! over the per-request [`crate::wearable_pool::WearablePool`]
//! (`loadWearablePool` / `pool.get` / `pool.getMany` / `pool.wearable()`), and
//! the reader no longer seeds shared archetypes into component resolution (a
//! cross-tier ref survives parsing as its UUID — bug 187 — and the pool
//! resolves it). The three finders below are kept ONLY as `LANE-REMOVE`
//! wrappers over the pool so the files of the `4a38222e6` round's other lanes
//! compile until each re-points its callers; the keystone's census
//! `no_wardrobe_lane_remove_markers_left` fails while any marker remains.

use rusqlite::Connection;
use serde_json::Value;

use super::archetype_wardrobe::{read_shared_wardrobe, with_origin, WardrobeOrigin};
use super::characters_read;
use super::doc_mount_documents::DocMountDocumentsRepository;
use super::vault_read_overlay::read_character_vault_wardrobe;
use super::DbError;
use crate::wearable_pool::is_archived_truthy;

/// v4's `hasLinkedVault(character)` — the character row carries a non-empty
/// `characterDocumentMountPointId`.
fn linked_vault_mount(character: &Value) -> Option<String> {
    character
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// v4 `findByCharacterId` / `getOverlaidWardrobeItems` — a character's overlaid
/// wardrobe items. `main` holds `characters` + `instance_settings`; `docs` is the
/// mount-index document store. Degrades to `[]` (never errors) on a missing
/// character or unlinked/unreadable vault, matching v4's graceful reads.
///
/// Since `9ecebf986` the read seeds NOTHING: component refs resolve against the
/// vault's own folder, a cross-tier UUID ref is kept as written (bug 187), and
/// the wearable pool resolves it per request.
pub fn find_by_character_id(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    let Some(character) = characters_read::find_by_id_raw(main, character_id)? else {
        return Ok(Vec::new());
    };
    let Some(mount_point_id) = linked_vault_mount(&character) else {
        return Ok(Vec::new());
    };

    let Some(vault) = read_character_vault_wardrobe(docs, &mount_point_id, character_id)? else {
        return Ok(Vec::new());
    };

    let items = vault
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .into_iter()
        .map(|mut it| {
            // v4 `{ ...item, characterId }` — coerce to the lookup id.
            if let Some(obj) = it.as_object_mut() {
                obj.insert(
                    "characterId".to_string(),
                    Value::String(character_id.to_string()),
                );
            }
            it
        })
        .filter(|it| include_archived || !is_archived_truthy(it))
        .collect())
}

/// v4 `WardrobeRepository.readSharedTiers(mountPointIds, includeArchived,
/// originOf)` (`wardrobe.repository.ts:69-98`, `9ecebf986`) — read the
/// `Wardrobe/` folder of each shared mount in order, a later mount's item
/// shadowing an earlier one with the same id (v4's `Map.set`: the value is
/// replaced IN PLACE, keeping the first position) — so pass mounts
/// weakest-tier-first. Each item is tagged `{ ...item, origin: originOf(mp) }`
/// (`origin` LAST).
///
/// A mount that can't be read is logged (WARN `Failed to read shared wardrobe
/// tier; skipping`) and skipped: one unreadable store must not cost a character
/// the rest of their wardrobe. Never fails. `[]` for no mounts BEFORE anything
/// (no line).
pub fn read_shared_tiers(
    docs: &DocMountDocumentsRepository,
    mount_point_ids: &[String],
    include_archived: bool,
    origin_of: &dyn Fn(&str) -> WardrobeOrigin,
) -> Vec<Value> {
    if mount_point_ids.is_empty() {
        return Vec::new();
    }
    let mut order: Vec<Value> = Vec::new();
    let mut index_by_id: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for mount_point_id in mount_point_ids {
        match read_shared_wardrobe(docs, mount_point_id, include_archived) {
            Ok(items) => {
                let origin = origin_of(mount_point_id);
                for item in with_origin(items, &origin) {
                    let id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if let Some(&i) = index_by_id.get(&id) {
                        order[i] = item;
                    } else {
                        index_by_id.insert(id, order.len());
                        order.push(item);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    mountPointId = %mount_point_id,
                    context = "wardrobe",
                    error = %crate::db::fallback::error_text(&e),
                    "Failed to read shared wardrobe tier; skipping"
                );
            }
        }
    }
    tracing::debug!(
        mountCount = mount_point_ids.len(),
        itemCount = order.len(),
        includeArchived = include_archived,
        context = "wardrobe",
        "Read shared wardrobe tiers"
    );
    order
}
