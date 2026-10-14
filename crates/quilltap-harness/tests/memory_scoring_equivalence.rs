//! Tier-1 differential: v4's scoring module at the pin (`83334b9a5`) —
//! `crate::memory_scoring` + `crate::embedding_scale` (P4.D289, C1-A items
//! 1–2). Drives every row the case emits:
//!
//! - `formula` — bug 216's `min(1, i + log2(max(1, n)) × 0.05)` over the
//!   `n ∈ {NaN, −1, 0, 0.5, 1, 2, 3, 7, 15, 64, 100, Infinity}` ×
//!   `i ∈ {0, 0.3, 0.5, 0.9, 0.97, 1}` grid;
//! - `salience` — the breakdown + `referenceTimeMs` (a future row, an
//!   unparseable `createdAt` → NaN propagated, `reinforcedImportance`
//!   absent / present / 0 / null, a falsy and an unparseable
//!   `lastReinforcedAt`);
//! - `sort` — `sortBySalience` (stable ties, a NaN key — V8's own short-array
//!   algorithm, `memory_scoring::js_sort`);
//! - `protection` — the wrapper for AUTO / MANUAL / CONSOLIDATED / absent /
//!   lower-case sources;
//! - `filter`, `band` (`0.29999 / 0.3 / 0.69999 / 0.7`, NaN), `lexical`
//!   (`BUILTIN`, `builtin`, `''`, null), and the constants.
//!
//! Non-finite numbers ride as strings (`"NaN"`, `"Infinity"`) — a NaN only
//! matches a NaN. Floats compare at 1e-12.
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-scoring.ts" > /tmp/oracle-memory-scoring.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_MEMORY_SCORING=/tmp/oracle-memory-scoring.ndjson \
//!     cargo test -p quilltap-harness --test memory_scoring_equivalence -- --nocapture

use quilltap_core::embedding_scale::is_lexical_embedding_provider;
use quilltap_core::memory_scoring::{
    date_ms, filter_importance, importance_band, protection, reference_time_ms,
    reinforced_importance, relevance, salience, sort_by_salience, truthy_date_ms, ProtectionInput,
    SalienceInput, DEFAULT_MIN_COSINE_NEURAL, DEFAULT_MIN_COSINE_TFIDF, DEFAULT_PROTECTION_CONFIG,
    IMPORTANCE_HIGH, IMPORTANCE_LOW, PROTECTION_THRESHOLD, RANKING_PRIORITY_WEIGHT,
    RANKING_RELEVANCE_WEIGHT, RECALL_MIN_IMPORTANCE, REINFORCED_IMPORTANCE_STEP,
    SALIENCE_HALF_LIFE_DAYS, TOOL_MIN_IMPORTANCE,
};
use serde_json::Value;

const NOW_MS: f64 = 1_782_561_600_000.0; // 2026-06-27T12:00:00.000Z
const EPS: f64 = 1e-12;

/// A JS number as the case writes it: a JSON number, or `"NaN"` / `"Infinity"`
/// / `"-Infinity"` for the non-finite ones.
fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => other
                .parse()
                .unwrap_or_else(|_| panic!("bad number token {other:?}")),
        },
        other => panic!("not a number: {other}"),
    }
}

fn same(got: f64, want: f64, ctx: &str) {
    if want.is_nan() {
        assert!(got.is_nan(), "{ctx}: rust={got} oracle=NaN");
    } else if want.is_infinite() {
        assert_eq!(got, want, "{ctx}");
    } else {
        assert!(
            (got - want).abs() <= EPS,
            "{ctx}: rust={got} oracle={want} (Δ={})",
            (got - want).abs()
        );
    }
}

fn opt_f64(m: &Value, key: &str) -> Option<f64> {
    m.get(key).and_then(Value::as_f64)
}

fn salience_input(m: &Value) -> SalienceInput {
    SalienceInput::from_iso(
        m["importance"].as_f64().unwrap(),
        opt_f64(m, "reinforcedImportance"),
        m["createdAt"].as_str().unwrap(),
        m.get("lastReinforcedAt").and_then(Value::as_str),
    )
}

