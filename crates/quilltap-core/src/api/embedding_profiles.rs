//! The embedding-profiles management dispatch handlers (P4.9H2A) — a differential
//! port of v4's `embedding-profiles/route.ts` (+ `[id]/route.ts`), composed over
//! the ported `db::embedding_profiles` repo (create/update/delete + the new
//! `find_by_name` / `unset_all_defaults` / nullable-clearing `EpUpdate`
//! tri-state), the api-key + tag enrichment reads, the BUILTIN `vocabularyStats`
//! and `embeddingStats` list enrichment, and the queue-service enqueue helpers.
//!
//! **The PUT trigger matrix** ([`embedding_profile_update`]) is the subtle heart:
//! any change that alters what vectors the *default* profile produces forces a
//! full re-embed; a truncation-only narrowing is a pure-local re-apply; nothing
//! fires on a non-default profile or a `normalizeL2`-alone edit. v4's doctrine
//! comment is carried verbatim.
//!
//! `list-providers` / `list-models` reproduce v4's static plugin catalogs from
//! v5's provider manifest (`embeddings`-capable providers) plus the manifest-
//! omitted BUILTIN (the pinned Almanack omission is a DIFFERENT surface — not
//! disturbed here). `fetch-models` is a live-provider call and stays a loud
//! refusal arm this round (the `imageProfileListModels` precedent + the P4.D33
//! OpenRouter-SDK pagination bank).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{api_keys, embedding_profiles as ep, tags, tfidf_vocabulary};
use crate::provider_manifest::Registry;
use crate::services::queue_service;

// P4.56: the three profile-update handlers share ONE reader for the JS
// semantics of `apiKeyId` / `baseUrl`; only the consequences differ per site.
use super::settings::{classify_api_key_id, classify_base_url, ApiKeyIdPatch, BaseUrlPatch};
use super::types::{ErrorKind, Response};

// ===========================================================================
// Response helpers
// ===========================================================================

/// v4's outer-catch fixed sentences (`serverError('Failed to …')`) — the wire
/// answers the route's FIXED sentence and the underlying error goes to tracing
/// at the swallow site (the P4.18 arm; log output is outside the differential
/// contract). §3 unify-review fix: the original `internal(e)` leaked
/// `e.to_string()` onto the wire where v4 answers a constant.
fn internal_fixed(sentence: &str, e: impl std::fmt::Display) -> Response {
    tracing::error!(
        target: "quilltap::api::embedding_profiles",
        error = %e,
        "{sentence}"
    );
    Response::error(ErrorKind::Internal, sentence)
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
/// v4 create's hand-rolled `{error: '…'}` 409 and PUT's `conflict('…')` produce
/// the SAME body (`errorResponse` and the hand-rolled literal both emit
/// `{error: message}` at 409) — so both map to `ErrorKind::Conflict` here.
fn conflict(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Conflict, msg)
}
fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' embedding-profile action is recognized but not yet available."),
    )
}

// ===========================================================================
// Enrichment (v4 `enrichWithApiKey` / `enrichWithTags` / `enrichProfile`)
// ===========================================================================

/// v4 `enrichWithApiKey` — `{id, label, provider, isActive} | null`. `null` when
/// the id is falsy OR the key is not found; never the key value.
fn enrich_api_key(
    conn: &Connection,
    api_key_id: Option<&str>,
) -> Result<Value, crate::db::DbError> {
    let Some(id) = api_key_id else {
        return Ok(Value::Null);
    };
    Ok(match api_keys::find_by_id(conn, id)? {
        Some(k) => json!({
            "id": k.id,
            "label": k.label,
            "provider": k.provider,
            "isActive": k.is_active,
        }),
        None => Value::Null,
    })
}

/// v4 `enrichWithTags` — `[{tagId, tag}]` in INPUT tag-id order, dropping ids with
/// no matching tag. Empty input → `[]`.
fn enrich_tags(conn: &Connection, tag_ids: &[String]) -> Result<Value, crate::db::DbError> {
    let mut out = Vec::new();
    for id in tag_ids {
        if let Some(tag) = tags::find_full_by_id(conn, id)? {
            out.push(json!({ "tagId": id, "tag": tag }));
        }
    }
    Ok(Value::Array(out))
}

