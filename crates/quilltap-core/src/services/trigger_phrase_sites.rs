//! The two places Pascal listens for trigger phrases (v4
//! `lib/services/chat-message/trigger-phrase-sites.ts`, `362426c8f`).
//!
//! Only words someone said are scanned, and only at the two sites that already
//! run the auto-RNG scan: the operator's message (the orchestrator, after it is
//! persisted) and a character's reply (the finalizer, after it is persisted).
//! Each helper derives that site's speaker and room, then hands off to
//! [`run_triggered_custom_tools`], which owns the gate, the roster and the
//! deal. Neither helper fails.
//!
//! A swipe or regeneration never reaches either site (it runs through
//! `services::regenerate_swipe`), so a swipe fires nothing, matching the dice —
//! `trigger_sites_order_guard` pins that, and the two sites' positions.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::pascal::llm_consult::ConsultRunner;
use crate::pascal::trigger_runner::{
    run_triggered_custom_tools, RunTriggeredCustomToolsParams, TriggerSettings, TriggeredSpeaker,
};

/// Every character in the room — the roster's `participant` tier: the
/// participants whose `type` is `CHARACTER` and that carry a (truthy)
/// `characterId`, in stored order (v4 `roomCharacterIds`).
pub fn room_character_ids(participants: &[Value]) -> Vec<String> {
    participants
        .iter()
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
        .filter_map(|p| p.get("characterId").and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

/// The operator's message (v4 `UserMessageTriggerParams`).
pub struct UserMessageTriggerParams<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    /// The room's `projectId` (`None` when the chat has none).
    pub project_id: Option<&'a str>,
    /// The room's participants (for [`room_character_ids`]).
    pub participants: &'a [Value],
    /// The message as persisted; `""` when nothing was said.
    pub content: &'a str,
    pub is_continue_mode: bool,
    /// The seat the message is attributed to (the participant resolver's
    /// `user_participant`).
    pub user_participant: Option<&'a Value>,
    pub user_participant_id: Option<&'a str>,
    pub chat_settings: Option<TriggerSettings>,
    /// The spine's ONE clock reading for the message.
    pub now_ms: i64,
}

/// The operator's message (v4 `runUserMessageTriggers`). The seat speaks for
/// itself: a user-driven or impersonated character's words are that
/// character's, and its roster is theirs; the bare operator rolls as nobody.
/// Continue mode said nothing. Pascal's posts are spliced onto
/// `existing_messages` — this turn's in-memory context — so the responding
/// model sees what Pascal just said.
pub async fn run_user_message_triggers(
    db: &Db,
    params: UserMessageTriggerParams<'_>,
    consult: Option<&dyn ConsultRunner>,
    existing_messages: &mut Vec<Value>,
    on_posted: &mut (dyn FnMut(&Value) + Send),
) -> Vec<Value> {
    // `!params.content` — an EMPTY string; a whitespace-only one goes through
    // to the runner's `trim` gate.
    if params.is_continue_mode || params.content.is_empty() {
        return Vec::new();
    }

    let seat_character = params
        .user_participant
        .filter(|seat| seat.get("type").and_then(Value::as_str) == Some("CHARACTER"))
        .and_then(|seat| seat.get("characterId").and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let triggered = run_triggered_custom_tools(
        db,
        RunTriggeredCustomToolsParams {
            user_id: params.user_id.to_string(),
            chat_id: params.chat_id.to_string(),
            content: params.content.to_string(),
            speaker: TriggeredSpeaker {
                participant_id: params.user_participant_id.map(str::to_string),
                character_id: seat_character,
            },
            project_id: params.project_id.map(str::to_string),
            character_ids: room_character_ids(params.participants),
            chat_settings: params.chat_settings,
        },
        consult,
        params.now_ms,
        on_posted,
    )
    .await;

    // The responding model must see what Pascal just said — the same in-memory
    // splice the RNG TOOL rows and Carina's public answer rely on.
    existing_messages.extend(triggered.iter().cloned());
    triggered
}

/// A character's reply (v4 `ReplyTriggerParams`).
pub struct ReplyTriggerParams<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub project_id: Option<&'a str>,
    pub participants: &'a [Value],
    /// The reply as persisted.
    pub content: &'a str,
    pub character_participant_id: &'a str,
    pub character_id: &'a str,
    pub chat_settings: Option<TriggerSettings>,
    pub now_ms: i64,
}

/// A character's reply, from that character's own perspective (v4
/// `runReplyTriggers`). No in-memory splice: a chained next speaker re-reads
/// the transcript.
pub async fn run_reply_triggers(
    db: &Db,
    params: ReplyTriggerParams<'_>,
    consult: Option<&dyn ConsultRunner>,
    on_posted: &mut (dyn FnMut(&Value) + Send),
) -> Vec<Value> {
    if params.content.is_empty() {
        return Vec::new();
    }
    run_triggered_custom_tools(
        db,
        RunTriggeredCustomToolsParams {
            user_id: params.user_id.to_string(),
            chat_id: params.chat_id.to_string(),
            content: params.content.to_string(),
            speaker: TriggeredSpeaker {
                participant_id: Some(params.character_participant_id.to_string()),
                character_id: Some(params.character_id.to_string()),
            },
            project_id: params.project_id.map(str::to_string),
            character_ids: room_character_ids(params.participants),
            chat_settings: params.chat_settings,
        },
        consult,
        params.now_ms,
        on_posted,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// v4 `trigger-phrase-sites.test.ts`: a three-participant room with a
    /// PERSONA in the middle → `['char-1', 'char-2']`; a CHARACTER without a
    /// `characterId` is skipped.
    #[test]
    fn room_character_ids_skip_personas_and_unlinked_seats() {
        let participants = vec![
            json!({ "type": "CHARACTER", "characterId": "char-1" }),
            json!({ "type": "PERSONA", "characterId": "persona-1" }),
            json!({ "type": "CHARACTER", "characterId": "char-2" }),
            json!({ "type": "CHARACTER", "characterId": null }),
            json!({ "type": "CHARACTER", "characterId": "" }),
        ];
        assert_eq!(room_character_ids(&participants), vec!["char-1", "char-2"]);
    }

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A freshly provisioned instance, opened WITHOUT its mount-index
    /// partition — so any roster read the runner reached would fail LOUDLY
    /// (its outer WARN), which is what makes "nothing was read" observable.
    fn scratch_db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: None,
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (db, dir)
    }

    /// Continue mode and EMPTY content never reach the runner: nothing is
    /// spliced and nothing is posted, even with the setting on.
    #[tokio::test(flavor = "current_thread")]
    async fn continue_mode_and_empty_content_say_nothing() {
        crate::test_support::global_capture::install();
        let (db, _dir) = scratch_db();
        let ((), lines) = crate::test_support::global_capture::capture_async(async {
            let on = Some(TriggerSettings {
                custom_tools: Some(true),
                custom_tool_triggers: Some(true),
            });
            for (content, continue_mode) in [("abracadabra", true), ("", false)] {
                let mut existing = vec![json!({ "id": "earlier" })];
                let mut sink = |_: &Value| panic!("nothing may be posted");
                let posted = run_user_message_triggers(
                    &db,
                    UserMessageTriggerParams {
                        user_id: "u",
                        chat_id: "c",
                        project_id: None,
                        participants: &[],
                        content,
                        is_continue_mode: continue_mode,
                        user_participant: None,
                        user_participant_id: None,
                        chat_settings: on,
                        now_ms: 0,
                    },
                    None,
                    &mut existing,
                    &mut sink,
                )
                .await;
                assert!(posted.is_empty());
                assert_eq!(existing, vec![json!({ "id": "earlier" })]);
            }
            let mut sink = |_: &Value| panic!("nothing may be posted");
            let posted = run_reply_triggers(
                &db,
                ReplyTriggerParams {
                    user_id: "u",
                    chat_id: "c",
                    project_id: None,
                    participants: &[],
                    content: "",
                    character_participant_id: "p",
                    character_id: "char-1",
                    chat_settings: on,
                    now_ms: 0,
                },
                None,
                &mut sink,
            )
            .await;
            assert!(posted.is_empty());
        })
        .await;
        // The silence leg: nothing was read, so not even the runner's outer
        // WARN (which a roster read on this mount-less instance would raise).
        assert!(lines.is_empty(), "no line may be logged: {lines:?}");
    }

    /// The runner's OUTER arm (v4 `:77-87`), reachable here: the setting on and
    /// a phrase said, but the instance has no mount index, so the roster read
    /// fails — one WARN, nothing posted, no Err.
    #[tokio::test(flavor = "current_thread")]
    async fn a_failed_roster_read_is_the_one_outer_warn() {
        crate::test_support::global_capture::install();
        let (db, _dir) = scratch_db();
        let (posted, lines) = crate::test_support::global_capture::capture_async(async {
            let mut existing = Vec::new();
            let mut sink = |_: &Value| panic!("nothing may be posted");
            run_user_message_triggers(
                &db,
                UserMessageTriggerParams {
                    user_id: "u",
                    chat_id: "c",
                    project_id: None,
                    participants: &[],
                    content: "abracadabra",
                    is_continue_mode: false,
                    user_participant: None,
                    user_participant_id: None,
                    chat_settings: Some(TriggerSettings {
                        custom_tools: None,
                        custom_tool_triggers: Some(true),
                    }),
                    now_ms: 0,
                },
                None,
                &mut existing,
                &mut sink,
            )
            .await
        })
        .await;
        assert!(posted.is_empty());
        let warns: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("WARN quilltap::pascal Trigger-phrase scan failed"))
            .collect();
        assert_eq!(warns.len(), 1, "{lines:?}");
        assert!(
            warns[0].starts_with(
                "WARN quilltap::pascal Trigger-phrase scan failed; no custom tools fired \
                 context=pascal.trigger-runner chatId=c speakerParticipantId=null error="
            ),
            "{}",
            warns[0]
        );
    }
}
