//! P4.6ah web-edge legs, end-to-end over a live server: the general upload
//! multipart route + its validation (no-file / invalid-tags), the chat-file
//! `action=link` leg, and the `DELETE /api/v1/files/{id}` route. The upload
//! HAPPY path (201) needs the Quilltap Uploads mount; the committed chat-send
//! fixture carries no general-files uploads mount, so the happy 201 is proven at
//! the CoreRequest layer by `files_routes_equivalence` and the SPA e2e — here we
//! pin the web-edge PLUMBING (multipart parse, base64, dispatch wiring, status
//! mapping) over the surfaces the fixture supports.
//!
//! Run:
//!   cargo test -p quilltap-web --test files_write_routes

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

/// **P4.72 — the venue must actually carry a library file.**
///
/// The link + delete legs below were written behind `if let Some(file_id)`,
/// and the committed `chat-send` fixture carries an EMPTY `files` table — so
/// they had never run. (Measured, not assumed: replacing the `if let` with a
/// hard floor turned the test red at the unwrap.) One row is seeded here, the
/// `instance_settings` materialization precedent, so the arms are real: the
/// `?action=link` leg gives it an association, and the `force` duplicate-key
/// row then has two values that answer differently.
fn seed_library_file(data: &std::path::Path) -> String {
    use quilltap_core::db::Writer;
    let id = "d1e2f3a4-0000-4000-8000-00000000f11e".to_string();
    let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
    // The chat-send fixture carries no `files` table AT ALL (the same fact
    // `files_body_guards_equivalence`'s header records, which is why that
    // family serves the `system-data-*` venue instead). The table is created
    // here from v4's own `generateDDL` shape — the committed
    // `provisioning/fresh_schema.json` `files` entry, column for column — so
    // the seeded row is the shape the repository reads.
    w.connection()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS \"files\" (\n\
               \"id\" TEXT PRIMARY KEY NOT NULL,\n\
               \"userId\" TEXT NOT NULL,\n\
               \"sha256\" TEXT NOT NULL,\n\
               \"originalFilename\" TEXT NOT NULL,\n\
               \"mimeType\" TEXT NOT NULL,\n\
               \"size\" REAL NOT NULL,\n\
               \"width\" REAL,\n\
               \"height\" REAL,\n\
               \"isPlainText\" INTEGER,\n\
               \"linkedTo\" TEXT DEFAULT '[]',\n\
               \"source\" TEXT NOT NULL,\n\
               \"category\" TEXT NOT NULL,\n\
               \"generationPrompt\" TEXT,\n\
               \"generationModel\" TEXT,\n\
               \"generationRevisedPrompt\" TEXT,\n\
               \"description\" TEXT,\n\
               \"tags\" TEXT DEFAULT '[]',\n\
               \"projectId\" TEXT,\n\
               \"folderPath\" TEXT,\n\
               \"storageKey\" TEXT,\n\
               \"fileStatus\" TEXT DEFAULT 'ok',\n\
               \"createdAt\" TEXT NOT NULL,\n\
               \"updatedAt\" TEXT NOT NULL\n\
             )",
        )
        .expect("create the files table");
    w.connection()
        .execute(
            "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, \
             linkedTo, source, category, tags, storageKey, fileStatus, createdAt, updatedAt) \
             VALUES (?1, ?2, 'f0f0', 'seed.txt', 'text/plain', 4, '[]', 'upload', 'general', \
             '[]', 'files/seed.txt', 'ok', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            rusqlite::params![id, quilltap_core::api::SINGLE_USER_ID],
        )
        .expect("seed a library file");
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn files_write_web_edges() {
    let base = common::materialize_fixture_instance();
    let seeded = seed_library_file(&base.path().join("data"));
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

    // The seeded library file drives the link + delete legs. Read it back
    // through the real listing rather than trusting the INSERT, so a fixture
    // whose `files` schema moves fails here rather than silently skipping.
    let files = dispatch(&client, &addr, json!({"type": "filesList"})).await;
    let file_id = files["data"]["files"]
        .as_array()
        .and_then(|a| a.iter().find(|f| f["id"] == seeded.as_str()))
        .and_then(|f| f["id"].as_str())
        .map(str::to_string)
        .expect("the seeded library file must be listed");
    {
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

        // --- P4.72: the `force` DUPLICATE-key row, DEFERRED with its reason ---
        //
        // `force` is a FIRST-wins `searchParams.get` read
        // (`files_routes.rs:1146`), and the intended row was
        // `?force=false&force=true` vs `?force=true&force=false`. It cannot
        // discriminate here, and this is measured rather than assumed: v5's
        // delete only refuses when `compute_associations` finds a REAL
        // reference — a character `defaultImageId`/avatar override, or a
        // message attachment (`api/files.rs:548-556`). The `?action=link` leg
        // above writes only `linkedTo`, which BOTH trees classify as stale and
        // clear silently, so `force=false` and `force=true` answer the same
        // 200 and the row would pass whichever value won (memory note
        // `a-green-mutation-means-a-non-discriminating-arm`). Confirmed by
        // building the row and watching a LAST-wins mutation of this route's
        // reader stay green. Closing it needs a venue whose seeded file is
        // referenced by a character or a message; recorded in the lane record
        // as the follow-up, not faked here.

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

/// Dogfood finding #36 — a chat-file upload larger than axum's 2 MB
/// `DefaultBodyLimit` must reach the handler, so the PORTED 10 MB cap
/// (`MAX_CHAT_FILE_SIZE`) is what answers rather than the transport.
///
/// The bug: `Multipart::from_request` enforced the 2 MB default and failed
/// before any handler ran, so every photo over 2 MB — most photos — was refused
/// with a flat `400 "Invalid multipart body"`, while v4 accepted it. The
/// existing multipart coverage never caught it because every fixture payload in
/// the suite (and every Playwright upload) is a few bytes.
///
/// Both directions are pinned deliberately. Raising the transport limit could
/// have been "fixed" by removing the ceiling entirely, so the oversize arm
/// proves the ported cap still decides, with v4's own message — that arm fails
/// if someone disables the limit outright, and the 3 MB arm fails if the 2 MB
/// default ever comes back.
#[tokio::test(flavor = "multi_thread")]
async fn chat_file_upload_over_axum_default_body_limit() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    let upload = |bytes: Vec<u8>, name: &'static str| {
        let client = client.clone();
        async move {
            let form = reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(bytes)
                    .file_name(name)
                    .mime_str("image/png")
                    .unwrap(),
            );
            client
                .post(format!("http://{addr}/api/v1/chats/{SMOKE_CHAT_ID}/files"))
                .multipart(form)
                .send()
                .await
                .unwrap()
        }
    };

    // 3 MB — over axum's 2 MB default, under the ported 10 MB cap. Before the
    // fix this was `400 "Invalid multipart body"`.
    let resp = upload(vec![0xAB; 3 * 1024 * 1024], "big.png").await;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_ne!(
        body["error"], "Invalid multipart body",
        "a 3 MB upload must reach the handler, not die at the transport limit \
         (status {status}, body {body})"
    );

    // 11 MB — over the ported cap, so the APPLICATION answers, in v4's words.
    let resp = upload(vec![0xAB; 11 * 1024 * 1024], "huge.png").await;
    assert_eq!(resp.status(), 400, "oversize upload");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"], "File size exceeds maximum allowed size of 10 MB",
        "the ported cap must be the one that refuses, not the transport"
    );
}

