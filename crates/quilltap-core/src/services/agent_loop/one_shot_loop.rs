//! One-shot tool loop — v4 `lib/services/agent-loop/one-shot-loop.ts`
//! (`runOneShotToolLoop`, `d1c06cd9d`): a non-persisting agent loop shared by
//! every surface that needs "one request → tools → one answer" with nothing
//! written to a chat.
//!
//! Callers (v4's module doc):
//!  - [`run_brahma_query`](crate::services::brahma_console::run_brahma_query) —
//!    Brahma consulted as a Carina answerer; sink controller, operator surface.
//!  - the Scenario Builder — The Host setting a scene; live controller,
//!    pre-built mount pool.
//!
//! The caller owns everything surface-specific: the profile and key, the tool
//! slate ([`build_tools`](crate::services::tool_build::build_tools)), the system
//! prompt and the tool context. This module owns the loop itself: tool-call
//! detection (native or text-block), threading, the `submit_final_response`
//! completion, the stuck-loop guard, the forced final turn, and the
//! budget-exhaustion salvage (bug 47).
//!
//! Nothing is persisted. Tool side effects stand; per-iteration assistant/TOOL
//! messages live only in memory. The only durable trace is the LLM log rows
//! the stream call writes, typed by [`RunOneShotToolLoopOptions::log_type`].
//!
//! The streaming Brahma orchestrator (`brahma_console::orchestrator`) is
//! deliberately NOT built on this: it persists every iteration and emits
//! done/error events (v4's own note).
//!
//! ## What the port adds to v4's shape
//!
//! - **The model boundaries are injected** ([`OneShotLoopDeps`] — the streaming
//!   provider, the tool runner, the tool-call detector), the way the Brahma
//!   console already took them. v4's `repos` is [`OneShotLoopDeps::db`]; v4's
//!   `apiKey` has no v5 counterpart here (the host streaming provider resolves
//!   keys itself — the caller still RESOLVES the key first, for v4's refusals).
//! - **"Never throws for model-side failures"** (v4's doc) is
//!   [`OneShotLoopResult`]; a THROW out of v4's loop (a mid-stream provider
//!   error propagating out of `for await`) is this function's `Err`
//!   ([`OneShotLoopError`]) — the Brahma caller folds it into its own
//!   `{ ok: false, detail }` as before, and the Scenario Builder turns it into
//!   its "detained" frame, exactly as v4's callers' `catch` blocks do.
//! - **The abort seam** (v4's `AbortSignal`) is an [`AtomicBool`]
//!   ([`RunOneShotToolLoopOptions::signal`]), checked BETWEEN turns and PER
//!   CHUNK mid-stream — v4 breaks its `for await` on the next chunk, so per
//!   chunk is v4's granularity. A stalled provider is the watchdog's business,
//!   not the abort's.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use crate::db::runtime::Db;
use crate::jsstr::js_trim;
use crate::message_formatter::normalize_content_block_format;
use crate::model::stream::{StreamParams, StreamUsage, StreamingCompletionProvider};
use crate::model::stream_watchdog::{watch_stream, StallBudgets, StallWatchdogContext};
use crate::services::agent_mode::{
    build_agent_mode_instructions, build_force_final_message,
    extract_submit_final_response_from_text,
};
use crate::services::brahma_console::orchestrator::normalize_tool_call_signature;
use crate::services::chat_events::{ChatEvent, EventSink};
use crate::services::llm_logging::log_type;
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::pseudo_tool::{
    build_native_tool_system_instructions, build_text_block_system_instructions,
    check_should_use_text_block_tools, parse_text_blocks_from_response,
    strip_text_block_markers_from_response, TextBlockEnabledToolOptions,
};
use crate::services::tool_call_threading::{
    build_assistant_tool_call_message, build_tool_result_messages, to_stream_messages,
    DetectedToolCall, ThreadedMessage,
};
use crate::services::tool_execution::{
    process_tool_calls, StatusContext, ToolCall, ToolExecutionContext, ToolRunner,
};
use crate::tools::pseudo_tool_support::ToolMode;

/// The `buildTools` result the loop consumes (v4 `BuiltTools { tools,
/// modelSupportsNativeTools }` — structurally the slate builder's own return,
/// so the port re-exports it rather than defining a second struct).
pub use crate::services::tool_build::BuiltTools;

