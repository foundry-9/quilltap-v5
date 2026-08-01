//! Tier-2 differential: the **`LLM_LOG_CLEANUP` job handler** and its
//! **`runScheduledCleanup` enqueuer** (P4.24 — v4 `handleLLMLogCleanup`,
//! `lib/background-jobs/handlers/llm-log-cleanup.ts`, and
//! `lib/background-jobs/scheduled-cleanup.ts`; dogfood findings #40 and #41).
//!
//! Both sides copy the same committed two-DB seed fixture
//! (`crates/quilltap-web/tests/fixtures/llm-log-cleanup-{main,llmlogs}.db`) and
//! drain the same eight pinned PENDING jobs through the same claim loop: claim
//! the next due job → run the handler → `markCompleted` on `Ok` /
//! `markFailed(message)` on `Err`; when nothing is due, REWIND any FAILED
//! retry-eligible job's `scheduledAt` to the epoch (raw SQL, both sides — the
//! backoff's wall-clock wait never enters the differential) until none remains.
//! The processed `(jobId, outcome, error)` sequence is asserted
//! element-for-element (pinning the claim ORDER), then `background_jobs` (main)
//! and `llm_logs` (the llm-logs PARTITION) are diffed in full, with only minted
//! timestamps placeholdered.
//!
//! ## Two timezone legs, and why one would not do
//!
//! v4's cutoff is `cutoffDate.setDate(cutoffDate.getDate() - retentionDays)` —
//! **local calendar-day** arithmetic. It agrees with `now − N × 86_400_000`
//! under UTC and differs by an hour across a DST transition, so a UTC-only
//! family is structurally blind to the entire bug class (the P4.d26 lesson).
//! The corpus is built around a pinned clock of `2026-03-15T17:00:00Z` — a week
//! after the 2026-03-08 spring-forward — with log rows placed AT each leg's
//! cutoff and inside the one-hour gap between them. The measured difference:
//! **the UTC leg leaves 16 rows and the Chicago leg leaves 12**, and the four
//! rows that separate them are precisely the ones a fixed-millisecond
//! subtraction would keep.
//!
//! Measured, not assumed: forcing the zone to `UTC` while leaving the calendar
//! step intact leaves the UTC leg **green** and turns the Chicago leg **red** —
//! so a zone-blind port really would have shipped through a one-leg family.
//! (Replacing the whole computation with `now − N × 86_400_000` reddens both,
//! because the `fractional` arm's `0.5 → day 14` truncation separates them under
//! UTC too.)
//!
//! Both legs run the whole claim loop over their own fresh copies. The oracle
//! pins its clock with jest fake timers (`Date` only) and reads the ambient
//! `TZ`; the Rust side passes the same instant and zone explicitly, because core
//! reads no ambient clock (the `day_references` seam).
//!
//! ## The enqueuer leg
//!
//! `run_scheduled_cleanup` had shipped since P4.1d with **no differential at
//! all** — which is how finding #41 (`retentionDays` serialized `7.0`) shipped.
//! The third leg drives v4's real `runScheduledCleanup` over the same main
//! fixture and diffs the minted `background_jobs` rows **including payload
//! bytes**, so the JSON rendering of the retention number is part of the diff.
//! Its first run caught a second, worse one: a `chat_settings` row whose
//! `llmLoggingSettings` cell is SQL NULL resolves through
//! `LLMLoggingSettingsSchema`'s default in v4 and IS swept, where v5 read the
//! NULL as "not configured" and dropped that user from the daily sweep forever.
//!
//! ⚠️ Every leg MUTATES its databases (cleanup deletes, the enqueuer inserts),
//! so all three work on /tmp copies keyed by pid AND leg — the committed
//! fixtures stay pristine and no two legs collide (D32's cross-family hazard).
//!
//! Rebuilding the COMMITTED fixture is a DELIBERATE act, never part of a regen
//! (the recipe-sweep policy: a rebuild mints fresh UUIDs and invalidates every
//! consumer). When a lane decides to rebuild:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_LLC_MAIN=/tmp/qt-llc-main.db QT_FIXTURE_LLC_LOGS=/tmp/qt-llc-llmlogs.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-llm-log-cleanup-fixture.ts
//!   # then, as the deliberate step: copy /tmp/qt-llc-{main,llmlogs}.db over
//!   # crates/quilltap-web/tests/fixtures/llm-log-cleanup-{main,llmlogs}.db
//!   # and regenerate ALL of this family's oracles against the new bytes.
//!
//! Generate the three oracles against the COMMITTED fixtures (Node 24, from
//! the v4 checkout; the /tmp mirror dodges jest's `/.claude/`
//! testPathIgnorePatterns; each leg gets its OWN clean invocation; the oracle
//! cases copy the DBs to per-pid /tmp scratch before mutating, so pointing at
//! the committed files is safe):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-llm-log-cleanup-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp $V5W/harness/oracle/cases/llm-log-cleanup-jobs.test.ts    "$TMPO/cases/"
//!   cp $V5W/harness/oracle/cases/llm-log-cleanup-enqueue.test.ts "$TMPO/cases/"
//!   cp $V5W/harness/oracle/fixtures/llm-log-cleanup.json         "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_LLC_LEG=UTC \
//!   QT_FIXTURE_LLC_MAIN=$V5W/crates/quilltap-web/tests/fixtures/llm-log-cleanup-main.db \
//!   QT_FIXTURE_LLC_LOGS=$V5W/crates/quilltap-web/tests/fixtures/llm-log-cleanup-llmlogs.db \
//!   QT_ORACLE_OUT=/tmp/oracle-llm-log-cleanup-UTC.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- llm-log-cleanup-jobs
//!   TZ=America/Chicago QT_LLC_LEG=America/Chicago \
//!   QT_FIXTURE_LLC_MAIN=$V5W/crates/quilltap-web/tests/fixtures/llm-log-cleanup-main.db \
//!   QT_FIXTURE_LLC_LOGS=$V5W/crates/quilltap-web/tests/fixtures/llm-log-cleanup-llmlogs.db \
//!   QT_ORACLE_OUT=/tmp/oracle-llm-log-cleanup-America-Chicago.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- llm-log-cleanup-jobs
//!   TZ=UTC \
//!   QT_FIXTURE_LLC_MAIN=$V5W/crates/quilltap-web/tests/fixtures/llm-log-cleanup-main.db \
//!   QT_FIXTURE_LLC_LOGS=$V5W/crates/quilltap-web/tests/fixtures/llm-log-cleanup-llmlogs.db \
//!   QT_ORACLE_OUT=/tmp/oracle-llm-log-cleanup-enqueue.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- llm-log-cleanup-enqueue
//!
//! Run (the harness passes the zone explicitly, so the process TZ does not
//! matter here — unlike the oracle, which reads the ambient one):
//!   QT_ORACLE_LLM_CLEANUP_UTC=/tmp/oracle-llm-log-cleanup-UTC.ndjson \
//!   QT_ORACLE_LLM_CLEANUP_CHICAGO=/tmp/oracle-llm-log-cleanup-America-Chicago.ndjson \
//!   QT_ORACLE_LLM_CLEANUP_ENQUEUE=/tmp/oracle-llm-log-cleanup-enqueue.ndjson \
//!     cargo test -p quilltap-harness --test llm_log_cleanup_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use quilltap_core::db::background_jobs::BackgroundJobsRepository;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::llm_log_cleanup_job::{handle_llm_log_cleanup, LlmLogCleanupPayload};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobW {
    id: String,
    #[allow(dead_code)]
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogW {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserW {
    key: String,
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    now_iso: String,
    timezone_legs: Vec<String>,
    users: Vec<UserW>,
    jobs: Vec<JobW>,
    logs: Vec<LogW>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/llm-log-cleanup.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn load_spec() -> (Spec, Value) {
    let text = std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}"));
    let json: Value = serde_json::from_str(&text).expect("parse spec json");
    let spec: Spec = serde_json::from_str(&text).expect("parse spec");
    (spec, json)
}

/// Fresh copies of the committed seeds, keyed by pid AND leg so concurrent legs
/// never share a file and the committed fixtures stay pristine.
fn copy_fixtures(leg: &str) -> (PathBuf, PathBuf) {
    let pid = std::process::id();
    let slug: String = leg
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let main = std::env::temp_dir().join(format!("qt-llc-main-rust-{slug}-{pid}.db"));
    let logs = std::env::temp_dir().join(format!("qt-llc-llmlogs-rust-{slug}-{pid}.db"));
    for p in [&main, &logs] {
        let _ = std::fs::remove_file(p);
    }
    std::fs::copy(fixtures_dir().join("llm-log-cleanup-main.db"), &main)
        .unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(fixtures_dir().join("llm-log-cleanup-llmlogs.db"), &logs)
        .unwrap_or_else(|e| panic!("copy llm-logs fixture: {e}"));
    (main, logs)
}

#[derive(Debug, PartialEq)]
struct ProcessedW {
    job_id: String,
    outcome: String,
    error: Option<String>,
}

/// Replace every non-null value of a timestamp column with `<ts>`. Nothing else
/// is normalized on the handler legs: no id is minted, every log row's own
/// `createdAt` is corpus-pinned, and statuses / attempts / payloads / `lastError`
/// all compare EXACT.
fn placeholder_timestamps(dump: &mut Value, table: &str, ts_columns: &[&str]) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{table}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{table}: row is not an object"));
        for col in ts_columns {
            if obj.get(*col).is_some_and(|v| !v.is_null()) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

fn read_oracle(path: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect()
}

fn take_table(rows: &[Value], table: &str) -> Value {
    let mut v = rows
        .iter()
        .find(|v| {
            v.get("kind").and_then(Value::as_str) == Some("table")
                && v.get("table").and_then(Value::as_str) == Some(table)
        })
        .unwrap_or_else(|| panic!("oracle missing table {table}"))
        .clone();
    v.as_object_mut().unwrap().remove("kind");
    v
}

/// One timezone leg of the handler differential.
async fn run_handler_leg(env_var: &str, leg: &str) {
    let Ok(oracle_path) = std::env::var(env_var) else {
        eprintln!("SKIP: set {env_var} to the oracle NDJSON (see header).");
        return;
    };
    let (spec, _spec_json) = load_spec();
    assert!(
        spec.timezone_legs.iter().any(|l| l == leg),
        "{leg} is not one of the corpus legs"
    );
    let now_ms = quilltap_core::clock::iso_to_ms(&spec.now_iso)
        .unwrap_or_else(|| panic!("parse nowIso {}", spec.now_iso));

    let oracle = read_oracle(&oracle_path);

    // The leg marker doubles as the stale-oracle guard: an NDJSON regenerated
    // for the OTHER zone (or an old one from before the legs existed) cannot
    // pass unnoticed.
    let recorded = oracle
        .iter()
        .find(|v| v.get("kind").and_then(Value::as_str) == Some("leg"))
        .unwrap_or_else(|| panic!("{env_var}: oracle carries no leg marker — regenerate"));
    assert_eq!(
        recorded.get("timezone").and_then(Value::as_str),
        Some(leg),
        "{env_var} was generated for a different timezone leg"
    );
    assert_eq!(
        recorded.get("nowIso").and_then(Value::as_str),
        Some(spec.now_iso.as_str()),
        "{env_var} was generated against a different pinned clock — regenerate"
    );

    let oracle_sequence: Vec<ProcessedW> = oracle
        .iter()
        .filter(|v| v.get("kind").and_then(Value::as_str) == Some("processed"))
        .map(|v| ProcessedW {
            job_id: v["jobId"].as_str().unwrap().to_string(),
            outcome: v["outcome"].as_str().unwrap().to_string(),
            error: v.get("error").and_then(Value::as_str).map(str::to_string),
        })
        .collect();

    // Corpus-shape assertions (not hand counts): the oracle drained exactly the
    // corpus job set, and a truncated fixture cannot pass silently.
    let spec_ids: BTreeSet<&str> = spec.jobs.iter().map(|j| j.id.as_str()).collect();
    let processed_ids: BTreeSet<&str> = oracle_sequence.iter().map(|p| p.job_id.as_str()).collect();
    assert_eq!(
        processed_ids, spec_ids,
        "{env_var}: oracle processed a different job set than the corpus — regenerate"
    );

    let (work_main, work_logs) = copy_fixtures(leg);
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: None,
            llm_logs: Some(work_logs.clone()),
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    // The claim loop — identical to the oracle's (see the module doc).
    const REWIND_SQL: &str = "UPDATE background_jobs \
         SET scheduledAt = '1999-01-01T00:00:00.000Z' \
         WHERE status = 'FAILED' AND attempts < maxAttempts";
    let mut got_sequence: Vec<ProcessedW> = Vec::new();
    let mut guard = 0;
    loop {
        guard += 1;
        assert!(guard <= 200, "claim loop failed to converge");
        let job = db
            .write(|ws| BackgroundJobsRepository::new(ws.main().connection()).claim_next_job())
            .await
            .expect("claim next job");
        let Some(job) = job else {
            let rewound = db
                .write(|ws| {
                    ws.main()
                        .connection()
                        .execute(REWIND_SQL, [])
                        .map_err(Into::into)
                })
                .await
                .expect("rewind failed jobs");
            if rewound == 0 {
                break;
            }
            continue;
        };
        let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
        let decoded = LlmLogCleanupPayload::from_json(&payload);
        // v4 passes `job.userId` — the row's own value, not the payload's.
        let outcome = handle_llm_log_cleanup(&db, &job.user_id, &decoded, now_ms, leg).await;
        let (outcome_str, error) = match outcome {
            Ok(()) => {
                let id = job.id.clone();
                db.write(move |ws| {
                    BackgroundJobsRepository::new(ws.main().connection())
                        .mark_completed(&id, None)
                        .map(|_| ())
                })
                .await
                .expect("mark completed");
                ("completed".to_string(), None)
            }
            Err(message) => {
                let (id, msg) = (job.id.clone(), message.clone());
                db.write(move |ws| {
                    BackgroundJobsRepository::new(ws.main().connection())
                        .mark_failed(&id, &msg)
                        .map(|_| ())
                })
                .await
                .expect("mark failed");
                ("failed".to_string(), Some(message))
            }
        };
        got_sequence.push(ProcessedW {
            job_id: job.id.clone(),
            outcome: outcome_str,
            error,
        });
    }

    assert_eq!(
        got_sequence.len(),
        oracle_sequence.len(),
        "[{leg}] processed-sequence length diverges\n got: {got_sequence:#?}\nwant: {oracle_sequence:#?}"
    );
    for (i, (g, w)) in got_sequence.iter().zip(oracle_sequence.iter()).enumerate() {
        assert_eq!(g, w, "[{leg}] processed sequence diverges at step {i}");
    }

    let mut got_jobs = db
        .read_main(|conn| dump_table_json_conn(conn, "background_jobs", "id"))
        .expect("dump background_jobs");
    let got_logs = db
        .read_llm_logs(|conn| dump_table_json_conn(conn, "llm_logs", "id"))
        .expect("dump llm_logs");
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_logs);

    let mut want_jobs = take_table(&oracle, "background_jobs");
    let want_logs = take_table(&oracle, "llm_logs");

    const JOB_TS: &[&str] = &["scheduledAt", "startedAt", "completedAt", "updatedAt"];
    for d in [&mut got_jobs, &mut want_jobs] {
        placeholder_timestamps(d, "background_jobs", JOB_TS);
    }
    // Nothing here writes an llm_logs row — every surviving row is corpus-pinned
    // down to its own timestamps, so the log table compares fully EXACT.
    assert_eq!(got_logs, want_logs, "[{leg}] llm_logs diverges");
    assert_eq!(got_jobs, want_jobs, "[{leg}] background_jobs diverges");

    // Shape guard: the surviving set must be a strict subset of the corpus, and
    // the legs must actually disagree (see `legs_disagree_on_the_dst_hour`).
    let corpus_logs: BTreeSet<&str> = spec.logs.iter().map(|l| l.id.as_str()).collect();
    let survivors = surviving_log_ids(&want_logs);
    assert!(
        survivors.iter().all(|id| corpus_logs.contains(id.as_str())),
        "[{leg}] a surviving log id is not in the corpus — regenerate"
    );
    assert!(
        survivors.len() < corpus_logs.len(),
        "[{leg}] nothing was deleted at all — the corpus no longer straddles the cutoff"
    );
}

fn surviving_log_ids(dump: &Value) -> BTreeSet<String> {
    dump["rows"]
        .as_array()
        .expect("rows array")
        .iter()
        .map(|r| r["id"].as_str().expect("row id").to_string())
        .collect()
}

#[tokio::test]
async fn llm_log_cleanup_jobs_match_oracle_utc() {
    run_handler_leg("QT_ORACLE_LLM_CLEANUP_UTC", "UTC").await;
}

#[tokio::test]
async fn llm_log_cleanup_jobs_match_oracle_chicago() {
    run_handler_leg("QT_ORACLE_LLM_CLEANUP_CHICAGO", "America/Chicago").await;
}

/// The family's own tripwire: if the two legs ever agree, the corpus has stopped
/// discriminating between the local calendar-day cutoff and a fixed-millisecond
/// subtraction — and a port that reverted to `now − N × 86_400_000` would sail
/// through both legs. Compares the ORACLES, so it fails on a corpus/oracle
/// regression even when the Rust side is fine.
#[test]
fn legs_disagree_on_the_dst_hour() {
    let (utc, chicago) = match (
        std::env::var("QT_ORACLE_LLM_CLEANUP_UTC"),
        std::env::var("QT_ORACLE_LLM_CLEANUP_CHICAGO"),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            eprintln!("SKIP: set QT_ORACLE_LLM_CLEANUP_{{UTC,CHICAGO}} (see header).");
            return;
        }
    };
    let utc_rows = surviving_log_ids(&take_table(&read_oracle(&utc), "llm_logs"));
    let chi_rows = surviving_log_ids(&take_table(&read_oracle(&chicago), "llm_logs"));
    let only_utc: Vec<&String> = utc_rows.difference(&chi_rows).collect();
    assert!(
        chi_rows.is_subset(&utc_rows),
        "the Chicago leg kept a row the UTC leg deleted — the corpus moved"
    );
    assert!(
        !only_utc.is_empty(),
        "the two timezone legs deleted the SAME rows: the corpus no longer \
         straddles a DST boundary, so it can no longer tell v4's calendar-day \
         cutoff apart from `now - N * 86_400_000`"
    );
}

