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

/// v4 `writer.ts`'s `CONTEXT` (`:43`).
const CONTEXT: &str = "pascal";

/// The dual body of an outcome announcement (v4 `buildPascalResultContent`'s
/// return). Both strings are the same — see the note on opacity in the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PascalResultContent {
    pub content: String,
    pub opaque_content: String,
}

/// Build the body of an outcome announcement: a heading line naming the run,
/// then the author's own message as its own paragraph. Deliberately knows
/// nothing about the roll — a table that wants its number read out says so with
/// `{{value}}`.
///
/// `chip_label` is the definition's rendered `chipLabel` when it declares one —
/// the header uses it, the same string as the chip, so transcript and chip never
/// disagree. Blank (after trimming) or absent falls back to the title.
///
/// The blank line is load-bearing: it makes the message its own Markdown block,
/// so an outcome that begins with `- `, `#`, `1.`, `>`, or a fence renders as
/// the list/heading/quote its author wrote instead of gluing inline to the bold
/// heading. Messages posted before v4 `c4d4b0de` keep their one-line bodies —
/// content is frozen at post time, which is correct for a record, so nothing
/// re-formats an old row.
pub fn build_pascal_result_content(
    tool_title: &str,
    chip_label: Option<&str>,
    message: &str,
) -> PascalResultContent {
    // v4 `params.chipLabel?.trim() || params.toolTitle` — an all-whitespace
    // label is falsy after trimming and falls back to the title.
    let heading = chip_label
        .map(js_trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(tool_title);
    let body = format!("🎲 **{heading}**\n\n{}", js_trim(message));
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
    /// The tool's `announceAsCharacter` (v4 `a05c6f755`): the rolling character
    /// whose name and face the bubble wears instead of Pascal's, through
    /// `customAnnouncer` — the one field the Salon, the LLM context's speaker
    /// tag, the Courier and the transcript export already read for "who is
    /// speaking this". The row stays Pascal's (`systemSender`, `pascalMeta`), so
    /// nothing that keys on a Pascal outcome — the chip, the trigger scan's
    /// blindness to it — changes. `None`/empty = Pascal announces it.
    pub announcer_character_id: Option<String>,
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
    try_build_pascal_message(params).ok()
}

/// [`build_pascal_message`] with the validation failure kept — the ERROR line's
/// `error` (v4's `addMessage` Zod throw, caught by `postPascalResult`).
fn try_build_pascal_message(
    params: &PostPascalResultParams,
) -> Result<(ChatEventInput, PostedPascalMessage), String> {
    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // v4: `targetParticipantIds && targetParticipantIds.length ? … : null`.
    let targets: Option<Vec<String>> = params
        .target_participant_ids
        .clone()
        .filter(|t| !t.is_empty());

    let mut message = json!({
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
    // v4's truthy spread, LAST (after `pascalMeta`), key order `kind,
    // characterId`.
    if let Some(id) = params
        .announcer_character_id
        .as_deref()
        .filter(|id| !id.is_empty())
    {
        message["customAnnouncer"] = json!({ "kind": "character", "characterId": id });
    }

    let event: ChatEventInput =
        serde_json::from_value(message.clone()).map_err(|e| e.to_string())?;
    Ok((
        event,
        PostedPascalMessage {
            id: message_id,
            message,
            target_participant_ids: targets,
        },
    ))
}

/// Persist a Pascal outcome announcement (v4 `postPascalResult`). Guards on the
/// chat existing, then writes best-effort — a failure → `None`, matching v4's
/// catch-and-return-null. Returns the posted message on success.
///
/// v4's three lines (P4.D319 R-I — v5 was silent until then): the `dropped`
/// DEBUG when there is no such chat, the INFO after a successful post (now
/// carrying `announcerCharacterId`), and the ERROR for anything inside v4's ONE
/// `try` that throws. v4's `chats.findById` is a `safeQuery` FALLBACK — an
/// unreadable row logs the repository's ERROR and answers null — so it takes the
/// `dropped` arm, never the catch; the catch's v5 homes are a failed read
/// connection, a message that fails to validate (v4: `addMessage`'s Zod throw)
/// and a failed write. Every line is emitted on the caller's task, after the
/// `.await`, never inside the writer closure.
pub async fn post_pascal_result(
    db: &Db,
    params: PostPascalResultParams,
) -> Option<PostedPascalMessage> {
    let tool = params
        .pascal_meta
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let failed = |error: &str| {
        tracing::error!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = params.chat_id.as_str(),
            tool = tool.as_str(),
            error = error,
            "[Pascal] Failed to post custom-tool outcome",
        );
    };

    // v4: `const chat = await repos.chats.findById(chatId); if (!chat) { … }`
    let cid = params.chat_id.clone();
    match db.read_main(move |c| Ok(crate::db::chats_read::find_by_id_or_none(c, &cid))) {
        Ok(Some(_)) => {}
        Ok(None) => {
            tracing::debug!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = params.chat_id.as_str(),
                tool = tool.as_str(),
                "Pascal outcome dropped — no such chat",
            );
            return None;
        }
        Err(error) => {
            failed(&crate::db::fallback::error_text(&error));
            return None;
        }
    }

    let (event, posted) = match try_build_pascal_message(&params) {
        Ok(built) => built,
        Err(error) => {
            failed(&error);
            return None;
        }
    };
    let chat_id = params.chat_id.clone();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await
    {
        Ok(()) => {
            let state = params
                .pascal_meta
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default();
            tracing::info!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = params.chat_id.as_str(),
                messageId = posted.id.as_str(),
                tool = tool.as_str(),
                state = state,
                whispered = posted.target_participant_ids.is_some(),
                announcerCharacterId = params
                    .announcer_character_id
                    .as_deref()
                    .unwrap_or("null"),
                "[Pascal] Custom-tool outcome posted",
            );
            Some(posted)
        }
        Err(error) => {
            failed(&crate::db::fallback::error_text(&error));
            None
        }
    }
}

