//! Writer for ad-hoc announcement bubbles — v4 `lib/services/announcer/writer.ts`
//! (`postAdhocAnnouncement`), the Insert Announcement composer button.
//!
//! The operator may post a public bubble authored by:
//!   - a Staff member (canonical avatar + name),
//!   - a workspace character not currently in this chat, or
//!   - a free-text custom display name (placeholder avatar).
//!
//! By default the result is persisted to `chat_messages` as a broadcast
//! (`targetParticipantIds = null`), indistinguishable in behaviour from an
//! automated Staff announcement: visible to all participants (present and
//! silent), and included verbatim in every character's LLM transcript via normal
//! message history.
//!
//! When the operator names an audience, the same bubble is persisted as a
//! *whisper* — `targetParticipantIds` carries the chosen participant ids and the
//! ordinary whisper machinery takes over: only those participants' LLM contexts
//! include it (`filter_whisper_messages`), and the Salon shows it to the operator
//! with the usual whisper chrome. Callers must have already verified that every id
//! is a current participant of this chat
//! ([`super::audience::resolve_announcement_audience`]).

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::jsstr::js_trim;

/// v4 `StaffSender` (`writer.ts:21`) — the ten canonical Staff voices an ad-hoc
/// announcement may be signed with. Mirrors `STAFF_SENDER_ENUM`
/// (`app/api/v1/chats/[id]/schemas.ts:180`) exactly, including the `pascal`
/// entry added by v4 `979aec66` ("Pascal is selectable in the Insert
/// Announcement dialog").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffSender {
    Lantern,
    Aurora,
    Librarian,
    Concierge,
    Prospero,
    Host,
    CommonplaceBook,
    Ariel,
    Suparna,
    Pascal,
}

impl StaffSender {
    /// The persisted `systemSender` string — v4's enum member spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            StaffSender::Lantern => "lantern",
            StaffSender::Aurora => "aurora",
            StaffSender::Librarian => "librarian",
            StaffSender::Concierge => "concierge",
            StaffSender::Prospero => "prospero",
            StaffSender::Host => "host",
            StaffSender::CommonplaceBook => "commonplaceBook",
            StaffSender::Ariel => "ariel",
            StaffSender::Suparna => "suparna",
            StaffSender::Pascal => "pascal",
        }
    }
}

/// v4 `AnnouncerSender` (`writer.ts:33`) — the discriminated union of who the
/// bubble is signed by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnouncerSender {
    Staff { staff_id: StaffSender },
    Character { character_id: String },
    Custom { display_name: String },
}

/// v4 `AdhocAnnouncementParams`.
#[derive(Debug, Clone)]
pub struct AdhocAnnouncementParams {
    pub chat_id: String,
    /// Plain Markdown body of the announcement bubble.
    pub content_markdown: String,
    pub sender: AnnouncerSender,
    /// Participant ids the announcement is whispered to. `None` / empty posts a
    /// public broadcast (the historical behaviour). Every id must already have
    /// been verified as a current participant of this chat by the caller.
    pub target_participant_ids: Option<Vec<String>>,
}

/// Assemble the announcement `MessageEvent` (v4's object literal, in its
/// declaration order) plus the typed row the single writer inserts. `None` when
/// the assembled object fails to validate — v4's post would throw there and its
/// catch returns `null`.
///
/// `message_id` / `now_iso` are the minted `randomUUID()` / `new
/// Date().toISOString()`, lifted to parameters so the caller owns the clock.
fn build_announcement_message(
    params: &AdhocAnnouncementParams,
    trimmed: &str,
    message_id: &str,
    now_iso: &str,
) -> Option<(ChatEventInput, Value)> {
    // Normalize an empty audience to null so "public" has exactly one
    // representation on the row — every whisper check downstream tests for a
    // non-empty array, and a stored `[]` would read as "whispered to nobody".
    let targets: Value = match &params.target_participant_ids {
        Some(ids) if !ids.is_empty() => json!(ids),
        _ => Value::Null,
    };

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": trimmed,
        "attachments": [],
        "createdAt": now_iso,
        "participantId": null,
        "systemKind": "announcement",
        "targetParticipantIds": targets,
        "systemSender": match &params.sender {
            AnnouncerSender::Staff { staff_id } => Value::String(staff_id.as_str().to_string()),
            _ => Value::Null,
        },
        "customAnnouncer": match &params.sender {
            AnnouncerSender::Character { character_id } =>
                json!({ "kind": "character", "characterId": character_id }),
            AnnouncerSender::Custom { display_name } =>
                json!({ "kind": "custom", "displayName": display_name }),
            AnnouncerSender::Staff { .. } => Value::Null,
        },
    });
    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    Some((event, message))
}

