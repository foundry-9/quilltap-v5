//! P4.D228 (v4 `ce2f1dabf`, #77) — "Try uncensored" on a text line
//! (`messageRetryUncensored { messageId, stream }`) AT THE DISPATCH WIRE.
//!
//! `retry_uncensored_tier3` drives `api::salon::message_retry_uncensored`
//! directly against v4's real route (the gate order, the 409 kinds, what the
//! route hands the swipe service). What it cannot see, and this does:
//!
//! 1. the trip through serde (`stream` optional, `messageId` required);
//! 2. that EVERY refusal is an ordinary dispatch error BEFORE any frame —
//!    §S.3's contract, the same rule the refresh icon's swipe keeps;
//! 3. the host half of the override (E.3): the real spine's `run_swipe`
//!    resolves the understudy's context limit and composes the swipe on THAT
//!    profile — the persisted row's `provider`/`modelName` are the
//!    understudy's and its `routeTrail` ends on a `via: 'concierge'`,
//!    `outcome: 'answered'` row — narrated on `swipeProgress` keyed by the
//!    TARGET message id, ending in `{done: true, message}`;
//! 4. the 409 kinds as the SPA reads them: `kind: 'conflict'` with the bare
//!    `locked` / `no-understudy` token as the message (§S.3).
//!
//! Run:
//!   cargo test -p quilltap-web --test message_retry_uncensored_dispatch_wire

mod common;
mod swipe_spine;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use quilltap_core::db::Writer;
use serde_json::{json, Value};

use swipe_spine::{SwipeSpineFactory, SWIPE_DELTAS};

/// The chat-send fixture's populated chat (ten USER/ASSISTANT pairs).
const POPULATED_CHAT_ID: &str = "c860cf74-128f-4a81-9a5c-6c2275f24302";
/// The understudy this test plants (a copy of a keyed profile).
const DESK_ID: &str = "d7de5c00-0000-4000-8000-00000000d35c";
const DESK_PROVIDER: &str = "OPENROUTER";
const DESK_MODEL: &str = "wire-uncensored";

fn main_db(base: &Path) -> std::path::PathBuf {
    base.join("data").join("quilltap.db")
}

/// Plant on the served copy: every existing connection profile NOT
/// uncensored-compatible, the Concierge on duty with no configured desk, and —
/// when `desk` — ONE keyed, compatible understudy on its own model.
fn plant(base: &Path, desk: bool, locked: bool) {
    let w = Writer::open_writable(&main_db(base), common::TEST_PEPPER).unwrap();
    let c = w.connection();
    quilltap_core::test_support::ensure_p4d225_columns(c);
    c.execute(
        "UPDATE connection_profiles SET isDangerousCompatible = 0",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE chat_settings SET conciergeSettings = '{\"enabled\":true}'",
        [],
    )
    .unwrap();
    if desk {
        c.execute_batch(&format!(
            "CREATE TEMP TABLE desk AS SELECT * FROM connection_profiles WHERE apiKeyId IS NOT NULL LIMIT 1;
             UPDATE desk SET id = '{DESK_ID}', name = 'Wire Desk', provider = '{DESK_PROVIDER}',
               modelName = '{DESK_MODEL}', isDangerousCompatible = 1;
             UPDATE desk SET transport = 'api' WHERE transport = 'courier';
             INSERT INTO connection_profiles SELECT * FROM desk;
             DROP TABLE desk;"
        ))
        .unwrap();
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM connection_profiles WHERE id = ?1 AND apiKeyId IS NOT NULL",
                [DESK_ID],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            n, 1,
            "the planted understudy must carry a key or nobody can take it"
        );
        let keys: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM api_keys k JOIN connection_profiles p ON p.apiKeyId = k.id \
                 WHERE p.id = ?1 AND k.userId = p.userId",
                [DESK_ID],
                |r| r.get(0),
            )
            .unwrap_or(-1);
        let owner: String = c
            .query_row(
                "SELECT userId FROM connection_profiles WHERE id = ?1",
                [DESK_ID],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            keys, 1,
            "the understudy's key must resolve for its owner {owner}"
        );
    }
    if locked {
        c.execute(
            "UPDATE chats SET conciergeMode = 'locked', conciergeModeSetBy = 'operator', conciergeModeReason = 'manual' WHERE id = ?1",
            [POPULATED_CHAT_ID],
        )
        .unwrap();
    }
}

