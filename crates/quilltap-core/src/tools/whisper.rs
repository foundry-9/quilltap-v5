//! Whisper tool handler — port of v4
//! `lib/tools/handlers/whisper-handler.ts` (+ `lib/tools/whisper-tool.ts`'s
//! `validateWhisperInput` Zod guard and `formatWhisperResults`).
//!
//! An LLM character sends a private message to another character in a
//! multi-character chat. The handler:
//!   1. validates the `{ target, message }` input (both non-empty strings),
//!   2. loads the chat (owned by `user_id`),
//!   3. resolves `target` to a whisper-receivable participant by character name,
//!      then alias (case-insensitive) — erroring on zero (not-found, with an
//!      "Available characters" list) or multiple (ambiguous) matches,
//!   4. rejects a self-whisper,
//!   5. sanitizes the message (strips any double-encoded `[[...]]` markers),
//!   6. writes a single ASSISTANT `chat_messages` row whose `participantId` is
//!      the caller and `targetParticipantIds` is `[target]`.
//!
//! ## STOP-rule finding
//!
//! The whisper write is **only** a `chat_messages` row (the ported
//! `chats_messages::add_message` path). v4's `executeWhisperTool` does exactly one
//! `repos.chats.addMessage(...)` and a `logger.info` — no post-office / SSE /
//! broadcast side effect beyond the message row. So this is a plain tier-2 write.
//!
//! The dispatcher's "requires a multi-character chat context" guard lives in
//! `tool-executor.ts` (it returns early when `callingParticipantId` is absent), so
//! this handler assumes `calling_participant_id` is set — matching v4.
//!
//! ## `canReceiveWhisper`
//!
//! v4 `lib/schemas/chat.types.ts`: a participant can receive a whisper iff its
//! status is `active` or `silent` (the same predicate as `isParticipantPresent`).
//! `absent`/`removed` cannot. The read overlay materializes each participant's
//! `status` default (`'active'`).

use serde::Serialize;
use serde_json::Value;
use std::sync::LazyLock;

use regex::Regex;

use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, DbError};
use crate::tools::text_block_parser::strip_text_block_markers;

/// v4 `WhisperToolOutput`. Fields rename to v4 camelCase; optional fields are
/// omitted when absent (v4's `undefined`, dropped by `JSON.stringify`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WhisperOutput {
    pub success: bool,
    /// Name of the character the whisper was sent to.
    #[serde(rename = "targetName", skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
    /// Participant id of the target.
    #[serde(
        rename = "targetParticipantId",
        skip_serializing_if = "Option::is_none"
    )]
    pub target_participant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl WhisperOutput {
    fn failure(error: impl Into<String>) -> Self {
        WhisperOutput {
            success: false,
            target_name: None,
            target_participant_id: None,
            error: Some(error.into()),
        }
    }
}

// v4 `sanitizeWhisperMessage`'s fallback extractor:
//   /\[\[\w+[^\]]*\]\]([\s\S]*?)\[\[\/\w+\]\]/i
// JS `\w` = ASCII `[A-Za-z0-9_]`; `[\s\S]` matches anything incl. newlines; the
// `?` after `*` makes the content capture non-greedy (nearest close wins). The
// `regex` crate is Unicode by default, so `\w` is forced ASCII with `(?-u:\w)`
// and the flag set carries the `i` (case-insensitive tag name).
static WHISPER_FALLBACK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)\[\[(?-u:\w)+[^\]]*\]\]([\s\S]*?)\[\[/(?-u:\w)+\]\]").unwrap()
});

/// v4 `sanitizeWhisperMessage`. LLMs sometimes double-encode a tool call — calling
/// the native `whisper` tool but ALSO wrapping the message in
/// `[[WHISPER ...]]content[[/WHISPER]]` markers. Strip those so the saved content
/// is clean. If stripping leaves nothing but the original was non-empty, fall back
/// to extracting just the inner content; finally fall back to the trimmed original.
pub fn sanitize_whisper_message(message: &str) -> String {
    let mut cleaned = strip_text_block_markers(message);

    // If stripping left us with nothing (the whole message was a marker), try
    // extracting just the content from inside the markers.
    if cleaned.trim().is_empty() && !message.trim().is_empty() {
        if let Some(caps) = WHISPER_FALLBACK_RE.captures(message) {
            if let Some(inner) = caps.get(1) {
                cleaned = inner.as_str().trim().to_string();
            }
        }
    }

    let trimmed = cleaned.trim();
    if !trimmed.is_empty() {
        trimmed.to_string()
    } else {
        message.trim().to_string()
    }
}

