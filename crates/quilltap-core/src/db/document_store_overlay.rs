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
//! `doc_mount_documents` 3-table join (`find_many_by_mount_points_and_path_or_empty`
//! — v4's fallback batch read, P4.142).
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
//!   - [`read_properties`] returns `None` ONLY for a genuinely absent
//!     `properties.json`, and ERRORS when the file exists but is unreadable or
//!     unparseable. Its caller treats `None` as "seed from the raw row", and
//!     post-cutover that row holds no property values — so a lenient read
//!     silently resets the settings bag to schema defaults (v4 `dcd9440a`).
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
    /// fields in schema-declaration order — a serde struct, so that order is
    /// declared and reviewable — because the stored bytes feed the content-dedup
    /// sha and must match v4's `JSON.stringify(parse(x), null, 2)` byte-for-byte.
    /// A Zod `.nullable().optional()` field is three-state — `Option<Option<T>>`
    /// with `deserialize_with = "double_option"` and `skip_serializing_if` — so an
    /// absent key stays absent AND an explicit `null` stays `null`. That one
    /// typing is what carries v4's nulls through this engine: the hydrated read
    /// spreads `to_value(&properties)`, and the write overlay seeds from it and
    /// re-serializes through it, so a plain `Option<T>` dropped a stored `null`
    /// on every read and on every read-modify-write (dogfood #136).
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

    /// Collapse into the [`DbError`] surface for callers whose error type is
    /// `DbError` (the closure plumbing). The `Unavailable` arm survives
    /// STRUCTURALLY as [`DbError::StoreUnavailable`] (P4.23), so the api layer
    /// can answer v4's contextful 503 — its Display is this error's full
    /// message, so surfaces that only Display lose nothing.
    pub fn into_db(self) -> DbError {
        let message = self.to_string();
        match self {
            OverlayError::Db(d) => d,
            OverlayError::Unavailable {
                entity_label, id, ..
            } => DbError::StoreUnavailable {
                entity_label,
                id,
                message,
            },
        }
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
            } => {
                // The character vault carries a DISTINCT message wording from the
                // project/group document stores: v4's `CharacterVaultUnavailableError`
                // reads "Character {id} has no usable **vault**
                // (**characterDocumentMountPointId**=…)", where
                // `Project`/`GroupStoreUnavailableError` read "has no usable
                // document store (officialMountPointId=…)". v5 raises one
                // `OverlayError::Unavailable` for all three, so match v4 at the
                // one site whose bytes are the user-facing error (and, via
                // `into_db`, the P4.23 503 body). The character write path
                // (`vault_character_update::read_current_properties`) is the only
                // consumer with `entity_label == "character"` — bug 8's #47
                // convergence made this reachable, so the byte parity now matters.
                if *entity_label == "character" {
                    return write!(
                        f,
                        "Character {} has no usable vault (characterDocumentMountPointId={}): {}",
                        id,
                        mount_point_id.as_deref().unwrap_or("null"),
                        detail
                    );
                }
                // v4 renders the label CAPITALIZED here ("Project …"/"Group …")
                // — its overlay config carries both `entityLabel` and
                // `entityLabelCapitalized`, and the message is built from the
                // latter. v5 keeps `entity_label()` lowercase (the other
                // sentences that use it read as prose), so capitalize at the one
                // site whose bytes must match v4's `error.message`.
                let (head, tail) = entity_label.split_at(1);
                write!(
                    f,
                    "{}{} {} has no usable document store (officialMountPointId={}): {}",
                    head.to_uppercase(),
                    tail,
                    id,
                    mount_point_id.as_deref().unwrap_or("null"),
                    detail
                )
            }
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
        let pairs = docs.find_many_by_mount_points_and_path_or_empty(mount_point_ids, path)?;
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
        // v4 WARNs on the parse failure only (`document-store-overlay.ts:179-
        // 184`); a `null` body defaults silently (`JSON.parse(raw) ?? {}`).
        Some(s) => match serde_json::from_str::<Value>(s) {
            Ok(v) if !v.is_null() => v,
            Ok(_) => Value::Object(Map::new()),
            Err(_) => {
                log_state_unparseable::<E>(id, mount_id);
                Value::Object(Map::new())
            }
        },
        None => Value::Object(Map::new()),
    };

    let mut out = row.clone();
    // Spread the typed property bag over the row.
    if let Value::Object(prop_map) = serde_json::to_value(&properties)
        .map_err(|e| OverlayError::Db(DbError::Internal(format!("properties serialize: {e}"))))?
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
    let mut dropped = 0usize;
    for row in &rows {
        match hydrate_one::<E>(row, &by_path) {
            Ok(v) => out.push(v),
            // Drop the bad row — with v4's per-row ERROR (P4.142: v5 dropped
            // SILENTLY, the divergence P4.D172 closed for the character overlay).
            Err(e) if e.is_unavailable() => {
                dropped += 1;
                log_store_drop::<E>(row, &e);
            }
            Err(e) => return Err(e),
        }
    }
    if dropped > 0 {
        log_store_drop_summary::<E>(dropped, rows.len());
    }
    Ok(out)
}

