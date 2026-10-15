//! P4.D297 tier-2 differential: the maintenance QUEUE, the watermark trigger,
//! the daily scheduler, the job handler and the completion region's commit
//! hooks — `quilltap_core::services::queue_service`'s P4.D297 region,
//! `memory_maintenance::{triggers, scheduler, ledger, after_job}` and
//! `memory_maintenance_job` against v4's REAL `enqueueMemoryMaintenance` /
//! `…Many` / `…ForUser`, `maybeEnqueueMaintenance`,
//! `runScheduledMemoryMaintenance` / `runStartupTick`,
//! `handleMemoryMaintenance`, the ledger, and the dispatcher's success arm
//! (`recordMaintenanceOutcome` + `mirrorCommonplaceAfterCommit`) at the
//! `83334b9a5` pin.
//!
//! The steps (`harness/oracle/fixtures/memory-maintenance-queue-steps.json`)
//! run IN ORDER on ONE copy of the maintenance fixture pair on each side.
//! Covered: the ledger seeded from the planted jobs (COMPLETED / PROCESSING
//! only, a payload without a `characterId` skipped, a non-maintenance type
//! never read); the enqueue — a new job (`maxAttempts` 1, the normalized
//! `exemptPhases`), the per-character WIDEN (the phase union, `undefined`
//! absorbing, `manual` winning the reason, the exemption union, the settings
//! merge), a PROCESSING job never absorbing, a deferred `scheduledAt`, the
//! batch (one pending read, no duplicate ids) and the user-wide fan-out; the
//! watermark (the debounce, the cap watermark over DEMOTABLE rows, the demote
//! throttle after an enqueue, a seeded consolidate throttle, the archived /
//! unknown arms, the consolidation watermark, both switches off, the
//! ineffective-demotion backoff after a real run); the scheduled pass (both
//! off; a pass absorbed by pending jobs) and the startup tick (runs; skipped
//! behind a recent scheduled completion); the handler (no characterId; the
//! defer behind an OLDER PROCESSING run, twice — the second widening the
//! successor; a run older than the running one; a full fold / demote /
//! retire run in Job mode, its fold through the host-RPC seam; a `dryRun`
//! payload running LIVE — v4's handler never passes it — with the hook
//! standing down; the whole automatic run) each followed by the completion
//! region (`after_job_completed` — the ledger's status read back); and the
//! mirror hook (a touched bucket mirrored; a dry payload, another job type,
//! nothing touched).
//!
//! Comparands: each step's value (job ids, the scheduled result, the ledger
//! status — v4's key order, compared as JSON values) and its captured lines
//! (`memory:maintenance*`, `jobs:memory-maintenance`,
//! `memory:commonplace-mirror`, `[Maintenance]`; the façade's
//! `[MemorySettings]` line by TAG only — P4.D288's bytes); the `jobs`
//! snapshots and the final `background_jobs` dump (payload TEXT byte-compared,
//! `scheduledAt − createdAt` to the second); the embeddings the fold
//! re-embeds; the `memories` table (`updatedAt` normalized); the
//! `Commonplace/` documents. Normalized on both sides: every minted id (to its
//! order of first appearance), every ISO timestamp, every `durationMs`, and the
//! ledger's epoch `at` — both sides read their OWN wall clock for the planted
//! jobs and the run stamps.
//!
//! Generate (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-maintenance-queue-oracle}
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
//!   QT_FIXTURE_MOUNT_OUT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-maintenance-fixture.ts
//!   rm -f ${QT_OUT:-/tmp/qt-maintenance-queue.ndjson}
//!   QT_FIXTURE_MAINTENANCE_MAIN=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
//!   QT_FIXTURE_MAINTENANCE_MOUNT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
//!   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-maintenance-queue.ndjson} \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-queue-tier2
//!   cd $V5W
//!   QT_ORACLE_MAINTENANCE_QUEUE=${QT_OUT:-/tmp/qt-maintenance-queue.ndjson} \
//!   QT_FIXTURE_MAINTENANCE_MAIN=${QT_MAIN:-/tmp/qt-maintenance-main.db} \
//!   QT_FIXTURE_MAINTENANCE_MOUNT=${QT_MOUNT:-/tmp/qt-maintenance-mount.db} \
//!     cargo test -p quilltap-harness --test memory_maintenance_queue_tier2_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::background_jobs::BackgroundJob;
use quilltap_core::db::instance_settings::{
    set_memory_consolidation_settings, set_memory_maintenance_settings,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::CannedCompletionProvider;
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::memory_maintenance::after_job::mirror_commonplace_after_job;
use quilltap_core::services::memory_maintenance::after_job_completed;
use quilltap_core::services::memory_maintenance::ledger::{
    get_maintenance_ledger_status, reset_maintenance_ledger_for_tests,
};
use quilltap_core::services::memory_maintenance::run::run_maintenance;
use quilltap_core::services::memory_maintenance::runner::{
    MaintenanceRunFuture, MaintenanceRunner,
};
use quilltap_core::services::memory_maintenance::scheduler::{
    run_scheduled_memory_maintenance, should_run_startup_tick,
};
use quilltap_core::services::memory_maintenance::triggers::maybe_enqueue_maintenance;
use quilltap_core::services::memory_maintenance::types::{MaintenanceContext, MaintenanceRequest};
use quilltap_core::services::memory_maintenance_job::handle_memory_maintenance;
use quilltap_core::services::queue_service::{
    enqueue_memory_maintenance, enqueue_memory_maintenance_for_user,
    enqueue_memory_maintenance_many,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Map, Value};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

/// The handler's run "as of" the fixture's instant — the oracle's ONE clock
/// seam over the real `runMaintenance` (an omitted `ctx.now` filled from
/// `spec.now`).
struct FixtureClockRunner {
    completion: CannedCompletionProvider,
    embedding: CannedEmbeddingProvider,
    executor: CheapLlmTaskExecutor,
    now_ms: i64,
}

impl MaintenanceRunner for FixtureClockRunner {
    fn run<'a>(
        &'a self,
        db: &'a Db,
        request: MaintenanceRequest,
        mut ctx: MaintenanceContext,
    ) -> MaintenanceRunFuture<'a> {
        ctx.now_ms = ctx.now_ms.or(Some(self.now_ms));
        Box::pin(run_maintenance(
            db,
            &self.completion,
            &self.embedding,
            &self.executor,
            request,
            ctx,
        ))
    }
}

