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
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
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
use crate::db::vault_wardrobe_public::{
    create_project_wardrobe_item, delete_project_wardrobe_item, update_project_wardrobe_item,
    WardrobePatch, WardrobePublicError,
};
use crate::db::{archetype_wardrobe, characters_read, DbError};
use crate::services::image_job_common::with_both_conns;
use crate::vault_overlay::WardrobeItem;

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

/// `z.string().regex(/^#(?:[0-9a-fA-F]{3}){1,2}$/)` — `#rgb` or `#rrggbb`. The
/// regex is unanchored-by-nothing (`^`/`$` present, no `m` flag), so it is a
/// whole-string match; JS `\d`-free, ASCII-only classes, so a byte walk is exact.
fn is_valid_hex_color(v: &str) -> bool {
    let Some(rest) = v.strip_prefix('#') else {
        return false;
    };
    if rest.len() != 3 && rest.len() != 6 {
        return false;
    }
    rest.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `z.string().max(N)` on a value already known to be a string.
fn within(v: &str, max: usize) -> bool {
    crate::jsstr::utf16_len(v) <= max
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
        ("color", &is_valid_hex_color),
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
        || bad(&color, &is_valid_hex_color)
        || bad(&icon, &|v| within(v, ICON_MAX))
    {
        return bad_request(VALIDATION_ERROR);
    }
    let input = GroupCreateInput {
        name,
        description: or_null(description.as_deref()),
        instructions: or_null(instructions.as_deref()),
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
        Err(e) => db_error_response(e),
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
    let out = with_both_conns(db, move |main, mount| {
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
    let out = with_both_conns(db, move |main, mount| {
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
        Err(e) => db_error_response(e),
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
            tracing::error!(target: "quilltap::groups", error = %e, "Error resetting state");
            Response::error(ErrorKind::Internal, "Failed to reset state")
        }
    }
}

// ===========================================================================
// Group wardrobe CRUD (P4.D112 — v4 `d7263f39` `groups/[id]/wardrobe/route.ts`
// + `[itemId]/route.ts`). The group tier of the four-tier wardrobe model
// (character vault > group stores > project stores > Quilltap General),
// mirroring the project wardrobe routes. Group and project items share the
// same mount-folder storage, so the writes reuse the mount-scoped helpers in
// `vault_wardrobe_public`. Served DISPATCH-ONLY (the project-wardrobe
// precedent — v4's REST URLs get no quilltap-web edge; the SPA calls the
// dispatch verbs and nothing else consumes the URL).
// ===========================================================================

/// The v4 wardrobe schemas' field bag, validated Zod-faithfully. Both schemas
/// share `wardrobeItemFieldsSchema`; `createWardrobeSchema` uses it as-is and
/// `updateWardrobeSchema` is its `.partial()`. A ZodError escapes the route
/// into v4's middleware, which answers the FLAT 400 `{error: 'Validation
/// error'}` (the `details` issue array is the standing project-wide deferral).
struct WardrobeFields {
    title: Option<String>,
    description: Option<Option<String>>,
    image_prompt: Option<Option<String>>,
    types: Option<Vec<String>>,
    appropriateness: Option<Option<String>>,
    is_default: Option<bool>,
    component_item_ids: Option<Vec<String>>,
    replace: Option<bool>,
}

/// Parse the shared field bag. `partial` = the update schema (every field
/// optional); create additionally requires `title` + `types`. `Err(())` = any
/// ZodError (the caller answers the flat `Validation error`). The checks
/// mirror Zod: `.min(1)` counts UTF-16 units on the RAW string (no trim);
/// `types` elements must be members of the slot enum; the nullable-optional
/// strings accept absent | null | string; `title`/`types`/booleans/
/// `componentItemIds` are optional but NOT nullable. `z.object` is non-strict,
/// so unknown keys are stripped (the builders below read only known fields).
fn parse_wardrobe_fields(body: &Value, partial: bool) -> Result<WardrobeFields, ()> {
    let Some(obj) = body.as_object() else {
        return Err(());
    };
    let get = |k: &str| obj.get(k);

    // title: z.string().min(1) (create) / the same `.optional()` (update).
    let title = match get("title") {
        None if partial => None,
        Some(Value::String(s)) if crate::jsstr::utf16_len(s) >= 1 => Some(s.clone()),
        _ => return Err(()),
    };

    // The three nullable-optional strings.
    let nullable = |k: &str| -> Result<Option<Option<String>>, ()> {
        match get(k) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
            _ => Err(()),
        }
    };
    let description = nullable("description")?;
    let image_prompt = nullable("imagePrompt")?;
    let appropriateness = nullable("appropriateness")?;

    // types: z.array(WardrobeItemTypeEnum).min(1) (`.optional()` on update).
    let types = match get("types") {
        None if partial => None,
        Some(Value::Array(a)) if !a.is_empty() => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                match v.as_str() {
                    Some(s) if crate::wardrobe::WARDROBE_SLOT_TYPES.contains(&s) => {
                        out.push(s.to_string())
                    }
                    _ => return Err(()),
                }
            }
            Some(out)
        }
        _ => return Err(()),
    };

    // isDefault / replace: z.boolean().optional().
    let boolean = |k: &str| -> Result<Option<bool>, ()> {
        match get(k) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            _ => Err(()),
        }
    };
    let is_default = boolean("isDefault")?;
    let replace = boolean("replace")?;

    // componentItemIds: z.array(z.string()).optional().
    let component_item_ids = match get("componentItemIds") {
        None => None,
        Some(Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                match v.as_str() {
                    Some(s) => out.push(s.to_string()),
                    None => return Err(()),
                }
            }
            Some(out)
        }
        _ => return Err(()),
    };

    Ok(WardrobeFields {
        title,
        description,
        image_prompt,
        types,
        appropriateness,
        is_default,
        component_item_ids,
        replace,
    })
}

