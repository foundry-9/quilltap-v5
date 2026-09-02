//! The image-profiles dispatch handlers (P4.6p) — a differential port of v4's
//! `image-profiles/route.ts` (+ `[id]/route.ts`), composed over the ported
//! `db::image_profiles` repo (create/update/delete + the new user-scoped reads,
//! `unset_all_defaults`, and the nullable-clearing `IpUpdate` tri-state), the
//! api-key + tag enrichment reads, and the manifest [`Registry`] (list-providers).
//!
//! `generate` was un-refused in P4.6ai and `list-models` in P4.D100 (v4
//! `ca22ec45` — the honest Fetch Models action, over the injected
//! [`ErasedImageDiscovery`] seam). `validate-key` remains a loud refusal arm.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{api_keys, image_profiles as ip, provider_models, tags};
use crate::image_gen::huggingface_lookup::ErasedLoraMetadata;
use crate::image_gen::lora_support::resolve_lora_support;
use crate::image_gen::lora_validation::{
    lora_issue_details, lora_issue_log_lines, validate_profile_loras,
};
use crate::image_gen_data::image_declarations_for;
use crate::model::image::ErasedImageDiscovery;
use crate::model::image_dialects::supported_image_models;
use crate::model::nanogpt_catalog::image_provider_options_schema;
use crate::provider_manifest::Registry;
use crate::tools::generate_image::{
    self, ErasedImageGeneration, ImageGenerationToolInput, ImageToolExecutionContext,
};

// P4.56: the three profile-update handlers share ONE reader for the JS
// semantics of `apiKeyId` / `baseUrl`; only the consequences differ per site.
use super::settings::{classify_api_key_id, classify_base_url, ApiKeyIdPatch, BaseUrlPatch};
use super::types::{ErrorKind, Response};

// ===========================================================================
// Response helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn conflict(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Conflict, msg)
}
/// v4 `serverError('Failed to fetch models')` — the list-models outer catch,
/// which swallows every unexpected failure into one fixed sentence.
fn server_error() -> Response {
    Response::error(ErrorKind::Internal, "Failed to fetch models")
}
fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' image-profile action is recognized but not yet available."),
    )
}

/// v4's `createImageProvider` probe collapsed to a no-IO registry check: map the
/// legacy `GOOGLE_IMAGEN` alias (uppercase compare) to `GOOGLE`, then require an
/// EXACT-CASE provider that declares image generation (all such built-ins carry a
/// `createImageProvider` impl). Unknown / non-image / wrong-case → not available.
fn create_image_provider_ok(provider: &str) -> bool {
    Registry::built_in()
        .get_provider(resolve_image_provider_id(provider))
        .map(|m| m.capabilities.image_generation)
        .unwrap_or(false)
}

/// v4 `createImageProvider`'s legacy-name map (`provider.toUpperCase() ===
/// 'GOOGLE_IMAGEN' ? 'GOOGLE' : provider`). The RESOLVED id names the plugin
/// whose `supportedModels` and discovery run; the RAW id is what the response
/// echoes and what the model cache is keyed on.
fn resolve_image_provider_id(provider: &str) -> &str {
    if provider.to_uppercase() == "GOOGLE_IMAGEN" {
        "GOOGLE"
    } else {
        provider
    }
}

// ===========================================================================
// Enrichment (v4 `enrichWithApiKey` / `enrichWithTags`)
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
/// no matching tag (`tag` is the full marshaled Tag entity). Empty input → `[]`.
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

/// Attach `apiKey` + enriched `tags` to a marshaled profile (v4's list/get spread).
fn enrich_profile(conn: &Connection, mut profile: Value) -> Result<Value, crate::db::DbError> {
    let api_key = enrich_api_key(conn, profile.get("apiKeyId").and_then(Value::as_str))?;
    let tags_enriched = enrich_tags(conn, &tag_ids_of(&profile))?;
    let obj = profile.as_object_mut().unwrap();
    obj.insert("apiKey".into(), api_key);
    obj.insert("tags".into(), tags_enriched);
    Ok(profile)
}

