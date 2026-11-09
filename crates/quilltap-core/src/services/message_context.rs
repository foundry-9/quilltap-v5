//! Port of v4's `lib/services/chat-message/context-builder.service.ts` —
//! `buildMessageContext`, the wrapper that sits between `processMessage` and
//! `buildContext`. It pre-filters/normalizes the whisper stream, builds the
//! conversation-message list, calls the ported [`build_context`], then
//! post-processes the result for the provider (multi-character name attribution,
//! the Lantern image merge, the trailing-prefix injection, and the
//! multi-character scene block).
//!
//! Structure (v4 line refs in `context-builder.service.ts`):
//!   - Pure leaves (tier-1): [`build_conversation_messages`] (313–398),
//!     [`normalize_whisper_roles`] (433–453),
//!     [`collect_lantern_image_file_ids_for_character`] (217–266).
//!   - The composition [`build_message_context`] (458–799): the A–N sections
//!     (commonplace strip / announcement attribution / whisper filter /
//!     opaque-anywhere / whisper
//!     re-role / conversation build / recap decision / timezone / buildContext /
//!     provider formatting / Lantern merge / final messages / scene block).
//!
//! The K file-loading half (`loadChatFilesForLLM`, `processFileAttachmentFallback`,
//! `formatFallbackAsMessagePrefix`) is the unported wave-4 file subsystem, so it is
//! an injected [`MessageContextSeams`] (default no-op). The pure id-collection leaf
//! IS ported and exercised; the corpus keeps message attachments empty so the leaf
//! returns `[]` and the seam is never reached.
//!
//! JS-fidelity notes: string ops use the UTF-16 [`crate::jsstr`] primitives; the
//! provider name-attribution + prefixing reuse the ported
//! [`crate::message_formatter`] (incl. the `LEGACY_PROVIDER_NAME_SUPPORT` quirk);
//! `??` (nullish) vs `||` (falsy) are reproduced exactly.

use serde_json::Value;

use crate::announcement_attribution::{
    attribute_adhoc_announcements, collect_announcer_character_ids, AnnouncerAttributable,
    CustomAnnouncer,
};
use crate::db::runtime::Db;
use crate::message_formatter::{self, FormattedMessage, MultiCharacterMessage, WireRole};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::build_context::{
    self, BuildContextError, BuildContextInput, BuildContextSeams, BuiltContext, ExistingMessage,
    MessageWithParticipant,
};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
// === P4.D367 ===
use crate::services::image_transport_budget::{
    has_persisted_image_description, tag_for_transport, ImageTransportOrigin, ImageTransportTag,
    TransportLogContext,
};
// === /P4.D367 ===

/// Number of ASSISTANT messages that must appear *after* a TOOL message before its
/// result body is elided (v4 `TOOL_RESULT_VERBATIM_TURNS`).
pub const TOOL_RESULT_VERBATIM_TURNS: i64 = 3;

/// How many ASSISTANT messages the Lantern walk scans before giving up (v4
/// `ASSISTANT_IMAGE_LOOKBACK`).
pub const ASSISTANT_IMAGE_LOOKBACK: usize = 6;

// ===========================================================================
// The whisper-pipeline message shape.
// ===========================================================================

/// A message as `buildMessageContext` reads it (the union of every field the
/// filters + leaves touch). Parsed from the raw `chat_messages` event JSON.
#[derive(Clone, Debug, Default)]
pub struct WhisperMessage {
    /// v4 `type` (only `"message"` events flow into the conversation).
    pub message_type: Option<String>,
    pub role: Option<String>,
    pub content: Option<String>,
    /// v4 `opaqueContent` (`string | null` — the persona-free body for the
    /// opaque-anywhere swap). `None` = null/absent.
    pub opaque_content: Option<String>,
    pub id: Option<String>,
    pub thought_signature: Option<String>,
    pub participant_id: Option<String>,
    pub target_participant_ids: Option<Vec<String>>,
    pub created_at: Option<String>,
    pub attachments: Option<Vec<String>>,
    pub system_sender: Option<String>,
    pub system_kind: Option<String>,
    /// v4 `customAnnouncer` (P4.D37 / `424a7381`) — who the operator signed an
    /// ad-hoc announcement as. A rendering field until the attribution pass gave
    /// it a voice in context. (`courier_transport` already read the same column
    /// for its own transcript; that parity is what the pass restores.)
    pub custom_announcer: Option<CustomAnnouncer>,
    // === P4.D372 ===
    /// v4 `isEstablishedFact` — `Some(b)` only for a JSON boolean (the seat-line
    /// predicate is v4's strict `=== true`).
    pub is_established_fact: Option<bool>,
    // === /P4.D372 ===
}

// === P4.D372 ===
impl WhisperMessage {
    /// The three fields the established-fact predicates read.
    pub(crate) fn fact_fields(&self) -> crate::established_fact::EstablishedFactFields<'_> {
        crate::established_fact::EstablishedFactFields {
            is_established_fact: self.is_established_fact,
            system_sender: self.system_sender.as_deref(),
            system_kind: self.system_kind.as_deref(),
        }
    }
}

/// v4 `frameEstablishedFacts` over the typed history (the twin of
/// [`crate::established_fact::frame_established_facts`]): a machine fact is
/// framed in BOTH `content` and `opaqueContent` (so whichever body the later
/// Staff swap picks reads the frame); a seat line in `content` only. Every
/// other row passes through untouched. The frame reads `content` (absent →
/// `''`) and never the stored `opaqueContent`.
pub(crate) fn frame_established_fact_messages(
    msgs: &[WhisperMessage],
    voice: crate::established_fact::EstablishedFactVoice,
) -> Vec<WhisperMessage> {
    use crate::established_fact::{
        established_fact_kind_of, frame_established_fact, EstablishedFactKind,
    };
    msgs.iter()
        .map(|m| {
            let Some(kind) = established_fact_kind_of(&m.fact_fields()) else {
                return m.clone();
            };
            let framed = frame_established_fact(m.content.as_deref().unwrap_or(""), kind, voice);
            let mut out = m.clone();
            if kind == EstablishedFactKind::Announcement {
                out.opaque_content = Some(framed.clone());
            }
            out.content = Some(framed);
            out
        })
        .collect()
}
/// v4 `buildMessageContext`'s established-fact step (`db6190b44`), between the
/// opacity rule and `normalizeWhisperRoles`: frame each fact row's body for
/// the model while `systemSender` / `systemKind` / `isEstablishedFact` are
/// still on the rows. The voice follows the same chat-wide opacity rule as the
/// Staff swap: if any LLM seat is opaque, everyone reads the plain frame with
/// no Host. The frame is never stored.
///
/// Returns the (possibly framed) history and, when any fact is present, the
/// ids of the fact rows taken BEFORE normalization erases the Host row's
/// `systemSender` (v4's `m.id && isEstablishedFact(m)`) — the multi-character
/// MARK's set. Logs v4's `[Context] Framed established facts` DEBUG only when
/// a fact is present.
pub(crate) fn frame_established_fact_pass<'m>(
    messages_after_whisper_filter: &'m [WhisperMessage],
    is_opaque_anywhere: bool,
    chat_id: &str,
) -> (
    std::borrow::Cow<'m, [WhisperMessage]>,
    Option<std::collections::HashSet<String>>,
) {
    use crate::established_fact::{
        count_established_facts, is_established_fact, EstablishedFactVoice,
    };
    let fact_voice = if is_opaque_anywhere {
        EstablishedFactVoice::Plain
    } else {
        EstablishedFactVoice::Host
    };
    let (seat_lines, machine_facts) = count_established_facts(
        messages_after_whisper_filter
            .iter()
            .map(WhisperMessage::fact_fields),
    );
    if seat_lines + machine_facts == 0 {
        return (
            std::borrow::Cow::Borrowed(messages_after_whisper_filter),
            None,
        );
    }
    tracing::debug!(
        target: "quilltap::context",
        chatId = %chat_id,
        seatLines = seat_lines,
        machineFacts = machine_facts,
        voice = fact_voice.as_str(),
        "[Context] Framed established facts"
    );
    let ids = messages_after_whisper_filter
        .iter()
        .filter(|m| is_established_fact(&m.fact_fields()))
        .filter_map(|m| m.id.clone().filter(|id| !id.is_empty()))
        .collect();
    (
        std::borrow::Cow::Owned(frame_established_fact_messages(
            messages_after_whisper_filter,
            fact_voice,
        )),
        Some(ids),
    )
}
// === /P4.D372 ===

/// v4 `isRecordOnlyMessage` (`context-builder.service.ts:847`, `e7d77bb60`):
/// true for a row that DOCUMENTS a delivery rather than being part of the
/// conversation — the Host's `inform` record (whatever its sender), a Host
/// "spoken of" card (`d68c1af88`), and every Commonplace Book whisper bar
/// `relevant-conversations`. Exact, case-sensitive
/// string matches, as v4's `===`.
///
/// P4.106 OUT-OF-MANDATE (§R.10(j)) — lifted from the inline closure in
/// [`build_message_context`] so the tier-1 `is_record_only_message_equivalence`
/// family can drive it; behaviour unchanged, and the record-only
/// strip there (section A) is still its ONLY production caller.
pub fn is_record_only_message(m: &WhisperMessage) -> bool {
    if m.system_kind.as_deref() == Some("inform") {
        return true;
    }
    // A Host "spoken of" card (v4 `d68c1af88`, `context-builder.service.ts:
    // 861`): the Salon shows it, the model hears about absent people from the
    // referent ledger instead. A left-behind notice is not a record.
    if crate::services::off_scene_kinds::is_off_scene_mention_introduction(
        m.system_sender.as_deref(),
        m.system_kind.as_deref(),
        m.content.as_deref(),
    ) {
        return true;
    }
    m.system_sender.as_deref() == Some("commonplaceBook")
        && m.system_kind.as_deref() != Some("relevant-conversations")
}

/// Row ids of the *human's* own turns, captured from the PRE-normalization
/// list — staff whispers are re-roled to USER by `normalize_whisper_roles`, so
/// after that pass "role === 'user'" no longer means "the user said it", and
/// the image attachment anchor needs it to (v4 `a14a1811`, bug 95). v4:
/// `m.type === 'message' && m.role === 'USER' && !m.systemSender && m.id`
/// (systemSender and id are JS-falsy for both absent and empty-string).
/// Extracted so the predicate is unit-pinned (the a14a1811 §3 review named the
/// exact silent mutation: a lowercase `"user"` here would empty the set, tier 2
/// would never fire, and every whisper-tailed regenerate would fall to the
/// last-user floor — the bug-95 symptom — with every other test green).
pub(crate) fn user_turn_message_ids(msgs: &[WhisperMessage]) -> std::collections::HashSet<String> {
    msgs.iter()
        .filter(|m| {
            m.message_type.as_deref() == Some("message")
                && m.role.as_deref() == Some("USER")
                && m.system_sender.as_deref().is_none_or(|s| s.is_empty())
        })
        .filter_map(|m| m.id.clone().filter(|id| !id.is_empty()))
        .collect()
}

impl WhisperMessage {
    /// Parse from the raw event JSON (the shape `chats_messages_read::get_messages`
    /// yields). Missing/typed-wrong fields → `None` (JS `undefined`).
    pub fn from_value(v: &Value) -> Self {
        let str_field = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
        let str_array = |k: &str| {
            v.get(k).and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
        };
        WhisperMessage {
            message_type: str_field("type"),
            role: str_field("role"),
            content: str_field("content"),
            opaque_content: str_field("opaqueContent"),
            id: str_field("id"),
            thought_signature: str_field("thoughtSignature"),
            participant_id: str_field("participantId"),
            target_participant_ids: str_array("targetParticipantIds"),
            created_at: str_field("createdAt"),
            attachments: str_array("attachments"),
            system_sender: str_field("systemSender"),
            system_kind: str_field("systemKind"),
            custom_announcer: CustomAnnouncer::from_value(v.get("customAnnouncer")),
            is_established_fact: v.get("isEstablishedFact").and_then(Value::as_bool), // P4.D372
        }
    }

    fn has_attachments(&self) -> bool {
        self.attachments
            .as_ref()
            .map(|a| !a.is_empty())
            .unwrap_or(false)
    }
}

/// The production carrier for the announcement-attribution pass (v4's
/// `existingMessages` element type gained `customAnnouncer` in `424a7381`).
impl AnnouncerAttributable for WhisperMessage {
    fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }
    fn custom_announcer(&self) -> Option<CustomAnnouncer> {
        self.custom_announcer.clone()
    }
    fn opaque_content(&self) -> Option<&str> {
        // `typeof m.opaqueContent === 'string'` — only a present string body takes
        // the opaque prefix; the field is `str_field`, so an absent/null column is
        // already `None`.
        self.opaque_content.as_deref()
    }
    fn system_sender(&self) -> Option<&str> {
        self.system_sender.as_deref()
    }
    fn system_kind(&self) -> Option<&str> {
        self.system_kind.as_deref()
    }
    /// v4's `{ ...m, content }` spread — every other field survives.
    fn with_content(&self, content: String) -> WhisperMessage {
        WhisperMessage {
            content: Some(content),
            ..self.clone()
        }
    }
    /// v4's `next.opaqueContent = …` — rewrites only `opaqueContent`. The A2 pass
    /// prefixes it BEFORE [`normalize_whisper_roles`] swaps it into the body for an
    /// opaque-anywhere chat, so the model's copy is tagged in that mode too.
    fn with_opaque_content(&self, opaque_content: String) -> WhisperMessage {
        WhisperMessage {
            opaque_content: Some(opaque_content),
            ..self.clone()
        }
    }
}

// ===========================================================================
// Pure leaves.
// ===========================================================================

/// A conversation message (v4 `buildConversationMessages` output element).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub id: Option<String>,
    /// Only set for ASSISTANT messages (v4 sets `undefined` otherwise).
    pub thought_signature: Option<String>,
}

