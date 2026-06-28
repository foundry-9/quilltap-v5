//! Tier-1 differential test #18 (Wave 3 / B9): message attribution & presence.
//!
//! Covers filterMessagesByHistoryAccess (date seam injected as ms),
//! computePresenceWindowsForParticipant, filterMessagesByPresenceWindows,
//! filterWhisperMessages, getParticipantName, attributeMessagesForCharacter,
//! findUserParticipantName. Exact equality on every field.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/message-attribution.ts \
//!     > /tmp/oracle-message-attribution.ndjson
//! Run:
//!   QT_ORACLE_MESSAGE_ATTRIBUTION=/tmp/oracle-message-attribution.ndjson \
//!     cargo test -p quilltap-harness

use std::collections::HashMap;

use quilltap_core::chat_predicates::ParticipantStatus;
use quilltap_core::message_attribution::{
    attribute_messages_for_character, compute_presence_windows_for_participant,
    filter_messages_by_history_access, filter_messages_by_presence_windows,
    filter_whisper_messages, find_user_participant_name, get_participant_name, AttributionMessage,
    AttributionParticipant, HistoryMessage, HostEvent, PresenceWindow,
};
use serde::Deserialize;

fn parse_status(s: &str) -> ParticipantStatus {
    match s {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "absent" => ParticipantStatus::Absent,
        "removed" => ParticipantStatus::Removed,
        other => panic!("unknown status {other}"),
    }
}

#[derive(Deserialize)]
struct WHistMsg {
    id: String,
    #[serde(rename = "createdAtMs")]
    created_at_ms: Option<f64>,
}

#[derive(Deserialize)]
struct WHostEvent {
    #[serde(rename = "participantId", default)]
    participant_id: Option<String>,
    #[serde(rename = "toStatus", default)]
    to_status: Option<String>,
}

#[derive(Deserialize)]
struct WPresMsg {
    #[serde(rename = "createdAt", default)]
    created_at: Option<String>,
    #[serde(rename = "hostEvent", default)]
    host_event: Option<WHostEvent>,
}

#[derive(Deserialize)]
struct WWindow {
    from: String,
    to: Option<String>,
}

#[derive(Deserialize)]
struct WIdMsg {
    id: String,
    #[serde(rename = "createdAt", default)]
    created_at: Option<String>,
    #[serde(rename = "participantId", default)]
    participant_id: Option<String>,
    #[serde(rename = "targetParticipantIds", default)]
    target_participant_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct WPart {
    id: String,
    #[serde(rename = "type")]
    ptype: String,
    #[serde(rename = "characterId", default)]
    character_id: Option<String>,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
    status: String,
}

impl WPart {
    fn to_participant(&self) -> AttributionParticipant {
        AttributionParticipant {
            id: self.id.clone(),
            participant_type: self.ptype.clone(),
            character_id: self.character_id.clone(),
            controlled_by: self.controlled_by.clone(),
            status: parse_status(&self.status),
        }
    }
}

#[derive(Deserialize)]
struct WAttrMsg {
    id: Option<String>,
    role: String,
    content: String,
    #[serde(rename = "participantId", default)]
    participant_id: Option<String>,
    #[serde(rename = "thoughtSignature", default)]
    thought_signature: Option<String>,
}

#[derive(Deserialize)]
struct WAttrOut {
    role: String,
    content: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "participantId", default)]
    participant_id: Option<String>,
    #[serde(rename = "thoughtSignature", default)]
    thought_signature: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "history")]
    History {
        id: String,
        msgs: Vec<WHistMsg>,
        #[serde(rename = "hasHistoryAccess")]
        has_history_access: bool,
        #[serde(rename = "joinMs")]
        join_ms: f64,
        out: Vec<String>,
    },
    #[serde(rename = "presence")]
    Presence {
        id: String,
        msgs: Vec<WPresMsg>,
        #[serde(rename = "participantId")]
        participant_id: String,
        #[serde(rename = "participantCreatedAt")]
        participant_created_at: String,
        out: Vec<WWindow>,
    },
    #[serde(rename = "presenceFilter")]
    PresenceFilter {
        id: String,
        msgs: Vec<WIdMsg>,
        windows: Vec<WWindow>,
        out: Vec<String>,
    },
    #[serde(rename = "whisper")]
    Whisper {
        id: String,
        msgs: Vec<WIdMsg>,
        #[serde(rename = "respondingId")]
        responding_id: String,
        out: Vec<String>,
    },
    #[serde(rename = "name")]
    Name {
        id: String,
        #[serde(rename = "participantId", default)]
        participant_id: Option<String>,
        characters: HashMap<String, String>,
        participants: Vec<WPart>,
        out: Option<String>,
    },
    #[serde(rename = "attribute")]
    Attribute {
        id: String,
        msgs: Vec<WAttrMsg>,
        #[serde(rename = "respondingId")]
        responding_id: String,
        characters: HashMap<String, String>,
        participants: Vec<WPart>,
        out: Vec<WAttrOut>,
    },
    #[serde(rename = "userName")]
    UserName {
        id: String,
        participants: Vec<WPart>,
        characters: HashMap<String, String>,
        #[serde(rename = "activeTyping", default)]
        active_typing: Option<String>,
        out: Option<String>,
    },
}

fn kept_ids(ids: &[String], mask: &[bool]) -> Vec<String> {
    ids.iter()
        .zip(mask)
        .filter(|(_, &k)| k)
        .map(|(id, _)| id.clone())
        .collect()
}

