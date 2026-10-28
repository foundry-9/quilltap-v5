//! Item mutations — the one way a wardrobe item is created or edited (v4
//! `lib/wardrobe/item-mutations.ts` + `lib/wardrobe/create-body.ts`,
//! `4a38222e6`; P4.D279, keystone C1 §11).
//!
//! The item routes (all four tiers), the `wardrobe_create` / `wardrobe_update`
//! tools and the transfer route build and patch items through here, so the
//! defaults, the composite `types` rule and the archive flag can't drift
//! between them:
//!
//!   - a fresh item's fields come from [`wardrobe_item_from_create_body`];
//!   - a composite's `types` are `build_composite_types(components, designated)`
//!     — the components' slots plus any designated extras. Widen, never narrow
//!     (bug 195): an edit that doesn't restate `types` keeps every slot the
//!     composite already claimed;
//!   - `archived: boolean` becomes `archivedAt` through
//!     [`crate::wardrobe::archived_patch`], which is idempotent (re-archiving
//!     keeps the original date, bug 188);
//!   - a requested `folder` is resolved against the wardrobe's own folders
//!     (`location.resolve_folder`: an existing folder as stored, else the new
//!     part normalised) on create, on an update that changes it, and in
//!     [`move_items_to_folder`] — the one way a batch is filed. An update
//!     naming the folder the item is already in moves nothing.
//!
//! Cycles are refused by the folder writer ([`WardrobeLocationError::Cycle`]).
//!
//! The lookup is keyed by id over READ-PATH item values (the shape the pool
//! and the collection readers emit), so a character's whole wearable pool —
//! shared tiers included — can be handed in as it is.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::Value;

use crate::db::archetype_wardrobe::read_general_wardrobe;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::vault_wardrobe_public::{
    UpdateManyOptions, WardrobeItemPatch, WardrobePatch, WardrobeScope,
};
use crate::db::wardrobe_location::{WardrobeLocation, WardrobeLocationError};
use crate::db::DbError;
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe::{archived_patch, build_composite_types, is_composite};
use crate::wardrobe_folders::is_same_folder;
use crate::wearable_pool::{load_wearable_pool, LoadWearablePoolOptions};

/// Every item a composite in this location could name as a component (v4
/// `ComponentLookup`): read-path item values by id.
pub type ComponentLookup = HashMap<String, Value>;

/// v4 `componentLookupFor(repos, location, projectMountPointIds)` — the
/// components a composite at `location` may gather: a character's whole
/// wearable pool (project tier included when the caller knows it), or for a
/// shared tier its own folder plus Quilltap General (the folder winning on a
/// shared id; General reads nothing extra for itself).
pub fn component_lookup_for(
    main: &Connection,
    mount: &Connection,
    location: &WardrobeLocation,
    project_mount_point_ids: &[String],
) -> Result<ComponentLookup, DbError> {
    if let Some(character_id) = &location.character_id {
        let pool = load_wearable_pool(
            main,
            mount,
            character_id,
            Some(project_mount_point_ids),
            LoadWearablePoolOptions::default(),
        );
        return Ok(pool.by_id().clone());
    }
    let mut map = ComponentLookup::new();
    if location.scope != WardrobeScope::General {
        let docs = DocMountDocumentsRepository::new(mount);
        for item in read_general_wardrobe(main, &docs, true)? {
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                map.insert(id.to_string(), item);
            }
        }
    }
    for item in location.read_items(mount, true)? {
        let id = item.id.clone();
        map.insert(id, item_value(&item));
    }
    Ok(map)
}

/// A stored item as a read-path value (the shape the lookup holds).
fn item_value(item: &WardrobeItem) -> Value {
    serde_json::to_value(item).unwrap_or(Value::Null)
}

/// v4 `validateComponentRefs(lookup, ids)` — split component ids into the
/// items found and the ids that weren't, deduplicating in order.
pub fn validate_component_refs<'a>(
    lookup: &'a ComponentLookup,
    ids: &[String],
) -> (Vec<&'a Value>, Vec<String>) {
    let mut components = Vec::new();
    let mut missing = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id.as_str()) {
            continue;
        }
        match lookup.get(id) {
            Some(item) => components.push(item),
            None => missing.push(id.clone()),
        }
    }
    (components, missing)
}