/// Render the content string for a single TOOL message (v4
/// `renderToolResultContent`). `assistant_after` = number of ASSISTANT messages
/// that follow this TOOL msg; ≥ [`TOOL_RESULT_VERBATIM_TURNS`] elides the body.
///
/// v4 `JSON.stringify(toolData.arguments)` renders the arguments; a
/// `serde_json::Value`'s key order can diverge from v4's insertion order for a
/// multi-key object (the established wave-4 open-JSON seam — TOOL messages are a
/// wave-4 subsystem and never appear in this corpus).
fn render_tool_result_content(tool_data: &Value, assistant_after: i64) -> String {
    let tool_name = tool_data
        .get("toolName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            tool_data
                .get("tool")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("Unknown");
    if assistant_after >= TOOL_RESULT_VERBATIM_TURNS {
        let mut compact_args = String::new();
        let args = tool_data.get("arguments");
        if let Some(args) = args.filter(|a| !a.is_null()) {
            let raw = serde_json::to_string(args).unwrap_or_default();
            compact_args = if crate::jsstr::utf16_len(&raw) > 200 {
                format!("{}…", crate::jsstr::utf16_truncate(&raw, 200))
            } else {
                raw
            };
        }
        return format!(
            "[Tool Result: {tool_name}] (args: {compact_args}) — result elided (>3 turns old); call again to re-read."
        );
    }
    // `result !== undefined && result !== null && result !== ''` → String(result).
    let result_text = match tool_data.get("result") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(v) if !v.is_null() => js_string_of(v),
        _ => "No result".to_string(),
    };
    format!("[Tool Result: {tool_name}]\n{result_text}")
}

/// v4 `String(value)` for a non-null JSON value (best-effort; TOOL args/results
/// are wave-4 and untested here).
fn js_string_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// v4 `buildConversationMessages` (313–398). Filters to `type:'message'` +
/// USER/ASSISTANT/TOOL, computes the `assistantAfter` counts, maps TOOL results
/// through [`render_tool_result_content`], and (for multi-character) also emits
/// the attribution `messagesWithParticipants`.
pub fn build_conversation_messages(
    messages: &[WhisperMessage],
    is_multi_character: bool,
) -> (
    Vec<ConversationMessage>,
    Option<Vec<MessageWithParticipant>>,
) {
    // Filtered sequence: type=message, roles USER/ASSISTANT/TOOL.
    let filtered: Vec<&WhisperMessage> = messages
        .iter()
        .filter(|m| {
            m.message_type.as_deref() == Some("message")
                && matches!(
                    m.role.as_deref(),
                    Some("USER") | Some("ASSISTANT") | Some("TOOL")
                )
        })
        .collect();

    // assistantAfter[i] — ASSISTANT-role messages after filtered[i] (reverse pass).
    let mut assistant_after: Vec<i64> = vec![0; filtered.len()];
    let mut trailing = 0i64;
    for i in (0..filtered.len()).rev() {
        assistant_after[i] = trailing;
        if filtered[i].role.as_deref() == Some("ASSISTANT") {
            trailing += 1;
        }
    }

    let mut conversation: Vec<ConversationMessage> = Vec::new();
    for (i, msg) in filtered.iter().enumerate() {
        if msg.role.as_deref() == Some("TOOL") {
            // JSON.parse(content || '{}') — a parse failure drops the message.
            let raw = msg.content.clone().unwrap_or_default();
            let raw = if raw.is_empty() {
                "{}".to_string()
            } else {
                raw
            };
            match serde_json::from_str::<Value>(&raw) {
                Ok(tool_data) => conversation.push(ConversationMessage {
                    role: "USER".to_string(),
                    content: render_tool_result_content(&tool_data, assistant_after[i]),
                    id: msg.id.clone(),
                    thought_signature: None,
                }),
                Err(_) => { /* filtered out (null in v4) */ }
            }
        } else {
            let is_assistant = msg.role.as_deref() == Some("ASSISTANT");
            conversation.push(ConversationMessage {
                role: msg.role.clone().unwrap_or_default(),
                content: msg.content.clone().unwrap_or_default(),
                id: msg.id.clone(),
                thought_signature: if is_assistant {
                    msg.thought_signature.clone()
                } else {
                    None
                },
            });
        }
    }

    let messages_with_participants = if is_multi_character {
        let mut mwp: Vec<MessageWithParticipant> = Vec::new();
        for (i, msg) in filtered.iter().enumerate() {
            if msg.role.as_deref() == Some("TOOL") {
                let raw = msg.content.clone().unwrap_or_default();
                let raw = if raw.is_empty() {
                    "{}".to_string()
                } else {
                    raw
                };
                match serde_json::from_str::<Value>(&raw) {
                    Ok(tool_data) => mwp.push(MessageWithParticipant {
                        id: msg.id.clone(),
                        role: "USER".to_string(),
                        content: render_tool_result_content(&tool_data, assistant_after[i]),
                        participant_id: None,
                        thought_signature: None,
                        created_at: msg.created_at.clone(),
                        target_participant_ids: None,
                        host_event: None,
                        is_established_fact: false, // P4.D372: marked after the build
                    }),
                    Err(_) => { /* filtered out */ }
                }
            } else {
                let is_assistant = msg.role.as_deref() == Some("ASSISTANT");
                mwp.push(MessageWithParticipant {
                    id: msg.id.clone(),
                    role: msg.role.clone().unwrap_or_default(),
                    content: msg.content.clone().unwrap_or_default(),
                    participant_id: msg.participant_id.clone(),
                    thought_signature: if is_assistant {
                        msg.thought_signature.clone()
                    } else {
                        None
                    },
                    created_at: msg.created_at.clone(),
                    target_participant_ids: msg.target_participant_ids.clone(),
                    host_event: None,
                    is_established_fact: false, // P4.D372: marked after the build
                });
            }
        }
        Some(mwp)
    } else {
        None
    };

    (conversation, messages_with_participants)
}

/// v4 `normalizeWhisperRoles` (433–453). Staff whispers (`systemSender` set) are
/// re-roled to USER (attachment-bearing whispers stay ASSISTANT), with the
/// opaque-anywhere body swap. Non-whisper messages pass through unchanged.
pub fn normalize_whisper_roles(
    messages: &[WhisperMessage],
    is_opaque_anywhere: bool,
) -> Vec<WhisperMessage> {
    messages
        .iter()
        .map(|m| {
            // `!m.systemSender` — null/undefined/"" are falsy.
            if m.system_sender
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_none()
            {
                return m.clone();
            }
            let has_attachments = m.has_attachments();
            // body = isOpaqueAnywhere ? (opaqueContent ?? content) : content.
            let body = if is_opaque_anywhere {
                m.opaque_content.clone().or_else(|| m.content.clone())
            } else {
                m.content.clone()
            };
            let mut out = m.clone();
            out.system_sender = None;
            out.role = Some(if has_attachments {
                m.role.clone().unwrap_or_else(|| "ASSISTANT".to_string())
            } else {
                "USER".to_string()
            });
            out.content = body;
            out
        })
        .collect()
}

/// v4 `collectLanternImageFileIdsForCharacter` (217–266). Walks the tail of the
/// (already whisper-normalized) message list collecting Lantern-image file ids the
/// character has not yet seen, stopping at the character's own most-recent
/// ASSISTANT message. Returns file ids oldest-first, deduped.
pub fn collect_lantern_image_file_ids_for_character(
    messages: &[WhisperMessage],
    character_participant_id: &str,
    is_multi_character: bool,
    history_cutoff: Option<&str>,
    lookback: usize,
) -> Vec<String> {
    let mut collected: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut scanned = 0usize;
    let mut i = messages.len();
    while i > 0 && scanned < lookback {
        i -= 1;
        let msg = &messages[i];
        if msg.message_type.as_deref() != Some("message")
            || msg.role.as_deref() != Some("ASSISTANT")
        {
            continue;
        }
        let has_attachments = msg.has_attachments();

        // Anything older than the character's own previous turn was already
        // delivered, so we stop the walk there.
        if is_characters_own_prior_response(msg, character_participant_id, is_multi_character) {
            break;
        }

        scanned += 1;

        if !has_attachments {
            continue;
        }

        // History-access guard (string compare == chronological for ISO stamps).
        if let (Some(cutoff), Some(created)) = (history_cutoff, msg.created_at.as_deref()) {
            if created < cutoff {
                continue;
            }
        }

        if let Some(atts) = &msg.attachments {
            for file_id in atts {
                if seen.insert(file_id.clone()) {
                    collected.push(file_id.clone());
                }
            }
        }
    }
    collected.reverse();
    collected
}

/// Has this ASSISTANT message been authored by the character we are building
/// context for? Both attachment walkers stop there: anything older was already
/// delivered on a previous turn and must not be re-sent (v4
/// `isCharactersOwnPriorResponse`, extracted by `e288ae2ec` from the inline pair
/// that had lived in the Lantern walk — byte-for-byte the same two tests).
///
/// Multi-character chats set `participantId` on every character response, while
/// Staff notifications leave it null — a direct id match is enough. Single-
/// character chats don't populate participantId on character responses, so we
/// fall back to the structural signal: Staff image notifications always carry
/// attachments and character responses never do, so an ASSISTANT message
/// without attachments is the character's own prior turn.
fn is_characters_own_prior_response(
    msg: &WhisperMessage,
    character_participant_id: &str,
    is_multi_character: bool,
) -> bool {
    if is_multi_character {
        return msg.participant_id.as_deref() == Some(character_participant_id);
    }
    !msg.has_attachments()
}

/// One carrying message and the file ids on it this character has not been shown
/// (v4's `{ messageId, fileIds }`).
#[derive(Clone, Debug, PartialEq)]
pub struct UnseenUserAttachments {
    pub message_id: String,
    pub file_ids: Vec<String>,
}

/// The USER-side counterpart of [`collect_lantern_image_file_ids_for_character`]
/// (v4 `collectUnseenUserAttachmentsForCharacter`, `e288ae2ec` / bug 121): walk
/// the tail of `messages` and collect the attachments the *human* shared that
/// this character has not yet been shown.
///
/// Bug 121. A file the user attaches is expanded into prompt text (or carried as
/// raw bytes) at request-assembly time by `load_and_process_files`, from the
/// `fileIds` on that one request, and the expansion is never written down —
/// `chat_messages` keeps the user's typed words and a pointer. The second
/// character to speak in a multi-character turn is a fresh request with no
/// `fileIds`, assembling context from those rows, so it received the typed words
/// alone: in the reported scene a 29 KB transcript reached 1 of 13 model calls
/// while the attachment chip sat in the UI telling the user otherwise. The
/// Lantern walk could not cover it, because its first filter is
/// `role !== 'ASSISTANT'` and a user upload is a USER-role message.
///
/// Returns rows in chronological order so the caller can splice each file back in
/// at the message that carried it, rather than restating it at the tail. The same
/// "stop at the character's own prior response" rule bounds the walk and gives the
/// budget for free: a character is shown a given attachment once, on its first
/// turn after the upload, and never again.
///
/// `history_cutoff` (ISO timestamp) excludes uploads older than a joining
/// character's arrival; `lookback` caps the scan for very long chats.
pub fn collect_unseen_user_attachments_for_character(
    messages: &[WhisperMessage],
    character_participant_id: &str,
    is_multi_character: bool,
    history_cutoff: Option<&str>,
    lookback: usize,
) -> Vec<UnseenUserAttachments> {
    let mut collected: Vec<UnseenUserAttachments> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut scanned = 0usize;
    let mut i = messages.len();
    while i > 0 && scanned < lookback {
        i -= 1;
        let msg = &messages[i];
        // A non-message event costs nothing against the lookback.
        if msg.message_type.as_deref() != Some("message") {
            continue;
        }

        if msg.role.as_deref() == Some("ASSISTANT") {
            if is_characters_own_prior_response(msg, character_participant_id, is_multi_character) {
                break;
            }
            scanned += 1;
            continue;
        }

        if msg.role.as_deref() != Some("USER") {
            continue;
        }
        scanned += 1;

        let Some(atts) = &msg.attachments else {
            continue;
        };
        if atts.is_empty() {
            continue;
        }
        // No row id ⇒ nowhere to splice the text back in.
        let Some(message_id) = msg.id.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };

        // A joining participant without history access must not see uploads from
        // before they arrived — symmetric with the Lantern walk's own guard.
        if let (Some(cutoff), Some(created)) = (history_cutoff, msg.created_at.as_deref()) {
            if created < cutoff {
                continue;
            }
        }

        let file_ids: Vec<String> = atts
            .iter()
            .filter(|id| !seen.contains(id.as_str()))
            .cloned()
            .collect();
        if file_ids.is_empty() {
            continue;
        }
        for id in &file_ids {
            seen.insert(id.clone());
        }
        collected.push(UnseenUserAttachments {
            message_id: message_id.to_string(),
            file_ids,
        });
    }

    collected.reverse();
    collected
}

/// How far back either attachment walk will look for a user upload the character
/// has not seen (v4 `USER_ATTACHMENT_LOOKBACK`). The real bound is the
/// character's own previous turn; this is the safety cap for a very long chat, or
/// a character that has never spoken and so has no previous turn to stop at.
pub const USER_ATTACHMENT_LOOKBACK: usize = 20;

// ===========================================================================
// The K file-loading seam (unported wave-4 file subsystem).
// ===========================================================================

// === P4.D367 ===
/// The result of the seamed Lantern image load (v4 `loadChatFilesForLLM`):
/// every loaded attachment, in load order. Collect only — the transport budget
/// decides (v4 `29c1e2690`): no fallback, no prefix, no budget here; the walk
/// tags each one and [`crate::services::image_transport_budget`] decides, for
/// the profile actually being called, which are sent, described or omitted.
#[derive(Clone, Debug, Default)]
pub struct LanternLoad {
    pub attachments: Vec<Value>,
}
// === /P4.D367 ===

