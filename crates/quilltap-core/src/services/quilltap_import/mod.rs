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

/// v4 `IdMappingState` (`types.ts:107`) — the ten insertion-ordered maps.
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

    // The BUILTIN TF-IDF profile is corpus-derived rather than model-derived;
    // per-row generates against it are not the right repair, so those rows are
    // left to the boot reconcile just as when no profile exists at all.
    let profile_id = match profile {
        Some((id, provider)) if provider != "BUILTIN" => id,
        other => {
            let reason = if other.is_some() {
                "the configured embedding profile is the built-in TF-IDF one"
            } else {
                "no default embedding profile is configured"
            };
            warnings.push(format!(
                "{} memories were imported without embeddings because {reason}; \
                 they will be indexed once an embedding profile is configured.",
                memory_refs.len()
            ));
            return;
        }
    };

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
        let c = entities::import_tags(main, user_id, items, options, &mut id_maps.tags)?;
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
        let c = characters::import_characters(
            main,
            mount,
            user_id,
            items,
            options,
            &mut id_maps.characters,
            warnings,
        )?;
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
    if let Some(mount_points) = non_empty_array(data, "mountPoints") {
        let mount_points = mount_points.clone();
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
            let c = memories::import_memories(main, items, id_maps, warnings)?;
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
}
