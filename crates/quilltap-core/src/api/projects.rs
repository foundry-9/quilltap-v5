//! The Projects-server dispatch handlers (P4.6k) — the Prospero projects
//! route-logic backfill, composed over the already-ported store-backed `projects`
//! repository + `project_doc_mount_links` + the chats/files/characters repos.
//!
//! Each handler is a differential port of a v4 projects route handler (the oracle:
//! `projects/route.ts`, `[id]/actions/{project-crud,roster,chats,state,tools}.ts`,
//! `[id]/mount-points/route.ts`) and returns a [`Response`] directly. Reads nest
//! [`Db::read_main`] + [`Db::read_mount_index`]; writes go through the
//! dual-connection writer ([`with_both_conns`]). v4's ownership guard
//! (`checkOwnership` until `55752ad4` renamed it `exists`) is a bare
//! existence guard (no userId filtering) → NotFound-on-absent here.

use serde_json::{json, Map, Value};

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::document_store_overlay::OverlayError;
use crate::db::ensure_official_store::ensure_official_store;
use crate::db::files::{FileUpdate, FilesRepository};
use crate::db::project_doc_mount_links::ProjectDocMountLinksRepository;
use crate::db::project_store_naming::pick_primary_project_store;
use crate::db::projects::{
    find_official_mount_point_id_raw, ProjectCreateInput, ProjectCreateOptions, ProjectEntity,
    ProjectsRepository,
};
use crate::db::runtime::Db;
use crate::db::vault_wardrobe_public::{
    create_project_wardrobe_item, delete_project_wardrobe_item, update_project_wardrobe_item,
    WardrobePatch, WardrobePublicError,
};
use crate::db::{archetype_wardrobe, characters_read, chats_read, tags, DbError};
use crate::services::character_enrichment::enrich_with_default_image;
use crate::services::dangerous_content::chat_override::get_concierge_state;
use crate::services::image_job_common::with_both_conns;
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe_instructions::{
    read_wardrobe_instructions_file, write_wardrobe_instructions_file,
};

use super::scenarios as scenarios_api;
use super::types::{db_error_response, ErrorKind, Response};

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
    // Structure-preserving (P4.23): the `Unavailable` refusal survives as
    // `DbError::StoreUnavailable` so the terminal arm can answer v4's
    // contextful 503 instead of a 500 + leaked detail.
    e.into_db()
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
        Err(e) => db_error_response(e),
    }
}

/// The `error` sentence v4's middleware emits for any uncaught `ZodError`
/// (`lib/api/responses.ts:113` via `middleware/context.ts:166`). The schema's
/// own per-issue messages — `'Name is required'` among them — only ever appear
/// inside the `details` array, which is the standing project-wide deferral;
/// they are NOT the top-level sentence.
const VALIDATION_ERROR: &str = "Validation error";

/// v4 `createProjectSchema` (`app/api/v1/projects/schemas.ts` — the `c93ec7ff`
/// bug-98 fix lifted it out of `route.ts` and made the four presentational
/// fields `.nullable()`) in declaration order:
/// `(key, rule, nullable, required)`.
///
/// `required` is v4's *absence* of `.optional()`: `name` is the only one, and a
/// missing `name` is Zod's `invalid_type` issue. `nullable` is `.nullable()`;
/// bug 98 is exactly the four `true`s below — the create dialogs send
/// `description || null` for a blank field, and the OLD schema's plain
/// `.optional()` refused the whole project over it. (The handler coerces falsy
/// to null afterwards either way.)
///
/// **Measured, not assumed** (v4's real schema at `f6a10055`, old vs new):
/// only the four `null` legs moved — `null` description/instructions/color/icon
/// went 400 → 200. Every length, regex, uuid and type leg is unchanged, and
/// `""` was always legal. v5's hand-rolled create enforced NONE of them, and
/// additionally rejected a whitespace-only name that v4 accepts (`.min(1)` runs
/// on the RAW string; there is no `.trim()` in this schema).
const PROJECT_CREATE_SCHEMA: &[(&str, FieldRule, bool, bool)] = &[
    ("name", FieldRule::Str(1, 100), false, true),
    ("description", FieldRule::Str(0, 2000), true, false),
    ("instructions", FieldRule::Str(0, 10_000), true, false),
    ("allowAnyCharacter", FieldRule::Bool, false, false),
    ("characterRoster", FieldRule::UuidArray, false, false),
    ("color", FieldRule::Color, true, false),
    ("icon", FieldRule::Str(0, 50), true, false),
];

/// `createProjectSchema.parse(body)` reduced to its verdict — the handler reads
/// the recognized keys itself, which is `z.object`'s unknown-key strip by
/// construction.
fn validate_project_create(body: &Value) -> Result<(), ()> {
    // Zod's root arm: a non-object body is its own `invalid_type` issue, so it
    // reaches the SAME flat sentence as a bad field (v5 used to answer its own
    // invented `Expected a JSON object body` here).
    let Some(obj) = body.as_object() else {
        return Err(());
    };
    for (key, rule, nullable, required) in PROJECT_CREATE_SCHEMA {
        match obj.get(*key) {
            None => {
                if *required {
                    return Err(());
                }
            }
            Some(Value::Null) => {
                if !nullable {
                    return Err(());
                }
            }
            Some(v) => {
                if !field_value_ok(rule, v) {
                    return Err(());
                }
            }
        }
    }
    Ok(())
}

// ===========================================================================
// Create (v4 POST /api/v1/projects) — default injection.
// ===========================================================================

