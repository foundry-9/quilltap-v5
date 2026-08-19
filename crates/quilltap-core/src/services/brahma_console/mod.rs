//! The Brahma one-shot console (W4.5b) — v4
//! `lib/services/brahma-console/one-shot.service.ts` (`runBrahmaQuery`), the
//! isolated operator console the Carina engine invokes when the answerer is
//! Brahma. It runs a single ISOLATED query and returns the final answer text:
//!
//!  - **No chat history.** The conversation slate is exactly `[system,
//!    user(question)]` — loading the Salon transcript would leak every other
//!    participant's content into Brahma and break Carina's isolation contract.
//!  - **No persistence.** Tool calls execute (their own side effects stand), but
//!    the per-iteration assistant / TOOL messages are never written to the Salon,
//!    no tokens are tracked, and no done/error SSE events are emitted. The answer
//!    is accumulated in memory and returned.
//!  - **Operator surface.** Tools run with `operator_surface = true`, unlocking
//!    `run_sql` and all-store document access. The caller (`run_carina_query`)
//!    gates reachability to the operator, user-controlled personas, and
//!    `systemTransparency` characters BEFORE calling here.
//!
//! It closes the [`RunBrahmaConsole`](super::carina_query::RunBrahmaConsole) seam
//! W4.5 left injected. [`RealBrahmaConsole`] is the production impl of the frozen
//! trait; the Carina engine's `answer_as_brahma` maps `detail == "no-profile"` to
//! the no-profile error and anything else (or an internal failure) to `llm-failed`
//! — so `run_brahma_query` NEVER throws, returning every failure as a `detail`.
//!
//! ## The seams
//!
//! Generic-consumed over the streaming provider / tool runner / tool detector (no
//! boxing; the future stays `Send` — the [`EmbeddingProvider`]/[`ToolRunner`]
//! precedent). The `search` tool's embedding rides the tool runner's own
//! `ErasedEmbeddingProvider`, so the console needs no separate embedding dep; the
//! api-key step reads `api_keys` directly (v4's UNSCOPED `findApiKeyById`), so no
//! resolver seam is needed (both differential sides read the same fixture; the
//! plaintext key value never reaches the diffed surface — the stream is canned).

pub mod orchestrator;
pub mod prompt_text;
pub mod turn_budget;

use std::future::Future;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{connection_profiles, DbError};
use crate::jsstr::js_trim;
use crate::model::stream::{StreamParams, StreamingCompletionProvider};
use crate::provider_manifest::Registry;
use crate::services::agent_mode::{
    build_agent_mode_instructions, build_force_final_message,
    extract_submit_final_response_from_text,
};
use crate::services::api_key_service::{self, ProfileApiKeyFailure, ProfileApiKeyResolution};
use crate::services::carina_query::{BrahmaConsoleResult, RunBrahmaConsole};
use crate::services::chat_events::{ChatEvent, EventSink};
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::pseudo_tool::{
    build_native_tool_system_instructions, build_text_block_system_instructions,
    check_should_use_text_block_tools, parse_text_blocks_from_response,
    strip_text_block_markers_from_response, TextBlockEnabledToolOptions,
};
use crate::services::tool_build::{build_tools, BuildToolsInput};
use crate::services::tool_call_threading::{
    build_assistant_tool_call_message, build_tool_result_messages, to_stream_messages,
    DetectedToolCall, ThreadedMessage,
};
use crate::services::tool_execution::{
    process_tool_calls, StatusContext, ToolCall, ToolExecutionContext,
};
use crate::tools::pseudo_tool_support::ToolMode;

use prompt_text::{BRAHMA_BASE_BRIEF, BRAHMA_SQL_PROMPT};

