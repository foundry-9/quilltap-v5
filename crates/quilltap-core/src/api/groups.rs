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
//! passes `SINGLE_USER_ID`. v4's ownership guard (`checkOwnership` until
//! `55752ad4` renamed it `exists`) collapses to NotFound-on-absent
//! for the single-user v5.

use serde_json::{json, Map, Value};

use crate::collation::locale_compare;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::document_store_overlay::OverlayError;
use crate::db::group_character_members::GroupCharacterMembersRepository;
use crate::db::group_doc_mount_links::GroupDocMountLinksRepository;
use crate::db::groups::{GroupCreateInput, GroupCreateOptions, GroupProperties, GroupsRepository};
use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};
use crate::services::image_job_common::{with_both_conns, with_both_conns_for, EntityRef};

use super::scenarios as scenarios_api;
use super::types::{db_error_response, ErrorKind, Response};
use super::wardrobe_route_factory::GROUP_WARDROBE;

// ===========================================================================
// Shared helpers
// ===========================================================================

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

/// v4 `groups/route.ts` GET: `findAll` → the optional `characterIds` membership
/// filter → createdAt-desc sort → `_count.members` enrichment. Body
/// `{ groups: [...] }`.
///
/// `character_ids` is v4's `?characterIds=` (`d1c06cd9d`, added for the Scenario
/// Builder's save dialog): when PRESENT — even empty — only the groups any of
/// those characters belongs to survive. Each requested id is trusted only if the
/// user-scoped `characters.findById` finds it (a miss is skipped silently, the
/// same rule as `groups/scenarios`); the union of their `groupId`s filters the
/// list BEFORE the sort and the enrichment. `None` is today's unfiltered list.
pub fn group_list(db: &Db, user_id: &str, character_ids: Option<Vec<String>>) -> Response {
    let result = read_both(db, |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        let mut groups = repo.find_all().map_err(overlay_to_db)?;
        if let Some(raw) = &character_ids {
            // v4 splits the query on `,`, trims, and drops empties; the dispatch
            // field is already split, so only the trim + filter remain.
            let requested: Vec<String> = raw
                .iter()
                .map(|s| crate::jsstr::js_trim(s).to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let members_repo = GroupCharacterMembersRepository::new(mount);
            let mut member_group_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for character_id in &requested {
                if characters_read::find_by_id(main, mount, character_id)?.is_none() {
                    continue;
                }
                member_group_ids.extend(members_repo.find_group_ids_by_character_id(character_id)?);
            }
            groups.retain(|g| {
                g.get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| member_group_ids.contains(id))
            });
            tracing::debug!(
                userId = %user_id,
                requested = requested.len(),
                matched = groups.len(),
                "[Groups v1] Filtered groups by character membership"
            );
        }
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
        Err(e) => db_error_response(e),
    }
}

/// Bridge `OverlayError` into a `DbError` inside a read closure (the closures are
/// `-> Result<_, DbError>`); an unavailable store becomes a `DbError::Internal`.
fn overlay_to_db(e: OverlayError) -> DbError {
    // Structure-preserving (P4.23): the `Unavailable` refusal survives as
    // `DbError::StoreUnavailable` so the terminal arm can answer v4's
    // contextful 503 instead of a 500 + leaked detail.
    e.into_db()
}

// ===========================================================================
// v4's two group validators (`createGroupSchema` / `updateGroupSchema`)
// ===========================================================================