/// v4's route-level write-error split: a component-cycle rejection from the
/// vault writer surfaces as a plain Error → 400 with ITS message; anything
/// else rethrows into the middleware → the generic 500.
fn group_wardrobe_write_err(e: WardrobePublicError) -> Response {
    match e {
        WardrobePublicError::Cycle(msg) => bad_request(msg),
        _ => Response::error(ErrorKind::Internal, "Internal server error"),
    }
}

/// Resolve the group + ensure its official store AND the `Wardrobe/` folder
/// (the COLLECTION routes' shape — v4 `route.ts` GET/POST both ensure the
/// folder). `Ok(Err(..))` carries the refusal: 404 `Group` when the group is
/// absent, the explicit 500 sentence when the store ensure fails.
fn ensure_group_wardrobe_mount(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    group_id: &str,
) -> Result<Result<String, Response>, DbError> {
    let repo = GroupsRepository::new(main, mount);
    let Some(group) = repo.find_by_id(group_id).map_err(overlay_to_db)? else {
        return Ok(Err(not_found("Group")));
    };
    let name = group.get("name").and_then(Value::as_str).unwrap_or("");
    let Some(ensured) = ensure_official_store::<GroupEntity>(main, mount, group_id, name)? else {
        return Ok(Err(internal("Failed to ensure group document store")));
    };
    let links = DocMountFileLinksRepository::new(mount);
    archetype_wardrobe::ensure_group_wardrobe_folder(&links, &ensured.mount_point_id)?;
    Ok(Ok(ensured.mount_point_id))
}

/// v4 `resolveGroupMount` (the ITEM routes' shape — store ensure only, NO
/// `Wardrobe/` folder ensure): the group is absent OR the ensure fails →
/// `None` → the caller's 404 `Group`.
fn resolve_group_wardrobe_mount(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    group_id: &str,
) -> Result<Option<String>, DbError> {
    let repo = GroupsRepository::new(main, mount);
    let Some(group) = repo.find_by_id(group_id).map_err(overlay_to_db)? else {
        return Ok(None);
    };
    let name = group.get("name").and_then(Value::as_str).unwrap_or("");
    Ok(
        ensure_official_store::<GroupEntity>(main, mount, group_id, name)?
            .map(|e| e.mount_point_id),
    )
}

