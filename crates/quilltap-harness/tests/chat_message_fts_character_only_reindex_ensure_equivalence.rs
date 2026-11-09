//! P4.D363 — real-DB differential: v5's
//! `db::chat_message_fts_character_only_reindex` against v4's REAL
//! `reindex-chat-message-fts-character-only-v1` migration + REAL ledger
//! (`migrations/state.ts`) + REAL DDL module, at `624d0d25c`, over the shared
//! spec `harness/oracle/fixtures/chat-message-fts-character-only-reindex-ensure.json`.
//!
//! The oracle follows v4's RUNNER sequence (ledger → `shouldRun` → `run` →
//! `recordCompletedMigration`); this side calls v5's boot ensure, which
//! reproduces it, plus `should_run` alone. Compared per scenario: the path,
//! `shouldRun`, the result's `itemsAffected` / `message` (the ledger row's),
//! the `chat_messages_fts_map` rows, the five objects' `sqlite_master` text
//! AFTER, the ledger rows (`completedAt` / `quilltapVersion` normalized away
//! — the id is the key) and metadata KEYS, and every log line in order — the
//! migration's two INFO lines (`quilltap::db`,
//! `context=migrations.reindex-chat-message-fts-character-only`) and the DDL
//! module's debug lines (`quilltap::db`, `context=db.chat-message-fts`), by
//! level, message and fields (`durationMs` a clock; arrays as `…Json`).
//!
//! ## Named both-ways tables
//!
//! - **`DEPENDENCY_PENDING`** — on the `missing-all` base (no FTS objects:
//!   a fresh v5 instance at this point of its boot) v4's `run()` THROWS
//!   `no such table: chat_messages_fts_map` (it counts the map before the
//!   ensure; v4 never reaches it, since `dependsOn` runs
//!   `create-chat-message-fts-v1` first). v5's literal [`run`] throws the same
//!   sentence — compared — and v5's ENTRY POINT answers `DependencyPending`,
//!   writing and logging nothing (the dependency gate). The table lists
//!   exactly the scenarios where the paths part; the test fails if one stops
//!   parting or another starts.
//! - **`FRESH_INSTANCE_REINDEX_STAMP` RETIRED by measurement** (the
//!   coordinator's ruling, 2026-11-09): the `fresh-create-then-reindex` row
//!   takes v4's REAL `create-chat-message-fts-v1` through the runner's sequence
//!   FIRST on a base with no FTS objects, then the reindex. v4 RUNS create and
//!   leaves the reindex UNSTAMPED (`shouldRun` false). v5's boot order on the
//!   same base — the reindex (`DependencyPending`), then the PHASE 3.65
//!   reconcile, v5's twin of create — must end the same way: no reindex row,
//!   and the same index (map + `sqlite_master`). v5 writes no `create` row
//!   (it has no runner; v4's `shouldRun` for create is false on the result).
//! - **`RUNNER_CREATES_LEDGER_TABLES`** — v4's `loadMigrationState()` creates
//!   the two ledger tables on every runner pass, before any migration is
//!   asked; v5 has no runner, and its skip arms create nothing. An absent
//!   ledger table on v5's side compares as v4's empty one.
//!
//! ## Regenerating
//!
//! ```bash
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-fts-reindex-ensure-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/chat-message-fts-character-only-reindex-ensure.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/chat-message-fts-character-only-reindex-ensure.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-fts-reindex-ensure.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "chat-message-fts-character-only-reindex-ensure\.test\.ts$"
//!   QT_ORACLE_FTS_REINDEX_ENSURE=/tmp/oracle-fts-reindex-ensure.ndjson \
//!     cargo test -p quilltap-harness --test chat_message_fts_character_only_reindex_ensure_equivalence -- --nocapture
//! ```
//!
//! With `QT_ORACLE_FTS_REINDEX_ENSURE` unset the differential SKIPs (and says so).

use std::path::PathBuf;

use quilltap_core::db::chat_message_fts::{
    chat_message_fts_object_names, ensure_chat_message_fts_schema,
};
use quilltap_core::db::chat_message_fts_character_only_reindex::{
    reindex_chat_message_fts_character_only, run, should_run, ReindexOutcome, MIGRATION_ID,
};
use quilltap_core::db::text_compression::register_qt_text;
use quilltap_core::test_support::captured_with;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

