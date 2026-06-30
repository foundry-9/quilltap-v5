//! Tier-2 differential test: the `background_jobs` repo (Phase-2, main DB).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-background-jobs-fixture.ts), run the SAME
//! op sequence from the committed spec (create / claimNextJob / markFailed×2 /
//! pause / resume / resetAllProcessingJobs / update), dump the `background_jobs`
//! table canonically (sorted by id), and assert the post-op state is identical.
//!
//! ⚠️ MINTED-TIMESTAMP placeholder normalization (like `embedding_status`, but
//! over four columns). `background_jobs` has NO base-method override — create /
//! update / delete honor pinned id/createdAt/updatedAt — BUT several queue ops
//! (`claimNextJob`, `markFailed`, `pause`, `resume`, `resetAllProcessingJobs`)
//! mint `now` (and `markFailed` mints `now + backoff`) UNCONDITIONALLY from the
//! system clock. So a pure zero-normalization form is impossible. ids +
//! createdAt are pinned and diffed EXACTLY; every DETERMINISTIC column (status,
//! attempts, lastError, payload, priority, maxAttempts) is diffed EXACTLY (this
//! is what proves the queue logic — e.g. markFailed's DEAD-vs-FAILED branch, the
//! attempts increment on claim, the em-dash lastError on reset); only the four
//! mintable timestamp columns (`scheduledAt`, `startedAt`, `completedAt`,
//! `updatedAt`) are collapsed to a `<ts>` placeholder on BOTH sides.
//!
//! Marshaling banked: the open-JSON `payload` object column (kept {}/single-key —
//! the multi-key key-order seam), three REAL-affinity number columns (`priority`,
//! `attempts`, `maxAttempts` — bare `z.number()`, bound `f64`, integer-collapsed
//! by `js_number_to_json` in the dump), the `userId` UserOwned column, and
//! nullable TEXT (`lastError`, `startedAt`, `completedAt`).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-bj-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-background-jobs-fixture.ts
//!   QT_FIXTURE_BJ=/tmp/qt-bj-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/background-jobs.ts \
//!     > /tmp/oracle-bj.ndjson
//! Run:
//!   QT_ORACLE_BJ=/tmp/oracle-bj.ndjson \
//!   QT_FIXTURE_BJ=/tmp/qt-bj-fixture.db \
//!     cargo test -p quilltap-harness --test background_jobs_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::background_jobs::{BjCreate, BjUpdate, CreateOptions};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The committed fixture spec — the single source driving both ports.
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        data: CreateData,
        options: CreateOpts,
    },
    #[serde(rename = "update")]
    Update { id: String, data: UpdateData },
    #[serde(rename = "delete")]
    Delete { id: String },
    #[serde(rename = "claimNextJob")]
    ClaimNextJob {
        #[serde(default, rename = "expectClaimId")]
        expect_claim_id: Option<String>,
    },
    #[serde(rename = "markFailed")]
    MarkFailed { id: String, error: String },
    #[serde(rename = "markCompleted")]
    MarkCompleted {
        id: String,
        #[serde(default)]
        result: Option<Value>,
    },
    #[serde(rename = "pause")]
    Pause { id: String },
    #[serde(rename = "resume")]
    Resume { id: String },
    #[serde(rename = "resetAllProcessingJobs")]
    ResetAllProcessingJobs {
        #[serde(default, rename = "expectCount")]
        expect_count: Option<usize>,
    },
    #[serde(rename = "resetStuckJobs")]
    ResetStuckJobs {
        #[serde(rename = "timeoutMinutes")]
        timeout_minutes: i64,
        #[serde(default, rename = "expectCount")]
        expect_count: Option<usize>,
    },
    #[serde(rename = "cancel")]
    Cancel {
        id: String,
        #[serde(default, rename = "expectModified")]
        expect_modified: Option<bool>,
    },
    #[serde(rename = "cancelByType")]
    CancelByType {
        #[serde(rename = "type")]
        job_type: String,
        #[serde(default, rename = "expectCount")]
        expect_count: Option<usize>,
    },
    #[serde(rename = "deleteByTypesAndStatuses")]
    DeleteByTypesAndStatuses {
        types: Vec<String>,
        statuses: Vec<String>,
        #[serde(default, rename = "expectCount")]
        expect_count: Option<usize>,
    },
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "type")]
    job_type: String,
    #[serde(default)]
    status: Option<String>,
    payload: Value,
    priority: f64,
    attempts: f64,
    #[serde(rename = "maxAttempts")]
    max_attempts: f64,
    #[serde(default, rename = "lastError")]
    last_error: Option<String>,
    #[serde(rename = "scheduledAt")]
    scheduled_at: String,
    #[serde(default, rename = "startedAt")]
    started_at: Option<String>,
    #[serde(default, rename = "completedAt")]
    completed_at: Option<String>,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
struct UpdateData {
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default, rename = "type")]
    job_type: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    payload: Option<Value>,
    #[serde(default)]
    priority: Option<f64>,
    #[serde(default)]
    attempts: Option<f64>,
    #[serde(default, rename = "maxAttempts")]
    max_attempts: Option<f64>,
    #[serde(default, rename = "lastError")]
    last_error: Option<String>,
    #[serde(default, rename = "scheduledAt")]
    scheduled_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

