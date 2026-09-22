//! P4.D207 tier-2 — the swipe's REST edge against v4's REAL route handler.
//!
//! `POST /api/v1/messages/{id}?action=swipe&stream=1` is new in v5
//! (`crates/quilltap-web/src/messages_swipe_routes.rs`; v5 had no POST edge on
//! this path at all). The oracle drives v4's REAL
//! `app/api/v1/messages/[id]/route.ts` `POST` twice per case — once without the
//! flag and once with it — and records the SSE leg's status, its three headers,
//! and the DECODED `data:` frames, throwing if any chunk is not v4's
//! `data: <json>\n\n` framing. This family replays the same four cases over the
//! real HTTP edge against the SAME committed salon pair and diffs them.
//!
//! **What makes this a byte diff rather than a shape check:** the canned
//! provider here yields the same two prose deltas v4's mocked `streamMessage`
//! yields (`swipe_spine::SWIPE_DELTAS`), against the same fixture, for the same
//! message ids. So every frame but the terminal `done` — whose `message`
//! carries a freshly minted swipe id — must match v4's recorded bytes exactly.
//!
//! **The refusal rule is the point of the other three cases.** v4's
//! `resolveSwipeTarget` runs BEFORE `new ReadableStream`, so a refusal is an
//! ordinary JSON error on the streaming leg too — same status, same sentence,
//! and NOT a 200 stream carrying an `error` frame. The oracle records
//! `kind: "json"` for all three, and this family asserts v5 answers JSON with
//! v4's status and copy.
//!
//! Regenerate the oracle (see `harness/oracle/cases/salon-swipe-generate.test.ts`
//! for the full recipe — it is the SAME oracle `salon_swipe_generate_equivalence`
//! reads, which is deliberate: one run records both the route-level tables and
//! this stream shape, so the two families cannot disagree about what v4 did):
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-salon-swipe.ndjson npx jest -- salon-swipe-generate
//! Run:
//!   QT_ORACLE_SALON_SWIPE=/tmp/oracle-salon-swipe.ndjson \
//!     cargo test -p quilltap-web --test messages_swipe_sse_route

mod common;
mod swipe_spine;

use std::sync::Arc;

use futures_util::StreamExt;
use serde_json::Value;
use swipe_spine::{SwipeSpineFactory, SWIPE_DELTAS};

/// The salon fixture's three swipe targets, as
/// `harness/oracle/cases/salon-swipe-generate.test.ts` names them.
const GROUP_ASSISTANT: &str = "d2000000-0000-4000-8000-000000000007";
const GROUP_USER: &str = "d2000000-0000-4000-8000-000000000001";
const GROUP_HOST: &str = "d2000000-0000-4000-8000-000000000003";
const MISSING: &str = "99999999-9999-4999-8999-999999999999";

/// Materialize an instance dir from the committed SALON pair — the fixture the
/// oracle runs against. Built here rather than as a `common::materialize_*`
/// twin (the `rewrite_fixture_user_ids` doc's own case).
fn materialize_salon_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        common::fixtures_dir().join("salon-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        common::fixtures_dir().join("salon-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(
            &data.join("quilltap.db"),
            common::TEST_PEPPER,
        )
        .unwrap();
        common::rewrite_fixture_user_ids(w.connection());
        // P4.D172: the committed salon pair predates the two `78b381a96`
        // schema moves, exactly as `salon_swipe_generate_equivalence` heals it.
        quilltap_core::test_support::ensure_p4d171_columns(w.connection());
    }
    base
}

/// The oracle's per-case record.
fn oracle_case(text: &str, name: &str) -> Value {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("name").and_then(Value::as_str) == Some(name) {
            return v;
        }
    }
    panic!("oracle has no case {name} — re-record it")
}

