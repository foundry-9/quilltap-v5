//! The native tool loop (Phase-3 Unit-3 wave 4, W4.1e) — v4
//! `lib/services/chat-message/native-tool-loop.service.ts` (`runNativeToolLoop`).
//!
//! The bounded **stream → execute native tool calls → re-stream** loop the
//! orchestrator runs after the primary stream completes. Each iteration:
//!
//!   1. detect provider-native tool calls in the last raw response (the injected
//!      [`ToolCallDetector`] seam — the provider wire parse is W4.7). Zero calls
//!      → break.
//!   2. In agent mode, `submit_final_response` terminates the loop and stamps
//!      `streaming.full_response` with the model's structured final answer; the
//!      **ghost-wrap guardrail** rejects a sole iteration-0
//!      `submit_final_response` with no prose.
//!   3. Per iteration: dispatch via [`process_tool_calls`], append the assistant
//!      turn + one message per tool result via the shared threading helpers
//!      ([`build_assistant_tool_call_message`] / [`build_tool_result_messages`]),
//!      re-stream with the same tool slate, accumulate into
//!      `streaming.full_response`.
//!   4. After the loop, if iterations hit `effective_max_turns` without a
//!      successful submit, agent mode runs a single **force-final** pass.
//!
//! The loop MUTATES the [`StreamingState`] in place (`full_response`, `usage`,
//! `cache_usage`, `attachment_results`, `raw_response`, `thought_signature`,
//! `reasoning_content`, `reasoning_segments`) and appends to `tool_messages` /
//! `generated_image_paths`. It does NOT persist the tool-result rows — that is
//! the finalizer's `save_tool_messages` (which consumes the accumulated
//! `tool_messages`). Its only DB write is the agent-mode `agentTurnCount` bump.
//!
//! ## Seams
//!
//! | v4 dependency | seam | corpus |
//! |---|---|---|
//! | `detectToolCallsInResponse` (provider wire parse, W4.7) | [`ToolCallDetector`] | canned by raw-response marker |
//! | `streamMessage` (the model stream) | [`StreamingCompletionProvider`] | canned streams (recorded keys) |
//! | `executeToolCallWithContext` (every handler, W4.1d) | [`ToolRunner`] | real batch-1 handlers + canned fallback |
//!
//! `ResolvedAgentMode` (the resolver is W4.4) is a plain input struct here.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{chats, DbError};
use crate::finish_reason::extract_finish_reason;
use crate::model::completion::{CompletionMessage, CompletionRole};
use crate::model::stream::{StreamError, StreamParams, StreamingCompletionProvider};

use super::agent_mode::{build_force_final_message, generate_iteration_summary, ResolvedAgentMode};
use super::chat_events::{ChatEvent, EventSink, StatusPayload};
use super::primary_stream::{
    apply_reasoning_chunk, flush_reasoning_segment, PreservePartialOnError, StreamingState,
};
use super::tool_call_threading::{
    build_assistant_tool_call_message, build_tool_result_messages, DetectedToolCall,
    ThreadedMessage,
};
use super::tool_execution::{
    process_tool_calls, GeneratedImage, StatusContext, ToolCall, ToolExecutionContext, ToolMessage,
    ToolProcessingResult, ToolRunner,
};

/// The synthetic tool result for an iteration-0 sole `submit_final_response` with
/// no accompanying prose — almost always a re-wrap of a previously-concluded turn
/// (v4 `GHOST_WRAP_REJECTION_MESSAGE`, verbatim).
const GHOST_WRAP_REJECTION_MESSAGE: &str = "Rejected: submit_final_response was called on the first iteration without any accompanying task work or conversational prose this turn. The previous turn already concluded — do not re-wrap completed work. Respond to the user's current message directly, in character, as natural prose. You may use memory or other tools first if helpful, but only call submit_final_response after completing fresh agentic work that warrants a structured summary.";

// ===========================================================================
// The tool-call detection seam (detectToolCallsInResponse)
// ===========================================================================

/// The tool-call detection boundary — v4's `detectToolCallsInResponse(rawResponse,
/// provider)` (the provider wire parse, W4.7). Injected so the differential can
/// replay canned detection identically on both sides; production wires the real
/// per-provider parser here.
pub trait ToolCallDetector {
    /// Extract native tool calls from a provider's raw response object. `[]` ⇒
    /// no tool calls (the loop breaks).
    fn detect(&self, raw_response: &Value, provider: &str) -> Vec<ToolCall>;
}

