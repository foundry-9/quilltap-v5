//! Custom Tools — the shared execution core (v4 `lib/pascal/custom-tools.ts`,
//! the `Execution core` half).
//!
//! Two entrances share this core: the `run_custom` tool (a model rolls) and the
//! composer popup (the human rolls). Both land on [`execute_custom_tool`] so a
//! roll means the same thing whoever asked for it.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use crate::state::paths::{get_at_path, parse_path, PathKey};
use regex::Captures;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use super::custom_tool_types::{
    is_state_ref_value, outcome_table_tests_llm, parse_effect_target, AnyOperand,
    CustomToolEffectBody, CustomToolLlm, CustomToolParameter, CustomToolWardrobeGesture,
    EffectCondition, EffectTarget, EffectValue, EffectWhen, LlmComparator, MetadataComparator,
    NumberOrParamRef, NumericComparator, OutcomeState, ParamComparator, ParameterType, PutOnMode,
    QtapCustomTool, Roll, RollRange, StateRef, StateRefFallback, StringOperand, Visibility,
    WardrobeEffectAction, When, WhenObject, MAX_LLM_OUTPUT_LENGTH,
};
// === P4.D329: the inventory deal ===
use super::custom_tool_types::{
    is_inventory_ref_value, CustomToolInventoryGesture, InventoryEffectAction, InventoryRef,
    InventoryStateOperand,
};
use super::inventory_aggregate::{describe_aggregate_value, inventory_aggregate_holds};
use super::inventory_subjects::{
    inventory_count_holds, inventory_state_holds, resolve_inventory_value,
    ResolvedInventoryCountTest, ResolvedInventoryStateTest,
};
// === LANE-REMOVE (P4.D329): C2 import ===
use super::dice::{format_dice_breakdown, parse_dice_notation, roll_notation, RandomBytes};
use super::expressions::{evaluate_expression, parse_expression};
use super::js_value::{json_stringify, number_to_string, to_js_string, to_number};
use super::metadata_match::{js_primitive, metadata_comparator_holds};
use super::p4d329_c2_standins::InventoryContext;
use super::placeholders::InventoryPlaceholderField;
use super::placeholders::{
    classify_placeholder, OutcomeField, PlaceholderRef, PLACEHOLDER_PATTERN,
};
use crate::jsstr::{self, js_trim};

/// A run that could not be completed. Never becomes a fabricated outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomToolRunError(pub String);

impl fmt::Display for CustomToolRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CustomToolRunError {}

fn err(message: impl Into<String>) -> CustomToolRunError {
    CustomToolRunError(message.into())
}

/// The persistent-state view a run resolves `$state` references against — the
/// merged cascade (chat → project → group → general), always a JSON object; an
/// entrance that has no state to offer passes `None` (v4 `{}`).
///
/// v4 `resolveStateValue` (`f48f34dc`): pure and total — the value at the path
/// when present AND of the fallback's type (a found number must also be
/// finite), and the fallback otherwise. It never fails — the required fallback
/// is exactly what makes a run always dealable.
pub fn resolve_state_value(r: &StateRef, state: Option<&Value>) -> ResolvedValue {
    let empty = Value::Object(Map::new());
    let state = state.unwrap_or(&empty);
    let found = get_at_path(state, &parse_path(Some(&r.state)));
    match (&found, &r.fallback) {
        (Some(Value::Number(n)), StateRefFallback::Number(fb)) => {
            // `typeof found === 'number'` + the finite guard.
            match n.as_f64() {
                Some(f) if f.is_finite() => ResolvedValue::Number(f),
                _ => ResolvedValue::Number(*fb),
            }
        }
        (Some(Value::String(s)), StateRefFallback::String(_)) => ResolvedValue::String(s.clone()),
        (Some(Value::Bool(b)), StateRefFallback::Bool(_)) => ResolvedValue::Bool(*b),
        _ => match &r.fallback {
            StateRefFallback::Number(f) => ResolvedValue::Number(*f),
            StateRefFallback::String(s) => ResolvedValue::String(s.clone()),
            StateRefFallback::Bool(b) => ResolvedValue::Bool(*b),
        },
    }
}

/// Rehydrate a schema-validated `$state` parameter default (stored as its raw
/// JSON object) into a [`StateRef`].
fn state_ref_from_default(v: &Value) -> Option<StateRef> {
    let obj = v.as_object()?;
    let state = obj.get("$state")?.as_str()?.to_string();
    let fallback = match obj.get("fallback")? {
        Value::Number(n) => StateRefFallback::Number(n.as_f64()?),
        Value::String(s) => StateRefFallback::String(s.clone()),
        Value::Bool(b) => StateRefFallback::Bool(*b),
        _ => return None,
    };
    Some(StateRef { state, fallback })
}

// === P4.D329: the `$inventory` parameter default ===
/// Rehydrate a schema-validated `$inventory` parameter default (stored as its
/// raw JSON object) into an [`InventoryRef`].
fn inventory_ref_from_default(v: &Value) -> Option<InventoryRef> {
    let obj = v.as_object()?;
    let inventory = obj.get("$inventory")?.as_str()?.to_string();
    let fallback = match obj.get("fallback")? {
        Value::Number(n) => StateRefFallback::Number(n.as_f64()?),
        Value::String(s) => StateRefFallback::String(s.clone()),
        Value::Bool(b) => StateRefFallback::Bool(*b),
        _ => return None,
    };
    Some(InventoryRef {
        inventory,
        fallback,
    })
}

/// One resolved parameter value: v4's `number | string | boolean`.
///
/// Declared in [`super::metadata_match`] with the shared fail-soft table (v4's
/// `Primitive`, `6864bf0e`) and re-exported here, which is where every existing
/// consumer names it.
pub use super::metadata_match::ResolvedValue;

/// Resolved parameter values, post-default and post-clamp. Ordered, mirroring
/// the declaration order v4's `Object.entries` walks.
pub type ResolvedParams = Vec<(String, ResolvedValue)>;

/// The metadata keys a run actually consulted, and what they held at the time —
/// v4's `MetadataTested = Record<string, number | string | boolean>`. Ordered,
/// mirroring the winning `when.metadata` key order v4's `Object.keys` walks.
pub type MetadataTested = Vec<(String, ResolvedValue)>;

fn lookup<'a>(params: &'a ResolvedParams, name: &str) -> Option<&'a ResolvedValue> {
    params.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

/// Which form of roll produced a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollForm {
    Range,
    Dice,
}

/// What an LLM invoker hands back: the model's raw answer, or the technical
/// reason there is none. The invoker never sees the author's `errorMessage` —
/// translating failure into the author's words is the execution core's job.
#[derive(Debug, Clone, PartialEq)]
pub enum LlmInvokeResult {
    Answered {
        output: String,
        provider: Option<String>,
        model: Option<String>,
    },
    Failed {
        reason: String,
    },
}

/// What the core tells an invoker about the answer it is prepared to keep.
///
/// The effective output cap (the definition's `maxOutput`, or the default).
/// Advisory: the real invoker scales the call's token budget from it, so a
/// long-form consult is not starved at the provider. The core still enforces
/// the cap on whatever comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmInvokeOptions {
    pub max_output_chars: i64,
}

/// The seam between the execution core and whatever actually talks to a model.
/// Injected by the entrances (`pascal::llm_consult` builds the real one) so the
/// core stays testable — and so the proving bench can hand in a pretend oracle
/// instead of spending money.
///
/// v4's `LlmInvoker` is an async function that MAY throw; `resolveLlmConsult`
/// catches the throw and folds it into the same `{ok:false, reason}` an explicit
/// failure produces. Rust has no throw to catch, so [`LlmInvokeResult::Failed`]
/// carries both — the distinction was never observable downstream.
pub trait LlmInvoker: Send + Sync {
    fn invoke<'a>(
        &'a self,
        prompt: &'a str,
        options: LlmInvokeOptions,
    ) -> Pin<Box<dyn Future<Output = LlmInvokeResult> + Send + 'a>>;
}

/// The consult as the rest of a run sees it — subjects, templates, and the roll
/// record all read from this one resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmConsultResult {
    /// Whether the consult produced an answer.
    pub ok: bool,
    /// The model's trimmed answer on success; the author's `errorMessage` on
    /// failure.
    pub output: String,
    /// The rendered prompt actually posed — the record of what was asked.
    pub prompt: String,
    /// Technical failure reason. Logged and recorded, never spoken in the fiction.
    pub reason: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// The pair `when.llm` tests and `{{llm}}` renders (v4 `LlmSubject`).
#[derive(Debug, Clone, PartialEq)]
pub struct LlmSubject {
    pub ok: bool,
    pub output: String,
}

impl LlmConsultResult {
    /// The subject view of a resolved consult.
    pub fn subject(&self) -> LlmSubject {
        LlmSubject {
            ok: self.ok,
            output: self.output.clone(),
        }
    }

    /// The consult as `pascalMeta.llm` / the preview response carries it: v4's
    /// declaration order, with each optional OMITTED rather than nulled (v4
    /// spreads `...(x ? { x } : {})`). The single source for all three writers —
    /// the `run_custom` handler, the chat run, and the Workbench preview — so
    /// the key order cannot drift between them.
    pub fn to_wire(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("ok".into(), Value::Bool(self.ok));
        m.insert("output".into(), Value::String(self.output.clone()));
        m.insert("prompt".into(), Value::String(self.prompt.clone()));
        if let Some(reason) = &self.reason {
            m.insert("reason".into(), Value::String(reason.clone()));
        }
        if let Some(provider) = &self.provider {
            m.insert("provider".into(), Value::String(provider.clone()));
        }
        if let Some(model) = &self.model {
            m.insert("model".into(), Value::String(model.clone()));
        }
        m
    }
}

/// Everything a run produced — enough to post the message and fill `pascalMeta`.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomToolRunResult {
    pub tool: String,
    pub params: ResolvedParams,
    pub roll_form: RollForm,
    pub notation: Option<String>,
    pub raw: f64,
    pub dice_rolls: Option<Vec<i64>>,
    pub value: f64,
    pub state: OutcomeState,
    pub outcome_index: usize,
    /// The rendered outcome message, templates substituted.
    pub message: String,
    /// Dice breakdown, or `""` for Form A.
    pub dice_breakdown: String,
    pub visibility: Visibility,
    /// The metadata keys the winning outcome tested, and what they held when it
    /// was tested. `None` when the winning row consulted no metadata.
    pub metadata_tested: Option<MetadataTested>,
    /// The LLM consult, when the definition declares one.
    pub llm: Option<LlmConsultResult>,
    /// The rendered `chipLabel`, when the definition declares one — rendered
    /// once, in [`execute_custom_tool`], after outcome selection, with the same
    /// subjects as the message. Both entrances copy it into `pascalMeta`; the
    /// Salon chip and the announcement header read the same string, so they can
    /// never disagree.
    pub chip_label: Option<String>,
    /// The definition's effects, resolved (or skipped, with reasons) against
    /// this run. Computed pure — the entrances apply them via
    /// [`super::side_effects::apply_custom_tool_effects`]; the Proving Bench
    /// only ever shows them. `None` when the definition declares none.
    pub effects: Option<Vec<ResolvedEffect>>,
}

/// Validate and coerce caller-supplied parameters against the declarations.
///
/// Unknown keys are rejected rather than ignored: a model passing `bonuss: 5`
/// has misunderstood the tool, and silently rolling without the bonus would look
/// like the tool worked.
pub fn resolve_params(
    definition: &QtapCustomTool,
    supplied: Option<&serde_json::Map<String, Value>>,
    state: Option<&Value>,
    // P4.D329: the rolling character's bag, for a `$inventory` default.
    inventory: Option<&dyn InventoryContext>,
) -> Result<ResolvedParams, CustomToolRunError> {
    let empty = Vec::new();
    let declared = definition.parameters.as_ref().unwrap_or(&empty);

    if let Some(given) = supplied {
        for key in given.keys() {
            if !declared.iter().any(|(k, _)| k == key) {
                let tail = if declared.is_empty() {
                    " (it takes none)".to_string()
                } else {
                    let names: Vec<&str> = declared.iter().map(|(k, _)| k.as_str()).collect();
                    format!(" (expected: {})", names.join(", "))
                };
                return Err(err(format!(
                    "\"{key}\" is not a parameter of {}{tail}",
                    definition.name
                )));
            }
        }
    }

    let mut resolved = Vec::with_capacity(declared.len());
    for (name, spec) in declared {
        let given = supplied.and_then(|g| g.get(name));
        resolved.push((
            name.clone(),
            coerce_param(&definition.name, name, spec, given, state, inventory)?,
        ));
    }
    Ok(resolved)
}

/// v4 `coerceParam`.
///
/// **This is Pascal's OWN coercion, not the tool layer's `llmNumber`.** v4 keeps
/// the two separate and they must not share an implementation.
fn coerce_param(
    tool_name: &str,
    name: &str,
    spec: &CustomToolParameter,
    value: Option<&Value>,
    state: Option<&Value>,
    inventory: Option<&dyn InventoryContext>,
) -> Result<ResolvedValue, CustomToolRunError> {
    // `undefined` and `null` both fall back to the declared default, which may
    // itself be a `$state` reference resolved against the merged state, or a
    // `$inventory` reference into an object the rolling character carries
    // (each fallback is type-checked against this parameter's type at load
    // time, so it always fits).
    let value = match value {
        None | Some(Value::Null) => {
            if is_state_ref_value(&spec.default) {
                if let Some(r) = state_ref_from_default(&spec.default) {
                    return Ok(resolve_state_value(&r, state));
                }
            }
            // === P4.D329: the `$inventory` default ===
            if is_inventory_ref_value(&spec.default) {
                if let Some(r) = inventory_ref_from_default(&spec.default) {
                    return Ok(resolve_inventory_value(&r, inventory));
                }
            }
            return Ok(default_value(spec));
        }
        Some(v) => v,
    };

    match spec.param_type {
        ParameterType::Number | ParameterType::Integer => {
            // Models routinely pass numbers as strings; accept that rather than
            // failing a roll over a quoting habit.
            let n = to_number(value);
            if !n.is_finite() {
                return Err(err(format!(
                    "{tool_name}: parameter \"{name}\" must be a number, got {}",
                    json_stringify(value)
                )));
            }
            let rounded = if spec.param_type == ParameterType::Integer {
                js_round(n)
            } else {
                n
            };
            Ok(ResolvedValue::Number(clamp(rounded, spec.min, spec.max)))
        }
        ParameterType::Boolean => match value {
            Value::Bool(b) => Ok(ResolvedValue::Bool(*b)),
            Value::String(s) if s == "true" => Ok(ResolvedValue::Bool(true)),
            Value::String(s) if s == "false" => Ok(ResolvedValue::Bool(false)),
            other => Err(err(format!(
                "{tool_name}: parameter \"{name}\" must be a boolean, got {}",
                json_stringify(other)
            ))),
        },
        ParameterType::String => Ok(ResolvedValue::String(match value {
            Value::String(s) => s.clone(),
            other => to_js_string(other),
        })),
    }
}

