//! P4.D140 heal family — v5's `recompute_chat_last_message_at` boot heal vs v4's
//! REAL `recompute-chat-last-message-at-v1` migration + ledger (`735d9408c`,
//! bug 112's data pass). Modelled on `thinking_prefill_heal_equivalence`.
//!
//! Both sides build the same migration-vintage `chats` + `chat_messages` tables
//! from the shared committed spec (`harness/oracle/fixtures/chat-activity-heal.
//! json`), run their pass, and the diff covers:
//!
//!   - every `chats` row's `lastMessageAt` — the walk-back past a Staff
//!     announcement, the whisper kept, the six clears-to-NULL arms (Staff,
//!     announcement bubble, TOOL, SYSTEM, context-summary, system event), the
//!     already-correct row untouched, the never-stamped row filled, and a chat
//!     with no messages at all;
//!   - the `''`-systemSender seam. The in-memory predicate reads an empty string
//!     as ABSENT (JS truthiness) while the SQL mirror reads `IS NULL`; the
//!     migration is SQL, so that chat CLEARS. Measured against v4, not assumed —
//!     this is the one place the two deliberate spellings visibly disagree.
//!   - the `migrations_state` ledger row — the CROSS-APP once-only mechanism —
//!     with `completedAt`/`lastChecked` normalized to `<ts>` and
//!     `quilltapVersion` to `<version>` (v4 stamps its package version, v5
//!     stamps quilltap-core's; the column is informational, the id is the key);
//!   - the `migrations_metadata` upserts;
//!   - the `MigrationResult` message string, byte-exact;
//!   - **the no-drift scenario**, where v4's runner skips and writes NO ledger
//!     row (the pass is simply retried next boot). A v5 stamp there would make a
//!     LATER v4 boot skip a migration it believes it already ran, so the family
//!     asserts the ledger stays EMPTY on both sides — and pins v4's own no-op
//!     sentence against `NO_DRIFT_MESSAGE`.
//!
//! Generate the oracle (Node 24; jest ignores `.claude/` paths, so the case is
//! staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-chat-activity-heal-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/chat-activity-heal.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/chat-activity-heal.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-chat-activity-heal.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "chat-activity-heal\.test\.ts$"
//! Run:
//!   QT_ORACLE_CHAT_ACTIVITY_HEAL=/tmp/oracle-chat-activity-heal.ndjson \
//!     cargo test -p quilltap-harness --test chat_activity_heal_equivalence

use quilltap_core::db::chat_activity_recompute_heal::{
    recompute_chat_last_message_at, RecomputeOutcome, NO_DRIFT_MESSAGE,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::PathBuf;

const FIXED_NOW: &str = "2026-09-01T12:00:00.000Z";

fn scenarios() -> Vec<Value> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-activity-heal.json");
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(p).expect("spec")).unwrap();
    spec["scenarios"].as_array().expect("scenarios").clone()
}

/// The same migration-vintage tables the oracle case and v4's own integration
/// test build.
fn build_db(scenario: &Value) -> Connection {
    let db = Connection::open_in_memory().expect("open");
    db.execute_batch(
        "CREATE TABLE chats (\n           id TEXT PRIMARY KEY,\n           lastMessageAt TEXT,\n           createdAt TEXT NOT NULL,\n           updatedAt TEXT NOT NULL\n         );\n         CREATE TABLE chat_messages (\n           id TEXT PRIMARY KEY,\n           chatId TEXT NOT NULL,\n           type TEXT DEFAULT 'message',\n           role TEXT,\n           systemSender TEXT DEFAULT NULL,\n           customAnnouncer TEXT DEFAULT NULL,\n           createdAt TEXT NOT NULL\n         );",
    )
    .expect("ddl");
    for c in scenario["chats"].as_array().expect("chats") {
        db.execute(
            "INSERT INTO chats (id, lastMessageAt, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                c["id"].as_str().unwrap(),
                c["lastMessageAt"].as_str(),
                "2026-01-01T00:00:00.000Z",
                "2026-12-31T00:00:00.000Z"
            ],
        )
        .expect("chat");
    }
    let mut seq = 0;
    for m in scenario["messages"].as_array().expect("messages") {
        seq += 1;
        let role = match m.get("role") {
            None => Some("ASSISTANT"),
            Some(Value::Null) => None,
            Some(v) => v.as_str(),
        };
        db.execute(
            "INSERT INTO chat_messages (id, chatId, type, role, systemSender, customAnnouncer, createdAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                format!("m{seq}"),
                m["chatId"].as_str().unwrap(),
                m.get("type").and_then(Value::as_str).unwrap_or("message"),
                role,
                m.get("systemSender").and_then(Value::as_str),
                m.get("customAnnouncer").and_then(Value::as_str),
                m["createdAt"].as_str().unwrap()
            ],
        )
        .expect("message");
    }
    db
}

fn table_exists(db: &Connection, name: &str) -> bool {
    db.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")
        .and_then(|mut s| s.exists([name]))
        .unwrap_or(false)
}

fn dump_chats(db: &Connection) -> Vec<Value> {
    let mut stmt = db
        .prepare("SELECT id, lastMessageAt FROM chats ORDER BY id")
        .expect("prep");
    stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "lastMessageAt": r.get::<_, Option<String>>(1)?,
        }))
    })
    .expect("query")
    .collect::<Result<Vec<_>, _>>()
    .expect("rows")
}