/// A detector that never finds tool calls (v4's no-tool-call response). The
/// production default until the W4.7 provider parse lands, and the orchestrator's
/// corpus-dormant default (the primary stream leaves `raw_response` null / carries
/// no native calls).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoToolCallDetector;

impl ToolCallDetector for NoToolCallDetector {
    fn detect(&self, _raw_response: &Value, _provider: &str) -> Vec<ToolCall> {
        Vec::new()
    }
}

// ===========================================================================
// Truncation guard (native-tool-loop.service.ts:91–105)
// ===========================================================================

/// Provider finish-reason strings meaning "the output was cut off at the token
/// ceiling" (v4 `isTruncatedFinishReason`, compared case-insensitively).
fn is_truncated_finish_reason(reason: Option<&str>) -> bool {
    match reason {
        None => false,
        Some(r) => {
            let r = r.to_lowercase();
            r == "max_tokens" || r == "length" || r == "model_length"
        }
    }
}

/// The tool result returned for a call cut off mid-arguments by the output-token
/// limit (v4 `buildToolTruncationMessage`, verbatim).
fn build_tool_truncation_message(tool_name: &str) -> String {
    format!(
        "Rejected: this {tool_name} call was cut off because the response reached the output token limit before the tool arguments finished generating. The arguments are incomplete, so the call was NOT executed. Retry with a smaller call — for file writes, write less content at once or use doc_str_replace for a targeted edit instead of rewriting the whole file — or ask the user to raise the output token limit (max_tokens) on this connection profile."
    )
}

// ===========================================================================
// Options / entry point
// ===========================================================================

/// Everything [`run_native_tool_loop`] needs beyond the injected provider / sink /
/// runner / detector / preserver (v4 `RunNativeToolLoopOptions` minus the pieces
/// resolved above the seam). The `state` / `tool_messages` / `generated_image_paths`
/// are mutated in place (v4's caller keeps live bindings).
pub struct RunNativeToolLoopOptions<'a> {
    pub chat_id: String,
    /// v4 `character.name` / `character.id` — the status-frame + threading identity.
    pub character_id: String,
    pub character_name: String,
    pub agent_mode: ResolvedAgentMode,
    /// The provider name (v4 `streaming.effectiveProfile.provider`) — detection +
    /// the re-stream call both key on it.
    pub provider: String,
    /// The connection profile base URL (v4 `createLLMProvider(..., baseUrl)`).
    pub base_url: Option<String>,
    /// The slate that primed the primary stream (v4 `formattedMessages`). The loop
    /// builds on top by copy.
    pub formatted_messages: Vec<ThreadedMessage>,
    /// The base stream params (model / temperature / tools / web-search / …) — the
    /// re-stream clones these and overwrites `messages` per iteration.
    pub base_params: StreamParams,
    /// The tool-execution context handed to every handler (v4 `toolContext`).
    pub tool_context: ToolExecutionContext,
    /// v4 `streaming` — mutated in place.
    pub state: &'a mut StreamingState,
    /// v4 `toolMessages` — appended in place.
    pub tool_messages: &'a mut Vec<ToolMessage>,
    /// v4 `generatedImagePaths` — appended in place.
    pub generated_image_paths: &'a mut Vec<GeneratedImage>,
}

