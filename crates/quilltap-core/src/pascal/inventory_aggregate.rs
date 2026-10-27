//! The `inventoryAggregate` subject — a derived sheet over everything in one
//! scope of the rolling character's bag (v4 `lib/pascal/inventory-aggregate.ts`,
//! `a53024d3c`; spec pascal-gate-broadening §5.1–5.3).
//!
//! v4's header rides verbatim:
//!
//! > Where `inventory` asks about ONE named object, an aggregate key reads ONE
//! > attribute across every object in a scope:
//! >
//! > - `equipped.names` — the names of everything equipped in this chat;
//! > - `owned.metadata.toolAbilities` — every owned object's `toolAbilities`,
//! >   as a list;
//! > - `equipped.metadata.toolVersion.max` — the largest numeric `toolVersion`
//! >   equipped.
//! >
//! > The key is `<scope>.<field>`, exactly as a `progress` key is
//! > `<id>.<field>`, and the value is compared through the one comparison table
//! > (`metadataComparatorHolds`), which compares a list entry by entry. This
//! > module only parses keys and builds values; it never compares.
//! >
//! > Fail-soft in the inventory direction: with no context every test
//! > declines; with a real context an empty scope is a fact (a count of 0, an
//! > empty list, absent folds).
//!
//! The list comparison is the shared table's (`metadata_match`, P4.D333's list
//! rule); until that lands on a branch, a list-valued key declines there.

use serde_json::{Map, Value};

use super::custom_tool_types::MetadataComparator;
use super::metadata_match::{js_primitive, metadata_comparator_holds, ResolvedValue};
use crate::inventory::refs::InventoryContext;
pub use crate::inventory::refs::{AggregateObject, AggregateScope};

/// v4 `AGGREGATE_SCOPES` — cumulative: equipped ⊆ owned ⊆ reachable.
pub const AGGREGATE_SCOPES: [&str; 3] = ["equipped", "owned", "reachable"];

/// v4 `AGGREGATE_FOLDS` — the numeric folds a `metadata.<key>` field may end in.
pub const AGGREGATE_FOLDS: [&str; 4] = ["min", "max", "sum", "count"];

/// v4 `AggregateFold`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateFold {
    Min,
    Max,
    Sum,
    Count,
}

impl AggregateFold {
    pub fn as_str(self) -> &'static str {
        match self {
            AggregateFold::Min => "min",
            AggregateFold::Max => "max",
            AggregateFold::Sum => "sum",
            AggregateFold::Count => "count",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        match word {
            "min" => Some(AggregateFold::Min),
            "max" => Some(AggregateFold::Max),
            "sum" => Some(AggregateFold::Sum),
            "count" => Some(AggregateFold::Count),
            _ => None,
        }
    }
}

/// v4 `AggregateField` — what an aggregate key reads off each object in its
/// scope. `Metadata` without a fold is a list; with one, a number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateField {
    Count,
    Names,
    Titles,
    Metadata {
        key: String,
        fold: Option<AggregateFold>,
    },
}

/// v4 `ParsedAggregateKey`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAggregateKey {
    pub scope: AggregateScope,
    pub field: AggregateField,
}

const METADATA_PREFIX: &str = "metadata.";

fn parse_scope(word: &str) -> Option<AggregateScope> {
    match word {
        "equipped" => Some(AggregateScope::Equipped),
        "owned" => Some(AggregateScope::Owned),
        "reachable" => Some(AggregateScope::Reachable),
        _ => None,
    }
}

/// v4 `parseAggregateKey(key)` — parse `<scope>.<field>`. The one place the
/// grammar lives: the schema's load-time check, the Workbench's validator and
/// the evaluator all delegate here. `Err` carries v4's reason verbatim.
///
/// In a `metadata.` field, a LAST dotted segment that is a fold word (with
/// something before it) is the fold; otherwise the whole rest is the key. So
/// `equipped.metadata.toolVersion.max` folds `toolVersion`, and
/// `equipped.metadata.a.b` lists the key `a.b`.
///
/// v4's `indexOf` / `lastIndexOf` count UTF-16 units; the separator is ASCII
/// and each test is emptiness at an end, so byte positions give the same
/// answers and every slice lands on a char boundary.
pub fn parse_aggregate_key(key: &str) -> Result<ParsedAggregateKey, String> {
    let scopes = AGGREGATE_SCOPES.join(", ");
    let dot = match key.find('.') {
        Some(dot) if dot > 0 => dot,
        _ => {
            return Err(format!(
                "\"{key}\" must be <scope>.<field>, where the scope is {scopes}"
            ))
        }
    };
    let scope_word = &key[..dot];
    let Some(scope) = parse_scope(scope_word) else {
        return Err(format!("\"{scope_word}\" is not a scope; use {scopes}"));
    };
    let rest = &key[dot + 1..];
    let field = match rest {
        "count" => Some(AggregateField::Count),
        "names" => Some(AggregateField::Names),
        "titles" => Some(AggregateField::Titles),
        _ => None,
    };
    if let Some(field) = field {
        return Ok(ParsedAggregateKey { scope, field });
    }
    if let Some(path) = rest.strip_prefix(METADATA_PREFIX) {
        if !path.is_empty() {
            if let Some(last_dot) = path.rfind('.') {
                if last_dot > 0 {
                    if let Some(fold) = AggregateFold::parse(&path[last_dot + 1..]) {
                        return Ok(ParsedAggregateKey {
                            scope,
                            field: AggregateField::Metadata {
                                key: path[..last_dot].to_string(),
                                fold: Some(fold),
                            },
                        });
                    }
                }
            }
            return Ok(ParsedAggregateKey {
                scope,
                field: AggregateField::Metadata {
                    key: path.to_string(),
                    fold: None,
                },
            });
        }
    }
    Err(format!(
        "\"{rest}\" is not a field; use count, names, titles, or metadata.<key> (optionally ending .min, .max, .sum or .count)"
    ))
}

