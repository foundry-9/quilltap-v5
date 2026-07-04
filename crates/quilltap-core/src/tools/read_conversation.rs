//! Port of v4's `read_conversation` tool (Project Scriptorium):
//! `lib/tools/handlers/read-conversation-handler.ts` +
//! `lib/tools/read-conversation-tool.ts`.
//!
//! Loads a chat's rendered Markdown, optionally merging or stripping character
//! annotations, and returns it with message/interchange counts. The pure
//! merge/strip transforms live in [`crate::scriptorium`]; the counts are the JS
//! `/^### Message \d+/gm` and `/^## Interchange \d+/gm` line-header tallies.
//!
//! ## Faithful details
//!
//! - **Input validation** (`validateReadConversationInput`, Zod `safeParse`):
//!   `conversationId` must be a string or absent; `exclude_annotations` a boolean
//!   or absent. On failure the handler returns the FIXED string
//!   `"Invalid input: exclude_annotations must be a boolean if provided."`.
//! - **targetChatId** = `conversationId || context.chatId` — JS `||`, so an EMPTY
//!   `conversationId` is falsy and falls through to the current chat.
//! - The chat is loaded by id with **no user scoping** in the handler (matching
//!   v4). Missing → `"Conversation not found."`.
//! - The cross-conversation participation guard fires only when `conversationId`
//!   is truthy AND `!= context.chatId` AND a `characterId` is present; a
//!   non-participant → `"Conversation not found."`.
//! - No `renderedMarkdown` → `"Conversation has not been rendered yet."`.
//! - Any thrown error (e.g. a DB error) becomes `error: error.message` — here the
//!   [`crate::db::DbError`] `Display` string.

use serde::Serialize;
use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{chats_read, DbError};
use crate::scriptorium::{merge_annotations, strip_annotations};

/// v4 `ReadConversationToolOutput`. Fields serialize in v4's declared order with
/// the optionals dropped when absent (`skip_serializing_if`), matching how the
/// handler builds the object (`success` always present; the rest conditional).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReadConversationOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
    #[serde(rename = "messageCount", skip_serializing_if = "Option::is_none")]
    pub message_count: Option<i64>,
    #[serde(rename = "interchangeCount", skip_serializing_if = "Option::is_none")]
    pub interchange_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ReadConversationOutput {
    fn error(message: impl Into<String>) -> Self {
        ReadConversationOutput {
            success: false,
            markdown: None,
            message_count: None,
            interchange_count: None,
            error: Some(message.into()),
        }
    }
}

/// v4 `validateReadConversationInput`. `conversationId` string-or-absent;
/// `exclude_annotations` boolean-or-absent. A non-object, or a wrong-typed field,
/// fails. (Extra keys are allowed — Zod objects are non-strict by default.)
fn validate_input(args: &Value) -> bool {
    let obj = match args.as_object() {
        Some(o) => o,
        None => return false,
    };
    if let Some(v) = obj.get("conversationId") {
        if !v.is_string() {
            return false;
        }
    }
    if let Some(v) = obj.get("exclude_annotations") {
        if !v.is_boolean() {
            return false;
        }
    }
    true
}

/// Count `/^### Message \d+/gm` matches — headers at line start, `\d` = ASCII.
fn count_message_headers(markdown: &str) -> i64 {
    count_line_headers(markdown, "### Message ")
}

/// Count `/^## Interchange \d+/gm` matches.
fn count_interchange_headers(markdown: &str) -> i64 {
    count_line_headers(markdown, "## Interchange ")
}

/// Count lines beginning with `prefix` immediately followed by 1+ ASCII digits
/// (the JS `^<prefix>\d+` per-line-header rule under `/m`). `\d+` requires at
/// least one digit; trailing chars after the digit run are irrelevant.
fn count_line_headers(markdown: &str, prefix: &str) -> i64 {
    let mut count = 0i64;
    for line in markdown.split('\n') {
        if let Some(rest) = line.strip_prefix(prefix) {
            if rest.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                count += 1;
            }
        }
    }
    count
}

/// Execute the `read_conversation` tool (v4 `executeReadConversationTool`).
pub async fn execute_read_conversation(
    db: &Db,
    _user_id: &str,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
) -> ReadConversationOutput {
    match execute_inner(db, chat_id, character_id, args) {
        Ok(out) => out,
        // v4's catch-all: `error instanceof Error ? error.message : 'Unknown error…'`.
        Err(e) => ReadConversationOutput::error(e.to_string()),
    }
}

fn execute_inner(
    db: &Db,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
) -> Result<ReadConversationOutput, DbError> {
    if !validate_input(args) {
        return Ok(ReadConversationOutput::error(
            "Invalid input: exclude_annotations must be a boolean if provided.",
        ));
    }

    // `conversationId` — a truthy (non-empty) string only. `exclude_annotations`
    // defaults to false.
    let conversation_id = args
        .get("conversationId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let exclude_annotations = args
        .get("exclude_annotations")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let target_chat_id = conversation_id.unwrap_or(chat_id);

    // Load chat (no user scoping — matches v4's `repos.chats.findById`).
    let chat = match db.read_main(|c| chats_read::find_by_id(c, target_chat_id))? {
        Some(c) => c,
        None => return Ok(ReadConversationOutput::error("Conversation not found.")),
    };

    // Cross-conversation participation guard.
    if let (Some(conv_id), Some(cid)) = (conversation_id, character_id) {
        if conv_id != chat_id {
            let participates = chat
                .get("participants")
                .and_then(Value::as_array)
                .is_some_and(|ps| {
                    ps.iter()
                        .any(|p| p.get("characterId").and_then(Value::as_str) == Some(cid))
                });
            if !participates {
                return Ok(ReadConversationOutput::error("Conversation not found."));
            }
        }
    }

    let rendered = chat
        .get("renderedMarkdown")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let rendered = match rendered {
        Some(r) => r,
        None => {
            return Ok(ReadConversationOutput::error(
                "Conversation has not been rendered yet.",
            ))
        }
    };

    let markdown = if exclude_annotations {
        strip_annotations(rendered)
    } else {
        let annotations = db.read_main(|c| db_find_by_chat_id(c, target_chat_id))?;
        if !annotations.is_empty() {
            merge_annotations(rendered, &annotations)
        } else {
            rendered.to_string()
        }
    };

    let message_count = count_message_headers(&markdown);
    let interchange_count = count_interchange_headers(&markdown);

    Ok(ReadConversationOutput {
        success: true,
        markdown: Some(markdown),
        message_count: Some(message_count),
        interchange_count: Some(interchange_count),
        error: None,
    })
}

/// Small helper so the read closure stays a single expression.
fn db_find_by_chat_id(
    conn: &rusqlite::Connection,
    chat_id: &str,
) -> Result<Vec<crate::scriptorium::Annotation>, DbError> {
    crate::db::conversation_annotations::ConversationAnnotationsRepository::new(conn)
        .find_by_chat_id(chat_id)
}

/// Format the tool result for LLM context (v4 `formatReadConversationResults`).
/// Failure → the error string; success with no markdown → the placeholder; else
/// the markdown itself.
pub fn format_read_conversation(out: &ReadConversationOutput) -> String {
    if !out.success {
        return out
            .error
            .clone()
            .unwrap_or_else(|| "Unknown error reading conversation.".to_string());
    }
    match &out.markdown {
        Some(md) => md.clone(),
        None => "No conversation content available.".to_string(),
    }
}
