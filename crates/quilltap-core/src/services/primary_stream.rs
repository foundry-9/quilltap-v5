//! The primary-stream service (Phase-3 Unit-3 wave 3) — v4
//! `primary-stream.service.ts` (`runPrimaryStream`, `makePreservePartialOnError`,
//! `findPreviousResponseId`) over the streaming model boundary
//! ([`crate::model::stream`]).
//!
//! `run_primary_stream` drives the first LLM turn after the slate is assembled:
//! it emits the `Sending to <name>…` status, folds the stream chunks into a
//! mutable [`StreamingState`] (reasoning captured + live-forwarded, the
//! `sending → streaming` status flip on the first content chunk, `done` capturing
//! usage/rawResponse/thoughtSignature), and forks the two error branches:
//!
//!   1. **Tool-unsupported retry** — retry the exact request with tools cleared
//!      once (e.g. some Gemini 3 variants reject function calling).
//!   2. **Request-limit recovery** — delegate to
//!      [`super::recovery::attempt_request_limit_recovery`], which produces a
//!      fully-finalized message; the orchestrator short-circuits via the
//!      [`PrimaryStreamResult::early_return`] field.
//!
//! On any other error (or a failed retry) it preserves the partial response with
//! an OOC marker and rethrows.
//!
//! ## Boundary notes (`api-boundary.md`)
//!
//! Token deltas are [`ChatEvent`]s pushed at the [`EventSink`], never writes; the
//! only committed artifact this service writes is the preserved-partial message
//! (through [`save_assistant_message`] on the [`Db`] writer). The stream loop
//! never blocks on the writer — the preserve write happens only on the terminal
//! error path, after the stream has ended.
//!
//! ## The error-classification helpers
//!
//! This module also ports the `lib/llm/errors.ts` string/shape matchers the
//! stream branches key on (`is_tool_unsupported_error`,
//! `is_recoverable_request_error`, `is_token_limit_error`,
//! `is_content_limit_error`, `parse_token_limit_error`,
//! `parse_content_limit_error`). They live here (v4 hosts them in
//! `lib/llm/errors.ts`, a not-yet-otherwise-ported module) and are re-exported to
//! [`super::recovery`] / [`super::provider_failover`], which classify the same
//! errors. The regexes reproduce v4's `RegExp` sources verbatim (JS `i` flag →
//! `(?i)`); `to_locale_string` reproduces `Number.prototype.toLocaleString()`'s
//! en-US thousands grouping (the recovery message text reaches the DB).

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::cache_prefix_hashes::{compute_request_prefix_hashes, PrefixMessage};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::finish_reason::extract_finish_reason;
use crate::message_formatter::{normalize_content_block_format, strip_character_name_prefix};
use crate::model::stream::{
    canned_stream_key, StreamCacheUsage, StreamChunk, StreamError, StreamMessage, StreamParams,
    StreamUsage, StreamingCompletionProvider,
};

use super::chat_events::{ChatEvent, EventSink, StatusPayload};
use super::llm_logging::{
    log_llm_call, log_type, LogCacheUsage, LogContext, LogLlmCallParams, LogRequest,
    LogRequestMessage, LogResponse, LogUsage,
};
use super::tool_execution::{save_tool_messages, GeneratedImage, ToolMessage, ToolWhisperContext};

// ===========================================================================
// Error classification (v4 `lib/llm/errors.ts`)
// ===========================================================================

/// v4's content-limit categories (`ContentLimitType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentLimitType {
    Token,
    PdfPages,
    ImageSize,
    FileSize,
    Unknown,
}

impl ContentLimitType {
    /// v4's `limitDescriptions[type]` (the recovery-message noun phrase).
    fn description(self) -> &'static str {
        match self {
            ContentLimitType::PdfPages => "PDF page limit",
            ContentLimitType::ImageSize => "image size limit",
            ContentLimitType::FileSize => "file size limit",
            ContentLimitType::Token => "token limit",
            ContentLimitType::Unknown => "content limit",
        }
    }
}

static TOKEN_LIMIT_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)context_length_exceeded",
        r"(?i)maximum context length",
        r"(?i)tokens.*exceeds?.*maximum",
        r"(?i)request too large",
        r"(?i)prompt is too long",
        r"(?i)maximum.*tokens",
        r"(?i)request would exceed",
        r"(?i)request payload size exceeds",
        r"(?i)input too long",
        r"(?i)context.*length.*exceeded",
        r"(?i)token.*limit",
        r"(?i)input.*too.*long",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("token-limit pattern compiles"))
    .collect()
});

struct ContentPattern {
    re: Regex,
    kind: ContentLimitType,
}

static CONTENT_LIMIT_PATTERNS: LazyLock<Vec<ContentPattern>> = LazyLock::new(|| {
    [
        (
            r"(?i)maximum of (\d+) PDF pages?",
            ContentLimitType::PdfPages,
        ),
        (
            r"(?i)PDF.*?(\d+).*?pages?.*?maximum",
            ContentLimitType::PdfPages,
        ),
        (r"(?i)too many PDF pages", ContentLimitType::PdfPages),
        (r"(?i)image.*?too large", ContentLimitType::ImageSize),
        (
            r"(?i)image.*?exceeds?.*?maximum",
            ContentLimitType::ImageSize,
        ),
        (
            r"(?i)maximum image (size|dimensions)",
            ContentLimitType::ImageSize,
        ),
        (r"(?i)file.*?too large", ContentLimitType::FileSize),
        (r"(?i)file.*?exceeds?.*?maximum", ContentLimitType::FileSize),
        (r"(?i)maximum file size", ContentLimitType::FileSize),
        (r"(?i)content.*?too (large|long)", ContentLimitType::Unknown),
        (
            r"(?i)exceeds?.*?maximum.*?(size|length|limit)",
            ContentLimitType::Unknown,
        ),
    ]
    .iter()
    .map(|(p, kind)| ContentPattern {
        re: Regex::new(p).expect("content-limit pattern compiles"),
        kind: *kind,
    })
    .collect()
});

static TOOL_UNSUPPORTED_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)tool use with function calling is unsupported",
        r"(?i)function.?calling is not supported",
        r"(?i)does not support function.?calling",
        r"(?i)does not support tools",
        r"(?i)tool_use.*not.*supported",
        r"(?i)tools are not supported",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("tool-unsupported pattern compiles"))
    .collect()
});

static TOKENS_GT_MAX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(\d+)\s*tokens?\s*>\s*(\d+)\s*maximum").unwrap());
static MAXIMUM_TOKENS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)maximum.*?(\d+)\s*tokens").unwrap());
static PDF_MAX_PAGES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)maximum of (\d+) PDF pages?").unwrap());

