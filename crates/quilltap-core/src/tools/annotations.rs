//! Port of v4's `upsert_annotation` + `delete_annotation` tools (Project
//! Scriptorium): `lib/tools/handlers/upsert-annotation-handler.ts` +
//! `delete-annotation-handler.ts`, with their tool schemas
//! (`upsert-annotation-tool.ts` / `delete-annotation-tool.ts`).
//!
//! Both create/mutate a per-character annotation on a specific message index of a
//! chat; the calling character's name is resolved by the dispatcher and passed
//! in. The DB ops route through [`crate::db::conversation_annotations`] (the
//! `upsert` / `delete_annotation` methods).
//!
//! ## Faithful details
//!
//! - **`message_index`** is a `z.number().int().min(0)`. It maps to a REAL column
//!   (only-`.min()`, no `.max()`; see the repo module header), so the DB layer
//!   binds it as `f64`. In the tool OUTPUT it is echoed as a JSON number — a
//!   validated (integer) index renders bare (`3`), matching v4's `JSON.stringify`.
//! - The **validation-failure** `message_index` echo is v4's
//!   `Number(input.message_index) || 0`: coerce to a JS number, and any falsy
//!   result (NaN / 0) becomes `0`. Reproduced by [`coerce_message_index_fallback`].
//! - **content** is `z.string().min(1).max(2000)` — the length bound is in UTF-16
//!   code units (JS `.length`), so the port measures `chars().count()` over
//!   UTF-16… actually JS `String.length` counts UTF-16 units; see the note in
//!   [`content_len_utf16`].
//! - Error strings are byte-exact (the out-of-range template included).
//! - **The message count comes from a LIVE render** (v4 `f7f3d7bf0` dropped
//!   `chats.renderedMarkdown`): the shared
//!   [`crate::services::scriptorium_render::render_chat_conversation`] under the
//!   executor's injected clock, then the `/^### Message \d+/gm` tally. Zero
//!   headers (no events, or no visible message) → `"Conversation has no messages
//!   to annotate yet."` — where a stored column with no headers used to fall
//!   through to the out-of-range error.

use serde::Serialize;
use serde_json::{Number, Value};

use crate::db::conversation_annotations::CaUpsertInput;
use crate::db::runtime::Db;
use crate::db::{chats_read, conversation_annotations, DbError};
use crate::services::scriptorium_render::render_chat_conversation;
use crate::tools::llm_number::{js_number_from_str, llm_number};

// ============================================================================
// upsert_annotation
// ============================================================================

/// v4 `UpsertAnnotationToolOutput`. Field order matches v4's object construction
/// (`success`, `message_index`, then `character_name`/`action` on success or
/// `error` on failure); the optionals drop when absent.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UpsertAnnotationOutput {
    pub success: bool,
    pub message_index: Number,
    #[serde(rename = "character_name", skip_serializing_if = "Option::is_none")]
    pub character_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// v4 `DeleteAnnotationToolOutput`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DeleteAnnotationOutput {
    pub success: bool,
    pub message_index: Number,
    #[serde(rename = "character_name", skip_serializing_if = "Option::is_none")]
    pub character_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// JS `String.prototype.length` — UTF-16 code-unit count (astral chars count as