/// One loaded + fallback-dispatched USER upload (v4's per-file loop body inside
/// `rehydrateUserAttachments`). The seam does the IO — the load, the fallback
/// dispatch, the prefix formatting — and [`rehydrate_user_attachments`] keeps the
/// budget arithmetic and the keep-vs-drop rule, so both stay unit-drivable
/// through a canned seam.
#[derive(Clone, Debug)]
pub struct UserAttachmentFile {
    /// v4's `fileAttachment` — the loaded `FileAttachment` JSON, pushed onto
    /// `rehydratedAttachmentsToKeep` VERBATIM when the provider takes it natively.
    pub attachment: Value,
    // === P4.D367 ===
    /// v4 `isImageFile(fileAttachment.mimeType)` — the LOADED mime. An image
    /// skips the fallback (`unsupported` / `errored` false, `prefix` empty): it
    /// goes on whole, tagged `rehydrated`, and the transport budget decides
    /// capability, count and bytes for it on the final array.
    pub is_image: bool,
    // === /P4.D367 ===
    /// `fallbackResult.type === 'unsupported'`.
    pub unsupported: bool,
    /// `!!fallbackResult.error`.
    pub errored: bool,
    /// `formatFallbackAsMessagePrefix(fallbackResult)`. Read ONLY when the result
    /// is not `unsupported` — v4 `continue`s before it formats anything, so an
    /// `unsupported`-with-error file contributes no `⚠️` line on this path (unlike
    /// the Lantern path, which formats first and filters second).
    pub prefix: String,
}

/// The result of the seamed USER-attachment load, one entry per file the loader
/// resolved (v4's `loadChatFilesForLLM` can return FEWER than it was asked for —
/// a per-file failure is skipped there, faithfully kept).
#[derive(Clone, Debug, Default)]
pub struct UserAttachmentLoad {
    pub files: Vec<UserAttachmentFile>,
}

/// The file-subsystem seam (v4 `buildMessageContext` section K). Default no-op;
/// [`crate::services::chat_files::RealMessageContextSeams`] (W4.4b) does the real
/// load + fallback dispatch. Async (RPITIT `+ Send`), matching
/// [`crate::services::build_context::BuildContextSeams`] — the real body issues a
/// vision-description call for any unsupported prior image.
pub trait MessageContextSeams {
    /// Load the collected Lantern image file ids for a provider (collect only).
    fn load_lantern_images(
        &self,
        file_ids: &[String],
        provider: &str,
    ) -> impl std::future::Future<Output = LanternLoad> + Send {
        let _ = (file_ids, provider);
        async { LanternLoad::default() }
    }

    /// Load + fallback-process one carrying message's USER uploads (v4 bug 121's
    /// `loadChatFilesForLLM(fileIds, {provider})` + the per-file
    /// `processFileAttachmentFallback` inside `rehydrateUserAttachments`).
    ///
    /// `Err` is the Rust spelling of v4's `try { … } catch` around the whole
    /// re-hydration: the turn proceeds exactly as it did before this existed,
    /// keeping whatever earlier rows already contributed.
    fn load_user_attachments(
        &self,
        file_ids: &[String],
        provider: &str,
    ) -> impl std::future::Future<Output = Result<UserAttachmentLoad, String>> + Send {
        let _ = (file_ids, provider);
        async { Ok(UserAttachmentLoad::default()) }
    }

    // === P4.D367 ===
    /// The primary pass (v4 `context-builder.service.ts:1356-1375`): one image
    /// transport budget over the assembled array, for the responding profile,
    /// BEFORE the multi-character turn anchor. The default hands the array back
    /// unchanged — the no-op seam holds no profile and no describer (and its
    /// composition corpora carry no attachments, so the gate never opens);
    /// [`crate::services::chat_files::RealMessageContextSeams`] runs
    /// [`crate::services::image_transport_budget::apply_image_transport_budget`].
    fn apply_image_transport_budget(
        &self,
        messages: Vec<FormattedMsg>,
        log_context: crate::services::image_transport_budget::TransportLogContext,
    ) -> impl std::future::Future<Output = Vec<FormattedMsg>> + Send {
        let _ = log_context;
        async { messages }
    }
    // === /P4.D367 ===
}

/// The default no-op seam.
pub struct NoopMessageContextSeams;
impl MessageContextSeams for NoopMessageContextSeams {}

/// What [`rehydrate_user_attachments`] hands back (v4's
/// `{ rehydratedContentByMessageId, rehydratedAttachmentsToKeep }`).
#[derive(Debug, Default)]
pub struct RehydratedUserAttachments {
    /// The text to splice in AHEAD of each carrying message, keyed by that
    /// message's row id.
    pub content_by_message_id: std::collections::HashMap<String, String>,
    /// The raw attachments the provider takes natively, for the caller to merge
    /// and anchor the usual way.
    pub attachments_to_keep: Vec<Value>,
}

/// Load the user uploads a character has not yet been shown and turn them back
/// into prompt content (v4 `rehydrateUserAttachments`, `e288ae2ec` / bug 121).
/// See [`collect_unseen_user_attachments_for_character`] for why this is a
/// read-side derivation rather than a stored body.
///
/// Never fails: an unreadable file leaves the turn exactly as it was before this
/// existed. v4 declares its two accumulators OUTSIDE the try, so a mid-walk throw
/// returns what earlier rows already contributed rather than nothing — kept here
/// by breaking out of the loop instead of discarding.
async fn rehydrate_user_attachments<MCS: MessageContextSeams>(
    mc_seams: &MCS,
    messages: &[WhisperMessage],
    character_participant_id: &str,
    is_multi_character: bool,
    history_cutoff: Option<&str>,
    provider: &str,
) -> RehydratedUserAttachments {
    let mut out = RehydratedUserAttachments::default();
    // v4: `if (!characterParticipantId) return { … }` — an absent participant has
    // no "own prior response" to bound the walk with.
    if character_participant_id.is_empty() {
        return out;
    }

    let unseen = collect_unseen_user_attachments_for_character(
        messages,
        character_participant_id,
        is_multi_character,
        history_cutoff,
        USER_ATTACHMENT_LOOKBACK,
    );
    if unseen.is_empty() {
        return out;
    }

    // v4's `charactersUsed` — a running sum of what was re-sent (UTF-16 units,
    // JS `text.length`), reported by the DEBUG below.
    let mut characters_used = 0usize;
    // === P4.D367 ===
    // v4 `carriedAt`: each row's `Date.parse(createdAt) || 0`, keyed by id (a
    // later row with the same id overwrites, as `Map.set` does) — the
    // `createdAt` a re-hydrated image is tagged with.
    let mut carried_at: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    for m in messages {
        if let (Some(id), Some(created)) = (
            m.id.as_deref().filter(|s| !s.is_empty()),
            m.created_at.as_deref().filter(|s| !s.is_empty()),
        ) {
            carried_at.insert(id, crate::episodic::js_date_parse_ms(created).unwrap_or(0));
        }
    }
    // === /P4.D367 ===

    for row in &unseen {
        let loaded = match mc_seams
            .load_user_attachments(&row.file_ids, provider)
            .await
        {
            Ok(l) => l,
            Err(error) => {
                // v4's outer catch: warn and leave the turn as it was, keeping
                // whatever earlier rows contributed.
                tracing::warn!(
                    error = %error,
                    character_participant_id,
                    "Failed to re-hydrate user attachments from history"
                );
                return out;
            }
        };
        let mut prefix = String::new();

        for file in loaded.files {
            // === P4.D367 ===
            // Images go on whole, tagged; the transport budget decides
            // capability, count and bytes for them on the final array (an image
            // the profile cannot read is described there, at the anchor).
            if file.is_image {
                out.attachments_to_keep.push(tag_for_transport(
                    file.attachment,
                    &ImageTransportTag {
                        origin: ImageTransportOrigin::Rehydrated,
                        created_at: carried_at
                            .get(row.message_id.as_str())
                            .copied()
                            .unwrap_or(0),
                        describable: true,
                    },
                ));
                continue;
            }
            // === /P4.D367 ===
            // Mirror the `load_and_process_files` filter: keep the raw bytes only
            // when the provider takes them natively. A failed fallback DROPS the
            // file rather than tripping the provider's "no image input" refusal.
            if file.unsupported {
                if !file.errored {
                    out.attachments_to_keep.push(file.attachment);
                }
                continue;
            }

            if file.prefix.is_empty() {
                continue;
            }
            // No size cap (bug 259, v4 `fa126bde8`): the first character to
            // answer is handed every file whole by `load_and_process_files`,
            // and each later one must see exactly what it saw. `build_context`
            // budgets the result like any other message body.
            characters_used += file.prefix.encode_utf16().count();
            prefix.push_str(&file.prefix);
        }

        if !prefix.is_empty() {
            out.content_by_message_id
                .insert(row.message_id.clone(), prefix);
        }
    }

    if !out.content_by_message_id.is_empty() || !out.attachments_to_keep.is_empty() {
        tracing::debug!(
            messages_expanded = out.content_by_message_id.len(),
            raw_attachments_kept = out.attachments_to_keep.len(),
            characters_used,
            character_participant_id,
            "Re-hydrated user attachments from history"
        );
    }

    out
}

// ===========================================================================
// The composition.
// ===========================================================================

/// The shape [`select_attachment_anchor_index`] scans (v4's inline
/// `Array<{role, metadata?}>` parameter type) — adapted from
/// [`crate::services::build_context::ContextMessage`] at the production call
/// site, and built directly by the tier-1 differential.
pub struct AnchorMessage<'a> {
    pub role: &'a str,
    /// `metadata.messageId` — the source `chat_messages` row id. `Some("")`
    /// is treated as falsy exactly as v4's `id &&` guard does.
    pub message_id: Option<&'a str>,
    /// `metadata.isUserTurn` truthiness (absent/false collapse together).
    pub is_user_turn: bool,
}

/// Pick the message that image attachments (and the Lantern description
/// prefix) should ride on — v4 `selectAttachmentAnchorIndex` (a14a1811,
/// bug 95).
///
/// Bug 95: this used to be "the last message, if it happens to be role user".
/// Staff whispers format as `role: user` and routinely accumulate after the
/// human's turn — a Host timestamp, a Prospero context memorandum, a
/// connection-profile-change bubble — so on any regenerate or swipe the
/// picture was stapled to "Abigail's current response model is now …" while
/// the Librarian's announcement was telling the model the bytes rode with the
/// user's message. Worse, after a tool call the tail isn't role user at all,
/// and the attachments were dropped on the floor without a word.
///
/// Preference order:
///  1. the message flagged as *this* turn's user input (`metadata.isUserTurn`,
///     role NOT checked in this tier), set where `newUserMessage` is appended;
///  2. the last `role: user` message whose source row was a genuine human turn
///     — the regenerate/swipe case, where there is no new user message and
///     the human's words are already in history;
///  3. the last `role: user` message of any kind. This is the old behaviour,
///     kept as a floor: a context shape we haven't anticipated should still
///     deliver the bytes *somewhere* rather than silently discard them.
///
/// Returns -1 when there is no user-role message at all, which the caller
/// logs — the attachments genuinely cannot be delivered in that shape.
pub fn select_attachment_anchor_index(
    messages: &[AnchorMessage],
    user_turn_message_ids: &std::collections::HashSet<String>,
) -> i64 {
    for (i, m) in messages.iter().enumerate().rev() {
        if m.is_user_turn {
            return i as i64;
        }
    }
    for (i, m) in messages.iter().enumerate().rev() {
        if m.role == "user" {
            if let Some(id) = m.message_id.filter(|id| !id.is_empty()) {
                if user_turn_message_ids.contains(id) {
                    return i as i64;
                }
            }
        }
    }
    for (i, m) in messages.iter().enumerate().rev() {
        if m.role == "user" {
            return i as i64;
        }
    }
    -1
}

/// A provider-formatted outgoing message (v4 `buildMessageContext`'s
/// `formattedMessages` element). `attachments` is set only on the anchor
/// message when there are attachments (v4 a14a1811 — the last user message
/// before bug 95); otherwise `thought_signature` is carried.
#[derive(Clone, Debug, PartialEq)]
pub struct FormattedMsg {
    pub role: String,
    pub content: String,
    pub name: Option<String>,
    pub thought_signature: Option<String>,
    pub attachments: Option<Vec<Value>>,
}

/// The result (v4 `MessageContextResult`).
pub struct MessageContextResult {
    pub built_context: BuiltContext,
    pub formatted_messages: Vec<FormattedMsg>,
    pub is_initial_message: bool,
}

/// The non-message inputs `buildMessageContext` needs beyond the pre-built
/// [`BuildContextInput`] (whose message fields this module fills).
pub struct MessageContextParams<'a> {
    pub is_multi_character: bool,
    pub provider: &'a str,
    /// v4 `character.name` (the responding character).
    pub responding_character_name: &'a str,
    /// v4 `characterParticipant.id`.
    pub responding_participant_id: &'a str,
    /// v4 `characterParticipant.hasHistoryAccess`.
    pub has_history_access: bool,
    /// v4 `characterParticipant.createdAt` (Lantern history cutoff).
    pub character_participant_created_at: Option<&'a str>,
    /// v4 `character.systemTransparency` (single-character opaque test).
    pub character_system_transparency: Option<bool>,
    /// Which route anchors a multi-character turn to this character —
    /// [`crate::services::multi_character_prefill::profile_uses_name_prefill`]
    /// over the chat's connection profile, resolved at the call site exactly as
    /// v4 resolves it (`profileUsesNamePrefill(connectionProfile)`). Ignored
    /// when the chat is single-character.
    pub use_prefill: bool,
    /// The chat's participants array (for the opaque-anywhere LLM-participant scan).
    pub participants: &'a [Value],
    /// characterId → systemTransparency for the multi-character opaque test.
    pub participant_transparency: &'a std::collections::HashMap<String, Option<bool>>,
}

