//! The `.qtap` **export writer** (P4.9G4) — v4 `lib/export/ndjson-writer.ts`
//! (the streaming NDJSON serialization, qtap-ndjson v1) plus
//! `lib/export/quilltap-export-service.ts`'s `previewExport` and the route-level
//! `handleExportEntities` listing.
//!
//! ## The record sequence is the contract
//!
//! v4 streams one `JSON.stringify(record) + '\n'` per line, in this exact order
//! ([`stream_export_records`], v4 `streamExportRecords` :658):
//!
//! 1. `__envelope__` — `{kind, format:'qtap-ndjson', version:1, manifest}` where
//!    the manifest's `counts` is **an empty object**. That quirk is deliberate in
//!    v4 (the writer has emitted nothing yet) and is carried verbatim.
//! 2. the per-type record stream (one generator per [`ExportEntityType`]).
//! 3. `__footer__` — `{kind, counts}` with the authoritative counts.
//!
//! Rust has no async generators, so the per-type generators return a `Vec<Value>`
//! built in yield order. The **line sequence** and each line's **key order** are
//! what the differential diffs, so every record is assembled with
//! `serde_json::Map` inserts in v4's spread order (`preserve_order` is on
//! workspace-wide — see the memory note `llm-logs-key-order-and-preserve-order`).
//!
//! ## Counts are insertion-ordered
//!
//! v4's `bump()` does `counts[key] = (counts[key] ?? 0) + delta`, so the footer's
//! key order is *first-bump* order, not a fixed schema order. [`Counts`] is a
//! `serde_json::Map` bumped the same way.
//!
//! ## What is NOT here
//!
//! The legacy monolithic-JSON writer does not exist in v4 either (only the reader
//! side keeps back-compat) — `.qtap` files v5 writes are always qtap-ndjson v1.

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::document_store_overlay::OverlayError;
use crate::db::{
    characters_read, chats_read, connection_profiles, embedding_profiles, groups, image_profiles,
    projects, roleplay_templates, tags, DbError,
};

mod entities;
mod excluded_files;
mod key_order;
mod preview;
mod records;

use excluded_files::is_file_excluded_from_export;

pub use entities::{export_entities, unknown_entity_type_message};
pub use preview::preview_export;

/// Raw bytes per blob chunk (v4 `BLOB_CHUNK_BYTES` :38) — 3 MB raw → ~4 MB base64.
const BLOB_CHUNK_BYTES: usize = 3 * 1024 * 1024;

/// v4 `QTAP_NDJSON_CONTENT_TYPE` (`ndjson-writer.ts:784`).
pub const QTAP_NDJSON_CONTENT_TYPE: &str = "application/x-ndjson";