/// Consecutive duplicate / stale tool iterations before forcing a final answer
/// (v4 `MAX_DUPLICATE_TOOL_CALLS`, exported by the loop module since
/// `d1c06cd9d`).
pub const MAX_DUPLICATE_TOOL_CALLS: usize = 2;

/// v4 `logLabel`'s default (`= 'One-shot loop'`).
pub const DEFAULT_LOG_LABEL: &str = "One-shot loop";

/// v4 `OneShotUsage` — the per-turn usage summed over the whole loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OneShotUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// v4 `OneShotLoopResult` — `{ ok: true; answer; toolsExecuted; usage } |
/// { ok: false; detail }`. v4's two `detail` strings are `'aborted'` and
/// `'empty response'`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OneShotLoopResult {
    Ok {
        answer: String,
        tools_executed: usize,
        usage: OneShotUsage,
    },
    Failed {
        detail: String,
    },
}

/// A THROW out of v4's loop — a mid-stream provider error propagates out of
/// `for await` (v4's loop has no try/catch). `message` is v4's
/// `error.message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OneShotLoopError {
    pub message: String,
}

/// The model boundaries the loop composes (the Brahma console's precedent —
/// generic-consumed, no boxing, the future stays `Send`).
pub struct OneShotLoopDeps<'a, STR, TR, TD>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
{
    /// v4 `repos` — also where the stream call's `llm_logs` row lands.
    pub db: &'a Db,
    /// The streaming model boundary (v4 `streamMessage`).
    pub streaming: &'a STR,
    /// The tool executor boundary (v4 `processToolCalls` →
    /// `executeToolCallWithContext` + every handler).
    pub tool_runner: &'a TR,
    /// v4 `detectToolCallsInResponse`.
    pub tool_detector: &'a TD,
}

/// A swallowing [`EventSink`] — v4's `opts.controller ?? { enqueue: () => {} }`
/// (a caller with no controller surfaces nothing live).
pub struct NoopSink;
impl EventSink for NoopSink {
    fn emit(&self, _event: ChatEvent) {}
}

/// v4 `RunOneShotToolLoopOptions`.
pub struct RunOneShotToolLoopOptions<'a, S: EventSink> {
    pub user_id: &'a str,
    /// Tool scope and log attribution; may be synthetic (no chat row behind it).
    pub chat_id: &'a str,
    /// The connection-profile row (`provider`, `modelName`, `baseUrl`, `id`,
    /// `name`, `pseudoToolMode` are read).
    pub connection_profile: &'a Value,
    pub system_prompt: &'a str,
    pub user_message: &'a str,
    pub tools: &'a BuiltTools,
    pub tool_context: &'a ToolExecutionContext,
    pub max_agent_turns: i64,
    /// Where tool events stream. v4's is optional (omitted → a sink); pass
    /// [`NoopSink`] for that.
    pub controller: &'a S,
    /// Checked between turns and while streaming; an abort ends the loop with
    /// `{ ok: false, detail: 'aborted' }`.
    pub signal: Option<&'a AtomicBool>,
    /// LLM log row type. `None` → `CHAT_MESSAGE` (v4's default).
    pub log_type: Option<&'static str>,
    /// Status-event attribution for `process_tool_calls`.
    pub status_context: Option<StatusContext>,
    /// Cumulative reasoning text for the current turn (replace, don't append).
    pub on_reasoning: Option<&'a mut (dyn FnMut(&str) + Send)>,
    /// A short label for log lines (e.g. `Brahma one-shot`). `None` →
    /// [`DEFAULT_LOG_LABEL`].
    pub log_label: Option<&'a str>,
}

// ===========================================================================
// Small JSON accessors.
// ===========================================================================

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn is_aborted(signal: Option<&AtomicBool>) -> bool {
    signal.is_some_and(|flag| flag.load(Ordering::SeqCst))
}

/// A role+content-only [`ThreadedMessage`] (no tool-call / reasoning fields).
pub fn plain_message(role: &str, content: &str) -> ThreadedMessage {
    ThreadedMessage {
        role: role.to_string(),
        content: content.to_string(),
        name: None,
        thought_signature: None,
        reasoning_content: None,
        tool_call_id: None,
        tool_calls: None,
        cache_control: None,
        attachments: None,
    }
}

