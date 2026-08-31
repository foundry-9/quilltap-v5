//! v4 `import-profiles.ts` — import connection / image / embedding profiles.
//! All three share the same conflict-strategy shape and never restore API keys
//! (`apiKeyId` is forced to null on insert).
//!
//! Per v4, a per-item failure (bad shape, DB error) is logged, named in
//! `warnings` as `Failed to import <kind> "<name>": <error>`, and the item is
//! dropped — only the loop preambles (the `findAll` that seeds the taken-name
//! set) throw into `executeImport`'s outer catch. The three arms only LOGGED
//! until v4 `275cd7bc` (bug 79): the import had just stopped swallowing
//! destination read errors, and strictness alone would have traded a silently
//! wrong branch for a silent skip.
//!
//! The `duplicate` arm reproduces v4's phantom-id quirk verbatim: a
//! `randomUUID()` goes INTO the id map, and the row is created under a
//! DIFFERENT freshly-minted id, so the map points at a row that does not exist
//! (`import-profiles.ts:65-66`). Downstream remaps (memory FKs, reconcile)
//! observe the phantom — a faithful port must not "fix" it.

use std::collections::HashSet;

use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ConflictStrategy, IdMap, ImportOptions};
use crate::db::{connection_profiles, embedding_profiles, image_profiles, DbError};
use crate::services::connection_profile_legacy_fields::seed_legacy_connection_profile_fields;
use crate::services::profile_names::{make_unique_profile_name, normalize_profile_name};

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
}

/// The connection-profile payload with v4's Zod defaults materialized
/// (`ConnectionProfileSchema`, `profile.types.ts:42`). Unknown keys (the export's
/// `_tagNames`, `_apiKeyLabel`) are dropped, mirroring Zod's non-strict parse.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportedConnectionProfile {
    name: String,
    provider: String,
    #[serde(default = "d_api")]
    transport: String,
    #[serde(default = "d_true")]
    courier_delta_mode: bool,
    #[serde(default)]
    base_url: Option<String>,
    model_name: String,
    #[serde(default = "d_obj")]
    parameters: Value,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    is_cheap: bool,
    #[serde(default)]
    allow_web_search: bool,
    #[serde(default)]
    use_native_web_search: bool,
    #[serde(default = "d_true")]
    allow_tool_use: bool,
    #[serde(default = "d_auto")]
    pseudo_tool_mode: String,
    #[serde(default)]
    model_class: Option<String>,
    #[serde(default)]
    max_context: Option<f64>,
    /// `["boolean","null"]` in the export schema (v4 `23af7146`). A real v4
    /// export never emits an explicit `null` — a never-chosen profile's net-read
    /// OMITS the key (NULL boolean → `undefined` → dropped by `JSON.stringify`)
    /// — so this `Option` cannot tell absent from null, and does not have to:
    /// like [`Self::supports_image_upload`] it is kept for its VALIDATION alone.
    /// The write reads the RAW record through
    /// [`crate::services::connection_profile_legacy_fields`].
    ///
    /// **The divergence recorded here at P4.D79 is RETIRED** (v4 `e000d6bfc`,
    /// P4.D126): a hand-crafted bundle carrying a literal `null` used to get an
    /// explicit NULL from v4's `insertOne` and an OMITTED column from v5 —
    /// observable on a migrated instance whose DDL default is 1. Both sides now
    /// write the explicit NULL, because both read key presence.
    #[serde(default)]
    #[allow(dead_code)]
    multi_character_prefill: Option<bool>,
    /// The 4.10 understudy (v4 `65f5021c8`). Like the two fields above, kept
    /// for its VALIDATION alone — the write reads the RAW record through
    /// [`crate::services::connection_profile_legacy_fields`], because the
    /// seeding decision is key presence, which this `Option` cannot express.
    ///
    /// ⚠ v4 declares it `UUIDSchema.nullable().optional()`, so a bundle naming
    /// a non-UUID understudy is REFUSED by v4's parse and accepted here. That
    /// is the module's standing Zod-format gap, not a new one: `apiKeyId` and
    /// every other `UUIDSchema` field on this DTO are plain strings too.
    #[serde(default)]
    #[allow(dead_code)]
    fallback_profile_id: Option<String>,
    /// The tier-pick opt-in (v4 `65f5021c8`), `z.boolean().default(false)` —
    /// so a non-boolean must be refused here exactly as v4's parse refuses it.
    /// Value unused for the same reason as its siblings.
    #[serde(default)]
    #[allow(dead_code)]
    allow_tier_fallback: bool,
    #[serde(default)]
    max_tokens: Option<f64>,
    #[serde(default)]
    is_dangerous_compatible: bool,
    /// Kept for its VALIDATION, not its value: since v4 `e000d6bfc` the seeding
    /// decision reads the raw record (key presence, which this `Option` cannot
    /// express), but a non-boolean `supportsImageUpload` must still be refused
    /// here exactly as v4's `z.boolean().default(false)` refuses it.
    #[serde(default)]
    #[allow(dead_code)]
    supports_image_upload: Option<bool>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    sort_index: f64,
    #[serde(default)]
    total_tokens: f64,
    #[serde(default)]
    total_prompt_tokens: f64,
    #[serde(default)]
    total_completion_tokens: f64,
    #[serde(default)]
    message_count: f64,
}

