//! P4.6ak web-edge legs, end-to-end over a live server: the text-replacement
//! settings REST surface (GET/POST/PATCH/DELETE + `?action=bulk-replace`) and
//! the chat `?action=get-background` resolver. Proves the REST edges UNWRAP the
//! dispatch envelope to v4's raw route body (the P4.6ah lesson), the status
//! mapping (201 create / 204 delete / 409 conflict / 404 missing / 400
//! bad-action), and the live wiring over the committed fixture. The exact bytes
//! of every arm are pinned by `text_replacements_routes_equivalence`; here we
//! pin the web-edge PLUMBING over a seeded instance so the guards actually fire.

mod common;

use serde_json::{json, Value};

// Pinned fixture ids (shared with the fixture spec + the differential).
const R1: &str = "1a000000-0000-4000-8000-0000000000a1"; // teh (ci)
const CHAT_BG: &str = "c0000000-0000-4000-8000-0000000000c1";
const CHAT_NOBG: &str = "c0000000-0000-4000-8000-0000000000c2";
const BG_FILE: &str = "f0000000-0000-4000-8000-0000000000b1";
const MISSING: &str = "99999999-9999-4999-8999-999999999999";

#[tokio::test(flavor = "multi_thread")]
async fn text_replacements_web_edges() {
    let base = common::materialize_text_replacements_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = |p: &str| format!("http://{addr}{p}");

    // --- GET list: raw {rules, count}, ordered sortOrder then createdAt ---
    let resp = client
        .get(url("/api/v1/settings/text-replacements"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "list");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["count"], 3, "seed count");
    let rules = body["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 3);
    // R1 (so0, 01-01), R3 (so0, 01-03), R2 (so1) — the secondary-sort order.
    assert_eq!(rules[0]["fromText"], "teh");
    assert_eq!(rules[1]["fromText"], "dont");
    assert_eq!(rules[2]["fromText"], "adn");
    assert!(body.get("type").is_none(), "envelope must be unwrapped");

    // --- POST create: 201 {rule} ---
    let resp = client
        .post(url("/api/v1/settings/text-replacements"))
        .json(&json!({ "fromText": "recieve", "toText": "receive" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create status");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["rule"]["fromText"], "recieve");
    assert_eq!(body["rule"]["enabled"], true, "default enabled");
    assert_eq!(body["rule"]["sortOrder"], 0, "default sortOrder");

    // --- POST create conflict: 409 {error} ---
    let resp = client
        .post(url("/api/v1/settings/text-replacements"))
        .json(&json!({ "fromText": "TEH", "toText": "the" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409, "conflict status");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        "A case-insensitive rule for \"TEH\" already exists"
    );

    // --- PATCH: 200 {rule} ---
    let resp = client
        .patch(url(&format!("/api/v1/settings/text-replacements/{R1}")))
        .json(&json!({ "toText": "THE" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "patch status");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["rule"]["toText"], "THE");

    // --- PATCH missing: 404 with the doubled v4 message ---
    let resp = client
        .patch(url(&format!(
            "/api/v1/settings/text-replacements/{MISSING}"
        )))
        .json(&json!({ "toText": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "patch missing");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        format!("Text replacement rule {MISSING} not found not found")
    );

    // --- DELETE: 204 empty ---
    let resp = client
        .delete(url(&format!("/api/v1/settings/text-replacements/{R1}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204, "delete status");
    assert!(resp.bytes().await.unwrap().is_empty(), "204 empty body");

    // --- DELETE missing: 404 ---
    let resp = client
        .delete(url(&format!(
            "/api/v1/settings/text-replacements/{MISSING}"
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "delete missing");

    // --- POST bulk-replace: 200 {rules, count} ---
    let resp = client
        .post(url(
            "/api/v1/settings/text-replacements?action=bulk-replace",
        ))
        .json(&json!({ "rules": [
            { "fromText": "alpha", "toText": "ALPHA", "sortOrder": 2 },
            { "fromText": "bravo", "toText": "BRAVO", "sortOrder": 0 },
        ] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "bulk status");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["count"], 2, "bulk count");
    // Echo is in INPUT order.
    assert_eq!(body["rules"][0]["fromText"], "alpha");

    // --- chat get-background: resolved arm ---
    let resp = client
        .get(url(&format!(
            "/api/v1/chats/{CHAT_BG}?action=get-background"
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "bg resolved");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["fileId"], BG_FILE);
    assert_eq!(body["backgroundUrl"], format!("/api/v1/files/{BG_FILE}"));
    assert_eq!(body["linkSummary"]["count"], 0);

    // --- chat get-background: no-id arm (all null) ---
    let resp = client
        .get(url(&format!(
            "/api/v1/chats/{CHAT_NOBG}?action=get-background"
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "bg no-id");
    let body: Value = resp.json().await.unwrap();
    assert!(body["backgroundUrl"].is_null());
    assert!(body["linkSummary"].is_null());

    // --- chat get-background: unknown action → 400 loud pointer ---
    let resp = client
        .get(url(&format!("/api/v1/chats/{CHAT_BG}?action=cost")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "unknown action pointer");
}
