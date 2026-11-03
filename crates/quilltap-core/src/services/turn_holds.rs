//! Turn holds — when an operator's post is recorded but no character answers it
//! (v4 `lib/services/chat-message/turn-holds.service.ts`, `a8214b253`, at
//! `236c04e35`).
//!
//! Three rules can hold a post:
//!
//! - **Paused chat.** A paused conversation never moves on its own. The post is
//!   recorded in full and draws no reply until the operator nudges a character
//!   or presses Resume ([`super::paused_hold`], bug 137).
//! - **Storyteller.** In a storyteller chat a public post opens a round of bids
//!   instead of drawing a reply; the spine opens it (it reads
//!   [`ProcessMessageResult::storyteller_hold`]).
//! - **Fair rotation.** In a room where the human drives two or more seats, if
//!   the rotation's next speaker after this post is *another* of those seats,
//!   the floor is theirs (bug 50). That one decides and records in one step
//!   (`orchestrator::maybe_pause_for_user_seat_turn`); this module only says
//!   whether a send qualifies for the check.
//!
//! The first two are decided before the turn is built ([`resolve_turn_hold`])
//! and finished after the post is recorded ([`finish_held_post`]). All three
//! record the post through [`super::operator_post::record_operator_post`], so a
//! held post is the same post an answered one is (bug 262).
//!
//! v5 record (P4.D348): `orchestrator::finish_held_user_turn` moved here as
//! [`finish_held_post`] (deleted, not wrapped), gaining the storyteller arm.

use serde_json::Value;

use super::chat_events::{ChainCompletePayload, ChatEvent, EventSink};
use super::message_finalizer::{ProcessMessageResult, StorytellerHold};
use super::orchestrator::SendMessageOptions;
use super::paused_hold::{should_hold_user_turn_for_pause, PausedHoldInput};
use super::whisper_send::is_operator_whisper_send;
use crate::db::runtime::Db;
use crate::db::DbError;

/// Why a recorded post draws no reply (v4 `TurnHold`; its `null` arm is
/// `Option::None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnHold {
    Paused,
    Storyteller,
}

impl TurnHold {
    /// v4's string value (the `hold` field of the recording INFO).
    pub fn as_str(self) -> &'static str {
        match self {
            TurnHold::Paused => "paused",
            TurnHold::Storyteller => "storyteller",
        }
    }
}

/// Decide whether this send's post is held (pure; v4 `resolveTurnHold`).
///
/// Continue-mode turns are the explicit summons themselves — Nudge, Skip, the
/// all-LLM modal's Continue, an autonomous-room turn — so they are never held;
/// the chain that would follow one is what `execute_turn_chain` stops while
/// `isPaused` stands. In a storyteller chat a whisper still draws its one reply
/// (bug 258), and an autonomous room never runs in that mode.
pub fn resolve_turn_hold(chat: &Value, options: &SendMessageOptions) -> Option<TurnHold> {
    if should_hold_user_turn_for_pause(PausedHoldInput {
        is_continue_mode: options.continue_mode,
        chat_is_paused: chat.get("isPaused").and_then(Value::as_bool) == Some(true),
        never_pause_for_user: Some(options.never_pause_for_user),
    }) {
        return Some(TurnHold::Paused);
    }
    if !options.continue_mode
        && chat.get("turnMode").and_then(Value::as_str) == Some("storyteller")
        && !is_operator_whisper_send(
            options.continue_mode,
            options.target_participant_ids.as_deref(),
        )
        && !options.never_pause_for_user
    {
        return Some(TurnHold::Storyteller);
    }
    None
}

