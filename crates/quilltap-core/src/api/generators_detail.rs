//! P4.9K1 — the per-character generator verbs (§B.2 of the `p4.9k` contract):
//! `characterRename`, `characterRefreshArchive`, and (later units)
//! `characterGenerateExternalPrompt` / `characterOptimize`. v4 mounts all four
//! on `POST /api/v1/characters/[id]?action=` (`app/api/v1/characters/[id]/
//! handlers/post.ts`); v5's SPA drives them over `/api/dispatch`, and the REST
//! edge (`quilltap-web::characters_routes::characters_action_post`) delegates
//! into these same handlers.
//!
//! ## The guard order, MEASURED on v4's route (the lane record's item 0)
//!
//! `handlePost` runs `repos.characters.findById(id)` and answers 404
//! `Character not found` BEFORE it validates the action or reads a body, so
//! every arm here resolves the character first. Then, per action:
//!
//! * `rename` — `renameSchema.parse(body)` (an uncaught `ZodError` → the
//!   middleware's 400 `{error: 'Validation error', details}`), then the
//!   route-level 400 `At least one replacement must be specified`, then the
//!   service inside a try whose catch answers 500 `Failed to process rename
//!   request`. NOTE the service receives the already-loaded character.
//! * `refresh-archive` — no body; the arm's own catch answers 500 `Failed to
//!   refresh conversation archive`.
//!
//! Neither arm carries an archived-character refusal: v4 hides the tabs
//! client-side (`CharacterDetailView.tsx:415`) and the server runs the request
//! — an executed rename on a tombstone then trips the repository's archive
//! write guard, which lands in the catch as the 500 above. Pinned by the
//! differential's `execute_archived` arm.

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};
use crate::generators::refresh_archive::refresh_archive;
use crate::generators::rename::{run_character_rename, RenameRequest, ReplacementPair};

use super::settings::zod_parsed_type;
use super::types::{db_error_response, ErrorKind, Response};

// ===========================================================================
// Shared helpers
// ===========================================================================

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

/// v4's ownership gate: `repos.characters.findById(id)` (the OVERLAID read) +
/// `exists`. A broken vault's `CharacterVaultUnavailableError` answers the
/// contextful 503 (P4.23), as every other `[id]` read in this tree does.
fn require_character(db: &Db, character_id: &str) -> Result<Value, Response> {
    let cid = character_id.to_string();
    match db.read_main(|main| {
        db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
    }) {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(not_found("Character")),
        Err(e) => Err(db_error_response(e)),
    }
}

// ===========================================================================
// Zod 4 issue rendering (the `chat_create::CreateZodIssue` idiom, as values)
// ===========================================================================

/// `invalid_type` — Zod 4's key order `expected, code, path, message`.
fn invalid_type(expected: &str, path: &[Value], got: Option<&Value>) -> Value {
    json!({
        "expected": expected,
        "code": "invalid_type",
        "path": path,
        "message": format!("Invalid input: expected {expected}, received {}", zod_parsed_type(got)),
    })
}

/// `z.string().min(n, message)` — `origin, code, minimum, inclusive, path, message`.
fn too_small_string(minimum: usize, path: &[Value], message: &str) -> Value {
    json!({
        "origin": "string",
        "code": "too_small",
        "minimum": minimum,
        "inclusive": true,
        "path": path,
        "message": message,
    })
}

fn push_path(path: &[Value], key: Value) -> Vec<Value> {
    let mut p = path.to_vec();
    p.push(key);
    p
}

/// v4 `renameReplacementPairSchema` over one value at `path`:
/// `oldValue: z.string().min(1, 'Find value is required')`, `newValue:
/// z.string().min(1, 'Replace value is required')`, `caseSensitive:
/// z.boolean().default(false)`. Issues are appended in schema key order; a
/// non-object answers ONE `invalid_type` and does not descend.
fn parse_pair(v: &Value, path: &[Value], issues: &mut Vec<Value>) -> Option<ReplacementPair> {
    let Some(obj) = v.as_object() else {
        issues.push(invalid_type("object", path, Some(v)));
        return None;
    };
    let mut ok = true;
    let mut old_value = String::new();
    let mut new_value = String::new();
    for (key, message, slot) in [
        ("oldValue", "Find value is required", &mut old_value),
        ("newValue", "Replace value is required", &mut new_value),
    ] {
        let p = push_path(path, Value::String(key.into()));
        match obj.get(key) {
            Some(Value::String(s)) => {
                if !crate::jsstr::zod_len_min_ok(s, 1) {
                    issues.push(too_small_string(1, &p, message));
                    ok = false;
                } else {
                    *slot = s.clone();
                }
            }
            other => {
                issues.push(invalid_type("string", &p, other));
                ok = false;
            }
        }
    }
    let case_sensitive = match obj.get("caseSensitive") {
        None => false,
        Some(Value::Bool(b)) => *b,
        other => {
            issues.push(invalid_type(
                "boolean",
                &push_path(path, Value::String("caseSensitive".into())),
                other,
            ));
            ok = false;
            false
        }
    };
    if ok {
        Some(ReplacementPair {
            old_value,
            new_value,
            case_sensitive,
        })
    } else {
        None
    }
}

