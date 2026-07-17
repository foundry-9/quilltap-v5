//! Tier-1 differential (P4.6ay unit 3): the Pascal custom-tool EXECUTION core —
//! `format_value`, `render_template`, `resolve_params`, `matches_when`, and
//! `execute_custom_tool`, against v4's real `lib/pascal/custom-tools.ts`.
//!
//! The byte source is INJECTED on both sides: the oracle mocks
//! `crypto.randomBytes` over a scripted pool and reports the cursor, and this
//! side replays the same pool through `FixedBytes` and asserts
//! `FixedBytes::consumed()` equals it. That is what pins the two byte-stream
//! facts inspection cannot: a degenerate `min === max` range short-circuits
//! WITHOUT drawing, and the dice path's rejection sampling draws exactly what v4
//! draws.
//!
//! Definitions travel as the ORACLE'S OWN parsed data and are re-parsed here
//! through `safe_parse`, so every case also re-exercises unit 1 and no case can
//! describe a definition that would not load.
//!
//! Generate the oracle output (v4 @ e3593f75, Node 24; jest ignores `.claude/`
//! paths, hence the /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-pascal-exec-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/pascal-custom-tools-execution.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-execution.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- pascal-custom-tools-execution
//! Run:
//!   QT_ORACLE_PASCAL_EXECUTION=/tmp/oracle-pascal-execution.ndjson \
//!     cargo test -p quilltap-harness --test pascal_custom_tools_execution_equivalence

use quilltap_core::pascal::custom_tool_types::safe_parse;
use quilltap_core::pascal::custom_tools::{
    execute_custom_tool, format_value, matches_when, render_template, CustomToolRunResult,
    OutcomeSubjects, ResolvedParams, ResolvedValue, RollForm, TemplateVars,
};
use quilltap_core::pascal::dice::FixedBytes;
use serde_json::{json, Map, Value};

/// v4's `ResolvedParams` is a plain record; render it as one, with JS numbers.
fn params_to_json(params: &ResolvedParams) -> Value {
    let mut m = Map::new();
    for (k, v) in params {
        m.insert(k.clone(), resolved_to_json(v));
    }
    Value::Object(m)
}

fn resolved_to_json(v: &ResolvedValue) -> Value {
    match v {
        // JS has one number type: an integer-valued double renders bare.
        ResolvedValue::Number(n) => js_num(*n),
        ResolvedValue::String(s) => Value::String(s.clone()),
        ResolvedValue::Bool(b) => Value::Bool(*b),
    }
}