/// The character's tag-id set for `?sortByCharacter=` (v4 `character?.tags || []`).
fn character_tag_ids(
    conn: &Connection,
    char_id: &str,
) -> Result<HashSet<String>, crate::db::DbError> {
    let raw: Option<Option<String>> = conn
        .query_row(
            "SELECT tags FROM characters WHERE id = ?1",
            rusqlite::params![char_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?;
    let set = raw
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default();
    Ok(set)
}

// ===========================================================================
// Handlers
// ===========================================================================

/// v4 `GET /api/v1/image-profiles` (+ `?sortByCharacter=`) → `{profiles, count}`.
pub fn image_profile_list(db: &Db, user_id: &str, sort_by_character: Option<String>) -> Response {
    let result = db.read_main(|conn| {
        let profiles = ip::find_by_user_id(conn, user_id)?;
        let mut enriched: Vec<Value> = Vec::with_capacity(profiles.len());
        for p in profiles {
            enriched.push(enrich_profile(conn, p)?);
        }
        // Default-first, then createdAt DESC.
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

        if let Some(char_id) = &sort_by_character {
            let char_tags = character_tag_ids(conn, char_id)?;
            let matching_count = |p: &Value| -> usize {
                p.get("tags")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter(|t| {
                                t.get("tagId")
                                    .and_then(Value::as_str)
                                    .map(|id| char_tags.contains(id))
                                    .unwrap_or(false)
                            })
                            .count()
                    })
                    .unwrap_or(0)
            };
            enriched.sort_by(|a, b| {
                let am = matching_count(a);
                let bm = matching_count(b);
                if am == bm {
                    // v4: b.isDefault ? 1 : a.isDefault ? -1 : 0.
                    if b.get("isDefault").and_then(Value::as_bool) == Some(true) {
                        std::cmp::Ordering::Greater
                    } else if a.get("isDefault").and_then(Value::as_bool) == Some(true) {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                } else {
                    bm.cmp(&am)
                }
            });
            let with_matches: Vec<Value> = enriched
                .into_iter()
                .map(|p| {
                    let matching: Vec<Value> = p
                        .get("tags")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter(|t| {
                                    t.get("tagId")
                                        .and_then(Value::as_str)
                                        .map(|id| char_tags.contains(id))
                                        .unwrap_or(false)
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    let matching_tags: Vec<Value> = matching
                        .iter()
                        .filter_map(|t| t.get("tag").cloned())
                        .collect();
                    let count = matching.len();
                    let mut obj = p.as_object().cloned().unwrap_or_default();
                    obj.insert("matchingTags".into(), Value::Array(matching_tags));
                    obj.insert("matchingTagCount".into(), json!(count));
                    Value::Object(obj)
                })
                .collect();
            let count = with_matches.len();
            return Ok(json!({ "profiles": with_matches, "count": count }));
        }

        let count = enriched.len();
        Ok(json!({ "profiles": enriched, "count": count }))
    });
    match result {
        Ok(v) => Response::ImageProfile(v),
        Err(e) => internal(e),
    }
}

/// v4 `GET /api/v1/image-profiles/[id]` → the bare profile + `apiKey` + enriched
/// tags; 404 on absent.
pub fn image_profile_get(db: &Db, profile_id: &str) -> Response {
    let result = db.read_main(|conn| match ip::find_by_id(conn, profile_id)? {
        Some(p) => Ok(Some(enrich_profile(conn, p)?)),
        None => Ok(None),
    });
    match result {
        Ok(Some(v)) => Response::ImageProfile(v),
        Ok(None) => not_found("Image profile"),
        Err(e) => internal(e),
    }
}

/// v4 `POST /api/v1/image-profiles` — hand validation (exact messages), provider
/// probe, apiKeyId existence, duplicate-name 409, isDefault→unset-others, then the
/// 201 created profile + `apiKey` (tags stay the raw empty array).
pub async fn image_profile_create(db: &Db, user_id: &str, body: Value) -> Response {
    let name = match body.get("name").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return bad_request("Name is required"),
    };
    let provider = match body.get("provider").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        _ => return bad_request("Provider is required"),
    };
    if !create_image_provider_ok(&provider) {
        return bad_request(format!("Provider {provider} is not available"));
    }
    let model_name = match body.get("modelName").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return bad_request("Model name is required"),
    };
    // parameters: default {}; must be an object (reject arrays).
    let parameters = match body.get("parameters") {
        None | Some(Value::Null) => json!({}),
        Some(Value::Object(_)) => body.get("parameters").cloned().unwrap(),
        Some(_) => return bad_request("Parameters must be an object"),
    };
    // `84f33ce94`: validate the reserved `loras` key before anything is written
    // — a malformed adapter list must not save cleanly and fail at generation.
    // The guard sits HERE, after the parameters-object check (whose refusal it
    // must not steal) and BEFORE the apiKeyId lookup (whose 404 it outranks).
    if let Some(issues) = validate_profile_loras(&parameters) {
        tracing::warn!(
            provider = %provider,
            issues = ?lora_issue_log_lines(&issues),
            "[Image Profiles v1] Rejected a profile with a malformed LoRA list"
        );
        return Response::validation_error(lora_issue_details(&issues));
    }
    let api_key_id = body
        .get("apiKeyId")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(id) = &api_key_id {
        match db.read_main(|conn| api_keys::find_by_id(conn, id)) {
            Ok(Some(_)) => {}
            Ok(None) => return not_found("API key"),
            Err(e) => return internal(e),
        }
    }
    match db.read_main(|conn| ip::find_id_by_user_and_name(conn, user_id, &name)) {
        Ok(Some(_)) => return conflict("An image profile with this name already exists"),
        Ok(None) => {}
        Err(e) => return internal(e),
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
    let is_dangerous_compatible = body
        .get("isDangerousCompatible")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let create = ip::IpCreate {
        user_id: user_id.to_string(),
        name: name.clone(),
        provider: provider.clone(),
        api_key_id: api_key_id.clone(),
        base_url: base_url.clone(),
        model_name: model_name.clone(),
        parameters: parameters.clone(),
        is_default,
        is_dangerous_compatible,
        tags: Vec::new(),
    };
    let opts = ip::CreateOptions {
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
                ip::ImageProfilesRepository::new(conn).unset_all_defaults(&uid, &unset_now)?;
            }
            ip::ImageProfilesRepository::new(conn).create(&create, &opts)
        })
        .await;
    if let Err(e) = write {
        return internal(e);
    }

    // The 201 body = v4's `{...validate(entityInput), apiKey}` — the in-memory
    // created object (apiKeyId/baseUrl present as null-or-value, tags raw []).
    let api_key = match db.read_main(|conn| enrich_api_key(conn, api_key_id.as_deref())) {
        Ok(v) => v,
        Err(e) => return internal(e),
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
    obj.insert("parameters".into(), parameters);
    obj.insert("isDefault".into(), Value::Bool(is_default));
    obj.insert(
        "isDangerousCompatible".into(),
        Value::Bool(is_dangerous_compatible),
    );
    obj.insert("tags".into(), Value::Array(Vec::new()));
    obj.insert("createdAt".into(), Value::String(now.clone()));
    obj.insert("updatedAt".into(), Value::String(now));
    obj.insert("apiKey".into(), api_key);
    Response::ImageProfile(Value::Object(obj))
}

