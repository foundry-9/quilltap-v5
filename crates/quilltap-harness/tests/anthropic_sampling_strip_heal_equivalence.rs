//! P4.D362 — the `strip-anthropic-sampling-parameters-v1` boot heal against
//! v4's REAL migration + REAL ledger (`anthropic-sampling-strip-heal.test.ts`).
//!
//! Both sides build the same reduced `connection_profiles` table from the
//! shared row spec (`harness/oracle/fixtures/anthropic-sampling-strip-heal.json`)
//! in four modes — (a) `unstamped`, (b) `already-stamped` (a row planted after
//! the stamp stays), (c) `no-parameters`, (d) `no-table` — and compare every
//! `parameters` cell `quote()`d, the `migrations_state` row (`completedAt`
//! masked — it is not dumped), the `migrations_metadata` keys, the outcome ↔
//! v4's `MigrationResult` / skip reason, the logged lines field-for-field
//! (`removed` ↔ `removedJson`), and, on (a), that v4's runner would skip a
//! re-run after the stamp (`skippedOnRerun`) — and that v5 skips after v4's.
//!
//! Generate the oracle (Node 24, from the pinned v4 worktree — jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-anthropic-strip-heal-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/anthropic-sampling-strip-heal.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/anthropic-sampling-strip-heal.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-anthropic-sampling-strip-heal.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "anthropic-sampling-strip-heal\.test\.ts$"
//! Run:
//!   cd $V5W
//!   QT_ORACLE_ANTHROPIC_STRIP_HEAL=/tmp/oracle-anthropic-sampling-strip-heal.ndjson \
//!     cargo test -p quilltap-harness --test anthropic_sampling_strip_heal_equivalence -- --nocapture
use quilltap_core::db::connection_profiles_sampling_strip_heal::{
    strip_anthropic_sampling_parameters, MIGRATION_ID,
};
use quilltap_core::db::memory_rewrite_heals::LedgeredHealOutcome;
use quilltap_core::test_support::global_capture;
use rusqlite::Connection;
use serde_json::Value;
use std::path::PathBuf;

const FIXED_NOW: &str = "2026-11-05T12:00:00.000Z";

fn spec_rows() -> Vec<Value> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/anthropic-sampling-strip-heal.json");
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(p).expect("spec")).unwrap();
    spec["rows"].as_array().expect("rows").clone()
}

fn seed_table(db: &Connection, with_parameters: bool) {
    db.execute_batch(if with_parameters {
        "CREATE TABLE connection_profiles (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider TEXT NOT NULL, parameters TEXT)"
    } else {
        "CREATE TABLE connection_profiles (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider TEXT NOT NULL)"
    })
    .unwrap();
    for r in spec_rows() {
        let provider = r["provider"].as_str().unwrap();
        if with_parameters {
            db.execute(
                "INSERT INTO connection_profiles (id, name, provider, parameters) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![r["id"].as_str().unwrap(), format!("{provider} profile"), provider, r["parameters"].as_str()],
            )
            .unwrap();
        } else {
            db.execute(
                "INSERT INTO connection_profiles (id, name, provider) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    r["id"].as_str().unwrap(),
                    format!("{provider} profile"),
                    provider
                ],
            )
            .unwrap();
        }
    }
}

fn table_exists(db: &Connection, name: &str) -> bool {
    db.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1")
        .unwrap()
        .exists([name])
        .unwrap()
}

