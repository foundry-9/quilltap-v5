//! Continue-in-new-chat ("change of venue") backfill — v4's
//! `lib/chat/apply-chat-continuation.ts`.
//!
//! When a chat is forked into a fresh one, [`apply_chat_continuation`]:
//!   1. posts a Host "continuation-from" bubble in the new chat,
//!   2. replays the carryover window (the most recent Librarian summary plus
//!      every later message) into the new chat, remapping participant ids by
//!      shared `characterId` and stripping old-chat-lifecycle fields,
//!      3. replicates turn state (isPaused / turnQueue / lastTurnParticipantId /
//!      activeTypingParticipantId / impersonatingParticipantIds /
//!      allLLMPauseTurnCount / spokenThisCycle) with the same remap, and
//!   4. posts a Host "continuation-to" tail bubble in the SOURCE chat (last, so
//!      we never link to a chat that failed to populate).
//!
//! Errors during backfill are logged, not fatal (the create handler's
//! try/catch). Composes the ported Host continuation writers + the single-writer
//! message/update path; mints message ids + `createdAt` per replayed row (v4
//! `randomUUID()` / `new Date().toISOString()`), so the differential is the
//! minted-values remap form.

use serde_json::{json, Value};

use crate::clock::now_iso;
use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, DbError};
use crate::services::host_notifications::{
    post_host_continuation_from_announcement, post_host_continuation_to_announcement,
    HostContinuationFromAnnouncement, HostContinuationToAnnouncement,
};

/// v4 `ApplyChatContinuationResult`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApplyChatContinuationResult {
    pub replayed_message_count: usize,
    pub had_librarian_summary: bool,
    pub posted_source_tail_bubble: bool,
}

/// v4 `buildParticipantIdMap`: old participant id → new participant id, keyed by
/// shared `characterId`. Old participants whose character isn't in the new chat
/// are absent (their messages drop during replay).
fn build_participant_id_map(
    source_chat: &Value,
    new_chat: &Value,
) -> std::collections::HashMap<String, String> {
    let mut new_by_char: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Some(ps) = new_chat.get("participants").and_then(Value::as_array) {
        for p in ps {
            if let (Some(cid), Some(id)) = (
                p.get("characterId").and_then(Value::as_str),
                p.get("id").and_then(Value::as_str),
            ) {
                new_by_char.insert(cid.to_string(), id.to_string());
            }
        }
    }
    let mut map = std::collections::HashMap::new();
    if let Some(ps) = source_chat.get("participants").and_then(Value::as_array) {
        for p in ps {
            let Some(cid) = p.get("characterId").and_then(Value::as_str) else {
                continue;
            };
            if let (Some(old_id), Some(new_id)) =
                (p.get("id").and_then(Value::as_str), new_by_char.get(cid))
            {
                map.insert(old_id.to_string(), new_id.clone());
            }
        }
    }
    map
}

/// v4 `findLibrarianSummaryAnchorIndex`: the index of the most recent Librarian
/// summary message, or `None` when none has been posted.
fn find_librarian_summary_anchor(message_events: &[&Value]) -> Option<usize> {
    for i in (0..message_events.len()).rev() {
        let ev = message_events[i];
        if ev.get("systemSender").and_then(Value::as_str) == Some("librarian")
            && ev.get("systemKind").and_then(Value::as_str) == Some("summary")
        {
            return Some(i);
        }
    }
    None
}