// P4.D103 (v4 `8f868109`) ported BOTH schemas whole. Before this, `group_create`
// hand-checked only a trimmed non-empty name and `group_update` was a RAW
// passthrough patch map with no validation at all (its doc comment claimed
// `updateGroupSchema.parse`, which was stale). The drift commit lands on these
// validators, so porting only the new `instructions` field would have been a
// half-port.
//
// v4:
//   createGroupSchema = z.object({
//     name:         z.string().min(1, 'Name is required').max(100),
//     description:  z.string().max(2000).nullable().optional(),
//     instructions: z.string().max(10000).nullable().optional(),
//     color:        z.string().regex(/^#(?:[0-9a-fA-F]{3}){1,2}$/).nullable().optional(),
//     icon:         z.string().max(50).nullable().optional(),
//   })
//   updateGroupSchema = the same five, with `name` `.min(1).max(100).optional()`
//                       (optional but NOT nullable — an explicit null 400s).
//
// Three behaviors that are easy to lose:
//  - `.min(1)` is on the RAW string. A name of `"   "` is LENGTH 3 and PASSES;
//    v5's old `name.trim().is_empty()` check wrongly rejected it.
//  - a ZodError surfaces through v4's middleware as the FLAT 400
//    `{error: 'Validation error'}`; the `details` issue array is the standing
//    project-wide deferral (the P4.6ay-unit-12 precedent, same as the wardrobe
//    archetype routes). The top-level sentence is NOT `'Name is required'` —
//    that string only ever appears inside `details`.
//  - `z.object` is non-strict, so unknown keys are STRIPPED before the patch
//    reaches `repos.groups.update`. v5's raw passthrough used to write them.
//
// Lengths are UTF-16 code units (Zod uses JS `.length`).

/// The `error` sentence v4's middleware emits for any uncaught `ZodError`.
const VALIDATION_ERROR: &str = "Validation error";

const NAME_MAX: usize = 100;
const DESCRIPTION_MAX: usize = 2000;
const INSTRUCTIONS_MAX: usize = 10_000;
const ICON_MAX: usize = 50;

/// `z.string().max(N)` on a value already known to be a string.
fn within(v: &str, max: usize) -> bool {
    // Zod ≥ 4.5.4 counts code points once the UTF-16 length overflows (v4
    // `6e1a64ea6`; `jsstr::zod_len_max_ok` carries the rule and its window).
    crate::jsstr::zod_len_max_ok(v, max)
}

/// One `z.string()…nullable().optional()` field read off a raw patch map.
/// `Ok(None)` = the key is absent (Zod omits it from the parse output);
/// `Ok(Some(Value::Null))` = an explicit null (Zod keeps it);
/// `Ok(Some(Value::String))` = a valid string; `Err(())` = a ZodError.
fn parse_nullable_string(
    patch: &Map<String, Value>,
    key: &str,
    check: impl Fn(&str) -> bool,
) -> Result<Option<Value>, ()> {
    match patch.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(Value::Null)),
        Some(Value::String(s)) if check(s) => Ok(Some(Value::String(s.clone()))),
        Some(_) => Err(()),
    }
}

