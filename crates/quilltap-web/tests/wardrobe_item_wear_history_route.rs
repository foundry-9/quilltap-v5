//! P4.D256 wire: `GET /api/v1/wardrobe/{itemId}[?action=wear-history]` (R-E —
//! the ONE wardrobe item-GET REST edge `quilltap-web` serves).
//!
//! `wardrobe_routes_equivalence` proves the CORE verb against v4's real route;
//! this proves the EDGE's three-way action rule on v5's own server (the P4.D65
//! cross-lane blind spot — and the `unwrap_to_http` fan-out trap: a core
//! response the edge does not list answers 500 `Unexpected core response`):
//!
//! - no action → the archetype item, tagged with the General `origin`;
//! - `?action=wear-history` → `{ history, wearers, lastWornChat }`;
//! - an unknown action → v4's `Unknown action` 400 envelope — but on a
//!   MISSING item the 404 wins (v4 runs the item lookup BEFORE
//!   `dispatchAction`), and so does it for `wear-history`.
//!
//! The host boots the committed `wardrobe-routes` pair, so its boot ensures
//! create `wardrobe_wear_stats` (a never-worn read is the honest one here).
//!
//! Run:
//!   cargo test -p quilltap-web --test wardrobe_item_wear_history_route

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

/// The General greatcoat (`wardrobe-routes.json#ids.gCoat`).
const G_COAT: &str = "e2000000-0000-4000-8000-000000000002";
/// Aria's own blouse — a CHARACTER item, so not in the General tier.
const W_TOP: &str = "e1000000-0000-4000-8000-000000000001";
const UNKNOWN: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";

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

#[tokio::test(flavor = "multi_thread")]
async fn the_item_get_edge_serves_the_item_the_wear_history_and_v4s_refusals() {
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
    let get = |item: &str, qs: &str| {
        let url = format!("http://{addr}/api/v1/wardrobe/{item}{qs}");
        let client = client.clone();
        async move {
            let resp = client.get(url).send().await.unwrap();
            let status = resp.status().as_u16();
            let body: Value = resp.json().await.unwrap();
            (status, body)
        }
    };

    // 1. No action → the item, tagged and — since `9ecebf986`'s factory
    // (`serializeOne`) — worn: `origin` then `wear`, last.
    let (status, body) = get(G_COAT, "").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["wardrobeItem"]["id"], G_COAT);
    assert_eq!(
        body["wardrobeItem"]["origin"],
        json!({ "scope": "general", "id": null, "name": "Quilltap General" })
    );
    assert_eq!(
        body["wardrobeItem"]["wear"],
        json!({ "wearCount": 0, "firstWornAt": null, "lastWornAt": null, "lastWornChatId": null })
    );
    let keys: Vec<&String> = body["wardrobeItem"].as_object().unwrap().keys().collect();
    assert_eq!(keys[keys.len() - 2..], ["origin", "wear"], "{body}");

    // 2. `?action=wear-history` → the payload (a never-worn item here).
    let (status, body) = get(G_COAT, "?action=wear-history").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({
            "history": { "wearCount": 0, "firstWornAt": null, "lastWornAt": null,
                         "lastWornChatId": null, "wearers": [] },
            "wearers": [],
            "lastWornChat": null
        })
    );
    // …whose 404 runs first: a character item is not a General archetype.
    let (status, body) = get(W_TOP, "?action=wear-history").await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["error"], "Archetype wardrobe item not found");

    // 3. An unknown action on a PRESENT item → v4's three-way refusal …
    let (status, body) = get(G_COAT, "?action=bogus").await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(
        body,
        json!({ "error": "Unknown action: bogus", "availableActions": ["wear-history"] })
    );
    // … a bare `?action=` too …
    let (status, body) = get(G_COAT, "?action=").await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "Unknown action: ");
    // … and on a MISSING item too: since `9ecebf986` the route factory's
    // `withActionDispatch` runs BEFORE the handler loads anything (v4's
    // factory test "refuses an unknown action rather than serving the item"),
    // so the pre-factory 404-first rule is retired by measurement.
    let (status, body) = get(UNKNOWN, "?action=bogus").await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(
        body,
        json!({ "error": "Unknown action: bogus", "availableActions": ["wear-history"] })
    );
}

/// The `f5e953a3f` unification's §3 finding, re-pinned at `9ecebf986`: the
/// pre-factory route read the item BEFORE `dispatchAction`, so a failed read
/// answered its error, never `Unknown action`. The route factory dispatches
/// the action FIRST — before any read — so on a DEGRADED mount index (a
/// garbage file, P4.159) the bare GET fails with the read's error while an
/// unknown action is still v4's 400 envelope (nothing was read to fail).
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_item_read_is_not_masked_as_an_unknown_action() {
    let base = materialize_instance();
    std::fs::write(
        base.path().join("data").join("quilltap-mount-index.db"),
        b"not a database at all, just bytes",
    )
    .unwrap();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let get = |qs: &str| {
        let url = format!("http://{addr}/api/v1/wardrobe/{G_COAT}{qs}");
        let client = client.clone();
        async move {
            let resp = client.get(url).send().await.unwrap();
            let status = resp.status().as_u16();
            let body: Value = resp.json().await.unwrap();
            (status, body)
        }
    };
    let bare = get("").await;
    assert!(
        bare.0 >= 500,
        "the bare GET fails on a degraded mount index: {bare:?}"
    );
    let bogus = get("?action=bogus").await;
    assert_eq!(
        bogus,
        (
            400,
            json!({ "error": "Unknown action: bogus", "availableActions": ["wear-history"] })
        ),
        "the action dispatch runs before the item read"
    );
}
