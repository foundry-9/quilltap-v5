//! P4.D205 — the three Inform verbs over the REAL HTTP edge.
//!
//! `chat_informs_routes_equivalence` drives the HANDLERS (`api::chat_informs::*`)
//! against v4's recorded bytes; nothing drove the ROUTE. That gap hid a total
//! outage: `wardrobe_routes::unwrap_to_http` — the fan-out all three actions
//! leave through — matched only `Wardrobe | ChatOutfit | ChatDialog`, so
//! `ChatInform` / `ChatInforms` / `ChatInformCancelled` fell to the `_` arm and
//! **every success answered 500 `Unexpected core response`**. The P4.56
//! `BrahmaConsole` shape exactly, and for the same reason: only the ERROR arm
//! worked, because a refusal leaves through `CoreResponse::Error`.
//!
//! ⚠ **Pre-fix status, measured on this branch before the fix landed:** the four
//! success cases below answered **500** with `{"error":"Unexpected core
//! response"}` (`post_inform_answers_201_with_v4s_four_keys`,
//! `the_pending_batch_is_listed_then_cancelled`), and the two refusal cases
//! answered 400 but with the `details` array MISSING — v4's
//! `validationError(zodError)` puts `{error, details}` on the wire
//! (`lib/api/responses.ts:108-118`) and the shared `error_to_http` renders only
//! `{error}`. Both halves are asserted below, so either regression reddens here.
//!
//! The fixture is the committed SALON pair — the same one
//! `messages_swipe_sse_route` boots — whose `Group Expedition` chat seats two
//! LLM-controlled CHARACTERs plus a user seat, which is exactly the room the
//! eligibility rule needs. `chat_informs` itself is created by the boot chain's
//! `ensure_chat_informs_table`, as it is on a real pre-4.10 instance.
//!
//! No oracle env var: v4's statuses and body keys are read off
//! `app/api/v1/chats/[id]/actions/inform.ts` at the pin (`created(...)` = 201 for
//! the post, plain `NextResponse.json` = 200 for the list and the cancel) and the
//! byte-level body diff is `chat_informs_routes_equivalence`'s job. What this
//! family owns is the EDGE: status, content type, and that the body reaches the
//! wire at all.
//!
//! Run:
//!   cargo test -p quilltap-web --test chat_informs_rest_routes

mod common;

use serde_json::{json, Value};

/// `Group Expedition` — two LLM CHARACTER seats (`b2…001`, `b2…002`) and one
/// user seat (`b2…003`), per `harness/oracle/fixtures/salon.json`.
const CHAT: &str = "c1000000-0000-4000-8000-000000000002";
const SEAT_ARIA: &str = "b2000000-0000-4000-8000-000000000001";

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
        // The committed pair predates the `78b381a96` schema moves, exactly as
        // `messages_swipe_sse_route` heals it.
        quilltap_core::test_support::ensure_p4d171_columns(w.connection());
    }
    base
}

async fn boot() -> (tempfile::TempDir, std::net::SocketAddr) {
    let base = materialize_salon_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    (base, addr)
}

