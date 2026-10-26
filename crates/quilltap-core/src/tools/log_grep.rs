//! `log_grep` — v4 `lib/tools/log-grep-tool.ts` + `executeLogGrepTool`
//! (`lib/tools/handlers/log-handler.ts:101-123`, post-`a495faa26`; P4.D331).
//!
//! "Search the log (every conversation, every character) for entries whose
//! message matches a regular expression." The pattern is compiled FIRST with
//! [`compile_sql_regexp`] — a bad one answers `Invalid regular expression:
//! <reason>` with no query and no log line, the prefix appearing once (v4's
//! `a495faa26` strip) — then searched through the llm-logs connection's
//! `regexp()` scalar, newest first.

use rusqlite::Connection;
use serde_json::Value;

use super::log_read::{
    log_tool_line, optional_limit, query_answer, LogDb, LogToolContext, DEFAULT_LOG_RESULT_LIMIT,
};
use crate::db::fallback::{error_text, log_partition_unavailable};
use crate::db::log_entries::{LogEntriesRepository, LOG_ENTRIES_TABLE};
use crate::db::regexp_function::compile_sql_regexp;
use crate::db::table_shape::Partition;
use crate::db::DbError;

/// v4 `MAX_LOG_GREP_PATTERN_LENGTH`.
pub(crate) const MAX_LOG_GREP_PATTERN_LENGTH: usize = 500;

/// v4's validated `log_grep` input.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogGrepInput {
    pub pattern: String,
    pub ignore_case: Option<bool>,
    pub limit: Option<i64>,
}

/// `logGrepToolInputSchema.safeParse` — `pattern` `.min(1).max(500)` (code
/// points), `ignore_case` an optional boolean, `limit` `llmNumber(int 1..200)`.
pub(crate) fn validate_log_grep_input(input: &Value) -> Option<LogGrepInput> {
    let obj = input.as_object()?;
    let pattern = match obj.get("pattern") {
        Some(Value::String(s))
            if crate::jsstr::zod_len_min_ok(s, 1)
                && crate::jsstr::zod_len_max_ok(s, MAX_LOG_GREP_PATTERN_LENGTH) =>
        {
            Some(s.clone())
        }
        _ => None,
    };
    let ignore_case = match obj.get("ignore_case") {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(()),
    };
    let limit = optional_limit(obj);
    Some(LogGrepInput {
        pattern: pattern?,
        ignore_case: ignore_case.ok()?,
        limit: limit.ok()?,
    })
}

/// v4 `executeLogGrepTool`.
pub(crate) fn execute_log_grep(
    main: &Connection,
    llm: &LogDb<'_>,
    input: &Value,
    ctx: &LogToolContext,
) -> Result<Value, String> {
    let Some(parsed) = validate_log_grep_input(input) else {
        log_tool_line!(
            debug,
            chatId = %ctx.chat_id,
            inputJson = %serde_json::to_string(input).unwrap_or_default(),
            "log_grep validation failed"
        );
        return Err(
            "Invalid input: pattern must be a non-empty string; limit, if given, 1–200."
                .to_string(),
        );
    };

    let flags = if parsed.ignore_case == Some(true) {
        "i"
    } else {
        ""
    };
    if let Err(reason) = compile_sql_regexp(&parsed.pattern, flags) {
        return Err(format!("Invalid regular expression: {reason}"));
    }

    let limit = parsed.limit.unwrap_or(DEFAULT_LOG_RESULT_LIMIT);
    let searched = match llm {
        LogDb::Open(conn) => {
            LogEntriesRepository::new(conn).search_messages(&parsed.pattern, flags, limit)
        }
        // v4's guard runs before the rethrow mode: quietly empty.
        LogDb::Unavailable(sentence) => {
            log_partition_unavailable(
                Partition::LlmLogs,
                LOG_ENTRIES_TABLE,
                &DbError::Internal((*sentence).to_string()),
            );
            Ok(Vec::new())
        }
    };
    match searched {
        Ok(entries) => {
            log_tool_line!(
                debug,
                chatId = %ctx.chat_id,
                returned = entries.len(),
                limit = limit,
                "log_grep answered"
            );
            Ok(query_answer(main, &entries, limit))
        }
        Err(error) => {
            let message = error_text(&error);
            log_tool_line!(
                warn,
                chatId = %ctx.chat_id,
                error = %message,
                "log_grep failed"
            );
            Err(format!("The log could not be searched: {message}"))
        }
    }
}
