//! Tier-1 differential (P4.D345): operator intervention's PURE half —
//! `pascal::intervention::{eligibility, overrides, recompute, spans,
//! veto_result}` and the engine's replay (`execute_custom_tool` with a recorded
//! draw / consult / pins) — against v4's REAL `lib/pascal/intervention/*` at
//! the `236c04e35` pin.
//!
//! Every comparison is BYTES (`serde_json::to_string`), never `Value ==`,
//! which ignores key order (memory note `serde-json-map-eq-ignores-order`):
//! the record's `dealt` facts, the views and the pins are wire.
//!
//! The engine is handed an EMPTY `FixedBytes` on every replay row — a held run
//! replays its draw and never rerolls, so any draw is a loud panic, and
//! `consumed() == 0` is asserted besides.
//!
//! Generate the oracle (v4 at the `236c04e35` pin, Node 24):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TZ=UTC npx tsx "$V5W/harness/oracle/cases/pascal-intervention.ts" \
//!     > /tmp/oracle-pascal-intervention.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_INTERVENTION=/tmp/oracle-pascal-intervention.ndjson \
//!     cargo test -p quilltap-harness --test pascal_intervention_pure_equivalence

use std::collections::BTreeMap;

use quilltap_core::api::custom_tools::run_result_to_value;
use quilltap_core::pascal::custom_tool_types::{safe_parse, QtapCustomTool, Visibility};
use quilltap_core::pascal::custom_tools::{
    execute_custom_tool, CustomToolRunResult, LlmConsultResult, RecordedDraw, ReplayInputs,
    RollForm, RunSpan,
};
use quilltap_core::pascal::dice::FixedBytes;
use quilltap_core::pascal::intervention::eligibility::{intervention_eligibility, is_intervenable};
use quilltap_core::pascal::intervention::overrides::{
    check_overrides, join_messages, parse_intervention_decision, parse_intervention_overrides,
    parse_preview_body, parse_resolve_body, DealtShape, InterventionDecisionInput,
    InterventionOverrides,
};
use quilltap_core::pascal::intervention::recompute::{
    changed_fields, facts_of, pins_from_overrides, pins_to_wire, presentation_for, recompute_run,
    summarize_effects, RecomputeInput, RunSnapshot,
};
use quilltap_core::pascal::intervention::spans::{describe_span, within_span};
use quilltap_core::pascal::intervention::veto_result::veto_result_for;
use serde_json::{json, Map, Value};

