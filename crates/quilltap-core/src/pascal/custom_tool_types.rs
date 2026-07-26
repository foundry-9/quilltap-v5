//! Custom Tools — the definition schema for Pascal's table (v4
//! `lib/pascal/custom-tool.types.ts`).
//!
//! A custom tool is a single JSON document (`Tools/*.tool.json` at the root of
//! any document store) describing a named action with parameters, a random roll,
//! and an ordered table of outcomes mapping the roll to a message and a semantic
//! state. This module is the single source of truth for that format: the schema
//! here validates every definition at load time, and the published JSON Schema
//! at v4's `public/schemas/qtap-custom-tool.schema.json` mirrors it for editor
//! completion.
//!
//! Design constraint that shapes everything below: **no expression evaluation,
//! anywhere.** Outcome tests are AND-composed comparator objects, and the only
//! indirection is a `{ "$param": "name" }` reference. There is no string grammar
//! to parse, so there is nothing to inject into.
//!
//! # Why this file reimplements a slice of Zod
//!
//! v4 validates with Zod, and [`format_definition_issues`] renders the result as
//! the sentence an author reads on the load-error badge. That sentence is not a
//! debugging aid: `load_tools_from_mount` stores it as `CustomToolLoadError`'s
//! `reason`, and the `/api/v1/chats/{id}/custom-tools` GET route returns it
//! verbatim in `errors[]`. It is user-visible payload on a diffed route, so the
//! port has to reproduce Zod 4.4.3's own messages and — because the join is
//! `"; "` — its issue ORDER too. Three of its rules are load-bearing here and
//! are the reason this is a small engine rather than a pile of `if`s:
//!
//! 1. An issue is **aborting** unless it came from a check (a `refine` /
//!    `superRefine`), which marks its issues `continue: true`.
//! 2. **Checks are skipped when the value already has an aborting issue** — that
//!    is what stops the top-level `superRefine` from walking an `outcomes` that
//!    failed to parse. When only continuable issues exist, the value still
//!    exists and the checks DO run.
//! 3. A union whose branches all fail reports `invalid_union` — **unless exactly
//!    one branch is non-aborted**, in which case that branch's issues are hoisted
//!    verbatim and no union wrapper appears (Zod's `handleUnionResults`). This is
//!    why a malformed dice notation reads as `roll: must be dice notation …`
//!    rather than being buried under an `Invalid input` at the union.
//!
//! # The one recorded divergence: overflow literals
//!
//! `z.number().finite()` is reachable only via a JSON literal that overflows to
//! `Infinity` (JSON has no `Infinity`/`NaN` literal), e.g. `{"gt": 1e999}`. v4
//! parses that to `Infinity` and rejects it HERE, at the schema. serde_json
//! rejects it one layer earlier, at the parse ("number out of range"), so v5
//! rejects the same file at [`super::custom_tools::read_tool_file`] instead.
//! Both refuse the definition; only the reason string differs, and it differs
//! below this schema. [`FINITE_EXPECTED`] is carried faithfully regardless, so
//! the check behaves if a non-finite value ever reaches it by another road.

use std::sync::LazyLock;

use regex::Regex;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::Value;

use super::dice::{parse_dice_notation, MAX_DIE_SIDES, MIN_DIE_SIDES};

/// Well-known folder, at a store's root, holding custom-tool definitions.
pub const TOOLS_FOLDER: &str = "Tools";

/// Filename suffix that marks a document as a custom-tool definition.
pub const TOOL_FILE_SUFFIX: &str = ".tool.json";

/// Cap on parameters per tool.
pub const MAX_PARAMETERS: usize = 8;

/// Cap on outcomes per tool.
pub const MAX_OUTCOMES: usize = 32;

/// Cap on tools in a single resolved roster.
pub const MAX_ROSTER_SIZE: usize = 64;

/// Cap on an outcome message, in characters.
pub const MAX_MESSAGE_LENGTH: usize = 1000;

/// Cap on a tool description, in characters.
pub const MAX_DESCRIPTION_LENGTH: usize = 500;

/// Cap on a tool's display title, in characters.
pub const MAX_TITLE_LENGTH: usize = 80;

/// Cap on an LLM consult prompt, in characters (measured before templating).
pub const MAX_LLM_PROMPT_LENGTH: usize = 4000;

/// Default cap on the answer an LLM consult may contribute, in characters. The
/// model's reply is trimmed and truncated before it is tested or rendered.
/// A definition that wants a longer (or shorter) leash sets `llm.maxOutput`;
/// this is only what applies when it doesn't say.
pub const MAX_LLM_OUTPUT_LENGTH: usize = 8000;

/// Hard ceiling on `llm.maxOutput`. Generous on purpose — a consult may well BE
/// the deliverable (a generated document, a deep outline) — but still bounded:
/// the answer lands in `pascalMeta` on a chat_messages row, and rows must not
/// become arbitrarily large because one oracle would not stop talking.
pub const MAX_LLM_OUTPUT_CEILING: i64 = 100_000;

/// Identifier rules shared by tool names and parameter names: lowercase, starts
/// with a letter, 1–64 characters. (v4 `IDENTIFIER_PATTERN`.)
pub static IDENTIFIER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_-]{0,63}$").expect("identifier pattern compiles"));

const IDENTIFIER_MESSAGE: &str = "must be lowercase, start with a letter, and be 1–64 characters";

const AT_LEAST_ONE: &str = "must specify at least one comparator (gt, gte, lt, lte, eq, neq)";

/// The wide form, for the comparator schemas that carry containment too.
const AT_LEAST_ONE_WIDE: &str =
    "must specify at least one comparator (gt, gte, lt, lte, eq, neq, contains, ncontains)";

/// v4's `z.number().finite()` expectation string. See the module note on the one
/// recorded divergence — this is unreachable from a serde_json parse.
const FINITE_EXPECTED: &str = "number";

/// The comparator keys, in the order tests are described to a reader.
pub const COMPARATOR_KEYS: [&str; 8] = [
    "gt",
    "gte",
    "lt",
    "lte",
    "eq",
    "neq",
    "contains",
    "ncontains",
];

/// The six keys of v4's `NUMERIC_COMPARATOR_SHAPE` — the shape a bare `when`,
/// a `roll` test, and a numeric comparator all carry. Deliberately NOT the full
/// [`COMPARATOR_KEYS`]: containment never appears on a numeric subject, so
/// `contains` at those positions is an unrecognized key, not a comparator.
const NUMERIC_COMPARATOR_KEYS: [&str; 6] = ["gt", "gte", "lt", "lte", "eq", "neq"];

/// The four keys that order two values, and so demand numbers on both sides.
const ORDERING_KEYS: [&str; 4] = ["gt", "gte", "lt", "lte"];

/// The two keys that search one string inside another, and so demand strings.
const CONTAINMENT_KEYS: [&str; 2] = ["contains", "ncontains"];

// ---------------------------------------------------------------- issue model

/// One rejection, in Zod's shape. `cont` mirrors Zod's `continue` flag: true
/// only for issues raised by a check, which is what makes them non-aborting.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub path: Vec<String>,
    pub message: String,
    pub cont: bool,
    /// Present only for `invalid_union`: each branch's issues, with paths left
    /// RELATIVE to the union (v4's `flattenIssues` re-walks them without the
    /// prefix, so they must not be prefixed here).
    pub union_errors: Option<Vec<Vec<Issue>>>,
}

impl Issue {
    /// An aborting issue — anything not raised by a check.
    fn hard(message: impl Into<String>) -> Self {
        Issue {
            path: Vec::new(),
            message: message.into(),
            cont: false,
            union_errors: None,
        }
    }

    /// A continuable issue — Zod's `ctx.addIssue` / `refine` shape.
    fn check(message: impl Into<String>) -> Self {
        Issue {
            path: Vec::new(),
            message: message.into(),
            cont: true,
            union_errors: None,
        }
    }

    fn at(mut self, seg: impl Into<String>) -> Self {
        self.path.insert(0, seg.into());
        self
    }
}

/// v4 `util.aborted`: a payload is aborted once any issue is not continuable.
fn aborted(issues: &[Issue]) -> bool {
    issues.iter().any(|i| !i.cont)
}

/// Prefix every issue's path with `seg`. Deliberately does NOT touch
/// `union_errors` — Zod keeps branch paths relative to the union.
fn prefix(seg: &str, issues: Vec<Issue>) -> Vec<Issue> {
    issues.into_iter().map(|i| i.at(seg)).collect()
}

/// A parse result: the value when one could be built, plus any issues.
struct Res<T> {
    value: Option<T>,
    issues: Vec<Issue>,
}

impl<T> Res<T> {
    fn ok(value: T) -> Self {
        Res {
            value: Some(value),
            issues: Vec::new(),
        }
    }

    /// Shape failure — no value, and the issue aborts.
    fn hard(message: impl Into<String>) -> Self {
        Res {
            value: None,
            issues: vec![Issue::hard(message)],
        }
    }
}

