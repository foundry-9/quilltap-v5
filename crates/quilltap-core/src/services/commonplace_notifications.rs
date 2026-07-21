//! The Commonplace Book chat-whisper writer + the fold-triggered
//! relevant-conversations refresh — v4
//! `lib/services/commonplace-notifications/{writer,relevant-conversations-refresh}.ts`.
//!
//! The Commonplace Book is the personified memory system. When a character is
//! about to take a turn, what they "remember" (narrative recap, relevant
//! memories, inter-character memories, knowledge) is handed to the Commonplace
//! Book to whisper into the conversation as a targeted `ASSISTANT`-role message
//! that only the responding character can see (`systemSender: 'commonplaceBook'`).
//!
//! Targeting is via `targetParticipantIds`: a single participant id = private to
//! that character + sender; `null` = public (single-present-character chats).
//! Commonplace Book whispers are always recall, never announcement.
//!
//! Errors never propagate — turn processing must never fail because a memory
//! whisper couldn't be written (best-effort; failures surface as `None` / a
//! swallowed no-op).
//!
//! # Handoffs
//! - The pure builders [`build_commonplace_persona_whisper`] /
//!   [`build_commonplace_llm_context`] are the CANONICAL public versions (their
//!   true v4 home is this writer file). `services::build_context` carries PRIVATE
//!   copies (5-part, no `relevantConversations`) that predate this port; the
//!   spine should later import these six-part versions and drop its copies.
//! - [`post_commonplace_whisper`] closes `BuildContextSeams::post_commonplace_-
//!   whisper`; [`refresh_relevant_conversations_on_fold`] closes
//!   `ContextSummarySeams::refresh_relevant_conversations`. The parent wires both
//!   seams (their trait methods live in `build_context` / `context_summary`,
//!   which are spine-owned and not edited here).

use serde_json::json;

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::model::embedding::EmbeddingProvider;
use crate::services::memory_recap::{
    ramp_limit, render_relevant_conversations_block, search_vault_conversation_summaries,
    READ_CONVERSATION_CALL_NOTE,
};

/// v4 `CommonplaceWhisperKind` — the `systemKind` tag of a Commonplace-Book
/// whisper. The per-turn `consolidated` recall is stripped from LLM context and
/// swept each turn; `relevant-conversations` PERSISTS across turns (refreshed on
/// each fold, exempt from the consolidated sweep + the LLM-context strip).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommonplaceWhisperKind {
    MemoryRecap,
    RelevantMemories,
    InterCharacterMemories,
    Consolidated,
    RelevantConversations,
}

impl CommonplaceWhisperKind {
    /// The exact v4 string-union member (written verbatim into `systemKind`).
    pub fn as_str(self) -> &'static str {
        match self {
            CommonplaceWhisperKind::MemoryRecap => "memory-recap",
            CommonplaceWhisperKind::RelevantMemories => "relevant-memories",
            CommonplaceWhisperKind::InterCharacterMemories => "inter-character-memories",
            CommonplaceWhisperKind::Consolidated => "consolidated",
            CommonplaceWhisperKind::RelevantConversations => "relevant-conversations",
        }
    }
}

/// v4 `CommonplaceParts` — the memory content parts for a single turn's recall.
/// Each section is raw formatted text (with its existing markdown headers). Every
/// field is `parts.x?.trim()`-truthy: absent OR whitespace-only = dropped.
#[derive(Clone, Debug, Default)]
pub struct CommonplaceParts {
    /// Snapshot of the chat's present moment; rendered ahead of memory sections.
    pub current_state: Option<String>,
    /// Narrative recap (chat start / character join only).
    pub recap: Option<String>,
    /// Semantic-search results that bear on the current moment.
    pub relevant: Option<String>,
    /// Memories about other present characters (multi-character chats only).
    pub inter_char: Option<String>,
    /// Material from the character's own `Knowledge/` vault folder.
    pub knowledge: Option<String>,
    /// The relevant-past-conversations list (rendered block + call note). Used by
    /// the fold-triggered refresh whisper; the per-turn consolidated whisper
    /// leaves this empty.
    pub relevant_conversations: Option<String>,
}

/// Trim + drop-empty view of a part (v4 `parts.x?.trim()` truthiness).
fn trimmed(v: &Option<String>) -> Option<&str> {
    v.as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty())
}

