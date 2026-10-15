//! Tier-3 differential: the F2 anchor-gate probe (P4.D269, P4.D290; v4
//! `3f7320138`, at the `83334b9a5` pin).
//!
//! Drives v5's REAL `services::anchor_gate_probe::run_anchor_gate_probe` and
//! `api::memories::memory_anchor_gate_probe` (the `memoryAnchorGateProbe`
//! dispatch arm's handler) over a fresh copy per case of the committed
//! `episodic-recall-{main,mount}.db` pair, against the NDJSON the v4 oracle
//! (`harness/oracle/cases/anchor-gate-probe-tier3.test.ts` — v4's REAL
//! `runAnchorGateProbe` and REAL `POST /api/v1/memories?action=anchor-gate-probe`)
//! emitted from the SAME copies. Both sides read one corpus
//! (`harness/oracle/fixtures/anchor-gate-probe-cases.json`): the memory-tiers
//! migration on each copy, then the `basePlant` + each case's `plant`
//! statements, identically.
//!
//! Embeddings run REAL on both sides through the fixture's fitted BUILTIN
//! TF-IDF profile; the one `failText` case wraps each side's real provider to
//! reject that exact text. Comparands: the result in WIRE key order (an
//! ordered walk — never `Map ==`), floats at 1e-12, integers and strings
//! exact, `embeddingsGenerated` exact; a refusal's status + v4's HTTP body;
//! and the captured lines (the probe's two WARNs and INFO, the route's DEBUG
//! and ERROR) rendered from v4's `{level, message, fields}` into v5's
//! `"<LEVEL> <target> <message> k=v …"` (an object field becomes `<key>Json`).
//!
//! P4.D290 (Tier 2, the `83334b9a5` pin): the probe decides through
//! admission's `decideAdmission` — the rows carry `anchoredAction` /
//! `anchorFreeAction`, the result `lexicalProfile` / `note`, the thresholds come
//! from the first embedding's provider (`probe_empty_character` keeps the
//! neural set and a null note). `PROBE_STORE_LOAD_FAILURE` (P4.D269 R-G)
//! RETIRED on measurement (LR-12): the mirror tables are gone at the pin and in
//! the widened pair, so `route_vector_entries_dropped` is re-planted `IF EXISTS`
//! and BOTH sides answer 200 with the same body — compared like any row. The
//! dimension skip is re-planted on `memories.embedding` (`probe_dim_mismatch`),
//! beside a mixed-width store and an empty one that proceed. LR-14's
//! `probe_neural_scripted` runs the NEURAL path: the oracle answers every text
//! by its summary at the store's width under provider `OPENAI` and EMITS the
//! exact texts (`canned`); this side builds a `CannedEmbeddingProvider` from
//! them. The probe's memory-calendar fallback on the route's entry
//! (`run_anchor_gate_probe`) is `"UTC"`, matching the oracle's `TZ=UTC`.
//!
//! Recorded at the pin, not compared (each a measured v4 shape no plant can
//! make v5 reach the same way): an unparseable `createdAt` never reaches the
//! probe in v4 (its `findByFilter` Zod-drops the row — `Data validation
//! failed` / `Safe validation failed`; v5's `memories_read` filter path keeps
//! it, a pre-existing read-path gap); a renamed `memories` table does not fail
//! v4's sample read (the collection is re-ensured; no error line).
//!
//! Generate the oracle (Node 24, from the v4 checkout or a pin; STAGE the case
//! outside `.claude/` — v4's jest ignores those paths), against /tmp COPIES of
//! the COMMITTED episodic-recall fixture. Do NOT run
//! `build-episodic-recall-fixture.ts` pointed at the committed `.db` files:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-anchor-gate-probe-stage
//!   mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures $STAGE/db
//!   cp $V5W/harness/oracle/cases/anchor-gate-probe-tier3.test.ts $STAGE/harness/oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/anchor-gate-probe-cases.json $STAGE/harness/oracle/fixtures/
//!   cp $V5W/harness/oracle/fixtures/episodic-recall.json $STAGE/harness/oracle/fixtures/
//!   cp $V5W/harness/oracle/fixtures/apply-memory-tiers-migration.ts $STAGE/harness/oracle/fixtures/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/episodic-recall-main.db  $STAGE/db/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/episodic-recall-mount.db $STAGE/db/
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_ER_MAIN=$STAGE/db/episodic-recall-main.db \
//!   QT_FIXTURE_ER_MOUNT=$STAGE/db/episodic-recall-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-anchor-gate-probe.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- anchor-gate-probe-tier3
//! Run:
//!   TZ=UTC QT_ORACLE_ANCHOR_GATE_PROBE=/tmp/oracle-anchor-gate-probe.ndjson \
//!     cargo test -p quilltap-harness --test anchor_gate_probe_tier3_equivalence -- --nocapture

