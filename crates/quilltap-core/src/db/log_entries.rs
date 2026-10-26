//! The Log's repository and its one write door — v4 `lib/schemas/log-entry.
//! types.ts`, `lib/database/repositories/log-entries.repository.ts` and
//! `lib/log-entries/write.ts` (`e4a60ad9e` "Feature: the Log", at the
//! `322a5488e` pin; P4.D331).
//!
//! v4's whys, carried:
//!   - "The instance's ledger: free-text lines written by `log_create_entry`
//!     and by a custom tool's `log` effect, read back by `log_grep` and
//!     `log_read`. Lives in the dedicated LLM logs database … and is pruned
//!     with the LLM logs (`LLM_LOG_CLEANUP`, the same retention days)."
//!   - "Entries are written once and never edited. Every read answers newest
//!     first." (`updatedAt` exists only because every entity carries one.)
//!   - "When the logs DB is in degraded mode, reads answer empty and writes
//!     throw, as the other LLM-logs tables do." — the guard runs BEFORE the
//!     failure mode (`withRawDb`), so the CALLER checks the partition state
//!     first ([`crate::db::runtime::Db::partition_state`]) and never hands this
//!     repository a connection it does not have; a degraded read is `[]` with
//!     the `Dedicated database unavailable` DEBUG
//!     ([`crate::db::fallback::log_partition_unavailable`]).
//!   - The write door "stamps the chat and the character the caller hands over
//!     (never anything the model said), trims, and clips an over-long line …
//!     rather than refusing it", and "throws when the line is empty after
//!     trimming or the logs database is unavailable."
//!   - SQLite's `X REGEXP Y` operator calls a user function `regexp(Y, X)` and
//!     fails `no such function: REGEXP` when none is registered — the search
//!     here relies on [`crate::db::regexp_function`], registered ONLY on
//!     llm-logs connections.
//!
//! **No lazy ensure (R-B).** v4's repository creates its table on first use,
//! but v4's PHASE 3.1 structural pass drives that ensure at EVERY boot; v5's
//! table arrives by fresh provisioning (the re-dumped `llmLogs` block) or by
//! `table_shape::create_missing_structural_tables` on an existing instance. A
//! `no such table: log_entries` from a connection here is a hand-built fixture,
//! never a production state after one boot. v4's `Failed to ensure log_entries
//! table in LLM logs database` has its v5 home in `fallback::ensure_table_or_log`.
//!
//! **NO-PORT — the job child's buffered create.** In v4 an autonomous turn runs
//! in a forked job child whose `logEntries.create` is BUFFERED until the turn's
//! writes apply, so a `log_read` later in the SAME turn does not see the line.
//! v5 has no child: its in-process runner writes through the writer task at
//! once (the ruled `CHILD_BUFFER_VISIBILITY` divergence, pinned in
//! `log_tools_equivalence`).

use rusqlite::{params_from_iter, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{Map, Value};

use super::fallback::{
    log_create_failure, log_entries_cleanup_or_zero, log_entries_read_or_empty,
    log_entries_search_or_rethrow, log_refused_create,
};
use super::DbError;

/// The table (v4 `super('log_entries', …)`).
pub const LOG_ENTRIES_TABLE: &str = "log_entries";

/// v4 `MAX_LOG_ENTRY_MESSAGE_LENGTH` — "Long enough for a paragraph, short
/// enough to grep." The SCHEMA's `.max()` counts code points (Zod ≥ 4.5); the
/// write door's clip counts UTF-16 units (`String.length` / `.slice`).
pub const MAX_LOG_ENTRY_MESSAGE_LENGTH: usize = 4000;

/// v4 `COLUMNS` — every raw read SELECTs these six, in schema order.
pub const LOG_ENTRY_COLUMNS: &str =
    r#""id", "message", "chatId", "characterId", "createdAt", "updatedAt""#;

/// One entry, in `LogEntrySchema` key order (`id, message, chatId,
/// characterId, createdAt, updatedAt`); `characterId` serialises `null` (v4's
/// `.nullable()` — "null when the operator ran a tool with no character
/// seat").
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntryRow {
    pub id: String,
    pub message: String,
    pub chat_id: String,
    pub character_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// v4 `LogEntryFilter` — "Every one is optional and they AND together."
/// `from` / `to` are INCLUSIVE ISO-8601 bounds compared as TEXT. Serialises
/// the PRESENT keys in v4's order (the `filter` field of `Error reading log
/// entries`).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntryFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub limit: i64,
}

/// v4's `CreateOptions` for this repository — each one JS-truthy
/// (`options?.id || generateId()`), so an empty string falls back too.
#[derive(Debug, Clone, Default)]
pub struct LogEntryCreateOptions {
    pub id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// v4 `WriteLogEntryParams`.
#[derive(Debug, Clone, Copy)]
pub struct WriteLogEntryParams<'a> {
    pub message: &'a str,
    pub chat_id: &'a str,
    /// The writing character; `None` when the operator acts with no character
    /// seat.
    pub character_id: Option<&'a str>,
}

/// v4's two guard sentences (`llm-logs-guard.ts:20-27`) by the partition's
/// state: v5's ABSENT file is v4's uninitialized client.
pub fn llm_logs_guard_sentence(state: crate::db::runtime::PartitionState) -> Option<&'static str> {
    use crate::db::runtime::PartitionState;
    match state {
        PartitionState::Open => None,
        PartitionState::Degraded => Some("LLM logs database is in degraded mode"),
        PartitionState::Absent => Some("LLM logs database not initialized"),
    }
}

