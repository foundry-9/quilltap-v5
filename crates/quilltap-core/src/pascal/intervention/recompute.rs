//! Operator intervention — recompute a held run under the operator's overrides
//! (v4 `lib/pascal/intervention/recompute.ts`).
//!
//! Pure apart from the engine's own logging: it never calls a model and never
//! writes. It calls [`execute_custom_tool`] with the recorded draw, the consult
//! in use and the pins, so a recomputed run is dealt by the same engine as
//! every other run, not a copy of it (spec §3.2). The preview, Re-ask and
//! resolve actions all come through here, so the final run is always computed
//! server-side from the frozen snapshot — never taken from the client.

use std::sync::Arc;

use serde_json::{Map, Value};

use super::overrides::{Announcer, InterventionOverrides};
use super::spans::{describe_span, within_span};
use crate::db::js_number_to_json;
use crate::inventory::refs::InventoryContext;
use crate::pascal::custom_tool_types::{
    display_title, CustomToolEffectBody, OutcomeState, QtapCustomTool, Visibility,
};
use crate::pascal::custom_tools::{
    execute_custom_tool, CustomToolRunError, CustomToolRunResult, EffectPin, LlmConsultResult,
    RecordedDraw, ReplayInputs, ResolvedEffect, ResolvedParams, RunPins,
};
use crate::pascal::dice::RandomBytes;
use crate::pascal::js_value::to_number;
use crate::pascal::metadata_match::ResolvedValue;

/// Everything a run was dealt against, frozen at deal time — a mid-hold edit
/// elsewhere is not picked up (v4 `RunSnapshot`).
#[derive(Clone)]
pub struct RunSnapshot {
    /// Parameters as supplied by the entrance (`None` = every default).
    pub params: Option<Map<String, Value>>,
    /// The entrance's explicit privacy, or `None` for the definition's default.
    pub private: Option<bool>,
    pub metadata: Map<String, Value>,
    pub progress: Map<String, Value>,
    pub now_ms: i64,
    pub character: Option<String>,
    /// The merged persistent state (`cascade?.merged ?? {}`).
    pub state: Value,
    /// The rolling character's bag, as the run service loaded it.
    pub inventory: Option<Arc<dyn InventoryContext + Send + Sync>>,
}

impl std::fmt::Debug for RunSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunSnapshot")
            .field("params", &self.params)
            .field("private", &self.private)
            .field("metadata", &self.metadata)
            .field("progress", &self.progress)
            .field("now_ms", &self.now_ms)
            .field("character", &self.character)
            .field("state", &self.state)
            .field("inventory", &self.inventory.as_ref().map(|_| "<bag>"))
            .finish()
    }
}

/// How the run will be presented — the run service's concerns, beside the
/// engine's result (v4 `RunPresentation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPresentation {
    pub title: String,
    pub visibility: Visibility,
    pub announcer: Announcer,
}

impl RunPresentation {
    /// `{title, visibility, announcer}` — v4's key order.
    pub fn to_wire(&self) -> Value {
        let mut m = Map::new();
        m.insert("title".into(), Value::String(self.title.clone()));
        m.insert(
            "visibility".into(),
            serde_json::to_value(self.visibility).unwrap_or(Value::Null),
        );
        m.insert(
            "announcer".into(),
            Value::String(self.announcer.as_str().into()),
        );
        Value::Object(m)
    }
}

/// What a recompute is handed (v4 `RecomputeInput`).
pub struct RecomputeInput<'a> {
    pub definition: &'a QtapCustomTool,
    pub snapshot: &'a RunSnapshot,
    pub draw: &'a RecordedDraw,
    /// The answer in use: the latest consult. `None` for a tool with no
    /// consult.
    pub consult: Option<&'a LlmConsultResult>,
    pub overrides: &'a InterventionOverrides,
    /// The row the run was first dealt — a pinned message written against it
    /// may not fit a new winner.
    pub dealt_outcome_index: Option<usize>,
}

/// A held run, recomputed (v4 `RecomputedRun`).
#[derive(Debug, Clone, PartialEq)]
pub struct RecomputedRun {
    pub result: CustomToolRunResult,
    pub presentation: RunPresentation,
    /// The consult's answer was produced by a different prompt than the one
    /// now in force.
    pub stale_answer: bool,
    /// The prompt a Re-ask would pose now: the pinned prompt, or the one
    /// rendered under the overrides.
    pub prompt_now: Option<String>,
    /// Parameter names whose supplied override was clamped or coerced.
    pub clamped_params: Vec<String>,
    /// Advisory notes for the dialog (spec §3.3). Never blocking.
    pub warnings: Vec<String>,
}

