//! P4.D324 — the Inventory's two column boot ensures
//! (`db::inventory_columns_repair`) against v4's REAL
//! `add-chat-equipped-inventory-v1` and `add-character-inventory-flag-v1`,
//! run through v4's REAL `MigrationRunner` narrowed to them, in seven shapes
//! (the oracle case `harness/oracle/cases/inventory-columns-ensure.ts` builds
//! the base files at the pin from v4's own generateDDL; its header names
//! them): C-A / C-B / C-C for `chats.equippedInventory`, F-A / F-B / F-C for
//! `characters.canManageInventory`, and the pair-order mode D (BOTH absent —
//! v4 runs both in registration order, v5 runs its chain).
//!
//! Comparands per shape, byte-exact: `PRAGMA table_info` of both tables
//! (order included — mode D's order is the point), both `sqlite_master.sql`
//! texts, every row's raw new cell (NULL on both sides — no DEFAULT back-fill).
//! v4's report is asserted per mode (`+RAN` on the A modes and D only), so no
//! mode can go vacuous. The `migrations_state` comparand is the ruled
//! asymmetry `UNSTAMPED_COLUMN_ENSURES`, pinned BOTH ways: v4's runner stamps
//! exactly the ids it ran (A modes + D), v5 stamps NOTHING (a re-ADD is a
//! no-op on either app — v4's `shouldRun` column gate — so no ping-pong).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-inv-ensure
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-inv-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/inventory-columns-ensure.ts \
//!     > /tmp/oracle-inv-ensure.ndjson
//! Run:
//!   QT_ORACLE_INV_ENSURE=/tmp/oracle-inv-ensure.ndjson \
//!   QT_FIXTURE_INV_ENSURE_DIR=/tmp/qt-inv-ensure \
//!     cargo test -p quilltap-harness --test inventory_columns_ensure_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::inventory_columns_repair::{
    ensure_characters_can_manage_inventory, ensure_chats_equipped_inventory,
    run_inventory_columns_ensures, MIGRATION_ID_EQUIPPED_INVENTORY, MIGRATION_ID_INVENTORY_FLAG,
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

fn snapshot(conn: &Connection, table: &str, column: &str) -> Value {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
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
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )
        .unwrap();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, quote(\"{column}\") AS cell FROM \"{table}\" ORDER BY id"
        ))
        .unwrap();
    let cells: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({"id": r.get::<_, String>(0)?, "cell": r.get::<_, String>(1)?}))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    json!({"tableInfo": info, "sql": sql, "cells": cells})
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

#[test]
fn the_two_column_ensures_match_v4s_migrations_in_seven_shapes() {
    let (Some(oracle_path), Some(dir)) = (
        std::env::var_os("QT_ORACLE_INV_ENSURE"),
        std::env::var_os("QT_FIXTURE_INV_ENSURE_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_INV_ENSURE and QT_FIXTURE_INV_ENSURE_DIR (see header).");
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
            "C-A" => &[MIGRATION_ID_EQUIPPED_INVENTORY],
            "F-A" => &[MIGRATION_ID_INVENTORY_FLAG],
            "D" => &[MIGRATION_ID_EQUIPPED_INVENTORY, MIGRATION_ID_INVENTORY_FLAG],
            "C-B" | "C-C" | "F-B" | "F-C" => &[],
            other => panic!("unknown mode {other}"),
        };
        if expected_ran.is_empty() {
            assert_eq!(report.len(), 1, "[{mode}] {report:?}");
            assert!(report[0].contains(" not needed"), "[{mode}] {report:?}");
        } else {
            assert_eq!(report.len(), expected_ran.len(), "[{mode}] {report:?}");
            for (line, id) in report.iter().zip(expected_ran) {
                assert!(line.starts_with(&format!("+RAN {id} ")), "[{mode}] {line}");
            }
        }
        // `UNSTAMPED_COLUMN_ENSURES` (v4's side): the runner stamped exactly
        // what it ran.
        let mut v4_ledger: Vec<&str> = v4["ledger"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        v4_ledger.sort();
        let mut want_stamped: Vec<&str> = expected_ran.to_vec();
        want_stamped.sort();
        assert_eq!(v4_ledger, want_stamped, "[{mode}] v4's ledger");

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(Path::new(&dir).join(format!("inv-{mode}.db")), &work).unwrap();
        let w = Writer::open_writable(&work, &pepper).unwrap();
        let added = match mode.as_str() {
            "C-A" | "C-B" | "C-C" => {
                vec![ensure_chats_equipped_inventory(w.connection()).expect("ensure")]
            }
            "F-A" | "F-B" | "F-C" => {
                vec![ensure_characters_can_manage_inventory(w.connection()).expect("ensure")]
            }
            _ => {
                run_inventory_columns_ensures(w.connection()).expect("the chain");
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
        let chats = snapshot(w.connection(), "chats", "equippedInventory");
        let characters = snapshot(w.connection(), "characters", "canManageInventory");
        for (label, got, want) in [
            ("chats", &chats, &v4["chats"]),
            ("characters", &characters, &v4["characters"]),
        ] {
            assert_eq!(
                serde_json::to_string(got).unwrap(),
                serde_json::to_string(want).unwrap(),
                "[{mode}] {label}"
            );
        }
        // `UNSTAMPED_COLUMN_ENSURES` (v5's side): nothing stamped, ever.
        assert!(
            ledger(w.connection()).is_empty(),
            "[{mode}] v5 stamped the ledger"
        );
        if mode == "D" {
            let chats_sql = chats["sql"].as_str().unwrap();
            let characters_sql = characters["sql"].as_str().unwrap();
            assert!(
                chats_sql.ends_with(", \"equippedInventory\" TEXT DEFAULT NULL)"),
                "{chats_sql}"
            );
            assert!(
                characters_sql.ends_with(", \"canManageInventory\" INTEGER DEFAULT NULL)"),
                "{characters_sql}"
            );
        }
        modes.push(mode);
    }
    assert_eq!(
        modes,
        ["C-A", "C-B", "C-C", "F-A", "F-B", "F-C", "D"],
        "every shape ran"
    );
    eprintln!("OK: the two Inventory column ensures matched v4 in shapes {modes:?}.");
}
