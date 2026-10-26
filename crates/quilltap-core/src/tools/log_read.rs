//! `log_read` and the pieces the three log tools share — v4
//! `lib/tools/log-read-tool.ts` (post-`a495faa26`) and
//! `lib/tools/handlers/log-handler.ts` (P4.D331, at the `322a5488e` pin).
//!
//! v4's why: the log tools run against "the instance's log (`log_entries`, in
//! the LLM logs database). A new entry is stamped with the call's chat and
//! character; the two readers see every entry in the instance and answer JSON,
//! newest first, with each author's name beside their id."
//!
//! A degraded / absent llm-logs partition is v4's `withRawDb` guard, which
//! runs BEFORE the failure mode: the readers answer `[]` quietly (the
//! `Dedicated database unavailable` DEBUG), the writer answers its error —
//! never `DbError::PartitionUnavailable` surfaced to the model.
//!
//! **NO-PORT — the job child's buffering.** v4: "In an autonomous room the
//! call runs in the job child, where a new entry is buffered until the job
//! commits — a read in the same turn does not see it." v5 runs every turn in
//! process and writes through the writer task at once, so it DOES see it (the
//! ruled `CHILD_BUFFER_VISIBILITY` divergence, `log_tools_equivalence`).

use std::collections::HashMap;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::db::character_resolver::resolve_character_by_name_or_id;
use crate::db::characters_read::find_names_by_ids;
use crate::db::fallback::{error_text, log_partition_unavailable};
use crate::db::log_entries::{LogEntriesRepository, LogEntryFilter, LogEntryRow};
use crate::db::table_shape::Partition;
use crate::db::DbError;

/// v4 `DEFAULT_LOG_RESULT_LIMIT` (`log-grep-tool.ts:16`).
pub(crate) const DEFAULT_LOG_RESULT_LIMIT: i64 = 50;
/// v4 `MAX_LOG_RESULT_LIMIT` (`log-grep-tool.ts:17`).
pub(crate) const MAX_LOG_RESULT_LIMIT: i64 = 200;

/// The three tool names, in v4's dispatch order (`tool-executor.ts:811`).
pub(crate) const LOG_TOOL_NAMES: [&str; 3] = ["log_create_entry", "log_grep", "log_read"];

/// v4 `LogToolContext` — what every log tool receives from the executor.
#[derive(Debug, Clone)]
pub(crate) struct LogToolContext {
    pub user_id: String,
    pub chat_id: String,
    /// The calling character; `None` when the operator runs the tool with no
    /// character seat (v4's `...(characterId ? { characterId } : {})` — an
    /// EMPTY id is falsy there and absent here).
    pub character_id: Option<String>,
}

/// The llm-logs partition as a handler sees it.
pub(crate) enum LogDb<'a> {
    Open(&'a Connection),
    /// v4's guard sentence (`llm-logs-guard.ts:20-27`).
    Unavailable(&'static str),
}

pub(crate) use crate::db::log_entries::llm_logs_guard_sentence;

/// v4 `LogEntryView` — one entry as the model sees it, in v4's key order.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogEntryView {
    pub id: String,
    pub message: String,
    pub conversation_id: String,
    pub character_id: Option<String>,
    pub character_name: Option<String>,
    pub created_at: String,
}

/// v4 `toViews` — "Label entries with their authors' names — one batched
/// lookup per call." An id with no (non-blank) name answers `null`.
pub(crate) fn to_views(main: &Connection, entries: &[LogEntryRow]) -> Vec<LogEntryView> {
    let ids: Vec<String> = entries
        .iter()
        .filter_map(|e| e.character_id.clone().filter(|id| !id.is_empty()))
        .collect();
    let names: HashMap<String, String> = find_names_by_ids(main, &ids);
    entries
        .iter()
        .map(|e| LogEntryView {
            id: e.id.clone(),
            message: e.message.clone(),
            conversation_id: e.chat_id.clone(),
            character_id: e.character_id.clone(),
            character_name: e
                .character_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .and_then(|id| names.get(id).cloned()),
            created_at: e.created_at.clone(),
        })
        .collect()
}