fn js_num(n: f64) -> Value {
    if n.is_finite() && n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

/// v4's `CustomToolRunResult` object literal, in ITS field order —
/// `JSON.stringify` drops the `undefined` optionals.
fn result_to_json(r: &CustomToolRunResult) -> Value {
    let mut m = Map::new();
    m.insert("tool".into(), Value::String(r.tool.clone()));
    m.insert("params".into(), params_to_json(&r.params));
    m.insert(
        "rollForm".into(),
        Value::String(match r.roll_form {
            RollForm::Range => "range".into(),
            RollForm::Dice => "dice".into(),
        }),
    );
    if let Some(n) = &r.notation {
        m.insert("notation".into(), Value::String(n.clone()));
    }
    m.insert("raw".into(), js_num(r.raw));
    if let Some(d) = &r.dice_rolls {
        m.insert("diceRolls".into(), json!(d));
    }
    m.insert("value".into(), js_num(r.value));
    m.insert("state".into(), serde_json::to_value(r.state).unwrap());
    m.insert("outcomeIndex".into(), json!(r.outcome_index));
    m.insert("message".into(), Value::String(r.message.clone()));
    m.insert(
        "diceBreakdown".into(),
        Value::String(r.dice_breakdown.clone()),
    );
    m.insert(
        "visibility".into(),
        serde_json::to_value(r.visibility).unwrap(),
    );
    Value::Object(m)
}

fn supplied_of(row: &Value) -> Option<Map<String, Value>> {
    row.get("supplied").and_then(|v| v.as_object()).cloned()
}

fn tool_of(row: &Value, id: &str) -> quilltap_core::pascal::custom_tool_types::QtapCustomTool {
    let raw = row
        .get("tool")
        .unwrap_or_else(|| panic!("case '{id}': row carries no tool"));
    safe_parse(raw).unwrap_or_else(|issues| {
        let text = quilltap_core::pascal::custom_tool_types::format_definition_issues(&issues);
        panic!("case '{id}': v5 rejected the oracle's own parsed definition: {text}")
    })
}

#[test]
fn pascal_custom_tools_execution_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_EXECUTION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_EXECUTION to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = std::collections::BTreeMap::<String, usize>::new();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap().to_string();
        let id = row["id"].as_str().unwrap().to_string();

        match kind.as_str() {
            "formatValue" => {
                let input = row["input"].as_f64().unwrap();
                let want = row["out"].as_str().unwrap();
                assert_eq!(format_value(input), want, "formatValue '{id}'");
            }

            "renderTemplate" => {
                let tool = tool_of(&row, &id);
                let params = quilltap_core::pascal::custom_tools::resolve_params(&tool, None)
                    .unwrap_or_else(|e| panic!("case '{id}': params: {e}"));
                let message = row["message"].as_str().unwrap();
                let got = render_template(
                    message,
                    &TemplateVars {
                        value: 12.3456,
                        roll: 0.6789,
                        dice: "3d6: [4, 2, 6] = 12",
                        params: &params,
                    },
                );
                assert_eq!(got, row["out"].as_str().unwrap(), "renderTemplate '{id}'");
            }

            "resolveParams" => {
                let tool = tool_of(&row, &id);
                let supplied = supplied_of(&row);
                match quilltap_core::pascal::custom_tools::resolve_params(&tool, supplied.as_ref())
                {
                    Ok(params) => {
                        assert!(
                            row["error"].is_null(),
                            "case '{id}': v5 resolved, v4 threw: {}",
                            row["error"]
                        );
                        assert_eq!(params_to_json(&params), row["out"], "resolveParams '{id}'");
                    }
                    Err(e) => {
                        assert_eq!(
                            Value::String(e.0.clone()),
                            row["error"],
                            "resolveParams '{id}': error differs"
                        );
                    }
                }
            }

            "matchesWhen" => {
                let tool = tool_of(&row, &id);
                let params = quilltap_core::pascal::custom_tools::resolve_params(&tool, None)
                    .unwrap_or_else(|e| panic!("case '{id}': params: {e}"));
                let when = &tool.outcomes[0].when;
                let subjects = OutcomeSubjects {
                    value: 12.0,
                    roll: 0.6,
                    params: &params,
                };
                match matches_when(when, &subjects, "probe") {
                    Ok(held) => {
                        assert!(row["error"].is_null(), "case '{id}': v5 matched, v4 threw");
                        assert_eq!(Value::Bool(held), row["out"], "matchesWhen '{id}'");
                    }
                    Err(e) => {
                        assert_eq!(
                            Value::String(e.0.clone()),
                            row["error"],
                            "matchesWhen '{id}'"
                        );
                    }
                }
            }

            "executeCustomTool" => {
                let tool = tool_of(&row, &id);
                let supplied = supplied_of(&row);
                let private = row["overrides"].get("private").and_then(Value::as_bool);

                let pool = hex_bytes(row["poolHex"].as_str().unwrap());
                let mut rng = FixedBytes::new(pool);

                let outcome = execute_custom_tool(&tool, supplied.as_ref(), private, &mut rng);

                // The byte cursor is the assertion inspection cannot make.
                assert_eq!(
                    rng.consumed(),
                    row["bytesConsumed"].as_u64().unwrap() as usize,
                    "executeCustomTool '{id}': bytes drawn differ"
                );

                match outcome {
                    Ok(r) => {
                        assert!(
                            row["error"].is_null(),
                            "case '{id}': v5 ran, v4 threw: {}",
                            row["error"]
                        );
                        assert_eq!(result_to_json(&r), row["out"], "executeCustomTool '{id}'");
                    }
                    Err(e) => {
                        assert_eq!(
                            Value::String(e.0.clone()),
                            row["error"],
                            "executeCustomTool '{id}': error differs"
                        );
                    }
                }
            }

            other => panic!("unknown row kind '{other}'"),
        }

        *counts.entry(kind).or_default() += 1;
    }

    assert!(!counts.is_empty(), "oracle file looks empty");
    for kind in [
        "formatValue",
        "renderTemplate",
        "resolveParams",
        "matchesWhen",
        "executeCustomTool",
    ] {
        assert!(counts.contains_key(kind), "oracle carried no '{kind}' rows");
    }
    eprintln!("OK: pascal execution core matched oracle ({counts:?}).");
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("oracle pool is hex"))
        .collect()
}
