//! The Proving Bench's mock bag (v4 `lib/pascal/bench-inventory.ts`,
//! `e1872c94f` + `a53024d3c`'s `metadata`).
//!
//! The bench has no chat and often no character, so it cannot read a real
//! inventory. Instead the author lists what the roller carries — `{ ref,
//! state, count?, metadata? }` rows — and the bench answers the `inventory`,
//! `inventoryCount` and `inventoryAggregate` subjects (and the gate) from them.
//!
//! `item()` is ALWAYS absent here, exactly as at the pin (R-U): a mock row
//! names a state, not an object, so `{{inventory.*}}` renders as written and a
//! `$inventory` operand falls back on the bench. A v4 filing candidate, ported
//! as is rather than enriched.

use serde_json::{json, Map, Value};

use crate::api::zod_issues::ZodIssue;
// === LANE-REMOVE (P4.D329): C2 import ===
use super::inventory_subjects::INVENTORY_STATE_VALUES;
use super::p4d329_c2_standins::{
    AggregateObject, AggregateScope, InventoryContext, InventoryState,
};
use crate::inventory::types::ResolvedInventoryItem;

/// v4 `MockInventoryRowSchema.ref`'s `.max(200)`.
const MAX_MOCK_REF_LENGTH: usize = 200;
/// v4 `MockInventorySchema`'s `.max(64)`.
pub const MAX_MOCK_INVENTORY_ROWS: usize = 64;
/// v4 `count: z.number().int().min(0).max(999)`.
const MAX_MOCK_COUNT: i64 = 999;

const ROW_KEYS: [&str; 4] = ["ref", "state", "count", "metadata"];

/// One mock row, as the bench sends it (v4 `MockInventoryRow`).
#[derive(Debug, Clone, PartialEq)]
pub struct MockInventoryRow {
    pub reference: String,
    pub state: InventoryState,
    pub count: Option<f64>,
    pub metadata: Option<Map<String, Value>>,
}

fn state_of(word: &str) -> Option<InventoryState> {
    match word {
        "equipped" => Some(InventoryState::Equipped),
        "carried" => Some(InventoryState::Carried),
        "reachable" => Some(InventoryState::Reachable),
        "absent" => Some(InventoryState::Absent),
        _ => None,
    }
}

fn at(path: &[Value], seg: Value) -> Vec<Value> {
    let mut p = path.to_vec();
    p.push(seg);
    p
}

/// v4 `MockPrimitiveSchema` — `z.union([z.number().finite(), z.string(),
/// z.boolean()])`. `None` when it parses; else the union's issue at `path`,
/// one branch per option.
fn primitive_issue(v: &Value, path: Vec<Value>) -> Option<ZodIssue> {
    match v {
        Value::Number(_) | Value::String(_) | Value::Bool(_) => None,
        other => Some(ZodIssue::invalid_union(
            vec![
                vec![ZodIssue::invalid_type("number", vec![], Some(other))],
                vec![ZodIssue::invalid_type("string", vec![], Some(other))],
                vec![ZodIssue::invalid_type("boolean", vec![], Some(other))],
            ],
            path,
        )),
    }
}

/// One metadata value: `z.union([MockPrimitiveSchema, z.array(MockPrimitiveSchema)])`.
fn metadata_value_issues(v: &Value, path: Vec<Value>) -> Vec<ZodIssue> {
    let Some(primitive) = primitive_issue(v, vec![]) else {
        return Vec::new();
    };
    let array: Vec<ZodIssue> = match v {
        Value::Array(entries) => entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| primitive_issue(entry, vec![json!(i)]))
            .collect(),
        other => vec![ZodIssue::invalid_type("array", vec![], Some(other))],
    };
    if array.is_empty() {
        return Vec::new();
    }
    vec![ZodIssue::invalid_union(vec![vec![primitive], array], path)]
}