/// Read a `text/event-stream` body into its decoded frames, asserting v4's
/// framing on the way: every chunk is `data: <json>` and nothing else — no
/// `event:` line, no `id:`, no retry, no keep-alive comment.
async fn read_sse(resp: reqwest::Response) -> Vec<Value> {
    let mut stream = resp.bytes_stream();
    let mut text = String::new();
    while let Some(Ok(bytes)) = stream.next().await {
        text.push_str(&String::from_utf8_lossy(&bytes));
    }
    text.split("\n\n")
        .filter(|c| !c.is_empty())
        .map(|chunk| {
            let payload = chunk.strip_prefix("data: ").unwrap_or_else(|| {
                panic!("unexpected SSE chunk framing: {chunk:?} — v4 writes `data: <json>\\n\\n`")
            });
            assert!(
                !payload.contains('\n'),
                "an SSE chunk carried more than one line: {chunk:?}"
            );
            serde_json::from_str(payload).expect("a JSON frame")
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_sse_edge_matches_v4s_real_route() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SALON_SWIPE") else {
        eprintln!("SKIP: QT_ORACLE_SALON_SWIPE not set");
        return;
    };
    let oracle = std::fs::read_to_string(&oracle_path).expect("oracle readable");

    let client = reqwest::Client::new();

    // ---- The three REFUSALS: JSON on the streaming leg too, same status and
    // sentence as the non-stream leg, and never a stream.
    for (name, message_id) in [
        ("swipe_not_assistant", GROUP_USER),
        ("swipe_systemsender", GROUP_HOST),
        ("swipe_not_found", MISSING),
    ] {
        let want = oracle_case(&oracle, name);
        let want_stream = &want["stream"];
        assert_eq!(
            want_stream["kind"].as_str(),
            Some("json"),
            "[{name}] the oracle says v4 answered a STREAM for a refusal — re-read \
             `resolveSwipeTarget`'s position"
        );

        let base = materialize_salon_instance();
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

        let resp = client
            .post(format!(
                "http://{addr}/api/v1/messages/{message_id}?action=swipe&stream=1"
            ))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();

        assert_eq!(
            resp.status().as_u16() as i64,
            want_stream["status"].as_i64().unwrap(),
            "[{name}] status on the streaming leg"
        );
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            !ct.starts_with("text/event-stream"),
            "[{name}] a refusal opened a STREAM ({ct}) — v4 answers an ordinary \
             JSON error, because the guards run before `new ReadableStream`"
        );
        let body: Value = resp.json().await.unwrap();
        assert_eq!(
            body.get("error").and_then(Value::as_str),
            want_stream["body"]["error"].as_str(),
            "[{name}] v4's error copy, verbatim"
        );
    }

    // ---- The HAPPY case: v4's three headers and v4's frames, byte for byte
    // bar the minted swipe id.
    let want = oracle_case(&oracle, "swipe_happy");
    let want_stream = &want["stream"];
    assert_eq!(want_stream["kind"].as_str(), Some("sse"));

    let base = materialize_salon_instance();
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

    // ⚠ **The oracle runs the non-stream leg FIRST, on the same database**, and
    // the stream leg then regenerates a message that is already in a swipe
    // group — so v4's recorded terminal frame carries `swipeIndex: 2`, not 1.
    // Reproducing the sequence is the whole comparison; skipping it reads as a
    // port defect (`left: 1, right: 2`) when it is a venue difference. Measured
    // on this family's first run.
    let warmup = client
        .post(format!(
            "http://{addr}/api/v1/messages/{GROUP_ASSISTANT}?action=swipe"
        ))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        warmup.status().as_u16(),
        201,
        "the oracle's first pass — the non-stream leg — must succeed here too"
    );

    let resp = client
        .post(format!(
            "http://{addr}/api/v1/messages/{GROUP_ASSISTANT}?action=swipe&stream=1"
        ))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 200);
    // v4's `SSE_HEADERS`, all three.
    for (header, want_value) in [
        ("content-type", "text/event-stream"),
        ("cache-control", "no-cache"),
        ("connection", "keep-alive"),
    ] {
        assert_eq!(
            want_stream["headers"][header].as_str(),
            Some(want_value),
            "the oracle's own record of v4's `{header}`"
        );
        let got = resp
            .headers()
            .get(header)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(
            got.starts_with(want_value),
            "header `{header}`: got {got:?}, v4 sends {want_value:?}"
        );
    }

    let got_frames = read_sse(resp).await;
    let want_frames: Vec<Value> = want_stream["frames"].as_array().cloned().unwrap();

    assert_eq!(
        got_frames.len(),
        want_frames.len(),
        "frame COUNT\n--- got ---\n{}\n--- want ---\n{}",
        serde_json::to_string_pretty(&got_frames).unwrap(),
        serde_json::to_string_pretty(&want_frames).unwrap()
    );

    // Every frame but the terminal `done` is byte-identical: same beats, same
    // sentences, same `kind`, same deltas.
    for (i, (got, want)) in got_frames.iter().zip(want_frames.iter()).enumerate() {
        if want.get("done").is_some() {
            continue;
        }
        assert_eq!(
            got, want,
            "frame {i} differs\n  got : {got}\n  want: {want}"
        );
    }

    // The deltas, spelled out, so a change to the canned stream on either side
    // cannot quietly make the comparison vacuous.
    let deltas: Vec<&str> = got_frames
        .iter()
        .filter_map(|f| f.get("content").and_then(Value::as_str))
        .collect();
    assert_eq!(deltas, SWIPE_DELTAS.to_vec());

    // The terminal frame: `done` last, carrying the persisted swipe. Its id is
    // minted, so the comparand is everything else.
    let got_done = got_frames.last().expect("a terminal frame");
    let want_done = want_frames.last().expect("v4's terminal frame");
    assert_eq!(got_done.get("done").and_then(Value::as_bool), Some(true));
    assert_eq!(want_done.get("done").and_then(Value::as_bool), Some(true));
    for field in [
        "role",
        "content",
        "participantId",
        "swipeGroupId",
        "swipeIndex",
        "tokenCount",
        "promptTokens",
        "completionTokens",
        "provider",
        "modelName",
        "createdAt",
    ] {
        assert_eq!(
            got_done.pointer(&format!("/message/{field}")),
            want_done.pointer(&format!("/message/{field}")),
            "the terminal frame's `message.{field}`"
        );
    }
    assert_eq!(
        got_done.pointer("/message/content").and_then(Value::as_str),
        Some(SWIPE_DELTAS.concat().as_str()),
        "the persisted content is the CONCATENATION of the deltas"
    );
}