/// two). Zod's `.min`/`.max` on a string bound this quantity.
fn content_len_utf16(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// v4's validation-failure `message_index` echo: `Number(x) || 0`.
///
/// `Number(...)` on the raw JSON value: a JSON number is itself; a numeric string
/// coerces; anything else → NaN. Then JS `|| 0` replaces any falsy result (NaN,
/// 0, -0) with `0`. Returns the echoed number.
///
/// This is the ERROR-ECHO path, NOT validation — do not mistake it for the
/// `llm_number` seam (which is what `message_index` is actually guarded by; see
/// [`validate_upsert_input`]). It shares that module's string arm because JS
/// `Number(s)` is subtle enough to be worth having exactly once — `Number('0x10')`
/// is 16, `Number('inf')` is NaN — but the two paths mean different things.
fn coerce_message_index_fallback(args: &Value) -> Number {
    let coerced: f64 = match args.get("message_index") {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => js_number_from_str(s),
        Some(Value::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::Null) => 0.0,
        // Objects/arrays → NaN in JS `Number(...)`; and absent → the whole `input`
        // object test already gated this, but be safe.
        _ => f64::NAN,
    };
    // JS `|| 0`: NaN, 0, -0 are all falsy.
    if coerced == 0.0 || coerced.is_nan() {
        return Number::from(0);
    }
    number_from_f64(coerced)
}

/// Turn an `f64` into a `serde_json::Number` that renders like JS: an integer
/// value renders without a fractional part.
fn number_from_f64(v: f64) -> Number {
    if v.fract() == 0.0 && v.is_finite() && v.abs() < 9.007_199_254_740_992e15 {
        Number::from(v as i64)
    } else {
        Number::from_f64(v).unwrap_or_else(|| Number::from(0))
    }
}

/// Validate `upsert_annotation` input (Zod `safeParse`). `message_index` an
/// integer `>= 0`; `content` a string of 1–2000 UTF-16 units. Returns the parsed
/// `(message_index, content)` on success.
fn validate_upsert_input(args: &Value) -> Option<(i64, String)> {
    let obj = args.as_object()?;
    let mi = obj.get("message_index")?;
    // llmNumber(...): a numeric-looking string converts before the .int()/.min(0)
    // checks below, exactly as v4's z.preprocess does.
    let n = llm_number(mi).as_f64()?;
    // `.int()` — must be an integer value; `.min(0)`.
    if n.fract() != 0.0 || !n.is_finite() || n < 0.0 {
        return None;
    }
    let content = obj.get("content")?.as_str()?.to_string();
    let len = content_len_utf16(&content);
    if !(1..=2000).contains(&len) {
        return None;
    }
    Some((n as i64, content))
}

/// Validate `delete_annotation` input: `message_index` an integer `>= 0`.
fn validate_delete_input(args: &Value) -> Option<i64> {
    let obj = args.as_object()?;
    // llmNumber(...) — see `validate_upsert_input`.
    let n = llm_number(obj.get("message_index")?).as_f64()?;
    if n.fract() != 0.0 || !n.is_finite() || n < 0.0 {
        return None;
    }
    Some(n as i64)
}

/// Count `/^### Message \d+/gm` headers (used for the range check).
fn count_message_headers(markdown: &str) -> i64 {
    let mut count = 0i64;
    for line in markdown.split('\n') {
        if let Some(rest) = line.strip_prefix("### Message ") {
            if rest.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                count += 1;
            }
        }
    }
    count
}

/// The pre-write outcome of the upsert checks: either a terminal Output (a
/// validation/not-found/range error) or "proceed" — carrying the validated index,
/// content, and the resolved `created`/`updated` action.
enum UpsertPlan {
    Done(UpsertAnnotationOutput),
    Proceed {
        message_index: i64,
        content: String,
        action: &'static str,
    },
}

/// Execute the `upsert_annotation` tool (v4 `executeUpsertAnnotationTool`).
///
/// `now_iso` is the live render's wall clock (its `Current time:` header line);
/// the executor passes [`crate::clock::now_iso`], a differential a frozen
/// instant.
pub async fn execute_upsert_annotation(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_name: &str,
    args: &Value,
    now_iso: &str,
) -> UpsertAnnotationOutput {
    // v4's one outer `catch` (`:143-148`).
    let log_failure = |e: &DbError| {
        tracing::error!(
            target: "quilltap::tools",
            context = UPSERT_LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            characterName = character_name,
            error = %e,
            "Upsert annotation tool execution failed",
        );
    };
    // All pre-checks + the action decision are synchronous reads; the actual
    // upsert is the one `db.write(...).await`.
    let plan = match upsert_plan(db, user_id, chat_id, character_name, args, now_iso) {
        Ok(p) => p,
        Err(e) => {
            log_failure(&e);
            return UpsertAnnotationOutput {
                success: false,
                message_index: coerce_message_index_fallback(args),
                character_name: None,
                action: None,
                error: Some(e.to_string()),
            };
        }
    };

    let (message_index, content, action) = match plan {
        UpsertPlan::Done(out) => return out,
        UpsertPlan::Proceed {
            message_index,
            content,
            action,
        } => (message_index, content, action),
    };

    // Upsert on the writer thread.
    // JS `content.length` — UTF-16 code units.
    let content_length = content.encode_utf16().count();
    let chat_id_owned = chat_id.to_string();
    let character_name_owned = character_name.to_string();
    let write_res = db
        .write(move |ws| {
            ws.main()
                .conversation_annotations()
                .upsert(&CaUpsertInput {
                    chat_id: chat_id_owned,
                    message_index: message_index as f64,
                    source_message_id: None,
                    character_name: character_name_owned,
                    content,
                })?;
            Ok(())
        })
        .await;

    if let Err(e) = write_res {
        log_failure(&e);
        return UpsertAnnotationOutput {
            success: false,
            message_index: coerce_message_index_fallback(args),
            character_name: None,
            action: None,
            error: Some(e.to_string()),
        };
    }

    tracing::info!(
        target: "quilltap::tools",
        context = UPSERT_LOG_CONTEXT,
        userId = user_id,
        chatId = chat_id,
        characterName = character_name,
        messageIndex = message_index,
        action = action,
        contentLength = content_length,
        "Upsert annotation tool completed",
    );

    UpsertAnnotationOutput {
        success: true,
        message_index: Number::from(message_index),
        character_name: Some(character_name.to_string()),
        action: Some(action.to_string()),
        error: None,
    }
}

