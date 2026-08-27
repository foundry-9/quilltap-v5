//! P4.62 — the Data & System edges' BODY GUARDS vs v4's REAL route handlers,
//! over real HTTP.
//!
//! This is the differential behind the wrong-type-collapse census's
//! `system_data_routes.rs` row. Every site that census counts reads a body key
//! with `.and_then(Value::as_*)`, which folds **absent**, explicit **null** and
//! present-but-**wrong-typed** into one `None`. Whether that is faithful is a
//! question about v4's route, not about the idiom — so each is measured here.
//!
//! The verdicts this family carries (the census prose is the ledger; these are
//! the arms):
//!
//! - **`action` (tasks-queue)** — FAITHFUL. v4's `!action ||
//!   !['start','stop'].includes(action)` sends every non-`start`/`stop` shape,
//!   wrong-typed included, to the SAME sentence.
//! - **`reportId`** — DIVERGENT, fixed here. `!reportId` is JS falsiness, so a
//!   TRUTHY non-string (`true`, `123`, `{}`, `[]`) passes the gate and dies in
//!   the `f.id === reportId` lookup as a **404**. v5 answered 400.
//! - **`concurrency`** — DIVERGENT, fixed here. v4 answers
//!   `validationError(...)` = `{error:'Validation error', details:[…zod issues]}`;
//!   v5 answered an invented flat sentence with no `details`.
//! - **`confirm` / `keepArchivedCharacterBundles`** — FAITHFUL. `confirm !==
//!   'DELETE_ALL_MY_DATA'` refuses every non-matching shape identically, and
//!   `keepArchivedCharacterBundles !== false` keeps the bundles for absent, null
//!   and wrong-typed alike — exactly what `Option<bool>` + "None keeps" does.
//! - **`threshold`** — FAITHFUL. v4 itself coerces
//!   (`typeof x === 'number' ? x : 0.80`).
//! - **`type`** — FAITHFUL. `BackgroundJobTypeEnum.safeParse` refuses every
//!   non-member with one sentence.
//! - **the passphrase `str_field`** — FAITHFUL. v4 is literally
//!   `typeof body.x === 'string' ? body.x : ''`.
//! - **`progressId`** — the 36-char + `Uuid::parse_str` gate was NOT faithful:
//!   Zod 4's `z.uuid()` is RFC-strict about the version and variant nibbles
//!   where `parse_str` is not. The `progressid_gate_*` arms measure Zod's whole
//!   accept-set and the port now mirrors its regex.
//!
//! Same-class NEIGHBOURS the reading turned up, all fixed here: v4's malformed-
//! body answer per action (a 500 with the leg's own sentence — except
//! `job-concurrency`, whose `.catch(() => ({}))` makes it a 400), and the
//! `system/unlock` action + body-shape gates.
//!
//! ## Recorded divergences (both directions, with a reason)
//!
//! - `jobs_body_not_json` — v4 relays `getErrorMessage(error)`, i.e. **V8's own
//!   `JSON.parse` text**, which no Rust port can reproduce; the oracle's own
//!   value is whatever its mock throws, so even the oracle is not measuring
//!   production here. v5 keeps its deterministic 400. Status AND body are
//!   asserted on the v5 side so the answer cannot drift unnoticed.
//!
//! Generate the oracle (Node 24, from the v4 checkout or a pinned worktree —
//! see the .test.ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-sys-guards-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/system-body-guards.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-system-body-guards.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- system-body-guards
//! Run:
//!   QT_ORACLE_SYSTEM_BODY_GUARDS=/tmp/oracle-system-body-guards.ndjson \
//!     cargo test -p quilltap-web --test system_body_guards_equivalence -- --nocapture

mod common;

use std::collections::HashMap;

use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Tools,
    Jobs,
    Unlock,
}

struct Case {
    name: &'static str,
    route: Route,
    /// `?action=`; `""` means send no action parameter at all.
    action: &'static str,
    /// `None` = raw bytes that are not JSON — v4's rejecting `req.json()`.
    body: Option<Value>,
}

fn c(name: &'static str, route: Route, action: &'static str, body: Value) -> Case {
    Case {
        name,
        route,
        action,
        body: Some(body),
    }
}

fn bad_json(name: &'static str, route: Route, action: &'static str) -> Case {
    Case {
        name,
        route,
        action,
        body: None,
    }
}

