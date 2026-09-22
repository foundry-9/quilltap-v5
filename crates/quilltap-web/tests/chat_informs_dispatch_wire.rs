//! P4.106 item 5 — the three Inform verbs AT THE DISPATCH WIRE (`POST
//! /api/dispatch`), the P4.D205 gap "no dispatch WIRE test for the three
//! verbs … the dispatch transport's serde stays argued from precedent".
//!
//! The dispatch transport decodes the whole body with
//! `serde_json::from_slice::<Request>`, where `Request::ChatInform`'s
//! `content_markdown` / `target_participant_ids` and `ChatInformCancel`'s
//! `batch_id` rely on `double_option` to keep an explicit `null` apart from an
//! absent key (`a-dispatch-wire-test-sees-what-the-differential-cannot`). A
//! plain `#[serde(default)]` over `Option<Option<Value>>` collapses `null` onto
//! absent — the handler then renders v4's Zod issue as `received undefined`
//! where v4 says `received null`.
//!
//! Measured (P4.106): stripping `double_option` from those two keys reddens
//! `the_tri_state_survives_the_dispatch_decode` here AND
//! `chat_informs_rest_routes`'s explicit-null arm — the REST route builds its
//! `Request` through the same serde, so the P4.D205 header's "the dispatch
//! transport's serde stays argued from precedent" undersold the REST test.
//! What this file adds is the DISPATCH transport itself: the `{type, data}`
//! success envelopes of all three verbs, v4's `details` riding the dispatch
//! error envelope, and the serde-typed `chatId` refusal.
//!
//! The Tauri IPC leg is proven by construction: `quilltap-tauri`'s `dispatch`
//! command calls `quilltap_web::dispatch::dispatch_body`, the same decode.
//!
//! Arms: a posted inform's four keys (Everyone → the record's `null` targets
//! echoed; a single seat → that seat); the list (both batches, the pending
//! seats resolved); the cancel (`removed`, `recordDeleted`, the list after);
//! the absent-key Zod envelope for BOTH verbs that take a body; an explicit
//! `null`'s `received null` on all three tri-state keys (`targetParticipantIds`
//! is `.nullable()`, so its `null` is ACCEPTED — which is itself the proof the
//! null survived as a value); wrong-typed body keys (`received number` /
//! `received string` — the handler's gate, not the decode); a malformed
//! `batchId` (`Invalid UUID`); and a wrong-typed `chatId`, which IS serde-typed
//! and so answers the dispatch decode sentence (the
//! `dispatch_wrong_type_census` class — this file adds no verb, so the census
//! is unmoved).
//!
//! Fixture: the committed SALON pair (per-run copy), whose `Group Expedition`
//! chat seats two LLM CHARACTERs + a user seat — the `chat_informs_rest_routes`
//! boot, verbatim.
//!
//! Run:
//!   cargo test -p quilltap-web --test chat_informs_dispatch_wire

mod common;

use serde_json::{json, Value};

/// `Group Expedition` — two LLM CHARACTER seats (`b2…001`, `b2…002`) and one
/// user seat (`b2…003`), per `harness/oracle/fixtures/salon.json`.
const CHAT: &str = "c1000000-0000-4000-8000-000000000002";
const SEAT_ARIA: &str = "b2000000-0000-4000-8000-000000000001";
const SEAT_TWO: &str = "b2000000-0000-4000-8000-000000000002";

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
        quilltap_core::test_support::ensure_p4d171_columns(w.connection());
    }
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

async fn boot() -> (tempfile::TempDir, Wire) {
    let base = materialize_salon_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let wire = Wire {
        client: reqwest::Client::new(),
        url: format!("http://{addr}/api/dispatch"),
    };
    (base, wire)
}