/// v4 `renameSchema.parse(body)` over the three raw body fields the verb
/// carries. `Err` is the middleware's `details` array (every issue, in schema
/// order — Zod collects them all).
pub fn parse_rename_body(
    primary_rename: Option<&Value>,
    additional_replacements: Option<&Value>,
    dry_run: Option<&Value>,
) -> Result<RenameRequest, Vec<Value>> {
    let mut issues: Vec<Value> = Vec::new();

    // `primaryRename: pairSchema.optional()` — absent is fine; `null` is not
    // (`.optional()` admits only `undefined`).
    let primary = match primary_rename {
        None => None,
        Some(v) => parse_pair(v, &[Value::String("primaryRename".into())], &mut issues),
    };

    // `additionalReplacements: z.array(pairSchema).default([])`.
    let mut additional: Vec<ReplacementPair> = Vec::new();
    match additional_replacements {
        None => {}
        Some(Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                let path = vec![
                    Value::String("additionalReplacements".into()),
                    Value::from(i),
                ];
                if let Some(p) = parse_pair(item, &path, &mut issues) {
                    additional.push(p);
                }
            }
        }
        other => issues.push(invalid_type(
            "array",
            &[Value::String("additionalReplacements".into())],
            other,
        )),
    }

    // `dryRun: z.boolean().default(true)`.
    let dry_run = match dry_run {
        None => true,
        Some(Value::Bool(b)) => *b,
        other => {
            issues.push(invalid_type(
                "boolean",
                &[Value::String("dryRun".into())],
                other,
            ));
            true
        }
    };

    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(RenameRequest {
        primary_rename: primary,
        additional_replacements: additional,
        dry_run,
    })
}

// ===========================================================================
// characterRename (v4 `post.ts:425-445`)
// ===========================================================================

/// v4's `rename` action, whole: 404 → Zod 400 → the neither-pair 400 → the
/// service (on the writer pair, one transaction) → the catch-all 500.
pub async fn character_rename(
    db: &Db,
    user_id: &str,
    character_id: &str,
    primary_rename: Option<&Value>,
    additional_replacements: Option<&Value>,
    dry_run: Option<&Value>,
) -> Response {
    let character = match require_character(db, character_id) {
        Ok(c) => c,
        Err(r) => return r,
    };

    let request = match parse_rename_body(primary_rename, additional_replacements, dry_run) {
        Ok(r) => r,
        Err(issues) => return Response::validation_error(Value::Array(issues)),
    };

    if request.primary_rename.is_none() && request.additional_replacements.is_empty() {
        return Response::error(
            ErrorKind::BadRequest,
            "At least one replacement must be specified",
        );
    }

    let uid = user_id.to_string();
    let cid = character_id.to_string();
    let outcome = db
        .write(move |writers| {
            let mount = writers
                .mount_index()
                .ok_or_else(|| DbError::Internal("no mount-index database".into()))?
                .connection();
            let main = writers.main().connection();
            run_character_rename(main, mount, &character, &request, &uid)
        })
        .await;

    match outcome {
        Ok(result) => Response::Character(serde_json::to_value(result).unwrap_or(Value::Null)),
        Err(e) => {
            tracing::error!(
                character_id = %cid,
                error = %e,
                "[Characters v1] Error processing rename"
            );
            Response::error(ErrorKind::Internal, "Failed to process rename request")
        }
    }
}

// ===========================================================================
// characterRefreshArchive (v4 `post.ts:336-370`)
// ===========================================================================

/// v4's `refresh-archive` action: 404, then the render enqueue over every chat
/// the character sits in; the arm's own catch answers the fixed 500.
pub async fn character_refresh_archive(db: &Db, user_id: &str, character_id: &str) -> Response {
    if let Err(r) = require_character(db, character_id) {
        return r;
    }
    match refresh_archive(db, user_id, character_id).await {
        Ok(body) => Response::Character(body),
        Err(e) => {
            tracing::error!(
                character_id = %character_id,
                error = %e,
                "[Characters v1] Error refreshing conversation archive"
            );
            Response::error(
                ErrorKind::Internal,
                "Failed to refresh conversation archive",
            )
        }
    }
}