/// The multi-character scene-block instruction appended to the system prompt on
/// the PROSE route (v4 verbatim incl. the em-dash and the `[Name]`/`Name:`
/// literals). Byte-unchanged by v4 `23af7146`, which only moved it out of the
/// Anthropic-only branch, and byte-unchanged again by `e22f7b36`, which only
/// moved it into the `systemAdditions` list — the leading `\n\n` that used to
/// live in these bytes is now contributed once by the append below.
fn multi_character_prose_instruction(name: &str) -> String {
    format!(
        "IMPORTANT — this is a multi-character scene. Respond as {name} and ONLY {name}: \
write only {name}'s own dialogue, actions, and thoughts for this single turn, then stop. \
Never write, narrate, quote, or continue another participant's turn, and never label any text \
with another participant's name (no \"[Name]\" or \"Name:\" speaker tags for anyone but {name}). \
Output only {name}'s contribution."
    )
}

/// Anti-chorus content rules for multi-character turns, appended to the system
/// message on BOTH anchor routes (the discipline is about what a turn contains,
/// not who speaks it). v4 `GROUP_SCENE_DISCIPLINE` (`e22f7b36`), transcribed
/// byte-for-byte from its source.
///
/// Motivated by the "committee meeting" failure mode observed with weaker
/// models: every character opens with a roll-call recap of the prior speakers,
/// endorses all of it, claims "the one thing nobody has named," parrots the
/// cast's coined phrases verbatim, and closes by restating the group's action
/// list. The rules target the *shape* of the chorus rather than specific
/// wording — phrase blocklists alone have proven too weak to hold.
pub const GROUP_SCENE_DISCIPLINE: &str = "GROUP-SCENE DISCIPLINE — the failure mode of a group scene is the chorus: each character recaps what the others said, agrees with all of it, and adds one small item shaped like everyone else's. Never join a chorus:\n\
     - Do not open by summarizing or listing what other characters just said. Everyone present heard it. React to at most one specific thing, or simply act.\n\
     - Do not agree-then-add (\"X is right — but there's one thing nobody has named\"). If all you have is agreement plus a small addendum, give the addendum alone in a sentence or two — or pass the turn if passing is offered.\n\
     - Never reuse another character's metaphors, images, or coined phrases. A striking phrase someone else used in this scene is spent; repeating it is a defect, not a callback. If several characters have already said much the same thing, saying it again in your own accent adds nothing.\n\
     - Do not restate the plan, the task list, or the group's conclusions. They are already on the record; a speech re-affirming what is decided adds nothing.\n\
     - Speak to change something: new information, a genuine objection or disagreement, a question, an action actually taken, a joke, a refusal. Re-pledging your commitment is not a turn.\n\
     - Vary register and length. Most real conversational turns are one to three sentences. A long speech is an event, not a default — and never the second one in a row.";

/// v4 `applyMultiCharacterTurnAnchor` (`23af7146`). In multi-character chats,
/// anchor each reply to the responding character and forbid it from writing
/// anyone else's turn. Two routes, chosen PER PROFILE by
/// `multiCharacterPrefill` — this replaced a hardcoded provider test:
///
///   - **prefill** — append an assistant `[Name]` message. The model
///     structurally continues only that character's line; the leading tag is
///     stripped downstream by `stripCharacterNamePrefix()`.
///   - **prose** — append an instruction to the system message instead, leaving
///     the conversation ending on a user message. v4 deliberately does NOT tell
///     the model to emit a `[Name]` tag: that both contradicts the always-on
///     Identity Reminder ("do not prefix with your name") and teaches weaker
///     models the very screenplay format they then run away with, writing the
///     whole cast's turns. Identity is anchored in prose and foreign speaker
///     tags forbidden outright.
///
/// `finalize_message_response` truncates a response at the first foreign
/// `[Name]`/`Name:` tag as a structural backstop either way.
///
/// Both routes ALSO append [`GROUP_SCENE_DISCIPLINE`] to the system message
/// (v4 `e22f7b36`): the identity anchor keeps a turn attributed to one
/// character, but says nothing about content, and with the previous turns as
/// the strongest style examples in context, models converge into the
/// recap-endorse-echo chorus the discipline block forbids.
///
/// The system append is a no-op when there is no system message (v4's
/// `if (systemIdx >= 0)`); the prefill push happens either way. A PROSE turn
/// therefore appends TWO blocks (identity, then discipline) separated by
/// `\n\n`, a PREFILL turn exactly ONE. Mutates in place, as v4 does.
pub fn apply_multi_character_turn_anchor(
    formatted_messages: &mut Vec<FormattedMsg>,
    character_name: &str,
    use_prefill: bool,
) {
    let sys_idx = formatted_messages.iter().position(|m| m.role == "system");

    // v4's `systemAdditions: string[]`, joined with `\n\n` and appended after a
    // leading `\n\n`. The prose route contributes the identity instruction
    // FIRST; the prefill route contributes only the discipline block (identity
    // is anchored structurally by the `[Name]` message pushed below).
    let mut system_additions: Vec<String> = Vec::new();
    if !use_prefill {
        system_additions.push(multi_character_prose_instruction(character_name));
    }
    system_additions.push(GROUP_SCENE_DISCIPLINE.to_string());

    if let Some(sys_idx) = sys_idx {
        formatted_messages[sys_idx]
            .content
            .push_str(&format!("\n\n{}", system_additions.join("\n\n")));
    }

    if use_prefill {
        formatted_messages.push(FormattedMsg {
            role: "assistant".to_string(),
            content: format!("[{character_name}]"),
            // v4 sets both explicitly `undefined` on this synthesized message.
            name: None,
            thought_signature: None,
            attachments: None,
        });
    }
}

/// Whether the responding participant is a party to `m` — v4's section-B whisper
/// predicate. Since `a163862c` it applies to EVERY role (it used to bail out early
/// on anything that wasn't a TOOL message), which makes it exactly the test
/// [`crate::message_attribution::filter_whisper_messages`] applies downstream in
/// multi-character mode. `message_context_whisper_predicate_matches_the_shared_rule`
/// pins that equivalence.
fn is_whisper_party(m: &WhisperMessage, responding_id: &str) -> bool {
    match &m.target_participant_ids {
        None => true,
        Some(t) if t.is_empty() => true,
        Some(t) => {
            m.participant_id.as_deref() == Some(responding_id)
                || t.iter().any(|x| x == responding_id)
        }
    }
}

/// Compute the opaque-anywhere flag (v4 section C).
fn compute_opaque_anywhere(params: &MessageContextParams<'_>) -> bool {
    if params.is_multi_character {
        // llmParticipants = controlledBy !== 'user' && status !== 'removed'.
        params.participants.iter().any(|p| {
            let controlled_by = p.get("controlledBy").and_then(Value::as_str).unwrap_or("");
            let status = p.get("status").and_then(Value::as_str).unwrap_or("");
            if controlled_by == "user" || status == "removed" {
                return false;
            }
            let character_id = p.get("characterId").and_then(Value::as_str).unwrap_or("");
            // Unknown character record → opaque; transparency !== true → opaque.
            !matches!(
                params.participant_transparency.get(character_id),
                Some(Some(true))
            )
        })
    } else {
        // character.systemTransparency !== true.
        params.character_system_transparency != Some(true)
    }
}

