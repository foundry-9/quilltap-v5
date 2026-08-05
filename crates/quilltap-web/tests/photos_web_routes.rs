//! P4.9a web-edge legs, end-to-end over a live server: the user photo gallery
//! REST surface (`GET|POST /api/v1/photos`, `GET|DELETE /api/v1/photos/{id}`).
//!
//! The exact BYTES of every arm are pinned by `photos_routes_equivalence`
//! against v4's real handlers; what this file proves is the web-edge PLUMBING
//! the differential cannot see:
//!
//! - the envelope is UNWRAPPED to v4's raw body (the P4.6ah lesson);
//! - `?tag=` REPEATS survive the extractor — v4 reads them with `getAll`, and a
//!   map-shaped query extractor would silently keep only one;
//! - `?limit=` goes through JS `Number()` BEFORE Zod, so a non-numeric value
//!   reaches the core as NaN and earns Zod's NaN message rather than being
//!   quietly dropped or repaired;
//! - POST answers **201** (v4 `created`), not 200;
//! - a search on an assembly with NO embedding seam is the loud named refusal,
//!   while a plain listing on that same assembly still works — the split that
//!   keeps `/photos` usable on a spine-less host.

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

/// The photos fixture family, laid out as an instance data dir. No user-id
/// rewrite is needed: the fixture already mints its rows under the engine's
/// `SINGLE_USER_ID`.
fn materialize_photos_instance() -> tempfile::TempDir {
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
    {
        let w = Writer::open_writable(&data.join("quilltap-llm-logs.db"), TEST_PEPPER).unwrap();
        w.connection().execute_batch(LLM_LOGS_DDL).unwrap();
    }
    base
}

#[tokio::test(flavor = "multi_thread")]
async fn photos_web_edges() {
    let base = materialize_photos_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = |p: &str| format!("http://{addr}{p}");

    // --- GET: the raw {entries,total,hasMore} body, envelope unwrapped ---
    let resp = client.get(url("/api/v1/photos")).send().await.unwrap();
    assert_eq!(resp.status(), 200, "photos list");
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("type").is_none(), "envelope must be unwrapped");
    assert_eq!(body["total"], 4, "four deduped photos/ images: {body}");
    assert_eq!(body["hasMore"], false);
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    // Newest primary first; the dedup collapse put sha A's vault link on top.
    assert_eq!(entries[0]["fileName"], "dawn-flight.webp");
    // The blobUrl is server-RELATIVE and points at the route that already
    // serves those bytes (no new byte route in this lane).
    assert!(
        entries[0]["blobUrl"]
            .as_str()
            .unwrap()
            .starts_with("/api/v1/mount-points/"),
        "blobUrl stays relative: {}",
        entries[0]["blobUrl"]
    );
    // The multi-link entry carries every hard link, photos/ and not.
    assert_eq!(entries[0]["linkSummary"]["count"], 5);

    // --- The `tag` repeat survives the extractor (v4 `getAll`) ---
    let body: Value = client
        .get(url("/api/v1/photos?tag=sky&tag=lantern"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["total"], 2,
        "both repeated tags must reach the filter: {body}"
    );

    // A single tag narrows to one; proves the repeat above wasn't a coincidence.
    let body: Value = client
        .get(url("/api/v1/photos?tag=sky"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["total"], 1, "one tag, one match: {body}");

    // --- Pagination ---
    let body: Value = client
        .get(url("/api/v1/photos?limit=2&offset=0"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["entries"].as_array().unwrap().len(), 2);
    assert_eq!(body["hasMore"], true);

    // --- `Number()` before Zod: a non-numeric limit is NaN, not "absent" ---
    let resp = client
        .get(url("/api/v1/photos?limit=abc"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        400,
        "NaN limit is a 400, not a silent default"
    );
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"].as_str().unwrap().contains("NaN"),
        "Zod's NaN message: {body}"
    );

    // --- A SEARCH with no embedding seam: the loud, named refusal ---
    let resp = client
        .get(url("/api/v1/photos?q=dawn"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 500, "search without the seam refuses");
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("memory embedding not assembled"),
        "the refusal names the seam: {body}"
    );

    // --- GET one entry, then DELETE it link-only ---
    let list: Value = client
        .get(url("/api/v1/photos"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let link_id = list["entries"][0]["linkId"].as_str().unwrap().to_string();

    let resp = client
        .get(url(&format!("/api/v1/photos/{link_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "entry get");
    let entry: Value = resp.json().await.unwrap();
    assert_eq!(entry["linkId"], link_id.as_str());
    assert_eq!(entry["caption"], "Dawn flight over the Lantern District");

    let resp = client
        .get(url("/api/v1/photos/00000000-0000-4000-8000-000000000bad"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "unknown entry is a 404");

    let resp = client
        .delete(url(&format!("/api/v1/photos/{link_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "delete");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["deleted"], true);
    // Four hard links survive the removal, so the bytes are NOT collected.
    assert_eq!(body["fileGC"], false);

    // The entry is gone but its sha still has other links, so the roll-up keeps
    // showing it — via a DIFFERENT primary link.
    let after: Value = client
        .get(url("/api/v1/photos"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["total"], 4, "the sha survives on its other links");
    assert_ne!(
        after["entries"][0]["linkId"].as_str().unwrap(),
        link_id,
        "the primary moved to a surviving link"
    );

    // --- POST answers 201 (v4 `created`), and rejects a bad body at 400 ---
    let resp = client
        .post(url("/api/v1/photos"))
        .json(&json!({ "fileId": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "empty fileId");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "fileId is required");

    // A save whose bytes the (unwired) store cannot read still proves the edge
    // reaches the core's save arm and carries its status out.
    let resp = client
        .post(url("/api/v1/photos"))
        .json(&json!({ "fileId": "00000000-0000-4000-8000-00000000ffff" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "unknown file id");
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"].as_str().unwrap().contains("Image not found"),
        "v4's message reaches the wire: {body}"
    );
}