/// The `tags` string array of a marshaled profile (or the empty slice).
fn tag_ids_of(profile: &Value) -> Vec<String> {
    profile
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// v4 BUILTIN `vocabularyStats` — `{vocabularySize, avgDocLength, includeBigrams,
/// fittedAt}` from `tfidfVocabularies.findByProfileId`, or `null` when the
/// profile is not BUILTIN or has no fitted vocabulary. `vocabularySize` is the
/// integer-valued REAL rendered as a JSON integer (v4's better-sqlite3 path).
fn vocabulary_stats(
    conn: &Connection,
    provider: &str,
    profile_id: &str,
) -> Result<Value, crate::db::DbError> {
    if provider != "BUILTIN" {
        return Ok(Value::Null);
    }
    Ok(
        match tfidf_vocabulary::find_by_profile_id(conn, profile_id)? {
            Some(v) => {
                let mut m = Map::new();
                m.insert(
                    "vocabularySize".into(),
                    crate::db::js_number_to_json(v.vocabulary_size),
                );
                m.insert(
                    "avgDocLength".into(),
                    crate::db::js_number_to_json(v.avg_doc_length),
                );
                m.insert("includeBigrams".into(), Value::Bool(v.include_bigrams));
                m.insert("fittedAt".into(), Value::String(v.fitted_at));
                Value::Object(m)
            }
            None => Value::Null,
        },
    )
}

/// v4 `embeddingStatus.getStatsByProfileId` — `{total, pending, embedded, failed}`
/// by counting `embedding_status` rows for the profile. Returned only when
/// `total > 0` (v4 attaches it only then); `None` here maps to `null` at the call
/// site, and any error is swallowed (v4's `try/catch` leaves it null). Inlined
/// rather than added to `db::embedding_status` to stay within this lane's owned
/// files.
fn embedding_stats(conn: &Connection, profile_id: &str) -> Value {
    let row: Result<(i64, i64, i64, i64), rusqlite::Error> = conn.query_row(
        "SELECT COUNT(*), \
                SUM(CASE WHEN status = 'PENDING' THEN 1 ELSE 0 END), \
                SUM(CASE WHEN status = 'EMBEDDED' THEN 1 ELSE 0 END), \
                SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) \
         FROM embedding_status WHERE profileId = ?1",
        rusqlite::params![profile_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    );
    match row {
        Ok((total, pending, embedded, failed)) if total > 0 => json!({
            "total": total,
            "pending": pending,
            "embedded": embedded,
            "failed": failed,
        }),
        // total == 0 → not attached; an error is swallowed (v4's catch → null).
        _ => Value::Null,
    }
}

/// Attach `apiKey` + enriched `tags` to a marshaled profile (v4's GET
/// `enrichProfile` → `{...profile, apiKey, tags}`).
fn enrich_get(conn: &Connection, mut profile: Value) -> Result<Value, crate::db::DbError> {
    let api_key = enrich_api_key(conn, profile.get("apiKeyId").and_then(Value::as_str))?;
    let tags_enriched = enrich_tags(conn, &tag_ids_of(&profile))?;
    let obj = profile.as_object_mut().unwrap();
    obj.insert("apiKey".into(), api_key);
    obj.insert("tags".into(), tags_enriched);
    Ok(profile)
}

// ===========================================================================
// Handlers
// ===========================================================================

/// v4 `GET /api/v1/embedding-profiles` → `{profiles, count}`. Each profile is
/// enriched (`apiKey`, enriched `tags`, BUILTIN `vocabularyStats`,
/// `embeddingStats` when `total > 0`) then sorted default-first, createdAt DESC.
pub fn embedding_profile_list(db: &Db, user_id: &str) -> Response {
    let result = db.read_main(|conn| {
        let profiles = ep::find_by_user_id(conn, user_id)?;
        let mut enriched: Vec<Value> = Vec::with_capacity(profiles.len());
        for mut p in profiles {
            let provider = p
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let id = p
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let api_key = enrich_api_key(conn, p.get("apiKeyId").and_then(Value::as_str))?;
            let tags_enriched = enrich_tags(conn, &tag_ids_of(&p))?;
            let vocab = vocabulary_stats(conn, &provider, &id)?;
            let stats = embedding_stats(conn, &id);
            let obj = p.as_object_mut().unwrap();
            obj.insert("apiKey".into(), api_key);
            obj.insert("tags".into(), tags_enriched);
            obj.insert("vocabularyStats".into(), vocab);
            obj.insert("embeddingStats".into(), stats);
            enriched.push(p);
        }
        // Default-first, then createdAt DESC (v4's comparator).
        enriched.sort_by(|a, b| {
            let ad = a.get("isDefault").and_then(Value::as_bool).unwrap_or(false);
            let bd = b.get("isDefault").and_then(Value::as_bool).unwrap_or(false);
            if ad != bd {
                return if bd {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Less
                };
            }
            let at = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
            let bt = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
            bt.cmp(at)
        });
        let count = enriched.len();
        Ok(json!({ "profiles": enriched, "count": count }))
    });
    match result {
        Ok(v) => Response::EmbeddingProfile(v),
        Err(e) => internal_fixed("Failed to fetch embedding profiles", e),
    }
}

/// v4 `GET /api/v1/embedding-profiles/[id]` → the profile + `apiKey` + enriched
/// tags; 404 on absent.
pub fn embedding_profile_get(db: &Db, profile_id: &str) -> Response {
    let result = db.read_main(|conn| match ep::find_full_json_by_id(conn, profile_id)? {
        Some(p) => Ok(Some(enrich_get(conn, p)?)),
        None => Ok(None),
    });
    match result {
        Ok(Some(v)) => Response::EmbeddingProfile(v),
        Ok(None) => not_found("Embedding profile"),
        Err(e) => internal_fixed("Failed to fetch embedding profile", e),
    }
}

/// v4 `POST /api/v1/embedding-profiles` — hand validation (exact messages),
/// apiKeyId existence, duplicate-name 409, isDefault→unset-others, create, the
/// created-as-default reindex enqueue (non-fatal), then the 201 created profile
/// + `apiKey`.
pub async fn embedding_profile_create(db: &Db, user_id: &str, body: Value) -> Response {
    let name = match body.get("name").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return bad_request("Name is required"),
    };
    let provider = match body.get("provider").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return bad_request("Provider is required"),
    };
    let model_name = match body.get("modelName").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return bad_request("Model name is required"),
    };
    // dimensions: undefined/null OK; else must be a positive number.
    let dimensions: Option<f64> = match body.get("dimensions") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_f64() {
            Some(n) if n > 0.0 => Some(n),
            _ => return bad_request("Dimensions must be a positive number"),
        },
    };
    // truncateToDimensions: undefined/null OK; else a positive INTEGER.
    let truncate_to_dimensions: Option<f64> = match body.get("truncateToDimensions") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_f64() {
            Some(n) if n > 0.0 && n.fract() == 0.0 => Some(n),
            _ => return bad_request("truncateToDimensions must be a positive integer"),
        },
    };
    // normalizeL2: undefined OK (default true); else must be a boolean.
    let normalize_l2: bool = match body.get("normalizeL2") {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => return bad_request("normalizeL2 must be a boolean"),
    };
    let api_key_id = body
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(id) = &api_key_id {
        match db.read_main(|conn| api_keys::find_by_id(conn, id)) {
            Ok(Some(_)) => {}
            Ok(None) => return not_found("API key"),
            Err(e) => return internal_fixed("Failed to create embedding profile", e),
        }
    }
    match db.read_main({
        let name = name.clone();
        let uid = user_id.to_string();
        move |conn| ep::find_by_name(conn, &uid, &name)
    }) {
        Ok(Some(_)) => return conflict("An embedding profile with this name already exists"),
        Ok(None) => {}
        Err(e) => return internal_fixed("Failed to create embedding profile", e),
    }
    let base_url = body
        .get("baseUrl")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let is_default = body
        .get("isDefault")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let create = ep::EpCreate {
        user_id: user_id.to_string(),
        name: name.clone(),
        provider: provider.clone(),
        api_key_id: api_key_id.clone(),
        base_url: base_url.clone(),
        model_name: model_name.clone(),
        dimensions,
        truncate_to_dimensions,
        normalize_l2,
        is_default,
        tags: Vec::new(),
    };
    let opts = ep::CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let unset_now = now.clone();
    let uid = user_id.to_string();
    let write = db
        .write(move |ws| {
            let conn = ws.main().connection();
            if is_default {
                ep::EmbeddingProfilesRepository::new(conn).unset_all_defaults(&uid, &unset_now)?;
            }
            ep::EmbeddingProfilesRepository::new(conn).create(&create, &opts)
        })
        .await;
    if let Err(e) = write {
        return internal_fixed("Failed to create embedding profile", e);
    }

    // Created-as-default → trigger initial embedding immediately (v4:
    // `enqueueEmbeddingReindexAll(user.id, {profileId})`, no scope). Non-fatal.
    if is_default {
        if let Err(e) = queue_service::enqueue_embedding_reindex_all(db, user_id, &id, None).await {
            tracing::warn!(profile_id = %id, error = %e, "Failed to trigger initial embedding for new default profile");
        }
    }

    // The 201 body = v4's `{...profile, apiKey}` — the created object with
    // apiKeyId/baseUrl/dimensions/truncateToDimensions present as null-or-value,
    // tags raw [].
    let api_key = match db.read_main(|conn| enrich_api_key(conn, api_key_id.as_deref())) {
        Ok(v) => v,
        Err(e) => return internal_fixed("Failed to create embedding profile", e),
    };
    let mut obj = Map::new();
    obj.insert("id".into(), Value::String(id));
    obj.insert("userId".into(), Value::String(user_id.to_string()));
    obj.insert("name".into(), Value::String(name));
    obj.insert("provider".into(), Value::String(provider));
    obj.insert(
        "apiKeyId".into(),
        api_key_id.map(Value::String).unwrap_or(Value::Null),
    );
    obj.insert(
        "baseUrl".into(),
        base_url.map(Value::String).unwrap_or(Value::Null),
    );
    obj.insert("modelName".into(), Value::String(model_name));
    obj.insert(
        "dimensions".into(),
        dimensions
            .map(crate::db::js_number_to_json)
            .unwrap_or(Value::Null),
    );
    obj.insert(
        "truncateToDimensions".into(),
        truncate_to_dimensions
            .map(crate::db::js_number_to_json)
            .unwrap_or(Value::Null),
    );
    obj.insert("normalizeL2".into(), Value::Bool(normalize_l2));
    obj.insert("isDefault".into(), Value::Bool(is_default));
    obj.insert("tags".into(), Value::Array(Vec::new()));
    obj.insert("createdAt".into(), Value::String(now.clone()));
    obj.insert("updatedAt".into(), Value::String(now));
    obj.insert("apiKey".into(), api_key);
    Response::EmbeddingProfile(Value::Object(obj))
}