#[test]
fn message_attribution_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MESSAGE_ATTRIBUTION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MESSAGE_ATTRIBUTION to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::History {
                id,
                msgs,
                has_history_access,
                join_ms,
                out,
            } => {
                let items: Vec<HistoryMessage> = msgs
                    .iter()
                    .map(|m| HistoryMessage {
                        created_at_ms: m.created_at_ms,
                    })
                    .collect();
                let mask = filter_messages_by_history_access(&items, has_history_access, join_ms);
                let ids: Vec<String> = msgs.iter().map(|m| m.id.clone()).collect();
                assert_eq!(kept_ids(&ids, &mask), out, "history '{id}'");
            }
            Row::Presence {
                id,
                msgs,
                participant_id,
                participant_created_at,
                out,
            } => {
                let items: Vec<AttributionMessage> = msgs
                    .iter()
                    .map(|m| AttributionMessage {
                        id: None,
                        role: String::new(),
                        content: String::new(),
                        participant_id: None,
                        thought_signature: None,
                        created_at: m.created_at.clone(),
                        target_participant_ids: None,
                        host_event: m.host_event.as_ref().map(|he| HostEvent {
                            participant_id: he.participant_id.clone(),
                            to_status: he.to_status.as_deref().map(parse_status),
                        }),
                    })
                    .collect();
                let got = compute_presence_windows_for_participant(
                    &items,
                    &participant_id,
                    &participant_created_at,
                );
                let want: Vec<PresenceWindow> = out
                    .into_iter()
                    .map(|w| PresenceWindow {
                        from: w.from,
                        to: w.to,
                    })
                    .collect();
                assert_eq!(got, want, "presence '{id}'");
            }
            Row::PresenceFilter {
                id,
                msgs,
                windows,
                out,
            } => {
                let items: Vec<AttributionMessage> = msgs.iter().map(id_msg_to_attr).collect();
                let wins: Vec<PresenceWindow> = windows
                    .into_iter()
                    .map(|w| PresenceWindow {
                        from: w.from,
                        to: w.to,
                    })
                    .collect();
                let mask = filter_messages_by_presence_windows(&items, &wins);
                let ids: Vec<String> = msgs.iter().map(|m| m.id.clone()).collect();
                assert_eq!(kept_ids(&ids, &mask), out, "presenceFilter '{id}'");
            }
            Row::Whisper {
                id,
                msgs,
                responding_id,
                out,
            } => {
                let items: Vec<AttributionMessage> = msgs.iter().map(id_msg_to_attr).collect();
                let mask = filter_whisper_messages(&items, &responding_id);
                let ids: Vec<String> = msgs.iter().map(|m| m.id.clone()).collect();
                assert_eq!(kept_ids(&ids, &mask), out, "whisper '{id}'");
            }
            Row::Name {
                id,
                participant_id,
                characters,
                participants,
                out,
            } => {
                let parts: Vec<AttributionParticipant> =
                    participants.iter().map(WPart::to_participant).collect();
                let got = get_participant_name(participant_id.as_deref(), &characters, &parts);
                assert_eq!(got, out, "name '{id}'");
            }
            Row::Attribute {
                id,
                msgs,
                responding_id,
                characters,
                participants,
                out,
            } => {
                let parts: Vec<AttributionParticipant> =
                    participants.iter().map(WPart::to_participant).collect();
                let items: Vec<AttributionMessage> = msgs
                    .iter()
                    .map(|m| AttributionMessage {
                        id: m.id.clone(),
                        role: m.role.clone(),
                        content: m.content.clone(),
                        participant_id: m.participant_id.clone(),
                        thought_signature: m.thought_signature.clone(),
                        created_at: None,
                        target_participant_ids: None,
                        host_event: None,
                    })
                    .collect();
                let got =
                    attribute_messages_for_character(&items, &responding_id, &characters, &parts);
                assert_eq!(got.len(), out.len(), "attribute '{id}' len");
                for (g, o) in got.iter().zip(&out) {
                    assert_eq!(g.role, o.role, "attribute '{id}' role");
                    assert_eq!(g.content, o.content, "attribute '{id}' content");
                    assert_eq!(g.id, o.id, "attribute '{id}' id");
                    assert_eq!(g.name, o.name, "attribute '{id}' name");
                    assert_eq!(g.participant_id, o.participant_id, "attribute '{id}' pid");
                    assert_eq!(
                        g.thought_signature, o.thought_signature,
                        "attribute '{id}' thoughtSignature"
                    );
                }
            }
            Row::UserName {
                id,
                participants,
                characters,
                active_typing,
                out,
            } => {
                let parts: Vec<AttributionParticipant> =
                    participants.iter().map(WPart::to_participant).collect();
                let got = find_user_participant_name(&parts, &characters, active_typing.as_deref());
                assert_eq!(got, out, "userName '{id}'");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: message-attribution matched oracle ({count} rows).");
}

fn id_msg_to_attr(m: &WIdMsg) -> AttributionMessage {
    AttributionMessage {
        id: Some(m.id.clone()),
        role: String::new(),
        content: String::new(),
        participant_id: m.participant_id.clone(),
        thought_signature: None,
        created_at: m.created_at.clone(),
        target_participant_ids: m.target_participant_ids.clone(),
        host_event: None,
    }
}
