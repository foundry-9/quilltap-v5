//! P4.6ah web-edge legs, end-to-end over a live server: the general upload
//! multipart route + its validation (no-file / invalid-tags), the chat-file
//! `action=link` leg, and the `DELETE /api/v1/files/{id}` route. The upload
//! HAPPY path (201) needs the Quilltap Uploads mount; the committed chat-send
//! fixture carries no general-files uploads mount, so the happy 201 is proven at
//! the CoreRequest layer by `files_routes_equivalence` and the SPA e2e — here we
//! pin the web-edge PLUMBING (multipart parse, base64, dispatch wiring, status
//! mapping) over the surfaces the fixture supports.

mod common;

use serde_json::{json, Value};

const SMOKE_CHAT_ID: &str = "9fe3f87b-3833-46a9-bc1e-a88fc43dee6b";

async fn dispatch(client: &reqwest::Client, addr: &std::net::SocketAddr, body: Value) -> Value {
    let resp = client
        .post(format!("http://{addr}/api/dispatch"))
        .json(&body)
        .send()
        .await
        .unwrap();
    resp.json().await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn files_write_web_edges() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // --- upload route: no file → 400 ---
    let form = reqwest::multipart::Form::new().text("projectId", "");
    let resp = client
        .post(format!("http://{addr}/api/v1/files?action=upload"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "no-file upload");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "No file provided");

    // --- upload route: invalid tags JSON → 400 (web-edge parse) ---
    let form = reqwest::multipart::Form::new()
        .text("tags", "not-json")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"hi".to_vec())
                .file_name("x.txt")
                .mime_str("text/plain")
                .unwrap(),
        );
    let resp = client
        .post(format!("http://{addr}/api/v1/files?action=upload"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "invalid-tags upload");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Invalid tags JSON");

    // Find an existing library file (the orchestrator corpus carries generated
    // images) to drive the link + delete legs.
    let files = dispatch(&client, &addr, json!({"type": "filesList"})).await;
    let file_id = files["data"]["files"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|f| f["id"].as_str())
        .map(str::to_string);

    if let Some(file_id) = file_id {
        // --- chat-file action=link → 200 {file} ---
        let resp = client
            .post(format!(
                "http://{addr}/api/v1/chats/{SMOKE_CHAT_ID}/files?action=link"
            ))
            .json(&json!({ "fileId": file_id }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "chat-file link");
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["file"]["id"], file_id);
        assert_eq!(body["file"]["url"], body["file"]["filepath"]);

        // --- DELETE /api/v1/files/{id}?force=true → 200 {success:true} ---
        let resp = client
            .delete(format!("http://{addr}/api/v1/files/{file_id}?force=true"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "force delete");
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["success"], true);
    }

    // --- chat-file link with a missing fileId → 400 (web-edge validation) ---
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/chats/{SMOKE_CHAT_ID}/files?action=link"
        ))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "link without fileId");
}
