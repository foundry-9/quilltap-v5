//! The realtime hint's wire shape (v4 `lib/schemas/realtime.types.ts`).

use serde::Serialize;

/// The protocol version this build speaks (v4 `REALTIME_PROTOCOL_VERSION`).
pub const REALTIME_PROTOCOL_VERSION: u8 = 1;

/// Canonical topic names (v4 `REALTIME_TOPICS`), closed for this round.
///
/// Each one is (or maps 1:1 onto) a namespace in v4's `lib/query/keys.ts`,
/// which is what keeps the client's topic map boring: adding an entity is one
/// row in each of two files. The client IGNORES topics it does not recognize,
/// so a server that learns a new one cannot break an older tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RealtimeTopic {
    /// Background-job lifecycle and inline activity spans — the toolbar chips.
    Jobs,
    /// Autonomous-room run state and budgets.
    AutonomousRooms,
    /// Chats: list membership, detail, per-chat background/state.
    Chats,
    /// Projects, including their story backgrounds.
    Projects,
    /// Characters and their prompts/photos.
    Characters,
    /// Document stores and their indexing/embedding status.
    MountPoints,
}

/// Every topic, in v4's declaration order.
pub const REALTIME_TOPICS: [RealtimeTopic; 6] = [
    RealtimeTopic::Jobs,
    RealtimeTopic::AutonomousRooms,
    RealtimeTopic::Chats,
    RealtimeTopic::Projects,
    RealtimeTopic::Characters,
    RealtimeTopic::MountPoints,
];

impl RealtimeTopic {
    /// The wire spelling — the server↔client join.
    pub const fn as_str(self) -> &'static str {
        match self {
            RealtimeTopic::Jobs => "jobs",
            RealtimeTopic::AutonomousRooms => "autonomousRooms",
            RealtimeTopic::Chats => "chats",
            RealtimeTopic::Projects => "projects",
            RealtimeTopic::Characters => "characters",
            RealtimeTopic::MountPoints => "mountPoints",
        }
    }
}

/// A server→client invalidation hint (v4 `RealtimeEvent`).
///
/// Wire bytes, per the round's §Shared contract §B.2:
/// `{"v":1,"topic":"<topic>"}` plus `"id":"<id>"` **only when scoped** (omitted,
/// never null — v4's object spread) and `"at":<server-ms>`. It rides the
/// [`Event`](crate::api::Event) envelope with none of the three scope tags set,
/// so no `chatId`/`roomId`/`progressId` key appears.
///
/// `at` is for debugging and log correlation ONLY. Clients must not order,
/// dedupe, or expire on it — the server's clock is not the client's, and the
/// bus coalesces anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RealtimeHint {
    /// Always [`REALTIME_PROTOCOL_VERSION`] (v4's `z.literal(1)`).
    pub v: u8,
    /// A topic name. Typed [`RealtimeTopic`] at every publish site; a plain
    /// string on the wire because v4's schema deliberately accepts any string.
    pub topic: String,
    /// Entity id, when the change is row-scoped rather than collection-wide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Server ms timestamp.
    pub at: i64,
}

impl RealtimeHint {
    /// Build a hint for `topic`, optionally scoped to `id`, stamped `at`.
    pub fn new(topic: RealtimeTopic, id: Option<&str>, at: i64) -> Self {
        Self {
            v: REALTIME_PROTOCOL_VERSION,
            topic: topic.as_str().to_string(),
            id: id.map(str::to_string),
            at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{Event, EventPayload};

    #[test]
    fn topics_are_v4s_six_in_order() {
        assert_eq!(
            REALTIME_TOPICS.map(RealtimeTopic::as_str).to_vec(),
            vec![
                "jobs",
                "autonomousRooms",
                "chats",
                "projects",
                "characters",
                "mountPoints"
            ]
        );
    }

    /// §Shared contract §B.2, byte for byte.
    #[test]
    fn a_collection_hint_omits_id_entirely() {
        let ev = Event::realtime(RealtimeHint::new(
            RealtimeTopic::Jobs,
            None,
            1_700_000_000_123,
        ));
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"v":1,"topic":"jobs","at":1700000000123}"#
        );
    }

