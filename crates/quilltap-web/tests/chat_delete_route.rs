//! P4.80, end-to-end over a live server: `DELETE /api/v1/chats/{id}` — the edge
//! v5 answered **405** on until this lane (dogfood finding #117: a salon chat
//! could not be deleted from anywhere).
//!
//! The BODIES and the whole dispatch are pinned against v4 by
//! `chat_delete_equivalence`, which drives `chat_delete_dispatch` itself. What
//! THIS test pins is the plumbing that differential cannot see — and the P4.D65
//! lesson says to pin it: in that round no lane actually SERVED the URL its two
//! halves had agreed on, and the wire defect only surfaced at unification.
//!
//!   1. The route is REGISTERED on DELETE (it was a 405 before).
//!   2. Each response variant the dispatch can answer is actually unwrapped —
//!      `ChatAdmin` (delete), `State` (reset-state), `ChatImpersonation`
//!      (stop-impersonate). A variant missing from an edge's success arm
//!      answers 500 on every success (the P4.56 `BrahmaConsole` defect).
//!   3. The BODY reaches `stop-impersonate` (Tier 2, item 6): the edge reads
//!      the request bytes and hands them to the ported Zod parse, so a body
//!      naming a participant nobody has answers 404 `Participant not found`
//!      rather than a validation error.
//!   4. v4's `validationError` envelope — `{error, details}` — survives the
//!      edge. `error_to_http` only knows `{error}`, so the Zod body is rendered
//!      by the route itself; without that arm the `details` array would vanish.
//!   5. The guard ORDER over the wire: a missing chat with a malformed body is
//!      a 404, not a 400.
//!   6. `?action=` (present, empty) is JS-falsy and DELETES; `?action=zzz`
//!      refuses and leaves the chat standing.
//!   7. A body that is not JSON at all is the SyntaxError v4's middleware turns
//!      into 500 `Internal server error` — not a ZodError, so not a 400.
//!   8. An EMPTY body is that same SyntaxError (`req.json()` on zero bytes).
//!
//! Run:
//!   cargo test -p quilltap-web --test chat_delete_route

mod common;

use serde_json::Value;

async fn send(
    client: &reqwest::Client,
    addr: &std::net::SocketAddr,
    method: reqwest::Method,
    path: &str,
    body: Option<&str>,
) -> (u16, Value) {
    let mut req = client.request(method, format!("http://{addr}{path}"));
    if let Some(b) = body {
        req = req
            .header("content-type", "application/json")
            .body(b.to_string());
    }
    let resp = req.send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (status, value)
}

