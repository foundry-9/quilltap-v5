//! Tier-1 differential: v4's PURE admission decision at the pin (`83334b9a5`)
//! — `crate::admission::{decide, outcome, describe, thresholds}` (P4.D289
//! unit 7, C1-A item 6) against v4's REAL
//! `lib/memory/admission/{decide,outcome,describe}.ts` and the threshold block
//! of `lib/memory/settings-defaults.ts` (+ `ADMISSION_TOP_K`).
//!
//! Rows (`kind`):
//! - `constant` — both threshold sets field by field AND their JSON bytes
//!   (`serde_json::to_string` vs v4's `JSON.stringify`), `ADMISSION_TOP_K`;
//! - `provider` — `admission_thresholds_for_provider` (`BUILTIN` only is
//!   TF-IDF; `builtin`, `''`, null, unknown read as neural);
//! - `distinct` — `occasions_are_distinct(a, b, date_guard_days)`;
//! - `decide` — every expectation of `admission-decide.test.ts`, the TF-IDF
//!   rows of `provider-thresholds.test.ts`, equal cosines (stable), NaN /
//!   infinite cosines (V8's sort, `memory_scoring::js_sort`), the three
//!   `insert-linked` reasons, cold dangling digests, `via: ''`, the
//!   exclusion order; the ACTION and the ASSESSMENTS compared as JSON BYTES
//!   (a `preserve_order` map's `==` is order-blind — memory note), plus
//!   `admission_inserted` / `admission_reinforced` /
//!   `is_near_duplicate_decision`;
//! - `describe` — `describe_admission` for every action kind (a non-BMP id
//!   prefix, a fractional count, `1e21`, a NaN similarity, the `unknown` /
//!   `n/a` / `?` fallbacks). Strings compare exactly.
//!
//! Non-finite numbers in the INPUTS ride as strings (`"NaN"`, `"Infinity"`).
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/admission-decide.ts" > /tmp/oracle-admission-decide.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_ADMISSION_DECIDE=/tmp/oracle-admission-decide.ndjson \
//!     cargo test -p quilltap-harness --test admission_decide_equivalence -- --nocapture

use quilltap_core::admission::{
    admission_inserted, admission_reinforced, admission_thresholds_for_provider, decide_admission,
    describe_admission, is_near_duplicate_decision, occasions_are_distinct, AdmissionAction,
    AdmissionCandidateView, AdmissionNeighbour, AdmissionOutcome, AdmissionThresholds,
    AdmissionTrace, NeighbourAssessment, NeighbourExclusion, Verdict, ADMISSION_THRESHOLDS,
    ADMISSION_THRESHOLDS_TFIDF, ADMISSION_TOP_K,
};
use serde_json::Value;

/// A JS number as the case writes it: a JSON number, or `"NaN"` /
/// `"Infinity"` / `"-Infinity"`.
fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => panic!("bad number token {other:?}"),
        },
        other => panic!("not a number: {other}"),
    }
}

fn set(name: &str) -> &'static AdmissionThresholds {
    match name {
        "neural" => &ADMISSION_THRESHOLDS,
        "tfidf" => &ADMISSION_THRESHOLDS_TFIDF,
        other => panic!("unknown threshold set {other}"),
    }
}

fn set_name(t: &AdmissionThresholds) -> &'static str {
    if *t == ADMISSION_THRESHOLDS {
        "neural"
    } else if *t == ADMISSION_THRESHOLDS_TFIDF {
        "tfidf"
    } else {
        "other"
    }
}

fn neighbour(v: &Value) -> AdmissionNeighbour {
    AdmissionNeighbour {
        memory: v["memory"].clone(),
        cosine: num(&v["cosine"]),
        via: v.get("via").and_then(Value::as_str).map(str::to_string),
        dangling_digest: v.get("danglingDigest") == Some(&Value::Bool(true)),
    }
}