/// v4 `resolveOneShotUsesTextBlockTools(profile, built)` — whether this
/// profile and slate run text-block (pseudo) tools rather than native ones.
/// `simple-json` downgrades to text-block: the one-shot loop does not implement
/// the simple-json continuation.
pub fn resolve_one_shot_uses_text_block_tools(
    connection_profile: &Value,
    built: &BuiltTools,
) -> bool {
    let effective_pseudo_tool_mode: Option<ToolMode> =
        match s(connection_profile, "pseudoToolMode").as_deref() {
            Some("simple-json") => Some(ToolMode::TextBlock),
            Some(other) => ToolMode::from_str(other),
            None => Some(ToolMode::Auto), // v4 `?? 'auto'`.
        };
    check_should_use_text_block_tools(
        built.model_supports_native_tools,
        effective_pseudo_tool_mode,
    )
}

/// v4 `buildOneShotToolInstructions` — the tool-instruction block for a
/// one-shot system prompt: native or text-block instructions (when any tools
/// were built), then the agent-mode instructions for the turn budget.
pub fn build_one_shot_tool_instructions(
    connection_profile: &Value,
    built: &BuiltTools,
    text_block_options: &TextBlockEnabledToolOptions,
    max_agent_turns: i64,
) -> String {
    let use_text_block_tools = resolve_one_shot_uses_text_block_tools(connection_profile, built);

    let mut tool_instructions = String::new();
    if use_text_block_tools && !built.tools.is_empty() {
        tool_instructions = build_text_block_system_instructions(text_block_options);
    } else if !built.tools.is_empty() {
        tool_instructions = build_native_tool_system_instructions();
    }

    let agent_instructions = build_agent_mode_instructions(max_agent_turns);
    if tool_instructions.is_empty() {
        agent_instructions
    } else {
        format!("{tool_instructions}\n\n{agent_instructions}")
    }
}

// ===========================================================================
// One streamed call (v4 `streamMessage` as the loop consumes it).
// ===========================================================================

/// What one loop turn's `llm_logs` row needs (v4's `streamMessage({...,
/// userId, chatId, logType})` — no `characterId`, no `messageId`: a one-shot
/// caller has neither). v4 logs EVERY `streamMessage` call at `chunk.done`
/// (`streaming.service.ts:470`); the one-shot engine once bypassed
/// `primary_stream`'s logger entirely, so a real Brahma query left no row at
/// all — dogfood finding #111's other half (2026-09-06).
struct OneShotStreamLog<'a> {
    db: &'a Db,
    user_id: &'a str,
    chat_id: &'a str,
    log_type: &'static str,
    profile: crate::services::primary_stream::EffectiveProfile,
}

/// The pieces one streamed LLM call yields to the loop.
struct RunStreamResult {
    answer: String,
    raw_response: Option<Value>,
    /// The turn's cumulative reasoning (replace-on-change).
    reasoning: String,
    thought_signature: Option<String>,
    /// The turn's usage — last-wins ("Providers may repeat usage across
    /// chunks; the last one is the turn's").
    usage: Option<StreamUsage>,
    /// A provider error mid-stream — v4's `for await` propagates it out of
    /// the loop (a THROW).
    error: Option<String>,
}