/// v4 `PUT /api/v1/image-profiles/[id]` — 404 / per-field `!== undefined` gating
/// (apiKeyId tri-state clear; `''`→null baseUrl; isDefault→unset-others; tags NOT
/// updatable) → the updated profile (in-memory merge) + enrichment.
pub async fn image_profile_update(
    db: &Db,
    user_id: &str,
    profile_id: &str,
    body: Value,
) -> Response {
    let existing = match db.read_main(|conn| ip::find_by_id(conn, profile_id)) {
        Ok(Some(p)) => p,
        Ok(None) => return not_found("Image profile"),
        Err(e) => return internal(e),
    };
    let Some(body_obj) = body.as_object() else {
        return bad_request("Expected object");
    };

    let mut merged = existing.clone();
    let mo = merged.as_object_mut().unwrap();
    let mut patch = ip::IpUpdate::default();

    if let Some(name_v) = body_obj.get("name") {
        let name = match name_v.as_str() {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return bad_request("Name must be a non-empty string"),
        };
        match db.read_main(|conn| ip::find_id_by_user_and_name(conn, user_id, &name)) {
            Ok(Some(dup)) if dup != profile_id => {
                return conflict("An image profile with this name already exists")
            }
            Ok(_) => {}
            Err(e) => return internal(e),
        }
        mo.insert("name".into(), Value::String(name.clone()));
        patch.name = Some(name);
    }
    if let Some(provider_v) = body_obj.get("provider") {
        let provider = match provider_v.as_str() {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => return bad_request("Provider must be a non-empty string"),
        };
        if !create_image_provider_ok(&provider) {
            return bad_request(format!("Provider {provider} is not available"));
        }
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
                    Err(e) => return internal(e),
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
            BaseUrlPatch::Refuse => return internal("Failed to update image profile"),
        }
    }
    if let Some(mn) = body_obj.get("modelName") {
        let model = match mn.as_str() {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return bad_request("Model name must be a non-empty string"),
        };
        mo.insert("modelName".into(), Value::String(model.clone()));
        patch.model_name = Some(model);
    }
    if let Some(p) = body_obj.get("parameters") {
        if !p.is_object() {
            return bad_request("Parameters must be an object");
        }
        // `84f33ce94`, inside the `parameters !== undefined` arm — same rule,
        // same placement, a different warn sentence.
        if let Some(issues) = validate_profile_loras(p) {
            tracing::warn!(
                profile_id = %profile_id,
                issues = ?lora_issue_log_lines(&issues),
                "[Image Profiles v1] Rejected an update with a malformed LoRA list"
            );
            return Response::validation_error(lora_issue_details(&issues));
        }
        mo.insert("parameters".into(), p.clone());
        patch.parameters = Some(p.clone());
    }
    let mut set_default = false;
    if let Some(d) = body_obj.get("isDefault") {
        let b = match d.as_bool() {
            Some(b) => b,
            None => return bad_request("isDefault must be a boolean"),
        };
        set_default = b;
        mo.insert("isDefault".into(), Value::Bool(b));
        patch.is_default = Some(b);
    }
    if let Some(d) = body_obj.get("isDangerousCompatible") {
        let b = match d.as_bool() {
            Some(b) => b,
            None => return bad_request("isDangerousCompatible must be a boolean"),
        };
        mo.insert("isDangerousCompatible".into(), Value::Bool(b));
        patch.is_dangerous_compatible = Some(b);
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
                ip::ImageProfilesRepository::new(conn).unset_all_defaults(&uid, &unset_now)?;
            }
            ip::ImageProfilesRepository::new(conn).update(&id, &patch)
        })
        .await;
    if let Err(e) = write {
        return internal(e);
    }

    // The echo = v4's `{...validate(merged), ...enriched}` — enrich the in-memory
    // merged (nulls present when explicitly set).
    let enriched = match db.read_main(|conn| {
        let api_key = enrich_api_key(conn, merged.get("apiKeyId").and_then(Value::as_str))?;
        let tags_enriched = enrich_tags(conn, &tag_ids_of(&merged))?;
        Ok::<_, crate::db::DbError>((api_key, tags_enriched))
    }) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let mo = merged.as_object_mut().unwrap();
    mo.insert("apiKey".into(), enriched.0);
    mo.insert("tags".into(), enriched.1);
    Response::ImageProfile(merged)
}

