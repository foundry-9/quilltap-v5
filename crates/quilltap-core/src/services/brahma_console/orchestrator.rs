//! The Brahma Console **orchestrator** (P4.9I1A) — v4
//! `lib/services/brahma-console/orchestrator.service.ts`
//! (`handleBrahmaConsoleMessage` + `processBrahmaResponse`), the interactive,
//! transcript-persisting multi-turn console the dedicated brahma-console
//! `[id]/messages` route drives. It is the SEPARATE engine from the already-ported
//! one-shot (`super::run_brahma_query`): the two share only
//! [`build_brahma_system_prompt`], the operator `run_sql`/doc tool surface, and
//! the two helpers ([`resolve_brahma_connection_profile`],
//! [`normalize_tool_call_signature`]) — the orchestrator does NOT import one-shot.
//!
//! It mirrors the Salon spine's streaming/agent-loop machinery but diverges:
//!
//!  - **No character.** One synthetic assistant voice (`characterName: 'Brahma
//!    Console'`, `characterId: ''`). No identity/wardrobe/scene/Concierge.
//!  - **No persistent memory.** The async tail fires the context-summary check
//!    (for auto-titling the past-chats list) — NEVER memory extraction. The only
//!    persistence is the chat transcript itself.
//!  - **Operator-scoped tools.** Search-without-memories + `doc_*` read/write
//!    reach every store the operator owns; read-only `run_sql`; web/curl from the
//!    profile. `operatorSurface: true`.
//!  - **Per-turn model resolution.** The connection profile is read from
//!    `chat.consoleConnectionProfileId` each send, so switching the model
//!    mid-conversation continues the same chat with the new engine.
//!
//! ## The v5 shape vs. v4's `ReadableStream`
//!
//! v4 returns a `ReadableStream` whose `start()` runs the loop and enqueues
//! SSE-encoded frames. v5 has no per-request SSE endpoint: the frames ride the
//! engine's global `Event` broadcast (an [`EventSink`], scope-tagged by `chatId`,
//! exactly as `ChatSend` does), and the function returns the typed
//! [`BrahmaSendResult`] as the dispatch reply. The seven v4 SSE frames map to
//! [`ChatEvent`] variants (`content`/`reasoning`/`toolsDetected`/`status`/
//! `toolResult`/`done`); v4's `encodeErrorEvent` frame is the transport-shell
//! [`crate::api::types::ChatErrorPayload`], which the HOST driver emits on `Err`
//! (the same split `ChatSend` uses — the free function returns the error and the
//! composing host maps it to both the error frame and the dispatch `Response`).
//!
//! ## The seams
//!
//! Generic-consumed over the streaming provider / tool runner / tool detector
//! (the `run_brahma_query` precedent) plus the [`CostTracker`] seam
//! (`estimateMessageCost` + the chat-aggregate half of `trackMessageTokenUsage`).
//! Persistence goes through the writer (`repos.chats.addMessage` /
//! `saveToolMessages`) — each write is its own short `Db::write`, never held
//! across the streaming await.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{api_keys, chats_messages_read, chats_read};
use crate::jsstr::js_trim;
use crate::model::stream::{StreamParams, StreamingCompletionProvider};
use crate::services::agent_mode::{
    build_agent_mode_instructions, build_force_final_message,
    extract_submit_final_response_from_text,
};
use crate::services::chat_events::{ChatEvent, DonePayload, DoneUsage, EventSink};
use crate::services::message_context::{build_conversation_messages, WhisperMessage};
use crate::services::message_finalizer::{CostTrackArgs, CostTracker};
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::pseudo_tool::{
    build_native_tool_system_instructions, build_text_block_system_instructions,
    check_should_use_text_block_tools, parse_text_blocks_from_response,
    strip_text_block_markers_from_response, TextBlockEnabledToolOptions,
};
use crate::services::tool_build::{build_tools, BuildToolsInput};
use crate::services::tool_call_threading::{
    build_assistant_tool_call_message, build_tool_result_messages, DetectedToolCall,
    ThreadedMessage,
};
use crate::services::tool_execution::{
    process_tool_calls, save_tool_messages, StatusContext, ToolCall, ToolExecutionContext,
    ToolRunner,
};
use crate::tools::pseudo_tool_support::ToolMode;

