//! Tier-1 differential test #8: LLM cost arithmetic (estimateCost).
//!
//! Each oracle row carries the per-1M rates + token counts and the expected USD
//! cost; the Rust port is fed the same inputs and compared within 1e-12. The
//! arithmetic is bit-identical to v4 (same divide-then-multiply-then-add order),
//! so the tolerance is slack — the `single-token-each` row
//! (0.000017999999999999997) is the float-fidelity canary.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/pricing.ts \
//!     > /tmp/oracle-pricing.ndjson
//! Run:
//!   QT_ORACLE_PRICING=/tmp/oracle-pricing.ndjson cargo test -p quilltap-harness

use quilltap_core::pricing::{estimate_cost, ModelCost};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "estimate")]
    Estimate {
        id: String,
        #[serde(rename = "promptCostPer1M")]
        prompt_cost_per_1m: f64,
        #[serde(rename = "completionCostPer1M")]
        completion_cost_per_1m: f64,
        #[serde(rename = "promptTokens")]
        prompt_tokens: i64,
        #[serde(rename = "completionTokens")]
        completion_tokens: i64,
        out: f64,
    },
}

#[test]
fn pricing_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PRICING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PRICING to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Estimate {
                id,
                prompt_cost_per_1m,
                completion_cost_per_1m,
                prompt_tokens,
                completion_tokens,
                out,
            } => {
                let pricing = ModelCost {
                    prompt_cost_per_1m,
                    completion_cost_per_1m,
                };
                let got = estimate_cost(&pricing, prompt_tokens, completion_tokens);
                assert!(
                    (got - out).abs() < 1e-12,
                    "estimate '{id}': rust={got} oracle={out}"
                );
                count += 1;
            }
        }
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: pricing matched oracle ({count} estimate).");
}