// ── v4's overlay lines (`document-store-overlay.ts:179-184,210-229`). v4 builds
// them from the config's `entityLabel` / `entityLabelCapitalized` / `idLogKey`
// (`projectId` / `groupId` — `lib/{projects/project,groups/group}-store/
// overlay.ts:29-31`). A tracing field NAME must be static, so each line is
// spelled once per store-backed entity, keyed on [`StoreEntity::entity_label`]
// (the only two implementors). ──

/// v4 ERROR `` `Dropping ${label} from list — document store unavailable` ``
/// `{[idLogKey]: row.id, officialMountPointId: row.officialMountPointId ?? null,
/// reason: err.message}` — `reason` is the unavailable error's message, which
/// [`OverlayError`]'s `Display` renders byte-for-byte as v4's.
fn log_store_drop<E: StoreEntity>(row: &Map<String, Value>, err: &OverlayError) {
    let id = row.get("id").and_then(Value::as_str).unwrap_or("");
    let mount = row
        .get("officialMountPointId")
        .and_then(Value::as_str)
        .unwrap_or("null");
    match E::entity_label() {
        "project" => tracing::error!(
            projectId = %id,
            officialMountPointId = %mount,
            reason = %err,
            "Dropping project from list — document store unavailable"
        ),
        "group" => tracing::error!(
            groupId = %id,
            officialMountPointId = %mount,
            reason = %err,
            "Dropping group from list — document store unavailable"
        ),
        label => tracing::error!(
            entityLabel = label,
            id = %id,
            officialMountPointId = %mount,
            reason = %err,
            "Dropping row from list — document store unavailable"
        ),
    }
}

/// v4 WARN `` `apply${Label}StoreOverlay dropped ${label}s with unavailable
/// stores` `` `{dropped, of}`.
fn log_store_drop_summary<E: StoreEntity>(dropped: usize, of: usize) {
    match E::entity_label() {
        "project" => tracing::warn!(
            dropped,
            of,
            "applyProjectStoreOverlay dropped projects with unavailable stores"
        ),
        "group" => tracing::warn!(
            dropped,
            of,
            "applyGroupStoreOverlay dropped groups with unavailable stores"
        ),
        label => tracing::warn!(
            entityLabel = label,
            dropped,
            of,
            "applyStoreOverlay dropped rows with unavailable stores"
        ),
    }
}

/// v4 WARN `` `${Label} state.json unparseable; defaulting to {}` ``
/// `{[idLogKey]: row.id, officialMountPointId: mountId}`.
fn log_state_unparseable<E: StoreEntity>(id: &str, mount_id: &str) {
    match E::entity_label() {
        "project" => tracing::warn!(
            projectId = %id,
            officialMountPointId = %mount_id,
            "Project state.json unparseable; defaulting to {{}}"
        ),
        "group" => tracing::warn!(
            groupId = %id,
            officialMountPointId = %mount_id,
            "Group state.json unparseable; defaulting to {{}}"
        ),
        label => tracing::warn!(
            entityLabel = label,
            id = %id,
            officialMountPointId = %mount_id,
            "state.json unparseable; defaulting to {{}}"
        ),
    }
}