/// The declared default, as the type the declaration promised. Load-time
/// validation has already matched the two, so the fallbacks are unreachable.
fn default_value(spec: &CustomToolParameter) -> ResolvedValue {
    match &spec.default {
        Value::Number(n) => ResolvedValue::Number(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => ResolvedValue::String(s.clone()),
        Value::Bool(b) => ResolvedValue::Bool(*b),
        other => ResolvedValue::String(to_js_string(other)),
    }
}

/// JS `Math.round`: halves go UP (toward +∞), not away from zero — so
/// `Math.round(-2.5)` is `-2`, where Rust's `f64::round` gives `-3`.
///
/// Compares the fraction against a half rather than flooring `n + 0.5`: that
/// addition is itself rounded, which is why the naive form famously answers `1`
/// for `Math.round(0.49999999999999994)` where the spec (and V8) say `0`.
fn js_round(n: f64) -> f64 {
    if !n.is_finite() {
        return n;
    }
    let f = n.floor();
    if n - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// Clamp into the declared range. Bounds are optional and independent.
fn clamp(value: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    let mut out = value;
    if let Some(m) = min {
        if out < m {
            out = m;
        }
    }
    if let Some(m) = max {
        if out > m {
            out = m;
        }
    }
    out
}

/// Resolve a roll field: literal, `$param` reference, or the field's default.
fn resolve_roll_field(
    tool_name: &str,
    field: &str,
    spec: Option<&NumberOrParamRef>,
    params: &ResolvedParams,
    fallback: f64,
    state: Option<&Value>,
    inventory: Option<&dyn InventoryContext>,
) -> Result<f64, CustomToolRunError> {
    let Some(spec) = spec else {
        return Ok(fallback);
    };

    match spec {
        // === P4.D329: a `$inventory` roll field ===
        // Resolves to the object's (numeric) metadata or its numeric fallback —
        // never a failure, exactly as `$state` does. The throw arm is a
        // regression tripwire (a non-numeric fallback is rejected at load).
        NumberOrParamRef::InventoryRef(r) => match resolve_inventory_value(r, inventory) {
            ResolvedValue::Number(n) if n.is_finite() => Ok(n),
            other => Err(err(format!(
                "{tool_name}: roll.{field} $inventory reference resolved to {} rather than a finite number",
                other.stringify()
            ))),
        },
        // A `$state` roll field resolves to its (numeric, load-checked) value
        // or its numeric fallback — never a failure, so a roll is always
        // dealable. The throw arm is a regression tripwire (a non-numeric
        // fallback is rejected at load time).
        NumberOrParamRef::StateRef(r) => {
            let value = resolve_state_value(r, state);
            match value {
                ResolvedValue::Number(n) if n.is_finite() => Ok(n),
                other => Err(err(format!(
                    "{tool_name}: roll.{field} $state reference resolved to {} rather than a finite number",
                    other.stringify()
                ))),
            }
        }
        NumberOrParamRef::ParamRef(r) => {
            let value = lookup(params, &r.param);
            match value {
                Some(ResolvedValue::Number(n)) if n.is_finite() => Ok(*n),
                other => Err(err(format!(
                    "{tool_name}: roll.{field} references \"{}\", which resolved to {} rather than a finite number",
                    r.param,
                    match other {
                        Some(v) => v.stringify(),
                        // `JSON.stringify(undefined)` is undefined, which a
                        // template literal renders "undefined".
                        None => "undefined".to_string(),
                    }
                ))),
            }
        }
        NumberOrParamRef::Number(n) => {
            if !n.is_finite() {
                return Err(err(format!(
                    "{tool_name}: roll.{field} is not a finite number"
                )));
            }
            Ok(*n)
        }
    }
}

/// A uniform float in [min, max), drawn from crypto-strength randomness.
///
/// `randomInt` only deals in integers, so the fraction comes from 6 random bytes
/// (48 bits — comfortably more precision than a double's 52-bit mantissa needs
/// for this, and far beyond what any outcome table can distinguish).
fn crypto_uniform(min: f64, max: f64, rng: &mut dyn RandomBytes) -> f64 {
    let bytes = rng.random_bytes(6);
    let mut scaled = 0f64;
    for byte in bytes {
        scaled = scaled * 256.0 + byte as f64;
    }
    let fraction = scaled / 2f64.powi(48);
    min + fraction * (max - min)
}

/// Draw the raw value for Form A, honouring `$param` bounds.
fn roll_range(
    definition: &QtapCustomTool,
    range: &RollRange,
    params: &ResolvedParams,
    state: Option<&Value>,
    inventory: Option<&dyn InventoryContext>,
    rng: &mut dyn RandomBytes,
) -> Result<(f64, f64), CustomToolRunError> {
    let name = &definition.name;
    let min = resolve_roll_field(
        name,
        "min",
        range.min.as_ref(),
        params,
        0.0,
        state,
        inventory,
    )?;
    let max = resolve_roll_field(
        name,
        "max",
        range.max.as_ref(),
        params,
        1.0,
        state,
        inventory,
    )?;
    let multiplier = resolve_roll_field(
        name,
        "multiplier",
        range.multiplier.as_ref(),
        params,
        1.0,
        state,
        inventory,
    )?;
    let offset = resolve_roll_field(
        name,
        "offset",
        range.offset.as_ref(),
        params,
        0.0,
        state,
        inventory,
    )?;

    if min > max {
        return Err(err(format!(
            "{name}: the roll's low bound ({}) is above its high bound ({})",
            number_to_string(min),
            number_to_string(max)
        )));
    }

    // A degenerate range short-circuits WITHOUT drawing — it consumes no bytes,
    // which the differential's `FixedBytes::consumed()` pins.
    let raw = if min == max {
        min
    } else {
        crypto_uniform(min, max, rng)
    };

    // The transform order is fixed and load-bearing: multiply, then offset, then
    // round. Rounding first would quantise before the offset could shift it.
    let mut value = raw * multiplier;
    value += offset;
    if range.round == Some(true) {
        value = js_round(value);
    }

    if !value.is_finite() {
        return Err(err(format!(
            "{name}: the roll produced a value that is not a finite number"
        )));
    }

    Ok((raw, value))
}

/// Everything an outcome test may be posed about.
pub struct OutcomeSubjects<'a> {
    /// The final post-transform value.
    pub value: f64,
    /// The raw pre-transform draw.
    pub roll: f64,
    /// The resolved parameters, post-default and post-clamp.
    pub params: &'a ResolvedParams,
    /// The invoking character's metadata sheet (`metadata.json`). `None` (or an
    /// empty map) when nobody in particular rolled — every `metadata` test then
    /// fails and the catch-all answers.
    pub metadata: Option<&'a Map<String, Value>>,
    /// The invoking character's timed progressions, FLATTENED to the primitive
    /// sheet `flatten_progressions` produces — `"cannon.percent"`,
    /// `"cannon.complete"` and the rest, derived from one `now_ms` taken at run
    /// start. `None` (or an empty map) when nobody in particular rolled, or when
    /// they carry no progressions; every `progress` test then fails and the
    /// catch-all answers, exactly as it does for a metadata key the character
    /// lacks.
    pub progress: Option<&'a Map<String, Value>>,
    /// The LLM consult's result. `None` when the definition declares no `llm`
    /// block — an `llm` test then fails soft and the table falls through.
    pub llm: Option<&'a LlmSubject>,
    /// The merged persistent state (chat → project → group → general) that
    /// `$state` comparator operands resolve against. `None` → treated as `{}`,
    /// so every `$state` operand falls to its own fallback.
    pub state: Option<&'a Value>,
    /// The rolling character's name, for `{{character}}`. Absent when nobody in
    /// particular rolled — the placeholder then renders as written (v4
    /// `a05c6f755`). v4's runtime literal places it between `state` and `llm`;
    /// the object is never serialized, so the position is unobservable and the
    /// field is appended here rather than reordering v5's struct (P4.D319 R-11).
    pub character: Option<&'a str>,
    // === P4.D329: the rolling character's bag (appended LAST — never serialized) ===
    /// The rolling character's Inventory for this chat, loaded once per run.
    /// `None` for a run nobody made, or a vault that could not be read: every
    /// `inventory` / `inventoryCount` / `inventoryAggregate` test then declines,
    /// and every `$inventory` operand falls back.
    pub inventory: Option<&'a dyn InventoryContext>,
}

/// A comparator operand as the evaluator sees it: a literal, a `$param`
/// reference, or a `$state` reference.
enum OperandSpec<'a> {
    Literal(ResolvedValue),
    Ref(&'a str),
    State(&'a StateRef),
    // === P4.D329 ===
    Inventory(&'a InventoryRef),
}

fn number_operand(v: &NumberOrParamRef) -> OperandSpec<'_> {
    match v {
        NumberOrParamRef::Number(n) => OperandSpec::Literal(ResolvedValue::Number(*n)),
        NumberOrParamRef::ParamRef(r) => OperandSpec::Ref(&r.param),
        NumberOrParamRef::StateRef(r) => OperandSpec::State(r),
        NumberOrParamRef::InventoryRef(r) => OperandSpec::Inventory(r),
    }
}

fn any_operand(v: &AnyOperand) -> OperandSpec<'_> {
    match v {
        AnyOperand::Number(n) => OperandSpec::Literal(ResolvedValue::Number(*n)),
        AnyOperand::String(s) => OperandSpec::Literal(ResolvedValue::String(s.clone())),
        AnyOperand::Bool(b) => OperandSpec::Literal(ResolvedValue::Bool(*b)),
        AnyOperand::ParamRef(r) => OperandSpec::Ref(&r.param),
        AnyOperand::StateRef(r) => OperandSpec::State(r),
        AnyOperand::InventoryRef(r) => OperandSpec::Inventory(r),
    }
}

/// Resolve a comparator operand: a literal, or the value of the parameter it
/// references.
///
/// The reference is validated at load time, so a failure here is a regression
/// rather than an authoring error — it throws instead of quietly declining to
/// match, which would look like the table simply skipping a row.
fn resolve_operand(
    tool_name: &str,
    operand: &OperandSpec,
    params: &ResolvedParams,
    label: &str,
    state: Option<&Value>,
    inventory: Option<&dyn InventoryContext>,
) -> Result<ResolvedValue, CustomToolRunError> {
    match operand {
        OperandSpec::Literal(v) => Ok(v.clone()),
        // A `$state` operand resolves against the merged state, falling back to
        // its own (load-typed) fallback — it never throws, matching the
        // metadata doctrine.
        OperandSpec::State(r) => Ok(resolve_state_value(r, state)),
        // P4.D329: so does a `$inventory` operand, against the rolling
        // character's bag.
        OperandSpec::Inventory(r) => Ok(resolve_inventory_value(r, inventory)),
        OperandSpec::Ref(name) => match lookup(params, name) {
            Some(v) => Ok(v.clone()),
            None => Err(err(format!(
                "{tool_name}: {label} references \"{name}\", which is not a declared parameter"
            ))),
        },
    }
}

/// Demand a number for an ordering comparison. Load-time validation precedes this.
fn require_number(
    tool_name: &str,
    value: &ResolvedValue,
    label: &str,
) -> Result<f64, CustomToolRunError> {
    match value {
        ResolvedValue::Number(n) if n.is_finite() => Ok(*n),
        other => Err(err(format!(
            "{tool_name}: {label} cannot be ordered — it is {} rather than a finite number",
            other.stringify()
        ))),
    }
}

/// Demand a string for a containment comparison. Load-time validation precedes
/// this, so a failure here is a regression rather than an authoring error.
///
/// UNREACHABLE from a loadable definition — `validate_comparator`'s containment
/// check rejects a non-string subject or needle first, exactly as it does for
/// the ordering arm's `require_number`. Kept as v4 keeps it: a regression
/// tripwire, not a live path.
fn require_string(
    tool_name: &str,
    value: &ResolvedValue,
    label: &str,
) -> Result<String, CustomToolRunError> {
    match value {
        ResolvedValue::String(s) => Ok(s.clone()),
        other => Err(err(format!(
            "{tool_name}: {label} cannot be searched — it is {} rather than a string",
            other.stringify()
        ))),
    }
}

/// Evaluate one comparator against one subject. Keys AND together, and an
/// operand may be a `$param` reference rather than a literal.
fn matches_comparator(
    tool_name: &str,
    operands: [(&str, Option<OperandSpec>); 8],
    subject: &ResolvedValue,
    subject_label: &str,
    params: &ResolvedParams,
    state: Option<&Value>,
    inventory: Option<&dyn InventoryContext>,
) -> Result<bool, CustomToolRunError> {
    for (key, operand) in operands {
        let Some(operand) = operand else { continue };
        let label = format!("{subject_label} {key}");

        if matches!(key, "gt" | "gte" | "lt" | "lte") {
            // v4 evaluates the SUBJECT first, so a non-orderable subject is what
            // is reported even when the operand is also broken.
            let a = require_number(tool_name, subject, subject_label)?;
            let resolved = resolve_operand(tool_name, &operand, params, &label, state, inventory)?;
            let b = require_number(tool_name, &resolved, &label)?;
            let held = match key {
                "gt" => a > b,
                "gte" => a >= b,
                "lt" => a < b,
                _ => a <= b,
            };
            if !held {
                return Ok(false);
            }
            continue;
        }

        if matches!(key, "contains" | "ncontains") {
            // Containment is strict and case-sensitive here, matching eq's
            // exactness on declared values; the forgiving variant lives with the
            // LLM subject, whose text is a model's prose rather than an author's
            // literal. v4 evaluates the SUBJECT first (it is the receiver of
            // `.includes`), so a non-string subject is reported even when the
            // operand is also broken.
            let haystack = require_string(tool_name, subject, subject_label)?;
            let resolved = resolve_operand(tool_name, &operand, params, &label, state, inventory)?;
            let needle = require_string(tool_name, &resolved, &label)?;
            let held = haystack.contains(&needle);
            if key == "contains" {
                if !held {
                    return Ok(false);
                }
            } else if held {
                return Ok(false);
            }
            continue;
        }

        let resolved = resolve_operand(tool_name, &operand, params, &label, state, inventory)?;
        let held = if key == "eq" {
            subject.strict_eq(&resolved)
        } else {
            !subject.strict_eq(&resolved)
        };
        if !held {
            return Ok(false);
        }
    }

    Ok(true)
}

fn string_operand(v: &StringOperand) -> OperandSpec<'_> {
    match v {
        StringOperand::String(s) => OperandSpec::Literal(ResolvedValue::String(s.clone())),
        StringOperand::ParamRef(r) => OperandSpec::Ref(&r.param),
        StringOperand::StateRef(r) => OperandSpec::State(r),
        StringOperand::InventoryRef(r) => OperandSpec::Inventory(r),
    }
}

fn numeric_operands(c: &NumericComparator) -> [(&'static str, Option<OperandSpec<'_>>); 8] {
    [
        ("gt", c.gt.as_ref().map(number_operand)),
        ("gte", c.gte.as_ref().map(number_operand)),
        ("lt", c.lt.as_ref().map(number_operand)),
        ("lte", c.lte.as_ref().map(number_operand)),
        ("eq", c.eq.as_ref().map(number_operand)),
        ("neq", c.neq.as_ref().map(number_operand)),
        // A numeric subject holds no substrings, so its schema carries neither key.
        ("contains", None),
        ("ncontains", None),
    ]
}

fn when_operands(w: &WhenObject) -> [(&'static str, Option<OperandSpec<'_>>); 8] {
    [
        ("gt", w.gt.as_ref().map(number_operand)),
        ("gte", w.gte.as_ref().map(number_operand)),
        ("lt", w.lt.as_ref().map(number_operand)),
        ("lte", w.lte.as_ref().map(number_operand)),
        ("eq", w.eq.as_ref().map(number_operand)),
        ("neq", w.neq.as_ref().map(number_operand)),
        ("contains", None),
        ("ncontains", None),
    ]
}

fn param_operands(c: &ParamComparator) -> [(&'static str, Option<OperandSpec<'_>>); 8] {
    [
        ("gt", c.gt.as_ref().map(number_operand)),
        ("gte", c.gte.as_ref().map(number_operand)),
        ("lt", c.lt.as_ref().map(number_operand)),
        ("lte", c.lte.as_ref().map(number_operand)),
        ("eq", c.eq.as_ref().map(any_operand)),
        ("neq", c.neq.as_ref().map(any_operand)),
        ("contains", c.contains.as_ref().map(string_operand)),
        ("ncontains", c.ncontains.as_ref().map(string_operand)),
    ]
}

fn llm_operands(c: &LlmComparator) -> [(&'static str, Option<OperandSpec<'_>>); 8] {
    [
        ("gt", c.gt.as_ref().map(number_operand)),
        ("gte", c.gte.as_ref().map(number_operand)),
        ("lt", c.lt.as_ref().map(number_operand)),
        ("lte", c.lte.as_ref().map(number_operand)),
        ("eq", c.eq.as_ref().map(any_operand)),
        ("neq", c.neq.as_ref().map(any_operand)),
        ("contains", c.contains.as_ref().map(string_operand)),
        ("ncontains", c.ncontains.as_ref().map(string_operand)),
    ]
}