/// The NON-stream leg of the same edge: v4 answers **201** with
/// `{ message }` (`created(...)`), not 200.
#[tokio::test(flavor = "multi_thread")]
async fn without_the_flag_the_edge_answers_201_json() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SALON_SWIPE") else {
        eprintln!("SKIP: QT_ORACLE_SALON_SWIPE not set");
        return;
    };
    let oracle = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let want = oracle_case(&oracle, "swipe_happy");
    assert_eq!(
        want["status"].as_i64(),
        Some(201),
        "v4's non-stream leg is a 201"
    );

    let base = materialize_salon_instance();
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

    let resp = reqwest::Client::new()
        .post(format!(
            "http://{addr}/api/v1/messages/{GROUP_ASSISTANT}?action=swipe"
        ))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 201);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body.pointer("/message/content").and_then(Value::as_str),
        Some(SWIPE_DELTAS.concat().as_str()),
        "the 201 body carries the persisted swipe: {body}"
    );
}

/// The route's own dispatcher: the SWITCH branch, the loud `reattribute`
/// deferral, and v4's sentence for everything else.
#[tokio::test(flavor = "multi_thread")]
async fn the_post_dispatcher_serves_swipe_defers_reattribute_and_refuses_the_rest() {
    let base = materialize_salon_instance();
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
    let client = reqwest::Client::new();

    // A `swipeIndex` in the body takes the SWITCH branch — 200, not 201, and
    // no generation happens.
    //
    // ⚠ The target must already BE in a swipe group, or v4's switch answers
    // 400 `Message is not part of a swipe group`. The salon fixture's message
    // is not grouped until something regenerates it, so this generates once
    // first — which is also what makes the 200 below meaningful (measured: the
    // first run of this test read 400 and looked like a routing bug).
    let seed = client
        .post(format!(
            "http://{addr}/api/v1/messages/{GROUP_ASSISTANT}?action=swipe"
        ))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(seed.status().as_u16(), 201, "the seeding generation");

    let resp = client
        .post(format!(
            "http://{addr}/api/v1/messages/{GROUP_ASSISTANT}?action=swipe"
        ))
        .json(&serde_json::json!({ "swipeIndex": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200, "the switch answers 200");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body.pointer("/message/swipeIndex").and_then(Value::as_i64),
        Some(0),
        "…carrying the variant asked for, not a new one: {body}"
    );

    // `reattribute` is a v5 VERB with no REST edge — a loud, typed deferral
    // naming itself, NOT v4's "Action parameter required" (which would claim
    // the action is unrecognised).
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/messages/{GROUP_ASSISTANT}?action=reattribute"
        ))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 501);
    let body: Value = resp.json().await.unwrap();
    let msg = body["error"].as_str().unwrap_or_default();
    assert!(
        msg.contains("reattribute") && msg.contains("not yet served"),
        "the deferral names itself: {body}"
    );

    // An unknown action and an ABSENT one take v4's one sentence.
    for url in [
        format!("http://{addr}/api/v1/messages/{GROUP_ASSISTANT}?action=nonsense"),
        format!("http://{addr}/api/v1/messages/{GROUP_ASSISTANT}"),
    ] {
        let resp = client
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 400, "{url}");
        let body: Value = resp.json().await.unwrap();
        assert_eq!(
            body["error"].as_str(),
            Some("Action parameter required: swipe or reattribute"),
            "v4's sentence, verbatim: {url}"
        );
    }
}

/// v4 reads the body as `await req.json().catch(() => ({}))`
/// (`app/api/v1/messages/[id]/route.ts:257`), so a body that is not JSON at
/// all — a form post, or malformed JSON — is `{}` and falls straight through
/// to the GENERATE branch. It is NOT a 415 and NOT a 400, which is what an
/// `Option<axum::Json<Value>>` extractor would have answered (its rejection
/// fires on the content-type before the handler runs, and on a parse error
/// after).
#[tokio::test(flavor = "multi_thread")]
async fn a_body_that_is_not_json_takes_the_generate_path() {
    let base = materialize_salon_instance();
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
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/v1/messages/{GROUP_ASSISTANT}?action=swipe");

    // (a) a form-encoded body: `application/x-www-form-urlencoded`, which the
    //     JSON extractor rejects with 415 before the handler is ever called.
    let resp = client
        .post(&url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body("swipeIndex=0")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        201,
        "v4 generates here; a 415 would be axum's extractor talking, not v4"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body.pointer("/message/content").and_then(Value::as_str),
        Some(SWIPE_DELTAS.concat().as_str()),
        "…and it really generated, rather than switching to the form's \
         `swipeIndex`: {body}"
    );

    // (b) malformed JSON under the JSON content-type: the extractor's 400.
    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .body("{\"swipeIndex\": ")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        201,
        "v4's `.catch(() => ({{}}))` swallows the parse error and generates"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body.pointer("/message/content").and_then(Value::as_str),
        Some(SWIPE_DELTAS.concat().as_str()),
        "{body}"
    );
}