/// The overrides' engine half, keyed the way the engine wants it (v4
/// `pinsFromOverrides`). Effect keys are re-keyed `Number(key)`; a later
/// duplicate (`"1"` then `"01"`) overwrites, as the JS object assignment does.
pub fn pins_from_overrides(overrides: &InterventionOverrides) -> RunPins {
    let effects = overrides.effects.as_ref().and_then(|pins| {
        let mut map = std::collections::BTreeMap::<usize, EffectPin>::new();
        for (key, pin) in pins {
            let index = to_number(&Value::String(key.clone()));
            if index.fract() == 0.0 && index >= 0.0 && index <= usize::MAX as f64 {
                map.insert(index as usize, pin.clone());
            }
        }
        (!map.is_empty()).then_some(map)
    });
    RunPins {
        roll: overrides.roll,
        dice: overrides.dice.clone(),
        value: overrides.value,
        llm_answer: overrides.llm_answer.clone(),
        outcome_index: overrides.outcome_index,
        message: overrides.message.clone(),
        state: overrides.state,
        chip_label: overrides.chip_label.clone(),
        effects,
    }
}

/// [`RunPins`] as JSON, in v4's spread order (the pure family's `pins` rows).
pub fn pins_to_wire(pins: &RunPins) -> Value {
    let mut m = Map::new();
    if let Some(roll) = pins.roll {
        m.insert("roll".into(), js_number_to_json(roll));
    }
    if let Some(dice) = &pins.dice {
        m.insert(
            "dice".into(),
            Value::Array(dice.iter().map(|f| js_number_to_json(*f)).collect()),
        );
    }
    if let Some(value) = pins.value {
        m.insert("value".into(), js_number_to_json(value));
    }
    if let Some(answer) = &pins.llm_answer {
        m.insert(
            "llmAnswer".into(),
            super::overrides::pinned_answer_wire(answer),
        );
    }
    if let Some(i) = pins.outcome_index {
        m.insert("outcomeIndex".into(), Value::from(i));
    }
    if let Some(message) = &pins.message {
        m.insert("message".into(), Value::String(message.clone()));
    }
    if let Some(state) = pins.state {
        m.insert("state".into(), Value::String(state.as_str().into()));
    }
    if let Some(chip) = &pins.chip_label {
        m.insert("chipLabel".into(), Value::String(chip.clone()));
    }
    if let Some(effects) = &pins.effects {
        m.insert(
            "effects".into(),
            Value::Object(
                effects
                    .iter()
                    .map(|(i, pin)| (i.to_string(), super::overrides::effect_pin_wire(pin)))
                    .collect(),
            ),
        );
    }
    Value::Object(m)
}

/// The presentation a definition gives a run, under any presentation
/// overrides (v4 `presentationFor`).
pub fn presentation_for(
    definition: &QtapCustomTool,
    visibility: Visibility,
    title: Option<&str>,
    announcer: Option<Announcer>,
) -> RunPresentation {
    RunPresentation {
        title: title
            .map(str::to_string)
            .unwrap_or_else(|| display_title(&definition.name, definition.title.as_deref())),
        visibility,
        announcer: announcer.unwrap_or(if definition.announce_as_character == Some(true) {
            Announcer::Character
        } else {
            Announcer::Pascal
        }),
    }
}

