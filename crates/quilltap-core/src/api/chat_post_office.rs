//! The in-chat Post Office / Announcer dispatch handlers (P4.9E2A) — a
//! differential port of v4's four chat actions, each composed over already-ported
//! pieces (never re-implemented):
//!
//!   - `chatAnnouncementPost` — v4 `actions/announcement.ts`
//!     (`handleInsertAnnouncement`), over
//!     [`announcer::writer::post_adhoc_announcement`].
//!   - `chatAnnouncementPreview` — v4 `actions/announcement-preview.ts`
//!     (`handleAnnouncementPreview`), over
//!     [`announcer::character_voiced::generate_character_voiced_announcement`]
//!     behind the host [`AnnouncementPreviewDriver`] seam (the completion +
//!     embedding providers live in the composing host — the recall-replay /
//!     courier-resolve precedent).
//!   - `chatSendMail` — v4 `actions/send-mail.ts` (`handleSendMail`), over the
//!     already-ported Post Office [`compose_and_deliver_letter`].
//!   - `chatMailboxList` — v4 `actions/mailbox.ts` (`handleGetMailbox`), over
//!     [`ensure_character_vault`] + [`list_mailbox`].
//!
//! ## The three server-side authorization arms (ported exactly)
//!
//! 1. **send-mail** re-verifies `fromCharacterId` against the participant list —
//!    "never trust the client to send as an LLM character or a stranger". The
//!    rejection is a **400**.
//! 2. **mailbox** authorizes the same way but rejects with **403**
//!    (`send-mail.ts:39` vs `mailbox.ts:36`). The status difference is real; the
//!    two are deliberately NOT unified.
//! 3. **announcement** verifies only the `character` branch's referenced
//!    character exists → `notFound('Character')`; `staff` and `custom` senders
//!    are unchecked.
//!
//! ## Two v4 read quirks a faithful port reproduces
//!
//! - `send-mail` and `mailbox` read the sender/recipient with **`findByIdRaw`**,
//!   not `findById` — v4 `send-mail.ts:42`: "Use raw rows (existence-only) so a
//!   hollow-but-present vault doesn't 503 here; delivery re-provisions the vault
//!   idempotently anyway." `announcement` / `announcement-preview` use the
//!   overlay `findById`. Each call site is matched.
//! - `mailbox` calls `ensureCharacterVault(character)` before listing, so
//!   **reading a mailbox can provision (or adopt) a vault** — a write on a GET
//!   path. That is v4's behavior and the differential diffs the resulting rows.
//!
//! Pinned by `post_office_routes_equivalence` (tier 2) and
//! `announcer_tier3_equivalence` (tier 3, the preview's mocked LLM).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::character_vault::ensure_character_vault;
use crate::db::runtime::Db;
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::db::{characters_read, chats_read};
use crate::post_office::deliver::{compose_and_deliver_letter, ComposeAndDeliverResult};
use crate::post_office::mailbox::list_mailbox;
use crate::services::announcer::audience::resolve_announcement_audience;
use crate::services::announcer::writer::{
    post_adhoc_announcement, AdhocAnnouncementParams, AnnouncerSender, StaffSender,
};

use super::types::{AnnouncerSenderWire, ErrorKind, Response, StaffSenderWire};

// ===========================================================================
// Response helpers (v4 `lib/api/responses.ts` semantics)
// ===========================================================================

fn ok(body: Value) -> Response {
    Response::ChatPostOffice(body)
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
/// v4 `notFound(resource)` → `` `${resource} not found` `` at 404.
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn forbidden(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Forbidden, msg)
}
fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
/// v4's Zod arm (`lib/api/middleware/context.ts:167` → `validationError`; the
/// file was `middleware/auth.ts` before `55752ad4`): a schema
/// rejection is a 400 whose body is `{error: 'Validation error', details: […]}`.
/// v5's error envelope carries no `details` array — the standing, named
/// P4.6bb deferral — so only the `error` string is reproduced. The
/// `post_office_routes_equivalence` validation cases assert that gap in BOTH
/// directions rather than normalizing it away.
fn validation_error() -> Response {
    bad_request("Validation error")
}

