//! The Characters-server dispatch handlers (P4.6f) — the route-logic backfill
//! the Characters SPA vertical consumes, composed over already-ported
//! repos/services.
//!
//! Each handler is a differential port of a v4 route handler (the oracle:
//! `characters/handlers/*`, `characters/[id]/handlers/*`, the sub-resource
//! `route.ts` files, and `tags/`) and returns a [`Response`] directly (the engine
//! arm is a one-line delegate). Reads nest [`Db::read_main`] +
//! [`Db::read_mount_index`] (separate pools → no deadlock) for the vault overlay;
//! writes go through [`Db::write`].
//!
//! `user_id` is a parameter (not hard-coded `SINGLE_USER_ID`) so the differential
//! harness can drive with the fixture's own user id on both sides; the engine
//! passes `SINGLE_USER_ID`. Ownership (v4 `checkOwnership`) collapses to
//! NotFound-on-absent for the single-user v5.

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{
    character_plugin_data, characters_read, doc_mount_documents::DocMountDocumentsRepository, tags,
    vault_character_arrays, vault_character_update, wardrobe_read, DbError,
};
use crate::photos::resolve_character_avatar::resolve_character_avatar;
use crate::services::character_enrichment;
use crate::services::image_job_common::with_both_conns;

use super::types::{ErrorKind, Response};

// ===========================================================================
// Shared helpers
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

/// The loud "recognized but not yet available" refusal (the BuiltInToolRunner
/// precedent) for characters-family variants whose handler is a documented
/// deferral (Tier 3) or lands in a later P4.6f milestone.
pub fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' characters action is recognized but not yet available."),
    )
}

/// Run `f` with BOTH a main and a mount-index connection (the vault overlay needs
/// both). Errors [`DbError::PartitionUnavailable`] if the instance has no
/// mount-index DB (never the case for a provisioned instance).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// v4's ownership gate for the `[id]` reads: `repos.characters.findById(id)`
/// (the OVERLAID read — throws on a broken vault) + `checkOwnership`. Returns the
/// overlaid character, or the NotFound/internal refusal.
fn require_character(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    character_id: &str,
) -> Result<Value, Response> {
    match characters_read::find_by_id(main, mount, character_id) {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(not_found("Character")),
        Err(e) => Err(internal(e)),
    }
}

// ===========================================================================
// List (v4 GET /api/v1/characters — characters/handlers/get.ts)
// ===========================================================================

