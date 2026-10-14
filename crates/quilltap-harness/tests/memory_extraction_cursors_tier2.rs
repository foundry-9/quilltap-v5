//! P4.D288 unit 1 — tier-2 differential: the memory extraction cursor
//! repository (`db::memory_extraction_cursors`, contract C1-1) against v4's
//! REAL `MemoryExtractionCursorsRepository` at the `83334b9a5` pin.
//!
//! The oracle case (`harness/oracle/cases/memory-extraction-cursors-tier2.ts`)
//! builds TWO base files — the table in v4's MIGRATION shape (v4's real
//! `add-memory-extraction-cursors-v1`, then the repository pass's
//! `ensureCollection`) and in generateDDL's BARE shape — and runs the seed's
//! `cursorOps` (`fixtures/memory-keystone-tier2.json`) over a copy of each.
//! This test runs the SAME ops through the Rust port over its own copy of the
//! SAME base files and compares, per shape and per op:
//!
//!   - the RESULT (a map compared as a map — v4's key order follows the query
//!     plan, measured: index order on the migration shape, rowid on the bare
//!     one — and v5's struct carries no order; `findAllByChat`'s CHAT order is
//!     compared); a thrown op is `{threw: <bare message>}`;
//!   - the `dump` ops' rows (stamps compared only when they are the seed's
//!     pinned `2020-…` value, plus whether a row's two stamps are equal);
//!   - every line an op marked `captureLogs` logged — ERROR / WARN / INFO /
//!     DEBUG — as whole strings, field order and the bare `error` included
//!     (v4's backend `SQLite <op> error` lines are unported by standing
//!     convention and dropped from the oracle's side). The silence of the
//!     uncaptured ops is not compared here; the repository emits nothing but
//!     the lines below.
//!
//! What the corpus proves, by shape: the upsert keeps `createdAt` and stamps
//! one `now`; three passes per chat; the R-X raw arm writes a `<chatId>:<raw>`
//! row every typed read DROPS in Zod (`Data validation failed` + `Safe
//! validation failed`, or `+ Error finding entity by ID` on `findCursor`);
//! a non-UUID `chatId` row dropped the same way; the clears' INFO + DEBUG; the
//! FK cascade on a chat delete (migration) and its absence (bare); a cursor for
//! an absent chat REFUSED (migration, `FOREIGN KEY constraint failed`) and
//! written (bare); then every verb over a DROPPED table, outside and inside the
//! strict scope — the base read's fallback line alone outside, the base line +
//! the method's own RETHROW line (both `strictFailures=true`) and a throw
//! inside; `setCursor`'s RETHROW; the clears' base `Error deleting entities` +
//! their FALLBACK line.
//!
//! The base files' DDL is also pinned: [`MIGRATION_TABLE_DDL`] and
//! [`CREATED_AT_INDEX_DDL`] are byte-equal to the migration base's
//! `sqlite_master.sql` (v4's real migration text).
//!
//! Regenerate + run:
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-cursors-tier2 /tmp/oracle-cursors-tier2.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-cursors-tier2 \
//!     $N/npx tsx $V5W/harness/oracle/cases/memory-extraction-cursors-tier2.ts \
//!     > /tmp/oracle-cursors-tier2.ndjson
//!   cd $V5W
//!   QT_ORACLE_CURSORS_TIER2=/tmp/oracle-cursors-tier2.ndjson \
//!   QT_FIXTURE_CURSORS_TIER2_DIR=/tmp/qt-cursors-tier2 \
//!     cargo test -p quilltap-harness --test memory_extraction_cursors_tier2 -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::fallback::{error_text, with_strict_repository_failures};
use quilltap_core::db::memory_extraction_cursors::{
    ExtractionCursorPass, MemoryExtractionCursorsRepository, CREATED_AT_INDEX_DDL,
    MIGRATION_TABLE_DDL,
};
use quilltap_core::db::{DbError, Writer};
use serde::Deserialize;
use serde_json::{json, Value};

const PINNED_TS: &str = "2020-01-01T00:00:00.000Z";