pub async fn project_create(db: &Db, body: Value) -> Response {
    // `createProjectSchema.parse(body)` (v4 `projects/route.ts:62`) — the parse
    // is UNCAUGHT, so any failure escapes into `handleRouteError` as the flat
    // 400 `{error: 'Validation error'}`. See PROJECT_CREATE_SCHEMA.
    if validate_project_create(&body).is_err() {
        return bad_request(VALIDATION_ERROR);
    }
    let obj = body.as_object().expect("validated as an object");
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
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
        Err(e) => db_error_response(e),
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
        // v4's `handleGetProject` wraps its WHOLE body in a local try/catch →
        // a FIXED `serverError('Failed to fetch project')` — so the middleware's
        // store-unavailable 503 never fires on the project GET (the routes
        // differential's `get_store_corrupt` arm pins this: 500 + this exact
        // body, unlike the PUT, which propagates and answers the 503).
        Err(e) => {
            tracing::error!(error = %e, "project GET failed");
            internal("Failed to fetch project")
        }
    }
}

// ===========================================================================
// Update / delete
// ===========================================================================

/// One field of v4's `updateProjectSchema`
/// (`app/api/v1/projects/[id]/schemas.ts:9-24`).
enum FieldRule {
    /// `z.boolean()`
    Bool,
    /// `z.string()` with an inclusive `[min, max]` length in UTF-16 code units
    /// (Zod measures `String.length`).
    Str(usize, usize),
    /// `z.string().regex(/^#(?:[0-9a-fA-F]{3}){1,2}$/)` — the hex colour.
    Color,
    /// `z.string().uuid()` (Zod 4's deprecated alias for `z.uuid()` — probed
    /// against v4's zod 4.4.3, the two agree on every arm).
    Uuid,
    /// `z.enum([...])`
    Enum(&'static [&'static str]),
    /// `z.array(z.uuid())`
    UuidArray,
}

/// v4 `updateProjectSchema` in declaration order: `(key, rule, nullable)`.
/// `nullable` is v4's `.nullable()`; every field is additionally `.optional()`,
/// so ABSENT is always legal and a present `null` is legal only where the third
/// column says so.
const PROJECT_UPDATE_SCHEMA: &[(&str, FieldRule, bool)] = &[
    ("name", FieldRule::Str(1, 100), false),
    ("description", FieldRule::Str(0, 2000), true),
    ("instructions", FieldRule::Str(0, 10_000), true),
    ("allowAnyCharacter", FieldRule::Bool, false),
    ("characterRoster", FieldRule::UuidArray, false),
    ("color", FieldRule::Color, true),
    ("icon", FieldRule::Str(0, 50), true),
    ("defaultAgentModeEnabled", FieldRule::Bool, true),
    ("defaultAvatarGenerationEnabled", FieldRule::Bool, true),
    ("defaultImageProfileId", FieldRule::Uuid, true),
    ("defaultRoleplayTemplateId", FieldRule::Uuid, true),
    (
        "defaultAlertCharactersOfLanternImages",
        FieldRule::Bool,
        true,
    ),
    (
        "answerConfirmationOverride",
        FieldRule::Enum(&["ON", "OFF"]),
        true,
    ),
    // [70505745a] Narrowed to the two modes that work; `project` and `static`
    // are refused here (the schema is the write gate, not a coercion point —
    // the coercion lives in `ProjectEntity::parse_properties` so a pre-4.9
    // import or restore still lands on a valid value).
    (
        "backgroundDisplayMode",
        FieldRule::Enum(&["latest_chat", "theme"]),
        false,
    ),
];

/// v4's hex-colour regex, `/^#(?:[0-9a-fA-F]{3}){1,2}$/` — `#abc` or `#abcdef`,
/// nothing between and nothing longer.
fn is_hex_color(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('#') else {
        return false;
    };
    matches!(rest.len(), 3 | 6) && rest.bytes().all(|b| b.is_ascii_hexdigit())
}

fn field_value_ok(rule: &FieldRule, v: &Value) -> bool {
    match rule {
        FieldRule::Bool => v.is_boolean(),
        FieldRule::Str(min, max) => v
            .as_str()
            .map(|s| s.encode_utf16().count())
            .is_some_and(|n| n >= *min && n <= *max),
        FieldRule::Color => v.as_str().is_some_and(is_hex_color),
        FieldRule::Uuid => v.as_str().is_some_and(super::chat_outfits::is_zod_uuid),
        FieldRule::Enum(members) => v.as_str().is_some_and(|s| members.contains(&s)),
        FieldRule::UuidArray => v.as_array().is_some_and(|a| {
            a.iter()
                .all(|e| e.as_str().is_some_and(super::chat_outfits::is_zod_uuid))
        }),
    }
}

/// P4.55 (the merge-verb silent-keep sweep): v4 runs
/// `updateProjectSchema.parse(body)` and hands the repository the PARSED data
/// (`[id]/actions/project-crud.ts:95-97`), so an invalid field is a 400
/// `Validation error` (the ZodError escapes the handler and
/// `handleRouteError` maps it) and an unknown key is STRIPPED, never written.
/// v5 used to pass the raw body straight through with no validation at all.
///
/// Returns the stripped patch in schema-declaration order (the order Zod's own
/// parsed output carries), or `Err(())` for the 400.
fn validate_project_patch(patch: &Value) -> Result<Map<String, Value>, ()> {
    let Some(obj) = patch.as_object() else {
        // Zod's root arm: a non-object body is its own `invalid_type` issue.
        return Err(());
    };
    let mut out = Map::new();
    for (key, rule, nullable) in PROJECT_UPDATE_SCHEMA {
        let Some(v) = obj.get(*key) else { continue };
        if v.is_null() {
            if !nullable {
                return Err(());
            }
        } else if !field_value_ok(rule, v) {
            return Err(());
        }
        out.insert((*key).to_string(), v.clone());
    }
    Ok(out)
}

/// The three terminal shapes of `project_update`'s one writer round-trip. v4
/// checks existence FIRST (`findById` → `notFound`) and only then reads and
/// parses the body, so a missing project answers 404 even for a garbage patch.
enum ProjectUpdateOutcome {
    NotFound,
    Invalid,
    Updated(Value),
}

