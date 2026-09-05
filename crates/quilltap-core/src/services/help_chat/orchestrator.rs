//! The help-chat **orchestrator** (P4.9I2A) — v4
//! `lib/services/help-chat/orchestrator.service.ts` (`handleHelpChatMessage` +
//! `processHelpResponse` + `triggerAsyncTasks`): the simplified send loop the
//! help-chats `[id]/messages` route drives. Every selected help character
//! answers in turn; it reuses the streaming, tool-execution and memory
//! machinery of the Salon but skips its complexity — no turn manager, whispers,
//! Concierge, scene state, story backgrounds, RNG or compression.
//!
//! It is the Brahma Console orchestrator's sibling (`services::brahma_console::
//! orchestrator`) and shares its seams (streaming provider / tool runner / tool
//! detector / `CostTracker`), but is a DIFFERENT engine, ported from its own v4
//! file, not from Brahma's:
//!
//!  - **Characters answer.** Each active `llm`-controlled participant gets a
//!    full agent loop of its own, with `turnStart` / `turnComplete` frames from
//!    the SECOND participant on and one `chainComplete` at the end (multi-
//!    character only). A per-participant failure is an `error` frame
//!    (`processing_error`) and the loop CONTINUES.
//!  - **A fixed 10-turn agent budget** (`maxAgentTurns = 10`, a lane-local
//!    constant — NOT Brahma's operator setting, NOT the chat setting).
//!  - **v4's older stuck guard**: the duplicate-call signature is the plain
//!    `JSON.stringify(calls.map(({name, arguments})))`, the threshold
//!    `MAX_DUPLICATE_TOOL_CALLS = 2`, the nudge sentence names the count — no
//!    stale-result fingerprinting, no budget-exhaustion salvage (those are v4's
//!    Brahma-only bug-47 additions).
//!  - **The async tail runs BOTH triggers** (v4 `triggerAsyncTasks`): the
//!    context-summary check AND per-turn memory extraction — help chats DO form
//!    memories (Brahma never does). Gated on `chatSettings.cheapLLMSettings`
//!    and the FIRST participant's profile resolving.
//!  - **History is the raw transcript**: every `message`-typed row forwarded as
//!    `{role: user|assistant|tool, content}` with SYSTEM rows DROPPED — so the
//!    `Help chat initiated…` / `[System: User navigated…]` rows are transcript-
//!    only, and a TOOL row rides as an id-less `tool` message.
//!  - **No reasoning frames**: v4's loop reads `chunk.content` and `usage` only.
//!
//! ## The v5 shape vs. v4's `ReadableStream`
//!
//! v4 returns a `ReadableStream` whose `start()` runs the loop and enqueues SSE
//! frames. v5 has no per-request SSE endpoint: the frames ride the engine's
//! global `Event` broadcast (an [`EventSink`], scope-tagged by `chatId`, exactly
//! as `ChatSend` and `BrahmaConsoleSend` do), and the function returns the typed
//! [`HelpSendResult`] as the dispatch reply. v4's top-level `fatal_error` frame is
//! the transport-shell [`crate::api::types::ChatErrorPayload`], which the HOST
//! driver emits on `Err` (the `ChatSend` split); the PER-PARTICIPANT
//! `processing_error` frame is emitted mid-stream by this loop as
//! [`ChatEvent::Error`] (added for this engine — v4's `encodeErrorEvent` bytes).
//!
//! ## Two recorded divergences at the provider seam
//!
//! 1. **Id-less `tool` history rows are dropped at the STREAM conversion.** v4
//!    hands `{role: 'tool', content}` (no `toolCallId`) to the plugin, and every
//!    plugin then drops it at format time (`qtap-plugin-anthropic`
//!    `formatMessagesWithAttachments`: `if (m.role === "tool" && !m.toolCallId)
//!    return false`; `openai-compatible` / `ollama` the same filter; the OpenAI
//!    Responses formatter logs `Skipping tool message without toolCallId`). v5's
//!    `StreamMessage::Tool` requires an id by construction, so the drop moves
//!    ONE layer up, to [`to_stream_messages`] here — the wire is identical. ⚠
//!    This means v4's help agent loop never feeds a tool RESULT back to the model
//!    on the next turn (the result row it pushes is id-less too): a **candidate
//!    upstream filing**, reproduced faithfully, recorded in the lane record.
//!    The tier-3 oracle's canned key is recorded over the same filtered list.
//! 2. **The async tail is awaited.** v4 fires `triggerAsyncTasks` without
//!    awaiting (`.catch`-warned promises); v5 awaits it before returning so the
//!    enqueue is durable when the dispatch reply lands. Same rows, later reply.

use std::future::Future;

use serde_json::{json, Value};

