//! Pascal's Inventory subjects — the read side of the bag (v4
//! `lib/pascal/inventory-subjects.ts`, `e1872c94f`; spec
//! inventory-tools-and-pascal §5.3–5.5), stated once for every reader.
//!
//! v4's header rides verbatim, because it records the doctrine:
//!
//! > - The `inventory` subject asks an object's state (`equipped` / `carried` /
//! >   `reachable` / `absent`) with `eq` / `neq`; `inventoryCount` asks how many
//! >   kin are equipped, with the ordering ladder.
//! > - The `$inventory` operand reads one `metadata` key of one object, typed by
//! >   its fallback.
//! >
//! > Fail-soft like `metadata`, and in the same direction: with no context (a
//! > run nobody made, or a vault that could not be read) every comparator
//! > DECLINES — including `eq: "absent"`, `neq`, and `lt: 1` — exactly as
//! > `metadataComparatorHolds` declines on a missing key. Only a real context
//! > ever yields `absent` or `0`.
//! >
//! > Every reference resolves through the context, which resolves through
//! > `resolveInventoryRef` (`lib/inventory/refs.ts`).
//!
//! Pure, no logging: the Workbench's twin runs the same rules against a mock
//! bag.

use serde_json::Value;

use super::custom_tool_types::{InventoryRef, StateRefFallback, ToolGate};
use super::metadata_match::ResolvedValue;
// === LANE-REMOVE (P4.D329): C2 import ===
use super::p4d329_c2_standins::{InventoryContext, InventoryState};
use super::placeholders::{split_inventory_reference, InventoryPlaceholderField};

/// v4 `INVENTORY_STATE_VALUES` — the four states, as the schema's enum spells
/// them.
pub const INVENTORY_STATE_VALUES: [&str; 4] = ["equipped", "carried", "reachable", "absent"];

/// v4 `parseInventoryOperand(path)` — the parts of a `$inventory` operand:
/// `<ref>.metadata.<key>`, split at the first `.metadata.`.
pub fn parse_inventory_operand(path: &str) -> Option<(String, String)> {
    match split_inventory_reference(path)? {
        (reference, InventoryPlaceholderField::Metadata, Some(key)) if !key.is_empty() => {
            Some((reference, key))
        }
        _ => None,
    }
}

/// The fallback as a resolved primitive.
pub fn fallback_value(fallback: &StateRefFallback) -> ResolvedValue {
    match fallback {
        StateRefFallback::Number(n) => ResolvedValue::Number(*n),
        StateRefFallback::String(s) => ResolvedValue::String(s.clone()),
        StateRefFallback::Bool(b) => ResolvedValue::Bool(*b),
    }
}

/// v4 `resolveInventoryValue(operand, ctx)`. Total: the object's metadata value
/// only when it exists and its `typeof` matches the fallback's, else the
/// fallback. A null context, an unparseable path or an absent object all fall
/// back. A number must be finite (`-0` is a number and is kept).
pub fn resolve_inventory_value(
    operand: &InventoryRef,
    ctx: Option<&dyn InventoryContext>,
) -> ResolvedValue {
    let fallback = || fallback_value(&operand.fallback);
    let Some(ctx) = ctx else {
        return fallback();
    };
    let Some((reference, key)) = parse_inventory_operand(&operand.inventory) else {
        return fallback();
    };
    let Some(item) = ctx.item(&reference) else {
        return fallback();
    };
    // `typeof value !== typeof operand.fallback` — an array, an object or a
    // null is `object` and never matches a primitive fallback.
    match (item.metadata().get(&key), &operand.fallback) {
        (Some(Value::Number(n)), StateRefFallback::Number(_)) => match n.as_f64() {
            Some(f) if f.is_finite() => ResolvedValue::Number(f),
            _ => fallback(),
        },
        (Some(Value::String(s)), StateRefFallback::String(_)) => ResolvedValue::String(s.clone()),
        (Some(Value::Bool(b)), StateRefFallback::Bool(_)) => ResolvedValue::Bool(*b),
        _ => fallback(),
    }
}