/// v4 `handleUnionResults`. The middle arm is the subtle one: when exactly one
/// branch is non-aborted, Zod returns THAT branch rather than wrapping, which is
/// how a failed `refine` inside one branch surfaces its own sentence instead of
/// a bare "Invalid input" at the union.
fn union<T>(branches: Vec<Res<T>>) -> Res<T> {
    let mut branches = branches;

    if let Some(idx) = branches.iter().position(|b| b.issues.is_empty()) {
        return branches.swap_remove(idx);
    }

    let nonaborted: Vec<usize> = (0..branches.len())
        .filter(|&i| !aborted(&branches[i].issues))
        .collect();
    if nonaborted.len() == 1 {
        return branches.swap_remove(nonaborted[0]);
    }

    let errors = branches.into_iter().map(|b| b.issues).collect();
    Res {
        value: None,
        issues: vec![Issue {
            path: Vec::new(),
            message: "Invalid input".into(),
            cont: false,
            union_errors: Some(errors),
        }],
    }
}

/// v4 `util.parsedType`. `None` is JS `undefined` — a missing key.
fn parsed_type(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

fn invalid_type(expected: &str, got: Option<&Value>) -> String {
    format!(
        "Invalid input: expected {expected}, received {}",
        parsed_type(got)
    )
}

// ------------------------------------------------------------------- the types

/// The four parameter types a definition may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    Number,
    Integer,
    String,
    Boolean,
}

impl ParameterType {
    /// The declared type's wire name (`"number"`/`"integer"`/`"string"`/
    /// `"boolean"`), as the `run_custom` roster listing prints it.
    pub fn as_str(self) -> &'static str {
        match self {
            ParameterType::Number => "number",
            ParameterType::Integer => "integer",
            ParameterType::String => "string",
            ParameterType::Boolean => "boolean",
        }
    }

    /// Fold a declared parameter type down to the type its values actually have.
    fn value_type(self) -> ValueType {
        match self {
            ParameterType::Integer | ParameterType::Number => ValueType::Number,
            ParameterType::String => ValueType::String,
            ParameterType::Boolean => ValueType::Boolean,
        }
    }
}

/// The value types a subject or an operand can carry, with `integer` folded in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueType {
    Number,
    String,
    Boolean,
}

impl ValueType {
    fn as_str(self) -> &'static str {
        match self {
            ValueType::Number => "number",
            ValueType::String => "string",
            ValueType::Boolean => "boolean",
        }
    }
}

/// Default visibility for a tool's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Whisper,
}

/// The semantic states an outcome may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutcomeState {
    Success,
    Partial,
    Failure,
    Info,
}

impl OutcomeState {
    /// The state's wire name (`"success"`/`"partial"`/`"failure"`/`"info"`), as
    /// the `run_custom` roster listing and `pascalMeta` print it.
    pub fn as_str(self) -> &'static str {
        match self {
            OutcomeState::Success => "success",
            OutcomeState::Partial => "partial",
            OutcomeState::Failure => "failure",
            OutcomeState::Info => "info",
        }
    }
}

/// A roll field: a literal number, a `$param`, or a `$state` reference.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum NumberOrParamRef {
    #[serde(serialize_with = "ser_js_number")]
    Number(f64),
    ParamRef(ParamRef),
    StateRef(StateRef),
}

/// A reference to a declared numeric parameter, usable anywhere a roll takes a
/// number. One of the two forms of indirection in the format (the other is
/// [`StateRef`], `f48f34dc`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParamRef {
    #[serde(rename = "$param")]
    pub param: String,
}

/// A reference into the merged persistent state (chat → project → group →
/// general), usable anywhere a `$param` reference is (v4 `StateRefSchema`,
/// `f48f34dc`). Its `fallback` is required and load-bearing: it types the
/// reference at load time and guarantees that run-time resolution can never
/// fail — an absent path or a value of the wrong type simply resolves to the
/// fallback. A run is therefore always dealable.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateRef {
    #[serde(rename = "$state")]
    pub state: String,
    pub fallback: StateRefFallback,
}

/// The `fallback` literal: `z.union([z.number().finite(), z.string(), z.boolean()])`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum StateRefFallback {
    #[serde(serialize_with = "ser_js_number")]
    Number(f64),
    String(String),
    Bool(bool),
}

impl StateRefFallback {
    /// JS `typeof fallback` — the load-time type the reference carries.
    pub fn type_name(&self) -> &'static str {
        match self {
            StateRefFallback::Number(_) => "number",
            StateRefFallback::String(_) => "string",
            StateRefFallback::Bool(_) => "boolean",
        }
    }
}

/// v4 `isStateRef` — true when a raw value is a `$state` reference rather than
/// a literal or `$param` (`'$state' in value`). Used on the raw stored
/// parameter `default`.
pub fn is_state_ref_value(value: &Value) -> bool {
    value.as_object().is_some_and(|o| o.contains_key("$state"))
}

/// A comparator operand for eq/neq, which may address a parameter of any
/// declared type — `{ "eq": "brass" }` is a legitimate test of a string — or a
/// `$state` reference typed by its fallback.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AnyOperand {
    #[serde(serialize_with = "ser_js_number")]
    Number(f64),
    String(String),
    Bool(bool),
    ParamRef(ParamRef),
    StateRef(StateRef),
}

/// A comparator operand for contains/ncontains: the substring to look for — a
/// literal string, a `$param` reference to a declared string parameter, or a
/// `$state` reference whose fallback is a string. The literal must be non-empty
/// because every string contains "", which makes an empty needle a typo wearing
/// a comparator's clothes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum StringOperand {
    String(String),
    ParamRef(ParamRef),
    StateRef(StateRef),
}

/// A declared parameter. Every parameter requires a `default` so that a
/// zero-argument run is always possible — the model may reach for a tool without
/// supplying anything, and the table must still deal.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CustomToolParameter {
    #[serde(rename = "type")]
    pub param_type: ParameterType,
    pub default: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_js_number"
    )]
    pub min: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_js_number"
    )]
    pub max: Option<f64>,
}

/// Form A — a uniform roll over a numeric range, with an optional transform.
/// The transform runs in a fixed order: multiply, then offset, then round.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RollRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round: Option<bool>,
}

/// Either form of roll.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Roll {
    /// Form B — dice notation, rolled by the shared dice module.
    Dice(String),
    Range(RollRange),
}

/// A comparator against a number — the rolled value, or the raw draw. Keys AND
/// together: `>= 0.3 && <= 0.6` is `{ gte: 0.3, lte: 0.6 }`.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct NumericComparator {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gte: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lt: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lte: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neq: Option<NumberOrParamRef>,
}

/// A comparator against a declared parameter. Identical to the numeric form
/// except that eq/neq widen to strings and booleans, since a parameter need not
/// be a number — and contains/ncontains appear, testing whether a string
/// parameter holds (or lacks) a substring. The substring is a literal or a
/// `$param` reference, so a table can ask whether one input appears inside
/// another.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct ParamComparator {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gte: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lt: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lte: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq: Option<AnyOperand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neq: Option<AnyOperand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<StringOperand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ncontains: Option<StringOperand>,
}

impl ParamComparator {
    /// v4's `comparator[key] !== undefined` — whether this comparator declares
    /// the named test. The shared fail-soft table walks the eight keys by name,
    /// so it needs presence without knowing the operand's shape.
    pub fn has(&self, key: &str) -> bool {
        match key {
            "gt" => self.gt.is_some(),
            "gte" => self.gte.is_some(),
            "lt" => self.lt.is_some(),
            "lte" => self.lte.is_some(),
            "eq" => self.eq.is_some(),
            "neq" => self.neq.is_some(),
            "contains" => self.contains.is_some(),
            "ncontains" => self.ncontains.is_some(),
            _ => false,
        }
    }
}

/// A comparator against the LLM consult's result. The comparator keys test the
/// answer string (reconciliation happens at run time, in `matches_llm_comparator`);
/// `ok` is an extra, non-comparator key testing whether the consult succeeded at
/// all.
///
/// Like `metadata`, the subject's run-time type is unknowable at load time — the
/// answer is whatever the model says — so type rules are enforced fail-soft at
/// run time: an ordering comparator against an answer that does not parse as a
/// number simply declines the row.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct LlmComparator {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gte: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lt: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lte: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq: Option<AnyOperand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neq: Option<AnyOperand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<StringOperand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ncontains: Option<StringOperand>,
    /// true: this row only applies when the consult succeeded. false: only when
    /// it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

/// The LLM consult block. When present, every run renders `prompt` (the same
/// placeholder families an outcome message takes, minus `{{llm}}` itself) and
/// poses it to the instance's cheap utility model AFTER the roll and BEFORE the
/// outcome table is evaluated. The result is a pair `{ ok, output }`: on success
/// the model's trimmed answer, on failure this block's `error_message` — the
/// AUTHOR's words, never the provider's stack trace, because whatever lands in
/// the fiction is the author's to write.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CustomToolLlm {
    pub prompt: String,
    #[serde(rename = "errorMessage")]
    pub error_message: String,
    #[serde(rename = "maxOutput", skip_serializing_if = "Option::is_none")]
    pub max_output: Option<i64>,
}

