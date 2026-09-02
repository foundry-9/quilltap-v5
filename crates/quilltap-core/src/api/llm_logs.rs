//! The llm-logs dispatch handlers (P4.6ar, lane A) — the read/delete surface the
//! Salon's LLM Inspector panel (lane B) reads.
//!
//! Each handler is a differential port of a v4 `app/api/v1/llm-logs` route
//! handler (the oracle), composed over the [`crate::db::llm_logs`] repo reads.
//! The logs live in the **llm-logs partition**, so every read goes through
//! [`Db::read_llm_logs`] and the two ownership checks read the MAIN partition.
//! A missing llm-logs partition is a plain engine error, not a v4-diffed arm:
//! v4 always has the partition (its `getCollection()` override throws
//! "LLM logs database not initialized" without one), and a v5 instance opened
//! without it has no logs to inspect.
//!
//! Each handler returns a [`Response::LlmLog`] carrying v4's RAW route body; the
//! REST edge unwraps the dispatch envelope to those bytes (the P4.6ah lesson).
//!
//! ## The three v4 quirks this surface carries (all pinned by the differential)
//!
//! 1. **Pagination is applied POST-fetch, and inconsistently.** The route reads
//!    `total` from the pre-slice length, then slices by `offset`, then slices by
//!    `limit`, and reports `count` from the post-slice length. The three ENTITY
//!    branches (`messageId`/`chatId`/`characterId`) fetch UNBOUNDED and are
//!    bounded only by that route-level slice, while `standalone`/`type`/recent
//!    pass `limit` into the repo AND get sliced again.
//! 2. **A garbage `limit` disables the slice entirely.** `parseInt('garbage')` is
//!    `NaN`; `Math.min(NaN, cap)` is `NaN`; `logs.length > NaN` is `false`. So
//!    `?limit=garbage` returns EVERYTHING, and the echoed `limit` serializes as
//!    JSON `null` (`JSON.stringify(NaN)`). See [`js_parse_int_10`] / [`js_min`] /
//!    [`js_number_or_null`] — Rust's `f64::min` would silently swallow the NaN
//!    and return the cap, which is exactly the divergence those helpers exist to
//!    prevent.
//! 3. **The item routes have NO ownership check.** `GET`/`DELETE
//!    /api/v1/llm-logs/[id]` read by id alone — any log, any user. Faithful port;
//!    the differential's `item_get_other_user` case reads a log the session user
//!    does not own and proves the 200.
//!
//! A fourth quirk lives one layer down, in the repo: `?standalone=true` can never
//! return a row (v4's `$eq: null` lowers to `col = NULL`). See
//! [`crate::db::llm_logs::LLMLogsRepository::find_standalone`].

use serde_json::{json, Value};

use crate::db::llm_logs::{LLMLogsRepository, LlmLogRow};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read, DbError};

use super::types::{ErrorKind, Response};

// ===========================================================================
// Shared helpers
// ===========================================================================

/// Both partitions in one closure (the `projects.rs` idiom) — the character
/// existence check needs the vault overlay's mount-index connection.
fn read_both<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

/// v4's `catch` arm on the list route: every failure below the handler collapses
/// to one opaque `serverError`. The real error is logged, not returned.
fn list_failed() -> Response {
    Response::error(ErrorKind::Internal, "Failed to list LLM logs")
}

/// JS `parseInt(s, 10)`: skip leading JS whitespace, take an optional sign, then
/// the leading run of ASCII digits. `pub` because `quilltap-web`'s chats
/// collection GET (`chats_routes.rs`) parses v4's `limit` param with it too. **`NaN` when no digit follows** — the arm the
/// route's garbage-limit quirk rides on. The result is a JS number (`f64`), not an
/// integer, so the NaN survives the arithmetic below exactly as it does in v4.
pub fn js_parse_int_10(s: &str) -> f64 {
    let t = s.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let (negative, rest) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    // JS parses the whole digit run as a float (losing precision past 2^53 rather
    // than overflowing); `f64::from_str` over an all-digit string does the same.
    let magnitude: f64 = digits.parse().unwrap_or(f64::NAN);
    if negative {
        -magnitude
    } else {
        magnitude
    }
}

