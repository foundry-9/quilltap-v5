//! The text-replacement-rules dispatch handlers (P4.6ak, lane A) — the
//! route-logic backfill the composer autocorrect (`TextReplacementPlugin`, ported
//! client-side by lane B) reads its rules from.
//!
//! Each handler is a differential port of a v4
//! `app/api/v1/settings/text-replacements` route handler (the oracle), composed
//! over the already-ported [`crate::db::text_replacement_rules`] repo. Rules are
//! **global — no `userId` scoping** (v4's single-user model). Reads go through
//! [`Db::read_main`]; the four mutators through [`Db::write`] (the single-writer
//! discipline — `bulkReplace`'s delete-all + re-insert is one write closure, i.e.
//! one transaction, the Rust analogue of v4's `safeQuery` wrap).
//!
//! Each handler returns a [`Response::TextReplacement`] carrying v4's RAW route
//! body (`{rules, count}` / `{rule}` / null-for-204); the REST edge unwraps the
//! dispatch envelope to that body (the P4.6ah lesson). The conflict arm surfaces
//! v4's `TextReplacementRuleConflictError` message verbatim (the repo's
//! [`TrrError::Conflict`] Display is byte-identical) as an [`ErrorKind::Conflict`]
//! → HTTP 409.
//!
//! Input validation ports v4's `TextReplacementRuleInputSchema` constraints
//! (fromText 1..100 + no leading/trailing whitespace, toText 1..1000, the three
//! defaulted fields). The EXACT multi-issue Zod message wording is the standing
//! Zod-throw-message seam (the `settings.rs` `Invalid <label>` precedent) — the
//! composer only ever sends valid input, so the differential does not drive the
//! invalid-input arm; the refine message (`fromText cannot have leading or
//! trailing whitespace`) is carried verbatim from the schema.

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::text_replacement_rules::{
    CreateOptions, TextReplacementRuleRow, TextReplacementRulesRepository, TrrBulkRow, TrrCreate,
    TrrError, TrrUpdate,
};
use crate::db::DbError;

use super::types::{ErrorKind, Response};

// ===========================================================================
// Shared helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn conflict(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Conflict, msg)
}
fn not_found(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::NotFound, msg)
}

/// v4's 404 message for a missing rule id. The routes call `notFound(`Text
/// replacement rule ${id} not found`)`, and v4's `notFound(resource)` helper
/// appends ` not found` to its argument — so the real body carries the phrase
/// TWICE. Ported faithfully (a v4 quirk the differential proves).
fn missing_rule_message(id: &str) -> String {
    format!("Text replacement rule {id} not found not found")
}

fn row_value(row: &TextReplacementRuleRow) -> Value {
    serde_json::to_value(row).unwrap_or(Value::Null)
}
fn rows_value(rows: &[TextReplacementRuleRow]) -> Value {
    Value::Array(rows.iter().map(row_value).collect())
}

/// The `Result<Result<T, TrrError>, DbError>` a write closure hands back —
/// infra failure at the outer layer, the conflict/not-found repo outcome at the
/// inner. Collapse it to a [`Response`] via the caller's `on_ok`.
fn resolve_write<T>(
    result: Result<Result<T, TrrError>, DbError>,
    on_ok: impl FnOnce(T) -> Response,
) -> Response {
    match result {
        Ok(Ok(v)) => on_ok(v),
        Ok(Err(e @ TrrError::Conflict { .. })) => conflict(e.to_string()),
        Ok(Err(TrrError::Db(e))) => internal(e),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// v4 TextReplacementRuleInputSchema validation
// ===========================================================================

struct ValidatedInput {
    from_text: String,
    to_text: String,
    case_sensitive: bool,
    enabled: bool,
    sort_order: i64,
}

/// Port v4's `TextReplacementRuleInputSchema.safeParse` constraints. Returns the
/// v4 issue string (`path: message`) on the first failure (the route joins these
/// with `; ` and prefixes `Invalid rule: ` / `Invalid rule at index N: `). Only
/// the first issue is surfaced — the exact multi-issue join is the Zod-message
/// seam (see the module header). `.length` bounds use char count (the UTF-16
/// code-unit count Zod uses matches for the ASCII composer rules).
fn validate_input(v: &Value) -> Result<ValidatedInput, String> {
    let o = v
        .as_object()
        .ok_or_else(|| "(root): Expected object, received non-object".to_string())?;

    let from_text = require_string(o, "fromText")?;
    let n = from_text.chars().count();
    if n < 1 {
        return Err("fromText: String must contain at least 1 character(s)".to_string());
    }
    if n > 100 {
        return Err("fromText: String must contain at most 100 character(s)".to_string());
    }
    if from_text.trim() != from_text {
        return Err("fromText: fromText cannot have leading or trailing whitespace".to_string());
    }

    let to_text = require_string(o, "toText")?;
    let m = to_text.chars().count();
    if m < 1 {
        return Err("toText: String must contain at least 1 character(s)".to_string());
    }
    if m > 1000 {
        return Err("toText: String must contain at most 1000 character(s)".to_string());
    }

    let case_sensitive = optional_bool(o, "caseSensitive", false)?;
    let enabled = optional_bool(o, "enabled", true)?;
    let sort_order = optional_int(o, "sortOrder", 0)?;

    Ok(ValidatedInput {
        from_text,
        to_text,
        case_sensitive,
        enabled,
        sort_order,
    })
}

fn require_string(o: &serde_json::Map<String, Value>, key: &str) -> Result<String, String> {
    match o.get(key) {
        None | Some(Value::Null) => Err(format!("{key}: Required")),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(format!("{key}: Expected string")),
    }
}

fn optional_bool(
    o: &serde_json::Map<String, Value>,
    key: &str,
    default: bool,
) -> Result<bool, String> {
    match o.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("{key}: Expected boolean")),
    }
}