/// Evaluate one comparator against one metadata key — the fail-soft twin of
/// [`matches_comparator`].
///
/// The semantics live in [`metadata_comparator_holds`], shared verbatim with the
/// availability gate so the two can never drift; what this wrapper adds is the
/// two things the shared table cannot know about — how to resolve a
/// `$param`/`$state` operand, and where to log a declined row.
///
/// `$param` operands still throw if they don't resolve: those ARE load-validated,
/// so a failure there is a regression rather than a fact about the character.
#[allow(clippy::too_many_arguments)]
fn matches_metadata_comparator(
    tool_name: &str,
    comparator: &MetadataComparator,
    key: &str,
    sheet: &Map<String, Value>,
    params: &ResolvedParams,
    state: Option<&Value>,
    // Which sheet is being read, for the operand label and the debug log.
    subject: ComparatorSubject,
    inventory: Option<&dyn InventoryContext>,
) -> Result<bool, CustomToolRunError> {
    let operands = param_operands(comparator);
    let subject = subject.as_str();
    metadata_comparator_holds(
        comparator,
        key,
        sheet,
        &mut |comparator_key| {
            let operand = operands
                .iter()
                .find(|(k, _)| *k == comparator_key)
                .and_then(|(_, o)| o.as_ref())
                .expect("the shared table only resolves operands it found present");
            resolve_operand(
                tool_name,
                operand,
                params,
                &format!("{subject} \"{key}\" {comparator_key}"),
                state,
                inventory,
            )
        },
        // v4's `logger.debug(`Custom tool ${subject} test did not match`, {…})`,
        // at the same point with the same fields (the P4.18 tracing surface, per
        // the `llm_consult` precedent: v4's `context` becomes the target). The
        // message is TEMPLATED on both sides — v4 interpolates the subject, and
        // `tracing`'s format-args form renders the same bytes.
        &mut |reason| {
            tracing::debug!(
                target: "quilltap::pascal",
                tool = tool_name,
                key,
                reason,
                "Custom tool {subject} test did not match",
            );
        },
    )
}

/// Which sheet a [`matches_metadata_comparator`] call is posed against. v4
/// passes the string literal `'metadata' | 'progress'` and defaults it to
/// `'metadata'`; v5 makes the two callers name it, because a default that only
/// one call site overrides is a silent trap the next reader has to go find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComparatorSubject {
    Metadata,
    Progress,
}

impl ComparatorSubject {
    /// v4's literal — the bytes that reach the operand label and the log line.
    fn as_str(self) -> &'static str {
        match self {
            ComparatorSubject::Metadata => "metadata",
            ComparatorSubject::Progress => "progress",
        }
    }
}

/// Evaluate one comparator against the LLM consult — the second fail-soft
/// evaluator, for the same reason as [`matches_metadata_comparator`]: the
/// subject's run-time type is unknowable at load time, because the answer is
/// whatever the model chose to say.
///
/// Reconciliation rules, chosen so an author testing an oracle they prompted
/// for "YES", "42", or "the west door" gets the match they meant:
///
/// - `ok` tests the consult's success flag directly.
/// - Ordering comparators need the answer to parse as a finite number; an
///   answer that doesn't simply declines the row.
/// - eq/neq compare numerically when both sides are numbers, and otherwise as
///   trimmed, case-insensitive strings — a model that says "yes." instead of
///   "YES" has still said yes. (A trailing `.` or `!` is forgiven for that
///   reason, and NOTHING else is.)
/// - contains/ncontains search the answer for the operand under that same
///   trimmed, case-insensitive reconciliation, with NO punctuation forgiveness.
///
/// `$param` operands still throw when they fail to resolve: those are
/// load-validated, so a failure is a regression, not a fact about the model.
fn matches_llm_comparator(
    tool_name: &str,
    comparator: &LlmComparator,
    llm: &LlmSubject,
    params: &ResolvedParams,
    state: Option<&Value>,
    inventory: Option<&dyn InventoryContext>,
) -> Result<bool, CustomToolRunError> {
    if let Some(want) = comparator.ok {
        if want != llm.ok {
            return Ok(false);
        }
    }

    let answer = js_trim(&llm.output).to_string();
    // v4: `answer !== '' && Number.isFinite(Number(answer))` — the empty string
    // is excluded because `Number('')` is 0, not NaN.
    let numeric_answer = if answer.is_empty() {
        None
    } else {
        let n = to_number(&Value::String(answer.clone()));
        n.is_finite().then_some(n)
    };

    let slots = llm_operands(comparator);

    // The ordering loop first, exactly as v4 writes it: the operand resolves
    // BEFORE the numeric check, so an unresolvable `$param` still throws.
    for (key, operand) in &slots {
        if !matches!(*key, "gt" | "gte" | "lt" | "lte") {
            continue;
        }
        let Some(operand) = operand else { continue };
        let label = format!("the llm answer {key}");
        let resolved = resolve_operand(tool_name, operand, params, &label, state, inventory)?;
        let (Some(a), ResolvedValue::Number(b)) = (numeric_answer, &resolved) else {
            return Ok(false);
        };
        let held = match *key {
            "gt" => a > *b,
            "gte" => a >= *b,
            "lt" => a < *b,
            _ => a <= *b,
        };
        if !held {
            return Ok(false);
        }
    }

    let equals_answer = |operand: &ResolvedValue| -> bool {
        if let (ResolvedValue::Number(n), Some(a)) = (operand, numeric_answer) {
            return a == *n;
        }
        let operand_text = js_trim(&resolved_to_js_string(operand)).to_lowercase();
        let answer_text = answer.to_lowercase();
        answer_text == operand_text
            || answer_text == format!("{operand_text}.")
            || answer_text == format!("{operand_text}!")
    };

    // Containment under eq's reconciliation: trimmed, case-insensitive, and the
    // operand stringified — an author hunting "west door" should find "the West
    // Door", because the subject here is a model's prose, not a declared value.
    let contains_answer = |operand: &ResolvedValue| -> bool {
        answer.to_lowercase().contains(
            js_trim(&resolved_to_js_string(operand))
                .to_lowercase()
                .as_str(),
        )
    };

    for (key, operand) in &slots {
        let Some(operand) = operand else { continue };
        let label = format!("the llm answer {key}");
        let held = match *key {
            "eq" => {
                let r = resolve_operand(tool_name, operand, params, &label, state, inventory)?;
                equals_answer(&r)
            }
            "neq" => {
                let r = resolve_operand(tool_name, operand, params, &label, state, inventory)?;
                !equals_answer(&r)
            }
            "contains" => {
                let r = resolve_operand(tool_name, operand, params, &label, state, inventory)?;
                contains_answer(&r)
            }
            "ncontains" => {
                let r = resolve_operand(tool_name, operand, params, &label, state, inventory)?;
                !contains_answer(&r)
            }
            _ => continue,
        };
        if !held {
            return Ok(false);
        }
    }

    Ok(true)
}

/// JS `String(operand)` over the three types a resolved operand can hold.
fn resolved_to_js_string(v: &ResolvedValue) -> String {
    match v {
        ResolvedValue::Number(n) => number_to_string(*n),
        ResolvedValue::String(s) => s.clone(),
        ResolvedValue::Bool(b) => b.to_string(),
    }
}

/// The metadata a winning outcome consulted, for the roll record — only the keys
/// that row actually tested, and only their primitive values.
///
/// A row whose metadata comparator declined (absent key, non-primitive value,
/// wrong type) is precisely a row that did not win, so recording every primitive
/// key it tested records exactly what the table saw, not the whole sheet.
///
/// Across every group of an any-of: a key some group consulted was consulted
/// (v4 `a53024d3c`) — so a key from a group that did NOT hold is recorded too.
/// A repeated key keeps its first position, as a JS object assignment does.
pub fn collect_metadata_tested(
    when: &When,
    metadata: Option<&Map<String, Value>>,
) -> Option<MetadataTested> {
    let metadata = metadata?;

    let mut tested: Vec<(String, ResolvedValue)> = Vec::new();
    for group in when.when_groups() {
        for (key, _comparator) in group.metadata.iter().flatten() {
            let Some(prim) = metadata.get(key).and_then(js_primitive) else {
                continue;
            };
            match tested.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = prim,
                None => tested.push((key.clone(), prim)),
            }
        }
    }
    if tested.is_empty() {
        None
    } else {
        Some(tested)
    }
}

/// Evaluate an outcome test. Every subject named must hold — bare comparators
/// against the final value, `roll` against the raw draw, `params` against what
/// the caller supplied, `metadata` against the invoking character's fact sheet.
pub fn matches_when(
    when: &When,
    subjects: &OutcomeSubjects,
    tool_name: &str,
) -> Result<bool, CustomToolRunError> {
    match when {
        When::CatchAll(_) => Ok(true),
        When::Object(w) => matches_when_object(w, subjects, tool_name),
        When::AnyOf(groups) => any_group_holds(
            groups,
            |group| matches_when_object(group, subjects, tool_name),
            tool_name,
        ),
    }
}

/// An any-of: the first group that holds wins, left to right (v4
/// `anyGroupHolds`, `a53024d3c`). A group's own subject tests short-circuit
/// inside as before, and their DEBUG lines fire inside the walk — BEFORE this
/// one line, which is logged for EVERY array evaluation, a miss (`heldGroup:
/// null`) included. A failing group (a `$param` regression) fails the run, as
/// v4's `findIndex` lets a throw through.
fn any_group_holds<G>(
    groups: &[G],
    mut holds: impl FnMut(&G) -> Result<bool, CustomToolRunError>,
    tool_name: &str,
) -> Result<bool, CustomToolRunError> {
    let mut held_group = None;
    for (g, group) in groups.iter().enumerate() {
        if holds(group)? {
            held_group = Some(g);
            break;
        }
    }
    tracing::debug!(
        target: "quilltap::pascal",
        context = "pascal.custom-tools",
        tool = tool_name,
        groups = groups.len(),
        heldGroup = %held_group.map_or_else(|| "null".to_string(), |g| g.to_string()),
        "Custom tool any-of test",
    );
    Ok(held_group.is_some())
}

/// The object form's body. Split out so an effect condition can delegate to it
/// after peeling off its one extra subject — v4's `matchesEffectWhen` does the
/// same thing by destructuring `outcome` off and passing the rest to
/// `matchesWhen`, which is a free operation in JS and a clone here.
fn matches_when_object(
    when: &WhenObject,
    subjects: &OutcomeSubjects,
    tool_name: &str,
) -> Result<bool, CustomToolRunError> {
    matches_when_object_with(when, subjects, tool_name, None)
}

/// [`matches_when_object`] with an effect condition's extra template subjects
/// (`{{dice}}`, `{{now}}` — P4.D329: an inventory key renders against them).
fn matches_when_object_with(
    when: &WhenObject,
    subjects: &OutcomeSubjects,
    tool_name: &str,
    effect: Option<(&str, i64, &WinningOutcome)>,
) -> Result<bool, CustomToolRunError> {
    let params = subjects.params;
    // v4 `subjects.state ?? {}` — `None` behaves as the empty object, so every
    // `$state` operand falls to its own fallback.
    let state = subjects.state;
    let inventory = subjects.inventory;
    let value = ResolvedValue::Number(subjects.value);

    if !matches_comparator(
        tool_name,
        when_operands(when),
        &value,
        "the rolled value",
        params,
        state,
        inventory,
    )? {
        return Ok(false);
    }

    if let Some(roll) = &when.roll {
        let raw = ResolvedValue::Number(subjects.roll);
        if !matches_comparator(
            tool_name,
            numeric_operands(roll),
            &raw,
            "the raw roll",
            params,
            state,
            inventory,
        )? {
            return Ok(false);
        }
    }

    for (name, comparator) in when.params.iter().flatten() {
        let Some(subject) = lookup(params, name) else {
            // Load-time validation rejects a test of an undeclared parameter.
            return Err(err(format!(
                "{tool_name}: an outcome tests \"{name}\", which is not a declared parameter"
            )));
        };
        let label = format!("parameter \"{name}\"");
        if !matches_comparator(
            tool_name,
            param_operands(comparator),
            subject,
            &label,
            params,
            state,
            inventory,
        )? {
            return Ok(false);
        }
    }

    // `metadata` against the invoking character's fact sheet — `None` behaves as
    // v4's `subjects.metadata ?? {}`, so every key is absent and declines.
    if let Some(entries) = when.metadata.as_ref() {
        let empty = Map::new();
        let metadata = subjects.metadata.unwrap_or(&empty);
        for (key, comparator) in entries {
            if !matches_metadata_comparator(
                tool_name,
                comparator,
                key,
                metadata,
                params,
                state,
                ComparatorSubject::Metadata,
                inventory,
            )? {
                return Ok(false);
            }
        }
    }

    // The derived progress sheet reads through the SAME fail-soft table as
    // metadata — one semantics, no second comparison code — differing only in
    // which sheet it looks in and what it calls the subject in the log.
    if let Some(entries) = when.progress.as_ref() {
        let empty = Map::new();
        let progress = subjects.progress.unwrap_or(&empty);
        for (key, comparator) in entries {
            if !matches_metadata_comparator(
                tool_name,
                comparator,
                key,
                progress,
                params,
                state,
                ComparatorSubject::Progress,
                inventory,
            )? {
                return Ok(false);
            }
        }
    }

    // === P4.D329: the inventory subjects ===
    if !matches_inventory_subjects_with(when, subjects, tool_name, effect)? {
        return Ok(false);
    }

    if let Some(comparator) = &when.llm {
        // No consult ran (a definition without an `llm` block, or a simulation
        // that supplied none): the test fails soft, exactly like a metadata key
        // the character doesn't carry.
        let Some(llm) = subjects.llm else {
            return Ok(false);
        };
        if !matches_llm_comparator(tool_name, comparator, llm, params, state, inventory)? {
            return Ok(false);
        }
    }

    Ok(true)
}

