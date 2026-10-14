//! Tier-1 differential (P4.D291): the recall ranking — v4
//! `lib/memory/scoring/rank.ts` (NEW in `584d28164`, pinned at `83334b9a5`)
//! against v5's `quilltap_core::services::recall::rank`.
//!
//! Every oracle row carries its INPUT; this side recomputes from the same bytes
//! and compares:
//! - `rankForRecall` — the ranked ids in order, each row's score, salience,
//!   multiplier, `fired` labels (incl. `extraFired`) and the blend before /
//!   after (floats 1e-12; NaN as NaN), over contexts covering every multiplier,
//!   the cross-project exclusion, the R1 gate / R2 cap (defaults and an
//!   explicit legacy tuning), and a tie / NaN-salience pool (the stable sort);
//! - `rankByBlend` — the blend-alone order;
//! - `reserve` — R6's `reserveBackgroundSlots` arithmetic (the id order).
//!
//! Generate the oracle (from the v4 checkout, or a worktree pinned at the
//! round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/recall-rank.ts \
//!     > /tmp/oracle-recall-rank.ndjson
//! Run:
//!   QT_ORACLE_RECALL_RANK=/tmp/oracle-recall-rank.ndjson \
//!     cargo test -p quilltap-harness --test recall_rank_equivalence -- --nocapture

use std::collections::HashSet;

use quilltap_core::recall_tags::{ContextTag, ScopePolicy, TemporalTag, TimeWindow};
use quilltap_core::recall_tuning::{resolve_recall_tuning, validate_recall_tuning_input};
use quilltap_core::services::recall::context::RecallContextInput;
use quilltap_core::services::recall::rank::{
    rank_by_blend, rank_for_recall, reserve_background_slots, RankInput,
};
use quilltap_core::services::recall::types::{RecallAdjustment, RecallCandidate};
use serde_json::{json, Value};

const EPS: f64 = 1e-12;

fn f(v: &Value) -> f64 {
    match v {
        Value::String(s) if s == "NaN" => f64::NAN,
        other => other
            .as_f64()
            .unwrap_or_else(|| panic!("number expected: {other}")),
    }
}

fn feq(a: f64, b: f64, what: &str) {
    if a.is_nan() || b.is_nan() {
        assert!(a.is_nan() && b.is_nan(), "{what}: rust={a} oracle={b}");
        return;
    }
    assert!((a - b).abs() <= EPS, "{what}: rust={a} oracle={b}");
}