/// A comparator against one key of the invoking character's metadata sheet.
/// Shape-identical to [`ParamComparator`] — the same six keys, the same widened
/// eq/neq, the same `$param` operands. It is a distinct name because the two
/// differ entirely in what can be known at load time: a `params` test names
/// something the file declares (a misspelling is a rejection), while a
/// `metadata` test names a key on a character the file has never met, so nothing
/// here is checkable beyond the comparator's own shape and the run-time rule
/// closes the gap.
pub type MetadataComparator = ParamComparator;

/// An outcome test's object form: one or more subjects, ALL of which must hold.
///
/// Bare comparator keys test the final value, so the common case stays as short
/// as it ever was and every definition written before this key existed still
/// means what it meant. `roll` tests the raw pre-transform draw; `params` tests
/// what the caller supplied, keyed by parameter name; `metadata` tests the
/// invoking character's own fact sheet (`metadata.json`), keyed by whatever the
/// user called it. A `metadata` key the character lacks is not an error — the
/// comparator is false and the table falls through to its catch-all.
///
/// There is still no OR and no nesting: ordered, first-match-wins outcomes make
/// OR unnecessary, and a flat AND of comparators keeps the evaluator eval-free.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct WhenObject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gte: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lt: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lte: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq: Option<NumberOrParamRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neq: Option<NumberOrParamRef>,
    /// Test the raw pre-transform draw rather than the final value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<NumericComparator>,
    /// Test the resolved parameters, keyed by parameter name.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_param_comparators"
    )]
    pub params: Option<Vec<(String, ParamComparator)>>,
    /// Test the invoking character's `metadata.json`, keyed by metadata key. A
    /// key the character lacks does not match. Shape-identical to `params`
    /// (a [`MetadataComparator`] is a [`ParamComparator`]), but its keys are the
    /// user's own vocabulary, not the identifier grammar.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_param_comparators"
    )]
    pub metadata: Option<Vec<(String, ParamComparator)>>,
    /// Test the LLM consult's answer (or, via `ok`, whether it succeeded). Only
    /// valid on a tool that declares an `llm` block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm: Option<LlmComparator>,
}

/// An outcome test: either the literal `true` (catch-all) or a [`WhenObject`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum When {
    /// The literal `true`.
    CatchAll(bool),
    Object(Box<WhenObject>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CustomToolOutcome {
    pub when: When,
    pub message: String,
    pub state: OutcomeState,
}

/// The custom-tool definition.
///
/// Unknown TOP-LEVEL keys are tolerated, which reserves room for v2 keys —
/// notably `persist` — without breaking older builds. [`collect_unknown_keys`]
/// surfaces them for a debug log.
///
/// That tolerance stops at the top level: every nested object below is strict.
/// The forward-compatibility argument does not reach them, and inside a `when`
/// an unrecognised key is overwhelmingly a misspelled comparator — which, if
/// tolerated, silently drops the test and leaves a row of the outcome table
/// looking like a dead branch. It is also what the published JSON Schema has
/// always claimed (`additionalProperties: false`), so an author's editor and the
/// loader now agree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QtapCustomTool {
    /// Editor hint; ignored at runtime.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(rename = "revealOdds", skip_serializing_if = "Option::is_none")]
    pub reveal_odds: Option<bool>,
    #[serde(rename = "defaultVisibility", skip_serializing_if = "Option::is_none")]
    pub default_visibility: Option<Visibility>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_parameters"
    )]
    pub parameters: Option<Vec<(String, CustomToolParameter)>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<Roll>,
    /// Ask an LLM for a generated result after the roll; outcomes may then test
    /// it and messages may render it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm: Option<CustomToolLlm>,
    pub outcomes: Vec<CustomToolOutcome>,
}

impl QtapCustomTool {
    /// Look up a declared parameter. Linear over at most [`MAX_PARAMETERS`]
    /// entries, and an ordered list is what keeps the authored order in the
    /// published description and the parsed JSON.
    pub fn parameter(&self, name: &str) -> Option<&CustomToolParameter> {
        self.parameters
            .as_ref()?
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
}

// ------------------------------------------------------------- serialization

/// JS renders an integer-valued double bare (`1`, not `1.0`); the shared helper
/// is the port's standing answer. Exotic renderings (`1e21`, `Infinity`) remain
/// the documented seam noted at `model/tool_wire/parsers.rs`.
fn ser_js_number<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    crate::db::js_number_to_json(*v).serialize(s)
}

fn ser_opt_js_number<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(f) => ser_js_number(f, s),
        None => s.serialize_none(),
    }
}

fn ser_opt_parameters<S: Serializer>(
    v: &Option<Vec<(String, CustomToolParameter)>>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match v {
        Some(entries) => {
            let mut m = s.serialize_map(Some(entries.len()))?;
            for (k, val) in entries {
                m.serialize_entry(k, val)?;
            }
            m.end()
        }
        None => s.serialize_none(),
    }
}

fn ser_opt_param_comparators<S: Serializer>(
    v: &Option<Vec<(String, ParamComparator)>>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match v {
        Some(entries) => {
            let mut m = s.serialize_map(Some(entries.len()))?;
            for (k, val) in entries {
                m.serialize_entry(k, val)?;
            }
            m.end()
        }
        None => s.serialize_none(),
    }
}

// ------------------------------------------------------------ leaf validators

/// v4 `z.string()` plus the optional length caps and the identifier regex.
/// `min`/`max` are Zod built-ins (so their sentences are Zod's); the regex
/// carries the author's message. The regex is a CHECK, so its issue is
/// continuable — which is what lets a malformed `$param` still reach the
/// cross-field rules and pick up a second sentence.
fn parse_string(
    input: Option<&Value>,
    min: Option<usize>,
    max: Option<usize>,
    identifier: bool,
) -> Res<String> {
    let Some(Value::String(s)) = input else {
        return Res::hard(invalid_type("string", input));
    };

    let mut issues = Vec::new();
    // JS `.length` is UTF-16 code units.
    let len = s.encode_utf16().count();
    if let Some(m) = min {
        if len < m {
            issues.push(Issue::check(format!(
                "Too small: expected string to have >={m} characters"
            )));
        }
    }
    if let Some(m) = max {
        if len > m {
            issues.push(Issue::check(format!(
                "Too big: expected string to have <={m} characters"
            )));
        }
    }
    if identifier && !IDENTIFIER_PATTERN.is_match(s) {
        issues.push(Issue::check(IDENTIFIER_MESSAGE));
    }

    Res {
        value: Some(s.clone()),
        issues,
    }
}

fn parse_bool(input: Option<&Value>) -> Res<bool> {
    match input {
        Some(Value::Bool(b)) => Res::ok(*b),
        other => Res::hard(invalid_type("boolean", other)),
    }
}

/// v4 `z.number().finite()`. See the module note: the `finite` arm is
/// unreachable from a serde_json parse, which rejects overflow literals first.
fn parse_finite_number(input: Option<&Value>) -> Res<f64> {
    match input {
        Some(Value::Number(n)) => match n.as_f64() {
            Some(f) if f.is_finite() => Res::ok(f),
            Some(_) | None => Res::hard(invalid_type(FINITE_EXPECTED, input)),
        },
        other => Res::hard(invalid_type("number", other)),
    }
}

/// v4 `z.number().int().min(lo).max(hi)` — the three checks run in declaration
/// order, and each is continuable, so a value may collect more than one.
fn parse_bounded_int(input: Option<&Value>, lo: i64, hi: i64) -> Res<i64> {
    let Some(Value::Number(n)) = input else {
        return Res::hard(invalid_type("number", input));
    };
    let Some(f) = n.as_f64() else {
        return Res::hard(invalid_type("number", input));
    };

    let mut issues = Vec::new();
    let is_int = f.is_finite() && f.fract() == 0.0;
    if !is_int {
        issues.push(Issue::check("Invalid input: expected int, received number"));
    }
    if f < lo as f64 {
        issues.push(Issue::check(format!(
            "Too small: expected number to be >={lo}"
        )));
    }
    if f > hi as f64 {
        issues.push(Issue::check(format!(
            "Too big: expected number to be <={hi}"
        )));
    }

    Res {
        value: Some(f as i64),
        issues,
    }
}

