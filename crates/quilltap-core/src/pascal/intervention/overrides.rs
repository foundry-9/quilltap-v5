//! Operator intervention — what the operator may change about a held run (v4
//! `lib/pascal/intervention/overrides.ts`).
//!
//! [`InterventionOverrides`] is both the request body of the preview / Re-ask /
//! resolve actions and the shape the dialog builds. Every present key is a PIN:
//! recompute never overwrites it, and everything downstream of it that is not
//! pinned is recomputed (spec §3.2).
//!
//! [`check_overrides`] is the server's enforcement of the author's flags: an
//! override naming a hidden field is refused, not quietly dropped — the flag
//! is a rule, not a UI hint.
//!
//! The parses below are twins of v4's Zod 4.6.7 schemas, reproducing the
//! issue MESSAGES in Zod's order (shape keys in declaration order, then the
//! strict tail). Only the messages matter: v4's `readBody` answers
//! `issues.map(i => i.message).join('; ')` and nothing else. Every sentence was
//! measured through v4's REAL schema (`pascal_intervention_pure_equivalence`'s
//! `parse` rows), never written from memory.

use serde_json::{Map, Value};

use super::eligibility::intervention_eligibility;
use crate::api::zod_issues::{ZodIssue, MAX_SAFE_INTEGER};
use crate::db::js_number_to_json;
use crate::pascal::custom_tool_types::{
    OutcomeState, QtapCustomTool, Visibility, MAX_CHIP_LABEL_LENGTH, MAX_LLM_OUTPUT_CEILING,
    MAX_LLM_PROMPT_LENGTH, MAX_TITLE_LENGTH,
};
use crate::pascal::custom_tools::{EffectPin, EffectPinMode, PinnedAnswer, RollForm};
use crate::pascal::dice::MAX_DICE_COUNT;
use crate::pascal::js_value::{number_to_string, sort_v8_own_keys, to_number};
use crate::pascal::metadata_match::ResolvedValue;

/// A rendered message may quote a long consult answer, so its cap is the
/// answer's plus room for the template (v4 `MAX_OVERRIDE_MESSAGE_LENGTH`).
pub const MAX_OVERRIDE_MESSAGE_LENGTH: usize = MAX_LLM_OUTPUT_CEILING as usize + 4000;

/// A pin's item reference cap and count cap (v4 `EffectPinSchema.items`).
const MAX_PIN_ITEM_LENGTH: usize = 400;
const MAX_PIN_ITEMS: usize = 16;

/// Who announces a run (v4 `Announcer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Announcer {
    Pascal,
    Character,
}

impl Announcer {
    pub fn as_str(self) -> &'static str {
        match self {
            Announcer::Pascal => "pascal",
            Announcer::Character => "character",
        }
    }
}

/// The operator's overrides, as v4's `InterventionOverridesSchema` parses them.
/// Every field is a pin; absent means "recompute it".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InterventionOverrides {
    /// Parameter overrides, in V8 own-key order (integer-like keys first).
    pub params: Option<Vec<(String, ResolvedValue)>>,
    pub roll: Option<f64>,
    pub dice: Option<Vec<f64>>,
    pub value: Option<f64>,
    pub llm_prompt: Option<String>,
    pub llm_answer: Option<PinnedAnswer>,
    pub outcome_index: Option<usize>,
    pub message: Option<String>,
    pub state: Option<OutcomeState>,
    pub chip_label: Option<String>,
    pub title: Option<String>,
    pub visibility: Option<Visibility>,
    pub announcer: Option<Announcer>,
    /// Effect pins keyed by their index STRING (JSON keys), V8 order.
    pub effects: Option<Vec<(String, EffectPin)>>,
}

