//! Tier-1 differential (P4.D329, v4 `e1872c94f` + `a53024d3c`): the Proving
//! Bench's mock bag — `pascal::bench_inventory` against v4's real
//! `lib/pascal/bench-inventory.ts`.
//!
//! - **parse** — `MockInventorySchema.nullish().safeParse(input)`: the parsed
//!   rows, or v4's raw Zod ISSUE objects compared WHOLE as bytes (each code's
//!   own key order) — what v4's bench 400 carries as `details`.
//! - **context** — `mockInventoryContext(rows)` probed through every method:
//!   `state`, `equippedKinCount`, `item` (always null — R-U), the three
//!   `scopeObjects`, `characterId` / `chatId`.
//!
//! Generate the oracle output (Node 24 `~/.nvm/versions/node/v24.13.1/bin`):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TZ=UTC npx tsx \
//!     "$V5W/harness/oracle/cases/pascal-bench-inventory.ts" \
//!     > /tmp/oracle-pascal-bench-inventory.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_BENCH_INVENTORY=/tmp/oracle-pascal-bench-inventory.ndjson \
//!     cargo test -p quilltap-harness --test pascal_bench_inventory_equivalence

use quilltap_core::pascal::bench_inventory::{
    mock_inventory_context, parse_mock_inventory, MockInventoryRow,
};
use quilltap_core::pascal::p4d329_c2_standins::{AggregateScope, InventoryContext};
use serde_json::{json, Map, Value};

/// A parsed row in v4's shape order (`ref`, `state`, `count?`, `metadata?`).
fn row_json(r: &MockInventoryRow) -> Value {
    let mut m = Map::new();
    m.insert("ref".into(), json!(r.reference));
    m.insert("state".into(), json!(r.state.as_str()));
    if let Some(c) = r.count {
        m.insert("count".into(), json!(c as i64));
    }
    if let Some(md) = &r.metadata {
        m.insert("metadata".into(), Value::Object(md.clone()));
    }
    Value::Object(m)
}

#[test]
fn pascal_bench_inventory_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_BENCH_INVENTORY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PASCAL_BENCH_INVENTORY to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut parses, mut refusals, mut contexts) = (0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap();
        match row["kind"].as_str().unwrap() {
            "parse" => {
                let got = match parse_mock_inventory(row.get("input")) {
                    Ok(rows) => json!({
                        "ok": true,
                        "data": rows.map(|rows| Value::Array(rows.iter().map(row_json).collect()))
                            .unwrap_or(Value::Null),
                    }),
                    Err(issues) => {
                        refusals += 1;
                        json!({
                            "ok": false,
                            "issues": issues.iter().map(|i| i.to_value()).collect::<Vec<_>>(),
                        })
                    }
                };
                let mut want = Map::new();
                want.insert("ok".into(), row["ok"].clone());
                if row["ok"] == json!(true) {
                    want.insert("data".into(), row["data"].clone());
                } else {
                    want.insert("issues".into(), row["issues"].clone());
                }
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&Value::Object(want)).unwrap(),
                    "parse '{id}'"
                );
                parses += 1;
            }
            "context" => {
                let rows = parse_mock_inventory(Some(&row["rows"]))
                    .unwrap_or_else(|e| panic!("context '{id}': rows refused: {e:?}"))
                    .unwrap_or_default();
                let ctx = mock_inventory_context(&rows);
                assert_eq!(
                    json!(ctx.character_id()),
                    row["characterId"],
                    "characterId '{id}'"
                );
                assert_eq!(
                    ctx.chat_id().map(Value::from).unwrap_or(Value::Null),
                    row["chatId"],
                    "chatId '{id}'"
                );
                for probe in row["probes"].as_array().unwrap() {
                    let r = probe["ref"].as_str().unwrap();
                    assert_eq!(
                        json!(ctx.state(r).as_str()),
                        probe["state"],
                        "state('{r}') '{id}'"
                    );
                    assert_eq!(
                        serde_json::to_string(&json!(ctx.equipped_kin_count(r) as i64)).unwrap(),
                        serde_json::to_string(&probe["equippedKinCount"]).unwrap(),
                        "equippedKinCount('{r}') '{id}'"
                    );
                    assert!(
                        ctx.item(r).is_none(),
                        "item('{r}') '{id}' — ALWAYS null (R-U)"
                    );
                    assert_eq!(probe["item"], Value::Null);
                }
                for scope in [
                    AggregateScope::Equipped,
                    AggregateScope::Owned,
                    AggregateScope::Reachable,
                ] {
                    let got: Vec<Value> = ctx
                        .scope_objects(scope)
                        .iter()
                        .map(
                            |o| json!({ "name": o.name, "title": o.title, "metadata": o.metadata }),
                        )
                        .collect();
                    assert_eq!(
                        serde_json::to_string(&got).unwrap(),
                        serde_json::to_string(&row["scopeObjects"][scope.as_str()]).unwrap(),
                        "scopeObjects({}) '{id}'",
                        scope.as_str()
                    );
                }
                contexts += 1;
            }
            other => panic!("unknown row kind '{other}'"),
        }
    }
    assert!(parses >= 30, "only {parses} parse rows");
    assert!(refusals >= 20, "only {refusals} refusals");
    assert!(contexts >= 8, "only {contexts} context rows");
    eprintln!("OK: pascal bench inventory matched oracle ({parses} parse, {contexts} context).");
}
