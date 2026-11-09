//! Tier-2 differential (P4.D378 item 9 — v4 `0c47f4539` at `624d0d25c`): the
//! Inventory owner resolver over the shared `classifyMount`.
//!
//! `cases/inventory-mount-owner.ts` runs every row of
//! `inventory-mount-owner.json` through v4's REAL `mountOwnerResolver()(mp)`
//! and `mountScopeOf(mp)` (`lib/inventory/dependents.ts`), each row in its own
//! child on a fresh copy of the `build-inventory-title-lint-fixture.ts` pair,
//! widened per row by in-case plants. Here each row runs on its own fresh copy
//! through `quilltap_core::inventory::dependents::{describe_mount,
//! mount_scope_of}`, the same plants applied through the same SQL and the REAL
//! `write_database_document`. Per row, compared as BYTES (`serde_json::
//! to_string` — key order is the contract, memory note
//! `serde-json-map-eq-ignores-order`):
//!   - `owner` — `{scope, ownerId, ownerName, ownerArchived}`, or the error;
//!   - `scope` — the tier, `null` for `unknown`, or the error.
//!
//! The R-F rows (`*_official_store_unreadable` — the owner's `properties.json`
//! planted as `{`) also PROVE the plant on v5 (round ruling "read project /
//! group owners raw", work order item 13): the HYDRATED `find_all` must drop
//! the owner or fail, `find_by_id` must fail, and the RAW
//! `find_by_official_mount_point_id_raw` must still name it with v4's raw
//! name. The v4 side asserts the same in-case (`findAll` drops, `findById`
//! throws). Those self-checks are hard asserts, never comparands.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-itl-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-itl-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-title-lint-fixture.ts
//!   QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/inventory-mount-owner.ts \
//!     > /tmp/oracle-inventory-mount-owner.ndjson
//! Run:
//!   QT_ORACLE_INV_MOUNT_OWNER=/tmp/oracle-inventory-mount-owner.ndjson \
//!   QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
//!     cargo test -p quilltap-harness --test inventory_mount_owner_tier2_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::database_store::write_database_document;
use quilltap_core::db::fallback::error_text;
use quilltap_core::db::groups::{self, GroupsRepository};
use quilltap_core::db::projects::{self, ProjectsRepository};
use quilltap_core::db::Writer;
use quilltap_core::inventory::dependents::{describe_mount, mount_scope_of, MountDescription};
use quilltap_core::inventory::lint_scope::TitleLintScope;
use regex::Regex;
use serde_json::{json, Value};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

/// v5's owner answer for one mount — the ONE call site (item 9 dropped
/// `describe_mount`'s `general_mount_point_id` parameter: `classify_mount`
/// reads General itself, as v4's `classifyMount` does).
fn v5_describe(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    mount_point_id: &str,
) -> Result<MountDescription, String> {
    describe_mount(main, mount, mount_point_id).map_err(|e| error_text(&e))
}

fn owner_json(d: &MountDescription) -> Value {
    json!({
        "scope": d.scope.as_str(),
        "ownerId": d.owner_id,
        "ownerName": d.owner_name,
        "ownerArchived": d.owner_archived,
    })
}

/// v4's `{result: JSON.stringify(x)}` / `{error: message}` envelope.
fn outcome(r: Result<Value, String>) -> Value {
    match r {
        Ok(v) => json!({ "result": serde_json::to_string(&v).unwrap() }),
        Err(e) => json!({ "error": e }),
    }
}

fn raw_mount(main: &rusqlite::Connection, sql: &str, id: &str) -> String {
    main.query_row(sql, [id], |r| r.get::<_, String>(0))
        .unwrap_or_else(|e| panic!("no mount for {sql}: {e}"))
}

/// The R-F self-check on v5: `(findAll, findById, rawName)`, v4's labels.
fn hydrated_v5(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    entity: &str,
    id: &str,
    probe: &str,
) -> (String, String, Option<String>) {
    let has = |rows: &[Value]| rows.iter().any(|r| r["id"] == id);
    let (all, by_id, raw) = match entity {
        "projects" => {
            let repo = ProjectsRepository::new(main, mount);
            (
                repo.find_all()
                    .map(|rows| has(&rows))
                    .map_err(|e| format!("{e:?}")),
                repo.find_by_id(id)
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}")),
                projects::find_by_official_mount_point_id_raw(main, probe),
            )
        }
        "groups" => {
            let repo = GroupsRepository::new(main, mount);
            (
                repo.find_all()
                    .map(|rows| has(&rows))
                    .map_err(|e| format!("{e:?}")),
                repo.find_by_id(id)
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}")),
                groups::find_by_official_mount_point_id_raw(main, probe),
            )
        }
        other => panic!("hydrated entity {other}"),
    };
    let find_all = match all {
        Ok(true) => "present".to_string(),
        Ok(false) => "dropped".to_string(),
        Err(e) => format!("error: {e}"),
    };
    let find_by_id = match by_id {
        Ok(()) => "ok".to_string(),
        Err(e) => format!("throws: {e}"),
    };
    let raw_name = raw.and_then(|r| r.get("name").and_then(Value::as_str).map(str::to_string));
    (find_all, find_by_id, raw_name)
}

