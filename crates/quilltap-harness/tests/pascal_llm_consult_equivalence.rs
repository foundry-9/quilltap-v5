//! Tier-1 differential (P4.d8 unit 3): the PURE leaves of the custom-tool LLM
//! consult — `consult_max_tokens` and the three caps — against v4's real
//! `lib/pascal/llm-consult.ts` at `616930db`.
//!
//! The rest of that module (fresh settings/profile resolution, the danger
//! reroute, the `'custom-tool-consult'` task type) is plumbing over the
//! cheap-LLM pipeline and is proven through the run_custom HANDLER
//! differential with a canned provider on both sides.
//!
//! Generate the oracle (v4 @ 616930db, Node 24
//! `~/.nvm/versions/node/v24.13.1/bin`):
//!   cd ~/source/quilltap-server
//!   npx tsx <V5W>/harness/oracle/cases/pascal-llm-consult.ts \
//!     > /tmp/oracle-pascal-llm-consult.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_LLM_CONSULT=/tmp/oracle-pascal-llm-consult.ndjson \
//!     cargo test -p quilltap-harness --test pascal_llm_consult_equivalence

use quilltap_core::pascal::custom_tool_types::{MAX_LLM_OUTPUT_CEILING, MAX_LLM_OUTPUT_LENGTH};
use quilltap_core::pascal::llm_consult::{consult_max_tokens, CONSULT_TIMEOUT_MS};
use serde_json::Value;

#[test]
fn pascal_llm_consult_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_LLM_CONSULT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_LLM_CONSULT to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut constants, mut budgets) = (0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap();
        match row["kind"].as_str().unwrap() {
            "constant" => {
                let want = row["out"].as_i64().unwrap();
                let got = match id {
                    "CONSULT_TIMEOUT_MS" => CONSULT_TIMEOUT_MS,
                    "MAX_LLM_OUTPUT_LENGTH" => MAX_LLM_OUTPUT_LENGTH as i64,
                    "MAX_LLM_OUTPUT_CEILING" => MAX_LLM_OUTPUT_CEILING,
                    other => panic!("unknown constant '{other}'"),
                };
                assert_eq!(got, want, "constant '{id}'");
                constants += 1;
            }
            "consultMaxTokens" => {
                let input = row["input"].as_f64().unwrap();
                let want = row["out"].as_f64().unwrap();
                assert_eq!(consult_max_tokens(input), want, "consultMaxTokens '{id}'");
                budgets += 1;
            }
            other => panic!("unknown row kind '{other}'"),
        }
    }

    assert!(
        constants == 3 && budgets > 0,
        "oracle file looks wrong: {constants} constants, {budgets} budgets"
    );
    eprintln!(
        "OK: pascal llm-consult leaves matched oracle ({constants} constants, {budgets} budgets)."
    );
}