// === P4.D324 (v4 `4fbdad186`, `writer.ts:185-228`) ===

/// v4 `PASCAL_INVENTORY_CHANGE_KIND` — the `systemKind` of Pascal's plain
/// Inventory line.
pub const PASCAL_INVENTORY_CHANGE_KIND: &str = "inventory-change";

/// Post Pascal's plain line for an Inventory gesture (v4
/// `postPascalInventoryChange`): *"Vivienne takes up Excalibur."* The row
/// carries NO `pascalMeta` (the key absent — it is not a roll, and nothing
/// keyed on a roll record may treat it as one). Guards on the chat existing;
/// every failure → `None` (v4's catch). The DEBUG `Pascal inventory line
/// dropped — no such chat` (U+2014), the INFO `[Pascal] Inventory change
/// posted` and the ERROR `[Pascal] Failed to post inventory change` all log
/// on the caller's task after the `.await` (the [`post_pascal_result`]
/// convention). The describer that builds `content` is P4.D328's.
pub async fn post_pascal_inventory_change(
    db: &Db,
    chat_id: &str,
    content: &str,
) -> Option<PostedPascalMessage> {
    let failed = |error: &str| {
        tracing::error!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            error = error,
            "[Pascal] Failed to post inventory change",
        );
    };
    let cid = chat_id.to_string();
    match db.read_main(move |c| Ok(crate::db::chats_read::find_by_id_or_none(c, &cid))) {
        Ok(Some(_)) => {}
        Ok(None) => {
            tracing::debug!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = chat_id,
                "Pascal inventory line dropped — no such chat",
            );
            return None;
        }
        Err(error) => {
            failed(&crate::db::fallback::error_text(&error));
            return None;
        }
    }
    let message_id = uuid::Uuid::new_v4().to_string();
    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": content,
        "attachments": [],
        "createdAt": crate::clock::now_iso(),
        "participantId": null,
        "systemSender": "pascal",
        "systemKind": PASCAL_INVENTORY_CHANGE_KIND,
        "targetParticipantIds": null,
    });
    let event: ChatEventInput = match serde_json::from_value(message.clone()) {
        Ok(e) => e,
        Err(error) => {
            failed(&error.to_string());
            return None;
        }
    };
    let cid = chat_id.to_string();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&cid, &event))
        .await
    {
        Ok(()) => {
            tracing::info!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = chat_id,
                messageId = message_id.as_str(),
                "[Pascal] Inventory change posted",
            );
            Some(PostedPascalMessage {
                id: message_id,
                message,
                target_participant_ids: None,
            })
        }
        Err(error) => {
            failed(&crate::db::fallback::error_text(&error));
            None
        }
    }
}

// === end P4.D324 ===

#[cfg(test)]
mod post_tests {
    //! P4.D319 (v4 `a05c6f755`, R-I): v4's two `writer.test.ts`
    //! `postPascalResult` `it`s, and the writer's three lines — each pinned on
    //! the caller's thread (they log after the `.await`, never inside the
    //! writer closure), each with its silence leg.
    use super::*;
    use crate::test_support::global_capture;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c2000000-0000-4000-8000-000000000001";