/// v4 `ExportEntityType` (`lib/export/types.ts:54`, `7189a968`) — the fifteen
/// exportable kinds, in v4's declaration order. Each export carries exactly ONE
/// type; no mixed exports.
///
/// v4's own checklist rides here verbatim (`types.ts:31-53`):
///
/// > Adding a member here is a cross-cutting change, not a one-liner. Every one
/// > of these layers must gain a matching case or the records are written and
/// > then silently evaporate on the way back in:
/// >
/// >   - `QuilltapExportCounts` key + `Qtap*Record` interface + `QtapRecord` union (this file)
/// >   - `lib/export/ndjson-writer.ts`: `stream*` generator, `resolveExportIds`, `streamExportRecords`
/// >   - `lib/export/quilltap-export-service.ts`: `previewExport` (its `default` throws)
/// >   - `lib/import/quilltap-import-stream.ts`: record `switch`, `CollectedArrays`,
/// >     `buildExportDataForType` (its `default` throws)
/// >   - `lib/import/quilltap-import/types.ts`: `AnyExportData` field
/// >   - `lib/import/quilltap-import/`: importer module, wired into `execute.ts`
/// >   - `components/tools/import-export/types.ts`: `ENTITY_TYPE_LABELS`
/// >   - `components/tools/import-export/steps/ExportTypeStep.tsx`: `EXPORTABLE_TYPES`
/// >   - `app/api/v1/system/tools/route.ts`: `handleExportEntities`
/// >   - `public/schemas/qtap-export.schema.json` + `qtap-export-ndjson.schema.json`
/// >
/// > Format compatibility: the NDJSON envelope stays `version: 1`. An older
/// > build warns-and-skips record kinds it doesn't know, but *throws* on an
/// > unknown `manifest.exportType` (`buildExportDataForType`), so it cannot
/// > consume an archive of a newly-added type. That's deliberate — the thrown
/// > message names the type plainly — but it means new types are forward-only.
///
/// The v5 analogs of those layers: this module's generators +
/// `resolve_export_ids` + `stream_export_records`, [`preview::preview_export`],
/// `quilltap_import::ndjson`'s record switch + `Collected` +
/// `build_export_data_for_type`, the `quilltap_import` importer modules wired
/// into `execute_import`, [`entities::export_entities`], and the SPA's picker
/// (P4.D47's half).
pub const EXPORT_ENTITY_TYPES: &[&str] = &[
    "characters",
    "chats",
    "roleplay-templates",
    "prompt-templates",
    "connection-profiles",
    "image-profiles",
    "embedding-profiles",
    "tags",
    "projects",
    "groups",
    "document-stores",
    "files",
    "provider-models",
    "plugin-configs",
    "instance-settings",
];

/// v4 `ExportOptions` (`lib/export/types.ts:586`).
#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub entity_type: String,
    /// `'all'` | `'selected'`. v4 never validates it — any other value behaves
    /// like `'all'` (only `=== 'selected'` is tested).
    pub scope: String,
    pub selected_ids: Vec<String>,
    pub include_memories: bool,
}

/// The one failure mode v4's writer throws (`Unknown export type: ${type}`) plus
/// the DB errors the Rust reads can surface (v4's repos swallow those into `[]`;
/// where v4 swallows, so do we — see the per-generator comments).
#[derive(Debug)]
pub enum ExportError {
    /// v4 `throw new Error(\`Unknown export type: ${options.type}\`)` — thrown by
    /// both `resolveExportIds` :650 and `streamExportRecords` :714.
    UnknownType(String),
    Db(DbError),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::UnknownType(t) => write!(f, "Unknown export type: {t}"),
            ExportError::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<DbError> for ExportError {
    fn from(e: DbError) -> Self {
        ExportError::Db(e)
    }
}

impl From<OverlayError> for ExportError {
    fn from(e: OverlayError) -> Self {
        ExportError::Db(match e {
            OverlayError::Db(d) => d,
            other => DbError::Key(other.to_string()),
        })
    }
}

/// v4's `QuilltapExportCounts` bag, insertion-ordered (see the module header).
#[derive(Debug, Default, Clone)]
pub struct Counts(Map<String, Value>);

impl Counts {
    /// v4 `bump(counts, key, delta = 1)` (`ndjson-writer.ts:105`).
    fn bump(&mut self, key: &str) {
        let next = self.0.get(key).and_then(Value::as_i64).unwrap_or(0) + 1;
        self.0.insert(key.to_string(), Value::from(next));
    }

