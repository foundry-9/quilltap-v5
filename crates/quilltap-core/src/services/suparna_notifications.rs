//! Writer for Suparṇā's Post Office mail whispers — v4
//! `lib/services/suparna-notifications/writer.ts` (the persona-voiced whisper +
//! post) and `lib/post-office/surface-operator-mail.ts`.
//!
//! Suparṇā is the personified mail carrier. When a character is about to take a
//! turn, the Post Office checks that character's mailbox; any letters not yet
//! announced trigger a whisper that reads each new letter aloud, names the sender
//! and date, and reminds the character how to read/answer/discard it.
//!
//! Unlike the Commonplace Book whisper (a per-turn snapshot that sweeps its own
//! prior whispers), this is EVENT-like: each new-mail announcement is a distinct
//! event and must not sweep earlier ones.
//!
//! Suparṇā is openly visible to characters (NOT opaque): `opaqueContent` is set
//! equal to `content` so the opaque-character swap is a no-op and the recipient
//! reads her announcement — and the real sender name — verbatim.
//! `systemSender: 'suparna'` / `systemKind: 'mail-delivery'`, targeted.
//!
//! Errors never propagate — a mail-check failure must never break the turn.
//!
//! The plain-second-person LLM-context builder (`buildSuparnaMailLLMContext`) is
//! the READ feeder ported in [`crate::services::suparna_mail`] (W4.6a); this
//! module is the persona whisper + the operator-mail surface. Reuses
//! [`crate::post_office::mailbox`] (`collect_unalerted_mail` / `mark_alerted`),
//! [`crate::post_office::instructions`] (`format_letter_actions` /
//! `format_letter_date`), and [`crate::db::characters_read::find_by_id_raw`].

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::jsstr::js_trim;
use crate::post_office::instructions::{format_letter_actions, format_letter_date};
use crate::post_office::mailbox::{collect_unalerted_mail, mark_alerted, DeliveredLetterSummary};