// ===========================================================================
// Zod floors (`app/api/v1/chats/[id]/schemas.ts:193-220`)
// ===========================================================================

/// `z.uuid()` — v4 (Zod 4) accepts any RFC 9562 UUID text form: 8-4-4-4-12 hex
/// with the variant/version nibbles unconstrained beyond the shape it checks.
/// Reproduced as the shape test, which is what every fixture id and every SPA
/// value exercises.
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// `z.string().min(1)` — JS string length is UTF-16 code units, but "at least one
/// unit" and "at least one byte" agree for every string, so a byte check is exact.
fn min1(s: &str) -> bool {
    !s.is_empty()
}

/// `z.string().min(1).max(120)` on `custom.displayName`. `max(120)` is 120 UTF-16
/// code units — counted as such, not bytes or scalars.
fn min1_max120(s: &str) -> bool {
    let units = s.encode_utf16().count();
    (1..=120).contains(&units)
}

impl From<StaffSenderWire> for StaffSender {
    fn from(w: StaffSenderWire) -> StaffSender {
        match w {
            StaffSenderWire::Lantern => StaffSender::Lantern,
            StaffSenderWire::Aurora => StaffSender::Aurora,
            StaffSenderWire::Librarian => StaffSender::Librarian,
            StaffSenderWire::Concierge => StaffSender::Concierge,
            StaffSenderWire::Prospero => StaffSender::Prospero,
            StaffSenderWire::Host => StaffSender::Host,
            StaffSenderWire::CommonplaceBook => StaffSender::CommonplaceBook,
            StaffSenderWire::Ariel => StaffSender::Ariel,
            StaffSenderWire::Suparna => StaffSender::Suparna,
            StaffSenderWire::Pascal => StaffSender::Pascal,
        }
    }
}

/// Validate + lower the wire sender union into the service's
/// [`AnnouncerSender`]. `None` = a Zod rejection.
fn lower_sender(w: &AnnouncerSenderWire) -> Option<AnnouncerSender> {
    match w {
        AnnouncerSenderWire::Staff { staff_id } => Some(AnnouncerSender::Staff {
            staff_id: (*staff_id).into(),
        }),
        AnnouncerSenderWire::Character { character_id } => {
            is_uuid(character_id).then(|| AnnouncerSender::Character {
                character_id: character_id.clone(),
            })
        }
        AnnouncerSenderWire::Custom { display_name } => {
            min1_max120(display_name).then(|| AnnouncerSender::Custom {
                display_name: display_name.clone(),
            })
        }
    }
}

// ===========================================================================
// Shared reads
// ===========================================================================

/// Nest [`Db::read_main`] + [`Db::read_mount_index`] (separate pools → no
/// deadlock) for the vault-overlay reads (the `api::chat_media` precedent).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&Connection, &Connection) -> Result<T, crate::db::DbError>,
) -> Result<T, crate::db::DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// v4 `findOperatorPlayedParticipant` (`participant-auth.ts:19`) — the
/// participant for a character the operator actually *plays* in this chat: a
/// CHARACTER participant they control (`controlledBy === 'user'`) that hasn't
/// been removed. **Security-load-bearing**: the Post Office "list mailbox" and
/// "compose mail" actions both gate on it so the operator can only act as a
/// character they are playing, never an LLM character or a stranger.
fn find_operator_played_participant(chat: &Value, character_id: &str) -> Option<Value> {
    chat.get("participants")
        .and_then(Value::as_array)?
        .iter()
        .find(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("characterId").and_then(Value::as_str) == Some(character_id)
                && p.get("controlledBy").and_then(Value::as_str) == Some("user")
                // `!p.removedAt` — absent, null or the empty string are all falsy.
                && !matches!(p.get("removedAt"), Some(Value::String(s)) if !s.is_empty())
        })
        .cloned()
}

