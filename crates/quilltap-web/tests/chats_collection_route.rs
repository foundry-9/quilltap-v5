//! P4.D143 §H, end-to-end over a live server: `GET /api/v1/chats` — the
//! collection route v5 had no REST edge for at all until this lane.
//!
//! The BODIES are pinned against v4 by `salon_reads_equivalence`, which drives
//! the handler (and, for the unknown-action refusal, records v4's exact 400
//! bytes). What THIS test pins is the plumbing that differential cannot see —
//! and the P4.D65 lesson says to pin it: in that round no lane actually SERVED
//! the URL its two halves had agreed on, and the wire defect only surfaced at
//! unification.
//!
//!   1. The route is REGISTERED and reaches the `ChatsHasDangerous` verb.
//!   2. `CoreResponse::ChatsHasDangerous` is actually unwrapped (a variant
//!      missing from an edge's success arm answers 500 on every success — the
//!      P4.56 `BrahmaConsole` defect).
//!   3. The unknown-action arm answers v4's 400 sentence, built from
//!      `CHAT_GET_ACTIONS` rather than transcribed.
//!   4. The no-action leg still lists (v4 serves the list here; refusing it
//!      would be an invention), in the `{chats: [...]}` envelope.
//!
//! Run:
//!   cargo test -p quilltap-web --test chats_collection_route

mod common;

use serde_json::Value;

async fn get(client: &reqwest::Client, addr: &std::net::SocketAddr, path: &str) -> (u16, Value) {
    let resp = client
        .get(format!("http://{addr}{path}"))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn chats_collection_get_edges() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // --- ?action=has-dangerous — both answers, driven from the same instance.
    //     Start by taking every chat OFF the uncensored row (the committed
    //     `chat-send` fixture seeds a chat that is already on it), so the
    //     `false` answer is measured rather than assumed.
    let flip = |sql: &'static str| {
        let path = base.path().join("data/quilltap.db");
        let w = quilltap_core::db::Writer::open_writable(&path, common::TEST_PEPPER).unwrap();
        w.connection().execute_batch(sql).unwrap();
    };
    flip("UPDATE \"chats\" SET \"conciergeOverride\" = NULL, \"isDangerousChat\" = 0");
    let (status, body) = get(&client, &addr, "/api/v1/chats?action=has-dangerous").await;
    assert_eq!(status, 200, "has-dangerous status");
    assert_eq!(
        body,
        serde_json::json!({ "hasDangerous": false }),
        "the probe's raw body — no successResponse envelope, exactly as v4 sends it"
    );

    // --- one chat onto the uncensored row by the operator's own hand: the
    //     label underneath stays false, so nothing but the new predicate can
    //     make this true (the pre-`c43d3b1b4` `isDangerousChat === true` probe
    //     would still answer false here) ---
    flip(
        "UPDATE \"chats\" SET \"conciergeOverride\" = 'UNCENSORED', \"isDangerousChat\" = 0 \
         WHERE rowid = (SELECT MIN(rowid) FROM \"chats\")",
    );
    let (status, body) = get(&client, &addr, "/api/v1/chats?action=has-dangerous").await;
    assert_eq!(status, 200, "has-dangerous status after the flip");
    assert_eq!(
        body,
        serde_json::json!({ "hasDangerous": true }),
        "an Uncensored chat is on the row the toggle hides"
    );

    // --- an unknown action: v4's exact sentence ---
    let (status, body) = get(&client, &addr, "/api/v1/chats?action=no-such-action").await;
    assert_eq!(status, 400, "unknown-action status");
    assert_eq!(
        body["error"].as_str(),
        Some("Unknown action: no-such-action. Available actions: has-dangerous"),
        "v4's unknown-action sentence, pinned against the oracle by salon_reads"
    );

    // --- no action: v4 lists here, and so must v5 ---
    let (status, body) = get(&client, &addr, "/api/v1/chats").await;
    assert_eq!(status, 200, "no-action list status");
    assert!(
        body.get("chats").map(Value::is_array).unwrap_or(false),
        "the no-action leg answers v4's {{chats: [...]}} envelope, got {body}"
    );
}