/// Post a user-authored announcement bubble (v4 `postAdhocAnnouncement`).
/// Returns the persisted message (so callers can also surface it to the current
/// turn's LLM context without a one-turn lag), or `None` on failure / when
/// content is empty.
///
/// **Errors never propagate** — this matches the established Staff-announcer
/// convention (Host, Librarian, Lantern) and avoids tearing the composer UX over
/// a transient repo failure. (v4's own header, `writer.ts:45-52`; the `Ok`/`Err`
/// collapse into `Option` here is that contract in Rust.)
pub async fn post_adhoc_announcement(db: &Db, params: &AdhocAnnouncementParams) -> Option<Value> {
    let trimmed = js_trim(&params.content_markdown).to_string();
    if trimmed.is_empty() {
        return None;
    }

    // v4: `const chat = await repos.chats.findById(chatId); if (!chat) return null;`
    let cid = params.chat_id.clone();
    match db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        _ => return None,
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let (event, message) = build_announcement_message(params, &trimmed, &message_id, &now)?;

    let chat_id = params.chat_id.clone();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await
    {
        Ok(()) => Some(message),
        Err(_) => None,
    }
}

// === P4.D205 (v4 `e7d77bb60`, `announcer/writer.ts:190-211`) ===

/// Parameters for [`post_inform_record`].
pub struct InformRecordParams {
    pub chat_id: String,
    pub content_markdown: String,
    /// `None` (or empty) = the record is PUBLIC. The API decides this by
    /// COVERAGE, not by how the operator clicked: a list naming every eligible
    /// seat is public.
    pub target_participant_ids: Option<Vec<String>>,
}

/// Post the Host's transcript record for an inform (v4 `postInformRecord`).
///
/// The record documents, for the operator, that an out-of-character passage was
/// handed to a seat. It is NOT how the passage reaches the model — that is the
/// inform block — and every context path strips it
/// ([`crate::services::message_context`]'s record-only pass, the Courier's own
/// skip, and `extract_visible_conversation`'s).
///
/// `opaqueContent` mirrors `content` on purpose: the dual-body convention is
/// honoured, and there is simply no persona to strip.
///
/// Returns the persisted message, or `None` on empty content / unknown chat /
/// any failure. **Errors never propagate** — the established Staff-announcer
/// convention, and a lost record must not cost the operator the batch itself.
pub async fn post_inform_record(db: &Db, params: &InformRecordParams) -> Option<Value> {
    let trimmed = js_trim(&params.content_markdown).to_string();
    if trimmed.is_empty() {
        return None;
    }

    let cid = params.chat_id.clone();
    match db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        _ => {
            tracing::warn!(
                target: "quilltap::announcer",
                context = "announcer",
                chat_id = %params.chat_id,
                "[Announcer] Inform record skipped — unknown chat",
            );
            return None;
        }
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // Same normalization as the ad-hoc announcer: "public" has exactly one
    // representation on the row, because a stored `[]` reads as "whispered to
    // nobody" to every downstream whisper check.
    let targets: Value = match &params.target_participant_ids {
        Some(ids) if !ids.is_empty() => json!(ids),
        _ => Value::Null,
    };

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": trimmed,
        "opaqueContent": trimmed,
        "attachments": [],
        "createdAt": now,
        "participantId": null,
        "systemSender": "host",
        "systemKind": "inform",
        "targetParticipantIds": targets,
        "customAnnouncer": null,
    });
    // Every other exit from this function says why it took itself out; this one
    // used to return `None` in silence, leaving a vanished record with nothing
    // in the log to explain it. (v4 has no counterpart arm — it hands the object
    // straight to `addMessage` — so a failure here is a v5-side modelling
    // mismatch between the literal above and `ChatEventInput`, which is exactly
    // the thing worth naming.)
    let event: ChatEventInput = match serde_json::from_value(message.clone()) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(
                target: "quilltap::announcer",
                context = "announcer",
                chat_id = %params.chat_id,
                message_id = %message_id,
                error = %e,
                "[Announcer] Inform record could not be built",
            );
            return None;
        }
    };

    let chat_id = params.chat_id.clone();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await
    {
        Ok(()) => {
            tracing::debug!(
                target: "quilltap::announcer",
                context = "announcer",
                chat_id = %params.chat_id,
                message_id = %message_id,
                audience = if targets.is_null() { "public" } else { "whisper" },
                target_count = targets.as_array().map(Vec::len).unwrap_or(0),
                "[Announcer] Inform record posted",
            );
            Some(message)
        }
        Err(e) => {
            tracing::error!(
                target: "quilltap::announcer",
                context = "announcer",
                chat_id = %params.chat_id,
                error = %e,
                "[Announcer] Failed to post inform record",
            );
            None
        }
    }
}

