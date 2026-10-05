//! Tier-2 differential: v5's `chat_informs.permanent` boot ensure against v4's
//! REAL `add-chat-informs-permanent-v1` migration (P4.D249, v4 `52d6e7ecd`).
//!
//! Both sides start from the SAME baseline-shape file, which the oracle case
//! builds at the target pin from v4's OWN generateDDL with the one `permanent`
//! line removed (the pre-`52d6e7ecd` statement byte for byte — the D23
//! re-dump #4 moved that line alone). The oracle then runs v4's own migration
//! module through `harness/oracle/lib/v4-migrations.ts` on a copy; this test
//! runs `ensure_chat_informs_permanent_column` on its own copy. Three
//! comparands, each byte-exact: `PRAGMA table_info` (name / type / notnull /
//! default / pk, column order included — the ALTER appends), the table's
//! `sqlite_master.sql` (the text SQLite stores for an ALTERed table), and every
//! row's stored `permanent` (no backfill: every existing row 0).
//!
//! Generate (Node 24, from the TARGET-pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-inform-ensure-base.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-informs-permanent-ensure.ts \
//!     > /tmp/oracle-inform-ensure.ndjson
//! Run:
//!   QT_ORACLE_INFORM_ENSURE=/tmp/oracle-inform-ensure.ndjson \
//!   QT_FIXTURE_INFORM_ENSURE=/tmp/qt-inform-ensure-base.db \
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
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'chat_informs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut stmt = conn
        .prepare("SELECT id, permanent FROM chat_informs ORDER BY id")
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({ "id": r.get::<_, String>(0)?, "permanent": r.get::<_, i64>(1)? }))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    (Value::Array(info), Value::String(sql), Value::Array(rows))
}

#[test]
fn the_boot_ensure_matches_v4s_migration() {
    let (Ok(oracle_path), Ok(fixture)) = (
        std::env::var("QT_ORACLE_INFORM_ENSURE"),
        std::env::var("QT_FIXTURE_INFORM_ENSURE"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_INFORM_ENSURE and QT_FIXTURE_INFORM_ENSURE (see header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    let oracle: Value = serde_json::from_str(text.trim()).expect("one NDJSON line");
    assert_eq!(
        oracle["report"],
        json!(["+RAN add-chat-informs-permanent-v1 (Added 1 column(s) to chat_informs table)"]),
        "the oracle must have RUN the migration (a baseline-shape input), or the diff is vacuous"
    );

    let scratch_dir = tempfile::Builder::new()
        .prefix("qt-inform-ensure-")
        .tempdir()
        .expect("tempdir");
    let work = scratch_dir.path().join("qt-inform-ensure.db");
    std::fs::copy(&fixture, &work).unwrap();
    let writer = Writer::open_writable(&work, &spec_pepper()).unwrap();
    let conn = writer.connection();
    assert!(
        !column_names(conn).iter().any(|c| c == "permanent"),
        "the input must be the baseline shape"
    );

    ensure_chat_informs_permanent_column(conn).unwrap();
    let (info, sql, rows) = snapshot(conn);
    drop(writer);
    let _ = std::fs::remove_file(&work);

    assert_eq!(info, oracle["tableInfo"], "PRAGMA table_info");
    assert_eq!(sql, oracle["sql"], "sqlite_master.sql");
    assert_eq!(
        rows, oracle["rows"],
        "stored permanent per row (no backfill)"
    );
    eprintln!(
        "OK: the boot ensure matches v4's migration ({} columns, {} rows).",
        info.as_array().unwrap().len(),
        rows.as_array().unwrap().len()
    );
}

/// The pre-ensure column list (there is no `permanent` to select yet).
fn column_names(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn.prepare("PRAGMA table_info(\"chat_informs\")").unwrap();
    stmt.query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}