// === P4.D329: the inventory subjects in a `when` ===
/// The inventory half of a `when` (v4 `matchesWhen`, `custom-tools.ts:1304-1381`
/// at the pin): `inventory` and `inventoryCount`, then `inventoryAggregate` —
/// after `progress`, before `llm`.
///
/// Each `inventory` / `inventoryCount` key is a TEMPLATE, rendered against the
/// run's subjects first (`{{params.weapon}}`), then resolved through the one
/// reference rule. Fail-soft like `metadata`: with no context every comparator
/// declines. `effect` carries an effect condition's extra template subjects
/// (`{{dice}}`, `{{now}}`); an outcome row renders `{{dice}}` as `""` and has
/// no run clock (v4 `{ ...subjects, dice: subjects.dice ?? '' }`).
fn matches_inventory_subjects_with(
    when: &WhenObject,
    subjects: &OutcomeSubjects,
    tool_name: &str,
    effect: Option<(&str, i64, &WinningOutcome)>,
) -> Result<bool, CustomToolRunError> {
    let params = subjects.params;
    let state = subjects.state;
    let inventory = subjects.inventory;
    let states = when.inventory.as_deref().unwrap_or_default();
    let counts = when.inventory_count.as_deref().unwrap_or_default();

    if !states.is_empty() || !counts.is_empty() {
        let render_vars = TemplateVars {
            value: subjects.value,
            roll: subjects.roll,
            dice: effect.map_or("", |(dice, _, _)| dice),
            params,
            metadata: subjects.metadata,
            progress: subjects.progress,
            now: effect.map(|(_, now, _)| now),
            llm: subjects.llm,
            state,
            character: subjects.character,
            inventory,
            // An effect condition's `{{outcome.*}}` (v4 spreads the whole
            // EffectSubjects); an outcome row has no winner yet.
            outcome: effect.map(|(_, _, outcome)| outcome),
        };
        let decline = |key: &str, reason: &str| {
            tracing::debug!(
                target: "quilltap::pascal",
                context = "pascal.custom-tools",
                tool = tool_name,
                key,
                reason,
                "Custom tool inventory test did not match",
            );
        };

        for (raw_key, comparator) in states {
            let reference = js_trim(&render_template(raw_key, &render_vars)).to_string();
            let operand_for = |op: &str,
                               operand: &Option<InventoryStateOperand>|
             -> Result<Option<String>, CustomToolRunError> {
                let Some(operand) = operand else {
                    return Ok(None);
                };
                let resolved = match operand {
                    InventoryStateOperand::State(s) => {
                        ResolvedValue::String(s.as_str().to_string())
                    }
                    InventoryStateOperand::ParamRef(r) => resolve_operand(
                        tool_name,
                        &OperandSpec::Ref(&r.param),
                        params,
                        &format!("inventory \"{raw_key}\" {op}"),
                        state,
                        inventory,
                    )?,
                };
                // v4 `String(resolveOperand(…))`.
                Ok(Some(resolved_to_js_string(&resolved)))
            };
            let test = ResolvedInventoryStateTest {
                eq: operand_for("eq", &comparator.eq)?,
                neq: operand_for("neq", &comparator.neq)?,
            };
            let holds = inventory_state_holds(
                &test,
                &reference,
                inventory,
                Some(&mut |reason: &str| decline(raw_key, reason)),
            );
            let state_word = inventory.map(|ctx| ctx.state(&reference).as_str());
            tracing::debug!(
                target: "quilltap::pascal",
                context = "pascal.custom-tools",
                tool = tool_name,
                key = raw_key.as_str(),
                r#ref = reference.as_str(),
                state = state_word,
                holds,
                "Custom tool inventory state test",
            );
            if !holds {
                return Ok(false);
            }
        }

        for (raw_key, comparator) in counts {
            let reference = js_trim(&render_template(raw_key, &render_vars)).to_string();
            let mut test = ResolvedInventoryCountTest::default();
            for (op, operand, slot) in [
                ("gt", &comparator.gt, &mut test.gt),
                ("gte", &comparator.gte, &mut test.gte),
                ("lt", &comparator.lt, &mut test.lt),
                ("lte", &comparator.lte, &mut test.lte),
                ("eq", &comparator.eq, &mut test.eq),
                ("neq", &comparator.neq, &mut test.neq),
            ] {
                let Some(operand) = operand else { continue };
                let label = format!("inventoryCount \"{raw_key}\" {op}");
                let resolved = resolve_operand(
                    tool_name,
                    &number_operand(operand),
                    params,
                    &label,
                    state,
                    inventory,
                )?;
                *slot = Some(require_number(tool_name, &resolved, &label)?);
            }
            let holds = inventory_count_holds(
                &test,
                &reference,
                inventory,
                Some(&mut |reason: &str| decline(raw_key, reason)),
            );
            let count = inventory.map(|ctx| ctx.equipped_kin_count(&reference));
            tracing::debug!(
                target: "quilltap::pascal",
                context = "pascal.custom-tools",
                tool = tool_name,
                key = raw_key.as_str(),
                r#ref = reference.as_str(),
                count,
                holds,
                "Custom tool inventory count test",
            );
            if !holds {
                return Ok(false);
            }
        }
    }

    // The aggregate sheet: one attribute across a scope of the bag, through the
    // same comparison table as `metadata`. The key is literal (it names a scope
    // and a field, never an object); operands resolve like any other.
    for (key, comparator) in when.inventory_aggregate.iter().flatten() {
        let operands = param_operands(comparator);
        let holds = inventory_aggregate_holds(
            comparator,
            key,
            inventory,
            &mut |comparator_key| {
                let operand = operands
                    .iter()
                    .find(|(k, _)| *k == comparator_key)
                    .and_then(|(_, o)| o.as_ref())
                    .expect("the shared table only resolves operands it found present");
                resolve_operand(
                    tool_name,
                    operand,
                    params,
                    &format!("inventoryAggregate \"{key}\" {comparator_key}"),
                    state,
                    inventory,
                )
            },
            &mut |reason| {
                tracing::debug!(
                    target: "quilltap::pascal",
                    context = "pascal.custom-tools",
                    tool = tool_name,
                    key = key.as_str(),
                    reason,
                    "Custom tool inventoryAggregate test did not match",
                );
            },
        )?;
        // v4's `value`: `null` (no field), `'absent'`, `list of N`, or the
        // number — a scalar, logged bare.
        let value = describe_aggregate_value(key, inventory).map(|v| scalar_text(&v.to_value()));
        tracing::debug!(
            target: "quilltap::pascal",
            context = "pascal.custom-tools",
            tool = tool_name,
            key = key.as_str(),
            value,
            holds,
            "Custom tool inventoryAggregate test",
        );
        if !holds {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The winning outcome, as an effect condition and a template see it — the one
/// subject that exists only after the deal.
///
/// Serialized in v4's key order (`state`, `index`, then `brief` only when the
/// row carries one) — the shape v4's spread builds. It is an INTERNAL subject:
/// nothing puts it on the run result (bug 257 moved no wire key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WinningOutcome {
    pub state: OutcomeState,
    pub index: usize,
    /// The winner's `brief`, rendered ONCE for the run (v4 `322a5488e`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
}

/// What an effect condition may be posed about: the run's subjects, plus the
/// winning outcome (v4 `EffectSubjects extends OutcomeSubjects`).
pub struct EffectSubjects<'a> {
    pub base: OutcomeSubjects<'a>,
    pub outcome: WinningOutcome,
    /// Dice breakdown for `{{dice}}` in expressions — a string, `""` for Form A.
    pub dice: &'a str,
    /// Epoch milliseconds at run start, for `{{now}}` in an effect expression —
    /// the idiom `{{now}} + 600000` re-arms a countdown ten minutes out without
    /// the format needing a date grammar.
    pub now: i64,
}

/// Evaluate an effect's condition. Delegates the shared subjects to the same
/// comparator chain outcome rows use, and adds the one subject only an effect
/// can test — the winning outcome's semantic state.
pub fn matches_effect_when(
    when: Option<&EffectCondition>,
    subjects: &EffectSubjects,
    tool_name: &str,
) -> Result<bool, CustomToolRunError> {
    match when {
        None => Ok(true),
        Some(EffectCondition::Group(group)) => {
            matches_effect_when_group(group, subjects, tool_name)
        }
        Some(EffectCondition::AnyOf(groups)) => any_group_holds(
            groups,
            |group| matches_effect_when_group(group, subjects, tool_name),
            tool_name,
        ),
    }
}

/// One group of an effect condition: the `outcome` test, then the shared
/// subjects. An outcome-only group has no shared subjects left — it holds on
/// the outcome alone, which `matches_when_object` answers for an empty group.
fn matches_effect_when_group(
    when: &EffectWhen,
    subjects: &EffectSubjects,
    tool_name: &str,
) -> Result<bool, CustomToolRunError> {
    if let Some(outcome) = &when.outcome {
        if let Some(eq) = outcome.eq {
            if subjects.outcome.state != eq {
                return Ok(false);
            }
        }
        if let Some(neq) = outcome.neq {
            if subjects.outcome.state == neq {
                return Ok(false);
            }
        }
    }

    // P4.D329: an effect condition's inventory keys render with `{{dice}}` and
    // `{{now}}` too (v4 hands `matchesWhen` the whole EffectSubjects).
    matches_when_object_with(
        &when.base,
        &subjects.base,
        tool_name,
        Some((subjects.dice, subjects.now, &subjects.outcome)),
    )
}

/// One effect, resolved by the pure core: the value it would write, the
/// wardrobe gesture it would make, or the reason it was skipped. The entrances
/// (and only they) apply the first two; the Proving Bench shows all three as a
/// dry run.
///
/// Serialization is payload — the Workbench preview body spreads the whole run
/// result — so the key order below is v4's object literals, not a convenience.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedEffect {
    Applicable {
        index: usize,
        target: EffectTarget,
        value: ResolvedValue,
    },
    /// v4's middle arm `{ index, wardrobe }` (`f0db8cbb8`).
    Wardrobe {
        index: usize,
        wardrobe: ResolvedWardrobeGesture,
    },
    // === P4.D329: the inventory gesture (v4's THIRD arm) ===
    /// v4 `{ index, inventory }`.
    Inventory {
        index: usize,
        inventory: ResolvedInventoryGesture,
    },
    // === P4.D331: the log line ===
    /// v4's `{ index, log }` arm (`e4a60ad9e`) — the rendered, trimmed line.
    Log {
        index: usize,
        log: String,
    },
    Skipped {
        index: usize,
        reason: String,
    },
}

/// A wardrobe gesture with its item references rendered — ready for the
/// applier (v4 `ResolvedWardrobeGesture`).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedWardrobeGesture {
    pub action: WardrobeEffectAction,
    /// Rendered, trimmed item references in file order. Empty strings are
    /// dropped here, with a debug log.
    pub items: Vec<String>,
    pub mode: Option<PutOnMode>,
    pub slot: Option<String>,
}

impl Serialize for ResolvedWardrobeGesture {
    /// `{ action, items, mode?, slot? }` — `items` ALWAYS present (`[]` on a
    /// `clear_slot`), `mode` / `slot` only when defined.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("action", self.action.as_str())?;
        m.serialize_entry("items", &self.items)?;
        if let Some(mode) = self.mode {
            m.serialize_entry("mode", mode.as_str())?;
        }
        if let Some(slot) = &self.slot {
            m.serialize_entry("slot", slot)?;
        }
        m.end()
    }
}

// === P4.D329: the inventory gesture ===
/// An Inventory gesture with its object references rendered — ready for the
/// applier (v4 `ResolvedInventoryGesture`).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInventoryGesture {
    pub action: InventoryEffectAction,
    /// Rendered, trimmed references in file order; empty ones dropped with a
    /// debug log.
    pub items: Vec<String>,
    /// pick_up only: equip the new instances (default true). `None` on
    /// equip / unequip, so the key is absent there.
    pub equip: Option<bool>,
}

impl Serialize for ResolvedInventoryGesture {
    /// `{ action, items }`, or `{ action, items, equip }` on a `pick_up`.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("action", self.action.as_str())?;
        m.serialize_entry("items", &self.items)?;
        if let Some(equip) = self.equip {
            m.serialize_entry("equip", &equip)?;
        }
        m.end()
    }
}

impl ResolvedEffect {
    /// True for a resolved effect of any kind that would apply — the Proving
    /// Bench's count (v4 `isApplicableEffect`, widened by each kind).
    pub fn is_applicable(&self) -> bool {
        self.is_applicable_write()
            || self.is_applicable_wardrobe()
            || self.is_applicable_inventory()
            || self.is_applicable_log()
    }

    /// True for a resolved effect that would change what the rolling
    /// character carries (v4 `isApplicableInventoryEffect`, P4.D329).
    pub fn is_applicable_inventory(&self) -> bool {
        matches!(self, ResolvedEffect::Inventory { .. })
    }


    /// True for a resolved effect that would write a line to the log (v4
    /// `isApplicableLogEffect`, P4.D331).
    pub fn is_applicable_log(&self) -> bool {
        matches!(self, ResolvedEffect::Log { .. })
    }

    /// True for a resolved effect that would actually write a value (v4
    /// `isApplicableWriteEffect`).
    pub fn is_applicable_write(&self) -> bool {
        matches!(self, ResolvedEffect::Applicable { .. })
    }

    /// True for a resolved effect that would actually change an outfit (v4
    /// `isApplicableWardrobeEffect`).
    pub fn is_applicable_wardrobe(&self) -> bool {
        matches!(self, ResolvedEffect::Wardrobe { .. })
    }
}

impl Serialize for ResolvedEffect {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        match self {
            ResolvedEffect::Applicable {
                index,
                target,
                value,
            } => {
                m.serialize_entry("index", index)?;
                m.serialize_entry("target", &effect_target_to_value(target))?;
                m.serialize_entry("value", &value.to_value())?;
            }
            ResolvedEffect::Wardrobe { index, wardrobe } => {
                m.serialize_entry("index", index)?;
                m.serialize_entry("wardrobe", wardrobe)?;
            }
            // === P4.D329 ===
            ResolvedEffect::Inventory { index, inventory } => {
                m.serialize_entry("index", index)?;
                m.serialize_entry("inventory", inventory)?;
            }
            // === P4.D331: the log line ===
            ResolvedEffect::Log { index, log } => {
                m.serialize_entry("index", index)?;
                m.serialize_entry("log", log)?;
            }
            ResolvedEffect::Skipped { index, reason } => {
                m.serialize_entry("index", index)?;
                m.serialize_entry("skipped", reason)?;
            }
        }
        m.end()
    }
}

/// v4's `EffectTarget` object literals: `{ kind: 'state', path, raw }`,
/// `{ kind: 'metadata', key, raw }` and `{ kind: 'progress', id, field, raw }`,
/// each in that key order. A path segment is a string or a number, exactly as
/// `parsePath` produces it.
fn effect_target_to_value(target: &EffectTarget) -> Value {
    let mut m = Map::new();
    match target {
        EffectTarget::State { path, raw } => {
            m.insert("kind".into(), Value::String("state".into()));
            m.insert(
                "path".into(),
                Value::Array(
                    path.iter()
                        .map(|k| match k {
                            PathKey::Prop(s) => Value::String(s.clone()),
                            PathKey::Index(i) => Value::from(*i),
                        })
                        .collect(),
                ),
            );
            m.insert("raw".into(), Value::String(raw.clone()));
        }
        EffectTarget::Metadata { key, raw } => {
            m.insert("kind".into(), Value::String("metadata".into()));
            m.insert("key".into(), Value::String(key.clone()));
            m.insert("raw".into(), Value::String(raw.clone()));
        }
        EffectTarget::Progress { id, field, raw } => {
            m.insert("kind".into(), Value::String("progress".into()));
            m.insert("id".into(), Value::String(id.clone()));
            m.insert("field".into(), Value::String(field.clone()));
            m.insert("raw".into(), Value::String(raw.clone()));
        }
    }
    Value::Object(m)
}

/// Resolve a definition's effects against a finished run. Pure — nothing is
/// written here; the module's "no writes, no message posting" contract holds.
///
/// Failure semantics are the [`render_template`] doctrine: a condition that does
/// not hold, or an expression that fails to evaluate (a division by zero, a
/// reference that resolves to nothing), skips THAT effect with a debug log and
/// a recorded reason — a broken effect never sinks a roll. Parse failures are
/// load-time rejections and only reach here as regressions, handled the same
/// fail-soft way.
fn resolve_effects(definition: &QtapCustomTool, subjects: &EffectSubjects) -> Vec<ResolvedEffect> {
    // Mirrors `render_template`'s lookups exactly — the grammar admits no name
    // the template does not already substitute. v4 `0506517d3` made that literal
    // rather than a promise: both readers now classify with
    // `classifyPlaceholder` and look the value up with `resolvePlaceholderValue`.
    let template_vars = TemplateVars {
        value: subjects.base.value,
        roll: subjects.base.roll,
        dice: subjects.dice,
        params: subjects.base.params,
        metadata: subjects.base.metadata,
        progress: subjects.base.progress,
        // An effect ALWAYS has a run clock — `EffectSubjects.now` is not
        // optional, which is what makes `{{now}} + 600000` the re-arming idiom.
        now: Some(subjects.now),
        llm: subjects.base.llm,
        state: subjects.base.state,
        // v4 `EffectSubjects` spreads the run's subjects, so an expression and
        // a wardrobe ref see the rolling character's name too.
        character: subjects.base.character,
        // P4.D329: and the bag, for `{{inventory.*}}`.
        inventory: subjects.base.inventory,
        // The winning row, brief included — `{{outcome.*}}` in an expression
        // or a gesture's reference (v4 `322a5488e`).
        outcome: Some(&subjects.outcome),
    };
    let mut resolve_ref = |refname: &str| -> Option<ResolvedValue> {
        resolve_placeholder_value(&classify_placeholder(refname), &template_vars)
    };

    let skipped = |index: usize, reason: String| -> ResolvedEffect {
        tracing::debug!(
            target: "quilltap::pascal",
            tool = definition.name,
            effect_index = index,
            reason,
            "Custom tool effect skipped",
        );
        ResolvedEffect::Skipped { index, reason }
    };

    definition
        .effects
        .iter()
        .flatten()
        .enumerate()
        .map(|(index, effect)| {
            let holds = match matches_effect_when(effect.when.as_ref(), subjects, &definition.name)
            {
                Ok(h) => h,
                // A comparator regression past load-time validation. An outcome
                // row in this state fails the run; an effect never does — it
                // just doesn't fire.
                Err(e) => {
                    return skipped(index, format!("condition could not be evaluated: {}", e.0))
                }
            };
            if !holds {
                return skipped(index, "condition did not hold".to_string());
            }

            // v4 then guards `effect is neither a complete write, a wardrobe
            // or inventory gesture, nor a log line` (`custom-tools.ts:1560-1562`
            // at the pin — the union string, R-G); here that state is
            // unrepresentable (`CustomToolEffectBody`), so there is no arm.
            let (target, value) = match &effect.body {
                CustomToolEffectBody::Wardrobe(gesture) => {
                    return resolve_wardrobe_gesture(
                        &definition.name,
                        index,
                        gesture,
                        &template_vars,
                        &skipped,
                    )
                }
                // === P4.D329: after the wardrobe arm, before the write ===
                CustomToolEffectBody::Inventory(gesture) => {
                    return resolve_inventory_gesture(
                        &definition.name,
                        index,
                        gesture,
                        &template_vars,
                        &skipped,
                    )
                }
                // === P4.D331: the log line ===
                // A template, like an outcome message — not an expression. The
                // SAME renderer and vars (an unknown placeholder stays as
                // written), then JS-trimmed; an empty line is skipped.
                CustomToolEffectBody::Log(template) => {
                    let line = js_trim(&render_template(template, &template_vars)).to_string();
                    return if line.is_empty() {
                        skipped(index, "log line rendered empty".to_string())
                    } else {
                        ResolvedEffect::Log { index, log: line }
                    };
                }
                CustomToolEffectBody::Write { target, value } => (target, value),
            };

            let target = match parse_effect_target(target) {
                Ok(t) => t,
                Err(reason) => return skipped(index, format!("target {reason}")),
            };

            let EffectValue::Str(source) = value else {
                let value = match value {
                    EffectValue::Number(n) => ResolvedValue::Number(*n),
                    EffectValue::Bool(b) => ResolvedValue::Bool(*b),
                    EffectValue::Str(_) => unreachable!("handled by the let-else"),
                };
                return ResolvedEffect::Applicable {
                    index,
                    target,
                    value,
                };
            };

            let parsed = match parse_expression(source) {
                Ok(p) => p,
                Err(reason) => {
                    return skipped(index, format!("expression did not parse: {reason}"))
                }
            };
            match evaluate_expression(&parsed, &mut resolve_ref) {
                Ok(value) => ResolvedEffect::Applicable {
                    index,
                    target,
                    value,
                },
                Err(reason) => skipped(index, format!("expression did not evaluate: {reason}")),
            }
        })
        .collect()
}