/// v4 `canReceiveWhisper` — a participant can receive a whisper iff present
/// (`active` or `silent`).
fn can_receive_whisper(status: &str) -> bool {
    status == "active" || status == "silent"
}

/// v4 `validateWhisperInput` — `z.object({ target: string().min(1), message:
/// string().min(1) })`. Returns `(target, message)` on success; `None` on any
/// deviation (missing key, non-string, or empty string). Extra keys are ignored
/// (Zod strips unknown keys on an object schema by default).
fn validate_whisper_input(args: &Value) -> Option<(String, String)> {
    let obj = args.as_object()?;
    let target = obj.get("target").and_then(Value::as_str)?;
    let message = obj.get("message").and_then(Value::as_str)?;
    if target.is_empty() || message.is_empty() {
        return None;
    }
    Some((target.to_string(), message.to_string()))
}

/// A matched whisper target.
struct WhisperMatch {
    participant_id: String,
    character_name: String,
}

/// v4 `executeWhisperTool`. See the module doc for the flow. Never returns a Rust
/// error — every failure is an `error`-bearing [`WhisperOutput`] (matching v4's
/// catch-all try/catch that funnels everything into `{ success: false, error }`).
pub async fn execute_whisper(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    calling_participant_id: &str,
    args: &Value,
) -> WhisperOutput {
    match execute_whisper_inner(db, user_id, chat_id, calling_participant_id, args).await {
        Ok(out) => out,
        // v4 `catch (error)` → `{ success: false, error: error.message }`.
        Err(e) => WhisperOutput::failure(e.to_string()),
    }
}

async fn execute_whisper_inner(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    calling_participant_id: &str,
    args: &Value,
) -> Result<WhisperOutput, DbError> {
    // 1. Validate input.
    let (target, message) = match validate_whisper_input(args) {
        Some(v) => v,
        None => {
            return Ok(WhisperOutput::failure(
                "Invalid input: both \"target\" (character name) and \"message\" \
                 (text content) are required.",
            ));
        }
    };

    // 2. Load the chat and confirm ownership.
    let chat = db.read_main(|c| chats_read::find_by_id(c, chat_id))?;
    let chat = match chat {
        Some(c) if c.get("userId").and_then(Value::as_str) == Some(user_id) => c,
        _ => return Ok(WhisperOutput::failure("Chat not found")),
    };

    // 3. Resolve the target among whisper-receivable participants.
    //    v4 iterates `chat.participants.filter(canReceiveWhisper(status))`, matching
    //    by character name first then aliases (case-insensitive), collecting every
    //    match. We collect the active participants' `(participantId, characterId)`
    //    first, then read each character (needs both connections for the vault
    //    overlay — `aliases` is a vault-managed field).
    let target_lower = target.to_lowercase();

    let mut active: Vec<(String, String)> = Vec::new(); // (participantId, characterId)
    if let Some(ps) = chat.get("participants").and_then(Value::as_array) {
        for p in ps {
            let status = p.get("status").and_then(Value::as_str).unwrap_or("active");
            if !can_receive_whisper(status) {
                continue;
            }
            // v4 skips participants without a characterId.
            if let Some(cid) = p.get("characterId").and_then(Value::as_str) {
                if let Some(pid) = p.get("id").and_then(Value::as_str) {
                    active.push((pid.to_string(), cid.to_string()));
                }
            }
        }
    }

    let mut matches: Vec<WhisperMatch> = Vec::new();
    // Available names, for the not-found error message (v4 rebuilds this in a
    // second pass; we capture names in the same order here).
    let mut available_names: Vec<String> = Vec::new();

    for (participant_id, character_id) in &active {
        let character = db.read_main(|main| {
            db.read_mount_index(|mount| characters_read::find_by_id(main, mount, character_id))
        })?;
        let character = match character {
            Some(c) => c,
            None => continue,
        };

        let name = character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        available_names.push(name.clone());

        // Exact name match (case-insensitive).
        if name.to_lowercase() == target_lower {
            matches.push(WhisperMatch {
                participant_id: participant_id.clone(),
                character_name: name,
            });
            continue;
        }

        // Alias match (case-insensitive).
        if let Some(aliases) = character.get("aliases").and_then(Value::as_array) {
            for alias in aliases {
                if let Some(a) = alias.as_str() {
                    if a.to_lowercase() == target_lower {
                        matches.push(WhisperMatch {
                            participant_id: participant_id.clone(),
                            character_name: name,
                        });
                        break;
                    }
                }
            }
        }
    }

    if matches.is_empty() {
        return Ok(WhisperOutput::failure(format!(
            "No character found matching \"{}\". Available characters: {}",
            target,
            available_names.join(", ")
        )));
    }

    if matches.len() > 1 {
        let names: Vec<&str> = matches.iter().map(|m| m.character_name.as_str()).collect();
        return Ok(WhisperOutput::failure(format!(
            "Ambiguous target \"{}\" matches multiple characters: {}. Please be \
             more specific.",
            target,
            names.join(", ")
        )));
    }

    let resolved = &matches[0];

    // 4. Can't whisper to self.
    if resolved.participant_id == calling_participant_id {
        return Ok(WhisperOutput::failure("You cannot whisper to yourself."));
    }

    // 5. Sanitize the message.
    let clean_message = sanitize_whisper_message(&message);

    // 6. Write the whisper as a new ASSISTANT message with targetParticipantIds.
    let whisper_message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    let target_participant_id = resolved.participant_id.clone();
    let calling = calling_participant_id.to_string();
    let chat = chat_id.to_string();
    let content = clean_message;
    let created_at = now;
    let msg_id = whisper_message_id;

    db.write(move |writers| {
        let event = crate::db::chats_messages::ChatEventInput::Message(Box::new(
            crate::db::chats_messages::MessageEventInput {
                id: msg_id,
                role: "ASSISTANT".to_string(),
                content,
                participant_id: Some(calling),
                target_participant_ids: Some(vec![target_participant_id]),
                created_at,
                attachments: Vec::new(),
                // All other MessageEvent fields default to None/empty (v4 passes
                // only type/id/role/content/participantId/targetParticipantIds/
                // createdAt/attachments).
                raw_response: None,
                token_count: None,
                prompt_tokens: None,
                completion_tokens: None,
                swipe_group_id: None,
                swipe_index: None,
                debug_memory_logs: None,
                thought_signature: None,
                reasoning_content: None,
                reasoning_segments: None,
                recovery_type: None,
                rendered_html: None,
                danger_flags: None,
                provider: None,
                model_name: None,
                confirmed: None,
                confirmation_checked: None,
                confirmation_revised: None,
                confirmation_notes: None,
                confirmation_original_content: None,
                is_silent_message: None,
                system_sender: None,
                system_kind: None,
                opaque_content: None,
                host_event: None,
                custom_announcer: None,
                carina_meta: None,
                pascal_meta: None,
                pending_external_prompt: None,
                pending_external_prompt_full: None,
                pending_external_attachments: None,
                summary_anchor: None,
            },
        ));
        writers.main().chat_messages().add_message(&chat, &event)
    })
    .await?;

    Ok(WhisperOutput {
        success: true,
        target_name: Some(resolved.character_name.clone()),
        target_participant_id: Some(resolved.participant_id.clone()),
        error: None,
    })
}