/// Run the bounded native-tool loop, including agent-mode `submit_final_response`
/// extraction, the iteration-0 ghost-wrap guardrail, and the max-turns force-final
/// branch (v4 `runNativeToolLoop`). Returns `Err(StreamError)` on an upstream
/// stream failure (after preserving the partial), matching v4's rethrow.
#[allow(clippy::too_many_lines)]
pub async fn run_native_tool_loop<P, S, R, D>(
    db: &Db,
    provider: &P,
    sink: &S,
    runner: &R,
    detector: &D,
    preserve: &mut PreservePartialOnError,
    opts: RunNativeToolLoopOptions<'_>,
) -> Result<(), StreamError>
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: ToolRunner,
    D: ToolCallDetector,
{
    let RunNativeToolLoopOptions {
        chat_id,
        character_id,
        character_name,
        agent_mode,
        provider: provider_name,
        base_url,
        formatted_messages,
        base_params,
        tool_context,
        state,
        tool_messages,
        generated_image_paths,
    } = opts;

    let status_context = StatusContext {
        character_name: character_name.clone(),
        character_id: character_id.clone(),
    };

    // Initial state: the primary stream's slate + response is what the loop's first
    // iteration sees (v4 :123–126).
    let mut current_messages: Vec<ThreadedMessage> = formatted_messages;
    let mut current_response: String = state.full_response.clone();
    let mut current_raw_response: Option<Value> = state.raw_response.clone();

    let effective_max_turns = if agent_mode.enabled {
        agent_mode.max_turns
    } else {
        5
    };
    let mut tool_iterations: i64 = 0;
    // Tracks iterations that ran a non-`submit_final_response` tool (v4
    // `realWorkIterations`): distinguishes "wrapped a turn with real work"
    // (replace prose with args.response) from "spuriously wrapped a conversational
    // turn" (preserve prose).
    let mut real_work_iterations: i64 = 0;
    let mut agent_mode_completed = false;

    while current_raw_response.is_some() && tool_iterations < effective_max_turns {
        let raw = current_raw_response.as_ref().unwrap();
        let tool_calls = detector.detect(raw, &provider_name);
        if tool_calls.is_empty() {
            break;
        }

        // The prose offset where the model paused to call this batch (v4 :149):
        // UTF-16 length of everything streamed so far.
        let batch_anchor = crate::jsstr::utf16_len(&state.full_response) as f64;

        // Truncation guard: a response that hit the output-token ceiling emitted a
        // cut-off call — reject the whole batch (v4 :156).
        let truncated = is_truncated_finish_reason(extract_finish_reason(raw).as_deref());

        let submit_final_index = if agent_mode.enabled {
            tool_calls
                .iter()
                .position(|tc| tc.name == "submit_final_response")
        } else {
            None
        };
        let has_submit = submit_final_index.is_some();
        let non_submit_tool_calls: Vec<ToolCall> = if has_submit {
            tool_calls
                .iter()
                .filter(|tc| tc.name != "submit_final_response")
                .cloned()
                .collect()
        } else {
            tool_calls.clone()
        };

        // Ghost-wrap guardrail (v4 :168–172): iteration-0, sole submit_final_response
        // with no accompanying prose.
        let is_ghost_wrap = has_submit
            && tool_iterations == 0
            && tool_calls.len() == 1
            && crate::jsstr::js_trim(&current_response).is_empty();

        if has_submit && !is_ghost_wrap && !truncated {
            // Process sibling tool calls FIRST so they don't get dropped (v4 :179).
            if !non_submit_tool_calls.is_empty() {
                let sibling_results = process_tool_calls(
                    &non_submit_tool_calls,
                    &tool_context,
                    sink,
                    runner,
                    Some(&status_context),
                )
                .await;
                flush_reasoning_segment(state);
                let sibling_seq = state.next_turn_seq() as f64;
                let mut siblings = sibling_results.tool_messages;
                for tm in &mut siblings {
                    tm.anchor_offset = Some(batch_anchor);
                    tm.seq = Some(sibling_seq);
                }
                tool_messages.extend(siblings);
                generated_image_paths.extend(sibling_results.generated_image_paths);
                real_work_iterations += 1;
            }

            let submit_call = &tool_calls[submit_final_index.unwrap()];
            let args_response = submit_call
                .arguments
                .get("response")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(String::from);
            agent_mode_completed = true;

            sink.emit(ChatEvent::status(StatusPayload {
                stage: "agent_completed".into(),
                message: "Agent completed task".into(),
                tool_name: None,
                character_name: Some(character_name.clone()),
                character_id: Some(character_id.clone()),
            }));

            if real_work_iterations == 0 {
                // No real tool work — preserve the streamed prose (v4 :208).
                // (v4 logs; logging is host-side.)
            } else {
                // v4: `args.response || currentResponse`.
                let agent_final_response =
                    args_response.unwrap_or_else(|| current_response.clone());
                if agent_final_response != current_response {
                    // The structured answer replaces the prose wholesale — every
                    // captured offset now points into text that no longer exists
                    // (v4 :233–239). Drop tool anchors + the positioned reasoning.
                    for tm in tool_messages.iter_mut() {
                        tm.anchor_offset = None;
                    }
                    state.reasoning_segments.clear();
                }
                state.full_response = agent_final_response;
            }
            break;
        }

        tool_iterations += 1;

        if agent_mode.enabled {
            let tool_names: Vec<String> = tool_calls.iter().map(|tc| tc.name.clone()).collect();
            let iteration_summary =
                generate_iteration_summary(tool_iterations, &tool_names, Some(&current_response));
            sink.emit(ChatEvent::status(StatusPayload {
                stage: "agent_iteration".into(),
                message: iteration_summary,
                tool_name: None,
                character_name: Some(character_name.clone()),
                character_id: Some(character_id.clone()),
            }));

            let update_chat_id = chat_id.clone();
            let turn_count = tool_iterations as f64;
            db.write(move |w| {
                w.main().chats().update(
                    &update_chat_id,
                    &chats::ChatUpdate {
                        agent_turn_count: Some(turn_count),
                        ..Default::default()
                    },
                )?;
                Ok(())
            })
            .await
            .map_err(db_to_stream)?;
        }

        let results: ToolProcessingResult = if truncated {
            // Don't execute the half-parsed call(s); return a recoverable failure
            // per call (v4 :262–282).
            ToolProcessingResult {
                tool_messages: tool_calls
                    .iter()
                    .map(|tc| ToolMessage {
                        tool_name: tc.name.clone(),
                        success: false,
                        content: build_tool_truncation_message(&tc.name),
                        arguments: Some(tc.arguments.clone()),
                        call_id: tc.call_id.clone(),
                        ..Default::default()
                    })
                    .collect(),
                generated_image_paths: Vec::new(),
            }
        } else if is_ghost_wrap {
            // v4 :283–297 — the ghost-wrap rejection result.
            let submit_call = &tool_calls[submit_final_index.unwrap()];
            ToolProcessingResult {
                tool_messages: vec![ToolMessage {
                    tool_name: "submit_final_response".into(),
                    success: false,
                    content: GHOST_WRAP_REJECTION_MESSAGE.to_string(),
                    arguments: Some(submit_call.arguments.clone()),
                    call_id: submit_call.call_id.clone(),
                    ..Default::default()
                }],
                generated_image_paths: Vec::new(),
            }
        } else {
            let r = process_tool_calls(
                &tool_calls,
                &tool_context,
                sink,
                runner,
                Some(&status_context),
            )
            .await;
            // Any non-ghost-wrap iteration here processed real tool calls (v4 :304).
            real_work_iterations += 1;
            r
        };

        flush_reasoning_segment(state);
        let batch_seq = state.next_turn_seq() as f64;
        let mut batch_messages = results.tool_messages;
        for tm in &mut batch_messages {
            tm.anchor_offset = Some(batch_anchor);
            tm.seq = Some(batch_seq);
        }
        tool_messages.extend(batch_messages.iter().cloned());
        generated_image_paths.extend(results.generated_image_paths);

        // Reconstruct the assistant turn + tool results (v4 :317–324).
        let detected: Vec<DetectedToolCall> = tool_calls.iter().map(to_detected).collect();
        let assistant_turn = build_assistant_tool_call_message(
            &detected,
            &current_response,
            state.reasoning_content.as_deref(),
            state.thought_signature.as_deref(),
        );
        current_messages.push(assistant_turn);
        current_messages.extend(build_tool_result_messages(&batch_messages));

        // Reset the user-visible stage before the follow-up stream (v4 :328–333).
        sink.emit(ChatEvent::status(StatusPayload {
            stage: "sending".into(),
            message: format!("Sending to {character_name}..."),
            tool_name: None,
            character_name: Some(character_name.clone()),
            character_id: Some(character_id.clone()),
        }));

        current_response.clear();
        current_raw_response = None;

        // Re-stream (v4 :338–384).
        let mut params = base_params.clone();
        params.messages = to_completion_messages(&current_messages);
        let mut emitted_streaming_status = false;
        let mut rx = provider
            .stream_message(&provider_name, base_url.as_deref(), &params)
            .await;
        loop {
            let Some(item) = rx.recv().await else { break };
            let chunk = match item {
                Ok(c) => c,
                Err(e) => {
                    preserve.preserve(db, state, &e.message).await;
                    return Err(e);
                }
            };
            apply_reasoning_chunk(state, &chunk, sink);
            if !chunk.content.is_empty() {
                if !emitted_streaming_status {
                    emitted_streaming_status = true;
                    sink.emit(ChatEvent::status(StatusPayload {
                        stage: "streaming".into(),
                        message: format!("{character_name} is responding..."),
                        tool_name: None,
                        character_name: Some(character_name.clone()),
                        character_id: Some(character_id.clone()),
                    }));
                }
                flush_reasoning_segment(state);
                current_response.push_str(&chunk.content);
                state.full_response.push_str(&chunk.content);
                sink.emit(ChatEvent::content(chunk.content.clone()));
            }
            if chunk.done {
                state.usage = chunk.usage;
                state.cache_usage = chunk.cache_usage;
                state.attachment_results = attachment_results_to_value(&chunk.attachment_results);
                current_raw_response = chunk.raw_response.clone();
                state.raw_response = chunk.raw_response.clone();
                if let Some(ts) = chunk.thought_signature.as_ref().filter(|s| !s.is_empty()) {
                    state.thought_signature = Some(ts.clone());
                }
                flush_reasoning_segment(state);
            }
        }

        // Silent tool use (raw response but no streamed text) → an explicit status
        // (v4 :388–395).
        if !emitted_streaming_status && current_raw_response.is_some() {
            sink.emit(ChatEvent::status(StatusPayload {
                stage: "processing_tools".into(),
                message: format!("{character_name} is using tools..."),
                tool_name: None,
                character_name: Some(character_name.clone()),
                character_id: Some(character_id.clone()),
            }));
        }
    }

    // Force-final branch (v4 :398–476). Non-agent mode just logs the cap (v4's
    // `else` — host-side logging, omitted), so the two conditions collapse.
    if tool_iterations >= effective_max_turns && !agent_mode_completed && agent_mode.enabled {
        {
            sink.emit(ChatEvent::status(StatusPayload {
                stage: "agent_force_final".into(),
                message: "Requesting final response...".into(),
                tool_name: None,
                character_name: Some(character_name.clone()),
                character_id: Some(character_id.clone()),
            }));

            let force_final_message = build_force_final_message();
            current_messages.push(ThreadedMessage {
                role: "assistant".to_string(),
                content: current_response.clone(),
                name: None,
                thought_signature: state.thought_signature.clone(),
                reasoning_content: state.reasoning_content.clone(),
                tool_call_id: None,
                tool_calls: None,
            });
            current_messages.push(ThreadedMessage {
                role: "user".to_string(),
                content: force_final_message,
                name: None,
                thought_signature: None,
                reasoning_content: None,
                tool_call_id: None,
                tool_calls: None,
            });

            let mut params = base_params.clone();
            params.messages = to_completion_messages(&current_messages);
            let mut rx = provider
                .stream_message(&provider_name, base_url.as_deref(), &params)
                .await;
            loop {
                let Some(item) = rx.recv().await else { break };
                let chunk = match item {
                    Ok(c) => c,
                    Err(e) => {
                        preserve.preserve(db, state, &e.message).await;
                        return Err(e);
                    }
                };
                apply_reasoning_chunk(state, &chunk, sink);
                if !chunk.content.is_empty() {
                    flush_reasoning_segment(state);
                    state.full_response.push_str(&chunk.content);
                    sink.emit(ChatEvent::content(chunk.content.clone()));
                }
                if chunk.done {
                    state.usage = chunk.usage;
                    state.cache_usage = chunk.cache_usage;
                    state.attachment_results =
                        attachment_results_to_value(&chunk.attachment_results);
                    state.raw_response = chunk.raw_response.clone();
                    if let Some(ts) = chunk.thought_signature.as_ref().filter(|s| !s.is_empty()) {
                        state.thought_signature = Some(ts.clone());
                    }
                    flush_reasoning_segment(state);

                    // If the force-final response contains submit_final_response,
                    // promote its `response` over what streamed (v4 :451–466).
                    if let Some(raw) = chunk.raw_response.as_ref() {
                        let final_calls = detector.detect(raw, &provider_name);
                        if let Some(submit) = final_calls
                            .iter()
                            .find(|tc| tc.name == "submit_final_response")
                        {
                            if let Some(resp) = submit
                                .arguments
                                .get("response")
                                .and_then(Value::as_str)
                                .filter(|s| !s.is_empty())
                            {
                                for tm in tool_messages.iter_mut() {
                                    tm.anchor_offset = None;
                                }
                                state.reasoning_segments.clear();
                                state.full_response = resp.to_string();
                            }
                        }
                    }
                }
            }
        }
        // Non-agent mode just logs the cap (host-side).
    }

    Ok(())
}

