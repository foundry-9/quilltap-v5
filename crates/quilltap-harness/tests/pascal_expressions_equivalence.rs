//! Tier-1 differential (P4.D35, v4 `c4d4b0de`): effect EXPRESSIONS — the closed
//! tokenizer/parser/evaluator behind an effect's `value`, byte-exact against
//! v4's real `parseExpression`/`evaluateExpression`/`formatValue`.
//!
//! Reasons are compared WHOLE, not by fragment. A parse reason is user-visible
//! payload (it is spliced into the definition rejection the
//! `/api/v1/chats/{id}/custom-tools` GET route returns in `errors[]`), and an
//! eval reason is recorded verbatim as an effect's skip reason. v4's own unit
//! test only `toContain`s a fragment of each; this is the stricter bar the
//! port needs.
//!
//! The corpus carries non-ASCII and astral rows on purpose: the tokenizer
//! indexes a JS string, so `at position N` counts UTF-16 code units, and a port
//! that walked bytes or `char`s would pass every ASCII row and fail exactly
//! those.
//!
//! Generate the oracle output (v4 @ c4d4b0de, Node 24
//! `~/.nvm/versions/node/v24.13.1/bin`):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx \
//!     <V5W>/harness/oracle/cases/pascal-expressions.ts \
//!     > /tmp/oracle-pascal-expressions.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_EXPRESSIONS=/tmp/oracle-pascal-expressions.ndjson \
//!     cargo test -p quilltap-harness --test pascal_expressions_equivalence

use quilltap_core::pascal::expressions::{
    evaluate_expression, format_value, parse_expression, ExprValue,
};
use quilltap_core::pascal::js_value::json_stringify;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Row {
    Parse {
        id: String,
        source: String,
        ok: bool,
        #[serde(default)]
        refs: Option<Vec<String>>,
        #[serde(default)]
        reason: Option<String>,
    },
    Eval {
        id: String,
        source: String,
        ok: bool,
        #[serde(rename = "valueJson", default)]
        value_json: Option<String>,
        #[serde(default)]
        reason: Option<String>,
    },
    Format {
        id: String,
        input: f64,
        output: String,
    },
}

/// The fixture ref table the oracle's evaluator reads. Kept in step with the
/// `REFS` literal in the oracle case; an absent key resolves to `None`, which
/// is the fail-soft path the corpus exercises.
fn fixture_refs(name: &str) -> Option<ExprValue> {
    let v = match name {
        "value" => ExprValue::Number(10.0),
        "roll" => ExprValue::Number(4.0),
        "dice" => ExprValue::String("3d6: [1, 2, 3] = 6".to_string()),
        "llm" => ExprValue::String("21".to_string()),
        "params.bonus" => ExprValue::Number(2.0),
        "params.label" => ExprValue::String("lockpick".to_string()),
        "params.flag" => ExprValue::Bool(true),
        "metadata.lockpicks" => ExprValue::Number(3.0),
        "state.floor" => ExprValue::Number(1.0),
        "state.debt" => ExprValue::Number(50.0),
        "state.name" => ExprValue::String("Aurum".to_string()),
        _ => return None,
    };
    Some(v)
}

#[test]
fn pascal_expressions_match_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_EXPRESSIONS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_EXPRESSIONS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    // Coverage, not a hand count: a corpus that quietly lost its interesting
    // rows would otherwise still pass. Each bucket must be seen both ways.
    let (mut parse_ok, mut parse_rejected) = (0usize, 0usize);
    let (mut eval_ok, mut eval_failed) = (0usize, 0usize);
    let mut formats = 0usize;
    let mut seen_position_reason = false;
    let mut seen_string_value = false;
    let mut seen_bool_value = false;
    // A source that names the SAME ref twice is the only row shape that can
    // catch a port which forgot to deduplicate `refs`; without one the whole
    // corpus passes with the dedup removed (proven by mutation).
    let mut seen_repeated_ref_source = false;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("bad row: {e}\n{line}"));
        match row {
            Row::Parse {
                id,
                source,
                ok,
                refs,
                reason,
            } => {
                let got = parse_expression(&source);
                assert_eq!(
                    got.is_ok(),
                    ok,
                    "parse '{id}': acceptance differs (v5 said {:?})",
                    got.as_ref().err()
                );
                match got {
                    Ok(expr) => {
                        parse_ok += 1;
                        // Count the `{{` openers: more of them than distinct
                        // refs means this row exercises the dedup.
                        if source.matches("{{").count() > expr.refs.len() {
                            seen_repeated_ref_source = true;
                        }
                        assert_eq!(
                            expr.refs,
                            refs.expect("an accepted parse row carries refs"),
                            "parse '{id}': refs differ (first-appearance order, deduplicated)"
                        );
                    }
                    Err(got_reason) => {
                        parse_rejected += 1;
                        let want = reason.expect("a rejected parse row carries a reason");
                        if want.contains("at position") {
                            seen_position_reason = true;
                        }
                        assert_eq!(got_reason, want, "parse '{id}': reason differs");
                    }
                }
            }

            Row::Eval {
                id,
                source,
                ok,
                value_json,
                reason,
            } => {
                let expr = parse_expression(&source)
                    .unwrap_or_else(|e| panic!("eval '{id}': fixture failed to parse: {e}"));
                let got = evaluate_expression(&expr, &mut fixture_refs);
                assert_eq!(
                    got.is_ok(),
                    ok,
                    "eval '{id}': success differs (v5 said {:?})",
                    got.as_ref().err()
                );
                match got {
                    Ok(value) => {
                        eval_ok += 1;
                        match &value {
                            ExprValue::String(_) => seen_string_value = true,
                            ExprValue::Bool(_) => seen_bool_value = true,
                            ExprValue::Number(_) => {}
                        }
                        // `JSON.stringify(value)` on both sides — the
                        // number/string/boolean distinction and JS number
                        // rendering survive the trip rather than being retyped.
                        assert_eq!(
                            json_stringify(&value.to_value()),
                            value_json.expect("a successful eval row carries valueJson"),
                            "eval '{id}': value differs"
                        );
                    }
                    Err(got_reason) => {
                        eval_failed += 1;
                        assert_eq!(
                            got_reason,
                            reason.expect("a failed eval row carries a reason"),
                            "eval '{id}': reason differs"
                        );
                    }
                }
            }

            Row::Format { id, input, output } => {
                formats += 1;
                assert_eq!(format_value(input), output, "formatValue '{id}'");
            }
        }
    }

    assert!(
        parse_ok >= 20 && parse_rejected >= 20,
        "parse coverage thinned: {parse_ok} accepted / {parse_rejected} rejected"
    );
    assert!(
        eval_ok >= 15 && eval_failed >= 10,
        "eval coverage thinned: {eval_ok} ok / {eval_failed} failed"
    );
    assert!(formats >= 5, "formatValue coverage thinned: {formats}");
    assert!(
        seen_position_reason,
        "no position-bearing tokenizer reason in the corpus — the UTF-16 arm is untested"
    );
    assert!(
        seen_repeated_ref_source,
        "no accepted row names a ref twice — the `refs` deduplication is untested"
    );
    assert!(
        seen_string_value && seen_bool_value,
        "the corpus produced no string and/or boolean value — concatenation and literals untested"
    );
}
