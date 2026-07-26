//! The availability gate — whether a tool is dealt to this invoker at all (v4
//! `lib/pascal/tool-gate.ts`, `6864bf0e`).
//!
//! An outcome table decides what happens once a tool is run. A gate decides
//! something earlier and coarser: whether the tool appears on the roster in the
//! first place. A definition that declares `availableWhen` is offered only to an
//! invoker whose fact sheet satisfies it; one that declares `withheldWhen` is
//! kept from an invoker whose sheet satisfies THAT. A definition declaring
//! neither is offered to everyone, which is every file written before this key
//! existed.
//!
//! # Why metadata alone
//!
//! The gate is answered before the deal. There is no roll to test, no resolved
//! parameters (nobody has called anything yet), and no consult. What a character
//! carries into the room is their `metadata.json`, so that is the only subject a
//! pre-roll test can honestly have — and the reason a gate's operands are
//! literals rather than `$param`/`$state` references.
//!
//! # Fail-soft, and therefore fail-CLOSED for `availableWhen`
//!
//! Metadata tests never fail: an absent key, a list, a value of the wrong type
//! all simply fail to match (see
//! [`metadata_comparator_holds`](super::metadata_match::metadata_comparator_holds)).
//! Read through the gate, that means a character with no sheet at all fails
//! every `availableWhen` — the tool is withheld — and satisfies no
//! `withheldWhen` — the tool is offered. Both are the safe reading of "we could
//! not establish that this character qualifies". That asymmetry is the whole
//! reason both keys exist; neither is a negation of the other.
//!
//! CLIENT-SAFE in v4's sense: pure and total.

use std::convert::Infallible;

use serde::Serialize;
use serde_json::{Map, Value};

use super::custom_tool_types::{QtapCustomTool, ToolGate};
use super::metadata_match::{metadata_comparator_holds, ResolvedValue};

/// What a gate decided, and which clause decided it (v4 `ToolGateVerdict`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ToolGateVerdict {
    /// Whether this invoker may be offered the tool.
    pub available: bool,
    /// The clause that withheld it. **Absent** — not null — whenever the tool is
    /// available, because v4's is an optional key.
    #[serde(rename = "withheldBy", skip_serializing_if = "Option::is_none")]
    pub withheld_by: Option<WithheldBy>,
}

/// Which clause withheld a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WithheldBy {
    #[serde(rename = "availableWhen")]
    AvailableWhen,
    #[serde(rename = "withheldWhen")]
    WithheldWhen,
}

impl WithheldBy {
    /// The clause's own key name, for a log line or a wire field.
    pub fn as_str(self) -> &'static str {
        match self {
            WithheldBy::AvailableWhen => "availableWhen",
            WithheldBy::WithheldWhen => "withheldWhen",
        }
    }
}

/// A definition carries a gate when it declares either clause (v4
/// `hasToolGate`).
pub fn has_tool_gate(definition: &QtapCustomTool) -> bool {
    definition.available_when.is_some() || definition.withheld_when.is_some()
}

/// Evaluate a definition's gate against one invoker's fact sheet (v4
/// `evaluateToolGate`).
///
/// Total: an ungated definition is available, and no input can make this fail.
/// `None` is v4's `metadata ?? {}` — an absent sheet reads as an empty one.
pub fn evaluate_tool_gate(
    definition: &QtapCustomTool,
    metadata: Option<&Map<String, Value>>,
) -> ToolGateVerdict {
    let empty = Map::new();
    let sheet = metadata.unwrap_or(&empty);

    if let Some(gate) = definition.available_when.as_ref() {
        if !gate_holds(gate, sheet) {
            return ToolGateVerdict {
                available: false,
                withheld_by: Some(WithheldBy::AvailableWhen),
            };
        }
    }

    if let Some(gate) = definition.withheld_when.as_ref() {
        if gate_holds(gate, sheet) {
            return ToolGateVerdict {
                available: false,
                withheld_by: Some(WithheldBy::WithheldWhen),
            };
        }
    }

    ToolGateVerdict {
        available: true,
        withheld_by: None,
    }
}

/// Every test in a gate must hold. Keys AND together, as they do in a `when`.
pub fn gate_holds(gate: &ToolGate, metadata: &Map<String, Value>) -> bool {
    for (key, comparator) in &gate.metadata {
        let holds = metadata_comparator_holds::<Infallible>(
            comparator,
            key,
            metadata,
            // A gate's operands are literals by construction — the schema admits
            // no `$param` or `$state` here — so resolution is the identity, and
            // cannot fail.
            &mut |comparator_key| Ok(literal_operand(comparator, comparator_key)),
            &mut |_| {},
        );
        match holds {
            Ok(true) => continue,
            Ok(false) => return false,
            // `Infallible` — the resolver above has no error arm to take.
            Err(never) => match never {},
        }
    }
    true
}

/// The literal a gate comparator holds at `key`. Total by construction: the
/// shared table only asks about keys it found present, and
/// `parse_gate_comparator` admits nothing but literals at any of them.
fn literal_operand(
    comparator: &super::custom_tool_types::GateComparator,
    key: &str,
) -> ResolvedValue {
    use super::custom_tool_types::{AnyOperand, NumberOrParamRef, StringOperand};

    let number = |v: Option<&NumberOrParamRef>| match v {
        Some(NumberOrParamRef::Number(n)) => ResolvedValue::Number(*n),
        _ => unreachable!("a gate comparator's ordering operand is a literal number"),
    };
    let any = |v: Option<&AnyOperand>| match v {
        Some(AnyOperand::Number(n)) => ResolvedValue::Number(*n),
        Some(AnyOperand::String(s)) => ResolvedValue::String(s.clone()),
        Some(AnyOperand::Bool(b)) => ResolvedValue::Bool(*b),
        _ => unreachable!("a gate comparator's eq/neq operand is a literal"),
    };
    let string = |v: Option<&StringOperand>| match v {
        Some(StringOperand::String(s)) => ResolvedValue::String(s.clone()),
        _ => unreachable!("a gate comparator's containment operand is a literal string"),
    };

    match key {
        "gt" => number(comparator.gt.as_ref()),
        "gte" => number(comparator.gte.as_ref()),
        "lt" => number(comparator.lt.as_ref()),
        "lte" => number(comparator.lte.as_ref()),
        "eq" => any(comparator.eq.as_ref()),
        "neq" => any(comparator.neq.as_ref()),
        "contains" => string(comparator.contains.as_ref()),
        "ncontains" => string(comparator.ncontains.as_ref()),
        other => unreachable!("not a comparator key: {other}"),
    }
}