impl InterventionOverrides {
    /// `Object.keys(overrides)` of the PARSED object — the present keys in the
    /// schema's declaration order (Zod's output order), as the `pinned` log
    /// field lists them.
    pub fn keys(&self) -> Vec<&'static str> {
        let mut k = Vec::new();
        let mut push = |present: bool, key: &'static str| {
            if present {
                k.push(key);
            }
        };
        push(self.params.is_some(), "params");
        push(self.roll.is_some(), "roll");
        push(self.dice.is_some(), "dice");
        push(self.value.is_some(), "value");
        push(self.llm_prompt.is_some(), "llmPrompt");
        push(self.llm_answer.is_some(), "llmAnswer");
        push(self.outcome_index.is_some(), "outcomeIndex");
        push(self.message.is_some(), "message");
        push(self.state.is_some(), "state");
        push(self.chip_label.is_some(), "chipLabel");
        push(self.title.is_some(), "title");
        push(self.visibility.is_some(), "visibility");
        push(self.announcer.is_some(), "announcer");
        push(self.effects.is_some(), "effects");
        k
    }

    /// The parsed object as JSON, in Zod's output order.
    pub fn to_wire(&self) -> Value {
        let mut m = Map::new();
        if let Some(params) = &self.params {
            m.insert(
                "params".into(),
                Value::Object(
                    params
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_value()))
                        .collect(),
                ),
            );
        }
        if let Some(roll) = self.roll {
            m.insert("roll".into(), js_number_to_json(roll));
        }
        if let Some(dice) = &self.dice {
            m.insert(
                "dice".into(),
                Value::Array(dice.iter().map(|f| js_number_to_json(*f)).collect()),
            );
        }
        if let Some(value) = self.value {
            m.insert("value".into(), js_number_to_json(value));
        }
        if let Some(prompt) = &self.llm_prompt {
            m.insert("llmPrompt".into(), Value::String(prompt.clone()));
        }
        if let Some(answer) = &self.llm_answer {
            m.insert("llmAnswer".into(), pinned_answer_wire(answer));
        }
        if let Some(i) = self.outcome_index {
            m.insert("outcomeIndex".into(), Value::from(i));
        }
        if let Some(message) = &self.message {
            m.insert("message".into(), Value::String(message.clone()));
        }
        if let Some(state) = self.state {
            m.insert("state".into(), Value::String(state.as_str().into()));
        }
        if let Some(chip) = &self.chip_label {
            m.insert("chipLabel".into(), Value::String(chip.clone()));
        }
        if let Some(title) = &self.title {
            m.insert("title".into(), Value::String(title.clone()));
        }
        if let Some(visibility) = self.visibility {
            m.insert("visibility".into(), visibility_wire(visibility));
        }
        if let Some(announcer) = self.announcer {
            m.insert("announcer".into(), Value::String(announcer.as_str().into()));
        }
        if let Some(effects) = &self.effects {
            m.insert(
                "effects".into(),
                Value::Object(
                    effects
                        .iter()
                        .map(|(k, pin)| (k.clone(), effect_pin_wire(pin)))
                        .collect(),
                ),
            );
        }
        Value::Object(m)
    }

    /// The overrides without `llmAnswer` — Re-ask's `const { llmAnswer, ...rest }`
    /// (a fresh answer replaces a pinned one: Re-ask unpins the answer).
    pub fn without_llm_answer(&self) -> Self {
        Self {
            llm_answer: None,
            ..self.clone()
        }
    }
}

/// `{ok, output}` — the pinned answer's wire.
pub fn pinned_answer_wire(answer: &PinnedAnswer) -> Value {
    let mut m = Map::new();
    m.insert("ok".into(), Value::Bool(answer.ok));
    m.insert("output".into(), Value::String(answer.output.clone()));
    Value::Object(m)
}