/// The Zod issue messages on a dispatch validation refusal, asserting the
/// envelope's shape on the way (`data.message` = v4's flat sentence, the
/// issues under `data.details`).
fn issue_messages(status: u16, v: &Value) -> Vec<(Value, String)> {
    assert_eq!(status, 400, "a validation refusal is a 400: {v}");
    assert_eq!(v["type"], json!("error"), "{v}");
    assert_eq!(v["data"]["kind"], json!("bad-request"), "{v}");
    assert_eq!(v["data"]["message"], json!("Validation error"), "{v}");
    v["data"]["details"]
        .as_array()
        .unwrap_or_else(|| panic!("v4's `details` must ride the dispatch envelope: {v}"))
        .iter()
        .map(|d| {
            (
                d["path"].clone(),
                d["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn post_list_and_cancel_round_trip_over_dispatch() {
    let (_base, wire) = boot().await;

    // ---- chatInform, Everyone: the record is PUBLIC, so the echoed targets are
    //      the RECORD's (`null`), not the two resolved seats.
    let (status, v) = wire
        .post(json!({
            "type": "chatInform",
            "chatId": CHAT,
            "contentMarkdown": "The clock in the hall has stopped.",
            "targetParticipantIds": Value::Null,
        }))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["type"], json!("chatInform"));
    let data = &v["data"];
    let keys: Vec<&String> = data.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        ["success", "batchId", "targetParticipantIds", "message"],
        "v4's four keys, in its order: {data}"
    );
    assert_eq!(data["success"], json!(true));
    assert_eq!(data["targetParticipantIds"], Value::Null);
    assert_eq!(data["message"]["systemSender"], json!("host"));
    assert_eq!(data["message"]["systemKind"], json!("inform"));
    assert_eq!(
        data["message"]["content"],
        json!("The clock in the hall has stopped.")
    );
    let everyone_batch = data["batchId"].as_str().expect("batchId").to_string();

    // ---- chatInform, one seat: a whisper whose record names the seat.
    let (status, v) = wire
        .post(json!({
            "type": "chatInform",
            "chatId": CHAT,
            "contentMarkdown": "You alone notice the draught.",
            "targetParticipantIds": [SEAT_ARIA],
        }))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["data"]["targetParticipantIds"], json!([SEAT_ARIA]));
    let whisper_batch = v["data"]["batchId"].as_str().expect("batchId").to_string();

    // ---- chatInformsList: both batches, oldest first, each with the seats it
    //      is still owed (Everyone resolved to the two LLM seats; the user seat
    //      is never owed an inform).
    let (status, v) = wire
        .post(json!({ "type": "chatInformsList", "chatId": CHAT }))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["type"], json!("chatInforms"));
    let batches = v["data"]["batches"].as_array().expect("batches");
    assert_eq!(batches.len(), 2, "{v}");
    assert_eq!(batches[0]["batchId"], json!(everyone_batch));
    assert_eq!(
        batches[0]["pendingParticipantIds"],
        json!([SEAT_ARIA, SEAT_TWO])
    );
    assert_eq!(batches[1]["batchId"], json!(whisper_batch));
    assert_eq!(batches[1]["pendingParticipantIds"], json!([SEAT_ARIA]));
    for b in batches {
        let keys: Vec<&String> = b.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            [
                "batchId",
                "contentMarkdown",
                "createdAt",
                "recordMessageId",
                "pendingParticipantIds"
            ],
            "{b}"
        );
    }

    // ---- chatInformCancel: withdraw the whisper; nothing consumed, so the
    //      record goes with it.
    let (status, v) = wire
        .post(json!({
            "type": "chatInformCancel",
            "chatId": CHAT,
            "batchId": whisper_batch,
        }))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["type"], json!("chatInformCancelled"));
    assert_eq!(
        v["data"],
        json!({ "success": true, "removed": 1, "recordDeleted": true })
    );

    let (_, v) = wire
        .post(json!({ "type": "chatInformsList", "chatId": CHAT }))
        .await;
    let batches = v["data"]["batches"].as_array().expect("batches");
    assert_eq!(batches.len(), 1, "only the Everyone batch is left: {v}");
    assert_eq!(batches[0]["batchId"], json!(everyone_batch));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tri_state_survives_the_dispatch_decode() {
    let (_base, wire) = boot().await;

    // ---- Absent keys: both required keys named, `received undefined`.
    let (status, v) = wire
        .post(json!({ "type": "chatInform", "chatId": CHAT }))
        .await;
    assert_eq!(
        issue_messages(status, &v),
        vec![
            (
                json!(["contentMarkdown"]),
                "Invalid input: expected string, received undefined".to_string()
            ),
            (
                json!(["targetParticipantIds"]),
                "Invalid input: expected array, received undefined".to_string()
            ),
        ]
    );

    // ---- Explicit nulls: `contentMarkdown: null` must read `received null`;
    //      `targetParticipantIds` is `.nullable()`, so its null raises NOTHING —
    //      under a collapse it would read `received undefined` instead.
    let (status, v) = wire
        .post(json!({
            "type": "chatInform",
            "chatId": CHAT,
            "contentMarkdown": Value::Null,
            "targetParticipantIds": Value::Null,
        }))
        .await;
    assert_eq!(
        issue_messages(status, &v),
        vec![(
            json!(["contentMarkdown"]),
            "Invalid input: expected string, received null".to_string()
        )],
        "an explicit null must survive the dispatch decode as a PRESENT null"
    );

    // ---- Wrong-typed body keys reach the handler's gate (not the decode).
    let (status, v) = wire
        .post(json!({
            "type": "chatInform",
            "chatId": CHAT,
            "contentMarkdown": 7,
            "targetParticipantIds": "everyone",
        }))
        .await;
    assert_eq!(
        issue_messages(status, &v),
        vec![
            (
                json!(["contentMarkdown"]),
                "Invalid input: expected string, received number".to_string()
            ),
            (
                json!(["targetParticipantIds"]),
                "Invalid input: expected array, received string".to_string()
            ),
        ]
    );

    // ---- The cancel verb's single key: absent, null, malformed.
    let (status, v) = wire
        .post(json!({ "type": "chatInformCancel", "chatId": CHAT }))
        .await;
    assert_eq!(
        issue_messages(status, &v),
        vec![(
            json!(["batchId"]),
            "Invalid input: expected string, received undefined".to_string()
        )]
    );
    let (status, v) = wire
        .post(json!({ "type": "chatInformCancel", "chatId": CHAT, "batchId": Value::Null }))
        .await;
    assert_eq!(
        issue_messages(status, &v),
        vec![(
            json!(["batchId"]),
            "Invalid input: expected string, received null".to_string()
        )],
        "the cancel verb's explicit null must survive the dispatch decode too"
    );
    let (status, v) = wire
        .post(json!({ "type": "chatInformCancel", "chatId": CHAT, "batchId": "not-a-uuid" }))
        .await;
    assert_eq!(
        issue_messages(status, &v),
        vec![(json!(["batchId"]), "Invalid UUID".to_string())]
    );

    // Nothing above wrote a batch.
    let (_, v) = wire
        .post(json!({ "type": "chatInformsList", "chatId": CHAT }))
        .await;
    assert_eq!(v["data"]["batches"], json!([]), "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_typed_chat_id_is_the_dispatch_decode_refusal() {
    let (_base, wire) = boot().await;

    // `chatId` is serde-typed (`String`), so a number never reaches a handler:
    // the dispatch decode refuses with its own sentence, on all three verbs.
    for body in [
        json!({ "type": "chatInform", "chatId": 42, "contentMarkdown": "x", "targetParticipantIds": Value::Null }),
        json!({ "type": "chatInformsList", "chatId": 42 }),
        json!({ "type": "chatInformCancel", "chatId": 42, "batchId": "aaaa0000-0000-4000-8000-000000000001" }),
    ] {
        let (status, v) = wire.post(body.clone()).await;
        assert_eq!(status, 400, "{body} → {v}");
        assert_eq!(v["data"]["kind"], json!("bad-request"), "{v}");
        assert_eq!(
            v["data"]["message"],
            json!("Invalid request: invalid type: integer `42`, expected a string"),
            "{body} → {v}"
        );
        assert!(
            v["data"].get("details").is_none(),
            "a decode refusal carries no Zod issues: {v}"
        );
    }
}