/// Whether a send is one the fair-rotation guard should look at (pure; v4
/// `qualifiesForFairRotationCheck`). A whisper, a nudge, an explicit responding
/// participant or a continue-mode turn name their own speaker; a held post is
/// already held (and a storyteller round asks every LLM seat at once); a
/// single-character chat has no rotation.
pub fn qualifies_for_fair_rotation_check(
    participants: &[Value],
    options: &SendMessageOptions,
    hold: Option<TurnHold>,
) -> bool {
    !options.continue_mode
        && hold.is_none()
        // v4 `!options.respondingParticipantId` — an empty id is no responder
        // (the inline guard this lifts tested `is_none()`; the grid's
        // `responding=empty` rows measured the difference).
        && options
            .responding_participant_id
            .as_deref()
            .is_none_or(str::is_empty)
        && !is_operator_whisper_send(
            options.continue_mode,
            options.target_participant_ids.as_deref(),
        )
        && options.nudge != Some(true)
        && !options.content.is_empty()
        && {
            let filter_parts: Vec<crate::participant_filters::ParticipantView> = participants
                .iter()
                .map(super::participant_resolver::to_filter_participant)
                .collect();
            crate::participant_filters::is_multi_character_chat(&filter_parts)
        }
}

/// The inputs to [`finish_held_post`] (v4 `finishHeldPost`'s params).
pub struct FinishHeldPostParams<'a> {
    pub chat_id: &'a str,
    pub hold: TurnHold,
    pub is_multi_character: bool,
    pub user_message_id: Option<String>,
    pub user_participant_id: Option<String>,
    /// What the operator typed (`""` for an attachments-only post) — the
    /// storyteller arm asks whether it is an `@Name:` query.
    pub content: &'a str,
}

/// Close out a post held by a paused chat or a storyteller round, once it is
/// recorded (v4 `finishHeldPost`).
///
/// Nobody is on deck either way: the rotation resumes from wherever the user
/// left it, and a stale pending seat would have the Salon announce a turn that
/// isn't coming.
///
/// - A paused hold emits the `paused` chain-complete — the same event a
///   mid-chain pause emits, so the client reconciles its pause flag through the
///   one path it already has (bug 123); it just arrives at depth 0, before any
///   character has spoken.
/// - A storyteller hold emits nothing: the spine opens the round, which holds
///   the stream. An `@Name:` query is answered as in every mode and opens no
///   round.
pub async fn finish_held_post<SNK: EventSink + Sync>(
    db: &Db,
    sink: &SNK,
    params: FinishHeldPostParams<'_>,
) -> Result<ProcessMessageResult, DbError> {
    super::turn_orchestrator::write_last_turn_participant_id(db, params.chat_id, None).await?;

    // v4's field is `string | null`; v5's is a bare `String`, so an absent id
    // reads as `""` — the same mapping the courier and fair-rotation returns
    // already use.
    let message_id = params.user_message_id.clone().unwrap_or_default();

    if params.hold == TurnHold::Storyteller {
        return Ok(ProcessMessageResult {
            is_multi_character: params.is_multi_character,
            has_content: false,
            message_id,
            user_participant_id: params.user_participant_id,
            is_paused: false,
            scene_tracking_character_ids: None,
            skipped: false,
            skipped_participant_id: None,
            yielded: false,
            storyteller_hold: Some(StorytellerHold {
                trigger_message_id: params.user_message_id,
                carina: !params.content.is_empty()
                    && crate::carina_parser::parse_carina_query(Some(params.content)).is_some(),
            }),
            bid: None,
        });
    }

    sink.emit(ChatEvent::chain_complete(ChainCompletePayload {
        reason: "paused".to_string(),
        next_speaker_id: None,
        chain_depth: 0,
        paused: Some(true),
        // The ONE site in the whole crate that sets this key
        // (`only_one_site_sets_held_user_turn` in `chat_events.rs` is the census).
        held_user_turn: Some(true),
    }));

    Ok(ProcessMessageResult {
        is_multi_character: params.is_multi_character,
        // No character took the floor, so the turn chain stops before it starts
        // and neither scene tracking nor the Scriptorium render fires for this
        // message.
        has_content: false,
        message_id,
        user_participant_id: params.user_participant_id,
        is_paused: true,
        scene_tracking_character_ids: None,
        skipped: false,
        skipped_participant_id: None,
        yielded: false,
        storyteller_hold: None,
        bid: None,
    })
}