/// A read-path item's `types` as strings.
fn value_types(item: &Value) -> Vec<String> {
    item.get("types")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `compositeTypes(lookup, componentItemIds, designated)` — the `types` a
/// composite stores, given its components and what it designates. Without a
/// lookup the designated slots stand as given; an empty result (every
/// component missing, nothing designated) keeps the designation too.
fn composite_types(
    lookup: Option<&ComponentLookup>,
    component_item_ids: &[String],
    designated: &[String],
) -> Vec<String> {
    let Some(lookup) = lookup else {
        return designated.to_vec();
    };
    let (components, _missing) = validate_component_refs(lookup, component_item_ids);
    let component_types: Vec<Vec<String>> = components.iter().map(|c| value_types(c)).collect();
    let types = build_composite_types(&component_types, designated);
    if types.is_empty() {
        designated.to_vec()
    } else {
        types
    }
}

// ============================================================================
// create-body.ts
// ============================================================================

/// A parsed `createWardrobeSchema` body (v4 `WardrobeCreateBody`). Optional
/// keys are `None` when the body omitted them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WardrobeCreateBody {
    pub title: String,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
    pub types: Vec<String>,
    pub appropriateness: Option<String>,
    pub is_default: Option<bool>,
    pub component_item_ids: Option<Vec<String>>,
    pub replace: Option<bool>,
    pub folder: Option<String>,
}

/// v4 `prose(value)` — a prose field as stored: trimmed-empty reads as absent.
fn prose(value: Option<&str>) -> Option<String> {
    match value {
        Some(s) if !crate::jsstr::js_trim(s).is_empty() => Some(s.to_string()),
        _ => None,
    }
}

/// Dedupe in first-seen order (v4 `Array.from(new Set(ids))`).
fn unique_in_order(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect()
}

