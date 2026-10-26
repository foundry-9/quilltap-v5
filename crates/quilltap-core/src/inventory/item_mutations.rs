//! Inventory mutations — the one way an Inventory object is created, edited,
//! instantiated, detached, copied, filed, archived or deleted (v4
//! `lib/inventory/item-mutations.ts`, `4fbdad186` at `322a5488e`; P4.D324).
//! The routes call these (and the tools will), so the shape rules, the
//! `canCopy` and archetype guards and the SVG sanitiser live once:
//!
//!   - a kit's members must resolve in the writer's pool to an item or an
//!     archetype, never a kit (`KIT_NESTING`);
//!   - `instantiate` writes `{ id, parentId, ...overrides }` and nothing
//!     else, so the instance inherits every field it does not override;
//!   - an instance's edit stores only the keys patched; `clear_overrides`
//!     removes keys (a JSON PUT cannot say "remove");
//!   - `detach` bakes the resolved values in and nulls `parentId`;
//!     `detach_all` does it everywhere, refusing when an archived character
//!     holds an instance (`ARCHIVED_DEPENDENTS`);
//!   - an archetype with dependents is deleted only with `force` (bake
//!     first), and a deleted kit member is dropped from the kits of its tier;
//!   - `canCopy: false` refuses every copy (`NOT_COPYABLE`); move and
//!     instantiate stay allowed.
//!
//! v4 also runs every create, edit, archive, restore, delete and copy through
//! the title lint (`title-lint.ts`). The lint is P4.D340's (R-7): each of
//! v4's EIGHT call sites is marked below with a `P4.D340 lint seam` comment
//! at v4's position, where that lane inserts the verdict and its
//! `write_title_lint_rows` call; until it lands, no title is linted.
//!
//! Every mutation logs at `debug` (`[InventoryMutations] <op>`); refusals at
//! `info` (`[InventoryMutations] Refused`) with the code.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::dependents::{find_dependents, find_records_anywhere, InventoryDependent, MountScope};
use super::errors::{ArchivedDependent, DependentHolder, InventoryError, InventoryRefusal};
use super::folders::{
    is_same_folder, normalize_inventory_folder_name, InventoryFolder, InventoryFolderError,
};
use super::location::InventoryLocation;
use super::pool::{load_pool_for_location, InventoryPool, LoadPoolContext};
use super::resolve::{resolve_item, with_inventory_defaults};
use super::svg::sanitize_inventory_svg;
use super::types::{
    inventory_shape_issues, js_truthy, InventoryFields, InventoryKind, InventoryRecord,
    InventoryScope, ResolvedInventoryItem, INVENTORY_INHERITABLE_FIELDS,
};
use crate::db::inventory_writes::{
    read_mount_inventory, update_inventory_many_in_mount, InventoryRecordPatch,
};
use crate::db::item_folder_ops::{
    resolve_vault_folder_mount, VaultFolderMount, VaultFolderMountScope,
};

/// The `meta` a refusal spreads into its line (v4's per-call-site object
/// literals — tracing field names are static, so each literal shape is a
/// variant, its keys in v4's order).
#[derive(Debug, Clone, Copy)]
pub enum MutationMeta<'a> {
    /// `{ scope, mountPointId, op }` (create).
    Location {
        scope: InventoryScope,
        mount_point_id: &'a str,
        op: &'a str,
    },
    /// `{ scope, mountPointId, archetypeId, op }` (instantiate).
    LocationArchetype {
        scope: InventoryScope,
        mount_point_id: &'a str,
        archetype_id: &'a str,
        op: &'a str,
    },
    /// `{ scope, mountPointId, itemId, op }` (update, clear-override,
    /// detach, delete).
    LocationItem {
        scope: InventoryScope,
        mount_point_id: &'a str,
        item_id: &'a str,
        op: &'a str,
    },
    /// `{ itemId, op }` (detach-all, duplicate).
    ItemOp { item_id: &'a str, op: &'a str },
    /// `{ op, itemId }` (the transfers' `transfer-copy` / `transfer-move`).
    OpItem { op: &'a str, item_id: &'a str },
    /// `{ op, folder }` (copy-items).
    OpFolder { op: &'a str, folder: &'a str },
}

/// Log a refusal at `info` and hand it back (v4 `refuse` — `{ code, message,
/// ...meta, context: 'inventory' }`).
pub fn refuse(error: InventoryRefusal, meta: MutationMeta<'_>) -> InventoryError {
    let code = error.code.as_str();
    let message = error.message.as_str();
    match meta {
        MutationMeta::Location {
            scope,
            mount_point_id,
            op,
        } => tracing::info!(
            code, message, scope = scope.as_str(), mountPointId = %mount_point_id, op,
            context = "inventory", "[InventoryMutations] Refused"
        ),
        MutationMeta::LocationArchetype {
            scope,
            mount_point_id,
            archetype_id,
            op,
        } => tracing::info!(
            code, message, scope = scope.as_str(), mountPointId = %mount_point_id,
            archetypeId = %archetype_id, op, context = "inventory", "[InventoryMutations] Refused"
        ),
        MutationMeta::LocationItem {
            scope,
            mount_point_id,
            item_id,
            op,
        } => tracing::info!(
            code, message, scope = scope.as_str(), mountPointId = %mount_point_id,
            itemId = %item_id, op, context = "inventory", "[InventoryMutations] Refused"
        ),
        MutationMeta::ItemOp { item_id, op } => tracing::info!(
            code, message, itemId = %item_id, op, context = "inventory", "[InventoryMutations] Refused"
        ),
        MutationMeta::OpItem { op, item_id } => tracing::info!(
            code, message, op, itemId = %item_id, context = "inventory", "[InventoryMutations] Refused"
        ),
        MutationMeta::OpFolder { op, folder } => tracing::info!(
            code, message, op, folder = %folder, context = "inventory", "[InventoryMutations] Refused"
        ),
    }
    InventoryError::Refusal(error)
}

