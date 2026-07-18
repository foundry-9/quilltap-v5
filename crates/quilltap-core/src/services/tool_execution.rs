//! Tool execution + persistence primitives — port of v4
//! `lib/services/chat-message/tool-execution.service.ts`.
//!
//! This is the execution *harness* and the persistence primitive that sit between
//! the tool loops (W4.1e/f, later) and the tool handlers (W4.1d, later):
//!
//!   - [`process_tool_calls`] — the per-call dispatch harness: emit the
//!     detection frame + a per-tool `tool_executing` status + the tool-result
//!     frame, dispatch through the injected [`ToolRunner`] boundary, build the
//!     [`ToolMessage`] slate, and extract generated-image descriptors.
//!   - [`save_tool_messages`] — the TOOL-row persistence primitive: one
//!     `type:'message'` / `role:'TOOL'` row per tool message (through the ported
//!     `chats_messages::add_message` path, so a TOOL row bumps the chat's minted
//!     `lastMessageAt`/`updatedAt` but is excluded from `messageCount` and never
//!     touches `spokenThisCycle`), with the whisper gate + generated-image
//!     link/tag loop.
//!   - [`compute_tool_message_targets`] — the whisper gate.
//!   - [`create_tool_context`] — the [`ToolExecutionContext`] data bag.
//!
//! ## The two injected boundaries
//!
//! Following W4.1a's inline-real style, the only seams here are the two genuine
//! external boundaries:
//!
//!   - [`ToolRunner`] — v4's `executeToolCallWithContext` + every real handler
//!     (`lib/chat/tool-executor.ts`). That whole dispatch is W4.1d; here both the
//!     differential's oracle and the Rust side inject the SAME canned per-call
//!     results ([`CannedToolRunner`]). W4.1d later provides the real
//!     implementation behind this boundary.
//!   - `detectToolCallsInResponse` — provider wire parsing, deferred to W4.7.
//!     `process_tool_calls` takes an already-detected `&[ToolCall]`, so no
//!     detector seam is needed at this layer (the detection frame it emits is
//!     built from the calls it is handed).
//!
//! ## The `ToolExecutionContext` callback slots
//!
//! v4's `ToolExecutionContext` carries a callback member reaching into a subsystem
//! not yet ported: `emitCarinaAnswer` (a live SSE callback owned by the
//! orchestrator — the Carina query engine, wave-4). It is not modeled until its
//! consumer lands. `loadedMemories` is now typed ([`LoadedMemoriesContext`], W4.1d)
//! since its consumer `self_inventory` is ported. `create_tool_context` reproduces
//! v4's `createToolContext`, which leaves the carina callback absent.
//!
//! ## Two content shapes — do NOT unify with the RNG one
//!
//! The TOOL-row `content` JSON here is the GENERIC shape (`toolName, success,
//! result, arguments, callId, [anchorOffset], [seq], provider, model, prompt` — v4
//! order). It is deliberately DIFFERENT from W4.1a's auto-detect RNG content
//! (`orchestrator::RngToolMessageContent`: `{tool, initiatedBy, success, result,
//! prompt, arguments}`). v4 itself writes two different TOOL-row shapes on the two
//! paths; both are byte-faithful and must stay separate.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::chat_events::{ChatEvent, EventSink, StatusPayload, ToolResultPayload};
use crate::db::{js_number_to_json, DbError, Writer};

// ===========================================================================
// The whisper-gate tool-name sets (tool-execution.service.ts:19–36)
// ===========================================================================

/// Tools whose results are inherently per-character (memories, conversation
/// transcripts). Whispered to the calling character regardless of chat settings.
const ALWAYS_PRIVATE_TOOLS: &[&str] = &["search", "read_conversation"];

/// Vault-read tools. Whispered UNLESS the chat has `allowCrossCharacterVaultReads`
/// enabled (the operator's "characters share each other's vaults" mode).
const VAULT_READ_TOOLS: &[&str] = &[
    "doc_read_file",
    "doc_list_files",
    "doc_grep",
    "doc_read_heading",
    "doc_read_frontmatter",
    "doc_read_blob",
    "doc_list_blobs",
    "doc_open_document",
];

/// Tools whose visible artifact is another message, so the TOOL row is persisted
/// for threading and the model but rendered by nobody (v4
/// `tool-execution.service.ts:42` `DELEGATED_DISPLAY_TOOLS`). `run_custom`'s
/// outcome is announced in Pascal's own bubble, so the row is stamped
/// `delegatedDisplay: true` and the Salon shows the run exactly once.
const DELEGATED_DISPLAY_TOOLS: &[&str] = &["run_custom"];

// ===========================================================================
// Shared data types
// ===========================================================================

