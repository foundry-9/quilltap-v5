//! Tier-2 differential (P4.171 C-19): v4's PHASE 3.4 / 3.4b scenario heals —
//! `services::startup_scenarios::{ensure_project_scenarios_for_all_projects,
//! ensure_group_scenarios_for_all_groups}` vs v4's REAL
//! `ensureProjectScenariosForAllProjects` / `ensureGroupScenariosForAllGroups`
//! (`lib/startup/ensure-{project,group}-scenarios.ts`, `322a5488e`).
//!
//! Both sides open a COPY of the scenario-location fixture pair (its storeless,
//! hand-linked, stale-pointer, whitespace-named and nameless owners are the
//! substrate) and replay the oracle's op list IN ORDER on that one copy: a
//! first boot, a second (idempotent) boot, then a dropped `doc_mount_folders`
//! so every owner's folder heal fails and is COUNTED. Each row compares the
//! loop's `{total, provisioned, healed, failed}`, every owner's
//! `officialMountPointId` (a mount minted during the run is `<minted>`), the
//! `doc_mount_points` count, the mounts holding `Scenarios/` / `Knowledge/`,
//! and the captured lines (the loops' own, the location's folder WARN, and
//! `ensureOfficialStore`'s four).
//!
//! Build the fixture (see `scenario_location_equivalence`'s header), then:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   LANG=en_US.UTF-8 TZ=UTC QT_FIXTURE_SL_MAIN=/tmp/qt-scenario-location-main.db \
//!   QT_FIXTURE_SL_MOUNT=/tmp/qt-scenario-location-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/startup-scenarios.ts \
//!     > /tmp/oracle-startup-scenarios.ndjson
//! Run:
//!   QT_ORACLE_STARTUP_SCENARIOS=/tmp/oracle-startup-scenarios.ndjson \
//!   QT_FIXTURE_SL_MAIN=/tmp/qt-scenario-location-main.db \
//!   QT_FIXTURE_SL_MOUNT=/tmp/qt-scenario-location-mount.db \
//!     cargo test -p quilltap-harness --test startup_scenarios_equivalence

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::startup_scenarios::{
    ensure_group_scenarios_for_all_groups, ensure_project_scenarios_for_all_projects,
};
use quilltap_core::test_support::captured_with;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/scenario-location.json")
}

/// The lines this family pins — the v4 case captures the same set.
const PINNED_MESSAGES: &[&str] = &[
    "ensure-project-scenarios: failed for project; continuing",
    "ensure-project-scenarios: complete",
    "ensure-group-scenarios: failed for group; continuing",
    "ensure-group-scenarios: complete",
    "[ScenarioLocation] Failed to ensure Scenarios folder",
    "Project officialMountPointId points to a missing mount point; will heal",
    "Group officialMountPointId points to a missing mount point; will heal",
    "Adopted existing linked store as project official",
    "Adopted existing linked store as group official",
    "Created project-official document store",
    "Created group-official document store",
];

/// Ops the corpus must carry (a truncated oracle must not pass).
const MIN_ROWS: usize = 7;

fn all_mounts(mount: &Connection) -> Vec<String> {
    let mut stmt = mount
        .prepare("SELECT id FROM doc_mount_points")
        .expect("prepare");
    stmt.query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect()
}

fn remap(s: &str, mount: &Connection, known: &HashSet<String>) -> String {
    let mut out = s.to_string();
    for id in all_mounts(mount) {
        if !known.contains(&id) {
            out = out.replace(&id, "<minted>");
        }
    }
    out
}

fn fks(main: &Connection, mount: &Connection, known: &HashSet<String>, table: &str) -> Value {
    let mut stmt = main
        .prepare(&format!(
            "SELECT id, officialMountPointId FROM \"{table}\" ORDER BY id"
        ))
        .expect("prepare");
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .expect("query")
        .map(|r| {
            let (id, mp) = r.expect("row");
            json!([id, mp.map(|m| remap(&m, mount, known))])
        })
        .collect();
    Value::Array(rows)
}