/// v4 `projectMessageForNewChat`: project an old MessageEvent onto a fresh row —
/// strip lifecycle fields, remap participant / target / hostEvent ids. Returns
/// `None` when the author (or every whisper target, or a hostEvent participant)
/// isn't in the new chat.
fn project_message_for_new_chat(
    source: &Value,
    participant_map: &std::collections::HashMap<String, String>,
) -> Option<Value> {
    // participantId: drop the message if the author's character isn't in the new chat.
    let new_participant_id: Value = match source.get("participantId").and_then(Value::as_str) {
        Some(pid) => match participant_map.get(pid) {
            Some(mapped) => Value::String(mapped.clone()),
            None => return None,
        },
        None => Value::Null,
    };

    // targetParticipantIds: remap, drop unknowns; if the original was a whisper
    // (non-empty) and every target is gone, drop the message.
    let new_targets: Value = match source.get("targetParticipantIds").and_then(Value::as_array) {
        Some(arr) if !arr.is_empty() => {
            let remapped: Vec<Value> = arr
                .iter()
                .filter_map(|v| v.as_str())
                .filter_map(|oid| participant_map.get(oid))
                .map(|nid| Value::String(nid.clone()))
                .collect();
            if remapped.is_empty() {
                return None;
            }
            Value::Array(remapped)
        }
        _ => Value::Null,
    };

    // hostEvent: remap its participantId when present; drop the message if the
    // referenced participant isn't in the new chat.
    let new_host_event: Value = match source.get("hostEvent") {
        Some(Value::Object(he)) => {
            let mut remapped = he.clone();
            if let Some(pid) = he.get("participantId").and_then(Value::as_str) {
                match participant_map.get(pid) {
                    Some(mapped) => {
                        remapped.insert("participantId".to_string(), Value::String(mapped.clone()));
                    }
                    None => return None,
                }
            }
            Value::Object(remapped)
        }
        _ => Value::Null,
    };

    let attachments = source
        .get("attachments")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let take = |k: &str| source.get(k).cloned().unwrap_or(Value::Null);

    Some(json!({
        "type": "message",
        "id": uuid::Uuid::new_v4().to_string(),
        "role": take("role"),
        "content": take("content"),
        "attachments": attachments,
        "createdAt": now_iso(),
        "participantId": new_participant_id,
        "swipeGroupId": take("swipeGroupId"),
        "swipeIndex": take("swipeIndex"),
        "systemSender": take("systemSender"),
        "systemKind": take("systemKind"),
        "hostEvent": new_host_event,
        "targetParticipantIds": new_targets,
        "isSilentMessage": take("isSilentMessage"),
        "dangerFlags": take("dangerFlags"),
    }))
}

/// v4 `replicateTurnState`: remap the source chat's turn-state fields and persist
/// via `chats.update`.
async fn replicate_turn_state(
    db: &Db,
    new_chat_id: &str,
    source_chat: &Value,
    participant_map: &std::collections::HashMap<String, String>,
) -> Result<(), DbError> {
    let remap_id = |old: Option<&str>| -> Option<String> {
        old.and_then(|id| participant_map.get(id).cloned())
    };
    // JSON string-array column: parse, remap, re-stringify (default "[]").
    let remap_json_array = |raw: Option<&str>| -> String {
        let parsed: Value = serde_json::from_str(raw.unwrap_or("[]")).unwrap_or(json!([]));
        let remapped: Vec<String> = parsed
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .filter_map(|id| participant_map.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default();
        serde_json::to_string(&remapped).unwrap_or_else(|_| "[]".to_string())
    };

    let new_turn_queue = remap_json_array(source_chat.get("turnQueue").and_then(Value::as_str));
    let new_spoken = remap_json_array(
        source_chat
            .get("spokenThisCycleParticipantIds")
            .and_then(Value::as_str),
    );
    let new_impersonating: Vec<String> = source_chat
        .get("impersonatingParticipantIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter_map(|id| participant_map.get(id).cloned())
                .collect()
        })
        .unwrap_or_default();

    let update = crate::db::chats::ChatUpdate {
        is_paused: Some(
            source_chat
                .get("isPaused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        last_turn_participant_id: Some(remap_id(
            source_chat
                .get("lastTurnParticipantId")
                .and_then(Value::as_str),
        )),
        active_typing_participant_id: Some(remap_id(
            source_chat
                .get("activeTypingParticipantId")
                .and_then(Value::as_str),
        )),
        impersonating_participant_ids: Some(new_impersonating),
        all_llm_pause_turn_count: Some(
            source_chat
                .get("allLLMPauseTurnCount")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        ),
        turn_queue: Some(new_turn_queue),
        spoken_this_cycle_participant_ids: Some(new_spoken),
        ..Default::default()
    };

    let new_chat_id = new_chat_id.to_string();
    db.write(move |writers| {
        writers
            .main()
            .chats()
            .update(&new_chat_id, &update)
            .map(|_| ())
    })
    .await
}

