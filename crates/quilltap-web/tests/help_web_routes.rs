//! P4.9I2A web-edge leg, end-to-end over a live server: the help-docs read
//! surface and the help-chats CRUD/send surface at v4's own URLs. The exact
//! bodies of every arm are pinned against v4 by `help_docs_routes_equivalence`
//! and `help_chats_routes_equivalence`, which drive the handlers; what THIS
//! test pins is the plumbing those differentials cannot see:
//!
//!   1. **The two `?action=` shapes.** The help-DOCS route is a default-serving
//!      shape (`?action=bogus` and `?action=` both LIST); the help-CHATS routes
//!      are the envelope shape (`?action=` takes the no-action leg, `?action=bogus`
//!      is v4's fixed `Unknown action: …` 400). Both are edge decisions.
//!   2. **The response variants are unwrapped** (a hand-maintained variant list
//!      — the `BrahmaConsole`-missing-since-P4.D57 class), on every success path,
//!      incl. create's 201.
//!   3. **The bodies decode THROUGH the `Request` enum**: a missing `characterIds`
//!      key reaches the handler as a present-and-invalid value (400), not a
//!      silent default; `q` absent reaches the search verb as `None` → `''`.
//!   4. **The send-driver refusal.** This server has no spine, so the
//!      `HelpChatSend` arm must answer its NAMED refusal — never a silent 200 —
//!      AFTER v4's own prologue (a bad body still 400s, a salon chat still 404s).
//!   5. **The boot ensure ran**: the served instance's `help_docs` grew from the
//!      fixture's 17 rows to the embedded tree's 120, and the fixture's rows kept
//!      their ids (content hashes agree).
//!
//! Run:
//!   cargo test -p quilltap-web --test help_web_routes

mod common;

use serde_json::{json, Value};

/// The fixture's `help/brahma-console.md` row id — DERIVED from the committed
/// `help-chat-main.db.meta.json`, never transcribed. v4's real `syncHelpDocs()`
/// mints doc ids with `randomUUID`, so every fixture rebuild re-mints them and a
/// literal here goes stale silently (it did, at P4.D162's rebuild). The meta
/// file is written by the builder alongside the databases, so the two can never
/// drift apart.
fn brahma_doc_id() -> String {
    let meta = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/help-chat-main.db.meta.json"),
    )
    .expect("the committed fixture meta");
    let v: serde_json::Value = serde_json::from_str(&meta).expect("meta json");
    v["helpDocs"]["byPath"]["help/brahma-console.md"]["id"]
        .as_str()
        .expect("brahma-console doc id in the fixture meta")
        .to_string()
}
const C1: &str = "b0000002-0000-4000-8000-000000000001";
const C2: &str = "b0000002-0000-4000-8000-000000000002";
const H1: &str = "c1000002-0000-4000-8000-000000000001";
const H2: &str = "c1000002-0000-4000-8000-000000000002";
const H3: &str = "c1000002-0000-4000-8000-000000000003";
const SALON: &str = "c1000002-0000-4000-8000-000000000031";

