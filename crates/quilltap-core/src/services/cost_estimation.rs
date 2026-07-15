//! The chat cost-breakdown READ service — v4
//! `lib/services/cost-estimation.service.ts` (`getChatCostBreakdown` +
//! `getDetailedChatCostBreakdown`).
//!
//! ## What is NOT here
//!
//!   - **`estimateMessageCost`** — the same v4 file's third export — is already
//!     ported as [`crate::services::pricing_fetcher::PricingFetcher::estimate_message_cost`]
//!     (W4.7e). It takes no part in either read path below.
//!   - **Live pricing fetch (OpenRouter)** likewise plays NO part here. Both
//!     breakdowns are pure reads over already-stored numbers — the chat row's
//!     running aggregates, or the per-row token columns. Nothing on this path
//!     ever contacts a pricing API, and a future surveyor should not add one:
//!     v4 doesn't.
//!
//! ## Two deliberate v4 quirks, carried
//!
//! 1. **Per-message cost is always `null` / `'unavailable'`** in the detailed
//!    breakdown. v4's own comment says why ("Would need model info to estimate")
//!    — the message rows store no provider/model, so there is nothing to price
//!    against. This is v4's behavior, not a v5 deferral.
//! 2. **The system-event `source` asymmetry.** The cost *accumulation* guard
//!    tests `!== null && !== undefined`, but the per-event `source` ternary tests
//!    `!== null` ONLY. A row whose `estimatedCostUSD` is `undefined` therefore
//!    lands `cost: null` (via `?? null`) with `source: 'registry'`. And since
//!    [`crate::db::chats_messages_read`] OMITS a NULL nullable-optional column
//!    (v4's repo hydration does the same), `undefined` is what EVERY uncosted
//!    system event actually presents — which makes the `'unavailable'` arm of
//!    that ternary unreachable through the repo read. Carried verbatim; the
//!    `cost_background_routes_equivalence` differential pins it.
//!
//! Neither read ever throws: v4 wraps each in a try/catch returning the
//! all-zeros `'unavailable'` object, and so does the port (a missing chat, a
//! missing message table, a decode failure — all land on the same zeros).

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::js_number_to_json;
use crate::db::{chats_messages_read, chats_read};

/// v4's all-zeros fallback object — the chat-not-found arm, the no-messages arm,
/// and both catch arms.
fn zeros() -> Value {
    json!({
        "totalTokens": 0,
        "promptTokens": 0,
        "completionTokens": 0,
        "estimatedCostUSD": Value::Null,
        "priceSource": "unavailable",
    })
}

/// `chat.totalPromptTokens || 0` — the JS-falsy coercion (a `0`, a NULL-omitted
/// key, and a NaN all land on `0`).
fn js_or_zero(v: Option<&Value>) -> f64 {
    match v.and_then(Value::as_f64) {
        Some(n) if n != 0.0 && !n.is_nan() => n,
        _ => 0.0,
    }
}

/// `event.promptTokens ?? 0` — the NULLISH coercion (absent/null → `0`, but a
/// stored `0` stays `0` and a stored NaN would stay NaN).
fn nullish_or_zero(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Null) | None => 0.0,
        Some(other) => other.as_f64().unwrap_or(0.0),
    }
}

/// v4 `getChatCostBreakdown`: the chat row's STORED aggregates, no recompute.
/// `priceSource` is the stored value when set, else the legacy inference (a
/// non-null cost with no stored source → `'registry'`), else `'unavailable'`.
pub fn get_chat_cost_breakdown(conn: &Connection, chat_id: &str) -> Value {
    let Ok(Some(chat)) = chats_read::find_by_id(conn, chat_id) else {
        // v4: chat-not-found warns and returns the zeros object; the catch arm
        // returns the same. Both collapse here.
        return zeros();
    };

    let total_prompt_tokens = js_or_zero(chat.get("totalPromptTokens"));
    let total_completion_tokens = js_or_zero(chat.get("totalCompletionTokens"));
    let total_tokens = total_prompt_tokens + total_completion_tokens;
    // `chat.estimatedCostUSD ?? null`.
    let estimated_cost_usd = match chat.get("estimatedCostUSD") {
        Some(Value::Null) | None => Value::Null,
        Some(v) => v.clone(),
    };

    // `if (chat.priceSource) … else if (estimatedCostUSD !== null) 'registry'`.
    let price_source = match chat
        .get("priceSource")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(stored) => stored.to_string(),
        None if !estimated_cost_usd.is_null() => "registry".to_string(),
        None => "unavailable".to_string(),
    };

    json!({
        "totalTokens": js_number_to_json(total_tokens),
        "promptTokens": js_number_to_json(total_prompt_tokens),
        "completionTokens": js_number_to_json(total_completion_tokens),
        "estimatedCostUSD": estimated_cost_usd,
        "priceSource": price_source,
    })
}