/// A JS number as v4's logger renders it in JSON: a finite value as its
/// number text, a non-finite one as `null` (`JSON.stringify(NaN)`).
pub fn js_number_json(n: f64) -> String {
    if n.is_finite() {
        crate::pascal::js_value::number_to_string(n)
    } else {
        "null".to_string()
    }
}

fn some_truthy(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|v| !v.is_empty())
}

fn row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<LogEntryRow> {
    Ok(LogEntryRow {
        id: r.get(0)?,
        message: r.get(1)?,
        chat_id: r.get(2)?,
        character_id: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

/// The conn-borrowing repository (the [`super::llm_logs::LLMLogsRepository`]
/// shape) over an llm-logs connection.
pub struct LogEntriesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> LogEntriesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4 `create` → the base `_create`: `id = options.id || generateId()`,
    /// `createdAt = options.createdAt || now`, `updatedAt = options.updatedAt
    /// || now`; the entity `{...data, id, createdAt, updatedAt}` validated
    /// against `LogEntrySchema` (a refusal logs `Data validation failed` then
    /// `Error creating entity` and rethrows); the INSERT; INFO `Entity created
    /// {collection, id}`.
    pub fn create(
        &self,
        message: &str,
        chat_id: &str,
        character_id: Option<&str>,
        options: Option<&LogEntryCreateOptions>,
    ) -> Result<LogEntryRow, DbError> {
        let opt = |f: fn(&LogEntryCreateOptions) -> &Option<String>| {
            options.and_then(|o| some_truthy(f(o))).map(str::to_string)
        };
        let id = opt(|o| &o.id).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let now = crate::clock::now_iso();
        let created_at = opt(|o| &o.created_at).unwrap_or_else(|| now.clone());
        let updated_at = opt(|o| &o.updated_at).unwrap_or(now);

        let mut entity = Map::new();
        entity.insert("message".into(), Value::String(message.to_string()));
        entity.insert("chatId".into(), Value::String(chat_id.to_string()));
        entity.insert(
            "characterId".into(),
            character_id.map_or(Value::Null, |c| Value::String(c.to_string())),
        );
        entity.insert("id".into(), Value::String(id.clone()));
        entity.insert("createdAt".into(), Value::String(created_at.clone()));
        entity.insert("updatedAt".into(), Value::String(updated_at.clone()));
        let issues = crate::api::zod_issues::zod_log_entry_issues(&entity);
        if !issues.is_empty() {
            let zod = crate::api::zod_issues::zod_error_message(&issues);
            log_refused_create(LOG_ENTRIES_TABLE, &zod);
            return Err(DbError::Internal(zod));
        }

        let row = LogEntryRow {
            id,
            message: message.to_string(),
            chat_id: chat_id.to_string(),
            character_id: character_id.map(str::to_string),
            created_at,
            updated_at,
        };
        self.conn
            .execute(
                r#"INSERT INTO "log_entries" ("id", "message", "chatId", "characterId", "createdAt", "updatedAt") VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
                rusqlite::params![
                    row.id,
                    row.message,
                    row.chat_id,
                    row.character_id,
                    row.created_at,
                    row.updated_at
                ],
            )
            .map_err(|e| {
                let error = DbError::Sqlite(e);
                log_create_failure(LOG_ENTRIES_TABLE, &error);
                error
            })?;
        tracing::info!(
            target: "quilltap::db",
            collection = LOG_ENTRIES_TABLE,
            id = %row.id,
            "Entity created"
        );
        Ok(row)
    }

    /// v4 `findEntries` — entries matching every given filter, newest first,
    /// at most `limit`. The clauses are pushed under JS-TRUTHY guards (`if
    /// (filter.chatId)` — an empty string is no clause) in v4's order; `limit`
    /// is bound unconditionally. A thrown query is the FALLBACK arm (`[]`).
    pub fn find_entries(&self, filter: &LogEntryFilter) -> Vec<LogEntryRow> {
        let mut clauses: Vec<&str> = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        for (value, clause) in [
            (some_truthy(&filter.chat_id), r#""chatId" = ?"#),
            (some_truthy(&filter.character_id), r#""characterId" = ?"#),
            (some_truthy(&filter.from), r#""createdAt" >= ?"#),
            (some_truthy(&filter.to), r#""createdAt" <= ?"#),
        ] {
            if let Some(v) = value {
                clauses.push(clause);
                params.push(rusqlite::types::Value::Text(v.to_string()));
            }
        }
        params.push(rusqlite::types::Value::Integer(filter.limit));
        let where_ = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        // v4's template leaves TWO spaces before `ORDER BY` when there is no
        // WHERE — the statement text is reproduced, not tidied.
        let sql = format!(
            r#"SELECT {LOG_ENTRY_COLUMNS} FROM log_entries {where_} ORDER BY "createdAt" DESC LIMIT ?"#
        );
        let filter_json = serde_json::to_string(filter).unwrap_or_else(|_| "{}".into());
        log_entries_read_or_empty(&filter_json, || {
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_from_iter(params.iter()), row_from)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            tracing::debug!(
                target: "quilltap::db",
                filters = clauses.len(),
                returned = rows.len(),
                "Log entries read"
            );
            Ok(rows)
        })
    }

    /// v4 `searchMessages` — entries whose message matches a JavaScript
    /// regular expression, newest first, at most `limit`. "The pattern must
    /// already have been checked with `compileSqlRegexp` — a bad one here is
    /// an error, rethrown." The statement text is v4's template literal,
    /// newlines and indentation included.
    pub fn search_messages(
        &self,
        pattern: &str,
        flags: &str,
        limit: i64,
    ) -> Result<Vec<LogEntryRow>, DbError> {
        let pattern_length = crate::jsstr::utf16_len(pattern);
        let sql = format!(
            "SELECT {LOG_ENTRY_COLUMNS} FROM log_entries\n             WHERE {}(?, \"message\", ?)\n             ORDER BY \"createdAt\" DESC LIMIT ?",
            super::regexp_function::REGEXP_FUNCTION_NAME
        );
        log_entries_search_or_rethrow(pattern_length, flags, || {
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params![pattern, flags, limit], row_from)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            tracing::debug!(
                target: "quilltap::db",
                patternLength = pattern_length,
                flags = flags,
                returned = rows.len(),
                "Log entries searched"
            );
            Ok(rows)
        })
    }

    /// v4 `cleanupOldEntries` — "Delete entries older than `retentionDays`.
    /// Returns how many went." `<= 0` answers 0 with NO query (negatives too —
    /// unlike `cleanupOldLogs`' `< 0` WARN). The cutoff is the host-local
    /// calendar arithmetic `setDate(getDate() - days)` — the SAME helper the
    /// LLM-log prune uses ([`super::llm_logs::llm_log_retention_cutoff_iso`]).
    ///
    /// v4 computes the cutoff BEFORE `withRawDb` (measured at the pin), so a
    /// `NaN` window — which passes the guard, `NaN <= 0` being false — throws
    /// `toISOString()`'s `RangeError: Invalid time value` straight out of this
    /// call: no ERROR line, no fallback `0`. Only a failure of the DELETE
    /// itself is the fallback arm (`Error cleaning up old log entries` + 0).
    /// The clock and zone are the caller's (core never reads the ambient ones).
    pub fn cleanup_old_entries(
        &self,
        retention_days: f64,
        now_ms: i64,
        tz: &str,
    ) -> Result<usize, DbError> {
        cleanup_old_entries_on(Ok(self.conn), retention_days, now_ms, tz)
    }

    /// One entry by id — a test / tooling read (v4 inherits `findById`).
    pub fn find_by_id(&self, id: &str) -> Result<Option<LogEntryRow>, DbError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {LOG_ENTRY_COLUMNS} FROM log_entries WHERE \"id\" = ?1"),
                [id],
                row_from,
            )
            .optional()?)
    }
}

