//! Custom Tools — the shared execution core (v4 `lib/pascal/custom-tools.ts`,
//! the `Execution core` half).
//!
//! Two entrances share this core: the `run_custom` tool (a model rolls) and the
//! composer popup (the human rolls). Both land on [`execute_custom_tool`] so a
//! roll means the same thing whoever asked for it.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use regex::{Captures, Regex};
use serde_json::{Map, Value};
use std::sync::LazyLock;

use super::custom_tool_types::{
    AnyOperand, CustomToolLlm, CustomToolParameter, LlmComparator, MetadataComparator,
    NumberOrParamRef, NumericComparator, OutcomeState, ParamComparator, ParameterType,
    QtapCustomTool, Roll, RollRange, StringOperand, Visibility, When, WhenObject,
    MAX_LLM_OUTPUT_LENGTH,
};
use super::dice::{format_dice_breakdown, parse_dice_notation, roll_notation, RandomBytes};
use super::js_value::{json_stringify, number_to_string, to_js_string, to_number, to_precision};
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

/// One resolved parameter value: v4's `number | string | boolean`.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedValue {
    Number(f64),
    String(String),
    Bool(bool),
}

impl ResolvedValue {
    /// JS `===` across the three types a parameter can hold.
    fn strict_eq(&self, other: &ResolvedValue) -> bool {
        match (self, other) {
            (ResolvedValue::Number(a), ResolvedValue::Number(b)) => a == b,
            (ResolvedValue::String(a), ResolvedValue::String(b)) => a == b,
            (ResolvedValue::Bool(a), ResolvedValue::Bool(b)) => a == b,
            _ => false,
        }
    }

    /// As a JSON [`Value`], the way v4 stores a resolved param / tested-metadata
    /// value: a number is stored JS-bare (an integer without a fractional part),
    /// a string as a string, a boolean as a boolean.
    pub fn to_value(&self) -> Value {
        match self {
            ResolvedValue::Number(n) => crate::db::js_number_to_json(*n),
            ResolvedValue::String(s) => Value::String(s.clone()),
            ResolvedValue::Bool(b) => Value::Bool(*b),
        }
    }

    /// The `JSON.stringify(value)` several error sentences interpolate.
    fn stringify(&self) -> String {
        match self {
            ResolvedValue::Number(n) => json_stringify(&crate::db::js_number_to_json(*n)),
            ResolvedValue::String(s) => json_stringify(&Value::String(s.clone())),
            ResolvedValue::Bool(b) => json_stringify(&Value::Bool(*b)),
        }
    }
}

/// Resolved parameter values, post-default and post-clamp. Ordered, mirroring
/// the declaration order v4's `Object.entries` walks.
pub type ResolvedParams = Vec<(String, ResolvedValue)>;

/// The metadata keys a run actually consulted, and what they held at the time —
/// v4's `MetadataTested = Record<string, number | string | boolean>`. Ordered,
/// mirroring the winning `when.metadata` key order v4's `Object.keys` walks.
pub type MetadataTested = Vec<(String, ResolvedValue)>;