fn parse_enum<T: Copy>(input: Option<&Value>, options: &[(&str, T)]) -> Res<T> {
    if let Some(Value::String(s)) = input {
        if let Some((_, v)) = options.iter().find(|(name, _)| name == s) {
            return Res::ok(*v);
        }
    }
    let list = options
        .iter()
        .map(|(n, _)| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join("|");
    Res::hard(format!("Invalid option: expected one of {list}"))
}

/// The strict-object tail: report keys outside `known` as ONE issue, in the
/// input's own order. Zod raises this AFTER the shape, and it is aborting.
fn unrecognized_keys(obj: &serde_json::Map<String, Value>, known: &[&str]) -> Vec<Issue> {
    let extra: Vec<&String> = obj
        .keys()
        .filter(|k| !known.contains(&k.as_str()))
        .collect();
    if extra.is_empty() {
        return Vec::new();
    }
    let quoted = extra
        .iter()
        .map(|k| format!("\"{k}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let message = if extra.len() == 1 {
        format!("Unrecognized key: {quoted}")
    } else {
        format!("Unrecognized keys: {quoted}")
    };
    vec![Issue::hard(message)]
}

fn as_object(input: Option<&Value>) -> Result<&serde_json::Map<String, Value>, Res<()>> {
    match input {
        Some(Value::Object(o)) => Ok(o),
        other => Err(Res::hard(invalid_type("object", other))),
    }
}

// --------------------------------------------------------- schema validators

fn parse_param_ref(input: Option<&Value>) -> Res<ParamRef> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let r = parse_string(obj.get("$param"), None, None, true);
    issues.extend(prefix("$param", r.issues));
    issues.extend(unrecognized_keys(obj, &["$param"]));

    let value = r
        .value
        .filter(|_| !aborted(&issues))
        .map(|param| ParamRef { param });
    Res { value, issues }
}

/// v4 `StateRefSchema` (`f48f34dc`) — a strict object `{ $state, fallback }`:
/// `$state` is `z.string().min(1)`; `fallback` is the required
/// number|string|boolean union.
fn parse_state_ref(input: Option<&Value>) -> Res<StateRef> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let path = parse_string(obj.get("$state"), Some(1), None, false);
    issues.extend(prefix("$state", path.issues));

    let fallback = {
        let input = obj.get("fallback");
        let n = parse_finite_number(input);
        let n = Res {
            value: n.value.map(StateRefFallback::Number),
            issues: n.issues,
        };
        let s = parse_string(input, None, None, false);
        let s = Res {
            value: s.value.map(StateRefFallback::String),
            issues: s.issues,
        };
        let b = parse_bool(input);
        let b = Res {
            value: b.value.map(StateRefFallback::Bool),
            issues: b.issues,
        };
        let r = union(vec![n, s, b]);
        issues.extend(prefix("fallback", r.issues));
        r.value
    };
    issues.extend(unrecognized_keys(obj, &["$state", "fallback"]));

    let value = match (path.value, fallback) {
        (Some(state), Some(fallback)) if !aborted(&issues) => Some(StateRef { state, fallback }),
        _ => None,
    };
    Res { value, issues }
}

/// v4 `NumberOrParamRefSchema` / `NumberOperandSchema` — the two are the same
/// union (number | `$param` | `$state`, in that order), and both roll fields
/// and ordering operands take it.
fn parse_number_or_param_ref(input: Option<&Value>) -> Res<NumberOrParamRef> {
    let n = parse_finite_number(input);
    let n = Res {
        value: n.value.map(NumberOrParamRef::Number),
        issues: n.issues,
    };
    let p = parse_param_ref(input);
    let p = Res {
        value: p.value.map(NumberOrParamRef::ParamRef),
        issues: p.issues,
    };
    let st = parse_state_ref(input);
    let st = Res {
        value: st.value.map(NumberOrParamRef::StateRef),
        issues: st.issues,
    };
    union(vec![n, p, st])
}

/// v4 `AnyOperandSchema` — number | string | boolean | ParamRef, in that order.
fn parse_any_operand(input: Option<&Value>) -> Res<AnyOperand> {
    let n = parse_finite_number(input);
    let n = Res {
        value: n.value.map(AnyOperand::Number),
        issues: n.issues,
    };
    let s = parse_string(input, None, None, false);
    let s = Res {
        value: s.value.map(AnyOperand::String),
        issues: s.issues,
    };
    let b = parse_bool(input);
    let b = Res {
        value: b.value.map(AnyOperand::Bool),
        issues: b.issues,
    };
    let p = parse_param_ref(input);
    let p = Res {
        value: p.value.map(AnyOperand::ParamRef),
        issues: p.issues,
    };
    let st = parse_state_ref(input);
    let st = Res {
        value: st.value.map(AnyOperand::StateRef),
        issues: st.issues,
    };
    union(vec![n, s, b, p, st])
}

/// v4 `StringOperandSchema` — a non-empty string literal (with the author's own
/// message) or a `$param` reference, in that order.
fn parse_string_operand(input: Option<&Value>) -> Res<StringOperand> {
    let s = match input {
        Some(Value::String(text)) => {
            let mut issues = Vec::new();
            if text.is_empty() {
                issues.push(Issue::check("the substring to look for must not be empty"));
            }
            Res {
                value: Some(StringOperand::String(text.clone())),
                issues,
            }
        }
        other => Res {
            value: None,
            issues: vec![Issue::hard(invalid_type("string", other))],
        },
    };
    let p = parse_param_ref(input);
    let p = Res {
        value: p.value.map(StringOperand::ParamRef),
        issues: p.issues,
    };
    let st = parse_state_ref(input);
    let st = Res {
        value: st.value.map(StringOperand::StateRef),
        issues: st.issues,
    };
    union(vec![s, p, st])
}

fn parse_numeric_comparator(input: Option<&Value>) -> Res<NumericComparator> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let mut out = NumericComparator::default();
    let mut slots: [&mut Option<NumberOrParamRef>; 6] = [
        &mut out.gt,
        &mut out.gte,
        &mut out.lt,
        &mut out.lte,
        &mut out.eq,
        &mut out.neq,
    ];
    for (key, slot) in NUMERIC_COMPARATOR_KEYS.iter().zip(slots.iter_mut()) {
        if let Some(v) = obj.get(*key) {
            let r = parse_number_or_param_ref(Some(v));
            issues.extend(prefix(key, r.issues));
            **slot = r.value;
        }
    }
    issues.extend(unrecognized_keys(obj, &NUMERIC_COMPARATOR_KEYS));

    if aborted(&issues) {
        return Res {
            value: None,
            issues,
        };
    }
    // `.refine(hasComparator, AT_LEAST_ONE)` — runs only when nothing aborted.
    // `hasComparator` walks the full eight keys, but a numeric comparator can
    // never carry a containment one: it would already have aborted as
    // unrecognized.
    if !COMPARATOR_KEYS.iter().any(|k| obj.contains_key(*k)) {
        issues.push(Issue::check(AT_LEAST_ONE));
    }
    Res {
        value: Some(out),
        issues,
    }
}

fn parse_param_comparator(input: Option<&Value>) -> Res<ParamComparator> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let mut out = ParamComparator::default();
    // Ordering keys stay numeric on both sides; eq/neq widen to any operand.
    let mut ordering: [&mut Option<NumberOrParamRef>; 4] =
        [&mut out.gt, &mut out.gte, &mut out.lt, &mut out.lte];
    for (key, slot) in ORDERING_KEYS.iter().zip(ordering.iter_mut()) {
        if let Some(v) = obj.get(*key) {
            let r = parse_number_or_param_ref(Some(v));
            issues.extend(prefix(key, r.issues));
            **slot = r.value;
        }
    }
    for (key, slot) in [("eq", &mut out.eq), ("neq", &mut out.neq)] {
        if let Some(v) = obj.get(key) {
            let r = parse_any_operand(Some(v));
            issues.extend(prefix(key, r.issues));
            *slot = r.value;
        }
    }
    for (key, slot) in [
        ("contains", &mut out.contains),
        ("ncontains", &mut out.ncontains),
    ] {
        if let Some(v) = obj.get(key) {
            let r = parse_string_operand(Some(v));
            issues.extend(prefix(key, r.issues));
            *slot = r.value;
        }
    }
    issues.extend(unrecognized_keys(obj, &COMPARATOR_KEYS));

    if aborted(&issues) {
        return Res {
            value: None,
            issues,
        };
    }
    if !COMPARATOR_KEYS.iter().any(|k| obj.contains_key(*k)) {
        issues.push(Issue::check(AT_LEAST_ONE_WIDE));
    }
    Res {
        value: Some(out),
        issues,
    }
}

/// v4 `LlmComparatorSchema` — the param-comparator shape plus the non-comparator
/// `ok` key, and a refine that accepts either.
fn parse_llm_comparator(input: Option<&Value>) -> Res<LlmComparator> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let mut out = LlmComparator::default();
    let mut ordering: [&mut Option<NumberOrParamRef>; 4] =
        [&mut out.gt, &mut out.gte, &mut out.lt, &mut out.lte];
    for (key, slot) in ORDERING_KEYS.iter().zip(ordering.iter_mut()) {
        if let Some(v) = obj.get(*key) {
            let r = parse_number_or_param_ref(Some(v));
            issues.extend(prefix(key, r.issues));
            **slot = r.value;
        }
    }
    for (key, slot) in [("eq", &mut out.eq), ("neq", &mut out.neq)] {
        if let Some(v) = obj.get(key) {
            let r = parse_any_operand(Some(v));
            issues.extend(prefix(key, r.issues));
            *slot = r.value;
        }
    }
    for (key, slot) in [
        ("contains", &mut out.contains),
        ("ncontains", &mut out.ncontains),
    ] {
        if let Some(v) = obj.get(key) {
            let r = parse_string_operand(Some(v));
            issues.extend(prefix(key, r.issues));
            *slot = r.value;
        }
    }
    if let Some(v) = obj.get("ok") {
        let r = parse_bool(Some(v));
        issues.extend(prefix("ok", r.issues));
        out.ok = r.value;
    }

    let mut known: Vec<&str> = COMPARATOR_KEYS.to_vec();
    known.push("ok");
    issues.extend(unrecognized_keys(obj, &known));

    if aborted(&issues) {
        return Res {
            value: None,
            issues,
        };
    }
    if !COMPARATOR_KEYS.iter().any(|k| obj.contains_key(*k)) && out.ok.is_none() {
        issues.push(Issue::check(
            "must test something: a comparator on the answer, or `ok`",
        ));
    }
    Res {
        value: Some(out),
        issues,
    }
}