/// Quote a letter body as a Markdown blockquote so Suparṇā "reads it aloud"
/// (v4 `quoteBody`). Blank → the italic placeholder.
fn quote_body(body: &str) -> String {
    let trimmed = js_trim(body);
    if trimmed.is_empty() {
        return "> *(the letter is blank)*".to_string();
    }
    trimmed
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Persona-voiced whisper for the Salon transcript / UI (and `opaqueContent`).
/// Reads each new letter aloud and appends the read/answer/discard reminders
/// (v4 `buildSuparnaMailWhisper`). `""` for an empty list.
pub fn build_suparna_mail_whisper(letters: &[DeliveredLetterSummary]) -> String {
    if letters.is_empty() {
        return String::new();
    }
    let count = letters.len();
    let opener = if count == 1 {
        "*Suparṇā glides in from the Post Office, a single letter held out for you.*".to_string()
    } else {
        format!(
            "*Suparṇā glides in from the Post Office with an armful of {count} letters for you.*"
        )
    };
    let parts = letters
        .iter()
        .map(|letter| {
            let head = format!(
                "**A letter from {}**, posted {}:",
                letter.from,
                format_letter_date(&letter.sent_at)
            );
            format!(
                "{head}\n\n{}\n\n{}",
                quote_body(&letter.body),
                format_letter_actions(&letter.path, &letter.from)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    format!("{opener}\n\n{parts}")
}

/// Parameters for [`post_suparna_mail_whisper`] (v4 `PostSuparnaMailWhisperParams`).
#[derive(Debug, Clone)]
pub struct PostSuparnaMailWhisperParams {
    pub chat_id: String,
    /// Participant id this whisper is targeted at (multi-character chats), else
    /// `None`.
    pub target_participant_id: Option<String>,
    /// Pre-built persona-voiced body (from [`build_suparna_mail_whisper`]).
    pub content: String,
}

/// Persist a Suparṇā mail whisper (v4 `postSuparnaMailWhisper`). A blank body →
/// `None` (no whisper). Reads the chat (missing → `None`), mints `id` +
/// `createdAt`, and inserts the `MessageEvent` (ASSISTANT role,
/// `participantId: null`, `systemSender: 'suparna'` / `systemKind: 'mail-delivery'`,
/// `targetParticipantIds` when targeted, non-opaque `opaqueContent = content`).
/// Returns the posted message or `None` on failure (v4 catches + returns null).
pub async fn post_suparna_mail_whisper(
    db: &Db,
    params: PostSuparnaMailWhisperParams,
) -> Option<Value> {
    // v4: `if (!content || content.trim().length === 0) return null;`
    if params.content.is_empty() || js_trim(&params.content).is_empty() {
        return None;
    }

    let cid = params.chat_id.clone();
    let exists = db
        .read_main(move |conn| crate::db::chats_read::find_by_id(conn, &cid))
        .ok()?
        .is_some();
    if !exists {
        return None;
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    let target_participant_ids: Value = match &params.target_participant_id {
        Some(id) => json!([id]),
        None => Value::Null,
    };

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": params.content,
        "attachments": [],
        "createdAt": now,
        "participantId": Value::Null,
        "systemSender": "suparna",
        "systemKind": "mail-delivery",
        "targetParticipantIds": target_participant_ids,
        // Non-opaque: see module header.
        "opaqueContent": params.content,
    });

    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    let cid = params.chat_id.clone();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&cid, &event))
        .await
    {
        Ok(()) => Some(message),
        Err(_) => None,
    }
}

/// Surface Post Office letters addressed to the OPERATOR's own character(s)
/// (v4 `surfaceOperatorMailForChat`).
///
/// The per-turn mail check only runs for the character whose turn it is (an
/// LLM-controlled participant). A user-controlled participant never takes an LLM
/// turn, so a letter to the operator's character would otherwise sit unannounced
/// forever. This sweeps every user-controlled CHARACTER participant's vault for
/// unannounced mail and posts a Suparṇā whisper TARGETED at that participant's id
/// (the Salon shows a whisper targeting a user-controlled participant to the
/// operator while keeping it private from the other characters). It injects NO
/// LLM context: the operator's character makes no model call.
///
/// `markAlerted` runs AFTER the post (the ordering invariant), so the two
/// triggers — and repeated loads — never double-announce. Warn-only: a mail
/// failure never breaks chat load or a turn (each participant is best-effort).
///
/// `participants` are the chat's participant objects (JSON). Returns the whispers
/// actually posted (one per character with fresh mail).
pub async fn surface_operator_mail_for_chat(
    db: &Db,
    chat_id: &str,
    participants: &[Value],
) -> Vec<Value> {
    let mut posted: Vec<Value> = Vec::new();

    // v4: `p.type === 'CHARACTER' && p.controlledBy === 'user' && !p.removedAt`.
    let operator_participants: Vec<&Value> = participants
        .iter()
        .filter(|p| {
            let is_char = p.get("type").and_then(Value::as_str) == Some("CHARACTER");
            let is_user = p.get("controlledBy").and_then(Value::as_str) == Some("user");
            // `!p.removedAt`: a non-empty `removedAt` string means removed.
            let removed = p
                .get("removedAt")
                .and_then(Value::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            is_char && is_user && !removed
        })
        .collect();
    if operator_participants.is_empty() {
        return posted;
    }

    for participant in operator_participants {
        let Some(participant_id) = participant.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(character_id) = participant.get("characterId").and_then(Value::as_str) else {
            continue;
        };

        // Existence-only raw read: a hollow-but-present vault must not fail the
        // chat load; only the mount-point id is needed.
        let char_id = character_id.to_string();
        let character = match db
            .read_main(move |conn| crate::db::characters_read::find_by_id_raw(conn, &char_id))
        {
            Ok(c) => c,
            Err(_) => continue,
        };
        let vault_id = character
            .as_ref()
            .and_then(|c| c.get("characterDocumentMountPointId"))
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let Some(vault_id) = vault_id else { continue };

        let vid = vault_id.clone();
        let unalerted = match db.read_mount_index(move |conn| collect_unalerted_mail(conn, &vid)) {
            Ok(u) => u,
            Err(_) => continue,
        };
        if unalerted.is_empty() {
            continue;
        }

        let content = build_suparna_mail_whisper(&unalerted);
        let message = post_suparna_mail_whisper(
            db,
            PostSuparnaMailWhisperParams {
                chat_id: chat_id.to_string(),
                target_participant_id: Some(participant_id.to_string()),
                content,
            },
        )
        .await;
        if let Some(message) = message {
            posted.push(message);
        }

        // Flip `alerted` AFTER the post (the ordering invariant).
        for letter in &unalerted {
            let vid = vault_id.clone();
            let path = letter.path.clone();
            let _ = db
                .write(move |writers| match writers.mount_index() {
                    Some(mount_w) => mark_alerted(mount_w.connection(), &vid, &path),
                    None => Ok(()),
                })
                .await;
        }
    }

    posted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letter(from: &str, sent_at: &str, body: &str, path: &str) -> DeliveredLetterSummary {
        DeliveredLetterSummary {
            path: path.to_string(),
            from: from.to_string(),
            sent_at: sent_at.to_string(),
            body: body.to_string(),
            alerted: false,
            in_reply_to: None,
        }
    }

    #[test]
    fn empty_letters_yield_empty_whisper() {
        assert_eq!(build_suparna_mail_whisper(&[]), "");
    }

    #[test]
    fn quote_body_blank_placeholder() {
        assert_eq!(quote_body("   "), "> *(the letter is blank)*");
        assert_eq!(quote_body("a\n\nb"), "> a\n>\n> b");
    }

    #[test]
    fn single_letter_whisper_opener_and_sections() {
        let l = letter(
            "Friday",
            "2026-02-01T09:05:00.000Z",
            "  Hello there.  ",
            "Mail/friday-2026-02-01.md",
        );
        let out = build_suparna_mail_whisper(std::slice::from_ref(&l));
        assert!(out.starts_with(
            "*Suparṇā glides in from the Post Office, a single letter held out for you.*"
        ));
        assert!(out.contains("**A letter from Friday**, posted "));
        assert!(out.contains("> Hello there."));
        assert!(out.contains("Read it again: doc_read_file"));
    }

    #[test]
    fn plural_letters_armful_opener_and_separator() {
        let l1 = letter("Ada", "2026-02-02T09:00:00.000Z", "First.", "Mail/a.md");
        let l2 = letter("Bob", "2026-02-01T09:00:00.000Z", "   ", "Mail/b.md");
        let out = build_suparna_mail_whisper(&[l1, l2]);
        assert!(out.contains("with an armful of 2 letters for you."));
        assert!(out.contains("\n\n---\n\n"));
        assert!(out.contains("> *(the letter is blank)*"));
    }
}
