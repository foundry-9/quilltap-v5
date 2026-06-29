//! Tier-1 differential test #29 (Wave 7 / B21): cheap-model classifiers —
//! isCheapModel / estimateModelCost / getCheapestModel, exact equality against
//! the v4 oracle. The oracle runs with no registry cheap-config, so the fallback
//! tables + pure heuristics are what's verified (registry list empty, default
//! None on the Rust side).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/cheap-model.ts \
//!     > /tmp/oracle-cheap-model.ndjson
//! Run:
//!   QT_ORACLE_CHEAP_MODEL=/tmp/oracle-cheap-model.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::cheap_model::{estimate_model_cost, get_cheapest_model, is_cheap_model};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "classify")]
    Classify {
        provider: String,
        model: String,
        cheap: bool,
        cost: i64,
    },
    #[serde(rename = "cheapest")]
    Cheapest { provider: String, out: String },
}

#[test]
fn cheap_model_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CHEAP_MODEL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHEAP_MODEL to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let no_registry: &[String] = &[];
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Classify {
                provider,
                model,
                cheap,
                cost,
            } => {
                assert_eq!(
                    is_cheap_model(&provider, &model, no_registry),
                    cheap,
                    "isCheapModel({provider}, {model})"
                );
                assert_eq!(
                    estimate_model_cost(&provider, &model, no_registry),
                    cost,
                    "estimateModelCost({provider}, {model})"
                );
            }
            Row::Cheapest { provider, out } => {
                assert_eq!(
                    get_cheapest_model(&provider, None),
                    Some(out.clone()),
                    "getCheapestModel({provider})"
                );
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: cheap-model matched oracle ({count} rows).");
}