/// A tool message saved to the chat (v4 `ToolMessage`, `chat-message/types.ts`).
/// The canonical single definition — like v4, where `ToolMessage` lives once in
/// `chat-message/types.ts` and is imported by both this service and the
/// tool-call-threading helpers ([`super::tool_call_threading`], which reads only
/// `tool_name`/`content`/`call_id`).
#[derive(Debug, Clone, Default)]
pub struct ToolMessage {
    pub tool_name: String,
    pub success: bool,
    /// The human-readable result text (the `result_text` from
    /// [`process_tool_calls`]), stored as the `content` row's `result` field.
    pub content: String,
    /// The call's arguments (`Record<string, unknown>`), order-preserving.
    pub arguments: Option<Value>,
    /// Provider-assigned call ID for native tool-result formatting.
    pub call_id: Option<String>,
    /// Character offset into the assistant turn's prose where this call fired.
    /// Set by the tool loops (W4.1e/f); `None` for [`process_tool_calls`]-built
    /// messages.
    pub anchor_offset: Option<f64>,
    /// Turn-monotonic sequence number, shared with reasoning segments. Set by the
    /// tool loops; `None` here.
    pub seq: Option<f64>,
    pub metadata: Option<ToolMetadata>,
}

/// Provider/model/expanded-prompt metadata carried on a tool result (v4
/// `ToolResult.metadata` / `ToolMessage.metadata`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolMetadata {
    pub provider: Option<String>,
    pub model: Option<String>,
    /// For image generation, the expanded prompt with `{{me}}` etc. resolved.
    pub expanded_prompt: Option<String>,
}

/// Generated-image metadata (v4 `GeneratedImage`). `size` is a JS number (bytes);
/// `width`/`height`/`sha256` are optional.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedImage {
    pub id: String,
    pub filename: String,
    pub filepath: String,
    pub mime_type: String,
    pub size: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub sha256: Option<String>,
}

/// Memories loaded into this turn's prompt, for introspection tools (v4
/// `LoadedMemoriesContext`, `lib/chat/tool-executor.ts`). Populated by the
/// orchestrator from the built context so `self_inventory` can report the exact
/// memory slate the LLM saw. Every field is optional in v4 (absent ⇒ empty).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadedMemoriesContext {
    /// Semantic-search memories rendered under `## Relevant Memories`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic: Vec<LoadedSemanticMemory>,
    /// Inter-character memories rendered under `## Memories About Other Characters`.
    #[serde(
        rename = "interCharacter",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub inter_character: Vec<LoadedInterCharacterMemory>,
    /// Memory recap text injected on chat start or character join, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recap: Option<String>,
}

/// One semantic memory in [`LoadedMemoriesContext`] (v4 `semantic[]` shape).
/// Numbers are JS numbers (`f64`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadedSemanticMemory {
    pub summary: String,
    pub importance: f64,
    pub score: f64,
    #[serde(rename = "effectiveWeight")]
    pub effective_weight: f64,
}

/// One inter-character memory in [`LoadedMemoriesContext`] (v4 `interCharacter[]`
/// shape).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadedInterCharacterMemory {
    #[serde(rename = "aboutCharacterName")]
    pub about_character_name: String,
    pub summary: String,
    pub importance: f64,
}

/// Per-chat context used to decide whether a tool result should be whispered (v4
/// `ToolWhisperContext`).
#[derive(Debug, Clone)]
pub struct ToolWhisperContext {
    pub user_participant_id: Option<String>,
    pub allow_cross_character_vault_reads: bool,
}

/// One detected tool call — the input to [`process_tool_calls`] (v4's inline
/// `{ name; arguments; callId? }`). `arguments` is an order-preserving [`Value`]
/// object.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
    pub call_id: Option<String>,
}

/// A tool handler's result — the output of the [`ToolRunner`] boundary (v4
/// `ToolResult`). `result` is `unknown` (carried as [`Value`]); on failure the
/// handler sets `error`/`message` and `result` is typically null.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_name: String,
    pub success: bool,
    pub result: Value,
    pub error: Option<String>,
    pub message: Option<String>,
    pub metadata: Option<ToolMetadata>,
}

/// The result of [`process_tool_calls`] (v4 `ToolProcessingResult`).
#[derive(Debug, Clone, Default)]
pub struct ToolProcessingResult {
    pub tool_messages: Vec<ToolMessage>,
    pub generated_image_paths: Vec<GeneratedImage>,
}

/// The status-frame context (v4's `statusContext`) — present when the caller wants
/// a `tool_executing` status per tool. Absent ⇒ no status frames (v4's
/// `if (statusContext)`).
#[derive(Debug, Clone)]
pub struct StatusContext {
    pub character_name: String,
    pub character_id: String,
}

/// The result of [`save_tool_messages`] (v4's `{ firstToolMessageId,
/// generatedImageIds }`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SaveToolMessagesResult {
    pub first_tool_message_id: Option<String>,
    pub generated_image_ids: Vec<String>,
}