fn d_api() -> String {
    "api".to_string()
}
fn d_auto() -> String {
    "auto".to_string()
}
fn d_true() -> bool {
    true
}
fn d_obj() -> Value {
    json!({})
}

/// v4 `importConnectionProfiles` (`import-profiles.ts:24`).
pub(super) fn import_connection_profiles(
    main: &Connection,
    user_id: &str,
    profiles: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;

    // Connection-profile names are unique per user (a DB expression index) — an
    // insert must never reuse a name already present. Seed the taken-name set
    // from existing profiles and add each freshly-inserted name as we go.
    let existing_profiles = connection_profiles::find_all(main)?;
    let mut taken_names: HashSet<String> = existing_profiles
        .iter()
        .filter_map(|p| p.get("name").and_then(Value::as_str))
        .map(normalize_profile_name)
        .collect();

    let repo = connection_profiles::ConnectionProfilesRepository::new(main);

    for raw_profile in profiles {
        let source_id = super::id_of(raw_profile);
        let name = super::warning_display_name(raw_profile);
        let out: Result<(), DbError> = (|| {
            let existing = connection_profiles::find_by_id(main, &source_id)?;

            if let Some(existing) = &existing {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id)?;
                        // The overwritten row's name is no longer taken by it.
                        if let Some(name) = existing.get("name").and_then(Value::as_str) {
                            taken_names.remove(&normalize_profile_name(name));
                        }
                    }
                    ConflictStrategy::Duplicate => {
                        // v4's phantom-map quirk: the mapped id is NOT the created
                        // row's id (see the module header).
                        let phantom = uuid::Uuid::new_v4().to_string();
                        id_map.set(source_id.clone(), phantom);
                        let p = match parse_connection_profile(raw_profile) {
                            Ok(p) => p,
                            Err(e) => {
                                warnings.push(format!(
                                    "Failed to import connection profile \"{name}\": {e}"
                                ));
                                return Ok(());
                            }
                        };
                        let unique = make_unique_profile_name(
                            &format!("{} (imported)", p.name),
                            &taken_names,
                        );
                        taken_names.insert(normalize_profile_name(&unique));
                        create_connection_profile(
                            options,
                            &source_id,
                            &repo,
                            user_id,
                            p,
                            raw_profile,
                            unique,
                        )?;
                        imported += 1;
                        return Ok(());
                    }
                }
            }

            let p = match parse_connection_profile(raw_profile) {
                Ok(p) => p,
                Err(e) => {
                    warnings.push(format!(
                        "Failed to import connection profile \"{name}\": {e}"
                    ));
                    return Ok(());
                }
            };
            let unique = make_unique_profile_name(&p.name, &taken_names);
            taken_names.insert(normalize_profile_name(&unique));
            let new_id = create_connection_profile(
                options,
                &source_id,
                &repo,
                user_id,
                p,
                raw_profile,
                unique,
            )?;
            id_map.set(source_id.clone(), new_id);
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to import connection profile \"{name}\": {e}"
            ));
            tracing::warn!(profile_id = %source_id, error = %e, "Failed to import connection profile");
        }
    }

    Ok(Counts { imported, skipped })
}

