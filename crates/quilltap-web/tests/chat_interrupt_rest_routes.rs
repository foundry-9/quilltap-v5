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
