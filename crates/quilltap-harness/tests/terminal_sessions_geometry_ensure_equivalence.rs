//! P4.D337 — the terminal-geometry boot ensure
//! (`db::terminal_sessions_geometry_repair`) against v4's REAL
//! `add-terminal-session-geometry-v1` (bug 255, v4 `a6292f169` — the
//! FIFTEENTH D23 move), run through v4's REAL `MigrationRunner` narrowed to it,
//! in five shapes (the oracle case `harness/oracle/cases/terminal-sessions-
//! geometry-ensure.ts` builds the base files at the pin from v4's own
//! generateDDL; its header names them): a (both columns absent), b (no
//! table), c (already migrated), d (the generateDDL-current REAL columns),
//! e (only `rows` absent).
//!
//! Comparands per shape, byte-exact: `PRAGMA table_info` (order included —
//! `cols` before `rows` is the point), `sqlite_master.sql` (`null` when the
//! table is absent), every row's `quote()`d `cols` / `rows` and their storage
//! classes (NULL — no backfill), and on (a) and (d) the row each side then
//! writes through its REAL repository create with `cols: 80, rows: 24`: the
//! migration's appended `INTEGER DEFAULT NULL` stores `integer` `80`, the fresh
//! `REAL` column stores `real` `80.0` — the same bind lands as two storage
//! classes, and v5 binds `f64` exactly as v4 binds a JS number. v4's report is
//! asserted per mode (`+RAN` only on a and e), so no mode goes vacuous. The
//! `migrations_state` comparand is the ruled asymmetry
//! `UNSTAMPED_COLUMN_ENSURES` (the P4.D324 / P4.D304 class), pinned BOTH ways:
//! v4's runner stamps the id on a and e, v5 stamps nothing (v4's `shouldRun`
//! is column-gated, so nothing re-runs — not the ping-pong class).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-ts-geom-ensure
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-ts-geom-ensure $N/npx tsx $V5W/harness/oracle/cases/terminal-sessions-geometry-ensure.ts > /tmp/oracle-ts-geom-ensure.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_TS_GEOMETRY_ENSURE=/tmp/oracle-ts-geom-ensure.ndjson QT_FIXTURE_TS_GEOMETRY_ENSURE_DIR=/tmp/qt-ts-geom-ensure cargo test -p quilltap-harness --test terminal_sessions_geometry_ensure_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::terminal_sessions::{CreateOptions, TsCreate};
use quilltap_core::db::terminal_sessions_geometry_repair::{
    ensure_terminal_sessions_geometry_columns, GEOMETRY_MIGRATION_ID,
};
use quilltap_core::db::Writer;
use rusqlite::Connection;
use serde_json::{json, Value};

fn pepper() -> String {
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/terminal-sessions-tier2.json"),
        )
        .unwrap(),
    )
    .unwrap();
    spec["testPepperBase64"].as_str().unwrap().to_string()
}

fn table_sql(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'terminal_sessions'",
        [],
        |r| r.get(0),
    )
    .ok()
}

fn snapshot(conn: &Connection) -> Value {
    let sql = table_sql(conn);
    if sql.is_none() {
        return json!({"tableInfo": [], "sql": null, "rows": []});
    }
    let mut stmt = conn
        .prepare("PRAGMA table_info(\"terminal_sessions\")")
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
    let names: Vec<String> = info
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    let has = |c: &str| names.iter().any(|n| n == c);
    let cell = |c: &str| {
        if has(c) {
            format!("quote(\"{c}\")")
        } else {
            "'absent'".to_string()
        }
    };
    let kind = |c: &str| {
        if has(c) {
            format!("typeof(\"{c}\")")
        } else {
            "'absent'".to_string()
        }
    };
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, {} AS cols, {} AS rows, {} AS colsType, {} AS rowsType \
             FROM terminal_sessions ORDER BY id",
            cell("cols"),
            cell("rows"),
            kind("cols"),
            kind("rows"),
        ))
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "cols": r.get::<_, String>(1)?,
                "rows": r.get::<_, String>(2)?,
                "colsType": r.get::<_, String>(3)?,
                "rowsType": r.get::<_, String>(4)?,
            }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    json!({"tableInfo": info, "sql": sql, "rows": rows})
}