/// v4 `isTokenLimitError` — a `TokenLimitError` instance OR a message matching a
/// token-limit pattern. We have no error class hierarchy here (the seam surfaces
/// [`StreamError`] carrying the provider message), so we match on the message.
pub fn is_token_limit_error(message: &str) -> bool {
    TOKEN_LIMIT_PATTERNS.iter().any(|p| p.is_match(message))
}

/// v4 `isContentLimitError` (message-based, as above).
pub fn is_content_limit_error(message: &str) -> bool {
    CONTENT_LIMIT_PATTERNS
        .iter()
        .any(|p| p.re.is_match(message))
}

/// v4 `isToolUnsupportedError`.
pub fn is_tool_unsupported_error(message: &str) -> bool {
    TOOL_UNSUPPORTED_PATTERNS
        .iter()
        .any(|p| p.is_match(message))
}

/// v4 `isRecoverableRequestError` — token limit OR content limit.
pub fn is_recoverable_request_error(message: &str) -> bool {
    is_token_limit_error(message) || is_content_limit_error(message)
}

/// v4 `parseTokenLimitError` — `(requestedTokens, maxTokens)` if the message
/// carries them.
pub fn parse_token_limit_error(message: &str) -> (Option<i64>, Option<i64>) {
    if let Some(c) = TOKENS_GT_MAX.captures(message) {
        let req = c.get(1).and_then(|m| m.as_str().parse::<i64>().ok());
        let max = c.get(2).and_then(|m| m.as_str().parse::<i64>().ok());
        return (req, max);
    }
    if let Some(c) = MAXIMUM_TOKENS.captures(message) {
        let max = c.get(1).and_then(|m| m.as_str().parse::<i64>().ok());
        return (None, max);
    }
    (None, None)
}

/// v4 `parseContentLimitError` — `(type, maxValue, description)`.
pub fn parse_content_limit_error(message: &str) -> (ContentLimitType, Option<i64>, Option<String>) {
    if let Some(c) = PDF_MAX_PAGES.captures(message) {
        let raw = c.get(1).unwrap().as_str();
        let max = raw.parse::<i64>().ok();
        return (
            ContentLimitType::PdfPages,
            max,
            Some(format!("PDF documents cannot exceed {raw} pages")),
        );
    }
    for cp in CONTENT_LIMIT_PATTERNS.iter() {
        if cp.re.is_match(message) {
            return (cp.kind, None, Some(message.to_string()));
        }
    }
    (ContentLimitType::Unknown, None, None)
}

/// Reproduce `Number.prototype.toLocaleString()` (no args) for a non-negative
/// integer under Node's default en-US locale: group thousands with `,`. The
/// recovery message text is persisted, so this must be byte-exact.
/// (`pub(crate)`: also reused by `enclave::announce` for the run-start
/// banner's token-cap grouping.)
pub(crate) fn to_locale_string(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    let len = digits.len();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

// ===========================================================================
// Content limit accessors for recovery / static-fallback message builders
// ===========================================================================

/// v4's `limitDescriptions[type]`, exposed for the recovery message builder.
pub(super) fn content_limit_description(kind: ContentLimitType) -> &'static str {
    kind.description()
}

// ===========================================================================
// StreamingState
// ===========================================================================

/// One captured reasoning / chain-of-thought block, positioned in the assistant
/// turn's prose (v4 `ReasoningSegment`). DISPLAY ONLY.
#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningSegment {
    pub anchor_offset: usize,
    pub content: String,
    pub seq: u64,
}

/// The connection profile fields the primary stream reads off `effectiveProfile`
/// (v4 `ConnectionProfile` subset). The port carries only what the stream / the
/// preserved-partial write consume; the full profile lives above the seam.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveProfile {
    pub id: String,
    pub provider: String,
    pub model_name: String,
    pub base_url: Option<String>,
}

/// Mutable streaming state threaded through the stream loop and its sub-services
/// (v4 `StreamingState`). Passed by `&mut` so failover / preserve read updated
/// values from the same object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamingState {
    pub full_response: String,
    pub effective_profile: Option<EffectiveProfile>,
    pub effective_api_key: String,
    pub usage: Option<StreamUsage>,
    pub cache_usage: Option<StreamCacheUsage>,
    pub attachment_results: Option<Value>,
    pub raw_response: Option<Value>,
    pub thought_signature: Option<String>,
    pub reasoning_content: Option<String>,
    pub reasoning_segments: Vec<ReasoningSegment>,
    /// How many chars of `reasoning_content` have been flushed into a segment.
    pub reasoning_flushed_len: usize,
    /// Turn-monotonic sequence handed to reasoning segments (and, later, tool
    /// anchors).
    pub next_turn_seq: u64,
    pub has_started_streaming: bool,
}

impl StreamingState {
    /// Hand out the next turn-monotonic sequence (v4 `nextTurnSeq`).
    fn next_seq(&mut self) -> u64 {
        let seq = self.next_turn_seq;
        self.next_turn_seq = seq + 1;
        seq
    }

    /// Hand out the next turn-monotonic sequence (v4 `nextTurnSeq`) — the public
    /// form the native tool loop ([`crate::services::native_tool_loop`]) uses to
    /// stamp each tool batch's `seq`, shared with reasoning segments.
    pub fn next_turn_seq(&mut self) -> u64 {
        self.next_seq()
    }
}

/// Capture reasoning from a chunk and live-forward it (v4 `applyReasoningChunk`).
/// Providers emit `reasoningContent` cumulatively, so it is last-wins onto the
/// state and the cumulative value pushed to the client. DISPLAY ONLY.
pub fn apply_reasoning_chunk<S: EventSink>(
    state: &mut StreamingState,
    chunk: &StreamChunk,
    sink: &S,
) {
    let Some(reasoning) = chunk.reasoning_content.as_ref() else {
        return;
    };
    if reasoning.is_empty() {
        // v4 guards `if (!chunk.reasoningContent) return` — an empty string is
        // falsy, so no forward.
        return;
    }
    if state.reasoning_content.as_deref() == Some(reasoning.as_str()) {
        return;
    }
    state.reasoning_content = Some(reasoning.clone());
    sink.emit(ChatEvent::reasoning(reasoning.clone()));
}