/// The v5 case list. Declared here AND in the oracle, and the two lengths are
/// compared, so a case added on one side only cannot pass silently.
fn cases() -> Vec<Case> {
    use Route::{Jobs, Tools, Unlock};
    vec![
        // ── tasks-queue: every shape lands on the one sentence ──
        c("queue_action_absent", Tools, "tasks-queue", json!({})),
        c(
            "queue_action_null",
            Tools,
            "tasks-queue",
            json!({"action": null}),
        ),
        c(
            "queue_action_number",
            Tools,
            "tasks-queue",
            json!({"action": 7}),
        ),
        c(
            "queue_action_true",
            Tools,
            "tasks-queue",
            json!({"action": true}),
        ),
        c(
            "queue_action_array",
            Tools,
            "tasks-queue",
            json!({"action": []}),
        ),
        c(
            "queue_action_unknown",
            Tools,
            "tasks-queue",
            json!({"action": "go"}),
        ),
        c(
            "queue_action_start",
            Tools,
            "tasks-queue",
            json!({"action": "start"}),
        ),
        c(
            "queue_action_stop",
            Tools,
            "tasks-queue",
            json!({"action": "stop"}),
        ),
        bad_json("queue_body_not_json", Tools, "tasks-queue"),
        // ── job-concurrency: v4's zod `validationError` envelope ──
        c("concurrency_absent", Tools, "job-concurrency", json!({})),
        c(
            "concurrency_null",
            Tools,
            "job-concurrency",
            json!({"concurrency": null}),
        ),
        c(
            "concurrency_string",
            Tools,
            "job-concurrency",
            json!({"concurrency": "4"}),
        ),
        c(
            "concurrency_bool",
            Tools,
            "job-concurrency",
            json!({"concurrency": true}),
        ),
        c(
            "concurrency_float",
            Tools,
            "job-concurrency",
            json!({"concurrency": 4.5}),
        ),
        c(
            "concurrency_zero",
            Tools,
            "job-concurrency",
            json!({"concurrency": 0}),
        ),
        c(
            "concurrency_too_big",
            Tools,
            "job-concurrency",
            json!({"concurrency": 33}),
        ),
        c(
            "concurrency_ok",
            Tools,
            "job-concurrency",
            json!({"concurrency": 4}),
        ),
        bad_json("concurrency_body_not_json", Tools, "job-concurrency"),
        // ── capabilities-report-delete: falsy → 400, TRUTHY non-string → 404 ──
        c(
            "report_delete_absent",
            Tools,
            "capabilities-report-delete",
            json!({}),
        ),
        c(
            "report_delete_null",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": null}),
        ),
        c(
            "report_delete_empty",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": ""}),
        ),
        c(
            "report_delete_zero",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": 0}),
        ),
        c(
            "report_delete_false",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": false}),
        ),
        c(
            "report_delete_true",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": true}),
        ),
        c(
            "report_delete_number",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": 123}),
        ),
        c(
            "report_delete_object",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": {}}),
        ),
        c(
            "report_delete_array",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": []}),
        ),
        c(
            "report_delete_unknown",
            Tools,
            "capabilities-report-delete",
            json!({"reportId": "nope"}),
        ),
        bad_json(
            "report_delete_body_not_json",
            Tools,
            "capabilities-report-delete",
        ),
        // ── delete-data: REFUSAL ARMS ONLY. Nothing here may pass the confirm
        //    gate — a passing arm would wipe the served instance.
        c(
            "delete_data_confirm_absent",
            Tools,
            "delete-data",
            json!({}),
        ),
        c(
            "delete_data_confirm_null",
            Tools,
            "delete-data",
            json!({"confirm": null}),
        ),
        c(
            "delete_data_confirm_number",
            Tools,
            "delete-data",
            json!({"confirm": 1}),
        ),
        c(
            "delete_data_confirm_wrong",
            Tools,
            "delete-data",
            json!({"confirm": "nope"}),
        ),
        c(
            "delete_data_confirm_wrong_keep_false",
            Tools,
            "delete-data",
            json!({"confirm": "nope", "keepArchivedCharacterBundles": false}),
        ),
        bad_json("delete_data_body_not_json", Tools, "delete-data"),
        // ── memory-dedup ──
        c(
            "dedup_threshold_string",
            Tools,
            "memory-dedup",
            json!({"threshold": "0.9"}),
        ),
        c(
            "dedup_threshold_low",
            Tools,
            "memory-dedup",
            json!({"threshold": 0.4}),
        ),
        c(
            "dedup_threshold_high",
            Tools,
            "memory-dedup",
            json!({"threshold": 1.5}),
        ),
        bad_json("dedup_body_not_json", Tools, "memory-dedup"),
        // ── the jobs-collection enqueue body (refusal arms; the ACCEPTED arms
        //    live in `system_jobs_collection_equivalence`, where the stored row
        //    is the comparand) ──
        c("jobs_type_absent", Jobs, "", json!({"payload": {}})),
        c(
            "jobs_type_null",
            Jobs,
            "",
            json!({"type": null, "payload": {}}),
        ),
        c(
            "jobs_type_number",
            Jobs,
            "",
            json!({"type": 7, "payload": {}}),
        ),
        c(
            "jobs_type_object",
            Jobs,
            "",
            json!({"type": {}, "payload": {}}),
        ),
        c(
            "jobs_payload_null",
            Jobs,
            "",
            json!({"type": "MEMORY_HOUSEKEEPING", "payload": null}),
        ),
        bad_json("jobs_body_not_json", Jobs, ""),
        // ── system/unlock: the action gate, then the body-shape gate ──
        c("unlock_action_missing", Unlock, "", json!({})),
        // `?action=` present but EMPTY — v4's `!action` reads it as absent.
        c("unlock_action_empty", Unlock, "__EMPTY__", json!({})),
        c("unlock_action_unknown", Unlock, "bogus", json!({})),
        bad_json("unlock_body_not_json", Unlock, "change-passphrase"),
        c("unlock_body_null", Unlock, "change-passphrase", Value::Null),
        c("unlock_body_array", Unlock, "change-passphrase", json!([])),
        c(
            "unlock_body_string",
            Unlock,
            "change-passphrase",
            json!("nope"),
        ),
        c("unlock_body_number", Unlock, "change-passphrase", json!(42)),
        c(
            "unlock_change_passphrase_locked",
            Unlock,
            "change-passphrase",
            json!({"oldPassphrase": "a", "newPassphrase": "b"}),
        ),
        c(
            "unlock_change_passphrase_locked_wrong_types",
            Unlock,
            "change-passphrase",
            json!({"oldPassphrase": 7, "newPassphrase": null}),
        ),
    ]
}