/// Render a wardrobe gesture's item references with the same subjects an
/// effect expression gets (v4 `resolveWardrobeGesture`). Templates, not
/// expressions: a reference that renders empty is dropped (a `wear` or
/// `take_off` left with none is skipped), and one that keeps an unresolved
/// placeholder is passed on as written — it will simply fail to resolve in the
/// wardrobe, fail-soft, at the applier.
fn resolve_wardrobe_gesture(
    tool_name: &str,
    index: usize,
    gesture: &CustomToolWardrobeGesture,
    vars: &TemplateVars,
    skipped: &dyn Fn(usize, String) -> ResolvedEffect,
) -> ResolvedEffect {
    let mut items: Vec<String> = Vec::new();
    for raw in gesture.items.iter().flatten() {
        let rendered = js_trim(&render_template(raw, vars)).to_string();
        if rendered.is_empty() {
            // Keys follow this file's sibling `Custom tool effect skipped`
            // line (`effect_index`, snake_case — P4.D306 R-15).
            tracing::debug!(
                target: "quilltap::pascal",
                context = "pascal.custom-tools",
                tool = tool_name,
                effect_index = index,
                reference = raw.as_str(),
                "Custom tool wardrobe item reference rendered empty; dropped",
            );
            continue;
        }
        items.push(rendered);
    }

    if gesture.action != WardrobeEffectAction::ClearSlot && items.is_empty() {
        return skipped(index, "every item reference rendered empty".to_string());
    }

    ResolvedEffect::Wardrobe {
        index,
        wardrobe: ResolvedWardrobeGesture {
            action: gesture.action,
            items,
            mode: gesture.mode,
            slot: gesture.slot.clone(),
        },
    }
}

// === P4.D329: the inventory gesture ===
/// Render an Inventory gesture's object references with the same subjects an
/// outcome message gets — templates, not expressions (v4
/// `resolveInventoryGesture`). A reference that renders empty is dropped; a
/// gesture left with none is skipped with that reason.
fn resolve_inventory_gesture(
    tool_name: &str,
    index: usize,
    gesture: &CustomToolInventoryGesture,
    vars: &TemplateVars,
    skipped: &dyn Fn(usize, String) -> ResolvedEffect,
) -> ResolvedEffect {
    let mut items: Vec<String> = Vec::new();
    for raw in &gesture.items {
        let rendered = js_trim(&render_template(raw, vars)).to_string();
        if rendered.is_empty() {
            tracing::debug!(
                target: "quilltap::pascal",
                context = "pascal.custom-tools",
                tool = tool_name,
                effectIndex = index,
                reference = raw.as_str(),
                "Custom tool inventory reference rendered empty; dropped",
            );
            continue;
        }
        items.push(rendered);
    }
    if items.is_empty() {
        return skipped(index, "every object reference rendered empty".to_string());
    }
    ResolvedEffect::Inventory {
        index,
        inventory: ResolvedInventoryGesture {
            action: gesture.action,
            items,
            // `gesture.equip !== false`, on a pick_up only.
            equip: (gesture.action == InventoryEffectAction::PickUp)
                .then_some(gesture.equip != Some(false)),
        },
    }
}

/// Render a number for display: integers plain, floats to 4 significant digits.
///
/// Lives in [`super::expressions`] since v4 `c4d4b0de` moved it there — one
/// number-rendering convention for templates and effect expressions alike — and
/// is re-exported here, as v4 re-exports it, for every existing importer.
pub use super::expressions::format_value;

/// The substitutions [`render_template`] draws on.
#[derive(Clone)]
pub struct TemplateVars<'a> {
    pub value: f64,
    pub roll: f64,
    pub dice: &'a str,
    pub params: &'a ResolvedParams,
    pub metadata: Option<&'a Map<String, Value>>,
    /// The flattened progress sheet, for `{{progress.<id>.<field>}}`.
    pub progress: Option<&'a Map<String, Value>>,
    /// Epoch milliseconds at run start, for `{{now}}` — ONE value for the whole
    /// run, so two effects in the same file that both say `{{now}}` agree.
    /// `None` while rendering with no run clock (the placeholder stands).
    pub now: Option<i64>,
    /// The consult's result. `None` while rendering the consult's own prompt.
    pub llm: Option<&'a LlmSubject>,
    /// The merged persistent state, for `{{state.path}}` placeholders.
    pub state: Option<&'a Value>,
    /// The rolling character's name, for `{{character}}` (v4 `a05c6f755`).
    /// `None` when nobody in particular rolled (the placeholder stands).
    pub character: Option<&'a str>,
    // === P4.D329: the rolling character's bag (LAST) ===
    /// For `{{inventory.<ref>.<field>}}`. `None` for a run nobody made (the
    /// placeholder stands).
    pub inventory: Option<&'a dyn InventoryContext>,
    /// The winning outcome, for `{{outcome.state}}` / `{{outcome.index}}` /
    /// `{{outcome.brief}}` (v4 `322a5488e`). Present once the table has dealt:
    /// before the consult when no row tests it, after otherwise (so absent
    /// from the prompt then).
    pub outcome: Option<&'a WinningOutcome>,
}

/// Substitute the four placeholder families. Plain string replacement — user
/// text is never interpreted, and an unknown placeholder is left as written.
///
/// `{{metadata.key}}` follows that same leave-it-as-written rule when the key is
/// absent or holds a list or an object: the placeholder left standing tells the
/// author exactly which key their character lacks.
pub fn render_template(message: &str, vars: &TemplateVars) -> String {
    // v4's `message.replace(PATTERN, (whole, rawKey) => …)`: one pass, each
    // occurrence classified and looked up through the shared pair. Kept as a
    // regex replacement rather than a scan-and-splice so the substitution
    // positions are the engine's, not arithmetic of ours.
    PLACEHOLDER_PATTERN
        .replace_all(message, |caps: &Captures| {
            let whole = caps.get(0).map_or("", |m| m.as_str());
            let key = js_trim(caps.get(1).map_or("", |m| m.as_str()));
            let place_ref = classify_placeholder(key);
            match resolve_placeholder_value(&place_ref, vars) {
                // Numbers render through `format_value`; anything else through
                // its JS string form.
                Some(ResolvedValue::Number(n)) => format_value(n),
                Some(ResolvedValue::String(s)) => s,
                Some(ResolvedValue::Bool(b)) => b.to_string(),
                // Nothing renderable: leave the hole visible, and say why (v4
                // `custom-tools.ts:1247-1270`, ported here at P4.77 — the four
                // `logger.debug` sites the collapse's comment above named as
                // never carried). v4 switches on the CLASSIFIED kind, not the
                // collapsed `None`, so a present-but-non-primitive
                // metadata/state value and an absent one log different
                // reasons; `resolve_placeholder_value` already primitive-filters
                // those two kinds inline, so the raw lookup is repeated here,
                // on this cold path only, to recover which one happened.
                None => {
                    match &place_ref {
                        PlaceholderRef::Llm => {
                            // `{{{{`/`}}}}`: this is a tracing format-string
                            // literal, so v4's literal `{{llm}}` needs doubled
                            // escaping to survive `format_args!`.
                            tracing::debug!(
                                target: "quilltap::pascal",
                                "Custom tool message references {{{{llm}}}} with no consult to render",
                            );
                        }
                        // v4 `322a5488e`: between the `llm` and `metadata` arms.
                        PlaceholderRef::Outcome { .. } => {
                            let reason = if vars.outcome.is_none() {
                                "the table has not dealt yet (or the consult is read by an outcome row)"
                            } else {
                                "the winning outcome has no brief"
                            };
                            tracing::debug!(
                                target: "quilltap::pascal",
                                context = "pascal.custom-tools",
                                placeholder = whole,
                                reason,
                                "Custom tool template references {{{{outcome.*}}}} it cannot render",
                            );
                        }
                        PlaceholderRef::Metadata { key: meta_key } => {
                            let reason = if vars.metadata.and_then(|m| m.get(meta_key)).is_none() {
                                "no such metadata key"
                            } else {
                                "the key does not hold a primitive"
                            };
                            tracing::debug!(
                                target: "quilltap::pascal",
                                placeholder = whole,
                                reason,
                                "Custom tool message references metadata the character cannot render",
                            );
                        }
                        // v4's two new arms sit between `metadata` and
                        // `state` in the switch. The progress reason is FIXED
                        // (unlike metadata's two-way split): the flattened sheet
                        // is primitives all the way down, so "no such key" is
                        // the only way a lookup can miss, and v4 says so in one
                        // sentence covering both halves of the key.
                        PlaceholderRef::Progress { .. } => {
                            tracing::debug!(
                                target: "quilltap::pascal",
                                placeholder = whole,
                                reason = "the character carries no such progression, or it has no such field",
                                "Custom tool message references a progression the character cannot render",
                            );
                        }
                        PlaceholderRef::Now => {
                            // v4 logs NO fields here — not even the
                            // placeholder, which `{{now}}` already names.
                            tracing::debug!(
                                target: "quilltap::pascal",
                                "Custom tool message references {{{{now}}}} with no run clock to render",
                            );
                        }
                        PlaceholderRef::Character => {
                            // v4 logs NO field but `context` here — the
                            // `{{now}}` line's shape (v4 `a05c6f755`).
                            tracing::debug!(
                                target: "quilltap::pascal",
                                "Custom tool message references {{{{character}}}} but nobody in particular rolled",
                            );
                        }
                        // === P4.D329: the inventory placeholder (v4's arm sits
                        // between `character` and `state` in the switch) ===
                        PlaceholderRef::Inventory {
                            reference,
                            field,
                            key,
                        } => {
                            let reason = if vars.inventory.is_none() {
                                "no inventory to read (nobody in particular rolled)"
                            } else if inventory_placeholder_raw(
                                reference,
                                *field,
                                key.as_deref(),
                                vars.inventory,
                            )
                            .is_none()
                            {
                                "the object is absent, or has no such field or metadata key"
                            } else {
                                "the metadata key does not hold a primitive"
                            };
                            tracing::debug!(
                                target: "quilltap::pascal",
                                context = "pascal.custom-tools",
                                placeholder = whole,
                                reason,
                                "Custom tool message references an inventory object it cannot render",
                            );
                        }
                        PlaceholderRef::State { path } => {
                            let empty = Value::Object(Map::new());
                            let state = vars.state.unwrap_or(&empty);
                            let reason =
                                if get_at_path(state, &parse_path(Some(path))).is_none() {
                                    "no such state path"
                                } else {
                                    "the path does not hold a primitive"
                                };
                            tracing::debug!(
                                target: "quilltap::pascal",
                                placeholder = whole,
                                reason,
                                "Custom tool message references state it cannot render",
                            );
                        }
                        // v4's switch has no `params`/`unknown`/etc. case, so
                        // every OTHER kind (a not-found params name, or a
                        // genuinely unknown placeholder) falls to its
                        // `default:` — byte-faithful, not just the closest fit.
                        _ => {
                            tracing::debug!(
                                target: "quilltap::pascal",
                                placeholder = whole,
                                "Custom tool message carries an unknown placeholder",
                            );
                        }
                    }
                    whole.to_string()
                }
            }
        })
        .into_owned()
}

/// v4 `resolvePlaceholderValue(ref, vars)` (NEW at `0506517d3`) — the value a
/// classified placeholder names in a run's subjects. The two readers, the
/// template renderer and the effect-expression resolver, share this lookup;
/// before the collapse each spelled the same chain out for itself.
///
/// v4 returns the value RAW and lets each reader apply `isPrimitive`; v5 applies
/// it here, because both readers already did and neither v5 reader has anything
/// to say about a non-primitive that the other does not. (v4's renderer says it
/// in a debug log — ported at `render_template`, P4.77 — and its resolver
/// simply skips the effect.)
///
/// `{{params.toString}}` is the correction that rides along, and Rust never had
/// its cause: JS's `name in vars.params` reached `Object.prototype`, so the
/// pre-fix renderer answered `String(Object.prototype.toString)` — the function
/// source, spliced into a character's message. A `ResolvedParams` lookup here is
/// an association-list scan that cannot see a prototype, and the corpus pins it
/// on both sides so the row exists where the bug was.
pub fn resolve_placeholder_value(
    place_ref: &PlaceholderRef,
    vars: &TemplateVars,
) -> Option<ResolvedValue> {
    match place_ref {
        PlaceholderRef::Value => Some(ResolvedValue::Number(vars.value)),
        PlaceholderRef::Roll => Some(ResolvedValue::Number(vars.roll)),
        PlaceholderRef::Dice => Some(ResolvedValue::String(vars.dice.to_string())),
        PlaceholderRef::Llm => vars.llm.map(|l| ResolvedValue::String(l.output.clone())),
        // v4 `vars.outcome?.[ref.field]` — the state as its string, the index
        // as a NUMBER (so it renders through `format_value`: `0`, `1`), the
        // brief as rendered, or nothing.
        PlaceholderRef::Outcome { field } => vars.outcome.and_then(|o| match field {
            OutcomeField::State => Some(ResolvedValue::String(o.state.as_str().to_string())),
            OutcomeField::Index => Some(ResolvedValue::Number(o.index as f64)),
            OutcomeField::Brief => o.brief.clone().map(ResolvedValue::String),
        }),
        PlaceholderRef::Params { name } => lookup(vars.params, name).cloned(),
        PlaceholderRef::Metadata { key } => vars
            .metadata
            .and_then(|m| m.get(key))
            .and_then(js_primitive),
        // The flattened sheet is keyed `"<id>.<field>"`, so the two halves the
        // classifier split are rejoined here rather than looked up in a nested
        // object — v4 `vars.progress?.[`${ref.id}.${ref.field}`]`.
        PlaceholderRef::Progress { id, field } => vars
            .progress
            .and_then(|m| m.get(&format!("{id}.{field}")))
            .and_then(js_primitive),
        PlaceholderRef::Now => vars.now.map(|n| ResolvedValue::Number(n as f64)),
        // v4 `case 'character': return vars.character;` — a string subject.
        PlaceholderRef::Character => vars.character.map(|c| ResolvedValue::String(c.to_string())),
        PlaceholderRef::State { path } => {
            // `state.` is stripped and the remainder is a full state path
            // (v4 `f48f34dc`).
            let empty = Value::Object(Map::new());
            let state = vars.state.unwrap_or(&empty);
            get_at_path(state, &parse_path(Some(path))).and_then(|v| js_primitive(&v))
        }
        // === P4.D329: the inventory placeholder ===
        PlaceholderRef::Inventory {
            reference,
            field,
            key,
        } => inventory_placeholder_raw(reference, *field, key.as_deref(), vars.inventory)
            .and_then(|v| js_primitive(&v)),
        PlaceholderRef::Unknown { .. } => None,
    }
}

