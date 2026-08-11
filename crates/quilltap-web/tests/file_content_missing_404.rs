//! P4.D62 / v4 Bug 55 (`d553f72a`): a `files` row can outlive its bytes, and
//! serving it 500'd on every render — inviting retries that can never work and
//! burying real storage faults. Both file routes now answer **404** for that
//! one condition and keep 500 for everything else.
//!
//! This is a web-edge test rather than an oracle differential on purpose: the
//! two arms are fixed sentences, quoted from v4's `responses.ts` helpers
//! (`notFound('File content')` → `{"error":"File content not found"}`,
//! `serverError('Failed to serve file' | 'Failed to download file')`), and what
//! actually needs proving is the PLUMBING no NDJSON diff can see — that the
//! typed carry survives the `Result<_, String>` storage trait, the manager's
//! wrapper, and the axum handler, and that a genuine fault still 500s.
//!
//! Three rows, each reached by BOTH routes (`/api/v1/files/{id}` and
//! `/api/v1/files/proxy/{key}`):
//!
//!  1. bytes present            → 200 with the bytes;
//!  2. a `mount-blob:` key whose blob does not exist → 404 (v4's
//!     `FileContentMissingError` from `manager.ts:389`);
//!  3. a disk key with nothing at that path → 404 (v4's local backend's ENOENT
//!     arm), which is the leg that proves the sentinel survives the trait.
//!
//! Run: `cargo test -p quilltap-web --test file_content_missing_404`

mod common;

use quilltap_core::db::Writer;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// The engine's single-user id — the `photos-*` fixture family already mints
/// its rows under it, which is why this test borrows that family: it is the
/// committed instance that carries a provisioned `files` table.
const USER_ID: &str = "00000000-0000-4000-8000-000000000001";

const LLM_LOGS_DDL: &str = "CREATE TABLE llm_logs (\
    id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
    chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
    modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
    request TEXT, response TEXT, usage TEXT, \
    cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
    durationMs REAL, createdAt TEXT, updatedAt TEXT);";

fn materialize_files_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fixtures.join("photos-main.db"), data.join("quilltap.db")).unwrap();
    std::fs::copy(
        fixtures.join("photos-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    let w = Writer::open_writable(&data.join("quilltap-llm-logs.db"), TEST_PEPPER).unwrap();
    w.connection().execute_batch(LLM_LOGS_DDL).unwrap();
    base
}

fn insert_file(
    conn: &rusqlite::Connection,
    id: &str,
    filename: &str,
    storage_key: &str,
    sha: &str,
) {
    conn.execute(
        "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, \
           linkedTo, source, category, tags, storageKey, fileStatus, createdAt, updatedAt) \
         VALUES (?1, ?2, ?3, ?4, 'application/octet-stream', 3, '[]', 'UPLOADED', \
                 'DOCUMENT', '[]', ?5, 'ok', '2026-03-01T00:00:00.000Z', \
                 '2026-03-01T00:00:00.000Z')",
        rusqlite::params![id, USER_ID, sha, filename, storage_key],
    )
    .expect("insert files row");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_row_without_bytes_is_404_and_a_real_fault_is_still_500() {
    let base = materialize_files_instance();
    let data = base.path().join("data");

    // One file whose bytes really are on disk, and two whose are not: a
    // mount-blob key naming a blob that does not exist, and a disk key with
    // nothing at that path.
    // The local backend roots at `<base>/files` (v4 `getFilesDir()`), a SIBLING
    // of the data dir.
    let files_dir = base.path().join("files");
    let present_key = format!("{USER_ID}/present.bin");
    std::fs::create_dir_all(files_dir.join(USER_ID)).expect("files dir");
    std::fs::write(files_dir.join(&present_key), b"abc").expect("write bytes");

    {
        let w = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).expect("open main");
        let conn = w.connection();
        insert_file(
            conn,
            "fa000000-0000-4000-8000-000000000001",
            "present.bin",
            &present_key,
            "aaaa",
        );
        insert_file(
            conn,
            "fa000000-0000-4000-8000-000000000002",
            "ghost-blob.bin",
            "mount-blob:fa000000-0000-4000-8000-0000000000aa:fa000000-0000-4000-8000-0000000000bb",
            "bbbb",
        );
        insert_file(
            conn,
            "fa000000-0000-4000-8000-000000000003",
            "ghost-disk.bin",
            &format!("{USER_ID}/never-written.bin"),
            "cccc",
        );
    }

    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = |p: &str| format!("http://{addr}{p}");

    // 1. Bytes present — both routes serve them.
    let resp = client
        .get(url("/api/v1/files/fa000000-0000-4000-8000-000000000001"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "a file with bytes still downloads");
    assert_eq!(resp.bytes().await.unwrap().as_ref(), b"abc");

    let resp = client
        .get(url(&format!("/api/v1/files/proxy/{present_key}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "the proxy still serves present bytes");

    // 2 + 3. The row outlived its bytes — 404 with v4's exact body, on BOTH
    // routes and for BOTH shapes of absence.
    for (id, key) in [
        (
            "fa000000-0000-4000-8000-000000000002",
            "mount-blob:fa000000-0000-4000-8000-0000000000aa:fa000000-0000-4000-8000-0000000000bb"
                .to_string(),
        ),
        (
            "fa000000-0000-4000-8000-000000000003",
            format!("{USER_ID}/never-written.bin"),
        ),
    ] {
        let resp = client
            .get(url(&format!("/api/v1/files/{id}")))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            404,
            "a row without bytes must be 404, not 500 (by id, {id})"
        );
        let body: Value = resp.json().await.unwrap();
        assert_eq!(
            body,
            serde_json::json!({"error": "File content not found"}),
            "v4 `notFound('File content')` body, verbatim"
        );

        let resp = client
            .get(url(&format!("/api/v1/files/proxy/{key}")))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            404,
            "…and the same through the proxy route ({key})"
        );
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body, serde_json::json!({"error": "File content not found"}));
    }

    // 4. A file row with NO storage key at all is a different condition — v4
    //    keeps it a 500, and its sentence is not the missing-content one.
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).expect("reopen main");
        w.connection()
            .execute(
                "UPDATE files SET storageKey = '' WHERE id = ?1",
                rusqlite::params!["fa000000-0000-4000-8000-000000000003"],
            )
            .expect("clear storage key");
    }
    let resp = client
        .get(url("/api/v1/files/fa000000-0000-4000-8000-000000000003"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        500,
        "a missing storage KEY is a fault, not a gone condition"
    );
    let body: Value = resp.json().await.unwrap();
    assert_ne!(
        body,
        serde_json::json!({"error": "File content not found"}),
        "the 404 body must not leak onto a genuine failure"
    );
}