/// [`LogEntriesRepository::cleanup_old_entries`] over the partition as the
/// caller holds it: `Err(sentence)` is a degraded / absent partition (v4's
/// guard). v4's ORDER, kept: the `<= 0` return, then the cutoff (so a `NaN`
/// window throws even on a degraded partition), THEN `withRawDb`'s guard —
/// the `Dedicated database unavailable` DEBUG and `0` — then the DELETE.
pub fn cleanup_old_entries_on(
    conn: Result<&Connection, &str>,
    retention_days: f64,
    now_ms: i64,
    tz: &str,
) -> Result<usize, DbError> {
    if retention_days <= 0.0 {
        return Ok(0);
    }
    let cutoff = super::llm_logs::llm_log_retention_cutoff_iso(now_ms, retention_days, tz)
        .ok_or_else(|| DbError::Internal("Invalid time value".to_string()))?;
    let conn = match conn {
        Ok(conn) => conn,
        Err(sentence) => {
            super::fallback::log_partition_unavailable(
                super::table_shape::Partition::LlmLogs,
                LOG_ENTRIES_TABLE,
                &DbError::Internal(sentence.to_string()),
            );
            return Ok(0);
        }
    };
    {
        Ok(log_entries_cleanup_or_zero(retention_days, || {
            let deleted = conn.execute(
                r#"DELETE FROM log_entries WHERE "createdAt" < ?"#,
                [&cutoff],
            )?;
            tracing::info!(
                target: "quilltap::db",
                retentionDays = %js_number_json(retention_days),
                deletedCount = deleted,
                cutoffDate = %cutoff,
                "Cleaned up old log entries"
            );
            Ok(deleted)
        }))
    }
}