/// v4 `ResolvedInventoryStateTest` — an `inventory` state comparator with its
/// operands already resolved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedInventoryStateTest {
    pub eq: Option<String>,
    pub neq: Option<String>,
}

/// v4 `inventoryStateHolds(test, ref, ctx, onDecline?)` — whether a state test
/// holds for `reference`. Null context → declines (a mismatch is a plain
/// `false`, never a decline).
pub fn inventory_state_holds(
    test: &ResolvedInventoryStateTest,
    reference: &str,
    ctx: Option<&dyn InventoryContext>,
    on_decline: Option<&mut dyn FnMut(&str)>,
) -> bool {
    let Some(ctx) = ctx else {
        if let Some(decline) = on_decline {
            decline(
                "no inventory to read (nobody in particular rolled, or the vault could not be read)",
            );
        }
        return false;
    };
    let state: InventoryState = ctx.state(reference);
    if let Some(eq) = &test.eq {
        if state.as_str() != eq {
            return false;
        }
    }
    if let Some(neq) = &test.neq {
        if state.as_str() == neq {
            return false;
        }
    }
    true
}

/// v4 `ResolvedInventoryCountTest` — an `inventoryCount` comparator with its
/// operands already resolved to numbers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedInventoryCountTest {
    pub gt: Option<f64>,
    pub gte: Option<f64>,
    pub lt: Option<f64>,
    pub lte: Option<f64>,
    pub eq: Option<f64>,
    pub neq: Option<f64>,
}

/// v4 `inventoryCountHolds(test, ref, ctx, onDecline?)` — the four orderings
/// through `compareOrdered`, then `eq`, then `neq`. Null context → declines.
pub fn inventory_count_holds(
    test: &ResolvedInventoryCountTest,
    reference: &str,
    ctx: Option<&dyn InventoryContext>,
    on_decline: Option<&mut dyn FnMut(&str)>,
) -> bool {
    let Some(ctx) = ctx else {
        if let Some(decline) = on_decline {
            decline(
                "no inventory to count (nobody in particular rolled, or the vault could not be read)",
            );
        }
        return false;
    };
    let count = ctx.equipped_kin_count(reference);
    for (key, operand) in [
        ("gt", test.gt),
        ("gte", test.gte),
        ("lt", test.lt),
        ("lte", test.lte),
    ] {
        if let Some(operand) = operand {
            if !compare_ordered(key, count, operand) {
                return false;
            }
        }
    }
    if test.eq.is_some_and(|eq| count != eq) {
        return false;
    }
    if test.neq.is_some_and(|neq| count == neq) {
        return false;
    }
    true
}

/// v4 `compareOrdered(key, a, b)` (`metadata-match.ts`) — JS's relational
/// operators on two numbers.
fn compare_ordered(key: &str, a: f64, b: f64) -> bool {
    match key {
        "gt" => a > b,
        "gte" => a >= b,
        "lt" => a < b,
        _ => a <= b,
    }
}

/// v4 `gateNamesInventory(gate)` — true when a gate clause names an Inventory
/// subject (`inventory`, `inventoryCount` or `inventoryAggregate`).
///
/// Truthiness on KEY COUNT (`Object.keys(group.inventory ?? {}).length > 0`):
/// an authored-but-empty record does NOT name it — unlike
/// `gate_names_progress`, which reads record presence. v4 walks every group of
/// an any-of gate; on this branch a gate is ONE group (P4.D333's
/// `gate_groups()` wraps this read at the union).
pub fn gate_names_inventory(gate: Option<&ToolGate>) -> bool {
    let Some(group) = gate else {
        return false;
    };
    group.inventory.as_ref().is_some_and(|r| !r.is_empty())
        || group
            .inventory_count
            .as_ref()
            .is_some_and(|r| !r.is_empty())
        || group
            .inventory_aggregate
            .as_ref()
            .is_some_and(|r| !r.is_empty())
}
