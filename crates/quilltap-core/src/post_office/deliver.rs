//! The Post Office — shared letter-delivery service (v4 `lib/post-office/deliver.ts`).
//!
//! The single composition + delivery path used by the `send_mail` tool handler.
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
    build_reply_preface, deliver_letter, read_letter, DeliverLetterParams, ParsedLetter,
    MAIL_FOLDER,
};
use crate::db::character_vault::ensure_character_vault;
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::db::DbError;

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

/// v4 `composeAndDeliverLetter`. `now_iso` is the injected `sentAt`.
pub fn compose_and_deliver_letter(
    main: &Connection,
    mount: &Connection,
    sender: &Value,
    recipient: &Value,
    message: &str,
    in_reply_to: Option<&str>,
    now_iso: &str,
) -> Result<ComposeAndDeliverResult, DbError> {
    let sender_vault_id = ensure_vault(main, mount, sender)?;
    let recipient_vault_id = ensure_vault(main, mount, recipient)?;

    let mut body = message.to_string();
    if let Some(reply) = in_reply_to {
        let Some(original) = resolve_reply_in_sender_mailbox(mount, &sender_vault_id, reply)?
        else {
            return Ok(ComposeAndDeliverResult::ReplyNotFound);
        };
        let preface = build_reply_preface(&original.body, &original.frontmatter.sent_at);
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

/// v4 `resolveReplyInSenderMailbox`: the `inReplyTo` reference must be a `Mail/…`
/// path in the SENDER's own mailbox. Strips leading slashes; a non-`Mail/` path
/// (case-insensitive) → `None`.
pub fn resolve_reply_in_sender_mailbox(
    mount: &Connection,
    sender_vault_id: &str,
    in_reply_to: &str,
) -> Result<Option<ParsedLetter>, DbError> {
    let normalized = in_reply_to.trim_start_matches('/');
    let prefix = format!("{MAIL_FOLDER}/");
    if !normalized
        .to_lowercase()
        .starts_with(&prefix.to_lowercase())
    {
        return Ok(None);
    }
    read_letter(mount, sender_vault_id, normalized)
}
