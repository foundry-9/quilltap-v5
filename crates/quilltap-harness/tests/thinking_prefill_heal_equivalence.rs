//! P4.D97 heal family — v5's `retire_prefill_on_thinking_profiles` boot heal
//! vs v4's REAL `retire-prefill-on-thinking-profiles-v1` migration + ledger
//! (`97d2fcb5` / `12fe3e6f`). Both sides build the same migration-vintage
//! `connection_profiles` table from the shared committed spec
//! (`harness/oracle/fixtures/thinking-prefill-heal.json` — the full
//! leave-alone matrix from v4's own integration test plus the modelName-NULL,
//! parameters-NULL, string-`"true"` and `''`-option shapes), run their pass,
//! and the diff covers:
//!
//!   - every `connection_profiles` row (the clears AND the leave-alones);
//!   - the `migrations_state` ledger row — the CROSS-APP once-only mechanism:
//!     v4's runner skips on the row this heal writes and vice versa — with
//!     `completedAt`/`lastChecked` normalized to `<ts>` and `quilltapVersion`
//!     normalized to `<version>` (v4 stamps its package version, v5 stamps
//!     quilltap-core's; the column is informational, the id is the key);
//!   - the `migrations_metadata` upserts;
//!   - the `MigrationResult` message string, byte-exact;
//!   - the oracle's own `skippedOnRerun` proof that v4's runner would skip
//!     after the record (asserted true).
//!
//! Generate the oracle (Node 24, from the pinned v4 worktree — jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-thinking-heal-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/thinking-prefill-heal.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/thinking-prefill-heal.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-thinking-prefill-heal.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "thinking-prefill-heal\.test\.ts$"
//! Run:
//!   QT_ORACLE_THINKING_HEAL=/tmp/oracle-thinking-prefill-heal.ndjson \
//!     cargo test -p quilltap-harness --test thinking_prefill_heal_equivalence
use quilltap_core::db::thinking_prefill_retire_heal::{
    retire_prefill_on_thinking_profiles, RetireOutcome,
};
use rusqlite::Connection;
use serde_json::Value;
use std::path::PathBuf;

const FIXED_NOW: &str = "2026-08-21T12:00:00.000Z";

fn spec_rows() -> Vec<Value> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/thinking-prefill-heal.json");
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(p).expect("spec")).unwrap();
    spec["rows"].as_array().expect("rows").clone()
}

/// The same migration-vintage table both the oracle case and v4's own
/// integration test build.
fn build_db(rows: &[Value]) -> Connection {
    let db = Connection::open_in_memory().expect("open");
    db.execute_batch(
        "CREATE TABLE connection_profiles (\n           id TEXT PRIMARY KEY,\n           name TEXT NOT NULL,\n           provider TEXT NOT NULL,\n           modelName TEXT,\n           parameters TEXT,\n           multiCharacterPrefill INTEGER DEFAULT 1\n         )",
    )
    .expect("ddl");
    let mut stmt = db
        .prepare(
            "INSERT INTO connection_profiles \
               (id, name, provider, modelName, parameters, multiCharacterPrefill) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .expect("prep");
    for r in rows {
        let provider = r["provider"].as_str().unwrap();
        stmt.execute(rusqlite::params![
            r["id"].as_str().unwrap(),
            format!("{provider} profile"),
            provider,
            r["modelName"].as_str(),
            r["parameters"].as_str(),
            r["prefill"].as_i64(),
        ])
        .expect("seed");
    }
    drop(stmt);
    db
}

/// `<ts>` for ISO timestamps, `<version>` for the app-version cells — the two
/// legitimately-differing fields (see the header).
fn normalize_cell(key: &str, v: &Value) -> Value {
    match (key, v) {
        ("completedAt", Value::String(_)) => Value::String("<ts>".into()),
        ("quilltapVersion", Value::String(_)) => Value::String("<version>".into()),
        ("value", Value::String(s)) if s.contains('T') && s.ends_with('Z') => {
            Value::String("<ts>".into())
        }
        ("value", Value::String(s)) if s.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
            Value::String("<version>".into())
        }
        _ => v.clone(),
    }
}

fn dump(db: &Connection, sql: &str, cols: &[&str]) -> Vec<Value> {
    let mut stmt = db.prepare(sql).expect("prep dump");
    let rows = stmt
        .query_map([], |r| {
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                let v: rusqlite::types::Value = r.get(i)?;
                let jv = match v {
                    rusqlite::types::Value::Null => Value::Null,
                    rusqlite::types::Value::Integer(n) => Value::from(n),
                    rusqlite::types::Value::Real(f) => Value::from(f),
                    rusqlite::types::Value::Text(s) => Value::String(s),
                    rusqlite::types::Value::Blob(_) => panic!("unexpected BLOB"),
                };
                obj.insert((*c).into(), normalize_cell(c, &jv));
            }
            Ok(Value::Object(obj))
        })
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows");
    rows
}

