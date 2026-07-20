//! The Groups-server dispatch handlers (P4.6k) — the Prospero groups route-logic
//! backfill the Groups/Projects SPA vertical consumes, composed over the
//! already-ported store-backed `groups` repository + the mount-index sibling
//! repos (`group_character_members`, `group_doc_mount_links`, `doc_mount_points`).
//!
//! Each handler is a differential port of a v4 groups route handler (the oracle:
//! `groups/route.ts`, `groups/[id]/actions/group-crud.ts`,
//! `groups/[id]/mount-points/route.ts`, `groups/scenarios/route.ts`) and returns
//! a [`Response`] directly (the engine arm is a one-line delegate). Reads nest
//! [`Db::read_main`] + [`Db::read_mount_index`] (separate pools → no deadlock) for
//! the store overlay; writes go through the dual-connection writer
//! ([`with_both_conns`]).
//!
//! `user_id` is a parameter (not hard-coded `SINGLE_USER_ID`) so the differential
//! harness can drive with the fixture's own user id on both sides; the engine
//! passes `SINGLE_USER_ID`. v4's `checkOwnership` collapses to NotFound-on-absent
//! for the single-user v5.

use serde_json::{json, Map, Value};

use crate::collation::locale_compare;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::document_store_overlay::OverlayError;
use crate::db::ensure_official_store::ensure_official_store;
use crate::db::group_character_members::GroupCharacterMembersRepository;
use crate::db::group_doc_mount_links::GroupDocMountLinksRepository;
use crate::db::groups::{
    find_name_and_official_mount_point_id_raw, GroupCreateInput, GroupCreateOptions, GroupEntity,
    GroupsRepository,
};
use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};
use crate::services::image_job_common::with_both_conns;

use super::scenarios as scenarios_api;
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

/// The loud "recognized but not yet available" refusal for groups-family variants
/// whose handler is a documented deferral or lands in a later P4.6k milestone.
pub fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' groups action is recognized but not yet available."),
    )
}

/// Run a read closure with both a main + mount-index connection (the store
/// overlay needs both).
fn read_both<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// `_count.members` for a group id (the mount-index membership count).
fn member_count(mount: &rusqlite::Connection, group_id: &str) -> Result<usize, DbError> {
    Ok(GroupCharacterMembersRepository::new(mount)
        .find_character_ids_by_group_id(group_id)?
        .len())
}

