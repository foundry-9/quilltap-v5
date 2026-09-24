//! Tier-3 differential test: the **EMBEDDING_GENERATE job handler** (P4.6BL —
//! `quilltap_core::services::embedding_generate_job`, v4
//! `handleEmbeddingGenerate`, lib/background-jobs/handlers/embedding-generate.ts).
//!
//! Both sides copy the same committed two-DB seed fixture
//! (`crates/quilltap-web/tests/fixtures/embedding-generate-{main,mount}.db`)
//! and drain the same fourteen pinned PENDING jobs through the same claim loop:
//! claim the next due job → run the handler → markCompleted on `Ok` /
//! markFailed(message) on `Err`; when nothing is due, REWIND any FAILED
//! retry-eligible job's `scheduledAt` to the epoch (raw SQL, both sides — the
//! backoff's wall-clock wait never enters the differential) until no such job
//! remains. The oracle drives v4's REAL handler + REAL queue claim/mark; this
//! test replays the RECORDED canned embeddings (successes AND per-text failure
//! messages) through the ported handler + the ported claim/mark, asserts the
//! processed (jobId, outcome, error) SEQUENCE element-for-element (pinning
//! claim order, the permanent-vs-transient classifier split, and the DEAD
//! transition), then diffs EIGHT tables — `background_jobs`,
//! `embedding_status`, `memories`, `vector_entries`, `vector_indices`,
//! `conversation_chunks`, `help_docs` (main) and `doc_mount_chunks` (mount) —
//! with minted timestamps placeholder-normalized and everything else EXACT.
//!
//! ⚠️ P4.D25 — `embedding_status` is now the one table that mints. Since v4
//! `a5d6cee5`, `markAsEmbedded`/`markAsFailed` UPSERT, so the corpus's
//! `mc-happy-no-status-row` job (whose entity was deliberately seeded WITHOUT a
//! status row) mints a fresh row with a fresh UUID. That table is therefore
//! keyed by the natural (entityType, entityId, profileId) triple, its minted id
//! blanked to `<minted>` and that row's own timestamps placeholdered; every
//! corpus-pinned id and timestamp still compares exactly. A shape assertion
//! pins that exactly one such row exists, so a port that reverted to the old
//! silent no-op cannot pass by dumping one row fewer unnoticed.
//!
//! Generate the oracle (Node 24, from the v4 checkout; the /tmp mirror dodges
//! jest's `/.claude/` testPathIgnorePatterns), against /tmp COPIES of the
//! COMMITTED fixture — the oracle mutates the DBs it is pointed at, so never
//! point it at the committed files (P4.D32's sweep hazard):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   mkdir -p /tmp/qt-eg-oracle/cases /tmp/qt-eg-oracle/fixtures /tmp/qt-eg-oracle/db
//!   cp $V5W/crates/quilltap-web/tests/fixtures/embedding-generate-main.db  /tmp/qt-eg-oracle/db/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/embedding-generate-mount.db /tmp/qt-eg-oracle/db/
//!   cp $V5W/harness/oracle/cases/embedding-generate-jobs.test.ts /tmp/qt-eg-oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/embedding-generate-jobs.json /tmp/qt-eg-oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_EG_MAIN=/tmp/qt-eg-oracle/db/embedding-generate-main.db \
//!   QT_FIXTURE_EG_MOUNT=/tmp/qt-eg-oracle/db/embedding-generate-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-embedding-generate.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "/tmp/qt-eg-oracle/cases" -- embedding-generate-jobs
//! Run (TZ pinned to match the oracle):
//!   TZ=UTC QT_ORACLE_EG=/tmp/oracle-embedding-generate.ndjson \
//!     cargo test -p quilltap-harness --test embedding_generate_jobs_equivalence
//!
//! ⚠ P4.D77: the committed main fixture's `help_doc_chunks` table was added by
//! `harness/oracle/fixtures/extend-help-doc-chunks.ts` (EXTENDER, not rebuild —
//! see its header). To re-derive it: copy the committed main DB to /tmp, run
//! `QT_CHUNK_SPEC=embedding-generate-jobs.json QT_CHUNK_DB=<copy>` through that
//! script from the v4 checkout, copy back, then regenerate this oracle.
//!
//! Rebuilding the COMMITTED fixture family is a separate, DELIBERATE act — a
//! rebuild mints fresh UUIDs and invalidates every family reading these DBs,
//! so it re-runs every consumer. When (and only when) the fixture itself must
//! change: `build-embedding-generate-fixture.ts` writes fresh DBs to the paths
//! in `QT_FIXTURE_OUT` / `QT_FIXTURE_MOUNT_OUT`; copy those over the committed
//! `embedding-generate-{main,mount}.db` and regenerate this family's oracle
//! plus every sibling that reads them.
//!
//! ## P4.D222 (v4 `492771aff`, bug 168) — the HELP_DOC job by SECTION
//!
//! The doc vector is now the normalised mean of its section vectors and the
//! whole-text call is GONE (`hd-happy` moved: its doc vector is
//! `normalize(e0 + e1)`). Eight more HELP_DOC arms are PLANTED, never baked
//! into the committed pair: the spec's `helpDocPlants.sql` runs on BOTH sides'
//! per-run copies before the claim loop (v4's four extended-test vectors — two
//! null sections, 80 sections with no rows, reused/null/stale-width, the first
//! section failing — plus a failed width re-embed that keeps the old row,
//! every section failing, a blank composed text, and a doc that slices to
//! nothing). `helpDocPlants.cannedVectors` gives specific texts their own
//! answer; the oracle records the vector it really answered, and an
//! `embedCalls` line per text, which this side matches call for call.
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::db::background_jobs::BackgroundJobsRepository;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::{
    CannedEmbeddingProvider, EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
use quilltap_core::services::embedding_generate_job::{
    handle_embedding_generate, EmbeddingGeneratePayload,
};
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
struct FailingTextW {
    text: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteBodyW {
    character_id: String,
    batch_size: Option<i64>,
    confirm: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteCaseW {
    name: String,
    kind: String,
    body: RouteBodyW,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    help_doc_plants: HelpDocPlantsW,
    failing_texts: Vec<FailingTextW>,
    route_cases: Vec<RouteCaseW>,
    jobs: Vec<JobW>,
}

#[derive(Deserialize)]
struct HelpDocPlantsW {
    sql: Vec<String>,
}

/// The canned provider, counting every call per text (P4.D222 — the jest
/// oracle writes no `llm_logs` rows, so per-text call counts are how the two
/// sides prove they made the SAME calls, not just the same set).
struct CountingEmbedding {
    inner: CannedEmbeddingProvider,
    calls: Mutex<HashMap<String, u64>>,
}

impl EmbeddingProvider for CountingEmbedding {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
        priority: EmbeddingPriority,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        *self
            .calls
            .lock()
            .unwrap()
            .entry(text.to_string())
            .or_default() += 1;
        self.inner
            .generate_embedding_for_user(text, user_id, profile_id, priority)
    }
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/embedding-generate-jobs.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// (table, order_by, main?, timestamp columns to placeholder, natural sort key).
/// Almost nothing here mints a UUID — every id is fixture data, so normalization
/// is timestamps-only and `lastError` / `error` strings, statuses, attempts and
/// payloads all compare EXACT.
///
/// The ONE exception is `embedding_status` since P4.D25 (v4 `a5d6cee5`):
/// `markAsEmbedded`/`markAsFailed` now UPSERT, so a job whose entity was never
/// given a status row (the corpus's `mc-happy-no-status-row`) mints one with a
/// fresh UUID. Those rows are therefore keyed by the natural
/// (entityType, entityId, profileId) triple and their minted id blanked; every
/// id the corpus pinned still compares exactly, so "the update arm reused the
/// existing row" stays visible.
type TableSpec = (
    &'static str,
    &'static str,
    bool,
    &'static [&'static str],
    &'static [&'static str],
);
const TABLES: &[TableSpec] = &[
    (
        "background_jobs",
        "id",
        true,
        &["scheduledAt", "startedAt", "completedAt", "updatedAt"],
        &[],
    ),
    (
        "embedding_status",
        "id",
        true,
        &["embeddedAt", "updatedAt"],
        &["entityType", "entityId", "profileId"],
    ),
    ("memories", "id", true, &["updatedAt"], &[]),
    ("vector_entries", "id", true, &["createdAt"], &[]),
    (
        "vector_indices",
        "id",
        true,
        &["createdAt", "updatedAt"],
        &[],
    ),
    ("conversation_chunks", "id", true, &["updatedAt"], &[]),
    ("help_docs", "id", true, &["updatedAt"], &[]),
    // P4.D77 — the section chunks the HELP_DOC job now embeds. `updatedAt` is
    // stamped on every chunk the pass writes; every id is corpus-pinned.
    ("help_doc_chunks", "id", true, &["updatedAt"], &[]),
    ("doc_mount_chunks", "id", false, &["updatedAt"], &[]),
];

/// Every UUID-shaped string anywhere in the committed corpus — the ids BOTH
/// sides pinned, so anything else in a dump was minted at run time.
fn pinned_ids(spec: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) => {
                if s.len() == 36 && s.as_bytes()[14] == b'4' && s.matches('-').count() == 4 {
                    out.insert(s.clone());
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(spec, &mut out);
    out
}

fn normalize_table(
    dump: &mut Value,
    table: &str,
    ts_columns: &[&str],
    natural_sort: &[&str],
    pinned: &BTreeSet<String>,
) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{table}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{table}: row is not an object"));
        for col in ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        if natural_sort.is_empty() {
            continue;
        }
        let minted = match obj.get("id") {
            Some(Value::String(s)) => !pinned.contains(s),
            _ => false,
        };
        if minted {
            obj.insert("id".to_string(), Value::String("<minted>".to_string()));
            // A minted row's own timestamps are minted too (the mark* create arm
            // reaches v4's `create` with no CreateOptions, so `createdAt` falls
            // through to `now`). Pinned rows keep theirs EXACT.
            let cols: Vec<String> = obj.keys().cloned().collect();
            for col in cols {
                if col.ends_with("At") && obj.get(&col).map(|v| !v.is_null()).unwrap_or(false) {
                    obj.insert(col, Value::String("<ts>".to_string()));
                }
            }
        }
    }
    if !natural_sort.is_empty() {
        rows.sort_by_key(|r| {
            natural_sort
                .iter()
                .map(|c| r.get(*c).and_then(Value::as_str).unwrap_or("").to_string())
                .collect::<Vec<_>>()
                .join(" ")
        });
    }
}