/// v4 `PUT /api/v1/embedding-profiles/[id]` — 404 / per-field `!== undefined`
/// gating (apiKeyId tri-state clear; `baseUrl || null`; dimensions/truncate
/// tri-state; isDefault→unset-others), then **the trigger matrix** (evaluated
/// AFTER the update commits) → the updated profile (in-memory merge) + enrichment
/// + `reembeddingTriggered`.
pub async fn embedding_profile_update(
    db: &Db,
    user_id: &str,
    profile_id: &str,
    body: Value,
) -> Response {
    let existing = match db.read_main(|conn| ep::find_full_json_by_id(conn, profile_id)) {
        Ok(Some(p)) => p,
        Ok(None) => return not_found("Embedding profile"),
        Err(e) => return internal_fixed("Failed to update embedding profile", e),
    };
    let Some(body_obj) = body.as_object() else {
        return bad_request("Expected object");
    };

    let mut merged = existing.clone();
    let mo = merged.as_object_mut().unwrap();
    let mut patch = ep::EpUpdate::default();

    // Condition flags for the matrix, captured as we gate each field.
    let mut provider_changed = false;
    let mut model_changed = false;
    let mut dimensions_changed = false;
    let mut truncate_changed = false;
    let mut set_default = false;

    let existing_provider = existing
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("");
    let existing_model = existing
        .get("modelName")
        .and_then(Value::as_str)
        .unwrap_or("");
    // existing.dimensions ?? null, existing.truncateToDimensions ?? null (absent
    // key on the marshaled shape means the column was NULL).
    let existing_dimensions = existing.get("dimensions").and_then(Value::as_f64);
    let existing_truncate = existing.get("truncateToDimensions").and_then(Value::as_f64);
    let existing_is_default = existing
        .get("isDefault")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if let Some(name_v) = body_obj.get("name") {
        let name = match name_v.as_str() {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return bad_request("Name must be a non-empty string"),
        };
        match db.read_main({
            let name = name.clone();
            let uid = user_id.to_string();
            move |conn| ep::find_by_name(conn, &uid, &name)
        }) {
            Ok(Some(dup)) if dup.get("id").and_then(Value::as_str) != Some(profile_id) => {
                return conflict("An embedding profile with this name already exists")
            }
            Ok(_) => {}
            Err(e) => return internal_fixed("Failed to update embedding profile", e),
        }
        mo.insert("name".into(), Value::String(name.clone()));
        patch.name = Some(name);
    }
    if let Some(provider_v) = body_obj.get("provider") {
        let provider = match provider_v.as_str() {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => return bad_request("Provider must be a non-empty string"),
        };
        provider_changed = provider != existing_provider;
        mo.insert("provider".into(), Value::String(provider.clone()));
        patch.provider = Some(provider);
    }
    if let Some(aki) = body_obj.get("apiKeyId") {
        match classify_api_key_id(aki) {
            ApiKeyIdPatch::Clear => {
                mo.insert("apiKeyId".into(), Value::Null);
                patch.api_key_id = Some(None);
            }
            ApiKeyIdPatch::Set(id) => {
                match db.read_main(|conn| api_keys::find_by_id(conn, id)) {
                    Ok(Some(_)) => {}
                    Ok(None) => return not_found("API key"),
                    Err(e) => return internal_fixed("Failed to update embedding profile", e),
                }
                mo.insert("apiKeyId".into(), Value::String(id.to_string()));
                patch.api_key_id = Some(Some(id.to_string()));
            }
            ApiKeyIdPatch::Refuse => return not_found("API key"),
        }
    }
    if let Some(bu) = body_obj.get("baseUrl") {
        match classify_base_url(bu) {
            BaseUrlPatch::Set(s) => {
                mo.insert("baseUrl".into(), Value::String(s.to_string()));
                patch.base_url = Some(Some(s.to_string()));
            }
            BaseUrlPatch::Clear => {
                mo.insert("baseUrl".into(), Value::Null);
                patch.base_url = Some(None);
            }
            BaseUrlPatch::Refuse => {
                return Response::error(ErrorKind::Internal, "Failed to update embedding profile")
            }
        }
    }
    if let Some(mn) = body_obj.get("modelName") {
        let model = match mn.as_str() {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return bad_request("Model name must be a non-empty string"),
        };
        model_changed = model != existing_model;
        mo.insert("modelName".into(), Value::String(model.clone()));
        patch.model_name = Some(model);
    }
    if let Some(dv) = body_obj.get("dimensions") {
        if dv.is_null() {
            dimensions_changed = existing_dimensions.is_some();
            // v4's Zod keeps an explicitly-cleared `.nullable().optional()` key
            // present-as-null in the PUT echo (base.repository `_update` →
            // validate) — same as the `apiKeyId` clear above. (§3 review fix:
            // this was `mo.remove(...)`, dropping the key; pinned by the
            // `update_clear_truncate_dims_null` corpus case, mutation-proven.)
            mo.insert("dimensions".into(), Value::Null);
            patch.dimensions = Some(None);
        } else {
            match dv.as_f64() {
                Some(n) if n > 0.0 => {
                    dimensions_changed = existing_dimensions != Some(n);
                    mo.insert("dimensions".into(), crate::db::js_number_to_json(n));
                    patch.dimensions = Some(Some(n));
                }
                _ => return bad_request("Dimensions must be a positive number"),
            }
        }
    }
    if let Some(tv) = body_obj.get("truncateToDimensions") {
        if tv.is_null() {
            truncate_changed = existing_truncate.is_some();
            // Present-as-null in the echo, as with `dimensions` above (§3 fix).
            mo.insert("truncateToDimensions".into(), Value::Null);
            patch.truncate_to_dimensions = Some(None);
        } else {
            match tv.as_f64() {
                Some(n) if n > 0.0 && n.fract() == 0.0 => {
                    truncate_changed = existing_truncate != Some(n);
                    mo.insert(
                        "truncateToDimensions".into(),
                        crate::db::js_number_to_json(n),
                    );
                    patch.truncate_to_dimensions = Some(Some(n));
                }
                _ => return bad_request("truncateToDimensions must be a positive integer"),
            }
        }
    }
    if let Some(nl) = body_obj.get("normalizeL2") {
        let b = match nl.as_bool() {
            Some(b) => b,
            None => return bad_request("normalizeL2 must be a boolean"),
        };
        mo.insert("normalizeL2".into(), Value::Bool(b));
        patch.normalize_l2 = Some(b);
    }
    if let Some(d) = body_obj.get("isDefault") {
        let b = match d.as_bool() {
            Some(b) => b,
            None => return bad_request("isDefault must be a boolean"),
        };
        set_default = b;
        mo.insert("isDefault".into(), Value::Bool(b));
        patch.is_default = Some(b);
    }

    let now = crate::clock::now_iso();
    patch.updated_at = now.clone();
    mo.insert("updatedAt".into(), Value::String(now.clone()));

    let uid = user_id.to_string();
    let id = profile_id.to_string();
    let unset_now = now.clone();
    let write = db
        .write(move |ws| {
            let conn = ws.main().connection();
            if set_default {
                ep::EmbeddingProfilesRepository::new(conn).unset_all_defaults(&uid, &unset_now)?;
            }
            ep::EmbeddingProfilesRepository::new(conn).update(&id, &patch)
        })
        .await;
    if let Err(e) = write {
        return internal_fixed("Failed to update embedding profile", e);
    }

    // ── The PUT trigger matrix ─────────────────────────────────────────────
    // Any change that alters which vectors the default profile produces
    // forces a full re-embed. There is one embedding standard per instance:
    // the default profile's output. That includes a DIFFERENT profile
    // becoming the default (the previous corpus was built by the old
    // default and is now non-conforming wholesale) — not just provider/
    // model/dimension edits to an already-default profile.
    let became_default = set_default && !existing_is_default;
    let updated_is_default = merged
        .get("isDefault")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let updated_provider = merged.get("provider").and_then(Value::as_str).unwrap_or("");

    let needs_full_reindex = updated_is_default
        && (provider_changed || model_changed || dimensions_changed || became_default);

    let reembedding_triggered = needs_full_reindex || (truncate_changed && updated_is_default);

    if needs_full_reindex {
        // Invalidate all existing embeddings, then trigger the right reindex.
        if let Err(e) = invalidate_all_embeddings(db, profile_id, &now).await {
            return internal_fixed("Failed to update embedding profile", e);
        }
        let enqueued = if updated_provider == "BUILTIN" {
            // BUILTIN: refit vocabulary first (which will trigger reindex).
            queue_service::enqueue_embedding_refit(db, user_id, profile_id, true).await
        } else {
            // External providers: directly reindex all (no scope).
            queue_service::enqueue_embedding_reindex_all(db, user_id, profile_id, None).await
        };
        if let Err(e) = enqueued {
            return internal_fixed("Failed to update embedding profile", e);
        }
    } else if truncate_changed && updated_is_default {
        // Matryoshka truncation change alone: narrowing is a pure-local
        // slice+renormalise; growing (or clearing to null) needs a full reindex.
        let old_effective = existing_truncate.or(existing_dimensions);
        let new_target = merged.get("truncateToDimensions").and_then(Value::as_f64);
        let narrowed = match (new_target, old_effective) {
            (Some(nt), Some(oe)) => nt <= oe,
            _ => false,
        };
        if narrowed {
            if let Err(e) =
                queue_service::enqueue_embedding_reapply_profile(db, user_id, profile_id).await
            {
                return internal_fixed("Failed to update embedding profile", e);
            }
        } else {
            if let Err(e) = invalidate_all_embeddings(db, profile_id, &now).await {
                return internal_fixed("Failed to update embedding profile", e);
            }
            if let Err(e) =
                queue_service::enqueue_embedding_reindex_all(db, user_id, profile_id, None).await
            {
                return internal_fixed("Failed to update embedding profile", e);
            }
        }
    }

    // The echo = v4's `{...updatedProfile, ...enriched, reembeddingTriggered}` —
    // enrich the in-memory merged bag. P4.55 corrected this comment: it used to
    // read "nulls dropped when cleared", which the §3 unification review of the
    // P4.9H2A round had already made false — a cleared key is echoed as an
    // EXPLICIT `null` in schema position (`mo.insert(k, Value::Null)`), exactly
    // as v4's in-memory merge does, pinned by `update_clear_apikey` and
    // `update_clear_truncate_dims_null`.
    let enriched = match db.read_main(|conn| {
        let api_key = enrich_api_key(conn, merged.get("apiKeyId").and_then(Value::as_str))?;
        let tags_enriched = enrich_tags(conn, &tag_ids_of(&merged))?;
        Ok::<_, crate::db::DbError>((api_key, tags_enriched))
    }) {
        Ok(v) => v,
        Err(e) => return internal_fixed("Failed to update embedding profile", e),
    };
    let mo = merged.as_object_mut().unwrap();
    mo.insert("apiKey".into(), enriched.0);
    mo.insert("tags".into(), enriched.1);
    mo.insert(
        "reembeddingTriggered".into(),
        Value::Bool(reembedding_triggered),
    );
    Response::EmbeddingProfile(merged)
}

