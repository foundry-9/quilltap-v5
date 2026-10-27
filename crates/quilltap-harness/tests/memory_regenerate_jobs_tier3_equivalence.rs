//! Tier-3 differential test: the memory-**regenerate** job handlers —
//! `quilltap_core::services::memory_regenerate_job::{handle_memory_regenerate_chat,
//! handle_memory_regenerate_all}` — against v4's real `handleMemoryRegenerateChat`
//! / `handleMemoryRegenerateAll` at the `83334b9a5` pin (work order P4.D296 R-V).
//! NEW; mirrors v4's own `memory-regenerate-chat-fold-catchup.test.ts` on real
//! repositories.
//!
//! Both sides start from the SAME fixture pair (the shared
//! `build-memory-processor-fixture.ts` under `QT_FIXTURE_SPEC=
//! memory-regenerate-jobs-tier3.json`: a salon with a greeting turn and planted
//! cursors on all three passes, an autonomous room, a third salon, an
//! Unmoderated chat, seeded memories incl. a MANUAL row and two orphan chat ids)
//! and run the SAME op list: the wipe + rebuild (cursors cleared, MANUAL spared,
//! one turn job per turn without the dedupe scan, then the catch-ups with the
//! `regenerate:<jobId>` anchor), an autonomous room's bounded turns, turn mode's
//! episode-only catch-up, the profile throw BEFORE any wipe, the orphan arm, and
//! two fan-outs (an in-flight wipe skipped, the Unmoderated chat routed through
//! the dangerous profile, an orphan chat id; then every wipe already in flight).
//!
//! Compared per op: the thrown message (or none), the `background_jobs` table
//! (type, payload BYTES, status, priority, attempts), the cursor table, the
//! `memories` rows (id, holder, chat, source, content — the wipe deletes, never
//! mints), and the captured `[MemoryRegenerateChat]` / `[MemoryRegenerateAll]`
//! lines.
//!
//! Generate (Node 24, from the v4 checkout or the lane's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-memory-regenerate-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/memory-regenerate-jobs-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/memory-regenerate-jobs-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SPEC=memory-regenerate-jobs-tier3.json QT_FIXTURE_OUT=/tmp/qt-memory-regenerate-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-memory-regenerate-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
//!   QT_FIXTURE_REGENERATE_MAIN=/tmp/qt-memory-regenerate-main.db \
//!   QT_FIXTURE_REGENERATE_MOUNT=/tmp/qt-memory-regenerate-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memory-regenerate.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "memory-regenerate-jobs-tier3\\.test\\.ts$"
//! Run:
//!   QT_ORACLE_REGENERATE=/tmp/oracle-memory-regenerate.ndjson \
//!   QT_FIXTURE_REGENERATE_MAIN=/tmp/qt-memory-regenerate-main.db \
//!   QT_FIXTURE_REGENERATE_MOUNT=/tmp/qt-memory-regenerate-mount.db \
//!     cargo test -p quilltap-harness --test memory_regenerate_jobs_tier3_equivalence

use std::sync::{Arc, Mutex};

use quilltap_core::db::memory_extraction_cursors::ExtractionCursorPass;
use quilltap_core::db::runtime::Db;
use quilltap_core::services::memory_regenerate_job::{
    handle_memory_regenerate_all, handle_memory_regenerate_chat, MemoryRegenerateAllPayload,
    MemoryRegenerateChatPayload,
};
use quilltap_core::services::queue_service::enqueue_memory_regenerate_chat;
use serde_json::{json, Value};

#[path = "admission_support/mod.rs"]
mod admission_support;
use admission_support::{oracle_lines, render_v4};

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-regenerate-jobs-tier3.json")
}

const PREFIXES: &[&str] = &[
    "[MemoryRegenerateChat]",
    "[MemoryRegenerateAll]",
    // P4.168 (survey B5): the repository fallback lines the fan-out's three
    // reads log.
    "Error finding entities by filter",
    "Error finding background jobs by user ID",
    "Error listing distinct memory chatIds",
];

/// P4.168 (P-4, the planner's ruling) — `REGENERATE_ALL_STRICT_CHAT_LIST`,
/// pinned both ways: with the chats table unreadable, v4's `findByUserId`
/// fallback answers `[]` and the fan-out queues an orphan `MEMORY_REGENERATE_
/// CHAT` wipe for EVERY chat that holds memories (no error); v5 logs v4's
/// line, fails the job, and queues nothing.
const REGENERATE_ALL_STRICT_CHAT_LIST: &str = "fan_out_chat_list_fails_strict";

