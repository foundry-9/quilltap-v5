//! P4.D265 tier-1 differential (RE-AIMED by P4.D291): v4's `sizeMemoryPools`,
//! the pool constants and `PROACTIVE_RECALL_POOL_SIZE` (`3f7320138` /
//! `d58548051`), plus the assembler's own caps — all in v4 Phase E's
//! `lib/memory/recall/assemble.ts` — against `quilltap_core::memory_pools` and
//! `services::recall::assemble` — exact equality over every clamp edge
//! (budgets 0 … 1e9, a fractional and a negative one) under both `retro`
//! values. NaN is excluded: every v4 caller passes a finite budget.
//!
//! Regenerate + run (a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-memory-pools.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/memory-pools.ts > /tmp/oracle-memory-pools.ndjson
//!   cd $V5W
//!   QT_ORACLE_MEMORY_POOLS=/tmp/oracle-memory-pools.ndjson \
//!     cargo test -p quilltap-harness --test memory_pools_equivalence -- --nocapture

use quilltap_core::memory_pools::{
    size_memory_pools, DYNAMIC_HEAD_BUDGET_RATIO, DYNAMIC_HEAD_DEFAULT_SIZE, DYNAMIC_HEAD_MAX_SIZE,
    DYNAMIC_HEAD_MAX_TOKEN_BUDGET, DYNAMIC_HEAD_TOKEN_BUDGET, FROZEN_ARCHIVE_MAX_SIZE,
    FROZEN_ARCHIVE_MIN_SIZE, PROACTIVE_RECALL_POOL_SIZE, RETRO_HEAD_SIZE, RETRO_HEAD_TOKEN_BUDGET,
};
use quilltap_core::services::recall::assemble::{
    INTER_CHAR_DIGEST_LIMIT, INTER_CHAR_PER_CHARACTER_LIMIT,
    INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT, MEMORY_RECAP_PHASE_TIMEOUT_MS,
    RETRO_MINI_RECAP_MAX_ENTRIES,
};
use serde_json::{json, Value};

#[test]
fn memory_pools_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_MEMORY_POOLS") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_POOLS to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    let oracle: Value = serde_json::from_str(text.trim()).expect("oracle line");

    let constants = json!({
        "DYNAMIC_HEAD_TOKEN_BUDGET": DYNAMIC_HEAD_TOKEN_BUDGET,
        "DYNAMIC_HEAD_DEFAULT_SIZE": DYNAMIC_HEAD_DEFAULT_SIZE,
        "RETRO_HEAD_TOKEN_BUDGET": RETRO_HEAD_TOKEN_BUDGET,
        "RETRO_HEAD_SIZE": RETRO_HEAD_SIZE,
        "DYNAMIC_HEAD_BUDGET_RATIO": DYNAMIC_HEAD_BUDGET_RATIO,
        "DYNAMIC_HEAD_MAX_TOKEN_BUDGET": DYNAMIC_HEAD_MAX_TOKEN_BUDGET,
        "DYNAMIC_HEAD_MAX_SIZE": DYNAMIC_HEAD_MAX_SIZE,
        "FROZEN_ARCHIVE_MIN_SIZE": FROZEN_ARCHIVE_MIN_SIZE,
        "FROZEN_ARCHIVE_MAX_SIZE": FROZEN_ARCHIVE_MAX_SIZE,
        "PROACTIVE_RECALL_POOL_SIZE": PROACTIVE_RECALL_POOL_SIZE,
        "INTER_CHAR_PER_CHARACTER_LIMIT": INTER_CHAR_PER_CHARACTER_LIMIT,
        "INTER_CHAR_DIGEST_LIMIT": INTER_CHAR_DIGEST_LIMIT,
        "INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT": INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT,
        "RETRO_MINI_RECAP_MAX_ENTRIES": RETRO_MINI_RECAP_MAX_ENTRIES,
        "MEMORY_RECAP_PHASE_TIMEOUT_MS": MEMORY_RECAP_PHASE_TIMEOUT_MS,
    });
    assert_eq!(
        serde_json::to_string(&constants).unwrap(),
        serde_json::to_string(&oracle["constants"]).unwrap(),
        "the pool constants"
    );

    let cases = oracle["cases"].as_array().expect("cases");
    let mut failures = Vec::new();
    for c in cases {
        let budget = c["budget"].as_f64().unwrap();
        let retro = c["retro"].as_bool().unwrap();
        let s = size_memory_pools(budget, retro);
        let got = json!({
            "headTokenBudget": s.head_token_budget,
            "headEntries": s.head_entries,
            "archiveTokenBudget": s.archive_token_budget,
            "archiveSize": s.archive_size,
        });
        if serde_json::to_string(&got).unwrap() != serde_json::to_string(&c["sizing"]).unwrap() {
            failures.push(format!(
                "budget {budget} retro {retro}: v5 {got} v4 {}",
                c["sizing"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(cases.len(), 32, "16 budgets x both retro values");
    eprintln!("OK: memory pools matched oracle ({} cases).", cases.len());
}