/// v4's ledger row with the two informational columns normalized.
fn dump_ledger(db: &Connection) -> Vec<Value> {
    if !table_exists(db, "migrations_state") {
        return Vec::new();
    }
    let mut stmt = db
        .prepare("SELECT id, completedAt, quilltapVersion, itemsAffected, message FROM migrations_state ORDER BY id")
        .expect("prep");
    stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "completedAt": normalize_ts(&r.get::<_, String>(1)?),
            "quilltapVersion": "<version>",
            "itemsAffected": r.get::<_, i64>(3)?,
            "message": r.get::<_, Option<String>>(4)?,
        }))
    })
    .expect("query")
    .collect::<Result<Vec<_>, _>>()
    .expect("rows")
}

fn dump_metadata(db: &Connection) -> Vec<Value> {
    if !table_exists(db, "migrations_metadata") {
        return Vec::new();
    }
    let mut stmt = db
        .prepare("SELECT key, value FROM migrations_metadata ORDER BY key")
        .expect("prep");
    stmt.query_map([], |r| {
        let key: String = r.get(0)?;
        let value: String = r.get(1)?;
        let value = match key.as_str() {
            "lastChecked" => normalize_ts(&value),
            "quilltapVersion" => "<version>".to_string(),
            _ => value,
        };
        Ok(json!({ "key": key, "value": value }))
    })
    .expect("query")
    .collect::<Result<Vec<_>, _>>()
    .expect("rows")
}

fn normalize_ts(_v: &str) -> String {
    "<ts>".to_string()
}

/// The oracle's ledger/metadata rows, normalized the same way.
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

fn normalize_oracle_metadata(rows: &Value) -> Vec<Value> {
    rows.as_array()
        .expect("metadata array")
        .iter()
        .map(|r| {
            let key = r["key"].as_str().unwrap();
            let value = match key {
                "lastChecked" => "<ts>",
                "quilltapVersion" => "<version>",
                other => panic!("unexpected metadata key {other}"),
            };
            json!({ "key": key, "value": value })
        })
        .collect()
}

#[test]
fn chat_activity_heal_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_CHAT_ACTIVITY_HEAL") else {
        eprintln!("SKIP: set QT_ORACLE_CHAT_ACTIVITY_HEAL (see the header).");
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

    let mut seen_ran = false;
    let mut seen_no_drift = false;

    for (scenario, want) in scenarios.iter().zip(&oracle) {
        let name = scenario["name"].as_str().expect("name");
        assert_eq!(name, want["scenario"].as_str().unwrap(), "scenario order");
        assert_eq!(
            want["completedBefore"].as_bool(),
            Some(false),
            "[{name}] the oracle instance must start unmigrated"
        );

        let db = build_db(scenario);
        let outcome = recompute_chat_last_message_at(&db, FIXED_NOW).expect("heal");

        match want["shouldRun"].as_bool().expect("shouldRun") {
            true => {
                seen_ran = true;
                let result = &want["result"];
                let updated = result["itemsAffected"].as_u64().expect("itemsAffected") as usize;
                let RecomputeOutcome::Ran {
                    updated: got_updated,
                    cleared,
                } = &outcome
                else {
                    panic!("[{name}] v4 ran the pass; v5 answered {outcome:?}");
                };
                assert_eq!(*got_updated, updated, "[{name}] itemsAffected");
                // The message carries BOTH counts, byte-exact.
                let want_msg = result["message"].as_str().expect("message");
                let got_msg = format!(
                    "Recomputed last-activity for {} chat{} ({} with no character-authored messages)",
                    got_updated,
                    if *got_updated == 1 { "" } else { "s" },
                    cleared
                );
                assert_eq!(got_msg, want_msg, "[{name}] MigrationResult message");
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
                // v5's own once-only guard, over the row v4's runner writes.
                assert_eq!(
                    recompute_chat_last_message_at(&db, FIXED_NOW).expect("re-run"),
                    RecomputeOutcome::AlreadyCompleted,
                    "[{name}] v5 must skip on the ledger row"
                );
            }
            false => {
                seen_no_drift = true;
                assert_eq!(
                    outcome,
                    RecomputeOutcome::NoDrift,
                    "[{name}] v4 skipped; v5 must too"
                );
                assert_eq!(
                    want["noDriftRunMessage"].as_str(),
                    Some(NO_DRIFT_MESSAGE),
                    "[{name}] the no-drift sentence"
                );
                assert!(
                    !table_exists(&db, "migrations_state"),
                    "[{name}] a clean boot must stamp NOTHING — a v5 row here would make a \
                     later v4 boot skip a migration it never ran"
                );
            }
        }

        assert_eq!(
            Value::Array(dump_chats(&db)),
            want["chats"],
            "[{name}] chats.lastMessageAt"
        );
        assert_eq!(
            dump_ledger(&db),
            normalize_oracle_ledger(&want["ledger"]),
            "[{name}] migrations_state"
        );
        assert_eq!(
            dump_metadata(&db),
            normalize_oracle_metadata(&want["metadata"]),
            "[{name}] migrations_metadata"
        );
    }

    // Shape guard: a spec that loses either scenario must not read as green.
    assert!(seen_ran, "the drifted scenario went missing");
    assert!(seen_no_drift, "the no-drift scenario went missing");
}