/// v4's message is V8's `JSON.parse` text — unreproducible, and the oracle's own
/// value is its mock's. See the module header.
/// `(case, v5's pinned status, the prefix of v5's pinned `error`)` — the job-type
/// roster itself is pinned by the `jobs_type_*` arms, so only the head is
/// repeated here.
const RECORDED_DIVERGENCES: &[(&str, u16, &str)] = &[(
    "jobs_body_not_json",
    400,
    "Invalid job type. Must be one of: ",
)];

/// Two fields cannot be equal by construction, and both are normalized on BOTH
/// sides:
///
/// - `processorStatus` — v4's `getProcessorStatus()` reports a forked-child
///   snapshot the oracle PINS with a mock; in v5 the pump is a host seam and a
///   `start` really starts it. The echo that matters (`success`, `action`) is
///   compared verbatim.
/// - the dedup sweep's `result` — the oracle's stub echo on one side, a REAL
///   pass over the served fixture on the other.
fn normalize(name: &str, body: &Value) -> Value {
    let mut b = body.clone();
    if let Some(o) = b.as_object_mut() {
        if name.starts_with("dedup_") && o.contains_key("result") {
            o.insert("result".into(), Value::String("<NORM>".into()));
        }
        if o.contains_key("processorStatus") {
            o.insert("processorStatus".into(), Value::String("<NORM>".into()));
        }
    }
    b
}

/// The venue: the committed `system-data-*` family, which is what the
/// system-data routes are diffed over everywhere else — and, unlike the shared
/// `chat-send` instance, it carries the `files` and `instance_settings` tables
/// the report-delete lookup and the concurrency write need. No user-id rewrite:
/// every arm here either refuses or looks up an id that resolves to nothing.
fn materialize_system_data_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for (fixture, name) in [
        ("system-data-main.db", "quilltap.db"),
        ("system-data-mount.db", "quilltap-mount-index.db"),
        ("system-data-llmlogs.db", "quilltap-llm-logs.db"),
    ] {
        std::fs::copy(common::fixtures_dir().join(fixture), data.join(name))
            .unwrap_or_else(|e| panic!("copy {fixture}: {e}"));
    }
    base
}

