//! The text-tool loop (Phase-3 Unit-3 wave 4, W4.1f) — v4
//! `lib/services/chat-message/text-tool-loop.service.ts` (`runTextToolPass`).
//!
//! The strategy-driven **detect text-format tool calls in the streamed response →
//! execute → re-stream a continuation** pass the orchestrator runs after the
//! native loop. Where the native loop reads provider-native tool calls off the raw
//! response object (W4.7 wire parse), this pass reads text markers OUT OF the
//! streamed prose — the prompt-injected `[[TOOL ...]]content[[/TOOL]]` text-block
//! format, the `<tool_call>{...}</tool_call>` simple-json format, or a provider's
//! own spontaneous XML emissions (DeepSeek `<function_calls>`, Gemini `<tool_use>`).
//!
//! The pass is **strategy-agnostic**: the concrete detector / parser / stripper /
//! result-formatter live behind the [`TextToolStrategy`] trait, so the engine
//! ports once and the orchestrator hands it whichever strategy the resolved tool
//! mode calls for. Three strategies ship here:
//!
//!   - [`SimpleJsonStrategy`] — composed from the ported b.1 simple-json leaves.
//!   - [`TextBlockStrategy`] — the ported b.1 text-block leaves.
//!   - **provider-text-markers** — the detector/parser/stripper come from the
//!     provider plugin (unported W4.7 surface), so in v5 this is an **injected
//!     strategy seam**: the orchestrator runs the provider pass only when a
//!     host-provided strategy exists (default: none). The ENGINE is still fully
//!     exercised against it in the differential (which hands both sides the same
//!     synthetic strategy), because `run_text_tool_pass` takes the strategy as a
//!     parameter.
//!
//! ## The flow (verify each against source)
//!
//!   1. **Entry gate** — no-op unless `full_response` is non-empty AND
//!      `strategy.has_markers(full_response)`.
//!   2. Per iteration (cap [`MAX_TEXT_TOOL_ITERATIONS`]), on the LATEST raw
//!      segment: re-check markers, parse (empty → break), fingerprint the call
//!      set. `duplicate_count >= `[`MAX_DUPLICATE_TOOL_CALLS`] → the **nudge
//!      branch** (do NOT execute; push the assistant turn + a synthetic user nudge;
//!      one final continuation; break). Otherwise execute via [`process_tool_calls`]
//!      (the landed c.2 harness), append the assistant turn (markers **kept on
//!      purpose** — stripping them here broke simple-json continuations) + one
//!      synthetic `user` message per tool result via
//!      [`TextToolStrategy::format_tool_result`], and re-stream a continuation.
//!   3. **Final assembly** — [`assemble_stripped_with_offsets`]: strip each raw
//!      segment EXACTLY ONCE, keep segments whose stripped text has non-whitespace,
//!      join with `\n\n`, and stamp every batch's tool messages with the UTF-16
//!      prose offset where its emitting segment ends.
//!
//! The engine MUTATES caller-owned state in place: `state.full_response` becomes
//! `<stripped>\n\n<continuation>`, `state.usage` / `cache_usage` / `raw_response` /
//! `thought_signature` are OVERWRITTEN (not accumulated) by the continuation done
//! chunk, and `tool_messages` / `generated_image_paths` are appended. On stream
//! failure the partial continuation is preserved (via the injected
//! [`PreservePartialOnError`]) before the error re-throws — the same shape as the
//! native loop.
//!
//! ## Seams
//!
//! | v4 dependency | seam | corpus |
//! |---|---|---|
//! | `streamMessage` (the model stream) | [`StreamingCompletionProvider`] | canned streams (recorded keys) |
//! | `executeToolCallWithContext` (every handler, W4.1d) | [`ToolRunner`] | real handlers + canned fallback |
//! | the strategy detector/parser/stripper | [`TextToolStrategy`] | real b.1 leaves / synthetic (provider-markers) |

use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::model::completion::{CompletionMessage, CompletionRole};
use crate::model::stream::{StreamError, StreamParams, StreamingCompletionProvider};

