//! Port of v4's `list_email` tool (`lib/tools/handlers/list-email-handler.ts` +
//! `lib/tools/list-email-tool.ts`).
//!
//! Lists the CALLER's own mailbox (and only its own), newest first, spelling out
//! the exact tool calls to read, answer, or discard each letter. A missing/empty
//! `Mail/` folder reads as an empty postbox, not an error. Runs on both writer
//! connections.

use rusqlite::Connection;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::db::character_vault::ensure_character_vault;
use crate::db::characters_read::find_by_id_raw;
use crate::db::vault_character_write::CharacterVaultWriteInput;
use crate::post_office::instructions::{format_letter_actions, format_letter_heading};
use crate::post_office::mailbox::list_mailbox;

const EMPTY_POSTBOX: &str = "Your postbox stands empty.";

/// v4 `ListEmailToolOutput` — `{ success, listing, count, error? }`. Serialized in
/// a fixed order; success omits `error`.
#[derive(Debug, Clone)]
pub struct ListEmailOutput {
    pub success: bool,
    pub listing: String,
    pub count: usize,
    pub error: Option<String>,
}

impl Serialize for ListEmailOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let len = if self.error.is_some() { 4 } else { 3 };
        let mut st = s.serialize_struct("ListEmailOutput", len)?;
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
fn fail(message: &str) -> ListEmailOutput {
    ListEmailOutput {
        success: false,
        listing: message.to_string(),
        count: 0,
        error: Some(message.to_string()),
    }
}

/// Execute the `list_email` tool (v4 `executeListEmailTool`). Runs on both writer
/// connections.
pub fn execute_list_email(
    main: &Connection,
    mount: &Connection,
    character_id: Option<&str>,
    args: &Value,
) -> ListEmailOutput {
    // v4 `validateListEmailInput` = `z.object({})` safeParse: any object is valid.
    if !args.is_object() {
        return fail("That request to the Post Office made no sense.");
    }
    let Some(character_id) = character_id.filter(|s| !s.is_empty()) else {
        return fail("Only a character keeps a postbox, and no character holds this one.");
    };

    let me = match find_by_id_raw(main, character_id) {
        Ok(Some(m)) => m,
        Ok(None) => return fail(
            "The Post Office cannot find your postbox; your character seems to have gone astray.",
        ),
        Err(e) => {
            return fail(&format!(
                "The Post Office stumbled and couldn't sort your post — {e}"
            ))
        }
    };

    // ensureCharacterVault(me).mountPointId — a provisioned character returns its
    // existing FK.
    let name = me.get("name").and_then(Value::as_str).unwrap_or_default();
    let fk = me
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str);
    let input: CharacterVaultWriteInput = serde_json::from_value(me.clone()).unwrap_or_default();
    let my_vault_id = match ensure_character_vault(main, mount, character_id, name, &input, fk) {
        Ok(r) => r.mount_point_id,
        Err(e) => {
            return fail(&format!(
                "The Post Office stumbled and couldn't sort your post — {e}"
            ))
        }
    };

    let letters = match list_mailbox(mount, &my_vault_id) {
        Ok(l) => l,
        Err(e) => {
            return fail(&format!(
                "The Post Office stumbled and couldn't sort your post — {e}"
            ))
        }
    };

    if letters.is_empty() {
        return ListEmailOutput {
            success: true,
            listing: EMPTY_POSTBOX.to_string(),
            count: 0,
            error: None,
        };
    }

    let n = letters.len();
    let header = format!(
        "Your postbox holds {n} letter{}, newest first. (Each letter shows its qtap://self/… URI; the \"self\" authority always addresses your own vault.)",
        if n == 1 { "" } else { "s" }
    );
    let blocks: Vec<String> = letters
        .iter()
        .enumerate()
        .map(|(i, letter)| {
            format!(
                "{}\n{}",
                format_letter_heading(letter, i + 1),
                format_letter_actions(&letter.path, &letter.from)
            )
        })
        .collect();

    ListEmailOutput {
        success: true,
        listing: format!("{header}\n\n{}", blocks.join("\n\n")),
        count: n,
        error: None,
    }
}

/// v4 `formatListEmailResults`: `success ? listing : error || listing`.
pub fn format_list_email_results(out: &ListEmailOutput) -> String {
    if out.success {
        out.listing.clone()
    } else {
        out.error.clone().unwrap_or_else(|| out.listing.clone())
    }
}