/// v4's `''`→`null` create coercion (JS `x || null`: undefined/''/null → null).
fn or_null(v: Option<&str>) -> Option<String> {
    match v {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

// ===========================================================================
// List (v4 GET /api/v1/groups)
// ===========================================================================

/// v4 `groups/route.ts` GET: `findAll` → createdAt-desc sort → `_count.members`
/// enrichment. Body `{ groups: [...] }`.
pub fn group_list(db: &Db) -> Response {
    let result = read_both(db, |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        let mut groups = repo.find_all().map_err(overlay_to_db)?;
        // createdAt descending (ISO strings sort lexically == v4 `getTime()`).
        groups.sort_by(|a, b| {
            let ta = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
            let tb = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
            tb.cmp(ta)
        });
        let mut enriched = Vec::with_capacity(groups.len());
        for g in groups {
            let id = g
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let count = member_count(mount, &id)?;
            let mut obj = g.as_object().cloned().unwrap_or_default();
            obj.insert("_count".into(), json!({ "members": count }));
            enriched.push(Value::Object(obj));
        }
        Ok(enriched)
    });
    match result {
        Ok(groups) => Response::Group(json!({ "groups": groups })),
        Err(e) => internal(e),
    }
}

/// Bridge `OverlayError` into a `DbError` inside a read closure (the closures are
/// `-> Result<_, DbError>`); an unavailable store becomes a `DbError::Key`.
fn overlay_to_db(e: OverlayError) -> DbError {
    match e {
        OverlayError::Db(d) => d,
        OverlayError::Unavailable { detail, .. } => DbError::Key(detail),
    }
}

// ===========================================================================
// Create (v4 POST /api/v1/groups)
// ===========================================================================

/// v4 `groups/route.ts` POST: `createGroupSchema.parse` → `groups.create` (with
/// the `|| null` coercions + `state: {}`) → best-effort Scenarios/Knowledge
/// folder ensure. Body `{ group }`.
pub async fn group_create(
    db: &Db,
    name: String,
    description: Option<String>,
    color: Option<String>,
    icon: Option<String>,
) -> Response {
    if name.trim().is_empty() {
        return bad_request("Name is required");
    }
    let input = GroupCreateInput {
        name,
        description: or_null(description.as_deref()),
        instructions: None,
        state: json!({}),
        color: or_null(color.as_deref()),
        icon: or_null(icon.as_deref()),
    };
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        let group = repo
            .create(&input, &GroupCreateOptions::default())
            .map_err(overlay_to_db)?;
        // Best-effort Scenarios/ + Knowledge/ folder ensure (v4 non-fatal try/catch
        // in the POST handler; the GET /scenarios path also ensures them).
        if let Some(mp) = group.get("officialMountPointId").and_then(Value::as_str) {
            let links = crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount);
            let _ = links.ensure_folder_path(mp, "Scenarios");
            let _ = links.ensure_folder_path(mp, "Knowledge");
        }
        Ok(group)
    })
    .await;
    match out {
        Ok(group) => Response::Group(json!({ "group": group })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Get / members (v4 groups/[id]/actions/group-crud.ts)
// ===========================================================================

/// v4 `handleGetDefault`: ownership → `{ ...group, _count: { members } }`.
pub fn group_get(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        let Some(group) = repo.find_by_id(&gid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        let count = member_count(mount, &gid)?;
        let mut obj = group.as_object().cloned().unwrap_or_default();
        obj.insert("_count".into(), json!({ "members": count }));
        Ok(Some(Value::Object(obj)))
    });
    match result {
        Ok(Some(group)) => Response::Group(json!({ "group": group })),
        Ok(None) => not_found("Group"),
        Err(e) => internal(e),
    }
}

/// v4 `handleGetMembers`: for each membership, `characters.findById` → `{id,name}`,
/// dropping nulls (user-scoping / orphans). Body `{ members }`.
pub fn group_members(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        let member_ids =
            GroupCharacterMembersRepository::new(mount).find_character_ids_by_group_id(&gid)?;
        let mut members = Vec::new();
        for cid in member_ids {
            if let Some(c) = characters_read::find_by_id(main, mount, &cid)? {
                members.push(json!({
                    "id": c.get("id").cloned().unwrap_or(Value::Null),
                    "name": c.get("name").cloned().unwrap_or(Value::Null),
                }));
            }
        }
        Ok(Some(members))
    });
    match result {
        Ok(Some(members)) => Response::Group(json!({ "members": members })),
        Ok(None) => not_found("Group"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Update / delete (v4 group-crud.ts)
// ===========================================================================

/// v4 `handlePutDefault`: ownership → `updateGroupSchema.parse` (passthrough, no
/// `|| null`) → `groups.update`. Body `{ group }`.
pub async fn group_update(db: &Db, group_id: &str, patch: Value) -> Response {
    let gid = group_id.to_string();
    let Some(patch_map) = patch.as_object().cloned() else {
        return bad_request("Expected a JSON object body");
    };
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        let updated = repo.update(&gid, &patch_map).map_err(overlay_to_db)?;
        Ok(updated)
    })
    .await;
    match out {
        Ok(Some(group)) => Response::Group(json!({ "group": group })),
        Ok(None) => not_found("Group"),
        Err(e) => internal(e),
    }
}