/// `{mode, value?, items?, log?}` — `EffectPinSchema`'s output order.
pub fn effect_pin_wire(pin: &EffectPin) -> Value {
    let mut m = Map::new();
    m.insert("mode".into(), Value::String(pin.mode.as_str().into()));
    if let Some(value) = &pin.value {
        m.insert("value".into(), value.to_value());
    }
    if let Some(items) = &pin.items {
        m.insert(
            "items".into(),
            Value::Array(items.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(log) = &pin.log {
        m.insert("log".into(), Value::String(log.clone()));
    }
    Value::Object(m)
}

fn visibility_wire(v: Visibility) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// The operator's decision on a held run (v4 `InterventionDecisionSchema`).
#[derive(Debug, Clone, PartialEq)]
pub enum InterventionDecisionInput {
    Deal {
        overrides: Box<InterventionOverrides>,
    },
    AsRolled,
    Veto,
}

impl InterventionDecisionInput {
    /// The `kind` the wire names.
    pub fn kind(&self) -> &'static str {
        match self {
            InterventionDecisionInput::Deal { .. } => "deal",
            InterventionDecisionInput::AsRolled => "as-rolled",
            InterventionDecisionInput::Veto => "veto",
        }
    }
}

/// What a held run was dealt in — enough to say which overrides fit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DealtShape {
    pub roll_form: RollForm,
    /// Dice form: how many faces the notation rolls.
    pub dice_count: Option<usize>,
}

// ---------------------------------------------------------------- the parses

/// The issue messages of a refused body, joined as v4's `readBody` joins them.
pub fn join_messages(issues: &[ZodIssue]) -> String {
    issues
        .iter()
        .map(|i| i.message())
        .collect::<Vec<_>>()
        .join("; ")
}

fn no_path() -> Vec<Value> {
    Vec::new()
}

/// `z.number()` (JSON numbers are always finite, so `.finite()` never speaks).
fn number(v: &Value, issues: &mut Vec<ZodIssue>) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        other => {
            issues.push(ZodIssue::invalid_type("number", no_path(), Some(other)));
            None
        }
    }
}

/// `z.number().int()` — a non-number is the plain type miss; a non-integer
/// number is the `int` miss (which aborts the checks after it); a number past
/// the safe range is the safe-int bound.
fn int(v: &Value, issues: &mut Vec<ZodIssue>) -> Option<f64> {
    let f = number(v, issues)?;
    if f.fract() != 0.0 {
        issues.push(ZodIssue::invalid_int_type(no_path(), Some(v)));
        return None;
    }
    if f < -MAX_SAFE_INTEGER {
        issues.push(ZodIssue::too_small_int(no_path()));
        return None;
    }
    if f > MAX_SAFE_INTEGER {
        issues.push(ZodIssue::too_big_int(no_path()));
        return None;
    }
    Some(f)
}

/// `z.string()` with optional code-point `.min` / `.max`.
fn string(
    v: &Value,
    min: Option<usize>,
    max: Option<usize>,
    issues: &mut Vec<ZodIssue>,
) -> Option<String> {
    let Value::String(s) = v else {
        issues.push(ZodIssue::invalid_type("string", no_path(), Some(v)));
        return None;
    };
    let before = issues.len();
    if let Some(n) = min.filter(|n| !crate::jsstr::zod_len_min_ok(s, *n)) {
        issues.push(ZodIssue::too_small_string(Value::from(n), no_path()));
    }
    if let Some(n) = max.filter(|n| !crate::jsstr::zod_len_max_ok(s, *n)) {
        issues.push(ZodIssue::too_big_string(Value::from(n), no_path()));
    }
    (issues.len() == before).then(|| s.clone())
}

fn boolean(v: &Value, issues: &mut Vec<ZodIssue>) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        other => {
            issues.push(ZodIssue::invalid_type("boolean", no_path(), Some(other)));
            None
        }
    }
}

/// `z.enum([...])` — an absent key reads as `undefined`, which the enum
/// refuses with the same option list.
fn enum_of<T: Copy>(
    v: Option<&Value>,
    options: &[(&str, T)],
    issues: &mut Vec<ZodIssue>,
) -> Option<T> {
    if let Some(Value::String(s)) = v {
        if let Some((_, t)) = options.iter().find(|(name, _)| name == s) {
            return Some(*t);
        }
    }
    let names: Vec<&str> = options.iter().map(|(n, _)| *n).collect();
    issues.push(ZodIssue::invalid_value(&names, no_path()));
    None
}