/// Close the current reasoning run into a positioned segment if any reasoning has
/// accumulated since the last flush (v4 `flushReasoningSegment`). Uses UTF-16
/// lengths for the JS `.length` / `.slice` semantics — reasoning offsets are
/// display coordinates the client matches against `fullResponse.length`.
pub fn flush_reasoning_segment(state: &mut StreamingState) {
    let full = state.reasoning_content.clone().unwrap_or_default();
    let full_len = crate::jsstr::utf16_len(&full);
    let flushed = state.reasoning_flushed_len;
    if full_len <= flushed {
        return;
    }
    let content = utf16_slice_from(&full, flushed);
    // Advance the cursor regardless so we never re-flush this span.
    state.reasoning_flushed_len = full_len;
    if crate::jsstr::js_trim(&content).is_empty() {
        return;
    }
    let anchor_offset = crate::jsstr::utf16_len(&state.full_response);
    let seq = state.next_seq();
    state.reasoning_segments.push(ReasoningSegment {
        anchor_offset,
        content,
        seq,
    });
}

/// `s.slice(from)` in UTF-16 units (v4's `reasoningContent.slice(flushedLen)`).
fn utf16_slice_from(s: &str, from: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    if from >= units.len() {
        return String::new();
    }
    String::from_utf16_lossy(&units[from..])
}

// ===========================================================================
// saveAssistantMessage (persistence primitive — reused by the finalizer wave)
// ===========================================================================

/// The inputs [`save_assistant_message`] needs — the character identity and the
/// responding participant (v4's `character` + `characterParticipant`). Kept slim:
/// the vault-overlaid `Character` lives above the seam; the write only reads
/// `name` (for the OOC strip, done by the caller) and `id` (for image links, out
/// of scope here) plus the participant `id` / `status`.
#[derive(Clone, Debug)]
pub struct AssistantMessageContext<'a> {
    pub character_id: &'a str,
    pub character_name: &'a str,
    pub participant_id: &'a str,
    /// The participant's `status` — a `'silent'` status makes `isSilentMessage`
    /// true (v4 `characterParticipant.status === 'silent' || null`).
    pub participant_status: Option<&'a str>,
}

/// The answer-confirmation result keys v4's `saveAssistantMessage` writes onto the
/// message (v4's `confirmation` object). Each field is a **three-state**
/// [`ConfirmationField`]: absent (no key written), or a value. v4 only writes a
/// key when the corresponding field is `!== undefined`; `confirmationChecked` is
/// derived (`confirmed !== undefined ? true : undefined`). The finalizer's
/// SKIPPED paths (user-driven / silent / inactive) all leave `confirmed`
/// undefined, so the whole bag is absent — the shape the corpus exercises. The
/// active-confirmation call (which sets these) is an injected seam (wave 4).
#[derive(Clone, Debug, Default)]
pub struct ConfirmationFields {
    /// `confirmed` — `Absent` → no key; `Null`/`Value(bool)` → written. A
    /// resolved value (incl. `null`) also materializes `confirmationChecked: 1`.
    pub confirmed: ConfirmationField<bool>,
    pub revised: ConfirmationField<bool>,
    pub notes: ConfirmationField<String>,
    pub original_content: ConfirmationField<String>,
}

/// A three-state confirmation field mirroring v4's `x !== undefined` gate on a
/// `boolean | null` / `string | null` value: `Absent` = no key written, `Null` =
/// written as JSON `null`, `Value` = written as the value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ConfirmationField<T> {
    /// v4 `undefined` — no message key written.
    #[default]
    Absent,
    /// Written as JSON `null` (v4's explicit `null`).
    Null,
    /// Written as this value.
    Value(T),
}

impl ConfirmationField<bool> {
    fn to_json(&self) -> Option<Value> {
        match self {
            ConfirmationField::Absent => None,
            ConfirmationField::Null => Some(Value::Null),
            ConfirmationField::Value(b) => Some(json!(b)),
        }
    }
}

impl ConfirmationField<String> {
    fn to_json(&self) -> Option<Value> {
        match self {
            ConfirmationField::Absent => None,
            ConfirmationField::Null => Some(Value::Null),
            ConfirmationField::Value(s) => Some(json!(s)),
        }
    }
}

