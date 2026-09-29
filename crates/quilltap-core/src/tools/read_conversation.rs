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
//! - The transcript is RENDERED LIVE from the stored messages (v4 `f7f3d7bf0`
//!   dropped `chats.renderedMarkdown`) through the shared
//!   [`crate::services::scriptorium_render::render_chat_conversation`], under
//!   the executor's injected clock. No events, or a render with ZERO
//!   interchanges → `"Conversation has no messages to read yet."` — both the
//!   string and the gate moved: a header-only render used to be a success.
//! - Any thrown error (e.g. a DB error) becomes `error: error.message` — here the
//!   [`crate::db::DbError`] `Display` string.

use serde::Serialize;
use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{chats_read, DbError};
use crate::scriptorium::{merge_annotations, strip_annotations};
use crate::services::scriptorium_render::render_chat_conversation;

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
///
/// `now_iso` is the wall clock the live render's `Current time:` header line
/// carries (v4 reads `new Date()` inside the renderer); the executor passes
/// [`crate::clock::now_iso`], a differential a frozen instant.
pub async fn execute_read_conversation(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
    now_iso: &str,
) -> ReadConversationOutput {
    match execute_inner(db, user_id, chat_id, character_id, args, now_iso) {
        Ok(out) => out,
        // v4's catch-all: `error instanceof Error ? error.message : 'Unknown error…'`.
        Err(e) => {
            // Hoisted: inside `tracing!` the name `Value` is the macro's own.
            let requested = args
                .get("conversationId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            tracing::error!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                requestedConversationId = requested,
                error = %e,
                "Read conversation tool execution failed",
            );
            ReadConversationOutput::error(e.to_string())
        }
    }
}

/// v4's `context` field on every line this handler logs.
const LOG_CONTEXT: &str = "read-conversation-handler";

fn execute_inner(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
    now_iso: &str,
) -> Result<ReadConversationOutput, DbError> {
    if !validate_input(args) {
        tracing::warn!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            input = %args,
            "Read conversation tool validation failed",
        );
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

    // Load chat (no user scoping — matches v4's `repos.chats.findById`, the
    // fallback `_findById`: a failed read logs the repository's ERROR and takes
    // the not-found arm below, never the tool's catch — the `97b25fc53`
    // unification review).
    let chat = match db.read_main(|c| Ok(chats_read::find_by_id_or_none(c, target_chat_id)))? {
        Some(c) => c,
        None => {
            tracing::warn!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                chatId = target_chat_id,
                userId = user_id,
                requestedConversationId = conversation_id.unwrap_or_default(),
                "Read conversation tool: chat not found",
            );
            return Ok(ReadConversationOutput::error("Conversation not found."));
        }
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
                tracing::warn!(
                    target: "quilltap::tools",
                    context = LOG_CONTEXT,
                    chatId = target_chat_id,
                    characterId = cid,
                    "Read conversation tool: character does not participate in target chat",
                );
                return Ok(ReadConversationOutput::error("Conversation not found."));
            }
        }
    }

    // Rendered live from the stored messages — never a stored copy, so every
    // conversation is readable however long it has been quiet (v4 :99-107).
    let rendered = db.read_main(|c| render_chat_conversation(c, &chat, now_iso))?;
    let rendered = match rendered {
        Some(r) if !r.interchanges.is_empty() => r.markdown,
        _ => {
            return Ok(ReadConversationOutput::error(
                "Conversation has no messages to read yet.",
            ))
        }
    };
    let rendered = rendered.as_str();

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

    tracing::info!(
        target: "quilltap::tools",
        context = LOG_CONTEXT,
        userId = user_id,
        chatId = target_chat_id,
        messageCount = message_count,
        interchangeCount = interchange_count,
        // JS `string.length` — UTF-16 code units.
        markdownLength = markdown.encode_utf16().count(),
        excludeAnnotations = exclude_annotations,
        "Read conversation tool completed",
    );

    Ok(ReadConversationOutput {
        success: true,
        markdown: Some(markdown),
        message_count: Some(message_count),
        interchange_count: Some(interchange_count),
        error: None,
    })
}