use std::future::Future;
use std::path::PathBuf;

use quilltap_core::api::memories::memory_anchor_gate_probe;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::{
    CannedEmbeddingProvider, EmbeddingCallOptions, EmbeddingError, EmbeddingProvider,
    EmbeddingResult, ErasedEmbeddingProvider,
};
use quilltap_core::model::wire::CannedWireTransport;
use quilltap_core::services::anchor_gate_probe::{
    run_anchor_gate_probe, RunAnchorGateProbeInput, LOG_TARGET,
};
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use serde::Deserialize;
use serde_json::Value;

const EPS: f64 = 1e-12;

/// The lines the family compares (the oracle's `CAPTURED`).
const CAPTURED: &[&str] = &[
    "Probe embedding dimension does not match the store; skipping row",
    "Probe failed for row; continuing",
    "Anchor gate probe complete",
    "[Memories API] Running anchor gate probe",
    "[Memories API] Anchor gate probe failed",
    "Error finding entities by filter",
    "Error finding recent memories",
    "Error finding memories by IDs",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    character_id: String,
    user_id: String,
    base_plant: Vec<String>,
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct Input {
    character_id: Option<String>,
    limit: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseSpec {
    name: String,
    kind: String,
    #[serde(default)]
    input: Option<Input>,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    plant: Vec<String>,
    #[serde(default)]
    fail_text: Option<String>,
    #[serde(default)]
    fail_message: Option<String>,
    #[serde(default)]
    neural: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureSpec {
    test_pepper_base64: String,
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh Db over a private copy of the episodic-recall pair: the tier
/// migration, then the plants, on the writable copy before the open.
struct ScratchDb {
    db: Db,
    _dir: tempfile::TempDir,
}

fn fresh_db(pepper: &str, plants: &[String]) -> ScratchDb {
    let dir = tempfile::Builder::new()
        .prefix("qt-agp-")
        .tempdir()
        .expect("tempdir");
    let fixtures = repo().join("crates/quilltap-web/tests/fixtures");
    let main = dir.path().join("main.db");
    let mount = dir.path().join("mount.db");
    std::fs::copy(fixtures.join("episodic-recall-main.db"), &main).unwrap();
    std::fs::copy(fixtures.join("episodic-recall-mount.db"), &mount).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, pepper).expect("open copy");
        quilltap_core::test_support::ensure_memory_tiers_columns(w.connection());
        for sql in plants {
            w.connection()
                .execute_batch(sql)
                .unwrap_or_else(|e| panic!("plant `{sql}`: {e}"));
        }
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open db");
    ScratchDb { db, _dir: dir }
}

/// The production embedding path over the fixture (BUILTIN TF-IDF; the canned
/// transport is never touched).
fn provider(db: &Db) -> ErasedEmbeddingProvider {
    ErasedEmbeddingProvider::new(ApiEmbeddingProvider::new(
        db.clone(),
        CannedWireTransport::new(),
    ))
}

/// The `failText` case's wrapper: rejects that exact text, passes every other
/// call through to the real provider (the oracle's twin).
struct FailingProvider {
    inner: ErasedEmbeddingProvider,
    fail_text: String,
    message: String,
}

impl EmbeddingProvider for FailingProvider {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
        opts: EmbeddingCallOptions,
    ) -> impl Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        let fail = text == self.fail_text;
        let message = self.message.clone();
        let call = self
            .inner
            .generate_embedding_for_user(text, user_id, profile_id, opts);
        async move {
            if fail {
                Err(EmbeddingError::new(message))
            } else {
                call.await
            }
        }
    }
}

/// An ordered structural compare: same key ORDER, strings / bools / nulls
/// exact, integers exact, floats at 1e-12. Returns the first difference.
fn ordered_diff(path: &str, got: &Value, want: &Value) -> Option<String> {
    match (got, want) {
        (Value::Object(g), Value::Object(w)) => {
            let gk: Vec<&String> = g.keys().collect();
            let wk: Vec<&String> = w.keys().collect();
            if gk != wk {
                return Some(format!("{path}: keys {gk:?} vs {wk:?}"));
            }
            g.iter()
                .find_map(|(k, gv)| ordered_diff(&format!("{path}.{k}"), gv, &w[k]))
        }
        (Value::Array(g), Value::Array(w)) => {
            if g.len() != w.len() {
                return Some(format!("{path}: len {} vs {}", g.len(), w.len()));
            }
            g.iter()
                .zip(w)
                .enumerate()
                .find_map(|(i, (gv, wv))| ordered_diff(&format!("{path}[{i}]"), gv, wv))
        }
        (Value::Number(g), Value::Number(w)) => {
            let (gf, wf) = (g.as_f64().unwrap(), w.as_f64().unwrap());
            let int_like = |n: &serde_json::Number| n.is_i64() || n.is_u64();
            if int_like(w) || int_like(g) {
                // An integer renders bare on both wires — byte compare.
                (g.to_string() != w.to_string()).then(|| format!("{path}: {g} vs {w}"))
            } else {
                ((gf - wf).abs() > EPS).then(|| format!("{path}: {gf} vs {wf}"))
            }
        }
        (g, w) => (g != w).then(|| format!("{path}: {g} vs {w}")),
    }
}

/// v4 `{level, message, fields}` → v5's capture rendering.
fn render_v4_line(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let message = line["message"].as_str().unwrap();
    let target = if message.starts_with("[Memories API]") {
        "quilltap_core::api::memories"
    } else if message.starts_with("Error finding") {
        "quilltap::db"
    } else {
        LOG_TARGET
    };
    let mut out = format!("{level} {target} {message}");
    if let Some(fields) = line["fields"].as_object() {
        for (k, v) in fields {
            match v {
                Value::String(s) => out.push_str(&format!(" {k}={s}")),
                Value::Object(_) | Value::Array(_) => {
                    out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
                }
                other => out.push_str(&format!(" {k}={other}")),
            }
        }
    }
    out
}

/// v5's captured lines the family compares (message in [`CAPTURED`]).
fn compared_v5_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            // `<LEVEL> <target> <message>…`
            let rest = l.splitn(3, ' ').nth(2).unwrap_or("");
            CAPTURED.iter().any(|m| rest.starts_with(m))
        })
        .cloned()
        .collect()
}

