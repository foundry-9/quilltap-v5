//! Route-level integration for `POST /api/v1/characters?action=reset-builtins`
//! (the P4.6n/P4.6o/P4.4u4 unification wire): the edge route over the
//! differential-proven `quilltap_import::reset::reset_builtins` service. The
//! characters fixture ships without the built-in pair, so the first reset
//! imports them fresh; the second reset exercises the delete + id-preserving
//! remap round (v4 `handleResetBuiltins`).

mod common;

use serde_json::Value;

#[tokio::test(flavor = "multi_thread")]
async fn characters_reset_builtins_roundtrip() {
    let base = common::materialize_characters_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        // The route is the seeding surface under test — keep first boot bare.
        c.seed_sample_content = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/v1/characters?action=reset-builtins");

    // --- Round 1: no built-ins exist — a fresh import, nothing deleted. ---
    let resp = client.post(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200, "first reset");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["deletedCharacterIds"].as_array().unwrap().len(), 0);
    assert_eq!(body["preservedIds"]["Lorian"], Value::Null);
    assert_eq!(body["preservedIds"]["Riya"], Value::Null);
    let lorian_id = body["postResetIds"]["Lorian"]
        .as_str()
        .expect("Lorian id after first reset")
        .to_string();
    let riya_id = body["postResetIds"]["Riya"]
        .as_str()
        .expect("Riya id after first reset")
        .to_string();
    assert_eq!(body["remappedIdCount"], 0);
    let import = &body["importResult"];
    assert_eq!(import["success"], true);
    assert_eq!(import["imported"]["characters"], 2);
    assert_eq!(import["imported"]["memories"], 42);
    // v4's full QuilltapExportCounts key set rides along (unported kinds stay 0).
    assert_eq!(import["imported"]["chats"], 0);
    assert_eq!(import["skipped"]["characters"], 0);

    // --- Round 2: both exist — cascade delete, then re-import. The seed→
    // preserved id remap is computed (remappedIdCount 2) but v4's `create`
    // mints fresh ids regardless, so the post-reset ids DIFFER from the
    // preserved ones — the ported-faithfully quirk the `reset_builtins`
    // differential pins (`postResetDiffersFromPreserved: true` in the oracle). ---
    let resp = client.post(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200, "second reset");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);
    let deleted: Vec<&str> = body["deletedCharacterIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(deleted.len(), 2);
    assert!(deleted.contains(&lorian_id.as_str()));
    assert!(deleted.contains(&riya_id.as_str()));
    assert_eq!(body["preservedIds"]["Lorian"], lorian_id.as_str());
    assert_eq!(body["preservedIds"]["Riya"], riya_id.as_str());
    let post_lorian = body["postResetIds"]["Lorian"].as_str().expect("post id");
    let post_riya = body["postResetIds"]["Riya"].as_str().expect("post id");
    assert_ne!(post_lorian, lorian_id, "v4 create re-mints across a reset");
    assert_ne!(post_riya, riya_id, "v4 create re-mints across a reset");
    assert_eq!(body["remappedIdCount"], 2);
    assert_eq!(body["importResult"]["imported"]["characters"], 2);
}
