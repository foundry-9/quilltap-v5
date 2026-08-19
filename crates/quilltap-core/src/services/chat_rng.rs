//! The manual RNG chat action (P4.9E3A) — a differential port of v4
//! `app/api/v1/chats/[id]/actions/rng.ts` (`handleRng` :32).
//!
//! The dice themselves were ported long ago ([`crate::tools::rng`], pinned
//! byte-for-byte by `rng_executor_equivalence` — including the rejection-sampling
//! loop's byte consumption), so this module is the ROUTE half only: v4's request
//! schema, the two human-readable strings it builds (`requestPrompt` and the
//! chip `summary`), the preview body, and the persisted TOOL message.
//!
//! ## Two shapes, one execution
//!
//! `preview: true` runs the tool and returns everything WITHOUT writing a
//! message — the composer's gutter uses it to show a roll before committing it.
//! Otherwise the same result is written as a `role: 'TOOL'` message whose
//! `content` is a JSON string (v4's own comment: "result field must be a string
//! for ToolMessage component compatibility" — so the persisted `result` is the
//! FORMATTED text, not the structured output).
//!
//! ## `summary` is computed and then, for a die, thrown away
//!
//! v4 builds `summary` for all three shapes but only the PREVIEW body carries
//! it; the persisted message and the non-preview body do not. Reproduced.
//!
//! ## `sum` is dice-only
//!
//! `flip_coin` / `spin_the_bottle` outputs carry no `sum`, and
//! `NextResponse.json` drops `undefined` — so the key is ABSENT from those
//! bodies rather than `null`. The port omits it the same way.
//!
//! Pinned by `chat_admin_routes_equivalence`.

use serde_json::{json, Map, Value};

use crate::api::types::Response;
use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::chat_admin::{
    bad_request, internal, load_chat, not_found, ok, validation_error,
};
use crate::tools::rng::{
    execute_rng_tool, format_rng_results, RandomBytes, RngToolContext, RngToolOutput,
};

/// v4's `rngRequestSchema.type`: either a die size (integer 2–1000) or one of the
/// two named shapes.
enum RngKind {
    Dice(i64),
    FlipCoin,
    SpinTheBottle,
}

/// v4 `rngRequestSchema.parse` — the union on `type`, `rolls` (int 1–100,
/// default 1) and `preview` (default false). A failure is v4's Zod 400.
fn parse_kind(kind: &Value) -> Option<RngKind> {
    if let Some(n) = kind.as_i64() {
        // `z.number().int().min(2).max(1000)` — a float would fail `.int()`.
        if kind.as_f64().is_some_and(|f| f.fract() != 0.0) {
            return None;
        }
        return (2..=1000).contains(&n).then_some(RngKind::Dice(n));
    }
    match kind.as_str() {
        Some("flip_coin") => Some(RngKind::FlipCoin),
        Some("spin_the_bottle") => Some(RngKind::SpinTheBottle),
        _ => None,
    }
}

/// v4's `requestPrompt` — the human-readable request line shown on the bubble.
fn request_prompt(kind: &RngKind, rolls: u32) -> String {
    match kind {
        RngKind::FlipCoin => {
            if rolls == 1 {
                "Flip a coin".to_string()
            } else {
                format!("Flip {rolls} coins")
            }
        }
        RngKind::SpinTheBottle => {
            if rolls == 1 {
                "Spin the bottle".to_string()
            } else {
                format!("Spin the bottle {rolls} times")
            }
        }
        RngKind::Dice(sides) => {
            if rolls == 1 {
                format!("Roll a d{sides}")
            } else {
                format!("Roll {rolls}d{sides}")
            }
        }
    }
}

/// v4's `summary` — the short chip label. Built for every shape but returned
/// ONLY in the preview body.
fn summary(kind: &RngKind, rolls: u32, output: &RngToolOutput) -> String {
    // v4 interpolates `result.results?.[0]` / `.join(', ')`, so a missing entry
    // would stringify as `undefined`; the success path always has `rolls` of them.
    let as_text = |v: &Value| -> String {
        match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    };
    let results: Vec<Value> = serde_json::to_value(&output.results)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let first = results.first().map(as_text).unwrap_or_default();
    match kind {
        RngKind::FlipCoin | RngKind::SpinTheBottle => {
            if rolls == 1 {
                first
            } else {
                results.iter().map(as_text).collect::<Vec<_>>().join(", ")
            }
        }
        RngKind::Dice(sides) => {
            if rolls == 1 {
                format!("d{sides}: {first}")
            } else {
                format!("{rolls}d{sides}: {}", output.sum.unwrap_or_default())
            }
        }
    }
}