// ===========================================================================
// The tool-execution context bag (createToolContext)
// ===========================================================================

/// Extended context for tool execution (v4 `ToolExecutionContext`). The data bag
/// passed to every handler through the [`ToolRunner`] boundary. The callback
/// members reaching unported subsystems are documented deferral slots (see the
/// module docs).
#[derive(Debug, Clone, Default)]
pub struct ToolExecutionContext {
    pub chat_id: String,
    pub user_id: String,
    pub image_profile_id: Option<String>,
    pub character_id: Option<String>,
    /// The rolling character's vault mount — the `run_custom` roster's fast path
    /// for the `character` tier (v4 `ToolExecutionContext.characterMountPointId`).
    pub character_mount_point_id: Option<String>,
    /// Every character in the chat — their vaults form the `run_custom` roster's
    /// `participant` tier (v4 `ToolExecutionContext.characterIds`).
    pub character_ids: Option<Vec<String>>,
    pub embedding_profile_id: Option<String>,
    /// Participant ID of who is calling the tool (for `{{me}}` resolution).
    pub calling_participant_id: Option<String>,
    pub project_id: Option<String>,
    /// Browser User-Agent from the originating request (scrubbed).
    pub browser_user_agent: Option<String>,
    /// Memories loaded into this turn's prompt, for introspection tools
    /// (`self_inventory`). Typed as [`LoadedMemoriesContext`] (W4.1d, its consumer
    /// `self_inventory` landed) — the orchestrator fills it from the built context.
    pub loaded_memories: Option<LoadedMemoriesContext>,
    /// Character IDs whose wardrobe was modified during this turn (v4's
    /// `pendingWardrobeAnnouncements` Set). Wardrobe tool handlers (W4.1d batch 2)
    /// add to it; the orchestrator drains it once at end-of-turn (the drain is a
    /// documented deferral — batch 2 only populates).
    ///
    /// It is `Arc<Mutex<…>>` (not a plain `HashSet`) so the handlers can record an
    /// announcement through the [`ToolRunner::run`] boundary's shared
    /// `&ToolExecutionContext` WITHOUT changing the trait signature (W4.1e consumes
    /// that trait concurrently). Cloning the context shares the same set, matching
    /// v4's shared-Set-by-reference semantics. The legacy "no set present → enqueue
    /// immediately" fallback (v4 `scheduleWardrobeAnnouncement`) is not ported: the
    /// ported context always carries a set.
    pub pending_wardrobe_announcements: Arc<Mutex<HashSet<String>>>,
    /// The Brahma Console operator surface (v4 `operatorSurface`). Character tool
    /// handlers never set this.
    pub operator_surface: bool,
}

/// Create a tool-execution context (v4 `createToolContext`, :269–292). Mints an
/// empty `pending_wardrobe_announcements` set; leaves the carina callback and
/// `operator_surface` absent (v4's `createToolContext` sets neither).
#[allow(clippy::too_many_arguments)]
pub fn create_tool_context(
    chat_id: impl Into<String>,
    user_id: impl Into<String>,
    character_id: impl Into<String>,
    character_participant_id: impl Into<String>,
    image_profile_id: Option<String>,
    embedding_profile_id: Option<String>,
    project_id: Option<String>,
    browser_user_agent: Option<String>,
    loaded_memories: Option<LoadedMemoriesContext>,
) -> ToolExecutionContext {
    ToolExecutionContext {
        chat_id: chat_id.into(),
        user_id: user_id.into(),
        // v4: `imageProfileId || undefined` (a falsy string collapses to absent).
        image_profile_id: image_profile_id.filter(|s| !s.is_empty()),
        character_id: Some(character_id.into()),
        // The orchestrator fills these for the `run_custom` roster (unit 9);
        // `createToolContext` leaves them unset, as v4 does.
        character_mount_point_id: None,
        character_ids: None,
        embedding_profile_id,
        calling_participant_id: Some(character_participant_id.into()),
        // v4: `projectId || undefined`.
        project_id: project_id.filter(|s| !s.is_empty()),
        browser_user_agent,
        loaded_memories,
        pending_wardrobe_announcements: Arc::new(Mutex::new(HashSet::new())),
        operator_surface: false,
    }
}

// ===========================================================================
// The injected executor boundary (executeToolCallWithContext)
// ===========================================================================

/// The tool executor boundary — v4's `executeToolCallWithContext` + every real
/// handler (`lib/chat/tool-executor.ts`). That dispatch is W4.1d; here it is
/// injected so both differential sides replay identical canned per-call results.
/// Async + generic-consumed like [`CompletionProvider`](crate::model::completion)
/// — no boxing, the future stays `Send`.
pub trait ToolRunner {
    /// Execute one tool call. Handler failures are surfaced IN-BAND as a
    /// `ToolResult { success: false, .. }` (v4's handler `try/catch` returns a
    /// failure result rather than throwing), so this never `Err`s.
    fn run(
        &self,
        tool_call: &ToolCall,
        ctx: &ToolExecutionContext,
    ) -> impl Future<Output = ToolResult> + Send;
}

