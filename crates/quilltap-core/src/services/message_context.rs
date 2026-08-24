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

        // The character's own previous ASSISTANT turn stops the walk.
        let is_own_prior_response = if is_multi_character {
            msg.participant_id.as_deref() == Some(character_participant_id)
        } else {
            !has_attachments
        };
        if is_own_prior_response {
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

// ===========================================================================
// The K file-loading seam (unported wave-4 file subsystem).
// ===========================================================================

/// The result of the seamed Lantern image load (v4 `loadChatFilesForLLM` +
/// `processFileAttachmentFallback` + `formatFallbackAsMessagePrefix`): a prefix
/// string to inject onto the last user message and the attachments to keep.
#[derive(Clone, Debug, Default)]
pub struct LanternLoad {
    pub prefix: String,
    pub attachments: Vec<Value>,
}

/// The file-subsystem seam (v4 `buildMessageContext` section K). Default no-op;
/// [`crate::services::chat_files::RealMessageContextSeams`] (W4.4b) does the real
/// load + fallback dispatch. Async (RPITIT `+ Send`), matching
/// [`crate::services::build_context::BuildContextSeams`] — the real body issues a
/// vision-description call for any unsupported prior image.
pub trait MessageContextSeams {
    /// Load + fallback-process the collected Lantern image file ids for a provider.
    fn load_lantern_images(
        &self,
        file_ids: &[String],
        provider: &str,
    ) -> impl std::future::Future<Output = LanternLoad> + Send {
        let _ = (file_ids, provider);
        async { LanternLoad::default() }
    }
}

/// The default no-op seam.
pub struct NoopMessageContextSeams;
impl MessageContextSeams for NoopMessageContextSeams {}

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

    // --- A. Drop persisted Commonplace-Book whispers (except relevant-conversations). ---
    let is_strippable_cmpb = |m: &WhisperMessage| {
        m.system_sender.as_deref() == Some("commonplaceBook")
            && m.system_kind.as_deref() != Some("relevant-conversations")
    };
    let cmpb_stripped = parsed.iter().filter(|m| is_strippable_cmpb(m)).count();
    let messages_without_cmpb: Vec<WhisperMessage> = if cmpb_stripped > 0 {
        parsed
            .into_iter()
            .filter(|m| !is_strippable_cmpb(m))
            .collect()
    } else {
        parsed
    };

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

    // --- D. Whisper-role normalization. ---
    let filtered = normalize_whisper_roles(&messages_after_whisper_filter, is_opaque_anywhere);

    // Row ids of the *human's* own turns, captured before normalization erases
    // the distinction (v4 a14a1811, bug 95) — see [`user_turn_message_ids`].
    let user_turn_message_ids = user_turn_message_ids(&messages_after_whisper_filter);

    // --- E. Conversation messages + first-message detection. ---
    let (conversation, mwp) = build_conversation_messages(&filtered, is_multi);
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
        })
        .collect();
    build_input.messages_with_participants = if is_multi { mwp } else { None };
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
    let mut merged_attachments: Vec<Value> = attachments_to_send.to_vec();
    let mut lantern_prefix = String::new();
    {
        let has_prior_response = filtered.iter().any(|m| {
            m.message_type.as_deref() == Some("message")
                && m.role.as_deref() == Some("ASSISTANT")
                && m.participant_id.as_deref() == Some(responding_id)
        });
        let history_cutoff: Option<String> =
            if is_multi && !params.has_history_access && !has_prior_response {
                params.character_participant_created_at.map(String::from)
            } else {
                None
            };
        let ids = collect_lantern_image_file_ids_for_character(
            &filtered,
            responding_id,
            is_multi,
            history_cutoff.as_deref(),
            ASSISTANT_IMAGE_LOOKBACK,
        );
        if !ids.is_empty() {
            let load = mc_seams.load_lantern_images(&ids, params.provider).await;
            lantern_prefix = load.prefix;
            if !load.attachments.is_empty() {
                merged_attachments = attachments_to_send
                    .iter()
                    .cloned()
                    .chain(load.attachments)
                    .collect();
            }
        }
    }

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

    let mut formatted_messages: Vec<FormattedMsg> = construct_formatted_messages(
        &intermediate,
        attachment_anchor_index,
        &lantern_prefix,
        &merged_attachments,
    );

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

/// Section L (v4 context-builder.service.ts:952–981) — the final message
/// construction. The ANCHOR message (bug 95, v4 a14a1811 — previously
/// "the last message, if role user") gets the Lantern prefix prepended and
/// carries the merged attachments (its `thought_signature` is dropped in the
/// attachment arm exactly as v4's object literal omits it); every other
/// message rides through with `thought_signature` intact and no attachments.
/// An anchor index of -1 matches no message: nothing is spliced, which the
/// caller has already warned about.
///
/// Extracted so the WIRING — that the selected anchor, not "last user",
/// drives BOTH the prefix and the splice — is unit-pinned (the composition
/// corpora cannot see it: their Lantern seam is inert and their ops carry no
/// attachments).
fn construct_formatted_messages(
    intermediate: &[InterMsg],
    attachment_anchor_index: i64,
    lantern_prefix: &str,
    merged_attachments: &[Value],
) -> Vec<FormattedMsg> {
    let mut formatted_messages: Vec<FormattedMsg> = Vec::new();
    for (idx, m) in intermediate.iter().enumerate() {
        let is_anchor = idx as i64 == attachment_anchor_index;
        let content = if is_anchor && !lantern_prefix.is_empty() {
            format!("{lantern_prefix}{}", m.content)
        } else {
            m.content.clone()
        };
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
    /// message, if role user" — drives BOTH the Lantern prefix and the
    /// attachment splice. The composition corpora cannot see this (inert
    /// Lantern seam, no attachment ops), so the extracted constructor is
    /// pinned here: flipping `is_anchor` back to the old last-user rule
    /// reddens both asserts.
    #[test]
    fn anchor_not_last_user_drives_prefix_and_splice() {
        // The bug-95 shape: the human's turn at index 1, a whisper wearing
        // role=user after it.
        let msgs = vec![
            inter("assistant", "earlier"),
            inter("user", "look at this picture"),
            inter("user", "[Host] the clock strikes nine"),
        ];
        let atts = vec![serde_json::json!({"id": "att-1"})];
        let out = construct_formatted_messages(&msgs, 1, "[Image: photo.png]\n", &atts);
        assert_eq!(out.len(), 3);
        // The anchor (index 1) carries prefix + attachments, and drops its
        // thought signature exactly as v4's attachment-arm literal does.
        assert_eq!(out[1].content, "[Image: photo.png]\nlook at this picture");
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
        let out = construct_formatted_messages(&msgs, -1, "prefix", &atts);
        assert!(out.iter().all(|m| m.attachments.is_none()));
        assert!(out.iter().all(|m| !m.content.starts_with("prefix")));
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
}