    fn into_value(self) -> Value {
        Value::Object(self.0)
    }
}

// ============================================================================
// Manifest
// ============================================================================

/// v4 `buildManifest(options, counts)` (`ndjson-writer.ts:85`). `created_at` is
/// injected (v4 reads `new Date().toISOString()`); `app_version` is injected too
/// (v4 reads `packageJson.version`) so the differential can pin both.
///
/// ## `preserve_ids` rides as a parameter, not on [`ExportOptions`]
///
/// v4 hangs `preserveIds` off `ExportOptions` (`export/types.ts:868`,
/// `01e481f6`) — "restore the archive at its original IDs (refuse on
/// collision)". The writer only ever RECORDS it in the manifest; the honoring
/// is entirely the import side's (`quilltap_import::ImportOptions`), and the
/// preview path never emits a manifest at all. v5 threads it as an explicit
/// parameter of the two writer entrances instead of adding a field to
/// `ExportOptions`, because that struct is also constructed by the preview
/// dispatch arm in `api/engine.rs`, which belongs to a sibling lane this round
/// (P4.D63). The emitted bytes are identical either way; a later round that
/// owns `engine.rs` may lift it onto the struct. Nothing but the
/// character-archive service (round 2) ever passes `true`.
pub fn build_manifest(
    options: &ExportOptions,
    preserve_ids: bool,
    counts: Value,
    created_at: &str,
    app_version: &str,
) -> Value {
    let mut settings = Map::new();
    settings.insert(
        "includeMemories".into(),
        Value::Bool(options.include_memories),
    );
    settings.insert("scope".into(), Value::String(options.scope.clone()));
    settings.insert(
        "selectedIds".into(),
        Value::Array(
            options
                .selected_ids
                .iter()
                .map(|s| Value::String(s.clone()))
                .collect(),
        ),
    );
    // v4 `preserveIds: options.preserveIds ?? false` (`01e481f6`) — last key of
    // the settings bag, after `selectedIds`.
    settings.insert("preserveIds".into(), Value::Bool(preserve_ids));

    let mut m = Map::new();
    m.insert("format".into(), Value::String("quilltap-export".into()));
    m.insert("version".into(), Value::String("1.0".into()));
    m.insert(
        "exportType".into(),
        Value::String(options.entity_type.clone()),
    );
    m.insert("createdAt".into(), Value::String(created_at.to_string()));
    m.insert("appVersion".into(), Value::String(app_version.to_string()));
    m.insert("settings".into(), Value::Object(settings));
    m.insert("counts".into(), counts);
    Value::Object(m)
}

// ============================================================================
// resolveExportIds
// ============================================================================

/// v4 `resolveExportIds` (`ndjson-writer.ts:617`). `scope === 'selected'` returns
/// `selectedIds ?? []` verbatim (no existence check — the generators skip missing
/// rows). Otherwise the per-type `findAll().map(id)`, with two special cases:
/// roleplay-templates filters `!isBuiltIn && userId === userId` (:634-636), and
/// document-stores reads the INSTANCE-scoped `globalRepos.docMountPoints`.
pub fn resolve_export_ids(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    options: &ExportOptions,
) -> Result<Vec<String>, ExportError> {
    if options.scope == "selected" {
        return Ok(options.selected_ids.clone());
    }

    let ids = match options.entity_type.as_str() {
        "characters" => id_list(&characters_read::find_all(main, mount)?),
        "chats" => id_list(&chats_read::find_all(main)?),
        "roleplay-templates" => roleplay_templates::find_all_for_user(main, user_id)?
            .iter()
            .filter(|t| {
                !t.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false)
                    && t.get("userId").and_then(Value::as_str) == Some(user_id)
            })
            .filter_map(|t| t.get("id").and_then(Value::as_str).map(str::to_string))
            .collect(),
        "connection-profiles" => id_list(&connection_profiles::find_all(main)?),
        "image-profiles" => id_list(&image_profiles::find_all(main)?),
        "embedding-profiles" => embedding_profiles::find_all_full_json(main)?
            .iter()
            .filter_map(|p| p.get("id").and_then(Value::as_str).map(str::to_string))
            .collect(),
        "tags" => id_list(&tags::find_all(main)?),
        "projects" => id_list(&projects::ProjectsRepository::new(main, mount).find_all()?),
        "groups" => id_list(&groups::GroupsRepository::new(main, mount).find_all()?),
        "document-stores" => id_list(&crate::db::doc_mount_points::find_all_full_json(mount)?),
        // The five `7189a968` additions.
        "files" => crate::services::backup::marshal::query_all(
            main,
            "files",
            crate::services::backup::collect::FILES,
            "",
            &[],
        )?
        .iter()
        .filter(|f| !is_file_excluded_from_export(f))
        .filter_map(|f| f.get("id").and_then(Value::as_str).map(str::to_string))
        .collect(),
        "prompt-templates" => crate::services::backup::marshal::query_all(
            main,
            "prompt_templates",
            crate::services::backup::collect::PROMPT_TEMPLATES,
            "",
            &[],
        )?
        .iter()
        .filter(|t| {
            !t.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false)
                && t.get("userId").and_then(Value::as_str) == Some(user_id)
        })
        .filter_map(|t| t.get("id").and_then(Value::as_str).map(str::to_string))
        .collect(),
        "provider-models" => id_list(&crate::db::provider_models::find_all(main)?),
        "plugin-configs" => crate::services::backup::marshal::query_all(
            main,
            "plugin_configs",
            crate::services::backup::collect::PLUGIN_CONFIGS,
            "userId = ?1",
            &[&user_id],
        )?
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect(),
        // Keyed by setting key rather than a UUID — the table has no id column.
        "instance-settings" => crate::db::instance_settings::list_portable_instance_settings(main)?
            .into_iter()
            .map(|(k, _)| k)
            .collect(),
        other => return Err(ExportError::UnknownType(other.to_string())),
    };
    Ok(ids)
}