/// v4 `wardrobeItemFromCreateBody(body, characterId)` — map a validated create
/// body onto the stored item fields for `character_id` (`None` for a shared
/// item). Optional body fields resolve to their storage defaults
/// (`componentItemIds: []`, `isDefault: false`, `replace: false`, `null` for a
/// missing *or blank* prose field, the root folder); `migratedFromClothingRecordId`
/// and `imageFileId` are always `null` for a fresh item — a picture hangs off
/// an item that already has an id, through the images route. The folder is
/// carried as requested; [`create_item`] resolves it against the
/// destination's folders before writing. `id` / timestamps are blank — the
/// caller's.
pub fn wardrobe_item_from_create_body(
    body: &WardrobeCreateBody,
    character_id: Option<&str>,
) -> WardrobeItem {
    WardrobeItem {
        id: String::new(),
        character_id: Some(character_id.map(str::to_string)),
        title: body.title.clone(),
        description: Some(prose(body.description.as_deref())),
        image_prompt: Some(prose(body.image_prompt.as_deref())),
        types: body.types.clone(),
        component_item_ids: unique_in_order(body.component_item_ids.as_deref().unwrap_or(&[])),
        appropriateness: Some(prose(body.appropriateness.as_deref())),
        is_default: body.is_default.unwrap_or(false),
        replace: body.replace.unwrap_or(false),
        migrated_from_clothing_record_id: Some(None),
        image_file_id: Some(None),
        archived_at: Some(None),
        folder: body.folder.clone().unwrap_or_default(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// An existing item spread into a create body — v4's `createItem(location,
/// { ...item, title, folder, componentItemIds }, …)` (the transfers route's
/// planned item; folder filing's copies). Everything `wardrobeItemFromCreateBody`
/// reads is carried as the item holds it — so a copy of a default item is a
/// default item, and its `replace` / `appropriateness` / prose ride along —
/// while `imageFileId` / `migratedFromClothingRecordId` / the stamps come only
/// through [`PreservedIdentity`].
pub(crate) fn item_as_create_body(
    item: &WardrobeItem,
    title: String,
    folder: String,
    component_item_ids: Vec<String>,
) -> WardrobeCreateBody {
    WardrobeCreateBody {
        title,
        description: item.description.clone().flatten(),
        image_prompt: item.image_prompt.clone().flatten(),
        types: item.types.clone(),
        appropriateness: item.appropriateness.clone().flatten(),
        is_default: Some(item.is_default),
        component_item_ids: Some(component_item_ids),
        replace: Some(item.replace),
        folder: Some(folder),
    }
}

// ============================================================================
// item-mutations.ts
// ============================================================================

/// v4 `CreateItemOptions.preserve` — keep an existing item's identity and
/// history (transfers). The three optional keys are v4's `Partial<Pick<…>>`:
/// `None` → the fresh-item default (`null`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreservedIdentity {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<Option<String>>,
    pub image_file_id: Option<Option<String>>,
    pub migrated_from_clothing_record_id: Option<Option<String>>,
}

/// v4 `CreateItemOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CreateItemOptions<'a> {
    /// Component lookup; without it a composite keeps the body's `types` as given.
    pub lookup: Option<&'a ComponentLookup>,
    /// Keep an existing item's identity and history (transfers).
    pub preserve: Option<&'a PreservedIdentity>,
}

/// v4 `createItem(location, body, opts)` — create an item at a location from a
/// create body. DEBUG `[ItemMutations] Created wardrobe item`.
pub fn create_item(
    main: &Connection,
    mount: &Connection,
    location: &WardrobeLocation,
    body: &WardrobeCreateBody,
    opts: CreateItemOptions<'_>,
) -> Result<WardrobeItem, WardrobeLocationError> {
    let mut fields = wardrobe_item_from_create_body(body, location.character_id.as_deref());
    // The root needs no lookup (the tools always create there).
    fields.folder = if fields.folder.is_empty() {
        String::new()
    } else {
        location.resolve_folder(mount, &fields.folder)?
    };
    if is_composite(&fields.component_item_ids) {
        fields.types = composite_types(opts.lookup, &fields.component_item_ids, &fields.types);
    }
    let now = crate::clock::now_iso();
    let preserve = opts.preserve;
    let item = WardrobeItem {
        id: preserve
            .map(|p| p.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        archived_at: Some(preserve.and_then(|p| p.archived_at.clone().flatten())),
        image_file_id: Some(preserve.and_then(|p| p.image_file_id.clone().flatten())),
        migrated_from_clothing_record_id: Some(
            preserve.and_then(|p| p.migrated_from_clothing_record_id.clone().flatten()),
        ),
        created_at: preserve
            .map(|p| p.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: preserve.map(|p| p.updated_at.clone()).unwrap_or(now),
        ..fields
    };
    let stored = location.create(main, mount, &item)?;
    tracing::debug!(
        scope = location.scope.as_str(),
        mountPointId = %location.mount_point_id,
        itemId = %stored.id,
        composite = is_composite(&stored.component_item_ids),
        types = %serde_json::to_string(&stored.types).unwrap_or_default(),
        context = "wardrobe",
        "[ItemMutations] Created wardrobe item"
    );
    Ok(stored)
}

/// v4 `ItemPatch` — an edit: any item field (never `id` / timestamps /
/// `archivedAt` — the `archived` flag below is the request shape), plus a RAW
/// requested `folder`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemPatch {
    pub title: Option<String>,
    pub types: Option<Vec<String>>,
    pub component_item_ids: Option<Vec<String>>,
    pub description: Option<Option<String>>,
    pub image_prompt: Option<Option<String>>,
    pub appropriateness: Option<Option<String>>,
    pub is_default: Option<bool>,
    pub replace: Option<bool>,
    pub image_file_id: Option<Option<String>>,
    pub migrated_from_clothing_record_id: Option<Option<String>>,
    pub folder: Option<String>,
    pub archived: Option<bool>,
}

/// Which caller built an [`ItemPatch`] (contract C3, P4.170 — ruling P-7,
/// survey C.3 option A). v4's `fields: Object.keys(next)` is the `...fields`
/// spread of the CALLER's object, so the `[ItemMutations] Updated wardrobe
/// item` line's key order is the caller's: the item routes hand
/// `updateItem` the Zod parse of `updateWardrobeSchema` (shape order), the
/// `wardrobe_update` tool its hand-built patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchOrigin {
    /// The item routes (`wardrobe-route-factory.ts:495-506`): Zod's output
    /// follows `updateWardrobeSchema`'s shape — `wardrobeItemFieldsSchema`
    /// (`wardrobe.types.ts:155-173`) `.partial()`, then the `.extend`'s
    /// `archived` (stripped before `updateItem`) and `imageFileId`.
    Route,
    /// `wardrobe-update-handler.ts:99-107`'s conditional assignments.
    Tool,
}

impl PatchOrigin {
    /// The caller's key order. Each list ends with the keys that caller never
    /// sends, so every [`ItemPatch`] field has a slot whatever the origin
    /// (`migratedFromClothingRecordId` is the migration's alone).
    fn key_order(self) -> &'static [&'static str; 11] {
        match self {
            PatchOrigin::Route => &[
                "title",
                "description",
                "imagePrompt",
                "types",
                "appropriateness",
                "isDefault",
                "componentItemIds",
                "replace",
                "folder",
                "imageFileId",
                "migratedFromClothingRecordId",
            ],
            PatchOrigin::Tool => &[
                "title",
                "description",
                "imagePrompt",
                "appropriateness",
                "types",
                "isDefault",
                "replace",
                "componentItemIds",
                "folder",
                "imageFileId",
                "migratedFromClothingRecordId",
            ],
        }
    }
}

