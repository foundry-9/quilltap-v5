//! Tier-2 differential: v5's `chat_informs.permanent` boot ensure against v4's
//! REAL `add-chat-informs-permanent-v1` migration (P4.D249, v4 `52d6e7ecd`) in
//! FOUR starting shapes (P4.151 B3 — P4.D251's multi-mode template; before
//! this the family was baseline-only and the other three arms of v4's
//! `shouldRun` were unit-pinned, never oracle-compared).
//!
//! Both sides start from the SAME base file per mode, which the oracle case
//! builds at the target pin from v4's OWN generateDDL (with the one
//! `permanent` line removed for the pre-`52d6e7ecd` shape — the D23 re-dump #4
//! moved that line alone):
//!
//!   (a) BASELINE — the pre-`52d6e7ecd` table, three rows; v4 RUNS;
//!   (b) NO TABLE — v4's table gate answers `not needed`; neither side creates it;
//!   (c) ALREADY MIGRATED — (a)'s file after one v4 run; the column gate answers
//!       `not needed`, nothing moves;
//!   (d) GENERATEDDL-CURRENT — the schema-order nullable `permanent` a fresh v4
//!       creates, with ONE row's cell set to NULL after the seed (P4.157 R-D);
//!       `not needed`, left alone — the NULL stays NULL on both sides (v4
//!       MEASURED at `94fbb1ae3`: no backfill, no COALESCE).
//!
//! The oracle runs v4's own migration module through
//! `harness/oracle/lib/v4-migrations.ts` on a copy; this test runs
//! `ensure_chat_informs_permanent_column` on its own copy. Three comparands per
//! mode, each byte-exact: `PRAGMA table_info` (name / type / notnull / default /
//! pk, column order included — the ALTER appends), the table's
//! `sqlite_master.sql` (`null` when absent), and every row's stored
//! `permanent` (no backfill: every existing row 0, and (d)'s planted NULL
//! read back as `null`). v4's report is asserted per
//! mode (`+RAN` only in (a)), so a mode cannot silently become vacuous.
//!
//! Generate (Node 24, from the TARGET-pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-inform-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/chat-informs-permanent-ensure.ts \
//!     > /tmp/oracle-inform-ensure.ndjson
//! Run:
//!   QT_ORACLE_INFORM_ENSURE=/tmp/oracle-inform-ensure.ndjson \
//!   QT_FIXTURE_INFORM_ENSURE_DIR=/tmp/qt-inform-ensure \
//!     cargo test -p quilltap-harness --test chat_informs_permanent_ensure_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::chat_informs_permanent_repair::ensure_chat_informs_permanent_column;
use quilltap_core::db::Writer;
use serde_json::{json, Value};

fn spec_pepper() -> String {
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chat-informs-tier2.json"),
        )
        .unwrap(),
    )
    .unwrap();
    spec["testPepperBase64"].as_str().unwrap().to_string()
}

fn snapshot(conn: &rusqlite::Connection) -> (Value, Value, Value) {
    let mut stmt = conn.prepare("PRAGMA table_info(\"chat_informs\")").unwrap();
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
        .map(|r| r.unwrap())
        .collect();
    let sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'chat_informs'",
            [],
            |r| r.get(0),
        )
        .ok();
    let rows: Vec<Value> = if sql.is_some() {
        let mut stmt = conn
            .prepare("SELECT id, permanent FROM chat_informs ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| {
            // `Option` — mode (d)'s nullable column carries a NULL cell
            // (P4.157 R-D); an `i64` read panicked on it.
            Ok(json!({ "id": r.get::<_, String>(0)?, "permanent": r.get::<_, Option<i64>>(1)? }))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    } else {
        Vec::new()
    };
    (Value::Array(info), json!(sql), Value::Array(rows))
}

/// v4's report per mode: the migration RUNS only on the baseline shape.
fn expected_report(mode: &str) -> Value {
    match mode {
        "a" => {
            json!(["+RAN add-chat-informs-permanent-v1 (Added 1 column(s) to chat_informs table)"])
        }
        "b" | "c" | "d" => json!(["add-chat-informs-permanent-v1 not needed"]),
        other => panic!("unknown mode {other}"),
    }
}