fn optional_int(
    o: &serde_json::Map<String, Value>,
    key: &str,
    default: i64,
) -> Result<i64, String> {
    match o.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(num)) => match num.as_i64() {
            Some(i) => Ok(i),
            // A non-integer number (v4 `.int()`).
            None => Err(format!("{key}: Expected integer, received float")),
        },
        Some(_) => Err(format!("{key}: Expected number")),
    }
}

// ===========================================================================
// Handlers (v4 route bodies)
// ===========================================================================

/// v4 `GET /api/v1/settings/text-replacements` → `{ rules, count }`, ordered by
/// sortOrder then createdAt.
pub fn list(db: &Db) -> Response {
    match db.read_main(|conn| {
        TextReplacementRulesRepository::new(conn)
            .list(false)
            .map_err(trr_db_only)
    }) {
        Ok(rules) => {
            let count = rules.len();
            Response::TextReplacement(json!({ "rules": rows_value(&rules), "count": count }))
        }
        Err(e) => internal(e),
    }
}

/// v4 `POST /api/v1/settings/text-replacements` (default action) → `created({
/// rule })` (201). Duplicate `(fromText, caseSensitive)` → the conflict arm.
pub async fn create(db: &Db, body: &Value) -> Response {
    let input = match validate_input(body) {
        Ok(i) => i,
        Err(msg) => return bad_request(format!("Invalid rule: {msg}")),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let row = TextReplacementRuleRow {
        id: id.clone(),
        from_text: input.from_text.clone(),
        to_text: input.to_text.clone(),
        case_sensitive: input.case_sensitive,
        enabled: input.enabled,
        sort_order: input.sort_order,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let create = TrrCreate {
        from_text: input.from_text,
        to_text: input.to_text,
        case_sensitive: input.case_sensitive,
        enabled: input.enabled,
        sort_order: input.sort_order,
    };
    let opts = CreateOptions {
        id,
        created_at: now.clone(),
        updated_at: now,
    };
    let result = db
        .write(move |w| {
            let repo = w.main().text_replacement_rules();
            Ok(match repo.create(&create, &opts) {
                Ok(()) => Ok(row),
                Err(e) => Err(e),
            })
        })
        .await;
    resolve_write(result, |row| {
        Response::TextReplacement(json!({ "rule": row_value(&row) }))
    })
}

/// v4 `PATCH /api/v1/settings/text-replacements/[id]` → `{ rule }`; a missing id
/// → 404 with v4's message; a colliding `(fromText, caseSensitive)` → 409.
pub async fn update(db: &Db, id: &str, body: &Value) -> Response {
    let patch = match validate_patch(body) {
        Ok(p) => p,
        Err(msg) => return bad_request(format!("Invalid patch: {msg}")),
    };
    let now = crate::clock::now_iso();
    let upd = TrrUpdate {
        from_text: patch.from_text,
        to_text: patch.to_text,
        case_sensitive: patch.case_sensitive,
        enabled: patch.enabled,
        sort_order: patch.sort_order,
        updated_at: now,
    };
    let idc = id.to_string();
    let id_for_msg = id.to_string();
    let result = db
        .write(move |w| {
            let repo = w.main().text_replacement_rules();
            Ok(match repo.update(&idc, &upd) {
                Ok(false) => Ok(None),
                Ok(true) => match repo.find_by_id(&idc) {
                    Ok(row) => Ok(row),
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            })
        })
        .await;
    resolve_write(
        result,
        move |row: Option<TextReplacementRuleRow>| match row {
            Some(row) => Response::TextReplacement(json!({ "rule": row_value(&row) })),
            None => not_found(missing_rule_message(&id_for_msg)),
        },
    )
}

/// v4 `DELETE /api/v1/settings/text-replacements/[id]` → `noContent()` (204, no
/// body); a missing id → 404 with v4's message. The dispatch carries `null` for
/// the 204; the REST edge maps it to an empty 204 response.
pub async fn delete(db: &Db, id: &str) -> Response {
    let idc = id.to_string();
    let id_for_msg = id.to_string();
    let result = db
        .write(move |w| {
            let repo = w.main().text_replacement_rules();
            Ok(match repo.delete(&idc) {
                Ok(b) => Ok(b),
                Err(e) => Err(e),
            })
        })
        .await;
    resolve_write(result, move |deleted| {
        if deleted {
            Response::TextReplacement(Value::Null)
        } else {
            not_found(missing_rule_message(&id_for_msg))
        }
    })
}

/// v4 `POST /api/v1/settings/text-replacements?action=bulk-replace` →
/// `successResponse({ rules, count })` — the transactional full-list replace.
pub async fn bulk_replace(db: &Db, body: &Value) -> Response {
    let Some(arr) = body.get("rules").and_then(Value::as_array) else {
        return bad_request("Body must include a `rules` array");
    };
    let mut bulk_rows: Vec<TrrBulkRow> = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let input = match validate_input(item) {
            Ok(v) => v,
            Err(msg) => return bad_request(format!("Invalid rule at index {i}: {msg}")),
        };
        let now = crate::clock::now_iso();
        bulk_rows.push(TrrBulkRow {
            data: TrrCreate {
                from_text: input.from_text,
                to_text: input.to_text,
                case_sensitive: input.case_sensitive,
                enabled: input.enabled,
                sort_order: input.sort_order,
            },
            opts: CreateOptions {
                id: uuid::Uuid::new_v4().to_string(),
                created_at: now.clone(),
                updated_at: now,
            },
        });
    }
    let result = db
        .write(move |w| {
            let repo = w.main().text_replacement_rules();
            Ok(match repo.bulk_replace(&bulk_rows) {
                Ok(rows) => Ok(rows),
                Err(e) => Err(e),
            })
        })
        .await;
    resolve_write(result, |rows: Vec<TextReplacementRuleRow>| {
        let count = rows.len();
        Response::TextReplacement(json!({ "rules": rows_value(&rows), "count": count }))
    })
}

// ===========================================================================
// Patch validation (v4 TextReplacementRulePatchSchema = Input.partial())
// ===========================================================================

struct ValidatedPatch {
    from_text: Option<String>,
    to_text: Option<String>,
    case_sensitive: Option<bool>,
    enabled: Option<bool>,
    sort_order: Option<i64>,
}

/// Port v4's `TextReplacementRulePatchSchema` (the Input schema `.partial()`):
/// every field optional; a PROVIDED field still satisfies its constraint. Absent
/// fields stay `None` (v4's partial short-circuits before the field default).
fn validate_patch(v: &Value) -> Result<ValidatedPatch, String> {
    let o = v
        .as_object()
        .ok_or_else(|| "(root): Expected object, received non-object".to_string())?;

    let from_text = match o.get("fromText") {
        None => None,
        Some(Value::String(s)) => {
            let n = s.chars().count();
            if n < 1 {
                return Err("fromText: String must contain at least 1 character(s)".to_string());
            }
            if n > 100 {
                return Err("fromText: String must contain at most 100 character(s)".to_string());
            }
            if s.trim() != s {
                return Err(
                    "fromText: fromText cannot have leading or trailing whitespace".to_string(),
                );
            }
            Some(s.clone())
        }
        Some(_) => return Err("fromText: Expected string".to_string()),
    };

    let to_text = match o.get("toText") {
        None => None,
        Some(Value::String(s)) => {
            let m = s.chars().count();
            if m < 1 {
                return Err("toText: String must contain at least 1 character(s)".to_string());
            }
            if m > 1000 {
                return Err("toText: String must contain at most 1000 character(s)".to_string());
            }
            Some(s.clone())
        }
        Some(_) => return Err("toText: Expected string".to_string()),
    };

    let case_sensitive = match o.get("caseSensitive") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err("caseSensitive: Expected boolean".to_string()),
    };
    let enabled = match o.get("enabled") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err("enabled: Expected boolean".to_string()),
    };
    let sort_order = match o.get("sortOrder") {
        None => None,
        Some(Value::Number(num)) => match num.as_i64() {
            Some(i) => Some(i),
            None => return Err("sortOrder: Expected integer, received float".to_string()),
        },
        Some(_) => return Err("sortOrder: Expected number".to_string()),
    };

    Ok(ValidatedPatch {
        from_text,
        to_text,
        case_sensitive,
        enabled,
        sort_order,
    })
}

/// A read-path helper: `list`/`find_by_id` never conflict, so map their
/// [`TrrError`] straight to [`DbError`] (the Conflict arm is unreachable there).
fn trr_db_only(e: TrrError) -> DbError {
    match e {
        TrrError::Db(d) => d,
        TrrError::Conflict { .. } => unreachable!("read path cannot conflict"),
    }
}