use super::{
    b, build_brahma_system_prompt, normalize_tool_call_signature, plain_message, requires_api_key,
    resolve_brahma_connection_profile, s, to_completion_messages, MAX_AGENT_TURNS,
    MAX_DUPLICATE_TOOL_CALLS,
};

// ===========================================================================
// Deps + result / error types.
// ===========================================================================

/// The model boundaries + seams the orchestrator composes for one send. Borrowed
/// for the life of one turn. The `cost` tracker is `&mut` (v4 `estimateMessageCost`
/// is a per-call effect; the chat-aggregate write follows it).
pub struct BrahmaSendDeps<'a, STR, TR, TD, COST>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    COST: CostTracker,
{
    pub db: &'a Db,
    /// The streaming model boundary (v4 `streamMessage`). The api-key resolution
    /// happens here for the early-failure behavior; the resolved value is unused
    /// (the host streaming provider resolves keys internally, as elsewhere).
    pub streaming: &'a STR,
    /// The tool executor boundary. The Brahma slate has NO `ask_carina`, so no
    /// recursion.
    pub tool_runner: &'a TR,
    /// v4 `detectToolCallsInResponse` — reads provider-native tool calls off the
    /// raw response.
    pub tool_detector: &'a TD,
    /// `estimateMessageCost` + the chat-aggregate half of `trackMessageTokenUsage`.
    pub cost: &'a mut COST,
    /// Injected `checkModelSupportsTools(provider, model, userId)` for the resolved
    /// profile (registry-seam pattern; feeds `build_tools` + the tool-mode gate).
    pub model_supports_native_tools: bool,
}

/// The typed dispatch reply of a Brahma send (v4's send has no JSON body — the
/// response is pure SSE — so the reply carries only the final assistant message
/// id for client correlation; `None` when the run produced no persisted answer).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrahmaSendResult {
    pub message_id: Option<String>,
}

/// A runtime send failure (v4's caught error → `encodeErrorEvent(message,
/// 'fatal_error', '')`). The composing host emits the transport-shell error frame
/// and maps this into the dispatch `Response` (the `ChatSend` split).
#[derive(Debug, Clone)]
pub struct BrahmaSendError {
    /// The human message (v4's `error.message`), carried into the error frame's
    /// `error` field.
    pub message: String,
}

impl BrahmaSendError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BrahmaSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for BrahmaSendError {}

/// The `BrahmaConsoleSendOptions` a send carries (v4 `types.ts`).
#[derive(Debug, Clone, Default)]
pub struct BrahmaConsoleSendOptions {
    pub content: String,
    /// v4 `fileIds?` — accepted but unused by the orchestrator (the console's
    /// user message carries no attachments; parity with v4, which threads them
    /// nowhere in the loop).
    pub file_ids: Vec<String>,
}

// ===========================================================================
// The entry point (v4 handleBrahmaConsoleMessage).
// ===========================================================================

