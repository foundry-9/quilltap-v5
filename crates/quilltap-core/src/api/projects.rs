//! The Projects-server dispatch handlers (P4.6k) — the Prospero projects
//! route-logic backfill, composed over the already-ported store-backed `projects`
//! repository + `project_doc_mount_links` + the chats/files/characters repos.
//!
//! Each handler is a differential port of a v4 projects route handler (the oracle:
//! `projects/route.ts`, `[id]/actions/{project-crud,roster,chats,state,tools}.ts`,
//! `[id]/mount-points/route.ts`) and returns a [`Response`] directly. Reads nest
//! [`Db::read_main`] + [`Db::read_mount_index`]; writes go through the
//! dual-connection writer ([`with_both_conns`]). v4's `checkOwnership` is a bare
//! existence guard (no userId filtering) → NotFound-on-absent here.

use serde_json::{json, Map, Value};

use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::document_store_overlay::OverlayError;
use crate::db::files::FilesRepository;
use crate::db::project_doc_mount_links::ProjectDocMountLinksRepository;
use crate::db::projects::{ProjectCreateInput, ProjectCreateOptions, ProjectsRepository};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, tags, DbError};
use crate::services::character_enrichment::enrich_with_default_image;
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

/// The loud "recognized but not yet available" refusal for projects-family
/// variants whose handler lands in a later P4.6k unit.
pub fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' projects action is recognized but not yet available."),
    )
}

fn overlay_to_db(e: OverlayError) -> DbError {
    match e {
        OverlayError::Db(d) => d,
        OverlayError::Unavailable { detail, .. } => DbError::Key(detail),
    }
}