/// v4 `logOp(op, location, meta)` — DEBUG `[InventoryMutations] ${op}` `{ op,
/// scope, mountPointId, ...meta, context }`; `$rest` are the meta's keys.
macro_rules! log_op {
    ($op:literal, $location:expr; $($rest:tt)*) => {
        tracing::debug!(
            op = $op,
            scope = $location.scope.as_str(),
            mountPointId = %$location.mount_point_id,
            $($rest)*
            context = "inventory",
            concat!("[InventoryMutations] ", $op)
        )
    };
}

fn json_text(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn now_iso() -> String {
    crate::clock::now_iso()
}

fn mint_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// `Array.from(new Set(ids))` over a JSON array's string entries.
fn unique_strings(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in v
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    }
    out
}

fn unique(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// Check a kit's members against the pool the kit is written in: each must
/// resolve to an item or an archetype (v4 `assertKitMembers`). Kits refuse
/// FIRST (`KIT_NESTING`, a 409), then missing ones (`INVALID_SHAPE` `Kit
/// members not reachable from this tier: …`, a 400). `allow_missing` excuses
/// a member the pool cannot see but the kit may still reach in context (a
/// project object named by a character's kit).
pub fn assert_kit_members(
    pool: &InventoryPool,
    item_ids: &[String],
    meta: MutationMeta<'_>,
    allow_missing: &[String],
) -> Result<(), InventoryError> {
    let mut missing: Vec<String> = Vec::new();
    let mut kits: Vec<String> = Vec::new();
    for id in item_ids {
        match pool.get(id) {
            None => {
                if !allow_missing.contains(id) {
                    missing.push(id.clone());
                }
            }
            Some(member) if member.kind() == InventoryKind::Kit => kits.push(id.clone()),
            Some(_) => {}
        }
    }
    if !kits.is_empty() {
        return Err(refuse(InventoryRefusal::kit_nesting(&kits), meta));
    }
    if !missing.is_empty() {
        let mut details = Map::new();
        details.insert("missing".into(), json!(missing));
        return Err(refuse(
            InventoryRefusal::shape(
                format!(
                    "Kit members not reachable from this tier: {}",
                    missing.join(", ")
                ),
                details,
            ),
            meta,
        ));
    }
    Ok(())
}

/// v4 `assertShape` — `INVALID_SHAPE` with `issues.join('; ')` and `{ issues }`.
fn assert_shape(record: &Map<String, Value>, meta: MutationMeta<'_>) -> Result<(), InventoryError> {
    let issues = inventory_shape_issues(record);
    if issues.is_empty() {
        return Ok(());
    }
    let mut details = Map::new();
    details.insert("issues".into(), json!(issues));
    Err(refuse(
        InventoryRefusal::shape(issues.join("; "), details),
        meta,
    ))
}

fn no_details() -> Map<String, Value> {
    Map::new()
}

fn archetype_details(archetype_id: &str) -> Map<String, Value> {
    let mut d = Map::new();
    d.insert("archetypeId".into(), json!(archetype_id));
    d
}

/// Borrow the caller's pool or load the location's own.
fn pool_or_load<'p>(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    given: Option<&'p InventoryPool>,
    slot: &'p mut Option<InventoryPool>,
    ctx: LoadPoolContext<'_>,
) -> Result<&'p InventoryPool, InventoryError> {
    match given {
        Some(p) => Ok(p),
        None => Ok(slot.insert(load_pool_for_location(main, mount, location, ctx)?)),
    }
}

// ============================================================================
// Create
// ============================================================================

/// An existing object's identity and history, kept by a create (v4
/// `CreateInventoryItemOptions.preserve` — transfers).
#[derive(Debug, Clone, Default)]
pub struct PreservedIdentity {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub image_file_id: Option<String>,
}

/// v4 `CreateInventoryItemOptions`.
#[derive(Debug, Default)]
pub struct CreateInventoryItemOptions<'a> {
    pub preserve: Option<PreservedIdentity>,
    /// A pool already loaded for the location (a kit's member check).
    pub pool: Option<&'a InventoryPool>,
}