/// One streamed LLM call. The slate crosses the stream boundary losslessly (v4
/// passes `ThreadedMessage[]` straight to `streamMessage`; P4.13 unit 2 made the
/// v5 boundary carry the tool-call linkage instead of flattening it away).
///
/// v4 has TWO layers here and so does this function: `streamMessage`'s own
/// generator (which accumulates the content/usage it LOGS, and writes the
/// `llm_logs` row when the terminal chunk passes through it — BEFORE yielding
/// it), and the loop's `for await` body (which checks the abort FIRST and only
/// then folds the chunk into the turn). So a chunk that arrives after an abort
/// was still produced — and, if it is the terminal chunk, still logged — but
/// never reaches the turn.
#[allow(clippy::too_many_arguments)]
async fn run_stream<STR: StreamingCompletionProvider>(
    streaming: &STR,
    provider: &str,
    base_url: Option<&str>,
    model: &str,
    messages: &[ThreadedMessage],
    tools: &[Value],
    log: Option<&OneShotStreamLog<'_>>,
    watchdog_user_id: &str,
    watchdog_chat_id: &str,
    signal: Option<&AtomicBool>,
    on_reasoning: &mut Option<&mut (dyn FnMut(&str) + Send)>,
) -> RunStreamResult {
    // v4 `streaming.service.ts:382` — `const startTime = Date.now()`, captured
    // immediately before the provider loop.
    let started_at_ms = crate::clock::now_unix_ms();
    // v4: `tools.length > 0 ? tools : undefined`.
    let tools_value = if tools.is_empty() {
        None
    } else {
        Some(Value::Array(tools.to_vec()))
    };
    let params = StreamParams {
        messages: to_stream_messages(messages),
        model: model.to_string(),
        // v4 passes `modelParams: {}` — NO temperature (never the profile's).
        temperature: None,
        max_tokens: None,
        top_p: None,
        tools: tools_value,
        // v4 hardcodes `useNativeWebSearch: false` in the one-shot stream.
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
        // v4 sets no `requestTimeoutMs` on any streaming call (P4.D83).
        request_timeout_ms: None,
    };
    // A provider that answers with headers and then goes silent would otherwise
    // hold this loop open forever — the SDK's own timeout stops at the headers.
    // The watchdog turns that into an ordinary `Err` (v4 `f90144ac4`, bug 141;
    // v4 wraps its ONE `streamMessage` funnel, v5 wraps each of its own
    // consumers). v4's one-shot calls pass `userId` + `chatId` only — neither a
    // `messageId` nor a `characterId` to carry.
    let mut rx = watch_stream(
        streaming.stream_message(provider, base_url, &params).await,
        StallBudgets::default(),
        StallWatchdogContext::streaming_service(provider, model).with_ids(
            Some(watchdog_user_id),
            Some(watchdog_chat_id),
            None,
            None,
        ),
    );
    // The loop's turn state (v4's `for await` body).
    let mut answer = String::new();
    let mut raw: Option<Value> = None;
    let mut reasoning = String::new();
    let mut thought_signature: Option<String> = None;
    let mut turn_usage: Option<StreamUsage> = None;
    let mut error: Option<String> = None;
    // `streamMessage`'s own generator state (what the `llm_logs` row records).
    let mut log_content = String::new();
    let mut log_raw: Option<Value> = None;
    let mut log_usage: Option<StreamUsage> = None;
    let mut log_cache_usage: Option<crate::model::stream::StreamCacheUsage> = None;
    let mut log_raw_provider_usage: Option<Value> = None;
    let mut saw_done = false;
    let mut aborted = false;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            Ok(c) => {
                // v4 `streaming.service.ts:449-456`: `streamMessage` rewrites each
                // chunk's content through `normalizeContentBlockFormat` BEFORE it
                // accumulates the log text and yields the chunk — so both halves
                // below see the normalized text, PER CHUNK (a block split across
                // chunks matches in neither and stays raw) (P4.114).
                let content = normalize_content_block_format(&c.content);
                // The generator side — runs for every chunk it yields.
                log_content.push_str(&content);
                if let Some(u) = &c.usage {
                    log_usage = Some(*u);
                }
                if let Some(cu) = &c.cache_usage {
                    log_cache_usage = Some(*cu);
                }
                if let Some(rpu) = &c.raw_provider_usage {
                    log_raw_provider_usage = Some(rpu.clone());
                }
                if let Some(raw_r) = &c.raw_response {
                    log_raw = Some(raw_r.clone());
                }
                if c.done {
                    saw_done = true;
                }
                // The loop side. "Leaving the for-await closes the provider
                // stream where the SDK allows."
                if is_aborted(signal) {
                    aborted = true;
                    break;
                }
                // Reasoning is request-local continuation state for providers
                // that pair it with the tool-use turn (e.g. Anthropic); surfaced
                // only via the caller's callback, never re-fed as prose.
                if let Some(rc) = &c.reasoning_content {
                    if !rc.is_empty() && *rc != reasoning {
                        reasoning = rc.clone();
                        if let Some(cb) = on_reasoning.as_mut() {
                            cb(&reasoning);
                        }
                    }
                }
                answer.push_str(&content);
                if let Some(raw_r) = c.raw_response {
                    raw = Some(raw_r);
                }
                // v4 `one-shot-loop.ts:241` `if (chunk.thoughtSignature)` — JS
                // truthiness, so an EMPTY signature is absent and never
                // overwrites a real one from an earlier chunk (P4.114).
                if let Some(ts) = c.thought_signature.filter(|ts| !ts.is_empty()) {
                    thought_signature = Some(ts);
                }
                // Providers may repeat usage across chunks; the last one is the
                // turn's.
                if let Some(u) = c.usage {
                    turn_usage = Some(u);
                }
            }
            Err(e) => {
                error = Some(e.to_string());
                break;
            }
        }
    }
    // v4 logs the call as the terminal chunk passes through the generator (a
    // thrown stream logs nothing; a consumer that broke off BEFORE the terminal
    // chunk was produced never let the generator reach its log).
    if error.is_none() && (!aborted || saw_done) {
        if let Some(l) = log {
            let ctx = crate::services::primary_stream::StreamLogCtx {
                db: l.db,
                user_id: l.user_id,
                chat_id: l.chat_id,
                message_id: "",
                character_id: None,
                log_context: &crate::services::llm_logging::LogContext::none(),
                started_at_ms,
            };
            crate::services::primary_stream::log_stream_message_call(
                &ctx,
                l.log_type,
                &l.profile,
                &params,
                log_content,
                log_usage,
                log_cache_usage,
                log_raw_provider_usage,
                log_raw,
            )
            .await;
        }
    }
    RunStreamResult {
        answer,
        raw_response: raw,
        reasoning,
        thought_signature,
        usage: turn_usage,
        error,
    }
}