/// Recompute a held run. `rng` is never drawn from on a replay — the recorded
/// draw stands in — and the pure family hands it an EMPTY `FixedBytes` to
/// prove it. Fails with the engine's [`CustomToolRunError`] when the overrides
/// make the run undealable (a parameter that will not coerce, a faces count
/// that does not fit) — the caller reports it.
pub async fn recompute_run(
    input: RecomputeInput<'_>,
    rng: &mut (dyn RandomBytes + Send),
) -> Result<RecomputedRun, CustomToolRunError> {
    let RecomputeInput {
        definition,
        snapshot,
        overrides,
        ..
    } = input;
    // `{...snapshot.params, ...overrides.params}` — an overridden key keeps its
    // place, a new one is appended.
    let mut supplied = snapshot.params.clone().unwrap_or_default();
    for (name, value) in overrides.params.iter().flatten() {
        supplied.insert(name.clone(), value.to_value());
    }
    let privacy = match overrides.visibility {
        Some(v) => Some(v == Visibility::Whisper),
        None => snapshot.private,
    };

    let pins = pins_from_overrides(overrides);
    let replay = ReplayInputs {
        draw: Some(input.draw),
        consult: input.consult,
        pins: Some(&pins),
    };
    let result = execute_custom_tool(
        definition,
        Some(&supplied),
        privacy,
        Some(&snapshot.metadata),
        Some(&snapshot.state),
        rng,
        // Never a model: the consult in use, or a pinned answer, stands in.
        None,
        Some(&snapshot.progress),
        snapshot.now_ms,
        snapshot.character.as_deref(),
        snapshot
            .inventory
            .as_deref()
            .map(|i| i as &dyn InventoryContext),
        Some(&replay),
    )
    .await?;

    let clamped_params = overrides
        .params
        .iter()
        .flatten()
        .filter(|(name, given)| {
            !result
                .params
                .iter()
                .find(|(k, _)| k == name)
                .is_some_and(|(_, resolved)| resolved.strict_eq(given))
        })
        .map(|(name, _)| name.clone())
        .collect();

    let prompt_now = definition.llm.as_ref().and_then(|_| {
        overrides
            .llm_prompt
            .clone()
            .or_else(|| result.llm_prompt_rendered.clone())
    });
    // A pinned answer is the operator's own and is never "out of date".
    let stale_answer = definition.llm.is_some()
        && overrides.llm_answer.is_none()
        && match (input.consult, &prompt_now) {
            (Some(consult), Some(now)) => *now != consult.prompt,
            _ => false,
        };

    let mut warnings = Vec::new();
    if let Some(table) = result.table_outcome_index {
        if table != result.outcome_index && definition.reveal_odds != Some(false) {
            warnings.push(format!(
                "The table would have dealt row {} here; the odds are shown to the model, so a forced row may look out of place.",
                table + 1
            ));
        }
    }
    if overrides.message.is_some()
        && input
            .dealt_outcome_index
            .is_some_and(|dealt| result.outcome_index != dealt)
    {
        warnings.push(
            "The message is pinned, but a different outcome row now wins; check that the words still fit it."
                .to_string(),
        );
    }
    if let Some(roll) = overrides.roll {
        if !within_span(roll, &result.roll_span) {
            warnings.push(format!(
                "The pinned draw lies outside what the table could deal ({}).",
                describe_span(&result.roll_span)
            ));
        }
    }
    if let Some(value) = overrides.value {
        if !within_span(value, &result.value_span) {
            warnings.push(format!(
                "The pinned value lies outside what the modifiers could reach ({}).",
                describe_span(&result.value_span)
            ));
        }
    }
    if stale_answer {
        warnings.push(
            "The consult's answer was given to a different prompt. Re-ask, or deal it as it stands."
                .to_string(),
        );
    }

    let presentation = presentation_for(
        definition,
        result.visibility,
        overrides.title.as_deref(),
        overrides.announcer,
    );
    Ok(RecomputedRun {
        result,
        presentation,
        stale_answer,
        prompt_now,
        clamped_params,
        warnings,
    })
}

/// One effect as the record keeps it: whether it fired, and what it would
/// write (v4 `EffectSummary`).
#[derive(Debug, Clone, PartialEq)]
pub struct EffectSummary {
    pub index: usize,
    pub fired: bool,
    pub target: String,
    /// What it would write; `None` for a skipped one.
    pub next: Option<Value>,
}

impl EffectSummary {
    /// `{index, fired, target, next?}`.
    pub fn to_wire(&self) -> Value {
        let mut m = Map::new();
        m.insert("index".into(), Value::from(self.index));
        m.insert("fired".into(), Value::Bool(self.fired));
        m.insert("target".into(), Value::String(self.target.clone()));
        if let Some(next) = &self.next {
            m.insert("next".into(), next.clone());
        }
        Value::Object(m)
    }
}