fn load_chat(db: &Db, chat_id: &str) -> Result<Option<Value>, crate::db::DbError> {
    let cid = chat_id.to_string();
    db.read_main(move |c| chats_read::find_by_id(c, &cid))
}

// ===========================================================================
// `?action=announcement` — v4 `handleInsertAnnouncement`
// ===========================================================================

/// v4 `POST /api/v1/chats/[id]?action=announcement`. The POST wrapper resolves
/// the chat before dispatching (`handlers/post.ts:114`), so an unknown chat is a
/// `notFound('Chat')` here too.
///
/// `target_participant_ids` is the operator's chosen whisper audience (CHAT
/// PARTICIPANT ids). Omitted / empty posts publicly, exactly as it always has;
/// every id is re-verified against the chat's CURRENT participants and a dangling
/// one is a 400 — the alternative is persisting a message no filter would ever
/// surface.
pub async fn chat_announcement_post(
    db: &Db,
    chat_id: &str,
    content_markdown: &str,
    sender: &AnnouncerSenderWire,
    target_participant_ids: Option<&[String]>,
) -> Response {
    match load_chat(db, chat_id) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    }

    // `insertAnnouncementSchema.parse(body)`.
    if !min1(content_markdown) {
        return validation_error();
    }
    let Some(sender) = lower_sender(sender) else {
        return validation_error();
    };
    // `targetParticipantIds: z.array(z.uuid()).nullable().optional()`.
    if target_participant_ids.is_some_and(|ids| ids.iter().any(|id| !is_uuid(id))) {
        return validation_error();
    }

    // For the 'character' branch, verify the referenced character actually
    // exists so we fail fast with a clear error rather than persisting a
    // dangling reference.
    if let AnnouncerSender::Character { character_id } = &sender {
        let cid = character_id.clone();
        match read_main_mount(db, move |main, mount| {
            characters_read::find_by_id(main, mount, &cid)
        }) {
            Ok(Some(_)) => {}
            Ok(None) => return not_found("Character"),
            Err(e) => return internal(e),
        }
    }

    // A whisper audience is only meaningful if every id is a live participant of
    // this chat — otherwise we'd persist a message nobody can ever be shown.
    let audience = resolve_announcement_audience(db, chat_id, target_participant_ids);
    if !audience.unknown_ids.is_empty() {
        return bad_request(format!(
            "Unknown whisper target(s) for this chat: {}",
            audience.unknown_ids.join(", ")
        ));
    }

    let message = post_adhoc_announcement(
        db,
        &AdhocAnnouncementParams {
            chat_id: chat_id.to_string(),
            content_markdown: content_markdown.to_string(),
            sender,
            target_participant_ids: audience.target_participant_ids,
        },
    )
    .await;

    let Some(message) = message else {
        return bad_request("Failed to post announcement (empty content or unknown chat).");
    };

    ok(json!({ "success": true, "message": message }))
}

// ===========================================================================
// `?action=announcement-preview` — v4 `handleAnnouncementPreview`
// ===========================================================================

/// The host seam for the in-character rewrite: only the composing host holds the
/// completion provider + cheap executor + embedding provider the rewrite's
/// Commonplace recall and cheap-LLM call ride (the [`RecallReplayDriver`]
/// precedent). `Ok(result)` is v4's `CharacterVoicedAnnouncementResult`;
/// `Err(message)` is the not-assembled refusal.
///
/// [`RecallReplayDriver`]: super::recall_replay::RecallReplayDriver
pub trait AnnouncementPreviewDriver: Send + Sync {
    fn run(
        &self,
        input: CharacterVoicedRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CharacterVoicedOutcome, String>> + Send + '_>>;
}

/// What the driver needs to run one rewrite — the route's already-resolved
/// character + profile rows plus the seed.
#[derive(Debug, Clone)]
pub struct CharacterVoicedRequest {
    pub chat_id: String,
    pub character: Value,
    pub profile: Value,
    pub seed_markdown: String,
    pub system_prompt_id: Option<String>,
    pub user_id: String,
    /// Display names of the whisper audience the operator has chosen, already
    /// resolved and verified. Empty means the announcement is public and the
    /// character addresses the whole room.
    pub audience_names: Vec<String>,
}

/// v4 `CharacterVoicedAnnouncementResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterVoicedOutcome {
    pub success: bool,
    pub proposed_markdown: String,
    pub error: Option<String>,
}