/// v4 `DELETE /api/v1/image-profiles/[id]` — 404 / delete / `{message}`.
pub async fn image_profile_delete(db: &Db, profile_id: &str) -> Response {
    let existing = db.read_main(|conn| ip::find_by_id(conn, profile_id));
    match existing {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Image profile"),
        Err(e) => return internal(e),
    }
    let id = profile_id.to_string();
    let out = db
        .write(move |ws| ip::ImageProfilesRepository::new(ws.main().connection()).delete(&id))
        .await;
    match out {
        Ok(_) => Response::ImageProfile(json!({ "message": "Image profile deleted successfully" })),
        Err(e) => internal(e),
    }
}

/// v4 `GET /api/v1/image-profiles?action=list-providers` — the manifest registry
/// filtered on image generation → `{providers, count}` (registry-only, no IO).
pub fn image_provider_list() -> Response {
    let providers: Vec<Value> = Registry::built_in()
        .all_providers()
        .iter()
        .filter(|m| m.capabilities.image_generation)
        .map(|m| {
            json!({
                "value": m.id,
                "label": m.display_name,
                "defaultModels": m.image_generation_models,
                "apiKeyProvider": m.id,
                "legacyNames": m.legacy_names,
            })
        })
        .collect();
    let count = providers.len();
    Response::ImageProfile(json!({ "providers": providers, "count": count }))
}