/// v4 `handleGet`: `findByUserId` → in-memory `npc`/`controlledBy` filter →
/// createdAt-desc sort → per-character whitelist DTO enrichment. `npc` /
/// `controlled_by` mirror v4's query-string values (`"true"`/`"false"`,
/// `"user"`/`"llm"`).
pub fn character_list(
    db: &Db,
    user_id: &str,
    npc: Option<&str>,
    controlled_by: Option<&str>,
) -> Response {
    let user_id = user_id.to_string();
    let npc = npc.map(str::to_string);
    let controlled_by = controlled_by.map(str::to_string);
    let result = read_main_mount(db, move |main, mount| {
        let mut characters = characters_read::find_by_user_id(main, mount, &user_id)?;

        // Filter by NPC status (v4 `c.npc === true` / `!c.npc`).
        match npc.as_deref() {
            Some("true") => {
                characters.retain(|c| c.get("npc").and_then(Value::as_bool) == Some(true))
            }
            Some("false") => {
                characters.retain(|c| c.get("npc").and_then(Value::as_bool) != Some(true))
            }
            _ => {}
        }
        // Filter by controlledBy (v4: 'user' exact; 'llm' includes undefined).
        match controlled_by.as_deref() {
            Some("user") => {
                characters.retain(|c| c.get("controlledBy").and_then(Value::as_str) == Some("user"))
            }
            Some("llm") => characters.retain(|c| {
                let v = c.get("controlledBy").and_then(Value::as_str);
                v == Some("llm") || v.is_none()
            }),
            _ => {}
        }
        // Sort by createdAt descending (string ISO timestamps sort lexically;
        // v4 sorts by `new Date(...).getTime()` — equivalent for ISO-8601).
        characters.sort_by(|a, b| {
            let ta = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
            let tb = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
            tb.cmp(ta)
        });

        let mut enriched = Vec::with_capacity(characters.len());
        for c in &characters {
            enriched.push(character_enrichment::build_list_dto(main, mount, c)?);
        }
        Ok(enriched)
    });

    match result {
        Ok(list) => {
            let count = list.len();
            Response::Characters(json!({ "characters": list, "count": count }))
        }
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Detail + cheap read actions (v4 characters/[id]/handlers/get.ts)
// ===========================================================================

/// v4 default GET branch: ownership → `{...character, defaultImage, _count}`.
pub fn character_get(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let character_id = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let character = match require_character(main, mount, &character_id) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        Ok(Ok(character_enrichment::build_detail(
            main, mount, character,
        )?))
    });
    match result {
        Ok(Ok(body)) => Response::Character(json!({ "character": body })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `?action=default-partner` — `{ partnerId: character.defaultPartnerId || null }`.
pub fn character_default_partner(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let character_id = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let character = match require_character(main, mount, &character_id) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        let partner_id = match character.get("defaultPartnerId") {
            Some(Value::String(s)) if !s.is_empty() => Value::String(s.clone()),
            _ => Value::Null,
        };
        Ok(Ok(partner_id))
    });
    match result {
        Ok(Ok(partner_id)) => Response::Character(json!({ "partnerId": partner_id })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `?action=get-tags` — N+1 `tags.findById` → `{id,name,visualStyle}`,
/// dropping missing, order-preserving.
pub fn character_get_tags(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let character_id = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let character = match require_character(main, mount, &character_id) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        let tag_ids: Vec<String> = character
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let details = tags::find_details_by_ids(main, &tag_ids)?;
        Ok(Ok(details))
    });
    match result {
        Ok(Ok(details)) => Response::Character(json!({ "tags": details })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Sub-resource reads
// ===========================================================================

/// v4 `GET /characters/[id]/prompts` — ownership → `{ prompts }`
/// (`character.systemPrompts || []`).
pub fn character_prompt_list(db: &Db, _user_id: &str, character_id: &str) -> Response {
    sub_array_read(db, character_id, "systemPrompts", "prompts")
}

/// v4 `GET /characters/[id]/scenarios` — ownership → `{ scenarios }`.
pub fn character_scenario_list(db: &Db, _user_id: &str, character_id: &str) -> Response {
    sub_array_read(db, character_id, "scenarios", "scenarios")
}

/// Shared body for the two "ownership → `{ key: character.field || [] }`" reads.
fn sub_array_read(db: &Db, character_id: &str, field: &str, out_key: &str) -> Response {
    let character_id = character_id.to_string();
    let field = field.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let character = match require_character(main, mount, &character_id) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        let arr = match character.get(&field) {
            Some(Value::Array(a)) => Value::Array(a.clone()),
            _ => Value::Array(vec![]),
        };
        Ok(Ok(arr))
    });
    match result {
        Ok(Ok(arr)) => Response::Character(json!({ out_key: arr })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `GET /characters/[id]/wardrobe` — ownership → `{ wardrobeItems }`
/// (`repos.wardrobe.findByCharacterId(id)`, the overlaid vault read).
pub fn character_wardrobe_list(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let character_id = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        if let Err(r) = require_character(main, mount, &character_id) {
            return Ok(Err(r));
        }
        let docs = DocMountDocumentsRepository::new(mount);
        let items = wardrobe_read::find_by_character_id(main, &docs, &character_id, false)?;
        Ok(Ok(items))
    });
    match result {
        Ok(Ok(items)) => Response::Character(json!({ "wardrobeItems": items })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `GET /characters/[id]/plugin-data` — ownership → `{ pluginData: map }`.
pub fn character_plugin_data_map(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let character_id = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        if let Err(r) = require_character(main, mount, &character_id) {
            return Ok(Err(r));
        }
        let map = character_plugin_data::get_plugin_data_map(main, &character_id)?;
        Ok(Ok(map))
    });
    match result {
        Ok(Ok(map)) => Response::Character(json!({ "pluginData": map })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `GET /characters/[id]/plugin-data/[pluginName]` — ownership → `{ pluginData:
/// entry }` or NotFound.
pub fn character_plugin_data_get(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    plugin_name: &str,
) -> Response {
    let character_id = character_id.to_string();
    let plugin_name = plugin_name.to_string();
    let result = read_main_mount(db, move |main, mount| {
        if let Err(r) = require_character(main, mount, &character_id) {
            return Ok(Err(r));
        }
        match character_plugin_data::find_by_character_and_plugin(
            main,
            &character_id,
            &plugin_name,
        )? {
            Some(entry) => Ok(Ok(entry)),
            None => Ok(Err(not_found("Plugin data"))),
        }
    });
    match result {
        Ok(Ok(entry)) => Response::Character(json!({ "pluginData": entry })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

// ===========================================================================
// The thin action verbs (v4 characters/[id]/handlers/post.ts)
// ===========================================================================

/// Overlay a patch onto the pre-update overlaid character (v4 base `_update`:
/// `validate({...existing, ...data, id, createdAt, updatedAt: now})` — it MERGES
/// the patch onto the pre-update read and does NOT re-read, so an explicit `null`
/// in the patch survives in the echo; the P4.6c D4 finding). `id`/`createdAt` are
/// preserved by keeping `existing`'s; `updatedAt` is minted.
fn merge_update_echo(mut pre: Value, patch: &[(&str, Value)]) -> Value {
    if let Some(o) = pre.as_object_mut() {
        for (k, v) in patch {
            o.insert((*k).to_string(), v.clone());
        }
        o.insert("updatedAt".into(), Value::String(crate::clock::now_iso()));
    }
    pre
}

/// v4 `?action=favorite` — flip `isFavorite`; `{ character }` (the merged
/// pre-update character `setFavorite`→`update` returns).
pub async fn character_favorite(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let character = match characters_read::find_by_id(main, mount, &cid)? {
            Some(c) => c,
            None => return Ok(Err(not_found("Character"))),
        };
        let cur = character
            .get("isFavorite")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        vault_character_arrays::set_favorite(main, mount, &cid, !cur)?;
        Ok(Ok(merge_update_echo(
            character,
            &[("isFavorite", json!(!cur))],
        )))
    })
    .await;
    merged_character_response(out)
}

/// v4 `?action=toggle-controlled-by` — `'user'` ⇄ `'llm'`; `{ character }`.
pub async fn character_toggle_controlled_by(
    db: &Db,
    _user_id: &str,
    character_id: &str,
) -> Response {
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let character = match characters_read::find_by_id(main, mount, &cid)? {
            Some(c) => c,
            None => return Ok(Err(not_found("Character"))),
        };
        let next = if character.get("controlledBy").and_then(Value::as_str) == Some("user") {
            "llm"
        } else {
            "user"
        };
        vault_character_arrays::set_controlled_by(main, mount, &cid, next)?;
        Ok(Ok(merge_update_echo(
            character,
            &[("controlledBy", json!(next))],
        )))
    })
    .await;
    merged_character_response(out)
}

/// v4 `?action=toggle-carina` — flip `canBeCarina` (null/undefined → true);
/// `{ character }`.
pub async fn character_toggle_carina(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let character = match characters_read::find_by_id(main, mount, &cid)? {
            Some(c) => c,
            None => return Ok(Err(not_found("Character"))),
        };
        // v4 `!character.canBeCarina`: a null/undefined/false current value → true.
        let next = !character
            .get("canBeCarina")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        vault_character_arrays::set_can_be_carina(main, mount, &cid, next)?;
        Ok(Ok(merge_update_echo(
            character,
            &[("canBeCarina", json!(next))],
        )))
    })
    .await;
    merged_character_response(out)
}

/// Shared tail for the three "flip a slim flag → merged `{ character }`" verbs.
fn merged_character_response(out: Result<Result<Value, Response>, DbError>) -> Response {
    match out {
        Ok(Ok(c)) => Response::Character(json!({ "character": c })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `?action=set-default-partner` — guards (partner exists, is
/// `controlledBy:'user'`, not self) → `update({defaultPartnerId})` →
/// `{ partnerId, success: true }`.
pub async fn character_set_default_partner(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    partner_id: Option<&str>,
) -> Response {
    let cid = character_id.to_string();
    let partner_id = partner_id.map(str::to_string);
    let out = with_both_conns(db, move |main, mount| {
        // Ownership gate (v4 `findById(id)` first).
        if characters_read::find_by_id(main, mount, &cid)?.is_none() {
            return Ok(Err(not_found("Character")));
        }
        if let Some(pid) = partner_id.as_deref() {
            let partner = match characters_read::find_by_id(main, mount, pid)? {
                Some(p) => p,
                None => return Ok(Err(not_found("Partner character"))),
            };
            if partner.get("controlledBy").and_then(Value::as_str) != Some("user") {
                return Ok(Err(bad_request(
                    "Partner must be a user-controlled character",
                )));
            }
            if pid == cid {
                return Ok(Err(bad_request("Character cannot be its own partner")));
            }
        }
        let mut patch = serde_json::Map::new();
        patch.insert(
            "defaultPartnerId".into(),
            match &partner_id {
                Some(p) => Value::String(p.clone()),
                None => Value::Null,
            },
        );
        vault_character_update::update_character(main, mount, &cid, &patch)?;
        Ok(Ok(partner_id))
    })
    .await;
    match out {
        Ok(Ok(partner_id)) => Response::Character(json!({
            "partnerId": partner_id.map(Value::String).unwrap_or(Value::Null),
            "success": true,
        })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `?action=avatar` — `{imageId: string|null}`: validate the image resolves
/// and is `image/*`, then `update({defaultImageId})` + re-enrich →
/// `{ data: {...character, defaultImage} }`.
pub async fn character_avatar(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    image_id: Option<&str>,
) -> Response {
    let cid = character_id.to_string();
    let image_id = image_id.map(str::to_string);
    let out = with_both_conns(db, move |main, mount| {
        // Ownership gate + the pre-update read the merged echo is built from.
        let pre = match characters_read::find_by_id(main, mount, &cid)? {
            Some(c) => c,
            None => return Ok(Err(not_found("Character"))),
        };
        // Validate a provided image (v4: resolve → must exist and be image/*).
        if let Some(iid) = image_id.as_deref() {
            match resolve_character_avatar(main, mount, Some(iid))? {
                None => return Ok(Err(not_found("Image file"))),
                Some(resolved) => {
                    if let Some(mime) = &resolved.mime_type {
                        if !mime.starts_with("image/") {
                            return Ok(Err(bad_request(format!(
                                "Invalid file type. Expected an image, got {mime}"
                            ))));
                        }
                    }
                }
            }
        }
        let image_val = match &image_id {
            Some(i) => Value::String(i.clone()),
            None => Value::Null,
        };
        let mut patch = serde_json::Map::new();
        patch.insert("defaultImageId".into(), image_val.clone());
        vault_character_update::update_character(main, mount, &cid, &patch)?;
        // v4 builds `{...update(id,{defaultImageId}), defaultImage}` — the update
        // return is the patch merged onto the pre-update read (D4), and
        // enrichWithDefaultImage runs off the updated defaultImageId.
        let merged = merge_update_echo(pre, &[("defaultImageId", image_val)]);
        let default_image =
            character_enrichment::enrich_with_default_image(main, mount, image_id.as_deref())?;
        Ok(Ok((merged, default_image)))
    })
    .await;
    match out {
        Ok(Ok((mut character, default_image))) => {
            if let Some(obj) = character.as_object_mut() {
                obj.insert(
                    "defaultImage".into(),
                    match default_image {
                        Some(i) => serde_json::to_value(i).unwrap_or(Value::Null),
                        None => Value::Null,
                    },
                );
            }
            Response::Character(json!({ "data": character }))
        }
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `?action=add-tag` — verify the tag exists, add its id to the character's
/// slim `tags` array (idempotent), `{ success: true, tag }` (201).
pub async fn character_add_tag(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    tag_id: &str,
) -> Response {
    let cid = character_id.to_string();
    let tid = tag_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let character = match characters_read::find_by_id(main, mount, &cid)? {
            Some(c) => c,
            None => return Ok(Err(not_found("Character"))),
        };
        let tag = match tags::find_full_by_id(main, &tid)? {
            Some(t) => t,
            None => return Ok(Err(not_found("Tag"))),
        };
        // v4 TaggableBaseRepository.addTag: push + update only if not present.
        let mut current: Vec<String> = character
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if !current.iter().any(|t| t == &tid) {
            current.push(tid.clone());
            let mut patch = serde_json::Map::new();
            patch.insert(
                "tags".into(),
                Value::Array(current.into_iter().map(Value::String).collect()),
            );
            vault_character_update::update_character(main, mount, &cid, &patch)?;
        }
        Ok(Ok(tag))
    })
    .await;
    match out {
        Ok(Ok(tag)) => Response::Character(json!({ "success": true, "tag": tag })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `?action=remove-tag` — filter the id from the character's slim `tags`
/// array (update only if changed), `{ success: true }`.
pub async fn character_remove_tag(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    tag_id: &str,
) -> Response {
    let cid = character_id.to_string();
    let tid = tag_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let character = match characters_read::find_by_id(main, mount, &cid)? {
            Some(c) => c,
            None => return Ok(Err(not_found("Character"))),
        };
        let current: Vec<String> = character
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let filtered: Vec<String> = current.iter().filter(|t| *t != &tid).cloned().collect();
        if filtered.len() != current.len() {
            let mut patch = serde_json::Map::new();
            patch.insert(
                "tags".into(),
                Value::Array(filtered.into_iter().map(Value::String).collect()),
            );
            vault_character_update::update_character(main, mount, &cid, &patch)?;
        }
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Character(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}
