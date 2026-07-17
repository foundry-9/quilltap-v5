//! Tier-1 differential (P4.d5 unit 2): `llmNumber` — lenient numbers for
//! LLM-supplied tool arguments (`quilltap_core::tools::llm_number` — v4
//! `lib/tools/llm-number.ts`).
//!
//! Exact equivalence against v4's REAL `llmNumber`, driven two ways:
//!
//!  * `preprocess` — `llmNumber(z.any())`, so v4's emitted `output` IS the raw
//!    preprocess result. Both the VALUE and its JS `typeof` are diffed, so
//!    "handed the original string back" can never be mistaken for "converted to
//!    a number".
//!  * `bounded` — `llmNumber(z.number().int().min(2).max(1000))` (the rng `type`
//!    arm's real inner schema), proving bounds and `.int()` apply AFTER
//!    conversion.
//!
//! The two things this exists to catch:
//!   1. **`Number()` is not `str::parse::<f64>`.** `"0x10"` is 16 (Rust errors),
//!      `"5px"` is NaN (parseInt would say 5), and `"inf"`/`"infinity"`/`"nan"`
//!      are NaN even though Rust's parser accepts all three spellings.
//!   2. **Non-strings must fall through UNTOUCHED** — the deliberate divergence
//!      from `z.coerce.number()`, which would turn `true` into 1 and `null`/`[]`
//!      into 0, trading a rejected call for a wrong one (the worse failure).
//!
//! Generate the oracle (Node 24, from the v4 checkout @ `a33ac8b8`). The case
//! file imports `zod` as a BARE package, which resolves from the case file's own
//! directory — a worktree has no `node_modules` there, so mirror to /tmp WITH a
//! `node_modules` symlink ([[jest-oracle-bare-package-imports]]; the same trap
//! bites tsx, not just jest). Mirror from the WORKTREE
//! ([[oracle-regen-from-worktree-path]]) and verify by grepping for a new row:
//!   W=<this worktree>
//!   rm -rf /tmp/qt-oracle-llmnum && mkdir -p /tmp/qt-oracle-llmnum
//!   cp $W/harness/oracle/cases/llm-number.ts /tmp/qt-oracle-llmnum/
//!   ln -sfn ~/source/quilltap-server/node_modules /tmp/qt-oracle-llmnum/node_modules
//!   cd ~/source/quilltap-server
//!   npx tsx /tmp/qt-oracle-llmnum/llm-number.ts > /tmp/oracle-llm-number.ndjson
//! Run:
//!   QT_ORACLE_LLM_NUMBER=/tmp/oracle-llm-number.ndjson \
//!     cargo test -p quilltap-harness --test llm_number_equivalence

use quilltap_core::tools::llm_number::llm_number;
use serde_json::Value;

/// JS `typeof`, with `null`/array named apart (mirrors the oracle's `typeofOf`).
fn typeof_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// v4 `llmNumber(z.number().int().min(2).max(1000))` — the rng `type` arm.
/// The preprocess runs first; the inner schema then applies to whatever it
/// returned. Returns the parsed number on success.
fn bounded_parse(input: &Value) -> Option<f64> {
    let coerced = llm_number(input);
    let n = match &*coerced {
        Value::Number(n) => n.as_f64()?,
        // Anything the preprocess handed back unchanged that is not a number
        // fails `z.number()` on its own merits — including `true`, `null`, `[]`.
        _ => return None,
    };
    // `.int()` is Number.isInteger; then `.min(2).max(1000)`.
    if !n.is_finite() || n.fract() != 0.0 || !(2.0..=1000.0).contains(&n) {
        return None;
    }
    Some(n)
}

#[test]
fn llm_number_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_LLM_NUMBER") else {
        eprintln!("SKIP: set QT_ORACLE_LLM_NUMBER to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut pre, mut bnd) = (0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        let id = row["id"].as_str().expect("id");
        let input = &row["input"];

        match row["kind"].as_str().expect("kind") {
            "preprocess" => {
                assert!(
                    row["ok"].as_bool() == Some(true),
                    "case '{id}': z.any() must always accept the preprocess output"
                );
                let got = llm_number(input);
                assert_eq!(
                    &*got, &row["output"],
                    "llmNumber preprocess value mismatch for '{id}' (input {input})"
                );
                // The typeof guard: `6` and `"6"` are distinguishable in JSON, but
                // this makes a convert-vs-passthrough regression impossible to miss.
                assert_eq!(
                    typeof_of(&got),
                    row["outputType"].as_str().expect("outputType"),
                    "llmNumber preprocess TYPE mismatch for '{id}' (input {input})"
                );
                pre += 1;
            }
            "bounded" => {
                let got = bounded_parse(input);
                let want_ok = row["ok"].as_bool().expect("ok");
                assert_eq!(
                    got.is_some(),
                    want_ok,
                    "bounded llmNumber accept/reject mismatch for '{id}' (input {input})"
                );
                if want_ok {
                    let want = row["output"].as_f64().expect("output number");
                    assert_eq!(
                        got.unwrap(),
                        want,
                        "bounded llmNumber value mismatch for '{id}' (input {input})"
                    );
                }
                bnd += 1;
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    assert!(pre >= 45, "preprocess corpus too small: {pre}");
    assert!(bnd >= 15, "bounded corpus too small: {bnd}");
    eprintln!("OK: llm-number matched oracle (preprocess {pre}, bounded {bnd}).");
}
