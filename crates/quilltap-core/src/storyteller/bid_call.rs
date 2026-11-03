//! Storyteller bid — the menu offered (v4 `lib/storyteller/bid-call.ts`
//! `buildBidOffer`, P4.D343).
//!
//! The bid call is a full in-character call: the same connection profile,
//! identity stack, recall and transcript as a real turn (the turn path builds
//! it with `purpose == Bid`), with the bid section trailing. The same tool list
//! is sent with tool choice `"none"`, so the request shares the character's
//! cached prefix and warms it for the speaking turn; a provider that calls a
//! tool anyway has the call discarded. The call is logged as
//! `STORYTELLER_BID`, with the round's id and no message id.

use std::collections::BTreeMap;

use serde_json::Value;

use super::types::{StorytellerBidCallOutcome, StorytellerBidOffer};
use crate::message_formatter::normalize_content_block_format;
use crate::model::stream::{StreamParams, StreamingCompletionProvider};
use crate::model::stream_watchdog::{watch_stream, StallBudgets, StallWatchdogContext};
use crate::services::llm_logging::log_type;
use crate::services::primary_stream::{
    log_stream_message_call, EffectiveProfile, StreamLogCtx, StreamOutcome,
};
use crate::tools::effects::{max_effect_of_tool, EffectLookups, ToolEffect};

/// Tools never listed on a bid's menu: `run_custom` is listed by custom tool
/// instead (v4 `UNLISTED_TOOLS`).
const UNLISTED_TOOLS: [&str; 2] = ["run_custom", "submit_final_response"];

fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("function")
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| tool.get("name").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
}

/// The menu one character's bid is offered, from the tools its turn would get
/// (v4 `buildBidOffer`). Walks the slate in order; a nameless tool is skipped;
/// each listed tool counts at its highest possible effect (plugin effects
/// only — v4 passes no custom map here); the custom tools are listed only
/// when `run_custom` was on the slate.
pub fn build_bid_offer(
    tools: &[Value],
    custom_tool_effects: &BTreeMap<String, ToolEffect>,
    plugin_tool_effects: &BTreeMap<String, ToolEffect>,
    silent: bool,
) -> StorytellerBidOffer {
    let lookups = EffectLookups {
        custom_tool_effects: BTreeMap::new(),
        plugin_tool_effects: plugin_tool_effects.clone(),
    };
    let mut listed: Vec<(String, ToolEffect)> = Vec::new();
    let mut has_run_custom = false;
    for tool in tools {
        let Some(name) = tool_name(tool) else {
            continue;
        };
        if name == "run_custom" {
            has_run_custom = true;
        }
        if UNLISTED_TOOLS.contains(&name) {
            continue;
        }
        let effect = max_effect_of_tool(name, &lookups);
        // A JS object key assigned twice keeps its first position.
        match listed.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = effect,
            None => listed.push((name.to_string(), effect)),
        }
    }
    let custom_tools: Vec<(String, ToolEffect)> = if has_run_custom {
        custom_tool_effects
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    } else {
        Vec::new()
    };
    let destroy_offered = listed.iter().any(|(_, e)| *e == ToolEffect::Destroy);
    StorytellerBidOffer {
        tools: listed,
        custom_tools,
        destroy_offered,
        silent,
    }
}

/// The one bid call's inputs (v4 `callForBid`'s `StreamOptions`, minus the
/// `logType` / `toolChoice` it sets itself).
pub(crate) struct BidCall<'a, STR: StreamingCompletionProvider> {
    pub streaming: &'a STR,
    pub profile: &'a EffectiveProfile,
    pub base_url: Option<&'a str>,
    pub api_key: &'a str,
    /// The turn's params as the primary stream would send them; the call
    /// overrides `tool_choice` (`"none"`) and leaves the rest.
    pub params: StreamParams,
    pub log: StreamLogCtx<'a>,
    pub chat_id: &'a str,
    pub character_id: &'a str,
}

/// Make the one bid call and collect its text (v4 `callForBid`). Never fails:
/// a provider error is WARN `Storyteller bid call failed` and an `Err`
/// outcome; success is DEBUG `Storyteller bid call returned` (`length` in
/// UTF-16 code units). Whatever ends the stream, the funnel writes its ONE
/// `STORYTELLER_BID` row first (v4's generator `finally`).
pub(crate) async fn call_for_bid<STR: StreamingCompletionProvider>(
    call: BidCall<'_, STR>,
    offer: StorytellerBidOffer,
) -> StorytellerBidCallOutcome {
    let mut params = call.params;
    params.tool_choice = Some("none".to_string());
    // No turn stop: a bid builds outside any registered turn.
    let mut rx = watch_stream(
        call.streaming
            .stream_message_keyed(&call.profile.provider, call.base_url, call.api_key, &params)
            .await,
        StallBudgets::default(),
        StallWatchdogContext::streaming_service(&call.profile.provider, &call.profile.model_name)
            .with_ids(
                Some(call.log.user_id),
                Some(call.chat_id),
                Some(call.character_id),
                None,
            ),
    );
    let mut text = String::new();
    let mut raw: Option<Value> = None;
    let mut usage = None;
    let mut cache_usage = None;
    let mut raw_provider_usage: Option<Value> = None;
    let mut saw_done = false;
    let mut error: Option<String> = None;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            Ok(c) => {
                let content = normalize_content_block_format(&c.content);
                text.push_str(&content);
                if let Some(u) = &c.usage {
                    usage = Some(*u);
                }
                if let Some(cu) = &c.cache_usage {
                    cache_usage = Some(*cu);
                }
                if let Some(rpu) = &c.raw_provider_usage {
                    raw_provider_usage = Some(rpu.clone());
                }
                if let Some(r) = &c.raw_response {
                    raw = Some(r.clone());
                }
                if c.done {
                    saw_done = true;
                }
            }
            Err(e) => {
                error = Some(e.message.clone());
                break;
            }
        }
    }
    let outcome = match &error {
        Some(message) => StreamOutcome::Failed {
            error: message.clone(),
            chunk_count: rx.chunks_received(),
        },
        None if saw_done => StreamOutcome::Done { raw_response: raw },
        None => StreamOutcome::Exhausted {
            chunk_count: rx.chunks_received(),
        },
    };
    log_stream_message_call(
        &call.log,
        log_type::STORYTELLER_BID,
        call.profile,
        &params,
        text.clone(),
        usage,
        cache_usage,
        raw_provider_usage,
        outcome,
    )
    .await;
    if let Some(reason) = error {
        tracing::warn!(
            target: "quilltap::storyteller",
            chatId = %call.chat_id,
            characterId = %call.character_id,
            error = %reason,
            "Storyteller bid call failed"
        );
        return StorytellerBidCallOutcome::Err { reason };
    }
    tracing::debug!(
        target: "quilltap::storyteller",
        chatId = %call.chat_id,
        characterId = %call.character_id,
        length = crate::jsstr::utf16_len(&text),
        "Storyteller bid call returned"
    );
    StorytellerBidCallOutcome::Ok {
        text,
        discarded_tool_calls: Vec::new(),
        offered: offer,
    }
}
