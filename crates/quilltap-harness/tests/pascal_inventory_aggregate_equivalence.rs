//! Tier-1 differential (P4.D329, v4 `a53024d3c`): the `inventoryAggregate`
//! subject — `pascal::inventory_aggregate` against v4's real
//! `lib/pascal/inventory-aggregate.ts` (`parseAggregateKey`,
//! `isListAggregate`, `aggregateKeyEndsInFold`, `aggregateValue`,
//! `inventoryAggregateHolds`, `describeAggregateValue`), over literal objects,
//! v4's bench bag (`mockInventoryContext` — v5's `mock_inventory_context` over
//! the same rows), and v4's REAL `inventoryContextFrom` context with its
//! answers RECORDED (the R-3 idiom).
//!
//! Reasons are compared WHOLE (they surface as `inventoryAggregate.<key>:
//! <reason>` load rejections), values with `-0` kept apart from `0`, and the
//! decline reasons a test hands its hook as a list.
//!
//! The LIST-valued `holds` rows compare entry by entry through the shared
//! table's list rule — P4.D333's (`metadata_match.rs`). On a branch without it
//! they decline; they are named by [`LIST_RULE_RESIDUE`], which fails the run
//! if a carved row turns green (lift it then).
//!
//! Generate the oracle output (Node 24 `~/.nvm/versions/node/v24.13.1/bin`):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TZ=UTC npx tsx \
//!     "$V5W/harness/oracle/cases/pascal-inventory-aggregate.ts" \
//!     > /tmp/oracle-pascal-inventory-aggregate.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_INVENTORY_AGGREGATE=/tmp/oracle-pascal-inventory-aggregate.ndjson \
//!     cargo test -p quilltap-harness --test pascal_inventory_aggregate_equivalence

use std::convert::Infallible;

use quilltap_core::pascal::custom_tool_types::{
    AnyOperand, MetadataComparator, NumberOrParamRef, StringOperand,
};
use quilltap_core::pascal::inventory_aggregate::{
    aggregate_key_ends_in_fold, aggregate_value, describe_aggregate_value,
    inventory_aggregate_holds, is_list_aggregate, parse_aggregate_key, AggregateField,
    AggregateObject, AggregateValue, DescribedAggregate, ParsedAggregateKey,
};
use quilltap_core::pascal::metadata_match::ResolvedValue;
use serde_json::{json, Map, Value};

mod recorded_inventory;
use recorded_inventory::RowBag;

/// The list-valued `holds` rows with a bag (every one: an un-folded
/// `metadata.<key>` is a list even over one string entry) — they decline on a
/// branch without P4.D333's per-entry list rule (`metadata_match.rs`), or
/// decline for the old reason (`the key holds an array…` where the pin says
/// `fold it`), and are green on the union. A carved row that turns GREEN fails
/// the run. MEASURED: exactly the 16 `list: true` rows whose bag is not null.
const LIST_RULE_RESIDUE: &[&str] = &[
    "holds-real-names-neq-kit",
    "holds-mock-list-eq",
    "holds-empty-names-eq",
    "holds-empty-names-neq",
    "holds-empty-names-ncontains",
    "holds-s8-equipped-string",
    "holds-s8-equipped-string-stowed",
    "holds-s8-owned-string-stowed",
    "holds-s8-owned-string-shelf",
    "holds-s8-reachable-string-shelf",
    "holds-s8-equipped-list",
    "holds-s8-reachable-list-shelf",
    "holds-s8-names-exact",
    "holds-s8-names-not-prefix",
    "holds-s8-names-not-case",
    "holds-list-ordering-declines",
];

/// `-0`-safe number encoding, the oracle's `enc`.
fn enc_number(n: f64) -> String {
    if n == 0.0 && n.is_sign_negative() {
        "-0".to_string()
    } else {
        serde_json::to_string(&AggregateValue::Number(n).to_value()).unwrap()
    }
}

fn enc_value(v: Option<&AggregateValue>) -> String {
    match v {
        None => "absent".to_string(),
        Some(AggregateValue::Number(n)) => enc_number(*n),
        Some(list @ AggregateValue::List(_)) => serde_json::to_string(&list.to_value()).unwrap(),
    }
}

fn enc_described(d: &DescribedAggregate) -> String {
    match d {
        DescribedAggregate::Number(n) => enc_number(*n),
        other => serde_json::to_string(&other.to_value()).unwrap(),
    }
}

/// v4's `ParsedAggregateKey` object (`{ scope, field: { kind, key?, fold? } }`).
fn parsed_json(p: &ParsedAggregateKey) -> Value {
    let field = match &p.field {
        AggregateField::Count => json!({ "kind": "count" }),
        AggregateField::Names => json!({ "kind": "names" }),
        AggregateField::Titles => json!({ "kind": "titles" }),
        AggregateField::Metadata { key, fold: None } => json!({ "kind": "metadata", "key": key }),
        AggregateField::Metadata {
            key,
            fold: Some(fold),
        } => json!({ "kind": "metadata", "key": key, "fold": fold.as_str() }),
    };
    json!({ "scope": p.scope.as_str(), "field": field })
}

/// A literal comparator (the gate's `GateComparatorSchema` shape) from the
/// oracle's JSON.
fn comparator_of(v: &Value) -> MetadataComparator {
    let number = |k: &str| {
        v.get(k)
            .and_then(Value::as_f64)
            .map(NumberOrParamRef::Number)
    };
    let any = |k: &str| {
        v.get(k).map(|x| match x {
            Value::Number(n) => AnyOperand::Number(n.as_f64().unwrap()),
            Value::String(s) => AnyOperand::String(s.clone()),
            Value::Bool(b) => AnyOperand::Bool(*b),
            other => panic!("operand {other}"),
        })
    };
    let string = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .map(|s| StringOperand::String(s.to_string()))
    };
    MetadataComparator {
        gt: number("gt"),
        gte: number("gte"),
        lt: number("lt"),
        lte: number("lte"),
        eq: any("eq"),
        neq: any("neq"),
        contains: string("contains"),
        ncontains: string("ncontains"),
    }
}

