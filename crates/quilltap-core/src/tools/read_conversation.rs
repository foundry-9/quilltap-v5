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
/// `exclude_annotations` boolean-or-absent; `interchange_start`/`interchange_end`
/// llmNumber(int ≥ 1)-or-absent (episodic overhaul: the range slice). A
/// non-object, or a wrong-typed field, fails. (Extra keys are allowed — Zod
/// objects are non-strict by default.)
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
    for key in ["interchange_start", "interchange_end"] {
        if let Some(v) = obj.get(key) {
            match crate::tools::llm_number::llm_number(v).as_f64() {
                Some(n) if n.fract() == 0.0 && n >= 1.0 => {}
                _ => return false,
            }
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

/// `/^## Interchange \d+/` at the start of a line/section.
fn is_interchange_header(s: &str) -> bool {
    s.strip_prefix("## Interchange ")
        .and_then(|rest| rest.as_bytes().first().copied())
        .is_some_and(|b| b.is_ascii_digit())
}

/// `section.match(/^## Interchange (\d+)/)` → `parseInt(m[1], 10)` as f64
/// (huge digit runs lose precision exactly as JS parseInt does — both go
/// through the same double).
fn interchange_number(section: &str) -> Option<f64> {
    let rest = section.strip_prefix("## Interchange ")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<f64>().ok()
}

/// Render an integral JS number the way a template literal does (`5`, not
/// `5.0`); non-integral values cannot reach here (validation gates ints).
fn fmt_js_int(n: f64) -> String {
    if n.is_finite() {
        format!("{}", n as i64)
    } else {
        "Infinity".to_string()
    }
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

    let mut markdown = if exclude_annotations {
        strip_annotations(rendered)
    } else {
        let annotations = db.read_main(|c| db_find_by_chat_id(c, target_chat_id))?;
        if !annotations.is_empty() {
            merge_annotations(rendered, &annotations)
        } else {
            rendered.to_string()
        }
    };

    // Interchange-range slicing (episodic overhaul): a character drilling into
    // a very long chat can pull just the relevant slice instead of the whole
    // transcript.
    let interchange_start = args
        .get("interchange_start")
        .and_then(|v| crate::tools::llm_number::llm_number(v).as_f64());
    let interchange_end = args
        .get("interchange_end")
        .and_then(|v| crate::tools::llm_number::llm_number(v).as_f64());
    let total_interchanges = count_interchange_headers(&markdown);
    if interchange_start.is_some() || interchange_end.is_some() {
        let start = interchange_start.unwrap_or(1.0);
        let end = interchange_end.unwrap_or(f64::INFINITY);
        if end < start {
            return Ok(ReadConversationOutput::error(
                "interchange_end must be >= interchange_start.",
            ));
        }
        // v4 `markdown.split(/^(?=## Interchange \d+)/m)`: split BEFORE each
        // line-start header (a zero-width match at index 0 produces no leading
        // empty section in V8).
        let mut split_positions: Vec<usize> = Vec::new();
        let mut offset = 0usize;
        for line in markdown.split_inclusive('\n') {
            if offset > 0 && is_interchange_header(line) {
                split_positions.push(offset);
            }
            offset += line.len();
        }
        let mut sections: Vec<&str> = Vec::new();
        {
            let mut prev = 0usize;
            for pos in &split_positions {
                sections.push(&markdown[prev..*pos]);
                prev = *pos;
            }
            sections.push(&markdown[prev..]);
        }
        // `preamble = /^## Interchange \d+/.test(sections[0]) ? '' : shift()`.
        let preamble = if sections.first().is_some_and(|s| is_interchange_header(s)) {
            ""
        } else {
            // An empty markdown still yields one section here (JS split too).
            sections.remove(0)
        };
        let kept: Vec<&str> = sections
            .iter()
            .copied()
            .filter(|section| match interchange_number(section) {
                Some(n) => n >= start && n <= end,
                None => false,
            })
            .collect();
        if kept.is_empty() {
            let end_label = interchange_end.unwrap_or(total_interchanges as f64);
            return Ok(ReadConversationOutput::error(format!(
                "No interchanges in range {}\u{2013}{} (conversation has {}).",
                fmt_js_int(start),
                fmt_js_int(end_label),
                total_interchanges
            )));
        }
        let last_shown = end.min(total_interchanges as f64);
        let body = format!("{preamble}{}", kept.join(""));
        markdown = format!(
            "{}\n\n_Showing interchanges {}\u{2013}{} of {}._\n",
            crate::jsstr::js_trim_end(&body),
            fmt_js_int(start),
            fmt_js_int(last_shown),
            total_interchanges
        );
    }

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
