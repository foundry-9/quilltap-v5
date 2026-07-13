//! The P4.6y web-edge legs, end-to-end over a live server: the raw-read
//! FS-mount branch, the item-route PUT (JSON + multipart ingest), the
//! `?action=write-file` multipart verb, and the multipart blob upload (201).
//! Boots a bare provisioned instance, creates a database + a filesystem mount
//! via dispatch, and drives the v4-shaped HTTP routes.

mod common;

use serde_json::{json, Value};

async fn dispatch(client: &reqwest::Client, addr: &std::net::SocketAddr, body: Value) -> Value {
    let resp = client
        .post(format!("http://{addr}/api/dispatch"))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let v: Value = resp.json().await.unwrap();
    assert!(status.is_success(), "dispatch {body} failed: {status} {v}");
    v
}

#[tokio::test(flavor = "multi_thread")]
async fn mount_write_and_read_edges() {
    // The committed chat-send fixture instance (full schema incl. the
    // mount-index partition) — mounts are created via dispatch below.
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // A database mount + a filesystem mount (basePath inside the temp dir).
    let db_mount = dispatch(
        &client,
        &addr,
        json!({"type": "mountPointCreate", "mountPoint": {
            "name": "Edge DB Store", "mountType": "database", "storeType": "documents"
        }}),
    )
    .await;
    let db_id = db_mount["data"]["mountPoint"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let fs_root = base.path().join("edge-fs");
    std::fs::create_dir_all(fs_root.join("notes")).unwrap();
    std::fs::write(
        fs_root.join("notes/on-disk.md"),
        b"# Disk\n\nfrom the tree\n",
    )
    .unwrap();
    let fs_mount = dispatch(
        &client,
        &addr,
        json!({"type": "mountPointCreate", "mountPoint": {
            "name": "Edge FS Store", "mountType": "filesystem",
            "basePath": fs_root.to_str().unwrap()
        }}),
    )
    .await;
    let fs_id = fs_mount["data"]["mountPoint"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // --- the raw-read FS branch (new this order) ---
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/mount-points/{fs_id}/files/notes/on-disk.md?raw=1"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "fs raw read");
    assert_eq!(
        resp.headers()["content-type"],
        "text/markdown; charset=utf-8"
    );
    assert!(resp.headers().contains_key("x-file-sha256"));
    assert_eq!(
        resp.bytes().await.unwrap().as_ref(),
        b"# Disk\n\nfrom the tree\n"
    );

    // Boundary escapes stay refused at the edge.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/mount-points/{fs_id}/files/..%2Fsecret.md?raw=1"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "escape refused");

    // --- the item-route PUT, JSON leg (storeMountFile ingest) ---
    let resp = client
        .put(format!(
            "http://{addr}/api/v1/mount-points/{db_id}/files/notes/put.md"
        ))
        .json(&json!({"content": "# Put\n\nvia JSON leg\n"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "json PUT");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["kind"], "document");
    assert_eq!(body["fileType"], "markdown");

    // --- the item-route PUT, multipart leg ---
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"# Put2\n\nvia multipart leg\n".to_vec())
            .file_name("put2.md")
            .mime_str("text/markdown")
            .unwrap(),
    );
    let resp = client
        .put(format!(
            "http://{addr}/api/v1/mount-points/{db_id}/files/notes/put2.md"
        ))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "multipart PUT");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["kind"], "document");

    // --- the ?action=write-file multipart verb (byte-preserving) ---
    let form = reqwest::multipart::Form::new()
        .text("path", "raw/edge.md")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"# Raw edge\n".to_vec())
                .file_name("edge.md")
                .mime_str("text/markdown")
                .unwrap(),
        );
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/mount-points/{db_id}?action=write-file"
        ))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "write-file action");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["destPath"], "raw/edge.md");
    assert!(body["sha256"].as_str().unwrap().len() == 64);

    // A non-write-file action on this route gets the loud dispatch pointer.
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/mount-points/{db_id}?action=scan"
        ))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("/api/dispatch"));

    // --- the multipart blob upload (201) ---
    let form = reqwest::multipart::Form::new()
        .text("path", "images/edge.bin")
        .text("description", "An edge-case byte bundle.")
        .part(
            "file",
            reqwest::multipart::Part::bytes(vec![1, 2, 3, 4, 5])
                .file_name("edge.bin")
                .mime_str("application/octet-stream")
                .unwrap(),
        );
    let resp = client
        .post(format!("http://{addr}/api/v1/mount-points/{db_id}/blobs"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "blob upload");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["blob"]["relativePath"], "images/edge.bin");
    assert_eq!(body["blob"]["description"], "An edge-case byte bundle.");

    // The uploaded blob streams back through the existing blobs GET.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/mount-points/{db_id}/blobs/images/edge.bin"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), &[1, 2, 3, 4, 5]);
}
