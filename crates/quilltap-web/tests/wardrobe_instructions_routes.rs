//! P4.D119 wire: the `?action=instructions` REST edges (v4 `b86bb1a5`).
//!
//! `wardrobe_instructions_routes_equivalence` proves the CORE handlers against
//! v4's real route handlers; nothing there proves the URLs resolve on v5's own
//! server. That is the P4.D65 cross-lane blind spot, so it gets that lane's
//! remedy: a web-edge test on the `characters_wardrobe_route` pattern.
//!
//! **Scope, recorded per surface.** v4 puts the pair on all four collection
//! routes; v5 registers a REST edge only where a consumer already speaks the
//! URL:
//!   * `GET /api/v1/wardrobe?action=instructions` — extended here.
//!   * `POST /api/v1/wardrobe?action=instructions` — extended here.
//!   * `GET /api/v1/characters/{id}/wardrobe?action=instructions` — extended
//!     here (the edge already existed for `?scope=`).
//!   * the character POST has NO edge at all (v5 never registered `.post` on
//!     that path — adding one means porting the whole item-create route), and
//!     the group / project collection routes have no edges either. All three
//!     ride `POST /api/dispatch` as their named verbs (the P4.D112/P4.D115
//!     dispatch-only precedent).
//!
//! Fixture: the committed `wardrobe-routes-{main,mount}.db` family (Aria has a
//! vault; the instance has a provisioned Quilltap General).
//!
//! Run:
//!   cargo test -p quilltap-web --test wardrobe_instructions_routes

mod common;

use quilltap_core::db::Writer;
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

const LLM_LOGS_DDL: &str = "CREATE TABLE llm_logs (\
    id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
    chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
    modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
    request TEXT, response TEXT, usage TEXT, \
    cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
    durationMs REAL, createdAt TEXT, updatedAt TEXT);";

/// Aria — pinned in `wardrobe-routes.json#ids`; has a linked vault.
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";

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

#[tokio::test(flavor = "multi_thread")]
async fn the_instructions_action_resolves_on_the_registered_edges() {
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
    let general = format!("http://{addr}/api/v1/wardrobe?action=instructions");
    let character = format!("http://{addr}/api/v1/characters/{ARIA}/wardrobe?action=instructions");

    // --- Quilltap General: absent → null, then a round trip ----------------
    let body: Value = client
        .get(&general)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body, json!({ "instructions": null }), "General GET, absent");

    let resp = client
        .post(&general)
        .json(&json!({ "instructions": "  The house dresses for the weather.  " }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "General POST failed: {body}");
    assert_eq!(
        body,
        json!({ "instructions": "The house dresses for the weather." }),
        "the echo is the TRIMMED string"
    );

    let body: Value = client
        .get(&general)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        body,
        json!({ "instructions": "The house dresses for the weather." }),
        "the write must be readable back over the same edge"
    );

    // A missing `instructions` key is v4's flat `Validation error` 400 — the
    // arm that only survives because the edge decodes THROUGH the Request enum.
    let resp = client.post(&general).json(&json!({})).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 400, "an absent key must refuse: {body}");
    assert_eq!(body["error"], "Validation error");

    // `null` clears.
    let resp = client
        .post(&general)
        .json(&json!({ "instructions": Value::Null }))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body, json!({ "instructions": null }), "clear by null");
    let body: Value = client
        .get(&general)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        body,
        json!({ "instructions": null }),
        "cleared reads back null"
    );

    // --- the character GET edge --------------------------------------------
    let resp = client.get(&character).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "character GET failed: {body}");
    assert_eq!(body, json!({ "instructions": null }));

    // …and the default arm on the SAME path still lists items, so the action
    // dispatch did not swallow the collection read.
    let body: Value = client
        .get(format!("http://{addr}/api/v1/characters/{ARIA}/wardrobe"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        body["wardrobeItems"].is_array(),
        "the no-action arm must still be the listing: {body}"
    );

    // The same holds for the General collection GET.
    let body: Value = client
        .get(format!("http://{addr}/api/v1/wardrobe"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        body["wardrobeItems"].is_array(),
        "the no-action arm must still be the archetype listing: {body}"
    );
}