/// The keys of the patch object v4 actually sends to `location.update`, in
/// v4's `Object.keys(next)` order: the `...fields` spread in the CALLER's
/// order (`folder` dropped when it names the current folder), then
/// `archivedAt` when stamped, then a recomputed `types` — which keeps its
/// spread slot when the patch restated `types`, and lands LAST when it did
/// not.
fn patch_keys(
    next: &WardrobePatch,
    types_restated: bool,
    origin: PatchOrigin,
) -> Vec<&'static str> {
    let present = |key: &str| match key {
        "title" => next.title.is_some(),
        "types" => next.types.is_some(),
        "componentItemIds" => next.component_item_ids.is_some(),
        "description" => next.description.is_some(),
        "imagePrompt" => next.image_prompt.is_some(),
        "appropriateness" => next.appropriateness.is_some(),
        "isDefault" => next.is_default.is_some(),
        "replace" => next.replace.is_some(),
        "imageFileId" => next.image_file_id.is_some(),
        "migratedFromClothingRecordId" => next.migrated_from_clothing_record_id.is_some(),
        "folder" => next.folder.is_some(),
        "archivedAt" => next.archived_at.is_some(),
        _ => false,
    };
    let mut keys: Vec<&'static str> = origin
        .key_order()
        .iter()
        .copied()
        .filter(|k| present(k) && (*k != "types" || types_restated))
        .collect();
    if present("archivedAt") {
        keys.push("archivedAt");
    }
    if !types_restated && present("types") {
        keys.push("types");
    }
    keys
}

/// v4 `updateItem(location, current, patch, { lookup })` — patch `current` at
/// its location. A composite's `types` are recomputed whenever its components
/// or designated slots change, widening over the slots it already claimed
/// unless the patch restates them. `Ok(None)` when the item vanished under
/// the write. DEBUG `[ItemMutations] Updated wardrobe item`.
///
/// `origin` (C3) orders the line's `fields` as the caller's own object.
pub fn update_item_with_origin(
    main: &Connection,
    mount: &Connection,
    location: &WardrobeLocation,
    current: &WardrobeItem,
    patch: &ItemPatch,
    lookup: Option<&ComponentLookup>,
    origin: PatchOrigin,
) -> Result<Option<WardrobeItem>, WardrobeLocationError> {
    let mut next = WardrobePatch {
        title: patch.title.clone(),
        types: patch.types.clone(),
        component_item_ids: patch.component_item_ids.clone(),
        description: patch.description.clone(),
        image_prompt: patch.image_prompt.clone(),
        appropriateness: patch.appropriateness.clone(),
        is_default: patch.is_default,
        replace: patch.replace,
        archived_at: None,
        image_file_id: patch.image_file_id.clone(),
        folder: None,
        migrated_from_clothing_record_id: patch.migrated_from_clothing_record_id.clone(),
    };

    if let Some(folder) = &patch.folder {
        // An update naming the folder the item is already in moves nothing.
        if !is_same_folder(Some(folder), Some(&current.folder)) {
            next.folder = Some(location.resolve_folder(mount, folder)?);
        }
    }

    if let Some(archived) = patch.archived {
        let now = crate::clock::now_iso();
        if let Some(stamp) = archived_patch(
            current.archived_at.as_ref().and_then(|a| a.as_deref()),
            archived,
            &now,
        ) {
            next.archived_at = Some(stamp);
        }
    }

    let component_item_ids = patch
        .component_item_ids
        .as_deref()
        .unwrap_or(&current.component_item_ids);
    if !component_item_ids.is_empty()
        && (patch.component_item_ids.is_some() || patch.types.is_some())
    {
        next.types = Some(composite_types(
            lookup,
            component_item_ids,
            patch.types.as_deref().unwrap_or(&current.types),
        ));
    }

    let fields = patch_keys(&next, patch.types.is_some(), origin);
    let updated = location.update(main, mount, &current.id, &next)?;
    tracing::debug!(
        scope = location.scope.as_str(),
        mountPointId = %location.mount_point_id,
        itemId = %current.id,
        fields = %serde_json::to_string(&fields).unwrap_or_default(),
        found = updated.is_some(),
        context = "wardrobe",
        "[ItemMutations] Updated wardrobe item"
    );
    Ok(updated)
}