// === end P4.D205 ===

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staff_sender_spellings_match_v4_enum() {
        // v4 STAFF_SENDER_ENUM (schemas.ts:180), in declaration order.
        let all = [
            StaffSender::Lantern,
            StaffSender::Aurora,
            StaffSender::Librarian,
            StaffSender::Concierge,
            StaffSender::Prospero,
            StaffSender::Host,
            StaffSender::CommonplaceBook,
            StaffSender::Ariel,
            StaffSender::Suparna,
            StaffSender::Pascal,
        ];
        let spellings: Vec<&str> = all.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            spellings,
            vec![
                "lantern",
                "aurora",
                "librarian",
                "concierge",
                "prospero",
                "host",
                "commonplaceBook",
                "ariel",
                "suparna",
                "pascal",
            ]
        );
    }

    fn build(sender: AnnouncerSender, targets: Option<Vec<String>>) -> Value {
        let params = AdhocAnnouncementParams {
            chat_id: "chat".into(),
            content_markdown: "  body  ".into(),
            sender,
            target_participant_ids: targets,
        };
        build_announcement_message(&params, "body", "mid", "2026-01-01T00:00:00.000Z")
            .expect("validates")
            .1
    }

    #[test]
    fn message_shape_per_sender_branch() {
        let mk = |sender: AnnouncerSender| build(sender, None);

        let staff = mk(AnnouncerSender::Staff {
            staff_id: StaffSender::CommonplaceBook,
        });
        assert_eq!(staff["systemSender"], json!("commonplaceBook"));
        assert_eq!(staff["customAnnouncer"], Value::Null);
        assert_eq!(staff["systemKind"], json!("announcement"));
        assert_eq!(staff["targetParticipantIds"], Value::Null);
        assert_eq!(staff["participantId"], Value::Null);
        assert_eq!(staff["role"], json!("ASSISTANT"));

        let character = mk(AnnouncerSender::Character {
            character_id: "cid".into(),
        });
        assert_eq!(character["systemSender"], Value::Null);
        assert_eq!(
            character["customAnnouncer"],
            json!({ "kind": "character", "characterId": "cid" })
        );

        let custom = mk(AnnouncerSender::Custom {
            display_name: "The Night Porter".into(),
        });
        assert_eq!(custom["systemSender"], Value::Null);
        assert_eq!(
            custom["customAnnouncer"],
            json!({ "kind": "custom", "displayName": "The Night Porter" })
        );
    }

    /// v4's `postAdhocAnnouncement` audience suite
    /// (`announcer-audience.test.ts`), at the one place the audience touches the
    /// row. An empty array must store NULL, not `[]`: every downstream visibility
    /// check treats `[]` as public anyway, but only by accident.
    #[test]
    fn empty_audience_stores_null_and_a_whisper_keeps_its_marker() {
        let staff = AnnouncerSender::Staff {
            staff_id: StaffSender::Host,
        };
        assert_eq!(
            build(staff.clone(), None)["targetParticipantIds"],
            Value::Null
        );
        assert_eq!(
            build(staff.clone(), Some(vec![]))["targetParticipantIds"],
            Value::Null
        );

        let whispered = build(staff, Some(vec!["p-alice".into(), "p-bob".into()]));
        assert_eq!(
            whispered["targetParticipantIds"],
            json!(["p-alice", "p-bob"])
        );
        // The operator-authored marker must survive a whisper — it is what tells
        // the Salon the human wrote this and must always see it.
        assert_eq!(whispered["systemKind"], json!("announcement"));
        assert_eq!(whispered["participantId"], Value::Null);

        // A custom display name whispers just as happily.
        let custom = build(
            AnnouncerSender::Custom {
                display_name: "A Distant Bell".into(),
            },
            Some(vec!["p-alice".into()]),
        );
        assert_eq!(custom["targetParticipantIds"], json!(["p-alice"]));
        assert_eq!(custom["systemSender"], Value::Null);
        assert_eq!(
            custom["customAnnouncer"],
            json!({ "kind": "custom", "displayName": "A Distant Bell" })
        );
    }
}