pub async fn project_update(db: &Db, project_id: &str, patch: Value) -> Response {
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(ProjectUpdateOutcome::NotFound);
        }
        let Ok(patch_map) = validate_project_patch(&patch) else {
            return Ok(ProjectUpdateOutcome::Invalid);
        };
        Ok(
            match repo.update(&pid, &patch_map).map_err(overlay_to_db)? {
                Some(project) => ProjectUpdateOutcome::Updated(project),
                None => ProjectUpdateOutcome::NotFound,
            },
        )
    })
    .await;
    match out {
        Ok(ProjectUpdateOutcome::Updated(project)) => {
            Response::Project(json!({ "project": project }))
        }
        Ok(ProjectUpdateOutcome::NotFound) => not_found("Project"),
        // v4 `validationError(...)` — the `details` issues array is the
        // recorded no-details divergence class.
        Ok(ProjectUpdateOutcome::Invalid) => bad_request(VALIDATION_ERROR),
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        let Some(character) = characters_read::find_by_id(main, mount, &cid)? else {
            return Ok(Err(not_found("Character")));
        };
        // Archived characters can't join a roster — but existing roster edges
        // are never removed by archiving, so removal stays unguarded
        // (spec §5.1, v4 `d553f72a`).
        if crate::api::characters::is_archived(&character) {
            return Ok(Err(bad_request(
                "That character is archived; rehydrate them before adding them to a roster.",
            )));
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Chats (v4 chats.ts)
// ===========================================================================

/// v4 `handleListChats`: paginated, `lastMessageAt ?? createdAt` activity sort,
/// enriched participants/tags/storyBackground. Body `{ chats, pagination }`.
pub fn project_chat_list(
    db: &Db,
    project_id: &str,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Response {
    let pid = project_id.to_string();
    let limit = limit.unwrap_or(20);
    let offset = offset.unwrap_or(0);
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        let mut chats = project_chats(main, &pid)?;
        // Newest conversational activity first — see `crate::chat_activity`
        // (v4 `735d9408c`).
        chats.sort_by(crate::chat_activity::by_chat_activity_desc);
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
                // v4 `c43d3b1b4`: the DERIVED state + the classifier's
                // categories, in the raw label's slot.
                "conciergeState": get_concierge_state(Some(chat)).as_str(),
                "dangerCategories": chat
                    .get("dangerCategories")
                    .filter(|v| v.is_array())
                    .cloned()
                    .unwrap_or_else(|| json!([])),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Background (v4 background.ts) — BARE {backgroundUrl, displayMode, sourceChatId?}
// ===========================================================================

/// v4 `handleGetBackground`: URL resolution by `backgroundDisplayMode`. BARE
/// envelope. The mode is NORMALIZED first ([70505745a]), so `theme` — including
/// every retired or unrecognised stored value — answers
/// `{ backgroundUrl: null, displayMode: 'theme' }`; `latest_chat` scans project
/// chats (desc updatedAt) for the first with a story background.
pub fn project_background_get(db: &Db, project_id: &str) -> Response {
    use crate::clock::iso_to_ms;
    let pid = project_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(&pid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        // [70505745a] `normalizeBackgroundDisplayMode(project.backgroundDisplayMode)
        // ?? 'theme'`. A project stored in a mode retired in 4.9 ('project',
        // 'static') reads back as 'theme'; the properties schema coerces it, and
        // this guards a raw row that bypassed the overlay. (v4's pre-fix
        // `|| 'theme'` truthiness and this normalize agree on the empty string:
        // `''` is unrecognised, so it too becomes 'theme'.)
        let mode = crate::db::projects::normalize_background_display_mode(
            project.get("backgroundDisplayMode"),
        )
        .unwrap_or("theme")
        .to_string();
        let files = FilesRepository::new(main);
        let by_id = |img: Option<&str>| -> Result<Option<String>, DbError> {
            match img {
                Some(id) if !id.is_empty() => Ok(files.find_by_id(id)?.map(|f| file_path(&f.id))),
                _ => Ok(None),
            }
        };
        // [70505745a] The `'static'` and `'project'` resolution branches are
        // DELETED, not merely unreachable: `'project'` read a field only the
        // `'latest_chat'` path ever wrote, and `'static'` read a field nothing
        // anywhere wrote. Both image-id fields are KEPT — `storyBackgroundImageId`
        // is still written for a project in Latest chat mode.
        let body = match mode.as_str() {
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
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Aesthetic (v4 aesthetic.ts + lib/image-gen/aesthetic.ts)
// ===========================================================================

use crate::services::aesthetics::{
    aesthetic_filename_for_kind, read_aesthetic_file, write_aesthetic_file,
};

/// v4 `handleGetAesthetic`: ownership → kind guard → if no official store `{content:''}`
/// → `readAesthetic` (single tier, absent/error → ''). Body `{ content }`.
pub fn project_aesthetic_get(db: &Db, project_id: &str, kind: &str) -> Response {
    let Some(filename) = aesthetic_filename_for_kind(kind) else {
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
        // readStoreFile (the editor route path): the RAW content, absent doc or
        // read error → '' — shared with the system pair.
        Ok(Some(read_aesthetic_file(mount, mp, filename)))
    });
    match result {
        Ok(Some(content)) => Response::Project(json!({ "content": content })),
        Ok(None) => not_found("Project"),
        Err(e) => db_error_response(e),
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
    let Some(filename) = aesthetic_filename_for_kind(kind) else {
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
        // writeStoreFile: empty/whitespace content DELETES the file; else write.
        write_aesthetic_file(mount, mp, filename, &content)?;
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Project(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Wardrobe (v4 [id]/wardrobe/route.ts + [itemId]/route.ts)
// ===========================================================================
//
// PROJECT_WARDROBE_FOLDER = CHARACTER_WARDROBE_FOLDER ("Wardrobe"); the project
// tier reuses the same vault-write functions the P4.6f character wardrobe uses,
// pointed at the project's official store's `Wardrobe/` folder.

fn wardrobe_err(e: WardrobePublicError) -> Response {
    match e {
        WardrobePublicError::Cycle(msg) => bad_request(msg),
        WardrobePublicError::NoMount => internal("no wardrobe mount resolved"),
        WardrobePublicError::Db(d) => internal(d),
    }
}

/// v4 `optional string` field → `Some(v)` when present (even null-ish absent →
/// null), used for the create/patch nullable columns.
fn nullable_str(body: &Value, key: &str) -> Option<String> {
    body.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Resolve the project's official store mount id (v4 `ensureProjectOfficialStore`)
/// and ensure the `Wardrobe/` folder. Returns the mount id, or the NotFound
/// refusal when the project is absent.
fn ensure_project_wardrobe_mount(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    project_id: &str,
) -> Result<Result<String, Response>, DbError> {
    let mount_point_id = match ensure_project_store_mount(main, mount, project_id)? {
        Ok(mp) => mp,
        Err(r) => return Ok(Err(r)),
    };
    let links = DocMountFileLinksRepository::new(mount);
    let _ = links.ensure_folder_path(&mount_point_id, "Wardrobe");
    Ok(Ok(mount_point_id))
}

/// The store-ensure half of [`ensure_project_wardrobe_mount`] with NO
/// `Wardrobe/` folder ensure — v4's `?action=instructions` GET calls
/// `ensureProjectOfficialStore` alone (only its POST adds the folder ensure).
fn ensure_project_store_mount(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    project_id: &str,
) -> Result<Result<String, Response>, DbError> {
    let repo = ProjectsRepository::new(main, mount);
    let Some(project) = repo.find_by_id(project_id).map_err(overlay_to_db)? else {
        return Ok(Err(not_found("Project")));
    };
    let name = project.get("name").and_then(Value::as_str).unwrap_or("");
    let Some(ensured) = ensure_official_store::<ProjectEntity>(main, mount, project_id, name)?
    else {
        return Ok(Err(internal("Failed to ensure project document store")));
    };
    Ok(Ok(ensured.mount_point_id))
}

/// v4 GET `/wardrobe`: `{ mountPointId, wardrobeItems }` (include archived).
pub fn project_wardrobe_list(db: &Db, project_id: &str, include_archived: bool) -> Response {
    let pid = project_id.to_string();
    let out = read_both(db, move |main, mount| {
        let mp = match ensure_project_wardrobe_mount(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        let docs = DocMountDocumentsRepository::new(mount);
        // v4 `d25dacc1` replaced this route's hard-coded `true` with the
        // query param: the archived filter is SERVER-side now.
        let items = archetype_wardrobe::read_project_wardrobe(&docs, &mp, include_archived)?;
        Ok(Ok((mp, items)))
    });
    match out {
        Ok(Ok((mp, items))) => {
            Response::Project(json!({ "mountPointId": mp, "wardrobeItems": items }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 POST `/wardrobe`: mints the item id + ISO timestamps IN THE ROUTE, creates
/// it in the project store, re-lists. Body `{ mountPointId, wardrobeItem,
/// wardrobeItems }` (201). Component cycles → 400.
pub async fn project_wardrobe_create(db: &Db, project_id: &str, body: Value) -> Response {
    let pid = project_id.to_string();
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
        let mp = match ensure_project_wardrobe_mount(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        // v4 route mints id + ISO timestamps + the null columns (characterId,
        // migratedFromClothingRecordId, archivedAt all explicit null).
        let now = crate::clock::now_iso();
        let item = WardrobeItem {
            id: uuid::Uuid::new_v4().to_string(),
            character_id: Some(None),
            title,
            description: Some(description),
            image_prompt: Some(image_prompt),
            types,
            component_item_ids,
            appropriateness: Some(appropriateness),
            is_default,
            replace,
            migrated_from_clothing_record_id: Some(None),
            archived_at: Some(None),
            created_at: now.clone(),
            updated_at: now,
        };
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        let stored = match create_project_wardrobe_item(main, &links, &docs, &mp, &item) {
            Ok(s) => s,
            Err(e) => return Ok(Err(wardrobe_err(e))),
        };
        let items = archetype_wardrobe::read_project_wardrobe(&docs, &mp, true)?;
        Ok(Ok((
            mp,
            serde_json::to_value(stored).unwrap_or(Value::Null),
            items,
        )))
    })
    .await;
    match out {
        Ok(Ok((mp, item, items))) => Response::Project(json!({
            "mountPointId": mp,
            "wardrobeItem": item,
            "wardrobeItems": items,
        })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 GET `/wardrobe/[itemId]`: `{ wardrobeItem }` or NotFound.
pub fn project_wardrobe_get(db: &Db, project_id: &str, item_id: &str) -> Response {
    let pid = project_id.to_string();
    let iid = item_id.to_string();
    let out = read_both(db, move |main, mount| {
        let mp = match ensure_project_wardrobe_mount(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(_) => return Ok(None), // v4 resolveProjectMount → notFound('Project')
        };
        let docs = DocMountDocumentsRepository::new(mount);
        let items = archetype_wardrobe::read_project_wardrobe(&docs, &mp, true)?;
        Ok(Some(items.into_iter().find(|i| {
            i.get("id").and_then(Value::as_str) == Some(iid.as_str())
        })))
    });
    match out {
        Ok(Some(Some(item))) => Response::Project(json!({ "wardrobeItem": item })),
        Ok(Some(None)) => not_found("Project wardrobe item"),
        Ok(None) => not_found("Project"),
        Err(e) => db_error_response(e),
    }
}

/// v4 PUT `/wardrobe/[itemId]`: apply the partial patch. Body `{ wardrobeItem }`.
/// Component cycles → 400; missing item → NotFound.
pub async fn project_wardrobe_update(
    db: &Db,
    project_id: &str,
    item_id: &str,
    body: Value,
) -> Response {
    let pid = project_id.to_string();
    let iid = item_id.to_string();
    let mut patch = build_wardrobe_patch(&body);
    let archived = body.get("archived").and_then(Value::as_bool);
    let out = with_both_conns(db, move |main, mount| {
        let mp = match ensure_project_wardrobe_mount(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(_) => return Ok(Err(not_found("Project"))),
        };
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        // v4 `d25dacc1`: the project/group routes hold no already-loaded item, so
        // they pay for an EXTRA O(folder) read — but ONLY when `archived` is in
        // the body. That is where the new `Project wardrobe item not found` 404
        // lives: it is unreachable without the key.
        if let Some(a) = archived {
            let items = archetype_wardrobe::read_project_wardrobe(&docs, &mp, true)?;
            let Some(current) = items
                .iter()
                .find(|i| i.get("id").and_then(Value::as_str) == Some(iid.as_str()))
            else {
                return Ok(Err(not_found("Project wardrobe item")));
            };
            if let Some(v) = crate::wardrobe::archived_patch(
                current.get("archivedAt").and_then(Value::as_str),
                a,
                &crate::clock::now_iso(),
            ) {
                patch.archived_at = Some(v);
            }
        }
        match update_project_wardrobe_item(main, &links, &docs, &mp, &iid, &patch) {
            // Re-read through the overlay so the echo carries the full null-inclusive
            // shape v4's JS object emits (the WardrobeItem struct serialize skips
            // `None` fields; the Value read path renders them as null).
            Ok(Some(_)) => {
                let item = archetype_wardrobe::read_project_wardrobe(&docs, &mp, true)?
                    .into_iter()
                    .find(|i| i.get("id").and_then(Value::as_str) == Some(iid.as_str()))
                    .unwrap_or(Value::Null);
                Ok(Ok(item))
            }
            Ok(None) => Ok(Err(not_found("Project wardrobe item"))),
            Err(e) => Ok(Err(wardrobe_err(e))),
        }
    })
    .await;
    match out {
        Ok(Ok(item)) => Response::Project(json!({ "wardrobeItem": item })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 DELETE `/wardrobe/[itemId]`: `removeEquippedItemFromAllChats(itemId)`
/// warn-and-proceed → delete. Body `{ success: true }`; missing item → NotFound.
pub async fn project_wardrobe_delete(db: &Db, project_id: &str, item_id: &str) -> Response {
    let pid = project_id.to_string();
    let iid = item_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match ensure_project_wardrobe_mount(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(_) => return Ok(Err(not_found("Project"))),
        };
        // warn-and-proceed cleanup (v4 wraps this in its own try/catch → warn).
        let _ = ChatOutfitsRepository::new(main).remove_equipped_item_from_all_chats(&iid);
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        match delete_project_wardrobe_item(main, &links, &docs, &mp, &iid) {
            Ok(true) => Ok(Ok(())),
            Ok(false) => Ok(Err(not_found("Project wardrobe item"))),
            Err(e) => Ok(Err(wardrobe_err(e))),
        }
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Project(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// Build a `WardrobePatch` from the `updateWardrobeSchema.partial()` body (only
/// present keys are set; nullable columns distinguish present-null from absent).
fn build_wardrobe_patch(body: &Value) -> WardrobePatch {
    let arr = |k: &str| -> Option<Vec<String>> {
        body.get(k).and_then(Value::as_array).map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
    };
    // A present nullable key → Some(inner) where inner mirrors null-vs-value.
    let nullable =
        |k: &str| -> Option<Option<String>> { body.get(k).map(|v| v.as_str().map(str::to_string)) };
    WardrobePatch {
        title: body
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        types: arr("types"),
        component_item_ids: arr("componentItemIds"),
        description: nullable("description"),
        image_prompt: nullable("imagePrompt"),
        appropriateness: nullable("appropriateness"),
        is_default: body.get("isDefault").and_then(Value::as_bool),
        replace: body.get("replace").and_then(Value::as_bool),
        archived_at: None,
    }
}

// ===========================================================================
// Scenarios (v4 projects/[id]/scenarios/route.ts + [scenarioPath]/route.ts) —
// P4.6n. Mirrors the groups family, but ensures ONLY `Scenarios/` (no Knowledge)
// and the projects-side store resolvers. The mount-scoped CRUD is shared in
// [`super::scenarios`]. There is no projects participant-union.
// ===========================================================================

const PROJECT_SCENARIOS_FOLDER: &str = "Scenarios";

/// Ensure a project's official store + the Scenarios/ folder (v4 the collection
/// routes' `ensureProjectOfficialStore` + `ensureProjectScenariosFolder`). Returns
/// the mount id, or an early Response.
fn ensure_project_scenarios_store(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    project_id: &str,
) -> Result<Result<String, Response>, DbError> {
    let repo = ProjectsRepository::new(main, mount);
    let Some(project) = repo.find_by_id(project_id).map_err(overlay_to_db)? else {
        return Ok(Err(not_found("Project")));
    };
    let name = project.get("name").and_then(Value::as_str).unwrap_or("");
    let Some(ensured) = ensure_official_store::<ProjectEntity>(main, mount, project_id, name)?
    else {
        return Ok(Err(internal("Failed to ensure project document store")));
    };
    let _ = DocMountFileLinksRepository::new(mount)
        .ensure_folder_path(&ensured.mount_point_id, PROJECT_SCENARIOS_FOLDER);
    Ok(Ok(ensured.mount_point_id))
}

/// v4 `loadProjectAndStore` — the RAW official-store pointer (404 when null) + the
/// Scenarios/ folder ensure. Used by the [scenarioPath] item routes.
fn load_project_scenarios_store(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    project_id: &str,
) -> Result<Result<String, Response>, DbError> {
    let Some(fk) = find_official_mount_point_id_raw(main, project_id)? else {
        return Ok(Err(not_found("Project")));
    };
    let Some(mp) = fk else {
        return Ok(Err(not_found(
            "Project has no official document store yet — restart the server or call GET /scenarios first",
        )));
    };
    let _ =
        DocMountFileLinksRepository::new(mount).ensure_folder_path(&mp, PROJECT_SCENARIOS_FOLDER);
    Ok(Ok(mp))
}

/// v4 `GET /api/v1/projects/[id]/scenarios`.
pub async fn project_scenario_list(db: &Db, project_id: &str, include_archived: bool) -> Response {
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match ensure_project_scenarios_store(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(Ok(scenarios_api::list_body(mount, &mp, include_archived)?))
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Project(body),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `POST /api/v1/projects/[id]/scenarios`.
pub async fn project_scenario_create(db: &Db, project_id: &str, bag: Value) -> Response {
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match ensure_project_scenarios_store(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(match scenarios_api::create_op(mount, &mp, &bag) {
            Ok(inner) => inner,
            Err(e) => Err(scenarios_api::write_err(e)),
        })
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Project(body),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `GET /api/v1/projects/[id]/scenarios/[scenarioPath]`.
pub async fn project_scenario_get(db: &Db, project_id: &str, scenario_path: &str) -> Response {
    let resolved = match scenarios_api::resolve_path_or_400(scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match load_project_scenarios_store(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(match scenarios_api::get_op(mount, &mp, &resolved) {
            Ok(inner) => inner,
            Err(e) => Err(scenarios_api::write_err(e)),
        })
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Project(body),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `PUT /api/v1/projects/[id]/scenarios/[scenarioPath]`.
pub async fn project_scenario_update(
    db: &Db,
    project_id: &str,
    scenario_path: &str,
    bag: Value,
    include_archived: bool,
) -> Response {
    let resolved = match scenarios_api::resolve_path_or_400(scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match load_project_scenarios_store(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(
            match scenarios_api::update_op(mount, &mp, &resolved, &bag, include_archived) {
                Ok(inner) => inner,
                Err(e) => Err(scenarios_api::write_err(e)),
            },
        )
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Project(body),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `POST /api/v1/projects/[id]/scenarios/[scenarioPath]?action=rename`.
pub async fn project_scenario_rename(
    db: &Db,
    project_id: &str,
    scenario_path: &str,
    new_filename: &str,
    include_archived: bool,
) -> Response {
    let resolved = match scenarios_api::resolve_path_or_400(scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let pid = project_id.to_string();
    let nf = new_filename.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match load_project_scenarios_store(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(
            match scenarios_api::rename_op(mount, &mp, &resolved, &nf, include_archived) {
                Ok(inner) => inner,
                Err(e) => Err(scenarios_api::write_err(e)),
            },
        )
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Project(body),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `DELETE /api/v1/projects/[id]/scenarios/[scenarioPath]`.
pub async fn project_scenario_delete(
    db: &Db,
    project_id: &str,
    scenario_path: &str,
    include_archived: bool,
) -> Response {
    let resolved = match scenarios_api::resolve_path_or_400(scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let pid = project_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match load_project_scenarios_store(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(
            match scenarios_api::delete_op(mount, &mp, &resolved, include_archived) {
                Ok(inner) => inner,
                Err(e) => Err(scenarios_api::write_err(e)),
            },
        )
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Project(body),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Files (v4 projects/[id]/actions/files.ts) — the list-files two-branch +
// add-file/remove-file, P4.6n unit 5.
// ===========================================================================

/// v4 `mimeForMountFile` — a doc_mount_files fileType (+ filename for the `blob`
/// catch-all) → a concrete MIME. Non-enum fileTypes fall through to the `blob`
/// extension-detect (v4's switch is exhaustive over the fileType enum).
fn mime_for_mount_file(file_type: &str, file_name: &str) -> String {
    match file_type {
        "pdf" => "application/pdf".to_string(),
        "docx" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string()
        }
        "markdown" => "text/markdown".to_string(),
        "txt" => "text/plain".to_string(),
        "json" => "application/json".to_string(),
        "jsonl" => "application/jsonl".to_string(),
        _ => crate::mime::detect_mime_type(file_name),
    }
}

/// v4 `getProjectDocumentStore(projectId).mountPointId` — the primary linked
/// Scriptorium store's mount id, or `None`. Resolves each linked mount point's
/// naming subset and applies `pickPrimaryProjectStore`.
fn get_project_document_store_mount_id(
    mount: &rusqlite::Connection,
    project_id: &str,
) -> Result<Option<String>, DbError> {
    let link_ids = ProjectDocMountLinksRepository::new(mount).find_by_project_id(project_id)?;
    if link_ids.is_empty() {
        return Ok(None);
    }
    let points = DocMountPointsRepository::new(mount);
    let mut stores = Vec::new();
    for id in &link_ids {
        if let Some(s) = points.find_store_naming_by_id(id)? {
            stores.push(s);
        }
    }
    Ok(pick_primary_project_store(&stores).map(|s| s.id.clone()))
}

/// v4 `GET /api/v1/projects/[id]?action=list-files`. Branch A: when a primary
/// Scriptorium store is linked, list its `doc_mount_files` (store = source of
/// truth). Branch B: else the legacy `files` rows scoped to the project. Body
/// `{ files, count }`.
pub fn project_file_list(db: &Db, project_id: &str) -> Response {
    let pid = project_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Project")));
        }

        // Branch A: primary store present → the store's doc_mount_files.
        if let Some(store_mp) = get_project_document_store_mount_id(mount, &pid)? {
            let links =
                DocMountFileLinksRepository::new(mount).find_by_mount_point_id(&store_mp)?;
            let files: Vec<Value> = links
                .iter()
                .map(|l| {
                    let mime = mime_for_mount_file(&l.file_type, &l.file_name);
                    let category = if mime.starts_with("image/") {
                        "IMAGE"
                    } else {
                        "DOCUMENT"
                    };
                    // v4 `f.lastModified || f.updatedAt`; lastModified is always set
                    // on a store write (the `|| updatedAt` fallback is unreachable
                    // in-corpus, and LinkRow carries no separate updatedAt).
                    let updated = if l.last_modified.is_empty() {
                        l.created_at.clone()
                    } else {
                        l.last_modified.clone()
                    };
                    json!({
                        "id": l.id,
                        "originalFilename": l.file_name,
                        "filename": l.file_name,
                        "mimeType": mime,
                        "size": l.file_size_bytes,
                        "category": category,
                        "description": Value::Null,
                        "projectId": pid,
                        "folderPath": Value::Null,
                        "width": Value::Null,
                        "height": Value::Null,
                        "createdAt": l.created_at,
                        "updatedAt": updated,
                        "mountPointId": l.mount_point_id,
                        "relativePath": l.relative_path,
                    })
                })
                .collect();
            let count = files.len();
            return Ok(Ok(json!({ "files": files, "count": count })));
        }

        // Branch B: the legacy files table.
        let rows = FilesRepository::new(main).find_by_project_id_for_listing(&pid)?;
        let files: Vec<Value> = rows
            .iter()
            .map(|f| {
                let mut m = Map::new();
                m.insert("id".into(), json!(f.id));
                m.insert("userId".into(), json!(f.user_id));
                m.insert("originalFilename".into(), json!(f.original_filename));
                m.insert("filename".into(), json!(f.original_filename));
                m.insert("mimeType".into(), json!(f.mime_type));
                m.insert("size".into(), json!(f.size));
                m.insert("category".into(), json!(f.category));
                // Read marshaling omits null description/width/height → drop keys.
                if let Some(d) = &f.description {
                    m.insert("description".into(), json!(d));
                }
                m.insert(
                    "projectId".into(),
                    match &f.project_id {
                        Some(p) => json!(p),
                        None => Value::Null,
                    },
                );
                m.insert(
                    "folderPath".into(),
                    json!(crate::folder_utils::resolve_effective_folder_path(
                        f.folder_path.as_deref(),
                        f.storage_key.as_deref()
                    )),
                );
                if let Some(w) = f.width {
                    m.insert("width".into(), json!(w));
                }
                if let Some(h) = f.height {
                    m.insert("height".into(), json!(h));
                }
                m.insert("createdAt".into(), json!(f.created_at));
                m.insert("updatedAt".into(), json!(f.updated_at));
                Value::Object(m)
            })
            .collect();
        let count = files.len();
        Ok(Ok(json!({ "files": files, "count": count })))
    });
    match result {
        Ok(Ok(body)) => Response::Project(body),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `POST /api/v1/projects/[id]?action=add-file`: ownership('Project') → file
/// existence ('File') → `files.update(fileId, {projectId})`. Body `{ success: true }`.
pub async fn project_file_add(db: &Db, project_id: &str, file_id: &str) -> Response {
    let pid = project_id.to_string();
    let fid = file_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Project")));
        }
        let files = FilesRepository::new(main);
        if files.find_by_id(&fid)?.is_none() {
            return Ok(Err(not_found("File")));
        }
        files.update(
            &fid,
            &FileUpdate {
                project_id: Some(pid.clone()),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            },
        )?;
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Project(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `DELETE /api/v1/projects/[id]?action=remove-file`: ownership('Project') →
/// `files.update(fileId, {projectId: null})`. Body `{ success: true }`.
pub async fn project_file_remove(db: &Db, project_id: &str, file_id: &str) -> Response {
    let pid = project_id.to_string();
    let fid = file_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = ProjectsRepository::new(main, mount);
        if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Project")));
        }
        FilesRepository::new(main).clear_project_id(&fid)?;
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Project(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// `?action=instructions` — the project store's dressing instructions
// (v4 `b86bb1a5`, `app/api/v1/projects/[id]/wardrobe/route.ts` — a byte-copy of
// the group file with Project spellings)
// ===========================================================================

/// v4 `GET /api/v1/projects/[id]/wardrobe?action=instructions`.
pub fn project_wardrobe_instructions_get(db: &Db, project_id: &str) -> Response {
    let pid = project_id.to_string();
    let out = read_both(db, move |main, mount| {
        let mp = match ensure_project_store_mount(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        let instructions = read_wardrobe_instructions_file(mount, &mp);
        Ok(Ok((mp, instructions)))
    });
    match out {
        Ok(Ok((mount_point_id, instructions))) => {
            tracing::debug!(
                project_id,
                mount_point_id,
                present = instructions.is_some(),
                context = "wardrobe",
                "[Projects v1] Read project dressing instructions"
            );
            Response::Project(json!({ "instructions": instructions }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `POST /api/v1/projects/[id]/wardrobe?action=instructions`.
pub async fn project_wardrobe_instructions_set(
    db: &Db,
    project_id: &str,
    instructions: &Option<Option<Value>>,
) -> Response {
    let pid = project_id.to_string();
    let raw = instructions.clone();
    let out = with_both_conns(db, move |main, mount| {
        {
            let repo = ProjectsRepository::new(main, mount);
            if repo.find_by_id(&pid).map_err(overlay_to_db)?.is_none() {
                return Ok(Err(not_found("Project")));
            }
        }
        // Parsed only AFTER the 404 gate: v4 runs `findById → notFound` before
        // `instructionsBodySchema.parse`, so a missing project with an invalid
        // body answers 404, not 400.
        let instructions = match super::wardrobe::parse_instructions_body(&raw) {
            Ok(v) => v,
            Err(r) => return Ok(Err(r)),
        };
        let (cleared, echo) = super::wardrobe::instructions_echo(instructions.as_deref());
        let mp = match ensure_project_wardrobe_mount(main, mount, &pid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        let links = DocMountFileLinksRepository::new(mount);
        write_wardrobe_instructions_file(&links, &mp, instructions.as_deref())?;
        Ok(Ok((mp, cleared, echo)))
    })
    .await;
    match out {
        Ok(Ok((mount_point_id, cleared, echo))) => {
            tracing::info!(
                project_id,
                mount_point_id,
                cleared,
                context = "wardrobe",
                "[Projects v1] Project dressing instructions updated"
            );
            Response::Project(json!({ "instructions": echo }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

#[cfg(test)]
mod create_schema_tests {
    use super::*;

    fn ok(body: Value) -> bool {
        validate_project_create(&body).is_ok()
    }

    /// The bug-98 rows. MEASURED against v4's real `createProjectSchema` at
    /// `f6a10055` (old schema → new schema): each of these four went 400 → 200,
    /// and they are the ENTIRE behavioural delta of `c93ec7ff`.
    #[test]
    fn the_four_nullable_fields_accept_an_explicit_null() {
        assert!(ok(json!({ "name": "P", "description": null })));
        assert!(ok(json!({ "name": "P", "instructions": null })));
        assert!(ok(json!({ "name": "P", "color": null })));
        assert!(ok(json!({ "name": "P", "icon": null })));
        // All four at once — the create dialog's actual blank-field body.
        assert!(ok(json!({
            "name": "P", "description": null, "instructions": null,
            "color": null, "icon": null
        })));
    }

    /// `allowAnyCharacter` and `characterRoster` are `.optional().prefault(…)`
    /// but NOT `.nullable()` — an explicit null still refuses, in both schemas.
    #[test]
    fn the_prefaulted_fields_are_not_nullable() {
        assert!(!ok(json!({ "name": "P", "allowAnyCharacter": null })));
        assert!(!ok(json!({ "name": "P", "characterRoster": null })));
    }

    /// `name` is the one field with no `.optional()`.
    #[test]
    fn name_is_required_and_never_nullable() {
        assert!(!ok(json!({})));
        assert!(!ok(json!({ "name": null })));
        assert!(!ok(json!({ "name": 42 })));
        assert!(!ok(json!({ "name": "" })));
    }

    /// `.min(1)` runs on the RAW string — there is no `.trim()` in this schema,
    /// so a whitespace-only name is length 1 and PASSES. v5's old
    /// `name.trim().is_empty()` guard wrongly refused it.
    #[test]
    fn a_whitespace_only_name_passes_as_it_does_in_v4() {
        assert!(ok(json!({ "name": " " })));
        assert!(ok(json!({ "name": "   " })));
    }

    /// Zod measures `String.length` — UTF-16 code units. 50 astral characters
    /// are 100 units and pass; 51 are 102 and fail.
    #[test]
    fn lengths_are_utf16_code_units() {
        assert!(ok(json!({ "name": "🎩".repeat(50) })));
        assert!(!ok(json!({ "name": "🎩".repeat(51) })));
        assert!(ok(json!({ "name": "x".repeat(100) })));
        assert!(!ok(json!({ "name": "x".repeat(101) })));
        assert!(ok(json!({ "name": "P", "description": "x".repeat(2000) })));
        assert!(!ok(json!({ "name": "P", "description": "x".repeat(2001) })));
        assert!(ok(
            json!({ "name": "P", "instructions": "x".repeat(10_000) })
        ));
        assert!(!ok(
            json!({ "name": "P", "instructions": "x".repeat(10_001) })
        ));
        assert!(ok(json!({ "name": "P", "icon": "x".repeat(50) })));
        assert!(!ok(json!({ "name": "P", "icon": "x".repeat(51) })));
    }

    /// `/^#(?:[0-9a-fA-F]{3}){1,2}$/` — three or six hex digits, nothing else.
    #[test]
    fn the_colour_regex_is_whole_string_three_or_six_digits() {
        assert!(ok(json!({ "name": "P", "color": "#abc" })));
        assert!(ok(json!({ "name": "P", "color": "#ABCDEF" })));
        assert!(!ok(json!({ "name": "P", "color": "#abcd" })));
        assert!(!ok(json!({ "name": "P", "color": "blue" })));
    }

    #[test]
    fn the_roster_is_an_array_of_zod_uuids() {
        assert!(ok(json!({ "name": "P", "characterRoster": [] })));
        assert!(ok(
            json!({ "name": "P", "characterRoster": ["a1000000-0000-4000-8000-000000000001"] })
        ));
        assert!(!ok(
            json!({ "name": "P", "characterRoster": ["not-a-uuid"] })
        ));
        assert!(!ok(json!({ "name": "P", "characterRoster": "x" })));
    }

    /// `z.object` is non-strict: an unknown key is STRIPPED, never a refusal.
    /// (The handler reads only the recognized keys, which is that strip.)
    #[test]
    fn an_unknown_key_is_stripped_not_refused() {
        assert!(ok(json!({ "name": "P", "notAField": "should vanish" })));
    }

    /// A non-object body is Zod's root `invalid_type` — the same flat sentence.
    #[test]
    fn a_non_object_body_is_a_validation_error_too() {
        assert!(!ok(json!("hello")));
        assert!(!ok(json!([])));
        assert!(!ok(Value::Null));
    }
}