/// Insert a key only when the value is `Some` — v4's `NextResponse.json` drops
/// `undefined` members, so a dice-only key must be ABSENT (not null) for a coin.
fn put_opt(map: &mut Map<String, Value>, key: &str, value: Option<impl Into<Value>>) {
    if let Some(v) = value {
        map.insert(key.to_string(), v.into());
    }
}

/// v4 `?action=rng` — execute the RNG tool and either preview the result or
/// persist it as a Prospero-less `TOOL` message.
///
/// `rng` is the randomness source ([`crate::tools::rng::OsRandomBytes`] in
/// production; the differential injects the same committed byte stream v4's
/// mocked `crypto.randomBytes` serves). `now_iso` / `message_id` are the two
/// minted values, injected so the differential can pin them.
#[allow(clippy::too_many_arguments)]
pub async fn chat_rng(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    kind: &Value,
    rolls: Option<u32>,
    preview: Option<bool>,
    rng: &mut (dyn RandomBytes + Send),
    message_id: String,
    now_iso: String,
) -> Response {
    if let Ok(None) = load_chat(db, chat_id) {
        return not_found("Chat");
    }

    let Some(parsed) = parse_kind(kind) else {
        return validation_error();
    };
    let rolls = rolls.unwrap_or(1);
    if !(1..=100).contains(&rolls) {
        return validation_error();
    }
    let preview = preview.unwrap_or(false);

    // v4 hands the WHOLE validated object to the tool; `executeRngTool` reads
    // `type` / `rolls` / `modifier` and ignores `preview`.
    let tool_input = json!({ "type": kind, "rolls": rolls });
    let output = match execute_rng_tool(
        db,
        &tool_input,
        &RngToolContext {
            user_id: user_id.to_string(),
            chat_id: chat_id.to_string(),
        },
        rng,
    ) {
        Ok(o) => o,
        Err(e) => return internal(e),
    };

    if !output.success {
        // v4 `badRequest(result.error || 'RNG execution failed')`.
        return bad_request(
            output
                .error
                .clone()
                .unwrap_or_else(|| "RNG execution failed".to_string()),
        );
    }

    let formatted = format_rng_results(&output);
    let prompt = request_prompt(&parsed, rolls);
    let chip = summary(&parsed, rolls, &output);
    let results = serde_json::to_value(&output.results).unwrap_or(Value::Null);
    let arguments = json!({ "type": kind, "rolls": rolls });

    if preview {
        let mut result = Map::new();
        result.insert("type".into(), output.type_.clone());
        result.insert("rollCount".into(), json!(output.roll_count));
        result.insert("results".into(), results);
        put_opt(&mut result, "sum", output.sum);
        result.insert("formattedText".into(), json!(formatted));
        result.insert("summary".into(), json!(chip));
        result.insert("requestPrompt".into(), json!(prompt));
        result.insert("arguments".into(), arguments);
        return ok(json!({
            "success": true,
            "preview": true,
            "result": Value::Object(result),
        }));
    }

    // The persisted TOOL message. `content` is a JSON STRING (v4's note: the
    // ToolMessage component needs `result` as text), so the key order below is
    // part of the wire.
    let content = json!({
        "tool": "rng",
        "initiatedBy": "user",
        "success": true,
        "result": formatted,
        "prompt": prompt,
        "arguments": arguments,
    });
    // Key order is `ChatEventSchema`'s, not the handler literal's — v4 returns
    // the VALIDATED object, and Zod re-emits in schema order (`attachments`
    // before `createdAt`).
    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "TOOL",
        "content": serde_json::to_string(&content).unwrap_or_default(),
        "attachments": [],
        "createdAt": now_iso,
    });

    let event: ChatEventInput = match serde_json::from_value(message.clone()) {
        Ok(e) => e,
        Err(e) => return internal(DbError::Internal(format!("rng message marshal: {e}"))),
    };
    let cid = chat_id.to_string();
    if let Err(e) = db
        .write(move |w| w.main().chat_messages().add_message(&cid, &event))
        .await
    {
        return internal(e);
    }

    let mut result = Map::new();
    result.insert("type".into(), output.type_.clone());
    result.insert("rollCount".into(), json!(output.roll_count));
    result.insert(
        "results".into(),
        serde_json::to_value(&output.results).unwrap_or(Value::Null),
    );
    put_opt(&mut result, "sum", output.sum);
    result.insert("formattedText".into(), json!(formatted));

    ok(json!({
        "success": true,
        "message": message,
        "result": Value::Object(result),
    }))
}