fn literal(v: &Value, key: &str) -> ResolvedValue {
    match &v[key] {
        Value::Number(n) => ResolvedValue::Number(n.as_f64().unwrap()),
        Value::String(s) => ResolvedValue::String(s.clone()),
        Value::Bool(b) => ResolvedValue::Bool(*b),
        other => panic!("operand {other}"),
    }
}

#[test]
fn pascal_inventory_aggregate_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_INVENTORY_AGGREGATE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PASCAL_INVENTORY_AGGREGATE to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut parses, mut values, mut holds) = (0usize, 0usize, 0usize);
    let (mut recorded_rows, mut mock_rows, mut declined_rows) = (0usize, 0usize, 0usize);
    let mut failed: Vec<String> = Vec::new();
    let mut residue_green: Vec<String> = Vec::new();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match row["kind"].as_str().unwrap() {
                "parse" => {
                    let key = row["key"].as_str().unwrap();
                    let got = match parse_aggregate_key(key) {
                        Ok(p) => {
                            assert_eq!(
                                Value::Bool(is_list_aggregate(&p)),
                                row["isList"],
                                "isListAggregate '{id}'"
                            );
                            assert_eq!(
                                Value::Bool(aggregate_key_ends_in_fold(&p)),
                                row["endsInFold"],
                                "aggregateKeyEndsInFold '{id}'"
                            );
                            json!({ "ok": true, "parsed": parsed_json(&p) })
                        }
                        Err(reason) => json!({ "ok": false, "reason": reason }),
                    };
                    assert_eq!(
                        serde_json::to_string(&got).unwrap(),
                        serde_json::to_string(&row["result"]).unwrap(),
                        "parseAggregateKey '{id}'"
                    );
                }
                "value" => {
                    let parsed = parse_aggregate_key(row["key"].as_str().unwrap()).unwrap();
                    let objects: Vec<Value> =
                        serde_json::from_str(row["objectsJson"].as_str().unwrap()).unwrap();
                    let objects: Vec<AggregateObject> = objects
                        .iter()
                        .map(|o| AggregateObject {
                            name: o["name"].as_str().unwrap().to_string(),
                            title: o["title"].as_str().unwrap().to_string(),
                            metadata: o["metadata"].as_object().cloned().unwrap_or_else(Map::new),
                        })
                        .collect();
                    assert_eq!(
                        enc_value(aggregate_value(&parsed, &objects).as_ref()),
                        row["value"].as_str().unwrap(),
                        "aggregateValue '{id}'"
                    );
                }
                "holds" => {
                    let bag = RowBag::from_row(Some(&row["inventory"]));
                    let comparator_json = &row["comparator"];
                    let comparator = comparator_of(comparator_json);
                    let key = row["key"].as_str().unwrap();
                    let mut declines: Vec<String> = Vec::new();
                    let held = inventory_aggregate_holds::<Infallible>(
                        &comparator,
                        key,
                        bag.as_dyn(),
                        &mut |k| Ok(literal(comparator_json, k)),
                        &mut |reason| declines.push(reason),
                    )
                    .unwrap_or_else(|never| match never {});
                    assert_eq!(Value::Bool(held), row["holds"], "holds '{id}'");
                    assert_eq!(json!(declines), row["declines"], "declines '{id}'");
                    let described =
                        describe_aggregate_value(key, bag.as_dyn()).map(|d| enc_described(&d));
                    assert_eq!(
                        described.map(Value::String).unwrap_or(Value::Null),
                        row["described"],
                        "describeAggregateValue '{id}'"
                    );
                }
                other => panic!("unknown row kind '{other}'"),
            }
        }));
        let residue = LIST_RULE_RESIDUE.contains(&id.as_str());
        match (walked.is_ok(), residue) {
            (false, false) => failed.push(id.clone()),
            (true, true) => residue_green.push(id.clone()),
            _ => {}
        }
        match row["kind"].as_str().unwrap() {
            "parse" => parses += 1,
            "value" => values += 1,
            _ => {
                holds += 1;
                if row["inventory"].get("recorded").is_some() {
                    recorded_rows += 1;
                }
                if row["inventory"].get("rows").is_some() {
                    mock_rows += 1;
                }
                if row["declines"].as_array().is_some_and(|d| !d.is_empty()) {
                    declined_rows += 1;
                }
            }
        }
    }

    assert!(failed.is_empty(), "rows differ from v4: {failed:?}");
    assert!(
        residue_green.is_empty(),
        "LIST_RULE_RESIDUE rows turned GREEN (P4.D333's list rule landed?) — lift them: {residue_green:?}"
    );
    assert!(
        parses >= 25 && values >= 20 && holds >= 40,
        "oracle looked short: {parses} parse / {values} value / {holds} holds rows"
    );
    assert!(
        recorded_rows >= 20,
        "only {recorded_rows} rows read a real (recorded) context"
    );
    assert!(mock_rows >= 4, "only {mock_rows} rows read the bench's bag");
    assert!(declined_rows >= 8, "only {declined_rows} rows declined");
    eprintln!(
        "OK: pascal inventoryAggregate matched oracle ({parses} parse, {values} value, {holds} holds; residue {} by name).",
        LIST_RULE_RESIDUE.len()
    );
}