async fn get(client: &reqwest::Client, addr: &std::net::SocketAddr, path: &str) -> (u16, Value) {
    let resp = client
        .get(format!("http://{addr}{path}"))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap())
}
async fn send(
    client: &reqwest::Client,
    addr: &std::net::SocketAddr,
    method: reqwest::Method,
    path: &str,
    body: &str,
) -> (u16, Value) {
    let resp = client
        .request(method, format!("http://{addr}{path}"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn help_web_edges() {
    let base = common::materialize_help_chat_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    use reqwest::Method;
    let (post, patch, delete) = (Method::POST, Method::PATCH, Method::DELETE);

    // --- 5. the boot ensure: 17 fixture docs → the 120-file embedded tree ---
    let (status, body) = get(&client, &addr, "/api/v1/help-docs").await;
    assert_eq!(status, 200);
    let docs = body["documents"].as_array().expect("documents");
    assert_eq!(docs.len(), 120, "the boot ensure synced the embedded tree");
    assert!(
        docs.iter()
            .any(|d| d["id"] == brahma_doc_id().as_str() && d["slug"] == "brahma-console"),
        "the fixture's unchanged rows keep their ids"
    );
    // --- 1. help-docs: the default-serving action shape ---
    let (_, bogus) = get(&client, &addr, "/api/v1/help-docs?action=bogus").await;
    assert_eq!(
        bogus["documents"].as_array().map(|a| a.len()),
        Some(120),
        "unknown action → list"
    );
    let (_, empty) = get(&client, &addr, "/api/v1/help-docs?action=").await;
    assert_eq!(
        empty["documents"].as_array().map(|a| a.len()),
        Some(120),
        "empty action → list"
    );
    let (status, body) = get(&client, &addr, "/api/v1/help-docs?action=chat-count").await;
    assert_eq!(
        (status, body),
        (200, json!({ "count": 1 })),
        "one salon chat"
    );
    // --- 3. `q` absent decodes to None → '' → the short-circuit ---
    let (status, body) = get(&client, &addr, "/api/v1/help-docs?action=search").await;
    assert_eq!((status, body), (200, json!({ "matches": [] })));
    let (status, body) = get(&client, &addr, "/api/v1/help-docs?action=search&q=Brahma").await;
    assert_eq!(status, 200);
    assert!(body["matches"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["slug"] == "brahma-console" && m["titleHit"] == true));
    let (status, body) = get(&client, &addr, "/api/v1/help-docs/brahma-console").await;
    assert_eq!(status, 200);
    assert_eq!(body["document"]["id"], brahma_doc_id().as_str());
    assert!(
        body["document"].get("slug").is_none(),
        "the single-document body has NO slug"
    );
    let (status, body) = get(&client, &addr, "/api/v1/help-docs/no-such-doc").await;
    assert_eq!(
        (status, body),
        (404, json!({ "error": "Help document not found" }))
    );

    // --- help-chats: the envelope action shape ---
    let (status, body) = get(&client, &addr, "/api/v1/help-chats").await;
    assert_eq!(status, 200);
    // TWELVE since P4.D162 added H12, the GOOGLE seat (the fixture's only
    // non-ANTHROPIC profile — the one plugin that KEEPS an id-less tool row).
    assert_eq!(body["chats"].as_array().map(|a| a.len()), Some(12));
    let (status, body) = get(&client, &addr, "/api/v1/help-chats?action=").await;
    assert_eq!(status, 200);
    assert_eq!(
        body["chats"].as_array().map(|a| a.len()),
        Some(12),
        "empty action → the no-action leg"
    );
    let (status, body) = get(&client, &addr, "/api/v1/help-chats?action=eligibility").await;
    assert_eq!(status, 200);
    assert_eq!(body["eligible"], true);
    let (status, body) = get(&client, &addr, "/api/v1/help-chats?action=bogus").await;
    assert_eq!(
        (status, body),
        (
            400,
            json!({ "error": "Unknown action: bogus. Available actions: eligibility" })
        )
    );

    // --- 2 + 3. create: 201, the six-null echo, and the missing-key refusal ---
    let (status, body) = send(
        &client,
        &addr,
        post.clone(),
        "/api/v1/help-chats",
        &json!({ "characterIds": [C1, C2], "pageUrl": "/salon/x" }).to_string(),
    )
    .await;
    assert_eq!(status, 201, "v4 `created`");
    let new_id = body["chat"]["id"].as_str().expect("chat id").to_string();
    assert_eq!(body["chat"]["chatType"], "help");
    assert_eq!(body["chat"]["helpPageUrl"], "/salon/x");
    assert_eq!(body["chat"]["title"], "Help: Marigold");
    assert!(
        body["chat"].get("projectId") == Some(&Value::Null),
        "the explicit nulls survive"
    );
    assert_eq!(
        body["chat"]["participants"].as_array().map(|a| a.len()),
        Some(2)
    );
    let (status, body) = send(
        &client,
        &addr,
        post.clone(),
        "/api/v1/help-chats",
        r#"{"pageUrl":"/x"}"#,
    )
    .await;
    assert_eq!(
        (status, body),
        (400, json!({ "error": "Validation error" })),
        "a missing key is refused, not defaulted"
    );

    // --- item: get / rename / update-context / unknown action / delete ---
    let (status, body) = get(&client, &addr, &format!("/api/v1/help-chats/{new_id}")).await;
    assert_eq!(status, 200);
    assert_eq!(
        body["chat"]["messageCount"], 1,
        "the SYSTEM row create wrote"
    );
    let (status, body) = send(
        &client,
        &addr,
        patch.clone(),
        &format!("/api/v1/help-chats/{H2}"),
        r#"{"title":"Renamed"}"#,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["chat"]["title"], "Renamed");
    let (status, body) = send(
        &client,
        &addr,
        patch.clone(),
        &format!("/api/v1/help-chats/{H2}?action="),
        r#"{"title":"Via empty"}"#,
    )
    .await;
    assert_eq!(
        (status, body["chat"]["title"].clone()),
        (200, json!("Via empty")),
        "`?action=` renames"
    );
    let (status, body) = send(
        &client,
        &addr,
        patch.clone(),
        &format!("/api/v1/help-chats/{H2}?action=update-context"),
        r#"{"pageUrl":"/files"}"#,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["chat"]["helpPageUrl"], "/files");
    let (status, body) = send(
        &client,
        &addr,
        patch.clone(),
        &format!("/api/v1/help-chats/{H2}?action=bogus"),
        r#"{"title":"x"}"#,
    )
    .await;
    assert_eq!(
        (status, body),
        (
            400,
            json!({ "error": "Unknown action: bogus. Available actions: update-context" })
        )
    );
    let (status, body) = get(&client, &addr, &format!("/api/v1/help-chats/{H2}/messages")).await;
    assert_eq!(status, 200);
    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4, "3 seeded + the navigation SYSTEM row");
    assert_eq!(msgs[3]["content"], "[System: User navigated to /files]");
    let (status, body) = send(
        &client,
        &addr,
        delete.clone(),
        &format!("/api/v1/help-chats/{H3}"),
        "",
    )
    .await;
    assert_eq!(
        (status, body),
        (200, json!({ "message": "Help chat deleted successfully" }))
    );
    let (status, _) = get(&client, &addr, &format!("/api/v1/help-chats/{H3}")).await;
    assert_eq!(status, 404);
    let (status, body) = get(&client, &addr, &format!("/api/v1/help-chats/{SALON}")).await;
    assert_eq!(
        (status, body),
        (404, json!({ "error": "Help chat not found" }))
    );

    // --- 4. send: v4's prologue first, then the NAMED driver refusal ---
    let (status, body) = send(
        &client,
        &addr,
        post.clone(),
        &format!("/api/v1/help-chats/{SALON}/messages"),
        r#"{"content":123}"#,
    )
    .await;
    assert_eq!(
        (status, body),
        (404, json!({ "error": "Help chat not found" })),
        "verify before parse"
    );
    let (status, body) = send(
        &client,
        &addr,
        post.clone(),
        &format!("/api/v1/help-chats/{H1}/messages"),
        r#"{"content":""}"#,
    )
    .await;
    assert_eq!(
        (status, body),
        (400, json!({ "error": "Validation error" }))
    );
    let (status, body) = send(
        &client,
        &addr,
        post.clone(),
        &format!("/api/v1/help-chats/{H1}/messages"),
        r#"{"content":"hello"}"#,
    )
    .await;
    assert_eq!(
        status, 500,
        "no spine → the named refusal, never a silent success"
    );
    assert_eq!(
        body["error"],
        "help chat send not available: no HelpChatSendDriver is assembled"
    );
    // The refused send wrote NOTHING: H1 still has its five seeded rows.
    let (_, body) = get(&client, &addr, &format!("/api/v1/help-chats/{H1}/messages")).await;
    assert_eq!(body["messages"].as_array().map(|a| a.len()), Some(5));
}
