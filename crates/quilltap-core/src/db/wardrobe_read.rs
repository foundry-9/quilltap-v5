//! The public wardrobe **read** path (v4 `WardrobeRepository.findByCharacterId`
//! + `findByIdForCharacter` + `getOverlaidWardrobeItems`).
//!
//! Post-cutover a character's wardrobe lives only in its document vault, so the
//! public reads are vault overlays, not table scans:
//!
//!   - [`find_by_character_id`] (v4 `findByCharacterId` → `getOverlaidWardrobeItems`)
//!     resolves the character's `characterDocumentMountPointId`, reads its
//!     `Wardrobe/` via the seeded read overlay (so composites can reference shared
//!     archetypes), coerces every item's `characterId` to the lookup id, and
//!     applies the archived filter. Any missing character / unlinked vault /
//!     unreadable folder degrades to `[]` (v4 logs and returns empty).
//!   - [`find_by_id_for_character`] (v4 `findByIdForCharacter`) reads the
//!     character's wardrobe (including archived — equip paths need an archived
//!     item's `types`), returns the owned item if present, else falls back to
//!     [`super::archetype_wardrobe::find_archetype_by_id`] across the shared
//!     General/project tiers.
//!
//! **Tracked deferral — `findByCharacterIdRaw`.** v4's raw variant reads the
//! pre-cutover `wardrobe_items` SQL table (`findByFilter({ characterId })`),
//! bypassing the vault. It is deprecated and used only by the one-time vault
//! populator / cutover migrations while that table still exists; the vault era
//! drops the table, and no W4.0 consumer calls it (the transfers service and all
//! live reads go through the overlay). Port it with the migrations if/when they
//! are ported.

use rusqlite::Connection;
use serde_json::Value;

use super::archetype_wardrobe::{find_archetype_by_id, find_archetypes};
use super::characters_read;
use super::doc_mount_documents::DocMountDocumentsRepository;
use super::vault_read_overlay::read_character_vault_wardrobe;
use super::DbError;
use crate::vault_overlay::SeedArchetype;

/// v4's `hasLinkedVault(character)` — the character row carries a non-empty
/// `characterDocumentMountPointId`.
fn linked_vault_mount(character: &Value) -> Option<String> {
    character
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// v4 `!item.archivedAt` — active when `archivedAt` is falsy (`null`, absent, or
/// the empty string). Mirrors [`super::archetype_wardrobe`]'s filter.
fn is_archived_truthy(item: &Value) -> bool {
    match item.get("archivedAt") {
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

/// v4 `findByCharacterId` / `getOverlaidWardrobeItems` — a character's overlaid
/// wardrobe items. `main` holds `characters` + `instance_settings`; `docs` is the
/// mount-index document store. Degrades to `[]` (never errors) on a missing
/// character or unlinked/unreadable vault, matching v4's graceful reads.
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

    // The character read seeds shared archetypes so a composite can reference a
    // household item it doesn't hold — v4 `readCharacterVaultWardrobe(mp, charId)`
    // with the default `seedArchetypes = true` → `findArchetypes(true)`.
    let fetch = || -> Result<Vec<SeedArchetype>, DbError> {
        let archetypes = find_archetypes(main, docs, true, &[])?;
        Ok(archetypes.iter().map(SeedArchetype::from_value).collect())
    };
    let Some(vault) =
        read_character_vault_wardrobe(docs, &mount_point_id, character_id, true, &fetch)?
    else {
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

/// v4 `findByIdsForCharacter` — multiple wardrobe items wearable by a character.
/// Honours the vault overlay (reads with archived included) and folds in shared
/// archetypes for any ids not in the character's own wardrobe. Returns items in
/// v4's `Map` insertion order: the character's own matches first (in read order),
/// then the archetype fills for the still-missing ids (in archetype read order).
/// `[]` for an empty id list, matching v4's early return.
pub fn find_by_ids_for_character(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    ids: &[String],
    project_mount_point_ids: &[String],
) -> Result<Vec<Value>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let want: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();

    let items = find_by_character_id(main, docs, character_id, true)?;
    let mut ordered: Vec<Value> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for it in items {
        let Some(id) = it.get("id").and_then(Value::as_str) else {
            continue;
        };
        if want.contains(id) && seen.insert(id.to_string()) {
            ordered.push(it);
        }
    }

    // Any ids not owned by the character: fill from the merged archetype tiers.
    let has_missing = ids.iter().any(|id| !seen.contains(id));
    if has_missing {
        let archetypes = find_archetypes(main, docs, true, project_mount_point_ids)?;
        for a in archetypes {
            let Some(id) = a.get("id").and_then(Value::as_str) else {
                continue;
            };
            if want.contains(id) && seen.insert(id.to_string()) {
                ordered.push(a);
            }
        }
    }

    Ok(ordered)
}

/// v4 `findByIdForCharacter` — a single wardrobe item by id, preferring the
/// character's own (including archived), then falling back to the shared
/// archetype tiers. `project_mount_point_ids` scopes the archetype fallback.
pub fn find_by_id_for_character(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    id: &str,
    project_mount_point_ids: &[String],
) -> Result<Option<Value>, DbError> {
    let items = find_by_character_id(main, docs, character_id, true)?;
    if let Some(owned) = items
        .into_iter()
        .find(|it| it.get("id").and_then(Value::as_str) == Some(id))
    {
        return Ok(Some(owned));
    }
    find_archetype_by_id(main, docs, id, project_mount_point_ids)
}