/// Small helper so the read closure stays a single expression.
///
/// v4's `conversationAnnotations.findByChatId` is a FALLBACK `safeQuery`: a
/// failed read logs v4's ERROR and answers `[]` (the transcript is returned
/// un-annotated). Unreachable before P4.D235 on an instance with no
/// annotations table, because the stored-render gate stopped the tool first;
/// the live render made it reachable (found by the lane's full sweep:
/// `chat_admin_routes`' `run_tool_read_conversation` answered `no such table`
/// where v4 renders).
fn db_find_by_chat_id(
    conn: &rusqlite::Connection,
    chat_id: &str,
) -> Result<Vec<crate::scriptorium::Annotation>, DbError> {
    match crate::db::conversation_annotations::ConversationAnnotationsRepository::new(conn)
        .find_by_chat_id(chat_id)
    {
        Ok(rows) => Ok(rows),
        Err(e) => {
            tracing::error!(
                target: "quilltap::db",
                collection = "conversation_annotations",
                chatId = chat_id,
                error = %e,
                "Error finding annotations by chat ID",
            );
            Ok(Vec::new())
        }
    }
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

/// P4.D235 — v4's restored log lines, pinned with silence legs over the
/// shared scriptorium-tool fixture.
#[cfg(test)]
mod log_tests {
    use super::*;
    use crate::test_support::global_capture::capture;
    use crate::tools::annotations::log_tests::{
        db, drop_annotations, CHAT, FRIDAY, NOW, OTHER_CHAT, USER,
    };
    use serde_json::json;

    fn read(db: &Db, args: Value) -> (ReadConversationOutput, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        capture(|| {
            rt.block_on(execute_read_conversation(
                db,
                USER,
                CHAT,
                Some(FRIDAY),
                &args,
                NOW,
            ))
        })
    }

    fn only(lines: &[String], level: &str, needle: &str) -> String {
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with(&format!("{level} quilltap::tools")))
            .collect();
        assert_eq!(hits.len(), 1, "exactly one {level} line: {lines:?}");
        assert!(hits[0].contains(needle), "{needle} not in {}", hits[0]);
        assert!(
            hits[0].contains("context=read-conversation-handler"),
            "{}",
            hits[0]
        );
        hits[0].clone()
    }

    fn none_at(lines: &[String], level: &str) {
        assert!(
            !lines
                .iter()
                .any(|l| l.starts_with(&format!("{level} quilltap::tools"))),
            "no {level} line expected: {lines:?}"
        );
    }

    #[test]
    fn a_read_logs_v4s_completion_line_and_nothing_else() {
        let db = db();
        let (out, lines) = read(&db, json!({}));
        assert!(out.success, "{out:?}");
        let line = only(&lines, "INFO", "Read conversation tool completed");
        let md = out.markdown.as_deref().unwrap();
        for f in [
            "userId=00000000-0000-4000-8000-00000000000a".to_string(),
            "chatId=00000000-0000-4000-8000-0000000000c1".to_string(),
            "messageCount=2".to_string(),
            "interchangeCount=1".to_string(),
            format!("markdownLength={}", md.encode_utf16().count()),
            "excludeAnnotations=false".to_string(),
        ] {
            assert!(line.contains(&f), "{f} missing: {line}");
        }
        none_at(&lines, "WARN");
        none_at(&lines, "ERROR");
    }

    #[test]
    fn each_read_refusal_logs_its_own_warn() {
        let db = db();
        let (_, lines) = read(&db, json!({"exclude_annotations": "yes"}));
        let l = only(&lines, "WARN", "Read conversation tool validation failed");
        assert!(l.contains("input="), "{l}");
        none_at(&lines, "INFO");

        let missing = "00000000-0000-4000-8000-0000000000ff";
        let (_, lines) = read(&db, json!({"conversationId": missing}));
        let l = only(&lines, "WARN", "Read conversation tool: chat not found");
        assert!(
            l.contains(&format!("requestedConversationId={missing}")),
            "{l}"
        );

        let (_, lines) = read(&db, json!({"conversationId": OTHER_CHAT}));
        let l = only(
            &lines,
            "WARN",
            "Read conversation tool: character does not participate in target chat",
        );
        assert!(l.contains(&format!("characterId={FRIDAY}")), "{l}");
        none_at(&lines, "INFO");
    }

    /// v4's `repos.chats.findById` is the fallback `_findById`: a failed chat
    /// read logs the repository's `Error finding entity by ID` and the tool
    /// takes its NOT-FOUND arm (`Conversation not found.` + the chat-not-found
    /// WARN) — never the execution-failed ERROR, which nothing on this path can
    /// reach in v4 (the `97b25fc53` unification review; P4.D235 had pinned the
    /// catch as v4's).
    #[test]
    fn a_failed_chat_read_takes_v4s_not_found_arm() {
        let db = db();
        db.write_blocking(|w| {
            w.main().connection().execute_batch("DROP TABLE chats")?;
            Ok(())
        })
        .unwrap();
        let (out, lines) = read(&db, json!({}));
        assert!(!out.success);
        assert_eq!(
            out.error.as_deref(),
            Some("Conversation not found."),
            "{out:?}"
        );
        // The repository line carries its module target (`quilltap_core::db::chats_read`).
        let db_err: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("ERROR ") && l.contains("Error finding entity by ID"))
            .collect();
        assert_eq!(db_err.len(), 1, "{lines:?}");
        assert!(
            db_err[0].contains("Error finding entity by ID")
                && db_err[0].contains("collection=chats"),
            "{}",
            db_err[0]
        );
        only(&lines, "WARN", "Read conversation tool: chat not found");
        none_at(&lines, "ERROR");
        none_at(&lines, "INFO");
    }

    /// v4's `findByChatId` is a fallback `safeQuery`: with no annotations table
    /// the read still SUCCEEDS, un-annotated, and logs v4's db ERROR — never
    /// the tool's execution-failed line.
    #[test]
    fn a_failed_annotations_read_falls_back_to_none() {
        let db = db();
        drop_annotations(&db);
        let (out, lines) = read(&db, json!({}));
        assert!(out.success, "{out:?}");
        let db_err: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("ERROR quilltap::db"))
            .collect();
        assert_eq!(db_err.len(), 1, "{lines:?}");
        assert!(
            db_err[0].contains("Error finding annotations by chat ID")
                && db_err[0].contains("collection=conversation_annotations"),
            "{}",
            db_err[0]
        );
        none_at(&lines, "ERROR");
        only(&lines, "INFO", "Read conversation tool completed");
    }
}