/// JS `Math.min(a, b)` — **NaN-propagating**. Rust's `f64::min` returns the
/// non-NaN operand instead, which would quietly repair v4's garbage-limit quirk.
pub(super) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a < b {
        a
    } else {
        b
    }
}

/// `JSON.stringify` of a JS number: a non-finite number (`NaN`/`Infinity`)
/// serializes as `null`; an integer-valued float renders bare.
fn js_number_or_null(f: f64) -> Value {
    if f.is_finite() {
        crate::db::js_number_to_json(f)
    } else {
        Value::Null
    }
}

/// JS `Array.prototype.slice(start)` / `.slice(0, end)` over the fetched page,
/// applied exactly where v4 applies it. Both bounds arrive as JS numbers, so a
/// negative `end` counts back from the tail (`slice(0, -5)` drops the last five —
/// reachable via `?limit=-5`, and faithful).
fn js_slice_from(logs: Vec<LlmLogRow>, start: f64) -> Vec<LlmLogRow> {
    let len = logs.len() as f64;
    let start = if start < 0.0 {
        (len + start).max(0.0)
    } else {
        start.min(len)
    };
    logs.into_iter().skip(start as usize).collect()
}
fn js_slice_to(logs: Vec<LlmLogRow>, end: f64) -> Vec<LlmLogRow> {
    let len = logs.len() as f64;
    let end = if end < 0.0 {
        (len + end).max(0.0)
    } else {
        end.min(len)
    };
    logs.into_iter().take(end as usize).collect()
}

// ===========================================================================
// GET /api/v1/llm-logs — the list route
// ===========================================================================

/// The list route's query params, as the transport hands them over: `limit` and
/// `offset` are the RAW strings (v4 parses them inside the handler, and the NaN
/// arm must survive), the two flags are already the strict `=== 'true'` compares.
#[derive(Debug, Clone, Default)]
pub struct LlmLogsListParams<'a> {
    pub message_id: Option<&'a str>,
    pub chat_id: Option<&'a str>,
    pub character_id: Option<&'a str>,
    /// v4's `type` query param, cast to `LLMLogType` but **not validated** — any
    /// string reaches `findByType` and simply matches nothing.
    pub log_type: Option<&'a str>,
    pub standalone: bool,
    pub include_messages: bool,
    pub limit: Option<&'a str>,
    pub offset: Option<&'a str>,
}

/// v4 `GET /api/v1/llm-logs` (`route.ts:24-92`) — every filter branch.
///
/// Branch order is first-match-wins and matters: `messageId` → `chatId` →
/// `characterId` → `standalone` → `type` → recent. Only the `chatId` and
/// `characterId` branches check the entity exists (404 when it doesn't); the rest
/// take the param at face value.
pub fn llm_logs_list(db: &Db, user_id: &str, p: &LlmLogsListParams<'_>) -> Response {
    // `Math.min(parseInt(limit || (includeMessages ? '500' : '50'), 10),
    //           includeMessages ? 500 : 100)` — the default AND the cap both
    // depend on includeMessages. An empty-string param is JS-falsy, so `?limit=`
    // takes the default too.
    let limit_default = if p.include_messages { "500" } else { "50" };
    let cap = if p.include_messages { 500.0 } else { 100.0 };
    let limit_raw = p.limit.filter(|s| !s.is_empty()).unwrap_or(limit_default);
    let limit = js_min(js_parse_int_10(limit_raw), cap);
    let offset = js_parse_int_10(p.offset.filter(|s| !s.is_empty()).unwrap_or("0"));

    let logs = match fetch_branch(db, user_id, p, limit) {
        Ok(Ok(logs)) => logs,
        // The two ownership 404s.
        Ok(Err(r)) => return r,
        // v4's try/catch: any repo/DB failure → the one opaque serverError.
        Err(_) => return list_failed(),
    };

    // "Apply pagination if not already limited" — the post-fetch slice.
    let total = logs.len();
    let logs = if offset > 0.0 {
        js_slice_from(logs, offset)
    } else {
        logs
    };
    // `logs.length > limit` — FALSE when limit is NaN, so a garbage limit slices
    // nothing at all.
    let logs = if (logs.len() as f64) > limit {
        js_slice_to(logs, limit)
    } else {
        logs
    };

    Response::LlmLog(json!({
        "logs": logs,
        "count": logs.len(),
        "total": total,
        "limit": js_number_or_null(limit),
        "offset": js_number_or_null(offset),
    }))
}

