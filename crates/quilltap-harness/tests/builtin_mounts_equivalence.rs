//! Differential: the three built-in mount stores (P4.4u3, family 2).
//!
//! Runs v5's `builtin_mounts::ensure_builtin_mounts` over the same three
//! pre-states the v4 oracle drives through the REAL provisioning migrations, and
//! diffs `doc_mount_points` + `doc_mount_folders` + `instance_settings`. Because
//! every mount/folder id is minted, both dumps are remapped to the SHARED id map
//! (mount id → store name, folder id → path, settings value → store name) before
//! comparison:
//!
//!   - `empty`    → mint all three fresh + subfolders;
//!   - `dangling` → pointers set to non-existent ids → re-provision fresh;
//!   - `live`     → provisioned once, then re-run → ADOPT (no duplicate rows).
//!
//! Generate the oracle (Node 24, from the v4 checkout, via the mirror that lets
//! `better-sqlite3` + `@/` both resolve). The mirror is (re)staged by the
//! recipe's own first stage — it used to assume a pre-existing copy, which is
//! the staging-dependency defect the P4.47 sweep class flags for /tmp paths
//! and which bit here at the help-drift unification, where the `--v4` pin
//! rewrite pointed MIRROR into a fresh worktree that had no mirror at all:
//!   V5W=<this tree>
//!   MIRROR=~/source/quilltap-server/.qt-oracle-mirror
//!   cd ~/source/quilltap-server
//!   rm -rf "$MIRROR/oracle" ; mkdir -p "$MIRROR"
//!   cp -R "$V5W/harness/oracle" "$MIRROR/oracle"
//!   : > /tmp/oracle-builtin-mounts.ndjson
//!   for S in empty dangling live; do
//!     QT_STATE=$S npx tsx "$MIRROR/oracle/cases/builtin-mounts.ts" \
//!       >> /tmp/oracle-builtin-mounts.ndjson
//!   done
//! Run:
//!   QT_ORACLE_BUILTIN_MOUNTS=/tmp/oracle-builtin-mounts.ndjson \
//!     cargo test -p quilltap-harness --test builtin_mounts_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::Path;

use quilltap_core::db::instance_settings;
use quilltap_core::db::Writer;
use quilltap_core::services::builtin_mounts::ensure_builtin_mounts;
use serde_json::{json, Value};

const TEST_PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

const MOUNT_KEYS: [&str; 3] = [
    "lanternBackgroundsMountPointId",
    "userUploadsMountPointId",
    "generalMountPointId",
];

/// Load the captured fresh schema for the given partition.
fn schema_statements(partition: &str) -> Vec<String> {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/provisioning/fresh_schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).expect("read fresh_schema"))
            .expect("parse fresh_schema");
    schema[partition]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect()
}

fn instance_settings_ddl() -> String {
    schema_statements("main")
        .into_iter()
        .find(|s| s.contains("CREATE TABLE \"instance_settings\""))
        .expect("instance_settings DDL present")
}

/// Collapse an integer-valued REAL cell to an integer (v5's generateDDL REAL
/// count columns store `0.0`; v4's migration INTEGER columns store `0`).
fn collapse(v: &Value) -> Value {
    if let Some(f) = v.as_f64() {
        if v.is_f64() && f.fract() == 0.0 && f.is_finite() {
            return json!(f as i64);
        }
    }
    v.clone()
}