/// Create a standalone object, archetype or kit at a location (v4
/// `createInventoryItem`). `body` is a parsed `InventoryCreateBody` (shape
/// order). The record written: the defaults filled, `itemIds` de-duped,
/// `svg` sanitised (`null` WRITTEN for absent / blank), `archivedAt` /
/// `imageFileId` from `preserve` else `null`, `folder` resolved against the
/// tier's folders (else `''`).
pub fn create_inventory_item(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    body: &Map<String, Value>,
    opts: CreateInventoryItemOptions<'_>,
) -> Result<InventoryRecord, InventoryError> {
    let meta = MutationMeta::Location {
        scope: location.scope,
        mount_point_id: &location.mount_point_id,
        op: "create",
    };
    let folder = body
        .get("folder")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut fields = body.clone();
    fields.shift_remove("folder");
    assert_shape(&fields, meta)?;
    if js_truthy(fields.get("parentId")) {
        return Err(refuse(
            InventoryRefusal::invalid_parent("Create an instance with instantiate", no_details()),
            meta,
        ));
    }
    let item_ids = unique_strings(fields.get("itemIds"));
    let is_kit = fields.get("kind").and_then(Value::as_str) == Some("kit");
    let mut loaded = None;
    let pool = if is_kit {
        Some(pool_or_load(
            main,
            mount,
            location,
            opts.pool,
            &mut loaded,
            LoadPoolContext::default(),
        )?)
    } else {
        opts.pool
    };
    if is_kit {
        assert_kit_members(pool.expect("a kit's pool"), &item_ids, meta, &[])?;
    }
    let now = now_iso();
    let preserve = opts.preserve.as_ref();
    let mut seed = fields.clone();
    seed.insert(
        "id".into(),
        json!(preserve.map_or_else(mint_id, |p| p.id.clone())),
    );
    seed.insert(
        "createdAt".into(),
        json!(preserve.map_or_else(|| now.clone(), |p| p.created_at.clone())),
    );
    seed.insert(
        "updatedAt".into(),
        json!(preserve.map_or_else(|| now.clone(), |p| p.updated_at.clone())),
    );
    seed.insert("folder".into(), json!(""));
    let mut record = InventoryRecord(with_inventory_defaults(&InventoryRecord(seed)).0);
    record.set("itemIds", json!(item_ids));
    let svg = sanitize_inventory_svg(fields.get("svg").and_then(Value::as_str))?;
    record.set("svg", svg.map_or(Value::Null, Value::String));
    record.set(
        "archivedAt",
        preserve
            .and_then(|p| p.archived_at.clone())
            .map_or(Value::Null, Value::String),
    );
    record.set(
        "imageFileId",
        preserve
            .and_then(|p| p.image_file_id.clone())
            .map_or(Value::Null, Value::String),
    );
    let resolved_folder = match folder.as_deref().filter(|f| !f.is_empty()) {
        Some(f) => location.resolve_folder(mount, f)?,
        None => String::new(),
    };
    record.set("folder", json!(resolved_folder));
    // P4.D340 lint seam: door=create — v4 item-mutations.ts:170-172 (`{ pool }`,
    // skipped when `preserve`; the rows written after the create, :174).
    let stored = location.create(main, mount, &record)?;
    log_op!("Created object", location;
        itemId = %stored.id(),
        kind = stored.kind().as_str(),
        isArchetype = stored.is_archetype(),
        folder = %stored.folder(),
    );
    Ok(stored)
}

// ============================================================================
// Instantiate
// ============================================================================

/// v4 `InstantiateOptions`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InstantiateOptions<'a> {
    pub folder: Option<&'a str>,
    /// Inheritable overrides (other keys and `undefined` dropped).
    pub overrides: Option<&'a Map<String, Value>>,
    /// The destination's pool, when the caller already has it.
    pub pool: Option<&'a InventoryPool>,
}

/// What [`instantiate`] answers: the record and its resolution.
#[derive(Debug, Clone)]
pub struct Instantiated {
    pub record: InventoryRecord,
    pub item: ResolvedInventoryItem,
}

/// Make an instance of an archetype at a location (v4 `instantiate`). The
/// archetype must be an archetype reachable from the destination and not
/// archived. Instantiating is not copying: `canCopy` does not apply. The file
/// is named after the archetype until the instance has a title of its own
/// (`fileStem`).
pub fn instantiate(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    archetype_id: &str,
    opts: InstantiateOptions<'_>,
) -> Result<Instantiated, InventoryError> {
    let meta = MutationMeta::LocationArchetype {
        scope: location.scope,
        mount_point_id: &location.mount_point_id,
        archetype_id,
        op: "instantiate",
    };
    let mut loaded = None;
    let pool = pool_or_load(
        main,
        mount,
        location,
        opts.pool,
        &mut loaded,
        LoadPoolContext::default(),
    )?;
    let archetype = match pool.get(archetype_id) {
        Some(a) if a.is_archetype() => a.clone(),
        _ => {
            return Err(refuse(
                InventoryRefusal::invalid_parent(
                    "The parent must be an archetype this tier can reach",
                    archetype_details(archetype_id),
                ),
                meta,
            ))
        }
    };
    if archetype.is_archived() {
        return Err(refuse(
            InventoryRefusal::invalid_parent(
                "An archived archetype cannot be instantiated",
                archetype_details(archetype_id),
            ),
            meta,
        ));
    }
    let mut overrides = Map::new();
    for (key, value) in opts.overrides.into_iter().flatten() {
        if INVENTORY_INHERITABLE_FIELDS.contains(&key.as_str()) {
            overrides.insert(key.clone(), value.clone());
        }
    }
    if let Some(svg) = overrides.get("svg").cloned() {
        let clean = sanitize_inventory_svg(svg.as_str())?;
        overrides.insert("svg".into(), clean.map_or(Value::Null, Value::String));
    }
    let now = now_iso();
    let mut record = Map::new();
    record.insert("id".into(), json!(mint_id()));
    record.insert("parentId".into(), json!(archetype_id));
    for (k, v) in &overrides {
        record.insert(k.clone(), v.clone());
    }
    record.insert("createdAt".into(), json!(now));
    record.insert("updatedAt".into(), json!(now));
    let folder = match opts.folder.filter(|f| !f.is_empty()) {
        Some(f) => location.resolve_folder(mount, f)?,
        None => String::new(),
    };
    record.insert("folder".into(), json!(folder));
    // `archetype.title || archetype.name || undefined`.
    let stem = archetype.title_or_name();
    if !stem.is_empty() {
        record.insert("fileStem".into(), json!(stem));
    }
    let record = InventoryRecord(record);
    // P4.D340 lint seam: door=create — v4 item-mutations.ts:226-230 (only for
    // a non-blank `overrides.title` — an inherited title was linted on the
    // archetype; `{ pool }`).
    let stored = location.create(main, mount, &record)?;
    let item = resolve_item(&stored, |id| {
        if id == archetype_id {
            Some(&archetype)
        } else {
            pool.get(id)
        }
    });
    let override_keys: Vec<&String> = overrides.keys().collect();
    log_op!("Instantiated archetype", location;
        itemId = %stored.id(),
        archetypeId = %archetype_id,
        overridesJson = %json_text(&override_keys),
    );
    Ok(Instantiated {
        record: stored,
        item,
    })
}