/// The canonical lookup key for a canned tool result: `name | JSON.stringify(args)
/// | callId-or-'-'`. The v4 oracle's `executeToolCallWithContext` mock computes the
/// identical string, so a call either matches an entry registered on both sides or
/// surfaces as a corpus omission on both sides. (Arguments' inner key order is the
/// open-JSON seam — the corpus keeps them single-key/sorted.)
pub fn canned_tool_key(name: &str, arguments: &Value, call_id: Option<&str>) -> String {
    let args = serde_json::to_string(arguments).unwrap_or_default();
    format!("{name}|{args}|{}", call_id.unwrap_or("-"))
}

/// A canned [`ToolRunner`] for the differential + self-tests. Keyed by
/// [`canned_tool_key`]; an unregistered call panics (a dispatch-shape mismatch is
/// loud, never a silent wrong answer — the [`CannedCompletionProvider`] precedent).
///
/// [`CannedCompletionProvider`]: crate::model::completion::CannedCompletionProvider
#[derive(Default)]
pub struct CannedToolRunner {
    results: std::collections::HashMap<String, ToolResult>,
}

impl CannedToolRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the canned result for a `(name, arguments, callId)` call.
    pub fn register(&mut self, tool_call: &ToolCall, result: ToolResult) {
        self.results.insert(
            canned_tool_key(
                &tool_call.name,
                &tool_call.arguments,
                tool_call.call_id.as_deref(),
            ),
            result,
        );
    }
}

impl ToolRunner for CannedToolRunner {
    fn run(
        &self,
        tool_call: &ToolCall,
        _ctx: &ToolExecutionContext,
    ) -> impl Future<Output = ToolResult> + Send {
        let key = canned_tool_key(
            &tool_call.name,
            &tool_call.arguments,
            tool_call.call_id.as_deref(),
        );
        let result = self
            .results
            .get(&key)
            .cloned()
            .unwrap_or_else(|| panic!("CannedToolRunner: no canned result for `{key}`"));
        async move { result }
    }
}

// ===========================================================================
// compute_tool_message_targets — the whisper gate
// ===========================================================================

/// Decide the `targetParticipantIds` for a tool-result message (v4
/// `computeToolMessageTargets`, :50–61). `None` ⇒ public. `Some(vec)` ⇒ whispered
/// — to the **user participant** (`[userParticipantId]`), or `[]` when there is no
/// user participant. (NOT to the calling character — verified from source.)
pub fn compute_tool_message_targets(
    tool_name: &str,
    whisper_context: Option<&ToolWhisperContext>,
) -> Option<Vec<String>> {
    let ctx = whisper_context?;
    let is_always_private = ALWAYS_PRIVATE_TOOLS.contains(&tool_name);
    let is_vault_read = VAULT_READ_TOOLS.contains(&tool_name);
    let should_whisper =
        is_always_private || (is_vault_read && !ctx.allow_cross_character_vault_reads);
    if !should_whisper {
        return None;
    }
    Some(match &ctx.user_participant_id {
        Some(id) => vec![id.clone()],
        None => vec![],
    })
}

// ===========================================================================
// process_tool_calls — the dispatch harness (c.2)
// ===========================================================================