/// Deserialize with the pre-validation shape checks. An `Err` mirrors v4's Zod
/// throw landing in the per-item catch — which since `275cd7bc` (bug 79) names
/// the item in `warnings` as well as logging it, so the error text comes back
/// out rather than being swallowed here.
fn parse_connection_profile(raw: &Value) -> Result<ImportedConnectionProfile, serde_json::Error> {
    let parsed = serde_json::from_value::<ImportedConnectionProfile>(raw.clone());
    if let Err(e) = &parsed {
        tracing::warn!(error = %e, "Failed to import connection profile");
    }
    parsed
}

fn create_connection_profile(
    options: &ImportOptions,
    source_id: &str,
    repo: &connection_profiles::ConnectionProfilesRepository,
    user_id: &str,
    p: ImportedConnectionProfile,
    raw: &Value,
    unique_name: String,
) -> Result<String, DbError> {
    // Older exports predate some of the columns; seed them so the bundle's age,
    // not the table DEFAULT, decides what the profile comes back as. Shared with
    // backup restore (v4 `e000d6bfc`, bug 103) so the two paths cannot drift:
    // a `.qtap` bundle and a backup ZIP carrying the same profile land the same
    // row. Reads the RAW record, because the decision is key PRESENCE and
    // `ImportedConnectionProfile` folds an absent key and an explicit `null`
    // into the same `None`.
    //
    // This also retires v5's own copy of the provider set, which tested
    // membership on the STORED casing where v4 upcases first — so a legacy
    // bundle whose `provider` read `openai` lost its vision flag.
    let seeded = seed_legacy_connection_profile_fields(raw);
    if seeded.seeded_anything() {
        tracing::debug!(
            profile_id = %source_id,
            provider = %p.provider,
            seeded_multi_character_prefill = seeded.seeded_multi_character_prefill,
            seeded_supports_image_upload = seeded.seeded_supports_image_upload,
            "Seeded connection-profile columns the bundle predates"
        );
    }
    let create = connection_profiles::CpCreate {
        user_id: user_id.to_string(),
        name: unique_name,
        provider: p.provider,
        transport: p.transport,
        courier_delta_mode: p.courier_delta_mode,
        api_key_id: None, // Don't restore API keys
        base_url: p.base_url,
        model_name: p.model_name,
        parameters: p.parameters,
        is_default: p.is_default,
        is_cheap: p.is_cheap,
        allow_web_search: p.allow_web_search,
        use_native_web_search: p.use_native_web_search,
        allow_tool_use: p.allow_tool_use,
        pseudo_tool_mode: p.pseudo_tool_mode,
        // v4 `23af7146`'s importer carries `multiCharacterPrefill` through its
        // `...profileData` spread, so a 4.9 bundle's stored choice round-trips.
        // Since `e000d6bfc` the seeded record ALWAYS carries the key, so a
        // pre-4.9 bundle lands an explicit NULL ("never chosen") rather than the
        // table default — which on a migrated instance is `DEFAULT 1`, i.e. the
        // `[Name]` prefill switched on for a profile nobody chose it for.
        multi_character_prefill: Some(seeded.multi_character_prefill),
        model_class: p.model_class,
        // v4 `65f5021c8`: the understudy rides the `...profileData` spread the
        // same way. The id it names is a BUNDLE id at this point; the reconcile
        // pass remaps it once every profile has landed (an understudy may
        // appear later in the bundle).
        fallback_profile_id: seeded.fallback_profile_id.clone(),
        allow_tier_fallback: seeded.allow_tier_fallback,
        max_context: p.max_context,
        max_tokens: p.max_tokens,
        is_dangerous_compatible: p.is_dangerous_compatible,
        supports_image_upload: seeded.supports_image_upload,
        tags: p.tags,
        sort_index: p.sort_index,
        total_tokens: p.total_tokens,
        total_prompt_tokens: p.total_prompt_tokens,
        total_completion_tokens: p.total_completion_tokens,
        message_count: p.message_count,
    };
    // v4 `01e481f6` forks BOTH profile arms — the name-conflict `duplicate`
    // branch as well as the plain one. The duplicate branch's idMap entry stays
    // the phantom v4 minted, so under `preserveIds` the row is created at the
    // source id while the map points elsewhere; carried faithfully (unreachable
    // in practice — the rehydrate path never runs `duplicate`).
    let (new_id, now) = super::mint_or_preserve(options, source_id);
    let opts = connection_profiles::CreateOptions {
        id: new_id,
        created_at: now.clone(),
        updated_at: now,
    };
    repo.create(&create, &opts)?;
    Ok(opts.id)
}