// ============================================================================
// Update / clear overrides / archive
// ============================================================================

fn holders_of(dependents: &[InventoryDependent]) -> Vec<DependentHolder> {
    let mut seen: Vec<(String, DependentHolder)> = Vec::new();
    for d in dependents {
        let key = format!(
            "{}:{}",
            d.scope.as_str(),
            d.owner_id.as_deref().unwrap_or(&d.mount_point_id)
        );
        if !seen.iter().any(|(k, _)| *k == key) {
            seen.push((
                key,
                DependentHolder {
                    scope: d.scope.as_str().to_string(),
                    owner_id: d.owner_id.clone(),
                    owner_name: d.owner_name.clone(),
                },
            ));
        }
    }
    seen.into_iter().map(|(_, h)| h).collect()
}

/// Patch a record at its location (v4 `updateInventoryItem`). An instance
/// stores only the keys patched (its `kind` / `isArchetype` / `itemIds` are
/// refused when they would make it an archetype or a kit, else silently
/// dropped). Turning an archetype with dependents into an item is
/// `ARCHETYPE_IN_USE`; making a kit an archetype, or an archetype a kit, is
/// `INVALID_PARENT`. A kit's members are checked against the same pool its
/// creation was (`context` names the project a character's kit may borrow
/// from). `body` is a parsed `InventoryUpdateBody`.
pub fn update_inventory_item(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    current: &InventoryRecord,
    body: &Map<String, Value>,
    context: LoadPoolContext<'_>,
) -> Result<Option<InventoryRecord>, InventoryError> {
    let meta = MutationMeta::LocationItem {
        scope: location.scope,
        mount_point_id: &location.mount_point_id,
        item_id: current.id(),
        op: "update",
    };
    let archived = body.get("archived").and_then(Value::as_bool);
    let folder = body.get("folder").and_then(Value::as_str);
    let mut patch = Map::new();
    for (k, v) in body {
        if k != "archived" && k != "folder" {
            patch.insert(k.clone(), v.clone());
        }
    }
    let kind_of =
        |m: &Map<String, Value>| m.get("kind").and_then(Value::as_str).map(str::to_string);

    if current.parent_id().is_some() {
        if js_truthy(patch.get("isArchetype")) {
            return Err(refuse(
                InventoryRefusal::invalid_parent(
                    "An instance cannot be an archetype",
                    no_details(),
                ),
                meta,
            ));
        }
        let has_members = patch
            .get("itemIds")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty());
        if kind_of(&patch).as_deref() == Some("kit") || has_members {
            return Err(refuse(
                InventoryRefusal::shape("An instance is an item, never a kit", no_details()),
                meta,
            ));
        }
        for key in ["kind", "isArchetype", "itemIds"] {
            patch.shift_remove(key);
        }
    } else {
        // `patch.kind ?? current.kind ?? 'item'`.
        let next_kind = kind_of(&patch)
            .or_else(|| current.str_of("kind").map(str::to_string))
            .unwrap_or_else(|| "item".to_string());
        let patch_archetype = patch.get("isArchetype").and_then(Value::as_bool);
        if patch_archetype == Some(true) && !current.is_archetype() && next_kind == "kit" {
            return Err(refuse(
                InventoryRefusal::invalid_parent("A kit cannot be an archetype", no_details()),
                meta,
            ));
        }
        if patch_archetype == Some(false) && current.is_archetype() {
            let dependents = find_dependents(main, mount, current.id())?;
            if !dependents.is_empty() {
                return Err(refuse(
                    InventoryRefusal::archetype_in_use(dependents.len(), &holders_of(&dependents)),
                    meta,
                ));
            }
        }
        if kind_of(&patch).as_deref() == Some("kit")
            && current.is_archetype()
            && patch_archetype != Some(false)
        {
            return Err(refuse(
                InventoryRefusal::invalid_parent("An archetype cannot be a kit", no_details()),
                meta,
            ));
        }
        let mut merged = current.0.clone();
        for (k, v) in &patch {
            merged.insert(k.clone(), v.clone());
        }
        assert_shape(&merged, meta)?;
        if next_kind == "kit" && patch.contains_key("itemIds") {
            let ids = unique_strings(patch.get("itemIds"));
            patch.insert("itemIds".into(), json!(ids));
            let pool = load_pool_for_location(main, mount, location, context)?;
            assert_kit_members(&pool, &ids, meta, &[])?;
        }
    }

    if let Some(svg) = patch.get("svg").cloned() {
        let clean = sanitize_inventory_svg(svg.as_str())?;
        patch.insert("svg".into(), clean.map_or(Value::Null, Value::String));
    }
    if let Some(folder) = folder {
        if !is_same_folder(Some(folder), current.str_of("folder")) {
            patch.insert(
                "folder".into(),
                json!(location.resolve_folder(mount, folder)?),
            );
        }
    }
    if let Some(archived) = archived {
        if let Some(stamp) =
            crate::wardrobe::archived_patch(current.archived_at(), archived, &now_iso())
        {
            patch.insert(
                "archivedAt".into(),
                stamp.map_or(Value::Null, Value::String),
            );
        }
    }

    // P4.D340 lint seam: door=update — v4 item-mutations.ts:295-300 (`{ context }`;
    // the rows written when the update found the record, :302).
    let fields: Vec<String> = patch.keys().cloned().collect();
    let updated = location.update(main, mount, current.id(), patch, &[])?;
    log_op!("Updated object", location;
        itemId = %current.id(),
        fieldsJson = %json_text(&fields),
        found = updated.is_some(),
    );
    Ok(updated)
}