/// `LiteralSchema = z.union([z.number().finite(), z.string(), z.boolean()])` —
/// every other JSON value fails all three branches: `Invalid input`.
fn literal(v: &Value, issues: &mut Vec<ZodIssue>) -> Option<ResolvedValue> {
    match v {
        Value::Number(n) => n.as_f64().map(ResolvedValue::Number),
        Value::String(s) => Some(ResolvedValue::String(s.clone())),
        Value::Bool(b) => Some(ResolvedValue::Bool(*b)),
        _ => {
            issues.push(ZodIssue::invalid_union(Vec::new(), no_path()));
            None
        }
    }
}

/// The strict tail: one `Unrecognized key(s)` issue listing keys outside
/// `known`, in the input's own order.
fn strict_tail(obj: &Map<String, Value>, known: &[&str], issues: &mut Vec<ZodIssue>) {
    let extra: Vec<String> = obj
        .keys()
        .filter(|k| !known.contains(&k.as_str()))
        .cloned()
        .collect();
    if !extra.is_empty() {
        issues.push(ZodIssue::unrecognized_keys(extra, no_path()));
    }
}

fn object<'a>(v: Option<&'a Value>, issues: &mut Vec<ZodIssue>) -> Option<&'a Map<String, Value>> {
    match v {
        Some(Value::Object(o)) => Some(o),
        other => {
            issues.push(ZodIssue::invalid_type("object", no_path(), other));
            None
        }
    }
}

/// A record's entries in V8 own-key order (Zod iterates the object's keys).
fn record_entries(obj: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut entries: Vec<(&String, &Value)> = obj.iter().collect();
    sort_v8_own_keys(&mut entries, |(k, _)| k.as_str());
    entries
}

fn record<'a>(v: &'a Value, issues: &mut Vec<ZodIssue>) -> Option<&'a Map<String, Value>> {
    match v {
        Value::Object(o) => Some(o),
        other => {
            issues.push(ZodIssue::invalid_type("record", no_path(), Some(other)));
            None
        }
    }
}

/// v4 `EffectPinSchema`.
fn effect_pin(v: &Value, issues: &mut Vec<ZodIssue>) -> Option<EffectPin> {
    let before = issues.len();
    let obj = object(Some(v), issues)?;
    let mode = enum_of(
        obj.get("mode"),
        &[
            ("auto", EffectPinMode::Auto),
            ("on", EffectPinMode::On),
            ("off", EffectPinMode::Off),
        ],
        issues,
    );
    let value = obj.get("value").and_then(|v| literal(v, issues));
    let items = match obj.get("items") {
        Some(Value::Array(items)) if items.len() > MAX_PIN_ITEMS => {
            // Zod 4.6.7 `capSize`: past the cap, the size issue ALONE.
            issues.push(ZodIssue::too_big_array(
                Value::from(MAX_PIN_ITEMS),
                no_path(),
            ));
            None
        }
        Some(Value::Array(items)) => {
            let mut out = Vec::new();
            for item in items {
                if let Some(s) = string(item, None, Some(MAX_PIN_ITEM_LENGTH), issues) {
                    out.push(s);
                }
            }
            Some(out)
        }
        Some(other) => {
            issues.push(ZodIssue::invalid_type("array", no_path(), Some(other)));
            None
        }
        None => None,
    };
    let log = obj
        .get("log")
        .and_then(|v| string(v, None, Some(MAX_OVERRIDE_MESSAGE_LENGTH), issues));
    strict_tail(obj, &["mode", "value", "items", "log"], issues);
    if issues.len() != before {
        return None;
    }
    Some(EffectPin {
        mode: mode?,
        value,
        items,
        log,
    })
}