async fn collect_frames(resp: reqwest::Response, window: Duration) -> Vec<Value> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        let chunk = match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(Ok(bytes))) => bytes,
            _ => break,
        };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buf.find("\n\n") {
            let frame = buf[..end].to_string();
            buf.drain(..end + 2);
            for line in frame.lines() {
                if let Some(payload) = line.strip_prefix("data: ") {
                    if let Ok(v) = serde_json::from_str::<Value>(payload) {
                        out.push(v);
                    }
                }
            }
        }
    }
    out
}

fn swipe_frames(frames: &[Value], progress_id: &str) -> Vec<Value> {
    frames
        .iter()
        .filter(|f| {
            f.get("type").and_then(Value::as_str) == Some("swipeProgress")
                && f.get("progressId").and_then(Value::as_str) == Some(progress_id)
        })
        .map(|f| f.get("frame").cloned().unwrap_or(Value::Null))
        .collect()
}

struct Wire {
    client: reqwest::Client,
    base: String,
}
impl Wire {
    async fn post(&self, body: Value) -> (u16, Value) {
        let r = self
            .client
            .post(format!("{}/api/dispatch", self.base))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = r.status().as_u16();
        (status, r.json::<Value>().await.unwrap_or(Value::Null))
    }
    async fn events(&self) -> reqwest::Response {
        let r = self
            .client
            .get(format!("{}/api/events", self.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        tokio::time::sleep(Duration::from_millis(200)).await;
        r
    }
}

async fn serve(desk: bool, locked: bool) -> (tempfile::TempDir, Wire) {
    let base = common::materialize_fixture_instance();
    plant(base.path(), desk, locked);
    let base_dir = base.path().to_path_buf();
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(SwipeSpineFactory {
            base_dir,
            fail_stream: false,
        }));
        c
    })
    .await;
    (
        base,
        Wire {
            client: reqwest::Client::new(),
            base: format!("http://{addr}"),
        },
    )
}