/// v4 `formatWhisperResults` — the string threaded into conversation context.
pub fn format_whisper(out: &WhisperOutput) -> String {
    if !out.success {
        return format!(
            "Whisper Error: {}",
            out.error.as_deref().unwrap_or("Unknown error")
        );
    }
    format!(
        "Whispered to {}: Message sent privately.",
        out.target_name.as_deref().unwrap_or("")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_plain_message_unchanged() {
        assert_eq!(sanitize_whisper_message("Hello there"), "Hello there");
    }

    #[test]
    fn sanitize_trims_whitespace() {
        assert_eq!(sanitize_whisper_message("  hi  "), "hi");
    }

    #[test]
    fn sanitize_strips_double_encoded_markers() {
        // The entire message is a marker → strip leaves nothing → fallback
        // extractor pulls the inner content.
        let input = "[[WHISPER to=\"Alice\"]]meet me at dawn[[/WHISPER]]";
        assert_eq!(sanitize_whisper_message(input), "meet me at dawn");
    }

    #[test]
    fn sanitize_fallback_case_insensitive_tag() {
        let input = "[[whisper to=\"Bob\"]]the secret[[/whisper]]";
        assert_eq!(sanitize_whisper_message(input), "the secret");
    }

    #[test]
    fn sanitize_fallback_multiline_content() {
        // `[\s\S]*?` must match across newlines (non-greedy, nearest close).
        let input = "[[WHISPER]]line one\nline two[[/WHISPER]]";
        assert_eq!(sanitize_whisper_message(input), "line one\nline two");
    }

    #[test]
    fn sanitize_empty_stays_empty() {
        assert_eq!(sanitize_whisper_message("   "), "");
    }
}
