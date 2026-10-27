//! Differential (P4.D340 — v4 `lib/pascal/tool-gate-keys.ts`, `7263105c6` +
//! `a53024d3c`'s any-of walk, at `322a5488e`): the instance-wide gate-key
//! census behind the Inventory title lint.
//!
//! `cases/tool-gate-keys.ts` takes v4's REAL `collectToolGateKeys` on a fresh
//! copy of the title-lint pair per op of `tool-gate-keys.json` (planted tool
//! files through the linted store write, raw SQL for the rest). Here
//! `pascal::tool_gate_keys::collect_tool_gate_keys` runs on its own copy with
//! the same plants, and the two are compared on the census BYTES
//! (`{keys, unread, storesConsulted}` — key order is the contract) and the
//! `[ToolGateKeys] Census taken` DEBUG.
//!
//! No op is carved (the P4.D340 stand-in walk was replaced by v4's at the
//! `322a5488e` unification, §S.5 wire 18): keyed definitions, any-of groups,
//! and the `stage` filter (`read` / `json` unread, `schema` / `duplicate`
//! not) are all compared whole. `tgk-schema-error-keeps-keys` pins that a
//! schema failure beside two keyed definitions neither hides their keys nor
//! lands in `unread`.
//!
//! Generate (Node 24, from the v4 checkout — the title-lint pair first):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-itl-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-itl-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-title-lint-fixture.ts
//!   LOG_LEVEL=debug QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/tool-gate-keys.ts \
//!     > /tmp/oracle-tool-gate-keys.ndjson
//! Run:
//!   QT_ORACLE_TOOL_GATE_KEYS=/tmp/oracle-tool-gate-keys.ndjson \
//!   QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
//!     cargo test -p quilltap-harness --test tool_gate_keys_equivalence -- --nocapture

mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::database_store::{delete_database_document, write_database_document};
use quilltap_core::db::Writer;
use quilltap_core::inventory::lint_scope::TitleLintScope;
use quilltap_core::inventory::location::{
    resolve_inventory_location, ResolveInventoryLocationOptions,
};
use quilltap_core::inventory::types::InventoryScope;
use quilltap_core::pascal::tool_gate_keys::collect_tool_gate_keys;
use quilltap_core::test_support::captured_with;
use regex::Regex;
use serde_json::{json, Map, Value};

const CENSUS_LINE: &str = "[ToolGateKeys] Census taken";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures")
        .join(name)
}

fn v4_line(logs: &Value) -> Vec<String> {
    logs.as_array()
        .unwrap()
        .iter()
        .filter(|l| l["message"] == CENSUS_LINE)
        .map(|log| {
            let fields: Map<String, Value> = log["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|kv| (kv[0].as_str().unwrap().to_string(), kv[1].clone()))
                .collect();
            common::v4_log_as_capture_line(
                "T",
                &json!({ "level": log["level"], "message": CENSUS_LINE, "fields": Value::Object(fields) }),
            )
            .replacen(" T ", " ", 1)
        })
        .collect()
}

fn v5_line(captured: &[String]) -> Vec<String> {
    captured
        .iter()
        .filter_map(|l| {
            let mut parts = l.splitn(3, ' ');
            let (level, _target, rest) = (parts.next()?, parts.next()?, parts.next()?);
            rest.starts_with(CENSUS_LINE)
                .then(|| format!("{level} {rest}"))
        })
        .collect()
}

#[test]
fn tool_gate_keys_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_TOOL_GATE_KEYS") else {
        eprintln!(
            "SKIP: set QT_ORACLE_TOOL_GATE_KEYS (and QT_FIXTURE_ITL_MAIN / _MOUNT) — see header."
        );
        return;
    };
    let (Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_FIXTURE_ITL_MAIN"),
        std::env::var("QT_FIXTURE_ITL_MOUNT"),
    ) else {
        panic!("QT_ORACLE_TOOL_GATE_KEYS is set but QT_FIXTURE_ITL_MAIN / _MOUNT are not");
    };
    let lint_spec: Value = serde_json::from_str(
        &std::fs::read_to_string(fixture("inventory-title-lint.json")).unwrap(),
    )
    .unwrap();
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("tool-gate-keys.json")).unwrap())
            .unwrap();
    let rows: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["op"].as_str().unwrap().to_string(), v)
        })
        .collect();
    let pepper = lint_spec["testPepperBase64"].as_str().unwrap();
    let user_id = lint_spec["userId"].as_str().unwrap();
    let mp_re = Regex::new(r"\{\{mp:(\w+)\}\}").unwrap();

    let mut failures = Vec::new();
    let mut compared = 0;
    for op in spec["ops"].as_array().unwrap() {
        let name = op["name"].as_str().unwrap();
        let want = &rows[name];
        let dir = tempfile::tempdir().unwrap();
        let (main_path, mount_path) = (dir.path().join("main.db"), dir.path().join("mount.db"));
        std::fs::copy(&fixture_main, &main_path).unwrap();
        std::fs::copy(&fixture_mount, &mount_path).unwrap();
        let main_w = Writer::open_writable(&main_path, pepper).unwrap();
        let mount_w = Writer::open_writable(&mount_path, pepper).unwrap();
        let (main, mount) = (main_w.connection(), mount_w.connection());
        let mut mounts: HashMap<String, String> = HashMap::new();
        for (k, scope, id) in [
            ("general", InventoryScope::General, None),
            (
                "project",
                InventoryScope::Project,
                lint_spec["project"]["id"].as_str(),
            ),
            (
                "friday",
                InventoryScope::Character,
                lint_spec["friday"]["id"].as_str(),
            ),
        ] {
            let l = resolve_inventory_location(
                main,
                mount,
                scope,
                id,
                user_id,
                ResolveInventoryLocationOptions { ensure: false },
            )
            .unwrap()
            .unwrap();
            assert_eq!(want["mounts"][k].as_str(), Some(l.mount_point_id.as_str()));
            mounts.insert(k.to_string(), l.mount_point_id);
        }
        let mp = |s: &str| {
            mp_re
                .replace_all(s, |c: &regex::Captures| mounts[&c[1]].clone())
                .into_owned()
        };
        if op["tools"] != true {
            assert!(delete_database_document(
                mount,
                &mounts["general"],
                "Tools/measure_twice.tool.json",
                TitleLintScope::Linted,
            )
            .unwrap());
        }
        for step in op["steps"].as_array().unwrap() {
            match step["do"].as_str().unwrap() {
                "doc-plant" => {
                    write_database_document(
                        mount,
                        &mp(step["mp"].as_str().unwrap()),
                        step["path"].as_str().unwrap(),
                        step["content"].as_str().unwrap(),
                        TitleLintScope::Linted,
                    )
                    .unwrap();
                }
                "sql" => {
                    let changed = mount
                        .execute(&mp(step["sql"].as_str().unwrap()), [])
                        .unwrap();
                    assert!(changed > 0, "{name}: a plant changed no row");
                }
                other => panic!("step {other}"),
            }
        }
        let (census, captured) = captured_with(|| collect_tool_gate_keys(mount));
        let got = serde_json::to_string(&census).unwrap();
        let wanted = want["census"].as_str().unwrap();
        if got != wanted {
            failures.push(format!("{name} CENSUS\n  v5: {got}\n  v4: {wanted}"));
        }
        let (gl, wl) = (v5_line(&captured), v4_line(&want["logs"]));
        if gl != wl {
            failures.push(format!("{name} LINE\n  v5: {gl:?}\n  v4: {wl:?}"));
        }
        compared += 1;
    }
    eprintln!("tool gate keys: {compared} ops compared");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