/// `updateGroupSchema.parse(body)` — validate, and return ONLY the recognized
/// keys, in v4's schema-declaration order (unknown keys stripped by the
/// non-strict `z.object`).
fn parse_update_group(patch: &Map<String, Value>) -> Result<Map<String, Value>, ()> {
    let mut out = Map::new();
    // `name: z.string().min(1).max(100).optional()` — optional but NOT nullable.
    match patch.get("name") {
        None => {}
        Some(Value::String(s)) if !s.is_empty() && within(s, NAME_MAX) => {
            out.insert("name".to_string(), Value::String(s.clone()));
        }
        Some(_) => return Err(()),
    }
    for (key, check) in [
        (
            "description",
            &(|v: &str| within(v, DESCRIPTION_MAX)) as &dyn Fn(&str) -> bool,
        ),
        ("instructions", &|v: &str| within(v, INSTRUCTIONS_MAX)),
        ("color", &crate::api::zod_issues::zod_hex_color_ok),
        ("icon", &|v: &str| within(v, ICON_MAX)),
    ] {
        if let Some(v) = parse_nullable_string(patch, key, check)? {
            out.insert(key.to_string(), v);
        }
    }
    Ok(out)
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
    instructions: Option<String>,
    color: Option<String>,
    icon: Option<String>,
) -> Response {
    // `createGroupSchema.parse(body)`. The typed verb has already done the
    // non-strict `z.object`'s unknown-key strip and the "is it a string" checks;
    // what remains is the length / regex half. `min(1)` is on the RAW name — a
    // whitespace-only name is length 3 and PASSES, exactly as v4's does.
    if name.is_empty() || !within(&name, NAME_MAX) {
        return bad_request(VALIDATION_ERROR);
    }
    let bad = |v: &Option<String>, ok: &dyn Fn(&str) -> bool| matches!(v, Some(s) if !ok(s));
    if bad(&description, &|v| within(v, DESCRIPTION_MAX))
        || bad(&instructions, &|v| within(v, INSTRUCTIONS_MAX))
        || bad(&color, &crate::api::zod_issues::zod_hex_color_ok)
        || bad(&icon, &|v| within(v, ICON_MAX))
    {
        return bad_request(VALIDATION_ERROR);
    }
    let input = GroupCreateInput {
        name,
        description: or_null(description.as_deref()),
        instructions: or_null(instructions.as_deref()),
        state: json!({}),
    };
    // v4 `groups/route.ts:89-90` `color: validatedData.color || null, icon: …
    // || null` — an absent or `null` colour/icon, or an empty icon, is STORED as
    // an explicit `null` (dogfood #136), so the bag goes in whole. An empty
    // colour never gets here: the hex check above 400s it.
    let properties = GroupProperties {
        color: Some(or_null(color.as_deref())),
        icon: Some(or_null(icon.as_deref())),
    };
    let out = with_both_conns(db, move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        repo.create_with_properties(&input, &properties, &GroupCreateOptions::default())
            .map_err(overlay_to_db)
    })
    .await;
    let group = match out {
        Ok(group) => group,
        Err(e) => return db_error_response(e),
    };
    // v4 `groups/route.ts:95-98`: `logger.info('[Groups v1] Group created',
    // { groupId: group.id, name: group.name })` — v5 never logged it before
    // P4.D295 (the create hook below follows it).
    let group_id = group
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let group_name = group.get("name").and_then(Value::as_str).unwrap_or("");
    tracing::info!(groupId = %group_id, name = %group_name, "[Groups v1] Group created");
    // v4 `groups/route.ts:100-115` (`545e299bb`): when the row has an official
    // store, ensure `Scenarios/` through the scenario location (L1; a folder
    // failure logs the location's L2 and RETHROWS) and `Knowledge/` in the
    // ROW's store, caught as WARN `[Groups v1] Failed to ensure group
    // Scenarios/Knowledge folders on create` (L38). Non-fatal. A second writer
    // turn so the lines keep v4's order (INFO, then L1 / L2, then L38) — before
    // P4.D295 the ensure ran inside the create's closure and dropped both
    // errors silently.
    if let Some(official) = group
        .get("officialMountPointId")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
    {
        let gid = group_id.clone();
        let ensured = with_both_conns(db, move |main, mount| {
            let attempt = || -> Result<(), crate::scenarios::location::ScenarioLocationError> {
                crate::scenarios::location::resolve_store_scenario_location(
                    main,
                    mount,
                    crate::scenarios::location::ScenarioScope::Group,
                    Some(&gid),
                    // A group location never reads the user.
                    super::engine::SINGLE_USER_ID,
                    true,
                )?;
                scenarios_api::ensure_group_knowledge_folder(mount, &official)
            };
            Ok(attempt())
        })
        .await;
        let failure = match ensured {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(e) => Some(crate::db::fallback::error_text(&e)),
        };
        if let Some(error) = failure {
            tracing::warn!(
                groupId = %group_id,
                error = %error,
                "[Groups v1] Failed to ensure group Scenarios/Knowledge folders on create"
            );
        }
    }
    Response::Group(json!({ "group": group }))
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
        Err(e) => db_error_response(e),
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
                    // Archiving never removes membership edges; the badge lets
                    // the list reconcile "6 members / 4 can speak" (spec §5.2,
                    // v4 `d553f72a`).
                    "archivedAt": c.get("archivedAt").cloned().unwrap_or(Value::Null),
                }));
            }
        }
        Ok(Some(members))
    });
    match result {
        Ok(Some(members)) => Response::Group(json!({ "members": members })),
        Ok(None) => not_found("Group"),
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Update / delete (v4 group-crud.ts)
// ===========================================================================

/// The three terminal shapes of `group_update`'s one round-trip — v4 checks
/// existence FIRST (`findById` → `notFound`) and reads/parses the body only
/// after, so a missing group answers 404 even for a garbage patch (the same
/// guard order `project_update` mirrors on the P4.55 side).
enum GroupUpdateOutcome {
    NotFound,
    Invalid,
    Updated(Value),
}

/// v4 `handlePutDefault`: ownership FIRST (`findById` → 404) →
/// `updateGroupSchema.parse` (no `|| null`, so an explicit `""` is stored
/// VERBATIM — v4's quirk, which its own client compensates for by sending
/// `instructions || null`) → `groups.update`. Body `{ group }`. A non-object
/// body is Zod's own root-level `invalid_type` → the same 400 `Validation
/// error` (v4's `req.json()` succeeded; `parse(5)` is a plain ZodError).
///
/// The find-before-parse order was caught by the §3 unification review — the
/// lane's first port parsed first and answered 400 where v4 answers 404;
/// pinned by the `update_missing_group_invalid_body_404` arm.
pub async fn group_update(db: &Db, group_id: &str, patch: Value) -> Response {
    let gid = group_id.to_string();
    let out = with_both_conns_for(db, EntityRef::Group(gid.clone()), move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(GroupUpdateOutcome::NotFound);
        }
        let Some(patch_map) = patch.as_object().and_then(|p| parse_update_group(p).ok()) else {
            return Ok(GroupUpdateOutcome::Invalid);
        };
        Ok(
            match repo.update(&gid, &patch_map).map_err(overlay_to_db)? {
                Some(group) => GroupUpdateOutcome::Updated(group),
                None => GroupUpdateOutcome::NotFound,
            },
        )
    })
    .await;
    match out {
        Ok(GroupUpdateOutcome::Updated(group)) => Response::Group(json!({ "group": group })),
        Ok(GroupUpdateOutcome::NotFound) => not_found("Group"),
        Ok(GroupUpdateOutcome::Invalid) => bad_request(VALIDATION_ERROR),
        Err(e) => db_error_response(e),
    }
}