fn protection_input(m: &Value) -> ProtectionInput<'_> {
    ProtectionInput {
        importance: m["importance"].as_f64().unwrap(),
        reinforced_importance: opt_f64(m, "reinforcedImportance"),
        created_at_ms: date_ms(m["createdAt"].as_str().unwrap()),
        last_reinforced_at_ms: truthy_date_ms(m.get("lastReinforcedAt").and_then(Value::as_str)),
        last_accessed_at_ms: truthy_date_ms(m.get("lastAccessedAt").and_then(Value::as_str)),
        reinforcement_count: opt_f64(m, "reinforcementCount"),
        graph_degree: m
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        kind: m.get("kind").and_then(Value::as_str),
        source: m.get("source").and_then(Value::as_str),
    }
}

#[test]
fn memory_scoring_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MEMORY_SCORING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEMORY_SCORING to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap().to_string();
        *counts.entry(kind.clone()).or_default() += 1;
        match kind.as_str() {
            "formula" => {
                let i = row["i"].as_f64().unwrap();
                let n = num(&row["n"]);
                same(
                    reinforced_importance(i, n),
                    num(&row["out"]),
                    &format!("formula i={i} n={}", row["n"]),
                );
            }
            "salience" => {
                let id = row["id"].as_str().unwrap();
                let si = salience_input(&row["input"]);
                same(
                    reference_time_ms(si.created_at_ms, si.last_reinforced_at_ms),
                    num(&row["referenceTimeMs"]),
                    &format!("salience '{id}' referenceTimeMs"),
                );
                let s = salience(&si, NOW_MS);
                let o = &row["out"];
                same(
                    s.salience,
                    num(&o["salience"]),
                    &format!("salience '{id}' salience"),
                );
                same(s.base, num(&o["base"]), &format!("salience '{id}' base"));
                same(s.decay, num(&o["decay"]), &format!("salience '{id}' decay"));
                same(
                    s.days_since,
                    num(&o["daysSince"]),
                    &format!("salience '{id}' daysSince"),
                );
            }
            "sort" => {
                let id = row["id"].as_str().unwrap();
                let items: Vec<Value> = row["items"].as_array().unwrap().clone();
                let got: Vec<String> = sort_by_salience(items, salience_input, NOW_MS)
                    .iter()
                    .map(|m| m["id"].as_str().unwrap().to_string())
                    .collect();
                let want: Vec<String> = serde_json::from_value(row["out"].clone()).unwrap();
                assert_eq!(got, want, "sort '{id}'");
            }
            "protection" => {
                let id = row["id"].as_str().unwrap();
                let p = protection(
                    &protection_input(&row["input"]),
                    NOW_MS,
                    &DEFAULT_PROTECTION_CONFIG,
                );
                let o = &row["out"];
                same(
                    p.score.score,
                    num(&o["score"]),
                    &format!("protection '{id}' score"),
                );
                same(
                    p.score.content_component,
                    num(&o["contentComponent"]),
                    &format!("protection '{id}' contentComponent"),
                );
                same(
                    p.score.reinforcement_bonus,
                    num(&o["reinforcementBonus"]),
                    &format!("protection '{id}' reinforcementBonus"),
                );
                same(
                    p.score.graph_degree_bonus,
                    num(&o["graphDegreeBonus"]),
                    &format!("protection '{id}' graphDegreeBonus"),
                );
                same(
                    p.score.recent_access_bonus,
                    num(&o["recentAccessBonus"]),
                    &format!("protection '{id}' recentAccessBonus"),
                );
                same(
                    p.score.episodic_bonus,
                    num(&o["episodicBonus"]),
                    &format!("protection '{id}' episodicBonus"),
                );
                same(
                    p.score.days_since_ref_time,
                    num(&o["daysSinceRefTime"]),
                    &format!("protection '{id}' daysSinceRefTime"),
                );
                assert_eq!(
                    p.protected,
                    o["protected"].as_bool().unwrap(),
                    "protection '{id}' protected"
                );
                assert_eq!(
                    p.protected_by_source,
                    o["protectedBySource"].as_bool().unwrap(),
                    "protection '{id}' protectedBySource"
                );
            }
            "filter" => {
                let m = &row["input"];
                same(
                    filter_importance(
                        m["importance"].as_f64().unwrap(),
                        opt_f64(m, "reinforcedImportance"),
                    ),
                    num(&row["out"]),
                    &format!("filter '{}'", row["id"]),
                );
            }
            "band" => {
                let v = num(&row["value"]);
                assert_eq!(
                    importance_band(v).as_str(),
                    row["out"].as_str().unwrap(),
                    "band {v}"
                );
            }
            "lexical" => {
                assert_eq!(
                    is_lexical_embedding_provider(row["provider"].as_str()),
                    row["out"].as_bool().unwrap(),
                    "lexical '{}'",
                    row["id"]
                );
            }
            "constants" => {
                for (name, got) in [
                    ("REINFORCED_IMPORTANCE_STEP", REINFORCED_IMPORTANCE_STEP),
                    ("SALIENCE_HALF_LIFE_DAYS", SALIENCE_HALF_LIFE_DAYS),
                    ("PROTECTION_THRESHOLD", PROTECTION_THRESHOLD),
                    ("RANKING_RELEVANCE_WEIGHT", RANKING_RELEVANCE_WEIGHT),
                    ("RANKING_PRIORITY_WEIGHT", RANKING_PRIORITY_WEIGHT),
                    ("DEFAULT_MIN_COSINE_NEURAL", DEFAULT_MIN_COSINE_NEURAL),
                    ("DEFAULT_MIN_COSINE_TFIDF", DEFAULT_MIN_COSINE_TFIDF),
                    ("RECALL_MIN_IMPORTANCE", RECALL_MIN_IMPORTANCE),
                    ("TOOL_MIN_IMPORTANCE", TOOL_MIN_IMPORTANCE),
                    ("IMPORTANCE_HIGH", IMPORTANCE_HIGH),
                    ("IMPORTANCE_LOW", IMPORTANCE_LOW),
                ] {
                    assert_eq!(got, row[name].as_f64().unwrap(), "constant {name}");
                }
                let c = &row["DEFAULT_PROTECTION_CONFIG"];
                let d = DEFAULT_PROTECTION_CONFIG;
                for (name, got) in [
                    ("contentHalfLifeDays", d.content_half_life_days),
                    ("contentFloor", d.content_floor),
                    ("maxReinforcementBonus", d.max_reinforcement_bonus),
                    ("reinforcementCoeff", d.reinforcement_coeff),
                    ("maxGraphDegreeBonus", d.max_graph_degree_bonus),
                    ("graphDegreeCoeff", d.graph_degree_coeff),
                    ("recentAccessBonus", d.recent_access_bonus),
                    ("recentAccessWindowDays", d.recent_access_window_days),
                    ("maxContentContribution", d.max_content_contribution),
                    ("episodicBonus", d.episodic_bonus),
                ] {
                    assert_eq!(
                        got,
                        c[name].as_f64().unwrap(),
                        "DEFAULT_PROTECTION_CONFIG.{name}"
                    );
                }
                assert_eq!(
                    c.as_object().unwrap().len(),
                    10,
                    "DEFAULT_PROTECTION_CONFIG key count"
                );
                assert_eq!(relevance(0.5), row["relevanceOfHalf"].as_f64().unwrap());
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    let expected: Vec<(&str, usize)> = vec![
        ("band", 8),
        ("constants", 1),
        ("filter", 4),
        ("formula", 72),
        ("lexical", 6),
        ("protection", 10),
        ("salience", 12),
        ("sort", 5),
    ];
    let got: Vec<(&str, usize)> = counts.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(got, expected, "oracle row counts drifted from the corpus");
    eprintln!("OK: memory-scoring matched oracle ({counts:?}).");
}