// ===========================================================================
// generate (P4.6ai — the un-refusal over the injected W4.9a runner)
// ===========================================================================

/// v4 `POST /api/v1/image-profiles/[id]?action=generate` — the profile-404 gate,
/// then `executeImageGenerationTool` (the injected [`ErasedImageGeneration`] runner,
/// wired LIVE in the host from the W4.7f `Real*Provider`s), then the
/// `successResponse({success, data, expandedPrompt, metadata}, 201)` envelope or the
/// `badRequest(result.error || 'Image generation failed')` arm. Spine-less
/// assemblies never reach here (the engine's `image_generation` seam gate keeps the
/// loud not-assembled refusal). The dispatch variant carries only the Shared-contract
/// four (`prompt/chatId/count` + the profile id); v4's `size/quality/style/
/// aspectRatio/negativePrompt` extras are the KNOWN narrowing divergence and stay
/// `None`.
pub async fn image_profile_generate(
    db: &Db,
    runner: &ErasedImageGeneration,
    user_id: &str,
    profile_id: &str,
    prompt: &str,
    chat_id: Option<&str>,
    count: Option<i64>,
) -> Response {
    // v4: `notFound('Image profile')` before validating the body / running the tool.
    match db.read_main(|conn| ip::find_by_id(conn, profile_id)) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Image profile"),
        Err(e) => return internal(e),
    }

    // v4's `generateImageSchema.count` prefaults to 1 (BEFORE the profile's
    // `parameters.n`), so a missing count pins n=1 exactly as v4 does.
    let input = ImageGenerationToolInput {
        prompt: prompt.to_string(),
        negative_prompt: None,
        orientation: None,
        size: None,
        style: None,
        quality: None,
        aspect_ratio: None,
        count: Some(count.unwrap_or(1)),
    };
    let ctx = ImageToolExecutionContext {
        user_id: user_id.to_string(),
        profile_id: profile_id.to_string(),
        chat_id: chat_id.map(str::to_string),
        // v4's route passes no callingParticipantId (undefined).
        calling_participant_id: None,
    };

    let out = runner.run(db, &input, &ctx).await;

    if !out.success {
        // v4: `badRequest(result.error || 'Image generation failed')`.
        let msg = out
            .error
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Image generation failed".to_string());
        return bad_request(msg);
    }

    // v4 `successResponse({ success: true, data: result.images, expandedPrompt,
    // metadata: { originalPrompt, provider, model, count } }, 201)`. The whole
    // dispatch family answers 200 at the web edge (create is 201 in v4 too); the SPA
    // reads the body. `metadata.count` is the RETURNED image count, not the request's.
    let data: Vec<Value> = out
        .images
        .iter()
        .map(generate_image::generated_image_to_value)
        .collect();
    let mut metadata = Map::new();
    metadata.insert("originalPrompt".into(), Value::String(prompt.to_string()));
    if let Some(p) = &out.provider {
        metadata.insert("provider".into(), Value::String(p.clone()));
    }
    if let Some(m) = &out.model {
        metadata.insert("model".into(), Value::String(m.clone()));
    }
    metadata.insert("count".into(), json!(out.images.len()));

    let mut body = Map::new();
    body.insert("success".into(), Value::Bool(true));
    body.insert("data".into(), Value::Array(data));
    // `expandedPrompt: result.expandedPrompt` — a JS `undefined` drops (always Some
    // on the success path).
    if let Some(ep) = &out.expanded_prompt {
        body.insert("expandedPrompt".into(), Value::String(ep.clone()));
    }
    body.insert("metadata".into(), Value::Object(metadata));
    Response::ImageProfile(Value::Object(body))
}

// ===========================================================================
// The one remaining refusal arm (validate-key)
// ===========================================================================
//
// `validate-key` remains a loud typed refusal: v4's handler was NOT touched by
// the `ca22ec45` drift, and porting it needs the live per-provider key probes.
//
// The P4.D33 bank note that stood here is RETIRED. It warned that
// `@openrouter/sdk` 0.13 turned `models.list()` into a paginated async-iterable
// whose rows live at `page.result.data`, and that v4's image / embedding call
// sites still read the pre-0.13 `response.data`. The image half is now ported,
// and the measurement went further than the note: at `d5830439` v4's OpenRouter
// image discovery finds nothing under ANY payload, because the SDK's zod strips
// the keys its acceptance arms read. See `image_dialects::openrouter_sdk_project`
// for the reproduction and the convergence tripwire. The note still stands for
// the `p4.9h` embedding-profiles management surface.

