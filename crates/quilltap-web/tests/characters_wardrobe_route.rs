//! P4.D71 wire: `GET /api/v1/characters/{id}/wardrobe[?scope=group]`.
//!
//! v5's SPA reads the character wardrobe over `/api/dispatch`
//! (`characterWardrobeList`), so no lane had ever shipped this REST edge — but
//! v4 documents the path in `docs/developer/API.md` and `8600c83f` added the
//! `?scope=group` arm to it specifically. The `wardrobe_routes_equivalence`
//! differential proves the CORE handler against v4's real route; nothing proved
//! the URL resolves on v5's own server. That is the P4.D65 cross-lane blind
//! spot wearing a different hat, so it gets the same treatment: a web-edge test
//! on the `characters_action_route` pattern, proving the PLUMBING — the route
//! exists, carries the query string into the dispatch verb, and answers v4's
//! `{wardrobeItems}` envelope.
//!
//! Fixture: the committed `wardrobe-routes-{main,mount}.db` family, which
//! carries Aria (a member of "The Aeronauts", whose group store's `Wardrobe/`
//! holds the Aeronaut livery) and Bramwell (not a member).
//!
//! Run: `cargo test -p quilltap-web --test characters_wardrobe_route`

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

/// Aria — the group member. Pinned in `wardrobe-routes.json#ids`.
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
/// Bramwell — belongs to no group.
const BRAM: &str = "a1000000-0000-4000-8000-000000000002";

fn materialize_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(
        fixtures.join("wardrobe-routes-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("wardrobe-routes-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    let w = Writer::open_writable(&data.join("quilltap-llm-logs.db"), TEST_PEPPER).unwrap();
    w.connection().execute_batch(LLM_LOGS_DDL).unwrap();
    drop(w);
    base
}

fn titles(body: &Value) -> Vec<String> {
    body["wardrobeItems"]
        .as_array()
        .expect("wardrobeItems array")
        .iter()
        .map(|i| i["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_character_wardrobe_url_serves_the_vault_and_the_group_tier() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("quilltap=warn")
        .try_init();
    let base = materialize_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = |cid: &str, qs: &str| format!("http://{addr}/api/v1/characters/{cid}/wardrobe{qs}");

    // 1. No scope → the character's own vault items (v4's default arm).
    let resp = client.get(url(ARIA, "")).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "vault read failed: {body}");
    let own = titles(&body);
    assert!(
        own.contains(&"Brass-button blouse".to_string()),
        "vault items: {own:?}"
    );
    assert!(
        !own.contains(&"Aeronaut livery".to_string()),
        "the group livery must NOT be in the vault read: {own:?}"
    );

    // 2. `?scope=group` → the group tier ALONE (a standalone read for the
    //    client-side merge; it does not fold in the vault).
    let resp = client.get(url(ARIA, "?scope=group")).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "group read failed: {body}");
    assert_eq!(titles(&body), vec!["Aeronaut livery".to_string()]);
    assert!(
        body["wardrobeItems"][0]["characterId"].is_null(),
        "a shared item is owned by no character: {body}"
    );

    // 3. A non-member gets nothing — group stores follow the CHARACTER, never
    //    a co-participant.
    let resp = client.get(url(BRAM, "?scope=group")).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "non-member group read failed: {body}");
    assert!(
        titles(&body).is_empty(),
        "a non-member must see no group items: {body}"
    );

    // 4. Any other scope falls through to the vault read (v4 checks
    //    `scope === 'group'`, not "a scope was given").
    let resp = client
        .get(url(ARIA, "?scope=project"))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(titles(&body), own, "an unknown scope is the default read");

    // 5. A missing character is v4's route-level 404.
    let resp = client
        .get(url("00000000-0000-4000-8000-00000000dead", "?scope=group"))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 404, "unexpected status for a missing character");
    assert_eq!(body["error"], Value::String("Character not found".into()));
}
