//! P4.D304 (C1-4) — the two 4.11 `chat_settings` boot ensures
//! (`db::chat_settings_4_11_repair`) against v4's REAL
//! `add-custom-tool-triggers-field-v1` and `add-composer-slash-commands-field-v1`
//! in seven shapes (the oracle case `harness/oracle/cases/chat-settings-4-11-
//! columns-ensure.ts` builds the base files; its header names them): T-A / T-B /
//! T-C for the triggers column, S-A / S-B / S-C for the slash column, and the
//! pair-order mode D (BOTH absent — v4 runs both in registration order, v5 runs
//! its chain). A v5-only D-rev leg re-runs the D base through the chain and
//! asserts the appended order outright.
//!
//! Comparands per shape, byte-exact: `PRAGMA table_info` (order included —
//! mode D's order is the point), `sqlite_master.sql`, every row's two raw cells
//! (the DEFAULT back-fill: `0` / `1`, never NULL). v4's report is asserted per
//! mode (`+RAN` on the A modes and D only), so no mode can go vacuous. No
//! `migrations_state` comparand: neither side stamps (R-B).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-cs411-ensure
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-cs411-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/chat-settings-4-11-columns-ensure.ts \
//!     > /tmp/oracle-cs411-ensure.ndjson
//! Run:
//!   QT_ORACLE_CS411_ENSURE=/tmp/oracle-cs411-ensure.ndjson \
//!   QT_FIXTURE_CS411_ENSURE_DIR=/tmp/qt-cs411-ensure \
//!     cargo test -p quilltap-harness --test chat_settings_4_11_columns_ensure_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::chat_settings_4_11_repair::{
    ensure_chat_settings_composer_slash_commands, ensure_chat_settings_custom_tool_triggers,
    run_chat_settings_4_11_ensures, MIGRATION_ID_SLASH, MIGRATION_ID_TRIGGERS,
};
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
        .prepare(
            "SELECT id, \"customToolTriggers\", \"composerSlashCommands\" FROM chat_settings ORDER BY id",
        )
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "triggers": r.get::<_, Option<i64>>(1)?,
                "slash": r.get::<_, Option<i64>>(2)?,
            }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (Value::Array(info), Value::String(sql), Value::Array(rows))
}

#[test]
fn the_two_column_ensures_match_v4s_migrations_in_seven_shapes() {
    let (Some(oracle_path), Some(dir)) = (
        std::env::var_os("QT_ORACLE_CS411_ENSURE"),
        std::env::var_os("QT_FIXTURE_CS411_ENSURE_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_CS411_ENSURE and QT_FIXTURE_CS411_ENSURE_DIR (see header).");
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
        let report: Vec<&str> = v4["report"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        // v4's report per mode — the A modes and D RUN, B / C answer `not needed`.
        let expected_ran: &[&str] = match mode.as_str() {
            "T-A" => &[MIGRATION_ID_TRIGGERS],
            "S-A" => &[MIGRATION_ID_SLASH],
            "D" => &[MIGRATION_ID_TRIGGERS, MIGRATION_ID_SLASH],
            "T-B" | "T-C" | "S-B" | "S-C" => &[],
            other => panic!("unknown mode {other}"),
        };
        if expected_ran.is_empty() {
            assert_eq!(report.len(), 1, "[{mode}] {report:?}");
            assert!(report[0].ends_with(" not needed"), "[{mode}] {report:?}");
        } else {
            assert_eq!(report.len(), expected_ran.len(), "[{mode}] {report:?}");
            for (line, id) in report.iter().zip(expected_ran) {
                assert!(line.starts_with(&format!("+RAN {id} ")), "[{mode}] {line}");
            }
        }

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(Path::new(&dir).join(format!("cs411-{mode}.db")), &work).unwrap();
        let w = Writer::open_writable(&work, &pepper).unwrap();
        let added = match mode.as_str() {
            "T-A" | "T-B" | "T-C" => {
                vec![ensure_chat_settings_custom_tool_triggers(w.connection()).expect("ensure")]
            }
            "S-A" | "S-B" | "S-C" => {
                vec![ensure_chat_settings_composer_slash_commands(w.connection()).expect("ensure")]
            }
            _ => {
                run_chat_settings_4_11_ensures(w.connection()).expect("the chain");
                Vec::new()
            }
        };
        if mode != "D" {
            assert_eq!(
                added,
                [mode.ends_with("-A")],
                "[{mode}] the ensure's answer"
            );
        }
        let (info, sql, rows) = snapshot(w.connection());
        assert_eq!(info, v4["tableInfo"], "[{mode}] table_info");
        assert_eq!(sql, v4["sql"], "[{mode}] sqlite_master.sql");
        assert_eq!(rows, v4["rows"], "[{mode}] cells");
        if mode == "D" {
            // D-rev (v5-only): the chain's order, outright.
            let sql = sql.as_str().unwrap();
            assert!(
                sql.ends_with(
                    ", \"customToolTriggers\" INTEGER DEFAULT 0, \"composerSlashCommands\" INTEGER DEFAULT 1)"
                ),
                "{sql}"
            );
        }
        modes.push(mode);
    }
    assert_eq!(
        modes,
        ["T-A", "T-B", "T-C", "S-A", "S-B", "S-C", "D"],
        "every shape ran"
    );
    eprintln!("OK: the two 4.11 chat_settings ensures matched v4 in shapes {modes:?}.");
}