/// Remove an instance's overrides so those fields inherit again (v4
/// `clearOverrides`).
pub fn clear_overrides(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    current: &InventoryRecord,
    fields: &[String],
) -> Result<Option<InventoryRecord>, InventoryError> {
    let meta = MutationMeta::LocationItem {
        scope: location.scope,
        mount_point_id: &location.mount_point_id,
        item_id: current.id(),
        op: "clear-override",
    };
    if current.parent_id().is_none() {
        return Err(refuse(
            InventoryRefusal::invalid_parent(
                "Only an instance inherits; this object has no archetype",
                no_details(),
            ),
            meta,
        ));
    }
    // P4.D340 lint seam: door=update — v4 item-mutations.ts:316-318 (`after` =
    // `current` minus the fields: clearing a title override hands the instance
    // its archetype's title, an entering title like any other; rows at :320).
    let updated = location.update(main, mount, current.id(), Map::new(), fields)?;
    log_op!("Cleared overrides", location;
        itemId = %current.id(),
        fieldsJson = %json_text(&fields),
    );
    Ok(updated)
}

/// What [`set_inventory_item_archived`] answers (v4 `{ item, changed }`).
#[derive(Debug, Clone)]
pub struct ArchiveOutcome {
    pub item: Option<InventoryRecord>,
    pub changed: bool,
}

/// Archive (or restore) an object. Idempotent: already in the requested
/// state → `{ item: current, changed: false }` with NO write and no line (v4
/// `setInventoryItemArchived`).
pub fn set_inventory_item_archived(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    current: &InventoryRecord,
    archived: bool,
) -> Result<ArchiveOutcome, InventoryError> {
    let Some(stamp) = crate::wardrobe::archived_patch(current.archived_at(), archived, &now_iso())
    else {
        return Ok(ArchiveOutcome {
            item: Some(current.clone()),
            changed: false,
        });
    };
    let mut patch = Map::new();
    patch.insert(
        "archivedAt".into(),
        stamp.clone().map_or(Value::Null, Value::String),
    );
    // P4.D340 lint seam: door=archive — v4 item-mutations.ts:331-334 (archiving
    // takes a title out of the pool — the bell; restoring brings it back — the
    // lint; rows at :336).
    let item = location.update(main, mount, current.id(), patch, &[])?;
    log_op!("Archive state changed", location;
        itemId = %current.id(),
        archivedAt = stamp.as_deref().unwrap_or("null"),
    );
    let changed = item.is_some();
    Ok(ArchiveOutcome { item, changed })
}

// ============================================================================
// Detach
// ============================================================================

/// The fields a bake writes: the resolved values, the parent's picture left
/// behind (v4 `bakedFields`). A resolved field that is `undefined` is
/// returned in the REMOVE list — v4's spread writes `undefined`, which the
/// serialiser drops, and dropping the key is the same bytes.
fn baked_fields(
    record: &InventoryRecord,
    resolved: &ResolvedInventoryItem,
) -> (Map<String, Value>, Vec<String>) {
    let mut baked = Map::new();
    let mut remove = Vec::new();
    baked.insert("parentId".into(), Value::Null);
    baked.insert("kind".into(), json!("item"));
    baked.insert("isArchetype".into(), json!(false));
    baked.insert("itemIds".into(), json!([]));
    for field in INVENTORY_INHERITABLE_FIELDS {
        if field == "imageFileId" {
            continue;
        }
        match resolved.get(field) {
            Some(v) => {
                baked.insert(field.to_string(), v.clone());
            }
            None => remove.push(field.to_string()),
        }
    }
    // A picture is kept only when it is the instance's own; the archetype's
    // pictures belong to the archetype (and go with it).
    baked.insert(
        "imageFileId".into(),
        record
            .get("imageFileId")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(Value::Null),
    );
    (baked, remove)
}