pub fn image_profile_validate_key() -> Response {
    not_available("validate-key")
}
/// v4 `GET /api/v1/image-profiles?action=list-models` — the honest Fetch Models
/// action (`ca22ec45`).
///
/// `source` is honest: `provider` ONLY when the provider's API was actually
/// queried and answered; otherwise `builtin` (the plugin's curated list), with
/// the live-fetch error surfaced as `fetchError`. The flow is v4's exactly —
/// missing provider → 400, unknown provider → 400, dangling `apiKeyId` → 404,
/// and any other failure collapses into the outer catch's
/// `serverError('Failed to fetch models')`.
///
/// **Cache only genuinely live-fetched lists.** A built-in list written to
/// `provider_models` would masquerade as provider-confirmed on later reads. The
/// write is best-effort: v4 catches and warns, never failing the request.
pub async fn image_profile_list_models(
    db: &Db,
    discovery: &ErasedImageDiscovery,
    provider: Option<&str>,
    api_key_id: Option<&str>,
) -> Response {
    // v4 `searchParams.get('provider')` then `if (!provider)` — a JS falsy test,
    // so an EMPTY `provider=` is "required", not "not available".
    let Some(provider) = provider.filter(|p| !p.is_empty()) else {
        return bad_request("Provider is required");
    };
    // v4 `createImageProvider(provider)`; a throw is the 400 below.
    if !create_image_provider_ok(provider) {
        return bad_request(format!("Provider {provider} is not available"));
    }
    let resolved = resolve_image_provider_id(provider);
    let supported: Vec<String> = match supported_image_models(resolved) {
        Ok(list) => list.iter().map(|s| (*s).to_string()).collect(),
        // Unreachable behind the probe above (every image-capable built-in has a
        // curated list); v4's outer catch is the honest answer if it ever is.
        Err(_) => return server_error(),
    };

    let mut models = supported.clone();
    let mut source = "builtin";
    let mut fetch_error: Option<String> = None;

    // v4 `if (apiKeyId)` — falsy (absent or empty) takes the built-in path.
    if let Some(api_key_id) = api_key_id.filter(|s| !s.is_empty()) {
        let key = match db.read_main(|conn| api_keys::find_by_id(conn, api_key_id)) {
            Ok(k) => k,
            Err(_) => return server_error(),
        };
        let Some(key) = key else {
            return not_found("API key");
        };
        match discovery
            .available_models(resolved, Some(&key.key_value))
            .await
        {
            Ok(m) => {
                models = m;
                source = "provider";
            }
            Err(e) => {
                // v4 `error instanceof Error ? error.message : String(error)`.
                fetch_error = Some(e.message);
                models = supported.clone();
            }
        }
    }

    if source == "provider" {
        cache_image_models(db, provider, &models).await;
    }

    let mut body = Map::new();
    body.insert("provider".into(), Value::String(provider.to_string()));
    let model_ids = models;
    body.insert(
        "models".into(),
        Value::Array(model_ids.iter().cloned().map(Value::String).collect()),
    );
    body.insert(
        "supportedModels".into(),
        Value::Array(supported.into_iter().map(Value::String).collect()),
    );
    body.insert("source".into(), Value::String(source.to_string()));
    // `84f33ce94`: resolve LoRA support HERE rather than in the browser — the
    // resolution order (exact id → longest-prefix family → provider constraint)
    // lives in one host module, and the declarations it reads are server-side
    // only. Models that resolve nothing are simply ABSENT from the map, which
    // is the editor's signal to offer no LoRA rows at all.
    //
    // ⚠ The RAW provider string, not `resolved`: v4 calls
    // `resolveLoraSupport(provider, modelId)` with what the query string said,
    // so a `GOOGLE_IMAGEN` request resolves against `GOOGLE_IMAGEN` (which
    // declares nothing) even though its MODELS came from `GOOGLE`. Pinned by
    // the `list_models_legacy_alias` arm.
    let declarations = image_declarations_for(provider);
    let mut lora_support = Map::new();
    for model_id in &model_ids {
        if let Some(support) = resolve_lora_support(
            Some(&declarations.models),
            Some(model_id),
            declarations.lora_provider.as_ref(),
        ) {
            lora_support.insert(
                model_id.clone(),
                serde_json::to_value(&support).unwrap_or(Value::Null),
            );
        }
    }
    tracing::debug!(
        provider = %provider,
        model_count = model_ids.len(),
        lora_capable_count = lora_support.len(),
        "[Image Profiles v1] Resolved LoRA support for the model list"
    );
    body.insert("loraSupport".into(), Value::Object(lora_support));
    // `...(fetchError ? { fetchError } : {})` — OMITTED, never null.
    if let Some(e) = fetch_error {
        body.insert("fetchError".into(), Value::String(e));
    }
    Response::ImageProfile(Value::Object(body))
}

