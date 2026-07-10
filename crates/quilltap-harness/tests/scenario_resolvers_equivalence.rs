//! Read-differential (P4.4 unit 2, sub-unit 1): the preset-scenario resolvers
//! (`db::scenarios::resolve_{general,project}_scenario_body`) vs v4's REAL
//! `resolveGeneralScenarioBody` / `resolveProjectScenarioBody`. Both sides READ
//! the SAME baked fixtures (a General store + a project store, each with a seeded
//! `Scenarios/` folder) and run the SAME path matrix, comparing the resolved body
//! (or null) EXACTLY — no normalization.
//!
//! Matrix: bare filename / full path / missing `.md` suffix / a leading-slash
//! form / a missing file (→ null) / a whitespace-only body (→ null) for the
//! general store, plus full/bare/missing for the project store.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-scenario-resolvers-fixture.ts
//!   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/scenario-resolvers.ts > /tmp/oracle-scen.ndjson
//! Run:
//!   QT_ORACLE_SCEN=/tmp/oracle-scen.ndjson \
//!   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
//!     cargo test -p quilltap-harness --test scenario_resolvers_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::scenarios::{resolve_general_scenario_body, resolve_project_scenario_body};
use quilltap_core::db::Writer;
use serde::Deserialize;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "projectMountPointId")]
    project_mount_point_id: String,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    body: Option<String>,
}

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

    let mut oracle: HashMap<String, Option<String>> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Row = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(row.id, row.body);
    }

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-scen-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-scen-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let main = main_w.connection();
    let mount = mount_w.connection();
    let p = &spec.project_mount_point_id;

    // The matrix — MUST mirror harness/oracle/cases/scenario-resolvers.ts.
    let general = |path: &str| resolve_general_scenario_body(main, mount, path).expect("read");
    let project = |path: &str| resolve_project_scenario_body(mount, p, path);

    let cases: Vec<(&str, Option<String>)> = vec![
        ("general_bare", general("welcome")),
        ("general_full", general("Scenarios/welcome.md")),
        ("general_nomd", general("Scenarios/welcome")),
        ("general_leading_slash", general("/welcome")),
        ("general_missing", general("nonexistent")),
        ("general_empty", general("empty")),
        ("project_full", project("Scenarios/tavern.md")),
        ("project_bare", project("tavern")),
        ("project_missing", project("nope")),
    ];

    for (id, got) in cases {
        let want = oracle
            .get(id)
            .unwrap_or_else(|| panic!("oracle missing case {id}"));
        assert_eq!(&got, want, "case {id}: rust != oracle");
    }
    assert_eq!(oracle.len(), 9, "oracle case count drifted");
}