/// A ready-made [`AnnouncementPreviewDriver`] over the ported service: hold a
/// `Db`, a completion provider, an embedding provider and a cheap-LLM executor,
/// and the rewrite runs. The composing host constructs one of these from its
/// spine — that construction is the only piece this lane defers.
pub struct AnnouncementPreviewRunner<C, E> {
    pub db: Db,
    pub completion: C,
    pub embedding: E,
    pub executor: std::sync::Arc<crate::services::cheap_llm_exec::CheapLlmTaskExecutor>,
    /// The injected `Date.now()` (ms) the Commonplace recall's time-decay and
    /// relative-age labels read. `None` = the process clock (production); the
    /// differential pins it.
    pub now_ms: Option<f64>,
}

impl<C, E> AnnouncementPreviewDriver for AnnouncementPreviewRunner<C, E>
where
    C: crate::model::completion::CompletionProvider + Send + Sync,
    E: crate::model::embedding::EmbeddingProvider + Send + Sync,
{
    fn run(
        &self,
        input: CharacterVoicedRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CharacterVoicedOutcome, String>> + Send + '_>> {
        Box::pin(async move {
            let result = crate::services::announcer::character_voiced::
                generate_character_voiced_announcement(
                    &self.db,
                    &self.completion,
                    &self.embedding,
                    &self.executor,
                    &crate::services::announcer::character_voiced::
                        CharacterVoicedAnnouncementParams {
                        chat_id: &input.chat_id,
                        character: &input.character,
                        profile: &input.profile,
                        seed_markdown: &input.seed_markdown,
                        system_prompt_id: input.system_prompt_id.as_deref(),
                        user_id: &input.user_id,
                        audience_names: &input.audience_names,
                        now_ms: self
                            .now_ms
                            .unwrap_or_else(|| crate::clock::now_unix_ms() as f64),
                    },
                )
                .await;
            Ok(CharacterVoicedOutcome {
                success: result.success,
                proposed_markdown: result.proposed_markdown,
                error: result.error,
            })
        })
    }
}