#[cfg(test)]
mod tests {
    //! The differential over v4's REAL `resolveTurnHold` /
    //! `qualifiesForFairRotationCheck` is the harness's `turn_holds_equivalence`;
    //! these pin the storyteller arm of [`finish_held_post`] (P4.D343's tier-3
    //! row proves it live).
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::services::chat_events::RecordingSink;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c1000000-0000-4000-8000-00000000d348";
    const NOW: &str = "2026-11-02T00:00:00.000Z";

    fn opts(content: &str) -> SendMessageOptions {
        SendMessageOptions {
            content: content.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn paused_wins_over_storyteller() {
        let chat = json!({"isPaused": true, "turnMode": "storyteller"});
        assert_eq!(
            resolve_turn_hold(&chat, &opts("hi")),
            Some(TurnHold::Paused)
        );
    }

    #[test]
    fn a_whisper_in_a_storyteller_chat_is_not_held() {
        let chat = json!({"turnMode": "storyteller"});
        let mut o = opts("hi");
        o.target_participant_ids = Some(vec!["p".into()]);
        assert_eq!(resolve_turn_hold(&chat, &o), None);
        o.target_participant_ids = Some(vec![]);
        assert_eq!(resolve_turn_hold(&chat, &o), Some(TurnHold::Storyteller));
    }

    #[test]
    fn hold_strings_are_v4s() {
        assert_eq!(TurnHold::Paused.as_str(), "paused");
        assert_eq!(TurnHold::Storyteller.as_str(), "storyteller");
    }

    async fn db_with_chat() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("rs");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID,
            "title": "The Held Room",
            "participants": [],
        }))
        .unwrap();
        let copts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: NOW.to_string(),
            updated_at: NOW.to_string(),
        };
        db.write(move |w| {
            crate::db::chats::ChatsRepository::new(w.main().connection())
                .create(&create, &copts)
                .map(|_| ())
        })
        .await
        .unwrap();
        (dir, db)
    }

    async fn storyteller_result(content: &str) -> ProcessMessageResult {
        let (_dir, db) = db_with_chat().await;
        let sink = RecordingSink::default();
        let result = finish_held_post(
            &db,
            &sink,
            FinishHeldPostParams {
                chat_id: CHAT,
                hold: TurnHold::Storyteller,
                is_multi_character: true,
                user_message_id: Some("m-1".into()),
                user_participant_id: Some("p-1".into()),
                content,
            },
        )
        .await
        .unwrap();
        assert!(
            sink.events_json().is_empty(),
            "a storyteller hold emits no frame"
        );
        result
    }

    #[tokio::test]
    async fn the_storyteller_arm_marks_an_at_name_query() {
        let r = storyteller_result("@Vergil: what time is it?").await;
        assert_eq!(
            r.storyteller_hold,
            Some(StorytellerHold {
                trigger_message_id: Some("m-1".into()),
                carina: true
            })
        );
        assert!(!r.is_paused);
        assert!(!r.has_content);
        assert_eq!(r.message_id, "m-1");
    }

    #[tokio::test]
    async fn the_storyteller_arm_on_plain_or_empty_text() {
        for content in ["plain words", ""] {
            let r = storyteller_result(content).await;
            assert_eq!(
                r.storyteller_hold.map(|h| h.carina),
                Some(false),
                "{content:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_paused_arm_emits_the_held_chain_complete() {
        let (_dir, db) = db_with_chat().await;
        let sink = RecordingSink::default();
        let r = finish_held_post(
            &db,
            &sink,
            FinishHeldPostParams {
                chat_id: CHAT,
                hold: TurnHold::Paused,
                is_multi_character: false,
                user_message_id: None,
                user_participant_id: None,
                content: "x",
            },
        )
        .await
        .unwrap();
        assert!(r.is_paused);
        assert_eq!(r.storyteller_hold, None);
        let frames = sink.events_json();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["heldUserTurn"], json!(true));
    }
}