use super::chat_events::{ChatEvent, EventSink, StatusPayload};
use super::primary_stream::{apply_reasoning_chunk, PreservePartialOnError, StreamingState};
use super::tool_call_threading::ThreadedMessage;
use super::tool_execution::{
    process_tool_calls, GeneratedImage, StatusContext, ToolCall, ToolExecutionContext, ToolMessage,
    ToolRunner,
};

/// Cap on text-tool iterations per assistant turn (v4 `MAX_TEXT_TOOL_ITERATIONS`).
/// Mirrors the native-tool loop's `effectiveMaxTurns` default.
const MAX_TEXT_TOOL_ITERATIONS: i64 = 5;

/// Number of *prior* identical tool-call signatures permitted before the loop
/// stops executing and nudges the model to respond (v4 `MAX_DUPLICATE_TOOL_CALLS`).
/// `>= 2` prior matches means the current call is the third.
const MAX_DUPLICATE_TOOL_CALLS: usize = 2;

// ===========================================================================
// The strategy seam (TextToolStrategy)
// ===========================================================================

/// One parsed text-format tool call (v4 `ParsedTextToolCall`). `call_id` is
/// optional — the simple-json / text-block strategies never set it; a
/// provider-text-markers strategy may.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTextToolCall {
    pub name: String,
    pub arguments: Value,
    pub call_id: Option<String>,
}

