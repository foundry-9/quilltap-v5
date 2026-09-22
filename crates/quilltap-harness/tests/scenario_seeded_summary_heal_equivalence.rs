//! P4.106 item 8 — the bug-158 heal family: v5's
//! `clear_scenario_seeded_chat_summaries` boot heal (`db/scenario_seeded_
//! summary_heal.rs`, P4.D208) vs v4's REAL `clear-scenario-seeded-chat-
//! summaries-v1` migration + ledger (`da9c4f34f`). The shape is
//! `chat_activity_heal_equivalence`'s, scenario for scenario.
//!
//! Until this family, the heal had a native test (`host_scenario_seeded_
//! summary_heal.rs`) and the PREDICATE a differential
//! (`scenario_seeded_summary_equivalence.rs`), but nothing compared the pass
//! itself — the ledger row, the message, the once-only skip, the no-drift
//! silence — against v4's module (the P4.D208 record's named coverage gap).
//!
//! Both sides build the same migration-vintage reduced `chats` table from the
//! committed spec (`harness/oracle/fixtures/scenario-seeded-summary-heal.
//! json`), run their pass, and the diff covers:
//!
//!   - every `chats` row (`contextSummary`, `scenarioText`, and `updatedAt` as
//!     `<bumped>` when the pass moved it — v4 stamps `new Date()`, v5 `now_iso`);
//!   - the path taken, one of four: `ran` (the seeded rows cleared + the ledger
//!     row + the singular/plural message), `no-drift` (NO ledger row — a v5
//!     stamp there would make a later v4 boot skip a migration it never ran —
//!     and v4's no-op sentence pinned against `NO_DRIFT_MESSAGE`),
//!     `not-applicable` (a `chats` table without `scenarioText`: nothing
//!     stamped), `already-completed` (a planted prior ledger row: both skip and
//!     the still-seeded row stays seeded);
//!   - the `migrations_state` rows (`completedAt`/`quilltapVersion`
//!     normalized — the id is the key, the columns are informational);
//!   - the `migrations_metadata` upserts (`lastChecked`/`quilltapVersion`
//!     normalized).
//!
//! The `empty-scenario-pair` scenario is the `SEEDED_WHERE` `<> ''` conjunct
//! under test: both of its rows would match the predicate without it.
//!
//! Generate the oracle (Node 24; jest ignores `.claude/` paths, so the case is
//! staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-scenario-seeded-summary-heal-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/scenario-seeded-summary-heal.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/scenario-seeded-summary-heal.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-scenario-seeded-summary-heal.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "scenario-seeded-summary-heal\.test\.ts$"
//! Run:
//!   QT_ORACLE_SCENARIO_SEEDED_SUMMARY_HEAL=/tmp/oracle-scenario-seeded-summary-heal.ndjson \
//!     cargo test -p quilltap-harness --test scenario_seeded_summary_heal_equivalence

use quilltap_core::db::scenario_seeded_summary_heal::{
    clear_scenario_seeded_chat_summaries, cleared_message, SeededSummaryHealOutcome,
    NO_DRIFT_MESSAGE,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::PathBuf;

const FIXED_NOW: &str = "2026-09-22T12:00:00.000Z";
const SEED_UPDATED_AT: &str = "2026-12-31T00:00:00.000Z";
const MIGRATION_ID: &str = "clear-scenario-seeded-chat-summaries-v1";

/// v4 `migrations/state.ts:47-61` — the ledger DDL, used here ONLY to plant the
/// `already-run` scenario's prior row the way either app's earlier boot left it.
const LEDGER_DDL: &str = "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\n        \"id\" TEXT PRIMARY KEY,\n        \"completedAt\" TEXT NOT NULL,\n        \"quilltapVersion\" TEXT NOT NULL,\n        \"itemsAffected\" INTEGER NOT NULL DEFAULT 0,\n        \"message\" TEXT\n      );\n      CREATE TABLE IF NOT EXISTS \"migrations_metadata\" (\n        \"key\" TEXT PRIMARY KEY,\n        \"value\" TEXT NOT NULL\n      );";

fn scenarios() -> Vec<Value> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/scenario-seeded-summary-heal.json");
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(p).expect("spec")).unwrap();
    spec["scenarios"].as_array().expect("scenarios").clone()
}

fn omits_scenario(scenario: &Value) -> bool {
    scenario["omitScenarioColumn"].as_bool().unwrap_or(false)
}

