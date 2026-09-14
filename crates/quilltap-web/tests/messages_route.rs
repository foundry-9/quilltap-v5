//! **P4.D183, end-to-end over a live server: `GET /api/v1/messages`** — the
//! collection route v5 had no REST edge for at all until this lane.
//!
//! The BODIES are pinned against v4 by `transcript_route_equivalence`, which
//! drives v4's REAL route handler and records its exact bytes for the three
//! arms that live above the dispatch boundary. What THIS test pins is the
//! plumbing that differential cannot see — and the P4.D65 lesson says to pin
//! it: in that round no lane actually SERVED the URL its two halves had agreed
//! on, and the wire defect only surfaced at unification.
//!
//!   1. The route is REGISTERED and reaches both verbs.
//!   2. `CoreResponse::ChatTranscript` and `CoreResponse::ChatMessageEvents`
//!      are actually unwrapped — a variant missing from an edge's success arm
//!      answers 500 on every success (the P4.56 `BrahmaConsole` defect).
//!   3. The conditional really short-circuits: an agreeing `knownVersion`
//!      answers `{unchanged:true,version}` and NOTHING else, over the wire.
//!   4. `knownVersion` is converted with JS `Number()` semantics at the edge,
//!      so `"12abc"` is NaN (absent) rather than a prefix parse to 12, and a
//!      bare `?knownVersion=` is a genuine 0.
//!   5. The `chatId` 400 precedes the 404.
//!   6. An unknown `?action=` answers v4's envelope (NOT the listing — the
//!      measured shape of `withActionDispatch`), while a present-but-EMPTY
//!      action lists exactly like an absent one.
//!
//! Run:
//!   cargo test -p quilltap-web --test messages_route

mod common;

use serde_json::{json, Value};

