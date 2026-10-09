//! P4.D265 tier-2 differential: v5's memory-tiers boot ensure
//! (`db::memory_tiers_repair::ensure_memory_tiers`) against v4's REAL
//! `add-memory-tiers-v1` (`d58548051`) over the SAME base files, in six modes
//! (pre-round; no tables; already migrated + v4's ledger row; generateDDL-
//! current with planted NULLs and no index; no index; only `chats` behind) —
//! plus the two HELPER-PARITY arms of C1 §13: v4's
//! `apply-memory-tiers-migration.ts` (the narrowed REAL `MigrationRunner`) vs
//! `test_support::ensure_memory_tiers_columns`, over an ENCRYPTED and a PLAIN
//! copy of the pre-round shape (the encrypted one must still open under the
//! test pepper afterwards).
//!
//! Comparands are EFFECTS (v4's migration lines are NO-PORT, R-J): `PRAGMA
//! table_info` of both tables (order and the migration's appended `DEFAULT
//! NULL` text included), `sqlite_master.sql` of both tables and the index, every
//! memory's `(id, tier, consolidatedFrom)` and every chat's watermark; v4's
//! `itemsAffected` (from its report) against v5's outcome count.
//!
//! Regenerate + run (the case builds the base files):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-memory-tiers-ensure /tmp/oracle-memory-tiers-ensure.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MEMORY_TIERS_ENSURE_DIR=/tmp/qt-memory-tiers-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/memory-tiers-ensure.ts \
//!     > /tmp/oracle-memory-tiers-ensure.ndjson
//!   cd $V5W
//!   QT_ORACLE_MEMORY_TIERS_ENSURE=/tmp/oracle-memory-tiers-ensure.ndjson \
//!   QT_FIXTURE_MEMORY_TIERS_ENSURE_DIR=/tmp/qt-memory-tiers-ensure \
//!     cargo test -p quilltap-harness --test memory_tiers_ensure_equivalence -- --nocapture

use quilltap_core::db::memory_tiers_repair::ensure_memory_tiers;
use rusqlite::Connection;
use serde_json::{json, Map, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn open(path: &std::path::Path, encrypted: bool) -> Connection {
    let conn = Connection::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    if encrypted {
        let hex = quilltap_core::dbkey::pepper_b64_to_key_hex(PEPPER).expect("pepper");
        conn.execute_batch(&format!("PRAGMA key = \"x'{hex}'\";"))
            .expect("key");
    }
    conn
}

fn table_info(conn: &Connection, table: &str) -> Value {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            let mut m = Map::new();
            m.insert("cid".into(), json!(r.get::<_, i64>(0)?));
            m.insert("name".into(), json!(r.get::<_, String>(1)?));
            m.insert("type".into(), json!(r.get::<_, String>(2)?));
            m.insert("notnull".into(), json!(r.get::<_, i64>(3)?));
            m.insert("dflt_value".into(), json!(r.get::<_, Option<String>>(4)?));
            m.insert("pk".into(), json!(r.get::<_, i64>(5)?));
            Ok(Value::Object(m))
        })
        .unwrap();
    Value::Array(rows.map(Result::unwrap).collect())
}

fn sql_of(conn: &Connection, name: &str) -> Value {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE name = ?1",
        [name],
        |r| r.get::<_, String>(0),
    )
    .map(Value::String)
    .unwrap_or(Value::Null)
}

fn has_col(conn: &Connection, table: &str, col: &str) -> bool {
    table_info(conn, table)
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["name"] == col)
}

