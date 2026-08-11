//! P4.D65/P4.D66 unification wire: `POST /api/v1/characters/{id}?action=…`.
//!
//! v5's SPA drives the character JSON actions over `/api/dispatch`, so no lane
//! shipped this REST edge — but v4's CLI (`quilltap db characters
//! archive|rehydrate`, ported in P4.D66) POSTs exactly this URL, and without
//! the edge the ported CLI cannot speak to v5's own server (the round's
//! cross-lane blind spot: D66's Tier R proves the request/print halves against
//! a canned stub, D65's differential proves the verbs, and nothing proved the
//! URL resolves). This is a web-edge test on the file_content_missing_404
//! pattern: what needs proving is the PLUMBING — the route exists, delegates
//! into the P4.D65 dispatch arms, and answers v4's envelope (raw result bag on
//! success, `{error}` + status on failure).
//!
//! The rehydrate leg is also a live regression leg for the carried-blob-id
//! dedupe (`quilltap_import/mod.rs`): the photos fixture's first vaulted
//! character carries a twice-linked (sha-deduped) blob, exactly the shape
//! whose per-link export duplication used to make the preflight refuse every
//! rehydrate — remove the dedupe and this test's rehydrate answers 400
//! `Preserve IDs collision … (also seen as document store blob)`. This was a
//! pinned v5 divergence when the round-2 unification found it; v4 CONVERGED in
//! `de9f70bf` (Bug 57), so the leg is now a plain equality on both sides.
//!
//! Run: `cargo test -p quilltap-web --test characters_action_route`

mod common;

use quilltap_core::db::Writer;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

const LLM_LOGS_DDL: &str = "CREATE TABLE llm_logs (\
    id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
    chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
    modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
    request TEXT, response TEXT, usage TEXT, \
    cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
    durationMs REAL, createdAt TEXT, updatedAt TEXT);";

/// The photos family is the committed instance with vaulted characters
/// (Aria / Bram) — a vaulted character exercises the real bundle path
/// (export → encrypt → verify) end to end.
fn materialize_instance() -> (tempfile::TempDir, String) {
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

    // Any vaulted character will do; the id is read, never assumed.
    let main = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).unwrap();
    // The narrow photos fixture predates the embedding family; every real
    // instance carries these (v4's migrations), and the prune's fail-soft
    // stages report `pruneComplete: false` without them — environmental noise
    // this wire test is not about. Minimal shapes: the prune only DELETEs.
    main.connection()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS vector_entries (\
                id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB);\
             CREATE TABLE IF NOT EXISTS vector_indices (\
                id TEXT PRIMARY KEY, characterId TEXT);\
             CREATE TABLE IF NOT EXISTS embedding_status (\
                id TEXT PRIMARY KEY, entityType TEXT, entityId TEXT, \
                profileId TEXT, status TEXT);",
        )
        .unwrap();
    let character_id: String = main
        .connection()
        .query_row(
            "SELECT id FROM characters WHERE characterDocumentMountPointId IS NOT NULL \
             ORDER BY name LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("a vaulted character in the photos fixture");
    drop(main);
    (base, character_id)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_action_url_archives_and_rehydrates() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("quilltap=warn")
        .try_init();
    let (base, character_id) = materialize_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url =
        |action: &str| format!("http://{addr}/api/v1/characters/{character_id}?action={action}");

    // 1. An action this edge does not serve names the dispatch channel.
    let resp = client.post(url("favorite")).send().await.unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("archive and ?action=rehydrate only"),
        "unexpected 400 body: {body}"
    );

    // 2. Archive — v4's raw result bag (`NextResponse.json(result)`).
    let resp = client.post(url("archive")).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "archive failed: {body}");
    assert_eq!(body["archived"], Value::Bool(true), "bag: {body}");
    assert!(body["archiveFileId"].is_string(), "bag: {body}");
    assert_eq!(body["pruneComplete"], Value::Bool(true), "bag: {body}");

    // 3. Rehydrate — the bundle restores and the tombstone clears.
    let resp = client.post(url("rehydrate")).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "rehydrate failed: {body}");
    assert_eq!(body["rehydrated"], Value::Bool(true), "bag: {body}");
    assert_eq!(body["archived"], Value::Bool(false), "bag: {body}");
    assert!(body["archiveBundleFileId"].is_string(), "bag: {body}");

    // 4. A missing character answers v4's route-level 404 — the route
    //    resolves the character BEFORE dispatching any action
    //    (`handlers/post.ts:153–157`, `notFound('Character')`), so the
    //    archive arm never sees a missing id (§3 review finding 1).
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/characters/00000000-0000-4000-8000-00000000dead?action=archive"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"].as_str().unwrap(),
        "Character not found",
        "unexpected 404 body: {body}"
    );
}
