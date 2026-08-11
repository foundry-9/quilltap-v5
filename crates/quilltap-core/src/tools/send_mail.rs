//! Port of v4's `send_mail` tool (`lib/tools/handlers/send-mail-handler.ts` +
//! `lib/tools/send-mail-tool.ts`).
//!
//! Delivers a letter from the acting character into the recipient character's
//! vault `Mail/` folder via the shared Post Office service. All failure modes
//! return an in-voice error string; the handler never throws for an expected
//! condition. Runs on both writer connections (reads `characters` on main + the
//! recipient vault store on mount, writes the mount store).
//!
//! `now_iso` (the delivery `sentAt`) is injected so the differential can pin it.

use rusqlite::Connection;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::db::character_resolver::resolve_character_by_name_or_id;
use crate::db::characters_read::find_by_id_raw;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::doc_edit::uri_producers::doc_store_uri_for;
use crate::post_office::deliver::{compose_and_deliver_letter, ComposeAndDeliverResult};

/// v4 `SendMailToolOutput` — `{ success, message, path?, error? }`. Serialized in a
/// fixed order (`success, message, path, error`) reproducing both branches'
/// `JSON.stringify`: success omits `error`, failure omits `path`.
#[derive(Debug, Clone)]
pub struct SendMailOutput {
    pub success: bool,
    pub message: String,
    pub path: Option<String>,
    pub error: Option<String>,
}

impl Serialize for SendMailOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut len = 2;
        if self.path.is_some() {
            len += 1;
        }
        if self.error.is_some() {
            len += 1;
        }
        let mut st = s.serialize_struct("SendMailOutput", len)?;
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
fn fail(message: &str) -> SendMailOutput {
    SendMailOutput {
        success: false,
        message: message.to_string(),
        path: None,
        error: Some(message.to_string()),
    }
}

/// v4 `validateSendMailInput`: `character` + `message` non-empty strings,
/// `in_reply_to` an optional non-empty string.
fn validate(args: &Value) -> bool {
    let Some(obj) = args.as_object() else {
        return false;
    };
    let nonempty_str = |k: &str| matches!(obj.get(k), Some(Value::String(s)) if !s.is_empty());
    if !nonempty_str("character") || !nonempty_str("message") {
        return false;
    }
    match obj.get("in_reply_to") {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    }
}

/// Execute the `send_mail` tool (v4 `executeSendMailTool`). Runs on both writer
/// connections. `now_iso` is the injected delivery timestamp.
#[allow(clippy::too_many_arguments)]
pub fn execute_send_mail(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    character_id: Option<&str>,
    args: &Value,
    now_iso: &str,
) -> SendMailOutput {
    if !validate(args) {
        return fail(
            "A letter wants both a recipient and words; one or the other arrived missing.",
        );
    }
    let Some(character_id) = character_id.filter(|s| !s.is_empty()) else {
        return fail("Only a character may post a letter, and no character holds this pen.");
    };
    let obj = args.as_object().expect("validated object");
    let recipient_token = obj.get("character").and_then(Value::as_str).unwrap_or("");
    let message = obj.get("message").and_then(Value::as_str).unwrap_or("");
    let in_reply_to = obj.get("in_reply_to").and_then(Value::as_str);

    let sender = match find_by_id_raw(main, character_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return fail(
                "The Post Office cannot find your own postbox; your character seems to have gone astray.",
            )
        }
        Err(e) => return fail(&format!("The Post Office stumbled and the letter went unsent — {e}")),
    };
    // An archived sender may not post (v4 `d553f72a`,
    // `send-mail-handler.ts:56`).
    if crate::api::characters::is_archived(&sender) {
        return fail("That character is archived; rehydrate it to continue.");
    }

    let recipient = match resolve_character_by_name_or_id(main, mount, user_id, recipient_token) {
        Ok(Some(r)) => r,
        Ok(None) => return fail("No soul by that name keeps a postbox here."),
        Err(e) => {
            return fail(&format!(
                "The Post Office stumbled and the letter went unsent — {e}"
            ))
        }
    };
    // …nor may an archived recipient receive. An exact-id lookup still
    // RESOLVES a tombstone (the resolver's deliberate asymmetry), which is
    // what makes this named refusal reachable instead of a bare "no soul by
    // that name" (v4 `send-mail-handler.ts:64`).
    if crate::api::characters::is_archived(&recipient) {
        return fail("That recipient is archived; rehydrate them to continue.");
    }

    let path = match compose_and_deliver_letter(
        main,
        mount,
        &sender,
        &recipient,
        message,
        in_reply_to,
        now_iso,
    ) {
        Ok(ComposeAndDeliverResult::Ok(p)) => p,
        Ok(ComposeAndDeliverResult::ReplyNotFound) => {
            return fail("That letter isn't in your own postbox, so there's nothing to reply to.")
        }
        Err(e) => {
            return fail(&format!(
                "The Post Office stumbled and the letter went unsent — {e}"
            ))
        }
    };

    // The confirmation addresses the RECIPIENT's postbox with its qtap:// URI (its
    // name, UUID fallback if ambiguous) — never qtap://self/. Falls back to the raw
    // path if the recipient's vault mount can't be resolved.
    let mut postbox_ref = path.clone();
    if let Some(mount_id) = recipient
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)
    {
        if let Ok(Some(mp)) = DocMountPointsRepository::new(mount).find_by_id_for_docedit(mount_id)
        {
            // v4 passes NO characterId → never resolves to self.
            let uri = doc_store_uri_for(main, mount, &mp.id, &mp.name, &path, None, None, None);
            if !uri.is_empty() {
                postbox_ref = uri;
            }
        }
    }

    let recipient_name = recipient.get("name").and_then(Value::as_str).unwrap_or("");
    SendMailOutput {
        success: true,
        message: format!(
            "Suparṇā has the letter in hand and is already winging it to {recipient_name}. It will rest in their postbox at {postbox_ref}."
        ),
        path: Some(path),
        error: None,
    }
}

/// v4 `formatSendMailResults`: `success ? message : error || message`.
pub fn format_send_mail_results(out: &SendMailOutput) -> String {
    if out.success {
        out.message.clone()
    } else {
        out.error.clone().unwrap_or_else(|| out.message.clone())
    }
}