/// The same migration-vintage reduced table the oracle case builds.
fn build_db(scenario: &Value) -> Connection {
    let db = Connection::open_in_memory().expect("open");
    let omit = omits_scenario(scenario);
    if omit {
        db.execute_batch(
            "CREATE TABLE chats (\n        id TEXT PRIMARY KEY,\n        contextSummary TEXT,\n        createdAt TEXT NOT NULL,\n        updatedAt TEXT NOT NULL\n      );",
        )
        .expect("ddl");
    } else {
        db.execute_batch(
            "CREATE TABLE chats (\n        id TEXT PRIMARY KEY,\n        contextSummary TEXT,\n        scenarioText TEXT,\n        createdAt TEXT NOT NULL,\n        updatedAt TEXT NOT NULL\n      );",
        )
        .expect("ddl");
    }
    for c in scenario["chats"].as_array().expect("chats") {
        if omit {
            db.execute(
                "INSERT INTO chats (id, contextSummary, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    c["id"].as_str().unwrap(),
                    c["contextSummary"].as_str(),
                    "2026-01-01T00:00:00.000Z",
                    SEED_UPDATED_AT
                ],
            )
            .expect("chat");
        } else {
            db.execute(
                "INSERT INTO chats (id, contextSummary, scenarioText, createdAt, updatedAt) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    c["id"].as_str().unwrap(),
                    c["contextSummary"].as_str(),
                    c["scenarioText"].as_str(),
                    "2026-01-01T00:00:00.000Z",
                    SEED_UPDATED_AT
                ],
            )
            .expect("chat");
        }
    }
    if scenario["priorLedger"].as_bool().unwrap_or(false) {
        // v4's `recordCompletedMigration` writes the row + both metadata keys.
        db.execute_batch(LEDGER_DDL).expect("ledger ddl");
        db.execute(
            "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                MIGRATION_ID,
                "2026-09-20T00:00:00.000Z",
                "prior",
                3,
                "Cleared the scenario standing in as a summary on 3 conversations"
            ],
        )
        .expect("prior ledger row");
        for (k, v) in [
            ("lastChecked", "2026-09-20T00:00:00.000Z"),
            ("quilltapVersion", "prior"),
        ] {
            db.execute(
                "INSERT INTO migrations_metadata (key, value) VALUES (?1, ?2)",
                rusqlite::params![k, v],
            )
            .expect("prior metadata");
        }
    }
    db
}

fn table_exists(db: &Connection, name: &str) -> bool {
    db.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")
        .and_then(|mut s| s.exists([name]))
        .unwrap_or(false)
}

fn dump_chats(db: &Connection, scenario: &Value) -> Vec<Value> {
    let omit = omits_scenario(scenario);
    let sql = if omit {
        "SELECT id, contextSummary, NULL, updatedAt FROM chats ORDER BY id"
    } else {
        "SELECT id, contextSummary, scenarioText, updatedAt FROM chats ORDER BY id"
    };
    let mut stmt = db.prepare(sql).expect("prep");
    stmt.query_map([], |r| {
        let updated: String = r.get(3)?;
        let updated = if updated == SEED_UPDATED_AT {
            SEED_UPDATED_AT.to_string()
        } else {
            "<bumped>".to_string()
        };
        let mut row = json!({
            "id": r.get::<_, String>(0)?,
            "contextSummary": r.get::<_, Option<String>>(1)?,
        });
        if !omit {
            row["scenarioText"] = json!(r.get::<_, Option<String>>(2)?);
        }
        row["updatedAt"] = json!(updated);
        Ok(row)
    })
    .expect("query")
    .collect::<Result<Vec<_>, _>>()
    .expect("rows")
}

/// The ledger rows with the two informational columns normalized.
fn dump_ledger(db: &Connection) -> Vec<Value> {
    if !table_exists(db, "migrations_state") {
        return Vec::new();
    }
    let mut stmt = db
        .prepare("SELECT id, itemsAffected, message FROM migrations_state ORDER BY id")
        .expect("prep");
    stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "completedAt": "<ts>",
            "quilltapVersion": "<version>",
            "itemsAffected": r.get::<_, i64>(1)?,
            "message": r.get::<_, Option<String>>(2)?,
        }))
    })
    .expect("query")
    .collect::<Result<Vec<_>, _>>()
    .expect("rows")
}

fn dump_metadata_keys(db: &Connection) -> Vec<String> {
    if !table_exists(db, "migrations_metadata") {
        return Vec::new();
    }
    let mut stmt = db
        .prepare("SELECT key FROM migrations_metadata ORDER BY key")
        .expect("prep");
    stmt.query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows")
}

