//! The generic **document-store overlay engine** — `createDocumentStoreOverlay`
//! from v4's `lib/database/document-store-overlay.ts`, ported as a Rust generic
//! over a [`StoreEntity`].
//!
//! The project store and the group store are the same machine: a slim DB row
//! whose substantive content (description, instructions, state, and a typed JSON
//! property bag) actually lives in the entity's official document store, behind
//! **four overlay files** (`properties.json`, `description.md`, `instructions.md`,
//! `state.json`). This module is the single implementation of that machine;
//! `groups` instantiates it now (the pilot), `projects` will reuse it (the file
//! names and the read/write logic are identical — only the property bag differs).
//!
//! ## Where the bytes live, and how a field is split
//!
//! The split is **authoritative-per-field, not merge-from-one**: the slim row
//! (main DB) owns `id` / `name` / `officialMountPointId` / timestamps; the store
//! (mount-index DB) owns every *managed* field, projected into the four files.
//! Reads overlay the store onto the row; writes route the store-resident fields
//! to the files and strip them from the DB-bound patch. The byte-landing path
//! every file write ultimately calls is [`super::doc_mount_file_links`]'s
//! `write_database_document` (build step 1 of this slice); the read path is the
//! `doc_mount_documents` 3-table join (`find_many_by_mount_points_and_path`).
//!
//! ## Failure is asymmetric (a deliberate v4 divergence)
//!
//!   - [`apply_overlay_one`] (single, behind `findById`) **throws**
//!     [`OverlayError::Unavailable`] — the caller asked for that one entity, so
//!     fail loudly.
//!   - [`apply_overlay`] (batched, behind `findAll`) **drops** the offending row
//!     (a `Db` error still propagates) so one corrupt store can't take down the
//!     whole list. The keystone invariant is `properties.json` + a non-null
//!     `officialMountPointId`; a missing/unparseable `state.json` is non-fatal
//!     (`{}`), and an empty `description.md`/`instructions.md` hydrates to `null`.
//!
//! ## What is NOT ported here (vs v4)
//!
//! v4 serializes per-mount-point writes through a promise chain (`runOnChain`) —
//! a Node-concurrency workaround. The single-writer Rust model is inherently
//! serialized (one owned connection, one mutator), so that machinery is dropped:
//! correctness, not a Node workaround. `readDatabaseDocument`'s mtime/size are
//! not surfaced (the overlay only needs `content`).

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::DbError;

/// The four overlay file paths inside a store-backed entity's official store.
/// Identical for every store-backed entity (groups, projects). Path lookups are
/// case-insensitive in the storage layer, so `Description.md` resolves too.
pub const PROPERTIES_JSON_PATH: &str = "properties.json";
pub const DESCRIPTION_MD_PATH: &str = "description.md";
pub const INSTRUCTIONS_MD_PATH: &str = "instructions.md";
pub const STATE_JSON_PATH: &str = "state.json";

/// The four single-file overlay paths, in stable order (the read overlay runs
/// one batched join query per entry).
pub const ALL_OVERLAY_PATHS: [&str; 4] = [
    PROPERTIES_JSON_PATH,
    DESCRIPTION_MD_PATH,
    INSTRUCTIONS_MD_PATH,
    STATE_JSON_PATH,
];

/// The store-fixed managed fields (every store-backed entity routes these to the
/// store). The entity's [`StoreEntity::property_keys`] are also managed; the
/// full managed set the write overlay strips is this plus those.
const FIXED_MANAGED_FIELDS: [&str; 3] = ["description", "instructions", "state"];

/// Per-entity wiring that specializes the generic overlay machine. The four file
/// paths and the read/write logic are shared; only the typed property bag and
/// the labels differ between entities.
pub trait StoreEntity {
    /// The typed property bag persisted as `properties.json`. MUST serialize its
    /// fields in schema-declaration order (a serde struct, **not**
    /// `serde_json::Value`, which would sort keys) so the stored bytes — which
    /// feed the content-dedup sha — match v4's `JSON.stringify(parse(x), null, 2)`
    /// byte-for-byte. Optional fields use `skip_serializing_if` so an absent key
    /// stays absent (Zod `.optional()`).
    type Properties: Serialize + DeserializeOwned;