/// v4 `saveAssistantMessage` (message-finalizer.service.ts:572) — the persistence
/// primitive that writes the assistant `MessageEvent` row (with the metadata
/// side-effect), links generated-image attachments, and returns the message id.
///
/// This port covers: the message row (all display + token + reasoning fields),
/// the `isSilentMessage` TEXT-affinity flag (from participant status), the
/// answer-confirmation key bag (exercised only in its absent shape — see
/// [`ConfirmationFields`]), and the image-link loop
/// (`repos.files.addLink(imageId, messageId)` per generated image, each failure
/// swallowed as v4 does). `saveToolMessages` (v4 calls it only when
/// `toolMessages.length > 0`) stays a wave-4 concern — the finalizer corpus only
/// exercises the empty path, so no tool-message write fires here (byte-faithful:
/// v4 skips the call entirely for an empty list).
///
/// Writes through the `chats` `addMessage` metadata side-effect (a `type:message`
/// event bumps `lastMessageAt`/`updatedAt`), on a borrowed writer connection.
#[allow(clippy::too_many_arguments)]
pub fn save_assistant_message(
    writer: &crate::db::Writer,
    chat_id: &str,
    ctx: &AssistantMessageContext<'_>,
    content: &str,
    usage: Option<StreamUsage>,
    raw_response: Option<&Value>,
    thought_signature: Option<&str>,
    pre_generated_message_id: Option<&str>,
    provider: Option<&str>,
    model_name: Option<&str>,
    reasoning_content: Option<&str>,
    reasoning_segments: &[ReasoningSegment],
    generated_image_paths: &[GeneratedImage],
    tool_messages: &[ToolMessage],
    whisper_context: Option<&ToolWhisperContext>,
    confirmation: &ConfirmationFields,
) -> Result<String, DbError> {
    let assistant_message_id = pre_generated_message_id
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = crate::clock::now_iso();
    // `attachments: generatedImagePaths.map(img => img.id)` — the ids the assistant
    // message carries (and forwards to the image-link loop below).
    let generated_image_ids: Vec<String> =
        generated_image_paths.iter().map(|g| g.id.clone()).collect();

    // v4: `usage?.totalTokens || null` (a 0 becomes null — the `|| null` idiom).
    let token_count = usage.map(|u| u.total_tokens).filter(|&t| t != 0);
    let prompt_tokens = usage.map(|u| u.prompt_tokens).filter(|&t| t != 0);
    let completion_tokens = usage.map(|u| u.completion_tokens).filter(|&t| t != 0);

    // v4: `characterParticipant.status === 'silent' || null` — true when silent,
    // else `null` (absent).
    let is_silent = if ctx.participant_status == Some("silent") {
        Some(true)
    } else {
        None
    };

    // Build the MessageEvent JSON exactly as v4's `assistantMessage` object, then
    // let `add_message` marshal it (schema-order JSON columns, TEXT-affinity
    // isSilentMessage, etc.). Omit `undefined`/`null-collapsed` keys so the row
    // matches v4's `JSON.stringify`-then-insert byte-for-byte: v4 sets a nullable
    // field to `x || null`, but the insert marshaling drops SQL-NULL columns, so
    // an omitted key here and a `null` value are equivalent on disk.
    let mut msg = serde_json::Map::new();
    msg.insert("id".into(), json!(assistant_message_id));
    msg.insert("type".into(), json!("message"));
    msg.insert("role".into(), json!("ASSISTANT"));
    msg.insert("content".into(), json!(content));
    msg.insert("createdAt".into(), json!(now));
    if let Some(t) = token_count {
        msg.insert("tokenCount".into(), json!(t));
    }
    if let Some(t) = prompt_tokens {
        msg.insert("promptTokens".into(), json!(t));
    }
    if let Some(t) = completion_tokens {
        msg.insert("completionTokens".into(), json!(t));
    }
    if let Some(raw) = raw_response {
        if !raw.is_null() {
            msg.insert("rawResponse".into(), raw.clone());
        }
    }
    // v4: `attachments: generatedImagePaths.map(img => img.id)` — the generated
    // image ids (empty on the preserve/recovery path).
    msg.insert("attachments".into(), json!(generated_image_ids));
    if let Some(ts) = thought_signature.filter(|s| !s.is_empty()) {
        msg.insert("thoughtSignature".into(), json!(ts));
    }
    if let Some(rc) = reasoning_content.filter(|s| !s.is_empty()) {
        msg.insert("reasoningContent".into(), json!(rc));
    }
    if !reasoning_segments.is_empty() {
        let segs: Vec<Value> = reasoning_segments
            .iter()
            .map(|s| {
                json!({
                    "anchorOffset": s.anchor_offset,
                    "content": s.content,
                    "seq": s.seq,
                })
            })
            .collect();
        msg.insert("reasoningSegments".into(), json!(segs));
    }
    msg.insert("participantId".into(), json!(ctx.participant_id));
    if let Some(p) = provider.filter(|s| !s.is_empty()) {
        msg.insert("provider".into(), json!(p));
    }
    if let Some(m) = model_name.filter(|s| !s.is_empty()) {
        msg.insert("modelName".into(), json!(m));
    }
    if let Some(silent) = is_silent {
        msg.insert("isSilentMessage".into(), json!(silent));
    }
    // Answer-confirmation keys (v4 spreads only the keys that are `!== undefined`).
    // `confirmationChecked` is derived: `confirmed !== undefined ? true : undefined`.
    if let Some(v) = confirmation.confirmed.to_json() {
        msg.insert("confirmed".into(), v);
        msg.insert("confirmationChecked".into(), json!(true));
    }
    if let Some(v) = confirmation.revised.to_json() {
        msg.insert("confirmationRevised".into(), v);
    }
    if let Some(v) = confirmation.notes.to_json() {
        msg.insert("confirmationNotes".into(), v);
    }
    if let Some(v) = confirmation.original_content.to_json() {
        msg.insert("confirmationOriginalContent".into(), v);
    }

    let event: crate::db::chats_messages::ChatEventInput =
        serde_json::from_value(Value::Object(msg))
            .map_err(|e| DbError::Internal(format!("saveAssistantMessage marshal: {e}")))?;
    writer.chat_messages().add_message(chat_id, &event)?;

    // Save tool messages (v4 message-finalizer.service.ts:635, `if
    // toolMessages.length > 0`) BEFORE the assistant image-link loop, so a
    // generated image's `linkedTo` order is `[firstToolMessageId,
    // assistantMessageId]` exactly as v4. v4 passes `userId: ''`; the whisper
    // context + character/participant ids gate the TOOL-row whisper targeting.
    // Dormant in the current corpus (the tool loops W4.1e/f supply a non-empty
    // slate); the primitive itself is proven by `tool_execution_tier2`.
    if !tool_messages.is_empty() {
        save_tool_messages(
            writer,
            chat_id,
            "",
            tool_messages,
            generated_image_paths,
            Some(ctx.character_id),
            Some(ctx.participant_id),
            whisper_context,
        )?;
    }

    // Link each generated-image attachment to the assistant message (v4's
    // `for (const imageId of assistantAttachments) repos.files.addLink(imageId,
    // assistantMessageId)`). A missing file / failed link is swallowed (v4
    // catches + logs a warn) so the message still stands.
    for image_id in &generated_image_ids {
        let _ = writer.files().add_link(image_id, &assistant_message_id);
    }
    Ok(assistant_message_id)
}

// ===========================================================================
// makePreservePartialOnError
// ===========================================================================

/// The idempotent partial-response preserver (v4 `makePreservePartialOnError`).
/// The first `preserve` call that finds streamed content writes it to the DB with
/// an OOC marker; subsequent calls no-op so the original error still propagates.
/// (v4's closure captures a `partialPreserved` flag; here it is an owned struct.)
pub struct PreservePartialOnError {
    chat_id: String,
    character_name: String,
    character_aliases: Vec<String>,
    participant_id: String,
    participant_status: Option<String>,
    character_id: String,
    pre_generated_assistant_message_id: String,
    partial_preserved: bool,
}

impl PreservePartialOnError {
    /// Build the preserver for this turn.
    pub fn new(
        chat_id: impl Into<String>,
        character_id: impl Into<String>,
        character_name: impl Into<String>,
        character_aliases: Vec<String>,
        participant_id: impl Into<String>,
        participant_status: Option<String>,
        pre_generated_assistant_message_id: impl Into<String>,
    ) -> Self {
        Self {
            chat_id: chat_id.into(),
            character_id: character_id.into(),
            character_name: character_name.into(),
            character_aliases,
            participant_id: participant_id.into(),
            participant_status,
            pre_generated_assistant_message_id: pre_generated_assistant_message_id.into(),
            partial_preserved: false,
        }
    }