/// v4 `getDetailedChatCostBreakdown`: walk every event, itemize the message +
/// system-event rows, and sum. Note the top-level `totalTokens` is
/// `promptTokens + completionTokens` — NOT the sum of the per-row `totalTokens`
/// (which may diverge: a message's `tokenCount` overrides the sum for that row
/// only).
pub fn get_detailed_chat_cost_breakdown(conn: &Connection, chat_id: &str) -> Value {
    let Ok(messages) = chats_messages_read::get_messages(conn, chat_id) else {
        return zeros();
    };
    if messages.is_empty() {
        return zeros();
    }

    let mut message_breakdown: Vec<Value> = Vec::new();
    let mut system_event_breakdown: Vec<Value> = Vec::new();
    let mut total_prompt_tokens = 0.0_f64;
    let mut total_completion_tokens = 0.0_f64;
    let mut total_cost = 0.0_f64;
    let mut has_any_cost = false;

    for event in &messages {
        match event.get("type").and_then(Value::as_str) {
            Some("message") => {
                let prompt_tokens = nullish_or_zero(event.get("promptTokens"));
                let completion_tokens = nullish_or_zero(event.get("completionTokens"));
                // `event.tokenCount ?? (promptTokens + completionTokens)`.
                let total = match event.get("tokenCount") {
                    Some(Value::Null) | None => prompt_tokens + completion_tokens,
                    Some(v) => v.as_f64().unwrap_or(0.0),
                };

                total_prompt_tokens += prompt_tokens;
                total_completion_tokens += completion_tokens;

                message_breakdown.push(json!({
                    "id": event.get("id").cloned().unwrap_or(Value::Null),
                    "promptTokens": js_number_to_json(prompt_tokens),
                    "completionTokens": js_number_to_json(completion_tokens),
                    "totalTokens": js_number_to_json(total),
                    // v4: "Would need model info to estimate" — always null.
                    "cost": Value::Null,
                    "source": "unavailable",
                }));
            }
            Some("system") => {
                let prompt_tokens = nullish_or_zero(event.get("promptTokens"));
                let completion_tokens = nullish_or_zero(event.get("completionTokens"));
                let total = match event.get("totalTokens") {
                    Some(Value::Null) | None => prompt_tokens + completion_tokens,
                    Some(v) => v.as_f64().unwrap_or(0.0),
                };

                total_prompt_tokens += prompt_tokens;
                total_completion_tokens += completion_tokens;

                // The ACCUMULATION guard: `!== null && !== undefined`.
                let cost_value = event.get("estimatedCostUSD");
                if let Some(cost) = cost_value.filter(|v| !v.is_null()).and_then(Value::as_f64) {
                    total_cost += cost;
                    has_any_cost = true;
                }

                system_event_breakdown.push(json!({
                    "id": event.get("id").cloned().unwrap_or(Value::Null),
                    "type": event.get("systemEventType").cloned().unwrap_or(Value::Null),
                    "promptTokens": js_number_to_json(prompt_tokens),
                    "completionTokens": js_number_to_json(completion_tokens),
                    "totalTokens": js_number_to_json(total),
                    // `event.estimatedCostUSD ?? null`.
                    "cost": match cost_value {
                        Some(Value::Null) | None => Value::Null,
                        Some(v) => v.clone(),
                    },
                    // The SOURCE ternary: `!== null` ONLY — an ABSENT (undefined)
                    // column passes it. See the module docs.
                    "source": match cost_value {
                        Some(Value::Null) => "unavailable",
                        _ => "registry",
                    },
                }));
            }
            // `context-summary` rows (and anything else) are itemized by neither
            // branch and contribute nothing.
            _ => {}
        }
    }

    json!({
        "totalTokens": js_number_to_json(total_prompt_tokens + total_completion_tokens),
        "promptTokens": js_number_to_json(total_prompt_tokens),
        "completionTokens": js_number_to_json(total_completion_tokens),
        "estimatedCostUSD": if has_any_cost { js_number_to_json(total_cost) } else { Value::Null },
        "priceSource": if has_any_cost { "registry" } else { "unavailable" },
        "messageBreakdown": message_breakdown,
        "systemEventBreakdown": system_event_breakdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_or_zero_coerces_falsy() {
        assert_eq!(js_or_zero(Some(&json!(1200))), 1200.0);
        assert_eq!(js_or_zero(Some(&json!(0))), 0.0);
        assert_eq!(js_or_zero(Some(&Value::Null)), 0.0);
        assert_eq!(js_or_zero(None), 0.0);
    }

    #[test]
    fn nullish_or_zero_keeps_a_stored_zero() {
        assert_eq!(nullish_or_zero(Some(&json!(0))), 0.0);
        assert_eq!(nullish_or_zero(Some(&json!(7))), 7.0);
        assert_eq!(nullish_or_zero(Some(&Value::Null)), 0.0);
        assert_eq!(nullish_or_zero(None), 0.0);
    }

    #[test]
    fn zeros_object_shape() {
        let z = zeros();
        assert_eq!(z["totalTokens"], json!(0));
        assert_eq!(z["estimatedCostUSD"], Value::Null);
        assert_eq!(z["priceSource"], json!("unavailable"));
        // The non-detailed shape carries exactly five keys.
        assert_eq!(z.as_object().unwrap().len(), 5);
    }
}
