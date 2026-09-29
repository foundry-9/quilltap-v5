//! Port of v4's `read_mail` tool (`lib/tools/handlers/read-mail-handler.ts` +
//! `lib/tools/read-mail-tool.ts`, NEW at v4 `39bc98ffc`).
//!
//! Reads one letter from the CALLER's own mailbox (and only its own — the
//! reference is confined to its `Mail/` folder by
//! [`resolve_mail_path`]). Like `list_mail`, it reaches the vault through
//! `ensure_character_vault` rather than the doc-tool resolver, so the
//! systemTransparency covenant — which hides character vaults from `doc_*`
//! tools (P4.D200) — never keeps a character from its own post. That bypass is
//! v4's design, not a hole to close.
//!
//! Reading a letter Suparṇā has not yet announced marks it announced: the
//! character has read it, so she has nothing left to bring. That flag is a
//! CONTENT rewrite of the letter (its frontmatter), so this "read" tool runs on
//! both WRITER connections.
//!
//! Nor does it consult the letter link's `allowCharacterRead` — a letter a
//! protected-document rule would hide from `doc_read_file` is read anyway, as
//! in v4 (the Post Office tools never pass through `assertCharacterMayRead`).

use rusqlite::Connection;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value;

use super::list_mail::ensure_own_vault;
use crate::db::characters_read::find_by_id_raw;
use crate::db::DbError;
use crate::jsstr::js_trim;
use crate::post_office::instructions::{format_letter_actions, format_letter_date};
use crate::post_office::mailbox::{letter_file_name, mark_alerted, read_letter, resolve_mail_path};

/// v4's module logger identity (`logger.child({ module: 'read-mail-handler' })`).
const LOG_MODULE: &str = "read-mail-handler";

/// v4 `ReadMailToolOutput` — `{ success, text, path?, error? }`. Serialized in
/// declaration order; success omits `error`, failure omits `path`.
#[derive(Debug, Clone)]
pub struct ReadMailOutput {
    pub success: bool,
    pub text: String,
    pub path: Option<String>,
    pub error: Option<String>,
}

impl Serialize for ReadMailOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let len = 2 + usize::from(self.path.is_some()) + usize::from(self.error.is_some());
        let mut st = s.serialize_struct("ReadMailOutput", len)?;
        st.serialize_field("success", &self.success)?;
        st.serialize_field("text", &self.text)?;
        if let Some(p) = &self.path {
            st.serialize_field("path", p)?;
        }
        if let Some(e) = &self.error {
            st.serialize_field("error", e)?;
        }
        st.end()
    }
}

/// v4 `fail(message)`: `{ success:false, text: message, error: message }`.
fn fail(message: &str) -> ReadMailOutput {
    ReadMailOutput {
        success: false,
        text: message.to_string(),
        path: None,
        error: Some(message.to_string()),
    }
}

/// v4 `validateReadMailInput`: `z.object({ letter: z.string().min(1) })`,
/// non-strict (extra keys pass). `min(1)` counts UTF-16 units, and every
/// non-empty Rust string has at least one. `"   "` PASSES here and is refused
/// by [`resolve_mail_path`] with the "rummage" message, not this one.
pub(crate) fn validate_letter_input(args: &Value) -> Option<&str> {
    match args.as_object()?.get("letter") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    }
}

/// Execute the `read_mail` tool (v4 `executeReadMailTool`). Runs on both writer
/// connections (the announced flag is a mount-store content write).
pub fn execute_read_mail(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
) -> ReadMailOutput {
    match read_mail_inner(main, mount, chat_id, character_id, args) {
        Ok(out) => out,
        Err(e) => {
            tracing::error!(
                module = LOG_MODULE,
                chatId = chat_id,
                error = %e,
                "read_mail handler threw unexpectedly"
            );
            fail(&format!(
                "The Post Office stumbled and couldn't fetch your letter — {e}"
            ))
        }
    }
}

/// The body of v4's `try` block. An `Err` is v4's catch.
fn read_mail_inner(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
) -> Result<ReadMailOutput, DbError> {
    let Some(letter_ref) = validate_letter_input(args) else {
        return Ok(fail(
            "The Post Office needs the letter's file name to fetch it.",
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
    let Some(letter) = read_letter(mount, &my_vault_id, &path)? else {
        tracing::debug!(
            module = LOG_MODULE,
            chatId = chat_id,
            characterId = me_id,
            path = path.as_str(),
            "read_mail: no such letter"
        );
        return Ok(fail(&format!(
            "No letter named \"{}\" rests in your postbox. list_mail will show you what does.",
            letter_file_name(&path)
        )));
    };

    let marked_alerted = !letter.frontmatter.alerted;
    if marked_alerted {
        mark_alerted(mount, &my_vault_id, &path)?;
    }
    tracing::debug!(
        module = LOG_MODULE,
        chatId = chat_id,
        characterId = me_id,
        path = path.as_str(),
        markedAlerted = marked_alerted,
        "read_mail: letter read"
    );

    let from = &letter.frontmatter.from;
    let body = js_trim(&letter.body);
    let text = [
        format!(
            "A letter from {from}, posted {}:",
            format_letter_date(&letter.frontmatter.sent_at)
        ),
        if body.is_empty() {
            "(the letter is blank)".to_string()
        } else {
            body.to_string()
        },
        format_letter_actions(&path, from, false),
    ]
    .join("\n\n");

    Ok(ReadMailOutput {
        success: true,
        text,
        path: Some(path),
        error: None,
    })
}

/// v4 `formatReadMailResults`: `success ? text : error || text`.
pub fn format_read_mail_results(out: &ReadMailOutput) -> String {
    if out.success {
        out.text.clone()
    } else {
        out.error
            .clone()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| out.text.clone())
    }
}
