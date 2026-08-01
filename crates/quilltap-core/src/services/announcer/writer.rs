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