/// v4 `applyChatContinuation`: the full backfill + turn-state replication +
/// cross-link bubbles.
pub async fn apply_chat_continuation(
    db: &Db,
    new_chat_id: &str,
    source_chat_id: &str,
) -> Result<ApplyChatContinuationResult, DbError> {
    let sid = source_chat_id.to_string();
    let Some(source_chat) = db.read_main(move |c| chats_read::find_by_id(c, &sid))? else {
        // v4 logs + returns the empty result.
        return Ok(ApplyChatContinuationResult::default());
    };
    let nid = new_chat_id.to_string();
    let Some(new_chat) = db.read_main(move |c| chats_read::find_by_id(c, &nid))? else {
        return Ok(ApplyChatContinuationResult::default());
    };

    let participant_map = build_participant_id_map(&source_chat, &new_chat);

    // 1. Host link bubble in the new chat first.
    post_host_continuation_from_announcement(
        db,
        HostContinuationFromAnnouncement {
            chat_id: new_chat_id.to_string(),
            source_chat_id: source_chat_id.to_string(),
            source_title: source_chat
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
    )
    .await;

    // 2. Carryover anchor + replay.
    let sid = source_chat_id.to_string();
    let events = db.read_main(move |c| chats_messages_read::get_messages(c, &sid))?;
    let message_events: Vec<&Value> = events
        .iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("message"))
        .collect();
    let anchor = find_librarian_summary_anchor(&message_events);
    let had_librarian_summary = anchor.is_some();
    let carryover = match anchor {
        Some(i) => &message_events[i..],
        None => &message_events[..],
    };

    let mut replayed_message_count = 0usize;
    for source_msg in carryover {
        let Some(projected) = project_message_for_new_chat(source_msg, &participant_map) else {
            continue;
        };
        let Ok(event) = serde_json::from_value::<ChatEventInput>(projected) else {
            continue;
        };
        let target = new_chat_id.to_string();
        let write = db
            .write(move |writers| writers.main().chat_messages().add_message(&target, &event))
            .await;
        if write.is_ok() {
            replayed_message_count += 1;
        }
        // v4 logs a failed replay and continues.
    }

    // 3. Replicate turn state (swallow errors — v4 logs and continues).
    let _ = replicate_turn_state(db, new_chat_id, &source_chat, &participant_map).await;

    // 4. Tail bubble in the source chat — last.
    let posted_source_tail_bubble = post_host_continuation_to_announcement(
        db,
        HostContinuationToAnnouncement {
            chat_id: source_chat_id.to_string(),
            new_chat_id: new_chat_id.to_string(),
            new_title: new_chat
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
    )
    .await
    .is_some();

    Ok(ApplyChatContinuationResult {
        replayed_message_count,
        had_librarian_summary,
        posted_source_tail_bubble,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat_with_participants(pairs: &[(&str, &str)]) -> Value {
        json!({
            "participants": pairs.iter().map(|(id, cid)| json!({"id": id, "characterId": cid})).collect::<Vec<_>>()
        })
    }

    #[test]
    fn participant_map_by_shared_character() {
        let source = chat_with_participants(&[("os1", "cA"), ("os2", "cB")]);
        let new = chat_with_participants(&[("ns1", "cA"), ("ns2", "cC")]);
        let map = build_participant_id_map(&source, &new);
        assert_eq!(map.get("os1"), Some(&"ns1".to_string()));
        assert!(!map.contains_key("os2")); // cB not in new chat
    }

    #[test]
    fn librarian_anchor_finds_last_summary() {
        let a = json!({"systemSender": "librarian", "systemKind": "summary"});
        let b = json!({"content": "hi"});
        let c = json!({"systemSender": "librarian", "systemKind": "summary"});
        let events = vec![&a, &b, &c];
        assert_eq!(find_librarian_summary_anchor(&events), Some(2));
        let none = vec![&b];
        assert_eq!(find_librarian_summary_anchor(&none), None);
    }

    #[test]
    fn projection_drops_unmapped_author() {
        let map = std::collections::HashMap::from([("os1".to_string(), "ns1".to_string())]);
        // Authored by a mapped participant → kept, id remapped.
        let msg = json!({"participantId": "os1", "role": "ASSISTANT", "content": "hi"});
        let p = project_message_for_new_chat(&msg, &map).unwrap();
        assert_eq!(p["participantId"], "ns1");
        assert_eq!(p["type"], "message");
        // Authored by an unmapped participant → dropped.
        let msg2 = json!({"participantId": "gone", "role": "ASSISTANT", "content": "hi"});
        assert!(project_message_for_new_chat(&msg2, &map).is_none());
    }

    #[test]
    fn projection_drops_whisper_with_all_targets_gone() {
        let map = std::collections::HashMap::from([("os1".to_string(), "ns1".to_string())]);
        let whisper =
            json!({"role": "ASSISTANT", "content": "psst", "targetParticipantIds": ["gone"]});
        assert!(project_message_for_new_chat(&whisper, &map).is_none());
        let kept = json!({"role": "ASSISTANT", "content": "psst", "targetParticipantIds": ["os1", "gone"]});
        let p = project_message_for_new_chat(&kept, &map).unwrap();
        assert_eq!(p["targetParticipantIds"], json!(["ns1"]));
    }
}
