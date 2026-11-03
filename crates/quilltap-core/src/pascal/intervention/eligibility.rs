//! Operator intervention — which parts of a run the hold dialog may show (v4
//! `lib/pascal/intervention/eligibility.ts`).
//!
//! One reading of the author's flags, shared by the server (which refuses an
//! override naming a hidden field) and the dialog (which never renders one).
//! The rule (spec I6): when the tool is intervenable, every eligible field is
//! shown unless its flag says `false`. Parameters and effects carry their own
//! `intervene` flag; the parts of a run that are not objects in the file (the
//! roll, the value, the outcome choice…) are flagged in `intervention.fields`.

use crate::pascal::custom_tool_types::{QtapCustomTool, INTERVENTION_FIELD_KEYS};
use crate::pascal::custom_tools::RollForm;

/// Whether a definition asks for its runs to be held. Flags on a tool without
/// this are inert (v4 `isIntervenable` — `intervention?.enabled === true`).
pub fn is_intervenable(definition: &QtapCustomTool) -> bool {
    definition.intervention.as_ref().is_some_and(|i| i.enabled)
}

/// What the dialog may show, and what an override may name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterventionEligibility {
    /// Tool-level fields, after the author's flags and the run's shape, in
    /// [`INTERVENTION_FIELD_KEYS`] order.
    pub fields: Vec<&'static str>,
    /// Parameter names the operator may set, in declaration order.
    pub params: Vec<String>,
    /// Effect indices (file order) the operator may switch or edit.
    pub effects: Vec<usize>,
}

impl InterventionEligibility {
    /// `{fields, params, effects}` — v4's key order.
    pub fn to_wire(&self) -> serde_json::Value {
        serde_json::json!({
            "fields": self.fields,
            "params": self.params,
            "effects": self.effects,
        })
    }
}

/// The eligible set for one run of `definition`. `roll_form` is the form the
/// run was actually dealt in — a `$inventory` roll resolves to dice at deal
/// time, so the definition alone cannot say whether `value` is separately
/// editable.
///
/// Shape rules, beside the author's flags: `value` exists only for the range
/// form (a dice total is the faces plus the modifier, and editing it alone
/// would make `{{dice}}` lie); `llmPrompt` / `llmAnswer` only when the tool
/// consults; `chipLabel` only when the tool declares one.
pub fn intervention_eligibility(
    definition: &QtapCustomTool,
    roll_form: RollForm,
) -> InterventionEligibility {
    let flags = definition
        .intervention
        .as_ref()
        .and_then(|i| i.fields.as_ref());
    let shown = |key: &str| flags.and_then(|f| f.get(key)) != Some(false);
    let applicable = |key: &str| match key {
        "value" => roll_form == RollForm::Range,
        "llmPrompt" | "llmAnswer" => definition.llm.is_some(),
        "chipLabel" => definition.chip_label.is_some(),
        _ => true,
    };
    let fields = INTERVENTION_FIELD_KEYS
        .iter()
        .copied()
        .filter(|key| applicable(key) && shown(key))
        .collect();
    let params = definition
        .parameters
        .iter()
        .flatten()
        .filter(|(_, spec)| spec.intervene != Some(false))
        .map(|(name, _)| name.clone())
        .collect();
    let effects = definition
        .effects
        .iter()
        .flatten()
        .enumerate()
        .filter(|(_, effect)| effect.intervene != Some(false))
        .map(|(index, _)| index)
        .collect();
    InterventionEligibility {
        fields,
        params,
        effects,
    }
}
