//! P4.D255 Tier 2 item 20 — the `chat_settings.wardrobeImageSettings` boot
//! ensure (`db::chat_settings_wardrobe_image_settings_repair`) against v4's
//! REAL `add-wardrobe-image-settings-field-v1` in its three shapes (the oracle
//! case `harness/oracle/cases/chat-settings-wardrobe-image-settings-ensure.ts`
//! builds the base files; its header names them).
//!
//! Comparands per shape, byte-exact: `PRAGMA table_info` (order included),
//! `sqlite_master.sql`, every row's raw cell. v4's report is asserted per mode
//! (A RUNS, B and C `not needed`). No `migrations_state` comparand: neither
//! side stamps this migration — v4's runner would, but a re-ADD is a no-op on
//! either app, so v5 records nothing (R-B; the sibling column ensures'
//! precedent).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-wardrobe-image-ensure
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-wardrobe-image-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/chat-settings-wardrobe-image-settings-ensure.ts \
//!     > /tmp/oracle-wardrobe-image-ensure.ndjson
//! Run:
//!   QT_ORACLE_WARDROBE_IMAGE_ENSURE=/tmp/oracle-wardrobe-image-ensure.ndjson \
//!   QT_FIXTURE_WARDROBE_IMAGE_ENSURE_DIR=/tmp/qt-wardrobe-image-ensure \
//!     cargo test -p quilltap-harness --test chat_settings_wardrobe_image_settings_ensure_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::chat_settings_wardrobe_image_settings_repair::ensure_chat_settings_wardrobe_image_settings;
use quilltap_core::db::Writer;
use rusqlite::Connection;
use serde_json::{json, Value};

fn pepper() -> String {
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chat-settings-tier2.json"),
        )
        .unwrap(),
    )
    .unwrap();
    spec["testPepperBase64"].as_str().unwrap().to_string()
}

fn snapshot(conn: &Connection) -> (Value, Value, Value) {
    let mut stmt = conn
        .prepare("PRAGMA table_info(\"chat_settings\")")
        .unwrap();
    let info: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "cid": r.get::<_, i64>(0)?,
                "name": r.get::<_, String>(1)?,
                "type": r.get::<_, String>(2)?,
                "notnull": r.get::<_, i64>(3)?,
                "dflt_value": r.get::<_, Option<String>>(4)?,
                "pk": r.get::<_, i64>(5)?,
            }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'chat_settings'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut stmt = conn
        .prepare("SELECT id, \"wardrobeImageSettings\" FROM chat_settings ORDER BY id")
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({ "id": r.get::<_, String>(0)?, "cell": r.get::<_, Option<String>>(1)? }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (Value::Array(info), Value::String(sql), Value::Array(rows))
}

#[test]
fn the_column_ensure_matches_v4s_migration_in_three_shapes() {
    let (Some(oracle_path), Some(dir)) = (
        std::env::var_os("QT_ORACLE_WARDROBE_IMAGE_ENSURE"),
        std::env::var_os("QT_FIXTURE_WARDROBE_IMAGE_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_WARDROBE_IMAGE_ENSURE and QT_FIXTURE_WARDROBE_IMAGE_ENSURE_DIR (see header)."
        );
        return;
    };
    let pepper = pepper();
    let mut modes = Vec::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v4: Value = serde_json::from_str(line).unwrap();
        let mode = v4["mode"].as_str().unwrap().to_string();
        let report = v4["report"][0].as_str().unwrap();
        let ran = report.starts_with("+RAN add-wardrobe-image-settings-field-v1");
        assert_eq!(ran, mode == "A", "[{mode}] v4's report: {report}");
        assert!(ran || report.ends_with(" not needed"), "[{mode}] {report}");

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(
            Path::new(&dir).join(format!("wardrobe-image-{mode}.db")),
            &work,
        )
        .unwrap();
        let w = Writer::open_writable(&work, &pepper).unwrap();
        ensure_chat_settings_wardrobe_image_settings(w.connection()).expect("the ensure");
        let (info, sql, rows) = snapshot(w.connection());
        assert_eq!(info, v4["tableInfo"], "[{mode}] table_info");
        assert_eq!(sql, v4["sql"], "[{mode}] sqlite_master.sql");
        assert_eq!(rows, v4["rows"], "[{mode}] cells");
        modes.push(mode);
    }
    assert_eq!(modes, ["A", "B", "C"], "every shape ran");
    eprintln!("OK: the wardrobeImageSettings ensure matched v4 in shapes {modes:?}.");
}