/// v4 `invalidateAllEmbeddings(userId, profileId)` = `markAllPendingByProfileId`
/// — reset every status row for the profile to PENDING; returns the count.
async fn invalidate_all_embeddings(
    db: &Db,
    profile_id: &str,
    now: &str,
) -> Result<usize, crate::db::DbError> {
    let id = profile_id.to_string();
    let now = now.to_string();
    db.write(move |ws| {
        crate::db::embedding_status::EmbeddingStatusRepository::new(ws.main().connection())
            .mark_all_pending_by_profile_id(&id, &now)
    })
    .await
}

/// v4 `DELETE /api/v1/embedding-profiles/[id]` — 404 / delete / `{message}` (the
/// `tfidf_vocabularies` FK cascades in DDL).
pub async fn embedding_profile_delete(db: &Db, profile_id: &str) -> Response {
    let existing = db.read_main(|conn| ep::find_by_id(conn, profile_id));
    match existing {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Embedding profile"),
        Err(e) => return internal_fixed("Failed to delete embedding profile", e),
    }
    let id = profile_id.to_string();
    let out = db
        .write(move |ws| ep::EmbeddingProfilesRepository::new(ws.main().connection()).delete(&id))
        .await;
    match out {
        Ok(_) => Response::EmbeddingProfile(json!({
            "message": "Embedding profile deleted successfully"
        })),
        Err(e) => internal_fixed("Failed to delete embedding profile", e),
    }
}

