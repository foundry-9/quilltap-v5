//! P4.9G3 jobs-COLLECTION differential: direct-drives the Rust
//! `api::system_data::{jobs_list, jobs_enqueue_now}` — the free functions the new
//! web-edge-only `/api/v1/system/jobs` route calls — over a FRESH copy of the
//! committed three-partition `system-data-*` fixture family per case, and diffs
//! each `{status, body}` (+ `extra`) against v4's REAL
//! `app/api/v1/system/jobs/route.ts` handlers (the `system-jobs-collection`
//! oracle).
//!
//! These two functions were P4.9G1's blind spot: written with no edge and no
//! case, so they had never been diffed. Wiring the edge in this lane closes it.
//!
//! The processor status is PINNED identically on both sides (the oracle mocks
//! v4's processor module; here we pass a fixed `ProcessorStatus`). The enqueue
//! case normalizes the minted `jobId` + the created row's three timestamps;
//! everything else about the row is diffed verbatim.
//!
//! Generate the oracle (see the .test.ts header), then run:
//!   QT_ORACLE_SYSTEM_JOBS_COLLECTION=/tmp/oracle-system-jobs-collection.ndjson \
//!     cargo test -p quilltap-harness --test system_jobs_collection_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::system_data::{self, ProcessorStatus};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const CHAT_1: &str = "c1000000-0000-4000-8000-000000000001";
const LORIAN: &str = "a1000000-0000-4000-8000-000000000001";

/// Minted-at-call-time fields the diff replaces with the oracle's sentinel.
const MINTED: &[&str] = &["id", "scheduledAt", "createdAt", "updatedAt"];

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
    let scratch = std::env::temp_dir().join(format!("qt-sysjobsc-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm = scratch.join("llm.db");
    std::fs::copy(fixtures_dir().join("system-data-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("system-data-mount.db"), &mount).unwrap();
    std::fs::copy(fixtures_dir().join("system-data-llmlogs.db"), &llm).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm),
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
        ErrorKind::Internal => 500,
    }
}

/// `created` is the POST success status (v4 answers 201 there, 200 on the GETs).
fn outcome(resp: &Response, created: bool) -> (u16, Value) {
    match resp {
        Response::System(v) => (if created { 201 } else { 200 }, v.clone()),
        Response::Error(e) => (status_of(e.kind), json!({ "error": e.message })),
        other => panic!("unexpected response variant: {other:?}"),
    }
}

#[test]
fn system_jobs_collection_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SYSTEM_JOBS_COLLECTION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_SYSTEM_JOBS_COLLECTION to the oracle NDJSON (see header)."
            );
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

    let mut check = |name: &str, status: u16, mut body: Value, extra: Option<Value>| {
        let Some(exp) = oracle.get(name) else {
            failed.push(format!("{name}: MISSING from oracle"));
            return;
        };
        let exp_status = exp["status"].as_u64().unwrap() as u16;
        if status != exp_status {
            failed.push(format!(
                "{name}: status {status} != oracle {exp_status}\n  rust body: {body}"
            ));
            return;
        }
        let mut exp_body = exp["body"].clone();
        // The oracle names the minted top-level fields to blank (`jobId`).
        if let Some(norm) = exp.get("normalize").and_then(Value::as_array) {
            for f in norm {
                let key = f.as_str().unwrap();
                for target in [&mut body, &mut exp_body] {
                    if let Some(o) = target.as_object_mut() {
                        if o.contains_key(key) {
                            o.insert(key.to_string(), Value::String("<NORM>".into()));
                        }
                    }
                }
            }
        }
        if body != exp_body {
            failed.push(format!(
                "{name}: body mismatch\n  rust:   {body}\n  oracle: {exp_body}"
            ));
            return;
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

    // --- the four read shapes share one db (no mutation) ---
    {
        let db = fresh_db("reads");
        let (s, b) = outcome(
            &system_data::jobs_list(&db, USER, false, None, &PINNED),
            false,
        );
        check("jobs_collection_get", s, b, None);

        let (s, b) = outcome(
            &system_data::jobs_list(&db, USER, true, None, &PINNED),
            false,
        );
        check("jobs_collection_get_include_jobs", s, b, None);

        let (s, b) = outcome(
            &system_data::jobs_list(&db, USER, false, Some(CHAT_1), &PINNED),
            false,
        );
        check("jobs_collection_get_chat", s, b, None);

        let (s, b) = outcome(
            &system_data::jobs_list(&db, USER, true, Some(CHAT_1), &PINNED),
            false,
        );
        check("jobs_collection_get_both", s, b, None);
    }

    // --- enqueue (mutates; the minted id + timestamps normalized) ---
    {
        let db = fresh_db("enqueue");
        let resp = rt.block_on(system_data::jobs_enqueue_now(
            &db,
            USER,
            "MEMORY_HOUSEKEEPING",
            &json!({ "characterId": LORIAN }),
            Some(3.0),
            Some(5.0),
        ));
        let (s, b) = outcome(&resp, true);
        let job_id = b
            .get("jobId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Read the row back through the same listing leg the oracle used.
        let (_, after) = outcome(
            &system_data::jobs_list(&db, USER, true, None, &PINNED),
            false,
        );
        let jobs = after
            .get("jobs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut row = jobs
            .iter()
            .find(|j| j.get("id").and_then(Value::as_str) == Some(job_id.as_str()))
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(o) = row.as_object_mut() {
            for k in MINTED {
                if o.contains_key(*k) {
                    o.insert((*k).to_string(), Value::String("<NORM>".into()));
                }
            }
        }
        let extra = json!({ "row": row, "jobCount": jobs.len() });
        check("jobs_collection_post", s, b, Some(extra));
    }

    // --- the three enqueue refusals (no mutation) ---
    {
        let db = fresh_db("refusals");
        let (s, b) = outcome(
            &rt.block_on(system_data::jobs_enqueue_now(
                &db,
                USER,
                "NOT_A_JOB",
                &json!({}),
                None,
                None,
            )),
            true,
        );
        check("jobs_collection_post_bad_type", s, b, None);

        let (s, b) = outcome(
            &rt.block_on(system_data::jobs_enqueue_now(
                &db,
                USER,
                "MEMORY_HOUSEKEEPING",
                &Value::Null,
                None,
                None,
            )),
            true,
        );
        check("jobs_collection_post_no_payload", s, b, None);

        let (s, b) = outcome(
            &rt.block_on(system_data::jobs_enqueue_now(
                &db,
                USER,
                "MEMORY_HOUSEKEEPING",
                &json!("nope"),
                None,
                None,
            )),
            true,
        );
        check("jobs_collection_post_payload_not_object", s, b, None);
    }

    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert_eq!(ran, 8, "expected 8 cases to run, ran {ran}");
}