#[test]
fn thinking_prefill_heal_matches_v4s_migration_and_ledger() {
    let path = match std::env::var("QT_ORACLE_THINKING_HEAL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_THINKING_HEAL (see test header).");
            return;
        }
    };
    let oracle: Value =
        serde_json::from_str(std::fs::read_to_string(&path).expect("read oracle").trim())
            .expect("parse oracle");
    assert_eq!(
        oracle["skippedOnRerun"],
        Value::Bool(true),
        "v4's runner must skip after the record — the ledger half the heal leans on"
    );

    let rows = spec_rows();
    let db = build_db(&rows);
    let out = retire_prefill_on_thinking_profiles(&db, FIXED_NOW).expect("heal");
    let RetireOutcome::Ran { examined, cleared } = out else {
        panic!("expected the heal to run, got {out:?}");
    };
    assert_eq!(
        (examined, cleared),
        (
            oracle["result"]["message"]
                .as_str()
                .and_then(|m| m.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
                .expect("examined in message"),
            oracle["result"]["itemsAffected"].as_u64().unwrap() as usize
        )
    );

    // The result message, byte-exact.
    let message = format!(
        "Examined {examined} prefill-enabled profile(s) on thinking-capable providers; turned the [Name] prefill off on {cleared}"
    );
    assert_eq!(oracle["result"]["message"].as_str().unwrap(), message);

    // Every profile row, cell-for-cell.
    let got = dump(
        &db,
        "SELECT id, name, provider, modelName, parameters, multiCharacterPrefill \
         FROM connection_profiles ORDER BY id",
        &[
            "id",
            "name",
            "provider",
            "modelName",
            "parameters",
            "multiCharacterPrefill",
        ],
    );
    let want = oracle["profiles"].as_array().unwrap();
    assert_eq!(got.len(), want.len(), "profile row count");
    for (g, w) in got.iter().zip(want.iter()) {
        assert_eq!(g, w, "profile row diverged");
    }

    // The ledger row (normalized ts/version) + the metadata upserts.
    let got_ledger = dump(
        &db,
        "SELECT id, completedAt, quilltapVersion, itemsAffected, message \
         FROM migrations_state ORDER BY id",
        &[
            "id",
            "completedAt",
            "quilltapVersion",
            "itemsAffected",
            "message",
        ],
    );
    let want_ledger: Vec<Value> = oracle["ledger"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut o = serde_json::Map::new();
            for (k, v) in r.as_object().unwrap() {
                o.insert(k.clone(), normalize_cell(k, v));
            }
            Value::Object(o)
        })
        .collect();
    assert_eq!(got_ledger, want_ledger, "migrations_state diverged");

    let got_meta = dump(
        &db,
        "SELECT key, value FROM migrations_metadata ORDER BY key",
        &["key", "value"],
    );
    let want_meta: Vec<Value> = oracle["metadata"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut o = serde_json::Map::new();
            let obj = r.as_object().unwrap();
            for (k, v) in obj {
                if k == "value" {
                    o.insert(k.clone(), normalize_cell("value", v));
                } else {
                    o.insert(k.clone(), v.clone());
                }
            }
            Value::Object(o)
        })
        .collect();
    assert_eq!(got_meta, want_meta, "migrations_metadata diverged");

    // Shape: the corpus must exercise both directions (some cleared, some
    // kept) or the whole matrix proves nothing.
    assert!(cleared >= 5, "expected several clears, got {cleared}");
    assert!(
        examined > cleared,
        "expected leave-alone rows among the examined"
    );
    assert!(
        got.len() > examined,
        "expected rows outside the provider scan entirely"
    );

    // The v5 second boot over the healed DB: a no-op that keeps a re-ticked
    // true (mirrors the module unit test, here over the full matrix).
    db.execute(
        "UPDATE connection_profiles SET multiCharacterPrefill = 1 WHERE id = 'ds-default'",
        [],
    )
    .expect("re-tick");
    let again = retire_prefill_on_thinking_profiles(&db, FIXED_NOW).expect("second boot");
    assert_eq!(again, RetireOutcome::AlreadyCompleted);
    let after: i64 = db
        .query_row(
            "SELECT multiCharacterPrefill FROM connection_profiles WHERE id = 'ds-default'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, 1, "the re-ticked true survives the second boot");

    eprintln!(
        "thinking-prefill heal: {} profile rows matched, ledger + metadata matched ({examined} examined / {cleared} cleared)",
        got.len()
    );
}