    /// Lowercase singular label for the unavailability error, e.g. `"group"`.
    fn entity_label() -> &'static str;

    /// The property-bag key names (schema-derived) — used to detect which keys a
    /// write patch touches and to strip them from the DB-bound remainder.
    fn property_keys() -> &'static [&'static str];

    /// Parse + validate a raw JSON value into the typed bag (mirrors Zod
    /// `.parse`: unknown keys stripped). `Err` carries the validation message,
    /// surfaced as the `Unavailable` detail.
    fn parse_properties(value: &Value) -> Result<Self::Properties, String>;

    // ── store-backed repository / provisioning wiring ──────────────────────
    // The slim-row table, the auto-created store's name prefix, and the
    // entity↔store link operations are the only things that differ between the
    // store-backed entities (groups, projects). The generic
    // [`super::store_backed::StoreBackedRepository`] + provisioning use these.

    /// The slim DB table name in the MAIN db, e.g. `"groups"` / `"projects"`.
    fn slim_table() -> &'static str;

    /// Name prefix for a freshly-minted official store, e.g. `"Group Files: "`.
    fn store_name_prefix() -> &'static str;

    /// All mount-point ids linked to this entity (the entity↔store link table in
    /// the MOUNT-INDEX db). Provisioning's adopt branch consults it.
    fn find_store_links(mount: &Connection, entity_id: &str) -> Result<Vec<String>, DbError>;

    /// Link a freshly created store to the entity (find-or-create, idempotent).
    fn link_store(mount: &Connection, entity_id: &str, mount_point_id: &str)
        -> Result<(), DbError>;
}

/// The error the overlay raises. `Unavailable` is the keystone-broken signal
/// (null/unreadable mount or missing/unparseable `properties.json`); `Db` wraps a
/// real SQLite failure (which is never swallowed by the list path).
#[derive(Debug)]
pub enum OverlayError {
    Unavailable {
        entity_label: &'static str,
        id: String,
        mount_point_id: Option<String>,
        detail: String,
    },
    Db(DbError),
}

impl OverlayError {
    fn unavailable<E: StoreEntity>(
        id: &str,
        mount_point_id: Option<&str>,
        detail: impl Into<String>,
    ) -> Self {
        OverlayError::Unavailable {
            entity_label: E::entity_label(),
            id: id.to_string(),
            mount_point_id: mount_point_id.map(str::to_string),
            detail: detail.into(),
        }
    }

    /// True for the keystone-broken signal (the list path drops these; a `Db`
    /// error it re-raises).
    pub fn is_unavailable(&self) -> bool {
        matches!(self, OverlayError::Unavailable { .. })
    }
}

impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverlayError::Unavailable {
                entity_label,
                id,
                mount_point_id,
                detail,
            } => write!(
                f,
                "{} {} has no usable document store (officialMountPointId={}): {}",
                entity_label,
                id,
                mount_point_id.as_deref().unwrap_or("null"),
                detail
            ),
            OverlayError::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OverlayError {}

impl From<DbError> for OverlayError {
    fn from(e: DbError) -> Self {
        OverlayError::Db(e)
    }
}

/// path → (mountPointId → file content).
type ContentByPath = HashMap<&'static str, HashMap<String, String>>;

/// Empty markdown file → `null` so nullable fields keep their unset semantics
/// (v4 `markdownToNullable`).
fn markdown_to_nullable(content: &str) -> Value {
    if content.is_empty() {
        Value::Null
    } else {
        Value::String(content.to_string())
    }
}

/// The store-resident fields written to / read from the four files on create and
/// full-entity writes (`writeManagedFields`). `properties` is any JSON object
/// carrying the property-bag keys (the engine re-parses it through
/// [`StoreEntity::parse_properties`], which strips extras); `state` is arbitrary
/// JSON (`null` → `{}`).
pub struct ManagedFields {
    pub properties: Value,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub state: Value,
}

