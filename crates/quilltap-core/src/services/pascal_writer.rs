//! Writer for Pascal the Croupier's custom-tool outcomes — v4
//! `lib/services/pascal/writer.ts` (`buildPascalResultContent` +
//! `postPascalResult`).
//!
//! When a custom tool is run — by a character via `run_custom`, or by the
//! operator from the manual popup — Pascal announces the outcome as a synthetic
//! ASSISTANT message tagged `systemSender: 'pascal'` /
//! `systemKind: 'custom-tool-result'`. The `pascalMeta` payload preserves the
//! whole provenance of the deal so the table can be audited long after the felt
//! is cleared.
//!
//! **Pascal only ever announces genuine outcomes.** A run that fails — an unknown
//! tool, a rejected parameter, a definition that would not load — is reported by
//! Prospero (`systemKind: 'custom-tool-error'`), never by Pascal. A Pascal
//! message in the transcript therefore always means the dice truly fell.
//!
//! **The croupier does not narrate** (v4 `a33ac8b8`'s deliberate removals). The
//! body is the tool's own title and the author's own outcome message, nothing
//! else — Pascal's "At Charlie's behest…" flourish and the italic `*(rolled 14)*`
//! suffix are both gone. What a roll says is the author's to decide with
//! `{{value}}` / `{{dice}}`; the whole roll record lives in `pascalMeta` anyway.
//!
//! Opacity: `opaque_content == content` here. The dual body exists to keep Staff
//! NAMES out of an opaque character's context, and with the flourishes gone there
//! is no persona left to strip — the same position Suparṇā and Carina are in.
//!
//! Errors never propagate — a failure to announce is surfaced to the caller as
//! `None` rather than thrown (v4 logs it host-side).

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::jsstr::js_trim;

/// The dual body of an outcome announcement (v4 `buildPascalResultContent`'s
/// return). Both strings are the same — see the note on opacity in the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PascalResultContent {
    pub content: String,
    pub opaque_content: String,
}

/// Build the body of an outcome announcement: the tool's title, and the author's
/// own message. Deliberately knows nothing about the roll — a table that wants
/// its number read out says so with `{{value}}`.
pub fn build_pascal_result_content(tool_title: &str, message: &str) -> PascalResultContent {
    let body = format!("🎲 **{tool_title}** — {}", js_trim(message));
    PascalResultContent {
        content: body.clone(),
        opaque_content: body,
    }
}

/// Parameters for [`post_pascal_result`] — v4's `PostPascalResultParams`.
pub struct PostPascalResultParams {
    pub chat_id: String,
    /// Persona-voiced body (from [`build_pascal_result_content`]).
    pub content: String,
    /// Plain body swapped in for opaque characters (identical to `content`).
    pub opaque_content: String,
    /// The provenance of the run — the assembled `pascalMeta` object. Validated
    /// into the typed row on the way in; a malformed one fails the whole post.
    pub pascal_meta: Value,
    /// When set and non-empty, the outcome is whispered only to these
    /// participants (a private run). `None`/empty = the whole room sees it.
    pub target_participant_ids: Option<Vec<String>>,
}

/// The persisted Pascal message, returned so callers can splice it into the
/// current turn's in-memory context and surface it over SSE.
#[derive(Debug, Clone)]
pub struct PostedPascalMessage {
    pub id: String,
    /// The full `MessageEvent` object literal that was validated + inserted.
    pub message: Value,
    /// The `targetParticipantIds` that were written (`None` for a public roll).
    pub target_participant_ids: Option<Vec<String>>,
}

/// Assemble the Pascal `MessageEvent` — mint `id` + `createdAt`, normalize the
/// whisper target (`[]` → null), and validate into the typed [`ChatEventInput`]
/// (the same shape `db::chats_messages` inserts). `None` if the assembled object
/// fails to validate (v4's post would then throw → caught → null).
pub fn build_pascal_message(
    params: &PostPascalResultParams,
) -> Option<(ChatEventInput, PostedPascalMessage)> {
    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // v4: `targetParticipantIds && targetParticipantIds.length ? … : null`.
    let targets: Option<Vec<String>> = params
        .target_participant_ids
        .clone()
        .filter(|t| !t.is_empty());

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": params.content,
        "opaqueContent": params.opaque_content,
        "attachments": [],
        "createdAt": now,
        "participantId": null,
        "systemSender": "pascal",
        "systemKind": "custom-tool-result",
        "targetParticipantIds": targets,
        "pascalMeta": params.pascal_meta,
    });

    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    Some((
        event,
        PostedPascalMessage {
            id: message_id,
            message,
            target_participant_ids: targets,
        },
    ))
}

/// Persist a Pascal outcome announcement (v4 `postPascalResult`). Guards on the
/// chat existing, then writes best-effort — `Err` → `None`, matching v4's
/// catch-and-return-null. Returns the posted message on success.
pub async fn post_pascal_result(
    db: &Db,
    params: PostPascalResultParams,
) -> Option<PostedPascalMessage> {
    // v4: `const chat = await repos.chats.findById(chatId); if (!chat) return null;`
    let cid = params.chat_id.clone();
    match db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        _ => return None,
    }

    let (event, posted) = build_pascal_message(&params)?;
    let chat_id = params.chat_id.clone();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await
    {
        Ok(()) => Some(posted),
        Err(_) => None,
    }
}
