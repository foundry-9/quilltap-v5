//! Workbench differential (P4.6ay unit 12): `pascal::workbench`
//! (`build_custom_tool_library` + `list_custom_tool_destinations`) vs v4's REAL
//! `lib/pascal/workbench.ts`, over the committed `workbench-{main,mount}.db`
//! fixture.
//!
//! v4's own `__tests__/unit/lib/pascal/workbench.test.ts` is the case template,
//! but it primes a MOCKED repository world; this differential drives both sides'
//! real repos over one baked instance, so the repo composition is proven too.
//! The fixture deliberately carries a BROKEN character vault (its
//! `properties.json` keystone unlinked): `surveyAttachments` reads `findAllRaw`
//! precisely so that store still earns its character badge — the one an author
//! would be repairing.
//!
//! Generate the oracle (v4 @ d68638b4, Node 24 — mirror to /tmp; jest ignores
//! `.claude/`):
//!   cd ~/source/quilltap-server
//!   M=/tmp/qt-workbench-mirror; rm -rf $M; mkdir -p $M/cases $M/fixtures
//!   cp <V5W>/harness/oracle/cases/pascal-workbench.test.ts $M/cases/
//!   cp <V5W>/harness/oracle/fixtures/workbench.json $M/fixtures/
//!   ln -sfn $PWD/node_modules $M/node_modules
//!   QT_FIXTURE_WORKBENCH_MAIN=<V5W>/crates/quilltap-web/tests/fixtures/workbench-main.db \
//!   QT_FIXTURE_WORKBENCH_MOUNT=<V5W>/crates/quilltap-web/tests/fixtures/workbench-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-workbench.ndjson \
//!     npx jest --silent --roots "$PWD" --roots "$M/cases" --testTimeout=120000 -- pascal-workbench
//! Run:
//!   QT_ORACLE_PASCAL_WORKBENCH=/tmp/oracle-pascal-workbench.ndjson \
//!     cargo test -p quilltap-harness --test pascal_workbench_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::pascal::workbench::{build_custom_tool_library, list_custom_tool_destinations};
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn open(case: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-workbench-{}-{case}", std::process::id()));
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

/// Collapse a JSON-PARSE failure reason to its prefix.
///
/// The unit-2 seam, unchanged and re-inherited here: `readToolFile` reports a
/// malformed file as `is not valid JSON: <engine message>`, and V8's parser
/// sentence is not serde_json's. Both refuse the same file for the same reason;
/// only the engine's wording differs, so no corpus row can assert it as
/// equality. Every OTHER reason — the whole Zod/`formatDefinitionIssues` family,
/// which this port reproduces byte-exactly and which the GET route returns
/// verbatim — is compared unnormalized.
fn canon_reason(v: &Value) -> Value {
    match v.as_str() {
        Some(s) if s.starts_with("is not valid JSON:") => {
            Value::String("is not valid JSON:".into())
        }
        _ => v.clone(),
    }
}

/// Fold integral floats → ints so JS's single number type and Rust's don't
/// disagree on `1` vs `1.0`.
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

#[test]
fn workbench_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PASCAL_WORKBENCH") else {
        eprintln!("SKIP: set QT_ORACLE_PASCAL_WORKBENCH (see header).");
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

    let mut checked = 0usize;

    for case in ["library", "destinations"] {
        let want = oracle
            .get(case)
            .unwrap_or_else(|| panic!("oracle missing case '{case}'"));
        let db = open(case);
        let got = db
            .read_main(|main| {
                db.read_mount_index(|mount| {
                    Ok(match case {
                        "library" => {
                            serde_json::to_value(build_custom_tool_library(main, mount)?).unwrap()
                        }
                        _ => serde_json::to_value(list_custom_tool_destinations(main, mount)?)
                            .unwrap(),
                    })
                })
            })
            .unwrap();

        assert_eq!(canon(&got), canon(&want["value"]), "case '{case}'");
        checked += 1;
    }

    // The corpus is only worth its coverage: assert the branches the fixture was
    // built to exercise actually appear, so a fixture that silently loses a
    // branch fails loudly rather than passing on a thinner world.
    let library = &oracle["library"]["value"];
    let tools = library["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 8, "library tool count");
    assert_eq!(
        library["errors"].as_array().map(Vec::len),
        Some(1),
        "one broken definition file"
    );
    assert!(
        tools.iter().any(|t| t["disabled"] == Value::Bool(true)),
        "a disabled definition"
    );
    assert!(
        tools.iter().any(|t| t["rollForm"] == "dice"),
        "a dice-form definition"
    );
    // §B: the `llm: boolean` badge needs BOTH arms present, or a port that
    // hardcoded the field would pass on an all-`false` world.
    assert!(
        tools.iter().any(|t| t["llm"] == Value::Bool(true)),
        "a consulting definition"
    );
    assert!(
        tools.iter().any(|t| t["llm"] == Value::Bool(false)),
        "a non-consulting definition"
    );
    assert!(
        tools
            .iter()
            .any(|t| t["attachments"].as_array().map(Vec::len) == Some(2)),
        "a mount carrying two badges (group before project)"
    );
    // P4.d19 §2(a): the `gate` field must be seen at ALL THREE of its values, or
    // a port that hardcoded `null` would pass on an ungated world.
    for want in [
        Value::Null,
        Value::String("available".into()),
        Value::String("withheld".into()),
    ] {
        assert!(
            tools.iter().any(|t| t["gate"] == want),
            "no library entry carries gate {want:?}"
        );
    }
    let destinations = &oracle["destinations"]["value"];
    assert!(destinations["general"].is_object(), "a General store");
    assert_eq!(
        destinations["groups"][0]["stores"].as_array().map(Vec::len),
        Some(3),
        "one group with an official + two unofficial stores"
    );
    assert_eq!(
        destinations["characters"].as_array().map(Vec::len),
        Some(2),
        "only the two characters WITH a vault"
    );
    assert!(
        !destinations["other"].as_array().unwrap().is_empty(),
        "an unattached store"
    );

    assert_eq!(checked, 2);
    eprintln!("OK: workbench matched oracle ({checked} cases).");
}