/// Load the four overlay files for every given mount point in one batched query
/// per path (v4 `loadStoreFiles`). Returns path → (mountPointId → content).
fn load_store_files(
    mount: &Connection,
    mount_point_ids: &[String],
) -> Result<ContentByPath, DbError> {
    let mut by_path: ContentByPath = HashMap::new();
    for path in ALL_OVERLAY_PATHS {
        by_path.insert(path, HashMap::new());
    }
    if mount_point_ids.is_empty() {
        return Ok(by_path);
    }
    let docs = DocMountDocumentsRepository::new(mount);
    for path in ALL_OVERLAY_PATHS {
        let pairs = docs.find_many_by_mount_points_and_path(mount_point_ids, path)?;
        let by_mount = by_path.get_mut(path).expect("path seeded above");
        for (mount_point_id, content) in pairs {
            by_mount.insert(mount_point_id, content);
        }
    }
    Ok(by_path)
}

/// Hydrate one slim row into the app-facing entity by overlaying its store files
/// (v4 `hydrateOne`). `{ ...row, ...properties, description, instructions, state }`
/// — the store overrides the row on overlap. Throws `Unavailable` when the mount
/// is null or `properties.json` is missing/unparseable.
fn hydrate_one<E: StoreEntity>(
    row: &Map<String, Value>,
    by_path: &ContentByPath,
) -> Result<Value, OverlayError> {
    let id = row.get("id").and_then(Value::as_str).unwrap_or("");
    let mount_id = match row.get("officialMountPointId").and_then(Value::as_str) {
        Some(m) => m,
        None => {
            return Err(OverlayError::unavailable::<E>(
                id,
                None,
                "officialMountPointId is null",
            ))
        }
    };

    let props_raw = match by_path
        .get(PROPERTIES_JSON_PATH)
        .and_then(|m| m.get(mount_id))
    {
        Some(s) => s,
        None => {
            return Err(OverlayError::unavailable::<E>(
                id,
                Some(mount_id),
                "properties.json missing",
            ))
        }
    };
    let props_value: Value = serde_json::from_str(props_raw).map_err(|e| {
        OverlayError::unavailable::<E>(
            id,
            Some(mount_id),
            format!("properties.json unparseable: {e}"),
        )
    })?;
    let properties = E::parse_properties(&props_value).map_err(|detail| {
        OverlayError::unavailable::<E>(
            id,
            Some(mount_id),
            format!("properties.json unparseable: {detail}"),
        )
    })?;

    let desc = by_path
        .get(DESCRIPTION_MD_PATH)
        .and_then(|m| m.get(mount_id));
    let instr = by_path
        .get(INSTRUCTIONS_MD_PATH)
        .and_then(|m| m.get(mount_id));
    let state_raw = by_path.get(STATE_JSON_PATH).and_then(|m| m.get(mount_id));

    let state = match state_raw {
        // A corrupt state.json is non-fatal — `{}` (not the keystone invariant).
        Some(s) => serde_json::from_str::<Value>(s)
            .ok()
            .filter(|v| !v.is_null())
            .unwrap_or_else(|| Value::Object(Map::new())),
        None => Value::Object(Map::new()),
    };

    let mut out = row.clone();
    // Spread the typed property bag over the row.
    if let Value::Object(prop_map) = serde_json::to_value(&properties)
        .map_err(|e| OverlayError::Db(DbError::Key(format!("properties serialize: {e}"))))?
    {
        for (k, v) in prop_map {
            out.insert(k, v);
        }
    }
    out.insert(
        "description".into(),
        desc.map(|s| markdown_to_nullable(s)).unwrap_or(Value::Null),
    );
    out.insert(
        "instructions".into(),
        instr
            .map(|s| markdown_to_nullable(s))
            .unwrap_or(Value::Null),
    );
    out.insert("state".into(), state);
    Ok(Value::Object(out))
}

