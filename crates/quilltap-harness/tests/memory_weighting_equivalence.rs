//! Tier-1 differential test: the Rust memory-weighting port must produce the
//! SAME numbers as the TS oracle, field by field, for every corpus case.
//!
//! Workflow:
//!   1) Generate the oracle output (once, from the v4 server checkout):
//!        cd ~/source/quilltap-server
//!        npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-weighting.ts \
//!          > /tmp/oracle-weighting.ndjson
//!   2) Point this test at it and run:
//!        QT_ORACLE_WEIGHTING=/tmp/oracle-weighting.ndjson \
//!          cargo test -p quilltap-harness
//!
//! If QT_ORACLE_WEIGHTING is unset, the test is skipped (prints a notice) so
//! the suite stays green on machines without the server checkout. If the file
//! is present and ANY field diverges, the test fails and names the case/field.

use std::collections::HashMap;

use quilltap_core::memory_weighting::{
    calculate_effective_weight, calculate_protection_score, DEFAULT_PROTECTION_CONFIG,
    DEFAULT_WEIGHTING_CONFIG,
};
use quilltap_harness::{corpus, NOW_MS};
use serde::Deserialize;

#[derive(Deserialize)]
struct OracleRow {
    id: String,
    weight: OracleWeight,
    protection: OracleProtection,
}
#[derive(Deserialize)]
struct OracleWeight {
    #[serde(rename = "effectiveWeight")] effective_weight: f64,
    #[serde(rename = "rawWeight")] raw_weight: f64,
    #[serde(rename = "minWeight")] min_weight: f64,
    #[serde(rename = "timeDecayFactor")] time_decay_factor: f64,
    #[serde(rename = "daysOld")] days_old: f64,
    #[serde(rename = "baseImportance")] base_importance: f64,
}
#[derive(Deserialize)]
struct OracleProtection {
    score: f64,
    #[serde(rename = "contentComponent")] content_component: f64,
    #[serde(rename = "reinforcementBonus")] reinforcement_bonus: f64,
    #[serde(rename = "graphDegreeBonus")] graph_degree_bonus: f64,
    #[serde(rename = "recentAccessBonus")] recent_access_bonus: f64,
    #[serde(rename = "daysSinceRefTime")] days_since_ref_time: f64,
}

/// Exact-equivalence tolerance. f64 math run in the same IEEE-754 order on both
/// sides should match to the last bit; we allow a sub-ULP epsilon only to
/// absorb JSON's shortest-round-trip decimal formatting on re-parse.
const EPS: f64 = 1e-12;

fn close(a: f64, b: f64, case: &str, field: &str) {
    assert!(
        (a - b).abs() <= EPS,
        "DIVERGENCE in case '{case}', field '{field}': rust={a} oracle={b} (Δ={})",
        (a - b).abs()
    );
}

#[test]
fn rust_matches_oracle_field_for_field() {
    let path = match std::env::var("QT_ORACLE_WEIGHTING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_WEIGHTING to the oracle NDJSON to run the \
                 differential check (see test header)."
            );
            return;
        }
    };

    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read oracle file {path}: {e}"));
    let oracle: HashMap<String, OracleRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let row: OracleRow = serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("bad oracle line: {e}\n  {l}"));
            (row.id.clone(), row)
        })
        .collect();

    let cases = corpus();
    assert_eq!(
        cases.len(),
        oracle.len(),
        "corpus size mismatch: rust has {}, oracle has {} (corpora drifted)",
        cases.len(),
        oracle.len()
    );

    for (id, m) in cases {
        let o = oracle
            .get(id)
            .unwrap_or_else(|| panic!("oracle missing case '{id}' (corpora drifted)"));

        let w = calculate_effective_weight(&m, &DEFAULT_WEIGHTING_CONFIG, NOW_MS);
        close(w.effective_weight, o.weight.effective_weight, id, "weight.effectiveWeight");
        close(w.raw_weight, o.weight.raw_weight, id, "weight.rawWeight");
        close(w.min_weight, o.weight.min_weight, id, "weight.minWeight");
        close(w.time_decay_factor, o.weight.time_decay_factor, id, "weight.timeDecayFactor");
        close(w.days_old, o.weight.days_old, id, "weight.daysOld");
        close(w.base_importance, o.weight.base_importance, id, "weight.baseImportance");

        let p = calculate_protection_score(&m, &DEFAULT_PROTECTION_CONFIG, NOW_MS);
        close(p.score, o.protection.score, id, "protection.score");
        close(p.content_component, o.protection.content_component, id, "protection.contentComponent");
        close(p.reinforcement_bonus, o.protection.reinforcement_bonus, id, "protection.reinforcementBonus");
        close(p.graph_degree_bonus, o.protection.graph_degree_bonus, id, "protection.graphDegreeBonus");
        close(p.recent_access_bonus, o.protection.recent_access_bonus, id, "protection.recentAccessBonus");
        close(p.days_since_ref_time, o.protection.days_since_ref_time, id, "protection.daysSinceRefTime");
    }

    eprintln!("OK: {} cases matched oracle field-for-field.", oracle.len());
}