// === P4.D329: the inventory placeholder ===
/// v4 `resolvePlaceholderValue`'s `inventory` case, RAW — `None` is JS
/// `undefined`. No context (a run nobody made): nothing renders, not even
/// `state`. `state` answers BEFORE the object is looked up, so an absent
/// reference renders `absent`.
fn inventory_placeholder_raw(
    reference: &str,
    field: InventoryPlaceholderField,
    key: Option<&str>,
    inventory: Option<&dyn InventoryContext>,
) -> Option<Value> {
    let ctx = inventory?;
    if field == InventoryPlaceholderField::State {
        return Some(Value::String(ctx.state(reference).as_str().to_string()));
    }
    let item = ctx.item(reference)?;
    let fields = &item.0;
    match field {
        // A title, or the name — so a message can always say *the sword*
        // (`item.title?.trim() || item.name`).
        InventoryPlaceholderField::Title => match fields.get("title") {
            Some(Value::String(t)) if !js_trim(t).is_empty() => {
                Some(Value::String(js_trim(t).to_string()))
            }
            _ => fields.get("name").cloned(),
        },
        InventoryPlaceholderField::Name => fields.get("name").cloned(),
        // `item.description ?? undefined`.
        InventoryPlaceholderField::Description => match fields.get("description") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.clone()),
        },
        InventoryPlaceholderField::Metadata => {
            let key = key?;
            fields
                .get("metadata")
                .and_then(Value::as_object)
                .and_then(|m| m.get(key))
                .cloned()
        }
        InventoryPlaceholderField::State => None,
    }
}

#[cfg(test)]
mod comparator_subject_debug_tests {
    //! P4.D169: v4's `logger.debug(`Custom tool ${subject} test did not match`)`
    //! is TEMPLATED — one call site, two subjects. Pinned in BOTH spellings,
    //! because a port that hard-coded either one would still satisfy a pin on
    //! the other.

    use super::*;
    use crate::pascal::custom_tool_types::{ParamComparator, When, WhenObject};
    use crate::test_support::captured;

    fn subjects<'a>(
        params: &'a ResolvedParams,
        metadata: Option<&'a Map<String, Value>>,
        progress: Option<&'a Map<String, Value>>,
    ) -> OutcomeSubjects<'a> {
        OutcomeSubjects {
            value: 1.0,
            roll: 1.0,
            params,
            metadata,
            progress,
            llm: None,
            state: None,
            character: None,
            inventory: None,
        }
    }

    fn missing_key_test(key: &str) -> Vec<(String, ParamComparator)> {
        vec![(
            key.to_string(),
            ParamComparator {
                eq: Some(crate::pascal::custom_tool_types::AnyOperand::Number(1.0)),
                ..Default::default()
            },
        )]
    }

    #[test]
    fn the_subject_is_interpolated_into_the_message_for_both_sheets() {
        let params: ResolvedParams = Vec::new();
        let sheet = Map::new();

        for (subject, when) in [
            (
                "metadata",
                WhenObject {
                    metadata: Some(missing_key_test("nickname")),
                    ..Default::default()
                },
            ),
            (
                "progress",
                WhenObject {
                    progress: Some(missing_key_test("cannon.percent")),
                    ..Default::default()
                },
            ),
        ] {
            let subs = subjects(&params, Some(&sheet), Some(&sheet));
            let lines = captured(|| {
                let held = matches_when(&When::Object(Box::new(when)), &subs, "fire_cannon");
                assert_eq!(held, Ok(false), "{subject}: a missing key fails soft");
            });
            assert_eq!(lines.len(), 1, "{subject}: {lines:?}");
            let key = if subject == "metadata" {
                "nickname"
            } else {
                "cannon.percent"
            };
            assert_eq!(
                lines[0],
                format!(
                    "DEBUG quilltap::pascal Custom tool {subject} test did not match \
tool=fire_cannon key={key} reason=the character has no such metadata key"
                ),
                "{subject}"
            );
        }
    }

    /// v4's decline sentences come from the SHARED `metadata-match` helper and
    /// say "metadata key" whichever sheet is being read. Rewording that for the
    /// progress subject would read better and diverge, so the quirk is pinned
    /// here rather than left to a future tidy-up.
    #[test]
    fn the_decline_reason_says_metadata_even_for_the_progress_sheet() {
        let params: ResolvedParams = Vec::new();
        let sheet = Map::new();
        let subs = subjects(&params, Some(&sheet), Some(&sheet));
        let lines = captured(|| {
            let _ = matches_when(
                &When::Object(Box::new(WhenObject {
                    progress: Some(missing_key_test("cannon.percent")),
                    ..Default::default()
                })),
                &subs,
                "fire_cannon",
            );
        });
        assert!(
            lines[0].ends_with("reason=the character has no such metadata key"),
            "{}",
            lines[0]
        );
    }
}

#[cfg(test)]
mod render_template_debug_tests {
    //! v4's four `render_template` debug lines (`custom-tools.ts:1247-1270`),
    //! ported at P4.77 — a log-only fix, invisible to every other proof in
    //! this repo (`differential-blind-to-a-log-only-fix`). Presence AND
    //! silence: one test per line, plus the arm that must stay quiet.

    use super::*;
    use crate::test_support::captured;

    static EMPTY_PARAMS: ResolvedParams = Vec::new();