fn status_of(r: &Response) -> u16 {
    match r {
        Response::Error(e) => match e.kind {
            ErrorKind::BadRequest => 400,
            ErrorKind::NotFound => 404,
            ErrorKind::Internal => 500,
            other => panic!("unexpected kind {other:?}"),
        },
        _ => 200,
    }
}

/// The body v4's route would answer for v5's response (`{error}` /
/// `{error, details}` for a refusal, the result bare for a 200).
fn http_body(r: &Response) -> Value {
    match r {
        Response::Error(e) => e
            .validation_wire_body()
            .unwrap_or_else(|| serde_json::json!({ "error": e.message })),
        Response::Memory(v) => v.clone(),
        other => panic!("unexpected response {other:?}"),
    }
}

#[test]
fn anchor_gate_probe_matches_v4() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_ANCHOR_GATE_PROBE") else {
        eprintln!("SKIP: set QT_ORACLE_ANCHOR_GATE_PROBE (see the header)");
        return;
    };
    let corpus: Corpus = serde_json::from_str(
        &std::fs::read_to_string(
            repo().join("harness/oracle/fixtures/anchor-gate-probe-cases.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let fixture: FixtureSpec = serde_json::from_str(
        &std::fs::read_to_string(repo().join("harness/oracle/fixtures/episodic-recall.json"))
            .unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        oracle.len(),
        corpus.cases.len(),
        "the oracle and the corpus disagree on the case count — regenerate"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failures: Vec<String> = Vec::new();
    for (c, want) in corpus.cases.iter().zip(&oracle) {
        assert_eq!(want["name"], c.name.as_str(), "oracle order drifted");
        let plants: Vec<String> = corpus.base_plant.iter().chain(&c.plant).cloned().collect();
        let scratch = fresh_db(&fixture.test_pepper_base64, &plants);
        let db = &scratch.db;
        let real = provider(db);
        let failing = c.fail_text.as_ref().map(|t| FailingProvider {
            inner: real.clone(),
            fail_text: t.clone(),
            message: c
                .fail_message
                .clone()
                .unwrap_or_else(|| "canned embedding failure".to_string()),
        });

        let mut case_failures: Vec<String> = Vec::new();
        let lines: Vec<String>;
        if c.kind == "service" {
            let input = c.input.clone().unwrap_or_default();
            let run_input = RunAnchorGateProbeInput {
                character_id: input
                    .character_id
                    .clone()
                    .unwrap_or_else(|| corpus.character_id.clone()),
                user_id: corpus.user_id.clone(),
                limit: input.limit,
                embedding_profile_id: None,
            };
            let neural = c.neural.then(|| {
                let mut p = CannedEmbeddingProvider::new();
                for entry in want["canned"]
                    .as_array()
                    .expect("the neural case emits its texts")
                {
                    let embedding: Vec<f32> = entry["embedding"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_f64().unwrap() as f32)
                        .collect();
                    p = p.with_result(
                        entry["text"].as_str().unwrap(),
                        EmbeddingResult {
                            dimensions: embedding.len(),
                            embedding,
                            model: "scripted".into(),
                            provider: "OPENAI".into(),
                        },
                    );
                }
                p
            });
            let (result, captured) = quilltap_core::test_support::captured_with(|| {
                rt.block_on(async {
                    match (&failing, &neural) {
                        (Some(p), _) => run_anchor_gate_probe(db, p, &run_input).await,
                        (None, Some(p)) => run_anchor_gate_probe(db, p, &run_input).await,
                        (None, None) => run_anchor_gate_probe(db, &real, &run_input).await,
                    }
                })
            });
            lines = captured;
            let got = serde_json::to_value(result.expect("the probe answers Ok")).unwrap();
            if let Some(d) = ordered_diff("result", &got, &want["result"]) {
                case_failures.push(d);
            }
        } else {
            let body = c.body.clone().unwrap_or(Value::Null);
            let character_id = body["characterId"].as_str().unwrap_or("").to_string();
            let limit = body.get("limit").and_then(Value::as_f64);
            let (resp, captured) = quilltap_core::test_support::captured_with(|| {
                rt.block_on(memory_anchor_gate_probe(
                    db,
                    &real,
                    &corpus.user_id,
                    &character_id,
                    limit,
                ))
            });
            lines = captured;
            let want_status = want["status"].as_u64().unwrap() as u16;
            let got_status = status_of(&resp);
            if got_status != want_status {
                case_failures.push(format!("status {got_status} vs {want_status}"));
            }
            if let Some(d) = ordered_diff("body", &http_body(&resp), &want["body"]) {
                case_failures.push(d);
            }
        }

        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4_line)
            .collect();
        let got_lines = compared_v5_lines(&lines);
        if got_lines != want_lines {
            case_failures.push(format!(
                "log lines differ\n--- v4 ---\n{}\n--- v5 ---\n{}",
                want_lines.join("\n"),
                got_lines.join("\n")
            ));
        }
        report(&c.name, case_failures, &mut failures);
    }
    assert!(
        failures.is_empty(),
        "anchor-gate probe mismatches:\n{}",
        failures.join("\n\n")
    );
}

fn report(name: &str, case_failures: Vec<String>, failures: &mut Vec<String>) {
    if case_failures.is_empty() {
        eprintln!("OK: {name}");
    } else {
        eprintln!("MISMATCH: {name}");
        failures.push(format!("[{name}]\n{}", case_failures.join("\n")));
    }
}
