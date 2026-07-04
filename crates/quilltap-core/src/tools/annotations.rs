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

use serde::Serialize;
use serde_json::{Number, Value};

use crate::db::conversation_annotations::CaUpsertInput;
use crate::db::runtime::Db;
use crate::db::{chats_read, conversation_annotations, DbError};

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
fn coerce_message_index_fallback(args: &Value) -> Number {
    let coerced: f64 = match args.get("message_index") {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
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
    let n = mi.as_f64()?;
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
    let n = obj.get("message_index")?.as_f64()?;
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
pub async fn execute_upsert_annotation(
    db: &Db,
    _user_id: &str,
    chat_id: &str,
    character_name: &str,
    args: &Value,
) -> UpsertAnnotationOutput {
    // All pre-checks + the action decision are synchronous reads; the actual
    // upsert is the one `db.write(...).await`.
    let plan = match upsert_plan(db, chat_id, character_name, args) {
        Ok(p) => p,
        Err(e) => {
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
        return UpsertAnnotationOutput {
            success: false,
            message_index: coerce_message_index_fallback(args),
            character_name: None,
            action: None,
            error: Some(e.to_string()),
        };
    }

    UpsertAnnotationOutput {
        success: true,
        message_index: Number::from(message_index),
        character_name: Some(character_name.to_string()),
        action: Some(action.to_string()),
        error: None,
    }
}

/// The synchronous pre-write validation/read chain for `upsert_annotation`.
fn upsert_plan(
    db: &Db,
    chat_id: &str,
    character_name: &str,
    args: &Value,
) -> Result<UpsertPlan, DbError> {
    let (message_index, content) = match validate_upsert_input(args) {
        Some(v) => v,
        None => {
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
            return Ok(UpsertPlan::Done(UpsertAnnotationOutput {
                success: false,
                message_index: mi_num,
                character_name: None,
                action: None,
                error: Some("Chat not found.".to_string()),
            }));
        }
    };

    let rendered = chat
        .get("renderedMarkdown")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let rendered = match rendered {
        Some(r) => r,
        None => {
            return Ok(UpsertPlan::Done(UpsertAnnotationOutput {
                success: false,
                message_index: mi_num,
                character_name: None,
                action: None,
                error: Some("Conversation has not been rendered yet.".to_string()),
            }));
        }
    };

    // Range check.
    let message_count = count_message_headers(rendered);
    if message_index >= message_count {
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
    let existing =
        db.read_main(|c| {
            conversation_annotations::ConversationAnnotationsRepository::new(c)
                .find_by_message_index(chat_id, message_index as f64, character_name)
        })?;
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