use crate::chat_predicates::{is_participant_present, participant_status_from_str};
use crate::db::runtime::Db;
use crate::db::{
    characters_read, chat_settings, chats_messages_read, chats_read, connection_profiles, users,
    DbError,
};
use crate::message_formatter::strip_character_name_prefix;
use crate::model::stream::{StreamMessage, StreamParams, StreamingCompletionProvider};
use crate::services::agent_mode::{
    build_agent_mode_instructions, build_force_final_message,
    extract_submit_final_response_from_text,
};
use crate::services::api_key_service::{self, ProfileApiKeyFailure, ProfileApiKeyResolution};
use crate::services::chat_events::{
    ChainCompletePayload, ChatEvent, DonePayload, DoneUsage, EventSink, TurnCompletePayload,
    TurnStartPayload,
};
use crate::services::message_finalizer::{CostTrackArgs, CostTracker};
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::pseudo_tool::{
    build_native_tool_system_instructions, build_text_block_system_instructions,
    check_should_use_text_block_tools, determine_text_block_tool_options,
    parse_text_blocks_from_response, strip_text_block_markers_from_response,
};
use crate::services::tool_build::{build_tools, BuildToolsInput};
use crate::services::tool_execution::{
    create_tool_context, process_tool_calls, save_tool_messages, StatusContext, ToolCall,
    ToolRunner,
};
use crate::tools::pseudo_tool_support::ToolMode;

use super::context_resolver::{resolve_all_help_content_for_url, HelpPageContext};
use super::system_prompt::{
    build_help_chat_system_prompt, HelpSystemPromptOptions, HelpUserCharacter,
};
use super::HelpDocument;

/// v4 `const maxAgentTurns = 10` (`orchestrator.service.ts:274`) — a lane-local
/// constant, NOT the Brahma operator setting and NOT the chat's agent setting.
pub const HELP_MAX_AGENT_TURNS: i64 = 10;
/// v4 `MAX_DUPLICATE_TOOL_CALLS = 2` (`:335`) — force a response after this many
/// identical calls.
pub const MAX_DUPLICATE_TOOL_CALLS: usize = 2;

// ===========================================================================
// Seams, deps, result / error types.
// ===========================================================================

/// v4 `triggerContextSummaryCheck` (`memory-trigger.service.ts:104-135`) →
/// `checkAndGenerateSummaryIfNeeded` — a cheap-LLM model call the core cannot
/// construct (it needs the completion + embedding providers and the logging
/// cheap executor). The host implements it; the tier-3 differential passes
/// [`NoHelpContextSummaryCheck`] on both sides (the v4 oracle mocks the trigger
/// to a no-op), so the memory-extraction enqueue is the tail's comparand.
pub trait HelpContextSummaryCheck {
    fn check(
        &self,
        db: &Db,
        user_id: &str,
        chat_id: &str,
        connection_profile: &Value,
        chat_settings: &Value,
        chat: &Value,
    ) -> impl Future<Output = Result<(), DbError>>;
}

/// The no-op summary check (the tier-3 venue; canned assemblies).
pub struct NoHelpContextSummaryCheck;
impl HelpContextSummaryCheck for NoHelpContextSummaryCheck {
    async fn check(
        &self,
        _db: &Db,
        _user_id: &str,
        _chat_id: &str,
        _connection_profile: &Value,
        _chat_settings: &Value,
        _chat: &Value,
    ) -> Result<(), DbError> {
        Ok(())
    }
}

/// The model boundaries + seams the orchestrator composes for one send.
pub struct HelpSendDeps<'a, STR, TR, TD, COST, SUM>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    COST: CostTracker,
    SUM: HelpContextSummaryCheck,
{
    pub db: &'a Db,
    /// The streaming model boundary (v4 `streamMessage`). The api-key resolution
    /// happens here for the early-failure behavior; the resolved value is unused
    /// (the host streaming provider resolves keys internally, as elsewhere).
    pub streaming: &'a STR,
    /// The tool executor boundary (v4 `processToolCalls` → the real handlers).
    pub tool_runner: &'a TR,
    /// v4 `detectToolCallsInResponse` — provider-native tool calls off the raw response.
    pub tool_detector: &'a TD,
    /// `estimateMessageCost` + the chat-aggregate half of `trackMessageTokenUsage`.
    pub cost: &'a mut COST,
    /// The async tail's context-summary check seam (see the trait).
    pub summary_check: &'a SUM,
    /// Injected `checkModelSupportsTools(provider, model, userId)`.
    pub model_supports_native_tools: bool,
}

/// v4 `HelpChatSendOptions` (`types.ts`).
#[derive(Debug, Clone, Default)]
pub struct HelpChatSendOptions {
    pub content: String,
    /// v4 `fileIds?` — accepted by the route and then IGNORED by the
    /// orchestrator (`:84-92` saves `attachments: []`). Pinned, not an
    /// attachment path.
    pub file_ids: Vec<String>,
}

/// The typed dispatch reply (§B): the id of the LAST persisted assistant message
/// across the participants, or `None` when no participant produced one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HelpSendResult {
    pub message_id: Option<String>,
}