fn parse_row(v: &Value, path: Vec<Value>, issues: &mut Vec<ZodIssue>) -> Option<MockInventoryRow> {
    let Value::Object(obj) = v else {
        issues.push(ZodIssue::invalid_type("object", path, Some(v)));
        return None;
    };
    let before = issues.len();

    let reference = match obj.get("ref") {
        Some(Value::String(s)) => {
            if !crate::jsstr::zod_len_min_ok(s, 1) {
                issues.push(ZodIssue::too_small_string(
                    json!(1),
                    at(&path, json!("ref")),
                ));
            }
            if !crate::jsstr::zod_len_max_ok(s, MAX_MOCK_REF_LENGTH) {
                issues.push(ZodIssue::too_big_string(
                    json!(MAX_MOCK_REF_LENGTH),
                    at(&path, json!("ref")),
                ));
            }
            Some(s.clone())
        }
        other => {
            issues.push(ZodIssue::invalid_type(
                "string",
                at(&path, json!("ref")),
                other,
            ));
            None
        }
    };

    let state = match obj.get("state").and_then(Value::as_str).and_then(state_of) {
        Some(state) => Some(state),
        None => {
            issues.push(ZodIssue::invalid_value(
                &INVENTORY_STATE_VALUES,
                at(&path, json!("state")),
            ));
            None
        }
    };

    let count = match obj.get("count") {
        None => None,
        Some(Value::Number(n)) => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            let p = at(&path, json!("count"));
            if x.fract() != 0.0 {
                issues.push(ZodIssue::invalid_int_type(p, obj.get("count")));
            } else {
                if x < 0.0 {
                    issues.push(ZodIssue::too_small_number(json!(0), p.clone()));
                }
                if x > MAX_MOCK_COUNT as f64 {
                    issues.push(ZodIssue::too_big_number(json!(MAX_MOCK_COUNT), p));
                }
            }
            Some(x)
        }
        other => {
            issues.push(ZodIssue::invalid_type(
                "number",
                at(&path, json!("count")),
                other,
            ));
            None
        }
    };

    let metadata = match obj.get("metadata") {
        None => None,
        Some(Value::Object(m)) => {
            for (key, value) in m {
                issues.extend(metadata_value_issues(
                    value,
                    at(&at(&path, json!("metadata")), json!(key)),
                ));
            }
            Some(m.clone())
        }
        other => {
            issues.push(ZodIssue::invalid_type(
                "record",
                at(&path, json!("metadata")),
                other,
            ));
            None
        }
    };

    let extra: Vec<String> = obj
        .keys()
        .filter(|k| !ROW_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    if !extra.is_empty() {
        issues.push(ZodIssue::unrecognized_keys(extra, path));
    }

    if issues.len() > before {
        return None;
    }
    Some(MockInventoryRow {
        reference: reference?,
        state: state?,
        count,
        metadata,
    })
}

/// v4 `MockInventorySchema.nullish().safeParse(body.inventory)` — absent or
/// `null` is `Ok(None)` (no inventory: inventory tests decline); a bad row is
/// the issue list v4's `badRequest('Invalid request body', issues)` carries.
pub fn parse_mock_inventory(
    body: Option<&Value>,
) -> Result<Option<Vec<MockInventoryRow>>, Vec<ZodIssue>> {
    let rows = match body {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(rows)) => rows,
        Some(other) => {
            let mut issues = vec![ZodIssue::invalid_type("array", vec![], Some(other))];
            // Zod ≥ 4.6's length checks run past the type failure on anything
            // with a `.length` (a string).
            if let Value::String(s) = other {
                if !crate::jsstr::zod_len_max_ok(s, MAX_MOCK_INVENTORY_ROWS) {
                    issues.push(ZodIssue::too_big_string(
                        json!(MAX_MOCK_INVENTORY_ROWS),
                        vec![],
                    ));
                }
            }
            return Err(issues);
        }
    };
    // Zod ≥ 4.6.7 (`capSize`): an oversized array answers its `too_big` ALONE.
    if rows.len() > MAX_MOCK_INVENTORY_ROWS {
        return Err(vec![ZodIssue::too_big_array(
            json!(MAX_MOCK_INVENTORY_ROWS),
            vec![],
        )]);
    }
    let mut issues = Vec::new();
    let mut parsed = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if let Some(row) = parse_row(row, vec![json!(i)], &mut issues) {
            parsed.push(row);
        }
    }
    if issues.is_empty() {
        Ok(Some(parsed))
    } else {
        Err(issues)
    }
}