// === P4.D372 (v4 `db6190b44`, `announcer/writer.ts:250-337`) ===

/// Parameters for [`post_established_fact`] (v4 `EstablishedFactParams`).
pub struct EstablishedFactParams {
    pub chat_id: String,
    pub user_id: String,
    /// v4 `contentMarkdown?` — `None` reads as `''`.
    pub content_markdown: Option<String>,
}

/// Post a machine-delivered established fact (v4 `postEstablishedFact`): a
/// public Host row (`systemSender: 'host'`, `systemKind: 'established-fact'`,
/// no participant) whose body is the trimmed text in BOTH `content` and
/// `opaqueContent` — the frame is applied at read time and never stored
/// ([`crate::established_fact`]). Then, OUTSIDE the write's error arm, it
/// enqueues `ESTABLISHED_FACT_MEMORIES`, which admits the fact to every
/// non-removed LLM seat; an enqueue failure is a WARN and the post stands.
///
/// Returns the persisted message, or `None` on empty content (silently) /
/// unknown chat (WARN) / a failed write (ERROR). **Errors never propagate**
/// (the Staff-announcer convention). The realtime hint rides `add_message`'s
/// own publish, as every announcer post's does — no new publish site.
pub async fn post_established_fact(db: &Db, params: &EstablishedFactParams) -> Option<Value> {
    use crate::established_fact::ESTABLISHED_FACT_KIND;
    let trimmed = js_trim(params.content_markdown.as_deref().unwrap_or("")).to_string();
    if trimmed.is_empty() {
        return None;
    }

    let fail = |error: &str| {
        tracing::error!(
            target: "quilltap::announcer",
            context = "announcer",
            chatId = %params.chat_id,
            error = %error,
            "[Announcer] Failed to post established fact",
        );
    };

    let cid = params.chat_id.clone();
    match db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        Ok(None) => {
            tracing::warn!(
                target: "quilltap::announcer",
                context = "announcer",
                chatId = %params.chat_id,
                "[Announcer] Established fact skipped — unknown chat",
            );
            return None;
        }
        Err(e) => {
            fail(&crate::db::fallback::error_text(&e));
            return None;
        }
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    // v4's key order.
    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": trimmed,
        "opaqueContent": trimmed,
        "attachments": [],
        "createdAt": crate::clock::now_iso(),
        "participantId": null,
        "systemSender": "host",
        "systemKind": ESTABLISHED_FACT_KIND,
        "targetParticipantIds": null,
        "customAnnouncer": null,
    });
    let event: ChatEventInput = match serde_json::from_value(message.clone()) {
        Ok(e) => e,
        Err(e) => {
            fail(&e.to_string());
            return None;
        }
    };
    let chat_id = params.chat_id.clone();
    if let Err(e) = db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await
    {
        fail(&crate::db::fallback::error_text(&e));
        return None;
    }
    tracing::info!(
        target: "quilltap::announcer",
        context = "announcer",
        chatId = %params.chat_id,
        messageId = %message_id,
        "[Announcer] Established fact posted",
    );

    match crate::services::queue_service::enqueue_established_fact_memories(
        db,
        &params.user_id,
        &params.chat_id,
        &message_id,
    )
    .await
    {
        Ok(job_id) => tracing::debug!(
            target: "quilltap::announcer",
            context = "announcer",
            chatId = %params.chat_id,
            messageId = %message_id,
            jobId = %job_id,
            "[Announcer] Established-fact memory job enqueued",
        ),
        Err(e) => tracing::warn!(
            target: "quilltap::announcer",
            context = "announcer",
            chatId = %params.chat_id,
            messageId = %message_id,
            error = %crate::db::fallback::error_text(&e),
            "[Announcer] Could not enqueue the established-fact memory job; the fact stands",
        ),
    }
    Some(message)
}
// === end P4.D372 ===

// === P4.D372 ===
#[cfg(test)]
mod established_fact_writer_tests {
    use super::*;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c3720000-0000-4000-8000-0000000000e1";