/// The image-profile payload with v4's Zod defaults (`ImageProfileSchema`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportedImageProfile {
    name: String,
    provider: String,
    #[serde(default)]
    base_url: Option<String>,
    model_name: String,
    #[serde(default = "d_obj")]
    parameters: Value,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    is_dangerous_compatible: bool,
    #[serde(default)]
    tags: Vec<String>,
}

/// v4 `importImageProfiles` (`import-profiles.ts:113`).
pub(super) fn import_image_profiles(
    main: &Connection,
    user_id: &str,
    profiles: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = image_profiles::ImageProfilesRepository::new(main);

    for raw in profiles {
        let source_id = super::id_of(raw);
        let name = super::warning_display_name(raw);
        let out: Result<(), DbError> = (|| {
            let existing = image_profiles::find_by_id(main, &source_id)?;
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id)?;
                    }
                    ConflictStrategy::Duplicate => {
                        let phantom = uuid::Uuid::new_v4().to_string();
                        id_map.set(source_id.clone(), phantom);
                        let Ok(p) = serde_json::from_value::<ImportedImageProfile>(raw.clone())
                        else {
                            return Ok(());
                        };
                        let name = format!("{} (imported)", p.name);
                        create_image_profile(options, &source_id, &repo, user_id, p, name)?;
                        imported += 1;
                        return Ok(());
                    }
                }
            }
            let p = match serde_json::from_value::<ImportedImageProfile>(raw.clone()) {
                Ok(p) => p,
                Err(e) => {
                    warnings.push(format!("Failed to import image profile \"{name}\": {e}"));
                    return Ok(());
                }
            };
            let name = p.name.clone();
            let new_id = create_image_profile(options, &source_id, &repo, user_id, p, name)?;
            id_map.set(source_id.clone(), new_id);
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!("Failed to import image profile \"{name}\": {e}"));
            tracing::warn!(profile_id = %source_id, error = %e, "Failed to import image profile");
        }
    }
    Ok(Counts { imported, skipped })
}

fn create_image_profile(
    options: &ImportOptions,
    source_id: &str,
    repo: &image_profiles::ImageProfilesRepository,
    user_id: &str,
    p: ImportedImageProfile,
    name: String,
) -> Result<String, DbError> {
    let create = image_profiles::IpCreate {
        user_id: user_id.to_string(),
        name,
        provider: p.provider,
        api_key_id: None, // Don't restore API keys
        base_url: p.base_url,
        model_name: p.model_name,
        parameters: p.parameters,
        is_default: p.is_default,
        is_dangerous_compatible: p.is_dangerous_compatible,
        tags: p.tags,
    };
    let (new_id, now) = super::mint_or_preserve(options, source_id);
    let opts = image_profiles::CreateOptions {
        id: new_id,
        created_at: now.clone(),
        updated_at: now,
    };
    repo.create(&create, &opts)?;
    Ok(opts.id)
}

/// The embedding-profile payload with v4's Zod defaults
/// (`EmbeddingProfileSchema` — `normalizeL2` default `true`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportedEmbeddingProfile {
    name: String,
    provider: String,
    #[serde(default)]
    base_url: Option<String>,
    model_name: String,
    #[serde(default)]
    dimensions: Option<f64>,
    #[serde(default)]
    truncate_to_dimensions: Option<f64>,
    #[serde(default = "d_true")]
    normalize_l2: bool,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    tags: Vec<String>,
}