/// The synchronous pre-write validation/read chain for `upsert_annotation`.
/// v4's `context` field on every line the upsert handler logs.
const UPSERT_LOG_CONTEXT: &str = "upsert-annotation-handler";

fn upsert_plan(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_name: &str,
    args: &Value,
    now_iso: &str,
) -> Result<UpsertPlan, DbError> {
    let (message_index, content) = match validate_upsert_input(args) {
        Some(v) => v,
        None => {
            tracing::warn!(
                target: "quilltap::tools",
                context = UPSERT_LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                characterName = character_name,
                input = %args,
                "Upsert annotation tool validation failed",
            );
            return Ok(UpsertPlan::Done(UpsertAnnotationOutput {
                success: false,
                message_index: coerce_message_index_fallback(args),
                character_name: None,
                action: None,
                error: Some(
                    "Invalid input: message_index (integer >= 0) and content (string, 1-2000 chars) are required."
                        .to_string(),
                ),
            }));
        }
    };
    let mi_num = Number::from(message_index);

    // Load chat.
    let chat = match db.read_main(|c| chats_read::find_by_id(c, chat_id))? {
        Some(c) => c,
        None => {
            tracing::warn!(
                target: "quilltap::tools",
                context = UPSERT_LOG_CONTEXT,
                chatId = chat_id,
                userId = user_id,
                "Upsert annotation tool: chat not found",
            );
            return Ok(UpsertPlan::Done(UpsertAnnotationOutput {
                success: false,
                message_index: mi_num,
                character_name: None,
                action: None,
                error: Some("Chat not found.".to_string()),
            }));
        }
    };

    // Message indices are the renderer's numbering, so count them from a live
    // render rather than a stored copy (v4 :83-95).
    let rendered = db.read_main(|c| render_chat_conversation(c, &chat, now_iso))?;
    let message_count = rendered
        .as_ref()
        .map_or(0, |r| count_message_headers(&r.markdown));
    if message_count == 0 {
        return Ok(UpsertPlan::Done(UpsertAnnotationOutput {
            success: false,
            message_index: mi_num,
            character_name: None,
            action: None,
            error: Some("Conversation has no messages to annotate yet.".to_string()),
        }));
    }

    // Range check.
    if message_index >= message_count {
        tracing::warn!(
            target: "quilltap::tools",
            context = UPSERT_LOG_CONTEXT,
            chatId = chat_id,
            messageIndex = message_index,
            messageCount = message_count,
            "Upsert annotation tool: message_index out of range",
        );
        return Ok(UpsertPlan::Done(UpsertAnnotationOutput {
            success: false,
            message_index: mi_num,
            character_name: None,
            action: None,
            error: Some(format!(
                "Message index {message_index} is out of range. The conversation has {message_count} messages (0-{}).",
                message_count - 1
            )),
        }));
    }

    // Determine action (existing → updated, else created) BEFORE the upsert.
    // v4's `findByMessageIndex` is a FALLBACK `safeQuery` (a failed read logs
    // v4's ERROR and answers `null` → "created"); P4.D235 ported it with the
    // live render, which made this read reachable on more instances.
    let existing = db
        .read_main(|c| {
            conversation_annotations::ConversationAnnotationsRepository::new(c)
                .find_by_message_index(chat_id, message_index as f64, character_name)
        })
        .unwrap_or_else(|e| {
            tracing::error!(
                target: "quilltap::db",
                collection = "conversation_annotations",
                chatId = chat_id,
                messageIndex = message_index,
                characterName = character_name,
                error = %e,
                "Error finding annotation by message index",
            );
            None
        });
    let action = if existing.is_some() {
        "updated"
    } else {
        "created"
    };

    Ok(UpsertPlan::Proceed {
        message_index,
        content,
        action,
    })
}