/// Read + validate `properties.json` for one mount point (v4 `readProperties` —
/// the read-modify-write seed).
///
/// Returns `None` **only** when `properties.json` is genuinely absent from the
/// store. Every other failure — an unreadable mount index, a transient
/// repository error, malformed JSON, a body the schema rejects — is an error:
/// the finder's [`DbError`] propagates as [`OverlayError::Db`] (v4's
/// "unreadable" arm), and the two parse failures raise
/// [`OverlayError::Unavailable`].
///
/// That distinction is load-bearing, not pedantry (v4 `dcd9440a`). Callers read
/// `None` as "nothing persisted yet, seed from the raw row", and post-cutover
/// the slim DB row carries no property values at all. Collapsing a *failed* read
/// into `None` therefore resets the entire settings bag to schema defaults — and
/// because the no-default optionals then serialize to nothing, the loss is
/// invisible in the file and compounds through every later write.
///
/// `entity_id` is for the error message only, so a failure names the
/// project/group rather than just its mount point. (v4 defaults it to
/// `'(unknown)'`; every v5 call site — like every post-`dcd9440a` v4 one — has
/// the id, so the parameter is required here rather than adding an unreachable
/// branch.)
///
/// The v4↔v5 arm mapping: v5's finder returns `Ok(None)` where v4's
/// `readDatabaseDocument` throws `DatabaseStoreError { code: 'NOT_FOUND' }`, so
/// `Ok(None)` ≡ v4's NOT_FOUND arm and `Err(DbError)` ≡ v4's every-other-read-
/// error arm.
pub fn read_properties<E: StoreEntity>(
    mount: &Connection,
    mount_point_id: &str,
    entity_id: &str,
) -> Result<Option<E::Properties>, OverlayError> {
    let content = DocMountDocumentsRepository::new(mount)
        .find_by_mount_point_and_path(mount_point_id, PROPERTIES_JSON_PATH)?;
    let Some(content) = content else {
        tracing::debug!(
            entity = E::entity_label(),
            entity_id,
            official_mount_point_id = mount_point_id,
            "{PROPERTIES_JSON_PATH} absent — caller may seed defaults"
        );
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&content).map_err(|e| {
        tracing::error!(
            entity = E::entity_label(),
            entity_id,
            official_mount_point_id = mount_point_id,
            reason = %e,
            "{PROPERTIES_JSON_PATH} unparseable — refusing to treat as absent"
        );
        OverlayError::unavailable::<E>(
            entity_id,
            Some(mount_point_id),
            format!("{PROPERTIES_JSON_PATH} unparseable: {e}"),
        )
    })?;
    let parsed = E::parse_properties(&value).map_err(|detail| {
        tracing::error!(
            entity = E::entity_label(),
            entity_id,
            official_mount_point_id = mount_point_id,
            reason = %detail,
            "{PROPERTIES_JSON_PATH} unparseable — refusing to treat as absent"
        );
        OverlayError::unavailable::<E>(
            entity_id,
            Some(mount_point_id),
            format!("{PROPERTIES_JSON_PATH} unparseable: {detail}"),
        )
    })?;
    Ok(Some(parsed))
}

/// Serialize the property bag the way v4 does: `JSON.stringify(parse(x), null, 2)`
/// — re-parse through the typed struct (key order + strip extras), 2-space pretty.
///
/// [P4.148] A refusal is the parse's own message, bare — v4's
/// `JSON.stringify(config.parseProperties(x), null, 2)` throws the ZodError
/// itself (`writeManagedFields` on create, the RMW write on update), so its
/// `.message` IS `JSON.stringify(issues, null, 2)` with no prefix (measured at
/// `07b8f0209`: the tier-2 `updateExpectError` arms on a `color: "red"`
/// patch). v5 used to prepend `properties parse: `.
fn serialize_properties<E: StoreEntity>(value: &Value) -> Result<String, OverlayError> {
    let props = E::parse_properties(value).map_err(|d| OverlayError::Db(DbError::Internal(d)))?;
    serde_json::to_string_pretty(&props)
        .map_err(|e| OverlayError::Db(DbError::Internal(format!("properties serialize: {e}"))))
}