/// Convert a [`ToolCall`] (tool-execution shape) to a [`DetectedToolCall`]
/// (threading shape) — field-identical; the two types mirror v4's single
/// `{ name, arguments, callId }` detection shape consumed by both subsystems.
fn to_detected(tc: &ToolCall) -> DetectedToolCall {
    DetectedToolCall {
        name: tc.name.clone(),
        arguments: tc.arguments.clone(),
        call_id: tc.call_id.clone(),
    }
}

/// Convert the threaded message slate to the stream's `[{role, content}]` view —
/// the re-stream keys on role+content (the threading metadata is provider wire
/// state the canned key ignores, matching v4's `streamMessage` seam).
fn to_completion_messages(messages: &[ThreadedMessage]) -> Vec<CompletionMessage> {
    messages
        .iter()
        .map(|m| CompletionMessage {
            role: match m.role.as_str() {
                "system" => CompletionRole::System,
                "assistant" => CompletionRole::Assistant,
                "tool" => CompletionRole::Tool,
                _ => CompletionRole::User,
            },
            content: m.content.clone(),
        })
        .collect()
}

/// The `AttachmentResults` → JSON shape the streaming state carries (mirrors
/// [`super::primary_stream`]'s private converter).
fn attachment_results_to_value(
    r: &Option<crate::model::stream::StreamAttachmentResults>,
) -> Option<Value> {
    r.as_ref().map(|res| {
        let failed: Vec<Value> = res
            .failed
            .iter()
            .map(|f| serde_json::json!({ "id": f.id, "error": f.error }))
            .collect();
        serde_json::json!({ "sent": res.sent, "failed": failed })
    })
}