/// v4 `importEmbeddingProfiles` (`import-profiles.ts:170`).
pub(super) fn import_embedding_profiles(
    main: &Connection,
    user_id: &str,
    profiles: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = embedding_profiles::EmbeddingProfilesRepository::new(main);

    for raw in profiles {
        let source_id = super::id_of(raw);
        let name = super::warning_display_name(raw);
        let out: Result<(), DbError> = (|| {
            let existing = embedding_profiles::find_by_id(main, &source_id)?;
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id)?;
                    }
                    ConflictStrategy::Duplicate => {
                        let phantom = uuid::Uuid::new_v4().to_string();
                        id_map.set(source_id.clone(), phantom);
                        let Ok(p) = serde_json::from_value::<ImportedEmbeddingProfile>(raw.clone())
                        else {
                            return Ok(());
                        };
                        let name = format!("{} (imported)", p.name);
                        create_embedding_profile(options, &source_id, &repo, user_id, p, name)?;
                        imported += 1;
                        return Ok(());
                    }
                }
            }
            let p = match serde_json::from_value::<ImportedEmbeddingProfile>(raw.clone()) {
                Ok(p) => p,
                Err(e) => {
                    warnings.push(format!(
                        "Failed to import embedding profile \"{name}\": {e}"
                    ));
                    return Ok(());
                }
            };
            let name = p.name.clone();
            let new_id = create_embedding_profile(options, &source_id, &repo, user_id, p, name)?;
            id_map.set(source_id.clone(), new_id);
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to import embedding profile \"{name}\": {e}"
            ));
            tracing::warn!(profile_id = %source_id, error = %e, "Failed to import embedding profile");
        }
    }
    Ok(Counts { imported, skipped })
}

