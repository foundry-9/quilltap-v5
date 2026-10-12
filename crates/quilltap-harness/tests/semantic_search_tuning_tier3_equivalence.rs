//! Tier-3 differential (P4.D267): `search_memories_semantic` against v4's REAL
//! `searchMemoriesSemantic` at the `01a83539d` pin — the recall multiplier
//! retuning inside the search (R1's relevance gate and its TF-IDF gate-off,
//! R2's cap, R4's specific entity anchors over a per-phrase hit cache, R6's
//! background reservation), the harness knobs (`exclude_memory_ids` filtered
//! inside the vector scan, `weight_clock_ms`, `head_size`, the embedding memo),
//! hot-only search (`include_cold`) and F6 (the search stamps nothing).
//!
//! Both sides run over a fresh copy of the committed
//! `semantic-search-tuning-main.db` per case (built AT the pin by
//! `harness/oracle/fixtures/build-semantic-search-tuning-fixture.ts`; every
//! embedding a 2-D unit vector `[c, √(1−c²)]`). ONLY the embedding boundary is
//! scripted, by exact text, with a per-case `provider` label (`OPENAI`
//! exercises R1; `BUILTIN` pins the gate-off): v4 `jest.doMock`s
//! `generateEmbeddingForUser`, v5 a [`ScriptedEmbedding`] that also COUNTS
//! calls. Comparands, per call: ids in order; `score`, `rawWeight`,
//! `effectiveWeight` (1e-12), `usedEmbedding`, `recallAdjustment {multiplier,
//! fired, blendedBefore, blendedAfter}`; per case: the embedded texts in
//! order (the memo), the search's captured lines (level, message, v4's keys in
//! order; arrays as `…Json`), and `lastAccessedAt` UNCHANGED before → after on
//! both sides. Plus the pure `reserve_background_slots` over v4's jest vectors.
//!
//! Generate the oracle (Node 24, from the v4 checkout pinned at the baseline;
//! STAGE outside `.claude/` — v4's jest ignores those paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-sst-stage
//!   rm -rf $STAGE; mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $V5W/harness/oracle/cases/semantic-search-tuning-tier3.test.ts $STAGE/harness/oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/semantic-search-tuning.json $STAGE/harness/oracle/fixtures/
//!   cp $V5W/harness/oracle/fixtures/semantic-search-tuning-cases.json $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_SST_MAIN=$V5W/crates/quilltap-web/tests/fixtures/semantic-search-tuning-main.db \
//!   QT_ORACLE_OUT=/tmp/oracle-semantic-search-tuning.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "semantic-search-tuning-tier3\.test\.ts$"
//! Run:
//!   QT_ORACLE_SEMANTIC_SEARCH_TUNING=/tmp/oracle-semantic-search-tuning.ndjson \
//!     cargo test -p quilltap-harness --test semantic_search_tuning_tier3_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::{
    EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
use quilltap_core::recall_tags::{ContextTag, ScopePolicy, TemporalTag, TimeWindow};
use quilltap_core::recall_tuning::{resolve_recall_tuning, validate_recall_tuning_input};
use quilltap_core::services::memory_service::{
    reserve_background_slots, search_memories_semantic, EmbeddingMemo, RecallAdjustment,
    RecallContextInput, SemanticSearchOptions, SemanticSearchResult,
};
use quilltap_core::test_support::global_capture;
use serde_json::Value;

const EPS: f64 = 1e-12;
const TARGET: &str = "quilltap::memories";

/// The embedding seam: the case's scripted vector + provider label by exact
/// text, or a thrown `EmbeddingError` — and every call recorded.
struct ScriptedEmbedding {
    spec: HashMap<String, Value>,
    calls: Mutex<Vec<String>>,
}