const NOW: &str = "2026-11-04T12:00:00.000Z";

/// `(scenario, v4 path, v5 entry outcome)` where the two part — see the
/// module doc.
const DEPENDENCY_PENDING: &[(&str, &str, &str)] = &[("missing-all", "threw", "dependency-pending")];

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "chatMessagesDdl")]
    chat_messages_ddl: String,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    #[serde(default, rename = "priorLedger")]
    prior_ledger: bool,
    #[serde(default, rename = "noChatMessages")]
    no_chat_messages: bool,
    #[serde(default, rename = "noFts")]
    no_fts: bool,
    #[serde(default)]
    damage: Vec<String>,
    seed: Vec<Seed>,
    #[serde(default, rename = "afterSeed")]
    after_seed: Vec<String>,
    #[serde(default, rename = "createFirst")]
    create_first: bool,
}

#[derive(Deserialize)]
struct Seed {
    id: String,
    role: String,
    content: String,
    #[serde(default, rename = "systemSender")]
    system_sender: Option<String>,
    #[serde(default, rename = "customAnnouncer")]
    custom_announcer: Option<String>,
}

#[derive(Deserialize, Debug)]
struct OracleLog {
    source: String,
    level: String,
    message: String,
    context: Value,
}

fn spec() -> Spec {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-message-fts-character-only-reindex-ensure.json");
    serde_json::from_str(&std::fs::read_to_string(&path).expect("read spec")).expect("parse spec")
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

/// v4's ledger DDL + one completed row, as `recordCompletedMigration` writes it.
fn plant_prior_ledger(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\n        \"id\" TEXT PRIMARY KEY,\n        \"completedAt\" TEXT NOT NULL,\n        \"quilltapVersion\" TEXT NOT NULL,\n        \"itemsAffected\" INTEGER NOT NULL DEFAULT 0,\n        \"message\" TEXT\n      );\n      CREATE TABLE IF NOT EXISTS \"migrations_metadata\" (\n        \"key\" TEXT PRIMARY KEY,\n        \"value\" TEXT NOT NULL\n      );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) VALUES (?1, '2026-11-01T00:00:00.000Z', '4.12.0', 2, 'Removed 2 Staff messages from the search index')",
        [MIGRATION_ID],
    )
    .unwrap();
    for k in ["lastChecked", "quilltapVersion"] {
        conn.execute(
            "INSERT INTO migrations_metadata (key, value) VALUES (?1, 'x')",
            [k],
        )
        .unwrap();
    }
}

fn build(spec: &Spec, s: &Scenario) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    register_qt_text(&conn).unwrap();
    if !s.no_chat_messages {
        conn.execute_batch(&spec.chat_messages_ddl).unwrap();
        if !s.no_fts {
            ensure_chat_message_fts_schema(&conn).unwrap();
        }
        for sql in &s.damage {
            conn.execute_batch(sql).unwrap();
        }
        for r in &s.seed {
            conn.execute(
                r#"INSERT INTO "chat_messages" ("id","chatId","type","role","content","systemSender","customAnnouncer","createdAt") VALUES (?,?,?,?,?,?,?,?)"#,
                rusqlite::params![
                    r.id,
                    "chat-1",
                    "message",
                    r.role,
                    r.content,
                    r.system_sender,
                    r.custom_announcer,
                    format!("2026-01-01T00:00:{}.000Z", &r.id[r.id.len() - 2..])
                ],
            )
            .unwrap();
        }
        for sql in &s.after_seed {
            conn.execute_batch(sql).unwrap();
        }
    }
    if s.prior_ledger {
        plant_prior_ledger(&conn);
    }
    conn
}

