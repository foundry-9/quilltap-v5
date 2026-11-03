//! Operator intervention — the wire shapes the hold dialog reads (v4
//! `lib/pascal/intervention/view.ts`, built in v4 by `app/api/v1/chats/[id]/
//! custom-tools/interventions.ts`'s `effectView` / `runView` / `previewOf`).
//!
//! Every view is an ORDERED map in v4's object-literal order, with absent
//! optionals OMITTED (never `null`) — v4 spreads `...(x ? {x} : {})`. The
//! client never computes a run itself: every edit asks the server to
//! recompute.

use serde_json::{Map, Value};

use super::recompute::{params_wire, RecomputedRun, RunPresentation};
use crate::db::js_number_to_json;
use crate::pascal::custom_tool_types::{CustomToolEffectBody, QtapCustomTool};
use crate::pascal::custom_tools::{CustomToolRunResult, ResolvedEffect, RollForm};

/// One declared effect, as this run resolved it (v4 `effectView`): `{index,
/// kind, label, fired, skipped?, value?, items?, log?}`.
pub fn effect_view(definition: &QtapCustomTool, effect: &ResolvedEffect) -> Value {
    let index = super::recompute::effect_index(effect);
    let authored = definition.effects.as_ref().and_then(|e| e.get(index));
    let (kind, label) = match authored.map(|a| &a.body) {
        Some(CustomToolEffectBody::Wardrobe(g)) => {
            ("wardrobe", format!("wardrobe {}", g.action.as_str()))
        }
        Some(CustomToolEffectBody::Inventory(g)) => {
            ("inventory", format!("inventory {}", g.action.as_str()))
        }
        Some(CustomToolEffectBody::Log(_)) => ("log", "log".to_string()),
        Some(CustomToolEffectBody::Write { target, .. }) => ("write", target.clone()),
        // An index past the file (unreachable from the engine): v4's
        // `authored?.target ?? 'write'`.
        None => ("write", "write".to_string()),
    };
    let mut m = Map::new();
    m.insert("index".into(), Value::from(index));
    m.insert("kind".into(), Value::String(kind.into()));
    m.insert("label".into(), Value::String(label));
    match effect {
        ResolvedEffect::Skipped { reason, .. } => {
            m.insert("fired".into(), Value::Bool(false));
            m.insert("skipped".into(), Value::String(reason.clone()));
        }
        ResolvedEffect::Applicable { value, .. } => {
            m.insert("fired".into(), Value::Bool(true));
            m.insert("value".into(), value.to_value());
        }
        ResolvedEffect::Wardrobe { wardrobe, .. } => {
            m.insert("fired".into(), Value::Bool(true));
            m.insert("items".into(), serde_json::json!(wardrobe.items));
        }
        ResolvedEffect::Inventory { inventory, .. } => {
            m.insert("fired".into(), Value::Bool(true));
            m.insert("items".into(), serde_json::json!(inventory.items));
        }
        ResolvedEffect::Log { log, .. } => {
            m.insert("fired".into(), Value::Bool(true));
            m.insert("log".into(), Value::String(log.clone()));
        }
    }
    Value::Object(m)
}

/// A run, as the dialog shows it (v4 `runView` → `InterventionRunView`).
pub fn run_view(
    definition: &QtapCustomTool,
    run: &CustomToolRunResult,
    presentation: &RunPresentation,
) -> Value {
    let mut m = Map::new();
    m.insert("params".into(), params_wire(&run.params));
    m.insert(
        "rollForm".into(),
        Value::String(
            match run.roll_form {
                RollForm::Range => "range",
                RollForm::Dice => "dice",
            }
            .into(),
        ),
    );
    // `...(run.notation ? { notation } : {})` — truthy, so `""` is dropped.
    if let Some(notation) = run.notation.as_ref().filter(|n| !n.is_empty()) {
        m.insert("notation".into(), Value::String(notation.clone()));
    }
    m.insert("raw".into(), js_number_to_json(run.raw));
    if let Some(d) = &run.dice_rolls {
        m.insert("diceRolls".into(), serde_json::json!(d));
    }
    m.insert(
        "diceBreakdown".into(),
        Value::String(run.dice_breakdown.clone()),
    );
    m.insert("rollSpan".into(), run.roll_span.to_wire());
    m.insert("value".into(), js_number_to_json(run.value));
    m.insert("valueSpan".into(), run.value_span.to_wire());
    m.insert("outcomeIndex".into(), Value::from(run.outcome_index));
    if let Some(t) = run.table_outcome_index {
        m.insert("tableOutcomeIndex".into(), Value::from(t));
    }
    m.insert("state".into(), Value::String(run.state.as_str().into()));
    m.insert("message".into(), Value::String(run.message.clone()));
    if let Some(chip) = &run.chip_label {
        m.insert("chipLabel".into(), Value::String(chip.clone()));
    }
    m.insert("title".into(), Value::String(presentation.title.clone()));
    m.insert(
        "visibility".into(),
        serde_json::to_value(presentation.visibility).unwrap_or(Value::Null),
    );
    m.insert(
        "announcer".into(),
        Value::String(presentation.announcer.as_str().into()),
    );
    if let Some(llm) = &run.llm {
        m.insert(
            "llm".into(),
            serde_json::json!({ "ok": llm.ok, "output": llm.output, "prompt": llm.prompt }),
        );
    }
    m.insert(
        "effects".into(),
        Value::Array(
            run.effects
                .iter()
                .flatten()
                .map(|e| effect_view(definition, e))
                .collect(),
        ),
    );
    Value::Object(m)
}

/// The result of a preview, Re-ask or resolve recompute (v4 `previewOf` →
/// `InterventionPreview`): `{run, staleAnswer, promptNow?, clampedParams,
/// warnings}`.
pub fn preview_of(definition: &QtapCustomTool, recomputed: &RecomputedRun) -> Value {
    preview_parts(
        run_view(definition, &recomputed.result, &recomputed.presentation),
        recomputed.stale_answer,
        recomputed.prompt_now.as_deref(),
        &recomputed.clamped_params,
        &recomputed.warnings,
    )
}

/// The preview literal from its parts — shared with the hold view's baseline.
pub fn preview_parts(
    run: Value,
    stale_answer: bool,
    prompt_now: Option<&str>,
    clamped_params: &[String],
    warnings: &[String],
) -> Value {
    let mut m = Map::new();
    m.insert("run".into(), run);
    m.insert("staleAnswer".into(), Value::Bool(stale_answer));
    if let Some(p) = prompt_now {
        m.insert("promptNow".into(), Value::String(p.to_string()));
    }
    m.insert("clampedParams".into(), serde_json::json!(clamped_params));
    m.insert("warnings".into(), serde_json::json!(warnings));
    Value::Object(m)
}