#[test]
fn inventory_mount_owner_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_INV_MOUNT_OWNER") else {
        eprintln!(
            "SKIP: set QT_ORACLE_INV_MOUNT_OWNER (and QT_FIXTURE_ITL_MAIN / _MOUNT) — see header."
        );
        return;
    };
    let (Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_FIXTURE_ITL_MAIN"),
        std::env::var("QT_FIXTURE_ITL_MOUNT"),
    ) else {
        panic!("QT_ORACLE_INV_MOUNT_OWNER is set but QT_FIXTURE_ITL_MAIN / _MOUNT are not");
    };
    let pair: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("inventory-title-lint.json")).unwrap(),
    )
    .unwrap();
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("inventory-mount-owner.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let by_op: HashMap<&str, &Value> = oracle
        .iter()
        .map(|r| (r["op"].as_str().unwrap(), r))
        .collect();
    let pepper = pair["testPepperBase64"].as_str().unwrap();
    let rows = spec["rows"].as_array().unwrap();
    assert_eq!(
        oracle.len(),
        rows.len(),
        "the oracle has {} rows, the spec {}",
        oracle.len(),
        rows.len()
    );
    let mp_re = Regex::new(r"\{\{mp:(\w+)\}\}").unwrap();
    let mut failures: Vec<String> = Vec::new();
    let mut green: Vec<&str> = Vec::new();

    for row in rows {
        let name = row["name"].as_str().unwrap();
        let want = by_op
            .get(name)
            .unwrap_or_else(|| panic!("{name}: no oracle row"));
        let dir = tempfile::tempdir().unwrap();
        let (main_path, mount_path) = (dir.path().join("main.db"), dir.path().join("mount.db"));
        std::fs::copy(&fixture_main, &main_path).unwrap();
        std::fs::copy(&fixture_mount, &mount_path).unwrap();
        let main_w = Writer::open_writable(&main_path, pepper).unwrap();
        let mount_w = Writer::open_writable(&mount_path, pepper).unwrap();
        let (main, mount) = (main_w.connection(), mount_w.connection());

        let mounts: HashMap<&str, String> = [
            (
                "general",
                pair["generalMountPointId"].as_str().unwrap().to_string(),
            ),
            (
                "friday",
                raw_mount(
                    main,
                    "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                    pair["friday"]["id"].as_str().unwrap(),
                ),
            ),
            (
                "project",
                raw_mount(
                    main,
                    "SELECT officialMountPointId FROM projects WHERE id = ?1",
                    pair["project"]["id"].as_str().unwrap(),
                ),
            ),
            (
                "group",
                raw_mount(
                    main,
                    "SELECT officialMountPointId FROM groups WHERE id = ?1",
                    pair["group"]["id"].as_str().unwrap(),
                ),
            ),
        ]
        .into_iter()
        .collect();
        for (k, v) in &mounts {
            assert_eq!(
                want["mounts"][*k].as_str(),
                Some(v.as_str()),
                "{name}: the {k} store resolves to a different mount on v5"
            );
        }
        let mp = |s: &str| {
            mp_re
                .replace_all(s, |c: &regex::Captures| mounts[&c[1]].clone())
                .into_owned()
        };

        for step in row["steps"].as_array().unwrap() {
            match step["do"].as_str().unwrap() {
                "sql" => {
                    let conn = if step["db"] == "main" { main } else { mount };
                    conn.execute_batch(&mp(step["sql"].as_str().unwrap()))
                        .unwrap_or_else(|e| panic!("{name}: sql plant failed: {e}"));
                }
                "doc-plant" => {
                    write_database_document(
                        mount,
                        &mp(step["mp"].as_str().unwrap()),
                        step["path"].as_str().unwrap(),
                        step["content"].as_str().unwrap(),
                        TitleLintScope::Linted,
                    )
                    .unwrap_or_else(|e| panic!("{name}: doc plant failed: {e:?}"));
                }
                other => panic!("{name}: step {other}"),
            }
        }

        let probe = mp(row["probe"].as_str().unwrap());
        assert_eq!(
            want["probe"].as_str(),
            Some(probe.as_str()),
            "{name}: probe"
        );
        let got_owner = outcome(v5_describe(main, mount, &probe).map(|d| owner_json(&d)));
        let got_scope = outcome(
            mount_scope_of(main, mount, &probe)
                .map(|s| s.map_or(Value::Null, |s| json!(s.as_str())))
                .map_err(|e| error_text(&e)),
        );

        // R-F: the plant fails the hydrated read and leaves the raw one whole, on v5 too.
        if let Some(h) = row.get("hydrated") {
            let (find_all, find_by_id, raw_name) = hydrated_v5(
                main,
                mount,
                h["entity"].as_str().unwrap(),
                h["id"].as_str().unwrap(),
                &probe,
            );
            eprintln!(
                "R-F {name}: v5 hydrated findAll={find_all} findById={find_by_id} rawName={raw_name:?}; v4 {}",
                want["hydrated"]
            );
            assert!(
                find_all != "present",
                "{name}: the plant did not fail v5's HYDRATED find_all (the row proves nothing)"
            );
            assert!(
                find_by_id.starts_with("throws"),
                "{name}: the plant did not fail v5's HYDRATED find_by_id"
            );
            assert_eq!(
                raw_name.as_deref(),
                want["hydrated"]["rawName"].as_str(),
                "{name}: v5's RAW read does not name the owner as v4's does"
            );
        }

        let s = |v: &Value| serde_json::to_string(v).unwrap();
        let mut red = false;
        if s(&got_owner) != s(&want["owner"]) {
            failures.push(format!(
                "{name} OWNER\n  v5: {}\n  v4: {}",
                s(&got_owner),
                s(&want["owner"])
            ));
            red = true;
        }
        if s(&got_scope) != s(&want["scope"]) {
            failures.push(format!(
                "{name} SCOPE\n  v5: {}\n  v4: {}",
                s(&got_scope),
                s(&want["scope"])
            ));
            red = true;
        }
        if !red {
            green.push(name);
        }
    }
    eprintln!(
        "inventory mount owner: {} rows compared, {} green: {green:?}",
        rows.len(),
        green.len()
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