fn rank_inputs(v: &Value) -> Vec<RankInput> {
    v.as_array()
        .expect("candidates")
        .iter()
        .map(|c| RankInput {
            memory: c["memory"].clone(),
            score: f(&c["score"]),
            raw_score: f(&c["rawScore"]),
            salience: f(&c["salience"]),
            used_embedding: c["usedEmbedding"].as_bool().unwrap(),
        })
        .collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn context(c: &Value) -> RecallContextInput {
    let s = |k: &str| c.get(k).and_then(Value::as_str).map(str::to_string);
    RecallContextInput {
        current_project_id: s("currentProjectId"),
        scope_policy: if s("scopePolicy").as_deref() == Some("exclude") {
            ScopePolicy::Exclude
        } else {
            ScopePolicy::DownWeight
        },
        present_about_character_ids: strings(&c["presentAboutCharacterIds"]),
        turn_context: s("turnContext").and_then(|t| ContextTag::from_kw(&t)),
        turn_temporal: s("turnTemporal").and_then(|t| TemporalTag::from_kw(&t)),
        turn_retrospective: c
            .get("turnRetrospective")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        occurred_within: c
            .get("occurredWithin")
            .filter(|w| w.is_object())
            .map(|w| TimeWindow {
                from: w["from"].as_str().unwrap().to_string(),
                to: w["to"].as_str().unwrap().to_string(),
            }),
        expand_related: c
            .get("expandRelated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        recently_whispered_ids: c
            .get("recentlyWhisperedIds")
            .map(|v| strings(v).into_iter().collect::<HashSet<_>>()),
        current_chat_id: s("currentChatId"),
        now_ms: c.get("nowMs").and_then(Value::as_f64),
        tuning: c.get("tuningInput").map(|t| {
            let (input, _) = validate_recall_tuning_input(t).expect("tuning input validates");
            resolve_recall_tuning(Some(&input))
        }),
        present_participant_names: strings(&c["presentParticipantNames"]),
    }
}

fn assert_candidates(got: &[RecallCandidate], want: &Value, id: &str) {
    let want = want.as_array().expect("output array");
    let got_ids: Vec<&str> = got.iter().map(|c| c.id()).collect();
    let want_ids: Vec<&str> = want.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(got_ids, want_ids, "{id}: ranked ids");
    for (g, w) in got.iter().zip(want) {
        let rid = format!("{id}/{}", g.id());
        feq(g.score, f(&w["score"]), &format!("{rid}: score"));
        feq(g.salience, f(&w["salience"]), &format!("{rid}: salience"));
        assert_eq!(
            g.used_embedding,
            w["usedEmbedding"].as_bool().unwrap(),
            "{rid}: usedEmbedding"
        );
        match (&g.recall_adjustment, &w["recallAdjustment"]) {
            (None, Value::Null) => {}
            (Some(a), Value::Object(wa)) => {
                feq(
                    a.multiplier,
                    f(&wa["multiplier"]),
                    &format!("{rid}: multiplier"),
                );
                assert_eq!(a.fired, strings(&wa["fired"]), "{rid}: fired");
                feq(
                    a.blended_before,
                    f(&wa["blendedBefore"]),
                    &format!("{rid}: blendedBefore"),
                );
                feq(
                    a.blended_after,
                    f(&wa["blendedAfter"]),
                    &format!("{rid}: blendedAfter"),
                );
            }
            (a, w) => panic!("{rid}: recallAdjustment rust={a:?} oracle={w}"),
        }
    }
}

/// Rebuild a ranked candidate from the oracle's output shape.
fn candidate(c: &Value) -> RecallCandidate {
    RecallCandidate {
        memory: json!({ "id": c["id"] }),
        score: f(&c["score"]),
        used_embedding: c["usedEmbedding"].as_bool().unwrap(),
        salience: f(&c["salience"]),
        recall_adjustment: c["recallAdjustment"].as_object().map(|a| RecallAdjustment {
            multiplier: f(&a["multiplier"]),
            fired: strings(&a["fired"]),
            blended_before: f(&a["blendedBefore"]),
            blended_after: f(&a["blendedAfter"]),
        }),
    }
}

#[test]
fn recall_rank_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_RECALL_RANK") else {
        eprintln!("SKIP: QT_ORACLE_RECALL_RANK not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("ndjson row");
        let kind = row["kind"].as_str().unwrap().to_string();
        let id = row["id"].as_str().unwrap_or("").to_string();
        match kind.as_str() {
            "rankForRecall" => {
                let input = &row["input"];
                let extra = strings(&input["extraFired"]);
                let extra: Vec<&str> = extra.iter().map(String::as_str).collect();
                let got = rank_for_recall(
                    &rank_inputs(&input["candidates"]),
                    &context(&input["context"]),
                    f(&input["bestCosine"]),
                    &extra,
                );
                assert_candidates(&got, &row["output"], &id);
            }
            "rankByBlend" => {
                let got = rank_by_blend(&rank_inputs(&row["input"]["candidates"]));
                assert_candidates(&got, &row["output"], &id);
            }
            "reserve" => {
                let input = &row["input"];
                let ranked: Vec<RecallCandidate> = input["ranked"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(candidate)
                    .collect();
                let background: Vec<RecallCandidate> = input["background"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(candidate)
                    .collect();
                let got = reserve_background_slots(
                    ranked,
                    background,
                    input["headSize"].as_u64().unwrap() as usize,
                    f(&input["fraction"]),
                );
                let got_ids: Vec<&str> = got.iter().map(|c| c.id()).collect();
                assert_eq!(got_ids, strings(&row["output"]), "reserve/{id}");
            }
            "meta" => continue,
            other => panic!("unknown oracle kind {other}"),
        }
        *counts.entry(kind).or_default() += 1;
    }
    eprintln!("recall_rank rows: {counts:?}");
    assert!(counts.get("rankForRecall").copied().unwrap_or(0) >= 100);
    assert_eq!(counts.get("rankByBlend").copied(), Some(2));
    assert_eq!(counts.get("reserve").copied(), Some(8));
}