#[test]
fn the_boot_ensure_matches_v4s_migration_in_four_starting_shapes() {
    let (Ok(oracle_path), Ok(fixture_dir)) = (
        std::env::var("QT_ORACLE_INFORM_ENSURE"),
        std::env::var("QT_FIXTURE_INFORM_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_INFORM_ENSURE and QT_FIXTURE_INFORM_ENSURE_DIR (see header)."
        );
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("NDJSON line"))
        .collect();
    assert_eq!(lines.len(), 4, "four modes, one line each");

    let mut seen = Vec::new();
    for oracle in &lines {
        let mode = oracle["mode"].as_str().unwrap();
        seen.push(mode.to_string());
        assert_eq!(
            oracle["report"],
            expected_report(mode),
            "[{mode}] v4's report — a mode that changed verdict is a vacuous diff"
        );

        let fixture = Path::new(&fixture_dir).join(format!("inform-ensure-{mode}.db"));
        let scratch_dir = tempfile::Builder::new()
            .prefix(&format!("qt-inform-ensure-{mode}-"))
            .tempdir()
            .expect("tempdir");
        let work = scratch_dir.path().join("qt-inform-ensure.db");
        std::fs::copy(&fixture, &work).unwrap();
        let writer = Writer::open_writable(&work, &spec_pepper()).unwrap();
        let conn = writer.connection();
        let before = column_names(conn);
        match mode {
            "a" => assert!(
                !before.is_empty() && !before.iter().any(|c| c == "permanent"),
                "[a] the baseline shape"
            ),
            "b" => assert!(before.is_empty(), "[b] no chat_informs table"),
            "c" => assert_eq!(
                before.last().map(String::as_str),
                Some("permanent"),
                "[c] already migrated (the ALTER appended it)"
            ),
            "d" => assert!(
                before.iter().any(|c| c == "permanent")
                    && before.last().map(String::as_str) != Some("permanent"),
                "[d] generateDDL-current (permanent in schema order)"
            ),
            _ => unreachable!(),
        }
        // (a) has no `permanent` to select yet; the other three must not move.
        let unchanged = (mode != "a").then(|| snapshot(conn));

        ensure_chat_informs_permanent_column(conn).unwrap();
        let (info, sql, rows) = snapshot(conn);
        drop(writer);
        let _ = std::fs::remove_file(&work);

        assert_eq!(info, oracle["tableInfo"], "[{mode}] PRAGMA table_info");
        assert_eq!(sql, oracle["sql"], "[{mode}] sqlite_master.sql");
        assert_eq!(
            rows, oracle["rows"],
            "[{mode}] stored permanent per row (no backfill)"
        );
        if mode == "d" {
            // The planted NULL survives on BOTH sides — the ensure neither
            // backfills nor rewrites it (P4.157 R-D, measured on v4).
            let nulls = |v: &Value| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .filter(|r| r["permanent"].is_null())
                    .count()
            };
            assert_eq!(
                (nulls(&rows), nulls(&oracle["rows"])),
                (1, 1),
                "[d] the planted NULL `permanent`"
            );
        }
        if let Some(before) = unchanged {
            assert_eq!(
                before,
                (info.clone(), sql.clone(), rows.clone()),
                "[{mode}] v4 answers `not needed`: v5's ensure must leave the file alone"
            );
        }
        eprintln!(
            "OK [{mode}]: the boot ensure matches v4's migration ({} columns, {} rows).",
            info.as_array().unwrap().len(),
            rows.as_array().unwrap().len()
        );
    }
    seen.sort();
    assert_eq!(
        seen,
        ["a", "b", "c", "d"],
        "every mode present in the oracle"
    );
}

/// The pre-ensure column list (empty when the table is absent).
fn column_names(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn.prepare("PRAGMA table_info(\"chat_informs\")").unwrap();
    stmt.query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}