    /// Preserve the partial `state.full_response` on an upstream error — exactly
    /// once for the turn. Clean the text (`normalizeContentBlockFormat` →
    /// `stripCharacterNamePrefix`), append the OOC marker, and write through the
    /// `Db` writer. A persist failure is swallowed (v4 logs and returns) so the
    /// original stream error still propagates.
    pub async fn preserve(&mut self, db: &Db, state: &StreamingState, error_reason: &str) {
        if self.partial_preserved {
            return;
        }
        if !state.has_started_streaming || state.full_response.is_empty() {
            return;
        }
        self.partial_preserved = true;

        let normalized = normalize_content_block_format(&state.full_response);
        let cleaned = strip_character_name_prefix(
            &normalized,
            Some(&self.character_name),
            Some(&self.character_aliases),
        );
        let preserved_content = format!(
            "{}\n\n{{{{OOC: stream ended abruptly ({error_reason})}}}}",
            crate::jsstr::js_trim_end(&cleaned)
        );

        // Snapshot the fields the write needs (the closure must be `'static`).
        let chat_id = self.chat_id.clone();
        let character_id = self.character_id.clone();
        let character_name = self.character_name.clone();
        let participant_id = self.participant_id.clone();
        let participant_status = self.participant_status.clone();
        let usage = state.usage;
        let raw_response = state.raw_response.clone();
        let thought_signature = state.thought_signature.clone();
        let pre_id = self.pre_generated_assistant_message_id.clone();
        let provider = state.effective_profile.as_ref().map(|p| p.provider.clone());
        let model_name = state
            .effective_profile
            .as_ref()
            .map(|p| p.model_name.clone());
        let reasoning_content = state.reasoning_content.clone();
        let reasoning_segments = state.reasoning_segments.clone();

        let write = db
            .write(move |writers| {
                let ctx = AssistantMessageContext {
                    character_id: &character_id,
                    character_name: &character_name,
                    participant_id: &participant_id,
                    participant_status: participant_status.as_deref(),
                };
                save_assistant_message(
                    writers.main(),
                    &chat_id,
                    &ctx,
                    &preserved_content,
                    usage,
                    raw_response.as_ref(),
                    thought_signature.as_deref(),
                    Some(&pre_id),
                    provider.as_deref(),
                    model_name.as_deref(),
                    reasoning_content.as_deref(),
                    &reasoning_segments,
                    // The preserve path carries no generated images, no tool
                    // messages, and no confirmation keys (v4's preserve write sets
                    // none of them).
                    &[],
                    &[],
                    None,
                    &ConfirmationFields::default(),
                )
                .map(|_| ())
            })
            .await;

        // Swallow-and-log on persist failure (v4 catches persistError and logs).
        if let Err(_e) = write {
            // A structured logger lands with the transport layer; the differential
            // asserts DB state, so a swallowed failure surfaces as an absent row.
        }
    }

    /// Test / caller accessor: whether a partial has already been preserved.
    pub fn preserved(&self) -> bool {
        self.partial_preserved
    }
}

// ===========================================================================
// runPrimaryStream
// ===========================================================================

/// The early-return short-circuit (v4 `ProcessMessageResult`) that
/// `attempt_request_limit_recovery` produces on success. The orchestrator returns
/// this instead of continuing into the tool loop.
#[derive(Clone, Debug, PartialEq)]
pub struct EarlyReturn {
    pub is_multi_character: bool,
    pub has_content: bool,
    pub message_id: Option<String>,
    pub user_participant_id: Option<String>,
    pub is_paused: bool,
}

/// The result of [`run_primary_stream`] (v4 `PrimaryStreamResult`): `early_return`
/// is `Some` when request-limit recovery handled the whole request.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrimaryStreamResult {
    pub early_return: Option<EarlyReturn>,
}

/// The chat metadata the primary stream reads (v4 `ChatMetadataBase` subset:
/// `isPaused`).
#[derive(Clone, Debug)]
pub struct PrimaryStreamChat {
    pub is_paused: bool,
}

/// Everything [`run_primary_stream`] needs. Mirrors v4's `RunPrimaryStreamOptions`
/// minus the pieces resolved above the seam (tool build, attachment delivery).
pub struct RunPrimaryStreamOptions<'a> {
    pub chat_id: String,
    pub user_id: String,
    pub chat: PrimaryStreamChat,
    pub character_id: String,
    pub character_name: String,
    pub character_aliases: Vec<String>,
    pub participant_id: String,
    pub participant_status: Option<String>,
    pub user_participant_id: Option<String>,
    pub is_multi_character: bool,
    /// The base stream params (messages / model / temperature / …) built above
    /// the seam. `run_primary_stream` clears `tools` for the retry.
    pub params: StreamParams,
    /// The attachments carried for recovery message building (v4 `attachedFiles`).
    pub attached_files: Vec<super::recovery::AttachedFile>,
    pub original_message: Option<String>,
    pub pre_generated_assistant_message_id: String,
    /// The [`LogContext`] stamped onto this stream's terminal `CHAT_MESSAGE`
    /// `llm_logs` row (U4.4, spec decision #4 — the explicit replacement for
    /// v4's ambient `runWithAutonomousRunId` AsyncLocalStorage). Defaults to
    /// [`LogContext::none()`] (`Default`), so every pre-existing caller and
    /// corpus is untouched; the autonomous turn passes
    /// `LogContext { autonomous_run_id: Some(run_id) }`.
    pub log_context: LogContext,
    /// The mutable streaming state (holds `effective_profile` / `effective_api_key`).
    pub state: &'a mut StreamingState,
}

/// The context [`consume_stream`] needs to write the v4 `CHAT_MESSAGE` `llm_logs`
/// row on the terminal chunk (streaming.service.ts:405). `None` when the caller
/// did not supply a `user_id` (v4 gates the log on `if (userId)`).
///
/// `pub(crate)` + [`log_chat_message_call`] so the provider-failover retry legs
/// reuse the SAME row construction (v4 logs per `streamMessage` call; the
/// `restreamInto` call site passes no `characterId` → `None` here, W4.11b).
pub(crate) struct StreamLogCtx<'a> {
    pub(crate) db: &'a Db,
    pub(crate) user_id: &'a str,
    pub(crate) chat_id: &'a str,
    pub(crate) message_id: &'a str,
    /// v4's primary/tool-retry stream passes `characterId`; `restreamInto`
    /// (failover) passes none → the log row's `characterId` is NULL there.
    pub(crate) character_id: Option<&'a str>,
    /// The explicit run-id context replacing v4's ambient `AsyncLocalStorage`
    /// (U4.4, spec decision #4): the autonomous turn wraps its whole generation
    /// in `runWithAutonomousRunId(runId, …)`, so the terminal `CHAT_MESSAGE`
    /// row is stamped with `autonomousRunId` for per-run budget accounting.
    /// `LogContext::none()` on every request-path caller.
    pub(crate) log_context: &'a LogContext,
    /// v4 `streaming.service.ts:382` — `const startTime = Date.now()`, captured
    /// immediately before the provider loop and subtracted at the terminal
    /// chunk (`:440`). Each `streamMessage` invocation gets its OWN start, so
    /// the primary attempt, the tool-unsupported retry, and every failover leg
    /// each time only their own call — which is why this rides the per-stream
    /// context rather than the enclosing request.
    pub(crate) started_at_ms: i64,
}

