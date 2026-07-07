//! Cost / system-event writer — v4 `lib/services/system-events.service.ts`
//! (`createSystemEvent` + the `createMemoryExtractionEvent` /
//! `createTitleGenerationEvent` / `createContextSummaryEvent` convenience
//! wrappers).
//!
//! A system event is a SYSTEM `chat_messages` row (`type: 'system'`) carrying a
//! `systemEventType` + description + optional token/cost fields. It appears in the
//! chat timeline and, when the event carries token usage, ALSO bumps the chat's
//! running token aggregates via the ported
//! [`crate::db::chats_tokens::ChatTokensRepository::increment_token_aggregates`]
//! (v4 `updateChatTokenAggregates` → `incrementTokenAggregates`).
//!
//! The insert itself is the byte-exact `db::chats_messages` marshaling (the row is
//! built as a `serde_json::json!` object matching v4's `SystemEvent` literal, then
//! validated into a [`ChatEventInput`]). Errors never propagate — v4 wraps
//! `createSystemEvent` in a try/catch returning `null`, and
//! `updateChatTokenAggregates` swallows its own errors — so the SYSTEM row is
//! posted best-effort and the token bump is a separate best-effort write (a bump
//! failure does NOT lose the event).

use serde_json::json;

use crate::clock::now_iso;
use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;

/// v4 `SystemEventInput` — the fields a caller supplies.
#[derive(Clone, Debug, Default)]
pub struct SystemEventInput {
    pub system_event_type: String,
    pub description: String,
    pub prompt_tokens: Option<f64>,
    pub completion_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub estimated_cost_usd: Option<f64>,
}

/// v4 `hasTokenUsage`: any of prompt/completion/total is defined.
fn has_token_usage(event: &SystemEventInput) -> bool {
    event.prompt_tokens.is_some()
        || event.completion_tokens.is_some()
        || event.total_tokens.is_some()
}

/// v4 `createSystemEvent`: post a SYSTEM row, then (when the event carries token
/// usage) bump the chat token aggregates. Returns the minted event id on success,
/// or `None` when the SYSTEM insert itself failed (v4's outer catch → `null`).
///
/// The token bump mirrors v4 `updateChatTokenAggregates`: `promptTokens || 0` /
/// `completionTokens || 0`, an early return when both are zero, otherwise
/// `incrementTokenAggregates(chatId, promptTokens, completionTokens,
/// estimatedCostUSD ?? null, priceSource)`. It is a SEPARATE best-effort write —
/// a bump failure is swallowed and the event id is still returned (v4's inner
/// try/catch).
pub async fn create_system_event(
    db: &Db,
    chat_id: &str,
    event: &SystemEventInput,
) -> Option<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_iso();

    // v4 `SystemEvent` object literal — each optional key set to `?? null`.
    let message = json!({
        "type": "system",
        "id": id,
        "systemEventType": event.system_event_type,
        "description": event.description,
        "promptTokens": event.prompt_tokens,
        "completionTokens": event.completion_tokens,
        "totalTokens": event.total_tokens,
        "provider": event.provider,
        "modelName": event.model_name,
        "estimatedCostUSD": event.estimated_cost_usd,
        "createdAt": now,
    });
    let parsed: ChatEventInput = serde_json::from_value(message).ok()?;

    let write_chat_id = chat_id.to_string();
    let posted = db
        .write(move |writers| {
            writers
                .main()
                .chat_messages()
                .add_message(&write_chat_id, &parsed)
        })
        .await;
    if posted.is_err() {
        // v4's outer catch → null.
        return None;
    }

    // v4 `updateChatTokenAggregates` (its own best-effort try/catch).
    if has_token_usage(event) {
        let prompt_tokens = event.prompt_tokens.unwrap_or(0.0);
        let completion_tokens = event.completion_tokens.unwrap_or(0.0);
        if prompt_tokens != 0.0 || completion_tokens != 0.0 {
            let cost = event.estimated_cost_usd;
            let write_chat_id = chat_id.to_string();
            // priceSource is not threaded here (v4's callers pass none for these
            // system-event wrappers).
            let _ = db
                .write(move |writers| {
                    writers.main().chat_tokens().increment_token_aggregates(
                        &write_chat_id,
                        prompt_tokens,
                        completion_tokens,
                        cost,
                        None,
                    )
                })
                .await;
        }
    }

    Some(id)
}

/// A token-usage triple (v4's `{ promptTokens?, completionTokens?, totalTokens? }`).
#[derive(Clone, Copy, Debug, Default)]
pub struct TokenUsage {
    pub prompt_tokens: Option<f64>,
    pub completion_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
}

/// v4 `createMemoryExtractionEvent`.
pub async fn create_memory_extraction_event(
    db: &Db,
    chat_id: &str,
    usage: Option<TokenUsage>,
    provider: Option<String>,
    model_name: Option<String>,
    estimated_cost_usd: Option<f64>,
) -> Option<String> {
    create_system_event(
        db,
        chat_id,
        &SystemEventInput {
            system_event_type: "MEMORY_EXTRACTION".to_string(),
            description: "Extracted memories from conversation".to_string(),
            prompt_tokens: usage.and_then(|u| u.prompt_tokens),
            completion_tokens: usage.and_then(|u| u.completion_tokens),
            total_tokens: usage.and_then(|u| u.total_tokens),
            provider,
            model_name,
            estimated_cost_usd,
        },
    )
    .await
}

/// v4 `createTitleGenerationEvent`.
pub async fn create_title_generation_event(
    db: &Db,
    chat_id: &str,
    usage: Option<TokenUsage>,
    provider: Option<String>,
    model_name: Option<String>,
    estimated_cost_usd: Option<f64>,
) -> Option<String> {
    create_system_event(
        db,
        chat_id,
        &SystemEventInput {
            system_event_type: "TITLE_GENERATION".to_string(),
            description: "Generated chat title".to_string(),
            prompt_tokens: usage.and_then(|u| u.prompt_tokens),
            completion_tokens: usage.and_then(|u| u.completion_tokens),
            total_tokens: usage.and_then(|u| u.total_tokens),
            provider,
            model_name,
            estimated_cost_usd,
        },
    )
    .await
}

/// v4 `createContextSummaryEvent`.
pub async fn create_context_summary_event(
    db: &Db,
    chat_id: &str,
    usage: Option<TokenUsage>,
    provider: Option<String>,
    model_name: Option<String>,
    estimated_cost_usd: Option<f64>,
) -> Option<String> {
    create_system_event(
        db,
        chat_id,
        &SystemEventInput {
            system_event_type: "CONTEXT_SUMMARY".to_string(),
            description: "Generated context summary".to_string(),
            prompt_tokens: usage.and_then(|u| u.prompt_tokens),
            completion_tokens: usage.and_then(|u| u.completion_tokens),
            total_tokens: usage.and_then(|u| u.total_tokens),
            provider,
            model_name,
            estimated_cost_usd,
        },
    )
    .await
}