#[derive(Debug, PartialEq)]
struct ProcessedW {
    job_id: String,
    outcome: String,
    error: Option<String>,
}

/// v4 `lib/api/responses.ts` kind→status (the routes-family mapping).
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
        // The store-unavailable refusal (P4.23) — v4's deliberate
        // contextful 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// A `Response` as (status, body) the oracle's route rows compare against.
fn status_body(r: &quilltap_core::api::Response) -> (u16, Value) {
    if let quilltap_core::api::Response::Error(e) = r {
        return (status_of(e.kind), serde_json::json!({ "error": e.message }));
    }
    let v = serde_json::to_value(r).unwrap();
    (200, v.get("data").cloned().unwrap_or(Value::Null))
}

#[tokio::test]
async fn embedding_generate_jobs_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_EG") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_EG to the oracle NDJSON (see header).");
            return;
        }
    };

    let spec_text =
        std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}"));
    let spec_json: Value = serde_json::from_str(&spec_text).expect("parse spec json");
    let pinned = pinned_ids(&spec_json);
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_sequence: Vec<ProcessedW> = Vec::new();
    let mut oracle_embeddings: Vec<(String, Vec<f32>)> = Vec::new();
    let mut oracle_failures: Vec<(String, String)> = Vec::new();
    let mut oracle_routes: Vec<(String, u16, Value)> = Vec::new();
    let mut oracle_tables: HashMap<String, Value> = HashMap::new();
    let mut oracle_calls: HashMap<String, u64> = HashMap::new();
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("processed") => oracle_sequence.push(ProcessedW {
                job_id: v["jobId"].as_str().unwrap().to_string(),
                outcome: v["outcome"].as_str().unwrap().to_string(),
                error: v.get("error").and_then(Value::as_str).map(str::to_string),
            }),
            Some("cannedEmbedding") => {
                let text = v["text"].as_str().expect("embedding text").to_string();
                let vec: Vec<f32> = v["vector"]
                    .as_array()
                    .expect("embedding vector")
                    .iter()
                    .map(|x| x.as_f64().unwrap() as f32)
                    .collect();
                oracle_embeddings.push((text, vec));
            }
            Some("cannedEmbeddingFailure") => oracle_failures.push((
                v["text"].as_str().unwrap().to_string(),
                v["message"].as_str().unwrap().to_string(),
            )),
            Some("route") => oracle_routes.push((
                v["name"].as_str().unwrap().to_string(),
                v["status"].as_u64().unwrap() as u16,
                v["body"].clone(),
            )),
            Some("embedCalls") => {
                oracle_calls.insert(
                    v["text"].as_str().unwrap().to_string(),
                    v["calls"].as_u64().unwrap(),
                );
            }
            Some("table") => {
                oracle_tables.insert(v["table"].as_str().unwrap().to_string(), v);
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Corpus-shape assertions (not hand counts): the oracle processed EXACTLY
    // the corpus job set (every id at least once), recorded every failing text,
    // and dumped every diffed table.
    let spec_ids: BTreeSet<&str> = spec.jobs.iter().map(|j| j.id.as_str()).collect();
    let processed_ids: BTreeSet<&str> = oracle_sequence.iter().map(|p| p.job_id.as_str()).collect();
    assert_eq!(
        processed_ids, spec_ids,
        "oracle processed a different job set than the corpus — regenerate"
    );
    assert!(
        oracle_sequence.len() >= spec.jobs.len(),
        "oracle sequence shorter than the job set"
    );
    assert_eq!(
        oracle_failures.len(),
        spec.failing_texts.len(),
        "oracle failure recordings diverge from the corpus failingTexts"
    );
    for f in &spec.failing_texts {
        assert!(
            oracle_failures
                .iter()
                .any(|(t, m)| t == &f.text && m == &f.message),
            "corpus failing text not recorded by the oracle: {:?}",
            f.text
        );
    }
    assert_eq!(
        oracle_tables.len(),
        TABLES.len(),
        "oracle dumped a different table set"
    );

    // P4.D77 (v4 `24633026`) — **section embeddings keep NO bookkeeping of their
    // own.** That is the stated reason v4 put them in the HELP_DOC job instead of
    // giving them an entity type: the reindex enqueue, the `embedding_status`
    // rows and the dimension reconcile all keep counting `help_docs`. The table
    // dumps below already compare `embedding_status` byte-for-byte against v4, so
    // an invented chunk row would diverge — but only if someone reads the diff.
    // Say it out loud instead, against the ORACLE, so the invariant has a name.
    {
        let rows_of = |table: &str| -> Vec<Value> {
            oracle_tables
                .get(table)
                .and_then(|v| v["rows"].as_array())
                .cloned()
                .unwrap_or_default()
        };
        let chunk_rows = rows_of("help_doc_chunks");
        let chunk_ids: BTreeSet<&str> = chunk_rows
            .iter()
            .filter_map(|r| r.get("id").and_then(Value::as_str))
            .collect();
        assert!(
            !chunk_ids.is_empty(),
            "the corpus lost its help_doc_chunks rows — the chunk arms are vacuous"
        );
        for row in rows_of("embedding_status").iter() {
            let entity_type = row.get("entityType").and_then(Value::as_str).unwrap_or("");
            let entity_id = row.get("entityId").and_then(Value::as_str).unwrap_or("");
            assert!(
                !entity_type.starts_with("HELP_DOC_"),
                "v4 minted a help-section embedding_status entityType: {entity_type}"
            );
            assert!(
                !chunk_ids.contains(entity_id),
                "v4 minted an embedding_status row for a help SECTION ({entity_id}) — the \
                 one-unit-of-work-per-document invariant has moved and the port must follow"
            );
        }
    }
    assert_eq!(
        oracle_routes
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        spec.route_cases
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        "oracle route-case set/order diverges from the corpus — regenerate"
    );

    // Fresh copies so the committed seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-eg-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-eg-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(
        fixtures_dir().join("embedding-generate-main.db"),
        &work_main,
    )
    .unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(
        fixtures_dir().join("embedding-generate-mount.db"),
        &work_mount,
    )
    .unwrap_or_else(|e| panic!("copy mount fixture: {e}"));

    // Replay EXACTLY the oracle-recorded embeddings — an input the oracle never
    // saw surfaces as a canned-miss (a transient failure), not an answer.
    let mut canned = CannedEmbeddingProvider::new();
    for (text, vec) in &oracle_embeddings {
        canned = canned.with_vector(text.clone(), vec.clone());
    }
    for (text, message) in &oracle_failures {
        canned = canned.with_failure_for(text.clone(), message.clone());
    }
    let embedding = CountingEmbedding {
        inner: canned,
        calls: Mutex::new(HashMap::new()),
    };

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    // P4.D222 — the planted HELP_DOC arms, on this run's copy (the oracle runs
    // the same statements on its own).
    let plants = spec.help_doc_plants.sql.clone();
    db.write(move |ws| {
        for sql in &plants {
            ws.main().connection().execute_batch(sql)?;
        }
        Ok(())
    })
    .await
    .expect("apply the helpDocPlants");

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
        let decoded = EmbeddingGeneratePayload::from_json(&payload);
        // v4 passes `job.userId` — the row's own value, not the corpus's.
        let outcome = handle_embedding_generate(&db, &embedding, &job.user_id, &decoded).await;
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
        "processed-sequence length diverges\n got: {got_sequence:#?}\nwant: {oracle_sequence:#?}"
    );
    for (i, (g, w)) in got_sequence.iter().zip(oracle_sequence.iter()).enumerate() {
        assert_eq!(g, w, "processed sequence diverges at step {i}");
    }

    // ── The tier-2 route phase (P4.6BL tier 2): the same seven requests the
    // oracle drove through v4's REAL route arms, through the ported dispatch
    // arms, over the same post-claim-loop state. Bodies compare exactly except
    // the Zod `details` array on 400s — the standing repo-wide deferral (the
    // v5 middleware 400 carries `Validation error` alone).
    for (case, (want_name, want_status, want_body)) in
        spec.route_cases.iter().zip(oracle_routes.iter())
    {
        let got = match case.kind.as_str() {
            "generate" => {
                quilltap_core::api::memories::memory_generate_embeddings(
                    &db,
                    &embedding,
                    &spec.user_id,
                    &case.body.character_id,
                    case.body.batch_size,
                )
                .await
            }
            "rebuild" => {
                quilltap_core::api::memories::memory_rebuild_index(
                    &db,
                    &case.body.character_id,
                    case.body.confirm.unwrap_or(false),
                )
                .await
            }
            other => panic!("unknown route-case kind {other}"),
        };
        let (got_status, got_body) = status_body(&got);
        let mut want_body = want_body.clone();
        if let Some(obj) = want_body.as_object_mut() {
            obj.remove("details");
        }
        assert_eq!(
            (got_status, &got_body),
            (*want_status, &want_body),
            "route case {want_name} diverges"
        );
    }

    // P4.D222 — the SAME provider calls, text for text and count for count.
    let got_calls = embedding.calls.lock().unwrap().clone();
    let mut call_diff: Vec<String> = Vec::new();
    for text in oracle_calls
        .keys()
        .chain(got_calls.keys())
        .collect::<BTreeSet<_>>()
    {
        let (w, g) = (oracle_calls.get(text), got_calls.get(text));
        if w != g {
            call_diff.push(format!("{text:?}: v4 {w:?} vs v5 {g:?}"));
        }
    }
    assert!(
        call_diff.is_empty(),
        "provider calls diverge:\n{}",
        call_diff.join("\n")
    );

    // Dump + diff the eight tables (timestamps placeholdered, all else exact).
    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|(table, order_by, main, _, _)| {
            if *main {
                db.read_main(|conn| dump_table_json_conn(conn, table, order_by))
            } else {
                db.read_mount_index(|conn| dump_table_json_conn(conn, table, order_by))
            }
            .unwrap_or_else(|e| panic!("dump {table}: {e:?}"))
        })
        .collect();
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|(table, _, _, _, _)| {
            let mut v = oracle_tables
                .remove(*table)
                .unwrap_or_else(|| panic!("oracle missing table {table}"));
            v.as_object_mut().unwrap().remove("kind");
            v
        })
        .collect();

    for (i, (table, _, _, ts_columns, natural_sort)) in TABLES.iter().enumerate() {
        normalize_table(&mut got[i], table, ts_columns, natural_sort, &pinned);
        normalize_table(&mut want[i], table, ts_columns, natural_sort, &pinned);
    }

    // Corpus-shape guard (P4.D25): the `mc-happy-no-status-row` job's entity has
    // no seeded status row, so the upserting markAsEmbedded MUST mint one. A
    // port that kept the old silent no-op would otherwise just dump one row
    // fewer, and only the row-count assert would notice.
    let minted_status = want[1]["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|r| r.get("id").and_then(Value::as_str) == Some("<minted>"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        minted_status, 1,
        "the corpus stopped exercising the mark* create arm — regenerate the oracle"
    );

    // P4.D222 — each planted arm really took its path, pinned against the
    // ORACLE (so two identically-broken sides cannot pass as agreement).
    {
        let table = |name: &str| -> Vec<Value> {
            let i = TABLES.iter().position(|t| t.0 == name).unwrap();
            want[i]["rows"].as_array().cloned().unwrap_or_default()
        };
        let (docs, chunks, status) = (
            table("help_docs"),
            table("help_doc_chunks"),
            table("embedding_status"),
        );
        let row = |rows: &[Value], id: &str| -> Value {
            rows.iter()
                .find(|r| r["id"] == id)
                .cloned()
                .unwrap_or_else(|| panic!("oracle row {id} missing"))
        };
        let emb = |r: &Value| r["embedding"].as_str().unwrap_or("").to_string();
        let doc_id = |n: u32| format!("bd000000-0000-4000-8000-0000000000{:02x}", 0x10 + n);
        let chunk_id =
            |n: u32, i: u32| format!("bc000000-0000-4000-8000-0000000001{:02x}", n * 0x10 + i);
        let status_of = |n: u32| {
            row(
                &status,
                &format!("e5000000-0000-4000-8000-0000000000{:02x}", 0x10 + n),
            )
        };
        // 1 two-null: both sections written, the doc vector their mean.
        assert!(!emb(&row(&docs, &doc_id(1))).is_empty());
        assert!(emb(&row(&chunks, &chunk_id(1, 0))).starts_with("eb01"));
        assert!(emb(&row(&chunks, &chunk_id(1, 1))).starts_with("eb01"));
        // 2 eighty sections, NO rows: sliced in memory, never persisted; every
        // call far shorter than the page.
        let huge = row(&docs, &doc_id(2));
        assert!(
            !emb(&huge).is_empty(),
            "the huge page gets a vector at last"
        );
        assert!(!chunks.iter().any(|c| c["docId"] == doc_id(2).as_str()));
        let huge_len = huge["content"].as_str().unwrap().len();
        let huge_calls: Vec<&String> = oracle_calls
            .keys()
            .filter(|t| t.starts_with("Huge \u{203a} "))
            .collect();
        assert_eq!(huge_calls.len(), 80, "one call per section");
        assert!(huge_calls.iter().all(|t| t.len() < huge_len / 10));
        // 3 width: the reused [1,0] row is NOT rewritten (still the planted
        // raw-f32 bytes); the stale [1,0,0] one is re-embedded.
        assert_eq!(emb(&row(&chunks, &chunk_id(3, 0))), "0000803f00000000");
        assert!(emb(&row(&chunks, &chunk_id(3, 2))).starts_with("eb01"));
        // 4 first fails: the doc still gets a vector and is marked embedded.
        assert_eq!(status_of(4)["status"], "EMBEDDED");
        // 5 a failed width re-embed keeps the OLD row in the database.
        assert_eq!(
            emb(&row(&chunks, &chunk_id(5, 1))),
            "0000803f0000000000000000"
        );
        assert!(!emb(&row(&docs, &doc_id(5))).is_empty());
        // 6 every section fails: the last (permanent) error is thrown and the
        // status is FAILED with it.
        assert_eq!(status_of(6)["status"], "FAILED");
        assert!(status_of(6)["error"]
            .as_str()
            .unwrap()
            .contains("maximum context length"));
        // 7 blank text / 8 nothing to slice: the empty arm.
        for n in [7, 8] {
            assert_eq!(status_of(n)["status"], "FAILED");
            assert_eq!(status_of(n)["error"], "Empty input — nothing to embed");
            assert!(emb(&row(&docs, &doc_id(n))).is_empty());
        }
        // hd-happy moved: the whole-text call is GONE (its text was never
        // asked for) and the reused e1 section is averaged in.
        assert!(!oracle_calls.contains_key("Aurora\n\nAurora is the memory subsystem."));
    }

    for (i, (table, _, _, _, _)) in TABLES.iter().enumerate() {
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{table} column set / order"
        );
        if got[i]["rows"] != want[i]["rows"] {
            let gp = std::env::temp_dir().join(format!("qt-eg-got-{table}.json"));
            let wp = std::env::temp_dir().join(format!("qt-eg-want-{table}.json"));
            let _ = std::fs::write(&gp, serde_json::to_string_pretty(&got[i]["rows"]).unwrap());
            let _ = std::fs::write(&wp, serde_json::to_string_pretty(&want[i]["rows"]).unwrap());
            let ga = got[i]["rows"].as_array().unwrap();
            let wa = want[i]["rows"].as_array().unwrap();
            for (j, (g, w)) in ga.iter().zip(wa.iter()).enumerate() {
                if g != w {
                    panic!(
                        "{table} rows diverge at index {j} (full dumps: {} / {})\n got: {g:#}\nwant: {w:#}",
                        gp.display(),
                        wp.display()
                    );
                }
            }
            panic!(
                "{table} row-count diverges: got {} vs want {} (full dumps: {} / {})",
                ga.len(),
                wa.len(),
                gp.display(),
                wp.display()
            );
        }
    }
    println!(
        "embedding_generate_jobs: {} jobs / {} processed steps OK",
        spec.jobs.len(),
        got_sequence.len()
    );
}
