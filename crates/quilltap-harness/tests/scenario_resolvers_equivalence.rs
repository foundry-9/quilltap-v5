//! Read-differential for the per-kind scenario reads (P4.4 unit 2; REWRITTEN by
//! P4.D295 — L-8). v4 `545e299bb` deleted the per-tier resolvers this family
//! used to compare (`resolveGeneralScenarioBody` / `resolveProjectScenarioBody`)
//! for ONE chokepoint, the scenario location; the v5 twins
//! (`db::scenarios::resolve_{general,project,group}_scenario_body`) died with
//! them. The family now compares `scenarios::location` against v4's REAL
//! `resolveScenarioLocation('general', …).read(path)` and `lookupScenarioRef`
//! over the SAME seeded fixtures — the pre-span matrix plus `..`, nested,
//! blank, a `%` escape decoded and a malformed one (v4 THROWS `URIError`, R-Z),
//! and a ref into a store with no owner row (refused).
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout or the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=${V5:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-scenario-resolvers-fixture.ts
//!   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/scenario-resolvers.ts > /tmp/oracle-scen.ndjson
//! Run:
//!   QT_ORACLE_SCEN=/tmp/oracle-scen.ndjson \
//!   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
//!     cargo test -p quilltap-harness --test scenario_resolvers_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::chats::ScenarioRef;
use quilltap_core::db::Writer;
use quilltap_core::scenarios::location::{
    lookup_scenario_ref, resolve_scenario_location, ScenarioLocationError, ScenarioRefLookup,
    ScenarioScope,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

/// The location only reads the user for a character (the oracle's `USER`).
const USER: &str = "f0000000-0000-4000-8000-00000000000f";

/// The corpus's row floor (a truncated oracle must not pass).
const MIN_ROWS: usize = 18;

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/scenario-resolvers.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// v4's thrown error as the oracle renders it: `<name>: <message>`.
fn threw(e: &ScenarioLocationError) -> String {
    let name = match e {
        ScenarioLocationError::NotFound(_) => "ScenarioNotFoundError",
        ScenarioLocationError::Conflict(_) => "ScenarioConflictError",
        ScenarioLocationError::Filename(_) => "ScenarioFilenameError",
        ScenarioLocationError::Uri(_) => "URIError",
        ScenarioLocationError::Other(_) => "Error",
    };
    format!("{name}: {e}")
}

#[test]
fn scenario_resolvers_match_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_SCEN"),
        env_or_skip("QT_FIXTURE_SCEN_MAIN"),
        env_or_skip("QT_FIXTURE_SCEN_MOUNT"),
    ) else {
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let rows: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line parses"))
        .collect();
    assert!(
        rows.len() >= MIN_ROWS,
        "{} rows, floor {MIN_ROWS}",
        rows.len()
    );

    // A scratch dir removed on drop (with the TRUNCATE-mode `-journal` files).
    let scratch = tempfile::Builder::new()
        .prefix("qt-scen-rust-")
        .tempdir()
        .expect("tempdir");
    let main_work = scratch.path().join("main.db");
    let mount_work = scratch.path().join("mount.db");
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));
    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let (main, mount) = (main_w.connection(), mount_w.connection());
    let general = resolve_scenario_location(main, mount, ScenarioScope::General, None, USER, false)
        .expect("resolve General")
        .expect("the fixture General store resolves");

    let mut failures = Vec::new();
    for row in &rows {
        let id = row["id"].as_str().unwrap();
        let path = row["path"].as_str().unwrap();
        let mut got = Map::new();
        got.insert("id".into(), json!(id));
        got.insert("op".into(), row["op"].clone());
        match row["op"].as_str().unwrap() {
            "general" => {
                got.insert("path".into(), json!(path));
                match general.read(mount, path) {
                    Ok(s) => {
                        got.insert("body".into(), s.map_or(Value::Null, |s| json!(s.body)));
                    }
                    Err(e) => {
                        got.insert("threw".into(), json!(threw(&e)));
                    }
                }
            }
            "ref" => {
                let mp = row["mountPointId"].as_str().unwrap();
                got.insert("mountPointId".into(), json!(mp));
                got.insert("path".into(), json!(path));
                let r = ScenarioRef {
                    mount_point_id: mp.into(),
                    path: path.into(),
                };
                match lookup_scenario_ref(main, mount, &r, USER) {
                    Ok(ScenarioRefLookup::Refused(reason)) => {
                        got.insert("status".into(), json!("refused"));
                        got.insert("reason".into(), json!(reason));
                    }
                    Ok(ScenarioRefLookup::Missing { path, .. }) => {
                        got.insert("status".into(), json!("missing"));
                        got.insert("missingPath".into(), json!(path));
                    }
                    Ok(ScenarioRefLookup::Ok { scenario, .. }) => {
                        got.insert("status".into(), json!("ok"));
                        got.insert("body".into(), json!(scenario.body));
                        got.insert("scenarioPath".into(), json!(scenario.path));
                    }
                    Err(e) => {
                        got.insert("threw".into(), json!(threw(&e)));
                    }
                }
            }
            other => panic!("unknown op {other}"),
        }
        let got = Value::Object(got);
        // Byte-compare: key order is part of the row.
        if serde_json::to_string(&got).unwrap() != serde_json::to_string(row).unwrap() {
            failures.push(format!("[{id}]\n  v4: {row}\n  v5: {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