/// One row as the mock keeps it: the TRIMMED ref, the count defaulted.
#[derive(Debug, Clone)]
struct MockEntry {
    reference: String,
    state: InventoryState,
    count: f64,
    metadata: Map<String, Value>,
}

/// v4 `mockInventoryContext(rows)` — a context over mock rows.
#[derive(Debug, Clone)]
pub struct MockInventoryContext {
    /// v4's `Map<key, row>`: a later row for the same key REPLACES the value
    /// but keeps the FIRST position (a JS `Map.set`).
    by_ref: Vec<(String, MockEntry)>,
}

/// v4 `mockInventoryContext(rows)`. Keyed by `ref.trim().toLowerCase()`; LATER
/// rows win; a blank ref is skipped; `count ?? (equipped ? 1 : 0)`.
pub fn mock_inventory_context(rows: &[MockInventoryRow]) -> MockInventoryContext {
    let mut by_ref: Vec<(String, MockEntry)> = Vec::new();
    for row in rows {
        let trimmed = crate::jsstr::js_trim(&row.reference);
        let key = trimmed.to_lowercase();
        if key.is_empty() {
            continue;
        }
        let entry = MockEntry {
            reference: trimmed.to_string(),
            state: row.state,
            count: row
                .count
                .unwrap_or(if row.state == InventoryState::Equipped {
                    1.0
                } else {
                    0.0
                }),
            metadata: row.metadata.clone().unwrap_or_default(),
        };
        match by_ref.iter_mut().find(|(k, _)| *k == key) {
            Some((_, slot)) => *slot = entry,
            None => by_ref.push((key, entry)),
        }
    }
    MockInventoryContext { by_ref }
}

impl MockInventoryContext {
    fn lookup(&self, reference: &str) -> Option<&MockEntry> {
        let key = crate::jsstr::js_trim(reference).to_lowercase();
        self.by_ref.iter().find(|(k, _)| *k == key).map(|(_, e)| e)
    }
}

fn in_scope(scope: AggregateScope, state: InventoryState) -> bool {
    match scope {
        AggregateScope::Equipped => state == InventoryState::Equipped,
        AggregateScope::Owned => {
            matches!(state, InventoryState::Equipped | InventoryState::Carried)
        }
        AggregateScope::Reachable => matches!(
            state,
            InventoryState::Equipped | InventoryState::Carried | InventoryState::Reachable
        ),
    }
}

impl InventoryContext for MockInventoryContext {
    fn character_id(&self) -> &str {
        "bench"
    }
    fn chat_id(&self) -> Option<&str> {
        None
    }
    fn state(&self, reference: &str) -> InventoryState {
        self.lookup(reference)
            .map_or(InventoryState::Absent, |e| e.state)
    }
    fn equipped_kin_count(&self, reference: &str) -> f64 {
        self.lookup(reference).map_or(0.0, |e| e.count)
    }
    /// ALWAYS absent (R-U): a mock row names a state, not an object.
    fn item(&self, _reference: &str) -> Option<ResolvedInventoryItem> {
        None
    }
    fn scope_objects(&self, scope: AggregateScope) -> Vec<AggregateObject> {
        let mut objects = Vec::new();
        for (_, entry) in &self.by_ref {
            if !in_scope(scope, entry.state) {
                continue;
            }
            // v4 `for (let i = 0; i < copies; i++)` — a JS number count.
            let copies = if entry.state == InventoryState::Equipped {
                entry.count
            } else {
                1.0
            };
            let mut i = 0.0;
            while i < copies {
                objects.push(AggregateObject {
                    name: entry.reference.clone(),
                    title: entry.reference.clone(),
                    metadata: entry.metadata.clone(),
                });
                i += 1.0;
            }
        }
        objects
    }
}