/// v4 `CustomToolLlmSchema` — the consult block itself.
fn parse_custom_tool_llm(input: Option<&Value>) -> Res<CustomToolLlm> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let prompt = parse_string(
        obj.get("prompt"),
        Some(1),
        Some(MAX_LLM_PROMPT_LENGTH),
        false,
    );
    issues.extend(prefix("prompt", prompt.issues));
    let error_message = parse_string(
        obj.get("errorMessage"),
        Some(1),
        Some(MAX_MESSAGE_LENGTH),
        false,
    );
    issues.extend(prefix("errorMessage", error_message.issues));

    let max_output = match obj.get("maxOutput") {
        Some(v) => {
            let r = parse_bounded_int(Some(v), 1, MAX_LLM_OUTPUT_CEILING);
            issues.extend(prefix("maxOutput", r.issues));
            r.value
        }
        None => None,
    };

    issues.extend(unrecognized_keys(
        obj,
        &["prompt", "errorMessage", "maxOutput"],
    ));

    let (Some(prompt), Some(error_message)) = (prompt.value, error_message.value) else {
        return Res {
            value: None,
            issues,
        };
    };
    if aborted(&issues) {
        return Res {
            value: None,
            issues,
        };
    }
    Res {
        value: Some(CustomToolLlm {
            prompt,
            error_message,
            max_output,
        }),
        issues,
    }
}

/// v4 `z.record(keySchema, …)`: a key failing `key_valid` is an `invalid_key`
/// issue — aborting, unlike the same regex used as a field check. `params` keys
/// take the identifier grammar; `metadata` keys take `z.string().min(1)`
/// (non-empty), since the user hand-authors `metadata.json` and `HOUSE` or
/// `Clearance Level` are perfectly ordinary keys. Zod's message is the same
/// "Invalid key in record" either way.
fn parse_record<T>(
    obj: &serde_json::Map<String, Value>,
    key_valid: impl Fn(&str) -> bool,
    mut parse_value: impl FnMut(&Value) -> Res<T>,
) -> Res<Vec<(String, T)>> {
    let mut issues = Vec::new();
    let mut out = Vec::new();
    for (key, v) in obj {
        if !key_valid(key) {
            issues.push(Issue::hard("Invalid key in record").at(key));
            continue;
        }
        let r = parse_value(v);
        issues.extend(prefix(key, r.issues));
        if let Some(val) = r.value {
            out.push((key.clone(), val));
        }
    }
    let value = if aborted(&issues) { None } else { Some(out) };
    Res { value, issues }
}

fn parse_parameter(input: Option<&Value>) -> Res<CustomToolParameter> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();

    let ty = parse_enum(
        obj.get("type"),
        &[
            ("number", ParameterType::Number),
            ("integer", ParameterType::Integer),
            ("string", ParameterType::String),
            ("boolean", ParameterType::Boolean),
        ],
    );
    issues.extend(prefix("type", ty.issues));

    // `default: z.union([number, string, boolean, StateRefSchema])` — no
    // `.finite()` on the number branch. A `$state` default is kept as its raw
    // object (the runtime resolves it against the merged state).
    let default = match obj.get("default") {
        Some(v @ (Value::Number(_) | Value::String(_) | Value::Bool(_))) => Some(v.clone()),
        other => {
            let st = parse_state_ref(other);
            if st.value.is_some() && st.issues.is_empty() {
                other.cloned()
            } else {
                let branches = vec![
                    Res::<()>::hard(invalid_type("number", other)),
                    Res::<()>::hard(invalid_type("string", other)),
                    Res::<()>::hard(invalid_type("boolean", other)),
                    Res::<()> {
                        value: if st.issues.is_empty() { Some(()) } else { None },
                        issues: st.issues,
                    },
                ];
                issues.extend(prefix("default", union(branches).issues));
                None
            }
        }
    };

    let description = match obj.get("description") {
        Some(v) => {
            let r = parse_string(Some(v), None, Some(MAX_DESCRIPTION_LENGTH), false);
            issues.extend(prefix("description", r.issues));
            r.value
        }
        None => None,
    };

    let num_field = |key: &str, issues: &mut Vec<Issue>| match obj.get(key) {
        Some(v) => {
            let r = parse_finite_number(Some(v));
            issues.extend(prefix(key, r.issues));
            r.value
        }
        None => None,
    };
    let min = num_field("min", &mut issues);
    let max = num_field("max", &mut issues);

    issues.extend(unrecognized_keys(
        obj,
        &["type", "default", "description", "min", "max"],
    ));

    let (Some(param_type), Some(default)) = (ty.value, default) else {
        return Res {
            value: None,
            issues,
        };
    };
    if aborted(&issues) {
        return Res {
            value: None,
            issues,
        };
    }

    // ---- superRefine (v4 `:99-129`), in source order ------------------------
    let numeric = matches!(param_type, ParameterType::Number | ParameterType::Integer);

    // `min`/`max` are meaningless on a string or boolean; silently ignoring them
    // would let an author believe a bound is in force when it is not.
    if !numeric && (min.is_some() || max.is_some()) {
        issues.push(Issue::check(format!(
            "min/max are only valid on number/integer parameters, not {}",
            param_type.as_str()
        )));
    }
    if let (Some(lo), Some(hi)) = (min, max) {
        if lo > hi {
            issues.push(Issue::check("min must not exceed max").at("min"));
        }
    }

    // The declared default must satisfy the parameter's own declared type. A
    // `$state` default is typed by its fallback (which run-time resolution is
    // guaranteed to fall back to), so the fallback is what must match the type.
    let effective_default: &Value = if is_state_ref_value(&default) {
        default.get("fallback").unwrap_or(&Value::Null)
    } else {
        &default
    };
    if numeric && !effective_default.is_number() {
        issues.push(
            Issue::check(format!(
                "default must be a number for type {}",
                param_type.as_str()
            ))
            .at("default"),
        );
    }
    if param_type == ParameterType::Integer && effective_default.is_number() {
        let is_int = effective_default
            .as_f64()
            .is_some_and(|f| f.fract() == 0.0 && f.is_finite());
        if !is_int {
            issues.push(
                Issue::check("default must be a whole number for type integer").at("default"),
            );
        }
    }
    if param_type == ParameterType::String && !effective_default.is_string() {
        issues.push(Issue::check("default must be a string for type string").at("default"));
    }
    if param_type == ParameterType::Boolean && !effective_default.is_boolean() {
        issues.push(Issue::check("default must be a boolean for type boolean").at("default"));
    }

    Res {
        value: Some(CustomToolParameter {
            param_type,
            default,
            description,
            min,
            max,
        }),
        issues,
    }
}

/// Form B — dice notation, validated here so a typo is a load-time rejection
/// rather than a run-time surprise. The `refine` is a check, so a bad notation
/// hoists out of the `roll` union with its own sentence.
fn parse_roll_dice(input: Option<&Value>) -> Res<String> {
    let r = parse_string(input, None, None, false);
    let Some(s) = r.value else {
        return Res {
            value: None,
            issues: r.issues,
        };
    };
    let mut issues = r.issues;
    if parse_dice_notation(&s).is_none() {
        issues.push(Issue::check(format!(
            "must be dice notation like \"3d6+2\" or \"1d20\" ({MIN_DIE_SIDES}–{MAX_DIE_SIDES} sides, 1–100 dice)"
        )));
    }
    Res {
        value: Some(s),
        issues,
    }
}

fn parse_roll_range(input: Option<&Value>) -> Res<RollRange> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let mut out = RollRange::default();
    let mut slots: [&mut Option<NumberOrParamRef>; 4] = [
        &mut out.min,
        &mut out.max,
        &mut out.multiplier,
        &mut out.offset,
    ];
    for (key, slot) in ["min", "max", "multiplier", "offset"]
        .iter()
        .zip(slots.iter_mut())
    {
        if let Some(v) = obj.get(*key) {
            let r = parse_number_or_param_ref(Some(v));
            issues.extend(prefix(key, r.issues));
            **slot = r.value;
        }
    }
    if let Some(v) = obj.get("round") {
        let r = parse_bool(Some(v));
        issues.extend(prefix("round", r.issues));
        out.round = r.value;
    }
    issues.extend(unrecognized_keys(
        obj,
        &["min", "max", "multiplier", "offset", "round"],
    ));

    let value = if aborted(&issues) { None } else { Some(out) };
    Res { value, issues }
}

