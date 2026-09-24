//! P4.D217 (v4 `d1c06cd9d`) — the Scenario Builder's DISPATCH WIRE.
//!
//! Three verbs (`scenarioBuilderBuild` / `scenarioBuilderAbort` /
//! `scenarioBuilderCapabilities`) and one Event family
//! (`scenarioBuilderProgress`) cross the boundary here, and what a core-side
//! family cannot see lives above it:
//!
//! 1. **Does the RAW body survive serde?** `body` is one `serde_json::Value`, so
//!    an explicit `null` `mode`, a missing `connectionProfileId` and a
//!    wrong-typed `characterIds` must each reach the Zod twin and come back as
//!    v4's `400 {error: 'Validation error', details: [...]}` — only a real
//!    `POST /api/dispatch` proves no serde layer collapsed them first.
//! 2. **Every §S.1 refusal, in v4's order, with its exact status and body**:
//!    404 `Connection profile not found` (missing AND foreign), 400 tools-off,
//!    400 the api-key sentence, 404 `Chat not found` (missing AND foreign), 400
//!    not-a-Salon — plus v5's 409 for a runId already in flight, checked FIRST.
//! 3. **The frames reach `/api/events` scope-tagged by the runId**, and the
//!    dispatch reply carries the SAME terminal object as the last frame.
//! 4. **Abort**: an unknown id answers `{aborted: false}` (never an error); a
//!    live one answers `{aborted: true}`, the run's reply is `{aborted: true}`,
//!    and NO terminal frame is published (v4's aborted run enqueues nothing).
//! 5. **Capabilities** answer `{webSearchConfigured: false, curlConfigured:
//!    false}` on a host with no search provider (curl: the recorded §R.4(k)
//!    divergence — always `false` on v5).
//!
//! The state the refusals need is PLANTED on the per-run copy of the chat-send
//! fixture (profiles and chats cloned from the fixture's own rows): a
//! tools-on OLLAMA profile (a provider that accepts no key, so the api-key gate
//! passes), a tools-OFF clone, an ANTHROPIC clone with no key, a FOREIGN-owned
//! clone; a Salon chat, a Brahma chat, a foreign chat.
//!
//! Run:
//!   cargo test -p quilltap-web --test scenario_builder_dispatch_wire

mod common;
mod scenario_builder_spine;

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value};

use scenario_builder_spine::{Canned, ScenarioBuilderSpineFactory, FAILING_STREAM_MESSAGE, SCENE};

const PROFILE_OK: &str = "5b170000-0000-4000-8000-0000000000a1";
const PROFILE_TOOLS_OFF: &str = "5b170000-0000-4000-8000-0000000000a2";
const PROFILE_NO_KEY: &str = "5b170000-0000-4000-8000-0000000000a3";
const PROFILE_FOREIGN: &str = "5b170000-0000-4000-8000-0000000000a4";
const CHAT_SALON: &str = "5b170000-0000-4000-8000-0000000000c1";
const CHAT_BRAHMA: &str = "5b170000-0000-4000-8000-0000000000c2";
const CHAT_FOREIGN: &str = "5b170000-0000-4000-8000-0000000000c3";
const NOBODY: &str = "5b170000-0000-4000-8000-0000000000ff";
const OTHER_USER: &str = "5b170000-0000-4000-8000-0000000000ee";

/// Clone one row of `table` (the first, by rowid) under a new id with `sets`
/// applied — every other column is the fixture's own.
fn clone_row(conn: &rusqlite::Connection, table: &str, new_id: &str, sets: &str) {
    conn.execute_batch(&format!(
        "CREATE TEMP TABLE \"p4d217_clone\" AS SELECT * FROM \"{table}\" ORDER BY rowid LIMIT 1;
         UPDATE \"p4d217_clone\" SET \"id\" = '{new_id}', {sets};
         INSERT INTO \"{table}\" SELECT * FROM \"p4d217_clone\";
         DROP TABLE \"p4d217_clone\";"
    ))
    .unwrap_or_else(|e| panic!("plant {table} {new_id}: {e}"));
}