fn ledger(conn: &Connection) -> Vec<String> {
    let has: bool = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
        > 0;
    if !has {
        return Vec::new();
    }
    let mut stmt = conn
        .prepare("SELECT id FROM migrations_state ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// The oracle's `WRITE_ROW`, through v5's real repository create.
fn write_row(w: &Writer) {
    w.terminal_sessions()
        .create(
            &TsCreate {
                chat_id: "00000000-0000-4000-8000-0000000000cc".into(),
                label: None,
                shell: "/bin/zsh".into(),
                cwd: "/tmp".into(),
                started_at: "2026-01-02T00:00:00.000Z".into(),
                exited_at: None,
                exit_code: None,
                transcript_path: None,
                cols: Some(80.0),
                rows: Some(24.0),
            },
            &CreateOptions {
                id: "00000000-0000-4000-8000-0000000000c3".into(),
                created_at: "2026-01-02T00:00:00.000Z".into(),
                updated_at: "2026-01-02T00:00:00.000Z".into(),
            },
        )
        .expect("create");
}

#[test]
fn the_geometry_ensure_matches_v4s_migration_in_five_shapes() {
    let (Some(oracle_path), Some(dir)) = (
        std::env::var_os("QT_ORACLE_TS_GEOMETRY_ENSURE"),
        std::env::var_os("QT_FIXTURE_TS_GEOMETRY_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_TS_GEOMETRY_ENSURE and QT_FIXTURE_TS_GEOMETRY_ENSURE_DIR (see header)."
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
        let report: Vec<&str> = v4["report"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        let runs = matches!(mode.as_str(), "a" | "e");
        assert_eq!(report.len(), 1, "[{mode}] {report:?}");
        if runs {
            assert!(
                report[0].starts_with(&format!("+RAN {GEOMETRY_MIGRATION_ID} ")),
                "[{mode}] {report:?}"
            );
        } else {
            assert!(report[0].contains(" not needed"), "[{mode}] {report:?}");
        }
        // `UNSTAMPED_COLUMN_ENSURES` (v4's side): the runner stamped what it ran.
        let v4_ledger: Vec<&str> = v4["ledger"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let want: &[&str] = if runs { &[GEOMETRY_MIGRATION_ID] } else { &[] };
        assert_eq!(v4_ledger, want, "[{mode}] v4's ledger");

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(Path::new(&dir).join(format!("geom-{mode}.db")), &work).unwrap();
        let w = Writer::open_writable(&work, &pepper).unwrap();
        let sql_before = table_sql(w.connection());
        let added = ensure_terminal_sessions_geometry_columns(w.connection()).expect("ensure");
        let expected_added = match mode.as_str() {
            "a" => 2,
            "e" => 1,
            _ => 0,
        };
        assert_eq!(added, expected_added, "[{mode}] the ensure's answer");
        if !runs {
            assert_eq!(table_sql(w.connection()), sql_before, "[{mode}] touched");
        }
        if matches!(mode.as_str(), "a" | "d") {
            write_row(&w);
        }
        let got = snapshot(w.connection());
        let want = json!({"tableInfo": v4["tableInfo"], "sql": v4["sql"], "rows": v4["rows"]});
        assert_eq!(
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&want).unwrap(),
            "[{mode}]"
        );
        // `UNSTAMPED_COLUMN_ENSURES` (v5's side): nothing stamped, ever.
        assert!(
            ledger(w.connection()).is_empty(),
            "[{mode}] v5 stamped the ledger"
        );
        if mode == "a" {
            let sql = got["sql"].as_str().unwrap();
            assert!(
                sql.ends_with(", \"cols\" INTEGER DEFAULT NULL, \"rows\" INTEGER DEFAULT NULL)"),
                "{sql}"
            );
            assert!(line.contains(r#"\"cols\" INTEGER DEFAULT NULL"#), "marker");
        }
        if mode == "d" {
            assert!(
                got["sql"].as_str().unwrap().contains("\"cols\" REAL,"),
                "marker"
            );
        }
        modes.push(mode);
    }
    assert_eq!(modes, ["a", "b", "c", "d", "e"], "every shape ran");
    eprintln!("OK: the geometry ensure matched v4 in shapes {modes:?}.");
}