fn parse_roll(input: Option<&Value>) -> Res<Roll> {
    let d = parse_roll_dice(input);
    let d = Res {
        value: d.value.map(Roll::Dice),
        issues: d.issues,
    };
    let r = parse_roll_range(input);
    let r = Res {
        value: r.value.map(Roll::Range),
        issues: r.issues,
    };
    union(vec![d, r])
}

fn parse_when_object(input: Option<&Value>) -> Res<WhenObject> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let mut out = WhenObject::default();
    let mut slots: [&mut Option<NumberOrParamRef>; 6] = [
        &mut out.gt,
        &mut out.gte,
        &mut out.lt,
        &mut out.lte,
        &mut out.eq,
        &mut out.neq,
    ];
    for (key, slot) in NUMERIC_COMPARATOR_KEYS.iter().zip(slots.iter_mut()) {
        if let Some(v) = obj.get(*key) {
            let r = parse_number_or_param_ref(Some(v));
            issues.extend(prefix(key, r.issues));
            **slot = r.value;
        }
    }
    if let Some(v) = obj.get("roll") {
        let r = parse_numeric_comparator(Some(v));
        issues.extend(prefix("roll", r.issues));
        out.roll = r.value;
    }
    let mut params_present = false;
    if let Some(Value::Object(p)) = obj.get("params") {
        params_present = true;
        let r = parse_record(
            p,
            |k| IDENTIFIER_PATTERN.is_match(k),
            |v| parse_param_comparator(Some(v)),
        );
        issues.extend(prefix("params", r.issues));
        out.params = r.value;
    } else if let Some(v) = obj.get("params") {
        params_present = true;
        issues.extend(prefix(
            "params",
            vec![Issue::hard(invalid_type("object", Some(v)))],
        ));
    }
    let mut metadata_present = false;
    if let Some(Value::Object(m)) = obj.get("metadata") {
        metadata_present = true;
        // Metadata keys are `z.string().min(1)` — any non-empty string, unlike
        // `params`' identifier grammar.
        let r = parse_record(m, |k| !k.is_empty(), |v| parse_param_comparator(Some(v)));
        issues.extend(prefix("metadata", r.issues));
        out.metadata = r.value;
    } else if let Some(v) = obj.get("metadata") {
        metadata_present = true;
        issues.extend(prefix(
            "metadata",
            vec![Issue::hard(invalid_type("object", Some(v)))],
        ));
    }

    let mut llm_present = false;
    if let Some(v) = obj.get("llm") {
        llm_present = true;
        let r = parse_llm_comparator(Some(v));
        issues.extend(prefix("llm", r.issues));
        out.llm = r.value;
    }

    let mut known: Vec<&str> = NUMERIC_COMPARATOR_KEYS.to_vec();
    known.extend_from_slice(&["roll", "params", "metadata", "llm"]);
    issues.extend(unrecognized_keys(obj, &known));

    if aborted(&issues) {
        return Res {
            value: None,
            issues,
        };
    }

    // `.refine(…)` — must test something.
    let has_comparator = COMPARATOR_KEYS.iter().any(|k| obj.contains_key(*k));
    let has_params = params_present && out.params.as_ref().is_some_and(|p| !p.is_empty());
    let has_metadata = metadata_present && out.metadata.as_ref().is_some_and(|m| !m.is_empty());
    if !has_comparator && out.roll.is_none() && !llm_present && !has_params && !has_metadata {
        issues.push(Issue::check(
            "must test something: a comparator on the value, `roll`, `llm`, a non-empty `params`, or a non-empty `metadata`",
        ));
    }
    Res {
        value: Some(out),
        issues,
    }
}

fn parse_when(input: Option<&Value>) -> Res<When> {
    // `z.literal(true)`.
    let lit = match input {
        Some(Value::Bool(true)) => Res::ok(When::CatchAll(true)),
        _ => Res {
            value: None,
            issues: vec![Issue::hard("Invalid input: expected true")],
        },
    };
    let o = parse_when_object(input);
    let o = Res {
        value: o.value.map(|w| When::Object(Box::new(w))),
        issues: o.issues,
    };
    union(vec![lit, o])
}

fn parse_outcome(input: Option<&Value>) -> Res<CustomToolOutcome> {
    let obj = match as_object(input) {
        Ok(o) => o,
        Err(e) => {
            return Res {
                value: None,
                issues: e.issues,
            }
        }
    };

    let mut issues = Vec::new();
    let when = parse_when(obj.get("when"));
    issues.extend(prefix("when", when.issues));

    let message = parse_string(obj.get("message"), Some(1), Some(MAX_MESSAGE_LENGTH), false);
    issues.extend(prefix("message", message.issues));

    let state = parse_enum(
        obj.get("state"),
        &[
            ("success", OutcomeState::Success),
            ("partial", OutcomeState::Partial),
            ("failure", OutcomeState::Failure),
            ("info", OutcomeState::Info),
        ],
    );
    issues.extend(prefix("state", state.issues));

    issues.extend(unrecognized_keys(obj, &["when", "message", "state"]));

    match (when.value, message.value, state.value) {
        (Some(when), Some(message), Some(state)) if !aborted(&issues) => Res {
            value: Some(CustomToolOutcome {
                when,
                message,
                state,
            }),
            issues,
        },
        _ => Res {
            value: None,
            issues,
        },
    }
}

// -------------------------------------------------------------- the top level

/// The rejection list from a failed [`safe_parse`], in Zod's order.
pub type DefinitionIssues = Vec<Issue>;

/// v4 `QtapCustomToolSchema.safeParse`.
pub fn safe_parse(raw: &Value) -> Result<QtapCustomTool, DefinitionIssues> {
    let obj = match raw {
        Value::Object(o) => o,
        other => return Err(vec![Issue::hard(invalid_type("object", Some(other)))]),
    };

    let mut issues = Vec::new();

    let schema = match obj.get("$schema") {
        Some(v) => {
            let r = parse_string(Some(v), None, None, false);
            issues.extend(prefix("$schema", r.issues));
            r.value
        }
        None => None,
    };

    let name = parse_string(obj.get("name"), None, None, true);
    issues.extend(prefix("name", name.issues));

    let title = match obj.get("title") {
        Some(v) => {
            let r = parse_string(Some(v), Some(1), Some(MAX_TITLE_LENGTH), false);
            issues.extend(prefix("title", r.issues));
            r.value
        }
        None => None,
    };

    let description = parse_string(
        obj.get("description"),
        Some(1),
        Some(MAX_DESCRIPTION_LENGTH),
        false,
    );
    issues.extend(prefix("description", description.issues));

    let opt_bool = |key: &str, issues: &mut Vec<Issue>| match obj.get(key) {
        Some(v) => {
            let r = parse_bool(Some(v));
            issues.extend(prefix(key, r.issues));
            r.value
        }
        None => None,
    };
    let disabled = opt_bool("disabled", &mut issues);
    let reveal_odds = opt_bool("revealOdds", &mut issues);

    let default_visibility = match obj.get("defaultVisibility") {
        Some(v) => {
            let r = parse_enum(
                Some(v),
                &[
                    ("public", Visibility::Public),
                    ("whisper", Visibility::Whisper),
                ],
            );
            issues.extend(prefix("defaultVisibility", r.issues));
            r.value
        }
        None => None,
    };

    let parameters = match obj.get("parameters") {
        Some(Value::Object(p)) => {
            let r = parse_record(
                p,
                |k| IDENTIFIER_PATTERN.is_match(k),
                |v| parse_parameter(Some(v)),
            );
            let mut param_issues = r.issues;
            // `.refine(… <= MAX_PARAMETERS)` — a check on the record itself, so
            // it is skipped when an entry already aborted.
            if !aborted(&param_issues) && p.len() > MAX_PARAMETERS {
                param_issues.push(Issue::check(format!("at most {MAX_PARAMETERS} parameters")));
            }
            issues.extend(prefix("parameters", param_issues));
            r.value
        }
        Some(v) => {
            issues.extend(prefix(
                "parameters",
                vec![Issue::hard(invalid_type("object", Some(v)))],
            ));
            None
        }
        None => None,
    };

    let roll = match obj.get("roll") {
        Some(v) => {
            let r = parse_roll(Some(v));
            issues.extend(prefix("roll", r.issues));
            r.value
        }
        None => None,
    };

    let llm = match obj.get("llm") {
        Some(v) => {
            let r = parse_custom_tool_llm(Some(v));
            issues.extend(prefix("llm", r.issues));
            r.value
        }
        None => None,
    };

    let outcomes = match obj.get("outcomes") {
        Some(Value::Array(items)) => {
            let mut list = Vec::new();
            let mut arr_issues = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let r = parse_outcome(Some(item));
                arr_issues.extend(prefix(&i.to_string(), r.issues));
                if let Some(o) = r.value {
                    list.push(o);
                }
            }
            // `.min(1, …)` / `.max(MAX_OUTCOMES, …)` are checks with authored
            // messages, so they are skipped once an element aborted.
            if !aborted(&arr_issues) {
                if items.is_empty() {
                    arr_issues.push(Issue::check("at least one outcome is required"));
                } else if items.len() > MAX_OUTCOMES {
                    arr_issues.push(Issue::check(format!("at most {MAX_OUTCOMES} outcomes")));
                }
            }
            let ok = !aborted(&arr_issues);
            issues.extend(prefix("outcomes", arr_issues));
            if ok {
                Some(list)
            } else {
                None
            }
        }
        other => {
            issues.extend(prefix(
                "outcomes",
                vec![Issue::hard(invalid_type("array", other))],
            ));
            None
        }
    };

    // The top level is NOT strict — unknown keys are tolerated here on purpose.

    let (Some(name), Some(description), Some(outcomes)) = (name.value, description.value, outcomes)
    else {
        return Err(issues);
    };
    if aborted(&issues) {
        return Err(issues);
    }

    let tool = QtapCustomTool {
        schema,
        name,
        title,
        description,
        disabled,
        reveal_odds,
        default_visibility,
        parameters,
        roll,
        llm,
        outcomes,
    };

    // ---- superRefine (v4 `:335-338`), in source order -----------------------
    validate_outcome_ordering(&tool.outcomes, &mut issues);
    validate_references(&tool, &mut issues);

    if issues.is_empty() {
        Ok(tool)
    } else {
        Err(issues)
    }
}