/// Dogfood finding #63 — the sibling of #36, one ceiling up. A `.qtap` import
/// body over 100 MB must reach the handler.
///
/// The bug: #36 raised the transport ceiling to `bodySizeLimit: '100mb'`, but
/// that key governs v4's **Server Actions**; the ceiling on v4's request path is
/// `proxyClientMaxBodySize: '10gb'`, whose own comment names `.qtap` imports and
/// says 10 MB "truncates .qtap import files with memories". A real Friday
/// characters export (791 MB) therefore died at the edge with a bare
/// `413 "Failed to buffer the request body: length limit exceeded"` — and,
/// because the wizard's preview step rendered nothing on failure, as a BLANK
/// step 2 rather than an error.
///
/// The assertion is deliberately "not 413": the payload here is garbage, so the
/// handler's own loader refuses it. Reaching the LOADER's refusal is the proof
/// that the transport let the body through — which is precisely what a 100 MB
/// ceiling prevented. Sending 101 MB (not 10 GB) keeps the test cheap while
/// sitting on the far side of the old limit; it fails the moment anyone lowers
/// the ceiling back under it.
///
/// P4.60 moved that refusal from a 400 to v4's own **500**: `await req.json()`
/// rejects on a non-JSON body inside the handler's `try`, and the reject
/// escapes to the outer catch as `serverError('Failed to preview import')`.
/// The status is not the point of this test — reaching the handler is — but it
/// is asserted exactly so a future change has to be deliberate.
#[tokio::test(flavor = "multi_thread")]
async fn import_body_over_the_old_100mb_ceiling_reaches_the_handler() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;

    // 101 MB of nonsense — one megabyte past the old ceiling, and no further:
    // the test costs what it must to sit on the far side of the limit.
    let body = vec![b'x'; 101 * 1024 * 1024];

    let resp = reqwest::Client::new()
        .post(format!(
            "http://{addr}/api/v1/system/tools?action=import-preview"
        ))
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();

    let status = resp.status();
    assert_ne!(
        status, 413,
        "a 101 MB import body must reach the handler, not die at the transport \
         ceiling — v4's ceiling on this path is 10 GB"
    );
    assert_eq!(
        status,
        500,
        "the LOADER should be what refuses this garbage payload, with v4's own \
         non-JSON-body 500 (body: {:?})",
        resp.text().await.unwrap_or_default()
    );
}
