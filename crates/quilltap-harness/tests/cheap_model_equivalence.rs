//! Tier-1 differential test #29 (Wave 7 / B21): the cheap-model selector —
//! getCheapestModel, exact equality against the v4 oracle. The oracle runs with
//! no registry cheap-config, so the fallback table is what's verified (default
//! None on the Rust side).
//!
//! P4.D157: v4 `d4138b96b` deleted `isCheapModel` / `estimateModelCost`; neither
//! twin had a production caller in v5 either, so both were deleted here and the
//! `classify` rows left the case (203 -> 7 rows; every surviving `cheapest` row
//! byte-identical).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/cheap-model.ts \
//!     > /tmp/oracle-cheap-model.ndjson
//! Run:
//!   QT_ORACLE_CHEAP_MODEL=/tmp/oracle-cheap-model.ndjson \
//!     cargo test -p quilltap-harness --test cheap_model_equivalence

use quilltap_core::cheap_model::get_cheapest_model;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
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

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
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
