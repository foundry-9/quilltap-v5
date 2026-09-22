//! P4.D212 Tier-1 item 6: the `chatRebuildSummary` verb (v4 `POST
//! /api/v1/chats/[id]?action=rebuild-summary`, `e7821606f`, bug 161) AT THE
//! DISPATCH WIRE.
//!
//! v5 serves this action over `POST /api/dispatch` only — the regenerate-title
//! precedent (the REST edge's pointer sentence sends every chat action it does
//! not serve to the dispatch route). The handler-level families
//! (`chat_rebuild_summary_equivalence`, `chat_admin_routes_equivalence`) drive
//! `services::chat_admin::chat_rebuild_summary` directly against v4's real
//! route; what they cannot see is the trip through serde and the envelope on
//! the way back — a serde collapse is visible only here.
//!
//! What it proves, each over its own served copy of the committed
//! `chat-send-{main,mount}` pair:
//!
//!   1. the 200 body is `{success: true, jobId}` in that key order, and the
//!      side effects landed: the summary, its anchors and the fold cursor
//!      cleared in one update with `lastFullRebuildTurn` UNTOUCHED, and ONE
//!      `CONTEXT_SUMMARY` job at v4's `enqueueJob` defaults (priority 0,
//!      maxAttempts 3) whose payload is `{chatId, connectionProfileId,
//!      forceRegenerate: false}` in that order;
//!   2. a running autonomous room answers v4's 409 sentence, and writes nothing;
//!   3. a user with no connection profiles answers v4's 400, and writes nothing;
//!   4. a missing chat answers v4's 404 `Chat not found`;
//!   5. a wrong-type `chatId` is refused at the serde decode (the census's own
//!      contract — a `*_id` field is a route identifier).
//!
//! Run:
//!   cargo test -p quilltap-web --test chat_rebuild_summary_dispatch_wire

mod common;

use std::path::Path;

use quilltap_core::db::Writer;
use serde_json::{json, Value};

const MISSING_CHAT: &str = "99999999-9999-4999-8999-999999999999";
/// Seeded before the call so the clear is observable and the untouched column
/// is distinguishable from a zeroed one.
const SEEDED_FULL_REBUILD_TURN: f64 = 60.0;

fn main_db(base: &Path) -> std::path::PathBuf {
    base.join("data").join("quilltap.db")
}

/// The v4 `background_jobs` shape (a real instance always carries it; the
/// committed pair may predate the first enqueue).
const BACKGROUND_JOBS_DDL: &str = "CREATE TABLE IF NOT EXISTS background_jobs (\
    id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, status TEXT NOT NULL, \
    payload TEXT NOT NULL, priority REAL NOT NULL, attempts REAL NOT NULL, \
    maxAttempts REAL NOT NULL, lastError TEXT, scheduledAt TEXT NOT NULL, \
    startedAt TEXT, completedAt TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