/// The name to show a human — the author's `title`, or one derived from `name`.
///
/// The single source of the display string: the announcement, the composer
/// popup, and the roster listing all come through here, so `scan_hawking_radiation`
/// reads as "Scan Hawking Radiation" in every one of them without three
/// implementations agreeing by luck. The model never sees this — it calls tools
/// by `name`, and a second string for one tool would only invite it to pass the
/// wrong one.
pub fn display_title(name: &str, title: Option<&str>) -> String {
    if let Some(authored) = title.map(str::trim).filter(|t| !t.is_empty()) {
        return authored.to_string();
    }

    name.split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rule: the final outcome must be the literal `true`, and no earlier outcome
/// may be.
///
/// The trailing catch-all makes a coverage gap structurally impossible — there
/// is always exactly one outcome to land on. An earlier catch-all would make
/// everything below it dead, which is a typo rather than an intent.
fn validate_outcome_ordering(outcomes: &[CustomToolOutcome], issues: &mut Vec<Issue>) {
    if outcomes.is_empty() {
        return;
    }

    for (i, outcome) in outcomes.iter().enumerate() {
        let is_catch_all = matches!(outcome.when, When::CatchAll(_));
        let is_last = i == outcomes.len() - 1;

        if is_catch_all && !is_last {
            issues.push(
                Issue::check(format!(
                    "outcome {i} is a catch-all (when: true), so every outcome after it is unreachable"
                ))
                .at("when")
                .at(i.to_string())
                .at("outcomes"),
            );
        }

        if is_last && !is_catch_all {
            issues.push(
                Issue::check(
                    "the final outcome must be a catch-all (when: true) so every roll lands somewhere",
                )
                .at("when")
                .at(i.to_string())
                .at("outcomes"),
            );
        }
    }
}

/// Rule: every `$param` reference — in a roll field or in a comparator — must
/// name a declared parameter, and the comparison it takes part in must be one
/// that can hold at run time.
///
/// All of this is authoring error caught at load: a misspelled parameter, an
/// ordering test against a string, `{ "eq": "brass" }` posed to a number. Left
/// to run time these are silent — a test that simply never fires reads as a dead
/// branch in the outcome table rather than the typo it is.
fn validate_references(tool: &QtapCustomTool, issues: &mut Vec<Issue>) {
    validate_roll_refs(tool, issues);

    for (i, outcome) in tool.outcomes.iter().enumerate() {
        let When::Object(when) = &outcome.when else {
            continue;
        };
        let path = |rest: Vec<String>| {
            let mut p = vec!["outcomes".to_string(), i.to_string(), "when".to_string()];
            p.extend(rest);
            p
        };

        // Bare comparator keys, and `roll`, both address a number.
        let bare = NumericComparator {
            gt: when.gt.clone(),
            gte: when.gte.clone(),
            lt: when.lt.clone(),
            lte: when.lte.clone(),
            eq: when.eq.clone(),
            neq: when.neq.clone(),
        };
        validate_numeric_comparator(
            tool,
            &bare,
            ValueType::Number,
            "the rolled value",
            issues,
            &path(vec![]),
        );
        if let Some(roll) = &when.roll {
            validate_numeric_comparator(
                tool,
                roll,
                ValueType::Number,
                "the raw roll",
                issues,
                &path(vec!["roll".into()]),
            );
        }

        for (name, comparator) in when.params.iter().flatten() {
            let p = path(vec!["params".into(), name.clone()]);
            let Some(target) = tool.parameter(name) else {
                issues.push(issue_at(
                    format!("tests undeclared parameter \"{name}\""),
                    &p,
                ));
                continue;
            };
            validate_param_comparator(
                tool,
                comparator,
                target.param_type.value_type(),
                &format!("parameter \"{name}\""),
                issues,
                &p,
            );
        }

        // An `llm` test on a tool with no `llm` block could never fire — there
        // is no consult whose answer it might match. That is a typo, not an
        // intent, and left alone it reads as a dead branch in the outcome table.
        if let Some(llm) = &when.llm {
            let p = path(vec!["llm".into()]);
            if tool.llm.is_none() {
                issues.push(issue_at(
                    "tests the LLM consult, but the tool declares no `llm` block".into(),
                    &p,
                ));
            }
            // The answer's run-time type is the model's business (see the
            // schema comment), so — exactly as with `metadata` — only the
            // `$param` operands are checkable here.
            validate_operand_refs(tool, llm_operands(llm), issues, &p);
        }

        // `metadata` gets a shallower check, and there is no way around it: the
        // keys live on a character the file has never seen, so neither the key's
        // existence nor its stored type is knowable here. What IS checkable is
        // the operand — a `$param` reference must still resolve to a declared
        // parameter. The rest (absent key, wrong type, non-primitive value) is
        // caught fail-soft at run time by `matches_when`.
        for (key, comparator) in when.metadata.iter().flatten() {
            validate_metadata_operands(
                tool,
                comparator,
                issues,
                &path(vec!["metadata".into(), key.clone()]),
            );
        }
    }
}

/// Check only what a metadata comparator can be checked on at load: that every
/// `$param` operand names a declared parameter. Deliberately silent about the
/// subject's type — see the caller.
fn validate_metadata_operands(
    tool: &QtapCustomTool,
    c: &MetadataComparator,
    issues: &mut Vec<Issue>,
    path: &[String],
) {
    validate_operand_refs(tool, wide_operands(c), issues, path);
}

/// The shared body: resolve every present operand purely to catch an undeclared
/// `$param`. Used by both fail-soft subjects (`metadata`, `llm`).
fn validate_operand_refs(
    tool: &QtapCustomTool,
    operands: [(&str, Option<Operand>); 8],
    issues: &mut Vec<Issue>,
    path: &[String],
) {
    for (key, operand) in operands {
        let Some(operand) = operand else { continue };
        let mut key_path = path.to_vec();
        key_path.push(key.to_string());
        // Only reports an undeclared-`$param` operand; the subject-type check is
        // deliberately skipped.
        resolve_operand_type(tool, &operand, issues, &key_path);
    }
}

fn issue_at(message: String, path: &[String]) -> Issue {
    Issue {
        path: path.to_vec(),
        message,
        cont: true,
        union_errors: None,
    }
}

/// Roll fields are numeric, so every `$param` in one must name a numeric parameter.
fn validate_roll_refs(tool: &QtapCustomTool, issues: &mut Vec<Issue>) {
    let Some(Roll::Range(range)) = &tool.roll else {
        return;
    };

    // v4 iterates `Object.entries(roll)`, so only fields the author WROTE are
    // seen, in the schema's own order (the parsed object drops absent keys).
    let fields: [(&str, &Option<NumberOrParamRef>); 4] = [
        ("min", &range.min),
        ("max", &range.max),
        ("multiplier", &range.multiplier),
        ("offset", &range.offset),
    ];

    for (field, value) in fields {
        // A `$state` roll field must carry a numeric fallback — the only thing
        // knowable about it at load time (v4 `f48f34dc`).
        if let Some(NumberOrParamRef::StateRef(r)) = value {
            if !matches!(r.fallback, StateRefFallback::Number(_)) {
                issues.push(issue_at(
                    format!(
                        "roll.{field} uses a $state reference whose fallback is {} rather than a number",
                        r.fallback.type_name()
                    ),
                    &["roll".to_string(), field.to_string()],
                ));
            }
            continue;
        }
        let Some(NumberOrParamRef::ParamRef(r)) = value else {
            continue;
        };
        let path = vec!["roll".to_string(), field.to_string()];

        let Some(target) = tool.parameter(&r.param) else {
            issues.push(issue_at(
                format!(
                    "roll.{field} references undeclared parameter \"{}\"",
                    r.param
                ),
                &path,
            ));
            continue;
        };

        if !matches!(
            target.param_type,
            ParameterType::Number | ParameterType::Integer
        ) {
            issues.push(issue_at(
                format!(
                    "roll.{field} references parameter \"{}\", which is {} rather than numeric",
                    r.param,
                    target.param_type.as_str()
                ),
                &path,
            ));
        }
    }
}

/// The operand shapes the two comparator forms present to the shared check.
enum Operand<'a> {
    Number,
    String,
    Bool,
    Ref(&'a str),
    /// A `$state` reference, typed by its fallback (v4 `resolveOperandType`).
    StateFallback(ValueType),
}

fn fallback_value_type(f: &StateRefFallback) -> ValueType {
    match f {
        StateRefFallback::Number(_) => ValueType::Number,
        StateRefFallback::String(_) => ValueType::String,
        StateRefFallback::Bool(_) => ValueType::Boolean,
    }
}

fn number_operand(v: &NumberOrParamRef) -> Operand<'_> {
    match v {
        NumberOrParamRef::Number(_) => Operand::Number,
        NumberOrParamRef::ParamRef(r) => Operand::Ref(&r.param),
        NumberOrParamRef::StateRef(r) => Operand::StateFallback(fallback_value_type(&r.fallback)),
    }
}