/// Materialize the chat-send instance and plant the refusal matrix on it.
fn planted_instance() -> tempfile::TempDir {
    let base = common::materialize_fixture_instance();
    let w = quilltap_core::db::Writer::open_writable(
        &base.path().join("data").join("quilltap.db"),
        common::TEST_PEPPER,
    )
    .unwrap();
    let c = w.connection();
    let owner: String = c
        .query_row(
            "SELECT \"userId\" FROM \"connection_profiles\" ORDER BY rowid LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("the fixture carries a connection profile");
    clone_row(
        c,
        "connection_profiles",
        PROFILE_OK,
        "\"name\" = 'Host OK', \"provider\" = 'OLLAMA', \"apiKeyId\" = NULL, \"allowToolUse\" = 1, \"isDefault\" = 0",
    );
    clone_row(
        c,
        "connection_profiles",
        PROFILE_TOOLS_OFF,
        "\"name\" = 'Host Off', \"provider\" = 'OLLAMA', \"apiKeyId\" = NULL, \"allowToolUse\" = 0, \"isDefault\" = 0",
    );
    clone_row(
        c,
        "connection_profiles",
        PROFILE_NO_KEY,
        "\"name\" = 'Host Keyless', \"provider\" = 'ANTHROPIC', \"apiKeyId\" = NULL, \"allowToolUse\" = 1, \"isDefault\" = 0",
    );
    clone_row(
        c,
        "connection_profiles",
        PROFILE_FOREIGN,
        &format!("\"name\" = 'Host Foreign', \"provider\" = 'OLLAMA', \"apiKeyId\" = NULL, \"allowToolUse\" = 1, \"isDefault\" = 0, \"userId\" = '{OTHER_USER}'"),
    );
    clone_row(c, "chats", CHAT_SALON, "\"chatType\" = 'salon'");
    clone_row(c, "chats", CHAT_BRAHMA, "\"chatType\" = 'brahma'");
    clone_row(
        c,
        "chats",
        CHAT_FOREIGN,
        &format!("\"chatType\" = 'salon', \"userId\" = '{OTHER_USER}'"),
    );
    // The owner the engine reads as SINGLE_USER must own the planted rows.
    assert!(!owner.is_empty());
    drop(w);
    base
}

struct Wire {
    client: reqwest::Client,
    url: String,
}
impl Wire {
    async fn post(&self, body: Value) -> (u16, Value) {
        let r = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = r.status().as_u16();
        (status, r.json::<Value>().await.unwrap_or(Value::Null))
    }
}

fn build(run_id: &str, body: Value) -> Value {
    json!({ "type": "scenarioBuilderBuild", "runId": run_id, "body": body })
}

fn good_body() -> Value {
    json!({
        "mode": "in-world",
        "location": "The quay",
        "time": "dusk",
        "connectionProfileId": PROFILE_OK,
    })
}

fn with(mut b: Value, k: &str, v: Value) -> Value {
    b[k] = v;
    b
}

/// Collect every `data:` frame that arrives within `window`.
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

fn run_frames(frames: &[Value], run_id: &str) -> Vec<Value> {
    frames
        .iter()
        .filter(|f| {
            f.get("type").and_then(Value::as_str) == Some("scenarioBuilderProgress")
                && f.get("progressId").and_then(Value::as_str) == Some(run_id)
        })
        .map(|f| f.get("frame").cloned().unwrap_or(Value::Null))
        .collect()
}

async fn serve(canned: Canned) -> (tempfile::TempDir, Wire, std::net::SocketAddr) {
    let base = planted_instance();
    let base_dir = base.path().to_path_buf();
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(ScenarioBuilderSpineFactory {
            base_dir,
            canned,
            driver: None,
        }));
        c
    })
    .await;
    let wire = Wire {
        client: reqwest::Client::new(),
        url: format!("http://{addr}/api/dispatch"),
    };
    (base, wire, addr)
}