/// v4 `POST /api/v1/embedding-profiles/[id]?action=refit` — 404 / BUILTIN gate /
/// enqueue refit (`triggerReindex: true`) / `{message, jobId}`.
pub async fn embedding_profile_refit(db: &Db, user_id: &str, profile_id: &str) -> Response {
    let profile = match db.read_main(|conn| ep::find_by_id(conn, profile_id)) {
        Ok(Some(p)) => p,
        Ok(None) => return not_found("Embedding profile"),
        Err(e) => return internal_fixed("Failed to trigger refit", e),
    };
    if profile.provider != "BUILTIN" {
        return bad_request(
            "Refit is only available for built-in embedding profiles. Use reindex action for external providers.",
        );
    }
    match queue_service::enqueue_embedding_refit(db, user_id, profile_id, true).await {
        Ok(job_id) => Response::EmbeddingProfile(json!({
            "message": "Vocabulary refit job enqueued",
            "jobId": job_id,
        })),
        Err(e) => internal_fixed("Failed to trigger refit", e),
    }
}

/// v4 `POST /api/v1/embedding-profiles/[id]?action=reindex` — 404, scope
/// validation (`mismatched-dim` needs a target dim), full-scope invalidate-up-
/// front, enqueue, `{message, jobId, scope, invalidatedCount}`. `scope` defaults
/// to `all` (v4's legacy no-body call).
pub async fn embedding_profile_reindex(
    db: &Db,
    user_id: &str,
    profile_id: &str,
    scope: Option<&Value>,
) -> Response {
    let profile = match db.read_main(|conn| ep::find_by_id(conn, profile_id)) {
        Ok(Some(p)) => p,
        Ok(None) => return not_found("Embedding profile"),
        Err(e) => return internal_fixed("Failed to trigger reindex", e),
    };
    // Optional body `{scope}`; absent = 'all'; anything else 400.
    //
    // The tri-state is load-bearing: v4's guard is `body.scope !== undefined`,
    // so an explicit `null` REACHES the refusal where an absent key defaults to
    // 'all'. And the refusal interpolates `String(body.scope)`, NOT
    // `JSON.stringify` — so an object reads `[object Object]` and an array
    // reads its comma-joined elements (P4.60: `Value::to_string()` printed the
    // JSON text for both, and `and_then(Value::as_str)` could not have told the
    // explicit null from an absent key at all).
    let scope = match scope {
        None => "all",
        Some(Value::String(s)) if s == "all" => "all",
        Some(Value::String(s)) if s == "mismatched-dim" => "mismatched-dim",
        Some(other) => {
            let shown = crate::pascal::js_value::to_js_string(other);
            return bad_request(format!(
                "Invalid scope: {shown}. Expected 'all' or 'mismatched-dim'."
            ));
        }
    };
    // Partial scope needs a target dim (truncateToDimensions ?? dimensions).
    if scope == "mismatched-dim" {
        let target = profile.truncate_to_dimensions.or(profile.dimensions);
        if !matches!(target, Some(t) if t > 0.0) {
            return bad_request(format!(
                "Profile {profile_id} has no truncateToDimensions or dimensions set, so 'mismatched-dim' has nothing to compare against. Use scope='all' instead."
            ));
        }
    }

    // Full scope: invalidate every status row up front (the count feeds the UI's
    // pending counter). Partial scope leaves status alone.
    let now = crate::clock::now_iso();
    let mut invalidated_count: usize = 0;
    if scope == "all" {
        match invalidate_all_embeddings(db, profile_id, &now).await {
            Ok(n) => invalidated_count = n,
            Err(e) => return internal_fixed("Failed to trigger reindex", e),
        }
    }

    match queue_service::enqueue_embedding_reindex_all(db, user_id, profile_id, Some(scope)).await {
        Ok(job_id) => {
            let message = if scope == "mismatched-dim" {
                "Partial re-embedding job enqueued — only rows whose stored vector dim differs from the profile target will be re-embedded."
            } else {
                "Re-embedding job enqueued"
            };
            Response::EmbeddingProfile(json!({
                "message": message,
                "jobId": job_id,
                "scope": scope,
                "invalidatedCount": invalidated_count,
            }))
        }
        Err(e) => internal_fixed("Failed to trigger reindex", e),
    }
}