/// A text-tool detection strategy (v4 `TextToolStrategy`). Keeps the engine
/// strategy-agnostic: the concrete detector / parser / stripper / result-formatter
/// are the strategy's business. The trait is object-safe (`&dyn` works), so the
/// orchestrator's provider-marker seam can hand a borrowed trait object.
pub trait TextToolStrategy {
    /// Identifies the pass for logging and the unsupported-strategy fallthrough
    /// (`provider-text-markers` / `text-block` / `simple-json`).
    fn name(&self) -> &str;
    /// Whether the response contains markers this strategy can parse.
    fn has_markers(&self, response: &str) -> bool;
    /// Parse tool calls out of the response. MAY return empty even when
    /// [`has_markers`](Self::has_markers) returned true; the pass no-ops then.
    fn parse(&self, response: &str) -> Vec<ParsedTextToolCall>;
    /// Remove the strategy's markers. Called once on the pre-continuation text AND
    /// once per raw segment at final assembly.
    fn strip(&self, response: &str) -> String;
    /// Format a single tool result as a synthetic `user`-role continuation message,
    /// framed symmetrically with the markers just stripped.
    fn format_tool_result(&self, tool_name: &str, content: &str) -> String;
    /// Optional provider stop sequences applied to the continuation re-stream
    /// (the orchestrator applies them to the *initial* stream; the loop applies
    /// them here for the continuation). Default: none.
    fn stop_sequences(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The simple-json strategy (`<tool_call>{...}</tool_call>`), composed entirely
/// from the ported b.1 leaves.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimpleJsonStrategy;

impl TextToolStrategy for SimpleJsonStrategy {
    fn name(&self) -> &str {
        "simple-json"
    }
    fn has_markers(&self, response: &str) -> bool {
        super::pseudo_tool::has_simple_json_in_response(response)
    }
    fn parse(&self, response: &str) -> Vec<ParsedTextToolCall> {
        super::pseudo_tool::parse_simple_json_from_response(response)
            .into_iter()
            .map(|r| ParsedTextToolCall {
                name: r.name,
                arguments: r.arguments,
                call_id: None,
            })
            .collect()
    }
    fn strip(&self, response: &str) -> String {
        super::pseudo_tool::strip_simple_json_from_response(response)
    }
    fn format_tool_result(&self, tool_name: &str, content: &str) -> String {
        super::pseudo_tool::format_simple_json_tool_result(tool_name, content)
    }
    fn stop_sequences(&self) -> Vec<String> {
        super::pseudo_tool::SIMPLE_JSON_STOP
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }
}

/// The text-block strategy (`[[TOOL_NAME ...]]content[[/TOOL_NAME]]`), composed
/// from the ported b.1 leaves. `format_tool_result` frames results as
/// `[Tool Result: <name>]\n<content>` (v4's inline arrow function).
#[derive(Debug, Clone, Copy, Default)]
pub struct TextBlockStrategy;

impl TextToolStrategy for TextBlockStrategy {
    fn name(&self) -> &str {
        "text-block"
    }
    fn has_markers(&self, response: &str) -> bool {
        crate::tools::text_block_parser::has_text_block_markers(response)
    }
    fn parse(&self, response: &str) -> Vec<ParsedTextToolCall> {
        super::pseudo_tool::parse_text_blocks_from_response(response)
            .into_iter()
            .map(|r| ParsedTextToolCall {
                name: r.name,
                arguments: r.arguments,
                call_id: None,
            })
            .collect()
    }
    fn strip(&self, response: &str) -> String {
        super::pseudo_tool::strip_text_block_markers_from_response(response)
    }
    fn format_tool_result(&self, tool_name: &str, content: &str) -> String {
        format!("[Tool Result: {tool_name}]\n{content}")
    }
}

// ===========================================================================
// Options / entry point
// ===========================================================================

/// A pending anchor: the caller-`tool_messages` range a batch occupies plus the
/// `raw_responses` index of the segment whose markers triggered it. The prose
/// offset is resolved into `anchor_offset` at final assembly (v4 resolves it in a
/// single pass so `strip` still runs exactly once per segment).
struct PendingAnchor {
    /// First index (inclusive) into the caller's `tool_messages`.
    start: usize,
    /// Last index (exclusive) into the caller's `tool_messages`.
    end: usize,
    /// The `raw_responses` index of the emitting segment.
    segment_index: usize,
}

/// Everything [`run_text_tool_pass`] needs beyond the injected provider / sink /
/// runner / strategy / preserver (v4 `RunTextToolPassOptions` minus the pieces
/// resolved above the seam). The `state` / `tool_messages` / `generated_image_paths`
/// are mutated in place (v4's caller keeps live bindings).
pub struct RunTextToolPassOptions<'a> {
    pub chat_id: String,
    /// v4 `character.id` / `character.name` — the status-frame + ledger identity.
    pub character_id: String,
    pub character_name: String,
    /// The provider name (v4 `streaming.effectiveProfile.provider`) — the
    /// continuation stream keys on it.
    pub provider: String,
    /// The connection profile base URL.
    pub base_url: Option<String>,
    /// The slate that primed the initial stream (v4 `formattedMessages`). The
    /// continuation starts from a fresh copy of this + the ledger.
    pub formatted_messages: Vec<ThreadedMessage>,
    /// The base stream params (model / temperature / profile params / …). The
    /// continuation clones these and overwrites `messages` / `tools` /
    /// `web_search_enabled` / `stop`.
    pub base_params: StreamParams,
    /// Tools to expose during the continuation re-stream (v4 `continuationTools`).
    /// The text-block pass generally sends `Some([])`; the provider pass sends the
    /// regular slate. `None` ⇒ no tools.
    pub continuation_tools: Option<Value>,
    /// Whether the continuation re-stream may use native web search.
    pub continuation_use_native_web_search: bool,
    /// The tool-execution context handed to every handler (v4 `toolContext`).
    pub tool_context: ToolExecutionContext,
    /// v4 `streaming` — mutated in place.
    pub state: &'a mut StreamingState,
    /// v4 `toolMessages` — appended in place; anchors stamped at final assembly.
    pub tool_messages: &'a mut Vec<ToolMessage>,
    /// v4 `generatedImagePaths` — appended in place.
    pub generated_image_paths: &'a mut Vec<GeneratedImage>,
}

/// Run text-tool detection-and-continuation up to [`MAX_TEXT_TOOL_ITERATIONS`]
/// times (v4 `runTextToolPass`). No-ops when the initial response has no markers.
/// Returns `Err(StreamError)` on a continuation stream failure (after preserving
/// the assembled partial), matching v4's rethrow.
#[allow(clippy::too_many_arguments)]
pub async fn run_text_tool_pass<P, S, R, Strat>(
    db: &Db,
    provider: &P,
    sink: &S,
    runner: &R,
    strategy: &Strat,
    preserve: &mut PreservePartialOnError,
    opts: RunTextToolPassOptions<'_>,
) -> Result<(), StreamError>
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: ToolRunner,
    Strat: TextToolStrategy + ?Sized,
{
    let RunTextToolPassOptions {
        chat_id: _chat_id,
        character_id,
        character_name,
        provider: provider_name,
        base_url,
        formatted_messages,
        base_params,
        continuation_tools,
        continuation_use_native_web_search,
        tool_context,
        state,
        tool_messages,
        generated_image_paths,
    } = opts;

    // Entry gate (v4 :158): a falsy `fullResponse` or no markers → no-op.
    if state.full_response.is_empty() || !strategy.has_markers(&state.full_response) {
        return Ok(());
    }

    let status_context = StatusContext {
        character_name: character_name.clone(),
        character_id: character_id.clone(),
    };

    // Raw (un-stripped) response from each stream pass — the primary stream first,
    // then each continuation. Stripped + joined at the end into the visible body.
    let mut raw_responses: Vec<String> = vec![state.full_response.clone()];
    // Accumulated (assistant, tool_result+) pairs sent back to the model on every
    // continuation. The assistant entries keep their markers so the causal chain
    // stays intact for the model.
    let mut ledger: Vec<ThreadedMessage> = Vec::new();
    // JSON-stringified signatures of executed tool-call batches.
    let mut tool_call_history: Vec<String> = Vec::new();
    // Per executed batch: the caller-`tool_messages` range + the emitting segment.
    let mut pending_anchors: Vec<PendingAnchor> = Vec::new();
    let mut iterations: i64 = 0;

    while iterations < MAX_TEXT_TOOL_ITERATIONS {
        let latest = raw_responses
            .last()
            .expect("raw_responses non-empty")
            .clone();
        if !strategy.has_markers(&latest) {
            break;
        }
        let parsed = strategy.parse(&latest);
        if parsed.is_empty() {
            break;
        }

        let call_signature = call_signature_of(&parsed);
        let duplicate_count = tool_call_history
            .iter()
            .filter(|s| s.as_str() == call_signature)
            .count();

        if duplicate_count >= MAX_DUPLICATE_TOOL_CALLS {
            // Don't execute the duplicate batch (v4 :198). Keep the assistant's
            // repeated request in the ledger so the model sees what it just tried,
            // then append a synthetic user nudge and re-stream once more.
            ledger.push(assistant_ledger_entry(&latest, state));
            ledger.push(nudge_user_entry(duplicate_count));

            if let Err(e) = stream_continuation(
                provider,
                sink,
                strategy,
                &character_name,
                &character_id,
                &provider_name,
                base_url.as_deref(),
                &base_params,
                &continuation_tools,
                continuation_use_native_web_search,
                &formatted_messages,
                &ledger,
                state,
                &mut raw_responses,
            )
            .await
            {
                state.full_response = assemble_stripped(strategy, &raw_responses);
                preserve.preserve(db, state, &e.message).await;
                return Err(e);
            }
            break;
        }

        tool_call_history.push(call_signature);
        iterations += 1;

        // Execute (v4 :246). `processToolCalls` emits the detection frame + per-tool
        // status + result frames (status context always present on this path).
        let tool_calls: Vec<ToolCall> = parsed
            .iter()
            .map(|p| ToolCall {
                name: p.name.clone(),
                arguments: p.arguments.clone(),
                call_id: p.call_id.clone(),
            })
            .collect();
        let results = process_tool_calls(
            &tool_calls,
            &tool_context,
            sink,
            runner,
            Some(&status_context),
        )
        .await;

        // Remember which assembled segment this batch's prose ends at — `latest` is
        // the segment that emitted the markers, and its continuation isn't pushed
        // until below. The actual offset is resolved once at final assembly.
        let segment_index = raw_responses.len() - 1;
        let start = tool_messages.len();
        tool_messages.extend(results.tool_messages.iter().cloned());
        let end = tool_messages.len();
        pending_anchors.push(PendingAnchor {
            start,
            end,
            segment_index,
        });
        generated_image_paths.extend(results.generated_image_paths);

        // Append the just-finished turn to the ledger. `latest` retains its markers
        // ON PURPOSE (v4 :311): stripping them here is what broke continuations on
        // simple-json (the model couldn't see why a tool_result had appeared in a
        // user turn).
        ledger.push(assistant_ledger_entry(&latest, state));
        for tm in &results.tool_messages {
            ledger.push(tool_result_ledger_entry(
                strategy,
                &tm.tool_name,
                &tm.content,
            ));
        }

        if let Err(e) = stream_continuation(
            provider,
            sink,
            strategy,
            &character_name,
            &character_id,
            &provider_name,
            base_url.as_deref(),
            &base_params,
            &continuation_tools,
            continuation_use_native_web_search,
            &formatted_messages,
            &ledger,
            state,
            &mut raw_responses,
        )
        .await
        {
            state.full_response = assemble_stripped(strategy, &raw_responses);
            preserve.preserve(db, state, &e.message).await;
            return Err(e);
        }
    }

    // Cap exit (v4 :340) is log-only (host-side).

    // Final assembly (v4 :344): strip each raw segment once, join, stamp anchors.
    let (text, segment_end_offsets) = assemble_stripped_with_offsets(strategy, &raw_responses);
    state.full_response = text;
    for anchor in &pending_anchors {
        // v4 `if (typeof anchor === 'number')` — segmentIndex is always in range
        // (a segment is pushed before its anchor is recorded), so this holds; the
        // guard is faithful.
        if let Some(&offset) = segment_end_offsets.get(anchor.segment_index) {
            for tm in &mut tool_messages[anchor.start..anchor.end] {
                tm.anchor_offset = Some(offset as f64);
            }
        }
    }

    Ok(())
}

// ===========================================================================
// Ledger entry builders
// ===========================================================================

/// The assistant ledger entry (v4 :212 / :306): the just-streamed segment WITH its
/// markers, carrying the CURRENT `thought_signature` / `reasoning_content`.
fn assistant_ledger_entry(latest: &str, state: &StreamingState) -> ThreadedMessage {
    ThreadedMessage {
        role: "assistant".to_string(),
        content: latest.to_string(),
        name: None,
        thought_signature: state.thought_signature.clone(),
        reasoning_content: state.reasoning_content.clone(),
        tool_call_id: None,
        tool_calls: None,
    }
}

/// The synthetic user nudge (v4 :219, byte-exact — the em-dash is U+2014, the
/// apostrophes are straight). Interpolates `duplicate_count + 1`.
fn nudge_user_entry(duplicate_count: usize) -> ThreadedMessage {
    let n = duplicate_count + 1;
    ThreadedMessage {
        role: "user".to_string(),
        content: format!(
            "You have already called the same tool with the same arguments {n} times and \
             received the same result each time. You already have the data you need — do NOT \
             call any more tools. Respond now, in character, using what you've already learned."
        ),
        name: None,
        thought_signature: None,
        reasoning_content: None,
        tool_call_id: None,
        tool_calls: None,
    }
}

/// A synthetic user tool-result entry (v4 :326): the strategy-formatted result.
fn tool_result_ledger_entry<Strat: TextToolStrategy + ?Sized>(
    strategy: &Strat,
    tool_name: &str,
    content: &str,
) -> ThreadedMessage {
    ThreadedMessage {
        role: "user".to_string(),
        content: strategy.format_tool_result(tool_name, content),
        name: None,
        thought_signature: None,
        reasoning_content: None,
        tool_call_id: None,
        tool_calls: None,
    }
}

/// The batch call signature (v4 :188): `JSON.stringify(parsed.map(tc => ({ name,
/// arguments })))` — insertion-order stringify, `callId` excluded.
fn call_signature_of(parsed: &[ParsedTextToolCall]) -> String {
    let arr: Vec<Value> = parsed
        .iter()
        .map(|p| {
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(p.name.clone()));
            m.insert("arguments".to_string(), p.arguments.clone());
            Value::Object(m)
        })
        .collect();
    serde_json::to_string(&Value::Array(arr)).unwrap_or_default()
}

