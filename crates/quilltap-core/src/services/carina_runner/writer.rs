//! Writer for Carina (inline LLM query) reference answers — v4
//! `lib/services/carina/writer.ts` (`postCarinaResponse`).
//!
//! A Carina answer is posted as an ASSISTANT message tagged
//! `systemSender: 'carina'` / `systemKind: 'carina-response'`. The `systemSender`
//! tag keeps it out of per-turn memory extraction (see `buildTurnTranscript`,
//! which skips any `systemSender` message), and the `carinaMeta.answererId` lets
//! the Salon render the message with the answerer character's OWN avatar — there
//! is no dedicated Carina staff avatar.
//!
//! Errors never propagate — a failure to post the answer is logged (host-side
//! logging is out of scope for the port) and surfaced to the caller as `None`
//! rather than thrown. The message is built as a JSON object literal (matching
//! v4's `MessageEvent` object) and validated into a [`ChatEventInput`], so the
//! stored row is byte-for-byte the `db::chats_messages` insert marshaling.

use serde_json::json;

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;

/// Parameters for [`post_carina_response`] — the v4 `PostCarinaResponseParams`.
#[derive(Debug, Clone)]
pub struct PostCarinaResponseParams {
    pub chat_id: String,
    /// The answerer's reply text.
    pub answer: String,
    /// Resolved answerer character id (drives avatar resolution + continuity).
    pub answerer_id: String,
    /// The verbatim question that was asked (stored for follow-up continuity).
    pub question: String,
    /// Answerer's participant id when they are a participant in this chat; else `None`.
    pub participant_id: Option<String>,
    /// True when the original query used `?` (whisper to the asker only).
    pub whisper: bool,
    /// Participant id of the asker — the whisper target when `whisper` is true.
    pub asker_participant_id: Option<String>,
}

/// The persisted Carina message, returned so callers can splice it into the
/// current turn's in-memory context. Carries the minted `id`, the message as a
/// `ChatEventInput` value (for re-emission / diffing), and the whisper target
/// set that was written.
#[derive(Debug, Clone)]
pub struct PostedCarinaMessage {
    /// The minted message id.
    pub id: String,
    /// The full `MessageEvent` object literal that was validated + inserted.
    pub message: serde_json::Value,
    /// The `targetParticipantIds` that were written (`None` for a public answer).
    pub target_participant_ids: Option<Vec<String>>,
}

/// Build the Carina `MessageEvent` — mint `id` ([`uuid::Uuid::new_v4`]) + a
/// `createdAt` ([`crate::clock::now_iso`]) (matching v4's `randomUUID()` +
/// `new Date().toISOString()`), compute the whisper target set, and assemble the
/// exact v4 object literal, validated into the typed [`ChatEventInput`] (the same
/// shape v4's `ChatEventSchema.parse` produces before insert). Returns `None` if
/// the assembled object fails to validate (v4's post would then throw → caught →
/// null). This is the shared marshaling both [`post_carina_response`] and the
/// differential harness's Rust-side query seam call, so the stored-row bytes live
/// in exactly one place.
pub fn build_carina_message(
    params: &PostCarinaResponseParams,
) -> Option<(ChatEventInput, PostedCarinaMessage)> {
    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // v4: `params.whisper && params.askerParticipantId ? [askerParticipantId] : null`
    let target_participant_ids: Option<Vec<String>> =
        match (params.whisper, &params.asker_participant_id) {
            (true, Some(asker)) => Some(vec![asker.clone()]),
            _ => None,
        };

    // The answer carries no Carina-persona framing, so the opaque body is the
    // same text (opaque characters see the reference answer verbatim).
    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": params.answer,
        "opaqueContent": params.answer,
        "attachments": [],
        "createdAt": now,
        "participantId": params.participant_id,
        "systemSender": "carina",
        "systemKind": "carina-response",
        "targetParticipantIds": target_participant_ids,
        "carinaMeta": { "answererId": params.answerer_id, "question": params.question },
    });

    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    Some((
        event,
        PostedCarinaMessage {
            id: message_id,
            message,
            target_participant_ids,
        },
    ))
}

/// Persist a Carina reference answer (v4 `postCarinaResponse`). Returns the
/// posted message (so callers can splice it into the current turn's in-memory
/// context), or `None` on failure (v4 logs + returns null; never thrown).
pub async fn post_carina_response(
    db: &Db,
    params: PostCarinaResponseParams,
) -> Option<PostedCarinaMessage> {
    let (event, posted) = build_carina_message(&params)?;
    let chat_id = params.chat_id.clone();
    let write_result = db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await;

    match write_result {
        Ok(()) => Some(posted),
        // v4 catches, logs, and returns null on a persistence failure.
        Err(_) => None,
    }
}