// ===========================================================================
// The loop.
// ===========================================================================

/// v4 `runOneShotToolLoop`. Never fails for model-side failures: returns
/// [`OneShotLoopResult::Failed`]; an abort returns `Failed { detail:
/// "aborted" }`. `Err` is v4's propagated THROW (a mid-stream provider error).
pub async fn run_one_shot_tool_loop<STR, TR, TD, S>(
    deps: &OneShotLoopDeps<'_, STR, TR, TD>,
    opts: RunOneShotToolLoopOptions<'_, S>,
) -> Result<OneShotLoopResult, OneShotLoopError>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    S: EventSink,
{
    let RunOneShotToolLoopOptions {
        user_id,
        chat_id,
        connection_profile,
        system_prompt,
        user_message,
        tools: built,
        tool_context,
        max_agent_turns,
        controller,
        signal,
        log_type,
        status_context,
        mut on_reasoning,
        log_label,
    } = opts;
    let log_label = log_label.unwrap_or(DEFAULT_LOG_LABEL);
    let row_log_type = log_type.unwrap_or(log_type::CHAT_MESSAGE);

    let provider = s(connection_profile, "provider").unwrap_or_default();
    let model = s(connection_profile, "modelName").unwrap_or_default();
    let base_url = s(connection_profile, "baseUrl");

    let use_text_block_tools = resolve_one_shot_uses_text_block_tools(connection_profile, built);
    let model_supports_native_tools = built.model_supports_native_tools;
    // v4: `(!useTextBlockTools && modelSupportsNativeTools) ? built.tools : []`.
    let effective_tools: Vec<Value> = if !use_text_block_tools && model_supports_native_tools {
        built.tools.clone()
    } else {
        Vec::new()
    };

    let mut conversation_messages: Vec<ThreadedMessage> = vec![
        plain_message("system", system_prompt),
        plain_message("user", user_message),
    ];

    // v4 gates the log on `if (userId)` (inside `streamMessage`).
    let stream_log = (!user_id.is_empty()).then(|| OneShotStreamLog {
        db: deps.db,
        user_id,
        chat_id,
        log_type: row_log_type,
        profile: crate::services::primary_stream::EffectiveProfile {
            id: s(connection_profile, "id").unwrap_or_default(),
            name: s(connection_profile, "name").unwrap_or_default(),
            provider: provider.clone(),
            model_name: model.clone(),
            base_url: base_url.clone(),
        },
    });

    let mut agent_turn_count: i64 = 0;
    let mut full_response = String::new();
    let mut tools_executed: usize = 0;
    let mut usage = OneShotUsage::default();
    let mut tool_call_history: Vec<String> = Vec::new();
    // Stuck-loop detection: an exact-signature repeat OR consecutive iterations
    // that surface nothing new force a final turn.
    let mut seen_result_fingerprints: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut stale_iterations: usize = 0;
    let mut last_tool_result_text = String::new();

    tracing::debug!(
        chatId = %chat_id,
        provider = %provider,
        model = %model,
        toolCount = built.tools.len(),
        useTextBlockTools = use_text_block_tools,
        maxAgentTurns = max_agent_turns,
        logType = %row_log_type,
        "{log_label}: starting"
    );

    while agent_turn_count <= max_agent_turns {
        if is_aborted(signal) {
            tracing::debug!(
                chatId = %chat_id,
                turn = agent_turn_count,
                "{log_label}: aborted between turns"
            );
            return Ok(OneShotLoopResult::Failed {
                detail: "aborted".to_string(),
            });
        }
        agent_turn_count += 1;

        if agent_turn_count == max_agent_turns {
            conversation_messages.push(plain_message("user", &build_force_final_message()));
        }

        let stream = run_stream(
            deps.streaming,
            &provider,
            base_url.as_deref(),
            &model,
            &conversation_messages,
            &effective_tools,
            stream_log.as_ref(),
            user_id,
            chat_id,
            signal,
            &mut on_reasoning,
        )
        .await;
        // v4's `for await` propagates a mid-stream throw straight out of the
        // loop — the caller's catch decides what it means.
        if let Some(e) = stream.error {
            return Err(OneShotLoopError { message: e });
        }

        if let Some(u) = stream.usage {
            usage.prompt_tokens += u.prompt_tokens;
            usage.completion_tokens += u.completion_tokens;
            usage.total_tokens += u.total_tokens;
        }

        if is_aborted(signal) {
            tracing::debug!(
                chatId = %chat_id,
                turn = agent_turn_count,
                "{log_label}: aborted mid-stream"
            );
            return Ok(OneShotLoopResult::Failed {
                detail: "aborted".to_string(),
            });
        }

        let mut current_response = stream.answer;
        let raw_response = stream.raw_response;
        let turn_reasoning = stream.reasoning;
        let turn_thought_signature = stream.thought_signature;

        // Detect tool calls (native or text-block).
        let mut has_tool_calls = false;
        let mut tool_calls_to_process: Option<Vec<ToolCall>> = None;

        if model_supports_native_tools && !use_text_block_tools {
            if let Some(raw) = &raw_response {
                let detected = deps.tool_detector.detect(raw, &provider);
                if !detected.is_empty() {
                    tool_calls_to_process = Some(detected);
                    has_tool_calls = true;
                }
            }
        } else if use_text_block_tools
            && crate::tools::text_block_parser::has_text_block_markers(&current_response)
        {
            let parsed = parse_text_blocks_from_response(&current_response);
            if !parsed.is_empty() {
                tool_calls_to_process = Some(
                    parsed
                        .into_iter()
                        .map(|p| ToolCall {
                            name: p.name,
                            arguments: p.arguments,
                            call_id: None,
                        })
                        .collect(),
                );
                has_tool_calls = true;
                current_response = strip_text_block_markers_from_response(&current_response);
            }
        }

        // submit_final_response (agent-mode completion).
        let mut is_submit_final = tool_calls_to_process
            .as_ref()
            .map(|calls| calls.iter().any(|tc| tc.name == "submit_final_response"))
            .unwrap_or(false);
        if is_submit_final {
            if let Some(calls) = &tool_calls_to_process {
                let submit = calls.iter().find(|tc| tc.name == "submit_final_response");
                let final_content = submit
                    .and_then(|c| c.arguments.get("response"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| current_response.clone());
                current_response = final_content;
                full_response = current_response.clone();
                has_tool_calls = false;
            }
        }

        // Fallback: submit_final_response emitted as raw JSON text.
        if !is_submit_final && !has_tool_calls {
            let extracted = extract_submit_final_response_from_text(&current_response);
            if extracted != current_response {
                is_submit_final = true;
                current_response = extracted.clone();
                full_response = extracted;
                has_tool_calls = false;
            }
        }

        if has_tool_calls && !is_submit_final && agent_turn_count < max_agent_turns {
            let calls = tool_calls_to_process.expect("has_tool_calls implies Some");
            let call_signature = normalize_tool_call_signature(&calls);
            let duplicate_count = tool_call_history
                .iter()
                .filter(|sig| **sig == call_signature)
                .count();
            tool_call_history.push(call_signature);

            let is_stuck = duplicate_count >= MAX_DUPLICATE_TOOL_CALLS
                || stale_iterations >= MAX_DUPLICATE_TOOL_CALLS;
            if is_stuck {
                tracing::warn!(
                    chatId = %chat_id,
                    turn = agent_turn_count,
                    duplicateCount = duplicate_count + 1,
                    staleIterations = stale_iterations,
                    "{log_label} stuck in tool-call loop, forcing final response"
                );
                let tool_data_reminder = if !last_tool_result_text.is_empty() {
                    format!(
                        "\n\nHere is the data you already received from your previous tool call:\n{last_tool_result_text}"
                    )
                } else {
                    String::new()
                };
                // Content-only assistant turn — we are NOT executing these calls,
                // so an attached tool-use block would be left unanswered for
                // strict providers.
                conversation_messages.push(plain_message("assistant", &current_response));
                conversation_messages.push(plain_message(
                    "user",
                    &format!(
                        "You have already gathered this data (a repeated call or repeated identical results). You already have what you need — do NOT call any more tools. Please call the submit_final_response tool NOW with your answer based on the data you already received.{tool_data_reminder}"
                    ),
                ));
                continue;
            }

            let tool_names: Vec<&str> = calls.iter().map(|tc| tc.name.as_str()).collect();
            // v4 logs `tools` as a real `string[]`; the `…Json` convention lands
            // it in the log file's context as that array (a `?` field would be
            // a Debug STRING there — the d1c06cd9d unification review).
            tracing::debug!(
                chatId = %chat_id,
                turn = agent_turn_count,
                toolsJson = %serde_json::to_string(&tool_names).unwrap_or_default(),
                "{log_label}: tool turn"
            );

            // Thread the assistant tool-call turn WITH its native tool_calls
            // (paired by callId on the next stream) so the model sees it already
            // issued them.
            let detected: Vec<DetectedToolCall> = calls
                .iter()
                .map(|tc| DetectedToolCall {
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    call_id: tc.call_id.clone(),
                })
                .collect();
            conversation_messages.push(build_assistant_tool_call_message(
                &detected,
                &current_response,
                if turn_reasoning.is_empty() {
                    None
                } else {
                    Some(turn_reasoning.as_str())
                },
                turn_thought_signature.as_deref(),
            ));

            let tool_result = process_tool_calls(
                &calls,
                tool_context,
                controller,
                deps.tool_runner,
                status_context.as_ref(),
            )
            .await;
            tools_executed += calls.len();

            if !tool_result.tool_messages.is_empty() {
                conversation_messages
                    .extend(build_tool_result_messages(&tool_result.tool_messages));

                let mut produced_new_info = false;
                for tm in &tool_result.tool_messages {
                    last_tool_result_text = tm.content.clone();
                    let fingerprint = format!("{}:{}:{}", tm.tool_name, tm.success, tm.content);
                    if seen_result_fingerprints.insert(fingerprint) {
                        produced_new_info = true;
                    }
                }
                stale_iterations = if produced_new_info {
                    0
                } else {
                    stale_iterations + 1
                };
            }

            continue;
        }

        // No tool calls or a final response — done. (A plain-text reply with no
        // tool call IS the answer.)
        full_response = current_response;
        break;
    }

    // Models that output submit_final_response as JSON text.
    full_response = extract_submit_final_response_from_text(&full_response);

    let mut final_answer = js_trim(&full_response).to_string();

    // Budget-exhaustion salvage (bug 47). The forced final turn runs no tools,
    // so a model that answers it with another native tool call instead of
    // `submit_final_response` leaves `full_response` empty. Rather than report a
    // bare failure after spending real budget, synthesise an explanatory answer
    // from the last tool result. With no tool data there is nothing to return.
    if final_answer.is_empty() && !last_tool_result_text.is_empty() {
        final_answer = format!(
            "I reached my {max_agent_turns}-turn budget before I could compose a final answer.\n\nHere is what I gathered before I stopped:\n\n{last_tool_result_text}"
        );
        tracing::warn!(
            chatId = %chat_id,
            maxAgentTurns = max_agent_turns,
            "{log_label} exhausted its turn budget without a final response"
        );
    }

    if final_answer.is_empty() {
        tracing::debug!(
            chatId = %chat_id,
            turns = agent_turn_count,
            "{log_label} produced an empty answer"
        );
        return Ok(OneShotLoopResult::Failed {
            detail: "empty response".to_string(),
        });
    }

    tracing::debug!(
        chatId = %chat_id,
        turns = agent_turn_count,
        toolsExecuted = tools_executed,
        // v4 `finalAnswer.length` — UTF-16 code units.
        answerLength = final_answer.encode_utf16().count(),
        "{log_label}: finished"
    );
    Ok(OneShotLoopResult::Ok {
        answer: final_answer,
        tools_executed,
        usage,
    })
}
