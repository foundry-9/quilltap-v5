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

use quilltap_core::pascal::custom_tool_types::{safe_parse, OutcomeState};
use quilltap_core::pascal::custom_tools::{
    execute_custom_tool, format_value, matches_when, render_template, CustomToolRunResult,
    LlmInvokeOptions, LlmInvokeResult, LlmInvoker, LlmSubject, OutcomeSubjects, ResolvedParams,
    ResolvedValue, RollForm, TemplateVars, WinningOutcome,
};
use quilltap_core::pascal::dice::FixedBytes;
use serde_json::{json, Map, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

// P4.D329: a row's bag — v4's real context, its answers recorded (R-3).
mod recorded_inventory;
use quilltap_core::pascal::custom_tool_types::{InventoryRef, StateRefFallback};
use quilltap_core::pascal::inventory_subjects::{
    inventory_count_holds, inventory_state_holds, parse_inventory_operand, resolve_inventory_value,
    ResolvedInventoryCountTest, ResolvedInventoryStateTest,
};
use recorded_inventory::RowBag;

/// P4.D169: Pascal's entrances now take their clock from the caller, so a
/// differential can freeze it. Until this family's corpus carries a
/// progression, no row reads the value; a fixed instant keeps the run
/// reproducible. 2026-08-29T12:00:00Z.
const PASCAL_NOW_MS: i64 = 1_788_004_800_000;

/// The oracle's `LlmScript`, rebuilt on this side so both runs hand the core the
/// SAME canned oracle. `None` leaves the seam unwired.
enum Script {
    Answer {
        answer: String,
        provider: Option<String>,
        model: Option<String>,
    },
    Fail(String),
    /// v4's invoker THROWS; `resolveLlmConsult` catches it into the same
    /// `{ok:false, reason}` an explicit failure produces, so this side reports
    /// the throw message as a plain failure — identical downstream.
    Throws(String),
}

fn script_of(v: &Value) -> Option<Script> {
    let o = v.as_object()?;
    if let Some(t) = o.get("throws").and_then(Value::as_str) {
        return Some(Script::Throws(t.to_string()));
    }
    if let Some(f) = o.get("fail").and_then(Value::as_str) {
        return Some(Script::Fail(f.to_string()));
    }
    Some(Script::Answer {
        answer: o.get("answer")?.as_str()?.to_string(),
        provider: o
            .get("provider")
            .and_then(Value::as_str)
            .map(str::to_string),
        model: o.get("model").and_then(Value::as_str).map(str::to_string),
    })
}

/// A canned invoker that also RECORDS what the core asked, so the differential
/// can pin the rendered prompt and the advertised cap — two facts the run result
/// alone would not carry.
struct CannedInvoker {
    script: Script,
    seen: Mutex<(Option<String>, Option<i64>)>,
}

impl LlmInvoker for CannedInvoker {
    fn invoke<'a>(
        &'a self,
        prompt: &'a str,
        options: LlmInvokeOptions,
    ) -> Pin<Box<dyn Future<Output = LlmInvokeResult> + Send + 'a>> {
        {
            let mut seen = self.seen.lock().unwrap();
            *seen = (Some(prompt.to_string()), Some(options.max_output_chars));
        }
        Box::pin(async move {
            match &self.script {
                Script::Throws(m) | Script::Fail(m) => {
                    LlmInvokeResult::Failed { reason: m.clone() }
                }
                Script::Answer {
                    answer,
                    provider,
                    model,
                } => LlmInvokeResult::Answered {
                    output: answer.clone(),
                    provider: provider.clone(),
                    model: model.clone(),
                },
            }
        })
    }
}

/// P4.D333 (v4 `322a5488e`): the `outcome` var a `renderTemplate` row may
/// pose — `{ state, index, brief? }`, the winning row as a template sees it.
fn outcome_of(v: Option<&Value>) -> Option<WinningOutcome> {
    let o = v?.as_object()?;
    let state = match o.get("state")?.as_str()? {
        "success" => OutcomeState::Success,
        "partial" => OutcomeState::Partial,
        "failure" => OutcomeState::Failure,
        "info" => OutcomeState::Info,
        other => panic!("not an outcome state: {other}"),
    };
    Some(WinningOutcome {
        state,
        index: o.get("index")?.as_u64()? as usize,
        brief: o.get("brief").and_then(Value::as_str).map(str::to_string),
    })
}

