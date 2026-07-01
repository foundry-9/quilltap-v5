//! Tier-1 differential test #7: enclave budget math.
//!
//! Covers checkBudget (the pre-turn exhaustion verdict) and
//! computeBudgetProgress (the progress-toward-binding-cap fraction). Each oracle
//! row carries the chat-row caps + run-state and the expected output; the Rust
//! port is fed the same inputs, with ISO instants bridged to epoch-ms via the
//! harness's `iso_to_ms` (the same parse the fixed clock uses). Bools/enums are
//! compared exact; the progress fraction within 1e-12.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/enclave-budget.ts \
//!     > /tmp/oracle-enclave-budget.ndjson
//! Run:
//!   QT_ORACLE_ENCLAVE_BUDGET=/tmp/oracle-enclave-budget.ndjson cargo test -p quilltap-harness

use quilltap_core::enclave_budget::{
    check_budget, compute_autonomous_context_cap, compute_budget_progress, BudgetCheck, BudgetState,
};
use quilltap_harness::iso_to_ms;
use serde::Deserialize;

#[derive(Deserialize)]
struct CapsRaw {
    #[serde(rename = "budgetMaxTurns")]
    budget_max_turns: Option<i64>,
    #[serde(rename = "budgetMaxTokens")]
    budget_max_tokens: Option<i64>,
    #[serde(rename = "budgetMaxWallClockMs")]
    budget_max_wall_clock_ms: Option<i64>,
    #[serde(rename = "runStartedAt")]
    run_started_at: Option<String>,
    #[serde(rename = "runPausedAccumMs")]
    run_paused_accum_ms: Option<i64>,
    #[serde(rename = "runTurnsConsumed")]
    run_turns_consumed: Option<i64>,
    #[serde(rename = "runTokensConsumed")]
    run_tokens_consumed: Option<i64>,
}

impl CapsRaw {
    fn to_state(&self) -> BudgetState {
        BudgetState {
            budget_max_turns: self.budget_max_turns,
            budget_max_tokens: self.budget_max_tokens,
            budget_max_wall_clock_ms: self.budget_max_wall_clock_ms,
            // ISO→ms at the call boundary, mirroring v4's Date.parse(runStartedAt).
            run_started_at_ms: self.run_started_at.as_deref().map(|s| iso_to_ms(s) as i64),
            run_paused_accum_ms: self.run_paused_accum_ms,
            run_turns_consumed: self.run_turns_consumed,
            run_tokens_consumed: self.run_tokens_consumed,
        }
    }
}

#[derive(Deserialize)]
struct CheckOut {
    exhausted: bool,
    #[serde(rename = "nextState")]
    next_state: Option<String>,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct ProgressOut {
    fraction: f64,
    binding: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "check")]
    Check {
        id: String,
        caps: CapsRaw,
        now: String,
        #[serde(rename = "dailyTokenBudget")]
        daily_token_budget: Option<i64>,
        #[serde(rename = "dailyTokensSpent")]
        daily_tokens_spent: i64,
        out: CheckOut,
    },
    #[serde(rename = "progress")]
    Progress {
        id: String,
        caps: CapsRaw,
        #[serde(rename = "turnsConsumed")]
        turns_consumed: i64,
        #[serde(rename = "tokensConsumed")]
        tokens_consumed: i64,
        now: String,
        #[serde(rename = "dailyBudget")]
        daily_budget: Option<i64>,
        #[serde(rename = "dailySpent")]
        daily_spent: i64,
        out: Option<ProgressOut>,
    },
    #[serde(rename = "cap")]
    Cap {
        id: String,
        caps: CapsRaw,
        // undefined (no token budget) is emitted as null by the oracle.
        out: Option<i64>,
    },
}

#[test]
fn enclave_budget_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_ENCLAVE_BUDGET") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ENCLAVE_BUDGET to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 3];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Check {
                id,
                caps,
                now,
                daily_token_budget,
                daily_tokens_spent,
                out,
            } => {
                let got = check_budget(
                    &caps.to_state(),
                    iso_to_ms(&now) as i64,
                    daily_token_budget,
                    daily_tokens_spent,
                );
                match got {
                    BudgetCheck::Ok => {
                        assert!(!out.exhausted, "check '{id}': rust=Ok but oracle exhausted");
                    }
                    BudgetCheck::Exhausted { next_state, reason } => {
                        assert!(out.exhausted, "check '{id}': rust=Exhausted but oracle ok");
                        assert_eq!(
                            Some(next_state.as_str()),
                            out.next_state.as_deref(),
                            "check '{id}' nextState"
                        );
                        assert_eq!(
                            Some(reason.as_str()),
                            out.reason.as_deref(),
                            "check '{id}' reason"
                        );
                    }
                }
                counts[0] += 1;
            }
            OracleRow::Progress {
                id,
                caps,
                turns_consumed,
                tokens_consumed,
                now,
                daily_budget,
                daily_spent,
                out,
            } => {
                let got = compute_budget_progress(
                    &caps.to_state(),
                    turns_consumed,
                    tokens_consumed,
                    iso_to_ms(&now) as i64,
                    daily_budget,
                    daily_spent,
                );
                match (got, out) {
                    (None, None) => {}
                    (Some(g), Some(o)) => {
                        assert!(
                            (g.fraction - o.fraction).abs() < 1e-12,
                            "progress '{id}' fraction: rust={} oracle={}",
                            g.fraction,
                            o.fraction
                        );
                        assert_eq!(g.binding.as_str(), o.binding, "progress '{id}' binding");
                    }
                    (g, o) => panic!(
                        "progress '{id}': presence mismatch rust={:?} oracle={:?}",
                        g.is_some(),
                        o.is_some()
                    ),
                }
                counts[1] += 1;
            }
            OracleRow::Cap { id, caps, out } => {
                let got = compute_autonomous_context_cap(&caps.to_state());
                assert_eq!(got, out, "cap '{id}'");
                counts[2] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!(
        "OK: enclave-budget matched oracle ({} check, {} progress, {} cap).",
        counts[0], counts[1], counts[2]
    );
}