fn approx_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            x == y || (x - y).abs() <= 1e-12
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| approx_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((k1, v1), (k2, v2))| k1 == k2 && approx_eq(v1, v2))
        }
        _ => a == b,
    }
}

fn is_uuid_at(b: &[u8], i: usize) -> bool {
    const DASHES: [usize; 4] = [8, 13, 18, 23];
    if i + 36 > b.len() {
        return false;
    }
    (0..36).all(|k| {
        let c = b[i + k];
        if DASHES.contains(&k) {
            c == b'-'
        } else {
            c.is_ascii_hexdigit()
        }
    })
}

fn is_iso_at(b: &[u8], i: usize) -> bool {
    // YYYY-MM-DDTHH:MM:SS.sssZ
    const PATTERN: &[u8] = b"dddd-dd-ddTdd:dd:dd.dddZ";
    i + PATTERN.len() <= b.len()
        && PATTERN.iter().enumerate().all(|(k, p)| match p {
            b'd' => b[i + k].is_ascii_digit(),
            other => b[i + k] == *other,
        })
}

/// Per-side normalizer: minted ids → `<id#n>` by first appearance, ISO
/// timestamps → `<ts>`.
struct Masker {
    known: HashSet<String>,
    minted: HashMap<String, usize>,
}

impl Masker {
    fn new(known: &HashSet<String>) -> Self {
        Masker {
            known: known.clone(),
            minted: HashMap::new(),
        }
    }

    fn text(&mut self, s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < b.len() {
            if is_uuid_at(b, i) {
                let id = &s[i..i + 36];
                if self.known.contains(&id.to_ascii_lowercase()) {
                    out.push_str(id);
                } else {
                    let n = self.minted.len();
                    let k = *self.minted.entry(id.to_string()).or_insert(n);
                    out.push_str(&format!("<id#{k}>"));
                }
                i += 36;
            } else if is_iso_at(b, i) {
                out.push_str("<ts>");
                i += 24;
            } else {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out
    }

    fn value(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.text(s)),
            Value::Array(a) => Value::Array(a.iter().map(|x| self.value(x)).collect()),
            Value::Object(o) => {
                let mut m = Map::new();
                for (k, x) in o {
                    let x = match (k.as_str(), x) {
                        ("durationMs", Value::Number(_)) => json!(0),
                        ("at", Value::Number(_)) => json!(0),
                        ("scheduledDelayMs", Value::Number(n)) => {
                            json!((n.as_f64().unwrap() / 1000.0).round() * 1000.0)
                        }
                        _ => self.value(x),
                    };
                    m.insert(self.text(k), x);
                }
                Value::Object(m)
            }
            other => other.clone(),
        }
    }
}