fn normalize_oracle_ledger(rows: &Value) -> Vec<Value> {
    rows.as_array()
        .expect("ledger array")
        .iter()
        .map(|r| {
            json!({
                "id": r["id"],
                "completedAt": "<ts>",
                "quilltapVersion": "<version>",
                "itemsAffected": r["itemsAffected"],
                "message": r["message"],
            })
        })
        .collect()
}

fn oracle_metadata_keys(rows: &Value) -> Vec<String> {
    rows.as_array()
        .expect("metadata array")
        .iter()
        .map(|r| {
            let key = r["key"].as_str().unwrap();
            assert!(
                matches!(key, "lastChecked" | "quilltapVersion"),
                "unexpected metadata key {key}"
            );
            key.to_string()
        })
        .collect()
}

#[test]
fn scenario_seeded_summary_heal_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_SCENARIO_SEEDED_SUMMARY_HEAL") else {
        println!("SKIP: set QT_ORACLE_SCENARIO_SEEDED_SUMMARY_HEAL (see the header).");
        return;
    };
    let oracle: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    let scenarios = scenarios();
    assert_eq!(
        oracle.len(),
        scenarios.len(),
        "the oracle and the spec disagree on the scenario count"
    );

    let mut seen = std::collections::BTreeSet::new();
    for (scenario, want) in scenarios.iter().zip(&oracle) {
        let name = scenario["name"].as_str().expect("name");
        assert_eq!(name, want["scenario"].as_str().unwrap(), "scenario order");

        let db = build_db(scenario);
        let outcome = clear_scenario_seeded_chat_summaries(&db, FIXED_NOW).expect("heal");
        let path_taken = want["path"].as_str().expect("path");
        seen.insert(path_taken.to_string());

        match path_taken {
            "ran" => {
                let result = &want["result"];
                let items = result["itemsAffected"].as_u64().expect("itemsAffected") as usize;
                assert_eq!(
                    outcome,
                    SeededSummaryHealOutcome::Ran { cleared: items },
                    "[{name}] v4 ran the pass"
                );
                assert_eq!(
                    cleared_message(items),
                    result["message"].as_str().expect("message"),
                    "[{name}] MigrationResult message"
                );
                assert_eq!(
                    want["shouldRunAfter"].as_bool(),
                    Some(false),
                    "[{name}] v4's rewrite must be its own fixed point"
                );
                assert_eq!(
                    want["skippedOnRerun"].as_bool(),
                    Some(true),
                    "[{name}] v4's runner must skip a re-run"
                );
                assert_eq!(
                    clear_scenario_seeded_chat_summaries(&db, FIXED_NOW).expect("re-run"),
                    SeededSummaryHealOutcome::AlreadyCompleted,
                    "[{name}] v5 must skip on the ledger row"
                );
            }
            "no-drift" => {
                assert_eq!(
                    outcome,
                    SeededSummaryHealOutcome::NoDrift,
                    "[{name}] v4 skipped; v5 must too"
                );
                assert_eq!(
                    want["noDriftRunMessage"].as_str(),
                    Some(NO_DRIFT_MESSAGE),
                    "[{name}] the no-drift sentence"
                );
                assert!(
                    !table_exists(&db, "migrations_state"),
                    "[{name}] a clean boot must stamp NOTHING"
                );
            }
            "not-applicable" => {
                assert_eq!(
                    outcome,
                    SeededSummaryHealOutcome::NotApplicable,
                    "[{name}] v4's chatsTableUsable() is false"
                );
            }
            "already-completed" => {
                assert_eq!(want["completedBefore"].as_bool(), Some(true));
                assert_eq!(
                    outcome,
                    SeededSummaryHealOutcome::AlreadyCompleted,
                    "[{name}] v4's runner skipped on the prior ledger row"
                );
            }
            other => panic!("[{name}] unknown oracle path {other}"),
        }

        assert_eq!(
            Value::Array(dump_chats(&db, scenario)),
            want["chats"],
            "[{name}] chats rows"
        );
        assert_eq!(
            dump_ledger(&db),
            normalize_oracle_ledger(&want["ledger"]),
            "[{name}] migrations_state"
        );
        assert_eq!(
            dump_metadata_keys(&db),
            oracle_metadata_keys(&want["metadata"]),
            "[{name}] migrations_metadata keys"
        );
    }

    // Shape guard: a spec that loses a path must not read as green.
    for p in ["ran", "no-drift", "not-applicable", "already-completed"] {
        assert!(seen.contains(p), "no scenario took the `{p}` path");
    }
    println!(
        "scenario_seeded_summary_heal: {} scenarios equal",
        scenarios.len()
    );
}