fn rows(conn: &Connection, sql: &str, cols: &[&str]) -> Value {
    let mut stmt = conn.prepare(sql).unwrap();
    let out: Vec<Value> = stmt
        .query_map([], |row| {
            let mut m = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                let v: rusqlite::types::Value = row.get(i)?;
                m.insert(
                    (*c).to_string(),
                    match v {
                        rusqlite::types::Value::Null => Value::Null,
                        rusqlite::types::Value::Integer(n) => json!(n),
                        rusqlite::types::Value::Text(t) => json!(t),
                        other => json!(format!("{other:?}")),
                    },
                );
            }
            Ok(Value::Object(m))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    Value::Array(out)
}

fn master(conn: &Connection) -> Value {
    let names = chat_message_fts_object_names();
    let sql = format!(
        "SELECT name, type, sql FROM sqlite_master WHERE name IN ({}) ORDER BY name",
        names
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(",")
    );
    rows(conn, &sql, &["name", "type", "sql"])
}

fn assert_log(line: &str, want: &OracleLog, at: &str) {
    let level = want.level.to_uppercase();
    assert!(
        line.starts_with(&format!("{level} quilltap::db")),
        "{at}: level/target: {line}"
    );
    assert!(
        line.contains(&want.message),
        "{at}: message {:?}: {line}",
        want.message
    );
    if let Value::Object(ctx) = &want.context {
        for (key, value) in ctx {
            if key == "durationMs" {
                continue;
            }
            let needle = match value {
                Value::Array(_) => format!("{key}Json={}", serde_json::to_string(value).unwrap()),
                Value::String(s) => format!("{key}={s}"),
                other => format!("{key}={other}"),
            };
            assert!(line.contains(&needle), "{at}: missing `{needle}`: {line}");
        }
    }
    let ctx = if want.source == "migration" {
        "context=migrations.reindex-chat-message-fts-character-only"
    } else {
        "context=db.chat-message-fts"
    };
    assert!(line.contains(ctx), "{at}: {ctx}: {line}");
    // v4's closing migration line carries no `durationMs`.
    if want.source == "migration" {
        assert!(!line.contains("durationMs"), "{at}: {line}");
    }
}

#[test]
fn chat_message_fts_character_only_reindex_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_FTS_REINDEX_ENSURE") else {
        eprintln!("SKIP: set QT_ORACLE_FTS_REINDEX_ENSURE to the oracle NDJSON (see header).");
        return;
    };
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    let spec = spec();
    assert_eq!(oracle.len(), spec.scenarios.len(), "regenerate the oracle");

    let mut parted = Vec::new();
    for (i, s) in spec.scenarios.iter().enumerate() {
        let want = &oracle[i];
        assert_eq!(want["scenario"], json!(s.name));
        let conn = build(&spec, s);

        if s.create_first {
            // v5's boot order: the reindex, then the reconcile.
            let (outcome, lines) =
                captured_with(|| reindex_chat_message_fts_character_only(&conn, NOW).unwrap());
            assert_eq!(outcome, ReindexOutcome::DependencyPending, "{}", s.name);
            assert!(
                !lines
                    .iter()
                    .any(|l| l
                        .contains("context=migrations.reindex-chat-message-fts-character-only")),
                "{}: {lines:?}",
                s.name
            );
            quilltap_core::db::chat_message_fts_reconcile::reconcile_chat_message_fts(&conn);
            assert_eq!(
                want["create"]["path"],
                json!("ran"),
                "{}: v4's create",
                s.name
            );
            assert_eq!(
                want["path"],
                json!("not-needed"),
                "{}: v4's reindex",
                s.name
            );
            // v4 leaves the reindex UNSTAMPED; v5 the same.
            let v4_reindex_rows = want["ledger"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["id"] == MIGRATION_ID)
                .count();
            let v5_reindex_rows = if table_exists(&conn, "migrations_state") {
                rows(
                    &conn,
                    &format!("SELECT id FROM migrations_state WHERE id = '{MIGRATION_ID}'"),
                    &["id"],
                )
                .as_array()
                .unwrap()
                .len()
            } else {
                0
            };
            assert_eq!(
                (v5_reindex_rows, v4_reindex_rows),
                (0, 0),
                "{}: FRESH_INSTANCE_REINDEX_STAMP",
                s.name
            );
            let map = rows(
                &conn,
                r#"SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId""#,
                &["ftsId", "messageId"],
            );
            assert_eq!(map, want["map"], "{}: map", s.name);
            assert_eq!(master(&conn), want["master"], "{}: sqlite_master", s.name);
            continue;
        }

        // `shouldRun` alone (v4 asks it only when not completed).
        if want["path"] != json!("already-completed") {
            let got = table_exists(&conn, "chat_messages") && should_run(&conn).unwrap();
            assert_eq!(json!(got), want["shouldRun"], "{}: shouldRun", s.name);
        }

        let (outcome, lines) =
            captured_with(|| reindex_chat_message_fts_character_only(&conn, NOW).unwrap());
        let v5_path = match &outcome {
            ReindexOutcome::AlreadyCompleted => "already-completed",
            ReindexOutcome::NotApplicable => "not-applicable",
            ReindexOutcome::DependencyPending => "dependency-pending",
            ReindexOutcome::NotNeeded => "not-needed",
            ReindexOutcome::Ran { .. } => "ran",
        };
        let v4_path = want["path"].as_str().unwrap();
        let ours: Vec<&String> = lines
            .iter()
            .filter(|l| {
                l.contains("context=migrations.reindex-chat-message-fts-character-only")
                    || l.contains("context=db.chat-message-fts")
            })
            .collect();

        if v5_path != v4_path {
            parted.push((s.name.as_str(), v4_path, v5_path));
            // DEPENDENCY_PENDING: v5's entry wrote and logged nothing, and
            // v5's LITERAL `run` throws v4's sentence on the same base.
            assert!(ours.is_empty(), "{}: {ours:?}", s.name);
            assert!(!table_exists(&conn, "migrations_state"), "{}", s.name);
            let err = run(&conn).unwrap_err();
            let msg = match &err {
                quilltap_core::db::DbError::Sqlite(inner) => inner.to_string(),
                other => other.to_string(),
            };
            assert_eq!(json!(msg), want["error"], "{}: run's throw", s.name);
            continue;
        }

        let theirs: Vec<OracleLog> = serde_json::from_value(want["logs"].clone()).unwrap();
        assert_eq!(
            ours.len(),
            theirs.len(),
            "{}: log COUNT\n  rust: {ours:#?}\n  oracle: {theirs:#?}",
            s.name
        );
        for (n, w) in theirs.iter().enumerate() {
            assert_log(ours[n], w, &format!("{} log {n}", s.name));
        }

        let map = if table_exists(&conn, "chat_messages_fts_map") {
            rows(
                &conn,
                r#"SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId""#,
                &["ftsId", "messageId"],
            )
        } else {
            Value::Null
        };
        assert_eq!(map, want["map"], "{}: map", s.name);
        assert_eq!(master(&conn), want["master"], "{}: sqlite_master", s.name);

        // RUNNER_CREATES_LEDGER_TABLES: an absent table compares as empty.
        let ledger = if table_exists(&conn, "migrations_state") {
            rows(
                &conn,
                "SELECT id, itemsAffected, message FROM migrations_state ORDER BY id",
                &["id", "itemsAffected", "message"],
            )
        } else {
            json!([])
        };
        let metadata = if table_exists(&conn, "migrations_metadata") {
            rows(
                &conn,
                "SELECT key FROM migrations_metadata ORDER BY key",
                &["key"],
            )
        } else {
            json!([])
        };
        assert_eq!(
            serde_json::to_string(&ledger).unwrap(),
            serde_json::to_string(&want["ledger"]).unwrap(),
            "{}: ledger",
            s.name
        );
        assert_eq!(metadata, want["metadata"], "{}: metadata keys", s.name);
        if let ReindexOutcome::Ran { dropped, .. } = &outcome {
            assert_eq!(
                json!(dropped),
                want["result"]["itemsAffected"],
                "{}",
                s.name
            );
        }
    }
    assert_eq!(
        parted,
        DEPENDENCY_PENDING.to_vec(),
        "DEPENDENCY_PENDING drifted"
    );
    eprintln!(
        "OK: chat_message_fts_character_only_reindex matched oracle over {} scenarios.",
        spec.scenarios.len()
    );
}
