//! P4.60 (the confirm-only pass) — the `.qtap` import legs' BODY GUARDS vs v4's
//! REAL route handlers, over real HTTP.
//!
//! The order listed `qtap_routes.rs:118-226` as CONFIRM-ONLY: the sub-objects
//! ride whole, and `data_key_absent` deliberately keeps an absent `data` key
//! apart from an explicit null (confirmed — the comment still describes the
//! code). Measuring the neighbouring guards rather than reading them found two
//! real divergences, both now fixed and pinned here:
//!
//! - `if (!body.exportData)` is JS **falsiness**, so `0`, `''` and `false` are
//!   "missing" exactly as `null` is. v5 excluded only `null`, so a falsy body
//!   fell through to the manifest check and answered the WRONG sentence.
//! - v4's `await req.json()` lives inside the handler's `try`, so a malformed
//!   body's rejection escapes to the outer catch as a **500** with the leg's own
//!   sentence — not a 400. A body that is literally `null` gets there too, via
//!   the TypeError from `null.exportData`; a body that is `42` or `"nope"` does
//!   NOT, because JS reads a missing property off a scalar as `undefined`.
//!
//! This is a route-level differential rather than a harness one because the
//! guards live at the transport edge: the comparand is the real axum route,
//! reached over real HTTP against a served instance.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-qtap-guards-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/qtap-import-guards.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-qtap-import-guards.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- qtap-import-guards
//! Run:
//!   QT_ORACLE_QTAP_GUARDS=/tmp/oracle-qtap-import-guards.ndjson \
//!     cargo test -p quilltap-web --test qtap_import_guards_equivalence -- --nocapture

mod common;

use std::collections::HashMap;

use serde_json::{json, Value};

/// A manifest that passes `validateExportFile` — the arms past it are about
/// `options` and `conflictStrategy`.
fn good_manifest() -> Value {
    json!({ "format": "quilltap-export", "version": "1.0", "exportType": "characters" })
}

/// `(case, action, body)`. `None` is the malformed-body arm: raw bytes that are
/// not JSON at all, which is what v4's rejecting `req.json()` models.
fn cases() -> Vec<(&'static str, &'static str, Option<Value>)> {
    let good = json!({ "manifest": good_manifest(), "data": {} });
    vec![
        // --- preview: `if (!body.exportData)` is JS falsiness ---
        (
            "preview_export_data_absent",
            "import-preview",
            Some(json!({})),
        ),
        (
            "preview_export_data_null",
            "import-preview",
            Some(json!({ "exportData": null })),
        ),
        (
            "preview_export_data_zero",
            "import-preview",
            Some(json!({ "exportData": 0 })),
        ),
        (
            "preview_export_data_empty_string",
            "import-preview",
            Some(json!({ "exportData": "" })),
        ),
        (
            "preview_export_data_false",
            "import-preview",
            Some(json!({ "exportData": false })),
        ),
        (
            "preview_export_data_number",
            "import-preview",
            Some(json!({ "exportData": 42 })),
        ),
        (
            "preview_export_data_array",
            "import-preview",
            Some(json!({ "exportData": [1] })),
        ),
        (
            "preview_manifest_format_wrong_type",
            "import-preview",
            Some(
                json!({ "exportData": { "manifest": { "format": 1, "version": "1.0" }, "data": {} } }),
            ),
        ),
        (
            "preview_manifest_version_wrong_type",
            "import-preview",
            Some(
                json!({ "exportData": { "manifest": { "format": "quilltap-export", "version": 1.0 }, "data": {} } }),
            ),
        ),
        (
            "preview_manifest_array",
            "import-preview",
            Some(json!({ "exportData": { "manifest": ["quilltap-export"], "data": {} } })),
        ),
        ("preview_body_not_json", "import-preview", None),
        ("preview_body_number", "import-preview", Some(json!(42))),
        ("preview_body_null", "import-preview", Some(Value::Null)),
        ("preview_body_string", "import-preview", Some(json!("nope"))),
        // --- execute: `!exportData`, then `!options`, then the allow-list ---
        (
            "execute_export_data_absent",
            "import-execute",
            Some(json!({})),
        ),
        (
            "execute_export_data_zero",
            "import-execute",
            Some(json!({ "exportData": 0 })),
        ),
        (
            "execute_options_absent",
            "import-execute",
            Some(json!({ "exportData": good })),
        ),
        (
            "execute_options_falsy",
            "import-execute",
            Some(
                json!({ "exportData": { "manifest": good_manifest(), "data": {} }, "options": 0 }),
            ),
        ),
        (
            "execute_options_wrong_type",
            "import-execute",
            Some(
                json!({ "exportData": { "manifest": good_manifest(), "data": {} }, "options": "nope" }),
            ),
        ),
        (
            "execute_strategy_wrong_type",
            "import-execute",
            Some(
                json!({ "exportData": { "manifest": good_manifest(), "data": {} },
                         "options": { "conflictStrategy": 7 } }),
            ),
        ),
        (
            "execute_strategy_unknown",
            "import-execute",
            Some(
                json!({ "exportData": { "manifest": good_manifest(), "data": {} },
                         "options": { "conflictStrategy": "merge" } }),
            ),
        ),
        ("execute_body_not_json", "import-execute", None),
        ("execute_body_number", "import-execute", Some(json!(42))),
        ("execute_body_null", "import-execute", Some(Value::Null)),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn qtap_import_guards_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_QTAP_GUARDS") else {
        eprintln!("SKIP: set QT_ORACLE_QTAP_GUARDS (see the test header).");
        return;
    };
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    let all = cases();
    let mut failed: Vec<String> = Vec::new();
    for (name, action, body) in &all {
        let url = format!("http://{addr}/api/v1/system/tools?action={action}");
        let req = client.post(&url).header("content-type", "application/json");
        let req = match body {
            // Raw bytes that are not JSON — v4's `req.json()` rejection.
            None => req.body("this is not json at all"),
            Some(v) => req.body(serde_json::to_string(v).unwrap()),
        };
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16() as u64;
        let text = resp.text().await.unwrap();
        let got: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));

        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("oracle missing case '{name}'"));
        if status != want["status"].as_u64().unwrap() {
            eprintln!("[{name}] STATUS {status} != {} ({got})", want["status"]);
            failed.push(format!("{name}_status"));
        } else if got != want["body"] {
            eprintln!("[{name}] BODY {got} != {}", want["body"]);
            failed.push((*name).to_string());
        } else {
            eprintln!("[{name}] OK ({status}).");
        }
    }

    // Declared on BOTH sides, so a case added to the oracle and forgotten here
    // would pass silently on a smaller set.
    assert_eq!(
        all.len(),
        oracle.len(),
        "the Rust case list and the oracle disagree: {} vs {}",
        all.len(),
        oracle.len()
    );
    assert!(failed.is_empty(), "qtap-import-guards FAILED: {failed:?}");
    eprintln!(
        "OK: qtap import guards matched oracle ({} cases).",
        all.len()
    );
}