/// v4 `writeLogEntry` — "the one door both writers come through: the
/// `log_create_entry` tool and a custom tool's `log` effect." JS-trims;
/// empty → `A log entry needs a message`; a line over
/// [`MAX_LOG_ENTRY_MESSAGE_LENGTH`] UTF-16 units is CLIPPED (`.slice(0,
/// 4000)` — DEBUG `Log entry clipped to the maximum length {chatId, length,
/// max}`, `length` in UTF-16 units) rather than refused; the create; DEBUG
/// `Log entry written {id, chatId, characterId}` (`characterId` a FIELD — v4
/// passes `null`, not `undefined`).
pub fn write_log_entry(
    conn: &Connection,
    params: WriteLogEntryParams<'_>,
) -> Result<LogEntryRow, DbError> {
    write_log_entry_via(params, |message, chat_id, character_id| {
        LogEntriesRepository::new(conn).create(message, chat_id, character_id, None)
    })
}

/// [`write_log_entry`] over an injected repository `create(message, chatId,
/// characterId)` — the seam v4's applier suite mocks (`getRepositories()
/// .logEntries.create`) while the door's trim and clip stay real; and the
/// applier's own unavailable-partition arm.
pub fn write_log_entry_via(
    params: WriteLogEntryParams<'_>,
    create: impl FnOnce(&str, &str, Option<&str>) -> Result<LogEntryRow, DbError>,
) -> Result<LogEntryRow, DbError> {
    let trimmed = crate::jsstr::js_trim(params.message);
    if trimmed.is_empty() {
        return Err(DbError::Internal("A log entry needs a message".to_string()));
    }
    let length = crate::jsstr::utf16_len(trimmed);
    let message = if length > MAX_LOG_ENTRY_MESSAGE_LENGTH {
        tracing::debug!(
            target: "quilltap::log_entries",
            chatId = %params.chat_id,
            length = length,
            max = MAX_LOG_ENTRY_MESSAGE_LENGTH,
            "Log entry clipped to the maximum length"
        );
        crate::jsstr::utf16_truncate(trimmed, MAX_LOG_ENTRY_MESSAGE_LENGTH)
    } else {
        trimmed.to_string()
    };
    let entry = create(&message, params.chat_id, params.character_id)?;
    tracing::debug!(
        target: "quilltap::log_entries",
        id = %entry.id,
        chatId = %params.chat_id,
        characterId = %params.character_id.unwrap_or("null"),
        "Log entry written"
    );
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_serialises_in_schema_order_with_a_null_author() {
        let row = LogEntryRow {
            id: "i".into(),
            message: "m".into(),
            chat_id: "c".into(),
            character_id: None,
            created_at: "a".into(),
            updated_at: "b".into(),
        };
        assert_eq!(
            serde_json::to_string(&row).unwrap(),
            r#"{"id":"i","message":"m","chatId":"c","characterId":null,"createdAt":"a","updatedAt":"b"}"#
        );
    }

    #[test]
    fn a_filter_serialises_its_present_keys_in_v4_order() {
        let f = LogEntryFilter {
            chat_id: Some("c".into()),
            to: Some("t".into()),
            limit: 5,
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&f).unwrap(),
            r#"{"chatId":"c","to":"t","limit":5}"#
        );
    }

    #[test]
    fn js_number_json_renders_like_json_stringify() {
        assert_eq!(js_number_json(7.0), "7");
        assert_eq!(js_number_json(7.5), "7.5");
        assert_eq!(js_number_json(f64::NAN), "null");
        assert_eq!(js_number_json(f64::INFINITY), "null");
    }
}