/// Materialize the pair, seed a non-empty summary state on its first chat, and
/// return `(base, chat_id)`. `plant` runs last on the writable connection.
fn instance(plant: impl FnOnce(&rusqlite::Connection, &str)) -> (tempfile::TempDir, String) {
    let base = common::materialize_fixture_instance();
    let w = Writer::open_writable(&main_db(base.path()), common::TEST_PEPPER).unwrap();
    let c = w.connection();
    c.execute_batch(BACKGROUND_JOBS_DDL).unwrap();
    let chat_id: String = c
        .query_row("SELECT id FROM chats ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("the pair carries a chat");
    c.execute(
        "UPDATE chats SET contextSummary = 'Active threads: a name nobody said.', \
         summaryAnchorMessageIds = '[\"m1\",\"m2\"]', lastSummaryTurn = 10, \
         lastFullRebuildTurn = ?1 WHERE id = ?2",
        rusqlite::params![SEEDED_FULL_REBUILD_TURN, chat_id],
    )
    .unwrap();
    plant(c, &chat_id);
    (base, chat_id)
}

async fn post(addr: std::net::SocketAddr, body: Value) -> (u16, String) {
    let r = reqwest::Client::new()
        .post(format!("http://{addr}/api/dispatch"))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    (status, r.text().await.unwrap())
}

fn sentence(v: &Value) -> &str {
    v.get("error")
        .and_then(Value::as_str)
        .or_else(|| v.pointer("/data/message").and_then(Value::as_str))
        .unwrap_or_default()
}

/// `(contextSummary, summaryAnchorMessageIds, lastSummaryTurn, lastFullRebuildTurn)`.
fn summary_state(base: &Path, chat_id: &str) -> (Option<String>, String, f64, f64) {
    let w = Writer::open_writable(&main_db(base), common::TEST_PEPPER).unwrap();
    w.connection()
        .query_row(
            "SELECT contextSummary, summaryAnchorMessageIds, lastSummaryTurn, \
             lastFullRebuildTurn FROM chats WHERE id = ?1",
            [chat_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
}

type JobRow = (String, String, String, f64, f64, f64);

/// `{id → (type, status, payload, priority, maxAttempts, attempts)}` for every
/// `CONTEXT_SUMMARY` job. The committed pair already carries jobs of its own
/// (one of them a FAILED `CONTEXT_SUMMARY`), so every assertion here is on the
/// set DIFFERENCE across the call.
fn summary_jobs(base: &Path) -> std::collections::BTreeMap<String, JobRow> {
    let w = Writer::open_writable(&main_db(base), common::TEST_PEPPER).unwrap();
    let mut stmt = w
        .connection()
        .prepare(
            "SELECT id, type, status, payload, priority, maxAttempts, attempts \
             FROM background_jobs WHERE type = 'CONTEXT_SUMMARY'",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn new_jobs(
    before: &std::collections::BTreeMap<String, JobRow>,
    after: std::collections::BTreeMap<String, JobRow>,
) -> Vec<(String, JobRow)> {
    after
        .into_iter()
        .filter(|(id, _)| !before.contains_key(id))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_rebuild_verb_clears_the_summary_and_enqueues_one_bounded_fold() {
    let (base, chat_id) = instance(|_, _| {});
    let profile_count: i64 = {
        let w = Writer::open_writable(&main_db(base.path()), common::TEST_PEPPER).unwrap();
        w.connection()
            .query_row("SELECT COUNT(*) FROM connection_profiles", [], |r| r.get(0))
            .unwrap()
    };
    assert!(
        profile_count > 0,
        "the happy path needs a profile to choose"
    );
    let before = summary_jobs(base.path());
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;

    let (status, text) = post(
        addr,
        json!({ "type": "chatRebuildSummary", "chatId": chat_id }),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    let data = &v["data"];
    let job_id = data["jobId"].as_str().expect("a jobId string").to_string();
    // Key order `success, jobId` — v4 `NextResponse.json({ success: true, jobId })`.
    // Read off the RAW wire text: a parsed `Value` may re-order keys.
    assert!(
        text.contains(&format!(r#""data":{{"success":true,"jobId":"{job_id}"}}"#)),
        "the body's two keys in v4's order: {text}"
    );
    assert_eq!(v["type"], "chatAdmin", "{text}");

    let (summary, anchors, last_summary_turn, last_full) = summary_state(base.path(), &chat_id);
    assert_eq!(summary, None, "contextSummary cleared to NULL");
    assert_eq!(anchors, "[]", "summaryAnchorMessageIds cleared to []");
    assert_eq!(last_summary_turn, 0.0, "the fold cursor back to 0");
    assert_eq!(
        last_full, SEEDED_FULL_REBUILD_TURN,
        "lastFullRebuildTurn deliberately UNTOUCHED — zeroing it would route a long \
         chat into the single-shot forceRegenerate path"
    );

    let jobs = new_jobs(&before, summary_jobs(base.path()));
    assert_eq!(jobs.len(), 1, "exactly one new summary job: {jobs:?}");
    let (id, (ty, _status, payload, priority, max_attempts, _attempts)) = &jobs[0];
    assert_eq!(id, &job_id, "the body's jobId is the enqueued row");
    assert_eq!(ty, "CONTEXT_SUMMARY");
    assert_eq!(
        *priority, 0.0,
        "v4 passes no options: enqueueJob's default priority"
    );
    assert_eq!(*max_attempts, 3.0);
    // `status`/`attempts` are not pinned: the served host's job pump may pick
    // the row up before this read (it cannot complete it — no provider).
    let payload: Value = serde_json::from_str(payload).unwrap();
    let keys: Vec<&str> = payload
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["chatId", "connectionProfileId", "forceRegenerate"]);
    assert_eq!(payload["chatId"], chat_id.as_str());
    assert_eq!(payload["forceRegenerate"], false);
    assert!(payload["connectionProfileId"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_autonomous_room_is_refused_with_409_and_nothing_written() {
    let (base, chat_id) = instance(|c, id| {
        c.execute(
            "UPDATE chats SET chatType = 'autonomous', runState = 'running' WHERE id = ?1",
            [id],
        )
        .unwrap();
    });
    let before = summary_jobs(base.path());
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let (status, text) = post(
        addr,
        json!({ "type": "chatRebuildSummary", "chatId": chat_id }),
    )
    .await;
    assert_eq!(status, 409, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        sentence(&v),
        "Pause the room before rebuilding its summary."
    );
    let (summary, _, last_summary_turn, _) = summary_state(base.path(), &chat_id);
    assert!(summary.is_some(), "the refusal clears nothing");
    assert_eq!(last_summary_turn, 10.0);
    assert!(
        new_jobs(&before, summary_jobs(base.path())).is_empty(),
        "the refusal enqueues nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn no_connection_profiles_is_refused_with_400_and_nothing_written() {
    let (base, chat_id) = instance(|c, _| {
        c.execute("DELETE FROM connection_profiles", []).unwrap();
    });
    let before = summary_jobs(base.path());
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let (status, text) = post(
        addr,
        json!({ "type": "chatRebuildSummary", "chatId": chat_id }),
    )
    .await;
    assert_eq!(status, 400, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(sentence(&v), "No connection profiles available");
    let (summary, _, _, _) = summary_state(base.path(), &chat_id);
    assert!(summary.is_some(), "the refusal clears nothing");
    assert!(
        new_jobs(&before, summary_jobs(base.path())).is_empty(),
        "the refusal enqueues nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_chat_is_404_and_a_wrong_type_chat_id_is_refused_at_decode() {
    let (base, _chat_id) = instance(|_, _| {});
    let before = summary_jobs(base.path());
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;

    let (status, text) = post(
        addr,
        json!({ "type": "chatRebuildSummary", "chatId": MISSING_CHAT }),
    )
    .await;
    assert_eq!(status, 404, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(sentence(&v), "Chat not found");

    let (status, text) = post(addr, json!({ "type": "chatRebuildSummary", "chatId": 7 })).await;
    assert_eq!(
        status, 400,
        "a numeric chatId never reaches the handler: {text}"
    );
    let (status, text) = post(addr, json!({ "type": "chatRebuildSummary" })).await;
    assert_eq!(
        status, 400,
        "an absent chatId never reaches the handler: {text}"
    );
    assert!(new_jobs(&before, summary_jobs(base.path())).is_empty());
}
