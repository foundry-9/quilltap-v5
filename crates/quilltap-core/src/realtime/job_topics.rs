//! Background-job type / write batch → realtime topics (v4
//! `lib/realtime/job-topics.ts`, `f3892158d`). **Pure** — no I/O, no clock.
//!
//! A completed job is the moment a lot of server state stops being what the
//! open tabs think it is. The runner knows the job's type and payload the
//! instant its writes commit, which makes that one place a better publisher
//! than a dozen scattered handlers.
//!
//! The job table is deliberately partial. A job type absent from it still moves
//! the `jobs` topic (the queue itself changed); it just has no *entity* worth
//! announcing, or its entity is covered by the write-batch mapping.

use serde_json::Value;

use crate::realtime::types::RealtimeTopic;
use crate::write_partition::ChildWritePayload;

/// One hint: a topic, optionally scoped to a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicHint {
    pub topic: RealtimeTopic,
    pub id: Option<String>,
}

impl TopicHint {
    fn collection(topic: RealtimeTopic) -> Self {
        Self { topic, id: None }
    }
    fn scoped(topic: RealtimeTopic, id: Option<String>) -> Self {
        Self { topic, id }
    }
}

/// v4's `str(payload, key)`: only a NON-EMPTY string counts. Anything else —
/// absent, null, a number, an empty string — reads as no id, and the hint goes
/// out collection-wide rather than not at all.
fn str_field(payload: Option<&Value>, key: &str) -> Option<String> {
    payload
        .and_then(|p| p.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The entity topics a finished job of `job_type` should announce (v4
/// `topicsForCompletedJob`).
///
/// `job_type` is `None` when the runner never saw the row (a result arriving
/// for a job it no longer tracks).
pub fn topics_for_completed_job(job_type: Option<&str>, payload: Option<&Value>) -> Vec<TopicHint> {
    match job_type {
        // Budgets consumed, run state possibly ended — the toolbar's room
        // badges read all of it.
        Some("AUTONOMOUS_ROOM_TURN") | Some("AUTONOMOUS_ROOM_SCHEDULE_TICK") => vec![
            TopicHint::collection(RealtimeTopic::AutonomousRooms),
            TopicHint::scoped(RealtimeTopic::Chats, str_field(payload, "chatId")),
        ],

        // Either a chat or a project owns the background; the payload says
        // which. Chat is probed FIRST and wins.
        Some("STORY_BACKGROUND_GENERATION") => {
            if let Some(chat_id) = str_field(payload, "chatId") {
                return vec![TopicHint::scoped(RealtimeTopic::Chats, Some(chat_id))];
            }
            if let Some(project_id) = str_field(payload, "projectId") {
                return vec![TopicHint::scoped(RealtimeTopic::Projects, Some(project_id))];
            }
            vec![]
        }

        Some("CHARACTER_AVATAR_GENERATION") => vec![
            TopicHint::scoped(RealtimeTopic::Chats, str_field(payload, "chatId")),
            TopicHint::scoped(RealtimeTopic::Characters, str_field(payload, "characterId")),
        ],

        Some("CHARACTER_HEADSHOULDERS_BACKFILL") => vec![TopicHint::scoped(
            RealtimeTopic::Characters,
            str_field(payload, "characterId"),
        )],

        Some("TITLE_UPDATE")
        | Some("CONTEXT_SUMMARY")
        | Some("CHAT_DANGER_CLASSIFICATION")
        | Some("SCENE_STATE_TRACKING")
        | Some("WARDROBE_OUTFIT_ANNOUNCEMENT") => vec![TopicHint::scoped(
            RealtimeTopic::Chats,
            str_field(payload, "chatId"),
        )],

        // A rendered conversation lands in a document store; the Scriptorium
        // and the character conversations tab both watch that.
        Some("CONVERSATION_RENDER") => vec![
            TopicHint::collection(RealtimeTopic::MountPoints),
            TopicHint::scoped(RealtimeTopic::Chats, str_field(payload, "chatId")),
        ],

        _ => vec![],
    }
}

/// Repository namespace → the realtime topic its rows belong to (v4
/// `REPOSITORY_TOPICS`).
///
/// Keyed on the part of a buffered write's `method` before the first dot, which
/// is precise in a way that probing argument shapes is not: `chats.update` is a
/// chat write no matter what its arguments look like. Namespaces absent from
/// this table have no client-visible topic yet and are simply skipped.
const REPOSITORY_TOPICS: &[(&str, RealtimeTopic)] = &[
    ("characters", RealtimeTopic::Characters),
    ("chats", RealtimeTopic::Chats),
    ("projects", RealtimeTopic::Projects),
    ("docMountPoints", RealtimeTopic::MountPoints),
    ("docMountFiles", RealtimeTopic::MountPoints),
    ("docMountFileLinks", RealtimeTopic::MountPoints),
    ("docMountFolders", RealtimeTopic::MountPoints),
    ("docMountDocuments", RealtimeTopic::MountPoints),
];

/// Which argument field on a write carries the id the topic is scoped by (v4
/// `TOPIC_ID_FIELDS`). A write whose first argument IS the id string
/// (`chats.update(chatId, patch)`) is the common shape; object-shaped payloads
/// name it instead.
fn topic_id_fields(topic: RealtimeTopic) -> &'static [&'static str] {
    match topic {
        RealtimeTopic::Characters => &["characterId", "id"],
        RealtimeTopic::Chats => &["chatId", "id"],
        RealtimeTopic::Projects => &["projectId", "id"],
        RealtimeTopic::MountPoints => &["mountPointId", "id"],
        RealtimeTopic::AutonomousRooms | RealtimeTopic::Jobs => &[],
    }
}

fn extract_topic_id(topic: RealtimeTopic, args: &[Value]) -> Option<String> {
    let first = args.first()?;
    if let Some(s) = first.as_str() {
        if !s.is_empty() {
            return Some(s.to_string());
        }
        // v4: a non-empty-string test that FAILS falls through to the object
        // probe, which a string cannot satisfy — so an empty first arg reads as
        // no id, not as a reason to stop.
        return None;
    }
    let obj = first.as_object()?;
    for field in topic_id_fields(topic) {
        if let Some(v) = obj.get(*field).and_then(Value::as_str) {
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Entity hints for a committed write batch, deduplicated (v4
/// `topicsForWriteBatch`).
///
/// A write whose id cannot be read still yields a collection-wide hint for its
/// topic — coarser than ideal, never wrong. Dedup is by `topic:id` key across
/// the batch, order-preserving.
///
/// **v5 needs no translation layer here.** The order anticipated that v5's
/// buffered writes would be typed and would have to be mapped onto v4's
/// namespaces separately. They are not: `write_partition::ChildWritePayload` is
/// v4's `{method, args}` verbatim — the partition logic ported that
/// representation whole, because it is the correctness property, not a Node
/// workaround. So this reads exactly the strings v4 reads.
pub fn topics_for_write_batch(writes: &[ChildWritePayload]) -> Vec<TopicHint> {
    let mut seen: Vec<String> = Vec::new();
    let mut hints: Vec<TopicHint> = Vec::new();

    for write in writes {
        let namespace = write.method.split('.').next().unwrap_or(&write.method);
        let Some((_, topic)) = REPOSITORY_TOPICS.iter().find(|(ns, _)| *ns == namespace) else {
            continue;
        };
        let id = extract_topic_id(*topic, &write.args);
        let key = match &id {
            Some(id) => format!("{}:{}", topic.as_str(), id),
            None => topic.as_str().to_string(),
        };
        if seen.iter().any(|k| k == &key) {
            continue;
        }
        seen.push(key);
        hints.push(TopicHint { topic: *topic, id });
    }

    hints
}
