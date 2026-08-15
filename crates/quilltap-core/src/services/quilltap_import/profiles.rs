//! v4 `import-profiles.ts` — import connection / image / embedding profiles.
//! All three share the same conflict-strategy shape and never restore API keys
//! (`apiKeyId` is forced to null on insert).
//!
//! Per v4, a per-item failure (bad shape, DB error) is logged and the item is
//! dropped WITHOUT a warnings entry — only the loop preambles (the `findAll`
//! that seeds the taken-name set) throw into `executeImport`'s outer catch.
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
use crate::services::profile_names::{make_unique_profile_name, normalize_profile_name};

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
}

/// v4 `LEGACY_IMAGE_CAPABLE_PROVIDERS` — seeds `supportsImageUpload` for exports
/// that predate the per-profile flag.
const LEGACY_IMAGE_CAPABLE_PROVIDERS: &[&str] = &["OPENAI", "ANTHROPIC", "GOOGLE", "GROK"];

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
    /// export never emits an explicit `null` — a never-chosen profile's
    /// net-read OMITS the key (NULL boolean → `undefined` → dropped by
    /// `JSON.stringify`) — so folding null and absent into `None` here is
    /// faithful for every bundle v4 can produce. (A hand-crafted bundle
    /// carrying a literal `null` would get an explicit NULL written by v4's
    /// `insertOne` and the column OMITTED by v5 — observable only on a
    /// migrated instance whose DDL default is 1. Recorded, not modeled.)
    #[serde(default)]
    multi_character_prefill: Option<bool>,
    #[serde(default)]
    max_tokens: Option<f64>,
    #[serde(default)]
    is_dangerous_compatible: bool,
    /// `None` = the key was absent — the pre-validation legacy fold fires.
    #[serde(default)]
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
                        let Some(p) = parse_connection_profile(raw_profile) else {
                            return Ok(());
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

            let Some(p) = parse_connection_profile(raw_profile) else {
                return Ok(());
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
            // v4 `moduleLogger.warn('Failed to import connection profile', …)` —
            // logged, dropped, NO warnings entry.
            tracing::warn!(profile_id = %source_id, error = %e, "Failed to import connection profile");
        }
    }

    Ok(Counts { imported, skipped })
}

/// Deserialize with the pre-validation shape checks; `None` mirrors v4's Zod
/// throw → per-item catch (logged, no warning, item dropped).
fn parse_connection_profile(raw: &Value) -> Option<ImportedConnectionProfile> {
    match serde_json::from_value::<ImportedConnectionProfile>(raw.clone()) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to import connection profile");
            None
        }
    }
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
    // Older exports predate the per-profile supportsImageUpload flag; seed it
    // from the historic provider capability map so image support round-trips.
    let supports_image_upload = p
        .supports_image_upload
        .unwrap_or_else(|| LEGACY_IMAGE_CAPABLE_PROVIDERS.contains(&p.provider.as_str()));
    let _ = raw;
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
        // `...profileData` spread, so a 4.9 bundle's stored choice round-trips
        // (absent → column omitted, the DDL default / tri-state decides — the
        // pre-4.9 shape). Landed at the round's unification: P4.D79's ordered
        // edit, applied once P4.48's ownership of this file had merged.
        multi_character_prefill: p.multi_character_prefill,
        model_class: p.model_class,
        max_context: p.max_context,
        max_tokens: p.max_tokens,
        is_dangerous_compatible: p.is_dangerous_compatible,
        supports_image_upload,
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
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = image_profiles::ImageProfilesRepository::new(main);

    for raw in profiles {
        let source_id = super::id_of(raw);
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
            let Ok(p) = serde_json::from_value::<ImportedImageProfile>(raw.clone()) else {
                return Ok(());
            };
            let name = p.name.clone();
            let new_id = create_image_profile(options, &source_id, &repo, user_id, p, name)?;
            id_map.set(source_id.clone(), new_id);
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
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
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = embedding_profiles::EmbeddingProfilesRepository::new(main);

    for raw in profiles {
        let source_id = super::id_of(raw);
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
            let Ok(p) = serde_json::from_value::<ImportedEmbeddingProfile>(raw.clone()) else {
                return Ok(());
            };
            let name = p.name.clone();
            let new_id = create_embedding_profile(options, &source_id, &repo, user_id, p, name)?;
            id_map.set(source_id.clone(), new_id);
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
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
               modelClass TEXT, maxContext REAL, maxTokens REAL, \
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

    /// The unification wire's pin (P4.D79's ordered edit, applied at unify):
    /// a 4.9 bundle's stored `multiCharacterPrefill` round-trips through the
    /// import, and an absent key still lets the DDL default decide (here the
    /// MIGRATED shape's `DEFAULT 1`; on a fresh generateDDL instance, NULL —
    /// pinned by `connection_profiles_tier2`).
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
        let counts = import_connection_profiles(
            &conn,
            "user",
            &profiles,
            &ImportOptions::seed_defaults(),
            &mut id_map,
        )
        .unwrap();
        assert_eq!(counts.imported, 3);
        // The stored choice round-trips (the pre-unify code imported ALL THREE
        // as "never chosen" — this is the arm that reddens on a regression)...
        assert_eq!(stored_prefill(&conn, "Off"), Some(0));
        assert_eq!(stored_prefill(&conn, "On"), Some(1));
        // ...and an absent key is still the column-omitting INSERT: the
        // migrated DDL default answers, not an explicit NULL write.
        assert_eq!(stored_prefill(&conn, "Silent"), Some(1));
    }
}
