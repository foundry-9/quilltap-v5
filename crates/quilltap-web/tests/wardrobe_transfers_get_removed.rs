//! P4.D378 (v4 `a0ed5ecad`): `GET /api/v1/wardrobe/transfers` is REMOVED.
//!
//! v4's route module exports `POST` only at the pin (the destination roster
//! had no callers; the dialog's destinations come from the SPA's container
//! catalogue), and Next's App Router answers 405 for a method a route does not
//! export. v5 un-routes the GET leg, so the path keeps its POST and axum
//! answers 405 to a GET.
//!
//! What this pins, over a live server:
//!   1. a GET answers **405** — the STATUS only. v4's 405 BODY and its
//!      `Allow` header are NOT measured (there is no Next.js server oracle),
//!      so this test makes no byte-parity claim about them;
//!   2. the POST still reaches the handler: a body that is not JSON is v4's
//!      `await req.json()` throw inside the route's own catch (500 `Failed to
//!      transfer wardrobe item`), and a JSON body the schema refuses is the
//!      route's ZodError arm (400) — never a 405.
//!
//! Run:
//!   cargo test -p quilltap-web --test wardrobe_transfers_get_removed

mod common;

use serde_json::Value;

async fn send(
    client: &reqwest::Client,
    addr: &std::net::SocketAddr,
    method: reqwest::Method,
    body: Option<&str>,
) -> (u16, Value) {
    let mut req = client.request(method, format!("http://{addr}/api/v1/wardrobe/transfers"));
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

#[tokio::test(flavor = "multi_thread")]
async fn the_transfers_get_is_gone_and_the_post_stays() {
    let base = common::materialize_bare_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // 1. The GET leg is un-routed: axum's 405 (status only — see the header).
    let (status, body) = send(&client, &addr, reqwest::Method::GET, None).await;
    assert_eq!(status, 405, "GET /api/v1/wardrobe/transfers: {body}");

    // 2. The POST is still routed to the handler.
    let (status, body) = send(&client, &addr, reqwest::Method::POST, Some("not json")).await;
    assert_eq!(status, 500, "a malformed POST body: {body}");
    assert_eq!(
        body,
        serde_json::json!({ "error": "Failed to transfer wardrobe item" }),
        "v4's catch for a `req.json()` throw"
    );
    let (status, body) = send(&client, &addr, reqwest::Method::POST, Some("{}")).await;
    assert_eq!(status, 400, "a POST body the schema refuses: {body}");
    assert!(
        body["error"].is_string(),
        "the ZodError arm's joined issues: {body}"
    );
}