/// What [`set_item_archived`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveOutcome {
    /// The item as written — `None` when it vanished under the write.
    pub item: Option<WardrobeItem>,
    pub changed: bool,
}

/// v4 `setItemArchived(location, current, archived)` — archive (or restore)
/// an item. Idempotent (bug 188): an item already in the requested state is
/// returned untouched with `changed: false` and NOTHING is written. DEBUG
/// `[ItemMutations] Wardrobe item archive state changed` on a write.
pub fn set_item_archived(
    main: &Connection,
    mount: &Connection,
    location: &WardrobeLocation,
    current: &WardrobeItem,
    archived: bool,
) -> Result<ArchiveOutcome, WardrobeLocationError> {
    let now = crate::clock::now_iso();
    let Some(stamp) = archived_patch(
        current.archived_at.as_ref().and_then(|a| a.as_deref()),
        archived,
        &now,
    ) else {
        return Ok(ArchiveOutcome {
            item: Some(current.clone()),
            changed: false,
        });
    };
    let patch = WardrobePatch {
        archived_at: Some(stamp.clone()),
        ..WardrobePatch::default()
    };
    let item = location.update(main, mount, &current.id, &patch)?;
    tracing::debug!(
        scope = location.scope.as_str(),
        itemId = %current.id,
        archivedAt = %stamp.as_deref().unwrap_or("null"),
        context = "wardrobe",
        "[ItemMutations] Wardrobe item archive state changed"
    );
    let changed = item.is_some();
    Ok(ArchiveOutcome { item, changed })
}