/// v4's stuck-loop threshold (`MAX_DUPLICATE_TOOL_CALLS`) — consecutive duplicate
/// or stale tool iterations before forcing a final answer. Independent of the
/// operator-set agent-turn budget ([`turn_budget::resolve_brahma_max_agent_turns`]),
/// which replaced v4's hardcoded `MAX_AGENT_TURNS = 25`.
const MAX_DUPLICATE_TOOL_CALLS: usize = 2;

// ===========================================================================
// Small JSON accessors (private, mirrors carina_query's).
// ===========================================================================

pub(super) fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}
pub(super) fn b(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}

// ===========================================================================
// The two ported helpers (v4 orchestrator.service.ts — the SEPARATE Phase-4
// streaming console; only these two are imported by the one-shot engine).
// ===========================================================================

/// v4 `resolveBrahmaConnectionProfile(repos, userId, consoleConnectionProfileId)`.
/// The one-shot engine always passes `null` for the console profile, so this
/// collapses to the user's default profile (there is no per-chat console profile
/// when Brahma is consulted from a Salon). Returns `None` when no default resolves.
pub fn resolve_brahma_connection_profile(
    db: &Db,
    user_id: &str,
    console_connection_profile_id: Option<&str>,
) -> Option<Value> {
    if let Some(pinned_id) = console_connection_profile_id.filter(|s| !s.is_empty()) {
        let pid = pinned_id.to_string();
        if let Ok(Some(pinned)) = db.read_main(move |c| connection_profiles::find_by_id(c, &pid)) {
            // v4 requires the pinned profile to be owned by the user.
            if s(&pinned, "userId").as_deref() == Some(user_id) {
                return Some(pinned);
            }
        }
    }
    let uid = user_id.to_string();
    db.read_main(move |c| connection_profiles::find_default(c, &uid))
        .ok()
        .flatten()
}

/// v4 `normalizeToolCallSignature`. The stuck-loop fingerprint: keys sorted, each
/// STRING value whitespace-collapsed + trimmed + lowercased; non-string values
/// pass through. Only EQUALITY matters (both differential sides feed identical
/// tool calls, so both compute the same duplicate counts), so the exact
/// `localeCompare`-vs-code-unit key order is not diff-critical.
pub fn normalize_tool_call_signature(tool_calls: &[ToolCall]) -> String {
    let projected: Vec<Value> = tool_calls
        .iter()
        .map(|tc| {
            let mut args = serde_json::Map::new();
            let mut keys: Vec<String> = tc
                .arguments
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            keys.sort();
            for k in keys {
                let raw = tc.arguments.get(&k).cloned().unwrap_or(Value::Null);
                let norm = match raw {
                    Value::String(sv) => Value::String(collapse_ws_lower(&sv)),
                    other => other,
                };
                args.insert(k, norm);
            }
            serde_json::json!({ "name": tc.name, "arguments": Value::Object(args) })
        })
        .collect();
    serde_json::to_string(&projected).unwrap_or_else(|_| "[]".to_string())
}

/// v4 `value.replace(/\s+/g, ' ').trim().toLowerCase()` (the JS `\s` set + default
/// Unicode lowercasing; `str::to_lowercase` matches JS `toLowerCase` byte-for-byte
/// per the ported case-mapping seam).
fn collapse_ws_lower(v: &str) -> String {
    let collapsed = collapse_js_whitespace(v);
    js_trim(&collapsed).to_lowercase()
}