/// v4 `buildMessageContext` (458–799). Runs the whisper pre-filters + the
/// conversation build, fills the message fields of `build_input`, calls
/// [`build_context`], then applies the provider formatting + scene block.
///
/// `build_input`'s `existing_messages` / `messages_with_participants` /
/// `is_initial_message` / `generate_memory_recap` fields are OVERWRITTEN here (the
/// caller leaves them as placeholders); every other field is threaded verbatim.
#[allow(clippy::too_many_arguments)]
pub async fn build_message_context<EMB, CMP, BCS, MCS>(
    db: &Db,
    embedding: &EMB,
    completion: &CMP,
    executor: &CheapLlmTaskExecutor,
    bc_seams: &BCS,
    mc_seams: &MCS,
    params: &MessageContextParams<'_>,
    mut build_input: BuildContextInput,
    existing_messages: &[Value],
    attachments_to_send: &[Value],
) -> Result<MessageContextResult, BuildContextError>
where
    EMB: EmbeddingProvider,
    CMP: CompletionProvider,
    BCS: BuildContextSeams,
    MCS: MessageContextSeams,
{
    let is_multi = params.is_multi_character;

    // Parse the raw events into the whisper-pipeline shape.
    let parsed: Vec<WhisperMessage> = existing_messages
        .iter()
        .map(WhisperMessage::from_value)
        .collect();

    // --- A. Drop record-only rows before anything else touches the history. ---
    //
    // === P4.D205 (v4 `e7d77bb60`, `context-builder.service.ts:848-894`) ===
    // v4 widened this strip from "Commonplace-Book whispers" to
    // `isRecordOnlyMessage`, which now also covers the Host's `inform` record.
    // Both kinds DOCUMENT a delivery rather than being part of the
    // conversation: the Commonplace whisper records a recall, and the inform
    // record records that a passage was handed to a seat — the passage itself
    // reaches the model as its own system block, never as transcript.
    //
    // This is the single call site for the predicate, so there is no dimension
    // along which a record could slip through on one path and not another.
    let cmpb_stripped = parsed.iter().filter(|m| is_record_only_message(m)).count();
    let messages_without_cmpb: Vec<WhisperMessage> = if cmpb_stripped > 0 {
        parsed
            .into_iter()
            .filter(|m| !is_record_only_message(m))
            .collect()
    } else {
        parsed
    };
    if cmpb_stripped > 0 {
        tracing::debug!(
            target: "quilltap::context",
            chat_id = %build_input.chat.id,
            stripped_count = cmpb_stripped,
            "[Context] Stripped record-only messages from LLM history",
        );
    }
    // === end P4.D205 ===

    // --- A2. Name the speaker on ad-hoc announcements (P4.D37 / v4 `424a7381`). ---
    // `customAnnouncer` is a rendering field — the Salon paints the name and
    // avatar on the bubble — so without this the model receives an anonymous block
    // of prose and guesses who said it. See [`crate::announcement_attribution`].
    let announcer_character_ids = collect_announcer_character_ids(&messages_without_cmpb);
    let mut announcer_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for id in &announcer_character_ids {
        let cid = id.clone();
        match db.read_main(|main| {
            db.read_mount_index(|mount| crate::db::characters_read::find_by_id(main, mount, &cid))
        }) {
            // v4: `if (character?.name) announcerNames.set(id, character.name)` —
            // stored untrimmed; the resolver trims.
            Ok(Some(c)) => {
                if let Some(name) = c
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty())
                {
                    announcer_names.insert(id.clone(), name.to_string());
                }
            }
            Ok(None) => {}
            // A deleted or unreadable character stays unnamed rather than blocking
            // the turn; the announcement passes through as it did before.
            Err(e) => {
                tracing::warn!(
                    character_id = %id,
                    error = %e,
                    "[Context] Could not resolve announcer name"
                );
            }
        }
    }
    // Always run the pass: a `custom` announcer carries its own display name and
    // needs no lookup, so gating on the resolved-name map would skip it.
    let messages_attributed =
        attribute_adhoc_announcements(&messages_without_cmpb, &announcer_names);

    // --- B. Drop whispers the responding character isn't a party to. ---
    // The same rule `filter_whisper_messages` applies in multi-character mode,
    // enforced here so single-character context can't be the one place a private
    // aside leaks. Operator-only Prospero runs (run-tool with `private: true`)
    // target the userId, so no character participant ever matches and the message
    // is filtered out of every context; a whispered ad-hoc announcement is excluded
    // from every character it wasn't addressed to on the same test. (Before v4
    // `a163862c` this arm tested TOOL messages only — the one path where a targeted
    // non-TOOL message could reach a character it wasn't addressed to.)
    let responding_id = params.responding_participant_id;
    let messages_after_whisper_filter: Vec<WhisperMessage> = if !responding_id.is_empty() {
        messages_attributed
            .into_iter()
            .filter(|m| is_whisper_party(m, responding_id))
            .collect()
    } else {
        messages_attributed
    };

    // --- C. Opaque-anywhere. ---
    let is_opaque_anywhere = compute_opaque_anywhere(params);

    // === P4.D372 === Established facts (v4 `db6190b44`): see
    // [`frame_established_fact_pass`].
    let (messages_framed, fact_row_ids) = frame_established_fact_pass(
        &messages_after_whisper_filter,
        is_opaque_anywhere,
        &build_input.chat.id,
    );
    // === /P4.D372 ===

    // --- D. Whisper-role normalization. ---
    let filtered = normalize_whisper_roles(&messages_framed, is_opaque_anywhere);

    // Row ids of the *human's* own turns, captured before normalization erases
    // the distinction (v4 a14a1811, bug 95) — see [`user_turn_message_ids`].
    let user_turn_message_ids = user_turn_message_ids(&messages_after_whisper_filter);

    // --- D2. The shared attachment history cutoff (v4 `e288ae2ec` hoisted it out
    // of the Lantern block so BOTH walks read one value). If this is a joining
    // character without history access and they have not yet responded, clamp both
    // attachment walks to messages posted after they joined. Computed from the
    // filtered set so opaque characters never reach Staff attachments either —
    // symmetric with their text-side filter.
    let has_prior_response = filtered.iter().any(|m| {
        m.message_type.as_deref() == Some("message")
            && m.role.as_deref() == Some("ASSISTANT")
            && m.participant_id.as_deref() == Some(responding_id)
    });
    // v4: `isMultiCharacter && characterParticipant && !hasHistoryAccess &&
    // !hasPriorResponse` — the `characterParticipant` conjunct is JS truthiness on
    // the participant row, spelled here as a non-empty id (an absent row has no
    // id AND no `created_at`, so the cutoff is `None` either way).
    let attachment_history_cutoff: Option<String> = if is_multi
        && !responding_id.is_empty()
        && !params.has_history_access
        && !has_prior_response
    {
        params.character_participant_created_at.map(String::from)
    } else {
        None
    };

    // --- D3. Bug 121: re-hydrate the human's own attachments out of history. ---
    //
    // `load_and_process_files` expands the files on *this* request's `fileIds` and
    // the expansion dies with the request — the row keeps the typed words and a
    // pointer. Everyone after the first character to answer therefore saw a bare
    // message where a document had been. Re-deriving from the file here (rather
    // than persisting the expanded body) keeps the file the single source of
    // truth, survives regenerate, swipe, import and restore, and treats an
    // uploaded image exactly like an uploaded transcript: the same
    // `process_file_attachment_fallback` pass either inlines the text, describes
    // the image, or hands the raw bytes back for a provider that takes them.
    //
    // This runs *before* `build_context` so the tokens are budgeted, compressed
    // and trimmed like any other message content — the Lantern prefix is spliced
    // in after budgeting, which is affordable for a description and would not be
    // for a 29 KB transcript.
    let rehydrated = rehydrate_user_attachments(
        mc_seams,
        &filtered,
        responding_id,
        is_multi,
        attachment_history_cutoff.as_deref(),
        params.provider,
    )
    .await;

    // A COPY only when something was re-hydrated (v4's `size > 0` ternary);
    // `filtered` itself is never spliced, so the Lantern walk below still reads
    // the stored rows.
    let rehydrated_messages: Option<Vec<WhisperMessage>> =
        if rehydrated.content_by_message_id.is_empty() {
            None
        } else {
            Some(
                filtered
                    .iter()
                    .map(|m| {
                        let prefix =
                            m.id.as_deref()
                                .and_then(|id| rehydrated.content_by_message_id.get(id));
                        match prefix {
                            Some(p) => {
                                let mut out = m.clone();
                                out.content =
                                    Some(format!("{p}{}", m.content.as_deref().unwrap_or("")));
                                out
                            }
                            None => m.clone(),
                        }
                    })
                    .collect(),
            )
        };
    let messages_for_conversation: &[WhisperMessage] =
        rehydrated_messages.as_deref().unwrap_or(&filtered);

    // --- E. Conversation messages + first-message detection. ---
    let (conversation, mwp) = build_conversation_messages(messages_for_conversation, is_multi);
    // === P4.D372 === Mark fact rows on the multi-character sequence:
    // everybody knows an established fact, so the history-access cut must not
    // hide one from a seat that joined after it was posted.
    let mwp = match (&fact_row_ids, mwp) {
        (Some(ids), Some(mut rows)) => {
            for m in rows.iter_mut() {
                if m.id.as_deref().is_some_and(|id| ids.contains(id)) {
                    m.is_established_fact = true;
                }
            }
            Some(rows)
        }
        (_, mwp) => mwp,
    };
    // === /P4.D372 ===
    let is_initial_message = !conversation
        .iter()
        .any(|m| m.role == "user" || m.role == "USER");

    let is_character_first_response = is_initial_message
        || (is_multi
            && !params.has_history_access
            && mwp
                .as_ref()
                .map(|mwp| {
                    !mwp.iter().any(|m| {
                        m.participant_id.as_deref() == Some(responding_id)
                            && (m.role == "assistant" || m.role == "ASSISTANT")
                    })
                })
                .unwrap_or(false));

    // --- F. Memory-recap decision (requestMemoryRecap ?? isCharacterFirstResponse;
    // the orchestrator never sets requestMemoryRecap, so it is the first-response). ---
    let should_generate_recap = is_character_first_response;

    // Fill the message-dependent fields of the buildContext input.
    build_input.existing_messages = conversation
        .iter()
        .map(|m| ExistingMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            id: m.id.clone(),
            thought_signature: m.thought_signature.clone(),
            message_type: Some("message".to_string()),
            // === P4.D205 ===
            // Always `None` on this path: the record-only strip in section A
            // above has already removed every `inform` row, so nothing reaching
            // here can carry the kind. Carried explicitly rather than defaulted
            // so the reason is on the record.
            system_kind: None,
            // === end P4.D205 ===
            // Likewise `None`: a Host "spoken of" card is record-only (P4.D347).
            system_sender: None,
            // === P4.D372 === `None`, as v4's `conversationMessages` carries no
            // flag: the fact is already framed in `content` (the pass above).
            is_established_fact: None,
            // === /P4.D372 ===
        })
        .collect();
    build_input.messages_with_participants = if is_multi { mwp } else { None };
    build_input.human_turn_message_ids = Some(user_turn_message_ids.clone());
    build_input.is_initial_message = is_initial_message;
    build_input.generate_memory_recap = should_generate_recap;

    // --- G/H. buildContext. ---
    let built_context =
        build_context::build_context(db, embedding, completion, executor, bc_seams, &build_input)
            .await?;

    // --- J. Provider-aware formatting for multi-character. ---
    let intermediate: Vec<InterMsg> = if is_multi {
        let mc_input: Vec<MultiCharacterMessage> = built_context
            .messages
            .iter()
            .map(|m| MultiCharacterMessage {
                role: wire_role_from_str(m.role),
                content: m.content.clone(),
                id: None,
                name: m.name.clone(),
                participant_id: None,
                thought_signature: m.thought_signature.clone(),
            })
            .collect();
        let formatted = message_formatter::format_messages_for_provider(
            &mc_input,
            params.provider,
            params.responding_character_name,
        );
        formatted.iter().map(InterMsg::from_formatted).collect()
    } else {
        built_context
            .messages
            .iter()
            .map(|m| InterMsg {
                role: m.role.to_string(),
                content: m.content.clone(),
                name: m.name.clone(),
                thought_signature: m.thought_signature.clone(),
            })
            .collect()
    };

    // --- K. Lantern image collection + load (seam). ---
    // v4 `e288ae2ec` seeds the merged slate from the re-hydrated raw attachments
    // as well, and the Lantern merge below CHAINS onto it rather than re-seeding
    // from `attachments_to_send` (which used to drop them).
    let mut merged_attachments: Vec<Value> = attachments_to_send
        .iter()
        .cloned()
        .chain(rehydrated.attachments_to_keep.iter().cloned())
        .collect();
    // === P4.D367 ===
    // The walk only collects candidates (v4 `29c1e2690`). Each is tagged
    // `lantern` with the time of the message that carried it, and whether
    // describing it is free (a generated image whose prompt is on file); the
    // transport budget below decides — for the profile actually being called —
    // which are sent, which are described, and which are omitted with a note.
    // That pass spends one count cap and one byte budget over every image on
    // the request, which is what bug 151 needed and a Lantern-only budget could
    // not give.
    {
        let ids = collect_lantern_image_file_ids_for_character(
            &filtered,
            responding_id,
            is_multi,
            attachment_history_cutoff.as_deref(),
            ASSISTANT_IMAGE_LOOKBACK,
        );
        if !ids.is_empty() {
            let load = mc_seams.load_lantern_images(&ids, params.provider).await;
            if !load.attachments.is_empty() {
                // v4 `carriedAt`: for every ASSISTANT row (EXACT, upper-case)
                // carrying attachments and a `createdAt`, each string id maps to
                // the LATEST `Date.parse(createdAt) || 0` that carried it.
                // (`WhisperMessage.attachments` is already string-only, so v4's
                // `typeof id === 'string'` filter is structural here.)
                let mut carried_at: std::collections::HashMap<&str, i64> =
                    std::collections::HashMap::new();
                for m in &filtered {
                    let (Some("ASSISTANT"), Some(atts), Some(created)) = (
                        m.role.as_deref(),
                        m.attachments.as_ref(),
                        m.created_at.as_deref().filter(|c| !c.is_empty()),
                    ) else {
                        continue;
                    };
                    let at = crate::episodic::js_date_parse_ms(created).unwrap_or(0);
                    for id in atts {
                        let e = carried_at.entry(id.as_str()).or_insert(0);
                        *e = (*e).max(at);
                    }
                }
                // Sequential probes, in LOAD order (v4 awaits each in turn).
                for fa in load.attachments {
                    let id = fa
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let tag = ImageTransportTag {
                        origin: ImageTransportOrigin::Lantern,
                        created_at: carried_at.get(id.as_str()).copied().unwrap_or(0),
                        describable: has_persisted_image_description(db, &id).await,
                    };
                    merged_attachments.push(tag_for_transport(fa, &tag));
                }
            }
        }
    }
    // === /P4.D367 ===

    // --- L. Final message construction (prefix injection / attachment merge). ---
    // The anchor is selected against `built_context.messages` (which carries
    // the metadata); `format_messages_for_provider` is a pure 1:1 map, so
    // indices stay parallel with `intermediate` and an index chosen against
    // one applies to the other (v4's comment, verified against v5's map).
    let anchor_view: Vec<AnchorMessage> = built_context
        .messages
        .iter()
        .map(|m| AnchorMessage {
            role: m.role,
            message_id: m.metadata.as_ref().and_then(|md| md.message_id.as_deref()),
            is_user_turn: m
                .metadata
                .as_ref()
                .and_then(|md| md.is_user_turn)
                .unwrap_or(false),
        })
        .collect();
    let attachment_anchor_index =
        select_attachment_anchor_index(&anchor_view, &user_turn_message_ids);
    if !merged_attachments.is_empty() && attachment_anchor_index == -1 {
        tracing::warn!(
            attachment_count = merged_attachments.len(),
            context_message_count = built_context.messages.len(),
            "Image attachments could not be anchored — no user-role message in context; images will not reach the model"
        );
    }

    // === P4.D367 ===
    let mut formatted_messages: Vec<FormattedMsg> =
        construct_formatted_messages(&intermediate, attachment_anchor_index, &merged_attachments);

    // One transport budget for every image on the request: capability, the
    // profile's per-request image cap and the total byte budget, spent down a
    // ranked list (uploads, re-hydrated uploads, Lantern images, each newest
    // first). The failover service runs the same pass for each substitute.
    // With nothing to change it hands back the same array. BEFORE the
    // multi-character turn anchor (v4 `:1356-1377`).
    if !formatted_messages.is_empty() && !merged_attachments.is_empty() {
        formatted_messages = mc_seams
            .apply_image_transport_budget(
                formatted_messages,
                TransportLogContext::primary(&build_input.chat.id, responding_id),
            )
            .await;
    }
    // === /P4.D367 ===

    // --- M. Multi-character turn anchor. ---
    if is_multi {
        apply_multi_character_turn_anchor(
            &mut formatted_messages,
            params.responding_character_name,
            params.use_prefill,
        );
    }

    Ok(MessageContextResult {
        built_context,
        formatted_messages,
        is_initial_message,
    })
}

/// The unified pre-L intermediate (either the raw buildContext messages for the
/// single-character path, or the provider-formatted messages for multi).
struct InterMsg {
    role: String,
    content: String,
    name: Option<String>,
    thought_signature: Option<String>,
}

// === P4.D367 ===
/// Section L (v4 `context-builder.service.ts:1340-1355` at `624d0d25c`) — the
/// final message construction. The ANCHOR message (bug 95, v4 a14a1811 —
/// previously "the last message, if role user") carries the merged attachments
/// (its `thought_signature` is dropped in the attachment arm exactly as v4's
/// object literal omits it); every other message rides through with
/// `thought_signature` intact and no attachments. An anchor index of -1
/// matches no message: nothing is spliced, which the caller has already warned
/// about. The anchor's content is the message's own: v4 `29c1e2690` deleted the
/// Lantern prefix splice (the transport budget now describes at the anchor),
/// and the parameter went with it rather than linger as `""`.
///
/// Extracted so the WIRING — that the selected anchor, not "last user",
/// drives the splice — is unit-pinned (the composition corpora cannot see it:
/// their Lantern seam is inert and their ops carry no attachments).
fn construct_formatted_messages(
    intermediate: &[InterMsg],
    attachment_anchor_index: i64,
    merged_attachments: &[Value],
) -> Vec<FormattedMsg> {
    let mut formatted_messages: Vec<FormattedMsg> = Vec::new();
    for (idx, m) in intermediate.iter().enumerate() {
        let is_anchor = idx as i64 == attachment_anchor_index;
        let content = m.content.clone();
        if is_anchor && !merged_attachments.is_empty() {
            formatted_messages.push(FormattedMsg {
                role: m.role.clone(),
                content,
                name: m.name.clone(),
                thought_signature: None,
                attachments: Some(merged_attachments.to_vec()),
            });
        } else {
            formatted_messages.push(FormattedMsg {
                role: m.role.clone(),
                content,
                name: m.name.clone(),
                thought_signature: m.thought_signature.clone(),
                attachments: None,
            });
        }
    }
    formatted_messages
}
// === /P4.D367 ===

impl InterMsg {
    fn from_formatted(f: &FormattedMessage) -> Self {
        InterMsg {
            role: wire_role_str(f.role).to_string(),
            content: f.content.clone(),
            name: f.name.clone(),
            thought_signature: f.thought_signature.clone(),
        }
    }
}

fn wire_role_from_str(role: &str) -> WireRole {
    match role {
        "system" => WireRole::System,
        "assistant" => WireRole::Assistant,
        _ => WireRole::User,
    }
}

