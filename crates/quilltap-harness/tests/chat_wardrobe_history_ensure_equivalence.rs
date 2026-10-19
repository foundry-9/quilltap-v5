//! P4.D315 Tier 1 item 6 — the chat wardrobe history's three boot ensures
//! (`db::chat_wardrobe_history_repair::run_chat_wardrobe_history_ensures`)
//! against v4's REAL migration RUNNER narrowed to
//! `CHAT_WARDROBE_HISTORY_MIGRATIONS` (the ledger gate before `shouldRun`, and
//! `recordCompletedMigration`'s stamps — R-C).
//!
//! Both sides start from the SAME base file per mode, built by the oracle case
//! (`harness/oracle/cases/chat-wardrobe-history-ensure.ts`, whose header names
//! the seven modes A / B / B' / C / D / E / E'). Comparands per mode,
//! byte-exact: `sqlite_master` (type, name, sql) of the table and both UNIQUE
//! indexes — BOTH legal table texts (A / C / D / E the fresh 11-column text; B /
//! B' the pre-column text ALTERed, SQLite's `\n  , "actorToolName" TEXT)`
//! splice); `PRAGMA table_info`; every history row minus `id`, ordered
//! `chatId, characterId, entryIndex` (`createdAt`, the `slots` text,
//! `slotsHash`, `source`, both actor columns, `isCurrent`); `migrations_state`
//! minus `completedAt` / `quilltapVersion`, by id (the seed's `Seeded N outfit
//! record(s) across M chat(s)` bytes and `itemsAffected`). v4's runner report
//! is asserted per mode, so no mode can go vacuous.
//!
//! Mode E' (R-3, a NULL `chats.updatedAt`): v4's runner FAILS the seed
//! (`success: false`, rolled back, unstamped — v4's boot would stop there); v5's
//! seed answers `Failed` and the boot continues (R-2 — the recorded divergence
//! class shared with the wear seed). The table state is compared as usual.
//!
//! v4's logger writes its ERROR lines to stdout beside the NDJSON; only lines
//! starting `{` are read.
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-cwh-ensure
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-cwh-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/chat-wardrobe-history-ensure.ts \
//!     > /tmp/oracle-cwh-ensure.ndjson
//! Run:
//!   QT_ORACLE_CWH_ENSURE=/tmp/oracle-cwh-ensure.ndjson \
//!   QT_FIXTURE_CWH_ENSURE_DIR=/tmp/qt-cwh-ensure \
//!     cargo test -p quilltap-harness --test chat_wardrobe_history_ensure_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::chat_wardrobe_history_repair::{
    run_chat_wardrobe_history_ensures, ChatWardrobeHistoryEnsureOutcome, HistorySeedOutcome,
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