fn mask_numeric_field(s: &str, field: &str) -> String {
    let mut out = s.to_string();
    let mut from = 0;
    while let Some(rel) = out[from..].find(field) {
        let start = from + rel + field.len();
        let end = out[start..]
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .map_or(out.len(), |e| start + e);
        out.replace_range(start..end, "<n>");
        from = start + 3;
    }
    out
}

fn render_oracle_line(line: &Value) -> String {
    let mut out = format!(
        "{} {}",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
    for (k, v) in line["context"].as_object().unwrap() {
        match v {
            Value::String(s) => out.push_str(&format!(" {k}={s}")),
            Value::Array(_) | Value::Object(_) => {
                out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
            }
            other => out.push_str(&format!(" {k}={other}")),
        }
    }
    out
}

fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _ = parts.next();
    format!("{level} {}", parts.next().unwrap_or_default())
}

fn finish_line(m: &mut Masker, s: &str) -> String {
    let s = mask_numeric_field(s, "durationMs=");
    let s = mask_numeric_field(&s, "\"at\":");
    m.text(&s)
}

const SETTINGS_TAG: &str = "[MemorySettings] Resolved memory settings";

fn v5_captured(line: &str) -> bool {
    let message = line.split_once(' ').map_or("", |(_, rest)| rest);
    !line.contains(SETTINGS_TAG)
        && (line.contains(" module=memory:maintenance")
            || line.contains(" module=jobs:memory-maintenance")
            || line.contains(" module=memory:commonplace-mirror")
            || message.starts_with("[Maintenance]"))
}