/// `findEntries` through v4's guard: an unavailable partition answers `[]`
/// with the `Dedicated database unavailable` DEBUG.
pub(crate) fn find_entries_guarded(llm: &LogDb<'_>, filter: &LogEntryFilter) -> Vec<LogEntryRow> {
    match llm {
        LogDb::Open(conn) => LogEntriesRepository::new(conn).find_entries(filter),
        LogDb::Unavailable(sentence) => {
            log_partition_unavailable(
                Partition::LlmLogs,
                crate::db::log_entries::LOG_ENTRIES_TABLE,
                &DbError::Internal((*sentence).to_string()),
            );
            Vec::new()
        }
    }
}

/// The `{entries, count, limit}` answer both readers give.
pub(crate) fn query_answer(main: &Connection, entries: &[LogEntryRow], limit: i64) -> Value {
    json!({
        "entries": to_views(main, entries),
        "count": entries.len(),
        "limit": limit,
    })
}

/// `llmNumber(z.number().int().min(1).max(200)).optional()` — `None` when
/// absent, `Some(Ok(n))` when valid, `Some(Err(()))` when refused.
pub(crate) fn optional_limit(input: &Map<String, Value>) -> Result<Option<i64>, ()> {
    let Some(raw) = input.get("limit") else {
        return Ok(None);
    };
    let value = crate::tools::llm_number::llm_number(raw);
    let Some(n) = value.as_f64() else {
        return Err(());
    };
    if !n.is_finite() || n.fract() != 0.0 || !(1.0..=MAX_LOG_RESULT_LIMIT as f64).contains(&n) {
        return Err(());
    }
    Ok(Some(n as i64))
}

/// The log tools' handler-logger lines carry v4's child bindings
/// (`createServiceLogger('LogToolHandler')` → `{service, module}`) first.
macro_rules! log_tool_line {
    ($level:ident, $($rest:tt)*) => {
        tracing::$level!(
            target: "quilltap::tools",
            service = "LogToolHandler",
            module = "service",
            $($rest)*
        )
    };
}
pub(crate) use log_tool_line;

/// v4's `log_read` input as validated (`logReadToolInputSchema`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogReadInput {
    pub conversation_id: Option<String>,
    pub character: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
}

/// Which end of a span a bound is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundEnd {
    From,
    To,
}

