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
//! # Why metadata and progress alone
//!
//! The gate is answered before the deal. There is no roll to test, no resolved
//! parameters (nobody has called anything yet), and no consult. What a character
//! carries into the room is their `metadata.json` — and, derived from the
//! `progressions` key inside it against the wall clock, their timed
//! progressions. Those two are the only subjects a pre-roll test can honestly
//! have, and the reason a gate's operands are literals rather than
//! `$param`/`$state` references.
//!
//! The progress sheet is what makes `availableWhen: { progress: {
//! "cannon.complete": { eq: true } } }` — the whole weapon-recharge use case —
//! a gate rather than a branch in every outcome table. The roster already holds
//! the invoker's metadata, so deriving the sheet from it adds no reads.
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
//! # Any-of
//!
//! A clause is one group (every test must hold) or an array of groups (any
//! group may hold — v4 `a53024d3c`). AND binds tighter than OR; groups are
//! tried left to right and the first to hold decides, and the verdict names it
//! as `heldGroup`. An empty context declines every group, so the fail-closed /
//! fail-open reading above holds for an array unchanged.
//!
//! CLIENT-SAFE in v4's sense: pure and total.

use std::convert::Infallible;

use serde::Serialize;
use serde_json::{Map, Value};

use super::custom_tool_types::{QtapCustomTool, ToolGate, ToolGateGroup};
use super::metadata_match::{metadata_comparator_holds, ResolvedValue};
// === P4.D329: the gate's inventory subjects ===
use super::custom_tool_types::{InventoryStateOperand, NumberOrParamRef, NumericComparator};
use super::inventory_aggregate::inventory_aggregate_holds;
use super::inventory_subjects::{
    inventory_count_holds, inventory_state_holds, ResolvedInventoryCountTest,
    ResolvedInventoryStateTest,
};
// === LANE-REMOVE (P4.D329): C2 import ===
use super::p4d329_c2_standins::InventoryContext;

/// What a gate decided, and which clause decided it (v4 `ToolGateVerdict`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ToolGateVerdict {
    /// Whether this invoker may be offered the tool.
    pub available: bool,
    /// The clause that withheld it. **Absent** — not null — whenever the tool is
    /// available, because v4's is an optional key.
    #[serde(rename = "withheldBy", skip_serializing_if = "Option::is_none")]
    pub withheld_by: Option<WithheldBy>,
    /// For an any-of clause (an array), the index of the group that held — the
    /// one that opened an `availableWhen` or closed a `withheldWhen` (v4
    /// `a53024d3c`). Absent when no group held, and for a bare-object clause,
    /// where there is only the one. Declared LAST: the key order `available,
    /// withheldBy, heldGroup` is the gate rows' byte contract.
    #[serde(rename = "heldGroup", skip_serializing_if = "Option::is_none")]
    pub held_group: Option<usize>,
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
    // The invoker's derived progress sheet (`flatten_progressions`), keyed
    // `"<id>.<field>"`. `None` → `{}`, so every `progress` test fails and the
    // fail-closed / fail-open reading applies to it unchanged.
    progress: Option<&Map<String, Value>>,
    // === P4.D329: the 4th positional ===
    // The invoker's Inventory context for this chat. `None` → every
    // `inventory` / `inventoryCount` / `inventoryAggregate` test declines.
    inventory: Option<&dyn InventoryContext>,
) -> ToolGateVerdict {
    let empty = Map::new();
    let sheet = metadata.unwrap_or(&empty);
    let progress_sheet = progress.unwrap_or(&empty);

    let mut opened = None;
    if let Some(clause) = definition.available_when.as_ref() {
        let Some(held) = held_group_of(clause, sheet, progress_sheet, inventory) else {
            return ToolGateVerdict {
                available: false,
                withheld_by: Some(WithheldBy::AvailableWhen),
                held_group: None,
            };
        };
        if clause.is_any_of() {
            opened = Some(held);
        }
    }

    if let Some(clause) = definition.withheld_when.as_ref() {
        if let Some(held) = held_group_of(clause, sheet, progress_sheet, inventory) {
            return ToolGateVerdict {
                available: false,
                withheld_by: Some(WithheldBy::WithheldWhen),
                held_group: clause.is_any_of().then_some(held),
            };
        }
    }

    ToolGateVerdict {
        available: true,
        withheld_by: None,
        held_group: opened,
    }
}