/// Break an instance's tie to its archetype, baking the resolved values in
/// (v4 `detach`).
pub fn detach(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    current: &InventoryRecord,
    pool: Option<&InventoryPool>,
) -> Result<Option<InventoryRecord>, InventoryError> {
    let meta = MutationMeta::LocationItem {
        scope: location.scope,
        mount_point_id: &location.mount_point_id,
        item_id: current.id(),
        op: "detach",
    };
    let Some(parent_id) = current.parent_id().map(str::to_string) else {
        return Err(refuse(
            InventoryRefusal::invalid_parent(
                "This object has no archetype to detach from",
                no_details(),
            ),
            meta,
        ));
    };
    let mut loaded = None;
    let lookup = pool_or_load(
        main,
        mount,
        location,
        pool,
        &mut loaded,
        LoadPoolContext::default(),
    )?;
    let resolved = lookup.resolve(current);
    let (patch, remove) = baked_fields(current, &resolved);
    let updated = location.update(main, mount, current.id(), patch, &remove)?;
    log_op!("Detached instance", location;
        itemId = %current.id(),
        parentId = %parent_id,
        parentMissing = resolved.parent_missing(),
    );
    Ok(updated)
}

/// Detach every instance of an archetype, wherever it lives — one write per
/// mount (v4 `detachAll`). Refused (`ARCHIVED_DEPENDENTS`) when any instance
/// lives in an archived character's vault. Answers how many were baked.
pub fn detach_all(
    main: &Connection,
    mount: &Connection,
    archetype_id: &str,
) -> Result<usize, InventoryError> {
    let meta = MutationMeta::ItemOp {
        item_id: archetype_id,
        op: "detach-all",
    };
    let dependents = find_dependents(main, mount, archetype_id)?;
    let archived: Vec<&InventoryDependent> =
        dependents.iter().filter(|d| d.owner_archived).collect();
    if !archived.is_empty() {
        // `new Map(archived.map(d => [d.ownerId, {id, name}]))` — first
        // position, last value.
        let mut characters: Vec<(Option<String>, ArchivedDependent)> = Vec::new();
        for d in &archived {
            let entry = ArchivedDependent {
                id: d.owner_id.clone(),
                name: d.owner_name.clone(),
            };
            match characters.iter_mut().find(|(k, _)| *k == d.owner_id) {
                Some((_, slot)) => *slot = entry,
                None => characters.push((d.owner_id.clone(), entry)),
            }
        }
        let characters: Vec<ArchivedDependent> = characters.into_iter().map(|(_, c)| c).collect();
        return Err(refuse(
            InventoryRefusal::archived_dependents(&characters),
            meta,
        ));
    }
    if dependents.is_empty() {
        return Ok(0);
    }
    let archetype = find_records_anywhere(mount, &[archetype_id.to_string()])
        .into_iter()
        .find(|(id, _)| id == archetype_id)
        .map(|(_, m)| m.record);
    let mut by_mount: Vec<(String, Vec<&InventoryDependent>)> = Vec::new();
    for d in &dependents {
        match by_mount.iter_mut().find(|(mp, _)| *mp == d.mount_point_id) {
            Some((_, group)) => group.push(d),
            None => by_mount.push((d.mount_point_id.clone(), vec![d])),
        }
    }
    let mut baked = 0usize;
    for (mount_point_id, group) in by_mount {
        let first = group[0];
        // A character's vault is re-resolved so the tombstone check sees it now.
        let target: Option<VaultFolderMount> = if first.scope == MountScope::Character {
            resolve_vault_folder_mount(main, first.owner_id.as_deref())?
        } else {
            Some(VaultFolderMount {
                mount_point_id: mount_point_id.clone(),
                scope: match first.scope {
                    MountScope::Unknown | MountScope::Project => VaultFolderMountScope::Project,
                    MountScope::Group => VaultFolderMountScope::Group,
                    MountScope::General => VaultFolderMountScope::General,
                    MountScope::Character => VaultFolderMountScope::Character,
                },
                character_id: None,
            })
        };
        let Some(target) = target else {
            continue;
        };
        let records = read_mount_inventory(mount, &target)?;
        let patches: Vec<InventoryRecordPatch> = group
            .iter()
            .filter_map(|d| records.iter().find(|r| r.id() == d.id))
            .map(|record| {
                let resolved = resolve_item(record, |id| {
                    if id == archetype_id {
                        archetype.as_ref()
                    } else {
                        None
                    }
                });
                let (patch, remove) = baked_fields(record, &resolved);
                InventoryRecordPatch {
                    id: record.id().to_string(),
                    patch,
                    remove,
                }
            })
            .collect();
        let written = update_inventory_many_in_mount(mount, &target, &patches, false)?;
        baked += written.len();
        tracing::debug!(
            op = "detach-all",
            mountPointId = %mount_point_id,
            scope = target.scope.as_str(),
            itemId = %archetype_id,
            count = written.len(),
            context = "inventory",
            "[InventoryMutations] Baked instances in one mount"
        );
    }
    Ok(baked)
}

