//! Tier-1 differential (P4.6ay unit 12): the Workbench outcome-table audit
//! (`pascal::custom_tools::simulate_outcomes`) vs v4's real `simulateOutcomes`.
//! DETERMINISTIC corpus (min===max ranges — no draw), so `runs`/`hits`/`share`/
//! min/max/mean match exactly (floats within 1e-9 for the accumulated mean); the
//! injected `FixedBytes` source is never consumed. Stochastic spread is covered
//! by the in-crate statistical tests in `pascal::custom_tools`.
//!
//! Generate the oracle (v4 @ d68638b4, Node 24):
//!   cd ~/source/quilltap-server
//!   npx tsx <V5W>/harness/oracle/cases/pascal-simulate.ts > /tmp/oracle-pascal-simulate.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_SIMULATE=/tmp/oracle-pascal-simulate.ndjson \
//!     cargo test -p quilltap-harness --test pascal_simulate_equivalence

use quilltap_core::pascal::custom_tool_types::safe_parse;
use quilltap_core::pascal::custom_tools::{simulate_outcomes, LlmSubject};
use quilltap_core::tools::rng::FixedBytes;
use serde_json::Value;

#[test]
fn simulate_outcomes_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_SIMULATE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_SIMULATE (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut rows = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap();
        let definition = safe_parse(&row["definition"])
            .unwrap_or_else(|e| panic!("case '{id}': definition parse: {e:?}"));
        let params = row["params"].as_object().cloned();
        let runs = row["runs"].as_u64().unwrap() as usize;
        let metadata = row["metadata"].as_object().cloned();
        // The fixed consult the whole audit shares; `simulate_outcomes` never
        // invokes, so this is a plain subject on both sides.
        let llm = row["llm"].as_object().map(|o| LlmSubject {
            ok: o["ok"].as_bool().unwrap(),
            output: o["output"].as_str().unwrap().to_string(),
        });

        // The mock merged state, held fixed across every draw (P4.d10).
        let state = match row.get("state") {
            Some(s @ Value::Object(_)) => Some(s.clone()),
            _ => None,
        };
        let mut rng = FixedBytes::new(vec![]);
        let got = simulate_outcomes(
            &definition,
            params.as_ref(),
            runs,
            metadata.as_ref(),
            llm.as_ref(),
            state.as_ref(),
            &mut rng,
        )
        .unwrap_or_else(|e| panic!("case '{id}': simulate failed: {e:?}"));
        assert_eq!(rng.consumed(), 0, "case '{id}' consumed bytes");

        let want = &row["result"];
        assert_eq!(
            got.runs as u64,
            want["runs"].as_u64().unwrap(),
            "case '{id}' runs"
        );

        let want_outcomes = want["outcomes"].as_array().unwrap();
        assert_eq!(
            got.outcomes.len(),
            want_outcomes.len(),
            "case '{id}' outcome count"
        );
        for (g, w) in got.outcomes.iter().zip(want_outcomes) {
            assert_eq!(
                g.index as u64,
                w["index"].as_u64().unwrap(),
                "case '{id}' index"
            );
            assert_eq!(
                g.hits as u64,
                w["hits"].as_u64().unwrap(),
                "case '{id}' hits"
            );
            approx(g.share, w["share"].as_f64().unwrap(), id, "share");
        }
        approx(
            got.value_min,
            want["valueMin"].as_f64().unwrap(),
            id,
            "valueMin",
        );
        approx(
            got.value_max,
            want["valueMax"].as_f64().unwrap(),
            id,
            "valueMax",
        );
        approx(
            got.value_mean,
            want["valueMean"].as_f64().unwrap(),
            id,
            "valueMean",
        );
        rows += 1;
    }

    assert!(rows > 0, "oracle empty");
    eprintln!("OK: simulate_outcomes matched oracle ({rows} rows).");
}

fn approx(got: f64, want: f64, id: &str, field: &str) {
    assert!(
        (got - want).abs() < 1e-9,
        "case '{id}' {field}: got {got}, want {want}"
    );
}