/// v4 `buildCommonplacePersonaWhisper` — the persona-voiced consolidated whisper
/// for the transcript / UI. The Commonplace Book speaks once, bundling whichever
/// sections are non-empty; the joined body is `.trim()`ed. Returns `""` when
/// nothing is set (v4 returns the empty string, treated as falsy by callers).
pub fn build_commonplace_persona_whisper(parts: &CommonplaceParts) -> String {
    let mut sections: Vec<String> = Vec::new();
    if let Some(s) = trimmed(&parts.current_state) {
        sections.push(format!(
            "*The Commonplace Book sets its memories aside for a moment to take stock of where you stand —*\n\n{s}"
        ));
    }
    if let Some(s) = trimmed(&parts.recap) {
        sections.push(format!(
            "*The Commonplace Book lays open at your bookmark; here is the gist of what you have noted so far —*\n\n{s}"
        ));
    }
    if let Some(s) = trimmed(&parts.relevant) {
        sections.push(format!(
            "*The Commonplace Book turns to the entries that bear on this moment.*\n\n{s}"
        ));
    }
    if let Some(s) = trimmed(&parts.inter_char) {
        sections.push(format!(
            "*The Commonplace Book opens to the pages where you have noted those present.*\n\n{s}"
        ));
    }
    if let Some(s) = trimmed(&parts.knowledge) {
        sections.push(format!(
            "*The Commonplace Book pulls down volumes from your own shelves — the references and reckonings you yourself have curated —*\n\n{s}"
        ));
    }
    if let Some(s) = trimmed(&parts.relevant_conversations) {
        sections.push(format!(
            "*The conversation has wandered on, and the Commonplace Book re-marks the past dialogues that now bear on the present —*\n\n{s}"
        ));
    }
    crate::jsstr::js_trim(&sections.join("\n\n")).to_string()
}

/// v4 `buildCommonplaceLLMContext` — the plain second-person framing for the LLM
/// context (no Staff persona). Returns `""` when nothing is set.
pub fn build_commonplace_llm_context(parts: &CommonplaceParts) -> String {
    let mut sections: Vec<String> = Vec::new();
    if let Some(s) = trimmed(&parts.current_state) {
        sections.push(format!("Here is the present state of the scene:\n\n{s}"));
    }
    if let Some(s) = trimmed(&parts.recap) {
        sections.push(format!(
            "You remember the gist of what has happened so far:\n\n{s}"
        ));
    }
    if let Some(s) = trimmed(&parts.relevant) {
        sections.push(format!(
            "You remember the following entries that bear on this moment:\n\n{s}"
        ));
    }
    if let Some(s) = trimmed(&parts.inter_char) {
        sections.push(format!("You also recall about the others present:\n\n{s}"));
    }
    if let Some(s) = trimmed(&parts.knowledge) {
        sections.push(format!(
            "You also recall the following from your knowledge base:\n\n{s}"
        ));
    }
    if let Some(s) = trimmed(&parts.relevant_conversations) {
        sections.push(format!(
            "You also recall these past conversations that bear on the present:\n\n{s}"
        ));
    }
    crate::jsstr::js_trim(&sections.join("\n\n")).to_string()
}

/// Parameters for [`post_commonplace_whisper`] (v4's `PostParams`).
#[derive(Clone, Debug)]
pub struct PostCommonplaceParams {
    pub chat_id: String,
    /// Participant id this whisper is targeted at; `None` = public.
    pub target_participant_id: Option<String>,
    /// Pre-formatted persona body text.
    pub content: String,
    /// Persona-free body swapped into opaque characters' LLM context. `None` for
    /// whispers stripped from LLM context anyway (the per-turn consolidated one).
    pub opaque_content: Option<String>,
    pub kind: CommonplaceWhisperKind,
}

/// The posted whisper (v4 returns the `MessageEvent`); callers need the minted
/// `id` (to sweep prior whispers) and the target set that was written.
#[derive(Clone, Debug)]
pub struct PostedCommonplaceMessage {
    /// The minted message id.
    pub id: String,
    /// The `targetParticipantIds` written (`None` for a public whisper).
    pub target_participant_ids: Option<Vec<String>>,
}