#[tokio::test(flavor = "multi_thread")]
async fn post_inform_answers_201_with_v4s_four_keys() {
    let (_base, addr) = boot().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://{addr}/api/v1/chats/{CHAT}?action=inform"))
        .json(&json!({
            "contentMarkdown": "The clock in the hall has stopped.",
            "targetParticipantIds": Value::Null,
        }))
        .send()
        .await
        .unwrap();

    // v4 `handleInform` ends in `created({...})` — 201, not 200.
    assert_eq!(
        resp.status().as_u16(),
        201,
        "v4 answers 201 for a posted inform (`created(...)`, inform.ts:133)"
    );
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default(),
        "application/json"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["success"], json!(true));
    assert!(
        body["batchId"].is_string(),
        "the batch id must reach the wire: {body}"
    );
    // `targetParticipantIds: null` was Everyone, so the record is PUBLIC and v4
    // echoes the RECORD targets (null), not the resolved seat list.
    assert_eq!(body["targetParticipantIds"], Value::Null);
    assert!(
        body["message"].is_object(),
        "the Host record message rides the body: {body}"
    );
    assert_eq!(body["message"]["systemKind"], json!("inform"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pending_batch_is_listed_then_cancelled() {
    let (_base, addr) = boot().await;
    let client = reqwest::Client::new();

    // One explicit target, so the batch is a whisper to a single seat.
    let posted: Value = client
        .post(format!("http://{addr}/api/v1/chats/{CHAT}?action=inform"))
        .json(&json!({
            "contentMarkdown": "You alone notice the draught.",
            "targetParticipantIds": [SEAT_ARIA],
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let batch_id = posted["batchId"].as_str().expect("a batch id").to_string();

    // ---- GET ?action=informs — v4's plain `NextResponse.json({ batches })`.
    let resp = client
        .get(format!("http://{addr}/api/v1/chats/{CHAT}?action=informs"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body: Value = resp.json().await.unwrap();
    let batches = body["batches"].as_array().expect("a batches array");
    assert_eq!(batches.len(), 1, "exactly the batch just posted: {body}");
    assert_eq!(batches[0]["batchId"], json!(batch_id));
    assert_eq!(
        batches[0]["contentMarkdown"],
        json!("You alone notice the draught.")
    );
    assert_eq!(
        batches[0]["pendingParticipantIds"],
        json!([SEAT_ARIA]),
        "the explicit single target is the only seat owed it"
    );

    // ---- POST ?action=cancel-inform — 200, three keys.
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/chats/{CHAT}?action=cancel-inform"
        ))
        .json(&json!({ "batchId": batch_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        200,
        "v4's cancel is a plain `NextResponse.json`, not a `created`"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["success"], json!(true));
    assert_eq!(body["removed"], json!(1));
    assert_eq!(
        body["recordDeleted"],
        json!(true),
        "nothing was consumed, so the record goes with the batch"
    );

    // And the chip is empty again.
    let body: Value = client
        .get(format!("http://{addr}/api/v1/chats/{CHAT}?action=informs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["batches"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_absent_key_answers_v4s_validation_envelope() {
    let (_base, addr) = boot().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://{addr}/api/v1/chats/{CHAT}?action=inform"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Validation error"));
    // v4's `validationError(zodError)` carries the ISSUES too
    // (`lib/api/responses.ts:108-118`); a `{error}`-only body is the shared
    // `error_to_http` swallowing them.
    let details = body["details"]
        .as_array()
        .unwrap_or_else(|| panic!("v4 puts `details` on the wire: {body}"));
    assert_eq!(details.len(), 2, "both required keys are missing: {body}");
    assert_eq!(details[0]["path"], json!(["contentMarkdown"]));
    assert_eq!(
        details[0]["message"],
        json!("Invalid input: expected string, received undefined")
    );
    assert_eq!(details[1]["path"], json!(["targetParticipantIds"]));
    assert_eq!(
        details[1]["message"],
        json!("Invalid input: expected array, received undefined")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_null_reads_received_null_not_undefined() {
    let (_base, addr) = boot().await;
    let client = reqwest::Client::new();

    // The tri-state's whole point: an explicit `null` is PRESENT, and Zod's
    // `parsedType` names it `null`. `targetParticipantIds` IS `.nullable()`, so
    // only `contentMarkdown` raises an issue here — which also proves the null
    // survived the REST edge's decode instead of collapsing to absent.
    let resp = client
        .post(format!("http://{addr}/api/v1/chats/{CHAT}?action=inform"))
        .json(&json!({ "contentMarkdown": Value::Null, "targetParticipantIds": Value::Null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 400);
    let body: Value = resp.json().await.unwrap();
    let details = body["details"]
        .as_array()
        .unwrap_or_else(|| panic!("v4 puts `details` on the wire: {body}"));
    assert_eq!(details.len(), 1, "only contentMarkdown is at fault: {body}");
    assert_eq!(
        details[0]["message"],
        json!("Invalid input: expected string, received null"),
        "an explicit null must NOT render as `received undefined`"
    );

    // The same rule on the cancel verb's single key.
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/chats/{CHAT}?action=cancel-inform"
        ))
        .json(&json!({ "batchId": Value::Null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["details"][0]["message"],
        json!("Invalid input: expected string, received null")
    );
}
