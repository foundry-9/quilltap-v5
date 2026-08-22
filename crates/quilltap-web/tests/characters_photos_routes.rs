//! Route-level integration for `POST /api/v1/characters/{id}/photos` (P4.6m): the
//! three legs (multipart upload, JSON `linkId`, JSON `fileId` in both
//! storage-key modes) + the v4 error arms, driven with REAL HTTP bodies against
//! the committed characters fixture (Aria + her vault).
//!
//! Run:
//!   cargo test -p quilltap-web --test characters_photos_routes

mod common;

use quilltap_core::db::Writer;
use serde_json::Value;

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const LOCAL_FILE_ID: &str = "f1000000-0000-4000-8000-0000000000aa";
const BLOB_FILE_ID: &str = "f1000000-0000-4000-8000-0000000000bb";
const TXT_FILE_ID: &str = "f1000000-0000-4000-8000-0000000000cc";

/// Seed a legacy `files` table (the fixture has none) with a LOCAL-key image
/// (bytes on disk), a `mount-blob:`-key image (pointing at Aria's avatar blob),
/// and a non-image row — the fileId leg's two storage modes + the guard arm.
fn seed_files(base: &std::path::Path, aria_mount: &str, avatar_blob: &str) {
    let files_root = base.join("files");
    std::fs::create_dir_all(files_root.join("test")).unwrap();
    // Distinct bytes so the local-fileId save doesn't dedup against the upload.
    std::fs::write(files_root.join("test/localimg.png"), b"local-image-bytes-B").unwrap();

    // The characters fixture already carries a real `files` table (userId +
    // sha256 + originalFilename + mimeType + size + source + category NOT NULL).
    let w = Writer::open_writable(&base.join("data/quilltap.db"), common::TEST_PEPPER).unwrap();
    let mount_blob_key = format!("mount-blob:{aria_mount}:{avatar_blob}");
    let ins = "INSERT INTO files \
        (id, userId, sha256, originalFilename, mimeType, size, source, category, storageKey, createdAt, updatedAt) \
        VALUES (?1, 'ffffffff-ffff-ffff-ffff-ffffffffffff', ?2, ?3, ?4, ?5, 'upload', ?6, ?7, \
                '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')";
    w.connection()
        .execute(
            ins,
            rusqlite::params![
                LOCAL_FILE_ID,
                "sha-local",
                "localimg.png",
                "image/png",
                19,
                "IMAGE",
                "test/localimg.png"
            ],
        )
        .unwrap();
    w.connection()
        .execute(
            ins,
            rusqlite::params![
                BLOB_FILE_ID,
                "sha-blob",
                "avatar.webp",
                "image/webp",
                100,
                "IMAGE",
                mount_blob_key
            ],
        )
        .unwrap();
    w.connection()
        .execute(
            ins,
            rusqlite::params![
                TXT_FILE_ID,
                "sha-txt",
                "notes.txt",
                "text/plain",
                9,
                "DOCUMENT",
                "test/localimg.png"
            ],
        )
        .unwrap();
}