fn state(conn: &Connection) -> Value {
    let has = |t: &str| {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [t],
            |_| Ok(()),
        )
        .is_ok()
    };
    let memory_rows = if has("memories") {
        let sel = if has_col(conn, "memories", "tier") {
            "SELECT id, tier, consolidatedFrom FROM memories ORDER BY id"
        } else {
            "SELECT id, NULL, NULL FROM memories ORDER BY id"
        };
        let mut stmt = conn.prepare(sel).unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "tier": r.get::<_, Option<String>>(1)?,
                    "consolidatedFrom": r.get::<_, Option<String>>(2)?,
                }))
            })
            .unwrap();
        Value::Array(rows.map(Result::unwrap).collect())
    } else {
        json!([])
    };
    let chat_rows = if has("chats") {
        let sel = if has_col(conn, "chats", "otherExtractionWatermarkMessageId") {
            "SELECT id, otherExtractionWatermarkMessageId FROM chats ORDER BY id"
        } else {
            "SELECT id, NULL FROM chats ORDER BY id"
        };
        let mut stmt = conn.prepare(sel).unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "watermark": r.get::<_, Option<String>>(1)?,
                }))
            })
            .unwrap();
        Value::Array(rows.map(Result::unwrap).collect())
    } else {
        json!([])
    };
    json!({
        "memoriesInfo": if has("memories") { table_info(conn, "memories") } else { json!([]) },
        "chatsInfo": if has("chats") { table_info(conn, "chats") } else { json!([]) },
        "memoriesSql": sql_of(conn, "memories"),
        "chatsSql": sql_of(conn, "chats"),
        "indexSql": sql_of(conn, "idx_memories_character_tier"),
        "memoryRows": memory_rows,
        "chatRows": chat_rows,
    })
}

/// v4's `itemsAffected` from its report (`… (Memory tiers in place (N changes))`),
/// or 0 for `not needed`.
fn items_affected(report: &Value) -> usize {
    let text = match report {
        Value::Array(a) => a.first().and_then(Value::as_str).unwrap_or("").to_string(),
        Value::String(s) => s.clone(),
        _ => String::new(),
    };
    text.find("in place (")
        .map(|at| {
            let rest = &text[at + "in place (".len()..];
            rest[..rest.find(' ').unwrap()].parse().unwrap()
        })
        .unwrap_or(0)
}

#[test]
fn memory_tiers_ensure_matches_v4s_migration() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MEMORY_TIERS_ENSURE") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_TIERS_ENSURE to the oracle NDJSON (see header).");
        return;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_MEMORY_TIERS_ENSURE_DIR") else {
        eprintln!(
            "SKIP: set QT_FIXTURE_MEMORY_TIERS_ENSURE_DIR to the case's base files (header)."
        );
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let mut modes = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line");
        let mode = row["mode"].as_str().unwrap().to_string();
        let encrypted = row["encrypted"].as_bool().unwrap();
        let scratch = tempfile::Builder::new()
            .prefix("qt-mte-rust-")
            .tempdir()
            .unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(std::path::Path::new(&dir).join(format!("{mode}.db")), &work)
            .unwrap_or_else(|e| panic!("copy {mode}: {e}"));
        let conn = open(&work, encrypted);
        let affected = if mode.starts_with("parity") {
            let before = state(&conn);
            quilltap_core::test_support::ensure_memory_tiers_columns(&conn);
            assert_ne!(state(&conn), before, "{mode}: the helper changed nothing");
            None
        } else {
            Some(ensure_memory_tiers(&conn).unwrap_or_else(|e| panic!("{mode}: {e}")))
        };
        let got = state(&conn);
        drop(conn);
        for key in [
            "memoriesInfo",
            "chatsInfo",
            "memoriesSql",
            "chatsSql",
            "indexSql",
            "memoryRows",
            "chatRows",
        ] {
            assert_eq!(
                serde_json::to_string(&got[key]).unwrap(),
                serde_json::to_string(&row[key]).unwrap(),
                "mode {mode}: {key}"
            );
        }
        if let Some(outcome) = affected {
            assert_eq!(
                outcome.items_affected(),
                items_affected(&row["report"]),
                "mode {mode}: v5's outcome {outcome:?} vs v4's report {}",
                row["report"]
            );
        }
        if mode == "parity-encrypted" {
            assert_eq!(
                row["opensAfter"],
                json!(true),
                "v4's helper left the file keyed"
            );
            let reopened = open(&work, true);
            reopened
                .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0))
                .expect("v5's healed encrypted copy still opens under the pepper");
        }
        modes.push(mode);
    }
    assert_eq!(
        modes,
        [
            "a",
            "b",
            "c",
            "d",
            "e",
            "f",
            "parity-encrypted",
            "parity-plain"
        ],
        "the eight arms"
    );
    eprintln!(
        "OK: memory tiers ensure matched v4's migration ({} arms).",
        modes.len()
    );
}
