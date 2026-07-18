//! Workbench ROUTE differential (P4.6ay unit 12): the four `/api/v1/custom-tools`
//! legs (`api::custom_tools::{custom_tools_library, custom_tools_destinations,
//! custom_tool_preview, custom_tool_audit}`) vs v4's REAL route handlers, over
//! the committed `workbench-{main,mount}.db` fixture. Diffs `{status, body}`.
//!
//! **The corpus lives in `harness/oracle/fixtures/workbench-route-cases.json`
//! and is read by BOTH sides**, so the jest oracle and this test cannot drift
//! apart on a definition or a character reference.
//!
//! **The simulate strategy (decided at round planning, recorded here).** v4's
//! `simulateOutcomes` draws through the crypto directly with no seam, so a
//! cross-language draw-for-draw diff is impossible. The corpus therefore uses
//! DETERMINISTIC definitions — `min === max` ranges, which short-circuit without
//! drawing a single byte — so `runs`/`hits`/`share`/min/max/mean are exact on
//! both sides. `AUDIT_RUNS` (10_000) applies identically either way. Stochastic
//! spread is covered instead by the v5-side statistical unit tests in
//! `pascal::custom_tools`, mirroring v4's own `custom-tools-simulate.test.ts`.
//!
//! Generate the oracle (v4 @ d68638b4, Node 24 — mirror to /tmp; jest ignores
//! `.claude/`):
//!   cd ~/source/quilltap-server
//!   M=/tmp/qt-workbench-mirror; rm -rf $M; mkdir -p $M/cases $M/fixtures
//!   cp <V5W>/harness/oracle/cases/pascal-workbench-route.test.ts $M/cases/
//!   cp <V5W>/harness/oracle/fixtures/workbench.json \
//!      <V5W>/harness/oracle/fixtures/workbench-route-cases.json $M/fixtures/
//!   ln -sfn $PWD/node_modules $M/node_modules
//!   QT_FIXTURE_WORKBENCH_MAIN=<V5W>/crates/quilltap-web/tests/fixtures/workbench-main.db \
//!   QT_FIXTURE_WORKBENCH_MOUNT=<V5W>/crates/quilltap-web/tests/fixtures/workbench-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-workbench-route.ndjson \
//!     npx jest --silent --roots "$PWD" --roots "$M/cases" --testTimeout=300000 -- pascal-workbench-route
//! Run:
//!   QT_ORACLE_PASCAL_WORKBENCH_ROUTE=/tmp/oracle-pascal-workbench-route.ndjson \
//!     cargo test -p quilltap-harness --test pascal_workbench_route_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::custom_tools::{
    custom_tool_audit, custom_tool_preview, custom_tools_destinations, custom_tools_library,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/workbench-route-cases.json")
}

fn open(case: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-workbench-route-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("workbench-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("workbench-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db")
}

fn status_body(resp: Response) -> (u16, Value) {
    match resp {
        Response::CustomToolsLibrary(v)
        | Response::CustomToolsDestinations(v)
        | Response::CustomToolPreview(v)
        | Response::CustomToolAudit(v) => (200, v),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

/// Collapse a JSON-PARSE failure reason to its prefix — unit 2's documented
/// seam, inherited by the library leg. See `pascal_workbench_equivalence`.
fn canon_reason(v: &Value) -> Value {
    match v.as_str() {
        Some(s) if s.starts_with("is not valid JSON:") => {
            Value::String("is not valid JSON:".into())
        }
        _ => v.clone(),
    }
}

/// Fold integral floats → ints so JS's single number type and Rust's agree.
fn canon(v: &Value) -> Value {
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15)
            .map(|f| Value::from(f as i64))
            .unwrap_or_else(|| v.clone()),
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| {
                    (
                        k.clone(),
                        if k == "reason" {
                            canon_reason(x)
                        } else {
                            canon(x)
                        },
                    )
                })
                .collect(),
        ),
        _ => v.clone(),
    }
}

/// Resolve a case's `metadata` — either given inline, or a `metadataCharacter`
/// reference the corpus resolves to a `{ characterId }` object (exactly as the
/// oracle's `bodyFor` does).
fn metadata_for(case: &Value, characters: &Value) -> Option<Value> {
    if let Some(m) = case.get("metadata") {
        return Some(m.clone());
    }
    let key = case.get("metadataCharacter")?.as_str()?;
    let id = characters.get(key)?.clone();
    Some(json!({ "characterId": id }))
}

#[test]
fn workbench_route_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PASCAL_WORKBENCH_ROUTE") else {
        eprintln!("SKIP: set QT_ORACLE_PASCAL_WORKBENCH_ROUTE (see header).");
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

    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).unwrap()).unwrap();
    let definitions = &corpus["definitions"];
    let characters = &corpus["characters"];
    let cases = corpus["cases"].as_array().expect("cases array");

    let mut checked = 0usize;
    let mut statuses: Vec<u16> = Vec::new();

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("oracle missing case '{name}'"));
        let db = open(name);

        let (status, body) = match case["method"].as_str().unwrap() {
            "GET" => match case.get("action").and_then(Value::as_str) {
                Some("destinations") => status_body(custom_tools_destinations(&db)),
                _ => status_body(custom_tools_library(&db)),
            },
            _ => {
                let definition = definitions[case["definition"].as_str().unwrap()].clone();
                let params = case.get("params").cloned();
                let metadata = metadata_for(case, characters);
                match case["action"].as_str().unwrap() {
                    // The consult seam made preview async (it may pose a
                    // consult); the route surface is otherwise unchanged.
                    "preview" => status_body(
                        tokio::runtime::Builder::new_current_thread()
                            .build()
                            .expect("a current-thread runtime")
                            .block_on(custom_tool_preview(
                                &db,
                                &definition,
                                params.as_ref(),
                                case.get("private").and_then(Value::as_bool),
                                metadata.as_ref(),
                            )),
                    ),
                    _ => status_body(custom_tool_audit(
                        &db,
                        &definition,
                        params.as_ref(),
                        metadata.as_ref(),
                    )),
                }
            }
        };

        assert_eq!(
            status as u64,
            want["status"].as_u64().unwrap(),
            "case '{name}' status"
        );
        assert_eq!(canon(&body), canon(&want["body"]), "case '{name}' body");
        statuses.push(status);
        checked += 1;
    }

    // Coverage the corpus was built for: a thinner corpus (or one whose arms
    // stopped firing) must fail loudly rather than pass on fewer branches.
    assert_eq!(checked, cases.len());
    assert!(statuses.contains(&200), "a success arm");
    assert!(statuses.contains(&400), "an invalid-definition arm");
    assert!(statuses.contains(&404), "an unknown-character arm");
    assert!(
        statuses.contains(&422),
        "a run-refusal / broken-vault arm (v5's first 422)"
    );
    eprintln!("OK: workbench route matched oracle ({checked} cases).");
}