/// The first group of a clause that holds, left to right; `None` when none
/// does (v4 `heldGroupOf`).
fn held_group_of(
    clause: &ToolGate,
    metadata: &Map<String, Value>,
    progress: &Map<String, Value>,
    inventory: Option<&dyn InventoryContext>,
) -> Option<usize> {
    clause
        .gate_groups()
        .iter()
        .position(|group| gate_holds(group, metadata, progress, inventory))
}

/// True when a gate clause tests the progress sheet in any group — the roster
/// derives the sheet only then (v4 `gateNamesProgress`, `a53024d3c`).
///
/// A record that is present but EMPTY names nothing: v4 counts its keys
/// (`Object.keys(group.progress ?? {}).length > 0`), where the pre-any-of
/// roster tested the record's truthiness and so derived the sheet for an
/// authored `progress: {}` too.
pub fn gate_names_progress(gate: Option<&ToolGate>) -> bool {
    gate.is_some_and(|gate| {
        gate.gate_groups()
            .iter()
            .any(|group| group.progress.as_ref().is_some_and(|p| !p.is_empty()))
    })
}

/// Every test in a gate group must hold. Keys AND together, as they do in a
/// `when`.
///
/// Both subjects go through the SAME comparison table — one semantics, no second
/// implementation to drift — differing only in which sheet they read.
pub fn gate_holds(
    gate: &ToolGateGroup,
    metadata: &Map<String, Value>,
    progress: &Map<String, Value>,
    inventory: Option<&dyn InventoryContext>,
) -> bool {
    for (sheet, tests) in [(metadata, &gate.metadata), (progress, &gate.progress)] {
        // v4: `Object.entries(gate.metadata ?? {})` — an absent record and an
        // empty one read alike here, which is why the parsed shape keeps them
        // apart and only the reader flattens.
        for (key, comparator) in tests.iter().flatten() {
            let holds = metadata_comparator_holds::<Infallible>(
                comparator,
                key,
                sheet,
                // A gate's operands are literals by construction — the schema
                // admits no `$param` or `$state` here — so resolution is the
                // identity, and cannot fail.
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
    }

    // === P4.D329: the inventory loops ===
    // Literal operands, literal keys: the comparison tables in
    // `inventory_subjects` / `inventory_aggregate`, against the invoker's bag.
    // No decline hook — the gate logs nothing.
    for (reference, comparator) in gate.inventory.iter().flatten() {
        let literal = |o: &Option<InventoryStateOperand>| match o {
            Some(InventoryStateOperand::State(s)) => Some(s.as_str().to_string()),
            Some(InventoryStateOperand::ParamRef(_)) => {
                unreachable!("a gate's inventory operand is a literal state")
            }
            None => None,
        };
        let test = ResolvedInventoryStateTest {
            eq: literal(&comparator.eq),
            neq: literal(&comparator.neq),
        };
        if !inventory_state_holds(&test, reference, inventory, None) {
            return false;
        }
    }
    for (reference, comparator) in gate.inventory_count.iter().flatten() {
        if !inventory_count_holds(&literal_count_test(comparator), reference, inventory, None) {
            return false;
        }
    }
    for (key, comparator) in gate.inventory_aggregate.iter().flatten() {
        let holds = inventory_aggregate_holds::<Infallible>(
            comparator,
            key,
            inventory,
            &mut |comparator_key| Ok(literal_operand(comparator, comparator_key)),
            &mut |_| {},
        );
        match holds {
            Ok(true) => continue,
            Ok(false) => return false,
            Err(never) => match never {},
        }
    }
    true
}

/// A gate's `inventoryCount` comparator as the resolved test — its six
/// operands are literal numbers by construction.
fn literal_count_test(comparator: &NumericComparator) -> ResolvedInventoryCountTest {
    let number = |v: &Option<NumberOrParamRef>| match v {
        Some(NumberOrParamRef::Number(n)) => Some(*n),
        Some(_) => unreachable!("a gate's inventoryCount operand is a literal number"),
        None => None,
    };
    ResolvedInventoryCountTest {
        gt: number(&comparator.gt),
        gte: number(&comparator.gte),
        lt: number(&comparator.lt),
        lte: number(&comparator.lte),
        eq: number(&comparator.eq),
        neq: number(&comparator.neq),
    }
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