/// v4 `InterventionOverridesSchema.safeParse(v)` — the parsed overrides, or
/// the issues in Zod's order.
pub fn parse_intervention_overrides(
    v: Option<&Value>,
) -> Result<InterventionOverrides, Vec<ZodIssue>> {
    let mut issues = Vec::new();
    let Some(obj) = object(v, &mut issues) else {
        return Err(issues);
    };
    let mut out = InterventionOverrides::default();

    if let Some(p) = obj.get("params") {
        if let Some(rec) = record(p, &mut issues) {
            let mut params = Vec::new();
            for (k, v) in record_entries(rec) {
                if let Some(lit) = literal(v, &mut issues) {
                    params.push((k.clone(), lit));
                }
            }
            out.params = Some(params);
        }
    }
    if let Some(v) = obj.get("roll") {
        out.roll = number(v, &mut issues);
    }
    if let Some(v) = obj.get("dice") {
        out.dice = match v {
            Value::Array(items) if items.len() > MAX_DICE_COUNT as usize => {
                issues.push(ZodIssue::too_big_array(
                    Value::from(MAX_DICE_COUNT),
                    no_path(),
                ));
                None
            }
            Value::Array(items) => {
                let before = issues.len();
                let faces: Vec<f64> = items.iter().filter_map(|f| int(f, &mut issues)).collect();
                if items.is_empty() {
                    issues.push(ZodIssue::too_small_array(Value::from(1), no_path()));
                }
                (issues.len() == before).then_some(faces)
            }
            other => {
                issues.push(ZodIssue::invalid_type("array", no_path(), Some(other)));
                None
            }
        };
    }
    if let Some(v) = obj.get("value") {
        out.value = number(v, &mut issues);
    }
    if let Some(v) = obj.get("llmPrompt") {
        out.llm_prompt = string(v, Some(1), Some(MAX_LLM_PROMPT_LENGTH), &mut issues);
    }
    if let Some(v) = obj.get("llmAnswer") {
        let before = issues.len();
        if let Some(a) = object(Some(v), &mut issues) {
            let ok = match a.get("ok") {
                Some(v) => boolean(v, &mut issues),
                None => {
                    issues.push(ZodIssue::invalid_type("boolean", no_path(), None));
                    None
                }
            };
            let output = match a.get("output") {
                Some(v) => string(v, None, Some(MAX_LLM_OUTPUT_CEILING as usize), &mut issues),
                None => {
                    issues.push(ZodIssue::invalid_type("string", no_path(), None));
                    None
                }
            };
            strict_tail(a, &["ok", "output"], &mut issues);
            if issues.len() == before {
                if let (Some(ok), Some(output)) = (ok, output) {
                    out.llm_answer = Some(PinnedAnswer { ok, output });
                }
            }
        }
    }
    if let Some(v) = obj.get("outcomeIndex") {
        if let Some(f) = int(v, &mut issues) {
            if f < 0.0 {
                issues.push(ZodIssue::too_small_number(Value::from(0), no_path()));
            } else {
                out.outcome_index = Some(f as usize);
            }
        }
    }
    if let Some(v) = obj.get("message") {
        out.message = string(v, Some(1), Some(MAX_OVERRIDE_MESSAGE_LENGTH), &mut issues);
    }
    if let Some(v) = obj.get("state") {
        out.state = enum_of(
            Some(v),
            &[
                ("success", OutcomeState::Success),
                ("partial", OutcomeState::Partial),
                ("failure", OutcomeState::Failure),
                ("info", OutcomeState::Info),
            ],
            &mut issues,
        );
    }
    if let Some(v) = obj.get("chipLabel") {
        out.chip_label = string(v, Some(1), Some(MAX_CHIP_LABEL_LENGTH), &mut issues);
    }
    if let Some(v) = obj.get("title") {
        out.title = string(v, Some(1), Some(MAX_TITLE_LENGTH), &mut issues);
    }
    if let Some(v) = obj.get("visibility") {
        out.visibility = enum_of(
            Some(v),
            &[
                ("public", Visibility::Public),
                ("whisper", Visibility::Whisper),
            ],
            &mut issues,
        );
    }
    if let Some(v) = obj.get("announcer") {
        out.announcer = enum_of(
            Some(v),
            &[
                ("pascal", Announcer::Pascal),
                ("character", Announcer::Character),
            ],
            &mut issues,
        );
    }
    if let Some(e) = obj.get("effects") {
        if let Some(rec) = record(e, &mut issues) {
            let mut pins = Vec::new();
            for (k, v) in record_entries(rec) {
                // `z.string().regex(/^\d+$/, 'effect keys are indices')` — a
                // refused key is zod's `invalid_key`, and its value is not
                // parsed.
                if k.is_empty() || !k.bytes().all(|b| b.is_ascii_digit()) {
                    issues.push(ZodIssue::invalid_record_key(no_path()));
                    continue;
                }
                if let Some(pin) = effect_pin(v, &mut issues) {
                    pins.push((k.clone(), pin));
                }
            }
            out.effects = Some(pins);
        }
    }

    strict_tail(
        obj,
        &[
            "params",
            "roll",
            "dice",
            "value",
            "llmPrompt",
            "llmAnswer",
            "outcomeIndex",
            "message",
            "state",
            "chipLabel",
            "title",
            "visibility",
            "announcer",
            "effects",
        ],
        &mut issues,
    );
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

/// v4 `InterventionDecisionSchema.safeParse(v)` — a discriminated union on
/// `kind` over three strict objects.
pub fn parse_intervention_decision(
    v: Option<&Value>,
) -> Result<InterventionDecisionInput, Vec<ZodIssue>> {
    let mut issues = Vec::new();
    let Some(obj) = object(v, &mut issues) else {
        return Err(issues);
    };
    let kind = obj.get("kind").and_then(Value::as_str);
    match kind {
        Some("deal") => {
            let overrides = match parse_intervention_overrides(obj.get("overrides")) {
                Ok(o) => Some(o),
                Err(e) => {
                    issues.extend(e);
                    None
                }
            };
            strict_tail(obj, &["kind", "overrides"], &mut issues);
            match overrides {
                Some(overrides) if issues.is_empty() => Ok(InterventionDecisionInput::Deal {
                    overrides: Box::new(overrides),
                }),
                _ => Err(issues),
            }
        }
        Some(k @ ("as-rolled" | "veto")) => {
            strict_tail(obj, &["kind"], &mut issues);
            if !issues.is_empty() {
                return Err(issues);
            }
            Ok(if k == "veto" {
                InterventionDecisionInput::Veto
            } else {
                InterventionDecisionInput::AsRolled
            })
        }
        _ => Err(vec![ZodIssue::invalid_discriminator(
            &["deal", "as-rolled", "veto"],
            no_path(),
        )]),
    }
}

/// `z.object({id: z.string().min(1), …})`'s `id` — the non-strict outer body
/// both actions read.
fn body_id(obj: &Map<String, Value>, issues: &mut Vec<ZodIssue>) -> Option<String> {
    match obj.get("id") {
        Some(v) => string(v, Some(1), None, issues),
        None => {
            issues.push(ZodIssue::invalid_type("string", no_path(), None));
            None
        }
    }
}

/// v4 `previewSchema = z.object({id: z.string().min(1), overrides:
/// InterventionOverridesSchema})` — the preview and Re-ask body. Unknown outer
/// keys are stripped (a plain `z.object`).
pub fn parse_preview_body(v: &Value) -> Result<(String, InterventionOverrides), Vec<ZodIssue>> {
    let mut issues = Vec::new();
    let Some(obj) = object(Some(v), &mut issues) else {
        return Err(issues);
    };
    let id = body_id(obj, &mut issues);
    let overrides = match parse_intervention_overrides(obj.get("overrides")) {
        Ok(o) => Some(o),
        Err(e) => {
            issues.extend(e);
            None
        }
    };
    match (id, overrides) {
        (Some(id), Some(o)) if issues.is_empty() => Ok((id, o)),
        _ => Err(issues),
    }
}

/// v4 `resolveSchema = z.object({id: z.string().min(1), decision:
/// InterventionDecisionSchema})`.
pub fn parse_resolve_body(v: &Value) -> Result<(String, InterventionDecisionInput), Vec<ZodIssue>> {
    let mut issues = Vec::new();
    let Some(obj) = object(Some(v), &mut issues) else {
        return Err(issues);
    };
    let id = body_id(obj, &mut issues);
    let decision = match parse_intervention_decision(obj.get("decision")) {
        Ok(d) => Some(d),
        Err(e) => {
            issues.extend(e);
            None
        }
    };
    match (id, decision) {
        (Some(id), Some(d)) if issues.is_empty() => Ok((id, d)),
        _ => Err(issues),
    }
}

// ---------------------------------------------------------------- the check

/// `None` when every override names a shown, applicable field of this run;
/// otherwise the reason it is refused (the API's 400). The checks run in v4's
/// order — the first refusal wins.
pub fn check_overrides(
    definition: &QtapCustomTool,
    dealt: DealtShape,
    overrides: &InterventionOverrides,
) -> Option<String> {
    let eligible = intervention_eligibility(definition, dealt.roll_form);
    let shown = |field: &str| eligible.fields.contains(&field);

    for (name, _) in overrides.params.iter().flatten() {
        if !eligible.params.contains(name) {
            return Some(format!("parameter \"{name}\" is not open to intervention"));
        }
    }

    if (overrides.roll.is_some() || overrides.dice.is_some()) && !shown("roll") {
        return Some("the roll is not open to intervention".into());
    }
    if overrides.roll.is_some() && dealt.roll_form != RollForm::Range {
        return Some("a dice roll is set by its faces, not a number".into());
    }
    if let Some(dice) = &overrides.dice {
        if dealt.roll_form != RollForm::Dice {
            return Some("this roll is not dice".into());
        }
        if Some(dice.len()) != dealt.dice_count {
            // `${dealt.diceCount}` — `undefined` when the shape carries none.
            let count = dealt
                .dice_count
                .map(|c| c.to_string())
                .unwrap_or_else(|| "undefined".into());
            return Some(format!("this roll takes {count} dice, not {}", dice.len()));
        }
    }

    let field_of: [(bool, &str); 10] = [
        (overrides.value.is_some(), "value"),
        (overrides.llm_prompt.is_some(), "llmPrompt"),
        (overrides.llm_answer.is_some(), "llmAnswer"),
        (overrides.outcome_index.is_some(), "outcome"),
        (overrides.message.is_some(), "message"),
        (overrides.state.is_some(), "state"),
        (overrides.chip_label.is_some(), "chipLabel"),
        (overrides.title.is_some(), "title"),
        (overrides.visibility.is_some(), "visibility"),
        (overrides.announcer.is_some(), "announcer"),
    ];
    for (present, field) in field_of {
        if present && !shown(field) {
            return Some(format!("the {field} is not open to intervention"));
        }
    }

    if let Some(i) = overrides.outcome_index {
        if i >= definition.outcomes.len() {
            return Some(format!("there is no outcome row {i}"));
        }
    }

    for (key, _) in overrides.effects.iter().flatten() {
        // `Number(key)` — the key passed `^\d+$`, so it is a plain decimal.
        let index = to_number(&Value::String(key.clone()));
        let eligible_index = index.fract() == 0.0
            && index >= 0.0
            && eligible.effects.iter().any(|e| *e as f64 == index);
        if !eligible_index {
            return Some(format!(
                "effect {} is not open to intervention",
                number_to_string(index)
            ));
        }
    }

    None
}