// ============================================================================
// Delete
// ============================================================================

/// What [`delete_inventory_item`] answers (v4 `{ deleted, detached }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletedInventoryItem {
    pub deleted: bool,
    pub detached: usize,
}

/// Delete an object (v4 `deleteInventoryItem`). The tombstone refuses
/// FIRST. An archetype with dependents needs `force` (every instance is baked
/// first); without it, `ARCHETYPE_IN_USE`. A deleted object is dropped from
/// every kit in the same tier that named it WITH MORE THAN ONE member (a kit
/// never empties: its last member stays named and is skipped at equip time).
/// Its pictures go with it. Chats' equipped sets are not scrubbed — the
/// reader drops the id and the next equip write prunes it.
pub fn delete_inventory_item(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    current: &InventoryRecord,
    force: bool,
) -> Result<DeletedInventoryItem, InventoryError> {
    let meta = MutationMeta::LocationItem {
        scope: location.scope,
        mount_point_id: &location.mount_point_id,
        item_id: current.id(),
        op: "delete",
    };
    location.writable_mount_point_id(main)?;

    let mut detached = 0usize;
    if current.is_archetype() {
        let dependents = find_dependents(main, mount, current.id())?;
        if !dependents.is_empty() {
            if !force {
                return Err(refuse(
                    InventoryRefusal::archetype_in_use(dependents.len(), &holders_of(&dependents)),
                    meta,
                ));
            }
            detached = detach_all(main, mount, current.id())?;
        }
    }

    // P4.D340 lint seam: door=delete — v4 item-mutations.ts:465 (`after: null`;
    // the rows written when something was deleted, :472).
    let id = current.id().to_string();
    let deleted = location.delete(main, mount, std::slice::from_ref(&id), |remaining| {
        remaining
            .iter()
            .filter(|r| {
                let members = r.item_ids();
                r.kind() == InventoryKind::Kit && members.contains(&id) && members.len() > 1
            })
            .map(|kit| {
                let mut patch = Map::new();
                let kept: Vec<String> = kit.item_ids().into_iter().filter(|m| *m != id).collect();
                patch.insert("itemIds".into(), json!(kept));
                InventoryRecordPatch {
                    id: kit.id().to_string(),
                    patch,
                    remove: Vec::new(),
                }
            })
            .collect()
    })?;
    if !deleted.is_empty() {
        super::item_images::cleanup_inventory_item_images(
            main,
            mount,
            &id,
            "[InventoryMutations]",
            location.scope,
            &location.mount_point_id,
        );
    }
    log_op!("Deleted object", location;
        itemId = %id,
        deleted = !deleted.is_empty(),
        forced = force,
        detached = detached,
    );
    Ok(DeletedInventoryItem {
        deleted: !deleted.is_empty(),
        detached,
    })
}

// ============================================================================
// Copies and filing
// ============================================================================

/// Refuse a copy of any object whose resolved `canCopy` is false (v4
/// `assertCopyable`).
pub fn assert_copyable(
    items: &[ResolvedInventoryItem],
    meta: MutationMeta<'_>,
) -> Result<(), InventoryError> {
    let blocked: Vec<String> = items
        .iter()
        .filter(|item| !item.can_copy())
        .map(|item| item.id().to_string())
        .collect();
    if blocked.is_empty() {
        Ok(())
    } else {
        Err(refuse(InventoryRefusal::not_copyable(&blocked), meta))
    }
}

/// The record a copy writes: a new id and timestamps, nothing archived, no
/// picture (v4 `copyRecord` — `{ ...source, id, folder, title?, archivedAt:
/// null, imageFileId: null, createdAt, updatedAt }`; `fileStem` rides along,
/// harmless).
fn copy_record(source: &InventoryRecord, folder: &str, title: Option<&str>) -> InventoryRecord {
    let now = now_iso();
    let mut copy = source.clone();
    copy.set("id", json!(mint_id()));
    copy.set("folder", json!(folder));
    if let Some(title) = title {
        copy.set("title", json!(title));
    }
    copy.set("archivedAt", Value::Null);
    copy.set("imageFileId", Value::Null);
    copy.set("createdAt", json!(now));
    copy.set("updatedAt", json!(now));
    copy
}