/// Process a detected tool-call batch (v4 `processToolCalls`, :73–171). Emits the
/// detection frame, then per call: a `tool_executing` status (when
/// `status_context` is present), a dispatch through `runner`, generated-image
/// extraction, and the tool-result frame. Aggregates the [`ToolMessage`] slate +
/// generated-image descriptors. Never errors — a handler failure is captured
/// in-band as a failure [`ToolMessage`] (v4 relies on the handler's own
/// `try/catch`).
pub async fn process_tool_calls<S, R>(
    tool_calls: &[ToolCall],
    tool_context: &ToolExecutionContext,
    sink: &S,
    runner: &R,
    status_context: Option<&StatusContext>,
) -> ToolProcessingResult
where
    S: EventSink,
    R: ToolRunner,
{
    let mut tool_messages: Vec<ToolMessage> = Vec::new();
    let mut generated_image_paths: Vec<GeneratedImage> = Vec::new();

    // Detection frame (the tool names + raw arguments, for the UI).
    sink.emit(ChatEvent::tools_detected(
        tool_calls.iter().map(|tc| tc.name.clone()).collect(),
        tool_calls.iter().map(|tc| tc.arguments.clone()).collect(),
    ));

    for (tool_index, tool_call) in tool_calls.iter().enumerate() {
        if let Some(sc) = status_context {
            sink.emit(ChatEvent::status(StatusPayload {
                stage: "tool_executing".into(),
                message: format!("Running {}...", tool_call.name),
                tool_name: Some(tool_call.name.clone()),
                character_name: Some(sc.character_name.clone()),
                character_id: Some(sc.character_id.clone()),
            }));
        }

        let tool_result = runner.run(tool_call, tool_context).await;

        // Generated-image extraction: on success, each array element with truthy
        // `filepath` AND `id` is a generated/kept image descriptor.
        if tool_result.success {
            if let Some(arr) = tool_result.result.as_array() {
                for img in arr {
                    if let Some(gi) = generated_image_from_value(img) {
                        generated_image_paths.push(gi);
                    }
                }
            }
        }

        let result_text = result_text_for(&tool_result);

        tool_messages.push(ToolMessage {
            tool_name: tool_result.tool_name.clone(),
            success: tool_result.success,
            content: result_text.clone(),
            arguments: Some(tool_call.arguments.clone()),
            call_id: tool_call.call_id.clone(),
            anchor_offset: None,
            seq: None,
            metadata: tool_result.metadata.clone(),
        });

        // Tool-result frame. On failure, carry the human-readable error text so
        // live UIs can show it (the raw `result` is often null on error).
        let error = if tool_result.success {
            None
        } else {
            Some(result_text)
        };
        sink.emit(ChatEvent::tool_result(ToolResultPayload {
            index: tool_index,
            name: tool_result.tool_name,
            success: tool_result.success,
            result: tool_result.result,
            error,
        }));
    }

    ToolProcessingResult {
        tool_messages,
        generated_image_paths,
    }
}

/// The human-readable result text for a tool message (v4 :127–136). On failure:
/// `Error: <error|'Unknown error'>[ - <message>]`. On `generate_image` /
/// `attach_image`: the count summary. Otherwise: `JSON.stringify(result, null, 2)`.
fn result_text_for(tool_result: &ToolResult) -> String {
    if !tool_result.success {
        // `toolResult.error || 'Unknown error'` — an empty string is falsy.
        let err = tool_result
            .error
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("Unknown error");
        // `toolResult.message ? ` - ${message}` : ''` — empty message ⇒ no suffix.
        let suffix = match tool_result.message.as_deref().filter(|s| !s.is_empty()) {
            Some(m) => format!(" - {m}"),
            None => String::new(),
        };
        format!("Error: {err}{suffix}")
    } else if tool_result.tool_name == "generate_image" {
        format!(
            "Generated {} image(s)",
            image_count_or_one(&tool_result.result)
        )
    } else if tool_result.tool_name == "attach_image" {
        format!(
            "Attached {} kept image(s)",
            image_count_or_one(&tool_result.result)
        )
    } else {
        // `JSON.stringify(result, null, 2)` — 2-space pretty. serde_json matches JS
        // for the constrained corpus shapes (scalars / arrays / single-key
        // objects); multi-key object key order is the open-JSON seam.
        serde_json::to_string_pretty(&tool_result.result).unwrap_or_else(|_| "null".into())
    }
}

/// `(result as unknown[])?.length || 1` — the array length, or 1 when the result
/// is not a non-empty array (v4's `|| 1` idiom).
fn image_count_or_one(result: &Value) -> usize {
    match result.as_array().map(|a| a.len()) {
        Some(n) if n > 0 => n,
        _ => 1,
    }
}

/// Extract a [`GeneratedImage`] from a result-array element iff it has truthy
/// `filepath` AND `id` (v4 :112). `mimeType` defaults `'image/png'`; `size`
/// defaults 0.
fn generated_image_from_value(img: &Value) -> Option<GeneratedImage> {
    let id = img
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    let filepath = img
        .get("filepath")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    Some(GeneratedImage {
        id: id.to_string(),
        filename: img
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        filepath: filepath.to_string(),
        // `img.mimeType || 'image/png'`.
        mime_type: img
            .get("mimeType")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("image/png")
            .to_string(),
        // `img.size || 0`.
        size: img.get("size").and_then(Value::as_f64).unwrap_or(0.0),
        width: img.get("width").and_then(Value::as_f64),
        height: img.get("height").and_then(Value::as_f64),
        sha256: img
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

// ===========================================================================
// save_tool_messages — the TOOL-row persistence primitive (c.1)
// ===========================================================================

/// The generic TOOL-row `content` JSON (v4 :206–223), serialized in v4's exact
/// field order. Only present keys are written (`skip_serializing_if`), matching
/// v4's `JSON.stringify` dropping `undefined` values. `arguments` is the raw
/// order-preserving object; `anchorOffset`/`seq` render as bare JS numbers.
#[derive(Serialize)]
struct ToolMessageContent<'a> {
    #[serde(rename = "toolName")]
    tool_name: &'a str,
    success: bool,
    result: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<&'a Value>,
    #[serde(rename = "callId", skip_serializing_if = "Option::is_none")]
    call_id: Option<&'a str>,
    /// `true` for a tool whose visible artifact is another message (v4's
    /// `DELEGATED_DISPLAY_TOOLS`): the row persists for threading and the model,
    /// but the Salon renders nothing for it, so the run shows up once, not twice.
    /// Omitted (not `false`) otherwise, matching v4's `...(has ? {…} : {})`.
    #[serde(rename = "delegatedDisplay", skip_serializing_if = "Option::is_none")]
    delegated_display: Option<bool>,
    #[serde(
        rename = "anchorOffset",
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_js_number"
    )]
    anchor_offset: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_js_number"
    )]
    seq: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<&'a str>,
}