/// A top-level send failure (v4's outer catch → `encodeErrorEvent(message,
/// 'fatal_error', '')`). The composing host emits the transport-shell error frame
/// and maps this into the dispatch `Response`.
#[derive(Debug, Clone)]
pub struct HelpSendError {
    pub message: String,
}
impl HelpSendError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
impl std::fmt::Display for HelpSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for HelpSendError {}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}
fn b(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}
/// JS truthiness over a JSON value.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(x)) => *x,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(Value::String(x)) => !x.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// One `{role, content}` message of the help loop's conversation (v4's local
/// `conversationMessages` shape — the loop never attaches native tool calls or
/// call ids; a tool result is a plain `tool`-role row).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpMessage {
    pub role: String,
    pub content: String,
}
fn msg(role: &str, content: impl Into<String>) -> HelpMessage {
    HelpMessage {
        role: role.to_string(),
        content: content.into(),
    }
}

/// The help slate → the provider seam's messages. `system`/`user`/`assistant`
/// map 1:1; an id-less `tool` row is DROPPED — the plugins' own filter, hoisted
/// one layer (module doc, divergence 1).
pub fn to_stream_messages(messages: &[HelpMessage]) -> Vec<StreamMessage> {
    messages
        .iter()
        .filter_map(|m| match m.role.as_str() {
            "system" => Some(StreamMessage::system(m.content.clone())),
            "assistant" => Some(StreamMessage::assistant(m.content.clone())),
            "tool" => None,
            _ => Some(StreamMessage::user(m.content.clone())),
        })
        .collect()
}

// ===========================================================================
// The entry point (v4 handleHelpChatMessage).
// ===========================================================================

/// v4 `handleHelpChatMessage(repos, chatId, userId, options)`. The route's
/// `verifyHelpChat` runs upstream (the dispatch arm); this function reproduces
/// the stream body's own three guards as top-level errors (`Chat not found` /
/// `Unauthorized` / `Not a help chat`).
pub async fn handle_help_chat_message<STR, TR, TD, COST, SUM>(
    deps: &mut HelpSendDeps<'_, STR, TR, TD, COST, SUM>,
    sink: &impl EventSink,
    user_id: &str,
    chat_id: &str,
    options: &HelpChatSendOptions,
) -> Result<HelpSendResult, HelpSendError>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    COST: CostTracker,
    SUM: HelpContextSummaryCheck,
{
    let db = deps.db;

    // Get chat metadata.
    let cid = chat_id.to_string();
    let chat = db
        .read_main(move |c| chats_read::find_by_id(c, &cid))
        .map_err(|e| HelpSendError::new(e.to_string()))?
        .ok_or_else(|| HelpSendError::new("Chat not found"))?;
    if s(&chat, "userId").as_deref() != Some(user_id) {
        return Err(HelpSendError::new("Unauthorized"));
    }
    if s(&chat, "chatType").as_deref() != Some("help") {
        return Err(HelpSendError::new("Not a help chat"));
    }

    // Save user message.
    persist_message(
        db,
        chat_id,
        json!({
            "type": "message",
            "id": uuid::Uuid::new_v4().to_string(),
            "role": "USER",
            "content": options.content,
            "attachments": [],
            "createdAt": crate::clock::now_iso(),
        }),
    )
    .await?;

    // Get active LLM-controlled participants sorted by displayOrder.
    let mut active: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            is_participant_present(participant_status_from_str(
                p.get("status").and_then(Value::as_str),
            )) && s(p, "controlledBy").as_deref() == Some("llm")
        })
        .collect();
    // `a.displayOrder - b.displayOrder` — a stable numeric sort.
    active.sort_by(|a, b| {
        let da = a.get("displayOrder").and_then(Value::as_f64).unwrap_or(0.0);
        let dbo = b.get("displayOrder").and_then(Value::as_f64).unwrap_or(0.0);
        da.partial_cmp(&dbo).unwrap_or(std::cmp::Ordering::Equal)
    });
    if active.is_empty() {
        return Err(HelpSendError::new("No active help characters in chat"));
    }
    let is_multi_character = active.len() > 1;

    // Resolve page context from the help docs (`chat.helpPageUrl || '/'`).
    let page_url = match chat.get("helpPageUrl") {
        Some(Value::String(u)) if !u.is_empty() => u.clone(),
        _ => "/".to_string(),
    };
    let docs = load_help_documents(db)?;
    let all_page_contexts = resolve_all_help_content_for_url(&page_url, &docs);
    let (primary_context, additional_contexts): (Option<&HelpPageContext>, &[HelpPageContext]) =
        match all_page_contexts.split_first() {
            Some((first, rest)) => (Some(first), rest),
            None => (None, &[]),
        };

    // Process each participant sequentially.
    let mut last_message_id: Option<String> = None;
    for (i, participant) in active.iter().enumerate() {
        let participant_id = s(participant, "id").unwrap_or_default();
        if is_multi_character && i > 0 {
            // Send turn start event for subsequent characters.
            let char_id = s(participant, "characterId").unwrap_or_default();
            let char_name = read_character(db, &char_id)
                .ok()
                .flatten()
                .and_then(|c| s(&c, "name"))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Unknown".to_string());
            sink.emit(ChatEvent::turn_start(TurnStartPayload {
                participant_id: participant_id.clone(),
                character_name: char_name,
                chain_depth: i as i64,
            }));
        }

        match process_help_response(
            deps,
            sink,
            user_id,
            chat_id,
            participant,
            primary_context,
            additional_contexts,
            &active,
        )
        .await
        {
            Ok(message_id) => {
                if message_id.is_some() {
                    last_message_id = message_id.clone();
                }
                if is_multi_character && i > 0 {
                    sink.emit(ChatEvent::turn_complete(TurnCompletePayload {
                        participant_id: participant_id.clone(),
                        // v4 `messageId || ''`.
                        message_id: message_id.unwrap_or_default(),
                        chain_depth: i as i64,
                        // v4's help frame carries no `skipped` key.
                        skipped: None,
                    }));
                }
            }
            Err(e) => {
                tracing::error!(
                    target: "quilltap::help",
                    chat_id = %chat_id,
                    participant_id = %participant_id,
                    error = %e.message,
                    "Error processing help response for participant"
                );
                sink.emit(ChatEvent::error(e.message, "processing_error", ""));
            }
        }
    }

    if is_multi_character {
        sink.emit(ChatEvent::chain_complete(ChainCompletePayload {
            reason: "cycle_complete".to_string(),
            next_speaker_id: None,
            chain_depth: (active.len() - 1) as i64,
        }));
    }

    // Trigger async background tasks (awaited here — divergence 2).
    trigger_async_tasks(deps, user_id, chat_id, &chat, &active[0]).await;

    Ok(HelpSendResult {
        message_id: last_message_id,
    })
}