/// Collapse each run of JS-`\s` whitespace to a single ASCII space.
fn collapse_js_whitespace(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut in_ws = false;
    for ch in v.chars() {
        if is_js_whitespace(ch) {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out
}

/// JS `\s`: ` \t\n\r\x0b\x0c` + Unicode space separators + BOM/line/para sep.
fn is_js_whitespace(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\t' | '\n' | '\r' | '\u{000b}' | '\u{000c}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

// ===========================================================================
// The system prompt (v4 buildBrahmaSystemPrompt).
// ===========================================================================

/// v4 `buildBrahmaSystemPrompt`: `[base brief, (SQL section), (tool instructions)]
/// .join("\n\n").trim()`. The one-shot console always passes `include_sql_access =
/// true` and a non-empty `tool_instructions` (agent-mode instructions are always
/// appended), so both sections are present.
pub fn build_brahma_system_prompt(tool_instructions: &str, include_sql_access: bool) -> String {
    let mut parts: Vec<&str> = vec![BRAHMA_BASE_BRIEF];
    if include_sql_access {
        parts.push(BRAHMA_SQL_PROMPT);
    }
    if !tool_instructions.is_empty() {
        parts.push(tool_instructions);
    }
    js_trim(&parts.join("\n\n")).to_string()
}

// ===========================================================================
// The injected deps + the engine.
// ===========================================================================

/// The model boundaries + seams the console composes. Borrowed for the life of one
/// query.
pub struct BrahmaQueryDeps<'a, STR, TR, TD>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
{
    pub db: &'a Db,
    /// The streaming model boundary (v4 `streamMessage`).
    pub streaming: &'a STR,
    /// The tool executor boundary (v4 `executeToolCallWithContext` + every
    /// handler). The Brahma slate has NO `ask_carina`, so no recursion.
    pub tool_runner: &'a TR,
    /// v4 `detectToolCallsInResponse` — reads provider-native tool calls off the
    /// raw response.
    pub tool_detector: &'a TD,
    /// Injected `checkModelSupportsTools(provider, model, userId)` for the resolved
    /// profile (registry-seam pattern; feeds `build_tools` + the tool-mode gate).
    pub model_supports_native_tools: bool,
}

use crate::services::tool_execution::ToolRunner;

/// A swallowing [`EventSink`] — the console emits NOTHING to SSE (v4's no-op
/// `StreamController { enqueue: () => {} }`).
struct NoopSink;
impl EventSink for NoopSink {
    fn emit(&self, _event: ChatEvent) {}
}

/// One streamed LLM call → `(answer, rawResponse, reasoning, thoughtSignature)`.
/// The slate crosses the stream boundary losslessly (v4 passes
/// `ThreadedMessage[]` straight to `streamMessage`; P4.13 unit 2 made the v5
/// boundary carry the tool-call linkage instead of flattening it away).
async fn run_stream<STR: StreamingCompletionProvider>(
    streaming: &STR,
    provider: &str,
    base_url: Option<&str>,
    model: &str,
    messages: &[ThreadedMessage],
    tools: &[Value],
) -> (String, Option<Value>, String, Option<String>) {
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
        // v4 hardcodes `useNativeWebSearch: false` in the Brahma stream.
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
        // v4 sets no `requestTimeoutMs` on any streaming call (P4.D83).
        request_timeout_ms: None,
    };
    let mut rx = streaming.stream_message(provider, base_url, &params).await;
    let mut answer = String::new();
    let mut raw: Option<Value> = None;
    let mut reasoning = String::new();
    let mut thought_signature: Option<String> = None;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            Ok(c) => {
                if let Some(rc) = &c.reasoning_content {
                    if *rc != reasoning {
                        reasoning = rc.clone();
                    }
                }
                answer.push_str(&c.content);
                if let Some(raw_r) = c.raw_response {
                    raw = Some(raw_r);
                }
                if let Some(ts) = c.thought_signature {
                    thought_signature = Some(ts);
                }
            }
            Err(_) => break,
        }
    }
    (answer, raw, reasoning, thought_signature)
}

fn fail(detail: impl Into<String>) -> BrahmaConsoleResult {
    BrahmaConsoleResult {
        ok: false,
        answer: String::new(),
        detail: Some(detail.into()),
    }
}

