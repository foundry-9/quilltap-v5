//! v4's quilltap-import service (`lib/import/quilltap-import/**`) — the full
//! `.qtap` import pipeline: the legacy monolithic-JSON parse (`validation.ts`),
//! the streaming NDJSON reassembler ([`ndjson`]), the count-only
//! [`preview::preview_import`], and [`execute_import`] — v4 `executeImport`
//! (`execute.ts:41`) with the ten-map id-mapping state, all four conflict
//! strategies (the route maps `'replace'` → `'overwrite'` before it gets here),
//! the per-entity importers, the chat sidecars, the legacy folds, and the
//! post-write [`reconcile`] pass.
//!
//! This module began as the P4.4u4 **seed subset** (characters + memories at
//! `skip` only, everything else a loud typed refusal); the P4.9G4 import-execute
//! unit extended it to the full v4 surface, so the subset refusals are gone —
//! the seed consumers ([`seed`], [`reset`]) now run through the same full
//! pipeline with the same options they always passed.
//!
//! ## Composition — no re-port
//!
//! [`execute_import`] operates directly over the MAIN + MOUNT-INDEX connections
//! (run it inside a single [`crate::db::runtime::Db::write`] closure — the
//! sync-handlers-over-both-connections idiom) and composes the already-ported
//! repositories. v4's error discipline is carried arm for arm: per-item
//! failures warn (or, for tags/templates/profiles, only log) and continue;
//! loop-preamble failures throw into the one big catch, which answers
//! `success: false` with `Import failed: <message>` appended to `warnings`.

mod characters;
mod configuration;
mod document_stores;
mod entities;
mod files;
mod legacy_presets;
mod memories;
pub mod ndjson;
pub mod preview;
mod profiles;
mod reconcile;
pub mod reset;
pub mod seed;
pub mod seed_assets;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::DbError;

/// v4 `ConflictStrategy` (`lib/export/types.ts`) as `executeImport` receives it —
/// the route's `'replace'` has already been remapped to `'overwrite'`
/// (`route.ts:780`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictStrategy {
    Skip,
    Overwrite,
    Duplicate,
}

/// v4 `PreserveIdsMode` (`quilltap-import/types.ts:138`, `01e481f6`) — how a
/// `preserveIds` import treats a claimed id that already exists. v4's own note
/// rides verbatim, because the distinction is the whole safety story:
///
/// > - `refuse-on-collision` (the default, WP B1): any collision refuses the
/// >   whole import before a single write — no partial application, no silent
/// >   remint. Right for importing a stranger's bundle into supposedly-empty
/// >   space.
/// > - `skip-if-present` (spec §6 / F4, **rehydrate only**): rehydration
/// >   restores a bundle into a mount that still exists, so it collides by
/// >   construction on the mount point, every surviving folder, the managed
/// >   documents, the avatar blob and its link. An id already present inside
/// >   the *target character's own vault* (or the target character itself, or
/// >   its own memories) is skipped — the surviving row wins and the record is
/// >   not imported. An id that exists anywhere else — a different mount, a
/// >   different character, another character's memory — still refuses the
/// >   whole import, atomically, exactly as `refuse-on-collision` would.
/// >
/// > The ordinary import wizard must never pass `skip-if-present`: silently
/// > skipping a colliding id there is precisely the partial application the
/// > refuse rule exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PreserveIdsMode {
    #[default]
    RefuseOnCollision,
    SkipIfPresent {
        /// The archived character being rehydrated.
        target_character_id: String,
        /// Its surviving vault mount point (`None` when it never had one).
        target_vault_mount_point_id: Option<String>,
    },
}

/// v4 `ImportOptions` (`types.ts:88`). `include_related_entities` and
/// `selected_ids` are **read by nothing** in the v4 pipeline (grep-confirmed) —
/// carried for fidelity with the route's call site.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub conflict_strategy: ConflictStrategy,
    pub include_memories: bool,
    /// Dead option — no code reads it. Kept to mirror v4's call site verbatim.
    pub include_related_entities: bool,
    /// Dead option — the route forwards it, nothing consumes it.
    pub selected_ids: Option<Value>,
    /// v4 `ImportOptions.preserveIds` (`01e481f6`) — claim the bundle's own row
    /// ids instead of minting. Only the character-archive rehydrate path (round
    /// 2) sets it; v4's import wizard never does.
    pub preserve_ids: bool,
    /// v4 `ImportOptions.preserveIdsMode`, defaulted to
    /// [`PreserveIdsMode::RefuseOnCollision`] and IGNORED when `preserve_ids`
    /// is false.
    pub preserve_ids_mode: PreserveIdsMode,
}

impl ImportOptions {
    /// The exact options both seed consumers pass (`seedFromImports` :212–216 and
    /// `handleResetBuiltins` :263–267): skip, memories on, related off.
    pub fn seed_defaults() -> Self {
        Self {
            conflict_strategy: ConflictStrategy::Skip,
            include_memories: true,
            include_related_entities: false,
            selected_ids: None,
            preserve_ids: false,
            preserve_ids_mode: PreserveIdsMode::RefuseOnCollision,
        }
    }
}

/// v4 `QuilltapExportCounts` as `executeImport` materializes it: the eleven
/// literal keys zeroed up front (`execute.ts:71-97`), plus the optional extras
/// the sidecar/document-store phases assign only when their data is present.
/// Field order here IS the wire key order (serde emits declaration order).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCounts {
    pub characters: u32,
    pub chats: u32,
    pub messages: u32,
    pub roleplay_templates: u32,
    pub connection_profiles: u32,
    pub image_profiles: u32,
    pub embedding_profiles: u32,
    pub tags: u32,
    pub memories: u32,
    pub projects: u32,
    pub groups: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_annotations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_documents: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_stores: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_store_folders: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_store_documents: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_store_blobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_store_project_links: Option<u32>,
    // The five `7189a968` additions (steps 9-10) — assigned only when their
    // phase runs, exactly like the sidecar/document-store extras above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folders: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_templates: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_models: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_configs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_settings: Option<u32>,
}

/// v4 `ImportResult` (`types.ts:93`). `imported_character_ids` is the values of
/// the character id-map — for `duplicate` these are the freshly created
/// characters (the Salon "Summon from Lore" flow reads them).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub success: bool,
    pub imported: ImportCounts,
    pub skipped: ImportCounts,
    pub warnings: Vec<String>,
    pub imported_character_ids: Vec<String>,
}

impl ImportResult {
    /// The raw route body (v4 `NextResponse.json(result)`), key order included.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// A parsed + validated export (v4 `QuilltapExport`). `manifest` and `data` stay
/// dynamic [`Value`]s — v4's `getExportData` reads them the same way.
#[derive(Debug, Clone)]
pub struct QuilltapExport {
    pub manifest: Value,
    pub data: Value,
}

/// A hard, typed refusal from the PARSE side (`validation.ts` + the NDJSON
/// sniff). Distinct from the per-item `warnings` inside [`ImportResult`]:
/// these mean "this is not a readable `.qtap` payload" and are returned as
/// `Err` before any writes. (`Db` carries a store failure surfaced by a
/// consumer that treats the whole parse+import as one fallible step.)
#[derive(Debug)]
pub enum ImportError {
    /// `JSON.parse` failed, or the top level is not an object (v4
    /// `parseExportFile` / `validateExportFormat`'s object guards).
    ParseJson(String),
    /// `manifest` missing or not an object.
    MissingManifest,
    /// `manifest.format` is not the legacy `quilltap-export` format.
    InvalidFormat { got: String },
    /// The streaming NDJSON serialization (`format: 'qtap-ndjson'`) reached the
    /// legacy-JSON parser — the seed path's entrance; the routes go through
    /// [`ndjson::load_qtap_from_upload`] instead.
    Ndjson,
    /// `manifest.version` is not the pinned `'1.0'`.
    UnsupportedVersion { got: String },
    /// `data` missing or not an object.
    MissingData,
    /// A store/DB write failed at a non-per-item chokepoint.
    Db(DbError),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::ParseJson(m) => write!(f, "Invalid export file: {m}"),
            ImportError::MissingManifest => write!(f, "Missing or invalid manifest"),
            ImportError::InvalidFormat { got } => {
                write!(f, "Invalid format: expected 'quilltap-export', got '{got}'")
            }
            ImportError::Ndjson => write!(
                f,
                "The streaming NDJSON export format ('qtap-ndjson') is not supported by the \
                 legacy monolithic-JSON parser (upload the file through the import route, \
                 which sniffs and reassembles it)."
            ),
            ImportError::UnsupportedVersion { got } => {
                write!(f, "Unsupported version: {got}. Only 1.0 is supported.")
            }
            ImportError::MissingData => write!(f, "Missing or invalid data section"),
            ImportError::Db(e) => write!(f, "Import failed: {e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<DbError> for ImportError {
    fn from(e: DbError) -> Self {
        ImportError::Db(e)
    }
}

/// An insertion-ordered id-map (source id → destination id) — v4's `Map`
/// (insertion-ordered). Insertion order matters: `imported_character_ids` is
/// `Array.from(map.values())`, and the reconcile loops iterate entries in
/// insertion order.
#[derive(Default)]
pub(crate) struct IdMap(Vec<(String, String)>);

impl IdMap {
    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn set(&mut self, key: String, value: String) {
        if let Some(entry) = self.0.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = value;
        } else {
            self.0.push((key, value));
        }
    }