/// The populated chat's events: `(a character ASSISTANT id, a USER id, a
/// staff id if any)`.
async fn targets(wire: &Wire) -> (String, String) {
    let (status, v) = wire
        .post(json!({ "type": "chatMessageEvents", "chatId": POPULATED_CHAT_ID }))
        .await;
    assert_eq!(status, 200, "chatMessageEvents: {v}");
    let msgs = v["data"]["messages"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let assistant = msgs
        .iter()
        .find(|m| {
            m["role"] == "ASSISTANT"
                && m.get("systemSender")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
        })
        .and_then(|m| m["id"].as_str())
        .expect("a character ASSISTANT line")
        .to_string();
    let user = msgs
        .iter()
        .find(|m| m["role"] == "USER")
        .and_then(|m| m["id"].as_str())
        .expect("a USER line")
        .to_string();
    (assistant, user)
}

fn err(v: &Value) -> (&str, &str) {
    (
        v["data"]["kind"].as_str().unwrap_or_default(),
        v["data"]["message"].as_str().unwrap_or_default(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn every_refusal_answers_before_any_frame() {
    // No understudy anywhere: the gate answers `no-understudy`.
    let (_base, wire) = serve(false, false).await;
    let (assistant, user) = targets(&wire).await;
    let events = wire.events().await;

    let (s, v) = wire
        .post(json!({ "type": "messageRetryUncensored", "messageId": user, "stream": true }))
        .await;
    assert_eq!(s, 400, "{v}");
    assert_eq!(
        err(&v),
        ("bad-request", "Only assistant messages can be retried"),
        "the retry's OWN sentence — not the swipe's `swiped`"
    );

    let (s, v) = wire
        .post(json!({
            "type": "messageRetryUncensored",
            "messageId": "00000000-0000-4000-8000-0000000000ff",
            "stream": true,
        }))
        .await;
    assert_eq!(s, 404, "{v}");
    assert_eq!(err(&v).1, "Message not found");

    // `stream` is optional: the same gate answers with it absent.
    let (s, v) = wire
        .post(json!({ "type": "messageRetryUncensored", "messageId": assistant }))
        .await;
    assert_eq!(s, 409, "{v}");
    assert_eq!(
        err(&v),
        ("conflict", "no-understudy"),
        "§S.3: the 409's message is the BARE token the SPA maps"
    );
    let (s, v) = wire
        .post(json!({ "type": "messageRetryUncensored", "messageId": assistant, "stream": true }))
        .await;
    assert_eq!((s, err(&v)), (409, ("conflict", "no-understudy")), "{v}");

    // `messageId` is required: the serde decode refuses, before the core.
    let (s, v) = wire
        .post(json!({ "type": "messageRetryUncensored", "stream": true }))
        .await;
    assert_eq!(s, 400, "a missing messageId is a decode refusal: {v}");

    let frames = collect_frames(events, Duration::from_secs(2)).await;
    let any: Vec<&Value> = frames
        .iter()
        .filter(|f| f["type"] == "swipeProgress")
        .collect();
    assert!(
        any.is_empty(),
        "a REFUSAL narrated: {any:?} — every refusal is a JSON error before the stream opens"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_locked_chat_is_refused_before_the_lookup() {
    // A desk EXISTS, and the chat is Locked: `locked` is checked FIRST.
    let (_base, wire) = serve(true, true).await;
    let (assistant, _) = targets(&wire).await;
    let (s, v) = wire
        .post(json!({ "type": "messageRetryUncensored", "messageId": assistant, "stream": true }))
        .await;
    assert_eq!((s, err(&v)), (409, ("conflict", "locked")), "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_streamed_retry_lands_on_the_understudy_with_the_concierge_trail() {
    let (_base, wire) = serve(true, false).await;
    let (assistant, _) = targets(&wire).await;
    let events = wire.events().await;

    let dispatch = {
        let client = wire.client.clone();
        let url = format!("{}/api/dispatch", wire.base);
        let id = assistant.clone();
        tokio::spawn(async move {
            let r = client
                .post(&url)
                .json(&json!({ "type": "messageRetryUncensored", "messageId": id, "stream": true }))
                .send()
                .await
                .unwrap();
            (
                r.status().as_u16(),
                r.json::<Value>().await.unwrap_or(Value::Null),
            )
        })
    };
    let frames = collect_frames(events, Duration::from_secs(20)).await;
    let (status, reply) = dispatch.await.unwrap();
    assert_eq!(status, 200, "{reply}");

    let mine = swipe_frames(&frames, &assistant);
    let stages: Vec<&str> = mine
        .iter()
        .filter_map(|f| f.pointer("/status/stage").and_then(Value::as_str))
        .collect();
    assert_eq!(
        stages,
        vec!["gathering", "sending", "regenerating", "saving"],
        "the swipe's four beats on the SAME channel, keyed by the target: {mine:?}"
    );
    let done = mine.last().expect("a terminal frame");
    assert_eq!(done["done"], true, "{done}");
    let message = &done["message"];
    assert_eq!(message["content"], SWIPE_DELTAS.concat());
    assert_eq!(
        (message["provider"].as_str(), message["modelName"].as_str()),
        (Some(DESK_PROVIDER), Some(DESK_MODEL)),
        "the override fed the row: the swipe was generated on the understudy"
    );
    let trail = message["routeTrail"]
        .as_array()
        .expect("a routeTrail on the swipe");
    let last = trail.last().unwrap();
    assert_eq!(
        (
            last["profileId"].as_str(),
            last["via"].as_str(),
            last["outcome"].as_str()
        ),
        (Some(DESK_ID), Some("concierge"), Some("answered")),
        "the trail ends via the Concierge: {trail:?}"
    );
    assert!(
        last.get("profileKind").is_none(),
        "a connection row carries no profileKind: {last}"
    );
    assert_eq!(
        reply["data"]["message"]["id"], message["id"],
        "the reply carries the same persisted swipe"
    );
}