/// v4 GET `/groups/[id]/wardrobe`: `{ mountPointId, wardrobeItems }` (include
/// archived).
pub fn group_wardrobe_list(db: &Db, group_id: &str) -> Response {
    let gid = group_id.to_string();
    let out = read_both(db, move |main, mount| {
        let mp = match ensure_group_wardrobe_mount(main, mount, &gid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        let docs = DocMountDocumentsRepository::new(mount);
        let items = archetype_wardrobe::read_group_wardrobe(&docs, &mp, true)?;
        Ok(Ok((mp, items)))
    });
    match out {
        Ok(Ok((mp, items))) => {
            Response::Group(json!({ "mountPointId": mp, "wardrobeItems": items }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 POST `/groups/[id]/wardrobe`: find-group FIRST (404 `Group`), THEN parse
/// `createWardrobeSchema` (400 `Validation error`), mint the item id + ISO
/// timestamps IN THE ROUTE, create in the group store, re-list. Body
/// `{ mountPointId, wardrobeItem, wardrobeItems }` (201 at v4's transport).
/// Component cycles → 400 with the writer's message.
pub async fn group_wardrobe_create(db: &Db, group_id: &str, body: Value) -> Response {
    let gid = group_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        // v4 order: repos.groups.findById → notFound BEFORE the schema parse.
        {
            let repo = GroupsRepository::new(main, mount);
            if repo.find_by_id(&gid).map_err(overlay_to_db)?.is_none() {
                return Ok(Err(not_found("Group")));
            }
        }
        let Ok(fields) = parse_wardrobe_fields(&body, false) else {
            return Ok(Err(bad_request(VALIDATION_ERROR)));
        };
        let mp = match ensure_group_wardrobe_mount(main, mount, &gid)? {
            Ok(mp) => mp,
            Err(r) => return Ok(Err(r)),
        };
        // v4 mints id + ISO timestamps + the explicit-null columns.
        let now = crate::clock::now_iso();
        let item = WardrobeItem {
            id: uuid::Uuid::new_v4().to_string(),
            character_id: Some(None),
            title: fields.title.unwrap_or_default(),
            description: Some(fields.description.flatten()),
            image_prompt: Some(fields.image_prompt.flatten()),
            types: fields.types.unwrap_or_default(),
            component_item_ids: fields.component_item_ids.unwrap_or_default(),
            appropriateness: Some(fields.appropriateness.flatten()),
            is_default: fields.is_default.unwrap_or(false),
            replace: fields.replace.unwrap_or(false),
            migrated_from_clothing_record_id: Some(None),
            archived_at: Some(None),
            created_at: now.clone(),
            updated_at: now,
        };
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        let stored = match create_project_wardrobe_item(main, &links, &docs, &mp, &item) {
            Ok(s) => s,
            Err(e) => return Ok(Err(group_wardrobe_write_err(e))),
        };
        // Return the freshly listed items so the client needs no follow-up GET.
        let items = archetype_wardrobe::read_group_wardrobe(&docs, &mp, true)?;
        Ok(Ok((
            mp,
            serde_json::to_value(stored).unwrap_or(Value::Null),
            items,
        )))
    })
    .await;
    match out {
        Ok(Ok((mp, item, items))) => Response::Group(json!({
            "mountPointId": mp,
            "wardrobeItem": item,
            "wardrobeItems": items,
        })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 GET `/groups/[id]/wardrobe/[itemId]`: `{ wardrobeItem }` or the 404s.
pub fn group_wardrobe_get(db: &Db, group_id: &str, item_id: &str) -> Response {
    let gid = group_id.to_string();
    let iid = item_id.to_string();
    let out = read_both(db, move |main, mount| {
        let Some(mp) = resolve_group_wardrobe_mount(main, mount, &gid)? else {
            return Ok(None);
        };
        let docs = DocMountDocumentsRepository::new(mount);
        let items = archetype_wardrobe::read_group_wardrobe(&docs, &mp, true)?;
        Ok(Some(items.into_iter().find(|i| {
            i.get("id").and_then(Value::as_str) == Some(iid.as_str())
        })))
    });
    match out {
        Ok(Some(Some(item))) => Response::Group(json!({ "wardrobeItem": item })),
        Ok(Some(None)) => not_found("Group wardrobe item"),
        Ok(None) => not_found("Group"),
        Err(e) => db_error_response(e),
    }
}

/// v4 PUT `/groups/[id]/wardrobe/[itemId]`: resolve the mount (404 `Group`),
/// parse `updateWardrobeSchema` (400 `Validation error`), apply the patch.
/// Body `{ wardrobeItem }`; cycles → 400; missing item → 404.
pub async fn group_wardrobe_update(
    db: &Db,
    group_id: &str,
    item_id: &str,
    body: Value,
) -> Response {
    let gid = group_id.to_string();
    let iid = item_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let Some(mp) = resolve_group_wardrobe_mount(main, mount, &gid)? else {
            return Ok(Err(not_found("Group")));
        };
        let Ok(fields) = parse_wardrobe_fields(&body, true) else {
            return Ok(Err(bad_request(VALIDATION_ERROR)));
        };
        let patch = WardrobePatch {
            title: fields.title,
            types: fields.types,
            component_item_ids: fields.component_item_ids,
            description: fields.description,
            image_prompt: fields.image_prompt,
            appropriateness: fields.appropriateness,
            is_default: fields.is_default,
            replace: fields.replace,
            archived_at: None,
        };
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        match update_project_wardrobe_item(main, &links, &docs, &mp, &iid, &patch) {
            // Re-read through the overlay so the echo carries the full
            // null-inclusive shape v4's JS object emits (the WardrobeItem
            // struct serialize skips `None` fields; the Value read path
            // renders them as null).
            Ok(Some(_)) => {
                let item = archetype_wardrobe::read_group_wardrobe(&docs, &mp, true)?
                    .into_iter()
                    .find(|i| i.get("id").and_then(Value::as_str) == Some(iid.as_str()))
                    .unwrap_or(Value::Null);
                Ok(Ok(item))
            }
            Ok(None) => Ok(Err(not_found("Group wardrobe item"))),
            Err(e) => Ok(Err(group_wardrobe_write_err(e))),
        }
    })
    .await;
    match out {
        Ok(Ok(item)) => Response::Group(json!({ "wardrobeItem": item })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// v4 DELETE `/groups/[id]/wardrobe/[itemId]`:
/// `removeEquippedItemFromAllChats(itemId)` warn-and-proceed → delete. Body
/// `{ success: true }`; missing item → 404 `Group wardrobe item`.
pub async fn group_wardrobe_delete(db: &Db, group_id: &str, item_id: &str) -> Response {
    let gid = group_id.to_string();
    let iid = item_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let Some(mp) = resolve_group_wardrobe_mount(main, mount, &gid)? else {
            return Ok(Err(not_found("Group")));
        };
        // warn-and-proceed cleanup (v4 wraps this in its own try/catch → warn).
        let _ = ChatOutfitsRepository::new(main).remove_equipped_item_from_all_chats(&iid);
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        match delete_project_wardrobe_item(main, &links, &docs, &mp, &iid) {
            Ok(true) => Ok(Ok(())),
            Ok(false) => Ok(Err(not_found("Group wardrobe item"))),
            Err(e) => Ok(Err(group_wardrobe_write_err(e))),
        }
    })
    .await;
    match out {
        Ok(Ok(())) => Response::Group(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}