/// The minted (nondeterministic) timestamp columns — placeholdered on both sides.
const TS_COLUMNS: &[&str] = &["scheduledAt", "startedAt", "completedAt", "updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/background-jobs-tier2.json")
}

/// Collapse each non-null minted timestamp column to a `<ts>` placeholder, in
/// place, on a `{ table, columns, rows }` dump. Every other field (ids,
/// createdAt, status, attempts, lastError, payload, priority, maxAttempts) is
/// left exact so the structural diff still catches real divergence.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

#[test]
fn background_jobs_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_BJ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_BJ to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_BJ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_BJ to the seed fixture .db (see test header).");
            return;
        }
    };

    // Parse the committed spec (pepper + op sequence) — same file the oracle used.
    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    // Parse the oracle's expected post-op dump (one NDJSON object).
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work = std::env::temp_dir().join(format!("qt-bj-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.background_jobs();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &BjCreate {
                            user_id: data.user_id.clone(),
                            job_type: data.job_type.clone(),
                            status: data.status.clone(),
                            payload: data.payload.clone(),
                            priority: data.priority,
                            attempts: data.attempts,
                            max_attempts: data.max_attempts,
                            last_error: data.last_error.clone(),
                            scheduled_at: data.scheduled_at.clone(),
                            started_at: data.started_at.clone(),
                            completed_at: data.completed_at.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("background_jobs.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &BjUpdate {
                                user_id: data.user_id.clone(),
                                job_type: data.job_type.clone(),
                                status: data.status.clone(),
                                payload: data.payload.clone(),
                                priority: data.priority,
                                attempts: data.attempts,
                                max_attempts: data.max_attempts,
                                // The spec models lastError as "set to this value";
                                // wrap in Some(Some(_)) to set the column.
                                last_error: data.last_error.clone().map(Some),
                                scheduled_at: data.scheduled_at.clone(),
                                started_at: None,
                                completed_at: None,
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("background_jobs.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("background_jobs.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
                Op::ClaimNextJob { expect_claim_id } => {
                    let claimed = repo
                        .claim_next_job()
                        .expect("background_jobs.claim_next_job");
                    if let Some(expected) = expect_claim_id {
                        let got = claimed.as_ref().map(|j| j.id.as_str());
                        assert_eq!(
                            got,
                            Some(expected.as_str()),
                            "claimNextJob claimed the wrong job"
                        );
                    }
                }
                Op::MarkFailed { id, error } => {
                    let res = repo
                        .mark_failed(id, error)
                        .expect("background_jobs.mark_failed");
                    assert!(res.is_some(), "markFailed target {id} not found in fixture");
                }
                Op::MarkCompleted { id, result } => {
                    let res = repo
                        .mark_completed(id, result.as_ref())
                        .expect("background_jobs.mark_completed");
                    assert!(
                        res.is_some(),
                        "markCompleted target {id} not found in fixture"
                    );
                }
                Op::Pause { id } => {
                    let res = repo.pause(id).expect("background_jobs.pause");
                    assert!(res.is_some(), "pause target {id} not found / not pausable");
                }
                Op::Resume { id } => {
                    let res = repo.resume(id).expect("background_jobs.resume");
                    assert!(
                        res.is_some(),
                        "resume target {id} not found / not resumable"
                    );
                }
                Op::ResetAllProcessingJobs { expect_count } => {
                    let count = repo
                        .reset_all_processing_jobs()
                        .expect("background_jobs.reset_all_processing_jobs");
                    if let Some(expected) = expect_count {
                        assert_eq!(count, *expected, "resetAllProcessingJobs count diverged");
                    }
                }
                Op::ResetStuckJobs {
                    timeout_minutes,
                    expect_count,
                } => {
                    let count = repo
                        .reset_stuck_jobs(*timeout_minutes)
                        .expect("background_jobs.reset_stuck_jobs");
                    if let Some(expected) = expect_count {
                        assert_eq!(count, *expected, "resetStuckJobs count diverged");
                    }
                }
                Op::Cancel {
                    id,
                    expect_modified,
                } => {
                    let modified = repo.cancel(id).expect("background_jobs.cancel");
                    if let Some(expected) = expect_modified {
                        assert_eq!(modified, *expected, "cancel modified flag diverged");
                    }
                }
                Op::CancelByType {
                    job_type,
                    expect_count,
                } => {
                    let count = repo
                        .cancel_by_type(job_type)
                        .expect("background_jobs.cancel_by_type");
                    if let Some(expected) = expect_count {
                        assert_eq!(count, *expected, "cancelByType count diverged");
                    }
                }
                Op::DeleteByTypesAndStatuses {
                    types,
                    statuses,
                    expect_count,
                } => {
                    let count = repo
                        .delete_by_types_and_statuses(types, statuses)
                        .expect("background_jobs.delete_by_types_and_statuses");
                    if let Some(expected) = expect_count {
                        assert_eq!(count, *expected, "deleteByTypesAndStatuses count diverged");
                    }
                }
            }
        }
    }

    let mut got = writer
        .dump_table_json("background_jobs", "id")
        .expect("dump background_jobs");

    let _ = std::fs::remove_file(&work);

    // One normalization (placeholder the minted timestamps), applied to both dumps.
    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label). assert_eq on serde_json::Value is order-independent for
    // object keys and exact for the row arrays (both sides sorted by id).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: background_jobs tier-2 matched oracle ({n} rows).");
}