/// Remap raw dumps to the shared id map (mount id → name, folder id → path,
/// settings value → store name), so the minted ids drop out of the comparison.
fn remap(dmp: &Value, dmf: &Value, isettings: &Value) -> Value {
    let mut id_to_name: HashMap<String, String> = HashMap::new();
    for row in dmp["rows"].as_array().unwrap() {
        id_to_name.insert(
            row["id"].as_str().unwrap().to_string(),
            row["name"].as_str().unwrap().to_string(),
        );
    }
    let mut fid_to_path: HashMap<String, String> = HashMap::new();
    for row in dmf["rows"].as_array().unwrap() {
        fid_to_path.insert(
            row["id"].as_str().unwrap().to_string(),
            row["path"].as_str().unwrap().to_string(),
        );
    }

    // doc_mount_points: drop id/createdAt/updatedAt, collapse count reals; by name.
    let count_cols = ["fileCount", "chunkCount", "totalSizeBytes"];
    let mut points: Vec<Value> = dmp["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut o = row.as_object().unwrap().clone();
            o.remove("id");
            o.remove("createdAt");
            o.remove("updatedAt");
            for c in count_cols {
                if let Some(v) = o.get(c) {
                    let cv = collapse(v);
                    o.insert(c.to_string(), cv);
                }
            }
            Value::Object(o)
        })
        .collect();
    points.sort_by_key(|r| r["name"].as_str().unwrap_or("").to_string());

    // doc_mount_folders: resolve mount/parent ids to names/paths; by (mount, path).
    let mut folders: Vec<Value> = dmf["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mount = id_to_name
                .get(row["mountPointId"].as_str().unwrap())
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_string());
            let parent = match row["parentId"].as_str() {
                Some(pid) => Value::from(fid_to_path.get(pid).cloned()),
                None => Value::Null,
            };
            json!({
                "mount": mount,
                "name": row["name"],
                "path": row["path"],
                "parent": parent,
            })
        })
        .collect();
    folders.sort_by_key(|r| {
        format!(
            "{}|{}",
            r["mount"].as_str().unwrap_or(""),
            r["path"].as_str().unwrap_or("")
        )
    });

    // instance_settings: the 3 mount pointers, value → store name; by key.
    let mut settings: Vec<Value> = isettings["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| MOUNT_KEYS.contains(&row["key"].as_str().unwrap_or("")))
        .map(|row| {
            let points_to = id_to_name
                .get(row["value"].as_str().unwrap())
                .cloned()
                .unwrap_or_else(|| "<dangling>".to_string());
            json!({ "key": row["key"], "points_to": points_to })
        })
        .collect();
    settings.sort_by_key(|r| r["key"].as_str().unwrap_or("").to_string());

    json!({ "points": points, "folders": folders, "settings": settings })
}

#[test]
fn builtin_mounts_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_BUILTIN_MOUNTS") else {
        eprintln!("SKIP: set QT_ORACLE_BUILTIN_MOUNTS to the oracle NDJSON (see header).");
        return;
    };
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let mut oracle_by_state: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        oracle_by_state.insert(v["state"].as_str().unwrap().to_string(), v);
    }

    let is_ddl = instance_settings_ddl();
    let mi_ddl = schema_statements("mountIndex");

    for state in ["empty", "dangling", "live"] {
        let want = oracle_by_state
            .get(state)
            .unwrap_or_else(|| panic!("oracle missing state {state}"));

        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("quilltap.db");
        let mi_path = dir.path().join("quilltap-mount-index.db");

        let main = Writer::open_writable(&main_path, TEST_PEPPER).expect("open main");
        main.connection().execute_batch(&is_ddl).expect("main ddl");
        let mount_index = Writer::open_writable(&mi_path, TEST_PEPPER).expect("open mi");
        for stmt in &mi_ddl {
            mount_index
                .connection()
                .execute_batch(stmt)
                .expect("mi ddl");
        }

        // Pre-state.
        if state == "dangling" {
            instance_settings::set_lantern_backgrounds_mount_point_id(
                main.connection(),
                "dangling-lantern",
            )
            .unwrap();
            instance_settings::set_user_uploads_mount_point_id(
                main.connection(),
                "dangling-uploads",
            )
            .unwrap();
            instance_settings::set_general_mount_point_id(main.connection(), "dangling-general")
                .unwrap();
        }

        // The unit under test (live runs it twice → adopt on the second pass).
        ensure_builtin_mounts(main.connection(), mount_index.connection()).expect("ensure mounts");
        if state == "live" {
            ensure_builtin_mounts(main.connection(), mount_index.connection())
                .expect("ensure mounts (adopt)");
        }

        let dmp = mount_index
            .dump_table_json("doc_mount_points", "id")
            .expect("dump points");
        let dmf = mount_index
            .dump_table_json("doc_mount_folders", "id")
            .expect("dump folders");
        let isettings = main
            .dump_table_json("instance_settings", "key")
            .expect("dump settings");

        let got = remap(&dmp, &dmf, &isettings);
        let want_remapped = remap(
            &want["docMountPoints"],
            &want["docMountFolders"],
            &want["instanceSettings"],
        );

        assert_eq!(
            got, want_remapped,
            "state {state} diverged\n  rust:   {got}\n  oracle: {want_remapped}"
        );
        eprintln!(
            "OK: builtin-mounts state '{}' matched oracle ({} points, {} folders).",
            state,
            got["points"].as_array().map(|a| a.len()).unwrap_or(0),
            got["folders"].as_array().map(|a| a.len()).unwrap_or(0),
        );
    }
}