    fn db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (db, dir)
    }

    async fn seed_chat(db: &Db) {
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID,
            "title": "Pascal's Table",
            "participants": [],
        }))
        .expect("a ChatCreate");
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: "2026-05-01T00:00:00.000Z".to_string(),
            updated_at: "2026-05-01T00:00:00.000Z".to_string(),
        };
        db.write(move |w| {
            crate::db::chats::ChatsRepository::new(w.main().connection()).create(&create, &opts)
        })
        .await
        .expect("seed the chat");
    }

    fn params(announcer: Option<&str>, targets: Option<Vec<String>>) -> PostPascalResultParams {
        let body = build_pascal_result_content("Coin", None, "Heads.");
        PostPascalResultParams {
            chat_id: CHAT.to_string(),
            content: body.content,
            opaque_content: body.opaque_content,
            pascal_meta: json!({
                "tool": "coin", "toolTitle": "Coin", "definitionTier": "character",
                "definitionMountId": "m1", "params": {}, "rollForm": "range", "raw": 1,
                "value": 1, "state": "success", "outcomeIndex": 0, "invokedBy": "llm"
            }),
            target_participant_ids: targets,
            announcer_character_id: announcer.map(str::to_string),
        }
    }

    fn pascal_lines(lines: Vec<String>) -> Vec<String> {
        lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::pascal "))
            .collect()
    }

    /// v4 `posts as Pascal, with no announcer, by default` — and the INFO line
    /// with `announcerCharacterId=null` (v4's `?? null`).
    #[tokio::test(flavor = "current_thread")]
    async fn posts_as_pascal_with_no_announcer_by_default() {
        global_capture::install();
        let (db, _dir) = db();
        seed_chat(&db).await;
        let (posted, lines) =
            global_capture::capture_async(post_pascal_result(&db, params(None, None))).await;
        let posted = posted.expect("posted");
        assert!(posted.message.get("customAnnouncer").is_none());
        assert_eq!(posted.message["systemSender"], "pascal");
        assert_eq!(
            pascal_lines(lines),
            vec![format!(
                "INFO quilltap::pascal [Pascal] Custom-tool outcome posted context=pascal \
                 chatId={CHAT} messageId={} tool=coin state=success whispered=false \
                 announcerCharacterId=null",
                posted.id
            )]
        );
    }

    /// v4 `wears the announcing character's name and face, and stays Pascal's
    /// record` — `customAnnouncer` LAST, `kind, characterId`; `systemSender`,
    /// `systemKind` and `pascalMeta` unchanged; the INFO names the announcer.
    #[tokio::test(flavor = "current_thread")]
    async fn wears_the_announcing_character_and_stays_pascals_record() {
        global_capture::install();
        let (db, _dir) = db();
        seed_chat(&db).await;
        let (posted, lines) = global_capture::capture_async(post_pascal_result(
            &db,
            params(Some("char-1"), Some(vec!["p1".into()])),
        ))
        .await;
        let posted = posted.expect("posted");
        let keys: Vec<&str> = posted
            .message
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "type",
                "id",
                "role",
                "content",
                "opaqueContent",
                "attachments",
                "createdAt",
                "participantId",
                "systemSender",
                "systemKind",
                "targetParticipantIds",
                "pascalMeta",
                "customAnnouncer"
            ]
        );
        assert_eq!(
            serde_json::to_string(&posted.message["customAnnouncer"]).unwrap(),
            r#"{"kind":"character","characterId":"char-1"}"#
        );
        assert_eq!(posted.message["systemSender"], "pascal");
        assert_eq!(posted.message["systemKind"], "custom-tool-result");
        assert_eq!(posted.message["pascalMeta"]["tool"], "coin");
        assert_eq!(
            pascal_lines(lines),
            vec![format!(
                "INFO quilltap::pascal [Pascal] Custom-tool outcome posted context=pascal \
                 chatId={CHAT} messageId={} tool=coin state=success whispered=true \
                 announcerCharacterId=char-1",
                posted.id
            )]
        );
    }

    /// An empty announcer id is v4's falsy spread: no key.
    #[test]
    fn an_empty_announcer_adds_no_key() {
        let (_, posted) = build_pascal_message(&params(Some(""), None)).expect("builds");
        assert!(posted.message.get("customAnnouncer").is_none());
    }

    /// No such chat: v4's `dropped` DEBUG — and nothing else (no INFO).
    #[tokio::test(flavor = "current_thread")]
    async fn no_such_chat_drops_with_the_debug_alone() {
        global_capture::install();
        let (db, _dir) = db();
        let (posted, lines) =
            global_capture::capture_async(post_pascal_result(&db, params(None, None))).await;
        assert!(posted.is_none());
        assert_eq!(
            pascal_lines(lines),
            vec![format!(
                "DEBUG quilltap::pascal Pascal outcome dropped — no such chat context=pascal \
                 chatId={CHAT} tool=coin"
            )]
        );
    }

    /// A read connection that cannot open is inside v4's one `try`: the ERROR,
    /// with the bare message — and neither the DEBUG nor the INFO. A fresh `Db`
    /// has opened no read connection yet, so removing the main file makes the
    /// first checkout fail.
    #[tokio::test(flavor = "current_thread")]
    async fn an_unreadable_chat_read_logs_the_error() {
        global_capture::install();
        let (db, dir) = db();
        std::fs::remove_file(dir.path().join("data").join("quilltap.db")).unwrap();
        let (posted, lines) =
            global_capture::capture_async(post_pascal_result(&db, params(None, None))).await;
        assert!(posted.is_none());
        let lines = pascal_lines(lines);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(&format!(
                "ERROR quilltap::pascal [Pascal] Failed to post custom-tool outcome \
                 context=pascal chatId={CHAT} tool=coin error="
            )),
            "{}",
            lines[0]
        );
        assert!(
            !lines[0].contains("sqlite error"),
            "the bare message: {}",
            lines[0]
        );
    }
}