/// v4 `BARE_DATE = /^\d{4}-\d{2}-\d{2}$/` — ASCII digits (no `u` flag).
fn is_bare_date(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

/// v4 `logReadBoundInstant(value, end)` — a bare-date `to` is the END of that
/// day in UTC by string CONCATENATION (no parse: `2026-02-31` stays
/// `2026-02-31T23:59:59.999Z`); everything else is `new Date(value)
/// .toISOString()` — V8's whole `Date.parse`, an expanded year rendered
/// `+275760-…`. `None` only for a value `Date.parse` refuses, which the
/// schema's refine has already rejected.
pub(crate) fn log_read_bound_instant(value: &str, end: BoundEnd) -> Option<String> {
    if end == BoundEnd::To && is_bare_date(value) {
        return Some(format!("{value}T23:59:59.999Z"));
    }
    crate::episodic::js_date_parse_ms(value).map(crate::clock::iso_from_unix_ms)
}

/// One key's Zod outcome: absent, valid, a NON-aborting issue (a failed
/// check — `min`, `max`, `int`, a `refine`), or an ABORTING one (a type
/// mismatch). Zod 4 runs an object's own refine only when no key aborted.
enum KeyCheck<T> {
    Absent,
    Valid(T),
    Issue,
    Aborted,
}

impl<T> KeyCheck<T> {
    fn aborted(&self) -> bool {
        matches!(self, KeyCheck::Aborted)
    }
    fn into_option(self) -> Result<Option<T>, ()> {
        match self {
            KeyCheck::Absent => Ok(None),
            KeyCheck::Valid(v) => Ok(Some(v)),
            KeyCheck::Issue | KeyCheck::Aborted => Err(()),
        }
    }
}

/// `z.string().min(1)` (+ the `instant` refine when `instant`), optional.
fn check_string(input: &Map<String, Value>, key: &str, instant: bool) -> KeyCheck<String> {
    match input.get(key) {
        None => KeyCheck::Absent,
        Some(Value::String(s)) => {
            let ok = crate::jsstr::zod_len_min_ok(s, 1)
                && (!instant || crate::episodic::js_date_parse_ms(s).is_some());
            if ok {
                KeyCheck::Valid(s.clone())
            } else {
                KeyCheck::Issue
            }
        }
        Some(_) => KeyCheck::Aborted,
    }
}

/// `llmNumber(z.number().int().min(1).max(200)).optional()`.
fn check_limit(input: &Map<String, Value>) -> KeyCheck<i64> {
    let Some(raw) = input.get("limit") else {
        return KeyCheck::Absent;
    };
    let value = crate::tools::llm_number::llm_number(raw);
    let Some(n) = value.as_f64() else {
        return KeyCheck::Aborted;
    };
    if n.is_finite() && n.fract() == 0.0 && (1.0..=MAX_LOG_RESULT_LIMIT as f64).contains(&n) {
        KeyCheck::Valid(n as i64)
    } else {
        KeyCheck::Issue
    }
}

/// `logReadToolInputSchema.safeParse` — `Ok(None)` when v4's
/// `validateLogReadInput` answers null, `Err(message)` when the parse THROWS.
///
/// The object refine compares the two RENDERED bounds as STRINGS (`<=` on
/// strings in V8 — for these ASCII renders, byte order; an expanded year sorts
/// by its sign character — reproduced, not fixed). Zod 4 runs that refine
/// whenever no key ABORTED — so it runs even after a bound failed its own
/// `instant` refine, and `new Date('garbage').toISOString()` throws
/// `RangeError: Invalid time value` out of `safeParse` (measured at the pin):
/// the executor's catch answers that message, with no handler line.
pub(crate) fn validate_log_read_input(input: &Value) -> Result<Option<LogReadInput>, String> {
    let Some(obj) = input.as_object() else {
        return Ok(None);
    };
    let conversation_id = check_string(obj, "conversationId", false);
    let character = check_string(obj, "character", false);
    let from = check_string(obj, "from", true);
    let to = check_string(obj, "to", true);
    let limit = check_limit(obj);
    let any_aborted = conversation_id.aborted()
        || character.aborted()
        || from.aborted()
        || to.aborted()
        || limit.aborted();
    let mut refine_failed = false;
    if !any_aborted {
        // The refine reads the RAW strings: `!input.from || !input.to || …`.
        let raw = |k: &str| obj.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
        if let (Some(f), Some(t)) = (raw("from"), raw("to")) {
            let from = log_read_bound_instant(f, BoundEnd::From)
                .ok_or_else(|| "Invalid time value".to_string())?;
            let to = log_read_bound_instant(t, BoundEnd::To)
                .ok_or_else(|| "Invalid time value".to_string())?;
            refine_failed = from.as_str() > to.as_str();
        }
    }
    let parsed = (|| {
        Some(LogReadInput {
            conversation_id: conversation_id.into_option().ok()?,
            character: character.into_option().ok()?,
            from: from.into_option().ok()?,
            to: to.into_option().ok()?,
            limit: limit.into_option().ok()?,
        })
    })();
    Ok(parsed.filter(|_| !refine_failed && !any_aborted))
}

/// v4 `executeLogReadTool`. The ORDER is v4's: validate → resolve the
/// character (a main + mount read, before any llm-logs touch — the RAW token
/// in the refusal) → `findEntries` → `toViews`.
pub(crate) fn execute_log_read(
    main: &Connection,
    mount: Option<&Connection>,
    llm: &LogDb<'_>,
    input: &Value,
    ctx: &LogToolContext,
) -> Result<Value, String> {
    let Some(parsed) = validate_log_read_input(input)? else {
        log_tool_line!(
            debug,
            chatId = %ctx.chat_id,
            inputJson = %serde_json::to_string(input).unwrap_or_default(),
            "log_read validation failed"
        );
        return Err("Invalid input: from and to must be ISO-8601 dates or date-times with from not after to; limit, if given, 1–200.".to_string());
    };

    let mut character_id: Option<String> = None;
    if let Some(token) = parsed.character.as_deref() {
        // v4 resolves outside the handler's try: a database failure is the
        // executor's error, not `The log could not be read`.
        let Some(mount) = mount else {
            return Err("log_read requires the mount-index database".to_string());
        };
        let found = resolve_character_by_name_or_id(main, mount, &ctx.user_id, token)
            .map_err(|e| error_text(&e))?;
        let Some(character) = found else {
            return Err(format!("No character named \"{token}\" was found."));
        };
        character_id = character
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string);
    }

    let chat_id = match parsed.conversation_id.as_deref() {
        Some("current") => Some(ctx.chat_id.clone()),
        other => other.map(str::to_string),
    };
    let limit = parsed.limit.unwrap_or(DEFAULT_LOG_RESULT_LIMIT);
    let filter = LogEntryFilter {
        chat_id: chat_id.filter(|s| !s.is_empty()),
        character_id: character_id.filter(|s| !s.is_empty()),
        // Normalised to the stored form so the bounds compare as the column
        // does; a bare-date `to` covers its whole day.
        from: parsed
            .from
            .as_deref()
            .and_then(|v| log_read_bound_instant(v, BoundEnd::From)),
        to: parsed
            .to
            .as_deref()
            .and_then(|v| log_read_bound_instant(v, BoundEnd::To)),
        limit,
    };
    let entries = find_entries_guarded(llm, &filter);
    log_tool_line!(
        debug,
        chatId = %ctx.chat_id,
        returned = entries.len(),
        limit = limit,
        "log_read answered"
    );
    Ok(query_answer(main, &entries, limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v5's ABSENT llm-logs file is v4's uninitialized client (a state a v4
    /// boot never leaves — no oracle row); DEGRADED is v4's degraded mode.
    #[test]
    fn the_guard_sentences_follow_the_partition_state() {
        use crate::db::runtime::PartitionState;
        assert_eq!(llm_logs_guard_sentence(PartitionState::Open), None);
        assert_eq!(
            llm_logs_guard_sentence(PartitionState::Degraded),
            Some("LLM logs database is in degraded mode")
        );
        assert_eq!(
            llm_logs_guard_sentence(PartitionState::Absent),
            Some("LLM logs database not initialized")
        );
    }

    #[test]
    fn a_bare_date_to_is_the_end_of_that_utc_day_by_concatenation() {
        assert_eq!(
            log_read_bound_instant("2026-02-31", BoundEnd::To).unwrap(),
            "2026-02-31T23:59:59.999Z"
        );
        assert_eq!(
            log_read_bound_instant("2026-10-19", BoundEnd::From).unwrap(),
            "2026-10-19T00:00:00.000Z"
        );
    }

    #[test]
    fn the_span_refine_compares_the_rendered_strings() {
        let same_day = json!({"from": "2026-10-19", "to": "2026-10-19"});
        assert!(validate_log_read_input(&same_day).unwrap().is_some());
        let backwards = json!({"from": "2026-10-20", "to": "2026-10-19"});
        assert!(validate_log_read_input(&backwards).unwrap().is_none());
        let thrown = json!({"from": "garbage", "to": "2026-01-01"});
        assert_eq!(
            validate_log_read_input(&thrown).unwrap_err(),
            "Invalid time value"
        );
    }
}
