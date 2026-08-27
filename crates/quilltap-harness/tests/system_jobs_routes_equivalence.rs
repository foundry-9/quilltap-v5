//! P4.9G1 tasks-queue / jobs route-surface differential: direct-drives the Rust
//! `api::system_data::*` free functions over a FRESH copy of the committed
//! `system-data-*` fixture family per case and diffs each response {status, body}
//! against v4's REAL route handlers (the `system-jobs-routes` oracle).
//!
//! The processor status is PINNED identically on both sides (the oracle mocks
//! v4's processor module; here we pass a fixed `ProcessorStatus`) — so the diff
//! proves the DB-derived fields, not the nondeterministic pump snapshot. Mutation
//! cases (pause/resume) normalize the fresh `updatedAt`/`scheduledAt`.
//!
//! Generate the oracle (see the .test.ts header), then run:
//!   QT_ORACLE_SYSTEM_JOBS=/tmp/oracle-system-jobs.ndjson \
//!     cargo test -p quilltap-harness --test system_jobs_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::system_data::{self, ProcessorStatus};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::instance_settings::get_max_concurrent_jobs;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";

const JOB_PENDING: &str = "b6000000-0000-4000-8000-000000000001";
const JOB_PROCESSING: &str = "b6000000-0000-4000-8000-000000000002";
const JOB_PAUSED: &str = "b6000000-0000-4000-8000-000000000007";
const JOB_PENDING_2: &str = "b6000000-0000-4000-8000-000000000008";
const JOB_MISSING: &str = "b6000000-0000-4000-8000-0000000000ff";