/// The overlaid character read (`repos.characters.findById`).
fn read_character(db: &Db, character_id: &str) -> Result<Option<Value>, DbError> {
    let cid = character_id.to_string();
    db.read_main(|main| db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid)))
}

/// v4 `getHelpSearch().listDocuments()` / `getDocument()` — the table, per call.
fn load_help_documents(db: &Db) -> Result<Vec<HelpDocument>, HelpSendError> {
    let rows = db
        .read_main(|c| crate::db::help_docs::HelpDocsRepository::new(c).find_all())
        .map_err(|e| HelpSendError::new(e.to_string()))?;
    Ok(rows.iter().map(HelpDocument::from_row).collect())
}

// ===========================================================================
// One character's response (v4 processHelpResponse).
// ===========================================================================

/// v4 `processHelpResponse`: one help character's agent-mode loop. Returns the
/// final assistant message id (or `None` when the loop produced no text).
#[allow(clippy::too_many_arguments)]
async fn process_help_response<STR, TR, TD, COST, SUM>(
    deps: &mut HelpSendDeps<'_, STR, TR, TD, COST, SUM>,
    sink: &impl EventSink,
    user_id: &str,
    chat_id: &str,
    participant: &Value,
    primary_context: Option<&HelpPageContext>,
    additional_contexts: &[HelpPageContext],
    all_participants: &[Value],
) -> Result<Option<String>, HelpSendError>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    COST: CostTracker,
    SUM: HelpContextSummaryCheck,
{
    let db = deps.db;
    let participant_id = s(participant, "id").unwrap_or_default();

    // Load character data.
    let character_id = s(participant, "characterId").unwrap_or_default();
    let character = read_character(db, &character_id)
        .map_err(|e| HelpSendError::new(e.to_string()))?
        .ok_or_else(|| HelpSendError::new("Character not found"))?;
    let character_name = s(&character, "name").unwrap_or_default();

    // Load connection profile (`if (!participant.connectionProfileId) throw`).
    let profile_id = match participant.get("connectionProfileId") {
        Some(Value::String(p)) if !p.is_empty() => p.clone(),
        _ => {
            return Err(HelpSendError::new(
                "No connection profile for help character",
            ))
        }
    };
    let pid = profile_id.clone();
    let profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .map_err(|e| HelpSendError::new(e.to_string()))?
        .ok_or_else(|| HelpSendError::new("Connection profile not found"))?;
    let provider = s(&profile, "provider").unwrap_or_default();
    let model = s(&profile, "modelName").unwrap_or_default();
    let base_url = s(&profile, "baseUrl");

    // Get API key — required for a hosted provider, optional but still forwarded
    // for one that merely accepts a key (bug 81). The resolved value is unused
    // (the host streaming provider resolves keys internally); this is the
    // early-failure gate with `describeProfileApiKeyFailure`'s sentences.
    let key_id = s(&profile, "apiKeyId");
    let provider_for_key = provider.clone();
    let resolution = db
        .read_main(move |c| {
            Ok::<_, DbError>(api_key_service::resolve_connection_profile_api_key(
                c,
                &provider_for_key,
                key_id.as_deref(),
            ))
        })
        .unwrap_or(ProfileApiKeyResolution::Failed(
            ProfileApiKeyFailure::ApiKeyNotFound,
        ));
    if let ProfileApiKeyResolution::Failed(reason) = resolution {
        return Err(HelpSendError::new(reason.describe()));
    }

    // Get user character identity (from the user profile):
    // `userSettings ? { name: userSettings.name || 'User', description: '' } : null`.
    let uid = user_id.to_string();
    let user_character: Option<HelpUserCharacter> = db
        .read_main(move |c| users::find_name_by_id(c, &uid))
        .map_err(|e| HelpSendError::new(e.to_string()))?
        .map(|name| HelpUserCharacter {
            name: name
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "User".to_string()),
            description: String::new(),
        });

    // Other character names for multi-character context (misses skipped).
    let mut other_character_names: Vec<String> = Vec::new();
    for p in all_participants {
        if s(p, "id") != Some(participant_id.clone()) {
            let ocid = s(p, "characterId").unwrap_or_default();
            if let Ok(Some(other)) = read_character(db, &ocid) {
                if let Some(n) = s(&other, "name") {
                    other_character_names.push(n);
                }
            }
        }
    }
    let is_multi_character = all_participants.len() > 1;

    // Build tools — v4 `buildTools(profile, null, null, userId, null, false, [],
    // [], true, isMultiCharacter, true)`: agent mode ON, help tools ON, no image
    // profile / project / compression, nothing disabled; the unnamed trailing
    // params take their `!== false` defaults (wardrobe ON, workspace ON) and
    // their falsy defaults (document editing / carina / sql / memory-exclusion OFF).
    let provider_supports_web_search = crate::provider_manifest::Registry::built_in()
        .supports_capability(&provider, crate::provider_manifest::Capability::WebSearch);
    let no_disabled: [String; 0] = [];
    let no_groups: [String; 0] = [];
    let built = build_tools(
        db,
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
            is_multi_character,
            help_tools_enabled: true,
            can_dress_themselves: true,
            can_create_outfits: true,
            document_editing_enabled: false,
            ask_carina_enabled: false,
            include_workspace_tools: true,
            exclude_memory_search: false,
            sql_access: false,
            model_supports_native_tools: deps.model_supports_native_tools,
            provider_supports_web_search,
            custom_tool_context: None,
        },
    )
    .map_err(|e| HelpSendError::new(format!("{e:?}")))?;
    let tools = built.tools;
    let model_supports_native_tools = built.model_supports_native_tools;

    // Tool mode (native vs pseudo-tool). Help-chat does not implement the
    // simple-json continuation loop, so anything resolving to simple-json is
    // downgraded to text-block here (v4 `:250-254`).
    let effective_pseudo_tool_mode: Option<ToolMode> =
        match s(&profile, "pseudoToolMode").as_deref() {
            Some("simple-json") => Some(ToolMode::TextBlock),
            Some(other) => ToolMode::from_str(other),
            None => Some(ToolMode::Auto),
        };
    let use_text_block_tools =
        check_should_use_text_block_tools(model_supports_native_tools, effective_pseudo_tool_mode);

    // Tool instructions: text-block (REGARDLESS of the tool count — v4 `:258`),
    // else native when tools exist.
    let mut tool_instructions = String::new();
    if use_text_block_tools {
        let opts = determine_text_block_tool_options(
            None,  // imageProfileId
            false, // allowWebSearch
            is_multi_character,
            false,       // hasProject
            Some(true),  // helpToolsEnabled
            Some(false), // canDressThemselves — not applicable for help chats
            Some(false), // canCreateOutfits — not applicable for help chats
        );
        tool_instructions = build_text_block_system_instructions(&opts);
    } else if !tools.is_empty() {
        tool_instructions = build_native_tool_system_instructions();
    }
    // Agent mode instructions (always enabled for help chats; the fixed budget).
    let agent_instructions = build_agent_mode_instructions(HELP_MAX_AGENT_TURNS);
    tool_instructions = if tool_instructions.is_empty() {
        agent_instructions
    } else {
        format!("{tool_instructions}\n\n{agent_instructions}")
    };

    // Build the help-specific system prompt.
    let system_prompt = build_help_chat_system_prompt(&HelpSystemPromptOptions {
        character: &character,
        user_character: user_character.as_ref(),
        page_context: primary_context,
        additional_page_contexts: additional_contexts,
        other_character_names: if other_character_names.is_empty() {
            None
        } else {
            Some(&other_character_names)
        },
        tool_instructions: Some(&tool_instructions),
    });

    // Messages for context — simplified history, no compression: every
    // `message`-typed row as `{role, content}`, SYSTEM rows dropped.
    let cid = chat_id.to_string();
    let history = db
        .read_main(move |c| chats_messages_read::get_messages(c, &cid))
        .map_err(|e| HelpSendError::new(e.to_string()))?;
    let mut conversation: Vec<HelpMessage> = vec![msg("system", system_prompt)];
    for m in &history {
        if m.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let content = s(m, "content").unwrap_or_default();
        match m.get("role").and_then(Value::as_str) {
            Some("USER") => conversation.push(msg("user", content)),
            Some("ASSISTANT") => conversation.push(msg("assistant", content)),
            Some("TOOL") => conversation.push(msg("tool", content)),
            _ => {}
        }
    }

    // Effective tools: native only when supported and not in text-block mode.
    let effective_tools: Vec<Value> = if !use_text_block_tools && model_supports_native_tools {
        tools.clone()
    } else {
        Vec::new()
    };
    let cache_key = crate::cheap_llm::build_character_cache_key(Some(&character_id));
    let status = StatusContext {
        character_name: character_name.clone(),
        character_id: character_id.clone(),
    };

    // Agent mode loop (faithful to v4 processHelpResponse).
    let mut agent_turn_count: i64 = 0;
    let mut full_response = String::new();
    let mut total_prompt_tokens: i64 = 0;
    let mut total_completion_tokens: i64 = 0;
    let mut tool_call_history: Vec<String> = Vec::new();

    while agent_turn_count <= HELP_MAX_AGENT_TURNS {
        agent_turn_count += 1;

        // Force final response if at turn limit.
        if agent_turn_count == HELP_MAX_AGENT_TURNS {
            conversation.push(msg("user", build_force_final_message()));
        }

        // Stream the response.
        let turn = stream_turn(
            deps.streaming,
            sink,
            &provider,
            base_url.as_deref(),
            &model,
            &conversation,
            &effective_tools,
            cache_key.clone(),
        )
        .await;
        let mut current_response = turn.content;
        total_prompt_tokens += turn.usage.map(|u| u.prompt_tokens).unwrap_or(0);
        total_completion_tokens += turn.usage.map(|u| u.completion_tokens).unwrap_or(0);

        // Check for tool calls.
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

        // Check for submit_final_response (agent mode completion).
        let mut is_submit_final = tool_calls_to_process
            .as_ref()
            .map(|calls| calls.iter().any(|tc| tc.name == "submit_final_response"))
            .unwrap_or(false);
        if is_submit_final {
            if let Some(calls) = &tool_calls_to_process {
                let submit = calls.iter().find(|tc| tc.name == "submit_final_response");
                // `(submitCall?.arguments?.response as string) || currentResponse`.
                let final_content = submit
                    .and_then(|c| c.arguments.get("response"))
                    .and_then(Value::as_str)
                    .filter(|x| !x.is_empty())
                    .map(str::to_string);
                if let Some(fc) = final_content {
                    if fc != current_response {
                        // The LLM put the response in the tool call — send it as content.
                        sink.emit(ChatEvent::content(fc.clone()));
                        current_response = fc;
                    }
                }
                full_response = current_response.clone();
                has_tool_calls = false; // Don't process submit_final_response as a tool.
            }
        }

        // Fallback: some models output submit_final_response as JSON text.
        if !is_submit_final && !has_tool_calls {
            let extracted = extract_submit_final_response_from_text(&current_response);
            if extracted != current_response {
                is_submit_final = true;
                current_response = extracted.clone();
                full_response = extracted;
                has_tool_calls = false;
            }
        }

        if has_tool_calls && !is_submit_final && agent_turn_count < HELP_MAX_AGENT_TURNS {
            let calls = tool_calls_to_process.expect("has_tool_calls implies Some");
            // Detect repeated identical tool calls (stuck agent loop):
            // `JSON.stringify(calls.map(tc => ({ name, arguments })))`.
            let call_signature = serde_json::to_string(
                &calls
                    .iter()
                    .map(|tc| json!({ "name": tc.name, "arguments": tc.arguments }))
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_default();
            let duplicate_count = tool_call_history
                .iter()
                .filter(|sig| **sig == call_signature)
                .count();
            tool_call_history.push(call_signature);

            if duplicate_count >= MAX_DUPLICATE_TOOL_CALLS {
                tracing::warn!(
                    target: "quilltap::help",
                    chat_id = %chat_id,
                    character_name = %character_name,
                    turn = agent_turn_count,
                    duplicate_count = duplicate_count + 1,
                    tool_names = ?calls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
                    "Agent stuck in tool call loop, forcing final response"
                );
                // The most recent tool result in the conversation, echoed back.
                let tool_data_reminder = conversation
                    .iter()
                    .rev()
                    .find(|m| m.role == "tool")
                    .map(|m| {
                        format!(
                            "\n\nHere is the data you already received from your previous tool call:\n{}",
                            m.content
                        )
                    })
                    .unwrap_or_default();
                conversation.push(msg("assistant", current_response.clone()));
                conversation.push(msg(
                    "user",
                    format!(
                        "You have already called the same tool with the same arguments {} times and received the same result each time. You already have all the data you need — do NOT call any more tools. Please call the submit_final_response tool NOW with your answer based on the data you already received. Read the tool results carefully and include the actual data in your response.{tool_data_reminder}",
                        duplicate_count + 1
                    ),
                ));
                continue;
            }

            // Save assistant message with tool calls (per-turn usage).
            persist_message(
                db,
                chat_id,
                json!({
                    "type": "message",
                    "id": uuid::Uuid::new_v4().to_string(),
                    "role": "ASSISTANT",
                    "content": current_response,
                    "participantId": participant_id,
                    "provider": provider,
                    "modelName": model,
                    "promptTokens": turn.usage.map(|u| u.prompt_tokens),
                    "completionTokens": turn.usage.map(|u| u.completion_tokens),
                    "tokenCount": turn.usage.map(|u| u.prompt_tokens + u.completion_tokens).unwrap_or(0),
                    "attachments": [],
                    "createdAt": crate::clock::now_iso(),
                }),
            )
            .await?;
            conversation.push(msg("assistant", current_response.clone()));

            // Execute tools (per-tool status frames are emitted inside).
            let tool_context = create_tool_context(
                chat_id,
                user_id,
                character_id.clone(),
                participant_id.clone(),
                None, // imageProfileId
                None, // embeddingProfileId
                None, // projectId
                None,
                None,
            );
            let tool_result =
                process_tool_calls(&calls, &tool_context, sink, deps.tool_runner, Some(&status))
                    .await;

            // Save tool messages (v4 `saveToolMessages(repos, chatId, userId,
            // toolMessages, generatedImagePaths, character.id)`).
            if !tool_result.tool_messages.is_empty() {
                let cid = chat_id.to_string();
                let uid = user_id.to_string();
                let char_for_save = character_id.clone();
                let tool_messages = tool_result.tool_messages.clone();
                let generated = tool_result.generated_image_paths.clone();
                db.write(move |ws| {
                    save_tool_messages(
                        ws.main(),
                        &cid,
                        &uid,
                        &tool_messages,
                        &generated,
                        Some(&char_for_save),
                        None,
                        None,
                    )
                    .map(|_| ())
                })
                .await
                .map_err(|e| HelpSendError::new(e.to_string()))?;

                // Add tool results to the conversation for the next iteration:
                // `JSON.stringify({ tool, success, result })`.
                for tm in &tool_result.tool_messages {
                    conversation.push(msg(
                        "tool",
                        json!({ "tool": tm.tool_name, "success": tm.success, "result": tm.content })
                            .to_string(),
                    ));
                }
            }

            continue;
        }

        // No tool calls or a final response — done.
        full_response = current_response;
        break;
    }

    // Clean up response — strip the character name prefix if present.
    full_response = strip_character_name_prefix(&full_response, Some(&character_name), None);
    // Models that output submit_final_response as JSON text.
    full_response = extract_submit_final_response_from_text(&full_response);

    // Save the final assistant message (v4 `if (fullResponse)`).
    if full_response.is_empty() {
        return Ok(None);
    }
    let message_id = uuid::Uuid::new_v4().to_string();
    persist_message(
        db,
        chat_id,
        json!({
            "type": "message",
            "id": message_id,
            "role": "ASSISTANT",
            "content": full_response,
            "participantId": participant_id,
            "provider": provider,
            "modelName": model,
            "promptTokens": total_prompt_tokens,
            "completionTokens": total_completion_tokens,
            "tokenCount": total_prompt_tokens + total_completion_tokens,
            "attachments": [],
            "createdAt": crate::clock::now_iso(),
        }),
    )
    .await?;

    // Estimate cost for token tracking.
    let estimate = deps.cost.estimate(&CostTrackArgs {
        provider: provider.clone(),
        model_name: model.clone(),
        prompt_tokens: total_prompt_tokens,
        completion_tokens: total_completion_tokens,
        user_id: user_id.to_string(),
        profile_id: profile_id.clone(),
    });

    // Send done event (v4: messageId, participantId, usage, cacheUsage: null,
    // attachmentResults: null, toolsExecuted — NO provider/modelName).
    sink.emit(ChatEvent::done(DonePayload {
        message_id: Some(message_id.clone()),
        participant_id: Some(participant_id.clone()),
        usage: Some(DoneUsage {
            prompt_tokens: Some(total_prompt_tokens),
            completion_tokens: Some(total_completion_tokens),
            total_tokens: None,
        }),
        cache_usage: None,
        attachment_results: None,
        tools_executed: agent_turn_count > 1,
        ..Default::default()
    }));

    // Track token usage (the chat-aggregate half of `trackMessageTokenUsage`).
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
    .map_err(|e| HelpSendError::new(e.to_string()))?;

    Ok(Some(message_id))
}