/// v4 `POST /api/v1/chats/[id]?action=announcement-preview`.
#[allow(clippy::too_many_arguments)] // mirrors the route's parameter surface
pub async fn chat_announcement_preview(
    db: &Db,
    driver: Option<&Arc<dyn AnnouncementPreviewDriver>>,
    user_id: &str,
    chat_id: &str,
    seed_markdown: &str,
    character_id: &str,
    connection_profile_id: &str,
    system_prompt_id: Option<&str>,
    target_participant_ids: Option<&[String]>,
) -> Response {
    match load_chat(db, chat_id) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    }

    // `insertAnnouncementPreviewSchema.parse(body)`.
    if !min1(seed_markdown)
        || !is_uuid(character_id)
        || !is_uuid(connection_profile_id)
        || system_prompt_id.is_some_and(|s| !is_uuid(s))
        || target_participant_ids.is_some_and(|ids| ids.iter().any(|id| !is_uuid(id)))
    {
        return validation_error();
    }

    let cid = character_id.to_string();
    let character = match read_main_mount(db, move |main, mount| {
        characters_read::find_by_id(main, mount, &cid)
    }) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Character"),
        Err(e) => return internal(e),
    };

    let pid = connection_profile_id.to_string();
    let profile = match db.read_main(move |c| crate::db::connection_profiles::find_by_id(c, &pid)) {
        Ok(Some(p)) => p,
        Ok(None) => return not_found("Connection profile"),
        Err(e) => return internal(e),
    };

    // Nothing is persisted here, so an unresolvable target is not worth failing
    // the rehearsal over — it simply doesn't contribute a name to the audience.
    // The post action is the gate that refuses dangling ids.
    let audience = resolve_announcement_audience(db, chat_id, target_participant_ids);

    let Some(driver) = driver else {
        return internal(
            "chatAnnouncementPreview is not available: the host has not assembled an \
             AnnouncementPreviewDriver (the in-character rewrite needs the completion + \
             embedding providers).",
        );
    };

    let outcome = driver
        .run(CharacterVoicedRequest {
            chat_id: chat_id.to_string(),
            character,
            profile,
            seed_markdown: seed_markdown.to_string(),
            system_prompt_id: system_prompt_id.map(str::to_string),
            user_id: user_id.to_string(),
            audience_names: audience.target_names,
        })
        .await;

    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return internal(e),
    };

    if !outcome.success {
        // v4: `badRequest(result.error || 'Failed to generate in-character announcement.')`
        // — an empty-string error is falsy in JS, so it takes the default too.
        let msg = outcome
            .error
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| "Failed to generate in-character announcement.".to_string());
        return bad_request(msg);
    }

    ok(json!({ "success": true, "proposedMarkdown": outcome.proposed_markdown }))
}

// ===========================================================================
// `?action=send-mail` — v4 `handleSendMail`
// ===========================================================================