/// The oracle's `{ok, output}` subject rows.
fn llm_subject_of(v: Option<&Value>) -> Option<LlmSubject> {
    let o = v?.as_object()?;
    Some(LlmSubject {
        ok: o.get("ok")?.as_bool()?,
        output: o.get("output")?.as_str()?.to_string(),
    })
}

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
    // `...(metadataTested ? { metadataTested } : {})` — last, and only when set.
    if let Some(tested) = &r.metadata_tested {
        let mut mt = Map::new();
        for (k, v) in tested {
            mt.insert(k.clone(), resolved_to_json(v));
        }
        m.insert("metadataTested".into(), Value::Object(mt));
    }
    // `...(llm ? { llm } : {})` — last, and only when the definition declared one.
    if let Some(llm) = &r.llm {
        let mut lm = Map::new();
        lm.insert("ok".into(), Value::Bool(llm.ok));
        lm.insert("output".into(), Value::String(llm.output.clone()));
        lm.insert("prompt".into(), Value::String(llm.prompt.clone()));
        // v4 spreads reason/provider/model only when present, in that order.
        if let Some(reason) = &llm.reason {
            lm.insert("reason".into(), Value::String(reason.clone()));
        }
        if let Some(p) = &llm.provider {
            lm.insert("provider".into(), Value::String(p.clone()));
        }
        if let Some(md) = &llm.model {
            lm.insert("model".into(), Value::String(md.clone()));
        }
        m.insert("llm".into(), Value::Object(lm));
    }
    // `...(chipLabel !== undefined ? { chipLabel } : {})` then
    // `...(effects ? { effects } : {})` — v4 `c4d4b0de`, in that order and last.
    // The Workbench preview body spreads this whole object, so the placement is
    // payload, not bookkeeping.
    if let Some(chip_label) = &r.chip_label {
        m.insert("chipLabel".into(), Value::String(chip_label.clone()));
    }
    if let Some(effects) = &r.effects {
        m.insert("effects".into(), serde_json::to_value(effects).unwrap());
    }
    Value::Object(m)
}

fn supplied_of(row: &Value) -> Option<Map<String, Value>> {
    row.get("supplied").and_then(|v| v.as_object()).cloned()
}

/// The metadata sheet a row carries, if any (`null`/absent → `None`).
/// The oracle row's mock merged state (absent → the empty view).
fn state_of(v: Option<&Value>) -> Option<Value> {
    match v {
        Some(o @ Value::Object(_)) => Some(o.clone()),
        _ => None,
    }
}

