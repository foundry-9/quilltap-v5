//! P4.D143 §H, end-to-end over a live server: `GET /api/v1/chats` — the
//! collection route v5 had no REST edge for at all until this lane.
//!
//! The refusal BYTES are pinned against v4 by `salon_reads_equivalence`, which
//! drives v4's real GET dispatcher and records its 400s. What THIS test pins
//! is the plumbing that differential cannot see — and the P4.D65 lesson says to
//! pin it: in that round no lane actually SERVED the URL its two halves had
//! agreed on, and the wire defect only surfaced at unification.
//!
//!   1. The route is REGISTERED and the no-action leg lists (v4 serves the
//!      list here), in the `{chats: [...]}` envelope.
//!   2. P4.D220 (v4 `944127d9a` + `ad1c4c37f`): the GET is `dispatchAction(req,
//!      {}, list)` — an EMPTY map — so v4's `route.get.test.ts` vectors each
//!      answer the `Unknown action` envelope with `availableActions: []`:
//!      the RETIRED `?action=has-dangerous` (the Quick-hide probe and its
//!      `ChatsHasDangerous` verb are gone end to end), a bare `?action=`, and
//!      a key-only `?action`. Each is also pinned to have NOT listed.
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

    // --- v4's `route.get.test.ts` vectors: every `?action=` shape refuses
    //     with the empty map's envelope — the retired probe, an unknown name, a
    //     bare `?action=` (which LISTED until `ad1c4c37f`), a key-only
    //     `?action` (`searchParams.get` reads `''` for it too) ---
    for (query, action) in [
        ("?action=has-dangerous", "has-dangerous"),
        ("?action=no-such-action", "no-such-action"),
        ("?action=", ""),
        ("?action", ""),
        // FIRST wins: the bare first value refuses even with a second behind it.
        ("?action=&action=has-dangerous", ""),
    ] {
        let (status, body) = get(&client, &addr, &format!("/api/v1/chats{query}")).await;
        assert_eq!(status, 400, "{query} status: {body}");
        assert_eq!(
            body,
            serde_json::json!({
                "error": format!("Unknown action: {action}"),
                "availableActions": [],
            }),
            "{query}: v4's empty-map envelope, pinned against the oracle by salon_reads"
        );
        assert!(body.get("chats").is_none(), "{query} must not list: {body}");
    }

    // --- no action: v4 lists here, and so must v5 ---
    let (status, body) = get(&client, &addr, "/api/v1/chats").await;
    assert_eq!(status, 200, "no-action list status");
    assert!(
        body.get("chats").map(Value::is_array).unwrap_or(false),
        "the no-action leg answers v4's {{chats: [...]}} envelope, got {body}"
    );

    // v4's `limit` is `parseInt(limitParam, 10)` — a PREFIX parse. `1abc`
    // must slice to ONE chat (Rust's whole-string parse would have answered
    // the whole list), and an empty `limit=` is falsy in v4 → no limit.
    let all = body["chats"].as_array().map(Vec::len).unwrap_or(0);
    assert!(
        all >= 2,
        "the venue must seed at least two chats for the limit arms to discriminate, got {all}"
    );
    let (status, body) = get(&client, &addr, "/api/v1/chats?limit=1abc").await;
    assert_eq!(status, 200);
    assert_eq!(
        body["chats"].as_array().map(Vec::len),
        Some(1),
        "`limit=1abc` must prefix-parse to 1"
    );
    let (status, body) = get(&client, &addr, "/api/v1/chats?limit=").await;
    assert_eq!(status, 200);
    assert_eq!(
        body["chats"].as_array().map(Vec::len),
        Some(all),
        "an empty `limit=` is no limit"
    );

    // P4.67 — the duplicate-key class on a NON-action key. v4 reads
    // `searchParams.get('limit')`, the FIRST occurrence; v5's old
    // `Query<HashMap>` extractor kept the LAST, so `?limit=1&limit=5` answered
    // 5. The module's own motivating example had no arm anywhere until the §3
    // unification review of the follow-ups round; this is it, over the same
    // two-chat floor the prefix-parse arm above already demands.
    let (status, body) = get(&client, &addr, "/api/v1/chats?limit=1&limit=2").await;
    assert_eq!(status, 200);
    assert_eq!(
        body["chats"].as_array().map(Vec::len),
        Some(1),
        "`?limit=1&limit=2` must read the FIRST occurrence (v4 `searchParams.get`)"
    );
    let (status, body) = get(&client, &addr, "/api/v1/chats?limit=2&limit=1").await;
    assert_eq!(status, 200);
    assert_eq!(
        body["chats"].as_array().map(Vec::len),
        Some(2),
        "`?limit=2&limit=1` must read the FIRST occurrence (v4 `searchParams.get`)"
    );
}
