//! P4.D163 — the five character-subprompt REST edges over a REAL HTTP server
//! (the `chat_delete_edge` shape): v4's URLs, the create's **201**, the raw
//! bodies, and the one arm the edge owns — a body that is not an object. The
//! guard ladders and the envelopes themselves are pinned against v4 by
//! `subprompts_routes_equivalence` (harness); this test proves the WIRE: that
//! each URL reaches its handler through the router and answers v4's status +
//! body shape.
//!
//! Run standalone:
//!   cargo test -p quilltap-web --test subprompts_web_routes

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

/// The fixture's ids (`harness/oracle/fixtures/subprompts.json`).
const CHAR_A: &str = "a1000000-0000-4000-8000-0000000000a1";
const CHAR_D: &str = "a1000000-0000-4000-8000-0000000000d4";
const MISSING: &str = "99999999-9999-4999-8999-999999999999";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subprompts_edges() {
    let base = common::materialize_subprompts_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let base_path = format!("/api/v1/characters/{CHAR_A}/subprompts");

    // --- GET list: the fixture's six root files, title-sorted ------------------
    let (status, body) = send(&client, &addr, reqwest::Method::GET, &base_path, None).await;
    assert_eq!(status, 200, "{body}");
    let ids: Vec<&str> = body["subprompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["zulu", "Verse", "terse", "no-title", "alpha"],
        "title order, nested + .txt + broken excluded"
    );

    // --- POST create: 201 + {subprompt} ---------------------------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::POST,
        &base_path,
        Some(r#"{"title":"Wire test","content":"Answer on the wire."}"#),
    )
    .await;
    assert_eq!(status, 201, "v4 `created(...)` — {body}");
    assert_eq!(body["subprompt"]["id"], "wire-test");
    assert_eq!(body["subprompt"]["path"], "Subprompts/wire-test.md");

    // --- POST with a NON-OBJECT body: the edge's own arm, v4's exact envelope --
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::POST,
        &base_path,
        Some("null"),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(
        body,
        json!({
            "error": "Validation error",
            "details": [{
                "expected": "object", "code": "invalid_type", "path": [],
                "message": "Invalid input: expected object, received null"
            }]
        })
    );

    // --- POST bad body on a MISSING character: 400 beats 404 (measured) -------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::POST,
        &format!("/api/v1/characters/{MISSING}/subprompts"),
        Some(r#"{"title":"","content":"x"}"#),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "Validation error");
    assert_eq!(body["details"][0]["code"], "too_small");

    // --- GET one + the id gate + the archived read ----------------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::GET,
        &format!("{base_path}/terse"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["subprompt"]["title"], "Be terse");
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::GET,
        &format!("{base_path}/a%2Fb"),
        None,
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "Invalid subprompt id");
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::GET,
        &format!("/api/v1/characters/{CHAR_D}/subprompts/keep"),
        None,
    )
    .await;
    assert_eq!(status, 200, "an archived character still READS — {body}");

    // --- PUT: 200 + {subprompt}; the archived write refuses 409 ----------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::PUT,
        &format!("{base_path}/terse"),
        Some(r#"{"title":"Be brief"}"#),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["subprompt"]["title"], "Be brief");
    assert_eq!(body["subprompt"]["id"], "terse");
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::PUT,
        &format!("/api/v1/characters/{CHAR_D}/subprompts/keep"),
        Some(r#"{"title":"x"}"#),
    )
    .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["error"],
        "Character is archived; subprompts cannot be edited"
    );

    // --- DELETE: {success:true}, then the gone id is 404 -----------------------
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::DELETE,
        &format!("{base_path}/terse"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "success": true }));
    let (status, body) = send(
        &client,
        &addr,
        reqwest::Method::DELETE,
        &format!("{base_path}/terse"),
        None,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["error"], "Subprompt not found");
    // The strip reached the seat: the LLM chat's selection lost `terse` —
    // read straight off the instance's main partition (the fixture is minimal
    // on purpose and the chat GET's enrichment wants tables it lacks).
    let w = quilltap_core::db::Writer::open_writable(
        &base.path().join("data").join("quilltap.db"),
        common::TEST_PEPPER,
    )
    .unwrap();
    let participants: String = w
        .connection()
        .query_row(
            "SELECT participants FROM chats WHERE id = 'c1000000-0000-4000-8000-000000000001'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let parsed: Value = serde_json::from_str(&participants).unwrap();
    assert_eq!(
        parsed[0]["selectedSubpromptIds"],
        json!(["VERSE"]),
        "DELETE fans out with removeSelection — {participants}"
    );
}