/// Summarise resolved effects for the record and the diff (v4
/// `summarizeEffects`).
pub fn summarize_effects(
    definition: &QtapCustomTool,
    effects: Option<&[ResolvedEffect]>,
) -> Vec<EffectSummary> {
    effects
        .unwrap_or_default()
        .iter()
        .map(|effect| {
            let index = effect_index(effect);
            let authored = definition.effects.as_ref().and_then(|e| e.get(index));
            // `authored?.target ?? (wardrobe ? 'wardrobe' : inventory ?
            // 'inventory' : 'log')`.
            let target = match authored.map(|a| &a.body) {
                Some(CustomToolEffectBody::Write { target, .. }) => target.clone(),
                Some(CustomToolEffectBody::Wardrobe(_)) => "wardrobe".into(),
                Some(CustomToolEffectBody::Inventory(_)) => "inventory".into(),
                _ => "log".into(),
            };
            let next = match effect {
                ResolvedEffect::Skipped { .. } => None,
                ResolvedEffect::Applicable { value, .. } => Some(value.to_value()),
                ResolvedEffect::Wardrobe { wardrobe, .. } => Some(Value::Array(
                    wardrobe.items.iter().cloned().map(Value::String).collect(),
                )),
                ResolvedEffect::Inventory { inventory, .. } => Some(Value::Array(
                    inventory.items.iter().cloned().map(Value::String).collect(),
                )),
                ResolvedEffect::Log { log, .. } => Some(Value::String(log.clone())),
            };
            EffectSummary {
                index,
                fired: next.is_some(),
                target,
                next,
            }
        })
        .collect()
}

/// A resolved effect's file index, whatever its arm.
pub fn effect_index(effect: &ResolvedEffect) -> usize {
    match effect {
        ResolvedEffect::Applicable { index, .. }
        | ResolvedEffect::Wardrobe { index, .. }
        | ResolvedEffect::Inventory { index, .. }
        | ResolvedEffect::Log { index, .. }
        | ResolvedEffect::Skipped { index, .. } => *index,
    }
}

/// A run as the record and the diff see it (v4 `RunFacts`).
#[derive(Debug, Clone, PartialEq)]
pub struct RunFacts {
    pub params: ResolvedParams,
    pub raw: f64,
    pub dice_rolls: Option<Vec<i64>>,
    pub value: f64,
    pub outcome_index: usize,
    pub state: OutcomeState,
    pub message: String,
    pub chip_label: Option<String>,
    pub tool_title: String,
    pub visibility: Visibility,
    pub announcer: Announcer,
    /// `{ok, output, prompt}` of the consult in the run.
    pub llm: Option<(bool, String, String)>,
    pub effects: Option<Vec<EffectSummary>>,
}

impl RunFacts {
    /// `{params, raw, diceRolls?, value, outcomeIndex, state, message,
    /// chipLabel?, toolTitle, visibility, announcer, llm?, effects?}` — v4's
    /// `factsOf` literal order (the `dealt` record's bytes).
    pub fn to_wire(&self) -> Value {
        let mut m = Map::new();
        m.insert("params".into(), params_wire(&self.params));
        m.insert("raw".into(), js_number_to_json(self.raw));
        if let Some(d) = &self.dice_rolls {
            m.insert(
                "diceRolls".into(),
                Value::Array(d.iter().map(|f| Value::from(*f)).collect()),
            );
        }
        m.insert("value".into(), js_number_to_json(self.value));
        m.insert("outcomeIndex".into(), Value::from(self.outcome_index));
        m.insert("state".into(), Value::String(self.state.as_str().into()));
        m.insert("message".into(), Value::String(self.message.clone()));
        if let Some(chip) = &self.chip_label {
            m.insert("chipLabel".into(), Value::String(chip.clone()));
        }
        m.insert("toolTitle".into(), Value::String(self.tool_title.clone()));
        m.insert(
            "visibility".into(),
            serde_json::to_value(self.visibility).unwrap_or(Value::Null),
        );
        m.insert(
            "announcer".into(),
            Value::String(self.announcer.as_str().into()),
        );
        if let Some(llm) = &self.llm {
            m.insert("llm".into(), llm_wire(llm));
        }
        if let Some(effects) = &self.effects {
            m.insert(
                "effects".into(),
                Value::Array(effects.iter().map(EffectSummary::to_wire).collect()),
            );
        }
        Value::Object(m)
    }
}

fn llm_wire((ok, output, prompt): &(bool, String, String)) -> Value {
    let mut m = Map::new();
    m.insert("ok".into(), Value::Bool(*ok));
    m.insert("output".into(), Value::String(output.clone()));
    m.insert("prompt".into(), Value::String(prompt.clone()));
    Value::Object(m)
}