fn metadata_of(v: Option<&Value>) -> Option<Map<String, Value>> {
    v.and_then(|v| v.as_object()).cloned()
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

/// P4.D329: the list-valued `inventoryAggregate` rows — they compare entry by
/// entry through P4.D333's list rule (`metadata_match.rs`) and decline on a
/// branch without it (the two `titles` rows decline where v4 holds; the miss
/// row logs the old `the key holds an array` decline v4 no longer logs).
/// MEASURED: exactly these three. A carved row that turns GREEN fails the run
/// (lift it).
const LIST_RULE_RESIDUE: &[&str] = &[
    "inv-agg-titles-param",
    "inv-agg-titles-param-miss",
    "inv-agg-owned-titles",
];

#[test]
fn pascal_custom_tools_execution_matches_oracle() {
    // The consult seam made `execute_custom_tool` async; everything else in this
    // corpus stays sync, so a current-thread runtime drives just those rows.
    // P4.D42: the cheap-LLM executor now bounds each attempt with
    // `tokio::time::timeout`, so that runtime needs the time driver too.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("a current-thread runtime");

    let path = match std::env::var("QT_ORACLE_PASCAL_EXECUTION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_EXECUTION to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    let mut coverage = std::collections::BTreeMap::<String, usize>::new();

    let mut failed: Vec<String> = Vec::new();
    let mut residue_green: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap().to_string();
        let id = row["id"].as_str().unwrap().to_string();
        let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // P4.D329: every row runs under the capture rig; a row carrying v4's
            // `inventoryLines` compares the inventory lines this lane owns.
            let ((), lines) = quilltap_core::test_support::captured_with(|| match kind.as_str() {
                "formatValue" => {
                    let input = row["input"].as_f64().unwrap();
                    let want = row["out"].as_str().unwrap();
                    assert_eq!(format_value(input), want, "formatValue '{id}'");
                }

                "renderTemplate" => {
                    let tool = tool_of(&row, &id);
                    let state = state_of(row.get("state"));
                    // P4.D329: `inventory`, absent on every pre-existing row.
                    let bag = RowBag::from_row(row.get("inventory"));
                    let params = quilltap_core::pascal::custom_tools::resolve_params(
                        &tool,
                        None,
                        state.as_ref(),
                        bag.as_dyn(),
                    )
                    .unwrap_or_else(|e| panic!("case '{id}': params: {e}"));
                    let message = row["message"].as_str().unwrap();
                    let metadata = metadata_of(row.get("metadata"));
                    let progress = metadata_of(row.get("progress"));
                    let llm = llm_subject_of(row.get("llm"));
                    let outcome = outcome_of(row.get("outcome"));
                    let got = render_template(
                        message,
                        &TemplateVars {
                            value: 12.3456,
                            roll: 0.6789,
                            dice: "3d6: [4, 2, 6] = 12",
                            params: &params,
                            metadata: metadata.as_ref(),
                            // P4.D169: a row may pose a flattened progress sheet and
                            // a run clock. Absent on every pre-existing row, which
                            // is what keeps their bytes unmoved.
                            progress: progress.as_ref(),
                            now: row.get("now").and_then(Value::as_i64),
                            llm: llm.as_ref(),
                            state: state.as_ref(),
                            // P4.D319: v4's `character` var, absent on every row
                            // but the `{{character}}` block's.
                            character: row.get("character").and_then(Value::as_str),
                            inventory: bag.as_dyn(),
                            outcome: outcome.as_ref(),
                        },
                    );
                    assert_eq!(got, row["out"].as_str().unwrap(), "renderTemplate '{id}'");
                }

                "resolveParams" => {
                    let tool = tool_of(&row, &id);
                    let supplied = supplied_of(&row);
                    let state = state_of(row.get("state"));
                    let bag = RowBag::from_row(row.get("inventory"));
                    match quilltap_core::pascal::custom_tools::resolve_params(
                        &tool,
                        supplied.as_ref(),
                        state.as_ref(),
                        bag.as_dyn(),
                    ) {
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
                    let state = state_of(row.get("state"));
                    let bag = RowBag::from_row(row.get("inventory"));
                    let params = quilltap_core::pascal::custom_tools::resolve_params(
                        &tool,
                        // P4.D329: a row may supply params (a templated key reads one).
                        supplied_of(&row).as_ref(),
                        state.as_ref(),
                        bag.as_dyn(),
                    )
                    .unwrap_or_else(|e| panic!("case '{id}': params: {e}"));
                    let when = &tool.outcomes[0].when;
                    let metadata = metadata_of(row.get("metadata"));
                    let progress = metadata_of(row.get("progress"));
                    let llm = llm_subject_of(row.get("llm"));
                    let subjects = OutcomeSubjects {
                        value: 12.0,
                        roll: 0.6,
                        params: &params,
                        metadata: metadata.as_ref(),
                        progress: progress.as_ref(),
                        llm: llm.as_ref(),
                        state: state.as_ref(),
                        character: None,
                        inventory: bag.as_dyn(),
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
                    let metadata = metadata_of(row["overrides"].get("metadata"));
                    // P4.D169: v4's `overrides.progress` / `overrides.now`. Absent
                    // on every pre-existing row.
                    let progress = metadata_of(row["overrides"].get("progress"));
                    let state = state_of(row["overrides"].get("state"));

                    let pool = hex_bytes(row["poolHex"].as_str().unwrap());
                    let mut rng = FixedBytes::new(pool);
                    // P4.D329: v4's `overrides.inventory`.
                    let bag = RowBag::from_row(row["overrides"].get("inventory"));

                    let invoker = script_of(&row["llmScript"]).map(|script| CannedInvoker {
                        script,
                        seen: Mutex::new((None, None)),
                    });

                    let outcome = rt.block_on(execute_custom_tool(
                        &tool,
                        supplied.as_ref(),
                        private,
                        metadata.as_ref(),
                        state.as_ref(),
                        &mut rng,
                        invoker.as_ref().map(|i| i as &dyn LlmInvoker),
                        progress.as_ref(),
                        row["overrides"]
                            .get("now")
                            .and_then(Value::as_i64)
                            .unwrap_or(PASCAL_NOW_MS),
                        // P4.D319: v4's `overrides.character`, untrimmed — the core
                        // applies `trim() || undefined` itself.
                        row["overrides"].get("character").and_then(Value::as_str),
                        bag.as_dyn(),
                    ));

                    // The byte cursor is the assertion inspection cannot make.
                    assert_eq!(
                        rng.consumed(),
                        row["bytesConsumed"].as_u64().unwrap() as usize,
                        "executeCustomTool '{id}': bytes drawn differ"
                    );

                    // What the core ASKED, and with what advertised cap — the two
                    // facts the result alone would not pin.
                    let (prompt_seen, cap_seen) = match &invoker {
                        Some(i) => i.seen.lock().unwrap().clone(),
                        None => (None, None),
                    };
                    assert_eq!(
                        prompt_seen.map(Value::String).unwrap_or(Value::Null),
                        row["llmPromptSeen"],
                        "executeCustomTool '{id}': the rendered consult prompt differs"
                    );
                    let want_cap = row["llmOptionsSeen"]
                        .get("maxOutputChars")
                        .and_then(Value::as_i64);
                    assert_eq!(
                        cap_seen, want_cap,
                        "executeCustomTool '{id}': the advertised output cap differs"
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

                // === P4.D329: v4 `inventory-subjects.ts` driven directly ===
                "inventorySubject" => {
                    let bag = RowBag::from_row(row.get("inventory"));
                    let mut declines: Vec<String> = Vec::new();
                    let got = match row["fn"].as_str().unwrap() {
                        "parseInventoryOperand" => {
                            match parse_inventory_operand(row["path"].as_str().unwrap()) {
                                Some((r, key)) => json!({ "ref": r, "key": key }),
                                None => Value::Null,
                            }
                        }
                        "resolveInventoryValue" => {
                            let operand = &row["operand"];
                            let fallback = match &operand["fallback"] {
                                Value::Number(n) => StateRefFallback::Number(n.as_f64().unwrap()),
                                Value::String(s) => StateRefFallback::String(s.clone()),
                                Value::Bool(b) => StateRefFallback::Bool(*b),
                                other => panic!("case '{id}': fallback {other}"),
                            };
                            let r = InventoryRef {
                                inventory: operand["$inventory"].as_str().unwrap().to_string(),
                                fallback,
                            };
                            resolved_to_json(&resolve_inventory_value(&r, bag.as_dyn()))
                        }
                        "inventoryStateHolds" => {
                            let test = ResolvedInventoryStateTest {
                                eq: row["test"]["eq"].as_str().map(str::to_string),
                                neq: row["test"]["neq"].as_str().map(str::to_string),
                            };
                            Value::Bool(inventory_state_holds(
                                &test,
                                row["ref"].as_str().unwrap(),
                                bag.as_dyn(),
                                Some(&mut |reason: &str| declines.push(reason.to_string())),
                            ))
                        }
                        "inventoryCountHolds" => {
                            let n = |k: &str| row["test"].get(k).and_then(Value::as_f64);
                            let test = ResolvedInventoryCountTest {
                                gt: n("gt"),
                                gte: n("gte"),
                                lt: n("lt"),
                                lte: n("lte"),
                                eq: n("eq"),
                                neq: n("neq"),
                            };
                            Value::Bool(inventory_count_holds(
                                &test,
                                row["ref"].as_str().unwrap(),
                                bag.as_dyn(),
                                Some(&mut |reason: &str| declines.push(reason.to_string())),
                            ))
                        }
                        other => panic!("case '{id}': unknown subject fn '{other}'"),
                    };
                    assert_eq!(
                        serde_json::to_string(&got).unwrap(),
                        serde_json::to_string(&row["out"]).unwrap(),
                        "inventorySubject '{id}'"
                    );
                    assert_eq!(
                        json!(declines),
                        row["declines"],
                        "inventorySubject '{id}': decline reasons differ"
                    );
                }

                other => panic!("unknown row kind '{other}'"),
            });
            if let Some(want) = row.get("inventoryLines").and_then(Value::as_array) {
                let want: Vec<String> = want
                    .iter()
                    .map(|l| {
                        recorded_inventory::v4_line(
                            l["level"].as_str().unwrap(),
                            "",
                            l["message"].as_str().unwrap(),
                            &l["bag"],
                        )
                    })
                    .collect();
                let got: Vec<String> = lines
                    .iter()
                    .filter(|l| recorded_inventory::is_inventory_line(l))
                    .map(|l| recorded_inventory::without_target(l))
                    .collect();
                assert_eq!(got, want, "'{id}': the inventory lines differ");
                !want.is_empty()
            } else {
                false
            }
        }));
        let residue = LIST_RULE_RESIDUE.contains(&id.as_str());
        match (&walked, residue) {
            (Err(_), false) => failed.push(id.clone()),
            (Ok(_), true) => residue_green.push(id.clone()),
            _ => {}
        }
        if walked.unwrap_or(false) {
            *coverage.entry("inventoryLines".to_string()).or_default() += 1;
        }
        if row
            .get("inventory")
            .or(row["overrides"].get("inventory"))
            .is_some_and(|b| b.get("recorded").is_some())
        {
            *coverage.entry("recordedBag".to_string()).or_default() += 1;
        }

        // P4.D169's floors. Every one of these was ZERO before this lane widened
        // the corpus, and every family below was GREEN at zero — which is the
        // whole reason they are asserted rather than assumed.
        if !row
            .get("progress")
            .and_then(Value::as_object)
            .is_none_or(serde_json::Map::is_empty)
            || !row["overrides"]
                .get("progress")
                .and_then(Value::as_object)
                .is_none_or(serde_json::Map::is_empty)
        {
            *coverage.entry(format!("{kind}:progress")).or_default() += 1;
        }
        if row.get("now").is_some() || row["overrides"].get("now").is_some() {
            *coverage.entry(format!("{kind}:now")).or_default() += 1;
        }

        *counts.entry(kind).or_default() += 1;
    }

    assert!(failed.is_empty(), "rows differ from v4: {failed:?}");
    assert!(
        residue_green.is_empty(),
        "LIST_RULE_RESIDUE rows turned GREEN (P4.D333's list rule landed?) — lift them: {residue_green:?}"
    );
    assert!(!counts.is_empty(), "oracle file looks empty");
    for key in [
        "renderTemplate:progress",
        "renderTemplate:now",
        "matchesWhen:progress",
        "executeCustomTool:progress",
        "executeCustomTool:now",
    ] {
        assert!(
            coverage.get(key).copied().unwrap_or(0) > 0,
            "no row exercised '{key}' (coverage: {coverage:?})"
        );
    }
    // P4.D329's floors: the recorded bag, and v4's lines compared.
    assert!(
        coverage.get("recordedBag").copied().unwrap_or(0) >= 60,
        "too few rows read a recorded bag (coverage: {coverage:?})"
    );
    assert!(
        coverage.get("inventoryLines").copied().unwrap_or(0) >= 30,
        "too few rows compared v4's inventory lines (coverage: {coverage:?})"
    );
    for kind in [
        "formatValue",
        "renderTemplate",
        "resolveParams",
        "matchesWhen",
        "executeCustomTool",
        // P4.D329
        "inventorySubject",
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
