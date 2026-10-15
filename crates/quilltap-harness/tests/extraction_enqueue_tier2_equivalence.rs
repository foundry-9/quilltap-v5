//! Tier-2 differential test: the extraction **enqueue** surface —
//! `quilltap_core::services::queue_service::{enqueue_memory_extraction_with,
//! enqueue_memory_extraction_batch}` and `api::memories::chat_queue_memories` —
//! against v4's real `enqueueMemoryExtraction` / `enqueueMemoryExtractionBatch`
//! (`lib/background-jobs/queue-service.ts`) and `handleQueueMemories`
//! (`app/api/v1/chats/[id]/actions/memories.ts`) at the `83334b9a5` pin (work
//! order P4.D296 Tier 1 #14–#15). NEW.
//!
//! Both sides start from the SAME fixture pair (the shared
//! `build-memory-processor-fixture.ts` under `QT_FIXTURE_SPEC=
//! extraction-enqueue-tier2.json`) and run the SAME op list:
//!
//!   - the dedupe matrix: v4's five terms (chatId; `turnOpenerMessageId` by
//!     `===`; the anchor `?? null`; the pass through `memoryExtractionPassOf`,
//!     so an explicit `turn` matches the default, a legacy `foldOtherCatchup`
//!     row matches `other-fold`, and an unknown pass compares raw; the cursor
//!     `?? null`) — and what is NOT a term (`extractAfterMessageId`,
//!     `backfill`, `ignoreIdle`); `skipDedupCheck`; L-6's absent keys (a
//!     pending payload with NO `turnOpenerMessageId` never matches an incoming
//!     `null`; one with no anchor key does, through `?? null`) — the planted
//!     rows are rewritten by the same SQL on both sides;
//!   - the batch: `backfill: true`, `extractAfterMessageId` only when set, the
//!     priority, an empty batch;
//!   - `queue-memories`: the seat profile (with a greeting turn), a second press
//!     stacking (no dedupe in the batch), the default cheap profile, the
//!     user-defined profile and its strategy gate, a profile that does not exist
//!     (the WARN + 400), an autonomous room's bounds, and both empty-chat 400s.
//!
//! Compared per op: whether the returned id was already pending and the payload
//! BYTES of the row it names (the batch: every payload in order; the handler:
//! status + body), the whole `background_jobs` table (type, payload bytes,
//! status, priority, attempts — ordered by payload), and the captured `[Chats
//! v1]` / `[MemoryExtraction]` / `Memory extraction batch enqueued` lines.
//!
//! Generate (Node 24, from the v4 checkout or the lane's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-extraction-enqueue-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/extraction-enqueue-tier2.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/extraction-enqueue-tier2.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SPEC=extraction-enqueue-tier2.json QT_FIXTURE_OUT=/tmp/qt-extraction-enqueue-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-extraction-enqueue-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-memory-processor-fixture.ts
//!   QT_FIXTURE_ENQUEUE_MAIN=/tmp/qt-extraction-enqueue-main.db \
//!   QT_FIXTURE_ENQUEUE_MOUNT=/tmp/qt-extraction-enqueue-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-extraction-enqueue.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "extraction-enqueue-tier2\\.test\\.ts$"
//! Run:
//!   QT_ORACLE_ENQUEUE=/tmp/oracle-extraction-enqueue.ndjson \
//!   QT_FIXTURE_ENQUEUE_MAIN=/tmp/qt-extraction-enqueue-main.db \
//!   QT_FIXTURE_ENQUEUE_MOUNT=/tmp/qt-extraction-enqueue-mount.db \
//!     cargo test -p quilltap-harness --test extraction_enqueue_tier2_equivalence

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::Db;
use quilltap_core::services::queue_service::{
    enqueue_memory_extraction_batch, enqueue_memory_extraction_with, MemoryExtractionBatchEntry,
    MemoryExtractionEnqueue,
};
use serde_json::{json, Value};