    fn values(&self) -> Vec<String> {
        self.0.iter().map(|(_, v)| v.clone()).collect()
    }

    fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// v4 `IdMappingState` (`types.ts:107`) — the ten insertion-ordered maps, plus
/// the four members `01e481f6` added.
#[derive(Default)]
pub(crate) struct IdMaps {
    pub tags: IdMap,
    pub characters: IdMap,
    pub chats: IdMap,
    pub connection_profiles: IdMap,
    pub image_profiles: IdMap,
    pub embedding_profiles: IdMap,
    pub roleplay_templates: IdMap,
    pub projects: IdMap,
    pub groups: IdMap,
    pub mount_points: IdMap,
    /// Source `doc_mount_file_links.id` → the created (or surviving) link id.
    /// Reconciliation remaps `defaultImageId` / `avatarOverrides[].imageId`
    /// through it — those are link ids in the SOURCE instance's vault (Bug 52).
    pub doc_mount_file_links: IdMap,
    /// v4 `characterVaultMounts`. New character id → the vault mount-point id
    /// the bundle claimed in the *source* instance. v4's note, verbatim:
    ///
    /// > Populated only for characters the importer actually created, because
    /// > `characters.create()` drops the incoming pointer and provisions a
    /// > scaffold vault of its own — so the character row can no longer tell
    /// > reconciliation which store the bundle meant. Reconciliation uses it to
    /// > repoint the character at its imported vault and cascade-delete the
    /// > scaffold (bundle wins, whole-store).
    pub character_vault_mounts: IdMap,
    /// v4 `skippedCharacterVaults`. Source vault mount-point ids belonging to
    /// characters the importer *skipped*. Their store records are dropped
    /// before the document-store phase: importing them would strand an orphan
    /// store no character points at.
    pub skipped_character_vaults: std::collections::HashSet<String>,
    /// v4 `preserveIdsSkips`. Ids the `skip-if-present` preflight sanctioned as
    /// already-present inside the rehydrate target (see [`PreserveIdsMode`]).
    /// Importers skip these records — the surviving row wins — instead of
    /// re-creating them. Always empty under `refuse-on-collision`, where the
    /// preflight guarantees no claimed id exists at all.
    pub preserve_ids_skips: std::collections::HashSet<String>,
}

/// The id a `preserveIds` create should claim, or a fresh one — v4's
/// `const createOptions = options.preserveIds ? { id: x.id } : undefined`
/// spelled once instead of at the fourteen call sites that repeat it.
///
/// An ABSENT source id falls back to minting, matching v4's
/// `options?.id ?? randomUUID()` when the record carries no `id` at all. (v4
/// would honor an explicit empty string, since `??` only tests null/undefined;
/// no writer can emit one, and honoring it would violate the primary key on
/// both engines.)
pub(crate) fn mint_or_preserve(options: &ImportOptions, source_id: &str) -> (String, String) {
    let now = crate::clock::now_iso();
    if options.preserve_ids && !source_id.is_empty() {
        (source_id.to_string(), now)
    } else {
        (uuid::Uuid::new_v4().to_string(), now)
    }
}

/// v4 `getPreserveIdsCreateOptions` (`execute.ts:55`, `01e481f6`).
///
/// **Dead in v4 too** — nothing calls it; every importer inlines the same
/// ternary. Ported under the vestigial-cruft rule (port dead code faithfully,
/// clean up after the port) so a later reader diffing the two files does not
/// have to wonder whether v5 dropped a live helper. [`mint_or_preserve`] is
/// what v5's call sites actually use.
#[cfg(test)]
pub(crate) fn get_preserve_ids_create_options(
    source_id: Option<&str>,
    options: &ImportOptions,
) -> Option<String> {
    let source_id = source_id.filter(|s| !s.is_empty())?;
    if !options.preserve_ids {
        return None;
    }
    Some(source_id.to_string())
}

/// The `<name>` v4 interpolates into `Failed to import <kind> "<name>": …`
/// (`275cd7bc`, bug 79) — read off the RAW payload item, since the arm fires
/// when the typed parse itself failed, so the field can be any JSON shape or
/// absent. v4's sentence is a template literal over that raw value, so the
/// rendering is JS `${…}`: an absent key is `undefined` → `"undefined"`, and
/// everything else follows `String(value)` ([`to_js_string`] — `null` →
/// `"null"`, `7` → `"7"`, an array joins with commas, an object is
/// `"[object Object]"`).
///
/// Only for the warning sentences. The create-path name fallbacks (projects /
/// groups `display_name`) keep their own string-only read — they feed WRITTEN
/// data, not a sentence.
///
/// [`to_js_string`]: crate::pascal::js_value::to_js_string
pub(super) fn warning_display_name(raw: &Value) -> String {
    match raw.get("name") {
        None => "undefined".to_string(),
        Some(v) => crate::pascal::js_value::to_js_string(v),
    }
}

/// `item.id` as a string (v4 reads it untyped; absent → `""`).
pub(crate) fn id_of(item: &Value) -> String {
    item.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// v4 `parseExportFile` + `validateExportFormat` (`validation.ts`) — parse the
/// legacy monolithic JSON and hard-pin `format`/`version`. Diverges only to sniff
/// and loudly refuse the NDJSON serialization (the seed path's entrance never
/// sees one; the routes reassemble NDJSON via [`ndjson::load_qtap_from_upload`]).
pub fn parse_export_file(json_string: &str) -> Result<QuilltapExport, ImportError> {
    let data: Value = serde_json::from_str(json_string).map_err(|e| {
        // A `qtap-ndjson` stream is line-delimited, not one JSON object, so it
        // fails to parse here — sniff for it to refuse with a clear message.
        if json_string.contains("qtap-ndjson") {
            ImportError::Ndjson
        } else {
            ImportError::ParseJson(e.to_string())
        }
    })?;
    validate_export_format(&data)?;
    let obj = data.as_object().expect("validated as object");
    Ok(QuilltapExport {
        manifest: obj.get("manifest").cloned().unwrap_or(Value::Null),
        data: obj.get("data").cloned().unwrap_or(Value::Null),
    })
}

/// v4 `validateExportFormat` — the `format`/`version` hard pins (+ the NDJSON
/// sniff, this port's divergence).
fn validate_export_format(data: &Value) -> Result<(), ImportError> {
    let obj = data.as_object().ok_or(ImportError::ParseJson(
        "Export data must be a JSON object".to_string(),
    ))?;

    let manifest = obj
        .get("manifest")
        .and_then(Value::as_object)
        .ok_or(ImportError::MissingManifest)?;

    let format = manifest.get("format").and_then(Value::as_str).unwrap_or("");
    if format == "qtap-ndjson" {
        return Err(ImportError::Ndjson);
    }
    if format != "quilltap-export" {
        return Err(ImportError::InvalidFormat {
            got: format.to_string(),
        });
    }

    // v4 pins `version === '1.0'` (a string). Match the JSON text form.
    let version = manifest.get("version");
    let version_ok = version.and_then(Value::as_str) == Some("1.0");
    if !version_ok {
        let got = match version {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => "undefined".to_string(),
        };
        return Err(ImportError::UnsupportedVersion { got });
    }

    if !obj.get("data").map(Value::is_object).unwrap_or(false) {
        return Err(ImportError::MissingData);
    }

    Ok(())
}

/// v4 `preflightPreserveIds` (`execute.ts:71`, `01e481f6`) — pre-scan every id
/// the bundle would claim and check it against the live instance, before a
/// single write happens.
///
/// The two modes are [`PreserveIdsMode`]'s. The fifteen check kinds run in v4's
/// declaration order, and the loop is v4's exactly: a REPEAT inside the bundle
/// throws the "(also seen as …)" shape; an id that already exists throws the
/// plain shape unless skip-if-present classifies it as living inside the
/// rehydrate target. Both messages are pushed to `warnings` BEFORE the throw,
/// so a refused import answers with the collision named.
///
/// Returns the collision message on refusal; `Ok(())` when the bundle may
/// proceed (including the no-op when `preserve_ids` is off).
///
/// ## [P4.48] Read failures REFUSE — a measured divergence from v4
///
/// Every exists-check here used to be `repo.find…(id).ok().flatten()`, which
/// collapses a `DbError` to `None` = "the id is free". That defeats the
/// preflight's whole purpose: the preserveIds machinery would go on to attempt
/// id-carrying INSERTs into a database it could not even read, and the
/// refuse-on-collision / skip-if-present semantics degrade silently.
///
/// The lane's fresh v4 survey (`aa464abf`) **refuted** the premise that v4
/// propagates here. v4's `AbstractBaseRepository._findById` is a
/// `safeQuery(…, null)` — FALLBACK mode — so v4 swallows a read error to `null`
/// at *every* repo this preflight consults, and its own preflight then treats
/// the id as free exactly as v5 used to. The two legs measured:
///
/// - **DB read error** (unreadable/missing table): v4 swallows → the import
///   proceeds and partially applies. v5 now REFUSES before any write. This is a
///   deliberate divergence under the standing backup/restore/import ruling
///   (2026-08-03: "fix v4 bugs in this family, don't match them"), pinned in
///   BOTH directions by the harness so a v4 convergence cannot pass unnoticed.
/// - **Overlay unavailable** on an EXISTING project/group row: v4's
///   `applyOverlayOne` genuinely throws (it is not wrapped in `safeQuery`), and
///   its caller catches ANY error into `success:false` with `warnings`
///   untouched. v5 now matches that byte for byte — this leg was a plain port
///   bug, no divergence owed.
///
/// The refusal message reaches the caller since v4 `275cd7bc` (bug 79):
/// `execute_import` logs it and answers `success:false` with `warnings`
/// carrying `Import refused before anything was written: <message>` — so a
/// collision is named twice (by the loop below, then wrapped) and a refusal
/// for any other reason is named at all, where both used to be silent.
fn preflight_preserve_ids(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    data: &serde_json::Map<String, Value>,
    options: &ImportOptions,
    warnings: &mut Vec<String>,
    id_maps: &mut IdMaps,
) -> Result<(), String> {
    if !options.preserve_ids {
        return Ok(());
    }

    let skip_target = match &options.preserve_ids_mode {
        PreserveIdsMode::RefuseOnCollision => None,
        PreserveIdsMode::SkipIfPresent {
            target_character_id,
            target_vault_mount_point_id,
        } => Some((
            target_character_id.clone(),
            target_vault_mount_point_id.clone(),
        )),
    };

    let arr = |key: &str| -> Vec<Value> {
        data.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let documents = arr("documents");
    let blobs = arr("blobs");

    // v4 `isNonEmpty` — a falsy id is dropped before any check runs.
    let non_empty = |v: Option<&Value>| -> Option<String> {
        v.and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let ids_of = |rows: &[Value]| -> Vec<String> {
        rows.iter()
            .map(|r| {
                r.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    };

    // Hard-link groups share one content row, so the same fileId legitimately
    // appears on several document/blob records — dedupe rather than treating
    // the repeat as a duplicate claim. (v4 dedupes ONLY the file ids; link
    // ids are 1:1 with their records and a repeat there is a real clash.)
    let mut carried_file_ids: Vec<String> = Vec::new();
    for row in documents.iter().chain(blobs.iter()) {
        if let Some(id) = non_empty(row.get("fileId")) {
            if !carried_file_ids.contains(&id) {
                carried_file_ids.push(id);
            }
        }
    }
    let carried_link_ids: Vec<String> = documents
        .iter()
        .chain(blobs.iter())
        .filter_map(|r| non_empty(r.get("linkId")))
        .collect();
    // Blob ids dedupe first-occurrence, for the same repeat-by-construction
    // reason as the file ids above. v4's own wording (`execute.ts:115`):
    //
    // > The blob leg of the export emits one record per LINK
    // > (`listByMountPoint` joins from `doc_mount_file_links`), so a
    // > content-addressed blob linked at two paths appears twice under one
    // > blobId — identical claims over a single row, not a duplicate claim.
    // > Dedupe, as with `carriedFileIds` above; the sha-match skip below still
    // > refuses a genuine same-id/different-bytes clash.
    //
    // CONVERGENCE (v4 `de9f70bf`, Bug 57): v5 carried this as a pinned
    // divergence from the archive round-2 §3 review — v4's undeduped list made
    // its own preflight throw `(also seen as document store blob)` on every
    // rehydrate of such a vault, so an archived character with a twice-linked
    // photo could not come back in v4 at all. Found BY this port's unification
    // wire test; v4 adopted the dedupe and the two readers now agree. (Reader
    // side only on both: the export writer still emits the per-link
    // duplicates.) `carriedLinkIds` stays undeduped on both sides — each
    // record carries its own link's id, so a repeat there is a real clash.
    let mut carried_blob_ids: Vec<String> = Vec::new();
    for row in blobs.iter() {
        if let Some(id) = non_empty(row.get("blobId")) {
            if !carried_blob_ids.contains(&id) {
                carried_blob_ids.push(id);
            }
        }
    }
    let carried_folder_ids: Vec<String> = arr("folders")
        .iter()
        .filter_map(|r| non_empty(r.get("id")))
        .collect();

    // [Bug 54] The content hashes the bundle claims for each carried content id.
    // v4's reasoning, verbatim:
    //
    // > The two content-addressed tables are found-or-created **by sha256**
    // > (`linkDocumentContent` / `linkBlobContent`), so when a row for these
    // > exact bytes already exists the writer reuses it and never honors the
    // > carried id — dedup, not a claim.
    let mut carried_content_sha: Vec<(String, String)> = Vec::new();
    let push_sha = |v: &mut Vec<(String, String)>, k: Option<String>, s: Option<String>| {
        if let (Some(k), Some(s)) = (k, s) {
            match v.iter_mut().find(|(kk, _)| *kk == k) {
                Some(slot) => slot.1 = s,
                None => v.push((k, s)),
            }
        }
    };
    for doc in &documents {
        push_sha(
            &mut carried_content_sha,
            non_empty(doc.get("fileId")),
            non_empty(doc.get("contentSha256")),
        );
    }
    for blob in &blobs {
        push_sha(
            &mut carried_content_sha,
            non_empty(blob.get("fileId")),
            non_empty(blob.get("sha256")),
        );
    }
    let mut carried_blob_sha: Vec<(String, String)> = Vec::new();
    for blob in &blobs {
        push_sha(
            &mut carried_blob_sha,
            non_empty(blob.get("blobId")),
            non_empty(blob.get("sha256")),
        );
    }
    let lookup = |v: &[(String, String)], k: &str| -> Option<String> {
        v.iter().find(|(kk, _)| kk == k).map(|(_, s)| s.clone())
    };

    let target_vault: Option<String> = skip_target.as_ref().and_then(|(_, v)| v.clone());
    let target_character: Option<String> = skip_target.as_ref().map(|(c, _)| c.clone());
    // Is this mount the rehydrate target's own vault?
    let is_target_vault = |mount_point_id: Option<&str>| -> bool {
        match (&target_vault, mount_point_id) {
            (Some(t), Some(m)) => t == m,
            _ => false,
        }
    };
    // Does this content row have a link inside the target vault?
    //
    // ⚠ The `unwrap_or(false)` here is DELIBERATE and v4-faithful — do NOT
    // "fix" it the way P4.48 fixed the exists-checks above. v4's
    // `docMountFileLinks.findByFileId` is a `safeQuery(…, [])` (FALLBACK mode),
    // so a read failure yields an EMPTY link list and `.some()` answers false.
    // This is a skip CLASSIFIER, not an existence check: answering false only
    // withholds the skip sanction, which makes the import refuse the collision
    // rather than silently claim an id.
    let file_linked_in_target_vault = |file_id: &str| -> bool {
        let Some(target) = target_vault.as_deref() else {
            return false;
        };
        crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount)
            .find_by_file_id(file_id)
            .map(|links| links.iter().any(|l| l.mount_point_id == target))
            .unwrap_or(false)
    };

    // v4's `checks` array, in declaration order. `exists` and `skippable` are
    // spelled as one classifier per kind returning `(exists, skippable)`.
    enum Kind {
        Character,
        Tag,
        ConnectionProfile,
        ImageProfile,
        EmbeddingProfile,
        RoleplayTemplate,
        Project,
        Group,
        Chat,
        Memory,
        DocumentStore,
        File,
        DocumentStoreFolder,
        DocumentStoreFile,
        DocumentStoreLink,
        DocumentStoreBlob,
    }
    let checks: Vec<(&str, Vec<String>, Kind)> = vec![
        ("character", ids_of(&arr("characters")), Kind::Character),
        ("tag", ids_of(&arr("tags")), Kind::Tag),
        (
            "connection profile",
            ids_of(&arr("connectionProfiles")),
            Kind::ConnectionProfile,
        ),
        (
            "image profile",
            ids_of(&arr("imageProfiles")),
            Kind::ImageProfile,
        ),
        (
            "embedding profile",
            ids_of(&arr("embeddingProfiles")),
            Kind::EmbeddingProfile,
        ),
        (
            "roleplay template",
            ids_of(&arr("roleplayTemplates")),
            Kind::RoleplayTemplate,
        ),
        ("project", ids_of(&arr("projects")), Kind::Project),
        ("group", ids_of(&arr("groups")), Kind::Group),
        ("chat", ids_of(&arr("chats")), Kind::Chat),
        ("memory", ids_of(&arr("memories")), Kind::Memory),
        (
            "document store",
            ids_of(&arr("mountPoints")),
            Kind::DocumentStore,
        ),
        ("file", ids_of(&arr("files")), Kind::File),
        (
            "document store folder",
            carried_folder_ids,
            Kind::DocumentStoreFolder,
        ),
        (
            "document store file",
            carried_file_ids,
            Kind::DocumentStoreFile,
        ),
        (
            "document store link",
            carried_link_ids,
            Kind::DocumentStoreLink,
        ),
        (
            "document store blob",
            carried_blob_ids,
            Kind::DocumentStoreBlob,
        ),
    ];

    let links_repo = crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount);
    let folders_repo = crate::db::doc_mount_folders::DocMountFoldersRepository::new(mount);
    let blobs_repo = crate::db::doc_mount_blobs::DocMountBlobsRepository::new(mount);

    let mut seen_ids: Vec<(String, &str)> = Vec::new();
    for (kind_name, ids, kind) in &checks {
        for id in ids {
            // v4 `if (!id) continue` — a falsy id is skipped entirely.
            if id.is_empty() {
                continue;
            }
            if let Some((_, existing_kind)) = seen_ids.iter().find(|(k, _)| k == id) {
                let message = format!(
                    "Preserve IDs collision for {kind_name} {id} (also seen as {existing_kind})"
                );
                warnings.push(message.clone());
                return Err(message);
            }
            let (exists, skippable) = match kind {
                Kind::Character => {
                    // ⚠ `find_by_id_raw`, not the overlay reader v4 uses
                    // (`repos.characters.findById`). Existence is a row
                    // question, and a broken vault must not change the answer.
                    // The measured consequence is recorded as a divergence in
                    // the P4.48 lane record, and it has TWO legs (scope
                    // widened at the aa464abf unification review):
                    //  - COLLIDING character, vault unavailable: v4's overlay
                    //    throw wins (→ `success:false`, EMPTY warnings) where
                    //    v5 reports the ordinary collision. Both refuse; only
                    //    the body differs.
                    //  - SKIPPABLE character (rehydrate's target seat, row
                    //    present), vault unavailable: v4's overlay throw
                    //    refuses the whole import up front, where v5 grants
                    //    the skip sanction and PROCEEDS — any vault failure
                    //    surfaces later, from whichever write first touches
                    //    the broken store. Same rationale: the seat's
                    //    existence is not the vault's health.
                    let e = crate::db::characters_read::find_by_id_raw(main, id)
                        .map_err(|e| e.to_string())?
                        .is_some();
                    (e, target_character.as_deref() == Some(id.as_str()))
                }
                Kind::Tag => (
                    crate::db::tags::find_full_by_id(main, id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::ConnectionProfile => (
                    crate::db::connection_profiles::find_by_id(main, id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::ImageProfile => (
                    crate::db::image_profiles::find_by_id(main, id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::EmbeddingProfile => (
                    crate::db::embedding_profiles::find_full_json_by_id(main, id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::RoleplayTemplate => (
                    crate::db::roleplay_templates::find_full_json_by_id(main, id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::Project => (
                    crate::db::projects::ProjectsRepository::new(main, mount)
                        .find_by_id(id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::Group => (
                    crate::db::groups::GroupsRepository::new(main, mount)
                        .find_by_id(id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::Chat => (
                    crate::db::chats_read::find_by_id(main, id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::Memory => {
                    // The rehydrate target's own memories may already be back (a
                    // partial restore being re-run); another character's memory
                    // refuses.
                    let row = crate::db::memories_read::find_by_id(main, id)
                        .map_err(|e| e.to_string())?;
                    let owner = row
                        .as_ref()
                        .and_then(|m| m.get("characterId").and_then(Value::as_str))
                        .map(str::to_string);
                    // v4 compares `memory?.characterId === skipTarget?.targetCharacterId`
                    // — with BOTH undefined that is true, but the arm is only
                    // consulted when the row exists and a skip target is set.
                    (row.is_some(), owner == target_character)
                }
                Kind::DocumentStore => (
                    crate::db::doc_mount_points::DocMountPointsRepository::new(mount)
                        .find_full_json_by_id(id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    is_target_vault(Some(id)),
                ),
                Kind::File => (
                    crate::db::files::FilesRepository::new(main)
                        .find_by_id(id)
                        .map_err(|e| e.to_string())?
                        .is_some(),
                    false,
                ),
                Kind::DocumentStoreFolder => {
                    let folder = folders_repo.find_by_id(id).map_err(|e| e.to_string())?;
                    let skippable =
                        is_target_vault(folder.as_ref().map(|f| f.mount_point_id.as_str()));
                    (folder.is_some(), skippable)
                }
                Kind::DocumentStoreFile => {
                    // [Bug 54] Matching bytes settle membership before the
                    // link-in-target-vault question. v4's note, verbatim:
                    //
                    // > Content rows are content-addressed and shared across
                    // > every vault holding the same bytes — a conversation
                    // > summary from a group chat lives on one row with one link
                    // > per participant. Archiving deletes the target's link but
                    // > leaves the row standing on its co-owners' links, so "is
                    // > it linked in the target vault?" answers no for content
                    // > the target legitimately owned. Matching bytes settle it
                    // > instead: the writer find-or-creates by sha256 and
                    // > discards the carried id, so an id whose content is
                    // > already present is dedup rather than a collision. A
                    // > same-id/different-bytes row is a real clash and still
                    // > refuses.
                    let existing = links_repo
                        .find_content_row_by_id(id)
                        .map_err(|e| e.to_string())?;
                    let carried_sha = lookup(&carried_content_sha, id);
                    let skippable = match (&existing, &carried_sha) {
                        (Some(row), Some(sha)) if &row.sha256 == sha => true,
                        _ => file_linked_in_target_vault(id),
                    };
                    (existing.is_some(), skippable)
                }
                Kind::DocumentStoreLink => {
                    let link = links_repo
                        .find_link_row_by_id(id)
                        .map_err(|e| e.to_string())?;
                    let skippable =
                        is_target_vault(link.as_ref().map(|l| l.mount_point_id.as_str()));
                    (link.is_some(), skippable)
                }
                Kind::DocumentStoreBlob => {
                    // [Bug 54] Same reasoning as the content row above: a blob
                    // row is 1:1 with its content row, which `linkBlobContent`
                    // resolves by sha256 before reusing whatever blob row
                    // already hangs off it.
                    let blob = blobs_repo.find_by_id(id).map_err(|e| e.to_string())?;
                    match blob {
                        None => (false, false),
                        Some(b) => {
                            let carried_sha = lookup(&carried_blob_sha, id);
                            let skippable = match &carried_sha {
                                Some(sha) if &b.sha256 == sha => true,
                                _ => file_linked_in_target_vault(&b.file_id),
                            };
                            (true, skippable)
                        }
                    }
                }
            };
            if exists {
                if skip_target.is_some() && skippable {
                    id_maps.preserve_ids_skips.insert(id.clone());
                    seen_ids.push((id.clone(), kind_name));
                    continue;
                }
                let message = format!("Preserve IDs collision for {kind_name} {id}");
                warnings.push(message.clone());
                return Err(message);
            }
            seen_ids.push((id.clone(), kind_name));
        }
    }

    Ok(())
}

/// v4 `executeImport` (`execute.ts:41`) — the full dependency-ordered pipeline
/// over the MAIN + MOUNT-INDEX connections (run inside one `Db::write` closure).
///
/// Always returns `Ok(ImportResult)` in practice: v4 wraps the whole body in one
/// `try` whose `catch` answers `success: false` with the error appended to
/// `warnings` — a chokepoint failure (a loop preamble's `findAll`, a
/// mid-transaction store failure) takes that arm rather than erroring the call.
/// The `Result` return survives for the seed/reset consumers' historical
/// signature.
pub fn execute_import(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    export: &QuilltapExport,
    options: &ImportOptions,
    // The image codec the file importers' storage bridges transcode with
    // (`7189a968`'s step 9). `None` — a caller with no codec seam (the seed
    // consumers, a host without one) — uses the not-configured codec, whose
    // transcode falls through to the original bytes, exactly as v4 behaves
    // when sharp fails.
    codec: Option<&dyn crate::services::file_storage::PixelCodec>,
) -> Result<ImportResult, ImportError> {
    let mut warnings: Vec<String> = Vec::new();
    let mut imported = ImportCounts::default();
    let mut skipped = ImportCounts::default();
    let mut id_maps = IdMaps::default();

    // v4 `getExportData` is a bare cast; the body's first read (`data.tags`)
    // throws only for null/undefined. Any other non-object (string, number,
    // array) property-reads to `undefined` everywhere, so every phase skips and
    // the result is a successful zero-count import — reproduced by the
    // empty-map walk below.
    let empty = serde_json::Map::new();
    let data = match export.data.as_object() {
        Some(d) => d,
        None if export.data.is_null() => {
            warnings
                .push("Import failed: Cannot read properties of null (reading 'tags')".to_string());
            return Ok(ImportResult {
                success: false,
                imported,
                skipped,
                warnings,
                imported_character_ids: Vec::new(),
            });
        }
        None => &empty,
    };

    // v4 runs the preflight AFTER `getExportData` and BEFORE any write; a
    // refusal answers `success: false` with the collision already in
    // `warnings` and nothing applied (`execute.ts:479`).
    if let Err(message) =
        preflight_preserve_ids(main, mount, data, options, &mut warnings, &mut id_maps)
    {
        tracing::warn!(error = %message, "Preserve IDs preflight failed");
        // `warnings` is the only channel the result carries to the user, and
        // this refusal aborts the whole import — saying nothing here is exactly
        // the silence bug 79 is about, whether the preflight refused a real
        // collision or gave up because the destination would not answer a read
        // (v4 `275cd7bc`). A collision therefore lands TWICE: once named by the
        // preflight, once wrapped by this line.
        warnings.push(format!(
            "Import refused before anything was written: {message}"
        ));
        return Ok(ImportResult {
            success: false,
            imported,
            skipped,
            warnings,
            imported_character_ids: id_maps.characters.values(),
        });
    }

    // The one-big-try body: a chokepoint error → success:false (v4's catch).
    let not_configured = crate::services::file_storage::NotConfiguredPixelCodec;
    let outcome = import_body(
        main,
        mount,
        user_id,
        data,
        options,
        codec.unwrap_or(&not_configured),
        &mut imported,
        &mut skipped,
        &mut warnings,
        &mut id_maps,
    );

    match outcome {
        Ok(()) => Ok(ImportResult {
            success: true,
            imported,
            skipped,
            warnings,
            imported_character_ids: id_maps.characters.values(),
        }),
        Err(e) => {
            // v4's catch: success:false with the error appended to warnings.
            warnings.push(format!("Import failed: {e}"));
            Ok(ImportResult {
                success: false,
                imported,
                skipped,
                warnings,
                imported_character_ids: id_maps.characters.values(),
            })
        }
    }
}

/// v4 `enqueueImportedMemoryEmbeddings` (`execute.ts:62-138`, `7189a968`) —
/// one targeted `EMBEDDING_GENERATE` per memory this import created.
///
/// Imported memories arrive with a NULL embedding on purpose (see
/// [`memories`]); without this, their semantic search stays broken until the
/// next boot's reconcile sweep runs. Mirrors the memory backfill sweeper,
/// including its reliance on the per-entity dedupe.
///
/// Deliberately *not* one `EMBEDDING_REINDEX_ALL`: that job walks every
/// character's entire memory table plus conversation chunks, help docs and
/// mount chunks — wildly disproportionate to an import of a handful of rows
/// (v4's own anti-decision at `execute.ts:56-58`).
///
/// Never throws: a failure to schedule re-indexing must not fail an import
/// whose rows are already committed. The boot reconcile remains the backstop.
///
/// **The seam** (named in the order so no one re-derives it): `execute_import`
/// runs INSIDE `db.write(...)` over raw connections, so the async
/// `queue_service::enqueue_embedding_generate(&Db, …)` cannot be called from
/// here — this is the sync connection-level shape
/// (`mount_index::embedding_scheduler::enqueue_mount_chunk_embedding`'s twin),
/// same dedupe via pending-for-entity, MEMORY priority 10.0, maxAttempts 3.
fn enqueue_imported_memory_embeddings(
    main: &rusqlite::Connection,
    user_id: &str,
    memory_refs: &[(String, String)],
    warnings: &mut Vec<String>,
) {
    if memory_refs.is_empty() {
        return;
    }

    // v4 `getDefaultEmbeddingProfile(userId)` → `findDefault(userId)`:
    // `{userId, isDefault: true}`, NO first-row fallback. A read failure is
    // v4's caught arm (logged, profile = null).
    let profile: Option<(String, String)> = main
        .query_row(
            "SELECT id, provider FROM embedding_profiles \
             WHERE userId = ?1 AND isDefault = 1 LIMIT 1",
            rusqlite::params![user_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .map(Some)
        .unwrap_or_else(|e| {
            if !matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                tracing::warn!(error = %e, "Failed to resolve default embedding profile after import");
            }
            None
        });

    // v4 `01e481f6` rewrote this arm: the **system default profile**, whatever
    // its provider, embeds everything — memories, chunks, help docs — so
    // imported rows use it too, never a different or second-guessed one. Only
    // the no-profile-at-all case warns now; the BUILTIN early-return is gone.
    let (profile_id, provider) = match profile {
        Some((id, provider)) => (id, provider),
        None => {
            warnings.push(format!(
                "{} memories were imported without embeddings because no default \
                 embedding profile is configured; they will be indexed once one is.",
                memory_refs.len()
            ));
            return;
        }
    };
    // ⚠ DEFERRED LOUDLY — v4's BUILTIN follow-up (`scheduleRefit`).
    //
    // v4 additionally calls `scheduleRefit(userId, profile.id)` for a BUILTIN
    // default (`lib/embedding/embedding-job-scheduler.ts:32`): a corpus-derived
    // TF-IDF vocabulary just grew, so a refit-with-reindex is queued. That
    // helper is a **5-second debounced in-process `setTimeout`**, which is a
    // host-cadence seam under this port's locked job-runner rule (timers are
    // the host's), and v5 has no refit scheduler at all — its only
    // `EMBEDDING_REFIT` enqueue is the async `queue_service::
    // enqueue_embedding_refit(&Db, …)`, unreachable from inside the
    // `Db::write` closure this runs in. Nothing is stubbed: the per-row
    // enqueues below run for BUILTIN exactly as v4's now do, so imported rows
    // are embedded; only the vocabulary refit that would improve those vectors
    // is missing, and the boot reconcile remains the backstop.
    //
    // The differential cannot see it either way — v4's debounce means the job
    // row does not exist when `executeImport` returns and the state is dumped.
    // Recorded in the P4.D62 lane record; the wire belongs with a host-side
    // refit scheduler.
    if provider == "BUILTIN" {
        tracing::debug!(
            profile_id = %profile_id,
            "Imported memories embed under the BUILTIN default; the debounced \
             vocabulary refit v4 schedules here is not ported (P4.D62 deferral)"
        );
    }

    let jobs = crate::db::background_jobs::BackgroundJobsRepository::new(main);
    let mut enqueued = 0usize;
    for (memory_id, character_id) in memory_refs {
        // v4 `enqueueEmbeddingGenerate`'s dedupe: an in-flight
        // EMBEDDING_GENERATE for the same entity means `isNew: false`.
        let is_new = (|| -> Result<bool, DbError> {
            let pending = jobs.find_pending_for_entity(memory_id)?;
            if pending.iter().any(|j| j.job_type == "EMBEDDING_GENERATE") {
                return Ok(false);
            }
            let now = crate::clock::now_iso();
            jobs.create(
                &crate::db::background_jobs::BjCreate {
                    user_id: user_id.to_string(),
                    job_type: "EMBEDDING_GENERATE".to_string(),
                    status: Some("PENDING".to_string()),
                    // v4's caller-literal key order (`execute.ts:106-111`).
                    payload: serde_json::json!({
                        "entityType": "MEMORY",
                        "entityId": memory_id,
                        "characterId": character_id,
                        "profileId": profile_id,
                    }),
                    // EMBEDDING_ENTITY_PRIORITIES['MEMORY'] = 10.
                    priority: 10.0,
                    attempts: 0.0,
                    max_attempts: 3.0,
                    last_error: None,
                    scheduled_at: now.clone(),
                    started_at: None,
                    completed_at: None,
                },
                &crate::db::background_jobs::CreateOptions {
                    id: uuid::Uuid::new_v4().to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            Ok(true)
        })();
        match is_new {
            Ok(true) => enqueued += 1,
            Ok(false) => {}
            // v4 warns to the logger and continues — no `warnings` entry.
            Err(e) => {
                tracing::warn!(memory_id = %memory_id, error = %e,
                    "Failed to enqueue embedding job for imported memory");
            }
        }
    }

    if enqueued < memory_refs.len() {
        warnings.push(format!(
            "{} of {} imported memories could not be queued for embedding; \
             the next startup sweep will pick them up.",
            memory_refs.len() - enqueued,
            memory_refs.len()
        ));
    }
}

#[cfg(test)]
mod preserve_ids_helper_tests {
    use super::*;

    /// v4's dead `getPreserveIdsCreateOptions` — pinned rather than merely
    /// present, so the shape survives if a future round gives it a caller.
    #[test]
    fn dead_helper_matches_v4s_shape() {
        let mut off = ImportOptions::seed_defaults();
        assert_eq!(get_preserve_ids_create_options(Some("abc"), &off), None);
        off.preserve_ids = true;
        assert_eq!(
            get_preserve_ids_create_options(Some("abc"), &off),
            Some("abc".to_string())
        );
        // A falsy source id short-circuits BEFORE the flag is read.
        assert_eq!(get_preserve_ids_create_options(None, &off), None);
        assert_eq!(get_preserve_ids_create_options(Some(""), &off), None);
    }

    /// The live fork the importers actually use.
    #[test]
    fn mint_or_preserve_claims_only_under_the_flag() {
        let mut opts = ImportOptions::seed_defaults();
        let (minted, _) = mint_or_preserve(&opts, "abc");
        assert_ne!(minted, "abc");
        opts.preserve_ids = true;
        let (claimed, _) = mint_or_preserve(&opts, "abc");
        assert_eq!(claimed, "abc");
        // An absent source id still mints (v4's `?? randomUUID()`).
        let (fallback, _) = mint_or_preserve(&opts, "");
        assert_ne!(fallback, "");
        assert_ne!(fallback, "abc");
    }
}

/// A non-empty JSON array under `key` (v4's `data.<key> && data.<key>.length > 0`
/// gate — only a non-empty ARRAY iterates in practice).
fn non_empty_array<'a>(
    data: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Option<&'a Vec<Value>> {
    data.get(key)
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
}

/// The dependency-ordered import steps — v4 `executeImport`'s try body.
/// Extracted so the mutable borrows end before `execute_import` builds the
/// result.
#[allow(clippy::too_many_arguments)]
fn import_body(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    data: &serde_json::Map<String, Value>,
    options: &ImportOptions,
    codec: &dyn crate::services::file_storage::PixelCodec,
    imported: &mut ImportCounts,
    skipped: &mut ImportCounts,
    warnings: &mut Vec<String>,
    id_maps: &mut IdMaps,
) -> Result<(), DbError> {
    // 1. Tags (no dependencies).
    if let Some(items) = non_empty_array(data, "tags") {
        let c = entities::import_tags(main, user_id, items, options, &mut id_maps.tags, warnings)?;
        imported.tags = c.imported;
        skipped.tags = c.skipped;
    }

    // 2. Connection profiles.
    if let Some(items) = non_empty_array(data, "connectionProfiles") {
        let c = profiles::import_connection_profiles(
            main,
            user_id,
            items,
            options,
            &mut id_maps.connection_profiles,
            warnings,
        )?;
        imported.connection_profiles = c.imported;
        skipped.connection_profiles = c.skipped;
    }

    // 3. Image profiles.
    if let Some(items) = non_empty_array(data, "imageProfiles") {
        let c = profiles::import_image_profiles(
            main,
            user_id,
            items,
            options,
            &mut id_maps.image_profiles,
            warnings,
        )?;
        imported.image_profiles = c.imported;
        skipped.image_profiles = c.skipped;
    }

    // 4. Embedding profiles.
    if let Some(items) = non_empty_array(data, "embeddingProfiles") {
        let c = profiles::import_embedding_profiles(
            main,
            user_id,
            items,
            options,
            &mut id_maps.embedding_profiles,
            warnings,
        )?;
        imported.embedding_profiles = c.imported;
        skipped.embedding_profiles = c.skipped;
    }

    // 5. Roleplay templates.
    if let Some(items) = non_empty_array(data, "roleplayTemplates") {
        let c = entities::import_roleplay_templates(
            main,
            user_id,
            items,
            options,
            &mut id_maps.roleplay_templates,
            warnings,
        )?;
        imported.roleplay_templates = c.imported;
        skipped.roleplay_templates = c.skipped;
    }

    // 5.5. Projects (before characters since projects reference characters in
    // roster).
    if let Some(items) = non_empty_array(data, "projects") {
        let c = entities::import_projects(
            main,
            mount,
            items,
            options,
            &mut id_maps.projects,
            warnings,
        )?;
        imported.projects = c.imported;
        skipped.projects = c.skipped;
    }

    // 5.6. Groups (before characters since groups reference characters in
    // membership).
    if let Some(items) = non_empty_array(data, "groups") {
        let c =
            entities::import_groups(main, mount, items, options, &mut id_maps.groups, warnings)?;
        imported.groups = c.imported;
        skipped.groups = c.skipped;
    }

    // 6. Characters.
    if let Some(items) = non_empty_array(data, "characters") {
        let c =
            characters::import_characters(main, mount, user_id, items, options, id_maps, warnings)?;
        imported.characters = c.imported;
        skipped.characters = c.skipped;
    }

    // 7. Chats.
    if let Some(items) = non_empty_array(data, "chats") {
        let c =
            entities::import_chats(main, user_id, items, options, &mut id_maps.chats, warnings)?;
        imported.chats = c.imported;
        imported.messages = c.messages;
        skipped.chats = c.skipped;
    }

    // 7a. Conversation annotations attached to imported chats. Remap chatId
    // through the chats map; sourceMessageId stays as-is because the message
    // import preserves message ids.
    if let Some(items) = non_empty_array(data, "conversationAnnotations") {
        let repo =
            crate::db::conversation_annotations::ConversationAnnotationsRepository::new(main);
        let mut annotations_imported = 0u32;
        for annotation in items {
            let source_chat_id = annotation
                .get("chatId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let remapped_chat_id = id_maps
                .chats
                .get(source_chat_id)
                .unwrap_or(source_chat_id)
                .to_string();
            let now = crate::clock::now_iso();
            let out = repo.create(
                &crate::db::conversation_annotations::CaCreate {
                    chat_id: remapped_chat_id,
                    message_index: annotation
                        .get("messageIndex")
                        .and_then(Value::as_f64)
                        .unwrap_or_default(),
                    source_message_id: annotation
                        .get("sourceMessageId")
                        .and_then(Value::as_str)
                        .map(|s| s.to_string()),
                    character_name: annotation
                        .get("characterName")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    content: annotation
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                },
                &crate::db::conversation_annotations::CreateOptions {
                    id: uuid::Uuid::new_v4().to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            match out {
                Ok(()) => annotations_imported += 1,
                Err(e) => warnings.push(format!("Failed to import conversation annotation: {e}")),
            }
        }
        imported.conversation_annotations = Some(annotations_imported);
    }

    // 7b. Chat documents (Document Mode pane state). Remap chatId; the rest is
    // opaque path/scope metadata that survives without remapping.
    if let Some(items) = non_empty_array(data, "chatDocuments") {
        let repo = crate::db::chat_documents::ChatDocumentsRepository::new(main);
        let mut chat_docs_imported = 0u32;
        for cd in items {
            let source_chat_id = cd.get("chatId").and_then(Value::as_str).unwrap_or_default();
            let remapped_chat_id = id_maps
                .chats
                .get(source_chat_id)
                .unwrap_or(source_chat_id)
                .to_string();
            let now = crate::clock::now_iso();
            let out = repo.create(
                &crate::db::chat_documents::CdCreate {
                    chat_id: remapped_chat_id,
                    file_path: cd
                        .get("filePath")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    scope: cd
                        .get("scope")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    mount_point: cd
                        .get("mountPoint")
                        .and_then(Value::as_str)
                        .map(|s| s.to_string()),
                    display_title: cd
                        .get("displayTitle")
                        .and_then(Value::as_str)
                        .map(|s| s.to_string()),
                    is_active: cd.get("isActive").and_then(Value::as_bool).unwrap_or(false),
                },
                &crate::db::chat_documents::CreateOptions {
                    id: uuid::Uuid::new_v4().to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            match out {
                Ok(()) => chat_docs_imported += 1,
                Err(e) => warnings.push(format!("Failed to import chat document: {e}")),
            }
        }
        imported.chat_documents = Some(chat_docs_imported);
    }

    // 7c. Document stores (Scriptorium) — mount point configs plus, for
    //    database-backed mounts, folder structures, document bodies and blobs.
    //
    //    Must run *before* the group↔store link step below (v4 `7189a968`,
    //    `execute.ts:382-407`): those links resolve through
    //    `id_maps.mount_points`, which only this importer populates. It ran
    //    dead-last for a long time, so in a mixed archive every group's linked
    //    stores were silently dropped. It still has to follow importProjects —
    //    its project links remap through `id_maps.projects`.
    //
    //    A characters bundle carries each character's vault here too (WP A2,
    //    `01e481f6`). Vaults belonging to characters the conflict strategy
    //    skipped are dropped: the existing character keeps the vault it already
    //    has, so importing the bundle's copy would strand an orphan store.
    //    Dropping the mount point is enough — its folders, documents and blobs
    //    resolve through `id_maps.mount_points` and are skipped with it.
    let importable_mount_points: Vec<Value> = data
        .get("mountPoints")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|mp| !id_maps.skipped_character_vaults.contains(&id_of(mp)))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if !importable_mount_points.is_empty() {
        let mount_points = importable_mount_points;
        let arr = |key: &str| -> Vec<Value> {
            data.get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let c = document_stores::import_document_stores(
            mount,
            &mount_points,
            &arr("folders"),
            &arr("documents"),
            &arr("blobs"),
            &arr("projectLinks"),
            options,
            id_maps,
            warnings,
        )?;
        imported.document_stores = Some(c.mount_points);
        imported.document_store_folders = Some(c.folders);
        imported.document_store_documents = Some(c.documents);
        imported.document_store_blobs = Some(c.blobs);
        imported.document_store_project_links = Some(c.project_links);
    }

    // 7d. Group character membership and linked document stores. Remap group and
    // character ids; skip members/links that don't exist in the import.
    if let Some(groups_data) = non_empty_array(data, "groups") {
        let members_repo =
            crate::db::group_character_members::GroupCharacterMembersRepository::new(mount);
        let links_repo = crate::db::group_doc_mount_links::GroupDocMountLinksRepository::new(mount);
        for group_export in groups_data {
            let source_group_id = id_of(group_export);
            let remapped_group_id = id_maps
                .groups
                .get(&source_group_id)
                .unwrap_or(source_group_id.as_str())
                .to_string();

            // Re-establish character membership.
            if let Some(member_ids) = group_export
                .get("_memberCharacterIds")
                .and_then(Value::as_array)
            {
                for character_id in member_ids.iter().filter_map(Value::as_str) {
                    let Some(remapped_character_id) = id_maps.characters.get(character_id) else {
                        // v4 debug-logs and skips — the character is not in the import.
                        continue;
                    };
                    if let Err(e) =
                        members_repo.add_member(&remapped_group_id, remapped_character_id)
                    {
                        // v4: logged, no warnings entry.
                        tracing::warn!(
                            group_id = %remapped_group_id,
                            character_id = %remapped_character_id,
                            error = %e,
                            "Failed to add group member"
                        );
                    }
                }
            }

            // Link additional document stores (beyond the official mount point).
            if let Some(store_ids) = group_export
                .get("_linkedStoreMountPointIds")
                .and_then(Value::as_array)
            {
                for mount_point_id in store_ids.iter().filter_map(Value::as_str) {
                    let Some(remapped_mount_point_id) = id_maps.mount_points.get(mount_point_id)
                    else {
                        continue;
                    };
                    if let Err(e) = links_repo.link(&remapped_group_id, remapped_mount_point_id) {
                        tracing::warn!(
                            group_id = %remapped_group_id,
                            mount_point_id = %remapped_mount_point_id,
                            error = %e,
                            "Failed to link document store to group"
                        );
                    }
                }
            }
        }
    }

    // 8. Memories (if includeMemories is enabled).
    let mut imported_memory_refs: Vec<(String, String)> = Vec::new();
    if options.include_memories {
        if let Some(items) = non_empty_array(data, "memories") {
            let c = memories::import_memories(main, items, options, id_maps, warnings)?;
            imported.memories = c.imported;
            skipped.memories = c.skipped;
            imported_memory_refs = c.created_ids;
        }
    }

    // 9. General file library (`7189a968`). Runs after projects (folders and
    //    files remap projectId) and after characters/chats (linkedTo
    //    resolution).
    if let Some(items) = non_empty_array(data, "files") {
        let folder_items = data
            .get("folders")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let c = files::import_files(
            main,
            mount,
            codec,
            user_id,
            items,
            &folder_items,
            options,
            id_maps,
            warnings,
        )?;
        imported.files = Some(c.files);
        imported.folders = Some(c.folders);
        skipped.files = Some(c.skipped);
    }

    // 10. Configuration-shaped types (`7189a968`). None of these are
    //     referenced by id, so they have no ordering constraint against the
    //     entity importers.
    if let Some(items) = non_empty_array(data, "promptTemplates") {
        let c = configuration::import_prompt_templates(main, user_id, items, options, warnings)?;
        imported.prompt_templates = Some(c.imported);
        skipped.prompt_templates = Some(c.skipped);
    }
    if let Some(items) = non_empty_array(data, "providerModels") {
        let c = configuration::import_provider_models(main, items, warnings)?;
        imported.provider_models = Some(c.imported);
        skipped.provider_models = Some(c.skipped);
    }
    if let Some(items) = non_empty_array(data, "pluginConfigs") {
        let c = configuration::import_plugin_configs(main, user_id, items, warnings)?;
        imported.plugin_configs = Some(c.imported);
        skipped.plugin_configs = Some(c.skipped);
    }
    if let Some(items) = non_empty_array(data, "instanceSettings") {
        let c = configuration::import_instance_settings(main, items, warnings)?;
        imported.instance_settings = Some(c.imported);
        skipped.instance_settings = Some(c.skipped);
    }

    // Post-import reconciliation.
    reconcile::reconcile_relationships(main, mount, id_maps, warnings);

    // Re-embed what we just inserted (v4 `execute.ts:532`, AFTER the
    // reconcile). Imported memories carry no vector, and without this their
    // semantic search stays broken until the next boot's reconcile sweep runs.
    enqueue_imported_memory_embeddings(main, user_id, &imported_memory_refs, warnings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;

    fn refuses_parse(s: &str) -> ImportError {
        parse_export_file(s).expect_err("should refuse")
    }

    /// [P4.D91 §3] The warning's quoted `<name>` is a JS template-literal
    /// interpolation of the RAW item's `name` in v4 (`` `…"${tag.name}"…` ``),
    /// and the arm fires exactly when the typed parse failed — so the field can
    /// be any JSON shape, or absent. Transcribed `${…}` semantics, backed by
    /// `pascal::js_value::to_js_string`'s own pins for `String(value)`.
    #[test]
    fn warning_display_name_follows_js_interpolation() {
        assert_eq!(warning_display_name(&json!({})), "undefined");
        assert_eq!(warning_display_name(&json!({ "name": null })), "null");
        assert_eq!(warning_display_name(&json!({ "name": "A Tag" })), "A Tag");
        assert_eq!(warning_display_name(&json!({ "name": 7 })), "7");
        assert_eq!(warning_display_name(&json!({ "name": ["a", "b"] })), "a,b");
        assert_eq!(
            warning_display_name(&json!({ "name": { "x": 1 } })),
            "[object Object]"
        );
    }

    #[test]
    fn parse_pins_format_and_version() {
        // Good manifest, empty data → parses.
        let ok = json!({"manifest": {"format": "quilltap-export", "version": "1.0"}, "data": {}});
        assert!(parse_export_file(&ok.to_string()).is_ok());

        // Wrong format.
        let bad_fmt =
            json!({"manifest": {"format": "something-else", "version": "1.0"}, "data": {}});
        assert!(matches!(
            refuses_parse(&bad_fmt.to_string()),
            ImportError::InvalidFormat { .. }
        ));

        // Wrong version.
        let bad_ver =
            json!({"manifest": {"format": "quilltap-export", "version": "2.0"}, "data": {}});
        assert!(matches!(
            refuses_parse(&bad_ver.to_string()),
            ImportError::UnsupportedVersion { .. }
        ));

        // Missing manifest.
        let no_manifest = json!({"data": {}});
        assert!(matches!(
            refuses_parse(&no_manifest.to_string()),
            ImportError::MissingManifest
        ));

        // Missing data.
        let no_data = json!({"manifest": {"format": "quilltap-export", "version": "1.0"}});
        assert!(matches!(
            refuses_parse(&no_data.to_string()),
            ImportError::MissingData
        ));
    }

    #[test]
    fn ndjson_serialization_refuses_on_sniff() {
        // An NDJSON stream is line-delimited (not one JSON object) — the sniff fires
        // on the parse failure.
        let ndjson = "{\"format\":\"qtap-ndjson\",\"version\":1}\n{\"kind\":\"character\"}\n";
        assert!(matches!(refuses_parse(ndjson), ImportError::Ndjson));

        // Even a single-object payload claiming the NDJSON format refuses.
        let claim = json!({"manifest": {"format": "qtap-ndjson", "version": 1}, "data": {}});
        assert!(matches!(
            refuses_parse(&claim.to_string()),
            ImportError::Ndjson
        ));
    }

    #[test]
    fn empty_supported_payload_is_a_clean_no_op() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let export = QuilltapExport {
            manifest: json!({"format": "quilltap-export", "version": "1.0"}),
            data: json!({"characters": [], "memories": []}),
        };
        let result = execute_import(
            &main,
            &mount,
            "user",
            &export,
            &ImportOptions::seed_defaults(),
            None,
        )
        .expect("empty payload imports cleanly");
        assert!(result.success);
        assert_eq!(result.imported, ImportCounts::default());
        assert!(result.imported_character_ids.is_empty());
    }

    #[test]
    fn null_data_takes_v4s_typeerror_catch() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let export = QuilltapExport {
            manifest: json!({"format": "quilltap-export", "version": "1.0"}),
            data: Value::Null,
        };
        let result = execute_import(
            &main,
            &mount,
            "user",
            &export,
            &ImportOptions::seed_defaults(),
            None,
        )
        .expect("null data answers success:false, not Err");
        assert!(!result.success);
        assert_eq!(
            result.warnings,
            vec!["Import failed: Cannot read properties of null (reading 'tags')".to_string()]
        );
    }

    #[test]
    fn result_body_key_order_is_v4s() {
        let imported = ImportCounts {
            conversation_annotations: Some(2),
            document_stores: Some(1),
            ..Default::default()
        };
        let result = ImportResult {
            success: true,
            imported,
            skipped: ImportCounts::default(),
            warnings: vec![],
            imported_character_ids: vec![],
        };
        let v = result.to_value();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(|s| s.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "success",
                "imported",
                "skipped",
                "warnings",
                "importedCharacterIds"
            ]
        );
        let imported_keys: Vec<&str> = v["imported"]
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(
            imported_keys,
            vec![
                "characters",
                "chats",
                "messages",
                "roleplayTemplates",
                "connectionProfiles",
                "imageProfiles",
                "embeddingProfiles",
                "tags",
                "memories",
                "projects",
                "groups",
                "conversationAnnotations",
                "documentStores",
            ]
        );
        // The skipped bag never gains the optional extras.
        let skipped_keys = v["skipped"].as_object().unwrap().len();
        assert_eq!(skipped_keys, 11);
    }

    // ── [P4.48] the read-failure propagation sweep ───────────────────────────
    //
    // Two plants, both cheap and both exercised through the PUBLIC entry points
    // so a regression at any single site is caught:
    //
    // 1. **No tables at all.** Every `find…` errors with `no such table`. This
    //    is the DB-read-error leg — the one v4 swallows (`safeQuery(…, null)`)
    //    and v5 now refuses, the ruled divergence.
    // 2. **A `projects` row whose `officialMountPointId` is NULL.** The slim
    //    read succeeds, then `apply_overlay_one` raises `Unavailable` without
    //    touching the mount DB. This is the leg where v4 GENUINELY throws
    //    (`applyOverlayOne` is not wrapped in `safeQuery`), so it is a plain
    //    fidelity assertion, not a divergence.
    //
    // The discriminator that makes these mutation-sensitive: a preflight
    // refusal answers `success:false` with `warnings` UNTOUCHED, where a
    // failure inside the import body appends `Import failed: …`. Asserting the
    // warnings are EMPTY therefore proves the refusal happened in the preflight
    // and not later. Restore any one site to `.ok().flatten()` and the matching
    // case flips to a body failure (or to success), reddening here.

    fn preserve_ids_options() -> ImportOptions {
        ImportOptions {
            preserve_ids: true,
            ..ImportOptions::seed_defaults()
        }
    }

    fn export_with(kind: &str, item: Value) -> QuilltapExport {
        QuilltapExport {
            manifest: json!({"format": "quilltap-export", "version": "1.0"}),
            data: json!({ kind: [item] }),
        }
    }

    /// A `projects` slim table holding one row with a NULL `officialMountPointId`.
    fn main_with_storeless_project(id: &str) -> Connection {
        let main = Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE projects (
                 id TEXT PRIMARY KEY,
                 name TEXT,
                 officialMountPointId TEXT,
                 createdAt TEXT,
                 updatedAt TEXT
             );",
        )
        .unwrap();
        main.execute(
            "INSERT INTO projects (id, name, officialMountPointId, createdAt, updatedAt)
             VALUES (?1, 'Planted', NULL, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            rusqlite::params![id],
        )
        .unwrap();
        main
    }

    /// Every preflight kind, against a database with no tables at all: the
    /// import must refuse BEFORE writing, with `warnings` untouched.
    #[test]
    fn preflight_refuses_when_the_existence_read_fails() {
        // (payload key, a minimally-shaped item) — one per `checks` entry that
        // reads through a repository. The document-store kinds are driven by
        // `documents`/`blobs` records rather than a top-level array.
        let cases: Vec<(&str, Value)> = vec![
            ("characters", json!({"id": "c1", "name": "C"})),
            ("tags", json!({"id": "t1", "name": "T"})),
            ("connectionProfiles", json!({"id": "cp1", "name": "P"})),
            ("imageProfiles", json!({"id": "ip1", "name": "P"})),
            ("embeddingProfiles", json!({"id": "ep1", "name": "P"})),
            ("roleplayTemplates", json!({"id": "rt1", "name": "R"})),
            ("projects", json!({"id": "pr1", "name": "P"})),
            ("groups", json!({"id": "g1", "name": "G"})),
            ("chats", json!({"id": "ch1", "title": "C"})),
            ("memories", json!({"id": "m1", "characterId": "c1"})),
            ("mountPoints", json!({"id": "mp1", "name": "S"})),
            ("files", json!({"id": "f1", "originalFilename": "a.png"})),
        ];

        for (key, item) in cases {
            let main = Connection::open_in_memory().unwrap();
            let mount = Connection::open_in_memory().unwrap();
            let result = execute_import(
                &main,
                &mount,
                "user",
                &export_with(key, item),
                &preserve_ids_options(),
                None,
            )
            .expect("a refused preflight is still Ok(success:false)");
            assert!(!result.success, "{key}: unreadable instance must refuse");
            // [P4.D91] The preflight's refusal used to leave `warnings` empty;
            // since v4 `275cd7bc` it names itself. That still tells the two
            // refusals apart — a BODY failure answers `Import failed: …`, and
            // it is one line either way, so nothing ran past the preflight.
            assert_eq!(
                result.warnings.len(),
                1,
                "{key}: exactly one refusal line — got {:?}",
                result.warnings
            );
            assert!(
                result.warnings[0].starts_with("Import refused before anything was written: "),
                "{key}: the refusal must come from the preflight, not from a \
                 body failure — got {:?}",
                result.warnings
            );
            assert_eq!(
                result.imported,
                ImportCounts::default(),
                "{key}: nothing may be written"
            );
        }
    }

    /// The document-store kinds ride `documents`/`blobs`, so they need their own
    /// payload shape — same refusal.
    #[test]
    fn preflight_refuses_for_the_document_store_kinds() {
        let payloads = vec![
            (
                "folder",
                json!({"folders": [{"id": "fo1", "mountPointId": "mp1"}]}),
            ),
            (
                "document",
                json!({"documents": [{"fileId": "df1", "linkId": "dl1", "contentSha256": "s"}]}),
            ),
            (
                "blob",
                json!({"blobs": [{"blobId": "b1", "fileId": "bf1", "linkId": "bl1", "sha256": "s"}]}),
            ),
        ];
        for (label, data) in payloads {
            let main = Connection::open_in_memory().unwrap();
            let mount = Connection::open_in_memory().unwrap();
            let export = QuilltapExport {
                manifest: json!({"format": "quilltap-export", "version": "1.0"}),
                data,
            };
            let result = execute_import(
                &main,
                &mount,
                "user",
                &export,
                &preserve_ids_options(),
                None,
            )
            .expect("a refused preflight is still Ok(success:false)");
            assert!(!result.success, "{label}: unreadable vault must refuse");
            assert_eq!(
                result.warnings.len(),
                1,
                "{label}: exactly one refusal line — got {:?}",
                result.warnings
            );
            assert!(
                result.warnings[0].starts_with("Import refused before anything was written: "),
                "{label}: refusal must be the preflight's — got {:?}",
                result.warnings
            );
        }
    }

    /// The overlay leg: v4's `applyOverlayOne` throws here too, so this is
    /// fidelity, not divergence.
    #[test]
    fn preflight_refuses_when_a_colliding_projects_store_is_unavailable() {
        let main = main_with_storeless_project("pr1");
        let mount = Connection::open_in_memory().unwrap();
        let result = execute_import(
            &main,
            &mount,
            "user",
            &export_with("projects", json!({"id": "pr1", "name": "P"})),
            &preserve_ids_options(),
            None,
        )
        .expect("a refused preflight is still Ok(success:false)");
        assert!(!result.success);
        assert_eq!(
            result.warnings,
            vec![
                "Import refused before anything was written: Project pr1 has no usable \
                 document store (officialMountPointId=null): officialMountPointId is null"
                    .to_string()
            ],
            "the overlay throw must land in the preflight's catch, which names the \
             refusal and stops (v4 `execute.ts:483` + `275cd7bc`)"
        );
    }

    /// With `preserve_ids` OFF the preflight is a no-op, so the same plant is
    /// felt by `import_projects` instead — where v4's per-item `catch` turns the
    /// throw into a named warning and drops the item.
    #[test]
    fn import_projects_warns_when_the_existence_read_fails() {
        let main = main_with_storeless_project("pr1");
        let mount = Connection::open_in_memory().unwrap();
        let result = execute_import(
            &main,
            &mount,
            "user",
            &export_with("projects", json!({"id": "pr1", "name": "Planted"})),
            &ImportOptions::seed_defaults(),
            None,
        )
        .expect("per-item failures never sink the import");
        assert_eq!(
            result.imported.projects, 0,
            "the item must be dropped, not created"
        );
        // The warning must name the OVERLAY failure. Asserting only the
        // `Failed to import project "…"` prefix would be vacuous: with the read
        // swallowed the importer falls through to `create`, which fails against
        // this stub database too and pushes an identically-shaped warning. The
        // message body is what discriminates the fixed site from the broken one.
        assert!(
            result.warnings.iter().any(|w| w.starts_with(
                "Failed to import project \"Planted\": Project pr1 has no usable \
                 document store (officialMountPointId=null)"
            )),
            "v4's per-item catch must carry the overlay failure verbatim — got {:?}",
            result.warnings
        );
    }
}