fn any_operand(v: &AnyOperand) -> Operand<'_> {
    match v {
        AnyOperand::Number(_) => Operand::Number,
        AnyOperand::String(_) => Operand::String,
        AnyOperand::Bool(_) => Operand::Bool,
        AnyOperand::ParamRef(r) => Operand::Ref(&r.param),
        AnyOperand::StateRef(r) => Operand::StateFallback(fallback_value_type(&r.fallback)),
    }
}

fn string_operand(v: &StringOperand) -> Operand<'_> {
    match v {
        StringOperand::String(_) => Operand::String,
        StringOperand::ParamRef(r) => Operand::Ref(&r.param),
        StringOperand::StateRef(r) => Operand::StateFallback(fallback_value_type(&r.fallback)),
    }
}

/// The eight operand slots a wide comparator presents, in [`COMPARATOR_KEYS`]
/// order — the order v4's `validateComparator` walks them in, which is what the
/// issue sequence pins.
fn wide_operands<'a>(c: &'a ParamComparator) -> [(&'static str, Option<Operand<'a>>); 8] {
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

/// The same eight slots on an [`LlmComparator`] (`ok` is not a comparator).
fn llm_operands<'a>(c: &'a LlmComparator) -> [(&'static str, Option<Operand<'a>>); 8] {
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

fn validate_numeric_comparator(
    tool: &QtapCustomTool,
    c: &NumericComparator,
    subject_type: ValueType,
    subject_label: &str,
    issues: &mut Vec<Issue>,
    path: &[String],
) {
    let operands: [(&str, Option<Operand>); 8] = [
        ("gt", c.gt.as_ref().map(number_operand)),
        ("gte", c.gte.as_ref().map(number_operand)),
        ("lt", c.lt.as_ref().map(number_operand)),
        ("lte", c.lte.as_ref().map(number_operand)),
        ("eq", c.eq.as_ref().map(number_operand)),
        ("neq", c.neq.as_ref().map(number_operand)),
        // A numeric comparator's schema carries no containment keys at all.
        ("contains", None),
        ("ncontains", None),
    ];
    validate_comparator(tool, operands, subject_type, subject_label, issues, path);
}

fn validate_param_comparator(
    tool: &QtapCustomTool,
    c: &ParamComparator,
    subject_type: ValueType,
    subject_label: &str,
    issues: &mut Vec<Issue>,
    path: &[String],
) {
    validate_comparator(
        tool,
        wide_operands(c),
        subject_type,
        subject_label,
        issues,
        path,
    );
}

/// Check one comparator against the type of what it tests: every operand must
/// resolve, and the comparison must be one the two types can sustain.
fn validate_comparator(
    tool: &QtapCustomTool,
    operands: [(&str, Option<Operand>); 8],
    subject_type: ValueType,
    subject_label: &str,
    issues: &mut Vec<Issue>,
    path: &[String],
) {
    for (key, operand) in operands {
        let Some(operand) = operand else { continue };
        let mut key_path = path.to_vec();
        key_path.push(key.to_string());

        let Some(operand_type) = resolve_operand_type(tool, &operand, issues, &key_path) else {
            continue;
        };

        if ORDERING_KEYS.contains(&key) {
            if subject_type != ValueType::Number || operand_type != ValueType::Number {
                issues.push(issue_at(
                    format!(
                        "{key} orders {subject_label} against a {}, and only numbers can be ordered",
                        operand_type.as_str()
                    ),
                    &key_path,
                ));
            }
            continue;
        }

        if CONTAINMENT_KEYS.contains(&key) {
            if subject_type != ValueType::String {
                issues.push(issue_at(
                    format!(
                        "{key} searches {subject_label}, which is a {} — only a string can contain a substring",
                        subject_type.as_str()
                    ),
                    &key_path,
                ));
            } else if operand_type != ValueType::String {
                issues.push(issue_at(
                    format!(
                        "{key} looks for a {} inside {subject_label}, and a substring must be a string",
                        operand_type.as_str()
                    ),
                    &key_path,
                ));
            }
            continue;
        }

        if subject_type != operand_type {
            issues.push(issue_at(
                format!(
                    "{key} compares {subject_label}, which is a {}, with a {} — this can never hold",
                    subject_type.as_str(),
                    operand_type.as_str()
                ),
                &key_path,
            ));
        }
    }
}

/// The type an operand carries: a literal's own, or that of the parameter it
/// references. `None` means the operand is broken and has already been reported.
fn resolve_operand_type(
    tool: &QtapCustomTool,
    operand: &Operand,
    issues: &mut Vec<Issue>,
    path: &[String],
) -> Option<ValueType> {
    match operand {
        Operand::Ref(name) => match tool.parameter(name) {
            Some(target) => Some(target.param_type.value_type()),
            None => {
                issues.push(issue_at(
                    format!("references undeclared parameter \"{name}\""),
                    path,
                ));
                None
            }
        },
        Operand::Number => Some(ValueType::Number),
        Operand::String => Some(ValueType::String),
        Operand::Bool => Some(ValueType::Boolean),
        // A `$state` operand carries the type of its (required) fallback —
        // that is what run-time resolution is guaranteed to produce when the
        // path misses (v4 `f48f34dc`).
        Operand::StateFallback(t) => Some(*t),
    }
}

// ------------------------------------------------------------------ rendering

/// Render a rejection as the sentence an author reads on the load-error badge.
///
/// Exists because of unions. `when` is `true | object` and `roll` is
/// `string | object`, and when both branches fail Zod reports a bare "Invalid
/// input" at the union and buries the actual complaint — the misspelled
/// comparator, the malformed dice notation — one level down in `issue.errors`.
/// A rejection nobody can read is barely better than no rejection at all, so
/// every branch's message is surfaced, joined by "or" since either would have
/// satisfied the schema.
pub fn format_definition_issues(issues: &[Issue]) -> String {
    flatten_issues(issues).join("; ")
}

fn flatten_issues(issues: &[Issue]) -> Vec<String> {
    issues
        .iter()
        .map(|issue| {
            let located = |message: String| {
                if issue.path.is_empty() {
                    message
                } else {
                    format!("{}: {message}", issue.path.join("."))
                }
            };

            if let Some(errors) = &issue.union_errors {
                // Sub-issue paths are relative to the union, so they are
                // flattened without the prefix — v4 does the same.
                let branches: Vec<String> = errors
                    .iter()
                    .map(|branch| flatten_issues(branch).join("; "))
                    .filter(|b| !b.is_empty())
                    .collect();
                if !branches.is_empty() {
                    return located(branches.join(" — or — "));
                }
            }

            located(issue.message.clone())
        })
        .collect()
}

/// Top-level keys the v1 format knows about. Anything else is reserved for v2.
const KNOWN_TOP_LEVEL_KEYS: [&str; 11] = [
    "$schema",
    "name",
    "title",
    "description",
    "disabled",
    "revealOdds",
    "defaultVisibility",
    "parameters",
    "roll",
    "llm",
    "outcomes",
];

/// Report top-level keys this build doesn't understand, so discovery can log
/// them. They are tolerated, not rejected: `persist` is a planned v2 key, and an
/// older build must not choke on a newer file.
pub fn collect_unknown_keys(raw: &Value) -> Vec<String> {
    match raw {
        Value::Object(o) => o
            .keys()
            .filter(|k| !KNOWN_TOP_LEVEL_KEYS.contains(&k.as_str()))
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}