    fn vars<'a>(
        metadata: Option<&'a Map<String, Value>>,
        state: Option<&'a Value>,
    ) -> TemplateVars<'a> {
        TemplateVars {
            value: 0.0,
            roll: 0.0,
            dice: "d6",
            params: &EMPTY_PARAMS,
            metadata,
            // The render-debug rig poses neither subject; the two P4.D169 arms
            // have their own tests below.
            progress: None,
            now: None,
            llm: None,
            state,
            character: None,
            inventory: None,
            outcome: None,
        }
    }

    #[test]
    fn an_llm_placeholder_with_no_consult_warns_and_leaves_the_hole() {
        let lines = captured(|| {
            let out = render_template("draw: {{llm}}", &vars(None, None));
            assert_eq!(out, "draw: {{llm}}", "the hole stays visible");
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("DEBUG quilltap::pascal"),
            "{}",
            lines[0]
        );
        assert!(
            lines[0].contains("Custom tool message references {{llm}} with no consult to render"),
            "{}",
            lines[0]
        );
    }

    #[test]
    fn a_missing_metadata_key_warns_no_such_key() {
        let metadata = Map::new();
        let lines = captured(|| {
            let out = render_template("{{metadata.nickname}}", &vars(Some(&metadata), None));
            assert_eq!(out, "{{metadata.nickname}}");
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert!(
            line.contains("Custom tool message references metadata the character cannot render"),
            "{line}"
        );
        assert!(line.contains("placeholder={{metadata.nickname}}"), "{line}");
        assert!(line.contains("reason=no such metadata key"), "{line}");
    }

    #[test]
    fn a_non_primitive_metadata_value_warns_not_a_primitive() {
        let mut metadata = Map::new();
        metadata.insert("tags".to_string(), Value::Array(vec![]));
        let lines = captured(|| {
            render_template("{{metadata.tags}}", &vars(Some(&metadata), None));
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("reason=the key does not hold a primitive"),
            "{}",
            lines[0]
        );
    }

    #[test]
    fn a_missing_state_path_warns_no_such_path() {
        let state = Value::Object(Map::new());
        let lines = captured(|| {
            let out = render_template("{{state.hp}}", &vars(None, Some(&state)));
            assert_eq!(out, "{{state.hp}}");
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert!(
            line.contains("Custom tool message references state it cannot render"),
            "{line}"
        );
        assert!(line.contains("placeholder={{state.hp}}"), "{line}");
        assert!(line.contains("reason=no such state path"), "{line}");
    }

    #[test]
    fn a_non_primitive_state_value_warns_not_a_primitive() {
        let mut inner = Map::new();
        inner.insert("hp".to_string(), Value::Object(Map::new()));
        let state = Value::Object(inner);
        let lines = captured(|| {
            render_template("{{state.hp}}", &vars(None, Some(&state)));
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("reason=the path does not hold a primitive"),
            "{}",
            lines[0]
        );
    }

    /// v4's switch has no `params`/`unknown` case — both fall to `default:`.
    /// A not-found params name is byte-faithful to that, not the closest fit.
    #[test]
    fn an_unknown_placeholder_and_a_missing_param_both_warn_unknown_placeholder() {
        for template in ["{{surprise}}", "{{params.missing}}"] {
            let lines = captured(|| {
                render_template(template, &vars(None, None));
            });
            assert_eq!(lines.len(), 1, "{template}: {lines:?}");
            assert!(
                lines[0].contains("Custom tool message carries an unknown placeholder"),
                "{template}: {}",
                lines[0]
            );
            assert!(
                lines[0].contains(&format!("placeholder={template}")),
                "{template}: {}",
                lines[0]
            );
        }
    }

    /// P4.D169: v4's two NEW `render_template` arms. The progress reason is
    /// FIXED where metadata's splits two ways — the flattened sheet is
    /// primitives all the way down, so a miss can only ever be a missing key,
    /// and v4 says so in one sentence covering both halves.
    #[test]
    fn a_missing_progression_field_warns_with_v4s_one_fixed_reason() {
        let mut sheet = Map::new();
        sheet.insert("cannon.percent".to_string(), Value::from(0.5));
        // Both halves of the key can miss, and both land here.
        for (template, why) in [
            ("{{progress.zeppelin.percent}}", "no such progression"),
            ("{{progress.cannon.altitude}}", "no such field"),
        ] {
            let mut v = vars(None, None);
            v.progress = Some(&sheet);
            let lines = captured(|| {
                let out = render_template(template, &v);
                assert_eq!(out, template, "{why}: the hole stays visible");
            });
            assert_eq!(lines.len(), 1, "{why}: {lines:?}");
            let line = &lines[0];
            assert!(line.starts_with("DEBUG quilltap::pascal"), "{why}: {line}");
            assert!(
                line.contains(
                    "Custom tool message references a progression the character cannot render"
                ),
                "{why}: {line}"
            );
            assert!(
                line.contains(&format!("placeholder={template}")),
                "{why}: {line}"
            );
            assert!(
                line.contains(
                    "reason=the character carries no such progression, or it has no such field"
                ),
                "{why}: {line}"
            );
        }
    }

    /// A character with no progressions at all takes the same arm — there is no
    /// separate "no sheet" sentence in v4, and inventing one would diverge.
    #[test]
    fn a_progress_placeholder_with_no_sheet_takes_the_same_arm() {
        let lines = captured(|| {
            render_template("{{progress.cannon.percent}}", &vars(None, None));
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains(
                "Custom tool message references a progression the character cannot render"
            ),
            "{}",
            lines[0]
        );
    }

    /// v4 logs NO fields on this one — not even the placeholder, which
    /// `{{now}}` already names. A pin that only matched the message would not
    /// notice a helpful extra field creeping in, so the field list is asserted
    /// EMPTY.
    #[test]
    fn a_now_placeholder_with_no_run_clock_warns_and_names_no_fields() {
        let lines = captured(|| {
            let out = render_template("at {{now}}", &vars(None, None));
            assert_eq!(out, "at {{now}}", "the hole stays visible");
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(
            lines[0],
            "DEBUG quilltap::pascal Custom tool message references {{now}} with no run clock to render",
            "v4 logs this line bare"
        );
    }

    /// P4.D319 (v4 `a05c6f755`): nobody in particular rolled — the hole stays
    /// visible and v4's bare line (no field but `context`, the `{{now}}`
    /// line's shape) fires ONCE per unrenderable occurrence. Field list
    /// asserted EMPTY, as for `{{now}}`.
    #[test]
    fn a_character_placeholder_with_nobody_rolling_logs_bare_once() {
        let lines = captured(|| {
            let out = render_template("{{character}} suits up.", &vars(None, None));
            assert_eq!(out, "{{character}} suits up.", "the hole stays visible");
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(
            lines[0],
            "DEBUG quilltap::pascal Custom tool message references {{character}} but nobody in particular rolled",
            "v4 logs this line bare"
        );
    }

    /// The silence leg: a named character renders quietly.
    #[test]
    fn a_rendered_character_logs_nothing() {
        let mut v = vars(None, None);
        v.character = Some("Charlie");
        let lines = captured(|| {
            assert_eq!(
                render_template("{{character}} suits up.", &v),
                "Charlie suits up."
            );
        });
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// The silence half for both new arms: a run clock and a sheet that answer
    /// render quietly. Without this, a line wired to the wrong branch — firing
    /// on every render rather than on a miss — would still pass above.
    #[test]
    fn a_rendered_progression_and_clock_log_nothing() {
        let mut sheet = Map::new();
        sheet.insert("cannon.percent".to_string(), Value::from(0.5));
        let mut v = vars(None, None);
        v.progress = Some(&sheet);
        v.now = Some(1_788_004_800_000);
        let lines = captured(|| {
            let out = render_template("{{progress.cannon.percent}} @ {{now}}", &v);
            assert_eq!(out, "0.5 @ 1788004800000");
        });
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// The silence half: a template with nothing unrenderable logs nothing at
    /// all — a line attached to the wrong branch is exactly what a
    /// presence-only assertion cannot catch.
    #[test]
    fn a_fully_renderable_template_logs_nothing() {
        let mut metadata = Map::new();
        metadata.insert("name".to_string(), Value::String("Kestrel".to_string()));
        let lines = captured(|| {
            let out = render_template(
                "{{value}} {{roll}} {{dice}} {{metadata.name}}",
                &vars(Some(&metadata), None),
            );
            assert_eq!(out, "0 0 d6 Kestrel");
        });
        assert!(lines.is_empty(), "{lines:?}");
    }
}

/// Resolve a definition's LLM consult: render the prompt, pose it through the
/// injected invoker, and translate whatever happens into the author's terms.
///
/// Fail-soft by design — a consult NEVER fails the run. A provider error, a
/// timeout, an empty answer, or an entrance that wired no invoker all land in
/// the same place: `ok: false` with the author's `error_message` as the output,
/// so the outcome table gets to deal with the silence the way its author wrote
/// it to.
async fn resolve_llm_consult(
    spec: &CustomToolLlm,
    prompt: String,
    invoke: Option<&dyn LlmInvoker>,
) -> LlmConsultResult {
    let failed = |reason: String, prompt: String| LlmConsultResult {
        ok: false,
        output: spec.error_message.clone(),
        prompt,
        reason: Some(reason),
        provider: None,
        model: None,
    };

    let Some(invoke) = invoke else {
        return failed(
            "no LLM invoker was available in this context".into(),
            prompt,
        );
    };

    // The author's own leash, or the default. `error_message` is never subject
    // to it — those are the author's words, kept whole.
    let max_output = spec.max_output.unwrap_or(MAX_LLM_OUTPUT_LENGTH as i64);

    let result = invoke
        .invoke(
            &prompt,
            LlmInvokeOptions {
                max_output_chars: max_output,
            },
        )
        .await;

    let (output, provider, model) = match result {
        LlmInvokeResult::Failed { reason } => return failed(reason, prompt),
        LlmInvokeResult::Answered {
            output,
            provider,
            model,
        } => (output, provider, model),
    };

    // Trim, cap, RE-trim — v4's exact sequence. The cap counts UTF-16 code
    // units, because v4's `.slice` does.
    let capped = jsstr::utf16_truncate(js_trim(&output), max_output.max(0) as usize);
    let output = js_trim(&capped).to_string();
    if output.is_empty() {
        return failed("the model returned an empty answer".into(), prompt);
    }

    LlmConsultResult {
        ok: true,
        output,
        prompt,
        reason: None,
        // v4 spreads these only when TRUTHY, so an empty string is dropped too.
        provider: provider.filter(|p| !p.is_empty()),
        model: model.filter(|m| !m.is_empty()),
    }
}

/// Run a definition: validate params, roll, transform, consult (when declared),
/// evaluate, render.
///
/// No writes, no message posting — both entrances call this and then decide how
/// to announce the result. The one impurity is the optional LLM consult, which
/// arrives as an injected [`LlmInvoker`] so this stays testable and the proving
/// bench can substitute a pretend oracle.
// === P4.D329: the whole-roll `$inventory` ===
/// v4 `prepareRoll`'s `$inventory` arm: the notation an object's metadata
/// holds, or the (load-checked) dice fallback when it holds none — or none
/// this build can roll. Returns the notation STRING (what the run records as
/// `notation`) with its parse.
fn prepare_inventory_roll(
    definition: &QtapCustomTool,
    r: &InventoryRef,
    inventory: Option<&dyn InventoryContext>,
) -> Result<(String, super::dice::DiceNotation), CustomToolRunError> {
    let resolved = resolve_inventory_value(r, inventory);
    if let ResolvedValue::String(spec) = &resolved {
        if let Some(parsed) = parse_dice_notation(spec) {
            return Ok((spec.clone(), parsed));
        }
    }
    let resolved_text = scalar_text(&resolved.to_value());
    let fallback_text = scalar_text(&serde_json::to_value(&r.fallback).unwrap_or_default());
    tracing::debug!(
        target: "quilltap::pascal",
        context = "pascal.custom-tools",
        tool = definition.name.as_str(),
        reference = r.inventory.as_str(),
        resolved = resolved_text.as_str(),
        fallback = fallback_text.as_str(),
        "Custom tool $inventory roll fell back",
    );
    match &r.fallback {
        StateRefFallback::String(spec) => match parse_dice_notation(spec) {
            Some(parsed) => Ok((spec.clone(), parsed)),
            None => Err(inventory_roll_fallback_error(definition)),
        },
        _ => Err(inventory_roll_fallback_error(definition)),
    }
}

/// A primitive log field as v4's bag holds it: a string BARE, a number or a
/// boolean as its JSON text (P4.D329).
fn scalar_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Unreachable past load (`validate_roll_refs` demands a dice-notation
/// fallback); a regression tripwire, kept as v4 keeps it.
fn inventory_roll_fallback_error(definition: &QtapCustomTool) -> CustomToolRunError {
    err(format!(
        "{}: the $inventory roll's fallback is not dice notation this build can roll",
        definition.name
    ))
}

// P4.D169: mirrors v4 `executeCustomTool` plus the run clock — the ONE reading
// `{{now}}` renders with and the progress sheet is derived against.
#[allow(clippy::too_many_arguments)]
pub async fn execute_custom_tool(
    definition: &QtapCustomTool,
    supplied_params: Option<&serde_json::Map<String, Value>>,
    private: Option<bool>,
    metadata: Option<&Map<String, Value>>,
    // Merged persistent state for `$state` refs and `{{state.path}}` — v4
    // `overrides.state ?? {}` (pass `None` for the empty view).
    state: Option<&Value>,
    rng: &mut (dyn RandomBytes + Send),
    llm_invoke: Option<&dyn LlmInvoker>,
    // The flattened progress sheet, derived by the ENTRANCE from the same
    // metadata snapshot it hands in above, against one `now_ms` taken at run
    // start. Derived there rather than here so the sheet and `{{now}}` are the
    // same clock reading the entrance's effect applier will later stamp with.
    progress: Option<&Map<String, Value>>,
    // Epoch milliseconds at run start, for `{{now}}`. v4 defaults it to
    // `Date.now()`; v5 makes every caller pass it, so a differential can freeze
    // it and no site can quietly take a second reading.
    now_ms: i64,
    // The rolling character's name, for `{{character}}`. `None` for a run
    // nobody made (v4 `overrides.character`, `a05c6f755`).
    character: Option<&str>,
    // === P4.D329: the LAST positional (R-Q) ===
    // The rolling character's Inventory for this chat, loaded once by the
    // entrance. `None` for a run nobody made — inventory tests decline,
    // `$inventory` operands fall back, `{{inventory.*}}` renders as written.
    inventory: Option<&dyn InventoryContext>,
) -> Result<CustomToolRunResult, CustomToolRunError> {
    // v4 `overrides?.character?.trim() || undefined` — whitespace alone is
    // nobody, and the trim is JS's (`js_trim`, never `str::trim`).
    let character = character.map(js_trim).filter(|c| !c.is_empty());
    let params = resolve_params(definition, supplied_params, state, inventory)?;

    let raw: f64;
    let value: f64;
    let roll_form: RollForm;
    let mut notation: Option<String> = None;
    let mut dice_rolls: Option<Vec<i64>> = None;
    let mut dice_breakdown = String::new();

    match &definition.roll {
        Some(Roll::Dice(spec)) => {
            roll_form = RollForm::Dice;
            let Some(parsed) = parse_dice_notation(spec) else {
                // Load-time validation should have caught this; a regression here
                // must still fail loudly rather than invent a number.
                return Err(err(format!(
                    "{}: \"{spec}\" is not dice notation this build can roll",
                    definition.name
                )));
            };
            let rolled = roll_notation(&parsed, rng);
            notation = Some(spec.clone());
            dice_rolls = Some(rolled.results.clone());
            dice_breakdown = format_dice_breakdown(&rolled);
            // Dice carry their own modifier; Form A's multiplier/offset/round do
            // not apply, so raw and value are the same total.
            raw = rolled.total as f64;
            value = rolled.total as f64;
        }
        // === P4.D329: a whole roll drawn from an object's metadata ===
        // The run's form is `dice` and its notation the RESOLVED string —
        // only the Workbench library calls this form `inventory`.
        Some(Roll::Inventory(r)) => {
            roll_form = RollForm::Dice;
            let (spec, parsed) = prepare_inventory_roll(definition, r, inventory)?;
            let rolled = roll_notation(&parsed, rng);
            notation = Some(spec);
            dice_rolls = Some(rolled.results.clone());
            dice_breakdown = format_dice_breakdown(&rolled);
            raw = rolled.total as f64;
            value = rolled.total as f64;
        }
        other => {
            roll_form = RollForm::Range;
            let default = RollRange::default();
            let range = match other {
                Some(Roll::Range(r)) => r,
                _ => &default,
            };
            let (drawn_raw, drawn_value) =
                roll_range(definition, range, &params, state, inventory, rng)?;
            raw = drawn_raw;
            value = drawn_value;
        }
    }

    // Two run orders, and the table decides which (v4 `322a5488e`, bug 257). A
    // table that TESTS the consult's answer needs the consult first (the
    // oracle: the model's answer is the verdict). Any other table is dealt
    // BEFORE the consult, so the prompt is told the verdict
    // (`{{outcome.brief}}`) rather than asked to recompute it — a cheap model
    // asked to redo arithmetic the engine has already done will sometimes
    // report the other branch.
    let consult_first = definition.llm.is_some() && outcome_table_tests_llm(&definition.outcomes);
    // ONE set of template vars for the run: no `llm` (there is no answer yet
    // to quote) and no `outcome` (no verdict yet).
    let template_vars = TemplateVars {
        value,
        roll: raw,
        dice: &dice_breakdown,
        params: &params,
        metadata,
        progress,
        now: Some(now_ms),
        llm: None,
        state,
        character,
        inventory,
        outcome: None,
    };
    let base_subjects = OutcomeSubjects {
        value,
        roll: raw,
        params: &params,
        metadata,
        progress,
        llm: None,
        state,
        character,
        inventory,
    };

    let llm: Option<LlmConsultResult>;
    let outcome_index: usize;
    // The brief the prompt was told, when the table dealt first (v4 `briefs`).
    let mut dealt_brief: Option<String> = None;
    match &definition.llm {
        Some(spec) if consult_first => {
            let prompt = render_template(&spec.prompt, &template_vars);
            let consult = resolve_llm_consult(spec, prompt, llm_invoke).await;
            let subject = consult.subject();
            let subjects = OutcomeSubjects {
                llm: Some(&subject),
                ..base_subjects
            };
            outcome_index = pick_outcome(definition, &subjects, value)?;
            llm = Some(consult);
        }
        spec => {
            outcome_index = pick_outcome(definition, &base_subjects, value)?;
            match spec {
                Some(spec) => {
                    let winner = &definition.outcomes[outcome_index];
                    // The brief is written before the consult, so it quotes
                    // only what the prompt may: no `{{llm}}`, no `{{outcome.*}}`
                    // (load rejects both).
                    let brief = winner
                        .brief
                        .as_ref()
                        .map(|b| render_template(b, &template_vars));
                    tracing::debug!(
                        target: "quilltap::pascal",
                        context = "pascal.custom-tools",
                        tool = definition.name,
                        outcomeIndex = outcome_index,
                        state = winner.state.as_str(),
                        hasBrief = brief.is_some(),
                        "Custom tool table dealt ahead of the consult",
                    );
                    let told = WinningOutcome {
                        state: winner.state,
                        index: outcome_index,
                        brief: brief.clone(),
                    };
                    let prompt = render_template(
                        &spec.prompt,
                        &TemplateVars {
                            outcome: Some(&told),
                            ..template_vars.clone()
                        },
                    );
                    dealt_brief = brief;
                    llm = Some(resolve_llm_consult(spec, prompt, llm_invoke).await);
                }
                None => llm = None,
            }
        }
    }
    let llm_subject = llm.as_ref().map(|c| c.subject());
    let outcome = &definition.outcomes[outcome_index];

    // Rendered once for the winner, whichever run order dealt it: a brief after
    // an oracle-first run is never read by the prompt, but `{{outcome.brief}}`
    // in a message or effect still resolves.
    let winning_brief = dealt_brief.or_else(|| {
        outcome
            .brief
            .as_ref()
            .map(|b| render_template(b, &template_vars))
    });
    let winning = WinningOutcome {
        state: outcome.state,
        index: outcome_index,
        brief: winning_brief,
    };
    let after_vars = TemplateVars {
        llm: llm_subject.as_ref(),
        outcome: Some(&winning),
        ..template_vars.clone()
    };

    let message = render_template(&outcome.message, &after_vars);
    let metadata_tested = collect_metadata_tested(&outcome.when, metadata);

    // F1 — the chip label, rendered AFTER the outcome is chosen so it may quote
    // everything the message may. This is the one render site; both entrances
    // copy the result, so the chip and the bubble header can never drift.
    let chip_label = definition
        .chip_label
        .as_ref()
        .map(|template| render_template(template, &after_vars));

    // F3 — effects, resolved pure against the finished run. Nothing is written
    // here; the entrances decide whether (and where) the writes land.
    let effects = match &definition.effects {
        Some(declared) if !declared.is_empty() => {
            let effect_subjects = EffectSubjects {
                base: OutcomeSubjects {
                    llm: llm_subject.as_ref(),
                    ..base_subjects
                },
                outcome: winning.clone(),
                dice: &dice_breakdown,
                now: now_ms,
            };
            Some(resolve_effects(definition, &effect_subjects))
        }
        _ => None,
    };

    let visibility = match private {
        Some(true) => Visibility::Whisper,
        Some(false) => Visibility::Public,
        None => definition.default_visibility.unwrap_or(Visibility::Public),
    };

    Ok(CustomToolRunResult {
        tool: definition.name.clone(),
        params,
        roll_form,
        notation,
        raw,
        dice_rolls,
        value,
        state: outcome.state,
        outcome_index,
        message,
        dice_breakdown,
        visibility,
        metadata_tested,
        llm,
        chip_label,
        effects,
    })
}

/// The first outcome whose `when` holds (v4 `pickOutcome`). The schema's
/// mandatory trailing catch-all makes a miss unreachable; a regression still
/// fails loudly rather than invent a verdict.
fn pick_outcome(
    definition: &QtapCustomTool,
    subjects: &OutcomeSubjects,
    value: f64,
) -> Result<usize, CustomToolRunError> {
    for (i, o) in definition.outcomes.iter().enumerate() {
        if matches_when(&o.when, subjects, &definition.name)? {
            return Ok(i);
        }
    }
    Err(err(format!(
        "{}: no outcome matched {}",
        definition.name,
        format_value(value)
    )))
}

// ===========================================================================
// Pascal's Workbench — the outcome-table audit (v4 `simulateOutcomes`)
// ===========================================================================

/// One outcome's share of the simulated deals.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditOutcome {
    pub index: usize,
    pub hits: usize,
    pub share: f64,
}

/// The result of a table audit (v4 `CustomToolAuditResult`).
#[derive(Debug, Clone, PartialEq)]
pub struct CustomToolAuditResult {
    pub runs: usize,
    pub outcomes: Vec<AuditOutcome>,
    pub value_min: f64,
    pub value_max: f64,
    pub value_mean: f64,
}

/// Deal many hands and count where they land — the proving bench's table audit
/// (v4 `simulateOutcomes`).
///
/// Same draw, same transform, same [`matches_when`] evaluation as
/// [`execute_custom_tool`], run `runs` times with ONE param resolution up front.
/// `render_template` is deliberately skipped: it is the expensive step and
/// contributes nothing to hit rates. Draws through the injected `rng` seam (v4
/// draws through the crypto directly); a deterministic corpus (min===max ranges)
/// never consumes a byte, which is how the cross-language differential pins the
/// exact hit counts.
// P4.D169: mirrors v4 `simulateOutcomes` plus the run clock, as above.
#[allow(clippy::too_many_arguments)]
pub fn simulate_outcomes(
    definition: &QtapCustomTool,
    supplied_params: Option<&Map<String, Value>>,
    runs: usize,
    metadata: Option<&Map<String, Value>>,
    // `llm`: a pretend consult, held FIXED across every draw — the audit never
    // spends a real LLM call, let alone ten thousand of them. Hit rates for a
    // table that branches on the answer are therefore conditional on this one
    // answer; the bench says so beside the field. This never invokes.
    llm: Option<&LlmSubject>,
    // Mock merged state for `$state` refs, held fixed across every draw (v4's
    // trailing `state: CustomToolState = {}`).
    state: Option<&Value>,
    rng: &mut dyn RandomBytes,
    // The mock progress sheet, held fixed across every draw. The bench derives
    // it from the typed fact sheet at ONE instant, so an audit of a table that
    // branches on `cannon.complete` is conditional on that one clock reading —
    // the same caveat the bench already states for a pretend consult.
    progress: Option<&Map<String, Value>>,
    // === P4.D329: the LAST positional (v4's 8th, `inventory = null`) ===
    // The mock Inventory (the bench's), held fixed across every draw.
    inventory: Option<&dyn InventoryContext>,
) -> Result<CustomToolAuditResult, CustomToolRunError> {
    let params = resolve_params(definition, supplied_params, state, inventory)?;

    let mut parsed_dice = None;
    let default_range = RollRange::default();
    let mut range_roll: &RollRange = &default_range;
    match &definition.roll {
        Some(Roll::Dice(spec)) => {
            parsed_dice = Some(parse_dice_notation(spec).ok_or_else(|| {
                err(format!(
                    "{}: \"{spec}\" is not dice notation this build can roll",
                    definition.name
                ))
            })?);
        }
        Some(Roll::Range(r)) => range_roll = r,
        // P4.D329: resolved ONCE, before the loop (v4 `prepareRoll`).
        Some(Roll::Inventory(r)) => {
            parsed_dice = Some(prepare_inventory_roll(definition, r, inventory)?.1);
        }
        None => {}
    }

    let mut hits = vec![0usize; definition.outcomes.len()];
    let mut value_min = f64::INFINITY;
    let mut value_max = f64::NEG_INFINITY;
    let mut value_sum = 0.0;

    for _ in 0..runs {
        let (raw, value) = match &parsed_dice {
            Some(notation) => {
                let rolled = roll_notation(notation, rng);
                (rolled.total as f64, rolled.total as f64)
            }
            None => roll_range(definition, range_roll, &params, state, inventory, rng)?,
        };

        let subjects = OutcomeSubjects {
            value,
            roll: raw,
            params: &params,
            metadata,
            progress,
            llm,
            state,
            // The audit never renders and never has a character (v4
            // `simulateOutcomes` — no hunk).
            character: None,
            inventory,
        };
        let mut outcome_index = None;
        for (i, o) in definition.outcomes.iter().enumerate() {
            if matches_when(&o.when, &subjects, &definition.name)? {
                outcome_index = Some(i);
                break;
            }
        }
        let Some(outcome_index) = outcome_index else {
            // The schema's mandatory trailing catch-all makes this unreachable.
            return Err(err(format!(
                "{}: no outcome matched {}",
                definition.name,
                format_value(value)
            )));
        };
        hits[outcome_index] += 1;

        if value < value_min {
            value_min = value;
        }
        if value > value_max {
            value_max = value;
        }
        value_sum += value;
    }

    Ok(CustomToolAuditResult {
        runs,
        outcomes: hits
            .iter()
            .enumerate()
            .map(|(index, &count)| AuditOutcome {
                index,
                hits: count,
                share: if runs > 0 {
                    count as f64 / runs as f64
                } else {
                    0.0
                },
            })
            .collect(),
        value_min: if runs > 0 { value_min } else { 0.0 },
        value_max: if runs > 0 { value_max } else { 0.0 },
        value_mean: if runs > 0 {
            value_sum / runs as f64
        } else {
            0.0
        },
    })
}

#[cfg(test)]
mod simulate_tests {
    use super::*;
    use crate::pascal::custom_tool_types::safe_parse;
    use crate::tools::rng::OsRandomBytes;

    fn def(raw: serde_json::Value) -> QtapCustomTool {
        safe_parse(&raw).unwrap()
    }

    #[test]
    fn a_uniform_roll_spreads_roughly_by_band_width() {
        // [0,1] uniform, a low band [.<0.3) and the rest. Over many runs the low
        // band's share should sit near its 0.3 width (v4's own statistical shape).
        let d = def(serde_json::json!({
            "name": "spread", "description": "s",
            "roll": { "min": 0, "max": 1 },
            "outcomes": [
                { "when": { "lt": 0.3 }, "message": "lo", "state": "info" },
                { "when": true, "message": "hi", "state": "info" },
            ],
        }));
        let mut rng = OsRandomBytes;
        let r =
            simulate_outcomes(&d, None, 20_000, None, None, None, &mut rng, None, None).unwrap();
        let low = r.outcomes[0].share;
        assert!(
            (0.2..0.4).contains(&low),
            "low band share {low} not near 0.3"
        );
        assert!(r.value_min >= 0.0 && r.value_max <= 1.0);
    }

    #[test]
    fn a_metadata_gate_flips_from_zero_to_full_when_the_key_arrives() {
        // Deterministic value (min===max), so the gate is the only variable.
        let d = def(serde_json::json!({
            "name": "gate", "description": "g",
            "roll": { "min": 0.7, "max": 0.7 },
            "outcomes": [
                { "when": { "metadata": { "luck": { "gte": 5 } } }, "message": "open", "state": "success" },
                { "when": true, "message": "shut", "state": "failure" },
            ],
        }));
        let mut rng = OsRandomBytes;
        let empty = simulate_outcomes(
            &d,
            None,
            100,
            Some(&serde_json::Map::new()),
            None,
            None,
            &mut rng,
            None,
            None,
        )
        .unwrap();
        assert_eq!(empty.outcomes[0].hits, 0);

        let mut sheet = serde_json::Map::new();
        sheet.insert("luck".into(), serde_json::json!(7));
        let carrying = simulate_outcomes(
            &d,
            None,
            100,
            Some(&sheet),
            None,
            None,
            &mut rng,
            None,
            None,
        )
        .unwrap();
        assert_eq!(carrying.outcomes[0].hits, 100);
    }

    #[test]
    fn an_inverted_range_refuses() {
        let d = def(serde_json::json!({
            "name": "bad", "description": "b",
            "roll": { "min": 1, "max": 0 },
            "outcomes": [{ "when": true, "message": "x", "state": "info" }],
        }));
        let mut rng = OsRandomBytes;
        assert!(simulate_outcomes(&d, None, 10, None, None, None, &mut rng, None, None).is_err());
    }
}

#[cfg(test)]
mod wardrobe_gesture_tests {
    //! v4 `resolveWardrobeGesture` (`f0db8cbb8`): the `rendered empty` DEBUG
    //! pinned on the caller thread, with its silence leg. The rendered bytes
    //! themselves are pinned against v4's real core by
    //! `pascal_custom_tools_execution_equivalence`'s `effects-wardrobe-*` rows.
    use super::*;
    use crate::test_support::captured_with;

    fn vars<'a>(params: &'a ResolvedParams) -> TemplateVars<'a> {
        TemplateVars {
            value: 1.0,
            roll: 1.0,
            dice: "",
            params,
            metadata: None,
            progress: None,
            now: Some(0),
            llm: None,
            state: None,
            character: None,
            inventory: None,
            outcome: None,
        }
    }

    fn gesture(action: WardrobeEffectAction, items: &[&str]) -> CustomToolWardrobeGesture {
        CustomToolWardrobeGesture {
            action,
            items: Some(items.iter().map(|s| s.to_string()).collect()),
            mode: None,
            slot: None,
        }
    }

    fn skipped(index: usize, reason: String) -> ResolvedEffect {
        ResolvedEffect::Skipped { index, reason }
    }

    #[test]
    fn an_item_rendering_empty_is_dropped_with_the_debug_line() {
        let params: ResolvedParams = vec![("garment".into(), ResolvedValue::String(String::new()))];
        let g = gesture(
            WardrobeEffectAction::TakeOff,
            &["{{params.garment}}", "Gloves"],
        );
        let (out, lines) =
            captured_with(|| resolve_wardrobe_gesture("blank", 3, &g, &vars(&params), &skipped));
        let ResolvedEffect::Wardrobe { index, wardrobe } = out else {
            panic!("{out:?}")
        };
        assert_eq!(index, 3);
        assert_eq!(wardrobe.items, vec!["Gloves".to_string()]);
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Custom tool wardrobe item reference rendered empty; dropped"))
            .collect();
        assert_eq!(hits.len(), 1, "{lines:?}");
        let line = hits[0];
        for field in [
            "context=pascal.custom-tools",
            "tool=blank",
            "effect_index=3",
            "reference={{params.garment}}",
        ] {
            assert!(line.contains(field), "missing {field}: {line}");
        }
        // Key order: v4's `tool, effectIndex, reference` after the context.
        let at = |k: &str| line.find(k).unwrap();
        assert!(at("tool=") < at("effect_index=") && at("effect_index=") < at("reference="));
    }

    #[test]
    fn every_item_empty_skips_through_the_shared_helper_and_clear_slot_never_does() {
        let params: ResolvedParams = vec![("garment".into(), ResolvedValue::String("  ".into()))];
        let wear = gesture(WardrobeEffectAction::Wear, &["{{params.garment}}"]);
        let (out, _) =
            captured_with(|| resolve_wardrobe_gesture("blank", 0, &wear, &vars(&params), &skipped));
        assert_eq!(
            out,
            ResolvedEffect::Skipped {
                index: 0,
                reason: "every item reference rendered empty".into()
            }
        );
        // `clear_slot` with no items is a gesture, never a skip.
        let clear = CustomToolWardrobeGesture {
            action: WardrobeEffectAction::ClearSlot,
            items: None,
            mode: None,
            slot: Some("hair".into()),
        };
        let out = resolve_wardrobe_gesture("blank", 1, &clear, &vars(&params), &skipped);
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            r#"{"index":1,"wardrobe":{"action":"clear_slot","items":[],"slot":"hair"}}"#
        );
    }

    #[test]
    fn a_rendered_item_logs_nothing() {
        // The silence leg: nothing empties, so the line never fires.
        let params: ResolvedParams =
            vec![("garment".into(), ResolvedValue::String("Boots".into()))];
        let mut g = gesture(
            WardrobeEffectAction::Wear,
            &["  {{params.garment}} ", "Hat"],
        );
        g.mode = Some(PutOnMode::Replace);
        let (out, lines) =
            captured_with(|| resolve_wardrobe_gesture("dress", 0, &g, &vars(&params), &skipped));
        assert!(
            !lines.iter().any(|l| l.contains("rendered empty")),
            "{lines:?}"
        );
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            r#"{"index":0,"wardrobe":{"action":"wear","items":["Boots","Hat"],"mode":"replace"}}"#
        );
        assert!(out.is_applicable() && out.is_applicable_wardrobe() && !out.is_applicable_write());
    }
}

#[cfg(test)]
mod any_of_and_outcome_debug_tests {
    //! P4.D333: the three DEBUG lines v4 `a53024d3c` / `322a5488e` added to the
    //! run core, pinned on the caller thread with their silence legs — the
    //! any-of line AFTER its groups' own lines, the table-first line, and the
    //! two `{{outcome.*}}` render reasons. The behaviour around them is the
    //! execution family's (`pascal_custom_tools_execution_equivalence`); this
    //! module pins the bytes.
    use super::*;
    use crate::pascal::custom_tool_types::safe_parse;
    use crate::test_support::{captured, captured_with};
    use serde_json::json;
    use std::sync::Mutex;

    fn tool(doc: Value) -> QtapCustomTool {
        safe_parse(&doc).expect("the fixture loads")
    }

    fn subjects<'a>(
        params: &'a ResolvedParams,
        metadata: &'a Map<String, Value>,
    ) -> OutcomeSubjects<'a> {
        OutcomeSubjects {
            value: 1.0,
            roll: 1.0,
            params,
            metadata: Some(metadata),
            progress: None,
            llm: None,
            state: None,
            character: None,
            inventory: None,
        }
    }

    fn any_of_when() -> When {
        tool(json!({
            "name": "t", "description": "d",
            "outcomes": [
                { "when": [{ "metadata": { "a": { "eq": 1 } } }, { "metadata": { "b": { "eq": 1 } } }], "message": "-", "state": "info" },
                { "when": true, "message": "-", "state": "info" },
            ],
        }))
        .outcomes
        .remove(0)
        .when
    }

    #[test]
    fn an_any_of_miss_logs_once_after_every_groups_own_lines() {
        let params: ResolvedParams = Vec::new();
        let sheet = Map::new();
        let when = any_of_when();
        let lines = captured(|| {
            assert_eq!(
                matches_when(&when, &subjects(&params, &sheet), "t"),
                Ok(false)
            );
        });
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::pascal Custom tool metadata test did not match tool=t key=a reason=the character has no such metadata key".to_string(),
                "DEBUG quilltap::pascal Custom tool metadata test did not match tool=t key=b reason=the character has no such metadata key".to_string(),
                "DEBUG quilltap::pascal Custom tool any-of test context=pascal.custom-tools tool=t groups=2 heldGroup=null".to_string(),
            ]
        );
    }

    #[test]
    fn an_any_of_hit_names_its_group_and_stops_there() {
        let params: ResolvedParams = Vec::new();
        let sheet: Map<String, Value> = json!({ "b": 1 }).as_object().unwrap().clone();
        let when = any_of_when();
        let lines = captured(|| {
            assert_eq!(
                matches_when(&when, &subjects(&params, &sheet), "t"),
                Ok(true)
            );
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            lines[1],
            "DEBUG quilltap::pascal Custom tool any-of test context=pascal.custom-tools tool=t groups=2 heldGroup=1"
        );
    }

    #[test]
    fn a_bare_object_logs_no_any_of_line() {
        let params: ResolvedParams = Vec::new();
        let sheet: Map<String, Value> = json!({ "a": 1 }).as_object().unwrap().clone();
        let when = tool(json!({
            "name": "t", "description": "d",
            "outcomes": [
                { "when": { "metadata": { "a": { "eq": 1 } } }, "message": "-", "state": "info" },
                { "when": true, "message": "-", "state": "info" },
            ],
        }))
        .outcomes
        .remove(0)
        .when;
        let lines = captured(|| {
            assert_eq!(
                matches_when(&when, &subjects(&params, &sheet), "t"),
                Ok(true)
            );
        });
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// A canned consult that records the prompt it was posed.
    struct Recording(Mutex<Option<String>>);

    impl LlmInvoker for Recording {
        fn invoke<'a>(
            &'a self,
            prompt: &'a str,
            _options: LlmInvokeOptions,
        ) -> Pin<Box<dyn Future<Output = LlmInvokeResult> + Send + 'a>> {
            *self.0.lock().unwrap() = Some(prompt.to_string());
            Box::pin(async {
                LlmInvokeResult::Answered {
                    output: "A report.".into(),
                    provider: None,
                    model: None,
                }
            })
        }
    }

    fn scanner(outcomes: Value) -> QtapCustomTool {
        tool(json!({
            "name": "entanglement_scan",
            "description": "Scan a shard for entanglement.",
            "parameters": { "layers": { "type": "number", "default": 95 } },
            "roll": { "min": 1, "max": 1 },
            "llm": {
                "prompt": "State: {{outcome.state}}, row {{outcome.index}}. Reach: {{outcome.brief}}",
                "errorMessage": "The link falters.",
            },
            "outcomes": outcomes,
        }))
    }

    fn run(definition: &QtapCustomTool, invoker: &Recording) -> (CustomToolRunResult, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let (result, lines) = captured_with(|| {
            rt.block_on(execute_custom_tool(
                definition,
                None,
                None,
                None,
                None,
                &mut crate::tools::rng::FixedBytes::new(vec![]),
                Some(invoker),
                None,
                0,
                None,
                None,
            ))
        });
        (result.expect("dealt"), lines)
    }

    #[test]
    fn a_table_that_does_not_test_the_consult_is_dealt_first_and_says_so() {
        let definition = scanner(json!([
            { "when": { "roll": { "gte": { "$param": "layers" } } }, "message": "{{llm}}", "state": "success", "brief": "Confirmed." },
            { "when": true, "message": "{{llm}}", "state": "failure", "brief": "Not found at {{params.layers}}." },
        ]));
        let invoker = Recording(Mutex::new(None));
        let (result, lines) = run(&definition, &invoker);
        assert_eq!(result.outcome_index, 1);
        assert_eq!(
            invoker.0.lock().unwrap().as_deref(),
            Some("State: failure, row 1. Reach: Not found at 95.")
        );
        let dealt: Vec<&String> = lines.iter().filter(|l| l.contains("dealt ahead")).collect();
        assert_eq!(
            dealt,
            vec!["DEBUG quilltap::pascal Custom tool table dealt ahead of the consult context=pascal.custom-tools tool=entanglement_scan outcomeIndex=1 state=failure hasBrief=true"]
        );
    }

    #[test]
    fn a_winner_without_a_brief_says_so_and_leaves_the_placeholder() {
        let definition =
            scanner(json!([{ "when": true, "message": "{{llm}}", "state": "failure" }]));
        let invoker = Recording(Mutex::new(None));
        let (_, lines) = run(&definition, &invoker);
        assert_eq!(
            invoker.0.lock().unwrap().as_deref(),
            Some("State: failure, row 0. Reach: {{outcome.brief}}")
        );
        assert!(lines.contains(&"DEBUG quilltap::pascal Custom tool table dealt ahead of the consult context=pascal.custom-tools tool=entanglement_scan outcomeIndex=0 state=failure hasBrief=false".to_string()), "{lines:?}");
        assert!(lines.contains(&"DEBUG quilltap::pascal Custom tool template references {{outcome.*}} it cannot render context=pascal.custom-tools placeholder={{outcome.brief}} reason=the winning outcome has no brief".to_string()), "{lines:?}");
    }

    #[test]
    fn a_table_that_tests_the_consult_keeps_the_old_order_silently() {
        let definition = tool(json!({
            "name": "forged_invitation",
            "description": "Present a forged invitation.",
            "llm": { "prompt": "YES or NO?", "errorMessage": "Silence." },
            "outcomes": [
                { "when": { "llm": { "eq": "A report." } }, "message": "In ({{outcome.state}})", "state": "success" },
                { "when": true, "message": "Out", "state": "failure" },
            ],
        }));
        let invoker = Recording(Mutex::new(None));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let (result, lines) = captured_with(|| {
            rt.block_on(execute_custom_tool(
                &definition,
                None,
                None,
                None,
                None,
                &mut crate::tools::rng::FixedBytes::new(vec![0; 16]),
                Some(&invoker),
                None,
                0,
                None,
                None,
            ))
        });
        let result = result.expect("dealt");
        assert_eq!(result.message, "In (success)");
        assert!(
            !lines.iter().any(|l| l.contains("dealt ahead")),
            "{lines:?}"
        );
    }

    #[test]
    fn an_outcome_placeholder_before_the_deal_names_that_reason() {
        let params: ResolvedParams = Vec::new();
        let vars = TemplateVars {
            value: 0.0,
            roll: 0.0,
            dice: "",
            params: &params,
            metadata: None,
            progress: None,
            now: None,
            llm: None,
            state: None,
            character: None,
            inventory: None,
            outcome: None,
        };
        let lines = captured(|| {
            assert_eq!(
                render_template("x {{outcome.state}} y", &vars),
                "x {{outcome.state}} y"
            );
        });
        assert_eq!(
            lines,
            vec!["DEBUG quilltap::pascal Custom tool template references {{outcome.*}} it cannot render context=pascal.custom-tools placeholder={{outcome.state}} reason=the table has not dealt yet (or the consult is read by an outcome row)".to_string()]
        );
        // And once dealt, the state and index render silently.
        let winning = WinningOutcome {
            state: OutcomeState::Partial,
            index: 3,
            brief: None,
        };
        let dealt = TemplateVars {
            outcome: Some(&winning),
            ..vars
        };
        let lines = captured(|| {
            assert_eq!(
                render_template("{{outcome.state}} #{{outcome.index}}", &dealt),
                "partial #3"
            );
        });
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// The winning row serializes in v4's spread order — `state`, `index`,
    /// then `brief` only when the row has one (Tier 2 item 24).
    #[test]
    fn the_winning_outcome_serializes_in_v4_key_order() {
        let without = WinningOutcome {
            state: OutcomeState::Success,
            index: 0,
            brief: None,
        };
        assert_eq!(
            serde_json::to_string(&without).unwrap(),
            r#"{"state":"success","index":0}"#
        );
        let with = WinningOutcome {
            brief: Some("Confirmed.".into()),
            ..without
        };
        assert_eq!(
            serde_json::to_string(&with).unwrap(),
            r#"{"state":"success","index":0,"brief":"Confirmed."}"#
        );
    }
}