fn url_for(addr: &std::net::SocketAddr, case: &Case) -> String {
    let path = match case.route {
        Route::Tools => "/api/v1/system/tools",
        Route::Jobs => "/api/v1/system/jobs",
        Route::Unlock => "/api/v1/system/unlock",
    };
    if case.route == Route::Jobs || case.action.is_empty() {
        format!("http://{addr}{path}")
    } else if case.action == "__EMPTY__" {
        format!("http://{addr}{path}?action=")
    } else {
        format!("http://{addr}{path}?action={}", case.action)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn system_body_guards_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SYSTEM_BODY_GUARDS") else {
        eprintln!("SKIP: set QT_ORACLE_SYSTEM_BODY_GUARDS (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let mut oracle: HashMap<String, Value> = HashMap::new();
    let mut gate: HashMap<String, Value> = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        let name = v["name"].as_str().unwrap().to_string();
        if let Some(rest) = name.strip_prefix("progressid_gate_") {
            gate.insert(rest.to_string(), v["tracked"].clone());
        } else {
            oracle.insert(name, v);
        }
    }

    let base = materialize_system_data_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    let all = cases();
    let mut failed: Vec<String> = Vec::new();
    for case in &all {
        let req = client
            .post(url_for(&addr, case))
            .header("content-type", "application/json");
        let req = match &case.body {
            None => req.body("this is not json at all"),
            Some(v) => req.body(serde_json::to_string(v).unwrap()),
        };
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let raw = resp.text().await.unwrap();
        let got: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));

        if let Some((_, want_status, want_error)) =
            RECORDED_DIVERGENCES.iter().find(|(n, ..)| *n == case.name)
        {
            let v4 = oracle
                .get(case.name)
                .unwrap_or_else(|| panic!("oracle missing '{}'", case.name));
            let got_error = got.get("error").and_then(Value::as_str).unwrap_or("");
            if status != *want_status || !got_error.starts_with(want_error) {
                eprintln!(
                    "[{}] RECORDED-DIVERGENCE DRIFTED: {status} {got} (pinned {want_status} / v4 answers {} {})",
                    case.name, v4["status"], v4["body"]
                );
                failed.push(format!("{}_recorded", case.name));
            } else {
                eprintln!("[{}] recorded divergence intact ({status}).", case.name);
            }
            continue;
        }

        let want = oracle
            .get(case.name)
            .unwrap_or_else(|| panic!("oracle missing case '{}'", case.name));
        let want_status = want["status"].as_u64().unwrap() as u16;
        let want_body = normalize(case.name, &want["body"]);
        let got_body = normalize(case.name, &got);
        if status != want_status {
            eprintln!("[{}] STATUS {status} != {want_status} ({got})", case.name);
            failed.push(format!("{}_status", case.name));
        } else if got_body != want_body {
            eprintln!("[{}] BODY {got_body} != {want_body}", case.name);
            failed.push(case.name.to_string());
        } else {
            eprintln!("[{}] OK ({status}).", case.name);
        }
    }

    // ── the progressId gate: v5's filter vs Zod 4's `z.uuid()`, arm for arm ──
    let arms: &[(&str, Option<&str>)] = &[
        ("absent", None),
        ("null", None),
        ("number", None),
        ("empty", Some("")),
        ("v4_uuid", Some("2f1c9a2e-6c3b-4f8a-9c1d-3a5b7e9f0d21")),
        ("nil_uuid", Some("00000000-0000-0000-0000-000000000000")),
        ("max_uuid", Some("ffffffff-ffff-ffff-ffff-ffffffffffff")),
        ("uppercase", Some("2F1C9A2E-6C3B-4F8A-9C1D-3A5B7E9F0D21")),
        ("braced", Some("{2f1c9a2e-6c3b-4f8a-9c1d-3a5b7e9f0d21}")),
        ("simple", Some("2f1c9a2e6c3b4f8a9c1d3a5b7e9f0d21")),
        ("urn", Some("urn:uuid:2f1c9a2e-6c3b-4f8a-9c1d-3a5b7e9f0d21")),
        ("not_a_uuid", Some("nope")),
        ("bad_version", Some("2f1c9a2e-6c3b-9f8a-9c1d-3a5b7e9f0d21")),
        ("bad_variant", Some("2f1c9a2e-6c3b-4f8a-1c1d-3a5b7e9f0d21")),
        ("non_hex_36", Some("gggggggg-6c3b-4f8a-9c1d-3a5b7e9f0d21")),
    ];
    for (name, value) in arms {
        let want = gate
            .get(*name)
            .unwrap_or_else(|| panic!("oracle missing progressid_gate_{name}"));
        let got = value
            .filter(|s| quilltap_web::system_data_routes::zod_uuid(s))
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null);
        if &got != want {
            eprintln!("[progressid_gate_{name}] {got} != {want}");
            failed.push(format!("progressid_gate_{name}"));
        } else {
            eprintln!("[progressid_gate_{name}] OK ({got}).");
        }
    }
    assert_eq!(
        arms.len(),
        gate.len(),
        "the Rust progressId arms and the oracle's disagree: {} vs {}",
        arms.len(),
        gate.len()
    );

    assert_eq!(
        all.len(),
        oracle.len(),
        "the Rust case list and the oracle disagree: {} vs {}",
        all.len(),
        oracle.len()
    );
    assert!(failed.is_empty(), "system body guards FAILED: {failed:?}");
    eprintln!(
        "OK: system body guards matched oracle ({} route arms + {} progressId arms).",
        all.len(),
        arms.len()
    );
}