#[path = "admission_support/mod.rs"]
mod admission_support;
use admission_support::{oracle_lines, render_v4};

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/extraction-enqueue-tier2.json")
}

const PREFIXES: &[&str] = &[
    "[Chats v1]",
    "[MemoryExtraction]",
    "Memory extraction batch enqueued",
];

fn status_of(kind: quilltap_core::api::ErrorKind) -> u16 {
    use quilltap_core::api::ErrorKind;
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 423,
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// A `Response` as the (status, body) pair the oracle's rows compare against.
fn status_body(r: &quilltap_core::api::Response) -> Value {
    if let quilltap_core::api::Response::Error(e) = r {
        return json!({ "status": status_of(e.kind), "body": { "error": e.message } });
    }
    let v = serde_json::to_value(r).unwrap();
    json!({ "status": 200, "body": v.get("data").cloned().unwrap_or(Value::Null) })
}

/// A numeric cell as the JS side reads it (v4's REAL columns hold integers).
fn num(v: rusqlite::types::ValueRef<'_>) -> Value {
    match v {
        rusqlite::types::ValueRef::Integer(i) => Value::from(i),
        rusqlite::types::ValueRef::Real(f) if f.fract() == 0.0 => Value::from(f as i64),
        rusqlite::types::ValueRef::Real(f) => Value::from(f),
        _ => Value::Null,
    }
}

fn jobs_of(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            r#"SELECT "type", "payload", "status", "priority", "maxAttempts", "attempts" FROM "background_jobs" ORDER BY "payload", "status""#,
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "type": r.get::<_, String>(0)?,
                    "payload": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?,
                    "priority": num(r.get_ref(3)?),
                    "maxAttempts": num(r.get_ref(4)?),
                    "attempts": num(r.get_ref(5)?),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::from(rows))
    })
    .unwrap()
}