const MISSING: &str = "99999999-9999-4999-8999-999999999999";

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
async fn messages_get_edges() {
    let base = common::materialize_fixture_instance();
    // The committed fixture predates `31436bae4`; on a real instance the boot
    // ensure has already run. Heal it here so the counter is a real column
    // rather than the swallowed-read 0 — otherwise arm 3 would be vacuous.
    {
        let path = base.path().join("data/quilltap.db");
        let w = quilltap_core::db::Writer::open_writable(&path, common::TEST_PEPPER).unwrap();
        quilltap_core::test_support::ensure_p4d182_columns(w.connection());
    }
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // Discover a chat the way `chat_delete_route` does — from the live list
    // rather than by transcribing an id, so a regenerated fixture cannot turn
    // this into a 404 that looks like a routing failure.
    let (status, listing) = get(&client, &addr, "/api/v1/chats").await;
    assert_eq!(status, 200, "the chats list must answer: {listing}");
    let chat = listing["chats"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| c["id"].as_str())
        .expect("the venue must seed at least one chat")
        .to_string();
    let chat = chat.as_str();
    let set_version = |v: i64| {
        let path = base.path().join("data/quilltap.db");
        let w = quilltap_core::db::Writer::open_writable(&path, common::TEST_PEPPER).unwrap();
        w.connection()
            .execute(
                "UPDATE \"chats\" SET \"transcriptVersion\" = ?1 WHERE \"id\" = ?2",
                rusqlite::params![v, chat],
            )
            .unwrap();
    };
    set_version(5);

    // --- 1/2/3: the conditional short-circuit, over the wire.
    let (status, body) = get(
        &client,
        &addr,
        &format!("/api/v1/messages?chatId={chat}&action=transcript&knownVersion=5"),
    )
    .await;
    assert_eq!(status, 200, "an agreeing knownVersion is a 200");
    assert_eq!(
        body,
        json!({"unchanged": true, "version": 5}),
        "the agreeing read must answer these two keys and NOTHING else — no \
         messages, no count. Serializing the transcript here is the cost the \
         whole feature exists to avoid."
    );

    // …and a DISagreeing one delivers the transcript.
    let (status, body) = get(
        &client,
        &addr,
        &format!("/api/v1/messages?chatId={chat}&action=transcript&knownVersion=4"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["unchanged"], json!(false));
    assert_eq!(body["version"], json!(5));
    assert!(
        body["messages"].is_array(),
        "the full body carries messages"
    );
    assert!(body["offSceneCharacters"].is_array());
    assert_eq!(
        body["count"],
        json!(body["messages"].as_array().unwrap().len()),
        "`count` is the message count"
    );
    // The key ORDER v4 emits (the differential pins the bytes; this pins that
    // the edge does not reshape them on the way out).
    assert_eq!(
        body.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec![
            "unchanged",
            "version",
            "messages",
            "offSceneCharacters",
            "count"
        ]
    );

    // --- 4: JS `Number()` at the edge, not a Rust parse.
    //     `"5abc"` is NaN → treated as absent → a FULL read, even though a
    //     prefix parse would have produced 5 and answered `unchanged`.
    let (status, body) = get(
        &client,
        &addr,
        &format!("/api/v1/messages?chatId={chat}&action=transcript&knownVersion=5abc"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["unchanged"],
        json!(false),
        "`5abc` is NaN under Number(), so it is not a known version — a prefix \
         parse would wrongly answer `unchanged` here"
    );
    // A float is a number but not an integer → also a full read.
    let (_, body) = get(
        &client,
        &addr,
        &format!("/api/v1/messages?chatId={chat}&action=transcript&knownVersion=5.5"),
    )
    .await;
    assert_eq!(body["unchanged"], json!(false));
    // `Number('')` is 0 — a real integer. Against a chat whose counter IS 0
    // that answers `unchanged`, which is the arm a "treat empty as absent"
    // shortcut would get wrong.
    set_version(0);
    let (_, body) = get(
        &client,
        &addr,
        &format!("/api/v1/messages?chatId={chat}&action=transcript&knownVersion="),
    )
    .await;
    assert_eq!(
        body,
        json!({"unchanged": true, "version": 0}),
        "`Number('')` is 0, so a bare knownVersion matches a counter of 0"
    );

    // --- 5: the `chatId` 400, and that it precedes the 404.
    for path in [
        "/api/v1/messages?action=transcript",
        "/api/v1/messages",
        "/api/v1/messages?chatId=&action=transcript",
    ] {
        let (status, body) = get(&client, &addr, path).await;
        assert_eq!(status, 400, "missing chatId is a 400 on both legs: {path}");
        assert_eq!(body, json!({"error": "Query parameter required: chatId"}));
    }
    // An unknown chat — WITH a chatId — is the 404, so the two gates are
    // ordered and not one guard doing double duty.
    let (status, body) = get(
        &client,
        &addr,
        &format!("/api/v1/messages?chatId={MISSING}&action=transcript"),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body, json!({"error": "Chat not found"}));

    // --- 6: the dispatcher's two shapes.
    let (status, body) = get(
        &client,
        &addr,
        &format!("/api/v1/messages?chatId={chat}&action=no-such-action"),
    )
    .await;
    assert_eq!(
        status, 400,
        "an unknown action is REFUSED, not quietly served by the default \
         handler — `withActionDispatch` tests `if (action)` first"
    );
    assert_eq!(
        body,
        json!({"error": "Unknown action: no-such-action", "availableActions": ["transcript"]})
    );

    // …while `?action=` is JS-falsy and takes the default leg, byte-identical
    // to an absent one.
    let (status, listed) = get(
        &client,
        &addr,
        &format!("/api/v1/messages?chatId={chat}&action="),
    )
    .await;
    assert_eq!(status, 200);
    let (status, absent) = get(&client, &addr, &format!("/api/v1/messages?chatId={chat}")).await;
    assert_eq!(status, 200);
    assert_eq!(listed, absent, "`?action=` lists exactly like no action");
    assert_eq!(
        absent.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["messages", "count"],
        "the listing envelope is v4's two keys"
    );
    assert_eq!(
        absent["count"],
        json!(absent["messages"].as_array().unwrap().len())
    );
}