/// Format upsert result (v4 `formatUpsertAnnotationResults`).
pub fn format_upsert_annotation(out: &UpsertAnnotationOutput) -> String {
    if !out.success {
        return out
            .error
            .clone()
            .unwrap_or_else(|| "Unknown error upserting annotation.".to_string());
    }
    format!(
        "Annotation {} on Message {} by {}.",
        out.action.as_deref().unwrap_or(""),
        out.message_index,
        out.character_name.as_deref().unwrap_or(""),
    )
}

// ============================================================================
// delete_annotation
// ============================================================================

/// Execute the `delete_annotation` tool (v4 `executeDeleteAnnotationTool`).
pub async fn execute_delete_annotation(
    db: &Db,
    _user_id: &str,
    chat_id: &str,
    character_name: &str,
    args: &Value,
) -> DeleteAnnotationOutput {
    let message_index = match validate_delete_input(args) {
        Some(v) => v,
        None => {
            return DeleteAnnotationOutput {
                success: false,
                message_index: coerce_message_index_fallback(args),
                character_name: None,
                error: Some("Invalid input: message_index (integer >= 0) is required.".to_string()),
            };
        }
    };
    let mi_num = Number::from(message_index);

    // Attempt the delete on the writer thread (v4 `deleteAnnotation`).
    let chat_id_owned = chat_id.to_string();
    let character_name_owned = character_name.to_string();
    let deleted = db
        .write(move |ws| {
            ws.main().conversation_annotations().delete_annotation(
                &chat_id_owned,
                message_index as f64,
                &character_name_owned,
            )
        })
        .await;

    let deleted = match deleted {
        Ok(d) => d,
        Err(e) => {
            return DeleteAnnotationOutput {
                success: false,
                message_index: coerce_message_index_fallback(args),
                character_name: None,
                error: Some(e.to_string()),
            };
        }
    };

    if !deleted {
        return DeleteAnnotationOutput {
            success: false,
            message_index: mi_num,
            character_name: Some(character_name.to_string()),
            error: Some("No annotation found for this message.".to_string()),
        };
    }

    DeleteAnnotationOutput {
        success: true,
        message_index: mi_num,
        character_name: Some(character_name.to_string()),
        error: None,
    }
}

/// Format delete result (v4 `formatDeleteAnnotationResults`).
pub fn format_delete_annotation(out: &DeleteAnnotationOutput) -> String {
    if !out.success {
        return out
            .error
            .clone()
            .unwrap_or_else(|| "Unknown error deleting annotation.".to_string());
    }
    format!("Annotation removed from Message {}.", out.message_index)
}

/// P4.D235 — the scriptorium-tool fixture + v4's restored log lines, pinned
/// with silence legs. Shared with `read_conversation`'s tests.
#[cfg(test)]
pub(crate) mod log_tests {
    use super::*;
    use crate::test_support::global_capture::{capture, install};
    use serde_json::json;

    pub(crate) const USER: &str = "00000000-0000-4000-8000-00000000000a";
    pub(crate) const FRIDAY: &str = "00000000-0000-4000-8000-0000000000b1";
    pub(crate) const SEAT: &str = "00000000-0000-4000-8000-0000000000a1";
    pub(crate) const CHAT: &str = "00000000-0000-4000-8000-0000000000c1";
    pub(crate) const OTHER_CHAT: &str = "00000000-0000-4000-8000-0000000000c2";
    pub(crate) const NOW: &str = "2026-09-28T12:00:00.000Z";

