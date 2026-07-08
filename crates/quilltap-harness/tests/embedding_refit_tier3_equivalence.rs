//! Tier-3 (DB-state) differential (W4.7e2): v4's EMBEDDING_REFIT handler — the
//! BUILTIN TF-IDF vocabulary refit. Both sides READ a copy of one baked two-DB
//! fixture (characters + vaults + memories + help docs + a BUILTIN embedding
//! profile), run the refit handler over it, and diff `tfidf_vocabularies` +
//! `background_jobs`.
//!
//! NORMALIZATION: id/createdAt/updatedAt (both tables) + scheduledAt (jobs) →
//! `<ts>` placeholder (minted on each side). `fittedAt` stays EXACT — the Rust fit
//! clock is injected to the same frozen value v4's frozen `Date` produces, proving
//! the state's fittedAt flows into the column. The `idf` column carries the
//! `Math.log` libm seam (macOS `ln` vs V8 `Math.log` differ by ≤1 ULP), so its
//! JSON array is compared NUMERICALLY at 1e-12 and then placeholdered; every other
//! column (vocabulary, avgDocLength, vocabularySize, includeBigrams, userId,
//! profileId, the job's type/status/payload/priority/attempts/maxAttempts/…) is
//! byte-exact. Corpus/vocabulary order is mathematically irrelevant to the output.
//!
//! Also runs the W4.8 runner-registration E2E: register the real
//! `EmbeddingRefitHandler`, enqueue an `EMBEDDING_REFIT` job, pump the runner, and
//! confirm the vocabulary row lands (dispatch → handle → markCompleted).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_REFIT_MAIN=/tmp/qt-refit-main.db QT_FIXTURE_REFIT_MOUNT=/tmp/qt-refit-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-embedding-refit-fixture.ts
//!   QT_FIXTURE_REFIT_MAIN=/tmp/qt-refit-main.db QT_FIXTURE_REFIT_MOUNT=/tmp/qt-refit-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-refit.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- embedding-refit-tier3
//! Run:
//!   QT_ORACLE_REFIT=/tmp/oracle-refit.ndjson \
//!   QT_FIXTURE_REFIT_MAIN=/tmp/qt-refit-main.db QT_FIXTURE_REFIT_MOUNT=/tmp/qt-refit-mount.db \
//!     cargo test -p quilltap-harness --test embedding_refit_tier3_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::embedding_refit_job::{
    handle_embedding_refit, EmbeddingRefitHandler, EmbeddingRefitPayload,
};
use quilltap_core::services::job_runner::{HandlerRegistry, JobRunner};
use serde::Deserialize;
use serde_json::Value;

const FROZEN_ISO: &str = "2026-07-07T12:00:00.000Z";
const TOL: f64 = 1e-12;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "frozenDate")]
    frozen_date: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "profileId")]
    profile_id: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/embedding-refit-tier3.json")
}

/// Timestamp columns to placeholder per table (id is minted, so it goes too).
fn placeholder_columns(table: &str) -> &'static [&'static str] {
    match table {
        "tfidf_vocabularies" => &["id", "createdAt", "updatedAt"],
        "background_jobs" => &["id", "createdAt", "updatedAt", "scheduledAt"],
        _ => &[],
    }
}

/// Normalize a dump in place: placeholder the minted id/timestamp columns, and
/// (for tfidf) parse the `idf` JSON array out for the numeric compare, replacing it
/// with a stable marker so the rest byte-compares. Returns the parsed idf per row.
fn normalize(dump: &mut Value, table: &str) -> Vec<Vec<f64>> {
    let mut idfs = Vec::new();
    let cols = placeholder_columns(table);
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{table}: dump missing rows array"));
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row object");
        for col in cols {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        if table == "tfidf_vocabularies" {
            let idf_str = obj
                .get("idf")
                .and_then(Value::as_str)
                .expect("idf column is JSON text");
            let parsed: Vec<f64> = serde_json::from_str(idf_str).expect("idf parses");
            idfs.push(parsed);
            obj.insert("idf".to_string(), Value::String("<idf>".to_string()));
        }
    }
    idfs
}

/// Strip the oracle's extra `case` field so the shape matches the Rust dump.
fn strip_case(mut v: Value) -> Value {
    if let Some(obj) = v.as_object_mut() {
        obj.remove("case");
    }
    v
}

fn oracle_table(text: &str, table: &str) -> Value {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line parses");
        if v.get("table").and_then(Value::as_str) == Some(table) {
            return v;
        }
    }
    panic!("oracle ndjson missing table {table}");
}

fn assert_idf_close(got: &[Vec<f64>], want: &[Vec<f64>]) {
    assert_eq!(got.len(), want.len(), "idf row count mismatch");
    for (r, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(g.len(), w.len(), "idf[{r}] length mismatch");
        for (i, (&a, &b)) in g.iter().zip(w).enumerate() {
            assert!(
                (a - b).abs() <= TOL,
                "idf[{r}][{i}] mismatch: got {a}, want {b} (|Δ|={})",
                (a - b).abs()
            );
        }
    }
}

