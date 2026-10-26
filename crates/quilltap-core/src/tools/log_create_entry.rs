//! `log_create_entry` — v4 `lib/tools/log-create-entry-tool.ts` +
//! `executeLogCreateEntryTool` (`lib/tools/handlers/log-handler.ts:72-96`;
//! P4.D331).
//!
//! "Write one line to the log: a durable, timestamped record that you (and
//! other characters) can search later with log_grep or read back with
//! log_read. The entry is recorded against this conversation and you as its
//! author." The chat and the character are the CALL's (never anything the
//! model said); the write goes through the one door,
//! [`crate::db::log_entries::write_log_entry`].

use serde_json::{json, Value};

use super::log_read::{log_tool_line, LogDb, LogToolContext};
use crate::db::fallback::{error_text, log_create_failure};
use crate::db::log_entries::{
    write_log_entry, WriteLogEntryParams, LOG_ENTRIES_TABLE, MAX_LOG_ENTRY_MESSAGE_LENGTH,
};
use crate::db::DbError;

/// `logCreateEntryToolInputSchema.safeParse` — `message: z.string().trim()
/// .min(1).max(4000)`: the TRIM runs first (so `'   '` is invalid input), the
/// bounds count code points over the trimmed text, and the parsed value IS
/// the trimmed text.
pub(crate) fn validate_log_create_entry_input(input: &Value) -> Option<String> {
    let message = input.as_object()?.get("message")?.as_str()?;
    let trimmed = crate::jsstr::js_trim(message);
    (crate::jsstr::zod_len_min_ok(trimmed, 1)
        && crate::jsstr::zod_len_max_ok(trimmed, MAX_LOG_ENTRY_MESSAGE_LENGTH))
    .then(|| trimmed.to_string())
}

/// v4 `executeLogCreateEntryTool` — `{ entry: {id, message, conversationId,
/// createdAt} }`, or the error. An unavailable partition is the base
/// create's rethrow: `Error creating entity` with the guard sentence, then the
/// tool's WARN and `The log entry could not be written: <sentence>`.
pub(crate) fn execute_log_create_entry(
    llm: &LogDb<'_>,
    input: &Value,
    ctx: &LogToolContext,
) -> Result<Value, String> {
    let Some(message) = validate_log_create_entry_input(input) else {
        log_tool_line!(
            debug,
            chatId = %ctx.chat_id,
            inputJson = %serde_json::to_string(input).unwrap_or_default(),
            "log_create_entry validation failed"
        );
        return Err("Invalid input: message must be a non-empty string.".to_string());
    };
    let written = match llm {
        LogDb::Open(conn) => write_log_entry(
            conn,
            WriteLogEntryParams {
                message: &message,
                chat_id: &ctx.chat_id,
                character_id: ctx.character_id.as_deref(),
            },
        ),
        LogDb::Unavailable(sentence) => {
            let error = DbError::Internal((*sentence).to_string());
            log_create_failure(LOG_ENTRIES_TABLE, &error);
            Err(error)
        }
    };
    match written {
        Ok(entry) => Ok(json!({
            "entry": {
                "id": entry.id,
                "message": entry.message,
                "conversationId": entry.chat_id,
                "createdAt": entry.created_at,
            }
        })),
        Err(error) => {
            let message = error_text(&error);
            log_tool_line!(
                warn,
                chatId = %ctx.chat_id,
                error = %message,
                "log_create_entry failed"
            );
            Err(format!("The log entry could not be written: {message}"))
        }
    }
}