/// v4 `handleDeleteGroup`: ownership → drop memberships + additional-store links
/// → delete the group row (the OFFICIAL store is orphaned). Body `{ success: true }`.
pub async fn group_delete(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(false);
        }
        GroupCharacterMembersRepository::new(mount).delete_by_group_id(&gid)?;
        GroupDocMountLinksRepository::new(mount).delete_by_group_id(&gid)?;
        repo.delete(&gid)?;
        Ok(true)
    })
    .await;
    match out {
        Ok(true) => Response::Group(json!({ "success": true })),
        Ok(false) => not_found("Group"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Members add / remove (v4 group-crud.ts)
// ===========================================================================

/// v4 `handleAddMember`: ownership('Group') → `characters.findById` (missing →
/// `badRequest('Character not found')`, a 400) → idempotent `addMember`. Body
/// `{ success: true }`.
pub async fn group_member_add(db: &Db, group_id: &str, character_id: &str) -> Response {
    let gid = group_id.to_string();
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Group")));
        }
        if characters_read::find_by_id(main, mount, &cid)?.is_none() {
            return Ok(Err(bad_request("Character not found")));
        }
        GroupCharacterMembersRepository::new(mount).add_member(&gid, &cid)?;
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Group(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `handleRemoveMember`: ownership → `removeMember` (result ignored — always
/// success). Body `{ success: true }`.
pub async fn group_member_remove(db: &Db, group_id: &str, character_id: &str) -> Response {
    let gid = group_id.to_string();
    let cid = character_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(false);
        }
        GroupCharacterMembersRepository::new(mount).remove_member(&gid, &cid)?;
        Ok(true)
    })
    .await;
    match out {
        Ok(true) => Response::Group(json!({ "success": true })),
        Ok(false) => not_found("Group"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Mount points (v4 groups/[id]/mount-points/route.ts)
// ===========================================================================

/// v4 GET `/mount-points`: ownership → resolve each linked mount point, drop the
/// deleted ones (whose links dangle). Body `{ mountPoints }` (the ADDITIONAL
/// stores only — never the official store).
pub fn group_mount_point_list(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        let link_ids = GroupDocMountLinksRepository::new(mount).find_by_group_id(&gid)?;
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
        Ok(Some(mps)) => Response::Group(json!({ "mountPoints": mps })),
        Ok(None) => not_found("Group"),
        Err(e) => internal(e),
    }
}

/// v4 POST `/mount-points`: ownership('Group') → mount-point existence
/// (`notFound('Mount point')`) → idempotent `link`. Body `{ link, mountPoint }`.
pub async fn group_mount_point_link(db: &Db, group_id: &str, mount_point_id: &str) -> Response {
    let gid = group_id.to_string();
    let mp_id = mount_point_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Group")));
        }
        let points = DocMountPointsRepository::new(mount);
        let Some(mp) = points.find_full_json_by_id(&mp_id)? else {
            return Ok(Err(not_found("Mount point")));
        };
        let link = GroupDocMountLinksRepository::new(mount).link_returning(&gid, &mp_id)?;
        Ok(Ok((link, mp)))
    })
    .await;
    match out {
        Ok(Ok((link, mount_point))) => {
            Response::Group(json!({ "link": link, "mountPoint": mount_point }))
        }
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 DELETE `/mount-points`: ownership → `unlink`; no link → `badRequest`. Body
/// `{ message: 'Mount point unlinked from group' }`.
pub async fn group_mount_point_unlink(db: &Db, group_id: &str, mount_point_id: &str) -> Response {
    let gid = group_id.to_string();
    let mp_id = mount_point_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Group")));
        }
        let unlinked = GroupDocMountLinksRepository::new(mount).unlink(&gid, &mp_id)?;
        if !unlinked {
            return Ok(Err(bad_request(
                "No link exists between this group and mount point",
            )));
        }
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Group(json!({ "message": "Mount point unlinked from group" })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Scenarios (v4 groups/[id]/scenarios/route.ts + [scenarioPath]/route.ts,
// groups/scenarios/route.ts) — P4.6n. The mount-scoped CRUD is shared in
// [`super::scenarios`]; here we resolve the group's official store (the two
// folder quirks: groups ensure BOTH Scenarios/ AND Knowledge/).
// ===========================================================================

const GROUP_SCENARIOS_FOLDER: &str = "Scenarios";
const GROUP_KNOWLEDGE_FOLDER: &str = "Knowledge";

/// Ensure a group's official store + the Scenarios/ + Knowledge/ folders (v4 the
/// collection routes' `ensureGroupOfficialStore` + folder ensures). Returns the
/// mount id, or an early Response.
fn ensure_group_scenarios_store(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    group_id: &str,
) -> Result<Result<String, Response>, DbError> {
    let Some((name, _fk)) = find_name_and_official_mount_point_id_raw(main, group_id)? else {
        return Ok(Err(not_found("Group")));
    };
    let Some(ensured) = ensure_official_store::<GroupEntity>(main, mount, group_id, &name)? else {
        return Ok(Err(internal("Failed to ensure group document store")));
    };
    let links = DocMountFileLinksRepository::new(mount);
    let _ = links.ensure_folder_path(&ensured.mount_point_id, GROUP_SCENARIOS_FOLDER);
    let _ = links.ensure_folder_path(&ensured.mount_point_id, GROUP_KNOWLEDGE_FOLDER);
    Ok(Ok(ensured.mount_point_id))
}

/// v4 `loadGroupAndStore` — the RAW official-store pointer (404 when null) + the
/// Scenarios/ + Knowledge/ folder ensure. Used by the [scenarioPath] item routes.
fn load_group_scenarios_store(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    group_id: &str,
) -> Result<Result<String, Response>, DbError> {
    let Some((_name, fk)) = find_name_and_official_mount_point_id_raw(main, group_id)? else {
        return Ok(Err(not_found("Group")));
    };
    let Some(mp) = fk else {
        return Ok(Err(not_found(
            "Group has no official document store yet — restart the server or call GET /scenarios first",
        )));
    };
    let links = DocMountFileLinksRepository::new(mount);
    let _ = links.ensure_folder_path(&mp, GROUP_SCENARIOS_FOLDER);
    let _ = links.ensure_folder_path(&mp, GROUP_KNOWLEDGE_FOLDER);
    Ok(Ok(mp))
}

/// v4 `GET /api/v1/groups/[id]/scenarios`.
pub async fn group_scenario_list(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match ensure_group_scenarios_store(main, mount, &gid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(Ok(scenarios_api::list_body(mount, &mp)?))
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Group(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `POST /api/v1/groups/[id]/scenarios`.
pub async fn group_scenario_create(db: &Db, group_id: &str, bag: Value) -> Response {
    let gid = group_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match ensure_group_scenarios_store(main, mount, &gid)? {
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
        Ok(Ok(body)) => Response::Group(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `GET /api/v1/groups/[id]/scenarios/[scenarioPath]`.
pub async fn group_scenario_get(db: &Db, group_id: &str, scenario_path: &str) -> Response {
    let resolved = match scenarios_api::resolve_path_or_400(scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let gid = group_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match load_group_scenarios_store(main, mount, &gid)? {
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
        Ok(Ok(body)) => Response::Group(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `PUT /api/v1/groups/[id]/scenarios/[scenarioPath]`.
pub async fn group_scenario_update(
    db: &Db,
    group_id: &str,
    scenario_path: &str,
    bag: Value,
) -> Response {
    let resolved = match scenarios_api::resolve_path_or_400(scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let gid = group_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match load_group_scenarios_store(main, mount, &gid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(
            match scenarios_api::update_op(mount, &mp, &resolved, &bag) {
                Ok(inner) => inner,
                Err(e) => Err(scenarios_api::write_err(e)),
            },
        )
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Group(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `POST /api/v1/groups/[id]/scenarios/[scenarioPath]?action=rename`.
pub async fn group_scenario_rename(
    db: &Db,
    group_id: &str,
    scenario_path: &str,
    new_filename: &str,
) -> Response {
    let resolved = match scenarios_api::resolve_path_or_400(scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let gid = group_id.to_string();
    let nf = new_filename.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match load_group_scenarios_store(main, mount, &gid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(match scenarios_api::rename_op(mount, &mp, &resolved, &nf) {
            Ok(inner) => inner,
            Err(e) => Err(scenarios_api::write_err(e)),
        })
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Group(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `DELETE /api/v1/groups/[id]/scenarios/[scenarioPath]`.
pub async fn group_scenario_delete(db: &Db, group_id: &str, scenario_path: &str) -> Response {
    let resolved = match scenarios_api::resolve_path_or_400(scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let gid = group_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let mp = match load_group_scenarios_store(main, mount, &gid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        Ok(match scenarios_api::delete_op(mount, &mp, &resolved) {
            Ok(inner) => inner,
            Err(e) => Err(scenarios_api::write_err(e)),
        })
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Group(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `GET /api/v1/groups/scenarios?characterIds=` — the participant-union. For
/// every group that ANY supplied (user-owned) character is a member of, that
/// group's Scenarios entries, grouped under the group name and sorted by name
/// (ICU4X en-US). Zero-scenario groups are skipped; per-group failures are
/// caught + skipped (not fatal). The ONE sanctioned exception to Groups'
/// per-responding-character isolation — chat-creation menu only.
pub async fn group_scenarios_union(db: &Db, character_ids: Vec<String>) -> Response {
    // Trim/drop empties (the query is comma-split upstream, but be faithful).
    let requested: Vec<String> = character_ids
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if requested.is_empty() {
        return Response::Group(json!({ "groupScenarios": [] }));
    }

    let out = with_both_conns(db, move |main, mount| {
        // Only trust character ids the caller can access (user-scoped find →
        // None for unowned/unknown ids), the security invariant.
        let mut character_ids: Vec<String> = Vec::new();
        for id in &requested {
            if characters_read::find_by_id(main, mount, id)?.is_some() {
                character_ids.push(id.clone());
            }
        }
        if character_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Distinct groups any supplied participant belongs to (insertion order —
        // the final sort makes it deterministic).
        let members_repo = GroupCharacterMembersRepository::new(mount);
        let mut group_ids: Vec<String> = Vec::new();
        for cid in &character_ids {
            for gid in members_repo.find_group_ids_by_character_id(cid)? {
                if !group_ids.contains(&gid) {
                    group_ids.push(gid);
                }
            }
        }

        let mut entries: Vec<Value> = Vec::new();
        for gid in &group_ids {
            // findByIdRaw + ensureGroupOfficialStore + ensureScenariosFolder +
            // listGroupScenarios; per-group failure caught + skipped.
            let group_entry: Result<Option<Value>, DbError> = (|| {
                let Some((name, _fk)) = find_name_and_official_mount_point_id_raw(main, gid)? else {
                    return Ok(None);
                };
                let Some(ensured) = ensure_official_store::<GroupEntity>(main, mount, gid, &name)?
                else {
                    return Ok(None);
                };
                scenarios_api::ensure_scenarios_folder(mount, &ensured.mount_point_id);
                let listed = crate::db::scenarios::list_scenarios_in_folder(
                    mount,
                    &ensured.mount_point_id,
                    GROUP_SCENARIOS_FOLDER,
                )?;
                if listed.scenarios.is_empty() {
                    return Ok(None);
                }
                Ok(Some(json!({
                    "groupId": gid,
                    "groupName": name,
                    "mountPointId": ensured.mount_point_id,
                    "scenarios": serde_json::to_value(&listed.scenarios).unwrap_or_else(|_| json!([])),
                    "warnings": listed.warnings,
                })))
            })();
            // v4 catches + logs per-group failures, not fatal.
            if let Ok(Some(entry)) = group_entry {
                entries.push(entry);
            }
        }

        // Stable ordering by group name (bare localeCompare → ICU4X en-US).
        entries.sort_by(|a, b| {
            let na = a.get("groupName").and_then(Value::as_str).unwrap_or("");
            let nb = b.get("groupName").and_then(Value::as_str).unwrap_or("");
            locale_compare(na, nb)
        });
        Ok(entries)
    })
    .await;

    match out {
        Ok(entries) => Response::Group(json!({ "groupScenarios": entries })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Group state (P4.d10 §A — v4 `app/api/v1/groups/[id]/actions/state.ts` at
// `f48f34dc` + the shared `lib/api/state-handlers.ts` factory)
// ===========================================================================

/// v4 `handleGetState` (group): the group's OWN state only — NO cascade (the
/// merge happens on the chat get-state route). Body `{ success, state }`.
pub fn group_state_get(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let result = read_both(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        let Some(group) = repo.find_by_id(&gid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        // v4: `(group.state ?? {})` — nullish coalescing.
        Ok(Some(match group.get("state") {
            Some(Value::Null) | None => json!({}),
            Some(v) => v.clone(),
        }))
    });
    match result {
        Ok(Some(state)) => Response::State(json!({ "success": true, "state": state })),
        Ok(None) => not_found("Group"),
        // v4's catch answers the fixed `serverError('Failed to get state')`.
        Err(e) => {
            eprintln!("[Groups v1] Error getting state: {e}");
            Response::error(ErrorKind::Internal, "Failed to get state")
        }
    }
}

/// v4 `createSetStateHandler` (group config: existence-only check — groups are
/// instance-global). Body `{ success, state: updated?.state || validated.state }`.
pub async fn group_state_set(db: &Db, group_id: &str, state: Value) -> Response {
    if !state.is_object() {
        return Response::error(ErrorKind::BadRequest, "Validation error");
    }
    let gid = group_id.to_string();
    let state_clone = state.clone();
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(None);
        }
        let mut patch = Map::new();
        patch.insert("state".into(), state_clone.clone());
        let updated = repo.update(&gid, &patch).map_err(overlay_to_db)?;
        // v4: `updated?.state || validated.state` (an object, even {}, is truthy).
        let stored = updated
            .as_ref()
            .and_then(|g| match g.get("state") {
                Some(Value::Null) | None => None,
                Some(v) => Some(v.clone()),
            })
            .unwrap_or(state_clone);
        Ok(Some(stored))
    })
    .await;
    match out {
        Ok(Some(state)) => Response::State(json!({ "success": true, "state": state })),
        Ok(None) => not_found("Group"),
        Err(e) => internal(e),
    }
}

/// v4 `createResetStateHandler` (group): `previousState = entity.state || {}`,
/// then replace with `{}`. Body `{ success, previousState }`.
pub async fn group_state_reset(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        let Some(group) = repo.find_by_id(&gid).map_err(overlay_to_db)? else {
            return Ok(None);
        };
        // v4 `entity.state || {}`.
        let previous = match group.get("state") {
            Some(v) if v.is_object() => v.clone(),
            _ => json!({}),
        };
        let mut patch = Map::new();
        patch.insert("state".into(), json!({}));
        repo.update(&gid, &patch).map_err(overlay_to_db)?;
        Ok(Some(previous))
    })
    .await;
    match out {
        Ok(Some(previous)) => {
            Response::State(json!({ "success": true, "previousState": previous }))
        }
        Ok(None) => not_found("Group"),
        // v4's reset handler's own catch → `serverError('Failed to reset state')`.
        Err(e) => {
            eprintln!("[Groups v1] Error resetting state: {e}");
            Response::error(ErrorKind::Internal, "Failed to reset state")
        }
    }
}