/// v4 `handleOptionsSchema` (`84f33ce94`) — the per-provider (and per-model)
/// image options schema plus that model's LoRA support.
///
/// Two things differ from `list-models` and both are v4's, not v5's:
///   - the provider gate is the plain registry lookup
///     (`providerRegistry.getProvider`), NOT `createImageProvider` — so a
///     text-only provider is "available" here and answers a null schema, where
///     `list-models` would refuse it;
///   - the legacy `GOOGLE_IMAGEN` alias is NOT resolved, because nothing here
///     asks a plugin for models.
///
/// `optionsSchema` and `loraSupport` are `null` — never a zero-cap object —
/// when nothing is declared: the editor reads null as "fall back to the legacy
/// hand-written panel" and "offer no LoRA rows at all".
pub fn image_profile_options_schema(provider: Option<&str>, model: Option<&str>) -> Response {
    // v4 `searchParams.get('provider')` then `if (!provider)` — a JS falsy
    // test, so an EMPTY `provider=` is "required", not "not available".
    let Some(provider) = provider.filter(|p| !p.is_empty()) else {
        return bad_request("Provider is required");
    };
    if Registry::built_in().get_provider(provider).is_none() {
        return bad_request(format!("Provider {provider} is not available"));
    }
    // v4 `searchParams.get('model') ?? undefined` — `??` catches only null, so
    // an EMPTY `model=` stays the empty string and echoes back as `""`.
    let options_schema = image_provider_options_schema(provider, model);
    let declarations = image_declarations_for(provider);
    let support = resolve_lora_support(
        Some(&declarations.models),
        model,
        declarations.lora_provider.as_ref(),
    );

    // Hoisted out of the macro: a `tracing` field expression cannot name
    // `Value::as_array` — inside the macro `Value` resolves to the `tracing`
    // trait of that name (the standing note).
    let group_count = options_schema
        .as_ref()
        .and_then(|s| s.get("groups"))
        .and_then(|g| g.as_array())
        .map(Vec::len)
        .unwrap_or(0);
    tracing::debug!(
        provider = %provider,
        model = ?model,
        has_schema = options_schema.is_some(),
        group_count,
        lora_support = ?support.as_ref().map(|s| s.max_loras),
        "[Image Profiles v1] Served image options schema"
    );

    let mut body = Map::new();
    body.insert("provider".into(), Value::String(provider.to_string()));
    // `model: model ?? null`.
    body.insert(
        "model".into(),
        model.map_or(Value::Null, |m| Value::String(m.to_string())),
    );
    body.insert(
        "optionsSchema".into(),
        options_schema.unwrap_or(Value::Null),
    );
    body.insert(
        "loraSupport".into(),
        support
            .as_ref()
            .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
            .unwrap_or(Value::Null),
    );
    Response::ImageProfile(Value::Object(body))
}