    fn instance() -> (tempfile::TempDir, Db, tokio::runtime::Runtime) {
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
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID, "title": "Facts", "participants": [],
        }))
        .unwrap();
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: "2026-05-01T00:00:00.000Z".to_string(),
            updated_at: "2026-05-01T00:00:00.000Z".to_string(),
        };
        rt.block_on(db.write(move |w| {
            crate::db::chats::ChatsRepository::new(w.main().connection()).create(&create, &opts)
        }))
        .unwrap();
        (dir, db, rt)
    }

    fn params(chat: &str, content: Option<&str>) -> EstablishedFactParams {
        EstablishedFactParams {
            chat_id: chat.to_string(),
            user_id: crate::api::SINGLE_USER_ID.to_string(),
            content_markdown: content.map(str::to_string),
        }
    }

    fn jobs(db: &Db) -> Vec<(String, String)> {
        db.read_main(|c| {
            let mut s = c.prepare("SELECT type, payload FROM background_jobs")?;
            let rows = s
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
    }

    #[test]
    fn a_fact_posts_a_host_row_logs_and_enqueues_once() {
        let (_d, db, rt) = instance();
        let (msg, lines) = crate::test_support::captured_with(|| {
            rt.block_on(post_established_fact(
                &db,
                &params(CHAT, Some("  The bell is cracked.  ")),
            ))
        });
        let msg = msg.expect("posted");
        let keys: Vec<&str> = msg
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
                "customAnnouncer"
            ]
        );
        assert_eq!(msg["content"], "The bell is cracked.");
        assert_eq!(msg["opaqueContent"], "The bell is cracked.");
        let id = msg["id"].as_str().unwrap();
        let info = format!(
            "INFO quilltap::announcer [Announcer] Established fact posted context=announcer chatId={CHAT} messageId={id}"
        );
        assert!(lines.contains(&info), "{lines:#?}");
        let enq: Vec<_> = lines
            .iter()
            .filter(|l| l.contains("Established-fact memory job enqueued"))
            .collect();
        assert_eq!(enq.len(), 1, "{lines:#?}");
        assert!(enq[0].starts_with("DEBUG") && enq[0].contains(&format!("messageId={id} jobId=")));
        let j = jobs(&db);
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].0, "ESTABLISHED_FACT_MEMORIES");
        assert_eq!(
            j[0].1,
            format!(r#"{{"chatId":"{CHAT}","messageId":"{id}"}}"#)
        );
    }

    #[test]
    fn empty_is_silent_and_an_unknown_chat_warns_without_enqueue() {
        let (_d, db, rt) = instance();
        let (none, lines) = crate::test_support::captured_with(|| {
            rt.block_on(post_established_fact(
                &db,
                &params(CHAT, Some(" \u{feff} ")),
            ))
        });
        assert!(none.is_none());
        assert!(lines.is_empty(), "{lines:#?}");
        let (none, lines) = crate::test_support::captured_with(|| {
            rt.block_on(post_established_fact(&db, &params(CHAT, None)))
        });
        assert!(none.is_none() && lines.is_empty());
        let missing = "c3720000-0000-4000-8000-0000000000ff";
        let (none, lines) = crate::test_support::captured_with(|| {
            rt.block_on(post_established_fact(&db, &params(missing, Some("x"))))
        });
        assert!(none.is_none());
        assert_eq!(
            lines,
            vec![format!(
                "WARN quilltap::announcer [Announcer] Established fact skipped — unknown chat context=announcer chatId={missing}"
            )]
        );
        assert!(jobs(&db).is_empty(), "no enqueue on a refusal");
    }

    #[test]
    fn a_failed_enqueue_warns_and_the_fact_stands() {
        let (_d, db, rt) = instance();
        rt.block_on(db.write(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE background_jobs")?;
            Ok(())
        }))
        .unwrap();
        let (msg, lines) = crate::test_support::captured_with(|| {
            rt.block_on(post_established_fact(
                &db,
                &params(CHAT, Some("The bell is cracked.")),
            ))
        });
        let msg = msg.expect("the fact stands");
        assert!(
            lines.iter().any(|l| l.starts_with("WARN")
                && l.contains(
                    "Could not enqueue the established-fact memory job; the fact stands"
                )
                && l.contains(&format!("messageId={}", msg["id"].as_str().unwrap()))
                && !l.contains("sqlite error")),
            "{lines:#?}"
        );
        assert!(lines
            .iter()
            .any(|l| l.contains("Failed to check for existing jobs during enqueue")));
        let rows = db
            .read_main(|c| crate::db::chats_messages_read::get_messages(c, CHAT))
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
// === /P4.D372 ===