/// Serialize an `Option<f64>` the JS way (an integer-valued double renders bare).
/// Only called via `skip_serializing_if = Option::is_none`, so the value is Some.
fn ser_opt_js_number<S: serde::Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    js_number_to_json(v.expect("skip_serializing_if guards None")).serialize(s)
}

/// Build the TOOL-row `content` JSON string for a tool message (v4 :206–223).
/// `provider`/`model`/`prompt` come from `metadata?` (absent ⇒ dropped).
fn build_tool_message_content(tool_msg: &ToolMessage) -> Result<String, DbError> {
    let content = ToolMessageContent {
        tool_name: &tool_msg.tool_name,
        success: tool_msg.success,
        result: &tool_msg.content,
        arguments: tool_msg.arguments.as_ref(),
        call_id: tool_msg.call_id.as_deref(),
        delegated_display: if DELEGATED_DISPLAY_TOOLS.contains(&tool_msg.tool_name.as_str()) {
            Some(true)
        } else {
            None
        },
        anchor_offset: tool_msg.anchor_offset,
        seq: tool_msg.seq,
        provider: tool_msg
            .metadata
            .as_ref()
            .and_then(|m| m.provider.as_deref()),
        model: tool_msg.metadata.as_ref().and_then(|m| m.model.as_deref()),
        prompt: tool_msg
            .metadata
            .as_ref()
            .and_then(|m| m.expanded_prompt.as_deref()),
    };
    serde_json::to_string(&content)
        .map_err(|e| DbError::Key(format!("tool message content serialize: {e}")))
}