// ===========================================================================
// The async tail (v4 triggerAsyncTasks).
// ===========================================================================

/// v4 `triggerAsyncTasks`: gated on `chatSettings.cheapLLMSettings` (JS
/// truthiness) and the FIRST participant's profile resolving; then BOTH the
/// context-summary check (the seam) and per-turn memory extraction (the Salon's
/// own `trigger_turn_memory_extraction` — help chats DO form memories). Each
/// leg warns and continues (v4's `.catch`es); the whole thing warns on failure.
async fn trigger_async_tasks<STR, TR, TD, COST, SUM>(
    deps: &HelpSendDeps<'_, STR, TR, TD, COST, SUM>,
    user_id: &str,
    chat_id: &str,
    chat: &Value,
    first_participant: &Value,
) where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    COST: CostTracker,
    SUM: HelpContextSummaryCheck,
{
    let db = deps.db;
    let uid = user_id.to_string();
    let settings = match db.read_main(move |c| chat_settings::find_by_user_id(c, &uid)) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(target: "quilltap::help", chat_id = %chat_id, error = %e, "Failed to trigger async tasks");
            return;
        }
    };
    let Some(settings) = settings else { return };
    if !truthy(settings.get("cheapLLMSettings")) {
        return;
    }
    let Some(Value::String(profile_id)) = first_participant.get("connectionProfileId") else {
        return;
    };
    if profile_id.is_empty() {
        return;
    }
    let pid = profile_id.clone();
    let profile = match db.read_main(move |c| connection_profiles::find_by_id(c, &pid)) {
        Ok(Some(p)) => p,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(target: "quilltap::help", chat_id = %chat_id, error = %e, "Failed to trigger async tasks");
            return;
        }
    };

    // Context summary check (for auto re-titling).
    if let Err(e) = deps
        .summary_check
        .check(db, user_id, chat_id, &profile, &settings, chat)
        .await
    {
        tracing::warn!(target: "quilltap::help", chat_id = %chat_id, error = %e, "Failed to trigger context summary check");
    }

    // Per-turn memory extraction. The trigger reads chat state itself to
    // resolve the turn opener.
    if let Err(e) = crate::services::message_finalizer::trigger_turn_memory_extraction(
        db, chat_id, user_id, profile_id,
    )
    .await
    {
        tracing::warn!(target: "quilltap::help", chat_id = %chat_id, error = %e, "Failed to trigger memory extraction");
    }
}