/// `JSON.stringify(state ?? {}, null, 2)` — `null` → `{}`, else 2-space pretty.
fn serialize_state(state: &Value) -> Result<String, OverlayError> {
    if state.is_null() {
        return Ok("{}".to_string());
    }
    serde_json::to_string_pretty(state)
        .map_err(|e| OverlayError::Db(DbError::Internal(format!("state serialize: {e}"))))
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
            // Read-modify-write so a partial patch doesn't blow away unspecified
            // keys. `read_properties` errors rather than returning `None` when
            // the file exists but can't be read or parsed, so the
            // seed-from-entity fallback fires only for a store that has no
            // `properties.json` at all — a broken invariant the backfill heals,
            // where there is nothing to preserve anyway. (Seeding from a raw row
            // on a *failed* read is what silently reset a project's whole
            // settings bag to defaults — v4 `dcd9440a`.)
            let seed: Value = match read_properties::<E>(mount, mount_point_id, id)? {
                Some(p) => serde_json::to_value(&p)
                    .map_err(|e| OverlayError::Db(DbError::Internal(format!("props seed: {e}"))))?,
                None => {
                    let entity_value = Value::Object(entity.clone());
                    // [P4.148] Bare, as v4's `config.parseProperties(entity)`
                    // throws it (see `serialize_properties`).
                    let parsed = E::parse_properties(&entity_value)
                        .map_err(|d| OverlayError::Db(DbError::Internal(d)))?;
                    serde_json::to_value(&parsed).map_err(|e| {
                        OverlayError::Db(DbError::Internal(format!("props seed: {e}")))
                    })?
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

#[cfg(test)]
mod drop_line_tests {
    use super::*;
    use crate::db::groups::GroupEntity;
    use crate::db::projects::ProjectEntity;

    /// The three tables the batch read joins, in the shape its SELECT names, with
    /// one healthy store `mp-ok` (a bare-object `properties.json`) and one whose
    /// `state.json` does not parse (`mp-badstate`).
    fn mount() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_file_links (id TEXT, mountPointId TEXT, relativePath TEXT, fileId TEXT);
             CREATE TABLE doc_mount_documents (fileId TEXT, content TEXT);
             CREATE TABLE doc_mount_files (id TEXT);",
        )
        .unwrap();
        for (i, (mp, path, content)) in [
            ("mp-ok", "properties.json", "{}"),
            ("mp-badstate", "properties.json", "{}"),
            ("mp-badstate", "state.json", "{not json"),
        ]
        .iter()
        .enumerate()
        {
            conn.execute(
                "INSERT INTO doc_mount_file_links VALUES (?1, ?2, ?3, ?1)",
                rusqlite::params![format!("f{i}"), mp, path],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_documents VALUES (?1, ?2)",
                rusqlite::params![format!("f{i}"), content],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_files VALUES (?1)",
                rusqlite::params![format!("f{i}")],
            )
            .unwrap();
        }
        conn
    }

    fn row(id: &str, mount: Option<&str>) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("id".into(), Value::String(id.into()));
        m.insert(
            "officialMountPointId".into(),
            mount.map_or(Value::Null, |s| Value::String(s.into())),
        );
        m
    }

    /// P4.142 — v4's per-row ERROR (`[idLogKey]`, `officialMountPointId ?? null`,
    /// `reason` = the unavailable error's message) and the summary WARN `{dropped,
    /// of}` (`document-store-overlay.ts:210-229`), for both store-backed
    /// entities; a healthy row hydrates and logs nothing of its own.
    #[test]
    fn a_dropped_store_logs_v4s_error_and_summary_warn() {
        let conn = mount();
        let (got, lines) = crate::test_support::captured_with(|| {
            apply_overlay::<ProjectEntity>(
                &conn,
                vec![
                    row("p-ok", Some("mp-ok")),
                    row("p-gone", Some("mp-gone")),
                    row("p-null", None),
                ],
            )
        });
        let got = got.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["id"], "p-ok");
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap_core::db::document_store_overlay Dropping project from list — document store unavailable projectId=p-gone officialMountPointId=mp-gone reason=Project p-gone has no usable document store (officialMountPointId=mp-gone): properties.json missing".to_string(),
                "ERROR quilltap_core::db::document_store_overlay Dropping project from list — document store unavailable projectId=p-null officialMountPointId=null reason=Project p-null has no usable document store (officialMountPointId=null): officialMountPointId is null".to_string(),
                "WARN quilltap_core::db::document_store_overlay applyProjectStoreOverlay dropped projects with unavailable stores dropped=2 of=3".to_string(),
            ]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            apply_overlay::<GroupEntity>(&conn, vec![row("g-gone", Some("mp-gone"))])
        });
        assert!(got.unwrap().is_empty());
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap_core::db::document_store_overlay Dropping group from list — document store unavailable groupId=g-gone officialMountPointId=mp-gone reason=Group g-gone has no usable document store (officialMountPointId=mp-gone): properties.json missing".to_string(),
                "WARN quilltap_core::db::document_store_overlay applyGroupStoreOverlay dropped groups with unavailable stores dropped=1 of=1".to_string(),
            ]
        );
    }

    /// The silence leg: every row hydrates → neither line.
    #[test]
    fn a_healthy_list_logs_no_drop_line() {
        let conn = mount();
        let (got, lines) = crate::test_support::captured_with(|| {
            apply_overlay::<ProjectEntity>(&conn, vec![row("p-ok", Some("mp-ok"))])
        });
        assert_eq!(got.unwrap().len(), 1);
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// The rider: v4's `${Label} state.json unparseable; defaulting to {}` WARN
    /// (`document-store-overlay.ts:179-184`) — the row still hydrates with `{}`.
    #[test]
    fn an_unparseable_state_json_warns_and_defaults() {
        let conn = mount();
        let (got, lines) = crate::test_support::captured_with(|| {
            apply_overlay::<GroupEntity>(&conn, vec![row("g-1", Some("mp-badstate"))])
        });
        let got = got.unwrap();
        assert_eq!(got[0]["state"], serde_json::json!({}));
        assert_eq!(
            lines,
            vec!["WARN quilltap_core::db::document_store_overlay Group state.json unparseable; defaulting to {} groupId=g-1 officialMountPointId=mp-badstate".to_string()]
        );
    }

    /// Under a failed batch read (no tables) every store is dropped — after the
    /// home's repository lines (P4.142 unit 2), the drop pair.
    #[test]
    fn a_failed_batch_read_drops_with_the_repository_lines_first() {
        let conn = Connection::open_in_memory().unwrap();
        let (got, lines) = crate::test_support::captured_with(|| {
            apply_overlay::<ProjectEntity>(&conn, vec![row("p-1", Some("mp-1"))])
        });
        assert!(got.unwrap().is_empty());
        let repo_lines = lines
            .iter()
            .filter(|l| l.starts_with("ERROR quilltap::db Error finding documents by mount point IDs and path collection=doc_mount_documents mountPointIdCount=1 relativePath="))
            .count();
        assert_eq!(repo_lines, ALL_OVERLAY_PATHS.len(), "{lines:#?}");
        assert_eq!(lines.len(), ALL_OVERLAY_PATHS.len() + 2, "{lines:#?}");
        assert!(lines[ALL_OVERLAY_PATHS.len()].contains("Dropping project from list"));
    }
}

