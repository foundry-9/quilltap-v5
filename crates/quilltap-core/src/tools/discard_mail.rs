//! Port of v4's `discard_mail` tool (`lib/tools/handlers/discard-mail-handler.ts`
//! + `lib/tools/discard-mail-tool.ts`, NEW at v4 `12c336fad`).
//!
//! Discards one letter from the CALLER's own mailbox (and only its own — the
//! reference is confined to its `Mail/` folder). Like `list_mail` and
//! `read_mail`, it reaches the vault through `ensure_character_vault` rather
//! than the doc-tool resolver, so the systemTransparency covenant never keeps a
//! character from its own post.
//!
//! The delete itself is [`discard_letter`], which goes through the
//! database-store delete chokepoint (`delete_with_gc`): hard links and file-row
//! collection are handled exactly as `doc_delete_file` handles them.
//!
//! ⚠ "The same chokepoint `doc_delete_file` uses" (v4's commit message) is true
//! of the GC step ONLY. `doc_delete_file` also runs `assertCharacterMayWrite`
//! (the protected-document check) and posts a Librarian delete announcement;
//! `discard_mail` does NEITHER, so a letter whose link carries
//! `allowCharacterWrite = 0` is discarded anyway and nothing is announced. That
//! is v4's property (`12c336fad`), ported as it stands — do not route this
//! through the doc-edit handler to "fix" it.

use rusqlite::Connection;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value;

use super::list_mail::ensure_own_vault;
use super::read_mail::validate_letter_input;
use crate::db::characters_read::find_by_id_raw;
use crate::db::DbError;
use crate::post_office::mailbox::{discard_letter, letter_file_name, resolve_mail_path};

/// v4's module logger identity (`logger.child({ module: 'discard-mail-handler' })`).
const LOG_MODULE: &str = "discard-mail-handler";

/// v4 `DiscardMailToolOutput` — `{ success, message, path?, error? }`. Serialized
/// in declaration order; success omits `error`, failure omits `path`.
#[derive(Debug, Clone)]
pub struct DiscardMailOutput {
    pub success: bool,
    pub message: String,
    pub path: Option<String>,
    pub error: Option<String>,
}

impl Serialize for DiscardMailOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let len = 2 + usize::from(self.path.is_some()) + usize::from(self.error.is_some());
        let mut st = s.serialize_struct("DiscardMailOutput", len)?;
        st.serialize_field("success", &self.success)?;
        st.serialize_field("message", &self.message)?;
        if let Some(p) = &self.path {
            st.serialize_field("path", p)?;
        }
        if let Some(e) = &self.error {
            st.serialize_field("error", e)?;
        }
        st.end()
    }
}

/// v4 `fail(message)`: `{ success:false, message, error: message }`.
fn fail(message: &str) -> DiscardMailOutput {
    DiscardMailOutput {
        success: false,
        message: message.to_string(),
        path: None,
        error: Some(message.to_string()),
    }
}

/// Execute the `discard_mail` tool (v4 `executeDiscardMailTool`). Runs on both
/// writer connections (the delete is a mount-store write).
pub fn execute_discard_mail(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
) -> DiscardMailOutput {
    match discard_mail_inner(main, mount, chat_id, character_id, args) {
        Ok(out) => out,
        Err(e) => {
            tracing::error!(
                module = LOG_MODULE,
                chatId = chat_id,
                error = %e,
                "discard_mail handler threw unexpectedly"
            );
            fail(&format!(
                "The Post Office stumbled and the letter stays where it was — {e}"
            ))
        }
    }
}

/// The body of v4's `try` block. An `Err` is v4's catch.
fn discard_mail_inner(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
) -> Result<DiscardMailOutput, DbError> {
    let Some(letter_ref) = validate_letter_input(args) else {
        return Ok(fail(
            "The Post Office needs the letter's file name to know which one to discard.",
        ));
    };
    let Some(character_id) = character_id.filter(|s| !s.is_empty()) else {
        return Ok(fail(
            "Only a character keeps a postbox, and no character holds this one.",
        ));
    };
    // v4 resolves the reference BEFORE looking the character up.
    let Some(path) = resolve_mail_path(letter_ref) else {
        return Ok(fail(
            "Name the letter by its file name alone — the Post Office will not rummage outside your postbox.",
        ));
    };

    let Some(me) = find_by_id_raw(main, character_id)? else {
        return Ok(fail(
            "The Post Office cannot find your postbox; your character seems to have gone astray.",
        ));
    };
    if crate::api::characters::is_archived(&me) {
        return Ok(fail(
            "That character is archived; rehydrate it to continue.",
        ));
    }
    let me_id = me.get("id").and_then(Value::as_str).unwrap_or(character_id);

    let my_vault_id = ensure_own_vault(main, mount, character_id, &me)?;
    let deleted = discard_letter(mount, &my_vault_id, &path)?;
    // v4 names the letter AFTER the delete, whatever its outcome.
    let name = letter_file_name(&path);
    if !deleted {
        tracing::debug!(
            module = LOG_MODULE,
            chatId = chat_id,
            characterId = me_id,
            path = path.as_str(),
            "discard_mail: no such letter"
        );
        return Ok(fail(&format!(
            "No letter named \"{name}\" rests in your postbox. list_mail will show you what does."
        )));
    }

    tracing::info!(
        module = LOG_MODULE,
        chatId = chat_id,
        characterId = me_id,
        path = path.as_str(),
        "discard_mail: letter discarded"
    );
    Ok(DiscardMailOutput {
        success: true,
        message: format!("The letter \"{name}\" has been consigned to the wastepaper basket."),
        path: Some(path.clone()),
        error: None,
    })
}

/// v4 `formatDiscardMailResults`: `success ? message : error || message`.
pub fn format_discard_mail_results(out: &DiscardMailOutput) -> String {
    if out.success {
        out.message.clone()
    } else {
        out.error
            .clone()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| out.message.clone())
    }
}