/// v4 `POST /api/v1/chats/[id]?action=send-mail`. `now_iso` is the injected
/// delivery `sentAt` (v4 mints it inside the delivery path).
pub async fn chat_send_mail(
    db: &Db,
    chat_id: &str,
    from_character_id: &str,
    to_character_id: &str,
    body_markdown: &str,
    in_reply_to_path: Option<&str>,
    now_iso: &str,
) -> Response {
    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    // `sendMailActionSchema.parse(body)`.
    if !is_uuid(from_character_id)
        || !is_uuid(to_character_id)
        || !min1(body_markdown)
        || in_reply_to_path.is_some_and(|p| !min1(p))
    {
        return validation_error();
    }

    // The operator may only post AS a character they actually play in this chat.
    // Re-verify against the participant list — never trust the client to send as
    // an LLM character or a stranger.
    if find_operator_played_participant(&chat, from_character_id).is_none() {
        return bad_request(
            "You can only post a letter as a character you are playing in this scene.",
        );
    }

    // Use raw rows (existence-only) so a hollow-but-present vault doesn't 503
    // here; delivery re-provisions the vault idempotently anyway.
    let from = from_character_id.to_string();
    let sender = match db.read_main(move |c| characters_read::find_by_id_raw(c, &from)) {
        Ok(Some(s)) => s,
        Ok(None) => return not_found("Sender character"),
        Err(e) => return internal(e),
    };
    let to = to_character_id.to_string();
    let recipient = match db.read_main(move |c| characters_read::find_by_id_raw(c, &to)) {
        Ok(Some(r)) => r,
        Ok(None) => return not_found("Recipient character"),
        Err(e) => return internal(e),
    };

    let message = body_markdown.to_string();
    let in_reply_to = in_reply_to_path.map(str::to_string);
    let now = now_iso.to_string();
    let delivered = db
        .write(move |writers| {
            let Some(mount_w) = writers.mount_index() else {
                return Err(crate::db::DbError::Key(
                    "the Post Office requires the mount-index database".to_string(),
                ));
            };
            let mount = mount_w.connection();
            let main = writers.main().connection();
            compose_and_deliver_letter(
                main,
                mount,
                &sender,
                &recipient,
                &message,
                in_reply_to.as_deref(),
                &now,
            )
        })
        .await;

    match delivered {
        Ok(ComposeAndDeliverResult::Ok(path)) => ok(json!({ "success": true, "path": path })),
        Ok(ComposeAndDeliverResult::ReplyNotFound) => bad_request(
            "That letter is no longer in your own postbox, so there's nothing to reply to.",
        ),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// `?action=mailbox` (GET) — v4 `handleGetMailbox`
// ===========================================================================

/// v4 `GET /api/v1/chats/[id]?action=mailbox&characterId=…`.
///
/// The `ensureCharacterVault` call is a **write on a read path** — v4's own
/// behavior (`mailbox.ts:44`): a character whose vault link is missing gets one
/// provisioned (or an existing same-name populated vault adopted) before the
/// listing walks it.
pub async fn chat_mailbox_list(db: &Db, chat_id: &str, character_id: &str) -> Response {
    // v4 reads the query param: absent → `null` → the 400. An EMPTY `characterId=`
    // is also falsy in JS, so it takes the same arm.
    if character_id.is_empty() {
        return bad_request("characterId is required");
    }

    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    // Authorize: only a character the operator plays in this chat.
    if find_operator_played_participant(&chat, character_id).is_none() {
        return forbidden(
            "You may only inspect the mailbox of a character you are playing in this scene.",
        );
    }

    let cid = character_id.to_string();
    let character = match db.read_main(move |c| characters_read::find_by_id_raw(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Character"),
        Err(e) => return internal(e),
    };

    let letters = db
        .write(move |writers| {
            let Some(mount_w) = writers.mount_index() else {
                return Err(crate::db::DbError::Key(
                    "the Post Office requires the mount-index database".to_string(),
                ));
            };
            let mount = mount_w.connection();
            let main = writers.main().connection();
            let id = character
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = character
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let fk = character
                .get("characterDocumentMountPointId")
                .and_then(Value::as_str);
            let input: CharacterVaultWriteInput =
                serde_json::from_value(character.clone()).unwrap_or_default();
            let vault = ensure_character_vault(main, mount, id, name, &input, fk)?;
            list_mailbox(mount, &vault.mount_point_id)
        })
        .await;

    match letters {
        Ok(letters) => ok(json!({
            "letters": letters
                .into_iter()
                .map(|l| json!({ "path": l.path, "from": l.from, "sentAt": l.sent_at }))
                .collect::<Vec<Value>>(),
        })),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_played_participant_gates_on_all_four_predicates() {
        let chat = json!({ "participants": [
            { "type": "CHARACTER", "characterId": "a", "controlledBy": "llm" },
            { "type": "CHARACTER", "characterId": "b", "controlledBy": "user",
              "removedAt": "2026-01-01T00:00:00.000Z" },
            { "type": "USER", "characterId": "c", "controlledBy": "user" },
            { "type": "CHARACTER", "characterId": "d", "controlledBy": "user",
              "removedAt": null, "id": "p-d" },
        ]});
        assert!(find_operator_played_participant(&chat, "a").is_none());
        assert!(find_operator_played_participant(&chat, "b").is_none());
        assert!(find_operator_played_participant(&chat, "c").is_none());
        assert_eq!(
            find_operator_played_participant(&chat, "d")
                .and_then(|p| p.get("id").and_then(Value::as_str).map(str::to_string)),
            Some("p-d".to_string())
        );
    }

    #[test]
    fn zod_floors() {
        assert!(is_uuid("c1000000-0000-4000-8000-000000000001"));
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid(""));
        assert!(min1("x"));
        assert!(!min1(""));
        assert!(min1_max120(&"x".repeat(120)));
        assert!(!min1_max120(&"x".repeat(121)));
        // UTF-16 code units, not scalars: an astral character costs two.
        assert!(min1_max120(&"🎲".repeat(60)));
        assert!(!min1_max120(&"🎲".repeat(61)));
    }
}