// ===========================================================================
// streamContinuation
// ===========================================================================

/// Re-stream a continuation (v4 `streamContinuation`). Emits the `sending`-status
/// frame, streams `formatted_messages ++ ledger`, appends content to a freshly
/// pushed `raw_responses` slot (pushed FIRST so a mid-stream failure preserves the
/// partial), forwards reasoning DISPLAY-ONLY (flat — no positioned segments on
/// this path; carry that comment), and OVERWRITES `usage`/`cache_usage`/
/// `raw_response`/`thought_signature` on done. Returns the stream error (if any).
#[allow(clippy::too_many_arguments)]
async fn stream_continuation<P, S, Strat>(
    provider: &P,
    sink: &S,
    strategy: &Strat,
    character_name: &str,
    character_id: &str,
    provider_name: &str,
    base_url: Option<&str>,
    base_params: &StreamParams,
    continuation_tools: &Option<Value>,
    continuation_use_native_web_search: bool,
    formatted_messages: &[ThreadedMessage],
    ledger: &[ThreadedMessage],
    state: &mut StreamingState,
    raw_responses: &mut Vec<String>,
) -> Result<(), StreamError>
where
    P: StreamingCompletionProvider,
    S: EventSink,
    Strat: TextToolStrategy + ?Sized,
{
    sink.emit(ChatEvent::status(StatusPayload {
        stage: "sending".into(),
        message: format!("Sending to {character_name}..."),
        tool_name: None,
        character_name: Some(character_name.to_string()),
        character_id: Some(character_id.to_string()),
    }));

    // continuationMessages = [...formattedMessages, ...ledger].
    let mut continuation_messages: Vec<CompletionMessage> =
        Vec::with_capacity(formatted_messages.len() + ledger.len());
    continuation_messages.extend(formatted_messages.iter().map(to_completion_message));
    continuation_messages.extend(ledger.iter().map(to_completion_message));

    // Push the empty slot FIRST so a mid-stream failure preserves partial content.
    raw_responses.push(String::new());
    let idx = raw_responses.len() - 1;

    let mut params = base_params.clone();
    params.messages = continuation_messages;
    params.tools = continuation_tools.clone();
    params.web_search_enabled = continuation_use_native_web_search;
    params.stop = strategy.stop_sequences();

    let mut rx = provider
        .stream_message(provider_name, base_url, &params)
        .await;
    while let Some(item) = rx.recv().await {
        let chunk = match item {
            Ok(c) => c,
            Err(e) => return Err(e),
        };
        // Capture + live-forward reasoning. This pseudo-tool path doesn't grow
        // `full_response` live (chunks accumulate in `raw_responses` and the prose
        // is reassembled at the end), so we can't anchor positioned reasoning
        // segments reliably here — the flat `reasoning_content` renders as a single
        // leading thinking block instead. DISPLAY ONLY.
        apply_reasoning_chunk(state, &chunk, sink);
        if !chunk.content.is_empty() {
            raw_responses[idx].push_str(&chunk.content);
            sink.emit(ChatEvent::content(chunk.content.clone()));
        }
        if chunk.done {
            // OVERWRITE (not accumulate): a done with no usage NULLS them (v4's
            // `chunk.usage || null`).
            state.usage = chunk.usage;
            state.cache_usage = chunk.cache_usage;
            state.raw_response = chunk.raw_response.clone();
            if let Some(ts) = chunk.thought_signature.as_ref().filter(|s| !s.is_empty()) {
                state.thought_signature = Some(ts.clone());
            }
        }
    }
    Ok(())
}