/// The enqueuer leg — the first differential ever pointed at
/// `run_scheduled_cleanup`. Diffs the minted `background_jobs` rows INCLUDING
/// payload bytes, which is what pins finding #41's `7`-not-`7.0` rendering.
#[tokio::test]
async fn llm_log_cleanup_enqueue_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_LLM_CLEANUP_ENQUEUE") else {
        eprintln!("SKIP: set QT_ORACLE_LLM_CLEANUP_ENQUEUE to the oracle NDJSON (see header).");
        return;
    };
    let (spec, _) = load_spec();
    let oracle = read_oracle(&oracle_path);

    let summary = oracle
        .iter()
        .find(|v| v.get("kind").and_then(Value::as_str) == Some("summary"))
        .unwrap_or_else(|| panic!("oracle carries no summary row — regenerate"));
    let want_users = summary["usersProcessed"].as_u64().unwrap() as usize;
    let want_jobs = summary["jobsEnqueued"].as_u64().unwrap() as usize;

    let (work_main, work_logs) = copy_fixtures("enqueue");
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: None,
            llm_logs: Some(work_logs.clone()),
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let (users_processed, jobs_enqueued) =
        quilltap_core::services::queue_service::run_scheduled_cleanup(&db)
            .await
            .expect("run_scheduled_cleanup");
    assert_eq!(
        (users_processed, jobs_enqueued),
        (want_users, want_jobs),
        "the enqueuer's (usersProcessed, jobsEnqueued) diverges"
    );

    let mut got = db
        .read_main(|conn| dump_table_json_conn(conn, "background_jobs", "id"))
        .expect("dump background_jobs");
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_logs);

    let mut want = take_table(&oracle, "background_jobs");

    // A minted job's id and every one of its timestamps are run-time values; a
    // corpus-pinned job's are not, and stay EXACT. `payload` is never
    // normalized on either kind — it is the point of this leg.
    let pinned: BTreeSet<&str> = spec.jobs.iter().map(|j| j.id.as_str()).collect();
    let user_keys: HashMap<&str, &str> = spec
        .users
        .iter()
        .map(|u| (u.id.as_str(), u.key.as_str()))
        .collect();
    for d in [&mut got, &mut want] {
        let rows = d["rows"].as_array_mut().expect("rows array");
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().unwrap();
            let id = obj["id"].as_str().unwrap().to_string();
            if pinned.contains(id.as_str()) {
                continue;
            }
            obj.insert("id".into(), Value::String("<minted>".into()));
            let cols: Vec<String> = obj.keys().cloned().collect();
            for col in cols {
                if col.ends_with("At") && obj.get(&col).is_some_and(|v| !v.is_null()) {
                    obj.insert(col, Value::String("<ts>".into()));
                }
            }
        }
        // Minted rows share an id placeholder, so sort by (userId, id, payload)
        // — the userId is what identifies which arm minted which row.
        rows.sort_by_key(|r| {
            format!(
                "{}|{}|{}",
                r["userId"].as_str().unwrap_or(""),
                r["id"].as_str().unwrap_or(""),
                r["payload"].as_str().unwrap_or("")
            )
        });
    }

    assert_eq!(got, want, "the enqueued background_jobs rows diverge");

    // Shape guard: name the arms that enqueued, so a corpus that quietly stopped
    // covering one of v4's filter branches shows up as a failure rather than as
    // a smaller, still-matching diff.
    let minted_users: BTreeSet<&str> = want["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == "<minted>")
        .map(|r| {
            *user_keys
                .get(r["userId"].as_str().unwrap())
                .expect("minted row for an unknown user")
        })
        .collect();
    assert_eq!(
        minted_users,
        BTreeSet::from(["bystander", "fractional", "nullBag", "windowed"]),
        "the enqueuer's swept set changed — every branch of v4's \
         `enabled && retentionDays > 0` filter must stay covered"
    );
}