/// The branch cascade. The outer `Result` is v4's try/catch (an `Err` becomes the
/// opaque serverError); the inner one carries the two ownership 404s.
#[allow(clippy::type_complexity)]
fn fetch_branch(
    db: &Db,
    user_id: &str,
    p: &LlmLogsListParams<'_>,
    limit: f64,
) -> Result<Result<Vec<LlmLogRow>, Response>, DbError> {
    // JS truthiness: an EMPTY string param falls through to the next branch.
    fn non_empty(s: Option<&str>) -> Option<&str> {
        s.filter(|v| !v.is_empty())
    }

    if let Some(message_id) = non_empty(p.message_id) {
        let message_id = message_id.to_string();
        return Ok(Ok(db.read_llm_logs(move |conn| {
            LLMLogsRepository::new(conn).find_by_message_id(&message_id)
        })?));
    }

    if let Some(chat_id) = non_empty(p.chat_id) {
        // "Verify chat ownership first" — v4's comment; the check is existence
        // only (the single-user model makes ownership vacuous).
        let cid = chat_id.to_string();
        if db
            .read_main(move |conn| chats_read::find_by_id(conn, &cid))?
            .is_none()
        {
            return Ok(Err(not_found("Chat")));
        }
        if p.include_messages {
            // The chat's message ids feed the $or union. v4 passes NO limit here,
            // so the repo's own default (500) bounds it — never the route's.
            let cid = chat_id.to_string();
            let message_ids: Vec<String> = db
                .read_main(move |conn| chats_messages_read::get_messages(conn, &cid))?
                .iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
                .collect();
            let cid = chat_id.to_string();
            return Ok(Ok(db.read_llm_logs(move |conn| {
                LLMLogsRepository::new(conn).find_all_for_chat(&cid, &message_ids, 500.0)
            })?));
        }
        let cid = chat_id.to_string();
        return Ok(Ok(db.read_llm_logs(move |conn| {
            LLMLogsRepository::new(conn).find_by_chat_id(&cid)
        })?));
    }

    if let Some(character_id) = non_empty(p.character_id) {
        let cid = character_id.to_string();
        if read_both(db, move |main, mount| {
            characters_read::find_by_id(main, mount, &cid)
        })?
        .is_none()
        {
            return Ok(Err(not_found("Character")));
        }
        let cid = character_id.to_string();
        return Ok(Ok(db.read_llm_logs(move |conn| {
            LLMLogsRepository::new(conn).find_by_character_id(&cid)
        })?));
    }

    if p.standalone {
        let uid = user_id.to_string();
        return Ok(Ok(db.read_llm_logs(move |conn| {
            LLMLogsRepository::new(conn).find_standalone(&uid, limit)
        })?));
    }

    if let Some(log_type) = non_empty(p.log_type) {
        let (uid, t) = (user_id.to_string(), log_type.to_string());
        return Ok(Ok(db.read_llm_logs(move |conn| {
            LLMLogsRepository::new(conn).find_by_type(&uid, &t, limit)
        })?));
    }

    // Default: recent logs for the user.
    let uid = user_id.to_string();
    Ok(Ok(db.read_llm_logs(move |conn| {
        LLMLogsRepository::new(conn).find_recent(&uid, limit)
    })?))
}