fn open_fixture_copy(spec: &Spec, tag: &str) -> Db {
    let main_fixture = std::env::var("QT_FIXTURE_REFIT_MAIN").expect("QT_FIXTURE_REFIT_MAIN");
    let mount_fixture = std::env::var("QT_FIXTURE_REFIT_MOUNT").expect("QT_FIXTURE_REFIT_MOUNT");
    let main_work = std::env::temp_dir().join(format!(
        "qt-refit-rust-{tag}-{}-main.db",
        std::process::id()
    ));
    let mount_work = std::env::temp_dir().join(format!(
        "qt-refit-rust-{tag}-{}-mount.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).expect("copy main fixture");
    std::fs::copy(&mount_fixture, &mount_work).expect("copy mount fixture");
    Db::open(
        DbPaths {
            main: main_work,
            mount_index: Some(mount_work),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture copy")
}

#[tokio::test]
async fn embedding_refit_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_REFIT") else {
        eprintln!("SKIP: set QT_ORACLE_REFIT to the oracle NDJSON (see header).");
        return;
    };
    if std::env::var("QT_FIXTURE_REFIT_MAIN").is_err() {
        eprintln!("SKIP: set QT_FIXTURE_REFIT_MAIN / QT_FIXTURE_REFIT_MOUNT to the fixtures.");
        return;
    }

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    assert_eq!(spec.frozen_date, FROZEN_ISO, "frozen date drift");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let db = open_fixture_copy(&spec, "diff");

    // Run the refit handler with the injected fit clock == v4's frozen Date.
    let payload = EmbeddingRefitPayload {
        profile_id: spec.profile_id.clone(),
        trigger_reindex: true,
    };
    handle_embedding_refit(&db, &spec.user_id, &payload, FROZEN_ISO)
        .await
        .expect("refit handler");

    for table in ["tfidf_vocabularies", "background_jobs"] {
        let order_by = if table == "tfidf_vocabularies" {
            "profileId"
        } else {
            "type"
        };
        let mut got = db
            .read_main(|conn| dump_table_json_conn(conn, table, order_by))
            .expect("dump table");
        let mut want = strip_case(oracle_table(&oracle_text, table));

        let got_idf = normalize(&mut got, table);
        let want_idf = normalize(&mut want, table);
        if table == "tfidf_vocabularies" {
            assert_idf_close(&got_idf, &want_idf);
        }
        assert_eq!(got, want, "{table} dump mismatch after normalization");
    }

    eprintln!("OK: embedding-refit tier-3 matched oracle (tfidf_vocabularies + background_jobs).");
}

/// W4.8 runner-registration E2E: register the real handler, enqueue an
/// `EMBEDDING_REFIT` job, pump the runner, and confirm the vocabulary row lands.
#[tokio::test]
async fn embedding_refit_runner_e2e() {
    if std::env::var("QT_FIXTURE_REFIT_MAIN").is_err() {
        eprintln!("SKIP: set QT_FIXTURE_REFIT_MAIN / QT_FIXTURE_REFIT_MOUNT to the fixtures.");
        return;
    }
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let db = open_fixture_copy(&spec, "e2e");

    // Register the real refit handler with a pinned fit clock.
    let mut reg = HandlerRegistry::new();
    reg.register(
        "EMBEDDING_REFIT",
        Box::new(EmbeddingRefitHandler {
            now_iso: Some(FROZEN_ISO.to_string()),
        }),
    );
    let runner = JobRunner::new(db.clone(), reg);

    // Enqueue a real EMBEDDING_REFIT job (payload { profileId, triggerReindex }).
    let payload = serde_json::json!({ "profileId": spec.profile_id, "triggerReindex": true });
    quilltap_core::services::queue_service::enqueue_job(
        &db,
        &spec.user_id,
        "EMBEDDING_REFIT",
        payload,
        1.0,
    )
    .await
    .expect("enqueue refit job");

    // Pump: claim → dispatch → handle → markCompleted. The refit handler enqueues
    // an EMBEDDING_REINDEX_ALL job mid-handling, which the claim-until-empty loop
    // also picks up (its handler is unported → the runner's loud fallback), so the
    // pump dispatches BOTH.
    let outcome = runner.pump_claim().await;
    assert_eq!(
        outcome.dispatched, 2,
        "runner should dispatch the refit + the reindex it enqueued"
    );

    // The vocabulary row landed (dispatch → real handler → upsert).
    let count: i64 = db
        .read_main(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM tfidf_vocabularies WHERE profileId = ?1",
                [&spec.profile_id],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .expect("count vocab rows");
    assert_eq!(count, 1, "refit should persist exactly one vocabulary row");

    // The refit job itself is COMPLETED; the reindex job it enqueued exists (the
    // loud fallback marked it FAILED — its handler is unported, which is correct).
    let (completed, reindex): (i64, i64) = db
        .read_main(|conn| {
            let c: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM background_jobs WHERE type = 'EMBEDDING_REFIT' AND status = 'COMPLETED'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let r: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM background_jobs WHERE type = 'EMBEDDING_REINDEX_ALL'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            Ok((c, r))
        })
        .expect("count jobs");
    assert_eq!(completed, 1, "the refit job should be COMPLETED");
    assert_eq!(reindex, 1, "the refit should have enqueued one reindex job");

    eprintln!("OK: embedding-refit runner E2E (dispatch → refit → reindex enqueue).");
}
