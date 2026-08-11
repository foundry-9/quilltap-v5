//! P4.D65 — the change-passphrase archive sweep, at the wire.
//!
//! `archive_reencrypt_tier2_equivalence` proves the sweep's BEHAVIOR against
//! v4's real `reencryptArchiveBundles`. What no differential can prove is that
//! anything CALLS it: `reencrypt.rs` sat on main for a whole round with no
//! caller, and a summary that never leaves the engine is indistinguishable from
//! one that is never computed. So this test drives v4's own URL — `POST
//! /api/v1/system/unlock?action=change-passphrase` — over a real server and
//! asserts the `archives` bag reaches the HTTP body.
//!
//! The sweep is given real work: the test archives a character over v4's own
//! action URL first, so the change-passphrase call meets a bundle the archive
//! service actually wrote and has to report `total: 1, reencrypted: 1`. A
//! summary that was hardcoded, or computed over an empty library, cannot say
//! that — and the rehydrate afterwards proves the re-encryption preserved the
//! bytes rather than merely rewriting them, since the bundle now opens only
//! under the NEW passphrase and rehydration goes through the engine's cache.
//!
//! Run: `cargo test -p quilltap-web --test change_passphrase_archive_sweep`

mod common;

use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[tokio::test(flavor = "multi_thread")]
async fn change_passphrase_answers_the_archive_sweep_summary() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("quilltap=warn")
        .try_init();

    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(
        fixtures.join("character-archive-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("character-archive-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    // The instance starts with no user passphrase, so the `.dbkey` wraps the
    // pepper under the internal sentinel — the empty-string leg of the change.
    quilltap_core::dbkey::save_dbkey(&data, TEST_PEPPER, "").expect("write .dbkey");

    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // 1. Archive Sable, so the library the sweep walks holds a REAL bundle,
    //    sealed under the internal sentinel this instance currently uses.
    let sable = "a1000000-0000-4000-8000-0000000000a1";
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/characters/{sable}?action=archive"
        ))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "archive failed: {body}");
    assert_eq!(body["archived"], Value::Bool(true), "bag: {body}");

    // 2. Change the passphrase. The `.dbkey` re-wrap is phase one; the archive
    //    sweep is phase two, and its summary must reach this body.
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/system/unlock?action=change-passphrase"
        ))
        .json(&serde_json::json!({"oldPassphrase": "", "newPassphrase": "a brass key"}))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "change-passphrase failed: {body}");
    assert_eq!(body["success"], Value::Bool(true), "body: {body}");

    let archives = &body["archives"];
    assert!(
        archives.is_object(),
        "the archive sweep summary never reached the wire: {body}"
    );
    assert_eq!(
        archives["total"],
        Value::from(1),
        "the sweep did not see the bundle: {body}"
    );
    assert_eq!(
        archives["reencrypted"],
        Value::from(1),
        "the bundle was not rewritten: {body}"
    );
    assert_eq!(archives["failures"], Value::Array(vec![]), "body: {body}");

    // 3. The passphrase change itself really happened.
    assert!(quilltap_core::dbkey::load_pepper(&data, Some("a brass key")).is_ok());
    assert!(quilltap_core::dbkey::load_pepper(&data, Some("")).is_err());

    // 4. And the re-encrypted bundle is still a bundle: rehydrate reads it back
    //    through the engine's now-updated passphrase cache. Without phase two
    //    this call fails — the bundle would still want the old passphrase.
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/characters/{sable}?action=rehydrate"
        ))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "rehydrate after re-encryption failed: {body}");
    assert_eq!(body["rehydrated"], Value::Bool(true), "bag: {body}");
}
