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
//! passes `SINGLE_USER_ID`. Ownership (v4 `checkOwnership`, renamed `exists`
//! by `55752ad4` — it had always ignored its userId argument) collapses to
//! NotFound-on-absent for the single-user v5.

use serde_json::{json, Map, Value};

use crate::db::characters::CharacterCreate;
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::database_store;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::runtime::Db;
use crate::db::tags::{
    CreateOptions as TagCreateOptions, TagCreate, TagUpdate, TagVisualStyle, TagsRepository,
};
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::db::vault_wardrobe_public::{
    create_vault_wardrobe_item, delete_vault_wardrobe_item, update_vault_wardrobe_item,
    WardrobePatch, WardrobePublicError,
};
use crate::db::{
    character_plugin_data, character_vault::create_character, characters_read,
    doc_mount_documents::DocMountDocumentsRepository, tags, vault_character_arrays,
    vault_character_update, wardrobe_read, DbError,
};
use crate::photos::resolve_character_avatar::resolve_character_avatar;
use crate::services::aesthetics::DEPICTION_GUIDELINES_FILENAME;
use crate::services::character_enrichment;
use crate::services::image_job_common::with_both_conns;
use crate::vault_overlay::WardrobeItem;

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
/// (the OVERLAID read — throws on a broken vault) + `exists` (`checkOwnership`
/// before `55752ad4`). Returns the
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

