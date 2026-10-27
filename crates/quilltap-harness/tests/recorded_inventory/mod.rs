//! P4.D329 — the RECORDED-context idiom (the order's R-3), the v5 half.
//!
//! The oracle wraps v4's REAL `inventoryContextFrom` context in a recording
//! proxy (`harness/oracle/lib/recorded-inventory.ts`) and emits every call's
//! argument and answer. This module implements C2's `InventoryContext` trait
//! by LOOKUP over that table — so a row pins the deal, the render and the gate
//! without re-implementing `resolveInventoryRef` (P4.D328's family pins that),
//! and stays valid on the union because every answer is v4's.
//!
//! A MISS panics with the method and the argument: a render or a trim
//! difference between the two sides surfaces as a named miss, never as a
//! silent fallback.
//!
//! A row's `inventory` field is one of: absent (v4 `undefined`), `null`,
//! `{ "rows": [...] }` (the bench's mock bag — v5's `mock_inventory_context`
//! over v4's own rows), or `{ "recorded": { characterId, chatId, calls } }`.

#![allow(dead_code)]

use quilltap_core::inventory::types::ResolvedInventoryItem;
use quilltap_core::pascal::bench_inventory::{
    mock_inventory_context, parse_mock_inventory, MockInventoryContext,
};
use std::collections::HashSet;
use std::sync::OnceLock;

use quilltap_core::inventory::refs::{
    AggregateObject, AggregateScope, InventoryContext, InventoryState, ResolvedInventoryPool,
    AGGREGATE_SCOPES,
};
use serde_json::{Map, Value};

/// A context answered from v4's recorded calls.
pub struct RecordedInventoryContext {
    character_id: String,
    chat_id: Option<String>,
    calls: Vec<(String, String, Value)>,
    /// Each scope's recorded answer, looked up on first ask (a scope v4 never
    /// read stays a MISS until v5 asks for it).
    scopes: [OnceLock<Vec<AggregateObject>>; 3],
}

impl RecordedInventoryContext {
    pub fn from_value(v: &Value) -> Self {
        let calls = v["calls"]
            .as_array()
            .expect("recorded.calls")
            .iter()
            .map(|c| {
                (
                    c["method"].as_str().expect("method").to_string(),
                    c["arg"].as_str().expect("arg").to_string(),
                    c["answer"].clone(),
                )
            })
            .collect();
        RecordedInventoryContext {
            character_id: v["characterId"].as_str().unwrap_or_default().to_string(),
            chat_id: v["chatId"].as_str().map(str::to_string),
            calls,
            scopes: Default::default(),
        }
    }

    fn answer(&self, method: &str, arg: &str) -> &Value {
        self.calls
            .iter()
            .find(|(m, a, _)| m == method && a == arg)
            .map(|(_, _, v)| v)
            .unwrap_or_else(|| {
                panic!("recorded inventory MISS: v5 asked {method}({arg:?}), v4 never did")
            })
    }
}

fn state_of(word: &str) -> InventoryState {
    match word {
        "equipped" => InventoryState::Equipped,
        "carried" => InventoryState::Carried,
        "reachable" => InventoryState::Reachable,
        "absent" => InventoryState::Absent,
        other => panic!("not a state: {other}"),
    }
}

impl InventoryContext for RecordedInventoryContext {
    fn character_id(&self) -> &str {
        &self.character_id
    }
    fn chat_id(&self) -> Option<&str> {
        self.chat_id.as_deref()
    }
    fn state(&self, reference: &str) -> InventoryState {
        state_of(self.answer("state", reference).as_str().expect("a state"))
    }
    fn equipped_kin_count(&self, reference: &str) -> usize {
        self.answer("equippedKinCount", reference)
            .as_u64()
            .expect("a count") as usize
    }
    fn item(&self, reference: &str) -> Option<ResolvedInventoryItem> {
        match self.answer("item", reference) {
            Value::Object(m) => Some(ResolvedInventoryItem(m.clone())),
            _ => None,
        }
    }
    fn scope_objects(&self, scope: AggregateScope) -> &[AggregateObject] {
        let at = AGGREGATE_SCOPES.iter().position(|s| *s == scope).unwrap();
        self.scopes[at].get_or_init(|| {
            self.answer("scopeObjects", scope.as_str())
                .as_array()
                .expect("objects")
                .iter()
                .map(|o| AggregateObject {
                    name: o["name"].as_str().unwrap_or_default().to_string(),
                    title: o["title"].as_str().unwrap_or_default().to_string(),
                    metadata: o["metadata"].as_object().cloned().unwrap_or_else(Map::new),
                })
                .collect()
        })
    }
    fn pool(&self) -> Option<&ResolvedInventoryPool> {
        None
    }
    fn equipped_ids(&self) -> Option<&HashSet<String>> {
        None
    }
}

/// A row's bag, as the oracle described it.
pub enum RowBag {
    /// v4 `undefined`.
    Absent,
    /// v4 `null`.
    Null,
    Mock(MockInventoryContext),
    Recorded(RecordedInventoryContext),
}

impl RowBag {
    pub fn from_row(v: Option<&Value>) -> Self {
        match v {
            None => RowBag::Absent,
            Some(Value::Null) => RowBag::Null,
            Some(Value::Object(o)) if o.contains_key("rows") => {
                let rows = parse_mock_inventory(o.get("rows"))
                    .unwrap_or_else(|e| panic!("bad mock rows: {e:?}"))
                    .unwrap_or_default();
                RowBag::Mock(mock_inventory_context(&rows))
            }
            Some(Value::Object(o)) if o.contains_key("recorded") => {
                RowBag::Recorded(RecordedInventoryContext::from_value(&o["recorded"]))
            }
            Some(other) => panic!("not a row bag: {other}"),
        }
    }

    /// The context as the core takes it (`None` for absent and `null` alike).
    pub fn as_dyn(&self) -> Option<&dyn InventoryContext> {
        match self {
            RowBag::Absent | RowBag::Null => None,
            RowBag::Mock(m) => Some(m),
            RowBag::Recorded(r) => Some(r),
        }
    }
}

/// v4's logger call rendered as the capture rig renders a tracing line: the
/// level, the target, the message, then each NON-null field as `k=v` (strings
/// bare, numbers and booleans as JSON, arrays / objects as JSON). A `null`
/// field is absent on the v5 side (`tracing` records no `None`), so it is
/// skipped here.
pub fn v4_line(level: &str, target: &str, message: &str, bag: &Value) -> String {
    let mut line = format!("{level} {target} {message}");
    for (k, v) in bag.as_object().into_iter().flatten() {
        match v {
            Value::Null => {}
            Value::String(s) => line.push_str(&format!(" {k}={s}")),
            other => line.push_str(&format!(" {k}={other}")),
        }
    }
    line
}

/// A captured v5 line with its target dropped (`"<LEVEL> <target> rest"` →
/// `"<LEVEL>  rest"`), to compare against [`v4_line`] rendered with an empty
/// target — v4's `context` field carries the module identity instead.
pub fn without_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _target = parts.next();
    format!("{level}  {}", parts.next().unwrap_or_default())
}

/// The v5 lines a P4.D329 row compares: the inventory subject / render / roll
/// lines this lane owns.
pub fn is_inventory_line(line: &str) -> bool {
    [
        "Custom tool inventory",
        "Custom tool message references an inventory",
        "Custom tool $inventory roll fell back",
    ]
    .iter()
    .any(|m| line.contains(m))
}
