//! Tier-2/3 differential — P4.D292: the index-store backfill pair
//! (`services::memory_index_backfill`, v4 `index-store/backfill.ts`) and
//! `queue_service::enqueue_embedding_generate_many` (v4 `queue-service.ts:
//! 1125-1182`, P4.D288's C1-3 artefact — this family carries its differential
//! and its two line pins) against v4's REAL code at the `83334b9a5` pin.
//!
//! One database, phases in v4's order (state accumulates):
//!   1. `generate_missing_embeddings` — the rows without a vector (NULL AND
//!      zero-length) of ONE character: anchored text in the instance calendar
//!      (`Asia/Kolkata`) + `lexical_text`, the tier kept, two failures (v4's
//!      `String(error)`: `EmbeddingError: …` vs `Error: …`) each WARNed with the
//!      id IN the message, the end DEBUG;
//!   2. `rebuild_vector_index` ×2 — `indexed` = decodable rows, `failed` = `IS
//!      NULL` rows, the DEBUG with `coldIndexed`; nothing written;
//!   3. `enqueue_embedding_generate_many` — empty input (no line), a batch with a
//!      PENDING- and a PROCESSING-busy entity, a duplicate and all four entity
//!      types (the INFO counts, priority per type), the same batch again (all
//!      busy → the DEBUG, no row).
//!
//! Compared per op: the result, the seam calls, the lines; then the `memories`
//! vectors (hex) and the `background_jobs` rows (minted ids masked; the batch
//! shares one `createdAt`, so rows compare as a set ordered by payload).
//!
//! Generate the oracle (Node 24, from the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-mib-oracle /tmp/oracle-memory-index-backfill.ndjson
//!   mkdir -p /tmp/qt-mib-oracle/cases /tmp/qt-mib-oracle/db
//!   cp $V5W/harness/oracle/cases/memory-index-backfill.test.ts /tmp/qt-mib-oracle/cases/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MIB_DIR=/tmp/qt-mib-oracle/db QT_ORACLE_OUT=/tmp/oracle-memory-index-backfill.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!     --roots "$PWD" --roots /tmp/qt-mib-oracle/cases -- memory-index-backfill
//! Run:
//!   cd $V5W
//!   QT_ORACLE_MEMORY_INDEX_BACKFILL=/tmp/oracle-memory-index-backfill.ndjson \
//!   QT_FIXTURE_MIB_DIR=/tmp/qt-mib-oracle/db \
//!     cargo test -p quilltap-harness --test memory_index_backfill_equivalence

use std::sync::Mutex;

use quilltap_core::db::runtime::Db;
use quilltap_core::model::embedding::{
    EmbeddingCallOptions, EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
use quilltap_core::services::memory_index_backfill::{
    generate_missing_embeddings, rebuild_vector_index,
};
use quilltap_core::services::queue_service::enqueue_embedding_generate_many;
use quilltap_core::test_support::global_capture;
use serde_json::{json, Map, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "e2920000-0000-4000-8000-0000000000aa";
const PROFILE: &str = "e2920000-0000-4000-8000-0000000000e2";
const VECTOR: [f32; 4] = [0.0, 1.0, 0.0, 0.0];
const FAIL_EMBEDDING: &str = "fails with an EmbeddingError\n\nprovider refused";
const FAIL_PLAIN: &str = "fails with a plain Error\n\nsocket closed";

#[derive(Default)]
struct Recording {
    calls: Mutex<Vec<Value>>,
}

impl EmbeddingProvider for Recording {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
        opts: EmbeddingCallOptions,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        self.calls.lock().unwrap().push(json!({
            "text": text,
            "userId": user_id,
            "profileId": profile_id,
            "priority": match opts.priority {
                EmbeddingPriority::Background => "background",
                EmbeddingPriority::Interactive => "interactive",
            },
            "lexicalText": opts.lexical_text,
        }));
        let out = match text {
            // v4's `EmbeddingError` carries its provider; a plain error none.
            FAIL_EMBEDDING => Err(EmbeddingError {
                message: "provider refused the batch".into(),
                provider: Some("OPENAI".into()),
            }),
            FAIL_PLAIN => Err(EmbeddingError {
                message: "socket closed".into(),
                provider: None,
            }),
            _ => Ok(EmbeddingResult {
                embedding: VECTOR.to_vec(),
                model: "canned".into(),
                dimensions: VECTOR.len(),
                provider: "canned".into(),
            }),
        };
        std::future::ready(out)
    }
}

fn want_lines(lines: &Value) -> Vec<String> {
    lines
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut out = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                match v {
                    Value::String(s) => out.push_str(&format!(" {k}={s}")),
                    other => out.push_str(&format!(" {k}={other}")),
                }
            }
            out
        })
        .collect()
}

/// v5's lines this family owns (the sibling index-store / memory-calendar
/// lines are other lanes'), minus their target.
fn got_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap().to_string();
            let _target = parts.next();
            format!("{level} {}", parts.next().unwrap_or_default())
        })
        .filter(|l| {
            let rest = l.split_once(' ').map_or("", |(_, r)| r);
            rest.starts_with("[Memory] Failed to generate embedding for memory")
                || rest.starts_with("[Memory] Backfilled missing embeddings")
                || rest.starts_with("[Memory] Rebuilt vector index from memory rows")
                || rest.starts_with("Embedding generate batch enqueued")
                || rest.starts_with("Embedding batch: every entity already has a job")
        })
        .collect()
}

