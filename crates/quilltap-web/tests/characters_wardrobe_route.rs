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
//! Run:
//!   cargo test -p quilltap-web --test characters_wardrobe_route

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
    // P4.D281: the served engine is the single user, and since `9ecebf986` a
    // character resolves through the wardrobe location's ownership gate (R-E)
    // — a real instance's characters all belong to v4's `SINGLE_USER_ID`, so
    // the materialized copy is made one (the committed pair is untouched).
    let main = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).unwrap();
    main.connection()
        .execute(
            "UPDATE characters SET userId = 'ffffffff-ffff-ffff-ffff-ffffffffffff'",
            [],
        )
        .unwrap();
    drop(main);
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

/// P4.D281 (v4 `4a38222e6`, R-F): the five folder actions on the REST edges
/// where their tier has one — the character collection (`GET ?action=folders`,
/// and the NEW `POST` registration carrying the factory's whole POST map) and
/// Quilltap General. Plumbing only: each URL resolves, carries its body into
/// the C2 verb, and answers v4's raw envelope; the bytes are
/// `wardrobe_folders_routes_equivalence`'s.
#[tokio::test(flavor = "multi_thread")]
async fn the_folder_actions_resolve_on_the_rest_edges() {
    let base = materialize_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let char_url = |qs: &str| format!("http://{addr}/api/v1/characters/{ARIA}/wardrobe{qs}");
    let general_url = |qs: &str| format!("http://{addr}/api/v1/wardrobe{qs}");
    let send = |req: reqwest::RequestBuilder| async move {
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap();
        (status, body)
    };

    // create-folder on the character edge, then it lists.
    let (status, body) = send(
        client
            .post(char_url("?action=create-folder"))
            .json(&serde_json::json!({ "path": "Coats" })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        serde_json::json!({ "folder": { "path": "Coats", "name": "Coats", "itemCount": 0 } })
    );
    let (status, body) = send(client.get(char_url("?action=folders"))).await;
    assert_eq!(status, 200, "{body}");
    assert!(
        body["folders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "Coats"),
        "{body}"
    );
    // rename + delete on the same edge.
    let (status, body) = send(
        client
            .post(char_url("?action=rename-folder"))
            .json(&serde_json::json!({ "path": "Coats", "name": "Outerwear" })),
    )
    .await;
    assert_eq!(
        (status, body["folder"]["path"].clone()),
        (200, "Outerwear".into())
    );
    let (status, body) = send(
        client
            .post(char_url("?action=delete-folder"))
            .json(&serde_json::json!({ "path": "Outerwear" })),
    )
    .await;
    assert_eq!(
        (status, body),
        (
            200,
            serde_json::json!({ "movedItems": 0, "movedFolders": 0 })
        )
    );
    // A Zod refusal crosses as v4's flat 400.
    let (status, body) = send(
        client
            .post(char_url("?action=move-items"))
            .json(&serde_json::json!({ "itemIds": [], "folder": "" })),
    )
    .await;
    assert_eq!(
        (status, body["error"].clone()),
        (400, "Validation error".into())
    );
    // The character collection's create rides the new POST too (201).
    let (status, body) = send(
        client
            .post(char_url(""))
            .json(&serde_json::json!({ "title": "Rain Hat", "types": ["accessories"] })),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["wardrobeItem"]["title"], "Rain Hat");
    assert_eq!(body["wardrobeItem"]["folder"], "");

    // General: the folder list (provisioned, empty) and a folder write.
    let (status, body) = send(client.get(general_url("?action=folders"))).await;
    assert_eq!((status, body), (200, serde_json::json!({ "folders": [] })));
    let (status, body) = send(
        client
            .post(general_url("?action=create-folder"))
            .json(&serde_json::json!({ "path": "Jewelry" })),
    )
    .await;
    assert_eq!(
        (status, body["folder"]["path"].clone()),
        (200, "Jewelry".into())
    );
}