/// v4 `logLLMCall` on `chunk.done` (streaming.service.ts:405) — one `CHAT_MESSAGE`
/// row with the request-prefix hashes, `rawProviderUsage`, `finishReason`, usage +
/// cacheUsage. `durationMs` is v4's `Date.now() - startTime`, measured from
/// [`StreamLogCtx::started_at_ms`]. (It emitted a hard-coded 0 until the
/// 2026-08-22 dogfood pass found that every streamed chat message logged a zero
/// duration where v4 logs a real one — dogfood finding #100. The deferral's
/// stated blocker, that a measured clock could not be diffed, had already been
/// lifted by `common::normalize_duration_ms`, which collapses any non-NULL
/// duration to `"<ms>"` on BOTH sides and keeps NULL NULL.) Awaited (the
/// writer never throws — the watermark precedent). The [`LogContext`] rides
/// [`StreamLogCtx::log_context`] — none on the request path, the run's id under
/// an autonomous turn (U4.4).
/// The CHAT_MESSAGE log's request-message projection — v4 logs
/// `attachments: m.attachments` verbatim (streaming.service.ts:452-455), so the
/// LLM Inspector shows the attachment bags on vision sends; a message with no
/// attachments stays absent in the log. (The §3 unification-review fix: P4.21
/// unit 1 threaded the wire but left this log projection on `None`.)
fn log_request_messages(messages: &[StreamMessage]) -> Vec<LogRequestMessage> {
    messages
        .iter()
        .map(|m| LogRequestMessage {
            role: m.role_str().to_string(),
            content: m.content().to_string(),
            attachments: match m.attachments() {
                [] => None,
                a => Some(a.to_vec()),
            },
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn log_chat_message_call(
    log: &StreamLogCtx<'_>,
    profile: &EffectiveProfile,
    params: &StreamParams,
    content: String,
    usage: Option<StreamUsage>,
    cache_usage: Option<StreamCacheUsage>,
    raw_provider_usage: Option<Value>,
    raw_response: Option<Value>,
) {
    // v4 passes `tools.length > 0 ? tools : undefined` to both the log request and
    // `computeRequestPrefixHashes`.
    let tools_nonempty: Option<Vec<Value>> = params
        .tools
        .as_ref()
        .and_then(|t| t.as_array())
        .filter(|a| !a.is_empty())
        .cloned();

    let prefix_messages: Vec<PrefixMessage> = params
        .messages
        .iter()
        .map(|m| PrefixMessage {
            role: m.role_str().to_string(),
            content: Value::String(m.content().to_string()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        })
        .collect();
    let request_hashes = compute_request_prefix_hashes(&prefix_messages, tools_nonempty.as_deref());

    let finish_reason = raw_response.as_ref().and_then(extract_finish_reason);

    let params_log = LogLlmCallParams {
        user_id: log.user_id.to_string(),
        log_type: log_type::CHAT_MESSAGE.to_string(),
        message_id: Some(log.message_id.to_string()),
        chat_id: Some(log.chat_id.to_string()),
        character_id: log.character_id.map(str::to_string),
        provider: profile.provider.clone(),
        model_name: profile.model_name.clone(),
        // v4 `0cde7fbc` streaming.service.ts:450 — `connectionProfile.id`, plain
        // (not `?? null`): the streaming path always has the effective profile.
        connection_profile_id: Some(profile.id.clone()),
        image_profile_id: None,
        request: LogRequest {
            messages: log_request_messages(&params.messages),
            temperature: params.temperature,
            max_tokens: params.max_tokens,
            tools: tools_nonempty,
        },
        response: LogResponse {
            content,
            error: None,
            finish_reason,
            tool_calls: None,
        },
        usage: usage.map(|u| LogUsage {
            prompt_tokens: Some(u.prompt_tokens),
            completion_tokens: Some(u.completion_tokens),
            total_tokens: Some(u.total_tokens),
        }),
        cache_usage: cache_usage.map(|c| LogCacheUsage {
            cache_creation_input_tokens: c.cache_creation_input_tokens,
            cache_read_input_tokens: c.cache_read_input_tokens,
        }),
        raw_provider_usage,
        request_hashes: Some(request_hashes),
        duration_ms: Some((crate::clock::now_unix_ms() - log.started_at_ms) as f64),
    };
    let _ = log_llm_call(log.db, params_log, log.log_context).await;
}

/// Drain a canned stream, applying every chunk to the state exactly as v4's
/// `for await` loop body does; return the mid-stream error if one arrived. On the
/// terminal chunk, writes the v4 `CHAT_MESSAGE` `llm_logs` row when `log` is set.
async fn consume_stream<P, S>(
    provider: &P,
    state: &mut StreamingState,
    sink: &S,
    character_name: &str,
    character_id: &str,
    params: &StreamParams,
    log: Option<&StreamLogCtx<'_>>,
) -> Option<StreamError>
where
    P: StreamingCompletionProvider,
    S: EventSink,
{
    let (provider_name, base_url) = match &state.effective_profile {
        Some(p) => (p.provider.clone(), p.base_url.clone()),
        None => (String::new(), None),
    };
    // v4 tracks the LAST non-null usage/cache/rawProviderUsage across all chunks
    // for the terminal log.
    let mut last_usage: Option<StreamUsage> = None;
    let mut last_cache_usage: Option<StreamCacheUsage> = None;
    let mut last_raw_provider_usage: Option<Value> = None;
    let mut rx = provider
        .stream_message(&provider_name, base_url.as_deref(), params)
        .await;
    while let Some(item) = rx.recv().await {
        let chunk = match item {
            Ok(c) => c,
            Err(e) => return Some(e),
        };
        // Capture + live-forward reasoning on any chunk. DISPLAY ONLY.
        apply_reasoning_chunk(state, &chunk, sink);
        if chunk.usage.is_some() {
            last_usage = chunk.usage;
        }
        if chunk.cache_usage.is_some() {
            last_cache_usage = chunk.cache_usage;
        }
        if chunk
            .raw_provider_usage
            .as_ref()
            .is_some_and(|v| v.is_object())
        {
            last_raw_provider_usage = chunk.raw_provider_usage.clone();
        }
        if !chunk.content.is_empty() {
            if !state.has_started_streaming {
                sink.emit(ChatEvent::status(StatusPayload {
                    stage: "streaming".into(),
                    message: format!("{character_name} is responding..."),
                    tool_name: None,
                    character_name: Some(character_name.to_string()),
                    character_id: Some(character_id.to_string()),
                }));
                state.has_started_streaming = true;
            }
            // Prose resumed — close any pending reasoning run.
            flush_reasoning_segment(state);
            state.full_response.push_str(&chunk.content);
            sink.emit(ChatEvent::content(chunk.content.clone()));
        }
        if chunk.done {
            state.usage = chunk.usage;
            state.cache_usage = chunk.cache_usage;
            state.attachment_results = attachment_results_to_value(&chunk.attachment_results);
            state.raw_response = chunk.raw_response.clone();
            if let Some(ts) = chunk.thought_signature.as_ref() {
                if !ts.is_empty() {
                    state.thought_signature = Some(ts.clone());
                }
            }
            // Flush any trailing reasoning that arrived without subsequent prose.
            flush_reasoning_segment(state);

            // v4 logs on `chunk.done` when a userId was provided.
            if let Some(log) = log {
                if let Some(profile) = state.effective_profile.clone() {
                    log_chat_message_call(
                        log,
                        &profile,
                        params,
                        state.full_response.clone(),
                        last_usage,
                        last_cache_usage,
                        last_raw_provider_usage.clone(),
                        chunk.raw_response.clone(),
                    )
                    .await;
                }
            }
        }
    }
    None
}

pub(crate) fn attachment_results_to_value(
    r: &Option<crate::model::stream::StreamAttachmentResults>,
) -> Option<Value> {
    r.as_ref().map(|res| {
        let failed: Vec<Value> = res
            .failed
            .iter()
            .map(|f| json!({ "id": f.id, "error": f.error }))
            .collect();
        json!({ "sent": res.sent, "failed": failed })
    })
}

/// Run the orchestrator's primary (post-context-build) stream, including the
/// tool-unsupported retry-without-tools and the request-limit recovery branches.
/// The two model boundaries (`P` streaming, and recovery's own `P` stream) are
/// generic; the writer is the `Db`.
pub async fn run_primary_stream<P, S>(
    db: &Db,
    provider: &P,
    sink: &S,
    preserve: &mut PreservePartialOnError,
    opts: RunPrimaryStreamOptions<'_>,
) -> Result<PrimaryStreamResult, StreamError>
where
    P: StreamingCompletionProvider,
    S: EventSink,
{
    let RunPrimaryStreamOptions {
        chat_id,
        user_id,
        chat,
        character_id,
        character_name,
        character_aliases: _character_aliases,
        participant_id,
        participant_status,
        user_participant_id,
        is_multi_character,
        params,
        attached_files,
        original_message,
        // v4 logs the CHAT_MESSAGE `llm_logs` row against this id (the
        // pre-generated assistant message id).
        pre_generated_assistant_message_id,
        log_context,
        state,
    } = opts;

    // v4 gates the terminal `logLLMCall` on `if (userId)`.
    let stream_log = (!user_id.is_empty()).then(|| StreamLogCtx {
        db,
        user_id: &user_id,
        chat_id: &chat_id,
        message_id: &pre_generated_assistant_message_id,
        character_id: Some(&character_id),
        log_context: &log_context,
        started_at_ms: crate::clock::now_unix_ms(),
    });

    sink.emit(ChatEvent::status(StatusPayload {
        stage: "sending".into(),
        message: format!("Sending to {character_name}..."),
        tool_name: None,
        character_name: Some(character_name.clone()),
        character_id: Some(character_id.clone()),
    }));

    let stream_err = consume_stream(
        provider,
        state,
        sink,
        &character_name,
        &character_id,
        &params,
        stream_log.as_ref(),
    )
    .await;

    let Some(err) = stream_err else {
        return Ok(PrimaryStreamResult::default());
    };

    let had_tools = params
        .tools
        .as_ref()
        .map(|t| t.as_array().map(|a| !a.is_empty()).unwrap_or(true))
        .unwrap_or(false);

    // --- Tool-unsupported retry ---
    if is_tool_unsupported_error(&err.message) && had_tools {
        sink.emit(ChatEvent::status(StatusPayload {
            stage: "sending".into(),
            message: format!("Retrying without tools for {character_name}..."),
            tool_name: None,
            character_name: Some(character_name.clone()),
            character_id: Some(character_id.clone()),
        }));

        let mut retry_params = params.clone();
        retry_params.tools = None;
        // The retry re-issues the SAME messages (the tool schemas were never in
        // the message body), so it must key to the same canned stream slot as the
        // primary call keyed on (provider, model, temperature, messages).
        let _ = canned_stream_key::<crate::model::stream::StreamMessage>; // documented: same key discipline as the seam.

        // v4's retry `streamMessage` call (primary-stream.service.ts:246–256) passes
        // NO `characterId` (unlike the primary attempt at :192), so the retry's
        // CHAT_MESSAGE log row carries `characterId = NULL`.
        let retry_stream_log = (!user_id.is_empty()).then(|| StreamLogCtx {
            db,
            user_id: &user_id,
            chat_id: &chat_id,
            message_id: &pre_generated_assistant_message_id,
            character_id: None,
            log_context: &log_context,
            started_at_ms: crate::clock::now_unix_ms(),
        });

        let retry_err = consume_stream(
            provider,
            state,
            sink,
            &character_name,
            &character_id,
            &retry_params,
            retry_stream_log.as_ref(),
        )
        .await;

        return match retry_err {
            None => Ok(PrimaryStreamResult::default()),
            Some(retry_error) => {
                preserve.preserve(db, state, &retry_error.message).await;
                Err(retry_error)
            }
        };
    }

    // --- Request-limit recovery ---
    if is_recoverable_request_error(&err.message) {
        let connection_profile = state.effective_profile.clone();
        let recovery = super::recovery::attempt_request_limit_recovery(
            db,
            provider,
            sink,
            super::recovery::RecoveryContext {
                chat_id: chat_id.clone(),
                character_name: character_name.clone(),
                connection_profile,
                api_key: state.effective_api_key.clone(),
                attached_files,
                original_message,
                error_message: err.message.clone(),
                character_participant_id: participant_id.clone(),
            },
        )
        .await;

        if recovery.success {
            return Ok(PrimaryStreamResult {
                early_return: Some(EarlyReturn {
                    is_multi_character,
                    has_content: true,
                    message_id: recovery.message_id,
                    user_participant_id,
                    is_paused: chat.is_paused,
                }),
            });
        }
        // recovery failed — fall through to preserve + rethrow.
    }

    preserve.preserve(db, state, &err.message).await;
    let _ = participant_status; // consumed by the preserve ctx above.
    Err(err)
}

// ===========================================================================
// findPreviousResponseId
// ===========================================================================

/// v4 `findPreviousResponseId` — extract the OpenAI Responses-API
/// `previousResponseId` from the most recent assistant message. Returns `None`
/// for any non-OPENAI provider or when no chainable response is found.
///
/// `existing_messages` are the read-side `chat_messages` values (a
/// `MessageEvent` has `type`/`role`/`rawResponse`); the scan walks newest-first
/// and returns the first assistant `rawResponse.id` starting with `resp_`.
pub fn find_previous_response_id(provider: &str, existing_messages: &[Value]) -> Option<String> {
    if provider != "OPENAI" {
        return None;
    }
    for msg in existing_messages.iter().rev() {
        let is_message = msg.get("type").and_then(Value::as_str) == Some("message");
        let is_assistant = msg.get("role").and_then(Value::as_str) == Some("ASSISTANT");
        if is_message && is_assistant {
            if let Some(raw) = msg.get("rawResponse") {
                if let Some(id) = raw.get("id").and_then(Value::as_str) {
                    if id.starts_with("resp_") {
                        return Some(id.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::chat_events::RecordingSink;

    /// The §3-review pin: the CHAT_MESSAGE log projection carries each
    /// message's attachment bags (v4 streaming.service.ts:452-455), None when
    /// a message has none.
    #[test]
    fn log_request_messages_carry_attachment_bags() {
        let bag = serde_json::json!({
            "id": "att-1", "filename": "boat.png", "mimeType": "image/png",
            "size": 3, "data": "AAAA"
        });
        let msgs = vec![
            StreamMessage::system("sys"),
            StreamMessage::User {
                content: "look".to_string(),
                cache_control: None,
                attachments: vec![bag.clone()],
            },
        ];
        let logged = log_request_messages(&msgs);
        assert_eq!(logged[0].attachments, None);
        assert_eq!(logged[1].attachments, Some(vec![bag]));
    }

    #[test]
    fn locale_string_groups_thousands() {
        assert_eq!(to_locale_string(0), "0");
        assert_eq!(to_locale_string(999), "999");
        assert_eq!(to_locale_string(1000), "1,000");
        assert_eq!(to_locale_string(210311), "210,311");
        assert_eq!(to_locale_string(200000), "200,000");
        assert_eq!(to_locale_string(1234567), "1,234,567");
    }

    #[test]
    fn token_limit_classification_and_parse() {
        // The real Anthropic-shape message: matches `prompt is too long` /
        // `maximum.*tokens`, and carries the `>` form `parse` reads.
        assert!(is_token_limit_error(
            "prompt is too long: 210311 tokens > 200000 maximum"
        ));
        assert!(is_recoverable_request_error(
            "context_length_exceeded: too big"
        ));
        assert_eq!(
            parse_token_limit_error("210311 tokens > 200000 maximum"),
            (Some(210311), Some(200000))
        );
        assert_eq!(
            parse_token_limit_error("maximum context length is 128000 tokens"),
            (None, Some(128000))
        );
        assert_eq!(parse_token_limit_error("no numbers here"), (None, None));
    }

    #[test]
    fn content_limit_classification_and_parse() {
        assert!(is_content_limit_error(
            "Exceeds the maximum of 100 PDF pages"
        ));
        assert!(!is_token_limit_error(
            "Exceeds the maximum of 100 PDF pages"
        ));
        let (kind, max, desc) = parse_content_limit_error("Exceeds the maximum of 100 PDF pages");
        assert_eq!(kind, ContentLimitType::PdfPages);
        assert_eq!(max, Some(100));
        assert_eq!(
            desc.as_deref(),
            Some("PDF documents cannot exceed 100 pages")
        );
    }

    #[test]
    fn tool_unsupported_classification() {
        assert!(is_tool_unsupported_error(
            "Tool use with function calling is unsupported for this model"
        ));
        assert!(!is_tool_unsupported_error("some other error"));
    }

    #[test]
    fn apply_reasoning_last_wins_and_forwards_once() {
        let sink = RecordingSink::new();
        let mut state = StreamingState::default();
        let mut chunk = StreamChunk {
            reasoning_content: Some("thinking a".into()),
            ..Default::default()
        };
        apply_reasoning_chunk(&mut state, &chunk, &sink);
        // Same cumulative value again → no re-forward.
        apply_reasoning_chunk(&mut state, &chunk, &sink);
        chunk.reasoning_content = Some("thinking a b".into());
        apply_reasoning_chunk(&mut state, &chunk, &sink);
        assert_eq!(
            sink.events_json(),
            vec![
                serde_json::json!({ "reasoning": "thinking a" }),
                serde_json::json!({ "reasoning": "thinking a b" }),
            ]
        );
        assert_eq!(state.reasoning_content.as_deref(), Some("thinking a b"));
    }

    #[test]
    fn flush_reasoning_segments_slice_the_cumulative_buffer() {
        let mut state = StreamingState {
            full_response: "Hello".into(),
            reasoning_content: Some("first".into()),
            ..Default::default()
        };
        flush_reasoning_segment(&mut state);
        // More reasoning after some prose.
        state.full_response = "Hello world".into();
        state.reasoning_content = Some("firstsecond".into());
        flush_reasoning_segment(&mut state);
        assert_eq!(state.reasoning_segments.len(), 2);
        assert_eq!(state.reasoning_segments[0].content, "first");
        assert_eq!(state.reasoning_segments[0].anchor_offset, 5);
        assert_eq!(state.reasoning_segments[0].seq, 0);
        assert_eq!(state.reasoning_segments[1].content, "second");
        assert_eq!(state.reasoning_segments[1].anchor_offset, 11);
        assert_eq!(state.reasoning_segments[1].seq, 1);
    }

    #[test]
    fn flush_skips_whitespace_only_but_advances_cursor() {
        let mut state = StreamingState {
            reasoning_content: Some("   ".into()),
            ..Default::default()
        };
        flush_reasoning_segment(&mut state);
        assert!(state.reasoning_segments.is_empty());
        assert_eq!(state.reasoning_flushed_len, 3);
    }

    #[test]
    fn find_previous_response_id_openai_hit_and_early_out() {
        let msgs = vec![
            serde_json::json!({
                "type": "message", "role": "ASSISTANT",
                "rawResponse": { "id": "resp_aaa" }
            }),
            serde_json::json!({ "type": "message", "role": "USER", "content": "hi" }),
            serde_json::json!({
                "type": "message", "role": "ASSISTANT",
                "rawResponse": { "id": "resp_bbb" }
            }),
        ];
        // Newest-first → resp_bbb.
        assert_eq!(
            find_previous_response_id("OPENAI", &msgs),
            Some("resp_bbb".into())
        );
        // Non-OPENAI early-out.
        assert_eq!(find_previous_response_id("ANTHROPIC", &msgs), None);
        // No chainable response.
        let none = vec![serde_json::json!({
            "type": "message", "role": "ASSISTANT",
            "rawResponse": { "id": "chatcmpl_x" }
        })];
        assert_eq!(find_previous_response_id("OPENAI", &none), None);
    }
}
