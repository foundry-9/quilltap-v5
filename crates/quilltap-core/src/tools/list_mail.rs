//! Port of v4's `list_mail` tool (`lib/tools/handlers/list-mail-handler.ts` +
//! `lib/tools/list-mail-tool.ts`; renamed from `list_email` at v4 `39bc98ffc`
//! with NO alias and NO data migration — a stored `disabledTools` naming
//! `list_email` goes inert, and a model echoing the old name takes the
//! unknown-tool path, exactly as in v4).
//!
//! Lists the CALLER's own mailbox (and only its own), newest first, spelling out
//! the exact tool calls to read (`read_mail`), answer, or discard each letter. A
//! missing/empty `Mail/` folder reads as an empty postbox, not an error. Runs on
//! both writer connections.

use rusqlite::Connection;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::db::character_vault::ensure_character_vault;
use crate::db::characters_read::find_by_id_raw_or_none;
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::post_office::instructions::{format_letter_actions, format_letter_heading};
use crate::post_office::mailbox::list_mailbox;
use jiff::tz::TimeZone;

const EMPTY_POSTBOX: &str = "Your postbox stands empty.";

/// v4 `ListMailToolOutput` — `{ success, listing, count, error? }`. Serialized in
/// a fixed order; success omits `error`.
#[derive(Debug, Clone)]
pub struct ListMailOutput {
    pub success: bool,
    pub listing: String,
    pub count: usize,
    pub error: Option<String>,
}

impl Serialize for ListMailOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let len = if self.error.is_some() { 4 } else { 3 };
        let mut st = s.serialize_struct("ListMailOutput", len)?;
        st.serialize_field("success", &self.success)?;
        st.serialize_field("listing", &self.listing)?;
        // v4 `count` is a JS number; the corpus values are small integers.
        st.serialize_field("count", &self.count)?;
        if let Some(e) = &self.error {
            st.serialize_field("error", e)?;
        }
        st.end()
    }
}

/// v4 `fail(message)`: `{ success:false, listing: message, count: 0, error: message }`.
fn fail(message: &str) -> ListMailOutput {
    ListMailOutput {
        success: false,
        listing: message.to_string(),
        count: 0,
        error: Some(message.to_string()),
    }
}

/// Execute the `list_mail` tool in the host's zone.
/// The production entry (the tool executor's): v4's zone-less date renders
/// resolve the HOST's zone (P4.119), read here once. Tests and differentials
/// call [`execute_list_mail_in_zone`] with their zone explicitly.
pub fn execute_list_mail(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
) -> ListMailOutput {
    execute_list_mail_in_zone(
        main,
        mount,
        chat_id,
        character_id,
        args,
        &crate::host_zone::system_display_zone(),
    )
}

/// Execute the `list_mail` tool (v4 `executeListMailTool`). Runs on both writer
/// connections; each letter's date renders in `zone`.
///
/// v4 wraps the whole handler in ONE `try`/`catch`: any thrown error lands in
/// the catch, which logs `list_mail handler threw unexpectedly {chatId}` at
/// ERROR and answers the in-voice "stumbled" failure. Here that is
/// [`list_mail_inner`]'s `Err` arm. v4's character read and mailbox listing are
/// FALLBACK repository reads that never throw (a failed one is the postbox
/// refusal / an empty postbox — P4.126), so what still reaches it is an
/// UNLINKED character's vault-create write.
pub fn execute_list_mail_in_zone(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: Option<&str>,
    args: &Value,
    zone: &TimeZone,
) -> ListMailOutput {
    match list_mail_inner(main, mount, character_id, args, zone) {
        Ok(out) => out,
        Err(e) => {
            tracing::error!(
                module = LOG_MODULE,
                chatId = chat_id,
                error = %e,
                "list_mail handler threw unexpectedly"
            );
            fail(&format!(
                "The Post Office stumbled and couldn't sort your post — {e}"
            ))
        }
    }
}

/// v4's module logger identity (`logger.child({ module: 'list-mail-handler' })`).
const LOG_MODULE: &str = "list-mail-handler";