    /// A fresh-schema main DB: Friday, a two-message chat she sits in, and a
    /// chat she does not.
    pub(crate) fn db() -> Db {
        install();
        let dir = std::env::temp_dir().join(format!(
            "qt-scriptorium-tools-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open_main(
            dir.join("main.db"),
            "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
        )
        .unwrap();
        let schema: Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        let ddl: Vec<String> = schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect();
        let participants = json!([{
            "id": SEAT, "type": "CHARACTER", "characterId": FRIDAY, "controlledBy": "llm",
            "createdAt": NOW, "updatedAt": NOW,
        }])
        .to_string();
        db.write_blocking(move |w| {
            let c = w.main().connection();
            for s in &ddl {
                c.execute_batch(s)?;
            }
            c.execute(
                "INSERT INTO characters (id, userId, name, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'Friday', ?3, ?3)",
                rusqlite::params![FRIDAY, USER, NOW],
            )?;
            for (id, parts) in [(CHAT, participants.as_str()), (OTHER_CHAT, "[]")] {
                c.execute(
                    "INSERT INTO chats (id, userId, participants, title, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, 'A Quiet Evening', ?4, ?4)",
                    rusqlite::params![id, USER, parts, NOW],
                )?;
            }
            for (id, role, content, at) in [
                (
                    "00000000-0000-4000-8000-0000000000e1",
                    "USER",
                    "Good evening.",
                    "2026-09-01T10:01:00.000Z",
                ),
                (
                    "00000000-0000-4000-8000-0000000000e2",
                    "ASSISTANT",
                    "And to you.",
                    "2026-09-01T10:02:00.000Z",
                ),
            ] {
                c.execute(
                    "INSERT INTO chat_messages (id, chatId, type, role, content, participantId, \
                     createdAt) VALUES (?1, ?2, 'message', ?3, ?4, ?5, ?6)",
                    rusqlite::params![id, CHAT, role, content, SEAT, at],
                )?;
            }
            Ok(())
        })
        .unwrap();
        db
    }

    pub(crate) fn drop_annotations(db: &Db) {
        db.write_blocking(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE conversation_annotations")?;
            Ok(())
        })
        .unwrap();
    }

    fn upsert(db: &Db, chat: &str, args: Value) -> (UpsertAnnotationOutput, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        capture(|| {
            rt.block_on(execute_upsert_annotation(
                db, USER, chat, "Friday", &args, NOW,
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
            hits[0].contains("context=upsert-annotation-handler"),
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
    fn a_created_annotation_logs_v4s_completion_line_and_nothing_else() {
        let db = db();
        let (out, lines) = upsert(&db, CHAT, json!({"message_index": 1, "content": "Noted."}));
        assert!(out.success, "{out:?}");
        let line = only(&lines, "INFO", "Upsert annotation tool completed");
        for f in [
            "userId=00000000-0000-4000-8000-00000000000a",
            "chatId=00000000-0000-4000-8000-0000000000c1",
            "characterName=Friday",
            "messageIndex=1",
            "action=created",
            "contentLength=6",
        ] {
            assert!(line.contains(f), "{f} missing: {line}");
        }
        none_at(&lines, "WARN");
        none_at(&lines, "ERROR");
    }

    #[test]
    fn each_upsert_refusal_logs_its_own_warn() {
        let db = db();
        let (_, lines) = upsert(&db, CHAT, json!({"message_index": 1, "content": ""}));
        let l = only(&lines, "WARN", "Upsert annotation tool validation failed");
        assert!(
            l.contains("characterName=Friday") && l.contains("input="),
            "{l}"
        );
        none_at(&lines, "INFO");

        let missing = "00000000-0000-4000-8000-0000000000ff";
        let (out, lines) = upsert(&db, missing, json!({"message_index": 0, "content": "x"}));
        assert_eq!(out.error.as_deref(), Some("Chat not found."));
        only(&lines, "WARN", "Upsert annotation tool: chat not found");
        none_at(&lines, "INFO");

        let (_, lines) = upsert(&db, CHAT, json!({"message_index": 9, "content": "x"}));
        let l = only(
            &lines,
            "WARN",
            "Upsert annotation tool: message_index out of range",
        );
        assert!(
            l.contains("messageIndex=9") && l.contains("messageCount=2"),
            "{l}"
        );
        none_at(&lines, "INFO");

        // The no-messages refusal (v4 `:89-95`) logs NOTHING — the silence leg.
        let (out, lines) = upsert(&db, OTHER_CHAT, json!({"message_index": 0, "content": "x"}));
        assert_eq!(
            out.error.as_deref(),
            Some("Conversation has no messages to annotate yet.")
        );
        none_at(&lines, "WARN");
        none_at(&lines, "INFO");
        none_at(&lines, "ERROR");
    }

    #[test]
    fn a_failed_read_logs_v4s_execution_failed_error() {
        let db = db();
        drop_annotations(&db);
        let (out, lines) = upsert(&db, CHAT, json!({"message_index": 1, "content": "x"}));
        assert!(!out.success);
        let l = only(&lines, "ERROR", "Upsert annotation tool execution failed");
        assert!(
            l.contains("characterName=Friday") && l.contains("error="),
            "{l}"
        );
        none_at(&lines, "INFO");
    }
}