/// P4.146 (dogfood #136): the three-state typed bag ALONE carries v4's explicit
/// `null`s through this engine — the hydrated read (`hydrate_one`'s spread) and
/// the read-modify-write (`apply_write_overlay`'s seed + re-serialize). No engine
/// hunk: these drive the real repositories over the fresh-instance DDL and would
/// redden on a plain `Option<T>` bag (the pre-#136 shape) in both directions.
#[cfg(test)]
mod null_preservation_tests {
    use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
    use crate::db::groups::{GroupCreateInput, GroupCreateOptions, GroupsRepository};
    use crate::db::projects::{ProjectCreateInput, ProjectCreateOptions, ProjectsRepository};
    use rusqlite::Connection;
    use serde_json::{json, Map, Value};

    fn conns() -> (Connection, Connection) {
        let schema: Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        let open = |part: &str| {
            let conn = Connection::open_in_memory().unwrap();
            for ddl in schema[part].as_array().unwrap() {
                conn.execute_batch(ddl.as_str().unwrap()).unwrap();
            }
            conn
        };
        (open("main"), open("mountIndex"))
    }

    /// The stored `properties.json` bytes and their content sha (the mount-index
    /// row the finding's symptom moves).
    fn stored(mount: &Connection, mount_point_id: &str) -> (String, String) {
        mount
            .query_row(
                "SELECT d.content, f.sha256 FROM doc_mount_file_links l \
                 JOIN doc_mount_documents d ON d.fileId = l.fileId \
                 JOIN doc_mount_files f ON f.id = l.fileId \
                 WHERE l.mountPointId = ?1 AND l.relativePath = 'properties.json'",
                [mount_point_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }

    fn patch(pairs: Value) -> Map<String, Value> {
        pairs.as_object().cloned().unwrap()
    }

    /// The finding's reference case, widened to all eleven nullable keys: a
    /// v4-written bag (the cutover shape — every NULL column an explicit `null`),
    /// one Allow-Any-Character toggle, and the written bytes differ from the
    /// planted bytes ONLY in that key.
    const PLANTED_PROJECT: &str = r#"{
  "allowAnyCharacter": false,
  "characterRoster": [],
  "color": null,
  "icon": null,
  "defaultDisabledTools": [],
  "defaultDisabledToolGroups": [],
  "defaultAgentModeEnabled": null,
  "defaultAvatarGenerationEnabled": null,
  "defaultImageProfileId": null,
  "defaultRoleplayTemplateId": null,
  "defaultAlertCharactersOfLanternImages": null,
  "answerConfirmationOverride": null,
  "storyBackgroundsEnabled": null,
  "staticBackgroundImageId": null,
  "storyBackgroundImageId": null,
  "backgroundDisplayMode": "theme"
}"#;

    #[test]
    fn a_one_key_project_update_keeps_every_planted_null() {
        let (main, mount) = conns();
        let repo = ProjectsRepository::new(&main, &mount);
        let created = repo
            .create(
                &ProjectCreateInput {
                    name: "LUC Ranch".into(),
                    description: None,
                    instructions: None,
                    state: json!({}),
                    properties: json!({ "color": "#abcdef" }),
                },
                &ProjectCreateOptions::default(),
            )
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        let mp = created["officialMountPointId"]
            .as_str()
            .unwrap()
            .to_string();
        DocMountFileLinksRepository::new(&mount)
            .write_database_document(&mp, "properties.json", PLANTED_PROJECT)
            .unwrap();
        assert_eq!(stored(&mount, &mp).0, PLANTED_PROJECT);

        // The read wire carries every planted null (v4 `hydrateOne` spreads the
        // parsed bag — Zod keeps `null` on a `.nullable().optional()` key).
        let read = repo.find_by_id(&id).unwrap().unwrap();
        for key in [
            "color",
            "icon",
            "defaultAgentModeEnabled",
            "defaultAvatarGenerationEnabled",
            "defaultImageProfileId",
            "defaultRoleplayTemplateId",
            "defaultAlertCharactersOfLanternImages",
            "answerConfirmationOverride",
            "storyBackgroundsEnabled",
            "staticBackgroundImageId",
            "storyBackgroundImageId",
        ] {
            assert_eq!(read.get(key), Some(&Value::Null), "{key} on the read");
        }

        repo.update(&id, &patch(json!({ "allowAnyCharacter": true })))
            .unwrap();
        let (bytes, sha) = stored(&mount, &mp);
        assert_eq!(
            bytes,
            PLANTED_PROJECT.replace(
                "\"allowAnyCharacter\": false",
                "\"allowAnyCharacter\": true"
            ),
            "ONLY allowAnyCharacter moved"
        );
        assert_eq!(
            sha,
            crate::services::mount_index::sync::types::sha256_hex(bytes.as_bytes())
        );
    }

    /// The other arms of the three states over the RMW: a `null` patch on a key
    /// with a value writes `null` (not absent); a value patch on a `null` key
    /// writes the value; an ABSENT key stays absent through an unrelated edit.
    #[test]
    fn a_null_patch_writes_null_and_absent_stays_absent() {
        let (main, mount) = conns();
        let repo = ProjectsRepository::new(&main, &mount);
        let created = repo
            .create(
                &ProjectCreateInput {
                    name: "Mixed".into(),
                    description: None,
                    instructions: None,
                    state: json!({}),
                    properties: json!({ "color": "#abcdef", "icon": null }),
                },
                &ProjectCreateOptions::default(),
            )
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        let mp = created["officialMountPointId"]
            .as_str()
            .unwrap()
            .to_string();
        let bag =
            |mount: &Connection| -> Value { serde_json::from_str(&stored(mount, &mp).0).unwrap() };
        assert_eq!(bag(&mount)["icon"], Value::Null, "create keeps the null");
        assert!(bag(&mount).get("storyBackgroundsEnabled").is_none());

        repo.update(&id, &patch(json!({ "color": null, "icon": "gear" })))
            .unwrap();
        let after = bag(&mount);
        assert_eq!(after.get("color"), Some(&Value::Null));
        assert_eq!(after["icon"], "gear");
        assert!(
            after.get("storyBackgroundsEnabled").is_none(),
            "an absent key stays absent through an unrelated edit"
        );
        let read = repo.find_by_id(&id).unwrap().unwrap();
        assert_eq!(read.get("color"), Some(&Value::Null));
        assert!(read.get("storyBackgroundsEnabled").is_none());
    }

    #[test]
    fn a_group_keeps_its_null_colour_through_create_read_and_update() {
        let (main, mount) = conns();
        let repo = GroupsRepository::new(&main, &mount);
        let created = repo
            .create_with_properties(
                &GroupCreateInput {
                    name: "Loners".into(),
                    description: None,
                    instructions: None,
                    state: json!({}),
                    color: None,
                    icon: None,
                },
                &crate::db::groups::GroupProperties {
                    color: Some(None),
                    icon: Some(None),
                },
                &GroupCreateOptions::default(),
            )
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        let mp = created["officialMountPointId"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            stored(&mount, &mp).0,
            "{\n  \"color\": null,\n  \"icon\": null\n}"
        );
        assert_eq!(created.get("color"), Some(&Value::Null));
        repo.update(&id, &patch(json!({ "icon": "gear" }))).unwrap();
        assert_eq!(
            stored(&mount, &mp).0,
            "{\n  \"color\": null,\n  \"icon\": \"gear\"\n}"
        );
        // The plain create leaves an absent colour absent (the restore + harness
        // callers' contract, unchanged).
        let plain = repo
            .create(
                &GroupCreateInput {
                    name: "Plain".into(),
                    description: None,
                    instructions: None,
                    state: json!({}),
                    color: None,
                    icon: Some("star".into()),
                },
                &GroupCreateOptions::default(),
            )
            .unwrap();
        let mp = plain["officialMountPointId"].as_str().unwrap();
        assert_eq!(stored(&mount, mp).0, "{\n  \"icon\": \"star\"\n}");
        assert!(plain.get("color").is_none());
    }
}
