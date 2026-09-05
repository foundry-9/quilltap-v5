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
    if let Err(e) = &posted {
        // v4 `system-events.service.ts:73-79` logs before returning null. v5
        // swallowed the write in silence, which since P4.49 means a lost SYSTEM
        // row leaves nothing at all in `combined.log` — the one place an
        // operator looks after a turn that came out wrong.
        //
        // v4's catch also spans `updateChatTokenAggregates`; v5 keeps that as a
        // separate best-effort write below (a bump failure must not lose the
        // event), and v4's own `updateChatTokenAggregates` swallows its errors,
        // so the reachable arm of that catch is exactly this one — the SYSTEM
        // insert failing.
        tracing::error!(
            target: "quilltap::system_events",
            chat_id = %chat_id,
            event_type = %event.system_event_type,
            error = %e,
            "Failed to create system event"
        );
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

#[cfg(test)]
mod log_context_tests {
    use super::*;
    use crate::test_support::captured_with as captured;

    // === P4.74: `createSystemEvent`'s swallowed write (v4
    // === `lib/services/system-events.service.ts:73-79`).
    //
    // v4 catches, logs `Failed to create system event` with
    // `{chatId, eventType, error}`, and returns null. v5 returned `None` in
    // silence — and since P4.49 made `combined.log` the place an operator looks
    // after a turn that came out wrong, a lost SYSTEM row left nothing there at
    // all.
    //
    // A differential cannot see a log-only fix, and no INPUT reaches this arm:
    // the write fails or it doesn't. So the failure is forced by breaking the
    // table (the `force-a-swallowed-catch-by-breaking-the-table` idiom) — the
    // fixture DB simply has no `chat_messages`.

    /// A main DB with NO `chat_messages` table — every `add_message` fails.
    fn db_without_chat_messages() -> Db {
        let dir = std::env::temp_dir().join(format!(
            "qt-cost-events-log-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Db::open_main(
            dir.join("main.db"),
            "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
        )
        .unwrap()
    }

    #[test]
    fn a_failed_system_event_write_is_logged_with_v4s_keys() {
        let db = db_without_chat_messages();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let event = SystemEventInput {
            system_event_type: "TITLE_GENERATION".to_string(),
            description: "The Host renamed the chat.".to_string(),
            ..Default::default()
        };
        let (id, lines) = captured(|| rt.block_on(create_system_event(&db, "chat-77", &event)));

        assert!(id.is_none(), "v4's catch returns null; v5 must return None");
        let line = lines
            .iter()
            .find(|l| l.contains("Failed to create system event"))
            .unwrap_or_else(|| panic!("the failed write was silent; got {lines:#?}"));
        for field in [
            "ERROR",
            "chat_id=chat-77",
            "event_type=TITLE_GENERATION",
            "error=",
        ] {
            assert!(line.contains(field), "line is missing {field}: {line}");
        }
    }
}