fn wire_role_str(role: WireRole) -> &'static str {
    match role {
        WireRole::System => "system",
        WireRole::User => "user",
        WireRole::Assistant => "assistant",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // -----------------------------------------------------------------------
    // The bug-121 re-hydration pins (v4 `e288ae2ec`). The walk itself is
    // oracle-pinned by `message_context_leaves_equivalence`; these drive
    // `rehydrate_user_attachments` through a CANNED seam so the budget
    // arithmetic, the skip-whole rule, the `unsupported`-with-error drop and
    // the three log lines are each measured on their own.
    // -----------------------------------------------------------------------

    /// A canned [`MessageContextSeams`] answering `load_user_attachments` from a
    /// fileId → (unsupported, errored, prefix) table. `Err` for a file id in
    /// `fail_on` stands in for v4's `loadChatFilesForLLM` throwing.
    struct CannedSeams {
        table: std::collections::HashMap<String, (bool, bool, String)>,
        fail_on: Option<String>,
        calls: Mutex<Vec<Vec<String>>>,
        // === P4.D367 ===
        /// File ids the canned loader reports as images (`is_image`).
        images: std::collections::HashSet<String>,
        /// What `load_lantern_images` answers (collect only).
        lantern: Vec<Value>,
        /// Every array handed to the transport pass, in call order (the
        /// scripted pass hands it back unchanged).
        passes: Mutex<Vec<Vec<FormattedMsg>>>,
        // === /P4.D367 ===
    }
    impl CannedSeams {
        fn new(rows: &[(&str, bool, bool, &str)]) -> Self {
            CannedSeams {
                table: rows
                    .iter()
                    .map(|(id, u, e, p)| (id.to_string(), (*u, *e, p.to_string())))
                    .collect(),
                fail_on: None,
                calls: Mutex::new(Vec::new()),
                images: std::collections::HashSet::new(),
                lantern: Vec::new(),
                passes: Mutex::new(Vec::new()),
            }
        }
        fn failing_on(mut self, id: &str) -> Self {
            self.fail_on = Some(id.to_string());
            self
        }
        // === P4.D367 ===
        fn with_images(mut self, ids: &[&str]) -> Self {
            self.images = ids.iter().map(|s| s.to_string()).collect();
            self
        }
        fn with_lantern(mut self, atts: Vec<Value>) -> Self {
            self.lantern = atts;
            self
        }
        // === /P4.D367 ===
    }
    impl MessageContextSeams for CannedSeams {
        // === P4.D367 ===
        async fn load_lantern_images(&self, _file_ids: &[String], _provider: &str) -> LanternLoad {
            LanternLoad {
                attachments: self.lantern.clone(),
            }
        }
        async fn apply_image_transport_budget(
            &self,
            messages: Vec<FormattedMsg>,
            _log_context: crate::services::image_transport_budget::TransportLogContext,
        ) -> Vec<FormattedMsg> {
            self.passes.lock().unwrap().push(messages.clone());
            messages
        }
        // === /P4.D367 ===
        async fn load_user_attachments(
            &self,
            file_ids: &[String],
            _provider: &str,
        ) -> Result<UserAttachmentLoad, String> {
            self.calls.lock().unwrap().push(file_ids.to_vec());
            if let Some(bad) = &self.fail_on {
                if file_ids.iter().any(|id| id == bad) {
                    return Err("storage read failed".to_string());
                }
            }
            let mut files = Vec::new();
            for id in file_ids {
                if let Some((unsupported, errored, prefix)) = self.table.get(id) {
                    files.push(UserAttachmentFile {
                        attachment: serde_json::json!({ "id": id }),
                        is_image: self.images.contains(id),
                        unsupported: *unsupported,
                        errored: *errored,
                        prefix: prefix.clone(),
                    });
                }
            }
            Ok(UserAttachmentLoad { files })
        }
    }

    fn user_msg(id: &str, content: &str, atts: &[&str]) -> WhisperMessage {
        WhisperMessage {
            message_type: Some("message".to_string()),
            role: Some("USER".to_string()),
            content: Some(content.to_string()),
            id: Some(id.to_string()),
            attachments: Some(atts.iter().map(|a| a.to_string()).collect()),
            ..Default::default()
        }
    }
    fn assistant_msg(id: &str, participant: &str) -> WhisperMessage {
        WhisperMessage {
            message_type: Some("message".to_string()),
            role: Some("ASSISTANT".to_string()),
            content: Some("a turn".to_string()),
            id: Some(id.to_string()),
            participant_id: Some(participant.to_string()),
            ..Default::default()
        }
    }

    /// The dev-dependency tokio runtime the other async unit pins in this crate
    /// use (`scheduled_maintenance`, `file_fallback`).
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    // NOT migrated to `crate::test_support` (P4.77's census surveyed this
    // site and found it genuinely different, not a drifted copy): this
    // `FieldVisitor` never overrides `record_str`, so a plain `&str` field —
    // `character_participant_id` here, passed bare rather than with `%` —
    // falls through `tracing::field::Visit`'s own default straight to
    // `record_debug`, which quotes it. `the_budget_skips_a_file_whole_…` and
    // `a_load_failure_warns_…` below pin that exact quoting
    // (`character_participant_id=\"cp\"`). The shared visitor implements
    // `record_str` explicitly (Display, unquoted) precisely because every
    // OTHER site relies on that instead — swapping this site to the shared
    // one silently un-quotes the field and reddens both tests. Kept local.
    struct CaptureLayer(Arc<Mutex<Vec<String>>>);
    struct FieldVisitor(String);
    impl tracing::field::Visit for FieldVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0.push_str(&format!(" {value:?}"));
            } else {
                self.0.push_str(&format!(" {}={value:?}", field.name()));
            }
        }
    }
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = event.metadata();
            let mut visitor = FieldVisitor(format!("{}", meta.level()));
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.0);
        }
    }
    /// `set_default` is THREAD-scoped, so parallel tests cannot steal each
    /// other's subscriber.
    fn captured<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        let out = {
            let _guard = tracing::subscriber::set_default(subscriber);
            f()
        };
        let lines = logs.lock().unwrap().clone();
        (out, lines)
    }
    fn line_with<'a>(lines: &'a [String], sentence: &str) -> &'a String {
        lines
            .iter()
            .find(|l| l.contains(sentence))
            .unwrap_or_else(|| panic!("no line carried {sentence:?}; got {lines:#?}"))
    }

    /// An `unsupported` result keeps the raw bytes ONLY when it carries no
    /// error — a failed fallback DROPS the file rather than tripping the
    /// provider's "no image input" refusal — and v4 `continue`s BEFORE
    /// formatting, so the errored file contributes no `⚠️` prefix here (unlike
    /// the Lantern path, which formats first and filters second).
    #[test]
    fn unsupported_keeps_bytes_only_without_an_error() {
        let seams = CannedSeams::new(&[
            ("native", true, false, "⚠️ never read\n\n"),
            (
                "failed",
                true,
                true,
                "⚠️ Attachment Processing Failed: x\nboom\n\n",
            ),
            (
                "text",
                false,
                false,
                "[User attached text file: n.md]\n\nbody\n\n",
            ),
        ]);
        let msgs = vec![user_msg("m1", "read these", &["native", "failed", "text"])];
        let out = block_on(rehydrate_user_attachments(
            &seams,
            &msgs,
            "cp",
            true,
            None,
            "ANTHROPIC",
        ));
        assert_eq!(
            out.attachments_to_keep,
            vec![serde_json::json!({ "id": "native" })],
            "only the errorless unsupported file keeps its bytes"
        );
        assert_eq!(
            out.content_by_message_id.get("m1").map(String::as_str),
            Some("[User attached text file: n.md]\n\nbody\n\n"),
            "neither unsupported file contributes prefix text"
        );
    }

    /// No size cap (bug 259, v4 `fa126bde8`): two files whose combined length
    /// exceeds the old 80,000-character ceiling both land whole, in order, and
    /// the DEBUG reports the running sum. No budget WARN exists any more.
    #[test]
    fn every_file_lands_whole_with_no_size_cap() {
        let big = "A".repeat(79_996);
        let over = "B".repeat(8);
        let small = "C".repeat(4);
        let seams = CannedSeams::new(&[
            ("big", false, false, &big),
            ("over", false, false, &over),
            ("small", false, false, &small),
        ]);
        let msgs = vec![user_msg("m1", "tail", &["big", "over", "small"])];
        let (out, lines) = captured(|| {
            block_on(rehydrate_user_attachments(
                &seams,
                &msgs,
                "cp",
                true,
                None,
                "ANTHROPIC",
            ))
        });
        let spliced = out.content_by_message_id.get("m1").cloned().unwrap();
        assert_eq!(spliced, format!("{big}{over}{small}"));
        let debug = line_with(&lines, "Re-hydrated user attachments from history");
        assert!(debug.contains("characters_used=80008"), "{debug}");
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("exceeded the per-turn budget")),
            "{lines:#?}"
        );
    }

    /// The debug line fires when anything came back, and counts what did.
    #[test]
    fn the_debug_line_reports_what_was_rehydrated() {
        let seams = CannedSeams::new(&[("t", false, false, "abcde"), ("n", true, false, "")]);
        let msgs = vec![
            assistant_msg("m0", "cp"),
            user_msg("m1", "one", &["t"]),
            assistant_msg("m2", "other"),
            user_msg("m3", "two", &["n"]),
        ];
        let (out, lines) = captured(|| {
            block_on(rehydrate_user_attachments(
                &seams,
                &msgs,
                "cp",
                true,
                None,
                "ANTHROPIC",
            ))
        });
        assert_eq!(out.content_by_message_id.len(), 1);
        assert_eq!(out.attachments_to_keep.len(), 1);
        let debug = line_with(&lines, "Re-hydrated user attachments from history");
        assert!(debug.starts_with("DEBUG"), "{debug}");
        assert!(debug.contains("messages_expanded=1"), "{debug}");
        assert!(debug.contains("raw_attachments_kept=1"), "{debug}");
        assert!(debug.contains("characters_used=5"), "{debug}");
    }

    /// Nothing unseen → no seam call, no logs, nothing spliced. And an absent
    /// participant returns before the walk runs at all (v4's first line).
    #[test]
    fn a_quiet_turn_calls_nothing_and_logs_nothing() {
        let seams = CannedSeams::new(&[("t", false, false, "x")]);
        let msgs = vec![user_msg("m1", "one", &["t"]), assistant_msg("m2", "cp")];
        let (out, lines) = captured(|| {
            block_on(rehydrate_user_attachments(
                &seams,
                &msgs,
                "cp",
                true,
                None,
                "ANTHROPIC",
            ))
        });
        assert!(out.content_by_message_id.is_empty());
        assert!(out.attachments_to_keep.is_empty());
        assert!(seams.calls.lock().unwrap().is_empty(), "no load was needed");
        assert!(lines.is_empty(), "{lines:#?}");

        let seams2 = CannedSeams::new(&[("t", false, false, "x")]);
        let msgs2 = vec![user_msg("m1", "one", &["t"])];
        let out2 = block_on(rehydrate_user_attachments(
            &seams2,
            &msgs2,
            "",
            true,
            None,
            "ANTHROPIC",
        ));
        assert!(out2.content_by_message_id.is_empty());
        assert!(seams2.calls.lock().unwrap().is_empty());
    }

    /// v4 declares both accumulators OUTSIDE its try, so a mid-walk failure
    /// warns and returns what earlier rows already contributed — the turn
    /// proceeds exactly as it did before this existed.
    #[test]
    fn a_load_failure_warns_and_keeps_what_earlier_rows_contributed() {
        let seams = CannedSeams::new(&[("good", false, false, "kept\n\n")]).failing_on("bad");
        let msgs = vec![
            assistant_msg("m0", "cp"),
            user_msg("m1", "one", &["good"]),
            assistant_msg("m2", "other"),
            user_msg("m3", "two", &["bad"]),
        ];
        let (out, lines) = captured(|| {
            block_on(rehydrate_user_attachments(
                &seams,
                &msgs,
                "cp",
                true,
                None,
                "ANTHROPIC",
            ))
        });
        assert_eq!(
            out.content_by_message_id.get("m1").map(String::as_str),
            Some("kept\n\n"),
            "the earlier row survives the later failure"
        );
        assert!(!out.content_by_message_id.contains_key("m3"));
        let warn = line_with(&lines, "Failed to re-hydrate user attachments from history");
        assert!(warn.starts_with("WARN"), "{warn}");
        assert!(warn.contains("error=storage read failed"), "{warn}");
        assert!(warn.contains("character_participant_id=\"cp\""), "{warn}");
        // The success lines must NOT fire on the failure path (v4's throw jumps
        // past both).
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Re-hydrated user attachments from history")),
            "{lines:#?}"
        );
    }

    // === P4.D367 ===
    // The bug-151 Lantern-only budget (`spend_lantern_image_budget` and its
    // five pins) RETIRED with v4 `29c1e2690`: the one transport budget
    // (`services::image_transport_budget`) spends every image on the request,
    // and its differential is `image_transport_budget_equivalence`. What stays
    // here is this module's share: the re-hydration TAG.

    /// v4's "hands an unseen image back whole and tagged": a re-hydrated image
    /// skips the fallback and rides on `attachments_to_keep` tagged
    /// `rehydrated` with its carrying row's parsed `createdAt` — and adds no
    /// text.
    #[test]
    fn a_rehydrated_image_is_handed_back_whole_and_tagged() {
        let seams = CannedSeams::new(&[("img-1", false, false, "")]).with_images(&["img-1"]);
        let mut carrier = user_msg("u1", "a photo", &["img-1"]);
        carrier.created_at = Some("2026-01-01T00:00:01.000Z".into());
        let out = block_on(rehydrate_user_attachments(
            &seams,
            &[carrier],
            "cp",
            false,
            None,
            "ANTHROPIC",
        ));
        assert!(out.content_by_message_id.is_empty());
        assert_eq!(
            serde_json::to_string(&out.attachments_to_keep).unwrap(),
            r#"[{"id":"img-1","transport":{"origin":"rehydrated","createdAt":1767225601000,"describable":true}}]"#
        );
    }
    // === /P4.D367 ===

    /// The a14a1811 §3 review's named silent mutation: the id-set predicate is
    /// the middle plumbing between the oracle-pinned selector and the
    /// unit-pinned constructor — with it wrong (e.g. a lowercase `"user"`),
    /// the set is empty, tier 2 never fires, and every whisper-tailed
    /// regenerate falls to the last-user floor while everything else stays
    /// green. Each row here kills one mis-spelling of one conjunct.
    #[test]
    fn user_turn_id_set_pins_every_conjunct() {
        let m = |ty: &str, role: &str, sender: Option<&str>, id: Option<&str>| WhisperMessage {
            message_type: Some(ty.to_string()),
            role: Some(role.to_string()),
            system_sender: sender.map(String::from),
            id: id.map(String::from),
            ..Default::default()
        };
        let msgs = vec![
            m("message", "USER", None, Some("keep-1")), // the human's turn
            m("message", "user", None, Some("case-x")), // DB rows are "USER" — lowercase excluded
            m("message", "USER", Some("Host"), Some("whisper-x")), // staff whisper excluded
            m("message", "USER", Some(""), Some("keep-2")), // empty systemSender is JS-falsy
            m("event", "USER", None, Some("event-x")),  // non-message excluded
            m("message", "USER", None, Some("")),       // empty id is JS-falsy
            m("message", "USER", None, None),           // absent id excluded
            m("message", "ASSISTANT", None, Some("asst-x")),
        ];
        let ids = user_turn_message_ids(&msgs);
        assert_eq!(
            ids,
            ["keep-1", "keep-2"].iter().map(|s| s.to_string()).collect()
        );
    }

    fn inter(role: &str, content: &str) -> InterMsg {
        InterMsg {
            role: role.to_string(),
            content: content.to_string(),
            name: None,
            thought_signature: Some(format!("sig-{content}")),
        }
    }

    /// Bug 95's WIRING (v4 a14a1811): the selected anchor — not "the last
    /// message, if role user" — drives the attachment splice. The composition
    /// corpora cannot see this (inert Lantern seam, no attachment ops), so the
    /// extracted constructor is pinned here: flipping `is_anchor` back to the
    /// old last-user rule reddens both asserts. (P4.D367: the anchor's content
    /// is its own — the Lantern prefix splice is gone.)
    #[test]
    fn anchor_not_last_user_drives_the_splice() {
        // The bug-95 shape: the human's turn at index 1, a whisper wearing
        // role=user after it.
        let msgs = vec![
            inter("assistant", "earlier"),
            inter("user", "look at this picture"),
            inter("user", "[Host] the clock strikes nine"),
        ];
        let atts = vec![serde_json::json!({"id": "att-1"})];
        let out = construct_formatted_messages(&msgs, 1, &atts);
        assert_eq!(out.len(), 3);
        // The anchor (index 1) carries the attachments with its own content,
        // and drops its thought signature exactly as v4's attachment-arm
        // literal does.
        assert_eq!(out[1].content, "look at this picture");
        assert_eq!(out[1].attachments.as_deref(), Some(&atts[..]));
        assert_eq!(out[1].thought_signature, None);
        // The trailing whisper — the OLD anchor — stays bare.
        assert_eq!(out[2].content, "[Host] the clock strikes nine");
        assert!(out[2].attachments.is_none());
        assert_eq!(
            out[2].thought_signature.as_deref(),
            Some("sig-[Host] the clock strikes nine")
        );
        // Non-anchor messages keep their signatures.
        assert_eq!(out[0].thought_signature.as_deref(), Some("sig-earlier"));
    }

    #[test]
    fn anchor_minus_one_splices_nothing() {
        // -1 matches no index: the bytes are delivered nowhere (the caller
        // warns) rather than stapled to a wrong message.
        let msgs = vec![inter("system", "s"), inter("assistant", "a")];
        let atts = vec![serde_json::json!({"id": "att-1"})];
        let out = construct_formatted_messages(&msgs, -1, &atts);
        assert!(out.iter().all(|m| m.attachments.is_none()));
    }

    fn msg(fields: &[(&str, Value)]) -> WhisperMessage {
        let mut m = serde_json::Map::new();
        for (k, v) in fields {
            m.insert((*k).to_string(), v.clone());
        }
        WhisperMessage::from_value(&Value::Object(m))
    }

    #[test]
    fn conversation_filters_and_roles() {
        let messages = vec![
            msg(&[
                ("type", Value::from("message")),
                ("role", Value::from("USER")),
                ("content", Value::from("hi")),
                ("id", Value::from("u1")),
            ]),
            msg(&[
                ("type", Value::from("context-summary")),
                ("role", Value::from("SYSTEM")),
                ("content", Value::from("summary")),
            ]),
            msg(&[
                ("type", Value::from("message")),
                ("role", Value::from("ASSISTANT")),
                ("content", Value::from("hello")),
                ("id", Value::from("a1")),
                ("thoughtSignature", Value::from("sig")),
            ]),
        ];
        let (conv, mwp) = build_conversation_messages(&messages, false);
        assert_eq!(conv.len(), 2);
        assert_eq!(conv[0].role, "USER");
        assert_eq!(conv[1].role, "ASSISTANT");
        assert_eq!(conv[1].thought_signature.as_deref(), Some("sig"));
        assert!(mwp.is_none());
    }

    #[test]
    fn tool_result_verbatim_vs_elided() {
        let tool =
            serde_json::json!({ "toolName": "roll", "result": "42", "arguments": { "sides": 6 } });
        assert_eq!(
            render_tool_result_content(&tool, 0),
            "[Tool Result: roll]\n42"
        );
        let elided = render_tool_result_content(&tool, 3);
        assert!(elided.contains("result elided (>3 turns old)"));
        assert!(elided.contains("[Tool Result: roll]"));
    }

    #[test]
    fn normalize_reroles_staff_whisper() {
        let messages = vec![
            msg(&[
                ("type", Value::from("message")),
                ("role", Value::from("ASSISTANT")),
                ("content", Value::from("The Librarian files a note.")),
                ("systemSender", Value::from("librarian")),
                ("opaqueContent", Value::from("A note was filed.")),
            ]),
            // Attachment-bearing whisper stays ASSISTANT.
            msg(&[
                ("type", Value::from("message")),
                ("role", Value::from("ASSISTANT")),
                ("content", Value::from("Here is an image.")),
                ("systemSender", Value::from("lantern")),
                ("attachments", serde_json::json!(["file-1"])),
            ]),
            // Non-whisper untouched.
            msg(&[
                ("type", Value::from("message")),
                ("role", Value::from("USER")),
                ("content", Value::from("plain")),
            ]),
        ];
        let opaque = normalize_whisper_roles(&messages, true);
        assert_eq!(opaque[0].role.as_deref(), Some("USER"));
        assert_eq!(opaque[0].content.as_deref(), Some("A note was filed."));
        assert!(opaque[0].system_sender.is_none());
        assert_eq!(opaque[1].role.as_deref(), Some("ASSISTANT"));
        assert!(opaque[1].system_sender.is_none());
        assert_eq!(opaque[2].role.as_deref(), Some("USER"));
        assert_eq!(opaque[2].content.as_deref(), Some("plain"));

        // Transparent: keep the persona body.
        let transparent = normalize_whisper_roles(&messages, false);
        assert_eq!(
            transparent[0].content.as_deref(),
            Some("The Librarian files a note.")
        );
    }

    #[test]
    fn lantern_walk_collects_until_own_turn() {
        // Multi-character: stop at the character's own participantId; collect newer
        // lantern images (attachment-bearing, participantId null).
        let cp = "cp-friday";
        let messages = vec![
            msg(&[
                ("type", Value::from("message")),
                ("role", Value::from("ASSISTANT")),
                ("participantId", Value::from(cp)),
                ("content", Value::from("Friday's older turn")),
            ]),
            msg(&[
                ("type", Value::from("message")),
                ("role", Value::from("ASSISTANT")),
                ("attachments", serde_json::json!(["img-1"])),
                ("content", Value::from("lantern")),
            ]),
            msg(&[
                ("type", Value::from("message")),
                ("role", Value::from("ASSISTANT")),
                ("participantId", Value::from(cp)),
                ("content", Value::from("Friday's latest own turn")),
            ]),
        ];
        // Walking from the end: the last message is the character's own turn → stop
        // immediately → nothing collected.
        let ids = collect_lantern_image_file_ids_for_character(&messages, cp, true, None, 6);
        assert!(ids.is_empty());

        // Drop the trailing own-turn: now the lantern image is newer than the own
        // turn → collected.
        let ids2 = collect_lantern_image_file_ids_for_character(&messages[..2], cp, true, None, 6);
        assert_eq!(ids2, vec!["img-1".to_string()]);
    }

    /// v4 `a163862c` deleted the `if (m.role !== 'TOOL') return true` first line
    /// from this filter so it "applies the same test as filterWhisperMessages to
    /// every role". Pin that equivalence directly: over a table that crosses every
    /// role with every target shape, the section-B predicate and the shared
    /// downstream rule must agree row for row. (The two are structurally identical
    /// today; this is the test that fails if either drifts — and it is the only
    /// place the widening is observable at all, since every chat that can produce
    /// an LLM turn is multi-character and therefore ALSO runs
    /// `filter_whisper_messages` downstream. See the P4.D37 lane record.)
    #[test]
    fn message_context_whisper_predicate_matches_the_shared_rule() {
        use crate::message_attribution::{filter_whisper_messages, AttributionMessage};

        const ME: &str = "p-me";
        let target_shapes: [Option<Vec<String>>; 5] = [
            None,
            Some(vec![]),
            Some(vec![ME.to_string()]),
            Some(vec!["p-other".to_string()]),
            Some(vec!["p-other".to_string(), ME.to_string()]),
        ];
        let senders = [None, Some(ME.to_string()), Some("p-other".to_string())];
        let roles = ["TOOL", "USER", "ASSISTANT", "SYSTEM"];

        let mut rows = 0;
        for role in roles {
            for targets in &target_shapes {
                for sender in &senders {
                    let wm = WhisperMessage {
                        role: Some(role.to_string()),
                        participant_id: sender.clone(),
                        target_participant_ids: targets.clone(),
                        ..Default::default()
                    };
                    let am = AttributionMessage {
                        id: None,
                        role: role.to_string(),
                        content: String::new(),
                        participant_id: sender.clone(),
                        thought_signature: None,
                        created_at: None,
                        target_participant_ids: targets.clone(),
                        host_event: None,
                        is_established_fact: false,
                    };
                    assert_eq!(
                        is_whisper_party(&wm, ME),
                        filter_whisper_messages(std::slice::from_ref(&am), ME)[0],
                        "role={role} targets={targets:?} sender={sender:?}"
                    );
                    rows += 1;
                }
            }
        }
        assert_eq!(rows, 60, "the cross table lost rows");

        // And the arm the widening added: a targeted NON-TOOL message addressed to
        // someone else is now dropped, where the old guard let it through.
        let whispered_announcement = WhisperMessage {
            role: Some("ASSISTANT".to_string()),
            participant_id: None,
            target_participant_ids: Some(vec!["p-other".to_string()]),
            system_kind: Some("announcement".to_string()),
            ..Default::default()
        };
        assert!(!is_whisper_party(&whispered_announcement, ME));
    }

    /// P4.137 (P4.D243-F1) — the attachment anchor on an EMPTY new user
    /// message. v4's send schema admits `content: ''` beside files
    /// (`orchestrator.service.ts:148-171`) and `buildContext`'s `if
    /// (newUserMessage)` (`context-manager.ts:2726`) then pushes NO user line
    /// and sets NO `isUserTurn`, so `selectAttachmentAnchorIndex`
    /// (`context-builder.service.ts:730-745`) falls through its ladder: the
    /// previous human row (preference 2) when there is one, else `-1` — the
    /// WARN at `:1272-1276` and the attachments dropped. Until P4.137 v5 took
    /// the first-responder branch on `Some("")`, flagged an EMPTY user line
    /// as the anchor, and this arm was unreachable on a non-continue turn.
    /// Driven through the real `build_message_context` over a provisioned
    /// instance; M1 (the branch back to `if let Some(..) =
    /// &input.new_user_message`) reddens both arms.
    mod empty_new_user_message_anchor {
        use super::*;
        use crate::model::completion::{
            CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
        };
        use crate::model::embedding::{
            EmbeddingCallOptions, EmbeddingError, EmbeddingProvider, EmbeddingResult,
        };
        use crate::services::build_context::{ContextCharacter, ContextChat, NoopSeams};

        const PEPPER: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        const WARN: &str = "Image attachments could not be anchored — no user-role message in context; images will not reach the model";

        struct UnusedCompletion;
        impl CompletionProvider for UnusedCompletion {
            async fn send_message(
                &self,
                _provider: &str,
                _base_url: Option<&str>,
                _params: &CompletionParams,
            ) -> Result<CompletionResponse, CompletionError> {
                Err(CompletionError::new("no completion provider in this pin"))
            }
        }
        struct FailingEmbedding;
        impl EmbeddingProvider for FailingEmbedding {
            async fn generate_embedding_for_user(
                &self,
                _text: &str,
                _user_id: &str,
                _profile_id: Option<&str>,
                _opts: EmbeddingCallOptions,
            ) -> Result<EmbeddingResult, EmbeddingError> {
                Err(EmbeddingError::new("no embedding provider in this pin"))
            }
        }

        fn lyra() -> ContextCharacter {
            ContextCharacter {
                id: "char-a".to_string(),
                name: "Lyra".to_string(),
                character_document_mount_point_id: None,
                sys: crate::system_prompt::Character {
                    name: "Lyra".to_string(),
                    ..Default::default()
                },
                metadata: None,
            }
        }

        /// A single-character, non-continue turn whose new user message is the
        /// empty string — the carrier the orchestrator builds for a send of
        /// `content: ""` with files and no inlined prefix.
        fn an_empty_send() -> BuildContextInput {
            BuildContextInput {
                model_context_limit: 32_000,
                user_id: "user".to_string(),
                character: lyra(),
                user_character: None,
                chat: ContextChat {
                    id: "chat-1".to_string(),
                    ..Default::default()
                },
                existing_messages: Vec::new(),
                new_user_message: Some(String::new()),
                active_user_participant_id: None,
                roleplay_template: None,
                embedding_profile_id: None,
                provider: None,
                skip_memories: true,
                min_memory_importance: 0.1,
                responding_participant: None,
                all_participants: None,
                participant_characters: None,
                messages_with_participants: None,
                tool_instructions: None,
                timestamp_config: None,
                is_initial_message: false,
                timezone: None,
                connection_profile: None,
                context_compression_settings: None,
                cheap_llm_selection: None,
                bypass_compression: true,
                cached_compression_result: None,
                cached_compression_message_count: None,
                generate_memory_recap: false,
                uncensored_fallback: None,
                is_continue_mode: false,
                now_ms: 1_767_225_600_000,
                local_offset_minutes: 0,
                server_tz: Some("UTC".to_string()),
                display_zone: crate::host_zone::TimeZone::UTC,
                minutes_since_last_timestamp_announcement: None,
                autonomous_context_cap: None,
                reserved_outgoing_tokens: None,
                regeneration_of_message_ids: None,
                turn_skip: None,
                offer_yield: false,
                pre_recall: None,
                human_turn_message_ids: None,
                purpose: crate::services::build_context::ContextPurpose::Turn,
                storyteller_section: None,
            }
        }

        fn row(id: &str, role: &str, content: &str) -> Value {
            serde_json::json!({ "type": "message", "id": id, "role": role, "content": content })
        }

        async fn send(history: &[Value]) -> (Vec<FormattedMsg>, Vec<String>) {
            let dir = tempfile::tempdir().expect("tempdir");
            crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER)
                .expect("provision a fresh instance");
            let db = Db::open_main(dir.path().join("quilltap.db"), PEPPER).expect("open main");
            let seams = CannedSeams::new(&[]);
            let transparency = std::collections::HashMap::new();
            let params = MessageContextParams {
                is_multi_character: false,
                provider: "ANTHROPIC",
                responding_character_name: "Lyra",
                responding_participant_id: "participant-a",
                has_history_access: true,
                character_participant_created_at: None,
                character_system_transparency: Some(true),
                use_prefill: false,
                participants: &[],
                participant_transparency: &transparency,
            };
            let attachments = [serde_json::json!({ "id": "img-1" })];

            use tracing_subscriber::layer::SubscriberExt;
            let logs = Arc::new(Mutex::new(Vec::<String>::new()));
            let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
            let out = {
                let _guard = tracing::subscriber::set_default(subscriber);
                build_message_context(
                    &db,
                    &FailingEmbedding,
                    &UnusedCompletion,
                    &CheapLlmTaskExecutor::new(),
                    &NoopSeams,
                    &seams,
                    &params,
                    an_empty_send(),
                    history,
                    &attachments,
                )
                .await
                .unwrap_or_else(|e| panic!("the turn must build: {e:?}"))
            };
            let lines = logs.lock().unwrap().clone();
            (out.formatted_messages, lines)
        }

        fn anchored(msgs: &[FormattedMsg]) -> Vec<&FormattedMsg> {
            msgs.iter().filter(|m| m.attachments.is_some()).collect()
        }

        /// One prior human row: the attachments ride on the human's PREVIOUS
        /// turn, no empty user line exists to take them, and the WARN stays
        /// silent. The one human row is also the LAST user row, so v4's
        /// preference 2 (a row in `userTurnMessageIds`) and preference 3 (the
        /// last user row) pick the same index here. This arm proves the anchor
        /// lands on the prior human turn, not which rung chose it.
        #[tokio::test]
        async fn the_attachments_anchor_on_the_previous_human_row() {
            let (msgs, lines) = send(&[
                row("h1", "USER", "Here is the map."),
                row("a1", "ASSISTANT", "Noted."),
            ])
            .await;
            let on = anchored(&msgs);
            assert_eq!(on.len(), 1, "{msgs:#?}");
            assert_eq!(on[0].role, "user");
            assert_eq!(on[0].content, "Here is the map.");
            assert_eq!(
                on[0].attachments.as_deref(),
                Some(&[serde_json::json!({ "id": "img-1" })][..])
            );
            assert!(
                !msgs
                    .iter()
                    .any(|m| m.role == "user" && m.content.is_empty()),
                "no empty user line: {msgs:#?}"
            );
            assert_eq!(
                msgs.last().unwrap().content,
                "Noted.",
                "nothing pushed after the history"
            );
            assert!(!lines.iter().any(|l| l.contains(WARN)), "{lines:#?}");
        }

        /// No human row at all: v4's `-1` — the WARN, byte for byte with its
        /// two counts, and the attachments reach no message.
        #[tokio::test]
        async fn with_no_human_row_the_attachments_drop_with_v4s_warn() {
            let (msgs, lines) = send(&[row("a1", "ASSISTANT", "Good evening.")]).await;
            assert!(anchored(&msgs).is_empty(), "{msgs:#?}");
            assert!(!msgs.iter().any(|m| m.role == "user"), "{msgs:#?}");
            let warns: Vec<&String> = lines.iter().filter(|l| l.contains(WARN)).collect();
            assert_eq!(warns.len(), 1, "{lines:#?}");
            assert_eq!(
                warns[0],
                &format!("WARN {WARN} attachment_count=1 context_message_count=3")
            );
        }

        // === P4.D367 ===
        /// [`send`] generalised: the seam, the attachments, and the
        /// multi-character switches are the caller's.
        async fn send_with(
            seams: &CannedSeams,
            history: &[Value],
            attachments: &[Value],
            is_multi: bool,
        ) -> Vec<FormattedMsg> {
            let dir = tempfile::tempdir().expect("tempdir");
            crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER)
                .expect("provision a fresh instance");
            let db = Db::open_main(dir.path().join("quilltap.db"), PEPPER).expect("open main");
            let transparency = std::collections::HashMap::new();
            let params = MessageContextParams {
                is_multi_character: is_multi,
                provider: "ANTHROPIC",
                responding_character_name: "Lyra",
                responding_participant_id: "participant-a",
                has_history_access: true,
                character_participant_created_at: None,
                character_system_transparency: Some(true),
                use_prefill: is_multi,
                participants: &[],
                participant_transparency: &transparency,
            };
            build_message_context(
                &db,
                &FailingEmbedding,
                &UnusedCompletion,
                &CheapLlmTaskExecutor::new(),
                &NoopSeams,
                seams,
                &params,
                an_empty_send(),
                history,
                attachments,
            )
            .await
            .unwrap_or_else(|e| panic!("the turn must build: {e:?}"))
            .formatted_messages
        }

        fn lantern_row(id: &str, created_at: &str, atts: &[&str]) -> Value {
            serde_json::json!({
                "type": "message", "id": id, "role": "ASSISTANT",
                "content": "The Lantern hangs a picture.", "systemSender": "lantern",
                "createdAt": created_at, "attachments": atts,
            })
        }

        /// The Lantern walk TAGS and nothing more (v4 `:1299-1316`): an id
        /// carried on two ASSISTANT rows takes the LATER `createdAt`; a
        /// non-`ASSISTANT` carrier contributes nothing (`0`); and a file the
        /// `files` table does not hold is not describable (R5). The pass is
        /// handed the ANCHOR-bearing array — after the splice — exactly once.
        #[tokio::test]
        async fn the_lantern_walk_tags_with_the_latest_carrier_and_the_pass_sees_the_anchor() {
            let lantern = vec![
                serde_json::json!({"id": "pic-a", "mimeType": "image/webp", "data": "AA"}),
                serde_json::json!({"id": "pic-b", "mimeType": "image/webp", "data": "BB"}),
            ];
            let seams = CannedSeams::new(&[]).with_lantern(lantern);
            let history = [
                row("h1", "USER", "Hang them, please."),
                lantern_row("l1", "2026-01-01T00:00:01.000Z", &["pic-a"]),
                lantern_row("l2", "2026-01-01T00:00:05.000Z", &["pic-a"]),
                // A USER row also naming `pic-b`: not an ASSISTANT carrier.
                serde_json::json!({"type": "message", "id": "u2", "role": "USER",
                    "content": "and this", "createdAt": "2026-01-01T00:00:09.000Z",
                    "attachments": ["pic-b"]}),
                lantern_row("l3", "", &["pic-b"]),
            ];
            let out = send_with(&seams, &history, &[], false).await;
            let passes = seams.passes.lock().unwrap();
            assert_eq!(passes.len(), 1, "one pass per build");
            assert_eq!(
                passes[0], out,
                "single-character: the pass's array is the result"
            );
            let anchored: Vec<&FormattedMsg> = passes[0]
                .iter()
                .filter(|m| m.attachments.is_some())
                .collect();
            assert_eq!(anchored.len(), 1, "{:#?}", passes[0]);
            assert_eq!(
                serde_json::to_string(anchored[0].attachments.as_ref().unwrap()).unwrap(),
                concat!(
                    r#"[{"id":"pic-a","mimeType":"image/webp","data":"AA","transport":{"origin":"lantern","createdAt":1767225605000,"describable":false}},"#,
                    r#"{"id":"pic-b","mimeType":"image/webp","data":"BB","transport":{"origin":"lantern","createdAt":0,"describable":false}}]"#
                )
            );
        }

        /// The pass runs BEFORE the multi-character turn anchor (v4 `:1356` vs
        /// `:1377`): the prefill route's `[Lyra]` line is absent from the array
        /// the pass was handed and present in the result.
        #[tokio::test]
        async fn the_pass_runs_before_the_multi_character_anchor() {
            let seams = CannedSeams::new(&[]);
            let out = send_with(
                &seams,
                &[row("h1", "USER", "Here is the map.")],
                &[serde_json::json!({"id": "img-1", "mimeType": "image/png"})],
                true,
            )
            .await;
            let passes = seams.passes.lock().unwrap();
            assert_eq!(passes.len(), 1);
            assert_eq!(out.last().map(|m| m.content.as_str()), Some("[Lyra]"));
            assert!(
                !passes[0].iter().any(|m| m.content == "[Lyra]"),
                "{:#?}",
                passes[0]
            );
            assert_eq!(passes[0].len() + 1, out.len());
        }

        /// No attachments, no pass (v4's `mergedAttachmentsToSend.length > 0`
        /// gate).
        #[tokio::test]
        async fn the_pass_is_not_called_without_attachments() {
            let seams = CannedSeams::new(&[]);
            let _ = send_with(&seams, &[row("h1", "USER", "Just words.")], &[], false).await;
            assert!(seams.passes.lock().unwrap().is_empty());
        }
        // === /P4.D367 ===
    }

    // === P4.D372 ===
    mod established_fact_pass {
        use super::super::*;
        use crate::established_fact::{
            leak_tests, EstablishedFactVoice, ESTABLISHED_FACT_HOST_FRAME,
            ESTABLISHED_FACT_PLAIN_FRAME, ESTABLISHED_FACT_SEAT_FRAME,
        };

        fn rows() -> Vec<WhisperMessage> {
            [
                leak_tests::seat_line(),
                leak_tests::machine_fact(),
                leak_tests::reply(),
            ]
            .iter()
            .map(WhisperMessage::from_value)
            .collect()
        }

        #[test]
        fn the_pass_frames_in_the_roster_voice_and_logs_once() {
            for (opaque, voice, frame) in [
                (false, "host", ESTABLISHED_FACT_HOST_FRAME),
                (true, "plain", ESTABLISHED_FACT_PLAIN_FRAME),
            ] {
                let input = rows();
                let ((framed, ids), lines) = crate::test_support::captured_with(|| {
                    let (f, i) = frame_established_fact_pass(&input, opaque, "chat-1");
                    (f.into_owned(), i)
                });
                assert_eq!(
                    lines,
                    vec![format!(
                        "DEBUG quilltap::context [Context] Framed established facts chatId=chat-1 seatLines=1 machineFacts=1 voice={voice}"
                    )]
                );
                let mut ids: Vec<_> = ids.unwrap().into_iter().collect();
                ids.sort();
                assert_eq!(ids, ["m-fact", "m-seat"]);
                let machine = &framed[1];
                assert!(machine.content.as_deref().unwrap().starts_with(frame));
                // Both bodies carry the frame, so whichever the Staff swap picks reads it.
                assert_eq!(machine.opaque_content, machine.content);
                assert!(framed[0]
                    .content
                    .as_deref()
                    .unwrap()
                    .starts_with(ESTABLISHED_FACT_SEAT_FRAME));
                assert_eq!(
                    framed[2].content.as_deref(),
                    Some("Then we take the ferry.")
                );
                // The leak walk: normalise + conversation (v4's order), then the text.
                let normalised = normalize_whisper_roles(&framed, opaque);
                let (conversation, _) = build_conversation_messages(&normalised, false);
                let text = format!("{conversation:?}");
                let v = if opaque {
                    EstablishedFactVoice::Plain
                } else {
                    EstablishedFactVoice::Host
                };
                leak_tests::expect_facts_framed(&text.replace("\\n", "\n"), v);
                // The machine fact reads as an input to the character, not its speech.
                let m = conversation
                    .iter()
                    .find(|c| c.content.contains(leak_tests::MACHINE_FACT))
                    .unwrap();
                assert_eq!(m.role.to_uppercase(), "USER", "{conversation:?}");
            }
        }

        #[test]
        fn no_fact_means_no_line_and_the_same_history() {
            let input: Vec<WhisperMessage> = vec![WhisperMessage::from_value(&leak_tests::reply())];
            let ((same, ids), lines) = crate::test_support::captured_with(|| {
                let (f, i) = frame_established_fact_pass(&input, false, "chat-1");
                (matches!(f, std::borrow::Cow::Borrowed(_)), i)
            });
            assert!(same && ids.is_none());
            assert!(lines.is_empty(), "{lines:?}");
        }
    }
    // === /P4.D372 ===
}