/// Persist a tool-message slate and link/tag generated images (v4
/// `saveToolMessages`, :176–254). One `type:'message'` / `role:'TOOL'` row per
/// tool message (through `add_message`, so a TOOL row bumps the chat's minted
/// `lastMessageAt`/`updatedAt` but is excluded from `messageCount` and never
/// touches `spokenThisCycle`), whisper-gated via [`compute_tool_message_targets`];
/// then each generated image is linked to the FIRST tool message and tagged with
/// the character. Runs on a borrowed writer connection.
///
/// `_user_id` mirrors v4's unused `_userId` parameter (the finalizer passes `''`).
#[allow(clippy::too_many_arguments)]
pub fn save_tool_messages(
    writer: &Writer,
    chat_id: &str,
    _user_id: &str,
    tool_messages: &[ToolMessage],
    generated_image_paths: &[GeneratedImage],
    character_id: Option<&str>,
    participant_id: Option<&str>,
    whisper_context: Option<&ToolWhisperContext>,
) -> Result<SaveToolMessagesResult, DbError> {
    let mut first_tool_message_id: Option<String> = None;
    let generated_image_ids: Vec<String> = generated_image_paths
        .iter()
        .map(|img| img.id.clone())
        .collect();

    for tool_msg in tool_messages {
        let tool_message_id = uuid::Uuid::new_v4().to_string();
        // `attach_image` resurfaces kept images via the same pipeline as
        // `generate_image`, so its descriptors get attached too.
        let tool_attachments: Vec<String> =
            if tool_msg.tool_name == "generate_image" || tool_msg.tool_name == "attach_image" {
                generated_image_ids.clone()
            } else {
                Vec::new()
            };

        let target_participant_ids =
            compute_tool_message_targets(&tool_msg.tool_name, whisper_context);
        let content = build_tool_message_content(tool_msg)?;

        // Build the TOOL `MessageEvent` object; `add_message` marshals it. v4 sets
        // `participantId: participantId ?? null` and `targetParticipantIds: <the
        // computed value | null>`; both collapse to SQL NULL when absent, so an
        // omitted key and a `null` value are equivalent on disk.
        let mut msg = serde_json::Map::new();
        msg.insert("id".into(), json!(tool_message_id));
        msg.insert("type".into(), json!("message"));
        msg.insert("role".into(), json!("TOOL"));
        msg.insert("participantId".into(), json!(participant_id));
        msg.insert("targetParticipantIds".into(), json!(target_participant_ids));
        msg.insert("content".into(), json!(content));
        msg.insert("createdAt".into(), json!(crate::clock::now_iso()));
        msg.insert("attachments".into(), json!(tool_attachments));

        let event: crate::db::chats_messages::ChatEventInput =
            serde_json::from_value(Value::Object(msg))
                .map_err(|e| DbError::Key(format!("saveToolMessages marshal: {e}")))?;
        writer.chat_messages().add_message(chat_id, &event)?;

        if first_tool_message_id.is_none() {
            first_tool_message_id = Some(tool_message_id);
        }
    }

    // Link each generated image to the FIRST tool message + tag it with the
    // character (v4 :234–251). A missing file / failed link is swallowed (v4
    // catches + logs a warn) so the message still stands.
    for image_id in &generated_image_ids {
        if let Some(first) = &first_tool_message_id {
            let _ = writer.files().add_link(image_id, first);
        }
        if let Some(cid) = character_id {
            let _ = writer.files().add_tag(image_id, cid);
        }
    }

    Ok(SaveToolMessagesResult {
        first_tool_message_id,
        generated_image_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::chat_events::RecordingSink;

    fn ctx() -> ToolExecutionContext {
        create_tool_context(
            "chat-1", "user-1", "char-1", "pp-1", None, None, None, None, None,
        )
    }

    #[test]
    fn whisper_gate_targets_the_user_participant_not_the_character() {
        let wc = ToolWhisperContext {
            user_participant_id: Some("pp-user".into()),
            allow_cross_character_vault_reads: false,
        };
        // ALWAYS_PRIVATE → whispered to the user participant.
        assert_eq!(
            compute_tool_message_targets("search", Some(&wc)),
            Some(vec!["pp-user".into()])
        );
        // VAULT_READ, cross-reads OFF → whispered.
        assert_eq!(
            compute_tool_message_targets("doc_read_file", Some(&wc)),
            Some(vec!["pp-user".into()])
        );
        // A public tool → None (public).
        assert_eq!(compute_tool_message_targets("state", Some(&wc)), None);
        // No whisper context → None.
        assert_eq!(compute_tool_message_targets("search", None), None);
    }

    #[test]
    fn whisper_gate_vault_read_public_when_cross_reads_allowed() {
        let wc = ToolWhisperContext {
            user_participant_id: Some("pp-user".into()),
            allow_cross_character_vault_reads: true,
        };
        // VAULT_READ + cross-reads ON → public.
        assert_eq!(compute_tool_message_targets("doc_grep", Some(&wc)), None);
        // ALWAYS_PRIVATE is unaffected by cross-reads → still whispered.
        assert_eq!(
            compute_tool_message_targets("read_conversation", Some(&wc)),
            Some(vec!["pp-user".into()])
        );
    }

    #[test]
    fn whisper_gate_empty_targets_when_no_user_participant() {
        let wc = ToolWhisperContext {
            user_participant_id: None,
            allow_cross_character_vault_reads: false,
        };
        assert_eq!(
            compute_tool_message_targets("search", Some(&wc)),
            Some(Vec::new())
        );
    }

    #[tokio::test]
    async fn process_tool_calls_success_builds_message_and_frames() {
        let calls = vec![ToolCall {
            name: "state".into(),
            arguments: json!({ "key": "mood" }),
            call_id: Some("c1".into()),
        }];
        let mut runner = CannedToolRunner::new();
        runner.register(
            &calls[0],
            ToolResult {
                tool_name: "state".into(),
                success: true,
                result: json!({ "mood": "curious" }),
                error: None,
                message: None,
                metadata: None,
            },
        );
        let sink = RecordingSink::new();
        let status = StatusContext {
            character_name: "Friday".into(),
            character_id: "char-1".into(),
        };
        let out = process_tool_calls(&calls, &ctx(), &sink, &runner, Some(&status)).await;

        assert_eq!(out.tool_messages.len(), 1);
        assert_eq!(out.tool_messages[0].tool_name, "state");
        assert!(out.tool_messages[0].success);
        // 2-space pretty single-key object.
        assert_eq!(
            out.tool_messages[0].content,
            "{\n  \"mood\": \"curious\"\n}"
        );
        assert!(out.generated_image_paths.is_empty());

        let frames = sink.events_json();
        // detection, status, result.
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0]["toolsDetected"], json!(1));
        assert_eq!(frames[1]["status"]["stage"], json!("tool_executing"));
        assert_eq!(frames[2]["toolResult"]["success"], json!(true));
        assert_eq!(frames[2]["toolResult"]["index"], json!(0));
    }

    #[tokio::test]
    async fn process_tool_calls_failure_builds_error_text_and_frame() {
        let calls = vec![ToolCall {
            name: "search".into(),
            arguments: json!({ "q": "x" }),
            call_id: None,
        }];
        let mut runner = CannedToolRunner::new();
        runner.register(
            &calls[0],
            ToolResult {
                tool_name: "search".into(),
                success: false,
                result: json!(null),
                error: Some("NOT_FOUND".into()),
                message: Some("no character".into()),
                metadata: None,
            },
        );
        let sink = RecordingSink::new();
        let out = process_tool_calls(&calls, &ctx(), &sink, &runner, None).await;

        assert_eq!(
            out.tool_messages[0].content,
            "Error: NOT_FOUND - no character"
        );
        let frames = sink.events_json();
        // no status frame (status_context None): detection + result only.
        assert_eq!(frames.len(), 2);
        assert_eq!(
            frames[1]["toolResult"]["error"],
            json!("Error: NOT_FOUND - no character")
        );
    }

    #[tokio::test]
    async fn process_tool_calls_generate_image_extracts_paths_and_count_text() {
        let calls = vec![ToolCall {
            name: "generate_image".into(),
            arguments: json!({ "prompt": "a cat" }),
            call_id: None,
        }];
        let mut runner = CannedToolRunner::new();
        runner.register(
            &calls[0],
            ToolResult {
                tool_name: "generate_image".into(),
                success: true,
                result: json!([
                    { "id": "img-1", "filepath": "/f/1.png", "filename": "1.png" },
                    { "id": "", "filepath": "/f/2.png" },
                ]),
                error: None,
                message: None,
                metadata: None,
            },
        );
        let sink = RecordingSink::new();
        let out = process_tool_calls(&calls, &ctx(), &sink, &runner, None).await;

        // Only the element with truthy id AND filepath extracts.
        assert_eq!(out.generated_image_paths.len(), 1);
        assert_eq!(out.generated_image_paths[0].id, "img-1");
        assert_eq!(out.generated_image_paths[0].mime_type, "image/png");
        assert_eq!(out.tool_messages[0].content, "Generated 2 image(s)");
    }

    #[test]
    fn tool_message_content_field_order_and_omission() {
        // Full metadata + arguments + callId.
        let tm = ToolMessage {
            tool_name: "state".into(),
            success: true,
            content: "{}".into(),
            arguments: Some(json!({ "k": "v" })),
            call_id: Some("c1".into()),
            anchor_offset: Some(12.0),
            seq: Some(3.0),
            metadata: Some(ToolMetadata {
                provider: Some("ANTHROPIC".into()),
                model: Some("claude".into()),
                expanded_prompt: Some("prompt".into()),
            }),
        };
        assert_eq!(
            build_tool_message_content(&tm).unwrap(),
            r#"{"toolName":"state","success":true,"result":"{}","arguments":{"k":"v"},"callId":"c1","anchorOffset":12,"seq":3,"provider":"ANTHROPIC","model":"claude","prompt":"prompt"}"#
        );
        // Minimal: no metadata, no callId, no anchors → those keys dropped.
        let tm2 = ToolMessage {
            tool_name: "state".into(),
            success: false,
            content: "Error: boom".into(),
            arguments: Some(json!({ "k": "v" })),
            call_id: None,
            anchor_offset: None,
            seq: None,
            metadata: None,
        };
        assert_eq!(
            build_tool_message_content(&tm2).unwrap(),
            r#"{"toolName":"state","success":false,"result":"Error: boom","arguments":{"k":"v"}}"#
        );
    }

    #[test]
    fn run_custom_row_carries_delegated_display() {
        // v4 `tool-execution.service.ts:206–223`: `delegatedDisplay: true` sits
        // between `callId` and `anchorOffset`, and ONLY for a DELEGATED_DISPLAY
        // tool. A run_custom TOOL row renders nothing (Pascal's bubble is the run's
        // single visible artifact) but still persists for threading + the model.
        let tm = ToolMessage {
            tool_name: "run_custom".into(),
            success: true,
            content: "ansible: The ansible flickers to life. [success]".into(),
            arguments: Some(json!({ "tool": "ansible" })),
            call_id: Some("call-1".into()),
            anchor_offset: None,
            seq: None,
            metadata: None,
        };
        assert_eq!(
            build_tool_message_content(&tm).unwrap(),
            r#"{"toolName":"run_custom","success":true,"result":"ansible: The ansible flickers to life. [success]","arguments":{"tool":"ansible"},"callId":"call-1","delegatedDisplay":true}"#
        );
        // A non-delegated tool never gains the key.
        let tm2 = ToolMessage {
            tool_name: "rng".into(),
            call_id: Some("call-1".into()),
            ..tm.clone()
        };
        assert!(!build_tool_message_content(&tm2)
            .unwrap()
            .contains("delegatedDisplay"));
    }
}
