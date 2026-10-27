//! P4.D335 (v4 `2b5e8ee24`) — the two interrupt actions over the REAL HTTP
//! edge: `POST /api/v1/chats/{id}?action=stop-turn` and `?action=raise-hand`.
//!
//! `salon_skip_equivalence` drives the HANDLERS (`api::chat_interrupt::*`)
//! against v4's REAL route; this family drives the ROUTE — the action list,
//! the body read (`req.json().catch(() => ({}))`: an unparsable body is `{}`,
//! a body that parses to a non-object is refused by the Zod parse), the
//! envelope, `unwrap_to_http`'s fan-out, the `details`-bearing 400 — and
//! compares status + body BYTES against the very same oracle rows
//! (`QT_ORACLE_SALON_SKIP`, the salon-skip case regenerated at the pin), incl.
//! the three REST-only rows a dispatch `Request` cannot express
//! (`raise_unparsable_body`, `raise_array_body`, `raise_string_body`).
//!
//! The fixture is the committed SALON pair (`Group Expedition` — two LLM seats
//! and a user seat). No stream is open in the served process, so every stop
//! answers `stopped: 0` and every raise `active: false` — exactly the oracle's
//! state.
//!
//! Generate the oracle: `salon_skip_equivalence`'s recipe. Run:
//!   QT_ORACLE_SALON_SKIP=/tmp/oracle-salon-skip.ndjson \
//!     cargo test -p quilltap-web --test chat_interrupt_rest_routes

mod common;

use std::collections::HashMap;

use serde_json::Value;

const GROUP: &str = "c1000000-0000-4000-8000-000000000002";
const UNKNOWN: &str = "c1000000-0000-4000-8000-0000000000ff";