/// v4 `isListAggregate(parsed)` — true when an aggregate key reads as a list
/// (and so declines ordering).
pub fn is_list_aggregate(parsed: &ParsedAggregateKey) -> bool {
    matches!(
        parsed.field,
        AggregateField::Names
            | AggregateField::Titles
            | AggregateField::Metadata { fold: None, .. }
    )
}

/// v4 `aggregateKeyEndsInFold(parsed)` — true when a `metadata.<key>` field's
/// own last segment is a fold word: the key `stats.max` cannot be read as a
/// plain list, because the parser takes `.max` as the fold. The Workbench
/// warns about it.
pub fn aggregate_key_ends_in_fold(parsed: &ParsedAggregateKey) -> bool {
    matches!(parsed.field, AggregateField::Metadata { fold: Some(_), .. })
}

/// The sheet's value for one key: a number or a list of primitives (`None` is
/// v4's `undefined` — a fold with nothing numeric to fold).
#[derive(Debug, Clone, PartialEq)]
pub enum AggregateValue {
    Number(f64),
    List(Vec<ResolvedValue>),
}

impl AggregateValue {
    /// As the JSON value the comparison table reads off the one-key sheet.
    pub fn to_value(&self) -> Value {
        match self {
            AggregateValue::Number(n) => crate::db::js_number_to_json(*n),
            AggregateValue::List(entries) => {
                Value::Array(entries.iter().map(ResolvedValue::to_value).collect())
            }
        }
    }
}

/// v4 `entriesOf(value)` — the primitives one object contributes for a
/// metadata key: a primitive, or each primitive of an array.
fn entries_of(value: &Value) -> Vec<ResolvedValue> {
    if let Some(primitive) = js_primitive(value) {
        return vec![primitive];
    }
    match value {
        Value::Array(items) => items.iter().filter_map(js_primitive).collect(),
        _ => Vec::new(),
    }
}