fn read_both<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// v4 `x || null` (undefined/''/null → null).
fn or_null(v: Option<&str>) -> Option<String> {
    match v {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

/// The `characterRoster` string ids off a hydrated project.
fn roster_of(project: &Value) -> Vec<String> {
    project
        .get("characterRoster")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The chats (hydrated) whose `projectId` matches — v4's `chats.findAll().filter`.
fn project_chats(main: &rusqlite::Connection, project_id: &str) -> Result<Vec<Value>, DbError> {
    Ok(chats_read::find_all(main)?
        .into_iter()
        .filter(|c| c.get("projectId").and_then(Value::as_str) == Some(project_id))
        .collect())
}

/// v4 `getFilePath(file)` = `/api/v1/files/${id}`.
fn file_path(id: &str) -> String {
    format!("/api/v1/files/{id}")
}

// ===========================================================================
// List (v4 GET /api/v1/projects) — the O(n²) _count. BARE {projects}.
// ===========================================================================

pub fn project_list(db: &Db) -> Response {
    let result = read_both(db, |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let mut projects = repo.find_all().map_err(overlay_to_db)?;
        projects.sort_by(|a, b| {
            let ta = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
            let tb = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
            tb.cmp(ta)
        });
        let files = FilesRepository::new(main);
        let mut out = Vec::with_capacity(projects.len());
        for p in projects {
            let id = p
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            // v4 re-runs chats.findAll()+files.findAll() per project (O(n²)).
            let chat_count = project_chats(main, &id)?.len();
            let file_count = files.count_by_project_id(&id)? as usize;
            let char_count = roster_of(&p).len();
            let mut obj = p.as_object().cloned().unwrap_or_default();
            obj.insert(
                "_count".into(),
                json!({ "chats": chat_count, "files": file_count, "characters": char_count }),
            );
            out.push(Value::Object(obj));
        }
        Ok(out)
    });
    match result {
        Ok(projects) => Response::Project(json!({ "projects": projects })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Create (v4 POST /api/v1/projects) — default injection.
// ===========================================================================

pub async fn project_create(db: &Db, body: Value) -> Response {
    let Some(obj) = body.as_object() else {
        return bad_request("Expected a JSON object body");
    };
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if name.trim().is_empty() {
        return bad_request("Name is required");
    }
    // createProjectSchema: allowAnyCharacter prefault false, characterRoster
    // prefault []. The four literals (defaultDisabledTools/…/backgroundDisplayMode)
    // are already ProjectProperties defaults; state:{} passed explicitly.
    let allow_any = obj
        .get("allowAnyCharacter")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let roster = obj.get("characterRoster").cloned().unwrap_or(json!([]));
    let mut properties = Map::new();
    properties.insert("allowAnyCharacter".into(), Value::Bool(allow_any));
    properties.insert("characterRoster".into(), roster);
    if let Some(c) = or_null(obj.get("color").and_then(Value::as_str)) {
        properties.insert("color".into(), Value::String(c));
    }
    if let Some(i) = or_null(obj.get("icon").and_then(Value::as_str)) {
        properties.insert("icon".into(), Value::String(i));
    }
    let input = ProjectCreateInput {
        name,
        description: or_null(obj.get("description").and_then(Value::as_str)),
        instructions: or_null(obj.get("instructions").and_then(Value::as_str)),
        state: json!({}),
        properties: Value::Object(properties),
    };
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let project = repo
            .create(&input, &ProjectCreateOptions::default())
            .map_err(overlay_to_db)?;
        // Best-effort Scenarios/ folder ensure (v4 non-fatal try/catch; projects
        // ensure only Scenarios/, no Knowledge/).
        if let Some(mp) = project.get("officialMountPointId").and_then(Value::as_str) {
            let links = crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount);
            let _ = links.ensure_folder_path(mp, "Scenarios");
        }
        Ok(project)
    })
    .await;
    match out {
        Ok(project) => Response::Project(json!({ "project": project })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Get (v4 handleGetDefault) — enriched roster + _count.
// ===========================================================================

pub fn project_get(db: &Db, project_id: &str) -> Response {
    let pid = project_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        let chats = project_chats(main, &pid)?;
        let files = FilesRepository::new(main);
        let file_count = files.count_by_project_id(&pid)? as usize;
        let roster = roster_of(&project);

        let mut enriched_roster = Vec::new();
        for char_id in &roster {
            let Some(char) = characters_read::find_by_id(main, mount, char_id)? else {
                continue;
            };
            // chats in this project that include this character as a participant.
            let chat_count = chats
                .iter()
                .filter(|c| {
                    c.get("participants")
                        .and_then(Value::as_array)
                        .map(|ps| {
                            ps.iter().any(|p| {
                                p.get("characterId").and_then(Value::as_str)
                                    == Some(char_id.as_str())
                            })
                        })
                        .unwrap_or(false)
                })
                .count();
            let default_image_id = char.get("defaultImageId").and_then(Value::as_str);
            let default_image = enrich_with_default_image(main, mount, default_image_id)?;
            let mut entry = Map::new();
            entry.insert("id".into(), char.get("id").cloned().unwrap_or(Value::Null));
            entry.insert(
                "name".into(),
                char.get("name").cloned().unwrap_or(Value::Null),
            );
            // v4 `defaultImageId: char.defaultImageId` — JS omits an `undefined`
            // value on stringify; the overlay leaves the key absent/null when the
            // character has no avatar, so mirror the present-non-null rule.
            match char.get("defaultImageId") {
                Some(v) if !v.is_null() => {
                    entry.insert("defaultImageId".into(), v.clone());
                }
                _ => {}
            }
            entry.insert(
                "defaultImage".into(),
                serde_json::to_value(default_image).unwrap_or(Value::Null),
            );
            entry.insert(
                "tags".into(),
                char.get("tags").cloned().unwrap_or(json!([])),
            );
            entry.insert("chatCount".into(), json!(chat_count));
            enriched_roster.push(Value::Object(entry));
        }

        let mut obj = project.as_object().cloned().unwrap_or_default();
        obj.insert("characterRoster".into(), Value::Array(enriched_roster));
        obj.insert(
            "_count".into(),
            json!({ "chats": chats.len(), "files": file_count, "characters": roster.len() }),
        );
        Ok(Some(Value::Object(obj)))
    });
    match result {
        Ok(Some(project)) => Response::Project(json!({ "project": project })),
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Update / delete
// ===========================================================================

pub async fn project_update(db: &Db, project_id: &str, patch: Value) -> Response {
    let pid = project_id.to_string();
    let Some(patch_map) = patch.as_object().cloned() else {
        return bad_request("Expected a JSON object body");
    };
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        repo.update(&pid, &patch_map).map_err(overlay_to_db)
    })
    .await;
    match out {
        Ok(Some(project)) => Response::Project(json!({ "project": project })),
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

/// v4 `handleDeleteProject`: null `projectId` on every associated chat + file,
/// delete the project row. **Does NOT touch `projectDocMountLinks`** (dangling
/// links are filtered at read time). The official store is orphaned.
pub async fn project_delete(db: &Db, project_id: &str) -> Response {
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(false);
        }
        // v4 does `chats.update(id, {projectId:null})` / `files.update(...)` per row
        // (minting updatedAt). The route echoes only `{success:true}` and the
        // differential dumps (id, projectId), so a single UPDATE per table is
        // behaviourally identical for the observed surface.
        main.execute(
            "UPDATE chats SET projectId = NULL WHERE projectId = ?1",
            rusqlite::params![pid],
        )?;
        main.execute(
            "UPDATE files SET projectId = NULL WHERE projectId = ?1",
            rusqlite::params![pid],
        )?;
        repo.delete(&pid)?;
        Ok(true)
    })
    .await;
    match out {
        Ok(true) => Response::Project(json!({ "success": true })),
        Ok(false) => not_found("Project"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Roster (v4 roster.ts) — hand-rolled via projects.update (quirk 14).
// ===========================================================================

/// v4 `handleListCharacters`: roster → `{id,name,tags}`, null-filtered. Body
/// `{ characters, count }`.
pub fn project_character_list(db: &Db, project_id: &str) -> Response {
    let pid = project_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        let mut chars = Vec::new();
        for char_id in roster_of(&project) {
            if let Some(char) = characters_read::find_by_id(main, mount, &char_id)? {
                chars.push(json!({
                    "id": char.get("id").cloned().unwrap_or(Value::Null),
                    "name": char.get("name").cloned().unwrap_or(Value::Null),
                    "tags": char.get("tags").cloned().unwrap_or(json!([])),
                }));
            }
        }
        Ok(Some(chars))
    });
    match result {
        Ok(Some(chars)) => {
            let count = chars.len();
            Response::Project(json!({ "characters": chars, "count": count }))
        }
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

/// v4 `handleAddCharacter`: ownership('Project') → `characters.findById` (missing
/// → `notFound('Character')`) → idempotent add (no write if already present).
pub async fn project_character_add(db: &Db, project_id: &str, character_id: &str) -> Response {
    let pid = project_id.to_string();
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(Err(not_found("Project")));
        };
        if characters_read::find_by_id(main, mount, &cid)?.is_none() {
            return Ok(Err(not_found("Character")));
        }
        let mut roster = roster_of(&project);
        if !roster.iter().any(|c| c == &cid) {
            roster.push(cid.clone());
            let mut patch = Map::new();
            patch.insert(
                "characterRoster".into(),
                Value::Array(roster.into_iter().map(Value::String).collect()),
            );
            repo.update(&pid, &patch).map_err(overlay_to_db)?;
        }
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Project(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `handleRemoveCharacter`: ownership → filter the id out and ALWAYS write
/// (never 404s on a missing member). Body `{ success: true }`.
pub async fn project_character_remove(db: &Db, project_id: &str, character_id: &str) -> Response {
    let pid = project_id.to_string();
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(false);
        };
        let roster: Vec<String> = roster_of(&project)
            .into_iter()
            .filter(|c| c != &cid)
            .collect();
        let mut patch = Map::new();
        patch.insert(
            "characterRoster".into(),
            Value::Array(roster.into_iter().map(Value::String).collect()),
        );
        repo.update(&pid, &patch).map_err(overlay_to_db)?;
        Ok(true)
    })
    .await;
    match out {
        Ok(true) => Response::Project(json!({ "success": true })),
        Ok(false) => not_found("Project"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Chats (v4 chats.ts)
// ===========================================================================

/// v4 `handleListChats`: paginated, `lastMessageAt ?? updatedAt` sort fallback,
/// enriched participants/tags/storyBackground. Body `{ chats, pagination }`.
pub fn project_chat_list(
    db: &Db,
    project_id: &str,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Response {
    use crate::clock::iso_to_ms;
    let pid = project_id.to_string();
    let limit = limit.unwrap_or(20);
    let offset = offset.unwrap_or(0);
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        let mut chats = project_chats(main, &pid)?;
        // Sort desc by lastMessageAt ?? updatedAt (v4 `new Date(...).getTime()`).
        chats.sort_by(|a, b| {
            let ta = a
                .get("lastMessageAt")
                .and_then(Value::as_str)
                .or_else(|| a.get("updatedAt").and_then(Value::as_str))
                .and_then(iso_to_ms)
                .unwrap_or(0);
            let tb = b
                .get("lastMessageAt")
                .and_then(Value::as_str)
                .or_else(|| b.get("updatedAt").and_then(Value::as_str))
                .and_then(iso_to_ms)
                .unwrap_or(0);
            tb.cmp(&ta)
        });
        let total = chats.len();
        let start = offset.max(0) as usize;
        let page: Vec<&Value> = if start >= chats.len() || limit <= 0 {
            Vec::new()
        } else {
            let end = start.saturating_add(limit as usize).min(chats.len());
            chats[start..end].iter().collect()
        };

        let files = FilesRepository::new(main);
        let mut enriched = Vec::with_capacity(page.len());
        for chat in &page {
            // Participants → {id, name, defaultImage, tags}; drop non-character.
            let mut participants = Vec::new();
            if let Some(ps) = chat.get("participants").and_then(Value::as_array) {
                for p in ps {
                    let Some(char_id) = p.get("characterId").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(char) = characters_read::find_by_id(main, mount, char_id)? else {
                        continue;
                    };
                    let default_image = enrich_with_default_image(
                        main,
                        mount,
                        char.get("defaultImageId").and_then(Value::as_str),
                    )?;
                    participants.push(json!({
                        "id": p.get("id").cloned().unwrap_or(Value::Null),
                        "name": char.get("name").cloned().unwrap_or(Value::Null),
                        "defaultImage": serde_json::to_value(default_image).unwrap_or(Value::Null),
                        "tags": char.get("tags").cloned().unwrap_or(json!([])),
                    }));
                }
            }
            // Tags → {tag:{id,name}}, dropping missing.
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
            let chat_tags: Vec<Value> = tag_rows
                .into_iter()
                .map(|t| json!({ "tag": { "id": t.id, "name": t.name } }))
                .collect();
            // Story background.
            let story_background = match chat.get("storyBackgroundImageId").and_then(Value::as_str)
            {
                Some(bg) if !bg.is_empty() => match files.find_by_id(bg)? {
                    Some(f) => json!({ "id": f.id, "filepath": file_path(&f.id) }),
                    None => Value::Null,
                },
                _ => Value::Null,
            };
            enriched.push(json!({
                "id": chat.get("id").cloned().unwrap_or(Value::Null),
                "title": chat.get("title").cloned().unwrap_or(Value::Null),
                "messageCount": chat.get("messageCount").cloned().unwrap_or(Value::Null),
                "participants": participants,
                "tags": chat_tags,
                "storyBackground": story_background,
                "isDangerousChat": chat.get("isDangerousChat").and_then(Value::as_bool) == Some(true),
                "lastMessageAt": chat.get("lastMessageAt").cloned().unwrap_or(Value::Null),
                "updatedAt": chat.get("updatedAt").cloned().unwrap_or(Value::Null),
                "createdAt": chat.get("createdAt").cloned().unwrap_or(Value::Null),
            }));
        }
        let has_more = (offset as usize).saturating_add(enriched.len()) < total;
        Ok(Some(json!({
            "chats": enriched,
            "pagination": { "total": total, "offset": offset, "limit": limit, "hasMore": has_more },
        })))
    });
    match result {
        Ok(Some(body)) => Response::Project(body),
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

/// v4 `handleAddChat`: ownership('Project') → `chats.findById` (missing →
/// `notFound('Chat')`) → set the chat's projectId. Body `{ success: true }`.
pub async fn project_chat_add(db: &Db, project_id: &str, chat_id: &str) -> Response {
    let pid = project_id.to_string();
    let cid = chat_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Project")));
        }
        if chats_read::find_by_id(main, &cid)?.is_none() {
            return Ok(Err(not_found("Chat")));
        }
        main.execute(
            "UPDATE chats SET projectId = ?1 WHERE id = ?2",
            rusqlite::params![pid, cid],
        )?;
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Project(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `handleRemoveChat`: ownership → null the chat's projectId (no existence
/// check on the chat). Body `{ success: true }`.
pub async fn project_chat_remove(db: &Db, project_id: &str, chat_id: &str) -> Response {
    let pid = project_id.to_string();
    let cid = chat_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(false);
        }
        main.execute(
            "UPDATE chats SET projectId = NULL WHERE id = ?1",
            rusqlite::params![cid],
        )?;
        Ok(true)
    })
    .await;
    match out {
        Ok(true) => Response::Project(json!({ "success": true })),
        Ok(false) => not_found("Project"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// State (v4 state.ts + state-handlers.ts)
// ===========================================================================

/// v4 `handleGetState`: `{ success: true, state }`.
pub fn project_state_get(db: &Db, project_id: &str) -> Response {
    let pid = project_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        Ok(Some(project.get("state").cloned().unwrap_or(json!({}))))
    });
    match result {
        Ok(Some(state)) => Response::Project(json!({ "success": true, "state": state })),
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

/// v4 set-state: REPLACES wholesale. Body `{ success, state: updated?.state ?? validated }`.
pub async fn project_state_set(db: &Db, project_id: &str, state: Value) -> Response {
    let pid = project_id.to_string();
    let state_clone = state.clone();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        let mut patch = Map::new();
        patch.insert("state".into(), state_clone.clone());
        let updated = repo.update(&pid, &patch).map_err(overlay_to_db)?;
        let stored = updated
            .as_ref()
            .and_then(|p| p.get("state").cloned())
            .unwrap_or(state_clone);
        Ok(Some(stored))
    })
    .await;
    match out {
        Ok(Some(state)) => Response::Project(json!({ "success": true, "state": state })),
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

/// v4 reset-state: `{ success, previousState }`.
pub async fn project_state_reset(db: &Db, project_id: &str) -> Response {
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        let previous = project.get("state").cloned().unwrap_or(json!({}));
        let mut patch = Map::new();
        patch.insert("state".into(), json!({}));
        repo.update(&pid, &patch).map_err(overlay_to_db)?;
        Ok(Some(previous))
    })
    .await;
    match out {
        Ok(Some(previous)) => {
            Response::Project(json!({ "success": true, "previousState": previous }))
        }
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Tool settings (v4 tools.ts)
// ===========================================================================

/// v4 `handleUpdateToolSettings`: ownership → update the two arrays. Body
/// `{ success, defaultDisabledTools, defaultDisabledToolGroups }`.
pub async fn project_tool_settings_update(
    db: &Db,
    project_id: &str,
    disabled_tools: Vec<String>,
    disabled_tool_groups: Vec<String>,
) -> Response {
    let pid = project_id.to_string();
    let dt = disabled_tools.clone();
    let dtg = disabled_tool_groups.clone();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(false);
        }
        let mut patch = Map::new();
        patch.insert(
            "defaultDisabledTools".into(),
            Value::Array(dt.into_iter().map(Value::String).collect()),
        );
        patch.insert(
            "defaultDisabledToolGroups".into(),
            Value::Array(dtg.into_iter().map(Value::String).collect()),
        );
        repo.update(&pid, &patch).map_err(overlay_to_db)?;
        Ok(true)
    })
    .await;
    match out {
        Ok(true) => Response::Project(json!({
            "success": true,
            "defaultDisabledTools": disabled_tools,
            "defaultDisabledToolGroups": disabled_tool_groups,
        })),
        Ok(false) => not_found("Project"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Background (v4 background.ts) — BARE {backgroundUrl, displayMode, sourceChatId?}
// ===========================================================================

/// v4 `handleGetBackground`: URL resolution by `backgroundDisplayMode`. BARE
/// envelope. `theme` (or any unresolvable mode) → `{ backgroundUrl: null,
/// displayMode }`; `static`/`project` resolve their image id; `latest_chat`
/// scans project chats (desc updatedAt) for the first with a story background.
pub fn project_background_get(db: &Db, project_id: &str) -> Response {
    use crate::clock::iso_to_ms;
    let pid = project_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        let mode = project
            .get("backgroundDisplayMode")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("theme")
            .to_string();
        let files = FilesRepository::new(main);
        let by_id = |img: Option<&str>| -> Result<Option<String>, DbError> {
            match img {
                Some(id) if !id.is_empty() => Ok(files.find_by_id(id)?.map(|f| file_path(&f.id))),
                _ => Ok(None),
            }
        };
        let body = match mode.as_str() {
            "static" => match by_id(
                project
                    .get("staticBackgroundImageId")
                    .and_then(Value::as_str),
            )? {
                Some(url) => json!({ "backgroundUrl": url, "displayMode": mode }),
                None => json!({ "backgroundUrl": null, "displayMode": mode }),
            },
            "project" => {
                match by_id(
                    project
                        .get("storyBackgroundImageId")
                        .and_then(Value::as_str),
                )? {
                    Some(url) => json!({ "backgroundUrl": url, "displayMode": mode }),
                    None => json!({ "backgroundUrl": null, "displayMode": mode }),
                }
            }
            "latest_chat" => {
                let mut chats: Vec<Value> = project_chats(main, &pid)?
                    .into_iter()
                    .filter(|c| {
                        c.get("storyBackgroundImageId")
                            .and_then(Value::as_str)
                            .map(|s| !s.is_empty())
                            .unwrap_or(false)
                    })
                    .collect();
                chats.sort_by(|a, b| {
                    let ta = a
                        .get("updatedAt")
                        .and_then(Value::as_str)
                        .and_then(iso_to_ms)
                        .unwrap_or(0);
                    let tb = b
                        .get("updatedAt")
                        .and_then(Value::as_str)
                        .and_then(iso_to_ms)
                        .unwrap_or(0);
                    tb.cmp(&ta)
                });
                match chats.first() {
                    Some(chat) => {
                        let bg = chat.get("storyBackgroundImageId").and_then(Value::as_str);
                        match by_id(bg)? {
                            Some(url) => json!({
                                "backgroundUrl": url,
                                "displayMode": mode,
                                "sourceChatId": chat.get("id").cloned().unwrap_or(Value::Null),
                            }),
                            None => json!({ "backgroundUrl": null, "displayMode": mode }),
                        }
                    }
                    None => json!({ "backgroundUrl": null, "displayMode": mode }),
                }
            }
            // theme + any fallthrough.
            _ => json!({ "backgroundUrl": null, "displayMode": mode }),
        };
        Ok(Some(body))
    });
    match result {
        Ok(Some(body)) => Response::Project(body),
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Aesthetic (v4 aesthetic.ts + lib/image-gen/aesthetic.ts)
// ===========================================================================

fn aesthetic_filename(kind: &str) -> Option<&'static str> {
    match kind {
        "lantern" => Some(crate::services::aesthetics::LANTERN_AESTHETICS_FILENAME),
        "aurora" => Some(crate::services::aesthetics::AURORA_AESTHETICS_FILENAME),
        _ => None,
    }
}

/// v4 `handleGetAesthetic`: ownership → kind guard → if no official store `{content:''}`
/// → `readAesthetic` (single tier, absent/error → ''). Body `{ content }`.
pub fn project_aesthetic_get(db: &Db, project_id: &str, kind: &str) -> Response {
    let Some(filename) = aesthetic_filename(kind) else {
        return bad_request("Query param \"kind\" must be \"lantern\" or \"aurora\"");
    };
    let pid = project_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        let Some(mp) = project.get("officialMountPointId").and_then(Value::as_str) else {
            return Ok(Some(String::new()));
        };
        // readStoreFile (the editor route path): `doc?.content ?? ''` — the RAW
        // content (NOT trimmed; the trim is only the aesthetic-injection path).
        // Absent doc or any read error → '' (swallowed).
        let content = match crate::db::database_store::read_database_document(mount, mp, filename) {
            Ok(doc) => doc.content,
            Err(_) => String::new(),
        };
        Ok(Some(content))
    });
    match result {
        Ok(Some(content)) => Response::Project(json!({ "content": content })),
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

/// v4 `handlePutAesthetic`: ownership → kind guard → no official store → 500 →
/// `writeAesthetic` (empty/whitespace content DELETES the file). Body
/// `{ success: true }`. (Malformed-JSON-body-means-delete is a transport concern;
/// the boundary receives the parsed `content` already.)
pub async fn project_aesthetic_set(
    db: &Db,
    project_id: &str,
    kind: &str,
    content: Option<String>,
) -> Response {
    let Some(filename) = aesthetic_filename(kind) else {
        return bad_request("Query param \"kind\" must be \"lantern\" or \"aurora\"");
    };
    // safeParse(...).data?.content ?? '' — a missing/invalid content → ''.
    let content = content.unwrap_or_default();
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(Err(not_found("Project")));
        };
        let Some(mp) = project.get("officialMountPointId").and_then(Value::as_str) else {
            return Ok(Err(internal(
                "Project has no official document store to write the aesthetic into",
            )));
        };
        // writeStoreFile: `!content || !content.trim()` → delete; else write.
        if crate::jsstr::js_trim(&content).is_empty() {
            let _ = crate::db::database_store::delete_database_document(mount, mp, filename)?;
        } else {
            crate::db::database_store::write_database_document(mount, mp, filename, &content)
                .map_err(|e| DbError::Key(e.to_string()))?;
        }
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Project(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Mount points (v4 [id]/mount-points/route.ts)
// ===========================================================================

/// v4 GET `/mount-points`: BARE `{ mountPoints }`, dangling links filtered.
pub fn project_mount_point_list(db: &Db, project_id: &str) -> Response {
    let pid = project_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        let link_ids = ProjectDocMountLinksRepository::new(mount).find_by_project_id(&pid)?;
        let points = DocMountPointsRepository::new(mount);
        let mut mps = Vec::new();
        for mp_id in link_ids {
            if let Some(mp) = points.find_full_json_by_id(&mp_id)? {
                mps.push(mp);
            }
        }
        Ok(Some(mps))
    });
    match result {
        Ok(Some(mps)) => Response::Project(json!({ "mountPoints": mps })),
        Ok(None) => not_found("Project"),
        Err(e) => internal(e),
    }
}

/// v4 POST `/mount-points`: ownership → mount-point existence → link. Body
/// `{ link, mountPoint }`.
pub async fn project_mount_point_link(db: &Db, project_id: &str, mount_point_id: &str) -> Response {
    let pid = project_id.to_string();
    let mp_id = mount_point_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Project")));
        }
        let points = DocMountPointsRepository::new(mount);
        let Some(mp) = points.find_full_json_by_id(&mp_id)? else {
            return Ok(Err(not_found("Mount point")));
        };
        let link = ProjectDocMountLinksRepository::new(mount).link_returning(&pid, &mp_id)?;
        Ok(Ok((link, mp)))
    })
    .await;
    match out {
        Ok(Ok((link, mount_point))) => {
            Response::Project(json!({ "link": link, "mountPoint": mount_point }))
        }
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 DELETE `/mount-points`: ownership → unlink; no link → `badRequest`. Body
/// `{ message: 'Mount point unlinked from project' }`.
pub async fn project_mount_point_unlink(
    db: &Db,
    project_id: &str,
    mount_point_id: &str,
) -> Response {
    let pid = project_id.to_string();
    let mp_id = mount_point_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Project")));
        }
        let unlinked = ProjectDocMountLinksRepository::new(mount).unlink(&pid, &mp_id)?;
        if !unlinked {
            return Ok(Err(bad_request(
                "No link exists between this project and mount point",
            )));
        }
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Project(json!({ "message": "Mount point unlinked from project" })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}