// ===========================================================================
// Helpers.
// ===========================================================================

struct StreamTurnResult {
    content: String,
    raw_response: Option<Value>,
    usage: Option<crate::model::stream::StreamUsage>,
}

/// One streamed LLM call, forwarding content chunks to `sink` (v4's per-chunk
/// `encodeContentChunk`). v4's help loop reads `content`, `usage` and
/// `rawResponse` only — reasoning chunks are ignored.
#[allow(clippy::too_many_arguments)]
async fn stream_turn<STR: StreamingCompletionProvider>(
    streaming: &STR,
    sink: &impl EventSink,
    provider: &str,
    base_url: Option<&str>,
    model: &str,
    messages: &[HelpMessage],
    tools: &[Value],
    cache_key: Option<String>,
) -> StreamTurnResult {
    let params = StreamParams {
        messages: to_stream_messages(messages),
        model: model.to_string(),
        // v4 passes `modelParams: {}` — no sampling overrides.
        temperature: None,
        max_tokens: None,
        top_p: None,
        // v4: `tools.length > 0 ? tools : undefined`.
        tools: if tools.is_empty() {
            None
        } else {
            Some(Value::Array(tools.to_vec()))
        },
        // v4 hardcodes `useNativeWebSearch: false`.
        web_search_enabled: false,
        profile_parameters: None,
        // v4 `buildCharacterCacheKey(characterId)`.
        cache_key,
        previous_response_id: None,
        stop: Vec::new(),
        request_timeout_ms: None,
    };
    let mut rx = streaming.stream_message(provider, base_url, &params).await;
    let mut content = String::new();
    let mut raw: Option<Value> = None;
    let mut usage: Option<crate::model::stream::StreamUsage> = None;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            Ok(c) => {
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
            }
            Err(_) => break,
        }
    }
    StreamTurnResult {
        content,
        raw_response: raw,
        usage,
    }
}

/// Persist one message event (v4 `repos.chats.addMessage`) as its own short write.
async fn persist_message(db: &Db, chat_id: &str, message: Value) -> Result<(), HelpSendError> {
    let event: crate::db::chats_messages::ChatEventInput = serde_json::from_value(message)
        .map_err(|e| HelpSendError::new(format!("help message marshal: {e}")))?;
    let cid = chat_id.to_string();
    db.write(move |ws| ws.main().chat_messages().add_message(&cid, &event))
        .await
        .map_err(|e| HelpSendError::new(e.to_string()))
}