fn folders(mount: &Connection, known: &HashSet<String>, broken: bool) -> Value {
    if broken {
        return json!(["<broken>"]);
    }
    let mut stmt = mount
        .prepare("SELECT mountPointId, path FROM doc_mount_folders WHERE LOWER(path) IN ('scenarios', 'knowledge')")
        .expect("prepare");
    let mut out: Vec<String> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .expect("query")
        .map(|r| {
            let (m, p) = r.expect("row");
            format!("{} {p}", remap(&m, mount, known))
        })
        .collect();
    out.sort();
    json!(out)
}

#[test]
fn startup_scenarios_match_oracle() {
    let Some(oracle_path) = std::env::var("QT_ORACLE_STARTUP_SCENARIOS").ok() else {
        eprintln!("SKIP: set QT_ORACLE_STARTUP_SCENARIOS to the oracle NDJSON (see header).");
        return;
    };
    let (Some(main_fixture), Some(mount_fixture)) = (
        std::env::var("QT_FIXTURE_SL_MAIN").ok(),
        std::env::var("QT_FIXTURE_SL_MOUNT").ok(),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_SL_MAIN / QT_FIXTURE_SL_MOUNT (see header).");
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec"))
            .expect("spec");
    let rows: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    assert!(
        rows.len() >= MIN_ROWS,
        "{} rows, floor {MIN_ROWS} — truncated oracle?",
        rows.len()
    );

    let scratch = tempfile::Builder::new()
        .prefix("qt-ss-rust-")
        .tempdir()
        .expect("tempdir");
    let main_work = scratch.path().join("main.db");
    let mount_work = scratch.path().join("mount.db");
    std::fs::copy(&main_fixture, &main_work).expect("copy main");
    std::fs::copy(&mount_fixture, &mount_work).expect("copy mount");
    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64).expect("open main");
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64).expect("open mount");
    let (main, mount) = (main_w.connection(), mount_w.connection());
    let known: HashSet<String> = all_mounts(mount).into_iter().collect();

    let mut failures: Vec<String> = Vec::new();
    let mut broken = false;
    for row in &rows {
        let name = row["name"].as_str().unwrap();
        let (result, lines) = captured_with(|| match row["op"].as_str().unwrap() {
            "projects" => {
                let c = ensure_project_scenarios_for_all_projects(main, mount);
                json!({ "total": c.total, "provisioned": c.provisioned, "healed": c.healed, "failed": c.failed })
            }
            "groups" => {
                let c = ensure_group_scenarios_for_all_groups(main, mount);
                json!({ "total": c.total, "provisioned": c.provisioned, "healed": c.healed, "failed": c.failed })
            }
            "breakFolders" => {
                mount
                    .execute_batch("DROP TABLE doc_mount_folders")
                    .expect("break folders");
                broken = true;
                Value::Null
            }
            other => panic!("unknown op {other}"),
        });
        let lines: Vec<String> = lines
            .into_iter()
            .filter_map(|l| {
                let (level, rest) = l.split_once(' ')?;
                let (_target, rest) = rest.split_once(' ')?;
                PINNED_MESSAGES
                    .iter()
                    .any(|m| rest.starts_with(m))
                    .then(|| format!("{level} {}", remap(rest, mount, &known)))
            })
            .collect();
        let checks = [
            ("result", row["result"].clone(), result),
            (
                "projects",
                row["projects"].clone(),
                fks(main, mount, &known, "projects"),
            ),
            (
                "groups",
                row["groups"].clone(),
                fks(main, mount, &known, "groups"),
            ),
            (
                "points",
                row["points"].clone(),
                json!(mount
                    .query_row("SELECT COUNT(*) FROM doc_mount_points", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap()),
            ),
            (
                "folders",
                row["folders"].clone(),
                folders(mount, &known, broken),
            ),
            ("lines", row["lines"].clone(), json!(lines)),
        ];
        for (what, want, got) in checks {
            if serde_json::to_string(&want).unwrap() != serde_json::to_string(&got).unwrap() {
                failures.push(format!("[{name}] {what}\n  v4: {want:#}\n  v5: {got:#}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    println!("startup_scenarios: {} rows match v4", rows.len());
}