fn cell(r: &rusqlite::Row<'_>, i: usize) -> rusqlite::Result<Value> {
    Ok(match r.get_ref(i)? {
        rusqlite::types::ValueRef::Null => Value::Null,
        rusqlite::types::ValueRef::Integer(n) => n.into(),
        rusqlite::types::ValueRef::Real(f) if f.fract() == 0.0 => (f as i64).into(),
        rusqlite::types::ValueRef::Real(f) => f.into(),
        rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned().into(),
        rusqlite::types::ValueRef::Blob(_) => "<blob>".into(),
    })
}

fn dump(db: &Db, sql: &'static str) -> Vec<Value> {
    db.read_main(move |c| {
        let mut stmt = c.prepare(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt
            .query_map([], |r| {
                let mut row = Map::new();
                for (i, n) in names.iter().enumerate() {
                    row.insert(n.clone(), cell(r, i)?);
                }
                Ok(Value::Object(row))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .expect("dump")
}

/// Minted ids masked; rows ordered by (id-after-mask, payload) — the batch
/// rows share one `createdAt`, so their id order is random on both sides.
fn jobs_set(rows: Vec<Value>) -> Vec<String> {
    let mut out: Vec<String> = rows
        .into_iter()
        .map(|mut r| {
            if r["id"]
                .as_str()
                .is_some_and(|id| !id.starts_with("e2920000"))
            {
                r["id"] = "<minted>".into();
            }
            serde_json::to_string(&r).unwrap()
        })
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn the_backfill_pair_and_the_batch_enqueue_match_v4() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MEMORY_INDEX_BACKFILL") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_INDEX_BACKFILL to the oracle NDJSON (see header).");
        return;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_MIB_DIR") else {
        eprintln!("SKIP: set QT_FIXTURE_MIB_DIR to the case's dir (see header).");
        return;
    };
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(oracle.len() >= 7, "six ops and the end state");
    global_capture::install();
    quilltap_core::services::memory_time_zone::reset_memory_time_zone_cache_for_tests();
    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join("main.db");
    std::fs::copy(std::path::Path::new(&dir).join("base.db"), &work).expect("copy base.db");
    let db = Db::open_main(&work, PEPPER).expect("open");
    let recording = Recording::default();

    let mut failures = Vec::new();
    for op in oracle.iter().filter(|l| l["kind"] == "op") {
        let kind = op["op"].as_str().unwrap();
        recording.calls.lock().unwrap().clear();
        let (result, lines) = match kind {
            "generateMissingEmbeddings" => {
                let cid = op["input"]["characterId"].as_str().unwrap();
                let (r, l) = global_capture::capture_async(generate_missing_embeddings(
                    &db,
                    &recording,
                    cid,
                    USER,
                    Some(PROFILE),
                    // The instance calendar (`Asia/Kolkata`) wins over the host rung.
                    "UTC",
                    1_800_000_000_000,
                ))
                .await;
                let r = r.expect("generate_missing_embeddings");
                (
                    json!({ "processed": r.processed, "failed": r.failed, "skipped": r.skipped }),
                    l,
                )
            }
            "rebuildVectorIndex" => {
                let cid = op["input"]["characterId"].as_str().unwrap();
                let (r, l) = global_capture::capture_async(rebuild_vector_index(&db, cid)).await;
                let r = r.expect("rebuild_vector_index");
                (json!({ "indexed": r.indexed, "failed": r.failed }), l)
            }
            "enqueueEmbeddingGenerateMany" => {
                let payloads: Vec<Value> = op["input"].as_array().unwrap().clone();
                let (r, l) = global_capture::capture_async(enqueue_embedding_generate_many(
                    &db, USER, &payloads,
                ))
                .await;
                let ids = r.expect("enqueue_embedding_generate_many");
                // v4 answered `[]` for the empty input, the length otherwise.
                let v = if payloads.is_empty() {
                    json!([])
                } else {
                    json!(ids.len())
                };
                (v, l)
            }
            other => panic!("unknown op {other}"),
        };
        let (got, want) = (
            serde_json::to_string(&result).unwrap(),
            serde_json::to_string(&op["result"]).unwrap(),
        );
        if got != want {
            failures.push(format!("{kind}: result\n  v5 {got}\n  v4 {want}"));
        }
        let calls = Value::Array(recording.calls.lock().unwrap().clone());
        if serde_json::to_string(&calls).unwrap() != serde_json::to_string(&op["calls"]).unwrap() {
            failures.push(format!("{kind}: calls\n  v5 {calls}\n  v4 {}", op["calls"]));
        }
        let (gl, wl) = (got_lines(lines), want_lines(&op["lines"]));
        if gl != wl {
            failures.push(format!("{kind}: lines\n  v5 {gl:#?}\n  v4 {wl:#?}"));
        }
    }

    let end = oracle
        .iter()
        .find(|l| l["kind"] == "end")
        .expect("end state");
    let memories = Value::Array(dump(
        &db,
        "SELECT id, tier, hex(embedding) AS embedding FROM memories ORDER BY id",
    ));
    if memories != end["memories"] {
        failures.push(format!(
            "memories\n  v5 {memories}\n  v4 {}",
            end["memories"]
        ));
    }
    let jobs = jobs_set(dump(
        &db,
        "SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, startedAt, completedAt FROM background_jobs ORDER BY createdAt, id",
    ));
    let want_jobs = jobs_set(end["jobs"].as_array().unwrap().clone());
    if jobs != want_jobs {
        failures.push(format!(
            "background_jobs\n  v5 {jobs:#?}\n  v4 {want_jobs:#?}"
        ));
    }
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!("OK: the backfill pair and the batch enqueue matched v4.");
}