/// Run an isolated Brahma Console query and return the final answer text (v4
/// `runBrahmaQuery`). NEVER errors out of the function — every failure is a
/// `BrahmaConsoleResult { ok: false, detail }`.
pub async fn run_brahma_query<STR, TR, TD>(
    deps: &BrahmaQueryDeps<'_, STR, TR, TD>,
    user_id: &str,
    chat_id: &str,
    question: &str,
) -> BrahmaConsoleResult
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
{
    // 1. Profile (model): the user's default — no per-chat console profile from a
    //    Salon.
    let Some(profile) = resolve_brahma_connection_profile(deps.db, user_id, None) else {
        return fail("no-profile");
    };
    let provider = s(&profile, "provider").unwrap_or_default();
    let model = s(&profile, "modelName").unwrap_or_default();
    let base_url = s(&profile, "baseUrl");

    // 2. API key: v4's `resolveConnectionProfileApiKey` (bug 81) — required where
    //    required, forwarded where merely accepted, and loud on a dangling id
    //    even there. UNSCOPED `findApiKeyById`, as v4's is. The resolved plaintext
    //    is off the diffed surface (the stream is canned) and unused here, as it
    //    was before: the host streaming provider resolves keys internally.
    //    ⚠ v4's sentences here are LOWER-CASE where the orchestrator's are not;
    //    the difference is pre-existing and deliberate — ported verbatim.
    let key_id = s(&profile, "apiKeyId");
    let provider_for_key = provider.clone();
    let resolution = match deps.db.read_main(move |c| {
        Ok::<_, DbError>(api_key_service::resolve_connection_profile_api_key(
            c,
            &provider_for_key,
            key_id.as_deref(),
        ))
    }) {
        Ok(r) => r,
        Err(_) => ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::ApiKeyNotFound),
    };
    if let ProfileApiKeyResolution::Failed(reason) = resolution {
        return fail(match reason {
            ProfileApiKeyFailure::NoApiKeyConfigured => {
                "no API key configured for this connection profile"
            }
            ProfileApiKeyFailure::ApiKeyNotFound => "API key not found",
        });
    }

    // 3. Tools — the console slate: agent mode, doc read/write, read-only run_sql,
    //    search-without-memories; NO ask_carina (recursion guard), NO workspace
    //    tools.
    let provider_supports_web_search = Registry::built_in()
        .supports_capability(&provider, crate::provider_manifest::Capability::WebSearch);
    let no_disabled: [String; 0] = [];
    let no_groups: [String; 0] = [];
    let built = match build_tools(
        deps.db,
        user_id,
        &BuildToolsInput {
            provider: &provider,
            use_native_web_search: b(&profile, "useNativeWebSearch").unwrap_or(false),
            allow_tool_use: b(&profile, "allowToolUse"),
            allow_web_search: b(&profile, "allowWebSearch").unwrap_or(false),
            image_profile_id: None,
            image_provider_constraints: None,
            project_id: None,
            request_full_context: false,
            disabled_tools: Some(&no_disabled),
            disabled_tool_groups: &no_groups,
            agent_mode_enabled: true,
            is_multi_character: false,
            help_tools_enabled: false,
            can_dress_themselves: false,
            can_create_outfits: false,
            document_editing_enabled: true,
            ask_carina_enabled: false,
            include_workspace_tools: false,
            exclude_memory_search: true,
            sql_access: true,
            model_supports_native_tools: deps.model_supports_native_tools,
            provider_supports_web_search,
            // The character-less Brahma Console never offers `run_custom`.
            custom_tool_context: None,
        },
    ) {
        Ok(built) => built,
        Err(e) => return fail(format!("{e:?}")),
    };
    let tools = built.tools;
    let model_supports_native_tools = built.model_supports_native_tools;

    // 4. Tool mode (native vs. text-block); simple-json COERCES to text-block.
    let effective_pseudo_tool_mode: Option<ToolMode> =
        match s(&profile, "pseudoToolMode").as_deref() {
            Some("simple-json") => Some(ToolMode::TextBlock),
            Some(other) => ToolMode::from_str(other),
            None => Some(ToolMode::Auto), // v4 `?? 'auto'`.
        };
    let use_text_block_tools =
        check_should_use_text_block_tools(model_supports_native_tools, effective_pseudo_tool_mode);

    // 5. Instructions.
    let mut tool_instructions = String::new();
    if use_text_block_tools && !tools.is_empty() {
        let opts = TextBlockEnabledToolOptions {
            image_generation: false,
            search: true,
            web_search: b(&profile, "allowWebSearch").unwrap_or(false),
            whisper: false,
            state: false,
            rng: false,
            project_info: false,
            help_search: false,
            help_settings: false,
            help_navigate: false,
            create_note: false,
            wardrobe_list: false,
            wardrobe_read: false,
            wardrobe_wear: false,
            wardrobe_take_off: false,
            wardrobe_create: false,
            wardrobe_update: false,
            wardrobe_archive: false,
        };
        tool_instructions = build_text_block_system_instructions(&opts);
    } else if !tools.is_empty() {
        tool_instructions = build_native_tool_system_instructions();
    }
    // Operator-set turn budget (Settings → Chat → Brahma Console); shared with
    // the streaming orchestrator. The stuck-loop guard below is independent of it.
    let max_agent_turns = turn_budget::resolve_brahma_max_agent_turns(deps.db);
    let agent_instructions = build_agent_mode_instructions(max_agent_turns);
    tool_instructions = if tool_instructions.is_empty() {
        agent_instructions
    } else {
        format!("{tool_instructions}\n\n{agent_instructions}")
    };

    let system_prompt = build_brahma_system_prompt(&tool_instructions, true);

    // 6. ISOLATION: system + the single question only — never the Salon transcript.
    let mut conversation_messages: Vec<ThreadedMessage> = vec![
        plain_message("system", &system_prompt),
        plain_message("user", question),
    ];

    // v4: `(!useTextBlockTools && modelSupportsNativeTools) ? tools : []`.
    let effective_tools: Vec<Value> = if !use_text_block_tools && model_supports_native_tools {
        tools.clone()
    } else {
        Vec::new()
    };

    let status = StatusContext {
        character_name: "Brahma Console".to_string(),
        character_id: String::new(),
    };

    // 7. The agent tool loop (its OWN loop, faithful to v4) — bounded by the
    //    operator-set `max_agent_turns` resolved above.
    let mut agent_turn_count: i64 = 0;
    let mut full_response = String::new();
    let mut tool_call_history: Vec<String> = Vec::new();
    let mut seen_result_fingerprints: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut stale_iterations: usize = 0;
    let mut last_tool_result_text = String::new();

    while agent_turn_count <= max_agent_turns {
        agent_turn_count += 1;

        if agent_turn_count == max_agent_turns {
            conversation_messages.push(plain_message("user", &build_force_final_message()));
        }

        let (mut current_response, raw_response, turn_reasoning, turn_thought_signature) =
            run_stream(
                deps.streaming,
                &provider,
                base_url.as_deref(),
                &model,
                &conversation_messages,
                &effective_tools,
            )
            .await;

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
                let tool_data_reminder = if !last_tool_result_text.is_empty() {
                    format!(
                        "\n\nHere is the data you already received from your previous tool call:\n{last_tool_result_text}"
                    )
                } else {
                    String::new()
                };
                // Content-only assistant turn — we are NOT executing these calls.
                conversation_messages.push(plain_message("assistant", &current_response));
                conversation_messages.push(plain_message(
                    "user",
                    &format!(
                        "You have already gathered this data (a repeated call or repeated identical results). You already have what you need — do NOT call any more tools. Please call the submit_final_response tool NOW with your answer based on the data you already received.{tool_data_reminder}"
                    ),
                ));
                continue;
            }

            // Thread the assistant tool-call turn (paired with its native
            // tool_calls) so the model sees it already issued them.
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

            // Execute the tools — operator surface (character-less, all-stores).
            // Side effects stand; result MESSAGES are threaded in-memory only.
            let tool_context = ToolExecutionContext {
                chat_id: chat_id.to_string(),
                user_id: user_id.to_string(),
                operator_surface: true,
                pending_wardrobe_announcements: Arc::new(Mutex::new(
                    std::collections::HashSet::new(),
                )),
                ..Default::default()
            };
            let tool_result = process_tool_calls(
                &calls,
                &tool_context,
                &NoopSink,
                deps.tool_runner,
                Some(&status),
            )
            .await;

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

        // No tool calls or a final response — done.
        full_response = current_response;
        break;
    }

    // Models that output submit_final_response as JSON text.
    full_response = extract_submit_final_response_from_text(&full_response);

    let mut final_answer = js_trim(&full_response).to_string();

    // Budget-exhaustion salvage (Bug 47) — the mirror of the streaming
    // orchestrator's. The forced final turn runs no tools, so a model that answers
    // it with another native tool call instead of `submit_final_response` leaves
    // `full_response` empty. Rather than report a bare failure to Carina after
    // spending real budget, synthesise an explanatory answer from the last tool
    // result we captured. With no tool data at all there is genuinely nothing to
    // return, so fall through to the empty-response failure below.
    if final_answer.is_empty() && !last_tool_result_text.is_empty() {
        final_answer = format!(
            "I reached my {max_agent_turns}-turn budget before I could compose a final answer.\n\nHere is what I gathered before I stopped:\n\n{last_tool_result_text}"
        );
        tracing::warn!(
            chat_id = %chat_id,
            max_agent_turns,
            "Brahma one-shot exhausted its turn budget without a final response",
        );
    }

    if final_answer.is_empty() {
        return fail("empty response");
    }

    BrahmaConsoleResult {
        ok: true,
        answer: final_answer,
        detail: None,
    }
}

