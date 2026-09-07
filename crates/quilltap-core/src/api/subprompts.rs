//! The five character-subprompt verbs (P4.D163, v4 `2f4254b42`):
//! `app/api/v1/characters/[id]/subprompts/route.ts` (GET list / POST create)
//! and `[subpromptId]/route.ts` (GET / PUT / DELETE), as dispatch handlers over
//! [`crate::subprompts`]. The REST edges (`quilltap-web/src/subprompts_routes.rs`)
//! and the dispatch arms both land here, so the guard ladder lives in ONE place.
//!
//! ## The guard ladders — MEASURED on v4's real handlers, not read off the source
//!
//! `subprompts_routes_equivalence` drove v4's routes through the real
//! `createContextParamsHandler` middleware over the committed fixture (the
//! `a6870c5a` guard-order class):
//!
//! - **POST / PUT parse the body FIRST** — `request.json()` + `schema.parse`
//!   sit above the character lookup, so a bad body on a MISSING character is
//!   the Zod 400 (`create_bad_body_missing_character_400_not_404`,
//!   `update_bad_body_missing_character_400_not_404`), and on PUT the Zod
//!   parse precedes even the id check (`update_bad_body_bad_id_400_zod_first`).
//! - **GET / DELETE validate the id first** — a bad id on a missing character
//!   is 400 `Invalid subprompt id` (`get_bad_id_missing_character_400_not_404`,
//!   `delete_bad_id_missing_character_400_not_404`); a good id on a missing
//!   character is 404 `Character not found`.
//! - **The SERVICE's validators run AFTER the character lookup**, inside the
//!   try: a blank title on a missing character is 404, not 400
//!   (`create_service_title_blank_missing_character_404`); a 100-astral-code-
//!   point title passes Zod's code-point `.max(100)` and fails the service's
//!   UTF-16 `.length` rule (`create_service_title_100_astral_400`).
//! - The character lookup is `findByIdRaw` — overlay-free, and an ARCHIVED
//!   character still resolves for reads (`list_d_archived_reads`,
//!   `get_d_keep_archived_reads`); the writes refuse it with the three
//!   per-verb 409 sentences.
//!
//! ## The Zod envelope
//!
//! v4's `validationError(err)` — `{error: 'Validation error', details:
//! err.issues}` at 400 — is [`Response::validation_error`] with the issues
//! built here in Zod 4.5's measured shapes (`CreateZodIssue`, the P4.78
//! constructors): `invalid_type` for an absent / null / wrong-typed key,
//! `too_small` (`minimum: 1`) and `too_big` (`maximum: 100`, CODE POINTS —
//! `jsstr::zod_len_*`), reported in the schema's key order (`title` before
//! `content`, `create_zod_both_bad_two_issues_400`). A body that is not an
//! object at all is `invalid_type expected object` at path `[]`
//! ([`body_not_object_details`], the REST edge's one arm).
//!
//! ## Realtime
//!
//! Every successful write publishes `characters/<id>`; PUT and DELETE first
//! run the fan-out (`chats/<id>` per touched chat) — both through the
//! [`FanoutSeams`] the caller passes (production: the real compiler + bus;
//! the differential: a recorder).

use std::sync::Arc;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};
use crate::services::chat_create::{create_issue_details, CreateZodIssue};
use crate::subprompts::{
    create_character_subprompt, delete_character_subprompt, fan_out_subprompt_change,
    list_character_subprompts, read_character_subprompt, update_character_subprompt, FanoutOptions,
    FanoutSeams, SubpromptCreateInput, SubpromptError, SubpromptPatch, SUBPROMPT_TITLE_MAX_LENGTH,
};

use super::types::{ErrorKind, Response};

/// The routes' `[Characters v1]` lines (v4's global `logger`).
pub const LOG_TARGET: &str = "quilltap::characters";

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn conflict(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Conflict, msg)
}
fn internal(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Internal, msg)
}
fn ok(v: Value) -> Response {
    Response::Character(v)
}