fn assessment(v: &Value) -> NeighbourAssessment {
    NeighbourAssessment {
        id: v["id"].as_str().unwrap().to_string(),
        cosine: num(&v["cosine"]),
        via: v.get("via").and_then(Value::as_str).map(str::to_string),
        verdict: serde_json::from_value::<Verdict>(v["verdict"].clone()).unwrap(),
        excluded_by: v
            .get("excludedBy")
            .map(|e| serde_json::from_value::<NeighbourExclusion>(e.clone()).unwrap()),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn outcome(v: &Value) -> AdmissionOutcome {
    let trace = &v["trace"];
    AdmissionOutcome {
        action: serde_json::from_value::<AdmissionAction>(v["action"].clone()).unwrap(),
        memory: match &v["memory"] {
            Value::Null => None,
            m => Some(m.clone()),
        },
        novel_details: strings(&v["novelDetails"]),
        linked_ids: strings(&v["linkedIds"]),
        trace: AdmissionTrace {
            thresholds: ADMISSION_THRESHOLDS,
            assessments: trace["assessments"]
                .as_array()
                .unwrap()
                .iter()
                .map(assessment)
                .collect(),
            embedding_attempts: trace["embeddingAttempts"].as_u64().unwrap() as u32,
            orphans_ignored: trace["orphansIgnored"].as_u64().unwrap() as u32,
            searched_k: trace["searchedK"].as_u64().unwrap() as u32,
            notes: strings(&trace["notes"]),
        },
    }
}

#[test]
fn admission_decide_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_ADMISSION_DECIDE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_ADMISSION_DECIDE to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut constants, mut providers, mut distincts, mut decides, mut describes) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        match row["kind"].as_str().unwrap() {
            "constant" => {
                if id == "top-k" {
                    if row["out"].as_u64().unwrap() as usize != ADMISSION_TOP_K {
                        failures.push(format!("{id}: {ADMISSION_TOP_K} vs {}", row["out"]));
                    }
                } else {
                    let t = set(id.strip_prefix("thresholds-").unwrap());
                    let out = &row["out"];
                    let fields_ok = t.near_duplicate == num(&out["nearDuplicate"])
                        && t.reinforce == num(&out["reinforce"])
                        && t.related == num(&out["related"])
                        && t.date_guard_days == num(&out["dateGuardDays"])
                        && t.footnote_cap as u64 == out["footnoteCap"].as_u64().unwrap();
                    let got_json = serde_json::to_string(t).unwrap();
                    if !fields_ok || got_json != row["json"].as_str().unwrap() {
                        failures.push(format!("{id}: rust {got_json} vs oracle {}", row["json"]));
                    }
                }
                constants += 1;
            }
            "provider" => {
                let got = set_name(admission_thresholds_for_provider(row["provider"].as_str()));
                if got != row["out"].as_str().unwrap() {
                    failures.push(format!("{id}: rust {got} vs oracle {}", row["out"]));
                }
                providers += 1;
            }
            "distinct" => {
                let got =
                    occasions_are_distinct(row["a"].as_str(), row["b"].as_str(), num(&row["days"]));
                if got != row["out"].as_bool().unwrap() {
                    failures.push(format!("{id}: rust {got} vs oracle {}", row["out"]));
                }
                distincts += 1;
            }
            "decide" => {
                let t = set(row["thresholds"].as_str().unwrap());
                let c = &row["candidate"];
                let candidate = AdmissionCandidateView {
                    holder_id: c["holderId"].as_str().unwrap(),
                    about_id: c["aboutId"].as_str(),
                    kind: c["kind"].as_str(),
                    source: c["source"].as_str(),
                    occurred_at: c["occurredAt"].as_str(),
                };
                let neighbours: Vec<AdmissionNeighbour> = row["neighbours"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(neighbour)
                    .collect();
                let decision = decide_admission(&candidate, &neighbours, t);
                let got_action = serde_json::to_string(&decision.action).unwrap();
                let got_assessments = serde_json::to_string(&decision.assessments).unwrap();
                let inserted = admission_inserted(&decision.action);
                let reinforced = admission_reinforced(&decision.action);
                let near = is_near_duplicate_decision(&decision, t);
                if got_action != row["actionJson"].as_str().unwrap()
                    || got_assessments != row["assessmentsJson"].as_str().unwrap()
                    || inserted != row["inserted"].as_bool().unwrap()
                    || reinforced != row["reinforced"].as_bool().unwrap()
                    || near != row["nearDuplicate"].as_bool().unwrap()
                {
                    failures.push(format!(
                        "{id}:\n  rust:   {got_action} {got_assessments} ins={inserted} rei={reinforced} near={near}\n  oracle: {} {} ins={} rei={} near={}",
                        row["actionJson"].as_str().unwrap(),
                        row["assessmentsJson"].as_str().unwrap(),
                        row["inserted"],
                        row["reinforced"],
                        row["nearDuplicate"]
                    ));
                }
                decides += 1;
            }
            "describe" => {
                let o = outcome(&row["outcome"]);
                let got = describe_admission(
                    &o,
                    row["label"].as_str().unwrap(),
                    row["summary"].as_str().unwrap(),
                );
                if got != row["out"].as_str().unwrap() {
                    failures.push(format!("{id}:\n  rust:   {got}\n  oracle: {}", row["out"]));
                }
                describes += 1;
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    assert!(
        failures.is_empty(),
        "diverged ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Corpus drift guard: the case's exact row counts.
    assert_eq!(
        (constants, providers, distincts, decides, describes),
        // 85 → 86 decides at the `83334b9a5` unification: the `nan-descending-run` row.
        (3, 7, 17, 86, 28),
        "row counts (constant, provider, distinct, decide, describe)"
    );
    eprintln!(
        "OK: admission-decide matched oracle ({constants} constant, {providers} provider, {distincts} distinct, {decides} decide, {describes} describe rows)."
    );
}