/// A cell as the JS side reads it (v4's REAL columns hold integers).
fn cell(v: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) if f.fract() == 0.0 => Value::from(f as i64),
        ValueRef::Real(f) => Value::from(f),
        ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Null => Value::Null,
        ValueRef::Blob(b) => Value::from(hex::encode(b)),
    }
}

fn dump(db: &Db, sql: &'static str) -> Value {
    db.read_main(move |c| {
        let mut stmt = c.prepare(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt
            .query_map([], |r| {
                let mut o = serde_json::Map::new();
                for (i, n) in names.iter().enumerate() {
                    o.insert(n.clone(), cell(r.get_ref(i)?));
                }
                Ok(Value::Object(o))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::from(rows))
    })
    .unwrap()
}

const JOBS_SQL: &str = r#"SELECT "type", "payload", "status", "priority", "maxAttempts", "attempts" FROM "background_jobs" ORDER BY "payload", "status""#;
const CURSORS_SQL: &str =
    r#"SELECT "id", "chatId", "pass", "messageId" FROM "memory_extraction_cursors" ORDER BY "id""#;
const MEMORIES_SQL: &str =
    r#"SELECT "id", "characterId", "chatId", "source", "content" FROM "memories" ORDER BY "id""#;

async fn exec(db: &Db, sql: &str, args: Vec<String>) {
    let sql = sql.to_string();
    db.write(move |w| {
        w.main()
            .connection()
            .execute(&sql, rusqlite::params_from_iter(args.iter()))
            .map_err(quilltap_core::db::DbError::from)
    })
    .await
    .expect("plant");
}

#[tokio::test(flavor = "current_thread")]
async fn memory_regenerate_jobs_tier3_matches_oracle() {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(oracle_path), Some(main), Some(mount)) = (
        env("QT_ORACLE_REGENERATE"),
        env("QT_FIXTURE_REGENERATE_MAIN"),
        env("QT_FIXTURE_REGENERATE_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_REGENERATE / QT_FIXTURE_REGENERATE_MAIN / _MOUNT (see header)."
        );
        return;
    };
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec")).unwrap();
    let oracle = oracle_lines(&oracle_path);
    let want_ops: Vec<&Value> = oracle.iter().filter(|l| l["kind"] == "op").collect();
    assert_eq!(
        want_ops.len(),
        spec["ops"].as_array().unwrap().len(),
        "op-count mismatch — regenerate the oracle"
    );

    let dir = tempfile::Builder::new()
        .prefix("qt-memory-regenerate-rust-")
        .tempdir()
        .unwrap();
    let paths = admission_support::copy_pair(&main, &mount, dir.path());
    let db =
        Db::open(paths, spec["testPepperBase64"].as_str().unwrap()).expect("open fixture copy");
    db.write(|w| {
        let conn = w.main().connection();
        quilltap_core::test_support::ensure_p4d171_columns(conn);
        quilltap_core::test_support::ensure_p4d182_columns(conn);
        quilltap_core::test_support::ensure_p4d225_columns(conn);
        Ok(())
    })
    .await
    .unwrap();

    use tracing_subscriber::layer::SubscriberExt;
    let logs = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(admission_support::Capture {
        lines: logs.clone(),
        owned: &[],
        prefixes: PREFIXES,
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    let user_id = spec["userId"].as_str().unwrap().to_string();
    let standard_profile = spec["connectionProfiles"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut failures: Vec<String> = Vec::new();
    for (op, want) in spec["ops"].as_array().unwrap().iter().zip(&want_ops) {
        let name = op["name"].as_str().unwrap();
        assert_eq!(want["op"].as_str(), Some(name), "op order mismatch");
        let empty = json!({});
        let plants = op.get("plants").unwrap_or(&empty);
        if let Some(cursor) = plants.get("cursor").and_then(Value::as_object) {
            let chat_id = cursor["chatId"].as_str().unwrap().to_string();
            for (pass, message_id) in cursor.iter().filter(|(k, _)| k.as_str() != "chatId") {
                let (c, m) = (chat_id.clone(), message_id.as_str().unwrap().to_string());
                let pass = ExtractionCursorPass::parse(pass).expect("cursor pass");
                db.write(move |w| {
                    w.main()
                        .memory_extraction_cursors()
                        .set_cursor(&c, pass, &m)
                })
                .await
                .unwrap();
            }
        }
        if let Some(v) = plants.get("mode") {
            exec(
                &db,
                r#"INSERT INTO "instance_settings" ("key", "value") VALUES ('memoryExtractionMode', ?1)"#,
                vec![serde_json::to_string(v).unwrap()],
            )
            .await;
        }
        if let Some(chat_id) = plants.get("inFlightWipe").and_then(Value::as_str) {
            enqueue_memory_regenerate_chat(&db, &user_id, chat_id, &standard_profile)
                .await
                .expect("plant an in-flight wipe");
        }

        for sql in plants
            .get("sqlBefore")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            exec(&db, sql.as_str().unwrap(), vec![]).await;
        }

        logs.lock().unwrap().clear();
        let job_id = format!("job-{name}");
        let error = if op["kind"] == "chat" {
            handle_memory_regenerate_chat(
                &db,
                &job_id,
                &user_id,
                &MemoryRegenerateChatPayload::decode(&op["payload"]),
            )
            .await
        } else {
            handle_memory_regenerate_all(
                &db,
                &job_id,
                &user_id,
                &MemoryRegenerateAllPayload::decode(&op["payload"]),
            )
            .await
        }
        .err();
        let captured = logs.lock().unwrap().clone();
        for sql in plants
            .get("sqlAfter")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            exec(&db, sql.as_str().unwrap(), vec![]).await;
        }
        if name == REGENERATE_ALL_STRICT_CHAT_LIST {
            // Both ways: v4 no error + ≥1 orphan wipe queued; v5 the error +
            // NOTHING queued; both log v4's chats-table line first.
            let got_jobs = dump(&db, JOBS_SQL);
            let v4_wipes = want["jobs"].as_array().map_or(0, Vec::len);
            let line = |l: &String| {
                l.starts_with("ERROR Error finding entities by filter collection=chats ")
            };
            let v4_line = want["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(render_v4)
                .any(|l| line(&l));
            let held = want["error"].is_null()
                && v4_wipes > 0
                && v4_line
                && error.is_some()
                && got_jobs.as_array().is_some_and(Vec::is_empty)
                && captured.first().is_some_and(line);
            if held {
                eprintln!(
                    "[{name}] REGENERATE_ALL_STRICT_CHAT_LIST holds (v4 queued {v4_wipes} orphan wipe(s); v5 failed the job: {error:?})."
                );
            } else {
                failures.push(format!(
                    "{name}: REGENERATE_ALL_STRICT_CHAT_LIST moved\n  v4 error {} / {v4_wipes} job(s) / line {v4_line}\n  v5 error {error:?} / jobs {got_jobs} / lines {captured:?}",
                    want["error"]
                ));
            }
            if plants.get("mode").is_some() {
                unreachable!("the strict op plants no mode");
            }
            continue;
        }
        if plants.get("mode").is_some() {
            exec(
                &db,
                r#"DELETE FROM "instance_settings" WHERE "key" = 'memoryExtractionMode'"#,
                vec![],
            )
            .await;
        }

        let got_error = error.map(Value::from).unwrap_or(Value::Null);
        if got_error != want["error"] {
            failures.push(format!(
                "{name}: error diverges\n  rust:   {got_error}\n  oracle: {}",
                want["error"]
            ));
        }
        for (key, sql) in [
            ("jobs", JOBS_SQL),
            ("cursors", CURSORS_SQL),
            ("memories", MEMORIES_SQL),
        ] {
            let got = dump(&db, sql);
            if got != want[key] {
                failures.push(format!(
                    "{name}: {key} diverge\n  rust:   {got:#}\n  oracle: {:#}",
                    want[key]
                ));
            }
        }
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4)
            .collect();
        if captured != want_lines {
            failures.push(format!(
                "{name}: captured lines diverge\n  rust:\n    {}\n  oracle:\n    {}",
                captured.join("\n    "),
                want_lines.join("\n    ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!(
        "OK: memory regenerate tier-3 matched the oracle ({} ops).",
        want_ops.len()
    );
}
