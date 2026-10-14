//! Tier-1 differential: the Phase-1 weighting corpus (17 ids) re-aimed at
//! v4's scoring module (P4.D289, pin `83334b9a5`) — `salience`, the blended
//! `calculateProtectionScore`, the `protection` wrapper and
//! `formatMemoryAge`, field for field (floats at 1e-12, the age label exact).
//!
//! v4 `e0ac7493a` deleted `lib/memory/memory-weighting.ts`: the floored
//! effective weight has NO successor (orderings moved to salience), so this
//! family no longer reads it. The corpus is `quilltap_harness::corpus()`.
//!
//! Generate the oracle output (from a v4 checkout or the round's pin), then
//! run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-weighting.ts" > /tmp/oracle-weighting.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_WEIGHTING=/tmp/oracle-weighting.ndjson \
//!     cargo test -p quilltap-harness --test memory_weighting_equivalence -- --nocapture
//!
//! If QT_ORACLE_WEIGHTING is unset, the test is skipped (prints a notice) so
//! the suite stays green on machines without the server checkout. If the file
//! is present and ANY field diverges, the test fails and names the case/field.

use std::collections::HashMap;

use quilltap_core::memory_scoring::{
    calculate_protection_score, protection, salience, DEFAULT_PROTECTION_CONFIG,
};
use quilltap_core::memory_time::format_memory_age;
use quilltap_harness::{corpus, NOW_MS};
use serde::Deserialize;

#[derive(Deserialize)]
struct OracleRow {
    id: String,
    salience: OracleSalience,
    protection: OracleProtection,
    #[serde(rename = "protectionWrapper")]
    protection_wrapper: OracleWrapper,
    /// Episodic spine: the event-clock age label.
    age: String,
}
#[derive(Deserialize)]
struct OracleSalience {
    salience: f64,
    base: f64,
    decay: f64,
    #[serde(rename = "daysSince")]
    days_since: f64,
}
#[derive(Deserialize)]
struct OracleProtection {
    score: f64,
    #[serde(rename = "contentComponent")]
    content_component: f64,
    #[serde(rename = "reinforcementBonus")]
    reinforcement_bonus: f64,
    #[serde(rename = "graphDegreeBonus")]
    graph_degree_bonus: f64,
    #[serde(rename = "recentAccessBonus")]
    recent_access_bonus: f64,
    #[serde(rename = "episodicBonus")]
    episodic_bonus: f64,
    #[serde(rename = "daysSinceRefTime")]
    days_since_ref_time: f64,
}
#[derive(Deserialize)]
struct OracleWrapper {
    protected: bool,
    #[serde(rename = "protectedBySource")]
    protected_by_source: bool,
    score: f64,
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
            let row: OracleRow =
                serde_json::from_str(l).unwrap_or_else(|e| panic!("bad oracle line: {e}\n  {l}"));
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

        let s = salience(&m.salience_input(), NOW_MS);
        close(s.salience, o.salience.salience, id, "salience.salience");
        close(s.base, o.salience.base, id, "salience.base");
        close(s.decay, o.salience.decay, id, "salience.decay");
        close(
            s.days_since,
            o.salience.days_since,
            id,
            "salience.daysSince",
        );

        let pin = m.protection_input();
        let p = calculate_protection_score(&pin, &DEFAULT_PROTECTION_CONFIG, NOW_MS);
        close(p.score, o.protection.score, id, "protection.score");
        close(
            p.content_component,
            o.protection.content_component,
            id,
            "protection.contentComponent",
        );
        close(
            p.reinforcement_bonus,
            o.protection.reinforcement_bonus,
            id,
            "protection.reinforcementBonus",
        );
        close(
            p.graph_degree_bonus,
            o.protection.graph_degree_bonus,
            id,
            "protection.graphDegreeBonus",
        );
        close(
            p.recent_access_bonus,
            o.protection.recent_access_bonus,
            id,
            "protection.recentAccessBonus",
        );
        close(
            p.episodic_bonus,
            o.protection.episodic_bonus,
            id,
            "protection.episodicBonus",
        );
        close(
            p.days_since_ref_time,
            o.protection.days_since_ref_time,
            id,
            "protection.daysSinceRefTime",
        );

        let w = protection(&pin, NOW_MS, &DEFAULT_PROTECTION_CONFIG);
        assert_eq!(
            (w.protected, w.protected_by_source),
            (
                o.protection_wrapper.protected,
                o.protection_wrapper.protected_by_source
            ),
            "DIVERGENCE in case '{id}', field 'protectionWrapper.flags'"
        );
        close(
            w.score.score,
            o.protection_wrapper.score,
            id,
            "protectionWrapper.score",
        );

        // Episodic spine: the event-clock age label, exact-string.
        let si = m.salience_input();
        let age = format_memory_age(
            si.created_at_ms,
            si.last_reinforced_at_ms,
            m.occurred_at,
            NOW_MS,
        );
        assert_eq!(
            age, o.age,
            "DIVERGENCE in case '{id}', field 'age': rust={age:?} oracle={:?}",
            o.age
        );
    }

    eprintln!("OK: {} cases matched oracle field-for-field.", oracle.len());
}