/// v4 `handleLoraMetadata` (`2ece98c90`) — asks HuggingFace what it knows about
/// a LoRA source and hands the answer back verbatim.
///
/// It renders **no compatibility verdict** — see
/// [`crate::image_gen::huggingface_lookup`] for why guessing at one would be
/// worse than silence — so this is a read-out the user interprets, not a gate.
///
/// POST rather than GET for one reason: the optional `hfToken` is a credential,
/// and a credential does not belong in a query string where it would land in
/// every access log between here and the browser.
///
/// The body arrives RAW because all four guards are type checks and v5's
/// wrong-type-collapse rule forbids narrowing them at the edge. Two of the four
/// are unreachable through v5's only entrance — the dispatch verb, whose body
/// is an object by construction, so a non-JSON or non-object body cannot occur
/// (there is no REST edge for image-profiles). They are ported anyway and the
/// differential drives this function directly, so all four are measured.
pub async fn image_profile_lora_metadata(lookup: &ErasedLoraMetadata, body: &Value) -> Response {
    // v4's `catch` on `req.json()` AND its `typeof body !== 'object' || null ||
    // Array.isArray` guard answer the SAME sentence.
    if !body.is_object() {
        return bad_request("A JSON body with a `source` is required");
    }
    let source = body.get("source");
    // `typeof source !== 'string' || source.trim().length === 0`.
    let Some(source) = source
        .and_then(Value::as_str)
        .filter(|s| !crate::jsstr::js_trim(s).is_empty())
    else {
        return bad_request("A LoRA source is required");
    };
    // `hfToken !== undefined && typeof hfToken !== 'string'` — an explicit
    // `null` is NOT undefined, so it refuses; absent passes.
    let hf_token = match body.get("hfToken") {
        None => None,
        Some(v) => match v.as_str() {
            Some(s) => Some(s),
            None => return bad_request("hfToken must be a string when supplied"),
        },
    };

    let result = lookup
        .lookup(
            source,
            // `typeof hfToken === 'string' && hfToken.length > 0 ? hfToken :
            // undefined` — an EMPTY token is passed as absent.
            hf_token.filter(|t| !t.is_empty()),
        )
        .await;

    // Failures answer 200 with `ok: false`: "HuggingFace would not tell us" is a
    // result the editor displays, not an error the form should treat as a broken
    // request.
    Response::ImageProfile(serde_json::to_value(&result).unwrap_or(Value::Null))
}

/// v4's `providerModels.upsertModelsForProvider(provider, models.map(modelId =>
/// ({modelId, displayName: modelId})), 'image', undefined)` — keyed on the RAW
/// provider string, `displayName` equal to the id, no base URL. Best-effort: a
/// failure is warned and swallowed (v4's `try/catch → logger.warn`), so a
/// cache-layer problem can never turn a successful fetch into an error page.
async fn cache_image_models(db: &Db, provider: &str, models: &[String]) {
    let rows: Vec<provider_models::PmCreate> = models
        .iter()
        .map(|id| provider_models::PmCreate {
            provider: provider.to_string(),
            model_id: id.clone(),
            model_type: "image".to_string(),
            display_name: id.clone(),
            base_url: None,
            context_window: None,
            max_output_tokens: None,
            deprecated: false,
            experimental: false,
        })
        .collect();
    let write = db
        .write(move |ws| {
            provider_models::ProviderModelsRepository::new(ws.main().connection())
                .upsert_model_for_provider(&rows)
        })
        .await;
    if let Err(e) = write {
        tracing::warn!(provider = %provider, error = %e, "Failed to cache image models");
    }
}

#[cfg(test)]
mod tests {
    use super::create_image_provider_ok;

    // The provider probe's ACCEPT path is proven end-to-end by
    // `create_happy` in the differential; the REJECT path can't be oracle-tested
    // (the v4 registry probe is a no-op in the jest sandbox — the plugin
    // registration doesn't reach `requireProvider`, so it accepts every provider
    // there, though standalone Node and production BOTH reject). This unit test
    // pins the production-correct behavior directly against the manifest registry.
    #[test]
    fn probe_matches_v4_production() {
        // Image-capable providers accepted (exact-case).
        // P4.D101 — NANOGPT joins off the manifest's `capabilities
        // .imageGeneration`, with no code change at this probe.
        for p in ["OPENAI", "GOOGLE", "GROK", "Z_AI", "OPENROUTER", "NANOGPT"] {
            assert!(create_image_provider_ok(p), "{p} should be available");
        }
        // Legacy alias resolves (v4 GOOGLE_IMAGEN → GOOGLE, uppercase compare).
        assert!(create_image_provider_ok("GOOGLE_IMAGEN"));
        assert!(create_image_provider_ok("google_imagen"));
        // Unknown, non-image, and wrong-case are rejected.
        for p in ["NOTAPROVIDER", "OPENAI_COMPATIBLE", "ANTHROPIC", "openai"] {
            assert!(!create_image_provider_ok(p), "{p} should be unavailable");
        }
    }
}
