//! P4.9c web-edge legs, end-to-end over a live server: the user-profile REST
//! surface (`GET|PUT|PATCH /api/v1/user/profile`), the data-directory pair
//! (`GET|POST /api/v1/system/data-dir`), and the additive health `version`
//! field.
//!
//! The exact BYTES of every profile arm are pinned by
//! `profile_routes_equivalence` against v4's real handlers; what this file
//! proves is the web-edge PLUMBING the differential cannot see:
//!
//! - the envelope is UNWRAPPED to v4's raw body (the P4.6ah lesson);
//! - the `?action=` multiplexing v4's route file owns — including the literal
//!   `null` an ABSENT action interpolates into the message;
//! - that the PUT edge preserves the absent / explicit-null distinction all the
//!   way to the Zod-faithful parse (a naive edge would collapse null to absent
//!   and turn v4's 400 into a silent no-op);
//! - the refusal on `?action=open`;
//! - the health `version` carry, which is what lets the SPA display a version
//!   at all.
//!
//! Run:
//!   cargo test -p quilltap-web --test profile_web_routes

mod common;

use serde_json::{json, Value};

#[tokio::test(flavor = "multi_thread")]
async fn profile_web_edges() {
    let base = common::materialize_profile_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = |p: &str| format!("http://{addr}{p}");

    // --- GET: the raw {profile} body, envelope unwrapped ---
    let resp = client
        .get(url("/api/v1/user/profile"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "profile get");
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("type").is_none(), "envelope must be unwrapped");
    assert_eq!(body["profile"]["username"], "friday");
    assert_eq!(body["profile"]["name"], "Friday");
    // The read arm OMITS a NULL column rather than emitting null.
    assert!(
        body["profile"].get("image").is_none(),
        "a read omits the NULL image key: {body}"
    );

    // --- PUT: a happy update round-trips and persists ---
    let resp = client
        .put(url("/api/v1/user/profile"))
        .json(&json!({ "name": "Reginald Jeeves" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "profile put");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["profile"]["name"], "Reginald Jeeves");

    let body: Value = client
        .get(url("/api/v1/user/profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["profile"]["name"], "Reginald Jeeves", "persisted");

    // --- PUT: the explicit-null 400. THIS is the edge claim — a naive edge
    //     that collapsed null into "absent" would answer 200 and quietly do
    //     nothing, which is not what v4 does. ---
    let resp = client
        .put(url("/api/v1/user/profile"))
        .json(&json!({ "name": null, "email": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "explicit nulls are a validation error");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Validation error");

    // --- PUT: the email-uniqueness rejection (the fixture's second user) ---
    let resp = client
        .put(url("/api/v1/user/profile"))
        .json(&json!({ "email": "amy@foundry-9.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "email clash");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Email already in use");

    // --- PATCH: the action gate, v4's message verbatim ---
    let resp = client
        .patch(url("/api/v1/user/profile"))
        .json(&json!({ "imageId": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "absent action");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"], "Unknown action: null. Available actions: set-avatar",
        "an absent action interpolates as the literal null, as in v4"
    );

    let resp = client
        .patch(url("/api/v1/user/profile?action=bogus"))
        .json(&json!({ "imageId": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "unknown action");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        "Unknown action: bogus. Available actions: set-avatar"
    );

    // --- PATCH set-avatar: the stored pointer shape ---
    let image_id = "9f000000-0000-4000-8000-000000000001";
    let resp = client
        .patch(url("/api/v1/user/profile?action=set-avatar"))
        .json(&json!({ "imageId": image_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "set avatar");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["profile"]["image"],
        format!("/api/v1/files/{image_id}"),
        "the stored value is the file API pointer, not the bare id"
    );

    // The DOCUMENT-category rejection quotes the category back.
    let resp = client
        .patch(url("/api/v1/user/profile?action=set-avatar"))
        .json(&json!({ "imageId": "9f000000-0000-4000-8000-000000000003" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "wrong category");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        "Invalid file category. Expected IMAGE or AVATAR, got DOCUMENT"
    );

    // --- the data directory ---
    let resp = client
        .get(url("/api/v1/system/data-dir"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "data-dir get");
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("type").is_none(), "envelope must be unwrapped");
    // The instance was booted from a temp dir via the explicit base dir, so the
    // reported path is the one actually in use — the divergence documented in
    // `api::data_dir`.
    assert_eq!(
        body["path"].as_str().unwrap(),
        base.path().to_string_lossy(),
        "the payload reports the base dir the engine is really serving"
    );
    assert_eq!(body["source"], "environment");
    assert!(body["sourceDescription"]
        .as_str()
        .unwrap()
        .starts_with("--data-dir command-line flag ("));
    assert!(body["shellCapabilities"].is_array());
    assert!(body["canOpen"].is_boolean());

    // The refusal, loud and by name.
    let resp = client
        .post(url("/api/v1/system/data-dir?action=open"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 500, "the open action refuses");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        "The 'open' data-directory action is recognized but not yet available."
    );

    // --- the health version carry (§3) ---
    let resp = client.get(url("/health")).send().await.unwrap();
    assert_eq!(resp.status(), 200, "health");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "healthy");
    let version = body["version"].as_str().expect("health carries a version");
    // The value is whatever the COMPOSING binary put in `HostConfig.version`:
    // the real `quilltap-web` binary overrides it with its own
    // `CARGO_PKG_VERSION` (`main.rs:78`), the Tauri shell with its own, and
    // this harness leaves `HostConfig::new`'s default (quilltap-host's). So the
    // assertion is on the SHAPE and on it being real, not on one crate's
    // number — which is also exactly what the About screen means by "the
    // server version": the version of the build that is serving you.
    assert!(
        !version.is_empty(),
        "the version must be a real version, not a placeholder"
    );
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(parts.len(), 3, "a semver-shaped version, got {version:?}");
    assert!(
        parts.iter().all(|p| p.chars().any(|c| c.is_ascii_digit())),
        "each version segment carries digits, got {version:?}"
    );
}