const PINNED: ProcessorStatus = ProcessorStatus {
    running: false,
    processing: false,
    in_flight: 0,
    child_crashed: false,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-sysjobs-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("system-data-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("system-data-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest | ErrorKind::Unprocessable => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// v4's routes map `Unprocessable`→422 but our concurrency validation uses
/// `BadRequest`; the out-of-range case is status-only so this never differs.
fn outcome(resp: &Response) -> (u16, Value) {
    match resp {
        Response::System(v) => (200, v.clone()),
        Response::Error(e) => (status_of(e.kind), json!({ "error": e.message })),
        other => panic!("unexpected response variant: {other:?}"),
    }
}

fn read_max(db: &Db) -> i64 {
    db.read_main(get_max_concurrent_jobs)
        .expect("read max concurrent")
}

/// Replace `body.job.<field>` (or top-level `<field>`) with a sentinel so the
/// wall-clock mutation fields compare equal.
fn normalize(body: &mut Value, fields: &[Value]) {
    for f in fields {
        let key = f.as_str().unwrap();
        if let Some(job) = body.get_mut("job").and_then(Value::as_object_mut) {
            if job.contains_key(key) {
                job.insert(key.to_string(), Value::String("<NORM>".into()));
            }
        }
    }
}

#[test]
fn system_jobs_routes_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SYSTEM_JOBS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SYSTEM_JOBS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;

    // Each closure returns (status, body, extra) for the named case.
    let mut check = |name: &str, status: u16, mut body: Value, extra: Option<Value>| {
        let Some(exp) = oracle.get(name) else {
            failed.push(format!("{name}: MISSING from oracle"));
            return;
        };
        let exp_status = exp["status"].as_u64().unwrap() as u16;
        if status != exp_status {
            failed.push(format!("{name}: status {status} != oracle {exp_status}"));
            return;
        }
        let status_only = exp
            .get("statusOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !status_only {
            let mut exp_body = exp["body"].clone();
            if let Some(norm) = exp.get("normalize").and_then(Value::as_array) {
                normalize(&mut body, norm);
                normalize(&mut exp_body, norm);
            }
            if body != exp_body {
                failed.push(format!(
                    "{name}: body mismatch\n  rust:   {body}\n  oracle: {exp_body}"
                ));
                return;
            }
        }
        if let Some(exp_extra) = exp.get("extra") {
            match &extra {
                Some(got) if got == exp_extra => {}
                other => {
                    failed.push(format!(
                        "{name}: extra mismatch\n  rust:   {other:?}\n  oracle: {exp_extra}"
                    ));
                    return;
                }
            }
        }
        ran += 1;
        eprintln!("OK {name}");
    };

    // --- read cases (no mutation) share one db ---
    {
        let db = fresh_db("reads");
        let (s, b) = outcome(&system_data::tasks_queue(&db, USER, &PINNED, read_max(&db)));
        check("tasks_queue_get", s, b, None);

        let (s, b) = outcome(&system_data::job_concurrency_get_response(read_max(&db)));
        check("job_concurrency_get_default", s, b, None);

        let (s, b) = outcome(&system_data::tasks_queue_control_response("start", &PINNED));
        check("tasks_queue_start", s, b, None);
        let (s, b) = outcome(&system_data::tasks_queue_control_response("stop", &PINNED));
        check("tasks_queue_stop", s, b, None);

        let resp = match system_data::validate_control_action("nope") {
            Ok(()) => unreachable!(),
            Err(r) => r,
        };
        let (s, b) = outcome(&resp);
        check("tasks_queue_bogus_action", s, b, None);

        let (s, b) = outcome(&system_data::job_get(&db, JOB_PENDING));
        check("job_get_existing", s, b, None);
        let (s, b) = outcome(&system_data::job_get(&db, JOB_MISSING));
        check("job_get_missing", s, b, None);
        let (s, b) = outcome(&system_data::job_get(&db, JOB_MISSING));
        check("job_get_missing_after_nothing", s, b, None);

        let (s, b) = outcome(&rt.block_on(system_data::job_delete(&db, JOB_PROCESSING)));
        check("job_delete_processing_blocked", s, b, None);
        let (s, b) = outcome(&rt.block_on(system_data::job_delete(&db, JOB_MISSING)));
        check("job_delete_missing", s, b, None);
        let (s, b) = outcome(&rt.block_on(job_control_resp(&db, JOB_PROCESSING, "pause")));
        check("job_pause_processing_blocked", s, b, None);
        let (s, b) = outcome(&rt.block_on(job_control_resp(&db, JOB_PENDING, "resume")));
        check("job_resume_pending_blocked", s, b, None);
        let (s, b) = outcome(&rt.block_on(job_control_resp(&db, JOB_PENDING, "frobnicate")));
        check("job_bogus_action", s, b, None);
    }

    // --- concurrency set (mutates instance_settings) ---
    {
        let db = fresh_db("conc8");
        let (s, b) = outcome(&rt.block_on(system_data::job_concurrency_set(&db, 8)));
        let extra = json!({ "afterConcurrency": read_max(&db) });
        check("job_concurrency_set_8", s, b, Some(extra));
    }
    {
        // The oracle case is `statusOnly` — and that mask is now NAMED rather
        // than merely inherited (P4.62). v4's refusal here is
        // `validationError(...)`, the two-key `{error:'Validation error',
        // details:[…]}` envelope; this comparand direct-drives the CORE
        // function, whose `validate_concurrency` answers v5's own flat sentence.
        // That is correct, because after P4.62 the core check is reachable only
        // from the `/api/dispatch` verb — a v5-internal transport with no v4
        // analog. The v4-URL edge validates FIRST and answers v4's envelope
        // byte-for-byte; its arms are `system_body_guards_equivalence`'s
        // `concurrency_*`.
        let db = fresh_db("conc0");
        let (s, b) = outcome(&rt.block_on(system_data::job_concurrency_set(&db, 0)));
        check("job_concurrency_set_out_of_range", s, b, None);
    }

    // --- delete pending (mutates) ---
    {
        let db = fresh_db("delp");
        let (s, b) = outcome(&rt.block_on(system_data::job_delete(&db, JOB_PENDING)));
        let (after, _) = outcome(&system_data::job_get(&db, JOB_PENDING));
        check(
            "job_delete_pending",
            s,
            b,
            Some(json!({ "afterStatus": after })),
        );
    }

    // --- pause / resume (mutate; fresh timestamps normalized) ---
    {
        let db = fresh_db("pause");
        let (s, b) = outcome(&rt.block_on(job_control_resp(&db, JOB_PENDING_2, "pause")));
        check("job_pause_pending", s, b, None);
    }
    {
        let db = fresh_db("resume");
        let (s, b) = outcome(&rt.block_on(job_control_resp(&db, JOB_PAUSED, "resume")));
        check("job_resume_paused", s, b, None);
    }

    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert_eq!(ran, 18, "expected 18 cases to run, ran {ran}");
}

/// Unwrap the `JobControlOutcome` to its `Response` (the pump nudge is a host
/// side-effect, irrelevant to the response body).
async fn job_control_resp(db: &Db, id: &str, action: &str) -> Response {
    match system_data::job_control(db, id, action).await {
        system_data::JobControlOutcome::Responded(r)
        | system_data::JobControlOutcome::Resumed(r) => r,
    }
}