/// v4 `?action=depiction-guidelines` (GET) — ownership (overlaid `findById`) →
/// read the `depiction-guidelines.md` file from the character's own vault root
/// (RAW, single-tier). `{ content }` (`''` when no vault or no file). Errors on
/// the read fall soft to `''` (v4's `readStoreFile` catch → null → `?? ''`).
pub fn character_depiction_guidelines(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let cid = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let character = match require_character(main, mount, &cid) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        let content = match character
            .get("characterDocumentMountPointId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            None => String::new(),
            Some(mount_id) => DocMountDocumentsRepository::new(mount)
                .find_by_mount_point_and_path(mount_id, DEPICTION_GUIDELINES_FILENAME)
                .ok()
                .flatten()
                .unwrap_or_default(),
        };
        Ok(Ok(content))
    });
    match result {
        Ok(Ok(content)) => Response::Character(json!({ "content": content })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `PUT ?action=depiction-guidelines` — ownership (RAW `findByIdRaw`, so a
/// broken-vault character can still edit) → `writeStoreFile` (empty/whitespace
/// deletes the file, else create-or-update) → `{ success: true }`; a character
/// with no vault → BadRequest.
pub async fn character_depiction_guidelines_update(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    content: &str,
) -> Response {
    let cid = character_id.to_string();
    let content = content.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let raw = match characters_read::find_by_id_raw(main, &cid)? {
            Some(c) => c,
            None => return Ok(Err(not_found("Character"))),
        };
        let mount_id = match raw
            .get("characterDocumentMountPointId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            Some(m) => m.to_string(),
            None => {
                return Ok(Err(bad_request(
                    "Character has no document vault to store depiction guidelines",
                )))
            }
        };
        // v4 `writeStoreFile`: empty/whitespace → delete, else write.
        if content.trim().is_empty() {
            database_store::delete_database_document(
                mount,
                &mount_id,
                DEPICTION_GUIDELINES_FILENAME,
            )?;
        } else if let Err(e) = database_store::write_database_document(
            mount,
            &mount_id,
            DEPICTION_GUIDELINES_FILENAME,
            &content,
        ) {
            return Ok(Err(internal(e)));
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

/// v4 `?action=stats` — a fan-out of independent counts + group hydration
/// (`[id]/handlers/get.ts:293`). Ownership (overlaid `findById`) → counts over
/// memories / chats / wardrobe / the vault file links / group memberships, with
/// photos/knowledge/core/characterFiles derived from the link relative paths (the
/// links fetched once and reused). `{ stats, groups }`.
pub fn character_stats(db: &Db, _user_id: &str, character_id: &str) -> Response {
    use crate::db::doc_mount_file_links::{is_photos_relative_path, DocMountFileLinksRepository};
    use crate::db::group_character_members::GroupCharacterMembersRepository;
    use crate::db::groups::GroupsRepository;
    use crate::db::vault_read_overlay::SINGLE_FILE_OVERLAY_PATHS;
    use std::collections::HashSet;

    let cid = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let character = match require_character(main, mount, &cid) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        let mount_point_id = character
            .get("characterDocumentMountPointId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());

        let memory_count = crate::db::memories_read::count_by_character_id(main, &cid)?;
        let conversations = crate::db::chats_read::find_by_character_id(main, &cid)?.len();
        let docs = DocMountDocumentsRepository::new(mount);
        let wardrobe_items = wardrobe_read::find_by_character_id(main, &docs, &cid, false)?.len();
        let file_links = match mount_point_id {
            Some(mid) => DocMountFileLinksRepository::new(mount).find_by_mount_point_id(mid)?,
            None => vec![],
        };
        let group_ids =
            GroupCharacterMembersRepository::new(mount).find_group_ids_by_character_id(&cid)?;

        // Photos / knowledge / core from the link relative paths (v4's predicate),
        // and the present-paths set for the characterFiles health figure.
        let mut present_paths: HashSet<String> = HashSet::new();
        let (mut photos, mut knowledge, mut core) = (0i64, 0i64, 0i64);
        for link in &file_links {
            let rel = link.relative_path.to_lowercase();
            present_paths.insert(rel.clone());
            if is_photos_relative_path(Some(&link.relative_path))
                || rel == "images/avatar.webp"
                || rel.starts_with("images/history/")
            {
                photos += 1;
            }
            if rel.starts_with("knowledge/") {
                knowledge += 1;
            }
            if rel.starts_with("core/") {
                core += 1;
            }
        }
        let mut character_files = 0i64;
        for core_path in SINGLE_FILE_OVERLAY_PATHS {
            if present_paths.contains(&core_path.to_lowercase()) {
                character_files += 1;
            }
        }

        // Hydrate the character's groups (deduped ids → the overlaid group record).
        let mut seen: HashSet<String> = HashSet::new();
        let groups_repo = GroupsRepository::new(main, mount);
        let mut groups: Vec<Value> = Vec::new();
        for gid in &group_ids {
            if !seen.insert(gid.clone()) {
                continue;
            }
            if let Ok(Some(g)) = groups_repo.find_by_id(gid) {
                groups.push(json!({
                    "id": g.get("id").cloned().unwrap_or(Value::Null),
                    "name": g.get("name").cloned().unwrap_or(Value::Null),
                    "description": g.get("description").cloned().unwrap_or(Value::Null),
                    "color": g.get("color").cloned().unwrap_or(Value::Null),
                    "icon": g.get("icon").cloned().unwrap_or(Value::Null),
                }));
            }
        }

        let scenarios = character
            .get("scenarios")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        let stats = json!({
            "memories": memory_count,
            "conversations": conversations,
            "wardrobeItems": wardrobe_items,
            "photos": photos,
            "scenarios": scenarios,
            "knowledge": knowledge,
            "core": core,
            "characterFiles": character_files,
            "characterFilesTotal": SINGLE_FILE_OVERLAY_PATHS.len(),
        });
        Ok(Ok(json!({ "stats": stats, "groups": groups })))
    });
    match result {
        Ok(Ok(body)) => Response::Character(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `?action=chats` (`characters/[id]/handlers/get.ts` `chats`) — the enriched
/// recent-chats DTO. Ownership (overlaid `findById`) → `chats.findByCharacterId`
/// → filter to the caller's `userId` → per-chat `getMessages` + `lastMessageAt`
/// (the max `type==='message'` createdAt, round-tripped through
/// `new Date(...).toISOString()`; else `chat.updatedAt`) → **stable** desc sort
/// by `lastMessageAt` → optional case-insensitive `search` over title + message
/// content → `slice(offset, offset+limit)` (defaults limit 10 / offset 0) → per
/// chat enrich (project map / tags / `_count` / scriptorium status / 3 most-recent
/// messages / story background / `isDangerousChat`). Body:
/// `{ chats, total: filteredChats.length }`.
pub fn character_chats(
    db: &Db,
    user_id: &str,
    character_id: &str,
    search: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Response {
    use crate::clock::{iso_from_unix_ms, iso_to_ms};
    use crate::db::conversation_chunks::ConversationChunksRepository;
    use crate::db::files::FilesRepository;
    use crate::db::projects::ProjectsRepository;
    use std::collections::HashMap;

    let cid = character_id.to_string();
    let user_id = user_id.to_string();
    // v4: `searchParams.get('search')?.toLowerCase() || ''`, then `if (search)`.
    let search = search.map(str::to_lowercase).filter(|s| !s.is_empty());
    let limit = limit.unwrap_or(10);
    let offset = offset.unwrap_or(0);

    let result = read_main_mount(db, move |main, mount| {
        // Ownership gate (overlaid read → 503 on a broken vault, matching v4).
        let character = match require_character(main, mount, &cid) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        let char_id = character.get("id").and_then(Value::as_str).unwrap_or(&cid);
        let char_name = character.get("name").cloned().unwrap_or(Value::Null);

        // chats.findByCharacterId → filter to this user.
        let all_chats = crate::db::chats_read::find_by_character_id(main, &cid)?;
        let mut with_messages: Vec<(Value, Vec<Value>, String)> = Vec::new();
        for chat in all_chats {
            if chat.get("userId").and_then(Value::as_str) != Some(user_id.as_str()) {
                continue;
            }
            let chat_id = chat
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let messages = crate::db::chats_messages_read::get_messages(main, &chat_id)?;
            // lastMessageAt = max(type==='message' createdAt) round-tripped through
            // JS `new Date(ms).toISOString()`, else chat.updatedAt (raw).
            let max_ms = messages
                .iter()
                .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
                .filter_map(|m| m.get("createdAt").and_then(Value::as_str))
                .filter_map(iso_to_ms)
                .max();
            let last_message_at = match max_ms {
                Some(ms) => iso_from_unix_ms(ms),
                None => chat
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            };
            with_messages.push((chat, messages, last_message_at));
        }

        // Stable desc sort by lastMessageAt (v4 `new Date(b) - new Date(a)`).
        with_messages.sort_by(|a, b| {
            let ta = iso_to_ms(&a.2).unwrap_or(0);
            let tb = iso_to_ms(&b.2).unwrap_or(0);
            tb.cmp(&ta)
        });

        // Optional search over title + message content (both already lowercased).
        let filtered: Vec<&(Value, Vec<Value>, String)> = match &search {
            None => with_messages.iter().collect(),
            Some(q) => with_messages
                .iter()
                .filter(|(chat, messages, _)| {
                    let title_hit = chat
                        .get("title")
                        .and_then(Value::as_str)
                        .map(|t| t.to_lowercase().contains(q))
                        .unwrap_or(false);
                    if title_hit {
                        return true;
                    }
                    messages.iter().any(|m| {
                        m.get("type").and_then(Value::as_str) == Some("message")
                            && m.get("content")
                                .and_then(Value::as_str)
                                .map(|c| c.to_lowercase().contains(q))
                                .unwrap_or(false)
                    })
                })
                .collect(),
        };
        let total = filtered.len();

        // slice(offset, offset+limit) — clamp to JS-Array.slice semantics.
        let start = offset.max(0) as usize;
        let page: Vec<&(Value, Vec<Value>, String)> = if start >= filtered.len() || limit <= 0 {
            Vec::new()
        } else {
            let end = start.saturating_add(limit as usize).min(filtered.len());
            filtered[start..end].to_vec()
        };

        // Project map for the page (v4 dedups projectIds → findById each).
        let projects = ProjectsRepository::new(main, mount);
        let mut project_map: HashMap<String, Value> = HashMap::new();
        for (chat, _, _) in &page {
            if let Some(pid) = chat.get("projectId").and_then(Value::as_str) {
                if !project_map.contains_key(pid) {
                    if let Ok(Some(p)) = projects.find_by_id(pid) {
                        let id = p.get("id").cloned().unwrap_or(Value::Null);
                        let name = p.get("name").cloned().unwrap_or(Value::Null);
                        project_map.insert(pid.to_string(), json!({ "id": id, "name": name }));
                    }
                }
            }
        }

        let chunks_repo = ConversationChunksRepository::new(main);
        let files_repo = FilesRepository::new(main);
        let mut enriched: Vec<Value> = Vec::with_capacity(page.len());
        for (chat, messages, last_message_at) in &page {
            let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or("");

            // Tags → `{ tag: { id, name } }`, input order, dropping missing.
            let tag_ids: Vec<String> = chat
                .get("tags")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let tag_rows = tags::find_by_ids(main, &tag_ids)?;
            let tag_data: Vec<Value> = tag_rows
                .into_iter()
                .map(|t| json!({ "tag": { "id": t.id, "name": t.name } }))
                .collect();

            // messageCount = type==='message' && role NOT SYSTEM/TOOL.
            let message_count = messages
                .iter()
                .filter(|m| {
                    m.get("type").and_then(Value::as_str) == Some("message")
                        && !matches!(
                            m.get("role").and_then(Value::as_str),
                            Some("SYSTEM") | Some("TOOL")
                        )
                })
                .count() as i64;
            let memory_count = crate::db::memories_read::count_by_chat_id(main, chat_id)?;

            // Scriptorium status from renderedMarkdown + chunk embedding coverage.
            let has_rendered = chat
                .get("renderedMarkdown")
                .map(|v| !v.is_null())
                .unwrap_or(false);
            let (total_chunks, embedded_chunks) = if has_rendered {
                chunks_repo.count_stats_by_chat_id(chat_id)?
            } else {
                (0, 0)
            };
            let scriptorium_status = if has_rendered {
                if embedded_chunks >= total_chunks && total_chunks > 0 {
                    "embedded"
                } else {
                    "rendered"
                }
            } else {
                "none"
            };

            // 3 most-recent type==='message' messages (stable desc by createdAt).
            let mut msg_refs: Vec<&Value> = messages
                .iter()
                .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
                .collect();
            msg_refs.sort_by(|a, b| {
                let ta = a
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .and_then(iso_to_ms)
                    .unwrap_or(0);
                let tb = b
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .and_then(iso_to_ms)
                    .unwrap_or(0);
                tb.cmp(&ta)
            });
            let recent_messages: Vec<Value> = msg_refs
                .iter()
                .take(3)
                .map(|m| {
                    json!({
                        "id": m.get("id").cloned().unwrap_or(Value::Null),
                        "role": m.get("role").cloned().unwrap_or(Value::Null),
                        "content": m.get("content").cloned().unwrap_or(Value::Null),
                        "createdAt": m.get("createdAt").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect();

            let project = chat
                .get("projectId")
                .and_then(Value::as_str)
                .and_then(|pid| project_map.get(pid).cloned())
                .unwrap_or(Value::Null);

            let story_background = match chat.get("storyBackgroundImageId").and_then(Value::as_str)
            {
                Some(bg_id) if !bg_id.is_empty() => match files_repo.find_by_id(bg_id)? {
                    Some(f) => json!({ "id": f.id, "filepath": format!("/api/v1/files/{}", f.id) }),
                    None => Value::Null,
                },
                _ => Value::Null,
            };

            let is_dangerous = chat.get("isDangerousChat").and_then(Value::as_bool) == Some(true);

            enriched.push(json!({
                "id": chat.get("id").cloned().unwrap_or(Value::Null),
                "title": chat.get("title").cloned().unwrap_or(Value::Null),
                "createdAt": chat.get("createdAt").cloned().unwrap_or(Value::Null),
                "updatedAt": chat.get("updatedAt").cloned().unwrap_or(Value::Null),
                "lastMessageAt": last_message_at,
                "character": { "id": char_id, "name": char_name },
                "project": project,
                "storyBackground": story_background,
                "messages": recent_messages,
                "tags": tag_data,
                "isDangerousChat": is_dangerous,
                "_count": { "messages": message_count, "memories": memory_count },
                "scriptoriumStatus": scriptorium_status,
            }));
        }

        Ok(Ok(json!({ "chats": enriched, "total": total })))
    });
    match result {
        Ok(Ok(body)) => Response::Character(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `?action=export` (`characters/[id]/handlers/get.ts:63`), JSON leg —
/// ownership (overlaid `findById`) → `exportSTCharacter(character)` → the ST
/// `chara_card_v2` card (the SPA downloads it client-side). `format=png` streams
/// binary and rides the quilltap-web REST route
/// (`GET /api/v1/characters/{id}?action=export&format=png`, P4.6m) — the dispatch
/// channel carries JSON only, so the PNG arm points there.
pub fn character_export(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    format: Option<&str>,
) -> Response {
    // v4: `format = searchParams.get('format') || 'json'`.
    let format = format.filter(|s| !s.is_empty()).unwrap_or("json");
    if format == "png" {
        return bad_request(
            "PNG export streams binary — use GET \
             /api/v1/characters/{id}?action=export&format=png",
        );
    }
    let cid = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let character = match require_character(main, mount, &cid) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        Ok(Ok(crate::services::sillytavern::export_st_character(
            &character,
        )))
    });
    match result {
        Ok(Ok(card)) => Response::Character(card),
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

// ===========================================================================
// Create / quick-create / update (v4 post.ts :304/:353, put.ts :86)
// ===========================================================================

/// A non-empty string field (`""` treated as absent, like v4's `|| null`).
fn opt_str(body: &Value, key: &str) -> Option<String> {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Build the slim `CharacterCreate` from a validated create body (v4
/// `handleCreate`'s hand-mapped `repos.characters.create` call). `controlledBy`
/// defaults to `'llm'`, `npc` to `false`; the rest of the slim columns take their
/// create-time defaults (fresh vault ⇒ `defaultImageId` null, empty tags /
/// partnerLinks / avatarOverrides). Returns BadRequest when `name` is missing.
fn build_create_slim(user_id: &str, body: &Value) -> Result<CharacterCreate, Response> {
    let name = match body.get("name").and_then(Value::as_str) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => return Err(bad_request("Name is required")),
    };
    let controlled_by = body
        .get("controlledBy")
        .and_then(Value::as_str)
        .filter(|s| *s == "user" || *s == "llm")
        .unwrap_or("llm")
        .to_string();
    let npc = body.get("npc").and_then(Value::as_bool).unwrap_or(false);
    Ok(CharacterCreate {
        user_id: user_id.to_string(),
        name,
        default_image_id: None,
        default_connection_profile_id: opt_str(body, "defaultConnectionProfileId"),
        default_partner_id: None,
        default_roleplay_template_id: None,
        default_image_profile_id: None,
        silly_tavern_data: None,
        is_favorite: false,
        npc,
        controlled_by,
        default_agent_mode_enabled: None,
        default_help_tools_enabled: None,
        default_timestamp_config: None,
        default_scenario_id: None,
        default_system_prompt_id: None,
        // create always provisions a fresh vault; the FK is nulled internally.
        character_document_mount_point_id: None,
        can_dress_themselves: None,
        can_create_outfits: None,
        system_transparency: None,
        core_whisper_enabled: None,
        can_be_carina: None,
        partner_links: vec![],
        tags: vec![],
        avatar_overrides: vec![],
    })
}

/// v4 `POST /characters` (`handleCreate`): `createCharacterSchema.parse` → slim
/// defaults + the managed inputs → `create_character` (which provisions the vault)
/// → reload through the overlay. Response `{ character }` (201). The managed-field
/// bag (`title`/`identity`/`description`/`manifesto`/`personality`/`firstMessage`/
/// `exampleDialogues`/`scenarios`/`systemPrompts`/`physicalDescription`)
/// deserializes straight off the body (unknown keys ignored; a scenario's
/// id/timestamps are irrelevant to the title/content vault projection).
pub async fn character_create(db: &Db, user_id: &str, body: Value) -> Response {
    let slim = match build_create_slim(user_id, &body) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let vault: CharacterVaultWriteInput = match serde_json::from_value(body) {
        Ok(v) => v,
        Err(e) => return bad_request(format!("Invalid character data: {e}")),
    };
    let out = with_both_conns(db, move |main, mount| {
        let id = create_character(main, mount, &slim, &vault)?;
        // v4 reloads through the overlay (`findById`) so the echo reflects the
        // vault-backed state incl. the freshly-set mount-point id.
        Ok(characters_read::find_by_id(main, mount, &id)?.unwrap_or(Value::Null))
    })
    .await;
    match out {
        Ok(c) => Response::Character(json!({ "character": c })),
        Err(e) => internal(e),
    }
}

/// v4 `POST /characters?action=quick-create` (`handleQuickCreate`): a minimal
/// character (name only via the contract) with a fixed `description` of
/// `"Character created during chat import"`, everything else defaulted, `'llm'`
/// control. Response `{ character }` (201).
pub async fn character_quick_create(db: &Db, user_id: &str, name: &str) -> Response {
    if name.is_empty() {
        return bad_request("Name is required");
    }
    let slim = CharacterCreate {
        user_id: user_id.to_string(),
        name: name.to_string(),
        default_image_id: None,
        default_connection_profile_id: None,
        default_partner_id: None,
        default_roleplay_template_id: None,
        default_image_profile_id: None,
        silly_tavern_data: None,
        is_favorite: false,
        npc: false,
        controlled_by: "llm".to_string(),
        default_agent_mode_enabled: None,
        default_help_tools_enabled: None,
        default_timestamp_config: None,
        default_scenario_id: None,
        default_system_prompt_id: None,
        character_document_mount_point_id: None,
        can_dress_themselves: None,
        can_create_outfits: None,
        system_transparency: None,
        core_whisper_enabled: None,
        can_be_carina: None,
        partner_links: vec![],
        tags: vec![],
        avatar_overrides: vec![],
    };
    // v4 passes `description: 'Character created during chat import'`; the rest of
    // the managed fields are their empty defaults.
    let vault = CharacterVaultWriteInput {
        description: Some("Character created during chat import".to_string()),
        ..Default::default()
    };
    let out = with_both_conns(db, move |main, mount| {
        let id = create_character(main, mount, &slim, &vault)?;
        Ok(characters_read::find_by_id(main, mount, &id)?.unwrap_or(Value::Null))
    })
    .await;
    match out {
        Ok(c) => Response::Character(json!({ "character": c })),
        Err(e) => internal(e),
    }
}

/// v4 `POST /characters?action=import` (`characters/handlers/post.ts`
/// `handleImport`, JSON leg). Parse the ST card (`body.characterData || body`,
/// then `importSTCharacter`'s `.data` unwrap), then create the character DIRECTLY
/// through the create primitive — NOT `character_create`: the import path calls
/// `repos.characters.create` with the ST-derived bag, so `sillyTavernData` lands
/// in the slim column and no `createCharacterSchema` runs. Echo the slim create
/// shape `{ character: { id, name, description, defaultImageId, createdAt,
/// updatedAt, _count: { chats } } }`. The PNG/multipart import leg rides the
/// quilltap-web REST route (`POST /api/v1/characters?action=import`, P4.6m),
/// which reuses this create spine and then writes the imported avatar.
pub async fn character_import(db: &Db, user_id: &str, payload: Value) -> Response {
    // v4: `characterData = body.characterData || body`.
    let character_data = payload
        .get("characterData")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or(payload);
    let imported = crate::services::sillytavern::import_st_character(&character_data);

    let name = match imported.get("name").and_then(Value::as_str) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => return bad_request("Character data is required"),
    };

    // The slim create bag: v4 `repos.characters.create({ userId, ...importedData,
    // isFavorite:false, tags:[], partnerLinks:[], avatarOverrides:[],
    // defaultImageId:null, physicalDescription:null })`. `sillyTavernData` is the
    // only slim managed input the import carries beyond name/controlledBy defaults.
    let slim = CharacterCreate {
        user_id: user_id.to_string(),
        name,
        default_image_id: None,
        default_connection_profile_id: None,
        default_partner_id: None,
        default_roleplay_template_id: None,
        default_image_profile_id: None,
        silly_tavern_data: imported.get("sillyTavernData").cloned(),
        is_favorite: false,
        npc: false,
        controlled_by: "llm".to_string(),
        default_agent_mode_enabled: None,
        default_help_tools_enabled: None,
        default_timestamp_config: None,
        default_scenario_id: None,
        default_system_prompt_id: None,
        character_document_mount_point_id: None,
        can_dress_themselves: None,
        can_create_outfits: None,
        system_transparency: None,
        core_whisper_enabled: None,
        can_be_carina: None,
        partner_links: vec![],
        tags: vec![],
        avatar_overrides: vec![],
    };
    // The vault-managed inputs deserialize off the imported bag
    // (title/description/personality/firstMessage/exampleDialogues/scenarios/
    // systemPrompts; `physicalDescription` is absent → `None`, matching the
    // handler's explicit `physicalDescription: null`).
    let vault: CharacterVaultWriteInput = match serde_json::from_value(imported) {
        Ok(v) => v,
        Err(e) => return bad_request(format!("Invalid character data: {e}")),
    };

    let out = with_both_conns(db, move |main, mount| {
        let id = create_character(main, mount, &slim, &vault)?;
        let character = characters_read::find_by_id(main, mount, &id)?.unwrap_or(Value::Null);
        let chat_count = crate::db::chats_read::find_by_character_id(main, &id)?.len();
        Ok((character, chat_count))
    })
    .await;
    match out {
        Ok((character, chat_count)) => Response::Character(json!({
            "character": {
                "id": character.get("id").cloned().unwrap_or(Value::Null),
                "name": character.get("name").cloned().unwrap_or(Value::Null),
                "description": character.get("description").cloned().unwrap_or(Value::Null),
                "defaultImageId": Value::Null,
                "createdAt": character.get("createdAt").cloned().unwrap_or(Value::Null),
                "updatedAt": character.get("updatedAt").cloned().unwrap_or(Value::Null),
                "_count": { "chats": chat_count },
            }
        })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Photo gallery (v4 characters/[id]/photos + [linkId]) — JSON legs
// ===========================================================================

/// Map a [`GalleryError`] to the route's response (v4's `catch` message routing:
/// `Character not found` → 404; the enumerated validation messages → 400; else 500).
fn gallery_err(e: crate::photos::character_gallery_service::GalleryError) -> Response {
    use crate::photos::character_gallery_service::GalleryError;
    match e {
        GalleryError::CharacterNotFound => not_found("Character"),
        GalleryError::BadRequest(msg) => bad_request(msg),
        GalleryError::Db(err) => internal(err),
    }
}

/// v4 `GET /characters/[id]/photos` — `listCharacterGallery` →
/// `{ entries, total, hasMore }`.
pub fn character_photo_list(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Response {
    let cid = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        Ok(
            crate::photos::character_gallery_service::list_character_gallery(
                main, mount, &cid, limit, offset,
            ),
        )
    });
    match result {
        Ok(Ok(body)) => Response::Character(body),
        Ok(Err(e)) => gallery_err(e),
        Err(e) => internal(e),
    }
}

/// v4 `DELETE /characters/[id]/photos/[linkId]` — `removeFromCharacterGallery`
/// (via the GC-safe `deleteWithGC` chokepoint). `{ deleted, fileGC }`; a
/// not-deleted result (unknown/foreign link) → `notFound('Gallery entry')`.
pub async fn character_photo_remove(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    link_id: &str,
) -> Response {
    let cid = character_id.to_string();
    let lid = link_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        Ok(
            crate::photos::character_gallery_service::remove_from_character_gallery(
                main, mount, &cid, &lid,
            ),
        )
    })
    .await;
    match out {
        Ok(Ok((deleted, file_gc))) => {
            if !deleted {
                not_found("Gallery entry")
            } else {
                Response::Character(json!({ "deleted": true, "fileGC": file_gc }))
            }
        }
        Ok(Err(e)) => gallery_err(e),
        Err(e) => internal(e),
    }
}

/// v4 `POST /characters/[id]/photos` JSON leg (`saveByIdSchema` — exactly one of
/// `fileId`/`linkId`). The `linkId` leg (`saveLinkToCharacterGallery`) reads bytes
/// from the source link's mount-blob and hard-links a copy into the character's
/// `photos/` folder — fully DB-resolvable and LIVE. The `fileId` leg
/// (`saveFileToCharacterGallery`) reads bytes via the host file store
/// (`fileStorageManager.downloadFile`), which the dispatch channel can't build;
/// it rides the quilltap-web REST route (`POST /api/v1/characters/{id}/photos`
/// with a JSON `{fileId}` body, P4.6m), which constructs the per-request storage
/// backend. `keptAt` is minted here (the injected ISO clock).
pub async fn character_photo_save_by_id(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    file_id: Option<&str>,
    link_id: Option<&str>,
) -> Response {
    // v4 `saveByIdSchema.refine`: exactly one of fileId/linkId.
    match (file_id, link_id) {
        (Some(_), Some(_)) | (None, None) => {
            return bad_request("Provide exactly one of fileId or linkId")
        }
        _ => {}
    }
    let Some(lid) = link_id else {
        // The fileId leg needs the host file-store bytes seam the dispatch
        // channel can't build — it rides the quilltap-web REST route.
        return bad_request(
            "Saving by fileId reads host-stored bytes — POST \
             /api/v1/characters/{id}/photos with a JSON {fileId} body",
        );
    };
    let kept_at = crate::clock::now_iso();
    let cid = character_id.to_string();
    let lid = lid.to_string();
    let out = with_both_conns(db, move |main, mount| {
        Ok(
            crate::photos::character_gallery_service::save_link_to_character_gallery(
                main,
                mount,
                &cid,
                &lid,
                None,
                &[],
                &kept_at,
            ),
        )
    })
    .await;
    match out {
        Ok(Ok(v)) => Response::Character(v),
        Ok(Err(e)) => gallery_err(e),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Cascade delete + preview (v4 cascade-delete.ts)
// ===========================================================================

/// v4 `?action=cascade-preview` (`characters/[id]/handlers/get.ts:236`) —
/// overlaid ownership → `getCascadeDeletePreview` → the projected preview DTO
/// `{ characterId, characterName, exclusiveChats:[{id,title,messageCount,
/// lastMessageAt}], exclusiveCharacterImageCount, exclusiveChatImageCount,
/// totalExclusiveImageCount, memoryCount }`.
pub fn character_cascade_preview(db: &Db, _user_id: &str, character_id: &str) -> Response {
    let cid = character_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        // v4's GET-action ownership gate (overlaid findById + `exists`,
        // spelled `checkOwnership` before `55752ad4`).
        if let Err(r) = require_character(main, mount, &cid) {
            return Ok(Err(r));
        }
        let preview =
            match crate::services::cascade_delete::get_cascade_delete_preview(main, mount, &cid)? {
                Some(p) => p,
                None => return Ok(Err(internal("Failed to generate preview"))),
            };
        let exclusive_chats: Vec<Value> = preview
            .exclusive_chats
            .iter()
            .map(|(chat, message_count)| {
                let mut o = serde_json::Map::new();
                o.insert("id".into(), chat.get("id").cloned().unwrap_or(Value::Null));
                o.insert(
                    "title".into(),
                    chat.get("title").cloned().unwrap_or(Value::Null),
                );
                o.insert("messageCount".into(), json!(message_count));
                // v4 `lastMessageAt: c.chat.lastMessageAt` — omitted when the slim
                // column is NULL (chats_read drops the key).
                if let Some(lma) = chat.get("lastMessageAt") {
                    o.insert("lastMessageAt".into(), lma.clone());
                }
                Value::Object(o)
            })
            .collect();
        let char_img = preview.exclusive_character_images.len() as i64;
        let chat_img = preview.exclusive_chat_images.len() as i64;
        Ok(Ok(json!({
            "characterId": cid,
            "characterName": preview.character_name,
            "exclusiveChats": exclusive_chats,
            "exclusiveCharacterImageCount": char_img,
            "exclusiveChatImageCount": chat_img,
            "totalExclusiveImageCount": char_img + chat_img,
            "memoryCount": preview.memory_count,
        })))
    });
    match result {
        Ok(Ok(body)) => Response::Character(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `DELETE /characters/[id]` (`[id]/handlers/delete.ts`) — `findByIdRaw`
/// ownership (a broken-vault character stays deletable), then
/// `executeCascadeDelete` with the `cascadeChats`/`cascadeImages` flags. Body
/// `{ success, deletedChats, deletedImages, deletedMemories }`; a not-`success`
/// result → serverError.
pub async fn character_delete(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    cascade_chats: bool,
    cascade_images: bool,
) -> Response {
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        // v4 raw-row ownership gate (broken-vault-safe).
        if characters_read::find_by_id_raw(main, &cid)?.is_none() {
            return Ok(Err(not_found("Character")));
        }
        let (success, dc, di, dm) = crate::services::cascade_delete::execute_cascade_delete(
            main,
            mount,
            &cid,
            cascade_chats,
            cascade_images,
        )?;
        Ok(Ok((success, dc, di, dm)))
    })
    .await;
    match out {
        Ok(Ok((success, dc, di, dm))) => {
            if !success {
                internal("Failed to delete character")
            } else {
                Response::Character(json!({
                    "success": true,
                    "deletedChats": dc,
                    "deletedImages": di,
                    "deletedMemories": dm,
                }))
            }
        }
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// The `updateCharacterSchema` keys (v4 `put.ts`). Zod `.parse` strips unknown
/// keys, so the patch is whitelisted to exactly these; the vault write-overlay
/// routes the managed ones (`title`/`identity`/… + `scenarios`/`systemPrompts`/
/// `physicalDescription`) and the slim `_update` takes the rest.
const UPDATE_SCHEMA_KEYS: &[&str] = &[
    "name",
    "title",
    "identity",
    "description",
    "manifesto",
    "personality",
    "scenarios",
    "firstMessage",
    "exampleDialogues",
    "talkativeness",
    "defaultConnectionProfileId",
    "defaultImageProfileId",
    "aliases",
    "pronouns",
    "controlledBy",
    "npc",
    "defaultAgentModeEnabled",
    "defaultHelpToolsEnabled",
    "defaultTimestampConfig",
    "defaultScenarioId",
    "defaultSystemPromptId",
    "characterDocumentMountPointId",
    "systemTransparency",
    "coreWhisperEnabled",
    "canBeCarina",
    // Vault-managed (`properties.json`) plain boolean — named here or the Zod strip
    // would drop it before `update()` could route it to the vault (v4 `put.ts:69`).
    "canChooseOutfit",
    // Wardrobe-permission tri-states (null = inherit global, true/false = override).
    // Real DB columns; omitting them here would strip the key before it reached
    // `update()`, so the editor toggles would silently never persist (v4 `put.ts:74`).
    "canDressThemselves",
    "canCreateOutfits",
    // The freeform fact sheet (vault `metadata.json`). Zod `.parse` strips unknown
    // keys, so it MUST be named here or a PUT carrying it would be silently dropped
    // before the write overlay ever saw it (v4 `put.ts:73`). Sending `metadata`
    // REPLACES the whole object.
    "metadata",
    "physicalDescription",
];

/// v4 `PUT /characters/[id]` (`handlePut` default branch). `findByIdRaw` first
/// (a broken-vault character stays editable and thereby self-repairs), then the
/// whitelisted patch (with v4's empty-string transforms) routes through
/// `update_character` (managed → vault, remainder → slim `_update`). Response
/// `{ character }` = the reloaded overlay (v4 `applyDocumentStoreOverlayOne` of the
/// merged slim; after the write a re-read is byte-equal once minted ids /
/// timestamps normalize).
pub async fn character_update(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    body: Value,
) -> Response {
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        // Existence/ownership on the RAW row (broken-vault chars stay editable).
        if characters_read::find_by_id_raw(main, &cid)?.is_none() {
            return Ok(Err(not_found("Character")));
        }
        let patch = build_update_patch(&body);
        vault_character_update::update_character(main, mount, &cid, &patch)?;
        // Reload through the overlay (managed fields read back from the vault).
        Ok(Ok(
            characters_read::find_by_id(main, mount, &cid)?.unwrap_or(Value::Null)
        ))
    })
    .await;
    match out {
        Ok(Ok(c)) => Response::Character(json!({ "character": c })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// Build the slim+managed patch from an update body: keep only
/// [`UPDATE_SCHEMA_KEYS`] (Zod strip-unknown) and apply v4's empty-string
/// transforms (`defaultConnectionProfileId`/`defaultImageProfileId` `""` → omit;
/// `characterDocumentMountPointId` `""` → `null`).
fn build_update_patch(body: &Value) -> Map<String, Value> {
    let mut patch = Map::new();
    let Some(obj) = body.as_object() else {
        return patch;
    };
    for key in UPDATE_SCHEMA_KEYS {
        let Some(v) = obj.get(*key) else { continue };
        match *key {
            // `.or(z.literal('').transform(() => undefined))` — drop empty string.
            "defaultConnectionProfileId" | "defaultImageProfileId" => {
                if v.as_str() == Some("") {
                    continue;
                }
                patch.insert((*key).to_string(), v.clone());
            }
            // `.or(z.literal('').transform(() => null))` — empty string → null.
            "characterDocumentMountPointId" => {
                let val = if v.as_str() == Some("") {
                    Value::Null
                } else {
                    v.clone()
                };
                patch.insert((*key).to_string(), val);
            }
            _ => {
                patch.insert((*key).to_string(), v.clone());
            }
        }
    }
    patch
}

// ===========================================================================
// Sub-resource mutations (prompts / scenarios / plugin-data)
// ===========================================================================

/// Ownership gate inside a write closure (v4 `findById` + `exists`, spelled
/// `checkOwnership` before `55752ad4`).
fn require_character_owned(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    character_id: &str,
) -> Result<Result<Value, Response>, DbError> {
    Ok(
        match characters_read::find_by_id(main, mount, character_id)? {
            Some(c) => Ok(c),
            None => Err(not_found("Character")),
        },
    )
}

/// v4 `POST /characters/[id]/prompts` — `addSystemPrompt` → `{ prompt }` (201).
pub async fn character_prompt_create(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    name: &str,
    content: &str,
    is_default: bool,
) -> Response {
    let (cid, name, content) = (
        character_id.to_string(),
        name.to_string(),
        content.to_string(),
    );
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        match vault_character_arrays::add_system_prompt(
            main, mount, &cid, &name, &content, is_default,
        )? {
            Some(p) => Ok(Ok(p)),
            None => Ok(Err(internal("Failed to add system prompt"))),
        }
    })
    .await;
    wrap_obj(out, "prompt")
}

/// v4 `PUT /characters/[id]/prompts/[promptId]` — verify the prompt exists,
/// `updateSystemPrompt` (patch = the present fields) → `{ prompt }`.
pub async fn character_prompt_update(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    prompt_id: &str,
    name: Option<&str>,
    content: Option<&str>,
    is_default: Option<bool>,
) -> Response {
    let (cid, pid) = (character_id.to_string(), prompt_id.to_string());
    let (name, content) = (name.map(str::to_string), content.map(str::to_string));
    let out = with_both_conns(db, move |main, mount| {
        let character = match require_character_owned(main, mount, &cid)? {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        if !sub_array_contains(&character, "systemPrompts", &pid) {
            return Ok(Err(not_found("Prompt")));
        }
        let mut patch = serde_json::Map::new();
        if let Some(n) = &name {
            patch.insert("name".into(), json!(n));
        }
        if let Some(c) = &content {
            patch.insert("content".into(), json!(c));
        }
        if let Some(d) = is_default {
            patch.insert("isDefault".into(), json!(d));
        }
        match vault_character_arrays::update_system_prompt(main, mount, &cid, &pid, &patch)? {
            Some(p) => Ok(Ok(p)),
            None => Ok(Err(internal("Failed to update system prompt"))),
        }
    })
    .await;
    wrap_obj(out, "prompt")
}

/// v4 `DELETE /characters/[id]/prompts/[promptId]` — `{ success: true }`.
pub async fn character_prompt_delete(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    prompt_id: &str,
) -> Response {
    let (cid, pid) = (character_id.to_string(), prompt_id.to_string());
    let out = with_both_conns(db, move |main, mount| {
        let character = match require_character_owned(main, mount, &cid)? {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        if !sub_array_contains(&character, "systemPrompts", &pid) {
            return Ok(Err(not_found("Prompt")));
        }
        if vault_character_arrays::delete_system_prompt(main, mount, &cid, &pid)? {
            Ok(Ok(()))
        } else {
            Ok(Err(internal("Failed to delete system prompt")))
        }
    })
    .await;
    success_or(out)
}

/// The `set-default-prompt` verb (contract-shared with the SPA; v4 reaches it via
/// the prompt PUT with `{isDefault:true}`). Composes the ported
/// `set_default_system_prompt`.
pub async fn character_prompt_set_default(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    prompt_id: &str,
) -> Response {
    let (cid, pid) = (character_id.to_string(), prompt_id.to_string());
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        if vault_character_arrays::set_default_system_prompt(main, mount, &cid, &pid)? {
            Ok(Ok(()))
        } else {
            Ok(Err(not_found("Prompt")))
        }
    })
    .await;
    success_or(out)
}

/// v4 `POST /characters/[id]/scenarios` — `addScenario` → `{ scenario }` (201).
pub async fn character_scenario_create(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    title: &str,
    content: &str,
) -> Response {
    let (cid, title, content) = (
        character_id.to_string(),
        title.to_string(),
        content.to_string(),
    );
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        match vault_character_arrays::add_scenario(main, mount, &cid, &title, &content)? {
            Some(s) => Ok(Ok(s)),
            None => Ok(Err(internal("Failed to add scenario"))),
        }
    })
    .await;
    wrap_obj(out, "scenario")
}

/// v4 `PUT /characters/[id]/scenarios?scenarioId=` — `updateScenario` (null →
/// NotFound) → `{ scenario }`.
pub async fn character_scenario_update(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    scenario_id: &str,
    title: Option<&str>,
    content: Option<&str>,
) -> Response {
    let (cid, sid) = (character_id.to_string(), scenario_id.to_string());
    let (title, content) = (title.map(str::to_string), content.map(str::to_string));
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        let mut patch = serde_json::Map::new();
        if let Some(t) = &title {
            patch.insert("title".into(), json!(t));
        }
        if let Some(c) = &content {
            patch.insert("content".into(), json!(c));
        }
        match vault_character_arrays::update_scenario(main, mount, &cid, &sid, &patch)? {
            Some(s) => Ok(Ok(s)),
            None => Ok(Err(not_found("Scenario"))),
        }
    })
    .await;
    wrap_obj(out, "scenario")
}

/// v4 `DELETE /characters/[id]/scenarios?scenarioId=` — `{ message: "Scenario
/// removed" }` (or NotFound).
pub async fn character_scenario_delete(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    scenario_id: &str,
) -> Response {
    let (cid, sid) = (character_id.to_string(), scenario_id.to_string());
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        if vault_character_arrays::remove_scenario(main, mount, &cid, &sid)? {
            Ok(Ok(()))
        } else {
            Ok(Err(not_found("Scenario")))
        }
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Character(json!({ "message": "Scenario removed" })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `POST/PUT plugin-data` — `upsert(id, pluginName, data)` → re-read →
/// `{ pluginData: entry }`.
pub async fn character_plugin_data_upsert(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    plugin_name: &str,
    data: Value,
) -> Response {
    let (cid, plugin) = (character_id.to_string(), plugin_name.to_string());
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        crate::db::character_plugin_data::CharacterPluginDataRepository::new(main).upsert(
            &cid,
            &plugin,
            data.clone(),
        )?;
        // v4's `upsert` returns the entity from the base create/update, whose
        // `data` is the INPUT value (an object), NOT the stored-then-re-parsed
        // string. Re-read for the row metadata (id/timestamps), then overlay the
        // input `data` object.
        let mut entry = character_plugin_data::find_by_character_and_plugin(main, &cid, &plugin)?
            .unwrap_or(Value::Null);
        if let Some(obj) = entry.as_object_mut() {
            obj.insert("data".into(), data);
        }
        Ok(Ok(entry))
    })
    .await;
    wrap_obj(out, "pluginData")
}

/// v4 `DELETE plugin-data/[pluginName]` — `{ success: true }` (or NotFound).
pub async fn character_plugin_data_delete(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    plugin_name: &str,
) -> Response {
    let (cid, plugin) = (character_id.to_string(), plugin_name.to_string());
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        if character_plugin_data::delete_by_character_and_plugin(main, &cid, &plugin)? {
            Ok(Ok(()))
        } else {
            Ok(Err(not_found("Plugin data")))
        }
    })
    .await;
    success_or(out)
}

// ===========================================================================
// Wardrobe mutations (v4 characters/[id]/wardrobe/**)
// ===========================================================================

/// A JSON `null | string` field → `Option<String>` (null/absent → `None`,
/// string → `Some`). v4's `?? null` create-default coalesces both to null.
fn nullable_str(body: &Value, key: &str) -> Option<String> {
    body.get(key).and_then(Value::as_str).map(str::to_string)
}

/// A wardrobe UPDATE patch triple-state for a nullable string key: absent →
/// `None` (unchanged), JSON `null` → `Some(None)` (clear), string →
/// `Some(Some(s))`.
fn patch_nullable(body: &Value, key: &str) -> Option<Option<String>> {
    match body.get(key) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(_) => None,
    }
}

/// Map a public-wardrobe write failure to the dispatch refusal (v4 lets the throw
/// surface as a 500 `serverError`).
fn wardrobe_err(e: WardrobePublicError) -> Response {
    match e {
        WardrobePublicError::NoMount => {
            internal("Character has no wardrobe vault to store the item")
        }
        WardrobePublicError::Cycle(m) => internal(m),
        WardrobePublicError::Db(db) => internal(db),
    }
}

/// v4 `POST /characters/[id]/wardrobe` — `findById` ownership →
/// `createWardrobeItemSchema` → `repos.wardrobe.create` (mints id/timestamps →
/// the vault-backed create) → `{ wardrobeItem }` (201).
pub async fn character_wardrobe_create(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    body: Value,
) -> Response {
    let cid = character_id.to_string();
    let title = match body.get("title").and_then(Value::as_str) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return bad_request("Title is required"),
    };
    let types: Vec<String> = body
        .get("types")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if types.is_empty() {
        return bad_request("At least one type is required");
    }
    let component_item_ids: Vec<String> = body
        .get("componentItemIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let description = nullable_str(&body, "description");
    let image_prompt = nullable_str(&body, "imagePrompt");
    let appropriateness = nullable_str(&body, "appropriateness");
    let is_default = body
        .get("isDefault")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let replace = body
        .get("replace")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        // Mint id/timestamps (v4 `repos.wardrobe.create` materializes them).
        let now = crate::clock::now_iso();
        let item = WardrobeItem {
            id: uuid::Uuid::new_v4().to_string(),
            character_id: Some(Some(cid.clone())),
            title,
            description: Some(description),
            image_prompt: Some(image_prompt),
            types,
            component_item_ids,
            appropriateness: Some(appropriateness),
            is_default,
            replace,
            // v4's create sets `migratedFromClothingRecordId: null` explicitly (it
            // appears as null in the create echo), but NOT `archivedAt` (absent),
            // so the create echo carries the former and omits the latter.
            migrated_from_clothing_record_id: Some(None),
            archived_at: None,
            created_at: now.clone(),
            updated_at: now,
        };
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        match create_vault_wardrobe_item(main, &links, &docs, &item) {
            Ok(stored) => Ok(Ok(serde_json::to_value(stored).unwrap_or(Value::Null))),
            Err(e) => Ok(Err(wardrobe_err(e))),
        }
    })
    .await;
    wrap_obj(out, "wardrobeItem")
}

/// v4 `GET /characters/[id]/wardrobe/[itemId]` — `findById` ownership →
/// `findByIdForCharacter` (guard `characterId === id`) → `{ wardrobeItem }`.
pub fn character_wardrobe_get(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    item_id: &str,
) -> Response {
    let cid = character_id.to_string();
    let iid = item_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        if let Err(r) = require_character(main, mount, &cid) {
            return Ok(Err(r));
        }
        let docs = DocMountDocumentsRepository::new(mount);
        match wardrobe_read::find_by_id_for_character(main, &docs, &cid, &iid, &[])? {
            Some(item) => Ok(Ok(item)),
            None => Ok(Err(not_found("Wardrobe item"))),
        }
    });
    match result {
        Ok(Ok(item)) => Response::Character(json!({ "wardrobeItem": item })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `PUT /characters/[id]/wardrobe/[itemId]` — ownership + existence pre-check
/// → `updateWardrobeItemSchema` patch → `repos.wardrobe.update` → `{ wardrobeItem }`.
pub async fn character_wardrobe_update(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    item_id: &str,
    body: Value,
) -> Response {
    let cid = character_id.to_string();
    let iid = item_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        let docs = DocMountDocumentsRepository::new(mount);
        // v4 pre-checks existence before update (findByIdForCharacter).
        if wardrobe_read::find_by_id_for_character(main, &docs, &cid, &iid, &[])?.is_none() {
            return Ok(Err(not_found("Wardrobe item")));
        }
        let mut patch = WardrobePatch::default();
        if let Some(t) = body.get("title").and_then(Value::as_str) {
            patch.title = Some(t.to_string());
        }
        if let Some(a) = body.get("types").and_then(Value::as_array) {
            patch.types = Some(
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
            );
        }
        if let Some(a) = body.get("componentItemIds").and_then(Value::as_array) {
            patch.component_item_ids = Some(
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
            );
        }
        patch.description = patch_nullable(&body, "description");
        patch.image_prompt = patch_nullable(&body, "imagePrompt");
        patch.appropriateness = patch_nullable(&body, "appropriateness");
        if let Some(v) = body.get("isDefault").and_then(Value::as_bool) {
            patch.is_default = Some(v);
        }
        if let Some(v) = body.get("replace").and_then(Value::as_bool) {
            patch.replace = Some(v);
        }
        let links = DocMountFileLinksRepository::new(mount);
        match update_vault_wardrobe_item(main, &links, &docs, &iid, &patch, Some(&cid)) {
            // v4's update echo is the full read-shaped item (the merged read row,
            // incl. `archivedAt: null`), not the write-struct — re-read for the
            // v4-exact Value shape (the reads differential's proven bytes).
            Ok(Some(_)) => {
                match wardrobe_read::find_by_id_for_character(main, &docs, &cid, &iid, &[])? {
                    Some(item) => Ok(Ok(item)),
                    None => Ok(Err(not_found("Wardrobe item"))),
                }
            }
            Ok(None) => Ok(Err(not_found("Wardrobe item"))),
            Err(e) => Ok(Err(wardrobe_err(e))),
        }
    })
    .await;
    wrap_obj(out, "wardrobeItem")
}

/// v4 `DELETE /characters/[id]/wardrobe/[itemId]` — ownership + existence
/// pre-check → `removeEquippedItemFromAllChats` cleanup → `repos.wardrobe.delete`
/// → `{ success: true }`.
pub async fn character_wardrobe_delete(
    db: &Db,
    _user_id: &str,
    character_id: &str,
    item_id: &str,
) -> Response {
    let cid = character_id.to_string();
    let iid = item_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        if let Err(r) = require_character_owned(main, mount, &cid)? {
            return Ok(Err(r));
        }
        let docs = DocMountDocumentsRepository::new(mount);
        if wardrobe_read::find_by_id_for_character(main, &docs, &cid, &iid, &[])?.is_none() {
            return Ok(Err(not_found("Wardrobe item")));
        }
        // Clean up equipped references before deleting (v4 logs + proceeds on
        // cleanup failure; composite componentItemIds are intentionally left).
        ChatOutfitsRepository::new(main).remove_equipped_item_from_all_chats(&iid)?;
        let links = DocMountFileLinksRepository::new(mount);
        match delete_vault_wardrobe_item(main, &links, &docs, &iid, Some(&cid)) {
            Ok(true) => Ok(Ok(())),
            Ok(false) => Ok(Err(not_found("Wardrobe item"))),
            Err(e) => Ok(Err(wardrobe_err(e))),
        }
    })
    .await;
    success_or(out)
}

// ===========================================================================
// Tags CRUD (v4 tags/route.ts + tags/[id]/route.ts) — all six taggable tables
// live in MAIN, so these are main-only reads/writes.
// ===========================================================================

/// Build the `_count` map + `totalUsage` for a tag (v4's 6-entity usage fan-out).
fn tag_usage(main: &rusqlite::Connection, tag_id: &str) -> Result<(Value, i64), DbError> {
    let mut counts = serde_json::Map::new();
    let mut total = 0i64;
    for (table, key) in tags::TAGGABLE_TABLES {
        let n = tags::count_tag_usage(main, table, tag_id)? as i64;
        counts.insert((*key).to_string(), json!(n));
        total += n;
    }
    Ok((Value::Object(counts), total))
}

/// The reduced tags-list DTO (v4 `tags/route.ts:58-77`): a whitelist (NO
/// `nameLower`/`userId`) with `visualStyle ?? null` ALWAYS present.
fn tag_list_dto(tag: &Value, count: Value, total: i64) -> Value {
    json!({
        "id": tag.get("id").cloned().unwrap_or(Value::Null),
        "name": tag.get("name").cloned().unwrap_or(Value::Null),
        "quickHide": tag.get("quickHide").cloned().unwrap_or(Value::Bool(false)),
        "visualStyle": tag.get("visualStyle").cloned().unwrap_or(Value::Null),
        "createdAt": tag.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": tag.get("updatedAt").cloned().unwrap_or(Value::Null),
        "_count": count,
        "totalUsage": total,
    })
}

/// v4 `GET /tags` — `findAll` → `search` filter (nameLower contains) → name-sort
/// (`localeCompare`) → per-tag usage-count DTO. `{ tags, count }`.
pub fn tag_list(db: &Db, _user_id: &str, search: Option<&str>) -> Response {
    let search = search.map(str::to_string);
    let result = db.read_main(move |main| {
        let mut all = tags::find_all(main)?;
        if let Some(s) = &search {
            let sl = s.to_lowercase();
            all.retain(|t| {
                t.get("nameLower")
                    .and_then(Value::as_str)
                    .map(|nl| nl.contains(&sl))
                    .unwrap_or(false)
            });
        }
        all.sort_by(|a, b| {
            let na = a.get("name").and_then(Value::as_str).unwrap_or("");
            let nb = b.get("name").and_then(Value::as_str).unwrap_or("");
            crate::collation::locale_compare(na, nb)
        });
        let mut out = Vec::with_capacity(all.len());
        for t in &all {
            let id = t.get("id").and_then(Value::as_str).unwrap_or("");
            let (count, total) = tag_usage(main, id)?;
            out.push(tag_list_dto(t, count, total));
        }
        Ok(out)
    });
    match result {
        Ok(list) => {
            let count = list.len();
            Response::Tags(json!({ "tags": list, "count": count }))
        }
        Err(e) => internal(e),
    }
}

/// v4 `GET /tags/[id]` — ownership → `{...tag, _count, totalUsage}` (the FULL tag
/// spread, incl. `userId`/`nameLower`).
pub fn tag_get(db: &Db, _user_id: &str, tag_id: &str) -> Response {
    let tid = tag_id.to_string();
    let result = db.read_main(move |main| {
        let tag = match tags::find_full_by_id(main, &tid)? {
            Some(t) => t,
            None => return Ok(Err(not_found("Tag"))),
        };
        let (count, total) = tag_usage(main, &tid)?;
        let mut o = tag.as_object().cloned().unwrap_or_default();
        o.insert("_count".into(), count);
        o.insert("totalUsage".into(), json!(total));
        Ok(Ok(Value::Object(o)))
    });
    match result {
        Ok(Ok(tag)) => Response::Tag(json!({ "tag": tag })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `POST /tags` — `createTagSchema` → dedup by name (return the existing tag)
/// → `create` → `{ tag }`.
pub async fn tag_create(db: &Db, user_id: &str, name: &str) -> Response {
    if name.is_empty() {
        return bad_request("Tag name is required");
    }
    let (uid, name) = (user_id.to_string(), name.to_string());
    let out = db
        .write(move |writers| {
            let main = writers.main().connection();
            // Dedup (case-insensitive) — return the existing tag, no create.
            if let Some(existing) = tags::find_by_name(main, &uid, &name)? {
                return Ok(existing);
            }
            let id = uuid::Uuid::new_v4().to_string();
            let now = crate::clock::now_iso();
            TagsRepository::new(main).create(
                &TagCreate {
                    user_id: uid.clone(),
                    name: name.clone(),
                    name_lower: None,
                    quick_hide: Some(false),
                    visual_style: None,
                },
                &TagCreateOptions {
                    id: id.clone(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            Ok(tags::find_full_by_id(main, &id)?.unwrap_or(Value::Null))
        })
        .await;
    match out {
        Ok(tag) => Response::Tag(json!({ "tag": tag })),
        Err(e) => internal(e),
    }
}

/// Parse the update body's `visualStyle` into the nullable patch triple-state
/// (absent → `None`, `null` → `Some(None)`, object → `Some(Some(parsed))` with
/// Zod-default materialization via [`TagVisualStyle`]'s serde defaults).
fn parse_visual_style(body: &Value) -> Result<Option<Option<TagVisualStyle>>, Response> {
    match body.get("visualStyle") {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(v) => match serde_json::from_value::<TagVisualStyle>(v.clone()) {
            Ok(vs) => Ok(Some(Some(vs))),
            Err(e) => Err(bad_request(format!("Invalid visualStyle: {e}"))),
        },
    }
}

/// v4 `PUT /tags/[id]` — ownership → `updateTagSchema` → name-conflict guard →
/// `update` → `{ tag }` (the merged updated tag, no `_count`).
pub async fn tag_update(db: &Db, user_id: &str, tag_id: &str, body: Value) -> Response {
    let (uid, tid) = (user_id.to_string(), tag_id.to_string());
    let visual_style = match parse_visual_style(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let out = db
        .write(move |writers| {
            let main = writers.main().connection();
            let existing = match tags::find_full_by_id(main, &tid)? {
                Some(t) => t,
                None => return Ok(Err(not_found("Tag"))),
            };
            // Rename-conflict guard (v4: only when the name actually changes).
            if let Some(new_name) = body.get("name").and_then(Value::as_str) {
                let cur = existing.get("name").and_then(Value::as_str).unwrap_or("");
                if new_name != cur {
                    if let Some(other) = tags::find_by_name(main, &uid, new_name)? {
                        if other.get("id").and_then(Value::as_str) != Some(tid.as_str()) {
                            return Ok(Err(bad_request("A tag with this name already exists")));
                        }
                    }
                }
            }
            let patch = TagUpdate {
                name: body.get("name").and_then(Value::as_str).map(str::to_string),
                quick_hide: body.get("quickHide").and_then(Value::as_bool),
                visual_style,
                updated_at: crate::clock::now_iso(),
            };
            TagsRepository::new(main).update(&tid, &patch)?;
            Ok(Ok(tags::find_full_by_id(main, &tid)?.unwrap_or(Value::Null)))
        })
        .await;
    match out {
        Ok(Ok(tag)) => Response::Tag(json!({ "tag": tag })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `DELETE /tags/[id]` — ownership → the multi-entity fan-out (remove the id
/// from every taggable table) → delete the tag → `{ success: true }`.
pub async fn tag_delete(db: &Db, _user_id: &str, tag_id: &str) -> Response {
    let tid = tag_id.to_string();
    let out = db
        .write(move |writers| {
            let main = writers.main().connection();
            if tags::find_full_by_id(main, &tid)?.is_none() {
                return Ok(Err(not_found("Tag")));
            }
            let now = crate::clock::now_iso();
            for (table, _key) in tags::TAGGABLE_TABLES {
                tags::remove_tag_from_table(main, table, &tid, &now)?;
            }
            TagsRepository::new(main).delete(&tid)?;
            Ok(Ok(()))
        })
        .await;
    match out {
        Ok(Ok(())) => Response::Tag(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// Whether `character[field]` (an array of `{id,…}`) contains an element with
/// `id == target`.
fn sub_array_contains(character: &Value, field: &str, target: &str) -> bool {
    character
        .get(field)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .any(|e| e.get("id").and_then(Value::as_str) == Some(target))
        })
        .unwrap_or(false)
}

/// Wrap a `{ key: value }` success body, or propagate the refusal.
fn wrap_obj(out: Result<Result<Value, Response>, DbError>, key: &str) -> Response {
    match out {
        Ok(Ok(v)) => Response::Character(json!({ key: v })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// `{ success: true }` on success, or propagate the refusal.
fn success_or(out: Result<Result<(), Response>, DbError>) -> Response {
    match out {
        Ok(Ok(())) => Response::Character(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}