/// v4 `POST /api/v1/embedding-profiles/[id]?action=reapply` — 404, no-truncation
/// 400, enqueue reapply, `{message, jobId, targetDimensions}`. No isDefault gate
/// (API-reachable from a non-default profile; only the SPA gates).
pub async fn embedding_profile_reapply(db: &Db, user_id: &str, profile_id: &str) -> Response {
    let profile = match db.read_main(|conn| ep::find_by_id(conn, profile_id)) {
        Ok(Some(p)) => p,
        Ok(None) => return not_found("Embedding profile"),
        Err(e) => return internal_fixed("Failed to trigger re-apply", e),
    };
    // `!profile.truncateToDimensions` — a null OR a falsy 0 both fall through.
    let target = match profile.truncate_to_dimensions {
        Some(t) if t != 0.0 => t,
        _ => {
            return bad_request(
                "Profile has no truncateToDimensions set. Re-apply only slices Matryoshka vectors; nothing to do.",
            )
        }
    };
    match queue_service::enqueue_embedding_reapply_profile(db, user_id, profile_id).await {
        Ok(job_id) => Response::EmbeddingProfile(json!({
            "message": "Embedding profile re-apply job enqueued. A backup of each affected database will be created before any rewrite.",
            "jobId": job_id,
            "targetDimensions": crate::db::js_number_to_json(target),
        })),
        Err(e) => internal_fixed("Failed to trigger re-apply", e),
    }
}

// ===========================================================================
// list-providers / list-models (v5 manifest + static catalog reproducing v4)
// ===========================================================================

/// v4 `getEmbeddingProviders()` — the provider ids whose plugin declares the
/// `embeddings` capability. v5's manifest carries OPENAI / OPENROUTER / OLLAMA
/// (in manifest order); BUILTIN is the manifest-omitted provider (the pinned
/// Almanack omission), appended here. v4's exact runtime order is plugin-load-
/// order dependent and unpinnable in the oracle sandbox (where no plugins
/// register); this deterministic manifest-order + BUILTIN is a documented v5
/// choice — the SET and each provider's models are byte-faithful.
fn embedding_provider_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = Registry::built_in()
        .all_providers()
        .iter()
        .filter(|m| m.capabilities.embeddings)
        .map(|m| m.id.as_str())
        .collect();
    ids.push("BUILTIN");
    ids
}

