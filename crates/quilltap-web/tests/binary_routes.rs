//! Binary-route integration (D4 subset): serve a fixture file (proxy + by-id),
//! a generated + cached thumbnail, a mount-file raw read, and a blob read
//! (documents fallback) — asserting bytes, content-type, cache headers, and
//! the sha headers per the v4 routes.
//!
//! Run:
//!   cargo test -p quilltap-web --test binary_routes

mod common;

use quilltap_core::db::Writer;
use serde_json::Value;

/// A valid 1×1 red PNG (69 bytes).
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xC9, 0xFE, 0x92, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

const FILE_ID: &str = "aaaaaaaa-1111-4111-8111-111111111111";
const IMG_ID: &str = "bbbbbbbb-2222-4222-8222-222222222222";

/// Seed a `files` table (the fixture builder never creates one) with a text
/// entry + an image entry, and write their bytes into `<base>/files/`.
fn seed_files(base: &std::path::Path) {
    let data = base.join("data");
    let files_root = base.join("files");
    std::fs::create_dir_all(files_root.join("test")).unwrap();
    std::fs::write(files_root.join("test/hello.txt"), b"hello quilltap").unwrap();
    std::fs::write(files_root.join("test/dot.png"), TINY_PNG).unwrap();

    let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
    w.connection()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS files (\
               id TEXT PRIMARY KEY, sha256 TEXT, originalFilename TEXT, mimeType TEXT, \
               size REAL, width REAL, height REAL, category TEXT, generationPrompt TEXT, \
               generationModel TEXT, generationRevisedPrompt TEXT, description TEXT, \
               storageKey TEXT, createdAt TEXT, updatedAt TEXT);",
        )
        .unwrap();
    w.connection()
        .execute(
            "INSERT INTO files (id, sha256, originalFilename, mimeType, size, category, storageKey, createdAt, updatedAt) \
             VALUES (?1, 'sha-txt', 'héllo.txt', 'text/plain', 14, 'DOCUMENT', 'test/hello.txt', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
            rusqlite::params![FILE_ID],
        )
        .unwrap();
    w.connection()
        .execute(
            "INSERT INTO files (id, sha256, originalFilename, mimeType, size, width, height, category, storageKey, createdAt, updatedAt) \
             VALUES (?1, 'sha-png', 'dot.png', 'image/png', 69, 1, 1, 'IMAGE', 'test/dot.png', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
            rusqlite::params![IMG_ID],
        )
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn binary_routes_serve_files_thumbnails_and_mount_bytes() {
    let base = common::materialize_fixture_instance();
    seed_files(base.path());
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // --- files proxy by storage key ---
    let resp = client
        .get(format!("http://{addr}/api/v1/files/proxy/test/hello.txt"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["content-type"], "text/plain");
    assert_eq!(
        resp.headers()["cache-control"],
        "public, max-age=31536000, immutable"
    );
    assert_eq!(resp.headers()["x-frame-options"], "SAMEORIGIN");
    assert_eq!(
        resp.headers()["content-security-policy"],
        "frame-ancestors 'self'"
    );
    // RFC 5987: the non-ASCII filename gets the ASCII fallback + filename*.
    let cd = resp.headers()["content-disposition"].to_str().unwrap();
    assert!(cd.starts_with("inline; filename=\"h_llo.txt\""), "{cd}");
    assert!(cd.contains("filename*=UTF-8''h%C3%A9llo.txt"), "{cd}");
    assert_eq!(resp.bytes().await.unwrap().as_ref(), b"hello quilltap");

    // Unknown key → 404 {"error":"File not found"}.
    let resp = client
        .get(format!("http://{addr}/api/v1/files/proxy/no/such.bin"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(body["error"], "File not found");

    // --- files by id ---
    let resp = client
        .get(format!("http://{addr}/api/v1/files/{FILE_ID}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), b"hello quilltap");

    // --- thumbnail: generated then cache-served ---
    for pass in ["generate", "cached"] {
        let resp = client
            .get(format!(
                "http://{addr}/api/v1/files/{IMG_ID}?action=thumbnail&size=64"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "pass: {pass}");
        assert_eq!(resp.headers()["content-type"], "image/webp");
        assert_eq!(
            resp.headers()["cache-control"],
            "public, max-age=31536000, immutable"
        );
        let bytes = resp.bytes().await.unwrap();
        assert!(bytes.len() > 8, "pass: {pass}");
        assert_eq!(&bytes[..4], b"RIFF", "pass: {pass}");
    }
    // The cache landed at the canonical key on disk.
    assert!(base
        .path()
        .join("files/_thumbnails")
        .join(format!("{IMG_ID}_64.webp"))
        .is_file());

    // Invalid size → 400; non-image → 400.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/files/{IMG_ID}?action=thumbnail&size=zero"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/files/{FILE_ID}?action=thumbnail"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(body["error"], "File is not a resizable image");

    // --- mount-point raw file read + the blobs documents fallback ---
    // Discover a (mountPointId, relativePath, sha) from the fixture's store.
    let (mp, rel, sha): (String, String, String) = {
        let w = Writer::open_writable(
            &base.path().join("data/quilltap-mount-index.db"),
            common::TEST_PEPPER,
        )
        .unwrap();
        let row = w.connection().query_row(
            "SELECT l.mountPointId, l.relativePath, f.sha256 \
             FROM doc_mount_file_links l JOIN doc_mount_files f ON f.id = l.fileId \
             WHERE l.relativePath LIKE '%.md' LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        );
        row.expect("the fixture store has a markdown file")
    };

    let resp = client
        .get(format!(
            "http://{addr}/api/v1/mount-points/{mp}/files/{rel}?raw=1"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()["content-type"],
        "text/markdown; charset=utf-8"
    );
    assert_eq!(resp.headers()["cache-control"], "private, max-age=3600");
    assert_eq!(resp.headers()["x-file-sha256"].to_str().unwrap(), sha);

    // The Accept: application/octet-stream form works too.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/mount-points/{mp}/files/{rel}"
        ))
        .header("accept", "application/octet-stream")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Blobs route: no blob row at this path → the documents fallback.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/mount-points/{mp}/blobs/{rel}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()["content-type"],
        "text/markdown; charset=utf-8"
    );
    assert_eq!(resp.headers()["x-blob-sha256"].to_str().unwrap(), sha);

    // Unknown path → 404 Blob not found.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/mount-points/{mp}/blobs/no/such/thing.bin"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
