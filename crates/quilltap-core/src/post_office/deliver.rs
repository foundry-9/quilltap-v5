//! The Post Office — shared letter-delivery service (v4 `lib/post-office/deliver.ts`).
//!
//! The single composition + delivery path used by the `send_mail` tool handler
//! and the Salon "Compose Mail" action.
//! Both vaults are ensured (idempotent), the reply-quoting rules applied when
//! `inReplyTo` is given, and the letter delivered into the recipient's `Mail/`
//! folder. No "Sent" copy is written into the sender's vault (a character replies
//! to letters it RECEIVED).
//!
//! `sentAt` (`new Date().toISOString()`) is an **injected** clock value so the
//! differential can pin it — the delivered filename's epoch prefix and the reply
//! preface's date both derive from it.

use rusqlite::Connection;
use serde_json::Value;

use super::mailbox::{
    build_reply_preface, deliver_letter, read_letter, resolve_mail_path, DeliverLetterParams,
    ParsedLetter,
};
use crate::db::character_vault::ensure_character_vault;
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::db::DbError;

/// The tracing target for the module's own lines (v4
/// `createServiceLogger('PostOffice:Deliver')`).
pub const LOG_TARGET: &str = "quilltap::post_office::deliver";

/// The result of [`compose_and_deliver_letter`] (v4 `ComposeAndDeliverResult`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeAndDeliverResult {
    /// Delivered to this recipient vault-relative path.
    Ok(String),
    /// `inReplyTo` referenced a letter not in the sender's own mailbox.
    ReplyNotFound,
}

/// Resolve (or provision) a character's vault mount-point id (v4
/// `ensureCharacterVault(character).mountPointId`). A provisioned character
/// short-circuits on its existing FK; otherwise the vault is scaffolded.
fn ensure_vault(
    main: &Connection,
    mount: &Connection,
    character: &Value,
) -> Result<String, DbError> {
    let cid = character
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
    let res = ensure_character_vault(main, mount, cid, name, &input, fk)?;
    Ok(res.mount_point_id)
}

/// v4 `composeAndDeliverLetter`. `now_iso` is the injected `sentAt`; `zone`
/// renders a reply preface's date (the host's in production — P4.119).
#[allow(clippy::too_many_arguments)]
pub fn compose_and_deliver_letter(
    main: &Connection,
    mount: &Connection,
    sender: &Value,
    recipient: &Value,
    message: &str,
    in_reply_to: Option<&str>,
    now_iso: &str,
    zone: &jiff::tz::TimeZone,
) -> Result<ComposeAndDeliverResult, DbError> {
    // Store the canonical `Mail/…` path whichever form the caller named it by
    // (v4 `39bc98ffc`: `params.inReplyTo ? (resolveMailPath(…) ?? raw) : null`).
    // An unresolvable reference stays raw, but then fails the lookup below, so a
    // raw value never lands in a letter. Shared by `send_mail` AND the Salon
    // "Compose Mail" action (`api/chat_post_office.rs`), which both move.
    let resolved_reply: Option<String> = in_reply_to
        .filter(|r| !r.is_empty())
        .map(|r| resolve_mail_path(r).unwrap_or_else(|| r.to_string()));
    let in_reply_to = resolved_reply.as_deref();

    let sender_vault_id = ensure_vault(main, mount, sender)?;
    let recipient_vault_id = ensure_vault(main, mount, recipient)?;

    let mut body = message.to_string();
    if let Some(reply) = in_reply_to {
        let Some(original) = resolve_reply_in_sender_mailbox(mount, &sender_vault_id, reply)?
        else {
            tracing::debug!(
                target: LOG_TARGET,
                senderVaultId = sender_vault_id.as_str(),
                inReplyTo = reply,
                "Reply target not in sender mailbox"
            );
            return Ok(ComposeAndDeliverResult::ReplyNotFound);
        };
        let preface = build_reply_preface(&original.body, &original.frontmatter.sent_at, zone);
        body = format!("{preface}\n\n{message}");
    }

    let sender_name = sender
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sender_id = sender.get("id").and_then(Value::as_str).unwrap_or_default();
    let path = deliver_letter(
        mount,
        &DeliverLetterParams {
            recipient_vault_id: &recipient_vault_id,
            from_name: sender_name,
            from_character_id: sender_id,
            sent_at: now_iso,
            body: &body,
            in_reply_to,
        },
    )?;
    Ok(ComposeAndDeliverResult::Ok(path))
}

/// v4 `resolveReplyInSenderMailbox`: resolve an `in_reply_to` reference — a
/// letter's file name (or its `Mail/…` path) — to a letter that exists in the
/// SENDER's own mailbox. `None` when the reference escapes `Mail/` or no such
/// letter exists (v4 `39bc98ffc`; it had required a raw `Mail/…` prefix and read
/// the slash-stripped reference as given, so a bare name was `reply-not-found`
/// and `Mail/sub/x.md` was read).
pub fn resolve_reply_in_sender_mailbox(
    mount: &Connection,
    sender_vault_id: &str,
    in_reply_to: &str,
) -> Result<Option<ParsedLetter>, DbError> {
    let Some(path) = resolve_mail_path(in_reply_to) else {
        return Ok(None);
    };
    read_letter(mount, sender_vault_id, &path)
}