/// v4 `postCommonplaceWhisper` — persist a Commonplace-Book recall whisper as an
/// `ASSISTANT` message tagged `systemSender: 'commonplaceBook'` / `systemKind:
/// kind`, `participantId: null`, `opaqueContent: opaqueContent ?? null`, targeted
/// per `targetParticipantId`. Best-effort: an empty (`!content.trim()`) body, a
/// missing chat, or a persistence failure all return `None` (v4 logs + returns
/// null; never thrown).
pub async fn post_commonplace_whisper(
    db: &Db,
    params: PostCommonplaceParams,
) -> Option<PostedCommonplaceMessage> {
    if crate::jsstr::js_trim(&params.content).is_empty() {
        return None;
    }

    // v4 `repos.chats.findById(chatId)`: no chat → null (never posts).
    let chat_id_read = params.chat_id.clone();
    let chat = db
        .read_main(move |conn| crate::db::chats_read::find_by_id(conn, &chat_id_read))
        .ok()?;
    chat?;

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // v4: `targetParticipantId ? [targetParticipantId] : null`.
    let target_participant_ids: Option<Vec<String>> = params
        .target_participant_id
        .as_ref()
        .map(|t| vec![t.clone()]);

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": params.content,
        "opaqueContent": params.opaque_content,
        "attachments": [],
        "createdAt": now,
        "participantId": null,
        "systemSender": "commonplaceBook",
        "systemKind": params.kind.as_str(),
        "targetParticipantIds": target_participant_ids,
    });

    let event: ChatEventInput = serde_json::from_value(message).ok()?;
    let chat_id = params.chat_id.clone();
    let write_result = db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await;

    match write_result {
        Ok(()) => Some(PostedCommonplaceMessage {
            id: message_id,
            target_participant_ids,
        }),
        Err(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Fold-triggered relevant-conversations refresh.
// ---------------------------------------------------------------------------

/// v4 `RELEVANT_CONVERSATIONS_MIN`.
const RELEVANT_CONVERSATIONS_MIN: i64 = 3;
/// v4 `RELEVANT_CONVERSATIONS_MAX`.
const RELEVANT_CONVERSATIONS_MAX: i64 = 10;
/// v4 `RELEVANT_CONVERSATIONS_RAMP_MIN_TOKENS`.
const RELEVANT_CONVERSATIONS_RAMP_MIN_TOKENS: i64 = 4000;
/// v4 `RELEVANT_CONVERSATIONS_RAMP_MAX_TOKENS`.
const RELEVANT_CONVERSATIONS_RAMP_MAX_TOKENS: i64 = 32000;

/// v4 `RefreshRelevantConversationsInput`.
#[derive(Clone, Debug)]
pub struct RefreshRelevantConversationsInput {
    pub chat_id: String,
    /// The fresh fold summary — the query that drives the relevance search.
    pub summary: String,
    pub user_id: String,
    /// Embedding profile for the relevance search.
    pub embedding_profile_id: Option<String>,
    /// Connection-profile maxContext; scales the list size (3→10 over 4K→32K).
    pub max_context: Option<i64>,
}

/// A present, LLM-controlled participant with its resolved character mount.
struct PresentParticipant {
    participant_id: String,
    character_id: String,
}

/// v4 `refreshRelevantConversationsOnFold`. For each present, LLM-controlled
/// character in the chat, re-run the relevant-past-conversations search against
/// the fresh fold summary and post a refreshed `relevant-conversations` whisper,
/// sweeping the character's prior one. Best-effort — never throws into the fold.
pub async fn refresh_relevant_conversations_on_fold<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    input: RefreshRelevantConversationsInput,
) {
    let query = crate::jsstr::js_trim(&input.summary).to_string();
    if query.is_empty() {
        return;
    }

    // v4 `repos.chats.findById(chatId)`; missing chat → best-effort return.
    let chat_id_read = input.chat_id.clone();
    let chat =
        match db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &chat_id_read)) {
            Ok(Some(c)) => c,
            _ => return,
        };

    // Present, LLM-controlled participants carrying a character id (v4 filter:
    // `isParticipantPresent(status) && controlledBy !== 'user' && characterId`).
    let present: Vec<PresentParticipant> = chat
        .get("participants")
        .and_then(serde_json::Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter_map(|p| {
                    let status = crate::chat_predicates::is_participant_present(parse_status(
                        p.get("status").and_then(serde_json::Value::as_str),
                    ));
                    if !status {
                        return None;
                    }
                    if p.get("controlledBy").and_then(serde_json::Value::as_str) == Some("user") {
                        return None;
                    }
                    let character_id = p
                        .get("characterId")
                        .and_then(serde_json::Value::as_str)
                        .filter(|c| !c.is_empty())?;
                    let participant_id = p
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    Some(PresentParticipant {
                        participant_id: participant_id.to_string(),
                        character_id: character_id.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if present.is_empty() {
        return;
    }

    // v4 targeting: public (untargeted) in a single-present chat, per-participant
    // otherwise.
    let is_multi_character = present.len() > 1;
    let limit = ramp_limit(
        input.max_context,
        RELEVANT_CONVERSATIONS_MIN,
        RELEVANT_CONVERSATIONS_MAX,
        RELEVANT_CONVERSATIONS_RAMP_MIN_TOKENS,
        RELEVANT_CONVERSATIONS_RAMP_MAX_TOKENS,
    );

    for participant in &present {
        // Resolve the character's vault mount (v4's search resolves it internally
        // from `characterId`; the ported search takes it as an argument).
        let cid = participant.character_id.clone();
        let mount_point_id: Option<String> = db
            .read_main(move |conn| crate::db::characters_read::find_by_id_raw(conn, &cid))
            .ok()
            .flatten()
            .and_then(|c| {
                c.get("characterDocumentMountPointId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            });

        let matches = search_vault_conversation_summaries(
            db,
            embedding,
            &participant.character_id,
            mount_point_id.as_deref(),
            &query,
            &input.user_id,
            input.embedding_profile_id.as_deref(),
            limit.max(0) as usize,
            Some(&input.chat_id),
            // Round 2 has no production caller passing a time window (the
            // round-3 mini-recap is the consumer).
            None,
        )
        .await;
        if matches.is_empty() {
            continue;
        }

        let block = format!(
            "{}\n\n{}",
            render_relevant_conversations_block(&matches),
            READ_CONVERSATION_CALL_NOTE
        );
        let parts = CommonplaceParts {
            relevant_conversations: Some(block),
            ..Default::default()
        };
        let persona_content = build_commonplace_persona_whisper(&parts);
        let llm_content = build_commonplace_llm_context(&parts);
        if persona_content.is_empty() {
            continue;
        }

        let target_participant_id = if is_multi_character {
            Some(participant.participant_id.clone())
        } else {
            None
        };
        let posted = post_commonplace_whisper(
            db,
            PostCommonplaceParams {
                chat_id: input.chat_id.clone(),
                target_participant_id: target_participant_id.clone(),
                content: persona_content,
                opaque_content: Some(llm_content),
                kind: CommonplaceWhisperKind::RelevantConversations,
            },
        )
        .await;
        let Some(posted) = posted else {
            continue;
        };

        // Sweep the character's prior relevant-conversations whisper(s) so only
        // the freshest one survives.
        sweep_prior_relevant_conversation_whispers(
            db,
            &input.chat_id,
            &posted.id,
            target_participant_id.as_deref(),
        )
        .await;
    }
}

/// Parse a participant status string (default `"active"`) into the enum — mirrors
/// the shared `parse_status` used across the participant resolvers.
fn parse_status(s: Option<&str>) -> crate::chat_predicates::ParticipantStatus {
    use crate::chat_predicates::ParticipantStatus;
    match s.unwrap_or("active") {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "removed" => ParticipantStatus::Removed,
        _ => ParticipantStatus::Absent,
    }
}

/// v4 `sweepPriorRelevantConversationWhispers` — remove every prior
/// `relevant-conversations` whisper for the same target, leaving the just-posted
/// one (`keep_id`). Matched on the same target scope the consolidated sweep uses
/// (`None` → untargeted; otherwise the whisper's `targetParticipantIds` must
/// include the target id). Best-effort — a failure is swallowed.
async fn sweep_prior_relevant_conversation_whispers(
    db: &Db,
    chat_id: &str,
    keep_id: &str,
    target_participant_id: Option<&str>,
) {
    let chat_id_read = chat_id.to_string();
    let messages = match db
        .read_main(move |conn| crate::db::chats_messages_read::get_messages(conn, &chat_id_read))
    {
        Ok(m) => m,
        Err(_) => return,
    };

    let stale: Vec<String> = messages
        .iter()
        .filter(|m| m.get("type").and_then(serde_json::Value::as_str) == Some("message"))
        .filter(|m| {
            m.get("systemSender").and_then(serde_json::Value::as_str) == Some("commonplaceBook")
                && m.get("systemKind").and_then(serde_json::Value::as_str)
                    == Some("relevant-conversations")
                && m.get("id").and_then(serde_json::Value::as_str) != Some(keep_id)
        })
        .filter(|m| {
            let ids = m.get("targetParticipantIds");
            match target_participant_id {
                // Untargeted scope: the whisper carries a null / absent target set.
                None => ids.is_none() || ids == Some(&serde_json::Value::Null),
                // Targeted scope: the array must include the target id.
                Some(target) => ids
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(target))),
            }
        })
        .filter_map(|m| {
            m.get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .collect();

    if stale.is_empty() {
        return;
    }
    let chat_id_owned = chat_id.to_string();
    let _ = db
        .write(move |writers| {
            writers
                .main()
                .chat_messages()
                .delete_messages_by_ids(&chat_id_owned, &stale)
        })
        .await;
}