/// Discover Aria's vault mount point, her avatar link id, and the avatar's blob
/// id (the `doc_mount_blobs.id` the `mount-blob:` key addresses).
fn discover(base: &std::path::Path) -> (String, String, String) {
    let w = Writer::open_writable(
        &base.join("data/quilltap-mount-index.db"),
        common::TEST_PEPPER,
    )
    .unwrap();
    w.connection()
        .query_row(
            "SELECT l.mountPointId, l.id, b.id \
             FROM doc_mount_file_links l JOIN doc_mount_blobs b ON b.fileId = l.fileId \
             WHERE l.relativePath = 'images/avatar.webp' LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("Aria's vault avatar link + blob")
}

async fn post_json(client: &reqwest::Client, url: &str, body: Value) -> (u16, Value) {
    let resp = client.post(url).json(&body).send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    let json = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (status, json)
}

#[tokio::test(flavor = "multi_thread")]
async fn characters_photos_post_all_three_legs_and_error_arms() {
    let base = common::materialize_characters_instance();
    let (aria_mount, avatar_link, avatar_blob) = discover(base.path());
    seed_files(base.path(), &aria_mount, &avatar_blob);
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let photos_url = format!("http://{addr}/api/v1/characters/{ARIA}/photos");

    // --- Leg 1: multipart upload ---
    let form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"upload-image-bytes-A".to_vec())
                .file_name("portrait.png")
                .mime_str("image/png")
                .unwrap(),
        )
        .text("caption", "A fine portrait")
        .text("tags", "airship")
        .text("tags", "adventure")
        .text("tags", ""); // empty tag dropped (v4 filter)
    let resp = client
        .post(&photos_url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "multipart upload");
    let body: Value = resp.json().await.unwrap();
    assert!(body["linkId"].as_str().is_some());
    assert_eq!(body["mountPointId"], aria_mount);
    assert!(
        body["relativePath"]
            .as_str()
            .unwrap()
            .starts_with("photos/"),
        "{body}"
    );
    assert_eq!(body["relativePath"], "photos/portrait.png");
    assert_eq!(body["sha256"].as_str().unwrap().len(), 64);

    // --- Leg 2: JSON linkId (save Aria's avatar into photos/) ---
    let (status, body) = post_json(
        &client,
        &photos_url,
        serde_json::json!({ "linkId": avatar_link }),
    )
    .await;
    assert_eq!(status, 201, "linkId leg: {body}");
    assert!(body["relativePath"]
        .as_str()
        .unwrap()
        .starts_with("photos/"));

    // --- Leg 3a: JSON fileId, LOCAL storage key (bytes off disk) ---
    let (status, body) = post_json(
        &client,
        &photos_url,
        serde_json::json!({ "fileId": LOCAL_FILE_ID, "caption": "from a file" }),
    )
    .await;
    assert_eq!(status, 201, "fileId local leg: {body}");
    assert_eq!(body["relativePath"], "photos/localimg.png");

    // --- Leg 3b: JSON fileId, mount-blob key (avatar bytes already in photos/
    //     via leg 2) → the dedup guard fires (proves the mount-blob fetch AND
    //     the same-sha refusal). ---
    let (status, body) = post_json(
        &client,
        &photos_url,
        serde_json::json!({ "fileId": BLOB_FILE_ID }),
    )
    .await;
    assert_eq!(status, 400, "mount-blob fileId dedup: {body}");
    assert!(
        body["error"].as_str().unwrap().contains("already in"),
        "{body}"
    );

    // --- Error arms ---
    // Missing "file" field.
    let form = reqwest::multipart::Form::new().text("caption", "no file");
    let resp = client
        .post(&photos_url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Non-image multipart upload → "Unsupported MIME type".
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"just text".to_vec())
            .file_name("notes.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let resp = client
        .post(&photos_url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let b: Value = resp.json().await.unwrap();
    assert!(b["error"]
        .as_str()
        .unwrap()
        .contains("Unsupported MIME type"));

    // fileId not found → 400 "Image not found".
    let (status, body) = post_json(
        &client,
        &photos_url,
        serde_json::json!({ "fileId": "no-such-file" }),
    )
    .await;
    assert_eq!(status, 400);
    assert!(body["error"].as_str().unwrap().contains("not found"));

    // Non-image fileId (the DOCUMENT row) → 400 "not an image".
    let (status, body) = post_json(
        &client,
        &photos_url,
        serde_json::json!({ "fileId": TXT_FILE_ID }),
    )
    .await;
    assert_eq!(status, 400);
    assert!(body["error"].as_str().unwrap().contains("not an image"));

    // Both fileId + linkId → 400 refine.
    let (status, _body) = post_json(
        &client,
        &photos_url,
        serde_json::json!({ "fileId": LOCAL_FILE_ID, "linkId": avatar_link }),
    )
    .await;
    assert_eq!(status, 400);

    // Neither fileId nor linkId → 400 refine.
    let (status, _body) = post_json(&client, &photos_url, serde_json::json!({})).await;
    assert_eq!(status, 400);

    // Unknown character → 404 (multipart upload against a missing id).
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"img".to_vec())
            .file_name("x.png")
            .mime_str("image/png")
            .unwrap(),
    );
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/characters/no-such-char/photos"
        ))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let b: Value = resp.json().await.unwrap();
    assert_eq!(b["error"], "Character not found");
}