/// `(oracle row, chat id, action, raw body or None)` — the raw bytes the
/// oracle's mocked `req.json()` stood for (`raise_unparsable_body`'s mock
/// REJECTS, i.e. bytes that are not JSON).
const CASES: &[(&str, &str, &str, Option<&str>)] = &[
    ("stop_no_stream_group", GROUP, "stop-turn", None),
    ("stop_unknown_chat", UNKNOWN, "stop-turn", None),
    ("raise_absent_body", GROUP, "raise-hand", Some("{}")),
    (
        "raise_true",
        GROUP,
        "raise-hand",
        Some(r#"{"raised":true}"#),
    ),
    (
        "raise_false",
        GROUP,
        "raise-hand",
        Some(r#"{"raised":false}"#),
    ),
    (
        "raise_null",
        GROUP,
        "raise-hand",
        Some(r#"{"raised":null}"#),
    ),
    (
        "raise_string",
        GROUP,
        "raise-hand",
        Some(r#"{"raised":"true"}"#),
    ),
    ("raise_number", GROUP, "raise-hand", Some(r#"{"raised":1}"#)),
    (
        "raise_object",
        GROUP,
        "raise-hand",
        Some(r#"{"raised":{}}"#),
    ),
    (
        "raise_extra_keys",
        GROUP,
        "raise-hand",
        Some(r#"{"raised":true,"extra":1}"#),
    ),
    (
        "raise_unparsable_body",
        GROUP,
        "raise-hand",
        Some("not json at all"),
    ),
    ("raise_array_body", GROUP, "raise-hand", Some("[]")),
    ("raise_string_body", GROUP, "raise-hand", Some(r#""yes""#)),
    (
        "raise_unknown_chat",
        UNKNOWN,
        "raise-hand",
        Some(r#"{"raised":null}"#),
    ),
    // P4.170 (ruling P-12): the "body before load" class. v4's `handlePost`
    // finds the chat BEFORE any action reads its body, so a missing chat is
    // the 404 whatever the body — every POST action whose REST edge refuses a
    // body itself. The `unparsable` rows' raw bytes stand for the oracle's
    // rejecting `req.json()`; on the real chat they are each action's 500
    // with its ERROR line (F.7 — pinned in `wardrobe_routes`' `p4170_f7_tests`).
    (
        "raise_array_body_unknown_chat",
        UNKNOWN,
        "raise-hand",
        Some("[]"),
    ),
    ("equip_unparsable_body", GROUP, "equip", Some("{not json")),
    (
        "equip_unparsable_unknown_chat",
        UNKNOWN,
        "equip",
        Some("{not json"),
    ),
    (
        "rollback_unparsable_body",
        GROUP,
        "wardrobe-rollback",
        Some("{not json"),
    ),
    (
        "rollback_unparsable_unknown_chat",
        UNKNOWN,
        "wardrobe-rollback",
        Some("{not json"),
    ),
    (
        "inventory_equip_unparsable_unknown_chat",
        UNKNOWN,
        "inventory-equip",
        Some("{not json"),
    ),
    (
        "regenerate_avatar_unparsable_unknown_chat",
        UNKNOWN,
        "regenerate-avatar",
        Some("{not json"),
    ),
    (
        "inform_array_body_unknown_chat",
        UNKNOWN,
        "inform",
        Some("[]"),
    ),
    (
        "cancel_inform_array_body_unknown_chat",
        UNKNOWN,
        "cancel-inform",
        Some("[]"),
    ),
];

#[tokio::test(flavor = "multi_thread")]
async fn the_interrupt_actions_answer_v4s_bytes_over_rest() {
    let Ok(path) = std::env::var("QT_ORACLE_SALON_SKIP") else {
        eprintln!("SKIP: set QT_ORACLE_SALON_SKIP (see the header).");
        return;
    };
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&path)
        .expect("read the oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("an oracle row");
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();

    let base = common::materialize_salon_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    let mut failures = Vec::new();
    for (case, chat, action, body) in CASES {
        let want = oracle
            .get(*case)
            .unwrap_or_else(|| panic!("{case}: no oracle row — regenerate"));
        let mut req = client.post(format!("http://{addr}/api/v1/chats/{chat}?action={action}"));
        if let Some(b) = body {
            req = req
                .header("content-type", "application/json")
                .body((*b).to_string());
        }
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let got = resp.text().await.unwrap();
        if u64::from(status) != want["status"].as_u64().unwrap() {
            failures.push(format!("{case}: status {status} vs v4 {}", want["status"]));
        }
        let want_body = serde_json::to_string(&want["body"]).unwrap();
        if got != want_body {
            failures.push(format!("{case}: body\n  v5 {got}\n  v4 {want_body}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} REST divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// P4.170 C.10 (ruling P-12): `PUT /api/v1/wardrobe/{itemId}` with an
/// unparsable body on a MISSING id — v4's factory runs `findItem` before
/// `updateWardrobeSchema.parse(await req.json())`, so the answer is the
/// item's 404 (`wardrobe-routes`'s `update_missing_bad_body` row, regenerated
/// at the pin), not the parse's 500. The item is missing from the salon
/// instance as it is from the wardrobe-routes pair.
#[tokio::test(flavor = "multi_thread")]
async fn the_general_item_put_finds_the_item_before_the_body() {
    let Ok(path) = std::env::var("QT_ORACLE_WARDROBE_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_WARDROBE_ROUTES (wardrobe_routes_equivalence's recipe).");
        return;
    };
    let want: Value = std::fs::read_to_string(&path)
        .expect("read the oracle")
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["name"] == "update_missing_bad_body")
        .expect("update_missing_bad_body: no oracle row — regenerate");
    // The wardrobe-routes pair itself (a provisioned Quilltap General store —
    // the salon pair has none, so the probe there answers the store's 500).
    let base = tempfile::tempdir().unwrap();
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for (from, to) in [
        ("wardrobe-routes-main.db", "quilltap.db"),
        ("wardrobe-routes-mount.db", "quilltap-mount-index.db"),
    ] {
        std::fs::copy(common::fixtures_dir().join(from), data.join(to)).unwrap();
    }
    {
        let w = quilltap_core::db::Writer::open_writable(
            &data.join("quilltap.db"),
            common::TEST_PEPPER,
        )
        .unwrap();
        common::rewrite_fixture_user_ids(w.connection());
    }
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let resp = reqwest::Client::new()
        .put(format!(
            "http://{addr}/api/v1/wardrobe/eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"
        ))
        .header("content-type", "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert_eq!(
        u64::from(resp.status().as_u16()),
        want["status"].as_u64().unwrap()
    );
    assert_eq!(
        resp.text().await.unwrap(),
        serde_json::to_string(&want["body"]).unwrap()
    );
}