/// The body of v4's `try` block. An `Err` is v4's catch.
fn list_mail_inner(
    main: &Connection,
    mount: &Connection,
    character_id: Option<&str>,
    args: &Value,
    zone: &TimeZone,
) -> Result<ListMailOutput, crate::db::DbError> {
    // v4 `validateListMailInput` = `z.object({})` safeParse: any object is valid.
    if !args.is_object() {
        return Ok(fail("That request to the Post Office made no sense."));
    }
    let Some(character_id) = character_id.filter(|s| !s.is_empty()) else {
        return Ok(fail(
            "Only a character keeps a postbox, and no character holds this one.",
        ));
    };

    let Some(me) = find_by_id_raw_or_none(main, character_id) else {
        return Ok(fail(
            "The Post Office cannot find your postbox; your character seems to have gone astray.",
        ));
    };
    // An archived character has no reachable postbox (v4 `d553f72a`,
    // `list-mail-handler.ts:53`).
    if crate::api::characters::is_archived(&me) {
        return Ok(fail(
            "That character is archived; rehydrate it to continue.",
        ));
    }

    let my_vault_id = ensure_own_vault(main, mount, character_id, &me)?;
    let letters = list_mailbox(mount, &my_vault_id)?;

    if letters.is_empty() {
        return Ok(ListMailOutput {
            success: true,
            listing: EMPTY_POSTBOX.to_string(),
            count: 0,
            error: None,
        });
    }

    let n = letters.len();
    let header = format!(
        "Your postbox holds {n} letter{}, newest first. (Each letter is named by its file name — hand that to read_mail, discard_mail, or send_mail's in_reply_to.)",
        if n == 1 { "" } else { "s" }
    );
    let blocks: Vec<String> = letters
        .iter()
        .enumerate()
        .map(|(i, letter)| {
            format!(
                "{}\n{}",
                format_letter_heading(letter, i + 1, zone),
                format_letter_actions(&letter.path, &letter.from, true)
            )
        })
        .collect();

    Ok(ListMailOutput {
        success: true,
        listing: format!("{header}\n\n{}", blocks.join("\n\n")),
        count: n,
        error: None,
    })
}

/// v4 `ensureCharacterVault(me).mountPointId` over the caller's RAW row — a
/// provisioned character returns its existing FK. Shared by the three
/// mailbox-owning tools (`list_mail`, `read_mail`, `discard_mail`), none of
/// which routes through the doc-tool resolver: the systemTransparency covenant
/// (P4.D200, `doc_edit::shared::build_read_resolution_context`) deliberately
/// never keeps a character from its own post (v4 `39bc98ffc`/`12c336fad`).
///
/// A LINKED character's arm is v4's exactly: `ensureCharacterVault` returns
/// the FK and touches no store (`character-vault.ts:146-148`), so it cannot
/// fail. v5's [`ensure_character_vault`] also runs the lazy fact-sheet
/// backfill on that arm — a mount-store read and write v4's mail tools never
/// make, and whose failure would reach the catch v4 cannot — so the mail tools
/// take the FK directly (P4.126). An UNLINKED character takes the full ensure,
/// whose store writes throw in v4 too.
pub(crate) fn ensure_own_vault(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    me: &Value,
) -> Result<String, crate::db::DbError> {
    let name = me.get("name").and_then(Value::as_str).unwrap_or_default();
    let fk = me
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str);
    // JS truthiness: an empty FK is unlinked.
    if let Some(fk) = fk.filter(|s| !s.is_empty()) {
        return Ok(fk.to_string());
    }
    let input: CharacterVaultWriteInput = serde_json::from_value(me.clone()).unwrap_or_default();
    Ok(ensure_character_vault(main, mount, character_id, name, &input, None)?.mount_point_id)
}

/// v4 `formatListMailResults`: `success ? listing : error || listing` — an
/// EMPTY `error` is falsy and falls through to the listing (the `read_mail` /
/// `discard_mail` siblings' filter).
pub fn format_list_mail_results(out: &ListMailOutput) -> String {
    if out.success {
        out.listing.clone()
    } else {
        out.error
            .clone()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| out.listing.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(error: Option<&str>) -> ListMailOutput {
        ListMailOutput {
            success: false,
            listing: "the listing".to_string(),
            count: 0,
            error: error.map(str::to_string),
        }
    }

    /// v4 `output.error || output.listing`: an empty `error` is falsy, so the
    /// listing answers (P4.126 — the siblings already filtered it).
    #[test]
    fn an_empty_error_falls_through_to_the_listing() {
        assert_eq!(format_list_mail_results(&failed(Some(""))), "the listing");
        assert_eq!(format_list_mail_results(&failed(None)), "the listing");
        assert_eq!(format_list_mail_results(&failed(Some("boom"))), "boom");
        let mut ok = failed(Some("ignored"));
        ok.success = true;
        assert_eq!(format_list_mail_results(&ok), "the listing");
    }
}