/// Run `f` with BOTH writable connections (the `image_job_common::with_both_conns`
/// shape — the storage ops borrow `main` + `mount` and the API hands them one
/// serialized write).
async fn with_both<T, F>(db: &Db, f: F) -> Result<T, DbError>
where
    T: Send + 'static,
    F: FnOnce(&Connection, &Connection) -> Result<T, DbError> + Send + 'static,
{
    db.write(move |w| {
        let mount = w
            .mount_index()
            .ok_or_else(|| {
                DbError::Internal("subprompts require the mount-index database".to_string())
            })?
            .connection();
        f(w.main().connection(), mount)
    })
    .await
}

// ── The Zod ladder ──────────────────────────────────────────────────────────

fn zod_received(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

fn invalid_type(expected: &'static str, path: Vec<Value>, got: Option<&Value>) -> CreateZodIssue {
    CreateZodIssue::InvalidType {
        expected,
        code: "invalid_type",
        path,
        message: format!(
            "Invalid input: expected {expected}, received {}",
            zod_received(got)
        ),
    }
}

/// `z.string().min(1)` (+ `.max(100)` for the title), `optional` when the
/// schema says so. Zod 4.5 counts CODE POINTS.
fn check_string(
    v: Option<&Value>,
    key: &'static str,
    max: Option<usize>,
    optional: bool,
    issues: &mut Vec<CreateZodIssue>,
) {
    let path = vec![Value::String(key.to_string())];
    let Some(v) = v else {
        if !optional {
            issues.push(invalid_type("string", path, None));
        }
        return;
    };
    let Some(s) = v.as_str() else {
        issues.push(invalid_type("string", path, Some(v)));
        return;
    };
    if !crate::jsstr::zod_len_min_ok(s, 1) {
        issues.push(CreateZodIssue::TooSmall {
            origin: "string",
            code: "too_small",
            minimum: json!(1),
            inclusive: true,
            path,
            message: "Too small: expected string to have >=1 characters".to_string(),
        });
        return;
    }
    if let Some(max) = max {
        if !crate::jsstr::zod_len_max_ok(s, max) {
            issues.push(CreateZodIssue::TooBig {
                origin: "string",
                code: "too_big",
                maximum: json!(max),
                inclusive: true,
                path,
                message: format!("Too big: expected string to have <={max} characters"),
            });
        }
    }
}

/// The `details` array for `schema.parse(<non-object>)` — ONE issue,
/// `invalid_type expected object received <type>` at path `[]`.
pub fn body_not_object_details(body: &Value) -> Value {
    create_issue_details(&[invalid_type("object", vec![], Some(body))])
}

/// The flat dispatch variant's `title` / `content` tri-states, folded back
/// into the object body the Zod ladder reads (absent → key absent; `Some(None)`
/// → `null`; `Some(Some(v))` → `v`).
pub fn flat_body(title: Option<Option<Value>>, content: Option<Option<Value>>) -> Value {
    let mut obj = serde_json::Map::new();
    if let Some(t) = title {
        obj.insert("title".to_string(), t.unwrap_or(Value::Null));
    }
    if let Some(c) = content {
        obj.insert("content".to_string(), c.unwrap_or(Value::Null));
    }
    Value::Object(obj)
}

/// v4 `createSubpromptSchema.parse(body)`: `{title: min1 max100, content: min1}`.
fn parse_create_body(body: &Value) -> Result<SubpromptCreateInput, Response> {
    let Some(obj) = body.as_object() else {
        return Err(Response::validation_error(body_not_object_details(body)));
    };
    let mut issues = Vec::new();
    check_string(
        obj.get("title"),
        "title",
        Some(SUBPROMPT_TITLE_MAX_LENGTH),
        false,
        &mut issues,
    );
    check_string(obj.get("content"), "content", None, false, &mut issues);
    if !issues.is_empty() {
        return Err(Response::validation_error(create_issue_details(&issues)));
    }
    Ok(SubpromptCreateInput {
        title: obj
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        content: obj
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

/// v4 `updateSubpromptSchema.parse(body)`: both optional; `Object.keys(
/// validated)` (the info line's `updatedFields`) is the PRESENT keys in schema
/// order.
fn parse_update_body(body: &Value) -> Result<(SubpromptPatch, Vec<&'static str>), Response> {
    let Some(obj) = body.as_object() else {
        return Err(Response::validation_error(body_not_object_details(body)));
    };
    let mut issues = Vec::new();
    check_string(
        obj.get("title"),
        "title",
        Some(SUBPROMPT_TITLE_MAX_LENGTH),
        true,
        &mut issues,
    );
    check_string(obj.get("content"), "content", None, true, &mut issues);
    if !issues.is_empty() {
        return Err(Response::validation_error(create_issue_details(&issues)));
    }
    let mut keys = Vec::new();
    let title = obj.get("title").and_then(Value::as_str).map(str::to_string);
    if title.is_some() {
        keys.push("title");
    }
    let content = obj
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_string);
    if content.is_some() {
        keys.push("content");
    }
    Ok((SubpromptPatch { title, content }, keys))
}

/// v4 `repos.characters.findByIdRaw(characterId)` + `exists` → 404.
fn character_exists(main: &Connection, character_id: &str) -> Result<bool, DbError> {
    Ok(characters_read::find_by_id_raw(main, character_id)?.is_some())
}

fn log_error(
    action: &str,
    character_id: &str,
    subprompt_id: Option<&str>,
    e: &dyn std::fmt::Display,
) {
    match subprompt_id {
        Some(sid) => tracing::error!(
            target: LOG_TARGET,
            character_id = %character_id,
            subprompt_id = %sid,
            error = %e,
            "[Characters v1] Error {action} subprompt"
        ),
        None => tracing::error!(
            target: LOG_TARGET,
            character_id = %character_id,
            error = %e,
            "[Characters v1] Error {action} subprompt"
        ),
    }
}

// ── GET /characters/[id]/subprompts ─────────────────────────────────────────

pub async fn character_subprompt_list(db: &Db, character_id: &str) -> Response {
    let cid = character_id.to_string();
    let out = db.read_main(|main| {
        db.read_mount_index(|mount| {
            if !character_exists(main, &cid)? {
                return Ok(Err(not_found("Character")));
            }
            Ok(Ok(list_character_subprompts(main, mount, &cid)?))
        })
    });
    match out {
        Ok(Ok(subprompts)) => {
            tracing::debug!(
                target: LOG_TARGET,
                character_id = %character_id,
                count = subprompts.len(),
                "[Characters v1] Listed subprompts"
            );
            ok(json!({ "subprompts": subprompts }))
        }
        Ok(Err(r)) => r,
        Err(e) => {
            tracing::error!(
                target: LOG_TARGET,
                character_id = %character_id,
                error = %e,
                "[Characters v1] Error listing subprompts"
            );
            internal("Failed to list subprompts")
        }
    }
}

// ── POST /characters/[id]/subprompts ────────────────────────────────────────

pub async fn character_subprompt_create(
    db: &Db,
    user_id: &str,
    character_id: &str,
    body: Value,
    seams: Arc<dyn FanoutSeams>,
) -> Response {
    // The body parses BEFORE the character lookup (measured).
    let input = match parse_create_body(&body) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let cid = character_id.to_string();
    let exists = db.read_main(|main| character_exists(main, &cid));
    match exists {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => {
            log_error("creating", character_id, None, &e);
            return internal("Failed to create subprompt");
        }
    }
    let cid = character_id.to_string();
    let out = with_both(db, move |main, mount| {
        Ok(create_character_subprompt(main, mount, &cid, &input))
    })
    .await;
    match out {
        Ok(Ok(subprompt)) => {
            tracing::info!(
                target: LOG_TARGET,
                character_id = %character_id,
                user_id = %user_id,
                subprompt_id = %subprompt.id,
                "[Characters v1] Subprompt created"
            );
            seams.publish_character(character_id);
            ok(json!({ "subprompt": subprompt }))
        }
        Ok(Err(SubpromptError::Archived { .. })) => {
            conflict("Character is archived; subprompts cannot be added")
        }
        Ok(Err(SubpromptError::Validation(e))) => bad_request(e.0),
        Ok(Err(e)) => {
            log_error("creating", character_id, None, &e);
            internal("Failed to create subprompt")
        }
        Err(e) => {
            log_error("creating", character_id, None, &e);
            internal("Failed to create subprompt")
        }
    }
}

// ── GET /characters/[id]/subprompts/[subpromptId] ───────────────────────────

pub async fn character_subprompt_get(db: &Db, character_id: &str, subprompt_id: &str) -> Response {
    // The id validates FIRST, then the character (measured).
    if !crate::subprompts::is_valid_subprompt_id(subprompt_id) {
        return bad_request("Invalid subprompt id");
    }
    let (cid, sid) = (character_id.to_string(), subprompt_id.to_string());
    let out = db.read_main(|main| {
        db.read_mount_index(|mount| {
            if !character_exists(main, &cid)? {
                return Ok(Err(not_found("Character")));
            }
            Ok(Ok(read_character_subprompt(main, mount, &cid, &sid)))
        })
    });
    match out {
        Ok(Ok(Ok(Some(subprompt)))) => ok(json!({ "subprompt": subprompt })),
        Ok(Ok(Ok(None))) => not_found("Subprompt"),
        Ok(Err(r)) => r,
        Ok(Ok(Err(e))) => {
            log_error("reading", character_id, Some(subprompt_id), &e);
            internal("Failed to read subprompt")
        }
        Err(e) => {
            log_error("reading", character_id, Some(subprompt_id), &e);
            internal("Failed to read subprompt")
        }
    }
}

// ── PUT /characters/[id]/subprompts/[subpromptId] ───────────────────────────

pub async fn character_subprompt_update(
    db: &Db,
    user_id: &str,
    character_id: &str,
    subprompt_id: &str,
    body: Value,
    seams: Arc<dyn FanoutSeams>,
) -> Response {
    // Body FIRST, then the id, then the character (measured: a bad body on a
    // bad id is the Zod 400; a good body on a bad id is `Invalid subprompt id`).
    let (patch, updated_fields) = match parse_update_body(&body) {
        Ok(p) => p,
        Err(r) => return r,
    };
    if !crate::subprompts::is_valid_subprompt_id(subprompt_id) {
        return bad_request("Invalid subprompt id");
    }
    let cid = character_id.to_string();
    match db.read_main(|main| character_exists(main, &cid)) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => {
            log_error("updating", character_id, Some(subprompt_id), &e);
            return internal("Failed to update subprompt");
        }
    }
    let (cid, sid) = (character_id.to_string(), subprompt_id.to_string());
    let fan_seams = seams.clone();
    let out = with_both(db, move |main, mount| {
        let subprompt = match update_character_subprompt(main, mount, &cid, &sid, &patch) {
            Ok(s) => s,
            Err(e) => return Ok(Err(e)),
        };
        let fanout = fan_out_subprompt_change(
            main,
            mount,
            &cid,
            &sid,
            FanoutOptions::default(),
            fan_seams.as_ref(),
        );
        Ok(Ok((subprompt, fanout)))
    })
    .await;
    match out {
        Ok(Ok((subprompt, fanout))) => {
            tracing::info!(
                target: LOG_TARGET,
                character_id = %character_id,
                user_id = %user_id,
                subprompt_id = %subprompt_id,
                updated_fields = ?updated_fields,
                chats_touched = fanout.chats_touched,
                seats_recompiled = fanout.seats_recompiled,
                "[Characters v1] Subprompt updated"
            );
            seams.publish_character(character_id);
            ok(json!({ "subprompt": subprompt }))
        }
        Ok(Err(SubpromptError::Archived { .. })) => {
            conflict("Character is archived; subprompts cannot be edited")
        }
        Ok(Err(SubpromptError::NotFound(_))) => not_found("Subprompt"),
        Ok(Err(SubpromptError::Validation(e))) => bad_request(e.0),
        Ok(Err(e)) => {
            log_error("updating", character_id, Some(subprompt_id), &e);
            internal("Failed to update subprompt")
        }
        Err(e) => {
            log_error("updating", character_id, Some(subprompt_id), &e);
            internal("Failed to update subprompt")
        }
    }
}

// ── DELETE /characters/[id]/subprompts/[subpromptId] ────────────────────────

pub async fn character_subprompt_delete(
    db: &Db,
    user_id: &str,
    character_id: &str,
    subprompt_id: &str,
    seams: Arc<dyn FanoutSeams>,
) -> Response {
    if !crate::subprompts::is_valid_subprompt_id(subprompt_id) {
        return bad_request("Invalid subprompt id");
    }
    let cid = character_id.to_string();
    match db.read_main(|main| character_exists(main, &cid)) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => {
            log_error("deleting", character_id, Some(subprompt_id), &e);
            return internal("Failed to delete subprompt");
        }
    }
    let (cid, sid) = (character_id.to_string(), subprompt_id.to_string());
    let fan_seams = seams.clone();
    let out = with_both(db, move |main, mount| {
        let deleted = match delete_character_subprompt(main, mount, &cid, &sid) {
            Ok(d) => d,
            Err(e) => return Ok(Err(e)),
        };
        if !deleted {
            // v4: `if (!deleted) return notFound('Subprompt')` — no fan-out.
            return Ok(Ok(None));
        }
        let fanout = fan_out_subprompt_change(
            main,
            mount,
            &cid,
            &sid,
            FanoutOptions {
                remove_selection: true,
            },
            fan_seams.as_ref(),
        );
        Ok(Ok(Some(fanout)))
    })
    .await;
    match out {
        Ok(Ok(Some(fanout))) => {
            tracing::info!(
                target: LOG_TARGET,
                character_id = %character_id,
                user_id = %user_id,
                subprompt_id = %subprompt_id,
                chats_touched = fanout.chats_touched,
                seats_recompiled = fanout.seats_recompiled,
                "[Characters v1] Subprompt deleted"
            );
            seams.publish_character(character_id);
            ok(json!({ "success": true }))
        }
        Ok(Ok(None)) => not_found("Subprompt"),
        Ok(Err(SubpromptError::Archived { .. })) => {
            conflict("Character is archived; subprompts cannot be deleted")
        }
        Ok(Err(e)) => {
            log_error("deleting", character_id, Some(subprompt_id), &e);
            internal("Failed to delete subprompt")
        }
        Err(e) => {
            log_error("deleting", character_id, Some(subprompt_id), &e);
            internal("Failed to delete subprompt")
        }
    }
}

#[cfg(test)]
mod log_tests {
    //! The routes' `[Characters v1]` lines, capture-pinned at v4's levels with
    //! v4's bags — the info lines carry `userId`, and PUT/DELETE fold the
    //! fan-out's `{chatsTouched, seatsRecompiled}` into the info line ONLY,
    //! never into the body (§C.2).
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::test_support::captured_with;
    use std::sync::Mutex;

    const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR_A: &str = "a1000000-0000-4000-8000-0000000000a1";
    const USER: &str = "11111111-1111-4111-8111-111111111111";

    struct Quiet(Mutex<Vec<String>>);
    impl FanoutSeams for Quiet {
        fn compile(
            &self,
            _m: &Connection,
            _mo: &Connection,
            _c: &Value,
            _p: &str,
        ) -> Result<(), DbError> {
            Ok(())
        }
        fn publish_chat(&self, _c: &str) {}
        fn publish_character(&self, id: &str) {
            self.0.lock().unwrap().push(id.to_string());
        }
    }

    fn open_db(dir: &tempfile::TempDir) -> Db {
        let fx = |n: &str| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../quilltap-web/tests/fixtures")
                .join(n)
        };
        let main = dir.path().join("main.db");
        let mount = dir.path().join("mount.db");
        std::fs::copy(fx("subprompts-main.db"), &main).unwrap();
        std::fs::copy(fx("subprompts-mount.db"), &mount).unwrap();
        Db::open(
            DbPaths {
                main,
                mount_index: Some(mount),
                llm_logs: None,
            },
            TEST_PEPPER,
        )
        .unwrap()
    }

    fn line<'a>(lines: &'a [String], needle: &str) -> &'a String {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no `{needle}` line in {lines:?}"))
    }

    #[test]
    fn the_five_info_and_debug_lines_carry_v4s_bags() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(&dir);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let seams: Arc<Quiet> = Arc::new(Quiet(Mutex::new(vec![])));
        let dyn_seams: Arc<dyn FanoutSeams> = seams.clone();

        let (_, lines) = captured_with(|| rt.block_on(character_subprompt_list(&db, CHAR_A)));
        let l = line(&lines, "[Characters v1] Listed subprompts");
        assert!(l.starts_with("DEBUG quilltap::characters"), "{l}");
        assert!(
            l.contains(&format!("character_id={CHAR_A}")) && l.contains("count=5"),
            "{l}"
        );

        let (r, lines) = captured_with(|| {
            rt.block_on(character_subprompt_create(
                &db,
                USER,
                CHAR_A,
                json!({ "title": "Logged", "content": "x" }),
                dyn_seams.clone(),
            ))
        });
        assert!(matches!(r, Response::Character(_)), "{r:?}");
        let l = line(&lines, "[Characters v1] Subprompt created");
        assert!(l.starts_with("INFO quilltap::characters"), "{l}");
        assert!(
            l.contains(&format!("user_id={USER}")) && l.contains("subprompt_id=logged"),
            "{l}"
        );

        let (r, lines) = captured_with(|| {
            rt.block_on(character_subprompt_update(
                &db,
                USER,
                CHAR_A,
                "terse",
                json!({ "title": "Brief" }),
                dyn_seams.clone(),
            ))
        });
        // The counts are in the LINE, never in the body.
        let Response::Character(body) = r else {
            panic!("{r:?}")
        };
        assert!(
            body.get("chatsTouched").is_none() && body["subprompt"].get("chatsTouched").is_none()
        );
        let l = line(&lines, "[Characters v1] Subprompt updated");
        assert!(l.starts_with("INFO quilltap::characters"), "{l}");
        assert!(l.contains("updated_fields=[\"title\"]"), "{l}");
        assert!(
            l.contains("chats_touched=2") && l.contains("seats_recompiled=2"),
            "{l}"
        );

        let (r, lines) = captured_with(|| {
            rt.block_on(character_subprompt_delete(
                &db,
                USER,
                CHAR_A,
                "terse",
                dyn_seams.clone(),
            ))
        });
        assert!(matches!(r, Response::Character(_)), "{r:?}");
        let l = line(&lines, "[Characters v1] Subprompt deleted");
        assert!(l.starts_with("INFO quilltap::characters"), "{l}");
        assert!(
            l.contains("subprompt_id=terse")
                && l.contains("chats_touched=2")
                && l.contains("seats_recompiled=2"),
            "{l}"
        );

        // Every successful write published `characters/<id>` — three writes.
        assert_eq!(seams.0.lock().unwrap().as_slice(), [CHAR_A, CHAR_A, CHAR_A]);

        // A refusal publishes NOTHING and logs no `[Characters v1]` info line.
        let (r, lines) = captured_with(|| {
            rt.block_on(character_subprompt_delete(
                &db,
                USER,
                CHAR_A,
                "gone",
                dyn_seams.clone(),
            ))
        });
        assert!(matches!(r, Response::Error(_)), "{r:?}");
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("[Characters v1] Subprompt deleted")),
            "{lines:?}"
        );
        assert_eq!(seams.0.lock().unwrap().len(), 3);
    }

    /// The `[Characters v1] Error listing subprompts` arm (error level): a
    /// main partition without the tables makes the character lookup throw.
    #[test]
    fn a_db_failure_logs_the_error_line_and_answers_v4s_fixed_sentence() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.db");
        let _ = crate::db::Writer::open_writable(&empty, TEST_PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: empty,
                mount_index: None,
                llm_logs: None,
            },
            TEST_PEPPER,
        )
        .unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (r, lines) = captured_with(|| rt.block_on(character_subprompt_list(&db, CHAR_A)));
        match r {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert_eq!(e.message, "Failed to list subprompts");
            }
            other => panic!("{other:?}"),
        }
        let l = line(&lines, "[Characters v1] Error listing subprompts");
        assert!(l.starts_with("ERROR quilltap::characters"), "{l}");
        assert!(
            l.contains(&format!("character_id={CHAR_A}")) && l.contains("error="),
            "{l}"
        );
    }
}
