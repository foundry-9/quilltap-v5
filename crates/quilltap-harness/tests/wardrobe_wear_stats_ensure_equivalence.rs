//! P4.D255 Tier 2 item 19 — the wardrobe wear ledger's boot ensure
//! (`db::wardrobe_wear_stats_repair::ensure_wardrobe_wear_stats`) against v4's
//! REAL migration RUNNER over `add-wardrobe-wear-stats-table-v1` +
//! `seed-wardrobe-wear-stats-v1` (the ledger gate before `shouldRun`, and
//! `recordCompletedMigration`'s stamps — R-B's handshake).
//!
//! Both sides start from the SAME base file per mode, built by the oracle case
//! (`harness/oracle/cases/wardrobe-wear-stats-ensure.ts`, its header names the
//! four modes). Comparands per mode, byte-exact: `PRAGMA table_info` of the
//! ledger table; `sqlite_master.sql` of the table and both hand indexes; every
//! ledger row minus `id` / `createdAt` / `updatedAt` in rowid order; the
//! `migrations_state` rows minus `completedAt` / `quilltapVersion` (v4's
//! `Credited N wear(s) across M chat(s)` bytes compared). v4's runner report is
//! asserted per mode, so no mode can go vacuous.
//!
//! One v5-only object is carved, by name: `idx_wardrobe_wear_stats_createdAt`.
//! v4's REPOSITORY adds it on first touch (`ensureCollection`); v5 has no lazy
//! collection init, so the ensure re-homes that touch — it must be PRESENT on
//! v5's side whenever the table is, and is excluded from the comparand.
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-wear-ensure
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-wear-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/wardrobe-wear-stats-ensure.ts \
//!     > /tmp/oracle-wear-ensure.ndjson
//! Run:
//!   QT_ORACLE_WEAR_ENSURE=/tmp/oracle-wear-ensure.ndjson \
//!   QT_FIXTURE_WEAR_ENSURE_DIR=/tmp/qt-wear-ensure \
//!     cargo test -p quilltap-harness --test wardrobe_wear_stats_ensure_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::wardrobe_wear_stats_repair::{
    ensure_wardrobe_wear_stats, SeedOutcome, WearStatsEnsureOutcome,
};
use quilltap_core::db::Writer;
use rusqlite::Connection;
use serde_json::{json, Value};

const CREATED_AT_INDEX: &str = "idx_wardrobe_wear_stats_createdAt";

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

fn query(conn: &Connection, sql: &str, cols: &[&str]) -> Vec<Value> {
    let mut stmt = conn.prepare(sql).unwrap();
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
            o.insert((*c).to_string(), v);
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

fn expected(mode: &str) -> (i64, i64, WearStatsEnsureOutcome) {
    match mode {
        "a" => (
            2,
            0,
            WearStatsEnsureOutcome {
                table_created: true,
                seed: SeedOutcome::Seeded {
                    wears: 4,
                    chats_with_outfits: 2,
                },
            },
        ),
        "b" => (
            1,
            1,
            WearStatsEnsureOutcome {
                table_created: false,
                seed: SeedOutcome::Seeded {
                    wears: 4,
                    chats_with_outfits: 2,
                },
            },
        ),
        "c" => (
            0,
            2,
            WearStatsEnsureOutcome {
                table_created: false,
                seed: SeedOutcome::AlreadyCompleted,
            },
        ),
        "d" => (
            2,
            0,
            WearStatsEnsureOutcome {
                table_created: true,
                seed: SeedOutcome::Seeded {
                    wears: 11,
                    chats_with_outfits: 9,
                },
            },
        ),
        other => panic!("unknown mode {other}"),
    }
}

#[test]
fn the_wear_ledger_ensure_matches_v4s_runner_in_four_modes() {
    let (Some(oracle_path), Some(dir)) = (
        std::env::var_os("QT_ORACLE_WEAR_ENSURE"),
        std::env::var_os("QT_FIXTURE_WEAR_ENSURE_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_WEAR_ENSURE and QT_FIXTURE_WEAR_ENSURE_DIR (see header).");
        return;
    };
    let pepper = pepper();
    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let mut modes = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v4: Value = serde_json::from_str(line).unwrap();
        let mode = v4["mode"].as_str().unwrap().to_string();
        let (run, skipped, want) = expected(&mode);
        assert_eq!(
            (
                v4["report"]["run"].as_i64(),
                v4["report"]["skipped"].as_i64()
            ),
            (Some(run), Some(skipped)),
            "[{mode}] v4's runner report: {}",
            v4["report"]
        );

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(
            Path::new(&dir).join(format!("wear-ensure-{mode}.db")),
            &work,
        )
        .unwrap();
        let w = Writer::open_writable(&work, &pepper).unwrap();
        let conn = w.connection();
        let outcome = ensure_wardrobe_wear_stats(conn).expect("the ensure");
        assert_eq!(outcome, want, "[{mode}] the ensure's outcome");

        let mut master = query(
            conn,
            "SELECT type, name, sql FROM sqlite_master WHERE tbl_name = 'wardrobe_wear_stats' \
             AND sql IS NOT NULL ORDER BY name",
            &["type", "name", "sql"],
        );
        let before = master.len();
        master.retain(|m| m["name"] != CREATED_AT_INDEX);
        assert_eq!(
            before - master.len(),
            usize::from(table_exists(conn, "wardrobe_wear_stats")),
            "[{mode}] the repository's createdAt index is present exactly when the table is"
        );
        assert_eq!(Value::Array(master), v4["master"], "[{mode}] sqlite_master");
        let info = query(
            conn,
            "SELECT cid, name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('wardrobe_wear_stats')",
            &["cid", "name", "type", "notnull", "dflt_value", "pk"],
        );
        assert_eq!(Value::Array(info), v4["tableInfo"], "[{mode}] table_info");
        let rows = if table_exists(conn, "wardrobe_wear_stats") {
            query(
                conn,
                "SELECT \"itemId\", \"wearerCharacterId\", \"wearCount\", \"firstWornAt\", \
                 \"lastWornAt\", \"lastWornChatId\" FROM \"wardrobe_wear_stats\" ORDER BY rowid",
                &[
                    "itemId",
                    "wearerCharacterId",
                    "wearCount",
                    "firstWornAt",
                    "lastWornAt",
                    "lastWornChatId",
                ],
            )
        } else {
            Vec::new()
        };
        assert!(
            mode == "c" || !rows.is_empty(),
            "[{mode}] a seeding mode must write rows"
        );
        assert_eq!(Value::Array(rows), v4["rows"], "[{mode}] ledger rows");
        let ledger = query(
            conn,
            "SELECT \"id\", \"itemsAffected\", \"message\" FROM \"migrations_state\" ORDER BY \"id\"",
            &["id", "itemsAffected", "message"],
        );
        assert_eq!(
            Value::Array(ledger),
            v4["ledger"],
            "[{mode}] migrations_state"
        );
        modes.push(mode);
    }
    assert_eq!(modes, ["a", "b", "c", "d"], "every mode ran");
    eprintln!("OK: the wear ledger ensure matched v4's runner in modes {modes:?}.");
}