/// v4 `getEmbeddingModels(provider)` — the plugin's static embedding-model list.
/// Byte-transcribed from the four v4 plugins' `getEmbeddingModels()`
/// (`plugins/dist/qtap-plugin-{openai,ollama,openrouter,builtin-embeddings}`).
/// Each model is `{id, name, dimensions?, description?}` — `dimensions` is
/// OMITTED where the plugin leaves it undefined (BUILTIN's sole model). Unknown
/// provider → `[]`.
///
/// `pub` so `embedding_wire_equivalence` can pin it against v4's REAL
/// `plugin.getEmbeddingModels()` (P4.D101). The route-level list-models
/// differential cannot: v4's plugin registry is EMPTY in the jest sandbox, which
/// is why these arms were Rust-only until now.
pub fn embedding_models_for(provider: &str) -> Vec<Value> {
    let model = |id: &str, name: &str, dims: Option<i64>, desc: &str| -> Value {
        let mut m = Map::new();
        m.insert("id".into(), Value::String(id.into()));
        m.insert("name".into(), Value::String(name.into()));
        if let Some(d) = dims {
            m.insert("dimensions".into(), json!(d));
        }
        m.insert("description".into(), Value::String(desc.into()));
        Value::Object(m)
    };
    match provider {
        "OPENAI" => vec![
            model(
                "text-embedding-3-small",
                "Text Embedding 3 Small",
                Some(1536),
                "Smaller, faster, and cheaper. Good for most use cases.",
            ),
            model(
                "text-embedding-3-large",
                "Text Embedding 3 Large",
                Some(3072),
                "Larger model with higher accuracy for complex tasks.",
            ),
            model(
                "text-embedding-ada-002",
                "Text Embedding Ada 002",
                Some(1536),
                "Legacy model. Consider using text-embedding-3-small instead.",
            ),
        ],
        "OLLAMA" => vec![
            model(
                "nomic-embed-text",
                "Nomic Embed Text",
                Some(768),
                "High-quality open embedding model. Good balance of speed and accuracy.",
            ),
            model(
                "mxbai-embed-large",
                "MixedBread Embed Large",
                Some(1024),
                "Large embedding model with excellent performance.",
            ),
            model(
                "all-minilm",
                "All MiniLM",
                Some(384),
                "Fast and lightweight. Good for quick semantic search.",
            ),
            model(
                "snowflake-arctic-embed",
                "Snowflake Arctic Embed",
                Some(1024),
                "State-of-the-art retrieval embedding model.",
            ),
        ],
        "OPENROUTER" => vec![
            model(
                "openai/text-embedding-3-small",
                "OpenAI Text Embedding 3 Small",
                Some(1536),
                "OpenAI small embedding model, efficient for most use cases",
            ),
            model(
                "openai/text-embedding-3-large",
                "OpenAI Text Embedding 3 Large",
                Some(3072),
                "OpenAI large embedding model for highest quality",
            ),
            model(
                "openai/text-embedding-ada-002",
                "OpenAI Ada 002",
                Some(1536),
                "OpenAI legacy embedding model",
            ),
            model(
                "cohere/embed-english-v3.0",
                "Cohere Embed English v3",
                Some(1024),
                "Cohere English embedding model",
            ),
            model(
                "cohere/embed-multilingual-v3.0",
                "Cohere Embed Multilingual v3",
                Some(1024),
                "Cohere multilingual embedding model",
            ),
            model(
                "voyage/voyage-large-2",
                "Voyage Large 2",
                Some(1536),
                "Voyage AI large embedding model",
            ),
            model(
                "voyage/voyage-code-2",
                "Voyage Code 2",
                Some(1536),
                "Voyage AI embedding model optimized for code",
            ),
        ],
        // P4.D101 (v4 `781fc420`): NanoGPT's curated embedding catalogue,
        // byte-transcribed from `plugins/dist/qtap-plugin-nanogpt/models.ts`
        // (`STATIC_EMBEDDING_MODELS`), mirrored from its `/embedding-models`
        // listing. Every row carries dimensions.
        "NANOGPT" => vec![
            model(
                "text-embedding-3-small",
                "Text Embedding 3 Small",
                Some(1536),
                "OpenAI, routed through NanoGPT. Cost-effective default; supports dimension reduction.",
            ),
            model(
                "text-embedding-3-large",
                "Text Embedding 3 Large",
                Some(3072),
                "OpenAI, routed through NanoGPT. Highest accuracy; supports dimension reduction.",
            ),
            model(
                "BAAI/bge-m3",
                "BGE-M3",
                Some(1024),
                "Multilingual BAAI model with an 8K-token input window.",
            ),
            model(
                "jina-embeddings-v3",
                "Jina Embeddings v3",
                Some(1024),
                "Multilingual Jina model with an 8K-token input window.",
            ),
            model(
                "Qwen/Qwen3-Embedding-0.6B",
                "Qwen3 Embedding 0.6B",
                Some(1024),
                "Compact Qwen3 embedder with an 8K-token input window.",
            ),
            model(
                "qwen/qwen3-embedding-8b",
                "Qwen3 Embedding 8B",
                Some(4096),
                "Large Qwen3 embedder with a 32K-token input window.",
            ),
            model(
                "gemini-embedding-001",
                "Gemini Embedding 001",
                Some(3072),
                "Google embedding model routed through NanoGPT.",
            ),
        ],
        "BUILTIN" => vec![{
            // The BUILTIN model has no `dimensions` (they vary with vocabulary).
            let mut m = Map::new();
            m.insert("id".into(), Value::String("tfidf-bm25-v1".into()));
            m.insert("name".into(), Value::String("TF-IDF with BM25".into()));
            m.insert(
                "description".into(),
                Value::String(
                    "Offline embedding model using TF-IDF with BM25 enhancement and Porter stemming. \
Dimensions vary based on vocabulary size (typically 1000-50000). \
Requires fitting on your document corpus."
                        .into(),
                ),
            );
            Value::Object(m)
        }],
        _ => Vec::new(),
    }
}

/// v4 `GET /api/v1/embedding-profiles?action=list-providers` →
/// `{providers: [...]}` (the embedding-capable provider ids).
pub fn embedding_provider_list() -> Response {
    let providers: Vec<Value> = embedding_provider_ids()
        .into_iter()
        .map(|id| Value::String(id.to_string()))
        .collect();
    Response::EmbeddingProfile(json!({ "providers": providers }))
}