/// v4's backend lines, unported by standing convention (the chat-informs
/// family's `UNPORTED_BACKEND_LINES`).
const UNPORTED_BACKEND_LINES: &[&str] = &[
    "SQLite find error",
    "SQLite findOne error",
    "SQLite deleteMany error",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Seed {
    test_pepper_base64: String,
    cursor_ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Op {
    kind: String,
    label: String,
    chat_id: Option<String>,
    pass: Option<String>,
    message_id: Option<String>,
    sql: Option<String>,
    #[serde(default)]
    capture_logs: bool,
    #[serde(default)]
    strict: bool,
}

fn seed_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-keystone-tier2.json")
}

/// One v4 line as the oracle recorded it, rendered the way v5's capture rig
/// renders a `quilltap::db` event.
fn v4_line(line: &Value) -> String {
    let level = line["level"].as_str().expect("level").to_ascii_uppercase();
    let mut out = format!(
        "{level} quilltap::db {}",
        line["message"].as_str().expect("message")
    );
    for pair in line["fields"].as_array().expect("fields") {
        let key = pair[0].as_str().expect("field key");
        let value = match &pair[1] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {key}={value}"));
    }
    out
}

fn pass(op: &Op) -> ExtractionCursorPass {
    ExtractionCursorPass::parse(op.pass.as_deref().expect("pass")).expect("a typed op's pass")
}

fn map_json(m: &quilltap_core::db::memory_extraction_cursors::ExtractionCursorMap) -> Value {
    m.to_json()
}

fn dump(writer: &Writer) -> Value {
    let mut stmt = writer
        .connection()
        .prepare(
            "SELECT \"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\" \
             FROM \"memory_extraction_cursors\" ORDER BY rowid",
        )
        .expect("dump");
    let rows = stmt
        .query_map([], |r| {
            let created: String = r.get(4)?;
            let updated: String = r.get(5)?;
            let stamp = |s: &str| {
                if s == PINNED_TS {
                    s.to_string()
                } else {
                    "<ts>".into()
                }
            };
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "chatId": r.get::<_, String>(1)?,
                "pass": r.get::<_, String>(2)?,
                "messageId": r.get::<_, String>(3)?,
                "createdAt": stamp(&created),
                "updatedAt": stamp(&updated),
                "sameStamp": created == updated,
            }))
        })
        .expect("dump rows")
        .map(Result::unwrap)
        .collect();
    Value::Array(rows)
}

fn run_op(writer: &Writer, op: &Op) -> Result<Value, DbError> {
    let repo = MemoryExtractionCursorsRepository::new(writer.connection());
    let chat = || op.chat_id.as_deref().expect("chatId");
    Ok(match op.kind.as_str() {
        "execSql" => {
            writer
                .connection()
                .execute_batch(op.sql.as_deref().expect("sql"))
                .expect("execSql");
            Value::Null
        }
        "deleteChat" => {
            writer
                .connection()
                .execute("DELETE FROM chats WHERE id = ?1", [chat()])
                .expect("deleteChat");
            Value::Null
        }
        "dump" => dump(writer),
        "findByChatId" => map_json(&repo.find_by_chat_id(chat())?),
        "findCursor" => json!(repo.find_cursor(chat(), pass(op))?),
        "findCursorRaw" => json!(repo.find_cursor_raw(chat(), op.pass.as_deref().unwrap())?),
        "findAllByChat" => Value::Array(
            repo.find_all_by_chat()?
                .iter()
                .map(|(id, m)| json!([id, map_json(m)]))
                .collect(),
        ),
        "setCursor" => {
            repo.set_cursor(chat(), pass(op), op.message_id.as_deref().unwrap())?;
            Value::Null
        }
        "setCursorRaw" => {
            repo.set_cursor_raw(
                chat(),
                op.pass.as_deref().unwrap(),
                op.message_id.as_deref().unwrap(),
            )?;
            Value::Null
        }
        "clearCursor" => json!(repo.clear_cursor(chat(), pass(op))?),
        "clearForChat" => json!(repo.clear_for_chat(chat())?),
        other => panic!("unknown op kind: {other}"),
    })
}