/// The threaded message → stream `[{role, content}]` view (the canned key ignores
/// the threading metadata, matching v4's `streamMessage` seam).
fn to_completion_message(m: &ThreadedMessage) -> CompletionMessage {
    CompletionMessage {
        role: match m.role.as_str() {
            "system" => CompletionRole::System,
            "assistant" => CompletionRole::Assistant,
            "tool" => CompletionRole::Tool,
            _ => CompletionRole::User,
        },
        content: m.content.clone(),
    }
}

// ===========================================================================
// assembleStripped / assembleStrippedWithOffsets
// ===========================================================================

/// The final assembled body (v4 `assembleStripped`) — the offset form's text.
fn assemble_stripped<Strat: TextToolStrategy + ?Sized>(
    strategy: &Strat,
    raw_responses: &[String],
) -> String {
    assemble_stripped_with_offsets(strategy, raw_responses).0
}

/// Strip every raw response once, join the non-empty results with `\n\n`, and
/// record — for each `raw_responses` index — the UTF-16 offset at which that
/// segment ends in the joined string (v4 `assembleStrippedWithOffsets`). Empty
/// (filtered) segments take the end offset of the last kept segment before them.
/// Lengths are UTF-16 code units (JS `.length`); the `+2` separator is added only
/// BEFORE a kept segment after the first.
fn assemble_stripped_with_offsets<Strat: TextToolStrategy + ?Sized>(
    strategy: &Strat,
    raw_responses: &[String],
) -> (String, Vec<usize>) {
    let mut segment_end_offsets: Vec<usize> = Vec::with_capacity(raw_responses.len());
    let mut kept: Vec<String> = Vec::new();
    let mut running_len: usize = 0;
    for raw in raw_responses {
        let stripped = strategy.strip(raw);
        // v4 `stripped.trim().length > 0` — JS trim.
        if !crate::jsstr::js_trim(&stripped).is_empty() {
            if !kept.is_empty() {
                running_len += 2; // the '\n\n' separator before this segment
            }
            running_len += crate::jsstr::utf16_len(&stripped);
            kept.push(stripped);
        }
        segment_end_offsets.push(running_len);
    }
    (kept.join("\n\n"), segment_end_offsets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stream::{CannedStreamingProvider, StreamChunk, StreamUsage};
    use crate::services::chat_events::RecordingSink;
    use crate::services::primary_stream::EffectiveProfile;
    use crate::services::tool_execution::{create_tool_context, CannedToolRunner, ToolResult};
    use serde_json::json;
    use tempfile::{tempdir, TempDir};

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    fn dummy_db() -> (TempDir, Db) {
        let dir = tempdir().expect("tempdir");
        let db = Db::open_main(dir.path().join("ttl.db"), PEPPER).expect("open db");
        (dir, db)
    }

    fn base_params(model: &str) -> StreamParams {
        StreamParams {
            messages: Vec::new(),
            model: model.to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            top_p: None,
            tools: None,
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

    fn state_with(full: &str) -> StreamingState {
        StreamingState {
            full_response: full.into(),
            effective_profile: Some(EffectiveProfile {
                id: "prof-1".into(),
                provider: "ANTHROPIC".into(),
                model_name: "m".into(),
                base_url: None,
            }),
            effective_api_key: "test-key".into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn no_markers_is_a_clean_no_op() {
        let (_dir, db) = dummy_db();
        let provider = CannedStreamingProvider::new();
        let sink = RecordingSink::new();
        let runner = CannedToolRunner::new();
        let mut preserve = preserver();
        let mut state = state_with("just some prose, no markers");
        let mut tool_messages = Vec::new();
        let mut images = Vec::new();
        run_text_tool_pass(
            &db,
            &provider,
            &sink,
            &runner,
            &TextBlockStrategy,
            &mut preserve,
            RunTextToolPassOptions {
                chat_id: "chat-1".into(),
                character_id: "char-1".into(),
                character_name: "Friday".into(),
                provider: "ANTHROPIC".into(),
                base_url: None,
                formatted_messages: vec![],
                base_params: base_params("m"),
                continuation_tools: None,
                continuation_use_native_web_search: false,
                tool_context: create_tool_context(
                    "chat-1", "user-1", "char-1", "pp-1", None, None, None, None, None,
                ),
                state: &mut state,
                tool_messages: &mut tool_messages,
                generated_image_paths: &mut images,
            },
        )
        .await
        .expect("no-op ok");
        assert_eq!(state.full_response, "just some prose, no markers");
        assert!(tool_messages.is_empty());
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn text_block_single_iteration_then_continuation() {
        let (_dir, db) = dummy_db();
        // The initial response emits one text-block state call; the continuation
        // returns plain prose with no markers → the loop breaks.
        let initial = "Let me check.[[state key=\"mood\"]][[/state]]";
        let parsed = super::super::pseudo_tool::parse_text_blocks_from_response(initial);
        assert_eq!(parsed.len(), 1);
        let call = ToolCall {
            name: parsed[0].name.clone(),
            arguments: parsed[0].arguments.clone(),
            call_id: None,
        };
        let mut runner = CannedToolRunner::new();
        runner.register(
            &call,
            ToolResult {
                tool_name: call.name.clone(),
                success: true,
                result: json!({ "mood": "curious" }),
                error: None,
                message: None,
                metadata: None,
            },
        );

        // The continuation slate = [assistant(markers kept), user(tool result)].
        let stripped_initial =
            super::super::pseudo_tool::strip_text_block_markers_from_response(initial);
        let tool_content = "{\n  \"mood\": \"curious\"\n}";
        let cont_messages = vec![
            CompletionMessage {
                role: CompletionRole::Assistant,
                content: initial.into(),
            },
            CompletionMessage {
                role: CompletionRole::User,
                content: format!("[Tool Result: {}]\n{}", call.name, tool_content),
            },
        ];
        let provider = CannedStreamingProvider::new().with_stream(
            "ANTHROPIC",
            "m",
            Some(0.7),
            &cont_messages,
            vec![
                Ok(StreamChunk::content(" All set.")),
                Ok(StreamChunk {
                    done: true,
                    usage: Some(StreamUsage {
                        prompt_tokens: 5,
                        completion_tokens: 1,
                        total_tokens: 6,
                    }),
                    raw_response: Some(json!({ "marker": "done" })),
                    ..Default::default()
                }),
            ],
        );

        let sink = RecordingSink::new();
        let mut preserve = preserver();
        let mut state = state_with(initial);
        let mut tool_messages = Vec::new();
        let mut images = Vec::new();
        run_text_tool_pass(
            &db,
            &provider,
            &sink,
            &runner,
            &TextBlockStrategy,
            &mut preserve,
            RunTextToolPassOptions {
                chat_id: "chat-1".into(),
                character_id: "char-1".into(),
                character_name: "Friday".into(),
                provider: "ANTHROPIC".into(),
                base_url: None,
                formatted_messages: vec![],
                base_params: base_params("m"),
                continuation_tools: Some(json!([])),
                continuation_use_native_web_search: false,
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

        // fullResponse = strip(seg0) + "\n\n" + strip(" All set.") — the text-block
        // strip trims each segment, so the continuation's leading space is dropped.
        let cont_stripped =
            super::super::pseudo_tool::strip_text_block_markers_from_response(" All set.");
        let expected = format!("{stripped_initial}\n\n{cont_stripped}");
        assert_eq!(state.full_response, expected);
        assert_eq!(tool_messages.len(), 1);
        assert_eq!(tool_messages[0].tool_name, call.name);
        // Anchor = end of segment 0 = UTF-16 length of the stripped initial.
        assert_eq!(
            tool_messages[0].anchor_offset,
            Some(crate::jsstr::utf16_len(&stripped_initial) as f64)
        );
        assert_eq!(state.usage.unwrap().total_tokens, 6);
    }

    #[test]
    fn assemble_offsets_drop_whitespace_segments_and_carry_offsets() {
        // Segment 1 strips to whitespace → dropped, its end offset inherits.
        struct FakeStrategy;
        impl TextToolStrategy for FakeStrategy {
            fn name(&self) -> &str {
                "fake"
            }
            fn has_markers(&self, _r: &str) -> bool {
                true
            }
            fn parse(&self, _r: &str) -> Vec<ParsedTextToolCall> {
                Vec::new()
            }
            fn strip(&self, r: &str) -> String {
                // Strip a leading "X" marker; leave the rest.
                r.strip_prefix('X').unwrap_or(r).to_string()
            }
            fn format_tool_result(&self, _n: &str, _c: &str) -> String {
                String::new()
            }
        }
        // segment 0 → "café" (4 UTF-16), segment 1 → "   " (whitespace → dropped),
        // segment 2 → "𐐷" (a surrogate pair → 2 UTF-16 units).
        let raws = vec!["Xcafé".to_string(), "X   ".to_string(), "X𐐷".to_string()];
        let (text, offsets) = assemble_stripped_with_offsets(&FakeStrategy, &raws);
        assert_eq!(text, "café\n\n𐐷");
        // seg0 end = 4; seg1 dropped → inherits 4; seg2 end = 4 + 2 (\n\n) + 2 = 8.
        assert_eq!(offsets, vec![4, 4, 8]);
    }
}