fn sql_rows(db: &Db, sql: &str, mount: bool) -> Vec<Value> {
    let read = |conn: &rusqlite::Connection| -> Result<Vec<Value>, quilltap_core::db::DbError> {
        let mut stmt = conn.prepare(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map([], |row| {
            let mut m = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v: rusqlite::types::Value = row.get(i)?;
                m.insert(
                    name.clone(),
                    match v {
                        rusqlite::types::Value::Null => Value::Null,
                        rusqlite::types::Value::Integer(i) => json!(i),
                        rusqlite::types::Value::Real(f) => json!(f),
                        rusqlite::types::Value::Text(s) => json!(s),
                        rusqlite::types::Value::Blob(b) => {
                            json!(b.iter().map(|x| format!("{x:02x}")).collect::<String>())
                        }
                    },
                );
            }
            Ok(Value::Object(m))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    };
    if mount {
        db.read_mount_index(read).unwrap()
    } else {
        db.read_main(read).unwrap()
    }
}

fn dump_jobs(db: &Db) -> Value {
    let rows = sql_rows(
        db,
        "SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, \
         scheduledAt, createdAt FROM background_jobs ORDER BY type, payload, status",
        false,
    );
    Value::Array(
        rows.into_iter()
            .map(|r| {
                let delay = quilltap_core::clock::iso_to_ms(r["scheduledAt"].as_str().unwrap())
                    .unwrap()
                    - quilltap_core::clock::iso_to_ms(r["createdAt"].as_str().unwrap()).unwrap();
                json!({
                    "id": r["id"], "userId": r["userId"], "type": r["type"], "status": r["status"],
                    "payload": r["payload"], "priority": r["priority"], "attempts": r["attempts"],
                    "maxAttempts": r["maxAttempts"], "lastError": r["lastError"],
                    "scheduledDelayMs": delay,
                })
            })
            .collect(),
    )
}

/// `'@holder'` → the holder's id, in values AND object keys.
fn resolve(spec: &Value, v: &Value) -> Value {
    let id = |key: &str| -> String {
        match key {
            "unknown" => spec["unknownId"].as_str().unwrap().to_string(),
            "subject" => spec["subject"]["id"].as_str().unwrap().to_string(),
            k => spec["holders"][k]["id"].as_str().unwrap().to_string(),
        }
    };
    match v {
        Value::String(s) => match s.strip_prefix('@') {
            Some(k) => Value::String(id(k)),
            None => v.clone(),
        },
        Value::Array(a) => Value::Array(a.iter().map(|x| resolve(spec, x)).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, x) in o {
                let k = match k.strip_prefix('@') {
                    Some(h) => id(h),
                    None => k.clone(),
                };
                m.insert(k, resolve(spec, x));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

fn collect_uuids(text: &str, into: &mut HashSet<String>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if is_uuid_at(b, i) {
            into.insert(text[i..i + 36].to_ascii_lowercase());
            i += 36;
        } else {
            i += 1;
        }
    }
}

async fn plant_job(db: &Db, spec: &Value, job: &Value) {
    let at = quilltap_core::clock::iso_from_unix_ms(
        quilltap_core::clock::now_unix_ms()
            - (job["hoursAgo"].as_f64().unwrap() * 3_600_000.0) as i64,
    );
    let (id, user, kind, status) = (
        job["id"].as_str().unwrap().to_string(),
        spec["userId"].as_str().unwrap().to_string(),
        job["type"].as_str().unwrap().to_string(),
        job["status"].as_str().unwrap().to_string(),
    );
    let payload = serde_json::to_string(&resolve(spec, &job["payload"])).unwrap();
    db.write(move |w| {
        w.main().connection().execute(
            "INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, \
             maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 1, NULL, ?6, NULL, NULL, ?6, ?6)",
            rusqlite::params![id, user, kind, status, payload, at],
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

fn object(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn memory_maintenance_queue_tier2_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_MAINTENANCE_QUEUE"),
        std::env::var("QT_FIXTURE_MAINTENANCE_MAIN"),
        std::env::var("QT_FIXTURE_MAINTENANCE_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_MAINTENANCE_QUEUE + QT_FIXTURE_MAINTENANCE_MAIN / _MOUNT (see header).");
        return;
    };
    global_capture::install();
    reset_maintenance_ledger_for_tests();
    let spec_text =
        std::fs::read_to_string(fixtures_dir().join("memory-maintenance-tier2.json")).unwrap();
    let steps_text =
        std::fs::read_to_string(fixtures_dir().join("memory-maintenance-queue-steps.json"))
            .unwrap();
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let corpus: Value = serde_json::from_str(&steps_text).unwrap();
    let mut known = HashSet::new();
    collect_uuids(&spec_text, &mut known);
    collect_uuids(&steps_text, &mut known);
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    let mut embedding = CannedEmbeddingProvider::new();
    let mut embedding_rows = 0;
    for row in oracle.iter().filter(|o| o["kind"] == "embedding") {
        embedding_rows += 1;
        embedding = embedding.with_vector(
            row["text"].as_str().unwrap().to_string(),
            row["vector"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap() as f32)
                .collect(),
        );
    }
    let runner = FixtureClockRunner {
        completion: CannedCompletionProvider::new(),
        embedding,
        executor: CheapLlmTaskExecutor::new(),
        now_ms: quilltap_core::clock::iso_to_ms(spec["now"].as_str().unwrap()).unwrap(),
    };

    let scratch = tempfile::Builder::new()
        .prefix("qt-maintenance-queue-rust-")
        .tempdir()
        .unwrap();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(&fixture_main, &main).unwrap();
    std::fs::copy(&fixture_mount, &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        spec["testPepperBase64"].as_str().unwrap(),
    )
    .expect("open the fixture copies");
    let user_id = spec["userId"].as_str().unwrap().to_string();

    for sql in corpus["plants"].as_array().unwrap() {
        let sql = sql.as_str().unwrap().to_string();
        db.write(move |w| Ok(w.main().connection().execute_batch(&sql)?))
            .await
            .unwrap();
    }
    for job in corpus["jobs"].as_array().unwrap() {
        plant_job(&db, &spec, job).await;
    }
    let base = quilltap_core::clock::now_unix_ms();

    let oracle_steps: Vec<&Value> = oracle.iter().filter(|o| o["kind"] == "step").collect();
    let mut want_mask = Masker::new(&known);
    let mut got_mask = Masker::new(&known);
    let mut failures: Vec<String> = Vec::new();
    let mut lines_compared = 0usize;
    let mut steps_compared = 0usize;
    let mut oracle_iter = oracle_steps.into_iter();
    for raw in corpus["steps"].as_array().unwrap() {
        let step = resolve(&spec, raw);
        let op = step["op"].as_str().unwrap();
        match op {
            "sql" => {
                let sql = step["sql"].as_str().unwrap().to_string();
                db.write(move |w| Ok(w.main().connection().execute_batch(&sql)?))
                    .await
                    .unwrap();
                continue;
            }
            "job" => {
                plant_job(&db, &spec, &step["job"]).await;
                continue;
            }
            "maintenanceSettings" => {
                let v = step["value"].clone();
                db.write(move |w| set_memory_maintenance_settings(w.main().connection(), &v))
                    .await
                    .unwrap();
                continue;
            }
            "consolidationSettings" => {
                let v = step["value"].clone();
                db.write(move |w| set_memory_consolidation_settings(w.main().connection(), &v))
                    .await
                    .unwrap();
                continue;
            }
            _ => {}
        }
        let name = step["name"].as_str().unwrap().to_string();
        let want = oracle_iter
            .next()
            .unwrap_or_else(|| panic!("{name}: oracle has fewer steps — regenerate"));
        assert_eq!(
            want["name"],
            name.as_str(),
            "step order — regenerate the oracle"
        );
        let (value, captured) = global_capture::capture_async(Box::pin(async {
            match op {
                "jobs" => dump_jobs(&db),
                "status" => Value::Array(
                    get_maintenance_ledger_status(&db, step["characterId"].as_str())
                        .iter()
                        .map(|s| s.to_json())
                        .collect(),
                ),
                "enqueue" => {
                    let scheduled = step["scheduledOffsetMs"].as_i64().map(|off| {
                        quilltap_core::clock::iso_from_unix_ms(
                            quilltap_core::clock::now_unix_ms() + off,
                        )
                    });
                    json!(enqueue_memory_maintenance(
                        &db,
                        &user_id,
                        object(&step["payload"]),
                        scheduled
                    )
                    .await
                    .unwrap())
                }
                "enqueueMany" => json!(enqueue_memory_maintenance_many(
                    &db,
                    &user_id,
                    &strings(&step["characterIds"]),
                    object(&step["payload"]),
                    None
                )
                .await
                .unwrap()),
                "enqueueForUser" => json!(enqueue_memory_maintenance_for_user(
                    &db,
                    &user_id,
                    object(&step["payload"])
                )
                .await
                .unwrap()),
                "maybeEnqueue" => json!(
                    maybe_enqueue_maintenance(
                        &db,
                        &strings(&step["ids"]),
                        (base + step["offsetMs"].as_i64().unwrap()) as f64
                    )
                    .await
                ),
                "scheduled" => {
                    let r = run_scheduled_memory_maintenance(&db).await.unwrap();
                    json!({"charactersScanned": r.characters_scanned, "jobsEnqueued": r.jobs_enqueued})
                }
                "startupTick" => {
                    if should_run_startup_tick(&db, base + step["offsetMs"].as_i64().unwrap()) {
                        run_scheduled_memory_maintenance(&db).await.unwrap();
                    }
                    json!("done")
                }
                "handler" => {
                    let j = &step["job"];
                    let at = quilltap_core::clock::iso_from_unix_ms(
                        quilltap_core::clock::now_unix_ms()
                            - (j["hoursAgo"].as_f64().unwrap() * 3_600_000.0) as i64,
                    );
                    let job = BackgroundJob {
                        id: j["id"].as_str().unwrap().to_string(),
                        user_id: user_id.clone(),
                        job_type: "MEMORY_MAINTENANCE".to_string(),
                        status: "PROCESSING".to_string(),
                        payload: serde_json::to_string(&j["payload"]).unwrap(),
                        priority: 0.0,
                        attempts: 0.0,
                        max_attempts: 1.0,
                        last_error: None,
                        scheduled_at: at.clone(),
                        started_at: None,
                        completed_at: None,
                        created_at: at.clone(),
                        updated_at: at,
                    };
                    handle_memory_maintenance(&db, &runner, &job, "UTC").await;
                    // The completion region, after the job's writes have landed.
                    after_job_completed(&db, &job).await;
                    json!("completed")
                }
                "hook" => {
                    let touched: Vec<(String, Vec<String>)> = step["touched"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| (t[0].as_str().unwrap().to_string(), strings(&t[1])))
                        .collect();
                    mirror_commonplace_after_job(
                        &db,
                        step["jobType"].as_str().unwrap(),
                        &step["payload"],
                        &touched,
                    )
                    .await;
                    json!("done")
                }
                other => panic!("unknown op {other}"),
            }
        }))
        .await;
        steps_compared += 1;
        let (g, w) = (got_mask.value(&value), want_mask.value(&want["value"]));
        if !approx_eq(&g, &w) {
            failures.push(format!("{name}: value\n  v5 {g}\n  v4 {w}"));
        }
        let got_lines: Vec<String> = captured
            .iter()
            .map(|l| strip_target(l))
            .filter(|l| v5_captured(l))
            .map(|l| finish_line(&mut got_mask, &l))
            .collect();
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["message"] != SETTINGS_TAG)
            .map(|l| finish_line(&mut want_mask, &render_oracle_line(l)))
            .collect();
        lines_compared += want_lines.len();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
        let got_tag = captured.iter().filter(|l| l.contains(SETTINGS_TAG)).count();
        let want_tag = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["message"] == SETTINGS_TAG)
            .count();
        if got_tag != want_tag {
            failures.push(format!(
                "{name}: [MemorySettings] tag v5 {got_tag} v4 {want_tag}"
            ));
        }
    }
    assert!(
        oracle_iter.next().is_none(),
        "oracle has more steps — regenerate"
    );

    // ── the final jobs table, the memories table, the documents ────────────
    let want_jobs = &oracle.iter().find(|o| o["kind"] == "jobs").unwrap()["rows"];
    let (g, w) = (got_mask.value(&dump_jobs(&db)), want_mask.value(want_jobs));
    if !approx_eq(&g, &w) {
        failures.push(format!("background_jobs:\n  v5 {g:#}\n  v4 {w:#}"));
    }
    let norm_rows = |rows: Vec<Value>| -> Value {
        Value::Array(
            rows.into_iter()
                .map(|mut r| {
                    r["updatedAt"] = json!("<ts>");
                    r
                })
                .collect(),
        )
    };
    let want_rows = norm_rows(
        oracle.iter().find(|o| o["kind"] == "table").unwrap()["rows"]
            .as_array()
            .unwrap()
            .clone(),
    );
    let got_rows = norm_rows(sql_rows(
        &db,
        "SELECT * FROM memories ORDER BY characterId, id",
        false,
    ));
    if !approx_eq(&got_rows, &want_rows) {
        let (ga, wa) = (got_rows.as_array().unwrap(), want_rows.as_array().unwrap());
        let first = ga
            .iter()
            .zip(wa.iter())
            .position(|(a, b)| !approx_eq(a, b))
            .unwrap_or(ga.len().min(wa.len()));
        failures.push(format!(
            "memories: {} vs {} rows, first divergence at {first}:\n  v5 {}\n  v4 {}",
            ga.len(),
            wa.len(),
            ga.get(first).cloned().unwrap_or(Value::Null),
            wa.get(first).cloned().unwrap_or(Value::Null)
        ));
    }
    let want_docs =
        want_mask.value(&oracle.iter().find(|o| o["kind"] == "documents").unwrap()["rows"]);
    let got_docs = got_mask.value(&Value::Array(sql_rows(
        &db,
        "SELECT l.mountPointId, l.relativePath, d.content FROM doc_mount_file_links l \
         JOIN doc_mount_documents d ON d.fileId = l.fileId \
         WHERE l.relativePath LIKE 'Commonplace/%' ORDER BY l.relativePath, d.content",
        true,
    )));
    if got_docs != want_docs {
        failures.push(format!(
            "commonplace documents:\n  v5 {got_docs:#}\n  v4 {want_docs:#}"
        ));
    }

    reset_maintenance_ledger_for_tests();
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(steps_compared >= 40, "steps compared: {steps_compared}");
    assert!(lines_compared >= 60, "lines compared: {lines_compared}");
    assert!(embedding_rows >= 2, "embeddings {embedding_rows}");
    assert_eq!(
        want_docs.as_array().map(Vec::len),
        Some(1),
        "the hook's mirrored bucket"
    );
    eprintln!(
        "OK: memory maintenance queue tier-2 matched oracle ({steps_compared} steps, {lines_compared} lines, {embedding_rows} re-embeds, the jobs table, the memories table, the Commonplace documents)."
    );
}