fn rows_json(db: &Connection, sql: &str) -> Vec<Value> {
    let mut stmt = db.prepare(sql).unwrap();
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    stmt.query_map([], |r| {
        let mut o = serde_json::Map::new();
        for (i, n) in names.iter().enumerate() {
            let v = match r.get::<_, rusqlite::types::Value>(i)? {
                rusqlite::types::Value::Null => Value::Null,
                rusqlite::types::Value::Integer(n) => Value::from(n),
                rusqlite::types::Value::Real(f) => Value::from(f),
                rusqlite::types::Value::Text(s) => Value::String(s),
                rusqlite::types::Value::Blob(_) => panic!("blob"),
            };
            o.insert(n.clone(), v);
        }
        Ok(Value::Object(o))
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

fn has_parameters(db: &Connection) -> bool {
    table_exists(db, "connection_profiles")
        && rows_json(
            db,
            "SELECT name FROM pragma_table_info('connection_profiles')",
        )
        .iter()
        .any(|r| r["name"] == "parameters")
}

/// The v4 run record ↔ the v5 outcome.
fn run_of(out: &LedgeredHealOutcome, db: &Connection) -> Value {
    match out {
        LedgeredHealOutcome::AlreadyCompleted => {
            serde_json::json!({"ran": false, "skipped": "completed", "result": null})
        }
        LedgeredHealOutcome::NotNeeded => {
            serde_json::json!({"ran": false, "skipped": "shouldRun", "result": null})
        }
        LedgeredHealOutcome::Ran { rewritten, total } => {
            let message: String = db
                .query_row(
                    "SELECT message FROM migrations_state WHERE id = ?1",
                    [MIGRATION_ID],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                message,
                format!("Examined {total} Anthropic profile(s); removed sampling parameters from {rewritten}")
            );
            serde_json::json!({"ran": true, "skipped": null, "result": {
                "id": MIGRATION_ID, "success": true, "itemsAffected": rewritten, "message": message,
            }})
        }
    }
}

fn dump(db: &Connection, run: Value) -> Value {
    let profiles = if has_parameters(db) {
        rows_json(
            db,
            "SELECT id, quote(parameters) AS parameters FROM connection_profiles ORDER BY rowid",
        )
    } else {
        Vec::new()
    };
    let ledger = if table_exists(db, "migrations_state") {
        rows_json(
            db,
            "SELECT id, itemsAffected, message FROM migrations_state ORDER BY id",
        )
    } else {
        Vec::new()
    };
    let metadata_keys = if table_exists(db, "migrations_metadata") {
        rows_json(db, "SELECT key FROM migrations_metadata ORDER BY key")
    } else {
        Vec::new()
    };
    serde_json::json!({
        "run": run,
        "profiles": profiles,
        "ledger": ledger,
        "metadataKeys": metadata_keys,
        "ledgerTables": table_exists(db, "migrations_state") && table_exists(db, "migrations_metadata"),
    })
}

/// v4's logger call → the v5 capture line it must equal, field for field
/// (`removed` → `removedJson`, v5's file-layer convention).
fn assert_logs(mode: &str, want: &[Value], got: &[String]) {
    let got: Vec<&String> = got
        .iter()
        .filter(|l| l.contains("quilltap::boot"))
        .collect();
    assert_eq!(
        got.len(),
        want.len(),
        "{mode}: log count — v5 {got:#?} vs v4 {want:#?}"
    );
    for (g, w) in got.iter().zip(want) {
        let level = w["level"].as_str().unwrap().to_uppercase();
        assert!(g.starts_with(&level), "{mode}: level {g} vs {w}");
        let msg = w["message"].as_str().unwrap();
        assert!(
            g.starts_with(&format!("{level} quilltap::boot {msg} "))
                || **g == format!("{level} quilltap::boot {msg}"),
            "{mode}: message {g} vs {w}"
        );
        let meta = w["meta"].as_object().unwrap();
        let mut fields = 0;
        for (k, v) in meta {
            let rendered = match (k.as_str(), v) {
                ("removed", v) => format!(" removedJson={}", serde_json::to_string(v).unwrap()),
                (_, Value::String(s)) => format!(" {k}={s}"),
                (_, v) => format!(" {k}={v}"),
            };
            assert!(
                g.contains(&rendered),
                "{mode}: field {rendered} missing from {g}"
            );
            fields += 1;
        }
        // No extra field on v5's side.
        assert_eq!(
            g.split(' ').filter(|t| t.contains('=')).count(),
            fields,
            "{mode}: extra v5 field in {g}"
        );
    }
}

#[test]
fn strip_heal_matches_v4s_migration_and_ledger_in_every_mode() {
    let path = match std::env::var("QT_ORACLE_ANTHROPIC_STRIP_HEAL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ANTHROPIC_STRIP_HEAL (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 4, "four modes");
    global_capture::install();

    for want in &lines {
        let mode = want["mode"].as_str().unwrap();
        let db = Connection::open_in_memory().unwrap();
        match mode {
            "unstamped" => seed_table(&db, true),
            "already-stamped" => {
                seed_table(&db, true);
                // v4 stamps first — the oracle's own first runner pass. The
                // v5 side models "v4 already ran" with v4's row bytes.
                db.execute_batch(
                    "CREATE TABLE migrations_state (id TEXT PRIMARY KEY, completedAt TEXT NOT NULL, quilltapVersion TEXT NOT NULL, itemsAffected INTEGER NOT NULL DEFAULT 0, message TEXT);
                     CREATE TABLE migrations_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     INSERT INTO migrations_metadata VALUES ('lastChecked', 'x'), ('quilltapVersion', '4.13.0-dev.8');",
                )
                .unwrap();
                let first = &lines[0];
                let r = &first["ledger"][0];
                db.execute(
                    "INSERT INTO migrations_state VALUES (?1, 'ts', '4.13.0-dev.8', ?2, ?3)",
                    rusqlite::params![
                        r["id"].as_str().unwrap(),
                        r["itemsAffected"].as_i64().unwrap(),
                        r["message"].as_str().unwrap()
                    ],
                )
                .unwrap();
                // …and v4's rewrite of the rows, from the oracle's (a) dump.
                for p in first["profiles"].as_array().unwrap() {
                    let quoted = p["parameters"].as_str().unwrap();
                    db.execute(
                        &format!(
                            "UPDATE connection_profiles SET parameters = {quoted} WHERE id = ?1"
                        ),
                        [p["id"].as_str().unwrap()],
                    )
                    .unwrap();
                }
                db.execute(
                    "INSERT INTO connection_profiles (id, name, provider, parameters) VALUES ('anth-after-stamp', 'late', 'ANTHROPIC', '{\"temperature\":0.4}')",
                    [],
                )
                .unwrap();
            }
            "no-parameters" => seed_table(&db, false),
            "no-table" => {}
            other => panic!("unknown mode {other}"),
        }
        let (out, logs) =
            global_capture::capture(|| strip_anthropic_sampling_parameters(&db, FIXED_NOW));
        let out = out.expect("heal");
        let got = dump(&db, run_of(&out, &db));
        for key in ["run", "profiles", "ledger", "metadataKeys", "ledgerTables"] {
            assert_eq!(
                serde_json::to_string(&got[key]).unwrap(),
                serde_json::to_string(&want[key]).unwrap(),
                "{mode}: {key}"
            );
        }
        assert_logs(mode, want["logs"].as_array().unwrap(), &logs);

        if mode == "unstamped" {
            assert_eq!(want["skippedOnRerun"], Value::Bool(true));
            let again = global_capture::capture(|| {
                strip_anthropic_sampling_parameters(&db, FIXED_NOW).unwrap()
            });
            assert_eq!(again.0, LedgeredHealOutcome::AlreadyCompleted);
            assert!(
                again.1.iter().all(|l| !l.contains("quilltap::boot")),
                "the AlreadyCompleted boot logs nothing: {:?}",
                again.1
            );
        }
        eprintln!("strip heal [{mode}]: matched ({out:?})");
    }
}