/// Find-by-id overlay (single): hydrate or **throw** (v4 `applyOverlayOne`).
pub fn apply_overlay_one<E: StoreEntity>(
    mount: &Connection,
    row: Option<Map<String, Value>>,
) -> Result<Option<Value>, OverlayError> {
    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };
    let mount_id = match row.get("officialMountPointId").and_then(Value::as_str) {
        Some(m) => m.to_string(),
        None => {
            let id = row.get("id").and_then(Value::as_str).unwrap_or("");
            return Err(OverlayError::unavailable::<E>(
                id,
                None,
                "officialMountPointId is null",
            ));
        }
    };
    let by_path = load_store_files(mount, std::slice::from_ref(&mount_id))?;
    Ok(Some(hydrate_one::<E>(&row, &by_path)?))
}

/// Find-all overlay (batched): hydrate each, **dropping** rows whose store is
/// unavailable (v4 `applyOverlay`). A real `Db` error still propagates.
pub fn apply_overlay<E: StoreEntity>(
    mount: &Connection,
    rows: Vec<Map<String, Value>>,
) -> Result<Vec<Value>, OverlayError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let mount_point_ids: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        let mut ids = Vec::new();
        for r in &rows {
            if let Some(m) = r.get("officialMountPointId").and_then(Value::as_str) {
                if seen.insert(m.to_string()) {
                    ids.push(m.to_string());
                }
            }
        }
        ids
    };
    let by_path = load_store_files(mount, &mount_point_ids)?;

    let mut out = Vec::new();
    for row in &rows {
        match hydrate_one::<E>(row, &by_path) {
            Ok(v) => out.push(v),
            Err(e) if e.is_unavailable() => continue, // drop the bad row
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

/// Read + parse `properties.json` for one mount point, or `None` on any failure
/// (v4 `readProperties` — the read-modify-write seed). Non-throwing by design.
pub fn read_properties<E: StoreEntity>(
    mount: &Connection,
    mount_point_id: &str,
) -> Result<Option<E::Properties>, DbError> {
    let content = DocMountDocumentsRepository::new(mount)
        .find_by_mount_point_and_path(mount_point_id, PROPERTIES_JSON_PATH)?;
    let Some(content) = content else {
        return Ok(None);
    };
    let value: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    Ok(E::parse_properties(&value).ok())
}

/// Serialize the property bag the way v4 does: `JSON.stringify(parse(x), null, 2)`
/// — re-parse through the typed struct (key order + strip extras), 2-space pretty.
fn serialize_properties<E: StoreEntity>(value: &Value) -> Result<String, OverlayError> {
    let props = E::parse_properties(value)
        .map_err(|d| OverlayError::Db(DbError::Key(format!("properties parse: {d}"))))?;
    serde_json::to_string_pretty(&props)
        .map_err(|e| OverlayError::Db(DbError::Key(format!("properties serialize: {e}"))))
}

/// `JSON.stringify(state ?? {}, null, 2)` — `null` → `{}`, else 2-space pretty.
fn serialize_state(state: &Value) -> Result<String, OverlayError> {
    if state.is_null() {
        return Ok("{}".to_string());
    }
    serde_json::to_string_pretty(state)
        .map_err(|e| OverlayError::Db(DbError::Key(format!("state serialize: {e}"))))
}

/// Write all four overlay files from an in-memory entity (v4 `writeManagedFields`)
/// — the create path and the startup backfill use it. `description`/`instructions`
/// `None` → empty file (which hydrates back to `null`).
pub fn write_managed_fields<E: StoreEntity>(
    mount: &Connection,
    mount_point_id: &str,
    fields: &ManagedFields,
) -> Result<(), OverlayError> {
    let links = DocMountFileLinksRepository::new(mount);
    let props_json = serialize_properties::<E>(&fields.properties)?;
    links.write_database_document(mount_point_id, PROPERTIES_JSON_PATH, &props_json)?;
    links.write_database_document(
        mount_point_id,
        DESCRIPTION_MD_PATH,
        fields.description.as_deref().unwrap_or(""),
    )?;
    links.write_database_document(
        mount_point_id,
        INSTRUCTIONS_MD_PATH,
        fields.instructions.as_deref().unwrap_or(""),
    )?;
    let state_json = serialize_state(&fields.state)?;
    links.write_database_document(mount_point_id, STATE_JSON_PATH, &state_json)?;
    Ok(())
}

/// Route store-resident patch fields to the store and return the DB-only
/// remainder (v4 `applyWriteOverlay`). Runs **before** the slim-row write. The
/// `patch` is the partial entity as a JSON map; the returned map is the patch
/// with every managed key stripped (so it never references a store-resident
/// column). When the remainder is empty the caller skips the SQL `_update`.
///
/// Property writes are read-modify-write (a partial patch must not clobber
/// unspecified keys), seeded from the existing `properties.json` or — when none
/// exists yet — from `parse(entity)` (`{}` for a slim row).
pub fn apply_write_overlay<E: StoreEntity>(
    mount: &Connection,
    raw_row: Option<&Map<String, Value>>,
    patch: &Map<String, Value>,
) -> Result<Map<String, Value>, OverlayError> {
    let entity = match raw_row {
        Some(r) => r,
        // Caller will hit the same not-found in `_update`; let it surface there.
        None => return Ok(patch.clone()),
    };
    let mut db_patch = patch.clone();

    let touched_props: Vec<&str> = E::property_keys()
        .iter()
        .copied()
        .filter(|k| patch.contains_key(*k))
        .collect();
    let touches_description = patch.contains_key("description");
    let touches_instructions = patch.contains_key("instructions");
    let touches_state = patch.contains_key("state");
    let touches_store =
        !touched_props.is_empty() || touches_description || touches_instructions || touches_state;

    if touches_store {
        let id = entity.get("id").and_then(Value::as_str).unwrap_or("");
        let mount_point_id = entity
            .get("officialMountPointId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                OverlayError::unavailable::<E>(
                    id,
                    None,
                    "write attempted with null officialMountPointId",
                )
            })?;
        let links = DocMountFileLinksRepository::new(mount);

        if touches_description {
            let value = patch["description"].as_str().unwrap_or("");
            links.write_database_document(mount_point_id, DESCRIPTION_MD_PATH, value)?;
        }
        if touches_instructions {
            let value = patch["instructions"].as_str().unwrap_or("");
            links.write_database_document(mount_point_id, INSTRUCTIONS_MD_PATH, value)?;
        }
        if touches_state {
            let value = serialize_state(&patch["state"])?;
            links.write_database_document(mount_point_id, STATE_JSON_PATH, &value)?;
        }
        if !touched_props.is_empty() {
            // Seed from the existing file, else from parse(entity) (= `{}`).
            let seed: Value = match read_properties::<E>(mount, mount_point_id)? {
                Some(p) => serde_json::to_value(&p)
                    .map_err(|e| OverlayError::Db(DbError::Key(format!("props seed: {e}"))))?,
                None => {
                    let entity_value = Value::Object(entity.clone());
                    let parsed = E::parse_properties(&entity_value).map_err(|d| {
                        OverlayError::Db(DbError::Key(format!("props seed parse: {d}")))
                    })?;
                    serde_json::to_value(&parsed)
                        .map_err(|e| OverlayError::Db(DbError::Key(format!("props seed: {e}"))))?
                }
            };
            let mut next = seed.as_object().cloned().unwrap_or_default();
            for k in &touched_props {
                next.insert((*k).to_string(), patch[*k].clone());
            }
            let value = serialize_properties::<E>(&Value::Object(next))?;
            links.write_database_document(mount_point_id, PROPERTIES_JSON_PATH, &value)?;
        }
    }

    for k in E::property_keys() {
        db_patch.remove(*k);
    }
    for k in FIXED_MANAGED_FIELDS {
        db_patch.remove(k);
    }
    Ok(db_patch)
}