fn bytes(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

fn roll_form(s: &str) -> RollForm {
    match s {
        "range" => RollForm::Range,
        "dice" => RollForm::Dice,
        other => panic!("roll form {other}"),
    }
}

fn draw_of(v: &Value) -> RecordedDraw {
    match v["form"].as_str().unwrap() {
        "range" => RecordedDraw::Range {
            unit: v["unit"].as_f64().unwrap(),
        },
        "dice" => RecordedDraw::Dice {
            faces: v["faces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_i64().unwrap())
                .collect(),
        },
        other => panic!("draw form {other}"),
    }
}

fn consult_of(v: &Value) -> Option<LlmConsultResult> {
    let o = v.as_object()?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
    Some(LlmConsultResult {
        ok: o["ok"].as_bool().unwrap(),
        output: s("output").unwrap(),
        prompt: s("prompt").unwrap(),
        reason: s("reason"),
        provider: s("provider"),
        model: s("model"),
    })
}

fn snapshot_of(v: &Value) -> RunSnapshot {
    let obj = |k: &str| v[k].as_object().cloned().unwrap_or_default();
    RunSnapshot {
        params: v["params"].as_object().cloned(),
        private: v["private"].as_bool(),
        metadata: obj("metadata"),
        progress: obj("progress"),
        now_ms: v["now"].as_i64().unwrap(),
        character: v["character"].as_str().map(str::to_string),
        state: v["state"].clone(),
        inventory: None,
    }
}

fn overrides_of(v: &Value, id: &str) -> InterventionOverrides {
    parse_intervention_overrides(Some(v)).unwrap_or_else(|issues| {
        panic!(
            "case '{id}': v5 refused overrides v4 accepted: {}",
            join_messages(&issues)
        )
    })
}

fn decision_wire(d: &InterventionDecisionInput) -> Value {
    match d {
        InterventionDecisionInput::Deal { overrides } => {
            json!({ "kind": "deal", "overrides": overrides.to_wire() })
        }
        other => json!({ "kind": other.kind() }),
    }
}

fn result_or_error(r: &Result<CustomToolRunResult, String>) -> (Value, Value) {
    match r {
        Ok(run) => (run_result_to_value(run), Value::Null),
        Err(e) => (Value::Null, Value::String(e.clone())),
    }
}

#[test]
fn intervention_pure_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_INTERVENTION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_INTERVENTION (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    let mut defs: BTreeMap<String, QtapCustomTool> = BTreeMap::new();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut failures: Vec<String> = Vec::new();
    let mut check = |id: &str, what: &str, got: String, want: String| {
        if got != want {
            failures.push(format!("{id} {what}\n  v5: {got}\n  v4: {want}"));
        }
    };

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap().to_string();
        *counts.entry(kind.clone()).or_default() += 1;
        let id = row["id"].as_str().unwrap_or("").to_string();
        match kind.as_str() {
            "definition" => {
                let key = row["key"].as_str().unwrap().to_string();
                let tool = safe_parse(&row["doc"]).unwrap_or_else(|issues| {
                    panic!(
                        "definition '{key}' refused: {}",
                        quilltap_core::pascal::custom_tool_types::format_definition_issues(&issues)
                    )
                });
                defs.insert(key, tool);
            }
            "eligibility" => {
                let d = &defs[row["def"].as_str().unwrap()];
                let got = intervention_eligibility(d, roll_form(row["rollForm"].as_str().unwrap()));
                check(
                    &id,
                    "eligibility",
                    bytes(&got.to_wire()),
                    bytes(&row["out"]),
                );
                check(
                    &id,
                    "intervenable",
                    is_intervenable(d).to_string(),
                    row["intervenable"].to_string(),
                );
            }
            "parse" => {
                let input = &row["input"];
                let (data, message): (Value, Value) = match row["schema"].as_str().unwrap() {
                    "overrides" => match parse_intervention_overrides(Some(input)) {
                        Ok(o) => (o.to_wire(), Value::Null),
                        Err(i) => (Value::Null, Value::String(join_messages(&i))),
                    },
                    "decision" => match parse_intervention_decision(Some(input)) {
                        Ok(d) => (decision_wire(&d), Value::Null),
                        Err(i) => (Value::Null, Value::String(join_messages(&i))),
                    },
                    "preview" => match parse_preview_body(input) {
                        Ok((pid, o)) => {
                            (json!({ "id": pid, "overrides": o.to_wire() }), Value::Null)
                        }
                        Err(i) => (Value::Null, Value::String(join_messages(&i))),
                    },
                    "resolve" => match parse_resolve_body(input) {
                        Ok((pid, d)) => (
                            json!({ "id": pid, "decision": decision_wire(&d) }),
                            Value::Null,
                        ),
                        Err(i) => (Value::Null, Value::String(join_messages(&i))),
                    },
                    other => panic!("schema {other}"),
                };
                check(&id, "message", bytes(&message), bytes(&row["message"]));
                check(&id, "data", bytes(&data), bytes(&row["data"]));
                if row["schema"] == "overrides" {
                    if let Ok(o) = parse_intervention_overrides(Some(input)) {
                        check(&id, "keys", bytes(&json!(o.keys())), bytes(&row["keys"]));
                    }
                }
            }
            "check" => {
                let d = &defs[row["def"].as_str().unwrap()];
                let dealt = DealtShape {
                    roll_form: roll_form(row["dealt"]["rollForm"].as_str().unwrap()),
                    dice_count: row["dealt"]["diceCount"].as_u64().map(|n| n as usize),
                };
                let overrides = overrides_of(&row["overrides"], &id);
                let got = check_overrides(d, dealt, &overrides)
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                check(&id, "checkOverrides", bytes(&got), bytes(&row["out"]));
            }
            "pins" => {
                let overrides = overrides_of(&row["overrides"], &id);
                let got = pins_to_wire(&pins_from_overrides(&overrides));
                check(&id, "pins", bytes(&got), bytes(&row["out"]));
            }
            "presentation" => {
                let d = &defs[row["def"].as_str().unwrap()];
                let overrides = overrides_of(&row["overrides"], &id);
                let visibility = match row["visibility"].as_str().unwrap() {
                    "whisper" => Visibility::Whisper,
                    _ => Visibility::Public,
                };
                let got = presentation_for(
                    d,
                    visibility,
                    overrides.title.as_deref(),
                    overrides.announcer,
                );
                check(
                    &id,
                    "presentation",
                    bytes(&got.to_wire()),
                    bytes(&row["out"]),
                );
            }
            "recompute" => {
                let d = &defs[row["def"].as_str().unwrap()];
                let snapshot = snapshot_of(&row["snapshot"]);
                let draw = draw_of(&row["draw"]);
                let consult = consult_of(&row["consult"]);
                let overrides = overrides_of(&row["overrides"], &id);

                // The baseline: the run as first dealt — the draw and the
                // consult replayed, no pins.
                let mut rng = FixedBytes::new(vec![]);
                let replay = ReplayInputs {
                    draw: Some(&draw),
                    consult: consult.as_ref(),
                    pins: None,
                };
                let baseline = rt
                    .block_on(execute_custom_tool(
                        d,
                        snapshot.params.as_ref(),
                        snapshot.private,
                        Some(&snapshot.metadata),
                        Some(&snapshot.state),
                        &mut rng,
                        None,
                        Some(&snapshot.progress),
                        snapshot.now_ms,
                        snapshot.character.as_deref(),
                        None,
                        Some(&replay),
                    ))
                    .map_err(|e| e.0);
                assert_eq!(
                    rng.consumed(),
                    0,
                    "case '{id}': the baseline replay drew bytes"
                );
                let (b_ok, b_err) = result_or_error(&baseline);
                check(&id, "baseline", bytes(&b_ok), bytes(&row["baseline"]));
                check(
                    &id,
                    "baselineError",
                    bytes(&b_err),
                    bytes(&row["baselineError"]),
                );

                let mut rng = FixedBytes::new(vec![]);
                let recomputed = rt.block_on(recompute_run(
                    RecomputeInput {
                        definition: d,
                        snapshot: &snapshot,
                        draw: &draw,
                        consult: consult.as_ref(),
                        overrides: &overrides,
                        dealt_outcome_index: baseline.as_ref().ok().map(|b| b.outcome_index),
                    },
                    &mut rng,
                ));
                assert_eq!(rng.consumed(), 0, "case '{id}': the recompute drew bytes");
                match &recomputed {
                    Ok(run) => {
                        let mut m = Map::new();
                        m.insert("result".into(), run_result_to_value(&run.result));
                        m.insert("presentation".into(), run.presentation.to_wire());
                        m.insert("staleAnswer".into(), Value::Bool(run.stale_answer));
                        if let Some(p) = &run.prompt_now {
                            m.insert("promptNow".into(), Value::String(p.clone()));
                        }
                        m.insert("clampedParams".into(), json!(run.clamped_params));
                        m.insert("warnings".into(), json!(run.warnings));
                        check(
                            &id,
                            "recompute",
                            bytes(&Value::Object(m)),
                            bytes(&row["out"]),
                        );
                        check(&id, "error", "null".into(), bytes(&row["error"]));
                    }
                    Err(e) => {
                        check(&id, "recompute", "null".into(), bytes(&row["out"]));
                        check(
                            &id,
                            "error",
                            bytes(&Value::String(e.0.clone())),
                            bytes(&row["error"]),
                        );
                    }
                }

                if let (Ok(base), Ok(run)) = (&baseline, &recomputed) {
                    let dealt =
                        facts_of(d, base, &presentation_for(d, base.visibility, None, None));
                    let final_ = facts_of(d, &run.result, &run.presentation);
                    let summary: Vec<Value> = summarize_effects(d, run.result.effects.as_deref())
                        .iter()
                        .map(|s| s.to_wire())
                        .collect();
                    let got = json!({
                        "before": dealt.to_wire(),
                        "after": final_.to_wire(),
                        "changed": changed_fields(&dealt, &final_),
                        "self": changed_fields(&final_, &final_),
                        "summary": summary,
                    });
                    check(&id, "facts", bytes(&got), bytes(&row["facts"]));
                } else {
                    check(&id, "facts", "null".into(), bytes(&row["facts"]));
                }
            }
            "span" => {
                let s = &row["span"];
                let span = RunSpan {
                    min: s["min"].as_f64().unwrap(),
                    max: s["max"].as_f64().unwrap(),
                    min_exclusive: s["minExclusive"].as_bool(),
                    max_exclusive: s["maxExclusive"].as_bool(),
                };
                check(
                    &id,
                    "describeSpan",
                    bytes(&Value::String(describe_span(&span))),
                    bytes(&row["describe"]),
                );
                for probe in row["within"].as_array().unwrap() {
                    let n = probe[0].as_f64().unwrap();
                    check(
                        &id,
                        &format!("withinSpan({n})"),
                        within_span(n, &span).to_string(),
                        probe[1].to_string(),
                    );
                }
            }
            "veto" => {
                let got = veto_result_for(row["transparent"].as_bool().unwrap());
                check(
                    &id,
                    "vetoResultFor",
                    got.to_string(),
                    row["out"].as_str().unwrap().to_string(),
                );
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    eprintln!("pascal_intervention_pure rows: {counts:?}");
    assert!(
        counts.get("recompute").copied().unwrap_or(0) > 0,
        "no recompute rows ran"
    );
    assert!(
        failures.is_empty(),
        "{} comparand(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