async fn chat_ids(client: &reqwest::Client, addr: &std::net::SocketAddr) -> Vec<String> {
    let (status, body) = send(client, addr, reqwest::Method::GET, "/api/v1/chats", None).await;
    assert_eq!(status, 200, "the list edge must answer: {body}");
    body["chats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

const MISSING: &str = "99999999-9999-4999-8999-999999999999";

#[tokio::test(flavor = "multi_thread")]
async fn chat_delete_edge() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let del = |path: String, body: Option<String>| {
        let client = client.clone();
        async move {
            send(
                &client,
                &addr,
                reqwest::Method::DELETE,
                &path,
                body.as_deref(),
            )
            .await
        }
    };

    let ids = chat_ids(&client, &addr).await;
    assert!(
        ids.len() >= 3,
        "the venue must seed at least three chats (one per destructive arm), got {}",
        ids.len()
    );

    // --- 2/6: an unknown action REFUSES, and the chat survives it ------------
    let victim = ids[0].clone();
    let (status, body) = del(format!("/api/v1/chats/{victim}?action=zzz"), None).await;
    assert_eq!(status, 400, "unknown-action status: {body}");
    assert_eq!(
        body["error"].as_str(),
        Some("Unknown DELETE action: zzz. Available DELETE actions: reset-state, stop-impersonate"),
        "v4's sentence, pinned against the oracle by `chat_delete_equivalence`"
    );
    assert!(
        chat_ids(&client, &addr).await.contains(&victim),
        "v4 refuses the unknown action precisely to PREVENT the delete"
    );

    // --- 1/2: the delete itself, then the row is gone ------------------------
    let (status, body) = del(format!("/api/v1/chats/{victim}"), None).await;
    assert_eq!(status, 200, "delete status: {body}");
    assert_eq!(body, serde_json::json!({ "success": true }));
    assert!(
        !chat_ids(&client, &addr).await.contains(&victim),
        "the chat must be gone from the list"
    );
    // The row itself is gone, asked through the verb rather than
    // `GET /api/v1/chats/{id}` — v5's per-id GET serves only `get-background`
    // and `cost` and sends everything else to `/api/dispatch` (a divergence
    // that predates this lane, `wardrobe_routes::chat_action_get`).
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::POST,
        "/api/dispatch",
        Some(&format!(r#"{{"type":"chatGet","chatId":"{victim}"}}"#)),
    )
    .await;
    assert_eq!(status, 404, "the deleted chat's verb must 404: {body}");

    // The 404 arm of the delete itself.
    let (status, body) = del(format!("/api/v1/chats/{MISSING}"), None).await;
    assert_eq!(status, 404);
    assert_eq!(body["error"].as_str(), Some("Chat not found"));

    // --- 6: `?action=` is present but EMPTY — JS-falsy, so it DELETES --------
    let empty_victim = ids[1].clone();
    let (status, body) = del(format!("/api/v1/chats/{empty_victim}?action="), None).await;
    assert_eq!(status, 200, "an empty action takes the delete leg: {body}");
    assert!(!chat_ids(&client, &addr).await.contains(&empty_victim));

    // --- 2: reset-state unwraps `CoreResponse::State` ------------------------
    let survivor = ids[2].clone();
    let (status, body) = del(format!("/api/v1/chats/{survivor}?action=reset-state"), None).await;
    assert_eq!(status, 200, "reset-state status: {body}");
    assert_eq!(body["success"].as_bool(), Some(true));
    assert!(
        body.get("previousState").is_some(),
        "v4's reset body carries `previousState`: {body}"
    );

    // --- 3: the BODY reaches the handler ------------------------------------
    // A well-formed participant id that nobody has: only a body that arrived
    // can produce `Participant not found`.
    let (status, body) = del(
        format!("/api/v1/chats/{survivor}?action=stop-impersonate"),
        Some(format!(r#"{{"participantId":"{MISSING}"}}"#)),
    )
    .await;
    assert_eq!(status, 404, "stop-impersonate body passthrough: {body}");
    assert_eq!(body["error"].as_str(), Some("Participant not found"));

    // --- 4: v4's `validationError` envelope survives the edge ---------------
    let (status, body) = del(
        format!("/api/v1/chats/{survivor}?action=stop-impersonate"),
        Some(r#"{"participantId":"not-a-uuid"}"#.to_string()),
    )
    .await;
    assert_eq!(status, 400, "the Zod refusal: {body}");
    assert_eq!(body["error"].as_str(), Some("Validation error"));
    let details = body["details"]
        .as_array()
        .expect("the `details` array must survive the edge");
    assert_eq!(details.len(), 1, "one issue: {body}");
    assert_eq!(details[0]["code"].as_str(), Some("invalid_format"));
    assert_eq!(details[0]["format"].as_str(), Some("uuid"));
    assert_eq!(details[0]["path"][0].as_str(), Some("participantId"));

    // --- 5: the guard ORDER — a missing chat beats a malformed body ---------
    let (status, body) = del(
        format!("/api/v1/chats/{MISSING}?action=stop-impersonate"),
        Some("{}".to_string()),
    )
    .await;
    assert_eq!(status, 404, "the chat gate runs BEFORE the parse: {body}");
    assert_eq!(body["error"].as_str(), Some("Chat not found"));

    // --- 7: a body that is not JSON at all ----------------------------------
    let (status, body) = del(
        format!("/api/v1/chats/{survivor}?action=stop-impersonate"),
        Some("not json at all".to_string()),
    )
    .await;
    assert_eq!(
        status, 500,
        "v4's `req.json()` SyntaxError is not a Zod error: {body}"
    );
    assert_eq!(body["error"].as_str(), Some("Internal server error"));

    // --- 8: an EMPTY body is the same SyntaxError -----------------------------
    // v4 `participants.ts:89` `await req.json()` on zero bytes throws exactly
    // as it does on prose, and the middleware (`context.ts:207`) answers 500.
    // The §3 unification review retired an edge-side `is_empty → {}` special
    // case that answered 400 here.
    let (status, body) = del(
        format!("/api/v1/chats/{survivor}?action=stop-impersonate"),
        Some(String::new()),
    )
    .await;
    assert_eq!(
        status, 500,
        "an empty body is a SyntaxError, not a Zod error: {body}"
    );
    assert_eq!(body["error"].as_str(), Some("Internal server error"));

    // The chat every action arm above ran against is still standing — none of
    // them may delete anything.
    assert!(
        chat_ids(&client, &addr).await.contains(&survivor),
        "no `?action=` arm may delete the chat it was aimed at"
    );
}
