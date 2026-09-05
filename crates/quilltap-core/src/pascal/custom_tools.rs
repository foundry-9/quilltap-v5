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
    is_state_ref_value, parse_effect_target, AnyOperand, CustomToolLlm, CustomToolParameter,
    EffectTarget, EffectValue, EffectWhen, LlmComparator, MetadataComparator, NumberOrParamRef,
    NumericComparator, OutcomeState, ParamComparator, ParameterType, QtapCustomTool, Roll,
    RollRange, StateRef, StateRefFallback, StringOperand, Visibility, When, WhenObject,
    MAX_LLM_OUTPUT_LENGTH,
};
use super::dice::{format_dice_breakdown, parse_dice_notation, roll_notation, RandomBytes};
use super::expressions::{evaluate_expression, parse_expression};
use super::js_value::{json_stringify, number_to_string, to_js_string, to_number};
use super::metadata_match::{js_primitive, metadata_comparator_holds};
use super::placeholders::{classify_placeholder, PlaceholderRef, PLACEHOLDER_PATTERN};
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
            coerce_param(&definition.name, name, spec, given, state)?,
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
) -> Result<ResolvedValue, CustomToolRunError> {
    // `undefined` and `null` both fall back to the declared default, which may
    // itself be a `$state` reference resolved against the merged state (its
    // fallback is type-checked against this parameter's type at load time, so
    // it always fits).
    let value = match value {
        None | Some(Value::Null) => {
            if is_state_ref_value(&spec.default) {
                if let Some(r) = state_ref_from_default(&spec.default) {
                    return Ok(resolve_state_value(&r, state));
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
) -> Result<f64, CustomToolRunError> {
    let Some(spec) = spec else {
        return Ok(fallback);
    };

    match spec {
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
    rng: &mut dyn RandomBytes,
) -> Result<(f64, f64), CustomToolRunError> {
    let name = &definition.name;
    let min = resolve_roll_field(name, "min", range.min.as_ref(), params, 0.0, state)?;
    let max = resolve_roll_field(name, "max", range.max.as_ref(), params, 1.0, state)?;
    let multiplier = resolve_roll_field(
        name,
        "multiplier",
        range.multiplier.as_ref(),
        params,
        1.0,
        state,
    )?;
    let offset = resolve_roll_field(name, "offset", range.offset.as_ref(), params, 0.0, state)?;

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
    /// The LLM consult's result. `None` when the definition declares no `llm`
    /// block — an `llm` test then fails soft and the table falls through.
    pub llm: Option<&'a LlmSubject>,
    /// The merged persistent state (chat → project → group → general) that
    /// `$state` comparator operands resolve against. `None` → treated as `{}`,
    /// so every `$state` operand falls to its own fallback.
    pub state: Option<&'a Value>,
}

/// A comparator operand as the evaluator sees it: a literal, a `$param`
/// reference, or a `$state` reference.
enum OperandSpec<'a> {
    Literal(ResolvedValue),
    Ref(&'a str),
    State(&'a StateRef),
}

fn number_operand(v: &NumberOrParamRef) -> OperandSpec<'_> {
    match v {
        NumberOrParamRef::Number(n) => OperandSpec::Literal(ResolvedValue::Number(*n)),
        NumberOrParamRef::ParamRef(r) => OperandSpec::Ref(&r.param),
        NumberOrParamRef::StateRef(r) => OperandSpec::State(r),
    }
}

fn any_operand(v: &AnyOperand) -> OperandSpec<'_> {
    match v {
        AnyOperand::Number(n) => OperandSpec::Literal(ResolvedValue::Number(*n)),
        AnyOperand::String(s) => OperandSpec::Literal(ResolvedValue::String(s.clone())),
        AnyOperand::Bool(b) => OperandSpec::Literal(ResolvedValue::Bool(*b)),
        AnyOperand::ParamRef(r) => OperandSpec::Ref(&r.param),
        AnyOperand::StateRef(r) => OperandSpec::State(r),
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
) -> Result<ResolvedValue, CustomToolRunError> {
    match operand {
        OperandSpec::Literal(v) => Ok(v.clone()),
        // A `$state` operand resolves against the merged state, falling back to
        // its own (load-typed) fallback — it never throws, matching the
        // metadata doctrine.
        OperandSpec::State(r) => Ok(resolve_state_value(r, state)),
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
) -> Result<bool, CustomToolRunError> {
    for (key, operand) in operands {
        let Some(operand) = operand else { continue };
        let label = format!("{subject_label} {key}");

        if matches!(key, "gt" | "gte" | "lt" | "lte") {
            // v4 evaluates the SUBJECT first, so a non-orderable subject is what
            // is reported even when the operand is also broken.
            let a = require_number(tool_name, subject, subject_label)?;
            let resolved = resolve_operand(tool_name, &operand, params, &label, state)?;
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
            let resolved = resolve_operand(tool_name, &operand, params, &label, state)?;
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

        let resolved = resolve_operand(tool_name, &operand, params, &label, state)?;
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
fn matches_metadata_comparator(
    tool_name: &str,
    comparator: &MetadataComparator,
    key: &str,
    metadata: &Map<String, Value>,
    params: &ResolvedParams,
    state: Option<&Value>,
) -> Result<bool, CustomToolRunError> {
    let operands = param_operands(comparator);
    metadata_comparator_holds(
        comparator,
        key,
        metadata,
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
                &format!("metadata \"{key}\" {comparator_key}"),
                state,
            )
        },
        // v4's `logger.debug('Custom tool metadata test did not match', {…})`,
        // at the same point with the same fields (the P4.18 tracing surface, per
        // the `llm_consult` precedent: v4's `context` becomes the target).
        &mut |reason| {
            tracing::debug!(
                target: "quilltap::pascal",
                tool = tool_name,
                key,
                reason,
                "Custom tool metadata test did not match",
            );
        },
    )
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
        let resolved = resolve_operand(tool_name, operand, params, &label, state)?;
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
                let r = resolve_operand(tool_name, operand, params, &label, state)?;
                equals_answer(&r)
            }
            "neq" => {
                let r = resolve_operand(tool_name, operand, params, &label, state)?;
                !equals_answer(&r)
            }
            "contains" => {
                let r = resolve_operand(tool_name, operand, params, &label, state)?;
                contains_answer(&r)
            }
            "ncontains" => {
                let r = resolve_operand(tool_name, operand, params, &label, state)?;
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
pub fn collect_metadata_tested(
    when: &When,
    metadata: Option<&Map<String, Value>>,
) -> Option<MetadataTested> {
    let When::Object(w) = when else {
        return None;
    };
    let tested_keys = w.metadata.as_ref()?;
    let metadata = metadata?;

    let mut tested = Vec::new();
    for (key, _comparator) in tested_keys {
        if let Some(v) = metadata.get(key) {
            if let Some(prim) = js_primitive(v) {
                tested.push((key.clone(), prim));
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
    let when = match when {
        When::CatchAll(_) => return Ok(true),
        When::Object(w) => w,
    };
    matches_when_object(when, subjects, tool_name)
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
    let params = subjects.params;
    // v4 `subjects.state ?? {}` — `None` behaves as the empty object, so every
    // `$state` operand falls to its own fallback.
    let state = subjects.state;
    let value = ResolvedValue::Number(subjects.value);

    if !matches_comparator(
        tool_name,
        when_operands(when),
        &value,
        "the rolled value",
        params,
        state,
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
            if !matches_metadata_comparator(tool_name, comparator, key, metadata, params, state)? {
                return Ok(false);
            }
        }
    }

    if let Some(comparator) = &when.llm {
        // No consult ran (a definition without an `llm` block, or a simulation
        // that supplied none): the test fails soft, exactly like a metadata key
        // the character doesn't carry.
        let Some(llm) = subjects.llm else {
            return Ok(false);
        };
        if !matches_llm_comparator(tool_name, comparator, llm, params, state)? {
            return Ok(false);
        }
    }

    Ok(true)
}

/// The winning outcome, as an effect condition sees it — the one subject that
/// exists only after the deal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinningOutcome {
    pub state: OutcomeState,
    pub index: usize,
}

/// What an effect condition may be posed about: the run's subjects, plus the
/// winning outcome (v4 `EffectSubjects extends OutcomeSubjects`).
pub struct EffectSubjects<'a> {
    pub base: OutcomeSubjects<'a>,
    pub outcome: WinningOutcome,
    /// Dice breakdown for `{{dice}}` in expressions — a string, `""` for Form A.
    pub dice: &'a str,
}

/// Evaluate an effect's condition. Delegates the shared subjects to the same
/// comparator chain outcome rows use, and adds the one subject only an effect
/// can test — the winning outcome's semantic state.
pub fn matches_effect_when(
    when: Option<&EffectWhen>,
    subjects: &EffectSubjects,
    tool_name: &str,
) -> Result<bool, CustomToolRunError> {
    let Some(when) = when else { return Ok(true) };

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

    matches_when_object(&when.base, &subjects.base, tool_name)
}

/// One effect, resolved by the pure core: either the value it would write, or
/// the reason it was skipped. The entrances (and only they) apply the former;
/// the Proving Bench shows both as a dry run.
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
    Skipped {
        index: usize,
        reason: String,
    },
}

impl ResolvedEffect {
    /// True for a resolved effect that would actually write (v4
    /// `isApplicableEffect`).
    pub fn is_applicable(&self) -> bool {
        matches!(self, ResolvedEffect::Applicable { .. })
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
            ResolvedEffect::Skipped { index, reason } => {
                m.serialize_entry("index", index)?;
                m.serialize_entry("skipped", reason)?;
            }
        }
        m.end()
    }
}

/// v4's `EffectTarget` object literals: `{ kind: 'state', path, raw }` and
/// `{ kind: 'metadata', key, raw }`, in that key order. A path segment is a
/// string or a number, exactly as `parsePath` produces it.
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
        llm: subjects.base.llm,
        state: subjects.base.state,
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

            let target = match parse_effect_target(&effect.target) {
                Ok(t) => t,
                Err(reason) => return skipped(index, format!("target {reason}")),
            };

            let EffectValue::Str(source) = &effect.value else {
                let value = match &effect.value {
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

/// Render a number for display: integers plain, floats to 4 significant digits.
///
/// Lives in [`super::expressions`] since v4 `c4d4b0de` moved it there — one
/// number-rendering convention for templates and effect expressions alike — and
/// is re-exported here, as v4 re-exports it, for every existing importer.
pub use super::expressions::format_value;

/// The substitutions [`render_template`] draws on.
pub struct TemplateVars<'a> {
    pub value: f64,
    pub roll: f64,
    pub dice: &'a str,
    pub params: &'a ResolvedParams,
    pub metadata: Option<&'a Map<String, Value>>,
    /// The consult's result. `None` while rendering the consult's own prompt.
    pub llm: Option<&'a LlmSubject>,
    /// The merged persistent state, for `{{state.path}}` placeholders.
    pub state: Option<&'a Value>,
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
            match resolve_placeholder_value(&classify_placeholder(key), vars) {
                // Numbers render through `format_value`; anything else through
                // its JS string form.
                Some(ResolvedValue::Number(n)) => format_value(n),
                Some(ResolvedValue::String(s)) => s,
                Some(ResolvedValue::Bool(b)) => b.to_string(),
                // Nothing renderable: leave the hole visible. v4 logs WHY at
                // debug (no such metadata key / not a primitive / unknown
                // placeholder); v5's renderer has never carried those debug
                // lines, and this collapse does not add them.
                None => whole.to_string(),
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
/// in a debug log v5 does not emit; its resolver simply skips the effect.)
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
        PlaceholderRef::Params { name } => lookup(vars.params, name).cloned(),
        PlaceholderRef::Metadata { key } => vars
            .metadata
            .and_then(|m| m.get(key))
            .and_then(js_primitive),
        PlaceholderRef::State { path } => {
            // `state.` is stripped and the remainder is a full state path
            // (v4 `f48f34dc`).
            let empty = Value::Object(Map::new());
            let state = vars.state.unwrap_or(&empty);
            get_at_path(state, &parse_path(Some(path))).and_then(|v| js_primitive(&v))
        }
        PlaceholderRef::Unknown { .. } => None,
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
) -> Result<CustomToolRunResult, CustomToolRunError> {
    let params = resolve_params(definition, supplied_params, state)?;

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
        other => {
            roll_form = RollForm::Range;
            let default = RollRange::default();
            let range = match other {
                Some(Roll::Range(r)) => r,
                _ => &default,
            };
            let (drawn_raw, drawn_value) = roll_range(definition, range, &params, state, rng)?;
            raw = drawn_raw;
            value = drawn_value;
        }
    }

    // The consult runs AFTER the roll — its prompt may quote the draw — and
    // BEFORE the table, which may test its answer. The prompt is rendered
    // WITHOUT an `llm` var: there is no answer yet to quote.
    let llm = match &definition.llm {
        Some(spec) => {
            let prompt = render_template(
                &spec.prompt,
                &TemplateVars {
                    value,
                    roll: raw,
                    dice: &dice_breakdown,
                    params: &params,
                    metadata,
                    llm: None,
                    state,
                },
            );
            Some(resolve_llm_consult(spec, prompt, llm_invoke).await)
        }
        None => None,
    };
    let llm_subject = llm.as_ref().map(|c| c.subject());

    let subjects = OutcomeSubjects {
        value,
        roll: raw,
        params: &params,
        metadata,
        llm: llm_subject.as_ref(),
        state,
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
    let outcome = &definition.outcomes[outcome_index];

    let message = render_template(
        &outcome.message,
        &TemplateVars {
            value,
            roll: raw,
            dice: &dice_breakdown,
            params: &params,
            metadata,
            llm: llm_subject.as_ref(),
            state,
        },
    );
    let metadata_tested = collect_metadata_tested(&outcome.when, metadata);

    // F1 — the chip label, rendered AFTER the outcome is chosen so it may quote
    // everything the message may. This is the one render site; both entrances
    // copy the result, so the chip and the bubble header can never drift.
    let chip_label = definition.chip_label.as_ref().map(|template| {
        render_template(
            template,
            &TemplateVars {
                value,
                roll: raw,
                dice: &dice_breakdown,
                params: &params,
                metadata,
                llm: llm_subject.as_ref(),
                state,
            },
        )
    });

    // F3 — effects, resolved pure against the finished run. Nothing is written
    // here; the entrances decide whether (and where) the writes land.
    let effects = match &definition.effects {
        Some(declared) if !declared.is_empty() => {
            let effect_subjects = EffectSubjects {
                base: OutcomeSubjects {
                    value,
                    roll: raw,
                    params: &params,
                    metadata,
                    llm: llm_subject.as_ref(),
                    state,
                },
                outcome: WinningOutcome {
                    state: outcome.state,
                    index: outcome_index,
                },
                dice: &dice_breakdown,
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
) -> Result<CustomToolAuditResult, CustomToolRunError> {
    let params = resolve_params(definition, supplied_params, state)?;

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
            None => roll_range(definition, range_roll, &params, state, rng)?,
        };

        let subjects = OutcomeSubjects {
            value,
            roll: raw,
            params: &params,
            metadata,
            llm,
            state,
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
        let r = simulate_outcomes(&d, None, 20_000, None, None, None, &mut rng).unwrap();
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
        )
        .unwrap();
        assert_eq!(empty.outcomes[0].hits, 0);

        let mut sheet = serde_json::Map::new();
        sheet.insert("luck".into(), serde_json::json!(7));
        let carrying =
            simulate_outcomes(&d, None, 100, Some(&sheet), None, None, &mut rng).unwrap();
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
        assert!(simulate_outcomes(&d, None, 10, None, None, None, &mut rng).is_err());
    }
}