/// Handle sending a message in a Brahma Console chat and stream the response
/// (v4 `handleBrahmaConsoleMessage`). Saves the user message, resolves the
/// chat's active connection profile, runs the multi-turn loop (emitting frames
/// on `sink`), then fires the async tail (context-summary only). Returns the
/// typed reply; a runtime failure is a [`BrahmaSendError`] the host maps to both
/// the transport-shell error frame and the dispatch `Response` (the `ChatSend`
/// split — see the module docs). The chat's owner/`brahma`-type gate is the
/// dispatch's `verify_brahma_chat`, upstream of here.
pub async fn handle_brahma_console_message<STR, TR, TD, COST>(
    deps: &mut BrahmaSendDeps<'_, STR, TR, TD, COST>,
    sink: &impl EventSink,
    user_id: &str,
    chat_id: &str,
    options: &BrahmaConsoleSendOptions,
) -> Result<BrahmaSendResult, BrahmaSendError>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    COST: CostTracker,
{
    let db = deps.db;

    // The chat carries the pinned console profile (v4 reads it off `chat`).
    let cid = chat_id.to_string();
    let chat = db
        .read_main(move |c| chats_read::find_by_id(c, &cid))
        .map_err(|e| BrahmaSendError::new(e.to_string()))?
        .ok_or_else(|| BrahmaSendError::new("Chat not found"))?;
    let console_profile_id = s(&chat, "consoleConnectionProfileId");

    // Save the user message (v4 `crypto.randomUUID()` id + `new Date()` stamp).
    let user_message = json!({
        "type": "message",
        "id": uuid::Uuid::new_v4().to_string(),
        "role": "USER",
        "content": options.content,
        "attachments": [],
        "createdAt": crate::clock::now_iso(),
    });
    persist_message(db, chat_id, user_message).await?;

    // Resolve the active connection profile (model).
    let profile = resolve_brahma_connection_profile(db, user_id, console_profile_id.as_deref())
        .ok_or_else(|| {
            BrahmaSendError::new("No connection profile available. Add a connection profile first.")
        })?;

    let result = process_brahma_response(deps, sink, user_id, chat_id, &profile).await?;

    // Async tail: the context-summary check (auto-titling of the past-chats
    // list) ONLY — memory extraction is NEVER fired (the console forms no
    // persistent memories). The DRIVE is a documented deferral (below).
    trigger_async_tasks_deferred(user_id, chat_id);

    Ok(result)
}

/// v4 `triggerAsyncTasks` — fire the context-summary check (for auto-titling)
/// after a Brahma turn. **Memory extraction is deliberately never called** (the
/// console forms no persistent memories — v4's load-bearing guarantee).
///
/// The context-summary DRIVE is a documented deferral, exactly as the production
/// chat finalizer defers its own context-summary drive
/// (`message_finalizer.rs`): both the finalizer and this tail reproduce v4's gate
/// but leave the cheap-LLM auto-title generation unwired (a display-only past-chats
/// nicety). No memory-extraction seam exists here to leak into — the guarantee is
/// satisfied by absence, not by a guard.
fn trigger_async_tasks_deferred(_user_id: &str, _chat_id: &str) {
    // DEFERRED: `triggerContextSummaryCheck` (auto-title generation). Wiring it
    // needs the same cheap-LLM context-summary seam the finalizer leaves deferred;
    // it lands with that wire, never before, and NEVER alongside a memory pass.
}

// ===========================================================================
// The multi-turn agent loop (v4 processBrahmaResponse).
// ===========================================================================

