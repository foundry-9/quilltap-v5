//! P4.83 — the five prompt-template REST edges over a REAL HTTP server (the
//! `subprompts_web_routes` shape): v4's URLs, the create's **201**, the raw
//! bodies, the lazy seeding actually running inside the server's own writer,
//! and the one arm the edge owns — a body that is not an object. The guard
//! ladders and the envelopes themselves are pinned against v4 by
//! `prompt_templates_routes_equivalence` (harness); this test proves the WIRE:
//! that each URL reaches its handler through the router and answers v4's status
//! + body shape.
//!
//! The instance is PROVISIONED FRESH here (`provision_fresh_instance`, the same
//! path a new v5 instance takes) rather than materialized from a committed
//! fixture — this lane commits nothing under `fixtures/`, and a fresh instance
//! is exactly the state whose first list must seed.
//!
//! Run standalone:
//!   cargo test -p quilltap-web --test prompt_templates_web_routes

mod common;

use serde_json::{json, Value};

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

const COLLECTION: &str = "/api/v1/prompt-templates";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompt_template_edges() {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    quilltap_core::services::provisioning::provision_fresh_instance(&data, common::TEST_PEPPER)
        .expect("provision a fresh instance");

    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // --- GET list on a FRESH instance: the lazy seeding runs, 21 built-ins -----
    let (status, body) = send(&client, &addr, reqwest::Method::GET, COLLECTION, None).await;
    assert_eq!(status, 200, "{body}");
    let templates = body["templates"].as_array().unwrap();
    assert_eq!(
        templates.len(),
        21,
        "the first list SEEDS v4's 21 sample prompts — {body}"
    );
    assert_eq!(body["count"], 21);
    // The registry's DISPLAY name, not the filename (the P4.83 measurement).
    assert_eq!(templates[0]["name"], "CLAUDE Companion");
    assert_eq!(
        templates[0]["description"],
        "COMPANION prompt optimized for CLAUDE models"
    );
    assert_eq!(templates[0]["isBuiltIn"], true);
    // A built-in's `userId` is SQL NULL, and v4's wire OMITS a null column.
    assert!(
        !templates[0].as_object().unwrap().contains_key("userId"),
        "a built-in carries no userId key — {}",
        templates[0]
    );
    let builtin_id = templates[0]["id"].as_str().unwrap().to_string();

    // --- the SECOND list seeds nothing ---------------------------------------
    let (status, body) = send(&client, &addr, reqwest::Method::GET, COLLECTION, None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["count"], 21, "the seeding is insert-if-absent");

    // --- POST create: 201 + {template}, with explicit nulls -------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::POST,
        COLLECTION,
        Some(r#"{"name":"Wire test","content":"Answer on the wire."}"#),
    )
    .await;
    assert_eq!(
        status, 201,
        "v4 `NextResponse.json(..., {{status: 201}})` — {body}"
    );
    assert_eq!(body["template"]["name"], "Wire test");
    assert_eq!(
        body["template"]["description"],
        Value::Null,
        "the CREATE body carries explicit nulls (it validates the input, never re-reads)"
    );
    let created = body["template"]["id"].as_str().unwrap().to_string();

    // --- POST with a NON-OBJECT body: the edge's own arm, v4's exact envelope --
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::POST,
        COLLECTION,
        Some("null"),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(
        body,
        json!({
            "error": "Validation error",
            "details": [{
                "expected": "object",
                "code": "invalid_type",
                "path": [],
                "message": "Invalid input: expected object, received null"
            }]
        })
    );

    // --- GET [id] -------------------------------------------------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::GET,
        &format!("{COLLECTION}/{created}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["template"]["name"], "Wire test");
    assert!(
        !body["template"]
            .as_object()
            .unwrap()
            .contains_key("description"),
        "the READ wire OMITS a null column — {body}"
    );

    // --- GET a missing id -----------------------------------------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::GET,
        &format!("{COLLECTION}/99999999-9999-4999-8999-999999999999"),
        None,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body, json!({ "error": "Template not found" }));

    // --- PUT the user template ------------------------------------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::PUT,
        &format!("{COLLECTION}/{created}"),
        Some(r#"{"name":"Renamed on the wire"}"#),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["template"]["name"], "Renamed on the wire");

    // --- PUT / DELETE a BUILT-IN: the route's 403 refusals ---------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::PUT,
        &format!("{COLLECTION}/{builtin_id}"),
        Some(r#"{"name":"Hijacked"}"#),
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body, json!({ "error": "Cannot update built-in templates" }));

    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::DELETE,
        &format!("{COLLECTION}/{builtin_id}"),
        None,
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body, json!({ "error": "Cannot delete built-in templates" }));

    // --- DELETE the user template --------------------------------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::DELETE,
        &format!("{COLLECTION}/{created}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "success": true }));

    // The built-ins survived both refusals; the user template is gone.
    let (_, body) = send(&client, &addr, reqwest::Method::GET, COLLECTION, None).await;
    assert_eq!(body["count"], 21, "{body}");
}

/// The gap the P4.83 e2e beat's first live run found: an instance whose main
/// partition has **no `prompt_templates` table at all**. v4 creates it lazily
/// (`AbstractBaseRepository.getCollection()` → `ensureCollection`, on the first
/// access of any kind), so a v4 instance nobody has ever opened the
/// Import-from-Template modal on simply has no such table — and v5 provisioned
/// it only on a FRESH instance. Before the fix this answered
/// `{templates: [], count: 0}` with `no such table` in the log and the modal
/// showed v4's "No templates available" forever.
///
/// A BARE instance (a writable main DB with no tables at all) is the strongest
/// form of that shape, and this is the arm that keeps the ensure honest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bare_instance_gets_the_table_and_the_catalogue() {
    let base = common::materialize_bare_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    let (status, body) = send(&client, &addr, reqwest::Method::GET, COLLECTION, None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["count"], 21,
        "the first list must CREATE the table and seed v4's 21 sample prompts — {body}"
    );
    assert_eq!(body["templates"][0]["name"], "CLAUDE Companion");

    // Idempotent: the second list neither re-creates nor re-seeds.
    let (status, body) = send(&client, &addr, reqwest::Method::GET, COLLECTION, None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["count"], 21);
}