/// A resolved parameter bag as v4's object (declaration order).
pub fn params_wire(params: &ResolvedParams) -> Value {
    Value::Object(
        params
            .iter()
            .map(|(k, v)| (k.clone(), v.to_value()))
            .collect(),
    )
}

/// v4 `factsOf`.
pub fn facts_of(
    definition: &QtapCustomTool,
    run: &CustomToolRunResult,
    presentation: &RunPresentation,
) -> RunFacts {
    let effects = summarize_effects(definition, run.effects.as_deref());
    RunFacts {
        params: run.params.clone(),
        raw: run.raw,
        dice_rolls: run.dice_rolls.clone(),
        value: run.value,
        outcome_index: run.outcome_index,
        state: run.state,
        message: run.message.clone(),
        chip_label: run.chip_label.clone(),
        tool_title: presentation.title.clone(),
        visibility: presentation.visibility,
        announcer: presentation.announcer,
        llm: run
            .llm
            .as_ref()
            .map(|c| (c.ok, c.output.clone(), c.prompt.clone())),
        effects: (!effects.is_empty()).then_some(effects),
    }
}

/// v4's `same = (a, b) => JSON.stringify(a) === JSON.stringify(b)` — a BYTE
/// compare, never `Value ==` (which ignores key order).
fn same(a: Option<&Value>, b: Option<&Value>) -> bool {
    a.map(|v| serde_json::to_string(v).unwrap_or_default())
        == b.map(|v| serde_json::to_string(v).unwrap_or_default())
}

/// The field keys that differ between the run as dealt and the run as
/// announced, in v4's order: `params.<name>`, `roll`, `value`, `llmAnswer`,
/// `outcome`, `message`, `state`, `chipLabel`, `title`, `visibility`,
/// `announcer`, `effects.<index>`. Empty means nothing changed (v4
/// `changedFields`).
pub fn changed_fields(dealt: &RunFacts, final_: &RunFacts) -> Vec<String> {
    let mut changed = Vec::new();
    // The UNION of both key sets, first-seen order (`new Set([...a, ...b])`).
    let mut names: Vec<&String> = Vec::new();
    for (name, _) in dealt.params.iter().chain(final_.params.iter()) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let lookup = |params: &ResolvedParams, name: &str| -> Option<Value> {
        params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v): &(String, ResolvedValue)| v.to_value())
    };
    for name in names {
        if !same(
            lookup(&dealt.params, name).as_ref(),
            lookup(&final_.params, name).as_ref(),
        ) {
            changed.push(format!("params.{name}"));
        }
    }
    let dice = |d: &Option<Vec<i64>>| d.as_ref().map(|v| serde_json::json!(v));
    if dealt.raw != final_.raw
        || !same(
            dice(&dealt.dice_rolls).as_ref(),
            dice(&final_.dice_rolls).as_ref(),
        )
    {
        changed.push("roll".into());
    }
    if dealt.value != final_.value {
        changed.push("value".into());
    }
    let answer = |l: &Option<(bool, String, String)>| {
        l.as_ref()
            .map(|(ok, output, _)| serde_json::json!({ "ok": ok, "output": output }))
    };
    if !same(answer(&dealt.llm).as_ref(), answer(&final_.llm).as_ref()) {
        changed.push("llmAnswer".into());
    }
    if dealt.outcome_index != final_.outcome_index {
        changed.push("outcome".into());
    }
    if dealt.message != final_.message {
        changed.push("message".into());
    }
    if dealt.state != final_.state {
        changed.push("state".into());
    }
    if dealt.chip_label != final_.chip_label {
        changed.push("chipLabel".into());
    }
    if dealt.tool_title != final_.tool_title {
        changed.push("title".into());
    }
    if dealt.visibility != final_.visibility {
        changed.push("visibility".into());
    }
    if dealt.announcer != final_.announcer {
        changed.push("announcer".into());
    }
    let finals = final_.effects.as_deref().unwrap_or_default();
    for effect in dealt.effects.as_deref().unwrap_or_default() {
        let after = finals.iter().find(|e| e.index == effect.index);
        let moved = match after {
            None => true,
            Some(after) => {
                after.fired != effect.fired || !same(after.next.as_ref(), effect.next.as_ref())
            }
        };
        if moved {
            changed.push(format!("effects.{}", effect.index));
        }
    }
    changed
}