/// Run the character-less multi-turn agent loop, streaming frames on `sink` and
/// persisting the transcript (v4 `processBrahmaResponse`). Faithful to the
/// one-shot loop's stuck-guards, but this engine PERSISTS (assistant tool-call
/// turns, TOOL rows, the final answer) and EMITS (content/reasoning/tools/done).
async fn process_brahma_response<STR, TR, TD, COST>(
    deps: &mut BrahmaSendDeps<'_, STR, TR, TD, COST>,
    sink: &impl EventSink,
    user_id: &str,
    chat_id: &str,
    profile: &Value,
) -> Result<BrahmaSendResult, BrahmaSendError>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    COST: CostTracker,
{
    let db = deps.db;
    let provider = s(profile, "provider").unwrap_or_default();
    let model = s(profile, "modelName").unwrap_or_default();
    let base_url = s(profile, "baseUrl");
    let profile_id = s(profile, "id").unwrap_or_default();

    // Resolve the api key (providers that require one) — the early-failure
    // behavior; the resolved value is unused (the host streaming provider resolves
    // keys internally). v4's UNSCOPED `findApiKeyById`.
    if requires_api_key(&provider) {
        let key_id = s(profile, "apiKeyId")
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                BrahmaSendError::new("No API key configured for this connection profile")
            })?;
        let kid = key_id.clone();
        match db.read_main(move |c| api_keys::find_by_id(c, &kid)) {
            Ok(Some(_)) => {}
            _ => return Err(BrahmaSendError::new("API key not found")),
        }
    }

    // Build tools — the Brahma flag vector (agent ON, help OFF, doc read/write ON,
    // wardrobe/Carina/workspace OFF, search-without-memories, read-only run_sql).
    let provider_supports_web_search = crate::provider_manifest::Registry::built_in()
        .supports_capability(&provider, crate::provider_manifest::Capability::WebSearch);
    let no_disabled: [String; 0] = [];
    let no_groups: [String; 0] = [];
    let built = build_tools(
        db,
        user_id,
        &BuildToolsInput {
            provider: &provider,
            use_native_web_search: b(profile, "useNativeWebSearch").unwrap_or(false),
            allow_tool_use: b(profile, "allowToolUse"),
            allow_web_search: b(profile, "allowWebSearch").unwrap_or(false),
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
            custom_tool_context: None,
        },
    )
    .map_err(|e| BrahmaSendError::new(format!("{e:?}")))?;
    let tools = built.tools;
    let model_supports_native_tools = built.model_supports_native_tools;

    // Tool mode (native vs. text-block); simple-json COERCES to text-block.
    let effective_pseudo_tool_mode: Option<ToolMode> = match s(profile, "pseudoToolMode").as_deref()
    {
        Some("simple-json") => Some(ToolMode::TextBlock),
        Some(other) => ToolMode::from_str(other),
        None => Some(ToolMode::Auto),
    };
    let use_text_block_tools =
        check_should_use_text_block_tools(model_supports_native_tools, effective_pseudo_tool_mode);

    // Instructions (text-block or native) + always-appended agent-mode.
    let mut tool_instructions = String::new();
    if use_text_block_tools && !tools.is_empty() {
        // The console's hand-built text-block options: search + (maybe) web search
        // only; no workspace/wardrobe/help/rng/state/whisper tools.
        let opts = TextBlockEnabledToolOptions {
            image_generation: false,
            search: true,
            web_search: b(profile, "allowWebSearch").unwrap_or(false),
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
    let agent_instructions = build_agent_mode_instructions(MAX_AGENT_TURNS);
    tool_instructions = if tool_instructions.is_empty() {
        agent_instructions
    } else {
        format!("{tool_instructions}\n\n{agent_instructions}")
    };

    // The neutral, character-less system prompt (SQL section always on).
    let system_prompt = build_brahma_system_prompt(&tool_instructions, true);

    // Load the full transcript (the just-saved user message included) and thread
    // it through the SAME builder the Salon uses — a prior turn's TOOL row becomes
    // a `[Tool Result: …]` user message, so the model can follow that it already
    // ran a query (the console's loop-bug fix). Drop the empty assistant turns the
    // console persists per tool iteration (no text; the tool result already
    // follows), and lowercase USER/ASSISTANT roles to the provider wire form.
    let cid = chat_id.to_string();
    let raw_messages = db
        .read_main(move |c| chats_messages_read::get_messages(c, &cid))
        .map_err(|e| BrahmaSendError::new(e.to_string()))?;
    let whispers: Vec<WhisperMessage> = raw_messages
        .iter()
        .map(WhisperMessage::from_value)
        .collect();
    let (history, _) = build_conversation_messages(&whispers, false);

    let mut conversation_messages: Vec<ThreadedMessage> =
        vec![plain_message("system", &system_prompt)];
    for m in &history {
        if m.role == "ASSISTANT" && m.content.trim().is_empty() {
            continue;
        }
        conversation_messages.push(plain_message(&m.role.to_lowercase(), &m.content));
    }

    // Effective tools: native only when supported and not in text-block mode.
    let effective_tools: Vec<Value> = if !use_text_block_tools && model_supports_native_tools {
        tools.clone()
    } else {
        Vec::new()
    };

    let status = StatusContext {
        character_name: "Brahma Console".to_string(),
        character_id: String::new(),
    };

    // Loop state (faithful to v4 processBrahmaResponse).
    let mut agent_turn_count: i64 = 0;
    let mut full_response = String::new();
    let mut total_prompt_tokens: i64 = 0;
    let mut total_completion_tokens: i64 = 0;
    let mut tool_call_history: Vec<String> = Vec::new();
    let mut seen_result_fingerprints: HashSet<String> = HashSet::new();
    let mut stale_iterations: usize = 0;
    let mut last_tool_result_text = String::new();
    // Reasoning ("thinking") accumulators — DISPLAY ONLY.
    let mut prior_reasoning = String::new();
    let mut run_reasoning = String::new();

    while agent_turn_count <= MAX_AGENT_TURNS {
        agent_turn_count += 1;

        if agent_turn_count == MAX_AGENT_TURNS {
            conversation_messages.push(plain_message("user", &build_force_final_message()));
        }

        let turn = stream_turn(
            deps.streaming,
            sink,
            &provider,
            base_url.as_deref(),
            &model,
            &conversation_messages,
            &effective_tools,
            &prior_reasoning,
        )
        .await;
        let mut current_response = turn.content;
        let turn_reasoning = turn.turn_reasoning;
        run_reasoning = format!("{prior_reasoning}{turn_reasoning}");
        // Fold this turn's reasoning into the run-level chain (next turn appends).
        if !turn_reasoning.trim().is_empty() {
            prior_reasoning = format!("{run_reasoning}\n\n");
        }

        total_prompt_tokens += turn.usage.map(|u| u.prompt_tokens).unwrap_or(0);
        total_completion_tokens += turn.usage.map(|u| u.completion_tokens).unwrap_or(0);

        // Detect tool calls (native or text-block).
        let mut has_tool_calls = false;
        let mut tool_calls_to_process: Option<Vec<ToolCall>> = None;
        if model_supports_native_tools && !use_text_block_tools {
            if let Some(raw) = &turn.raw_response {
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
                    .map(str::to_string);
                if let Some(fc) = final_content {
                    if fc != current_response {
                        // v4 re-emits the extracted final content as a content chunk.
                        sink.emit(ChatEvent::content(fc.clone()));
                        current_response = fc;
                    }
                }
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

        if has_tool_calls && !is_submit_final && agent_turn_count < MAX_AGENT_TURNS {
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
                let tool_data_reminder = if last_tool_result_text.is_empty() {
                    String::new()
                } else {
                    format!(
                        "\n\nHere is the data you already received from your previous tool call:\n{last_tool_result_text}"
                    )
                };
                // Content-only assistant turn (no toolCalls) — we are NOT executing
                // these calls; attaching them would leave an unanswered tool-use
                // block that strict providers reject.
                conversation_messages.push(plain_message("assistant", &current_response));
                conversation_messages.push(plain_message(
                    "user",
                    &format!(
                        "You have already gathered this data (a repeated call or repeated identical results). You already have what you need — do NOT call any more tools. Please call the submit_final_response tool NOW with your answer based on the data you already received.{tool_data_reminder}"
                    ),
                ));
                continue;
            }

            // Persist the assistant message carrying the tool calls (content-only;
            // the console renders tool cards from the TOOL rows saved below). Per-turn
            // usage, not the running total.
            let assistant_message = json!({
                "type": "message",
                "id": uuid::Uuid::new_v4().to_string(),
                "role": "ASSISTANT",
                "content": current_response,
                "provider": provider,
                "modelName": model,
                "promptTokens": turn.usage.map(|u| u.prompt_tokens),
                "completionTokens": turn.usage.map(|u| u.completion_tokens),
                "tokenCount": turn.usage.map(|u| u.prompt_tokens + u.completion_tokens).unwrap_or(0),
                "attachments": [],
                "createdAt": crate::clock::now_iso(),
            });
            persist_message(db, chat_id, assistant_message).await?;

            // Thread the assistant tool-call turn (paired with its native tool_calls)
            // so the model sees it already issued them.
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
                turn.thought_signature.as_deref(),
            ));

            // Execute the tools — operator surface (character-less, all-stores).
            // `process_tool_calls` emits toolsDetected/status/toolResult on `sink`.
            let tool_context = ToolExecutionContext {
                chat_id: chat_id.to_string(),
                user_id: user_id.to_string(),
                operator_surface: true,
                pending_wardrobe_announcements: Arc::new(Mutex::new(HashSet::new())),
                ..Default::default()
            };
            let tool_result =
                process_tool_calls(&calls, &tool_context, sink, deps.tool_runner, Some(&status))
                    .await;

            if !tool_result.tool_messages.is_empty() {
                // Persist the TOOL rows (v4 saveToolMessages) — the console passes
                // '' for userId + no character/participant/whisper context.
                let cid = chat_id.to_string();
                let tool_messages = tool_result.tool_messages.clone();
                let generated = tool_result.generated_image_paths.clone();
                db.write(move |ws| {
                    save_tool_messages(
                        ws.main(),
                        &cid,
                        "",
                        &tool_messages,
                        &generated,
                        None,
                        None,
                        None,
                    )
                    .map(|_| ())
                })
                .await
                .map_err(|e| BrahmaSendError::new(e.to_string()))?;

                // Pair each result back to its call (native `tool` role + toolCallId,
                // or `[Tool Result: …]` user text when the provider has no call IDs).
                conversation_messages
                    .extend(build_tool_result_messages(&tool_result.tool_messages));

                // Stuck-loop tracking: an iteration is "stale" when every result it
                // produced was one we'd already seen.
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

    // Save the final assistant message + emit the done frame (v4's `if (fullResponse)`).
    let mut result = BrahmaSendResult::default();
    if !full_response.is_empty() {
        let reasoning_trimmed = js_trim(&run_reasoning).to_string();
        let reasoning_value: Option<String> = if reasoning_trimmed.is_empty() {
            None
        } else {
            Some(reasoning_trimmed.clone())
        };
        let message_id = uuid::Uuid::new_v4().to_string();
        let assistant_message = json!({
            "type": "message",
            "id": message_id,
            "role": "ASSISTANT",
            "content": full_response,
            "provider": provider,
            "modelName": model,
            "promptTokens": total_prompt_tokens,
            "completionTokens": total_completion_tokens,
            "tokenCount": total_prompt_tokens + total_completion_tokens,
            "attachments": [],
            "reasoningContent": reasoning_value,
            "createdAt": crate::clock::now_iso(),
        });
        persist_message(db, chat_id, assistant_message).await?;

        // estimateMessageCost → trackMessageTokenUsage (chat-aggregate half).
        let estimate = deps.cost.estimate(&CostTrackArgs {
            provider: provider.clone(),
            model_name: model.clone(),
            prompt_tokens: total_prompt_tokens,
            completion_tokens: total_completion_tokens,
            user_id: user_id.to_string(),
            profile_id: profile_id.clone(),
        });

        sink.emit(ChatEvent::done(DonePayload {
            message_id: Some(message_id.clone()),
            usage: Some(DoneUsage {
                prompt_tokens: Some(total_prompt_tokens),
                completion_tokens: Some(total_completion_tokens),
                total_tokens: None,
            }),
            cache_usage: None,
            attachment_results: None,
            tools_executed: agent_turn_count > 1,
            provider: Some(provider.clone()),
            model_name: Some(model.clone()),
            reasoning_content: match reasoning_value.clone() {
                Some(r) => crate::services::chat_events::Omittable::Value(r),
                None => crate::services::chat_events::Omittable::Null,
            },
            ..Default::default()
        }));

        let agg_chat_id = chat_id.to_string();
        let prompt = total_prompt_tokens as f64;
        let completion = total_completion_tokens as f64;
        let est_cost = estimate.cost;
        let est_source = estimate.source.clone();
        db.write(move |ws| {
            ws.main().chat_tokens().increment_token_aggregates(
                &agg_chat_id,
                prompt,
                completion,
                est_cost,
                est_source.as_deref(),
            )
        })
        .await
        .map_err(|e| BrahmaSendError::new(e.to_string()))?;

        result.message_id = Some(message_id);
    }

    Ok(result)
}

// ===========================================================================
// Helpers.
// ===========================================================================

/// The pieces one streamed agent turn yields.
struct StreamTurnResult {
    content: String,
    raw_response: Option<Value>,
    /// The turn's cumulative reasoning (last-wins; DISPLAY ONLY).
    turn_reasoning: String,
    thought_signature: Option<String>,
    /// The last usage chunk this turn (None when the provider sent none — v4's
    /// `streamUsage ?? null`).
    usage: Option<crate::model::stream::StreamUsage>,
}

/// Run one streamed LLM call, forwarding content + cumulative reasoning to `sink`
/// as they arrive (v4's per-chunk `encodeContentChunk` / `encodeReasoningChunk`).
#[allow(clippy::too_many_arguments)]
async fn stream_turn<STR: StreamingCompletionProvider>(
    streaming: &STR,
    sink: &impl EventSink,
    provider: &str,
    base_url: Option<&str>,
    model: &str,
    messages: &[ThreadedMessage],
    tools: &[Value],
    prior_reasoning: &str,
) -> StreamTurnResult {
    // v4: `tools.length > 0 ? tools : undefined`.
    let tools_value = if tools.is_empty() {
        None
    } else {
        Some(Value::Array(tools.to_vec()))
    };
    let params = StreamParams {
        messages: to_completion_messages(messages),
        model: model.to_string(),
        // v4 passes `modelParams: {}` — NO temperature.
        temperature: None,
        max_tokens: None,
        top_p: None,
        tools: tools_value,
        // v4 hardcodes `useNativeWebSearch: false`.
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
    };
    let mut rx = streaming.stream_message(provider, base_url, &params).await;
    let mut content = String::new();
    let mut raw: Option<Value> = None;
    let mut turn_reasoning = String::new();
    let mut thought_signature: Option<String> = None;
    let mut usage: Option<crate::model::stream::StreamUsage> = None;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            Ok(c) => {
                // v4: `if (chunk.reasoningContent && chunk.reasoningContent !== turnReasoning)`
                // — cumulative, last-wins; emit the growing run-level chain (client replaces).
                if let Some(rc) = &c.reasoning_content {
                    if !rc.is_empty() && *rc != turn_reasoning {
                        turn_reasoning = rc.clone();
                        sink.emit(ChatEvent::reasoning(format!(
                            "{prior_reasoning}{turn_reasoning}"
                        )));
                    }
                }
                if !c.content.is_empty() {
                    content.push_str(&c.content);
                    sink.emit(ChatEvent::content(c.content.clone()));
                }
                if let Some(u) = c.usage {
                    usage = Some(u);
                }
                if let Some(r) = c.raw_response {
                    raw = Some(r);
                }
                if let Some(ts) = c.thought_signature {
                    thought_signature = Some(ts);
                }
            }
            Err(_) => break,
        }
    }
    StreamTurnResult {
        content,
        raw_response: raw,
        turn_reasoning,
        thought_signature,
        usage,
    }
}

/// Persist one message event (v4 `repos.chats.addMessage`) as its own short write.
async fn persist_message(db: &Db, chat_id: &str, message: Value) -> Result<(), BrahmaSendError> {
    let event: crate::db::chats_messages::ChatEventInput = serde_json::from_value(message)
        .map_err(|e| BrahmaSendError::new(format!("brahma message marshal: {e}")))?;
    let cid = chat_id.to_string();
    db.write(move |ws| ws.main().chat_messages().add_message(&cid, &event))
        .await
        .map_err(|e| BrahmaSendError::new(e.to_string()))
}

#[cfg(test)]
mod tests;
