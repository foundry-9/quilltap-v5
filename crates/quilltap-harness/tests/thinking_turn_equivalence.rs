//! P4.D97 unit 1 — the thinking-turn evaluator (v4 `97d2fcb5`,
//! `lib/llm/thinking-turn.ts`). Tier-1 exact: `evaluateThinkingTurn` over the
//! full rules × parameter-shapes × model-facts product.
//!
//! What the corpus is really pinning:
//!
//!   - `disabledValues` is checked BEFORE `enabledValues` (a value both lists
//!     claim answers `false`);
//!   - `isUnset` treats the EMPTY STRING like absent/null — "(model default)"
//!     falls through to `thinksByDefault` instead of answering `false`;
//!   - a set-but-unmatched value also falls through to the model habit;
//!   - value matching is JS `===` — no cross-type coercion, casing matters;
//!   - `supportsThinking` alone never turns the answer on.
//!
//! Generate the oracle output (from the pinned v4 worktree):
//!   cd ~/source/quilltap-server
//!   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/thinking-turn.ts \
//!     > /tmp/oracle-thinking-turn.ndjson
//! Run:
//!   QT_ORACLE_THINKING_TURN=/tmp/oracle-thinking-turn.ndjson \
//!     cargo test -p quilltap-harness --test thinking_turn_equivalence
use quilltap_core::services::thinking_turn::{
    evaluate_thinking_turn, ThinkingModelFacts, ThinkingTurnRule,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

/// The oracle's stand-in for a JS `undefined` input (NDJSON has no such
/// value). A JSON `null` is the other falsy shape and stays `null`.
const ABSENT: &str = "<absent>";

#[derive(Deserialize)]
struct Row {
    id: String,
    rule: Value,
    parameters: Value,
    model: Value,
    out: bool,
}

/// An oracle cell → `None` for both `<absent>` and JSON `null` (v4's
/// evaluator reads both through the same falsy/`?.` branch, so collapsing
/// them here loses nothing — the corpus carries both spellings to prove it).
fn present(cell: &Value) -> Option<&Value> {
    match cell {
        Value::Null => None,
        Value::String(s) if s == ABSENT => None,
        v => Some(v),
    }
}

#[test]
fn thinking_turn_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_THINKING_TURN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_THINKING_TURN to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut rows = 0usize;
    let (mut rules, mut params, mut models) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let mut parts = row.id.split('/');
        rules.insert(parts.next().unwrap_or("").to_string());
        params.insert(parts.next().unwrap_or("").to_string());
        models.insert(parts.next().unwrap_or("").to_string());

        let rule: Option<ThinkingTurnRule> = present(&row.rule)
            .map(|v| serde_json::from_value(v.clone()).expect("rule deserializes"));
        let model: Option<ThinkingModelFacts> = present(&row.model)
            .map(|v| serde_json::from_value(v.clone()).expect("model deserializes"));
        assert_eq!(
            evaluate_thinking_turn(rule.as_ref(), present(&row.parameters), model.as_ref()),
            row.out,
            "case '{}'",
            row.id
        );
        rows += 1;
    }

    // Shape assertions, not hand counts (`harness-corpus-shape-constants-rot`):
    // the corpus must be a clean product of the three axes, and each axis must
    // carry its load-bearing members.
    assert_eq!(
        rows,
        rules.len() * params.len() * models.len(),
        "the corpus must be a full product of the three axes"
    );
    for (axis, need) in [
        (&rules, vec!["deepseek", "ollama", "both-lists", "numeric"]),
        (&params, vec!["empty-string", "unmatched", "string-1"]),
        (&models, vec!["thinks", "supports-only"]),
    ] {
        for n in need {
            assert!(axis.contains(n), "axis member '{n}' missing from corpus");
        }
    }
    assert!(rows >= 700, "expected the full product grid, got {rows}");
    eprintln!(
        "thinking turn: {rows} cases matched ({} rules x {} params x {} models)",
        rules.len(),
        params.len(),
        models.len()
    );
}