/// v4 `moveItemsToFolder(location, itemIds, folder)` — file several items into
/// one folder at a location, with one read and one projection. Every id must
/// be an item of this location — a borrowed item (one shown in a character's
/// view from a shared tier) refuses the batch
/// ([`WardrobeLocationError::ItemNotInLocation`]), naming it, before anything
/// moves. Returns the moved items as written. DEBUG `[ItemMutations] Moved
/// wardrobe items to folder`.
pub fn move_items_to_folder(
    main: &Connection,
    mount: &Connection,
    location: &WardrobeLocation,
    item_ids: &[String],
    folder: &str,
) -> Result<Vec<WardrobeItem>, WardrobeLocationError> {
    let target = location.resolve_folder(mount, folder)?;
    let unique_ids = unique_in_order(item_ids);
    let patches: Vec<WardrobeItemPatch> = unique_ids
        .iter()
        .map(|id| WardrobeItemPatch {
            id: id.clone(),
            patch: WardrobePatch {
                folder: Some(target.clone()),
                ..WardrobePatch::default()
            },
        })
        .collect();
    let moved = location.update_many(
        main,
        mount,
        &patches,
        UpdateManyOptions { require_all: true },
    )?;
    tracing::debug!(
        scope = location.scope.as_str(),
        mountPointId = %location.mount_point_id,
        folder = %target,
        requested = unique_ids.len(),
        moved = moved.len(),
        context = "wardrobe",
        "[ItemMutations] Moved wardrobe items to folder"
    );
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn lookup_item(id: &str, types: &[&str]) -> Value {
        json!({ "id": id, "title": id, "types": types, "componentItemIds": [] })
    }

    fn lookup() -> ComponentLookup {
        [
            ("shirt", &["top"][..]),
            ("trousers", &["bottom"][..]),
            ("boots", &["footwear"][..]),
        ]
        .into_iter()
        .map(|(id, t)| (id.to_string(), lookup_item(id, t)))
        .collect()
    }

    #[test]
    fn prose_blank_or_whitespace_is_absent() {
        assert_eq!(prose(None), None);
        assert_eq!(prose(Some("")), None);
        assert_eq!(prose(Some("   ")), None);
        // JS trim strips U+00A0 and U+FEFF too.
        assert_eq!(prose(Some("\u{00a0}\u{feff}")), None);
        // Kept AS GIVEN, untrimmed.
        assert_eq!(prose(Some("  Pressed. ")), Some("  Pressed. ".to_string()));
    }

    #[test]
    fn create_body_applies_the_defaults() {
        let body = WardrobeCreateBody {
            title: "Cravat".into(),
            description: Some("   ".into()),
            types: s(&["accessories"]),
            component_item_ids: Some(s(&["a", "b", "a"])),
            ..WardrobeCreateBody::default()
        };
        let item = wardrobe_item_from_create_body(&body, Some("char-1"));
        assert_eq!(item.character_id, Some(Some("char-1".into())));
        assert_eq!(item.title, "Cravat");
        assert_eq!(item.description, Some(None));
        assert_eq!(item.image_prompt, Some(None));
        assert_eq!(item.appropriateness, Some(None));
        assert_eq!(item.component_item_ids, s(&["a", "b"]));
        assert!(!item.is_default);
        assert!(!item.replace);
        assert_eq!(item.migrated_from_clothing_record_id, Some(None));
        assert_eq!(item.image_file_id, Some(None));
        assert_eq!(item.folder, "");
        assert!(item.id.is_empty() && item.created_at.is_empty());
        // A shared item is owned by no character → `characterId: null`.
        let shared = wardrobe_item_from_create_body(&body, None);
        assert_eq!(shared.character_id, Some(None));
        let v = serde_json::to_value(&shared).unwrap();
        assert_eq!(v["characterId"], Value::Null);
        assert_eq!(v["description"], Value::Null);
    }

    #[test]
    fn composite_types_widens_never_narrows() {
        let l = lookup();
        // Components ∪ designated, canonical order.
        assert_eq!(
            composite_types(Some(&l), &s(&["shirt", "boots"]), &s(&["accessories"])),
            s(&["top", "footwear", "accessories"])
        );
        // Restated narrower types still widen back over the components.
        assert_eq!(
            composite_types(Some(&l), &s(&["shirt", "trousers"]), &s(&["hair"])),
            s(&["top", "bottom", "hair"])
        );
        // No lookup → the designation as given.
        assert_eq!(
            composite_types(None, &s(&["shirt", "boots"]), &s(&["top"])),
            s(&["top"])
        );
        // Every component missing and nothing designated → the (empty) designation.
        assert!(composite_types(Some(&l), &s(&["ghost"]), &[]).is_empty());
    }

    #[test]
    fn validate_component_refs_splits_and_dedupes() {
        let l = lookup();
        let (found, missing) =
            validate_component_refs(&l, &s(&["shirt", "ghost", "shirt", "boots"]));
        let ids: Vec<&str> = found.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["shirt", "boots"]);
        assert_eq!(missing, s(&["ghost"]));
    }

    #[test]
    fn patch_keys_follow_v4_object_order() {
        let next = WardrobePatch {
            description: Some(Some("x".into())),
            types: Some(s(&["top"])),
            archived_at: Some(None),
            ..WardrobePatch::default()
        };
        // `types` restated → keeps its spread slot — after `description` in
        // both callers' orders (Zod shape / the tool's assignments).
        assert_eq!(
            patch_keys(&next, true, PatchOrigin::Route),
            vec!["description", "types", "archivedAt"]
        );
        // `types` recomputed only → lands after `archivedAt`.
        assert_eq!(
            patch_keys(&next, false, PatchOrigin::Route),
            vec!["description", "archivedAt", "types"]
        );
    }

    /// C3 (P-7): each caller's object order. The route's Zod parse follows
    /// `updateWardrobeSchema`'s shape (`types` before `appropriateness`,
    /// `componentItemIds` before `replace`); the tool assigns `appropriateness`
    /// before `types` and `componentItemIds` last.
    #[test]
    fn patch_keys_follow_each_callers_order() {
        let next = WardrobePatch {
            title: Some("Coat".into()),
            types: Some(s(&["top"])),
            appropriateness: Some(Some("formal".into())),
            component_item_ids: Some(s(&["a"])),
            replace: Some(true),
            ..WardrobePatch::default()
        };
        assert_eq!(
            patch_keys(&next, true, PatchOrigin::Route),
            vec![
                "title",
                "types",
                "appropriateness",
                "componentItemIds",
                "replace"
            ]
        );
        assert_eq!(
            patch_keys(&next, true, PatchOrigin::Tool),
            vec![
                "title",
                "appropriateness",
                "types",
                "replace",
                "componentItemIds"
            ]
        );
    }
}