fn create_embedding_profile(
    options: &ImportOptions,
    source_id: &str,
    repo: &embedding_profiles::EmbeddingProfilesRepository,
    user_id: &str,
    p: ImportedEmbeddingProfile,
    name: String,
) -> Result<String, DbError> {
    let create = embedding_profiles::EpCreate {
        user_id: user_id.to_string(),
        name,
        provider: p.provider,
        api_key_id: None, // Don't restore API keys
        base_url: p.base_url,
        model_name: p.model_name,
        dimensions: p.dimensions,
        truncate_to_dimensions: p.truncate_to_dimensions,
        normalize_l2: p.normalize_l2,
        is_default: p.is_default,
        tags: p.tags,
    };
    let (new_id, now) = super::mint_or_preserve(options, source_id);
    let opts = embedding_profiles::CreateOptions {
        id: new_id,
        created_at: now.clone(),
        updated_at: now,
    };
    repo.create(&create, &opts)?;
    Ok(opts.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The migrated-shape table, columns exactly as the CpCreate INSERT names
    /// them (SQLite's dynamic typing makes the affinities immaterial here).
    fn migrated_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE connection_profiles (\
               id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT, \
               transport TEXT, courierDeltaMode INTEGER, apiKeyId TEXT, \
               baseUrl TEXT, modelName TEXT, parameters TEXT, isDefault INTEGER, \
               isCheap INTEGER, allowWebSearch INTEGER, useNativeWebSearch INTEGER, \
               allowToolUse INTEGER, pseudoToolMode TEXT, \
               \"multiCharacterPrefill\" INTEGER DEFAULT 1, \
               modelClass TEXT, \
               \"fallbackProfileId\" TEXT, \"allowTierFallback\" INTEGER DEFAULT 0, \
               maxContext REAL, maxTokens REAL, \
               isDangerousCompatible INTEGER, supportsImageUpload INTEGER, \
               tags TEXT, sortIndex REAL, totalTokens REAL, totalPromptTokens REAL, \
               totalCompletionTokens REAL, messageCount REAL, createdAt TEXT, \
               updatedAt TEXT)",
        )
        .unwrap();
    }

    fn record(id: &str, name: &str, prefill: Option<Value>) -> Value {
        let mut rec = json!({
            "id": id,
            "name": name,
            "provider": "OPENROUTER",
            "modelName": "m",
        });
        if let Some(v) = prefill {
            rec["multiCharacterPrefill"] = v;
        }
        rec
    }

    fn stored_prefill(conn: &Connection, name: &str) -> Option<i64> {
        conn.query_row(
            "SELECT \"multiCharacterPrefill\" FROM connection_profiles WHERE name = ?1",
            [name],
            |r| r.get::<_, Option<i64>>(0),
        )
        .unwrap()
    }

    /// The unification wire's pin (P4.D79's ordered edit, applied at unify),
    /// **plus bug 103's arm** (v4 `e000d6bfc`, P4.D126): a 4.9 bundle's stored
    /// `multiCharacterPrefill` round-trips through the import, and an absent key
    /// now lands an explicit NULL — the "never chosen" tri-state — instead of
    /// letting the table default decide.
    ///
    /// The table here is the MIGRATED shape, `INTEGER DEFAULT 1`, which is the
    /// only shape where the two answers differ (generateDDL declares the column
    /// with no default, so there both land NULL). **RED-FIRST:** before the port
    /// the `Silent` row read `Some(1)` — the `[Name]` prefill switched on for a
    /// profile whose owner never chose it.
    #[test]
    fn import_carries_the_stored_prefill_choice() {
        let conn = Connection::open_in_memory().unwrap();
        migrated_table(&conn);
        let profiles = vec![
            record(
                "a0000000-0000-4000-8000-000000000001",
                "Off",
                Some(json!(false)),
            ),
            record(
                "a0000000-0000-4000-8000-000000000002",
                "On",
                Some(json!(true)),
            ),
            record("a0000000-0000-4000-8000-000000000003", "Silent", None),
        ];
        let mut id_map = IdMap::default();
        let mut warnings: Vec<String> = Vec::new();
        let counts = import_connection_profiles(
            &conn,
            "user",
            &profiles,
            &ImportOptions::seed_defaults(),
            &mut id_map,
            &mut warnings,
        )
        .unwrap();
        assert_eq!(counts.imported, 3);
        assert!(
            warnings.is_empty(),
            "clean imports name nothing: {warnings:?}"
        );
        // The stored choice round-trips (the pre-unify code imported ALL THREE
        // as "never chosen" — this is the arm that reddens on a regression)...
        assert_eq!(stored_prefill(&conn, "Off"), Some(0));
        assert_eq!(stored_prefill(&conn, "On"), Some(1));
        // ...and an absent key is now an explicit NULL, not the migrated DDL
        // default. This assertion read `Some(1)` before bug 103 was ported.
        assert_eq!(stored_prefill(&conn, "Silent"), None);
    }

    // ── P4.D126 unit 3: bug 103's `supportsImageUpload` half ────────────────

    fn stored_image_flag(conn: &Connection, name: &str) -> Option<i64> {
        conn.query_row(
            "SELECT supportsImageUpload FROM connection_profiles WHERE name = ?1",
            [name],
            |r| r.get::<_, Option<i64>>(0),
        )
        .unwrap()
    }

    fn profile_record(id: &str, name: &str, provider: &str, extra: Value) -> Value {
        let mut rec = json!({
            "id": id,
            "name": name,
            "provider": provider,
            "modelName": "m",
        });
        if let Some(obj) = extra.as_object() {
            for (k, v) in obj {
                rec[k] = v.clone();
            }
        }
        rec
    }

    /// The import site's WIRING pin: the shared seeding helper (proven against
    /// v4's real module by `connection_profile_legacy_fields_equivalence`) is
    /// actually the thing this path answers with.
    ///
    /// The last row is the one that used to be wrong on its own: v5's private
    /// provider set tested membership on the STORED casing where v4 upcases
    /// first, so a legacy bundle whose `provider` read `openai` came back with
    /// vision stripped.
    #[test]
    fn import_seeds_the_columns_a_bundle_predates() {
        let conn = Connection::open_in_memory().unwrap();
        migrated_table(&conn);
        let profiles = vec![
            // Carried: never touched, a stored `false` included.
            profile_record(
                "a1000000-0000-4000-8000-000000000001",
                "Carried False",
                "GOOGLE",
                json!({ "supportsImageUpload": false }),
            ),
            // Absent + historically capable → true.
            profile_record(
                "a1000000-0000-4000-8000-000000000002",
                "Legacy Anthropic",
                "ANTHROPIC",
                json!({}),
            ),
            // Absent + never capable → false.
            profile_record(
                "a1000000-0000-4000-8000-000000000003",
                "Legacy Ollama",
                "OLLAMA",
                json!({}),
            ),
            // Absent + capable, stored lowercase → true (the case-insensitive
            // match v5 did not have).
            profile_record(
                "a1000000-0000-4000-8000-000000000004",
                "Legacy Lowercase",
                "openai",
                json!({}),
            ),
        ];
        let mut id_map = IdMap::default();
        let mut warnings: Vec<String> = Vec::new();
        let counts = import_connection_profiles(
            &conn,
            "user",
            &profiles,
            &ImportOptions::seed_defaults(),
            &mut id_map,
            &mut warnings,
        )
        .unwrap();
        assert_eq!(counts.imported, 4);
        assert!(
            warnings.is_empty(),
            "clean imports name nothing: {warnings:?}"
        );

        assert_eq!(stored_image_flag(&conn, "Carried False"), Some(0));
        assert_eq!(stored_image_flag(&conn, "Legacy Anthropic"), Some(1));
        assert_eq!(stored_image_flag(&conn, "Legacy Ollama"), Some(0));
        assert_eq!(stored_image_flag(&conn, "Legacy Lowercase"), Some(1));

        // Every one of them predates the prefill column, so every one is NULL.
        for name in [
            "Carried False",
            "Legacy Anthropic",
            "Legacy Ollama",
            "Legacy Lowercase",
        ] {
            assert_eq!(stored_prefill(&conn, name), None, "{name}");
        }
    }

    /// ## Convergence record — v4 bug 105, fixed upstream at `679e450e3`
    ///
    /// v4 `e000d6bfc` called `seedLegacyConnectionProfileFields(rawProfile)`
    /// at the TOP of `importConnectionProfiles`' loop body — outside the
    /// per-item `try` — and the helper's `(seeded.provider ?? '')
    /// .toUpperCase()` threw on a non-string `provider`, so one malformed
    /// profile aborted a WHOLE v4 import with
    ///
    /// ```text
    /// Import failed: (seeded.provider ?? "").toUpperCase is not a function
    /// ```
    ///
    /// **v5 was never affected**, under the standing 2026-08-03 ruling
    /// (backup / restore / import / export: v5 FIXES v4's bugs rather than
    /// reproducing them). Two things keep v5 lenient, and both are deliberate:
    /// [`seed_legacy_connection_profile_fields`] reads the provider as
    /// `as_str().unwrap_or("")` and cannot throw, and v5 parses the record
    /// BEFORE it seeds, so a non-string provider is refused by
    /// `parse_connection_profile` with its own named warning and the remaining
    /// profiles still import.
    ///
    /// Filed upstream as v4 bug 105 (v4 `b6c6d7793`); **v4 converged at
    /// `679e450e3`** — the seeding call moved inside the per-item `try` (the
    /// catch reads `rawProfile.name`/`rawProfile.id`) and the helper now
    /// type-tests the provider (`typeof seeded.provider === 'string'`).
    /// Measured at the P4.D131 pinned regen (drift-ledger §5.4): v4 names the
    /// item, skips it, and imports the rest — byte-for-byte v5's behavior.
    /// The oracle half, `system_import_state`'s `execute_bug105_seed_abort`,
    /// now pins the SHARED behavior as a plain state-compared equality; this
    /// test remains the within-importer half — the loop itself carrying on to
    /// the next profile. The corpus item that used to carry `provider: 42` was
    /// moved to a wrong-typed `modelName` so the five-named-warnings arm keeps
    /// measuring what it exists for on both sides.
    #[test]
    fn a_non_string_provider_is_named_and_does_not_abort_the_import() {
        let conn = Connection::open_in_memory().unwrap();
        migrated_table(&conn);
        let profiles = vec![
            json!({
                "id": "a2000000-0000-4000-8000-000000000001",
                "name": "Broken Connection",
                "provider": 42,
                "modelName": "a-model",
            }),
            profile_record(
                "a2000000-0000-4000-8000-000000000002",
                "Fine Connection",
                "ANTHROPIC",
                json!({}),
            ),
        ];
        let mut id_map = IdMap::default();
        let mut warnings: Vec<String> = Vec::new();
        let counts = import_connection_profiles(
            &conn,
            "user",
            &profiles,
            &ImportOptions::seed_defaults(),
            &mut id_map,
            &mut warnings,
        )
        .expect("v5 must not abort the import on one malformed profile");

        assert_eq!(counts.imported, 1, "the sound profile still imports");
        assert_eq!(warnings.len(), 1, "exactly the broken one is named");
        assert!(
            warnings[0].starts_with("Failed to import connection profile \"Broken Connection\": "),
            "the warning must name the item: {}",
            warnings[0]
        );
        // …and the survivor still got its seeding.
        assert_eq!(stored_image_flag(&conn, "Fine Connection"), Some(1));
        assert_eq!(stored_prefill(&conn, "Fine Connection"), None);
    }
}