/// Every column of every row, by name, as v4's `.all()` objects carry them.
fn query(conn: &Connection, sql: &str) -> Vec<Value> {
    let mut stmt = conn.prepare(sql).unwrap();
    let cols: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    stmt.query_map([], |r| {
        let mut o = serde_json::Map::new();
        for (i, c) in cols.iter().enumerate() {
            let v = match r.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(x) => json!(x),
                rusqlite::types::ValueRef::Real(x) => json!(x),
                rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                rusqlite::types::ValueRef::Blob(_) => Value::Null,
            };
            o.insert(c.clone(), v);
        }
        Ok(Value::Object(o))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

/// `(v4 success, run, skipped, v5's outcome)` per mode.
fn expected(mode: &str) -> (bool, i64, i64, ChatWardrobeHistoryEnsureOutcome) {
    let seeded = |entries, chats_scanned, pairs_skipped| HistorySeedOutcome::Seeded {
        entries,
        chats_scanned,
        pairs_skipped,
    };
    let o = |table_created, column_added, seed| ChatWardrobeHistoryEnsureOutcome {
        table_created,
        column_added,
        seed,
    };
    match mode {
        "A" => (true, 2, 1, o(true, false, seeded(3, 2, 0))),
        "B" => (true, 2, 1, o(false, true, seeded(2, 2, 1))),
        "B'" => (
            true,
            1,
            2,
            o(false, true, HistorySeedOutcome::AlreadyCompleted),
        ),
        "C" => (
            true,
            0,
            3,
            o(false, false, HistorySeedOutcome::AlreadyCompleted),
        ),
        "D" => (true, 1, 2, o(false, false, seeded(2, 2, 1))),
        "E" => (true, 2, 1, o(true, false, seeded(7, 10, 0))),
        "E'" => (false, 1, 1, o(true, false, HistorySeedOutcome::Failed)),
        other => panic!("unknown mode {other}"),
    }
}

#[test]
fn the_history_ensures_match_v4s_runner_in_seven_modes() {
    let (Some(oracle_path), Some(dir)) = (
        std::env::var_os("QT_ORACLE_CWH_ENSURE"),
        std::env::var_os("QT_FIXTURE_CWH_ENSURE_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_CWH_ENSURE and QT_FIXTURE_CWH_ENSURE_DIR (see header).");
        return;
    };
    let pepper = pepper();
    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let mut modes = Vec::new();
    for line in text.lines().filter(|l| l.starts_with('{')) {
        let v4: Value = serde_json::from_str(line).unwrap();
        let mode = v4["mode"].as_str().unwrap().to_string();
        let (success, run, skipped, want) = expected(&mode);
        assert_eq!(
            (
                v4["report"]["success"].as_bool(),
                v4["report"]["run"].as_i64(),
                v4["report"]["skipped"].as_i64()
            ),
            (Some(success), Some(run), Some(skipped)),
            "[{mode}] v4's runner report: {}",
            v4["report"]
        );

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(Path::new(&dir).join(v4["file"].as_str().unwrap()), &work).unwrap();
        let w = Writer::open_writable(&work, &pepper).unwrap();
        let conn = w.connection();
        let outcome = run_chat_wardrobe_history_ensures(conn).expect("the ensures");
        assert_eq!(outcome, want, "[{mode}] the ensures' outcome");

        let master = query(
            conn,
            "SELECT type, name, sql FROM sqlite_master WHERE tbl_name = 'chat_wardrobe_history' \
             AND sql IS NOT NULL ORDER BY name",
        );
        assert_eq!(master.len(), 3, "[{mode}] the table + both indexes");
        assert_eq!(Value::Array(master), v4["master"], "[{mode}] sqlite_master");
        let info = query(conn, "PRAGMA table_info(\"chat_wardrobe_history\")");
        assert_eq!(Value::Array(info), v4["tableInfo"], "[{mode}] table_info");
        let rows: Vec<Value> = query(
            conn,
            "SELECT * FROM \"chat_wardrobe_history\" ORDER BY \"chatId\", \"characterId\", \"entryIndex\"",
        )
        .into_iter()
        .map(|mut r| {
            r.as_object_mut().unwrap().remove("id");
            r
        })
        .collect();
        assert!(
            mode == "E'" || !rows.is_empty(),
            "[{mode}] every other mode holds rows"
        );
        assert_eq!(Value::Array(rows), v4["rows"], "[{mode}] history rows");
        assert!(table_exists(conn, "migrations_state"));
        let ledger = query(
            conn,
            "SELECT \"id\", \"itemsAffected\", \"message\" FROM \"migrations_state\" ORDER BY \"id\"",
        );
        assert_eq!(
            Value::Array(ledger),
            v4["ledger"],
            "[{mode}] migrations_state"
        );
        modes.push(mode);
    }
    assert_eq!(
        modes,
        ["A", "B", "B'", "C", "D", "E", "E'"],
        "every mode ran"
    );
    eprintln!("OK: the chat wardrobe history ensures matched v4's runner in modes {modes:?}.");
}