fn finite_number(value: &ResolvedValue) -> Option<f64> {
    match value {
        ResolvedValue::Number(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

/// JS `Math.min(...numbers)` over a non-empty finite list: `-0` is less than
/// `+0` there, which `f64::min` does not promise.
fn js_min(numbers: &[f64]) -> f64 {
    numbers.iter().fold(f64::INFINITY, |m, &n| {
        if n < m || (n == 0.0 && m == 0.0 && n.is_sign_negative()) {
            n
        } else {
            m
        }
    })
}

/// JS `Math.max(...numbers)` over a non-empty finite list: `+0` beats `-0`.
fn js_max(numbers: &[f64]) -> f64 {
    numbers.iter().fold(f64::NEG_INFINITY, |m, &n| {
        if n > m || (n == 0.0 && m == 0.0 && n.is_sign_positive()) {
            n
        } else {
            m
        }
    })
}

/// v4 `aggregateValue(parsed, objects)` — the aggregate sheet's value for one
/// parsed key: a number, a list of primitives, or `None` (absent).
pub fn aggregate_value(
    parsed: &ParsedAggregateKey,
    objects: &[AggregateObject],
) -> Option<AggregateValue> {
    match &parsed.field {
        AggregateField::Count => Some(AggregateValue::Number(objects.len() as f64)),
        AggregateField::Names => Some(AggregateValue::List(
            objects
                .iter()
                .map(|o| ResolvedValue::String(o.name.clone()))
                .collect(),
        )),
        AggregateField::Titles => Some(AggregateValue::List(
            objects
                .iter()
                .map(|o| ResolvedValue::String(o.title.clone()))
                .collect(),
        )),
        AggregateField::Metadata { key, fold } => {
            // `Object.prototype.hasOwnProperty.call(object.metadata, key)` — a
            // serde map has no prototype, so its own keys are all there is.
            let per_object: Vec<Vec<ResolvedValue>> = objects
                .iter()
                .filter_map(|o| o.metadata.get(key))
                .map(entries_of)
                .collect();
            let Some(fold) = fold else {
                return Some(AggregateValue::List(
                    per_object.into_iter().flatten().collect(),
                ));
            };
            let numeric_per_object: Vec<Vec<f64>> = per_object
                .iter()
                .map(|entries| entries.iter().filter_map(finite_number).collect())
                .collect();
            if *fold == AggregateFold::Count {
                return Some(AggregateValue::Number(
                    numeric_per_object.iter().filter(|e| !e.is_empty()).count() as f64,
                ));
            }
            let numbers: Vec<f64> = numeric_per_object.into_iter().flatten().collect();
            if numbers.is_empty() {
                return None;
            }
            Some(AggregateValue::Number(match fold {
                AggregateFold::Min => js_min(&numbers),
                AggregateFold::Max => js_max(&numbers),
                // A LEFT fold from +0, in list order — float addition is not
                // associative, so the order is the contract.
                _ => numbers.iter().fold(0.0, |total, n| total + n),
            }))
        }
    }
}

/// v4 `inventoryAggregateHolds(comparator, key, ctx, hooks)` — whether one
/// `inventoryAggregate` test holds. Builds the sheet value and hands it to the
/// one comparison table: a number compares as a number, a list entry by entry
/// (P4.D333's list rule), an absent fold declines.
pub fn inventory_aggregate_holds<E>(
    comparator: &MetadataComparator,
    key: &str,
    ctx: Option<&dyn InventoryContext>,
    resolve_operand: &mut dyn FnMut(&str) -> Result<ResolvedValue, E>,
    on_decline: &mut dyn FnMut(String),
) -> Result<bool, E> {
    let Some(ctx) = ctx else {
        on_decline(
            "no inventory to aggregate (nobody in particular rolled, or the vault could not be read)"
                .to_string(),
        );
        return Ok(false);
    };
    let parsed = match parse_aggregate_key(key) {
        Ok(parsed) => parsed,
        Err(reason) => {
            on_decline(reason);
            return Ok(false);
        }
    };
    let mut sheet = Map::new();
    if let Some(value) = aggregate_value(&parsed, &ctx.scope_objects(parsed.scope)) {
        sheet.insert(key.to_string(), value.to_value());
    }
    metadata_comparator_holds(comparator, key, &sheet, resolve_operand, on_decline)
}

/// What [`describe_aggregate_value`] reports for a DEBUG line.
#[derive(Debug, Clone, PartialEq)]
pub enum DescribedAggregate {
    Absent,
    ListOf(usize),
    Number(f64),
}

impl DescribedAggregate {
    /// v4's field value: `'absent'`, `` `list of ${n}` ``, or the number.
    pub fn to_value(&self) -> Value {
        match self {
            DescribedAggregate::Absent => Value::String("absent".into()),
            DescribedAggregate::ListOf(n) => Value::String(format!("list of {n}")),
            DescribedAggregate::Number(n) => crate::db::js_number_to_json(*n),
        }
    }
}

/// v4 `describeAggregateValue(key, ctx)` — the sheet value for a key against a
/// context, for a caller that logs it. `None` (v4's `null`) when it cannot be
/// read.
pub fn describe_aggregate_value(
    key: &str,
    ctx: Option<&dyn InventoryContext>,
) -> Option<DescribedAggregate> {
    let ctx = ctx?;
    let parsed = parse_aggregate_key(key).ok()?;
    Some(
        match aggregate_value(&parsed, &ctx.scope_objects(parsed.scope)) {
            None => DescribedAggregate::Absent,
            Some(AggregateValue::List(entries)) => DescribedAggregate::ListOf(entries.len()),
            Some(AggregateValue::Number(n)) => DescribedAggregate::Number(n),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &str) -> ParsedAggregateKey {
        parse_aggregate_key(k).unwrap()
    }

    #[test]
    fn the_two_predicates_over_every_field() {
        for (k, list, folds) in [
            ("equipped.count", false, false),
            ("owned.names", true, false),
            ("reachable.titles", true, false),
            ("equipped.metadata.a", true, false),
            ("equipped.metadata.a.b", true, false),
            ("equipped.metadata.stats.max", false, true),
            ("owned.metadata.v.count", false, true),
        ] {
            assert_eq!(is_list_aggregate(&key(k)), list, "{k}");
            assert_eq!(aggregate_key_ends_in_fold(&key(k)), folds, "{k}");
        }
    }

    #[test]
    fn js_min_and_max_order_the_zeros() {
        assert!(js_min(&[0.0, -0.0]).is_sign_negative());
        assert!(js_min(&[-0.0, 0.0]).is_sign_negative());
        assert!(js_max(&[-0.0, 0.0]).is_sign_positive());
        assert!(js_max(&[0.0, -0.0]).is_sign_positive());
        assert_eq!(js_min(&[3.0, -2.5, 7.0]), -2.5);
        assert_eq!(js_max(&[3.0, -2.5, 7.0]), 7.0);
    }
}