/// v4 `isPrimitive` folded with the JSON-value coercion: the value types a
/// comparator can compare, as a [`ResolvedValue`]. `None` for null/array/object.
fn js_primitive(v: &Value) -> Option<ResolvedValue> {
    match v {
        Value::Number(n) => Some(ResolvedValue::Number(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => Some(ResolvedValue::String(s.clone())),
        Value::Bool(b) => Some(ResolvedValue::Bool(*b)),
        _ => None,
    }
}

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
}

/// Validate and coerce caller-supplied parameters against the declarations.
///
/// Unknown keys are rejected rather than ignored: a model passing `bonuss: 5`
/// has misunderstood the tool, and silently rolling without the bonus would look
/// like the tool worked.
pub fn resolve_params(
    definition: &QtapCustomTool,
    supplied: Option<&serde_json::Map<String, Value>>,
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
            coerce_param(&definition.name, name, spec, given)?,
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
) -> Result<ResolvedValue, CustomToolRunError> {
    // `undefined` and `null` both fall back to the declared default.
    let value = match value {
        None | Some(Value::Null) => return Ok(default_value(spec)),
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
) -> Result<f64, CustomToolRunError> {
    let Some(spec) = spec else {
        return Ok(fallback);
    };

    match spec {
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
    rng: &mut dyn RandomBytes,
) -> Result<(f64, f64), CustomToolRunError> {
    let name = &definition.name;
    let min = resolve_roll_field(name, "min", range.min.as_ref(), params, 0.0)?;
    let max = resolve_roll_field(name, "max", range.max.as_ref(), params, 1.0)?;
    let multiplier =
        resolve_roll_field(name, "multiplier", range.multiplier.as_ref(), params, 1.0)?;
    let offset = resolve_roll_field(name, "offset", range.offset.as_ref(), params, 0.0)?;

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
}

/// A comparator operand as the evaluator sees it: a literal, or a reference.
enum OperandSpec<'a> {
    Literal(ResolvedValue),
    Ref(&'a str),
}

fn number_operand(v: &NumberOrParamRef) -> OperandSpec<'_> {
    match v {
        NumberOrParamRef::Number(n) => OperandSpec::Literal(ResolvedValue::Number(*n)),
        NumberOrParamRef::ParamRef(r) => OperandSpec::Ref(&r.param),
    }
}

fn any_operand(v: &AnyOperand) -> OperandSpec<'_> {
    match v {
        AnyOperand::Number(n) => OperandSpec::Literal(ResolvedValue::Number(*n)),
        AnyOperand::String(s) => OperandSpec::Literal(ResolvedValue::String(s.clone())),
        AnyOperand::Bool(b) => OperandSpec::Literal(ResolvedValue::Bool(*b)),
        AnyOperand::ParamRef(r) => OperandSpec::Ref(&r.param),
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
) -> Result<ResolvedValue, CustomToolRunError> {
    match operand {
        OperandSpec::Literal(v) => Ok(v.clone()),
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
) -> Result<bool, CustomToolRunError> {
    for (key, operand) in operands {
        let Some(operand) = operand else { continue };
        let label = format!("{subject_label} {key}");

        if matches!(key, "gt" | "gte" | "lt" | "lte") {
            // v4 evaluates the SUBJECT first, so a non-orderable subject is what
            // is reported even when the operand is also broken.
            let a = require_number(tool_name, subject, subject_label)?;
            let resolved = resolve_operand(tool_name, &operand, params, &label)?;
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
            let resolved = resolve_operand(tool_name, &operand, params, &label)?;
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

        let resolved = resolve_operand(tool_name, &operand, params, &label)?;
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
/// Everything `matches_comparator` treats as a regression worth throwing over
/// is, here, an ordinary fact about a character: the key may not exist, or may
/// hold a list, or may hold a string where the table wanted a number. Metadata
/// keys are undeclared by nature — no load-time check could have caught any of
/// it — so each of those simply fails to match, the row is passed over, and the
/// table's mandatory catch-all does what the author wrote it to do.
///
/// `$param` operands still throw if they don't resolve: those ARE load-validated,
/// so a failure there is a regression rather than a fact about the character.
fn matches_metadata_comparator(
    tool_name: &str,
    comparator: &MetadataComparator,
    key: &str,
    metadata: &Map<String, Value>,
    params: &ResolvedParams,
) -> Result<bool, CustomToolRunError> {
    // The character has no such metadata key — decline before touching operands,
    // so a `$param` operand is never even resolved (let alone thrown over).
    let Some(raw) = metadata.get(key) else {
        return Ok(false);
    };
    // The key holds null/array/object — cannot be compared.
    let Some(subject) = js_primitive(raw) else {
        return Ok(false);
    };

    let label = format!("metadata \"{key}\"");
    for (ck, operand) in param_operands(comparator) {
        let Some(operand) = operand else { continue };
        let op_label = format!("{label} {ck}");

        if matches!(ck, "gt" | "gte" | "lt" | "lte") {
            // v4 resolves the operand FIRST (so a `$param` still throws), THEN
            // declines when either side is not a number.
            let resolved = resolve_operand(tool_name, &operand, params, &op_label)?;
            let (ResolvedValue::Number(s), ResolvedValue::Number(o)) = (&subject, &resolved) else {
                return Ok(false);
            };
            let held = match ck {
                "gt" => s > o,
                "gte" => s >= o,
                "lt" => s < o,
                _ => s <= o,
            };
            if !held {
                return Ok(false);
            }
            continue;
        }

        if matches!(ck, "contains" | "ncontains") {
            // Containment follows the same fail-soft rule as ordering: a key
            // holding anything but a string cannot be searched, so the row
            // declines — including under ncontains, where (as with neq)
            // absence-of-a-string is not a miss.
            let resolved = resolve_operand(tool_name, &operand, params, &op_label)?;
            let (ResolvedValue::String(hay), ResolvedValue::String(needle)) = (&subject, &resolved)
            else {
                return Ok(false);
            };
            let held = hay.contains(needle.as_str());
            if ck == "contains" {
                if !held {
                    return Ok(false);
                }
            } else if held {
                return Ok(false);
            }
            continue;
        }

        let resolved = resolve_operand(tool_name, &operand, params, &op_label)?;
        let held = if ck == "eq" {
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
        let resolved = resolve_operand(tool_name, operand, params, &label)?;
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
                let r = resolve_operand(tool_name, operand, params, &label)?;
                equals_answer(&r)
            }
            "neq" => {
                let r = resolve_operand(tool_name, operand, params, &label)?;
                !equals_answer(&r)
            }
            "contains" => {
                let r = resolve_operand(tool_name, operand, params, &label)?;
                contains_answer(&r)
            }
            "ncontains" => {
                let r = resolve_operand(tool_name, operand, params, &label)?;
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

    let params = subjects.params;
    let value = ResolvedValue::Number(subjects.value);

    if !matches_comparator(
        tool_name,
        when_operands(when),
        &value,
        "the rolled value",
        params,
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
            if !matches_metadata_comparator(tool_name, comparator, key, metadata, params)? {
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
        if !matches_llm_comparator(tool_name, comparator, llm, params)? {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Render a number for display: integers plain, floats to 4 significant digits.
pub fn format_value(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        return number_to_string(value);
    }
    // `toPrecision` can hand back exponential form for extremes; Number() folds
    // that back to the shortest faithful representation.
    let rounded = to_precision(value, 4).parse::<f64>().unwrap_or(value);
    number_to_string(rounded)
}

static TEMPLATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{([^}]+)\}\}").expect("template pattern compiles"));

/// The substitutions [`render_template`] draws on.
pub struct TemplateVars<'a> {
    pub value: f64,
    pub roll: f64,
    pub dice: &'a str,
    pub params: &'a ResolvedParams,
    pub metadata: Option<&'a Map<String, Value>>,
    /// The consult's result. `None` while rendering the consult's own prompt.
    pub llm: Option<&'a LlmSubject>,
}

/// Substitute the four placeholder families. Plain string replacement — user
/// text is never interpreted, and an unknown placeholder is left as written.
///
/// `{{metadata.key}}` follows that same leave-it-as-written rule when the key is
/// absent or holds a list or an object: the placeholder left standing tells the
/// author exactly which key their character lacks.
pub fn render_template(message: &str, vars: &TemplateVars) -> String {
    TEMPLATE_RE
        .replace_all(message, |caps: &Captures| {
            let whole = caps.get(0).map_or("", |m| m.as_str());
            let key = js_trim(caps.get(1).map_or("", |m| m.as_str()));

            if key == "value" {
                return format_value(vars.value);
            }
            if key == "roll" {
                return format_value(vars.roll);
            }
            if key == "dice" {
                return vars.dice.to_string();
            }

            if key == "llm" {
                // No consult to render (the prompt's own pass, or a tool with no
                // `llm` block): the placeholder is left as written.
                return match vars.llm {
                    Some(llm) => llm.output.clone(),
                    None => whole.to_string(),
                };
            }

            if let Some(name) = key.strip_prefix("params.") {
                if let Some(v) = lookup(vars.params, name) {
                    return match v {
                        ResolvedValue::Number(n) => format_value(*n),
                        ResolvedValue::String(s) => s.clone(),
                        ResolvedValue::Bool(b) => b.to_string(),
                    };
                }
            }

            if let Some(name) = key.strip_prefix("metadata.") {
                // A primitive renders (numbers via `format_value`, else String);
                // a missing key or a list/object leaves the placeholder verbatim.
                if let Some(v) = vars
                    .metadata
                    .and_then(|m| m.get(name))
                    .and_then(js_primitive)
                {
                    return match v {
                        ResolvedValue::Number(n) => format_value(n),
                        ResolvedValue::String(s) => s,
                        ResolvedValue::Bool(b) => b.to_string(),
                    };
                }
                return whole.to_string();
            }

            // An unknown placeholder is left verbatim — v4 logs it at debug.
            whole.to_string()
        })
        .into_owned()
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
    rng: &mut (dyn RandomBytes + Send),
    llm_invoke: Option<&dyn LlmInvoker>,
) -> Result<CustomToolRunResult, CustomToolRunError> {
    let params = resolve_params(definition, supplied_params)?;

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
            let (drawn_raw, drawn_value) = roll_range(definition, range, &params, rng)?;
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
        },
    );
    let metadata_tested = collect_metadata_tested(&outcome.when, metadata);

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
    rng: &mut dyn RandomBytes,
) -> Result<CustomToolAuditResult, CustomToolRunError> {
    let params = resolve_params(definition, supplied_params)?;

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
            None => roll_range(definition, range_roll, &params, rng)?,
        };

        let subjects = OutcomeSubjects {
            value,
            roll: raw,
            params: &params,
            metadata,
            llm,
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
        let r = simulate_outcomes(&d, None, 20_000, None, None, &mut rng).unwrap();
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
        let empty = simulate_outcomes(&d, None, 100, Some(&serde_json::Map::new()), None, &mut rng)
            .unwrap();
        assert_eq!(empty.outcomes[0].hits, 0);

        let mut sheet = serde_json::Map::new();
        sheet.insert("luck".into(), serde_json::json!(7));
        let carrying = simulate_outcomes(&d, None, 100, Some(&sheet), None, &mut rng).unwrap();
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
        assert!(simulate_outcomes(&d, None, 10, None, None, &mut rng).is_err());
    }
}
