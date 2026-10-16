//! P4.D309 (v4 `bbbcd2388`): `?action=copy-items` on the two wardrobe REST
//! edges — Quilltap General (`/api/v1/wardrobe`) and a character's vault
//! (`/api/v1/characters/{id}/wardrobe`). Plumbing only — the bytes are
//! `wardrobe_folders_routes_equivalence`'s — but the plumbing here is the
//! hazard: `folder_write_request` once routed every folder action it did not
//! name to the MOVE verb, so `copy-items` in the action list without its own
//! arm would have filed every copy as a move. This proves the copy COPIES (the
//! original stays, with its id and folder), the move does not copy, and the
//! copy's `components: 'move'` is the schema's flat 400.
//!
//! Fixture: the committed `wardrobe-routes-{main,mount}.db` family
//! (`characters_wardrobe_route.rs`'s recipe — Aria owns a vault).
//!
//! Run:
//!   cargo test -p quilltap-web --test wardrobe_copy_items_route

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

/// Aria — pinned in `wardrobe-routes.json#ids`.
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
    // The served engine is the single user (the R-E ownership gate).
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

fn items(body: &Value) -> Vec<(String, String, String)> {
    body["wardrobeItems"]
        .as_array()
        .unwrap_or_else(|| panic!("wardrobeItems array: {body}"))
        .iter()
        .map(|i| {
            (
                i["id"].as_str().unwrap_or_default().to_string(),
                i["title"].as_str().unwrap_or_default().to_string(),
                i["folder"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn copy_items_copies_and_move_items_moves() {
    let base = materialize_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let general = |qs: &str| format!("http://{addr}/api/v1/wardrobe{qs}");
    let character = |qs: &str| format!("http://{addr}/api/v1/characters/{ARIA}/wardrobe{qs}");
    let send = |req: reqwest::RequestBuilder| async move {
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap();
        (status, body)
    };

    // A General archetype to file.
    let (status, made) = send(
        client
            .post(general(""))
            .json(&json!({ "title": "Opera Cloak", "types": ["top"] })),
    )
    .await;
    assert_eq!(status, 201, "{made}");
    let original = made["wardrobeItem"]["id"].as_str().unwrap().to_string();
    let (_, listed) = send(client.get(general(""))).await;
    let before = items(&listed);

    // copy-items → 200 (not 201), a fresh id with a `(copy)` title in the
    // folder; the original still where it was.
    let (status, body) = send(
        client
            .post(general("?action=copy-items"))
            .json(&json!({ "itemIds": [original], "folder": "Spares" })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["componentsCarried"], 0, "{body}");
    let copy = &body["items"][0];
    assert_ne!(copy["id"].as_str().unwrap(), original, "{body}");
    assert_eq!(copy["title"], "Opera Cloak (copy)");
    assert_eq!(copy["folder"], "Spares");
    let (_, listed) = send(client.get(general(""))).await;
    let after = items(&listed);
    assert_eq!(after.len(), before.len() + 1, "{listed}");
    assert!(
        after.contains(&(original.clone(), "Opera Cloak".into(), String::new())),
        "the original moved or vanished: {after:?}"
    );

    // move-items → the original MOVES; nothing is copied.
    let (status, body) = send(
        client
            .post(general("?action=move-items"))
            .json(&json!({ "itemIds": [original], "folder": "Spares" })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["items"][0]["id"].as_str(), Some(original.as_str()));
    assert_eq!(body["componentsCarried"], 0);
    let (_, listed) = send(client.get(general(""))).await;
    let moved = items(&listed);
    assert_eq!(moved.len(), after.len(), "a move must not copy: {moved:?}");
    assert!(moved.contains(&(original.clone(), "Opera Cloak".into(), "Spares".into())));

    // A copy cannot move the components — the schema's flat 400.
    let (status, body) = send(
        client
            .post(general("?action=copy-items"))
            .json(&json!({ "itemIds": [original], "folder": "", "components": "move" })),
    )
    .await;
    assert_eq!(
        (status, body["error"].clone()),
        (400, "Validation error".into())
    );

    // The character edge carries the verb too.
    let (_, vault) = send(client.get(character(""))).await;
    let (blouse, _, _) = items(&vault)
        .into_iter()
        .find(|(_, t, _)| t == "Brass-button blouse")
        .expect("Aria's blouse");
    let (status, body) = send(
        client
            .post(character("?action=copy-items"))
            .json(&json!({ "itemIds": [blouse], "folder": "" })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["items"][0]["title"], "Brass-button blouse (copy)");
    let (_, vault_after) = send(client.get(character(""))).await;
    assert_eq!(items(&vault_after).len(), items(&vault).len() + 1);
}