fn id_list(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

// ============================================================================
// The top-level streamer
// ============================================================================

/// v4 `streamExportRecords` (`ndjson-writer.ts:658`) materialized as a `Vec` in
/// yield order. See the module header for the envelope/footer discipline.
#[allow(clippy::too_many_arguments)]
pub fn stream_export_records(
    main: &Connection,
    mount: &Connection,
    storage: Option<&dyn crate::services::file_storage::StorageBackend>,
    user_id: &str,
    options: &ExportOptions,
    // See `build_manifest` — the manifest's `settings.preserveIds`, threaded as
    // a parameter rather than a field of `ExportOptions`.
    preserve_ids: bool,
    created_at: &str,
    app_version: &str,
) -> Result<Vec<Value>, ExportError> {
    // v4 resolves ids BEFORE the envelope, so an unknown type throws with nothing
    // emitted (the route then 500s — no partial file).
    let ids = resolve_export_ids(main, mount, user_id, options)?;

    let mut out: Vec<Value> = Vec::new();
    let mut envelope = Map::new();
    envelope.insert("kind".into(), Value::String("__envelope__".into()));
    envelope.insert("format".into(), Value::String("qtap-ndjson".into()));
    envelope.insert("version".into(), Value::from(1));
    envelope.insert(
        "manifest".into(),
        // The empty-counts quirk (v4 :677 passes a literal `{}`).
        build_manifest(
            options,
            preserve_ids,
            Value::Object(Map::new()),
            created_at,
            app_version,
        ),
    );
    out.push(Value::Object(envelope));

    let mut counts = Counts::default();
    let include_memories = options.include_memories;

    match options.entity_type.as_str() {
        "characters" => {
            records::stream_characters(main, mount, &ids, include_memories, &mut counts, &mut out)?
        }
        "chats" => {
            records::stream_chats(main, mount, &ids, include_memories, &mut counts, &mut out)?
        }
        "roleplay-templates" => {
            records::stream_roleplay_templates(main, user_id, &ids, &mut counts, &mut out)?
        }
        "connection-profiles" => {
            records::stream_connection_profiles(main, &ids, &mut counts, &mut out)?
        }
        "image-profiles" => records::stream_image_profiles(main, &ids, &mut counts, &mut out)?,
        "embedding-profiles" => {
            records::stream_embedding_profiles(main, &ids, &mut counts, &mut out)?
        }
        "tags" => records::stream_tags(main, &ids, &mut counts, &mut out)?,
        "projects" => records::stream_projects(main, mount, &ids, &mut counts, &mut out)?,
        "groups" => records::stream_groups(main, mount, &ids, &mut counts, &mut out)?,
        "document-stores" => records::stream_document_stores(mount, &ids, &mut counts, &mut out)?,
        "files" => {
            records::stream_files(main, mount, storage, user_id, &ids, &mut counts, &mut out)?
        }
        "prompt-templates" => {
            records::stream_prompt_templates(main, user_id, &ids, &mut counts, &mut out)?
        }
        "provider-models" => records::stream_provider_models(main, &ids, &mut counts, &mut out)?,
        "plugin-configs" => {
            records::stream_plugin_configs(main, user_id, &ids, &mut counts, &mut out)?
        }
        "instance-settings" => {
            records::stream_instance_settings(main, &ids, &mut counts, &mut out)?
        }
        other => return Err(ExportError::UnknownType(other.to_string())),
    }

    let mut footer = Map::new();
    footer.insert("kind".into(), Value::String("__footer__".into()));
    footer.insert("counts".into(), counts.into_value());
    out.push(Value::Object(footer));

    Ok(out)
}

/// v4 `createNdjsonStream`'s per-record encode (`ndjson-writer.ts:753`):
/// `JSON.stringify(record) + '\n'`, concatenated.
pub fn records_to_ndjson(records: &[Value]) -> String {
    let mut s = String::new();
    for r in records {
        s.push_str(&r.to_string());
        s.push('\n');
    }
    s
}

/// v4 `handleExport`'s filename derivation (`route.ts:418-420`):
/// `quilltap-<type with `_`→`-`, or 'data' when falsy>-<YYYY-MM-DD>.qtap`.
pub fn export_filename(entity_type: &str, now_iso: &str) -> String {
    let sanitized = if entity_type.is_empty() {
        "data".to_string()
    } else {
        entity_type.replace('_', "-")
    };
    let date = now_iso.split('T').next().unwrap_or(now_iso);
    format!("quilltap-{sanitized}-{date}.qtap")
}

// ============================================================================
// Shared inline resolvers (v4 `ndjson-writer.ts` HELPERS section)
// ============================================================================

/// v4 `resolveTagNames` (:44) — per-id `tags.findById`, pushing `.name` when the
/// tag resolves. A missing tag is skipped (v4 swallows the throw).
pub(crate) fn resolve_tag_names(main: &Connection, tag_ids: &[Value]) -> Vec<String> {
    let mut names = Vec::new();
    for id in tag_ids {
        let Some(id) = id.as_str() else { continue };
        if let Ok(Some(tag)) = tags::find_full_by_id(main, id) {
            if let Some(n) = tag.get("name").and_then(Value::as_str) {
                names.push(n.to_string());
            }
        }
    }
    names
}

/// v4 `resolveApiKeyLabel` (:61) — `connections.findApiKeyById(apiKeyId)?.label`.
/// `None` for a falsy id, a missing key, or a throw (all swallowed by v4).
pub(crate) fn resolve_api_key_label(main: &Connection, api_key_id: Option<&str>) -> Option<String> {
    let id = api_key_id.filter(|s| !s.is_empty())?;
    crate::db::api_keys::find_label_by_id(main, id)
        .ok()
        .flatten()
}

/// v4 `sanitizeProfile` (:74) — `{...rest}` with `apiKeyId` REMOVED, then
/// `_apiKeyLabel` appended when a label resolved (the `&&` spread drops it when
/// the label is falsy). Key order: the profile's own order minus `apiKeyId`, with
/// `_apiKeyLabel` last.
pub(crate) fn sanitize_profile(
    kind: &str,
    profile: &Value,
    api_key_label: Option<String>,
) -> Value {
    let ordered = key_order::reorder(kind, profile.clone());
    let mut out = Map::new();
    if let Some(obj) = ordered.as_object() {
        for (k, v) in obj {
            if k == "apiKeyId" {
                continue;
            }
            out.insert(k.clone(), v.clone());
        }
    }
    if let Some(label) = api_key_label.filter(|l| !l.is_empty()) {
        out.insert("_apiKeyLabel".into(), Value::String(label));
    }
    Value::Object(out)
}

/// `{ ...entity, ...(names.length > 0 && { _tagNames: names }) }` — the spread
/// idiom used for characters / chats / roleplay templates. `_tagNames` lands last.
/// `kind` selects the schema key-order template (see [`key_order`]).
pub(crate) fn with_tag_names(kind: &str, entity: &Value, names: Vec<String>) -> Map<String, Value> {
    let mut out = key_order::reorder_map(kind, entity.as_object().cloned().unwrap_or_default());
    if !names.is_empty() {
        out.insert(
            "_tagNames".into(),
            Value::Array(names.into_iter().map(Value::String).collect()),
        );
    }
    out
}

/// A `{kind, data}` record (the two-key shape most generators emit).
pub(crate) fn kind_data(kind: &str, data: Value) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), Value::String(kind.to_string()));
    m.insert("data".into(), data);
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_matches_v4_derivation() {
        assert_eq!(
            export_filename("characters", "2026-07-24T12:34:56.789Z"),
            "quilltap-characters-2026-07-24.qtap"
        );
        // `_` → `-`; an empty type falls back to `data`.
        assert_eq!(
            export_filename("doc_stores", "2026-01-02T00:00:00.000Z"),
            "quilltap-doc-stores-2026-01-02.qtap"
        );
        assert_eq!(
            export_filename("", "2026-01-02T00:00:00.000Z"),
            "quilltap-data-2026-01-02.qtap"
        );
    }

    #[test]
    fn counts_are_insertion_ordered() {
        let mut c = Counts::default();
        c.bump("chats");
        c.bump("messages");
        c.bump("chats");
        let v = c.into_value();
        assert_eq!(v.to_string(), r#"{"chats":2,"messages":1}"#);
    }

    #[test]
    fn manifest_key_order_is_v4s() {
        let opts = ExportOptions {
            entity_type: "tags".into(),
            scope: "all".into(),
            selected_ids: vec![],
            include_memories: false,
        };
        let m = build_manifest(&opts, false, Value::Object(Map::new()), "T", "1.2.3");
        assert_eq!(
            m.to_string(),
            r#"{"format":"quilltap-export","version":"1.0","exportType":"tags","createdAt":"T","appVersion":"1.2.3","settings":{"includeMemories":false,"scope":"all","selectedIds":[],"preserveIds":false},"counts":{}}"#
        );
        // …and the archive service's arm (round 2's only caller).
        let m = build_manifest(&opts, true, Value::Object(Map::new()), "T", "1.2.3");
        assert_eq!(m.pointer("/settings/preserveIds"), Some(&Value::Bool(true)));
    }

    #[test]
    fn blob_chunk_size_matches_v4() {
        // v4 `BLOB_CHUNK_BYTES = 3 * 1024 * 1024` (`ndjson-writer.ts:38`).
        assert_eq!(BLOB_CHUNK_BYTES, 3_145_728);
        // …and it must stay a multiple of 3. Each chunk is base64-encoded on its
        // own and the reader rejoins the ENCODED strings, so only the final chunk
        // may carry padding — otherwise a multi-chunk blob decodes to garbage.
        // Load-bearing since the reader stopped reproducing v4's sparse-array
        // `every` and multi-chunk blobs actually reassemble (see `ndjson.rs`).
        // v4 states the same constraint in its own source as of `c1507f47`
        // (`ndjson-writer.ts` — a comment-only change; the bytes do not move).
        assert_eq!(BLOB_CHUNK_BYTES % 3, 0);
    }

    #[test]
    fn sanitize_profile_drops_api_key_id_and_appends_label() {
        let p: Value =
            serde_json::from_str(r#"{"id":"1","name":"P","apiKeyId":"k","baseUrl":"u","tags":[]}"#)
                .unwrap();
        assert_eq!(
            sanitize_profile("connection_profile", &p, Some("mock-key".into())).to_string(),
            r#"{"id":"1","name":"P","baseUrl":"u","tags":[],"_apiKeyLabel":"mock-key"}"#
        );
        // No label → no key at all (v4's `&&` spread).
        assert_eq!(
            sanitize_profile("connection_profile", &p, None).to_string(),
            r#"{"id":"1","name":"P","baseUrl":"u","tags":[]}"#
        );
    }
}