// ===========================================================================
// GET / DELETE /api/v1/llm-logs/[id] — the item routes
// ===========================================================================

/// v4 `GET /api/v1/llm-logs/[id]` (`[id]/route.ts:16-35`): `findById` →
/// `notFound('LLM Log')`, else `successResponse(log)` — the RAW log object, NOT
/// wrapped in a `{log}` key. No ownership check (see the module header).
pub fn llm_log_get(db: &Db, id: &str) -> Response {
    let log_id = id.to_string();
    match db.read_llm_logs(move |conn| LLMLogsRepository::new(conn).find_by_id(&log_id)) {
        Ok(Some(log)) => match serde_json::to_value(&log) {
            Ok(v) => Response::LlmLog(v),
            Err(e) => Response::error(ErrorKind::Internal, format!("Failed to fetch LLM log: {e}")),
        },
        Ok(None) => not_found("LLM Log"),
        Err(_) => Response::error(ErrorKind::Internal, "Failed to fetch LLM log"),
    }
}

/// v4 `DELETE /api/v1/llm-logs/[id]` (`[id]/route.ts:40-67`): `findById` →
/// `notFound('LLM Log')`; then `delete(id)`, whose `false` is its own
/// `serverError('Failed to delete log')` (a lost race — the row vanished between
/// the two reads); success is `{success: true, deletedId}`. No ownership check.
pub async fn llm_log_delete(db: &Db, id: &str) -> Response {
    let log_id = id.to_string();
    let exists =
        match db.read_llm_logs(move |conn| LLMLogsRepository::new(conn).find_by_id(&log_id)) {
            Ok(found) => found.is_some(),
            Err(_) => return Response::error(ErrorKind::Internal, "Failed to delete LLM log"),
        };
    if !exists {
        return not_found("LLM Log");
    }

    let log_id = id.to_string();
    let deleted = db
        .write(move |writers| {
            let Some(writer) = writers.llm_logs() else {
                return Err(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::LlmLogs,
                ));
            };
            writer.llm_logs().delete(&log_id)
        })
        .await;
    match deleted {
        Ok(true) => Response::LlmLog(json!({ "success": true, "deletedId": id })),
        Ok(false) => Response::error(ErrorKind::Internal, "Failed to delete log"),
        Err(_) => Response::error(ErrorKind::Internal, "Failed to delete LLM log"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_parse_int_10_matches_js() {
        assert_eq!(js_parse_int_10("50"), 50.0);
        assert_eq!(js_parse_int_10("  7  "), 7.0);
        assert_eq!(js_parse_int_10("-5"), -5.0);
        assert_eq!(js_parse_int_10("+12"), 12.0);
        // parseInt stops at the first non-digit — `parseInt('1e3', 10)` is 1.
        assert_eq!(js_parse_int_10("1e3"), 1.0);
        assert_eq!(js_parse_int_10("12abc"), 12.0);
        assert!(js_parse_int_10("garbage").is_nan());
        assert!(js_parse_int_10("").is_nan());
    }

    #[test]
    fn js_min_propagates_nan_where_rust_would_not() {
        assert_eq!(js_min(50.0, 100.0), 50.0);
        assert_eq!(js_min(999.0, 100.0), 100.0);
        assert!(js_min(f64::NAN, 100.0).is_nan());
        // The divergence this helper exists to prevent.
        assert_eq!(f64::NAN.min(100.0), 100.0);
    }

    #[test]
    fn js_number_or_null_renders_like_json_stringify() {
        assert_eq!(js_number_or_null(50.0), json!(50));
        assert_eq!(js_number_or_null(f64::NAN), Value::Null);
        assert_eq!(js_number_or_null(f64::INFINITY), Value::Null);
    }
}