/// `{"type":"error","data":{…}}` → `(kind, message, details)`.
fn err(v: &Value) -> (String, String, Value) {
    (
        v["data"]["kind"].as_str().unwrap_or_default().to_string(),
        v["data"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        v["data"]["details"].clone(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn every_refusal_answers_v4s_status_and_body_before_any_frame() {
    let (_base, wire, _addr) = serve(Canned::Scene).await;

    // 1. The Zod twin through the RAW body — three shapes serde must not eat.
    let (status, v) = wire
        .post(build("r-null-mode", with(good_body(), "mode", Value::Null)))
        .await;
    let (kind, message, details) = err(&v);
    assert_eq!(
        (status, kind.as_str(), message.as_str()),
        (400, "bad-request", "Validation error"),
        "{v}"
    );
    assert_eq!(
        details,
        json!([{"code":"invalid_value","values":["real","in-world"],"path":["mode"],"message":"Invalid option: expected one of \"real\"|\"in-world\""}]),
        "an explicit null mode reaches the enum's invalid_value"
    );
    let mut no_profile = good_body();
    no_profile
        .as_object_mut()
        .unwrap()
        .remove("connectionProfileId");
    let (status, v) = wire.post(build("r-missing-profile", no_profile)).await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(
        err(&v).2,
        json!([{"expected":"string","code":"invalid_type","path":["connectionProfileId"],"message":"Invalid input: expected string, received undefined"}])
    );
    let (status, v) = wire
        .post(build(
            "r-cast-string",
            with(good_body(), "characterIds", json!("x")),
        ))
        .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(
        err(&v).2,
        json!([{"expected":"array","code":"invalid_type","path":["characterIds"],"message":"Invalid input: expected array, received string"}])
    );
    // An absent `body` is v4's `{}` (the four required-field issues).
    let (status, v) = wire
        .post(json!({ "type": "scenarioBuilderBuild", "runId": "r-no-body" }))
        .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(err(&v).2.as_array().map(Vec::len), Some(4), "{v}");

    // 2. The profile: missing, foreign, tools-off, no key.
    for (pid, label) in [(NOBODY, "missing"), (PROFILE_FOREIGN, "foreign")] {
        let (status, v) = wire
            .post(build(
                &format!("r-profile-{label}"),
                with(good_body(), "connectionProfileId", json!(pid)),
            ))
            .await;
        assert_eq!(status, 404, "{label}: {v}");
        assert_eq!(err(&v).1, "Connection profile not found");
    }
    let (status, v) = wire
        .post(build(
            "r-tools-off",
            with(good_body(), "connectionProfileId", json!(PROFILE_TOOLS_OFF)),
        ))
        .await;
    assert_eq!(
        (status, err(&v).1.as_str()),
        (400, quilltap_core::api::scenario_builder::TOOLS_OFF)
    );
    let (status, v) = wire
        .post(build(
            "r-no-key",
            with(good_body(), "connectionProfileId", json!(PROFILE_NO_KEY)),
        ))
        .await;
    assert_eq!(
        (status, err(&v).1.as_str()),
        (400, "No API key configured for this connection profile")
    );

    // 3. The chat: missing, foreign, not a Salon.
    for (cid, label) in [(NOBODY, "missing"), (CHAT_FOREIGN, "foreign")] {
        let (status, v) = wire
            .post(build(
                &format!("r-chat-{label}"),
                with(good_body(), "chatId", json!(cid)),
            ))
            .await;
        assert_eq!(
            (status, err(&v).1.as_str()),
            (404, "Chat not found"),
            "{label}: {v}"
        );
    }
    let (status, v) = wire
        .post(build(
            "r-brahma",
            with(good_body(), "chatId", json!(CHAT_BRAHMA)),
        ))
        .await;
    assert_eq!(
        (status, err(&v).1.as_str()),
        (400, quilltap_core::api::scenario_builder::NOT_A_SALON)
    );

    // v4's ORDER, multi-fault: a missing profile beats a bad chat (404 before
    // the chat is read), and a Zod issue beats both.
    let (status, v) = wire
        .post(build(
            "r-order",
            with(
                with(good_body(), "connectionProfileId", json!(NOBODY)),
                "chatId",
                json!(CHAT_BRAHMA),
            ),
        ))
        .await;
    assert_eq!(
        (status, err(&v).1.as_str()),
        (404, "Connection profile not found")
    );
    let (status, v) = wire
        .post(build(
            "r-order-2",
            with(
                with(good_body(), "connectionProfileId", json!(NOBODY)),
                "location",
                json!(""),
            ),
        ))
        .await;
    assert_eq!((status, err(&v).1.as_str()), (400, "Validation error"));

    // 4. Capabilities, and an abort of an id that was never live.
    let (status, v) = wire
        .post(json!({ "type": "scenarioBuilderCapabilities" }))
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        v,
        json!({ "type": "scenarioBuilder", "data": { "webSearchConfigured": false, "curlConfigured": false } })
    );
    let (status, v) = wire
        .post(json!({ "type": "scenarioBuilderAbort", "runId": "never-live" }))
        .await;
    assert_eq!(
        (status, v),
        (
            200,
            json!({ "type": "scenarioBuilder", "data": { "aborted": false } })
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_canned_run_publishes_its_frames_under_the_run_id_and_replies_the_terminal_frame() {
    let (_base, wire, addr) = serve(Canned::Scene).await;
    let events = wire
        .client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), 200);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let run_id = "5b170000-0000-4000-8000-00000000d0e1";
    let body = with(good_body(), "chatId", json!(CHAT_SALON));
    let dispatch = {
        let client = wire.client.clone();
        let url = wire.url.clone();
        tokio::spawn(async move {
            client
                .post(&url)
                .json(&build(run_id, body))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        })
    };
    let frames = collect_frames(events, Duration::from_secs(10)).await;
    let reply = dispatch.await.unwrap();
    let mine = run_frames(&frames, run_id);
    assert_eq!(
        mine.len(),
        1,
        "a tool-less plain answer publishes ONE frame (the done): {mine:?}"
    );
    let done = &mine[0];
    assert_eq!(
        done.to_string(),
        format!(
            r#"{{"done":true,"scenario":"{}","provider":"OLLAMA","modelName":{},"usage":{{"promptTokens":30,"completionTokens":9,"totalTokens":39}},"toolsExecuted":0,"webAvailable":false}}"#,
            SCENE.trim(),
            done["modelName"]
        ),
        "the done frame's bytes, v4's key order"
    );
    assert_eq!(
        reply,
        json!({ "type": "scenarioBuilder", "data": done }),
        "the reply IS the terminal frame"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_throwing_stream_is_the_detained_frame() {
    let (_base, wire, addr) = serve(Canned::Failing).await;
    let events = wire
        .client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let run_id = "5b170000-0000-4000-8000-00000000d0e2";
    let dispatch = {
        let client = wire.client.clone();
        let url = wire.url.clone();
        tokio::spawn(async move {
            client
                .post(&url)
                .json(&build(run_id, good_body()))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        })
    };
    let frames = collect_frames(events, Duration::from_secs(10)).await;
    let reply = dispatch.await.unwrap();
    let mine = run_frames(&frames, run_id);
    let want = json!({
        "error": quilltap_core::services::scenario_builder::DETAINED,
        "errorType": "scenario_builder_failed",
        "details": FAILING_STREAM_MESSAGE,
    });
    assert_eq!(mine, vec![want.clone()]);
    assert_eq!(reply["data"], want);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_duplicate_run_id_is_409_and_abort_ends_a_live_run_with_no_terminal_frame() {
    let (_base, wire, addr) = serve(Canned::Slow).await;
    let events = wire
        .client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let run_id = "5b170000-0000-4000-8000-00000000d0e3";
    let dispatch = {
        let client = wire.client.clone();
        let url = wire.url.clone();
        tokio::spawn(async move {
            client
                .post(&url)
                .json(&build(run_id, good_body()))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        })
    };
    // Let the slow stream start, then collide and abort.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (status, v) = wire.post(build(run_id, good_body())).await;
    assert_eq!(
        (status, err(&v).1.as_str()),
        (409, quilltap_core::api::scenario_builder::DUPLICATE_RUN),
        "a live id refuses before any v4 check: {v}"
    );
    let (status, v) = wire
        .post(json!({ "type": "scenarioBuilderAbort", "runId": run_id }))
        .await;
    assert_eq!(
        (status, v),
        (
            200,
            json!({ "type": "scenarioBuilder", "data": { "aborted": true } })
        )
    );
    let reply = tokio::time::timeout(Duration::from_secs(10), dispatch)
        .await
        .expect("the aborted run's reply arrives promptly")
        .unwrap();
    assert_eq!(
        reply,
        json!({ "type": "scenarioBuilder", "data": { "aborted": true } })
    );
    let frames = collect_frames(events, Duration::from_secs(2)).await;
    let mine = run_frames(&frames, run_id);
    assert!(
        mine.iter()
            .all(|f| f.get("done").is_none() && f.get("error").is_none()),
        "an aborted run publishes NO terminal frame: {mine:?}"
    );
    // The id is free again once the run has ended.
    let (_, v) = wire
        .post(json!({ "type": "scenarioBuilderAbort", "runId": run_id }))
        .await;
    assert_eq!(v["data"], json!({ "aborted": false }));
}