/// v4 `handleDeleteGroup`: ownership → drop memberships + additional-store links
/// → delete the group row (the OFFICIAL store is orphaned). Body `{ success: true }`.
pub async fn group_delete(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let out = with_both_conns_for(db, EntityRef::Group(gid.clone()), move |main, mount| {
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
        Err(e) => db_error_response(e),
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
    let out = with_both_conns_for(db, EntityRef::Group(gid.clone()), move |main, mount| {
        let repo = GroupsRepository::new(main, mount);
        if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
            return Ok(Err(not_found("Group")));
        }
        let Some(character) = characters_read::find_by_id(main, mount, &cid)? else {
            return Ok(Err(bad_request("Character not found")));
        };
        // Archived characters can't join a group; existing memberships are
        // never removed by archiving (spec §5.1, v4 `d553f72a`).
        if crate::api::characters::is_archived(&character) {
            return Ok(Err(bad_request(
                "That character is archived; rehydrate them before adding them to a group.",
            )));
        }
        GroupCharacterMembersRepository::new(mount).add_member(&gid, &cid)?;
        Ok(Ok(()))
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Group(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 `handleRemoveMember`: ownership → `removeMember` (result ignored — always
/// success). Body `{ success: true }`.
pub async fn group_member_remove(db: &Db, group_id: &str, character_id: &str) -> Response {
    let gid = group_id.to_string();
    let cid = character_id.to_string();
    let out = with_both_conns_for(db, EntityRef::Group(gid.clone()), move |main, mount| {
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
    }
}

/// v4 POST `/mount-points`: ownership('Group') → mount-point existence
/// (`notFound('Mount point')`) → idempotent `link`. Body `{ link, mountPoint }`.
pub async fn group_mount_point_link(db: &Db, group_id: &str, mount_point_id: &str) -> Response {
    let gid = group_id.to_string();
    let mp_id = mount_point_id.to_string();
    let out = with_both_conns_for(db, EntityRef::Group(gid.clone()), move |main, mount| {
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
        Err(e) => db_error_response(e),
    }
}

/// v4 DELETE `/mount-points`: ownership → `unlink`; no link → `badRequest`. Body
/// `{ message: 'Mount point unlinked from group' }`.
pub async fn group_mount_point_unlink(db: &Db, group_id: &str, mount_point_id: &str) -> Response {
    let gid = group_id.to_string();
    let mp_id = mount_point_id.to_string();
    let out = with_both_conns_for(db, EntityRef::Group(gid.clone()), move |main, mount| {
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
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Scenarios (v4 groups/[id]/scenarios/route.ts + [scenarioPath]/route.ts,
// groups/scenarios/route.ts) — thin configs over the ONE scenario route
// factory ([`super::scenarios`], v4 `scenario-item-route-factory.ts`,
// `545e299bb`); the group tier's `afterEnsure` heals `Knowledge/` beside
// `Scenarios/` on every write and every collection call.
// ===========================================================================

/// v4 the group scenario routes' factory config: `'Group'`, `'[Groups v1]'`,
/// `'groupId'`, `afterEnsure: ensureGroupKnowledgeFolder`.
pub const GROUP_SCENARIO_ROUTES: scenarios_api::ScenarioRouteConfig =
    scenarios_api::ScenarioRouteConfig {
        scope: crate::scenarios::location::ScenarioScope::Group,
        owner_label: "Group",
        log_tag: "[Groups v1]",
        log_id_key: scenarios_api::OwnerLogKey::GroupId,
        ensure_knowledge: true,
        wrap: Response::Group,
    };

/// v4 `GET /api/v1/groups/[id]/scenarios`.
pub async fn group_scenario_list(
    db: &Db,
    group_id: &str,
    include_archived: bool,
    user_id: &str,
) -> Response {
    scenarios_api::collection_list(
        db,
        GROUP_SCENARIO_ROUTES,
        Some(group_id.to_string()),
        include_archived,
        user_id.to_string(),
    )
    .await
}

/// v4 `POST /api/v1/groups/[id]/scenarios`.
pub async fn group_scenario_create(db: &Db, group_id: &str, bag: Value, user_id: &str) -> Response {
    scenarios_api::collection_create(
        db,
        GROUP_SCENARIO_ROUTES,
        Some(group_id.to_string()),
        bag,
        user_id.to_string(),
    )
    .await
}

/// v4 `GET /api/v1/groups/[id]/scenarios/[scenarioPath]`.
pub async fn group_scenario_get(
    db: &Db,
    group_id: &str,
    scenario_path: &str,
    user_id: &str,
) -> Response {
    scenarios_api::item_get(
        db,
        GROUP_SCENARIO_ROUTES,
        Some(group_id.to_string()),
        scenario_path.to_string(),
        user_id.to_string(),
    )
    .await
}

/// v4 `PUT /api/v1/groups/[id]/scenarios/[scenarioPath]`.
pub async fn group_scenario_update(
    db: &Db,
    group_id: &str,
    scenario_path: &str,
    bag: Value,
    include_archived: bool,
    user_id: &str,
) -> Response {
    scenarios_api::item_update(
        db,
        GROUP_SCENARIO_ROUTES,
        Some(group_id.to_string()),
        scenario_path.to_string(),
        bag,
        include_archived,
        user_id.to_string(),
    )
    .await
}

/// v4 `POST /api/v1/groups/[id]/scenarios/[scenarioPath]?action=rename`.
pub async fn group_scenario_rename(
    db: &Db,
    group_id: &str,
    scenario_path: &str,
    new_filename: &str,
    include_archived: bool,
    user_id: &str,
) -> Response {
    scenarios_api::item_rename(
        db,
        GROUP_SCENARIO_ROUTES,
        Some(group_id.to_string()),
        scenario_path.to_string(),
        new_filename.to_string(),
        include_archived,
        user_id.to_string(),
    )
    .await
}

/// v4 `DELETE /api/v1/groups/[id]/scenarios/[scenarioPath]`.
pub async fn group_scenario_delete(
    db: &Db,
    group_id: &str,
    scenario_path: &str,
    include_archived: bool,
    user_id: &str,
) -> Response {
    scenarios_api::item_delete(
        db,
        GROUP_SCENARIO_ROUTES,
        Some(group_id.to_string()),
        scenario_path.to_string(),
        include_archived,
        user_id.to_string(),
    )
    .await
}

/// v4 `GET /api/v1/groups/scenarios?characterIds=` (`groups/scenarios/route.ts:
/// 28-99`) — the participant-union. For every group that ANY supplied
/// (user-owned) character is a member of: resolve the group's scenario
/// location with `ensure` (L1; heals `Scenarios/`, and a folder failure
/// RETHROWS), skip a group with no location or an empty list, else
/// `{groupId, groupName: <the location's owner name — name || 'Group'>,
/// mountPointId, scenarios, warnings}`. Any per-group throw logs WARN `[Groups
/// v1] Failed to load scenarios for a group; skipping {groupId, error}` (L20 —
/// v5 swallowed it silently before P4.D295) and skips. Sorted by `groupName`
/// (bare `localeCompare` → ICU4X en-US). The ONE sanctioned exception to
/// Groups' per-responding-character isolation — a chat-creation menu only. No
/// `Knowledge/` ensure on this route (v4 has none).
pub async fn group_scenarios_union(
    db: &Db,
    character_ids: Vec<String>,
    include_archived: bool,
    user_id: &str,
) -> Response {
    // Trim/drop empties (the query is comma-split upstream, but be faithful).
    let requested: Vec<String> = character_ids
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if requested.is_empty() {
        return Response::Group(json!({ "groupScenarios": [] }));
    }
    let user_id = user_id.to_string();

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
            let entry = (|| -> Result<Option<Value>, crate::scenarios::location::ScenarioLocationError> {
                let Some(location) = crate::scenarios::location::resolve_store_scenario_location(
                    main,
                    mount,
                    crate::scenarios::location::ScenarioScope::Group,
                    Some(gid),
                    &user_id,
                    true,
                )?
                else {
                    return Ok(None);
                };
                let Some(location_id) = location.id().map(str::to_string) else {
                    return Ok(None);
                };
                let listed = location.list(mount, include_archived)?;
                if listed.scenarios.is_empty() {
                    return Ok(None);
                }
                Ok(Some(json!({
                    "groupId": location_id,
                    "groupName": location.owner_name(),
                    "mountPointId": location.mount_point_id(),
                    "scenarios": serde_json::to_value(&listed.scenarios).unwrap_or_else(|_| json!([])),
                    "warnings": listed.warnings,
                })))
            })();
            match entry {
                Ok(Some(e)) => entries.push(e),
                Ok(None) => {}
                Err(error) => tracing::warn!(
                    groupId = %gid,
                    error = %error,
                    "[Groups v1] Failed to load scenarios for a group; skipping"
                ),
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
        Err(e) => db_error_response(e),
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
            tracing::error!(target: "quilltap::groups", error = %e, "Error getting state");
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
    let out = with_both_conns_for(db, EntityRef::Group(gid.clone()), move |main, mount| {
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
        Err(e) => db_error_response(e),
    }
}

/// v4 `createResetStateHandler` (group): `previousState = entity.state || {}`,
/// then replace with `{}`. Body `{ success, previousState }`.
pub async fn group_state_reset(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let out = with_both_conns_for(db, EntityRef::Group(gid.clone()), move |main, mount| {
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
            tracing::error!(target: "quilltap::groups", error = %e, "Error resetting state");
            Response::error(ErrorKind::Internal, "Failed to reset state")
        }
    }
}

// ===========================================================================
// Group wardrobe CRUD (P4.D112 — v4 `groups/[id]/wardrobe/route.ts` +
// `[itemId]/route.ts`, at `9ecebf986` registrations of the one route factory;
// P4.D281). The group tier of the four-tier wardrobe model (character vault >
// group stores > project stores > Quilltap General). Served DISPATCH-ONLY (the
// project-wardrobe precedent — v4's REST URLs get no quilltap-web edge; the
// SPA calls the dispatch verbs and nothing else consumes the URL).
// ===========================================================================

/// The engine's single user (the group tier has no ownership gate; v4's INFO
/// lines log it as `userId`).
const WARDROBE_USER_ID: &str = super::engine::SINGLE_USER_ID;

/// v4 GET `/groups/[id]/wardrobe`: `{ mountPointId, wardrobeItems }` (origin +
/// wear); the store and its `Wardrobe/` folder are ensured.
pub async fn group_wardrobe_list(db: &Db, group_id: &str, include_archived: bool) -> Response {
    super::wardrobe_route_factory::list_store_items(
        db,
        GROUP_WARDROBE,
        Some(group_id),
        WARDROBE_USER_ID,
        include_archived,
    )
    .await
}

/// v4 POST `/groups/[id]/wardrobe`: Zod FIRST (an invalid body never
/// provisions), then the store → 201 `{ wardrobeItem }` (origin + wear).
pub async fn group_wardrobe_create(db: &Db, group_id: &str, body: Value) -> Response {
    super::wardrobe_route_factory::create(
        db,
        GROUP_WARDROBE,
        Some(group_id),
        WARDROBE_USER_ID,
        &body,
    )
    .await
}

/// v4 GET `/groups/[id]/wardrobe/[itemId]`: `{ wardrobeItem }` (origin + wear)
/// or the 404s; never provisions.
pub fn group_wardrobe_get(db: &Db, group_id: &str, item_id: &str) -> Response {
    super::wardrobe_route_factory::item_get(
        db,
        GROUP_WARDROBE,
        Some(group_id),
        WARDROBE_USER_ID,
        item_id,
    )
}

/// v4 GET `/groups/[id]/wardrobe/[itemId]?action=wear-history`.
pub(crate) fn group_wardrobe_wear_history(db: &Db, group_id: &str, item_id: &str) -> Response {
    super::wardrobe_route_factory::item_wear_history(
        db,
        GROUP_WARDROBE,
        Some(group_id),
        WARDROBE_USER_ID,
        item_id,
    )
}

/// v4 PUT `/groups/[id]/wardrobe/[itemId]`: 404 → Zod → `imageFileId` →
/// `updateItem` → `{ wardrobeItem }` (origin + wear).
pub async fn group_wardrobe_update(
    db: &Db,
    group_id: &str,
    item_id: &str,
    body: Value,
) -> Response {
    super::wardrobe_route_factory::item_update(
        db,
        GROUP_WARDROBE,
        Some(group_id),
        WARDROBE_USER_ID,
        item_id,
        body,
    )
    .await
}

/// v4 DELETE `/groups/[id]/wardrobe/[itemId]`: `{ success: true }`.
pub async fn group_wardrobe_delete(db: &Db, group_id: &str, item_id: &str) -> Response {
    super::wardrobe_route_factory::item_delete(
        db,
        GROUP_WARDROBE,
        Some(group_id),
        WARDROBE_USER_ID,
        item_id,
    )
    .await
}

// ===========================================================================
// `?action=instructions` — the group store's dressing instructions
// (v4 `b86bb1a5`; through the factory since `9ecebf986`)
// ===========================================================================

/// v4 `GET /api/v1/groups/[id]/wardrobe?action=instructions` (`ensure: true`).
pub async fn group_wardrobe_instructions_get(db: &Db, group_id: &str) -> Response {
    super::wardrobe_route_factory::instructions_get_store(
        db,
        GROUP_WARDROBE,
        Some(group_id),
        WARDROBE_USER_ID,
    )
    .await
}

/// v4 `POST /api/v1/groups/[id]/wardrobe?action=instructions` — the body is
/// parsed FIRST now, then the store is ensured.
pub async fn group_wardrobe_instructions_set(
    db: &Db,
    group_id: &str,
    instructions: &Option<Option<Value>>,
) -> Response {
    super::wardrobe_route_factory::instructions_set(
        db,
        GROUP_WARDROBE,
        Some(group_id),
        WARDROBE_USER_ID,
        instructions,
    )
    .await
}

// === P4.D325: inventory (the tier's route-factory config) ===
/// `groups/[id]/inventory` (+ `/[itemId]`) — v4's thin route files' config:
/// `{ scope: 'group', logTag: '[Groups v1]', logIdKey: 'groupId' }`. The handlers are
/// the factory's (`super::inventory_route_factory`).
pub const GROUP_INVENTORY: super::inventory_route_factory::InventoryRouteConfig =
    super::inventory_route_factory::InventoryRouteConfig {
        scope: crate::inventory::types::InventoryScope::Group,
        log_tag: "[Groups v1]",
        log_id_key: "groupId",
    };
// === end P4.D325 ===