/// Duplicate one object beside itself, titled `(copy)`; refused when not
/// copyable (v4 `duplicateInventoryItem`).
pub fn duplicate_inventory_item(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    current: &InventoryRecord,
) -> Result<InventoryRecord, InventoryError> {
    let pool = load_pool_for_location(main, mount, location, LoadPoolContext::default())?;
    let resolved = pool.resolve(current);
    assert_copyable(
        std::slice::from_ref(&resolved),
        MutationMeta::ItemOp {
            item_id: current.id(),
            op: "duplicate",
        },
    )?;
    let titles: Vec<String> = location
        .read_items(mount, true)?
        .iter()
        .map(|r| r.title_or_name().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let title = crate::wardrobe::next_copy_title(resolved.title_or_name(), &titles);
    // `current.folder ?? ''`.
    let folder = current.str_of("folder").unwrap_or("").to_string();
    let copy = copy_record(current, &folder, Some(&title));
    // P4.D340 lint seam: door=create — v4 item-mutations.ts:515-517 (`{ pool }`).
    let stored = location.create(main, mount, &copy)?;
    log_op!("Duplicated object", location;
        itemId = %current.id(),
        copyId = %stored.id(),
    );
    Ok(stored)
}

/// File several objects of this location into one folder (one projection; v4
/// `moveInventoryItemsToFolder`). Every id must be here (`require_all`).
pub fn move_inventory_items_to_folder(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    item_ids: &[String],
    folder: &str,
) -> Result<Vec<InventoryRecord>, InventoryError> {
    let target = location.resolve_folder(mount, folder)?;
    let unique_ids = unique(item_ids);
    let patches: Vec<InventoryRecordPatch> = unique_ids
        .iter()
        .map(|id| {
            let mut patch = Map::new();
            patch.insert("folder".into(), json!(target));
            InventoryRecordPatch {
                id: id.clone(),
                patch,
                remove: Vec::new(),
            }
        })
        .collect();
    let moved = location.update_many(main, mount, &patches, true)?;
    log_op!("Moved objects to folder", location;
        folder = %target,
        requested = unique_ids.len(),
        moved = moved.len(),
    );
    Ok(moved)
}

/// Copy several objects of this location into one folder; any not copyable
/// refuses the batch (v4 `copyInventoryItemsToFolder`). A missing id is the
/// folder error (`NotInLocation` — a 400, not a refusal). Each copy is
/// created ONE AT A TIME (N projections, as v4's — observable in the
/// `doc_mount_documents` row ids). A copy keeps its source's title.
pub fn copy_inventory_items_to_folder(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    item_ids: &[String],
    folder: &str,
) -> Result<Vec<InventoryRecord>, InventoryError> {
    let target = location.resolve_folder(mount, folder)?;
    let records = location.read_items(mount, true)?;
    let unique_ids = unique(item_ids);
    let missing: Vec<String> = unique_ids
        .iter()
        .filter(|id| !records.iter().any(|r| r.id() == id.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(InventoryFolderError::NotInLocation(missing).into());
    }
    let pool = load_pool_for_location(main, mount, location, LoadPoolContext::default())?;
    let sources: Vec<&InventoryRecord> = unique_ids
        .iter()
        .filter_map(|id| records.iter().find(|r| r.id() == id.as_str()))
        .collect();
    let resolved: Vec<ResolvedInventoryItem> = sources.iter().map(|r| pool.resolve(r)).collect();
    assert_copyable(
        &resolved,
        MutationMeta::OpFolder {
            op: "copy-items",
            folder: &target,
        },
    )?;
    let mut copies = Vec::new();
    for source in sources {
        let copy = copy_record(source, &target, None);
        // P4.D340 lint seam: door=create — v4 item-mutations.ts:554-556 (per
        // source, `{ pool }`; rows after each create).
        copies.push(location.create(main, mount, &copy)?);
    }
    log_op!("Copied objects to folder", location;
        folder = %target,
        count = copies.len(),
    );
    Ok(copies)
}

// ============================================================================
// Folders — the operator's filing under `Inventory/`
// ============================================================================

/// `path.trim().replace(/^\/+|\/+$/g, '')`.
fn strip_slashes(path: &str) -> String {
    crate::jsstr::js_trim(path).trim_matches('/').to_string()
}

/// Create a folder at a location (an existing one, in any casing, is
/// answered) (v4 `createInventoryFolder`).
pub fn create_inventory_folder(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    path: &str,
) -> Result<InventoryFolder, InventoryError> {
    let resolved = location.resolve_folder(mount, path)?;
    if resolved.is_empty() {
        return Err(InventoryFolderError::Path("A folder needs a name".to_string()).into());
    }
    let folder = location.create_folder(main, mount, &resolved)?;
    log_op!("Created folder", location;
        requested = %path,
        folder = %folder.path,
    );
    Ok(folder)
}

/// Rename a folder in place; its objects follow because the path is the
/// truth (v4 `renameInventoryFolder`).
pub fn rename_inventory_folder(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    path: &str,
    name: &str,
) -> Result<InventoryFolder, InventoryError> {
    let from = strip_slashes(path);
    let new_name = normalize_inventory_folder_name(Some(name))?;
    let folder = location.rename_folder(main, mount, &from, &new_name)?;
    log_op!("Renamed folder", location;
        from = %from,
        to = %folder.path,
    );
    Ok(folder)
}

/// v4 `deleteInventoryFolder`'s `{ movedItems, movedFolders }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletedInventoryFolder {
    pub moved_items: usize,
    pub moved_folders: usize,
}

/// Delete a folder, sending its objects and subfolders up to its parent (v4
/// `deleteInventoryFolder`).
pub fn delete_inventory_folder(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    path: &str,
) -> Result<DeletedInventoryFolder, InventoryError> {
    let folder = strip_slashes(path);
    let deleted = location.delete_folder(main, mount, &folder)?;
    log_op!("Deleted folder", location;
        folder = %folder,
        parent = %deleted.parent,
        movedItems = deleted.moved_items,
        movedFolders = deleted.moved_folders,
    );
    Ok(DeletedInventoryFolder {
        moved_items: deleted.moved_items,
        moved_folders: deleted.moved_folders,
    })
}