    #[test]
    fn a_scoped_hint_carries_id_between_topic_and_at() {
        let ev = Event::realtime(RealtimeHint::new(
            RealtimeTopic::Chats,
            Some("c-1"),
            1_700_000_000_123,
        ));
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"v":1,"topic":"chats","id":"c-1","at":1700000000123}"#
        );
    }

    /// An empty id is a scoped hint with an empty id — never a silent
    /// collection-wide one. The callers that must not produce one filter first
    /// (v4's `str()` reader); this pins that the type does not do it for them.
    #[test]
    fn the_hint_type_does_not_launder_an_empty_id() {
        let ev = Event::realtime(RealtimeHint::new(RealtimeTopic::Chats, Some(""), 0));
        assert!(serde_json::to_string(&ev).unwrap().contains(r#""id":""#));
    }

    /// §Shared contract §B.5: a frame is a hint iff it carries BOTH `topic` and
    /// `v`. The other payload families must carry neither, or the client's
    /// discrimination is ambiguous.
    #[test]
    fn no_other_event_family_carries_topic_or_v() {
        use crate::services::chat_events::{ChatEvent, StatusPayload};
        use crate::services::creation_progress::CreationProgressFrame;

        let others = [
            Event::chat(
                "c1",
                ChatEvent::Content {
                    content: "hi".into(),
                },
            ),
            Event::chat(
                "c1",
                ChatEvent::Status {
                    status: StatusPayload {
                        stage: "thinking".into(),
                        message: "…".into(),
                        tool_name: None,
                        character_name: None,
                        character_id: None,
                    },
                },
            ),
            Event::chat_error(
                "c1",
                crate::api::types::ChatErrorPayload {
                    error: "e".into(),
                    error_type: "t".into(),
                    details: "d".into(),
                },
            ),
            Event::creation_progress("p1", CreationProgressFrame::Done { ts: 1 }),
        ];
        for ev in others {
            let v: serde_json::Value = serde_json::to_value(&ev).unwrap();
            let obj = v.as_object().unwrap();
            assert!(!obj.contains_key("topic"), "{v} carries `topic`");
            assert!(!obj.contains_key("v"), "{v} carries `v`");
        }

        // …and the hint carries both, with no scope tag of its own.
        let hint = Event::realtime(RealtimeHint::new(RealtimeTopic::Jobs, None, 0));
        let v: serde_json::Value = serde_json::to_value(&hint).unwrap();
        let obj = v.as_object().unwrap();
        assert!(obj.contains_key("topic") && obj.contains_key("v"));
        for tag in ["chatId", "roomId", "progressId"] {
            assert!(!obj.contains_key(tag), "a hint must not carry {tag}");
        }
    }

    /// Adding an untagged variant must not move any existing family's bytes.
    #[test]
    fn the_existing_families_serialize_unchanged() {
        use crate::services::chat_events::ChatEvent;
        assert_eq!(
            serde_json::to_string(&Event::chat(
                "c1",
                ChatEvent::Content {
                    content: "hi".into()
                }
            ))
            .unwrap(),
            r#"{"chatId":"c1","content":"hi"}"#
        );
        assert_eq!(
            serde_json::to_string(&Event::creation_progress(
                "p1",
                crate::services::creation_progress::CreationProgressFrame::Done { ts: 7 }
            ))
            .unwrap(),
            r#"{"progressId":"p1","kind":"done","ts":7}"#
        );
        // And the payload enum still has exactly the four families.
        fn _exhaustive(p: &EventPayload) {
            match p {
                EventPayload::Chat(_)
                | EventPayload::ChatError(_)
                | EventPayload::CreationProgress(_)
                | EventPayload::Realtime(_) => {}
            }
        }
    }
}