impl EmbeddingProvider for ScriptedEmbedding {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        _user_id: &str,
        _profile_id: Option<&str>,
        _priority: EmbeddingPriority,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        self.calls.lock().unwrap().push(text.to_string());
        let result = match self.spec.get(text) {
            Some(s) if s.get("throw") != Some(&Value::Bool(true)) => {
                let v: Vec<f32> = s["vector"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap() as f32)
                    .collect();
                Ok(EmbeddingResult {
                    dimensions: v.len(),
                    embedding: v,
                    model: "canned".to_string(),
                    provider: s["provider"].as_str().unwrap().to_string(),
                })
            }
            _ => Err(EmbeddingError::new("scripted embedding failure")),
        };
        async move { result }
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The corpus's v4-shaped options → v5's [`SemanticSearchOptions`].
fn options(
    o: &Value,
    user_id: &str,
    now_ms: f64,
    memo: Option<EmbeddingMemo>,
) -> SemanticSearchOptions {
    let rc = o.get("recallContext").map(|rc| RecallContextInput {
        current_project_id: rc["currentProjectId"].as_str().map(str::to_string),
        scope_policy: if rc["scopePolicy"] == "exclude" {
            ScopePolicy::Exclude
        } else {
            ScopePolicy::DownWeight
        },
        present_about_character_ids: strings(&rc["presentAboutCharacterIds"]),
        turn_context: rc["turnContext"].as_str().and_then(ContextTag::from_kw),
        turn_temporal: rc["turnTemporal"].as_str().and_then(TemporalTag::from_kw),
        turn_retrospective: rc["turnRetrospective"] == true,
        expand_related: rc["expandRelated"] == true,
        recently_whispered_ids: rc
            .get("recentlyWhisperedIds")
            .map(|v| strings(v).into_iter().collect::<HashSet<_>>()),
        current_chat_id: rc["currentChatId"].as_str().map(str::to_string),
        now_ms: rc["nowMs"].as_f64(),
        present_participant_names: strings(&rc["presentParticipantNames"]),
    });
    let recall_tuning = o
        .get("recallContext")
        .and_then(|rc| rc.get("tuningInput"))
        .map(|raw| {
            let (input, _) = validate_recall_tuning_input(raw).expect("v4-valid tuning input");
            resolve_recall_tuning(Some(&input))
        });
    SemanticSearchOptions {
        user_id: user_id.to_string(),
        limit: o["limit"].as_u64().map(|n| n as usize),
        min_score: o["minScore"].as_f64(),
        min_importance: o["minImportance"].as_f64(),
        source: o["source"].as_str().map(str::to_string),
        about_character_id: o["aboutCharacterId"].as_str().map(str::to_string),
        apply_literal_phrase_boost: o["applyLiteralPhraseBoost"] == true,
        recall_context: rc,
        occurred_within: o.get("occurredWithin").map(|w| TimeWindow {
            from: w["from"].as_str().unwrap().to_string(),
            to: w["to"].as_str().unwrap().to_string(),
        }),
        entity_anchors: strings(&o["entityAnchors"]),
        extra_probes: strings(&o["extraProbes"]),
        now_ms,
        include_cold: o["includeCold"] == true,
        exclude_memory_ids: strings(&o["excludeMemoryIds"]),
        head_size: o["headSize"].as_u64().map(|n| n as usize),
        weight_clock_ms: o["weightClockMs"].as_i64(),
        recall_tuning,
        embedding_memo: memo,
        ..Default::default()
    }
}

fn feq(got: f64, want: &Value, what: &str) {
    let w = want
        .as_f64()
        .unwrap_or_else(|| panic!("{what}: oracle not a number: {want}"));
    assert!((got - w).abs() <= EPS, "{what}: rust={got} oracle={w}");
}

fn assert_adjustment(got: &Option<RecallAdjustment>, want: &Value, what: &str) {
    match (got, want) {
        (None, Value::Null) => {}
        (Some(a), Value::Object(w)) => {
            feq(
                a.multiplier,
                &w["multiplier"],
                &format!("{what} multiplier"),
            );
            assert_eq!(a.fired, strings(&w["fired"]), "{what} fired");
            feq(
                a.blended_before,
                &w["blendedBefore"],
                &format!("{what} blendedBefore"),
            );
            feq(
                a.blended_after,
                &w["blendedAfter"],
                &format!("{what} blendedAfter"),
            );
        }
        (g, w) => panic!("{what}: recallAdjustment rust={g:?} oracle={w}"),
    }
}

/// v4's captured `{level, message, fields}` rendered the way v5's capture rig
/// renders a `tracing` line: `LEVEL target message k=v …`, arrays / objects
/// under `…Json` (the file-layer convention), numbers as JS prints them.
fn expected_line(line: &Value) -> String {
    let mut out = format!(
        "{} {TARGET} {}",
        line["level"].as_str().unwrap().to_ascii_uppercase(),
        line["message"].as_str().unwrap()
    );
    for (k, v) in line["fields"].as_object().unwrap() {
        match v {
            Value::String(s) => out.push_str(&format!(" {k}={s}")),
            Value::Array(_) | Value::Object(_) => {
                out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
            }
            Value::Number(n) => out.push_str(&format!(
                " {k}={}",
                quilltap_core::pascal::js_value::number_to_string(n.as_f64().unwrap())
            )),
            other => out.push_str(&format!(" {k}={other}")),
        }
    }
    out
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn spec_json(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

async fn access_column(db: &Db) -> Vec<(String, Option<String>)> {
    db.read_main(|conn| {
        let mut st = conn
            .prepare(r#"SELECT "id", "lastAccessedAt" FROM "memories" ORDER BY "id""#)
            .map_err(quilltap_core::db::DbError::from)?;
        let rows = st
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(quilltap_core::db::DbError::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(quilltap_core::db::DbError::from)?;
        Ok(rows)
    })
    .unwrap()
}

fn oracle_access(v: &Value) -> Vec<(String, Option<String>)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_string(),
                r["lastAccessedAt"].as_str().map(str::to_string),
            )
        })
        .collect()
}

#[tokio::test]
async fn semantic_search_tuning_matches_oracle() {
    global_capture::install();
    let Ok(path) = std::env::var("QT_ORACLE_SEMANTIC_SEARCH_TUNING") else {
        eprintln!("SKIP: set QT_ORACLE_SEMANTIC_SEARCH_TUNING to the oracle NDJSON (see header).");
        return;
    };
    let rows: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let spec = spec_json("semantic-search-tuning-cases.json");
    let fixture = spec_json("semantic-search-tuning.json");
    let pepper = fixture["testPepperBase64"].as_str().unwrap();
    let user_id = spec["userId"].as_str().unwrap();
    let now_ms = spec["$nowMs"].as_f64().unwrap();
    let cases = spec["cases"].as_array().unwrap();
    let reserves = spec["reserve"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        cases.len() + reserves.len(),
        "corpus/oracle row-count mismatch — regenerate the oracle"
    );

    let (mut n_calls, mut n_lines) = (0, 0);
    for (case, oracle) in cases.iter().zip(&rows) {
        let name = case["name"].as_str().unwrap();
        assert_eq!(oracle["name"], name, "case order mismatch");
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-sst-{name}-"))
            .tempdir()
            .unwrap();
        let main = scratch.path().join("main.db");
        std::fs::copy(fixtures_dir().join("semantic-search-tuning-main.db"), &main).unwrap();
        let db = Db::open(
            DbPaths {
                main,
                mount_index: None,
                llm_logs: None,
            },
            pepper,
        )
        .expect("open fixture copy");
        let provider = ScriptedEmbedding {
            spec: case["embed"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            calls: Mutex::new(Vec::new()),
        };
        let memo: EmbeddingMemo = Default::default();

        let access_before = access_column(&db).await;
        let (results, lines) = global_capture::capture_async(async {
            let mut all: Vec<Vec<SemanticSearchResult>> = Vec::new();
            for call in case["calls"].as_array().unwrap() {
                let o = options(
                    &call["options"],
                    user_id,
                    now_ms,
                    (call["memo"] == true).then(|| memo.clone()),
                );
                let got = search_memories_semantic(
                    &db,
                    &provider,
                    call["characterId"].as_str().unwrap(),
                    call["query"].as_str().unwrap(),
                    &o,
                    None,
                )
                .await
                .expect("search");
                all.push(got);
            }
            all
        })
        .await;
        let access_after = access_column(&db).await;

        // Results, call by call.
        let want_calls = oracle["calls"].as_array().unwrap();
        assert_eq!(results.len(), want_calls.len(), "{name}: call count");
        for (ci, (got, want)) in results.iter().zip(want_calls).enumerate() {
            let want = want.as_array().unwrap();
            let got_ids: Vec<&str> = got
                .iter()
                .map(|r| r.memory["id"].as_str().unwrap())
                .collect();
            let want_ids: Vec<&str> = want.iter().map(|r| r["id"].as_str().unwrap()).collect();
            assert_eq!(got_ids, want_ids, "{name} call {ci}: ids in order");
            for (g, w) in got.iter().zip(want) {
                let what = format!("{name} call {ci} {}", w["id"]);
                feq(g.score, &w["score"], &format!("{what} score"));
                feq(g.raw_weight, &w["rawWeight"], &format!("{what} rawWeight"));
                feq(
                    g.effective_weight,
                    &w["effectiveWeight"],
                    &format!("{what} effectiveWeight"),
                );
                assert_eq!(
                    Value::Bool(g.used_embedding),
                    w["usedEmbedding"],
                    "{what} usedEmbedding"
                );
                assert_adjustment(&g.recall_adjustment, &w["recallAdjustment"], &what);
            }
            n_calls += 1;
        }

        // The memo: the texts embedded, in order.
        assert_eq!(
            *provider.calls.lock().unwrap(),
            strings(&oracle["embedTexts"]),
            "{name}: embedded texts"
        );

        // The search's lines (only this target's; the store's own debug line
        // rides `quilltap::db`).
        let got_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.split(' ').nth(1) == Some(TARGET))
            .collect();
        let want_lines: Vec<String> = oracle["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(expected_line)
            .collect();
        assert_eq!(
            got_lines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            want_lines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            "{name}: captured lines"
        );
        n_lines += want_lines.len();

        // F6: the search stamps nothing, on either side.
        assert_eq!(
            access_before, access_after,
            "{name}: v5 search wrote lastAccessedAt"
        );
        assert_eq!(
            oracle_access(&oracle["accessBefore"]),
            oracle_access(&oracle["accessAfter"]),
            "{name}: v4 search wrote lastAccessedAt"
        );
        assert_eq!(
            access_before,
            oracle_access(&oracle["accessBefore"]),
            "{name}: corpus"
        );
        drop(db);
    }

    // The pure R6 slotting.
    for (r, oracle) in reserves.iter().zip(&rows[cases.len()..]) {
        let name = r["name"].as_str().unwrap();
        assert_eq!(oracle["name"], name, "reserve order mismatch");
        let row = |pair: &Value| SemanticSearchResult {
            memory: serde_json::json!({ "id": pair[0] }),
            score: 0.0,
            used_embedding: true,
            effective_weight: 0.0,
            raw_weight: 0.0,
            recall_adjustment: Some(RecallAdjustment {
                multiplier: 1.0,
                fired: vec![],
                blended_before: pair[1].as_f64().unwrap(),
                blended_after: pair[1].as_f64().unwrap(),
            }),
        };
        let got = reserve_background_slots(
            r["ranked"].as_array().unwrap().iter().map(row).collect(),
            r["background"]
                .as_array()
                .unwrap()
                .iter()
                .map(row)
                .collect(),
            r["headSize"].as_u64().unwrap() as usize,
            r["fraction"].as_f64().unwrap(),
        );
        let ids: Vec<String> = got
            .iter()
            .map(|x| x.memory["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, strings(&oracle["ids"]), "reserve {name}");
    }
    eprintln!(
        "OK: semantic-search-tuning matched oracle ({} cases, {n_calls} calls, {n_lines} lines, {} reserve rows).",
        cases.len(),
        reserves.len()
    );
}