/// A role+content-only [`ThreadedMessage`] (no tool-call / reasoning fields).
pub(super) fn plain_message(role: &str, content: &str) -> ThreadedMessage {
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

// ===========================================================================
// The production impl of the frozen RunBrahmaConsole seam.
// ===========================================================================

/// The production Brahma console — holds the model boundaries + seams and
/// satisfies the frozen [`RunBrahmaConsole`] trait by running [`run_brahma_query`]
/// per call. Generic-consumed (no boxing); construct it at the Carina/spine
/// composition point and inject it where W4.5 injects `UnavailableBrahmaConsole`.
pub struct RealBrahmaConsole<STR, TR, TD> {
    db: Db,
    streaming: STR,
    tool_runner: TR,
    tool_detector: TD,
    model_supports_native_tools: bool,
}

impl<STR, TR, TD> RealBrahmaConsole<STR, TR, TD> {
    pub fn new(
        db: Db,
        streaming: STR,
        tool_runner: TR,
        tool_detector: TD,
        model_supports_native_tools: bool,
    ) -> Self {
        Self {
            db,
            streaming,
            tool_runner,
            tool_detector,
            model_supports_native_tools,
        }
    }
}

impl<STR, TR, TD> RunBrahmaConsole for RealBrahmaConsole<STR, TR, TD>
where
    STR: StreamingCompletionProvider + Sync,
    TR: ToolRunner + Sync,
    TD: ToolCallDetector + Sync,
{
    fn run(
        &self,
        user_id: &str,
        chat_id: &str,
        question: &str,
    ) -> impl Future<Output = BrahmaConsoleResult> + Send {
        let deps = BrahmaQueryDeps {
            db: &self.db,
            streaming: &self.streaming,
            tool_runner: &self.tool_runner,
            tool_detector: &self.tool_detector,
            model_supports_native_tools: self.model_supports_native_tools,
        };
        async move { run_brahma_query(&deps, user_id, chat_id, question).await }
    }
}

#[cfg(test)]
mod tests;