/// Map a writer-path [`DbError`] into the loop's [`StreamError`] channel (v4 would
/// throw; the orchestrator wraps the thrown error uniformly).
fn db_to_stream(e: DbError) -> StreamError {
    StreamError::new(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stream::{CannedStreamingProvider, StreamChunk, StreamUsage};
    use crate::services::chat_events::RecordingSink;
    use crate::services::tool_execution::{create_tool_context, CannedToolRunner, ToolResult};
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::{tempdir, TempDir};

    /// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A fresh encrypted `Db` for the self-tests (agent mode is off, so the
    /// writer channel is never exercised — the handle just has to be valid).
    fn dummy_db() -> (TempDir, Db) {
        let dir = tempdir().expect("tempdir");
        let db = Db::open_main(dir.path().join("loop.db"), PEPPER).expect("open db");
        (dir, db)
    }

    /// A detector keyed by the exact raw-response JSON (the differential's marker
    /// discipline, reused for self-tests).
    struct MapDetector {
        by_raw: HashMap<String, Vec<ToolCall>>,
    }
    impl ToolCallDetector for MapDetector {
        fn detect(&self, raw_response: &Value, _provider: &str) -> Vec<ToolCall> {
            self.by_raw
                .get(&raw_response.to_string())
                .cloned()
                .unwrap_or_default()
        }
    }

    fn base_params(model: &str) -> StreamParams {
        StreamParams {
            messages: Vec::new(),
            model: model.to_string(),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: None,
            tools: Some(json!([{ "function": { "name": "noop" } }])),
            web_search_enabled: false,
            profile_parameters: None,
            cache_key: None,
            previous_response_id: None,
            stop: Vec::new(),
        }
    }

    fn preserver() -> PreservePartialOnError {
        PreservePartialOnError::new("chat-1", "char-1", "Friday", vec![], "pp-1", None, "pre-1")
    }

    #[tokio::test]
    async fn empty_detection_is_a_no_op() {
        // No raw response → the while-guard is false, loop never runs.
        let (_dir, db) = dummy_db();
        let provider = CannedStreamingProvider::new();
        let sink = RecordingSink::new();
        let runner = CannedToolRunner::new();
        let detector = NoToolCallDetector;
        let mut preserve = preserver();
        let mut state = StreamingState {
            full_response: "hello".into(),
            raw_response: None,
            ..Default::default()
        };
        let mut tool_messages = Vec::new();
        let mut images = Vec::new();
        run_native_tool_loop(
            &db,
            &provider,
            &sink,
            &runner,
            &detector,
            &mut preserve,
            RunNativeToolLoopOptions {
                chat_id: "chat-1".into(),
                character_id: "char-1".into(),
                character_name: "Friday".into(),
                agent_mode: ResolvedAgentMode {
                    enabled: false,
                    max_turns: 10,
                },
                provider: "ANTHROPIC".into(),
                base_url: None,
                formatted_messages: vec![ThreadedMessage {
                    role: "user".into(),
                    content: "hi".into(),
                    name: None,
                    thought_signature: None,
                    reasoning_content: None,
                    tool_call_id: None,
                    tool_calls: None,
                }],
                base_params: base_params("m"),
                tool_context: create_tool_context(
                    "chat-1", "user-1", "char-1", "pp-1", None, None, None, None, None,
                ),
                state: &mut state,
                tool_messages: &mut tool_messages,
                generated_image_paths: &mut images,
            },
        )
        .await
        .expect("loop ok");
        assert_eq!(state.full_response, "hello");
        assert!(tool_messages.is_empty());
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn single_iteration_then_continuation() {
        let (_dir, db) = dummy_db();
        // Iteration 0 raw = marker0 → one `state` call (callId c1). After execution
        // the re-stream returns prose + a done with raw=marker1 (no calls → break).
        let marker0 = json!({ "marker": "m0" });
        let marker1 = json!({ "marker": "m1" });
        let call = ToolCall {
            name: "state".into(),
            arguments: json!({ "key": "mood" }),
            call_id: Some("c1".into()),
        };
        let mut by_raw = HashMap::new();
        by_raw.insert(marker0.to_string(), vec![call.clone()]);
        by_raw.insert(marker1.to_string(), vec![]);
        let detector = MapDetector { by_raw };

        let mut runner = CannedToolRunner::new();
        runner.register(
            &call,
            ToolResult {
                tool_name: "state".into(),
                success: true,
                result: json!({ "mood": "curious" }),
                error: None,
                message: None,
                metadata: None,
            },
        );

        // The continuation slate = [user, assistant(toolCalls), tool result].
        let cont_messages = vec![
            CompletionMessage::user("hi"),
            CompletionMessage {
                role: CompletionRole::Assistant,
                content: "before ".into(),
            },
            CompletionMessage {
                role: CompletionRole::Tool,
                content: "{\n  \"mood\": \"curious\"\n}".into(),
            },
        ];
        let provider = CannedStreamingProvider::new().with_stream(
            "ANTHROPIC",
            "m",
            Some(0.7),
            &cont_messages,
            vec![
                Ok(StreamChunk::content("after")),
                Ok(StreamChunk {
                    done: true,
                    usage: Some(StreamUsage {
                        prompt_tokens: 5,
                        completion_tokens: 1,
                        total_tokens: 6,
                    }),
                    raw_response: Some(marker1.clone()),
                    ..Default::default()
                }),
            ],
        );

        let sink = RecordingSink::new();
        let mut preserve = preserver();
        let mut state = StreamingState {
            full_response: "before ".into(),
            raw_response: Some(marker0.clone()),
            ..Default::default()
        };
        let mut tool_messages = Vec::new();
        let mut images = Vec::new();
        run_native_tool_loop(
            &db,
            &provider,
            &sink,
            &runner,
            &detector,
            &mut preserve,
            RunNativeToolLoopOptions {
                chat_id: "chat-1".into(),
                character_id: "char-1".into(),
                character_name: "Friday".into(),
                agent_mode: ResolvedAgentMode {
                    enabled: false,
                    max_turns: 10,
                },
                provider: "ANTHROPIC".into(),
                base_url: None,
                formatted_messages: vec![ThreadedMessage {
                    role: "user".into(),
                    content: "hi".into(),
                    name: None,
                    thought_signature: None,
                    reasoning_content: None,
                    tool_call_id: None,
                    tool_calls: None,
                }],
                base_params: base_params("m"),
                tool_context: create_tool_context(
                    "chat-1", "user-1", "char-1", "pp-1", None, None, None, None, None,
                ),
                state: &mut state,
                tool_messages: &mut tool_messages,
                generated_image_paths: &mut images,
            },
        )
        .await
        .expect("loop ok");

        // Prose accumulated; one tool message with the batch anchor (len "before ").
        assert_eq!(state.full_response, "before after");
        assert_eq!(tool_messages.len(), 1);
        assert_eq!(tool_messages[0].tool_name, "state");
        assert_eq!(tool_messages[0].anchor_offset, Some(7.0));
        assert_eq!(tool_messages[0].seq, Some(0.0));
        assert_eq!(state.usage.unwrap().total_tokens, 6);
    }
}