fn job_ids(db: &Db) -> HashSet<String> {
    db.read_main(|c| {
        let mut stmt = c.prepare(r#"SELECT "id" FROM "background_jobs""#)?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(ids)
    })
    .unwrap()
}

fn payload_of(db: &Db, id: &str) -> Value {
    let id = id.to_string();
    db.read_main(move |c| {
        let p = c
            .query_row(
                r#"SELECT "payload" FROM "background_jobs" WHERE "id" = ?1"#,
                [&id],
                |r| r.get::<_, String>(0),
            )
            .ok();
        Ok(p.map(Value::from).unwrap_or(Value::Null))
    })
    .unwrap()
}

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

fn opt_str<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

#[tokio::test(flavor = "current_thread")]
async fn extraction_enqueue_tier2_matches_oracle() {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(oracle_path), Some(main), Some(mount)) = (
        env("QT_ORACLE_ENQUEUE"),
        env("QT_FIXTURE_ENQUEUE_MAIN"),
        env("QT_FIXTURE_ENQUEUE_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_ENQUEUE / QT_FIXTURE_ENQUEUE_MAIN / _MOUNT (see header).");
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
        .prefix("qt-extraction-enqueue-rust-")
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
    let profile_id = spec["connectionProfiles"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let base_cheap = serde_json::to_string(&spec["chatSettings"]["cheapLLMSettings"]).unwrap();

    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0usize;
    for (op, want) in spec["ops"].as_array().unwrap().iter().zip(&want_ops) {
        let name = op["name"].as_str().unwrap();
        assert_eq!(want["op"].as_str(), Some(name), "op order mismatch");
        let empty = json!({});
        let plants = op.get("plants").unwrap_or(&empty);
        if let Some(v) = plants.get("cheapLLMSettings") {
            exec(
                &db,
                r#"UPDATE "chat_settings" SET "cheapLLMSettings" = ?1"#,
                vec![serde_json::to_string(v).unwrap()],
            )
            .await;
        }
        let before = job_ids(&db);
        logs.lock().unwrap().clear();
        let result = match op["kind"].as_str().unwrap() {
            "enqueue" => {
                let p = &op["payload"];
                let job = MemoryExtractionEnqueue {
                    chat_id: opt_str(p, "chatId").unwrap(),
                    turn_opener_message_id: opt_str(p, "turnOpenerMessageId"),
                    extraction_anchor_message_id: opt_str(p, "extractionAnchorMessageId"),
                    extract_after_message_id: opt_str(p, "extractAfterMessageId"),
                    connection_profile_id: opt_str(p, "connectionProfileId").unwrap(),
                    pass: opt_str(p, "pass"),
                    cursor: opt_str(p, "cursor"),
                    ignore_idle: p["ignoreIdle"].as_bool() == Some(true),
                    backfill: p["backfill"].as_bool() == Some(true),
                    skip_dedup_check: op["options"]["skipDedupCheck"].as_bool() == Some(true),
                };
                match enqueue_memory_extraction_with(&db, &user_id, &job).await {
                    Ok(id) => {
                        if let Some(raw) = op.get("rewritePayload").and_then(Value::as_str) {
                            exec(
                                &db,
                                r#"UPDATE "background_jobs" SET "payload" = ?1 WHERE "id" = ?2"#,
                                vec![raw.to_string(), id.clone()],
                            )
                            .await;
                        }
                        if let Some(key) = op.get("stripKey").and_then(Value::as_str) {
                            exec(
                                &db,
                                r#"UPDATE "background_jobs" SET "payload" = json_remove("payload", '$.' || ?1) WHERE "id" = ?2"#,
                                vec![key.to_string(), id.clone()],
                            )
                            .await;
                        }
                        json!({ "reused": before.contains(&id), "payload": payload_of(&db, &id) })
                    }
                    Err(e) => json!({ "error": quilltap_core::db::fallback::error_text(&e) }),
                }
            }
            "batch" => {
                let entries: Vec<MemoryExtractionBatchEntry> = op["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| MemoryExtractionBatchEntry {
                        turn_opener_message_id: opt_str(e, "turnOpenerMessageId")
                            .map(str::to_string),
                        extraction_anchor_message_id: opt_str(e, "extractionAnchorMessageId")
                            .map(str::to_string),
                        extract_after_message_id: opt_str(e, "extractAfterMessageId")
                            .map(str::to_string),
                    })
                    .collect();
                match enqueue_memory_extraction_batch(
                    &db,
                    &user_id,
                    op["chatId"].as_str().unwrap(),
                    &profile_id,
                    &entries,
                    op["priority"].as_f64().unwrap(),
                )
                .await
                {
                    Ok(ids) => json!({
                        "payloads": ids.iter().map(|id| payload_of(&db, id)).collect::<Vec<_>>(),
                    }),
                    Err(e) => json!({ "error": quilltap_core::db::fallback::error_text(&e) }),
                }
            }
            _ => status_body(
                &quilltap_core::api::memories::chat_queue_memories(
                    &db,
                    &user_id,
                    op["chatId"].as_str().unwrap(),
                )
                .await,
            ),
        };
        let captured = logs.lock().unwrap().clone();
        if plants.get("cheapLLMSettings").is_some() {
            exec(
                &db,
                r#"UPDATE "chat_settings" SET "cheapLLMSettings" = ?1"#,
                vec![base_cheap.clone()],
            )
            .await;
        }
        ran += 1;

        if result != want["result"] {
            failures.push(format!(
                "{name}: result diverges\n  rust:   {result}\n  oracle: {}",
                want["result"]
            ));
        }
        let jobs = jobs_of(&db);
        if jobs != want["jobs"] {
            failures.push(format!(
                "{name}: background_jobs diverge\n  rust:   {jobs:#}\n  oracle: {:#}",
                want["jobs"]
            ));
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
    eprintln!("OK: extraction enqueue tier-2 matched the oracle ({ran} ops).");
}