/// v4 `GET /api/v1/embedding-profiles?action=list-models` — with `provider`,
/// `{provider, models}`; without, the bare `Record<provider, models[]>` of all
/// providers with non-empty lists. Both carry v4's non-fatal `provider_models`
/// cache side-effect (`upsertModelsForProvider(..., 'embedding')`). The provider
/// is UPPER-CASED (v4's `provider?.toUpperCase()`); an unknown provider → 400.
pub async fn embedding_profile_list_models(db: &Db, provider: Option<String>) -> Response {
    let valid = embedding_provider_ids();
    match provider {
        Some(raw) => {
            let provider = raw.to_uppercase();
            if !valid.contains(&provider.as_str()) {
                return bad_request(format!(
                    "Invalid provider. Must be one of: {}",
                    valid.join(", ")
                ));
            }
            let models = embedding_models_for(&provider);
            cache_models(db, &provider, &models).await;
            Response::EmbeddingProfile(json!({ "provider": provider, "models": models }))
        }
        None => {
            let mut all = Map::new();
            for provider in valid {
                let models = embedding_models_for(provider);
                if !models.is_empty() {
                    cache_models(db, provider, &models).await;
                    all.insert(provider.to_string(), Value::Array(models));
                }
            }
            Response::EmbeddingProfile(Value::Object(all))
        }
    }
}

/// v4's non-fatal `providerModels.upsertModelsForProvider(provider,
/// models.map(m => ({modelId, displayName})), 'embedding')`. Best-effort: an
/// error is logged, never propagated (matching v4's `try/catch → warn`).
async fn cache_models(db: &Db, provider: &str, models: &[Value]) {
    let rows: Vec<crate::db::provider_models::PmCreate> = models
        .iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(Value::as_str)?;
            let name = m.get("name").and_then(Value::as_str)?;
            Some(crate::db::provider_models::PmCreate {
                provider: provider.to_string(),
                model_id: id.to_string(),
                model_type: "embedding".to_string(),
                display_name: name.to_string(),
                base_url: None,
                context_window: None,
                max_output_tokens: None,
                deprecated: false,
                experimental: false,
            })
        })
        .collect();
    let write = db
        .write(move |ws| {
            crate::db::provider_models::ProviderModelsRepository::new(ws.main().connection())
                .upsert_model_for_provider(&rows)
        })
        .await;
    if let Err(e) = write {
        tracing::warn!(provider = %provider, error = %e, "Failed to cache embedding models");
    }
}

// ===========================================================================
// Refusal arm (the live-provider fetch — deferred this round)
// ===========================================================================

/// v4 `GET /api/v1/embedding-profiles?action=fetch-models` — a LIVE provider call
/// (`getAvailableModels`). Refused via the family `not_available`, matching the
/// standing `imageProfileListModels`/`ValidateKey` precedent and the P4.D33
/// OpenRouter-SDK pagination bank.
///
/// **P4.D101 widens the refusal to NANOGPT.** NanoGPT's discovery is
/// `GET /api/v1/embedding-models`, and — unlike its image listing, which THROWS
/// on a non-ok response — it falls back to the curated static ids on a non-ok
/// status, an empty list, OR a thrown fetch, a deliberate asymmetry in v4's own
/// plugin. That fallback-not-throw shape is what this arm owes when the live
/// verb lands; until then the catalogue served by `list-models` is exactly what
/// v4's fallback would return, so a refused fetch costs the operator nothing but
/// freshness.
pub fn embedding_profile_fetch_models() -> Response {
    not_available("fetch-models")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_are_the_embedding_set_plus_builtin() {
        let ids = embedding_provider_ids();
        // The manifest-declared embedding providers + BUILTIN (appended last).
        assert!(ids.contains(&"OPENAI"));
        assert!(ids.contains(&"OPENROUTER"));
        assert!(ids.contains(&"OLLAMA"));
        // P4.D101 — NANOGPT declares `embeddings`, so it joins off the manifest
        // with no code change here; BUILTIN stays last (appended).
        assert!(ids.contains(&"NANOGPT"));
        assert_eq!(*ids.last().unwrap(), "BUILTIN");
        // No non-embedding provider leaks in.
        assert!(!ids.contains(&"ANTHROPIC"));
        assert!(!ids.contains(&"GROK"));
    }

    #[test]
    fn model_catalog_shapes_match_v4() {
        // OPENAI: three models, each with dimensions.
        let openai = embedding_models_for("OPENAI");
        assert_eq!(openai.len(), 3);
        assert_eq!(openai[0]["id"], json!("text-embedding-3-small"));
        assert_eq!(openai[0]["dimensions"], json!(1536));
        // OLLAMA: four models.
        assert_eq!(embedding_models_for("OLLAMA").len(), 4);
        // OPENROUTER: seven models.
        assert_eq!(embedding_models_for("OPENROUTER").len(), 7);
        // BUILTIN: one model, NO dimensions key.
        let builtin = embedding_models_for("BUILTIN");
        assert_eq!(builtin.len(), 1);
        assert_eq!(builtin[0]["id"], json!("tfidf-bm25-v1"));
        assert!(builtin[0].get("dimensions").is_none());
        // NANOGPT: seven models, every one carrying dimensions. The BYTES of
        // all five catalogues are pinned against v4's real
        // `plugin.getEmbeddingModels()` by `embedding_wire_equivalence`'s
        // `catalogue` rows (P4.D101); this stays a cheap shape guard.
        let nanogpt = embedding_models_for("NANOGPT");
        assert_eq!(nanogpt.len(), 7);
        assert!(nanogpt.iter().all(|m| m.get("dimensions").is_some()));
        // Unknown provider → empty.
        assert!(embedding_models_for("NOPE").is_empty());
    }
}