#[test]
fn memory_extraction_cursors_match_v4() {
    let (Ok(oracle_path), Ok(base_dir)) = (
        std::env::var("QT_ORACLE_CURSORS_TIER2"),
        std::env::var("QT_FIXTURE_CURSORS_TIER2_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_CURSORS_TIER2 + QT_FIXTURE_CURSORS_TIER2_DIR (see header).");
        return;
    };
    let seed: Seed =
        serde_json::from_str(&std::fs::read_to_string(seed_path()).expect("seed")).expect("seed");
    let oracle = std::fs::read_to_string(&oracle_path).expect("oracle");
    let shapes: Vec<Value> = oracle
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert_eq!(shapes.len(), 2, "one oracle line per table shape");

    let mut failures: Vec<String> = Vec::new();
    let mut compared_ops = 0usize;
    let mut compared_lines = 0usize;
    for want in &shapes {
        let shape = want["shape"].as_str().expect("shape");
        // The base's DDL (v4's real migration text, the repository pass's index).
        let master = want["master"].as_array().expect("master");
        let sql_of = |name: &str| {
            master
                .iter()
                .find(|m| m["name"] == name)
                .and_then(|m| m["sql"].as_str())
                .map(str::to_string)
        };
        if shape == "migration" {
            assert_eq!(
                sql_of("memory_extraction_cursors").as_deref(),
                Some(MIGRATION_TABLE_DDL),
                "MIGRATION_TABLE_DDL vs v4's real migration text"
            );
            assert!(
                master
                    .iter()
                    .any(|m| m["name"] == "sqlite_autoindex_memory_extraction_cursors_2"),
                "the migration shape carries the UNIQUE autoindex"
            );
        }
        assert_eq!(
            sql_of("idx_memory_extraction_cursors_createdAt").as_deref(),
            Some(CREATED_AT_INDEX_DDL),
            "{shape}: the repository pass's createdAt index"
        );

        let scratch = tempfile::tempdir().expect("tempdir");
        let work = scratch.path().join("work.db");
        std::fs::copy(
            Path::new(&base_dir).join(format!("cursors-{shape}.db")),
            &work,
        )
        .expect("copy base");
        let writer = Writer::open_writable(&work, &seed.test_pepper_base64).expect("open base");

        let results = want["results"].as_array().expect("results");
        assert_eq!(results.len(), seed.cursor_ops.len(), "{shape}: op count");
        for (op, w) in seed.cursor_ops.iter().zip(results) {
            assert_eq!(w["label"], op.label, "{shape}: op order");
            let (got, lines) = quilltap_core::test_support::captured_with(|| {
                let run = || run_op(&writer, op);
                let out = if op.strict {
                    with_strict_repository_failures(run)
                } else {
                    run()
                };
                out.unwrap_or_else(|e| json!({ "threw": error_text(&e) }))
            });
            compared_ops += 1;
            if got != w["result"] {
                failures.push(format!(
                    "{shape} `{}`: result\n  rust:   {got}\n  oracle: {}",
                    op.label, w["result"]
                ));
            }
            if op.capture_logs {
                let want_lines: Vec<String> = w["logs"]
                    .as_array()
                    .expect("captured logs")
                    .iter()
                    .filter(|l| {
                        !UNPORTED_BACKEND_LINES.contains(&l["message"].as_str().unwrap_or(""))
                    })
                    .map(v4_line)
                    .collect();
                let got_lines: Vec<String> = lines
                    .into_iter()
                    .filter(|l| l.contains(" quilltap::db "))
                    .collect();
                compared_lines += want_lines.len();
                if got_lines != want_lines {
                    failures.push(format!(
                        "{shape} `{}`: logged lines\n  rust:   {got_lines:#?}\n  oracle: {want_lines:#?}",
                        op.label
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("cursor tier-2: {compared_ops} ops, {compared_lines} captured lines — all agree");
}
