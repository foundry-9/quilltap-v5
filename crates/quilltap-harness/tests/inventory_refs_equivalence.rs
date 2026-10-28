//! Tier-1 differential (PURE): the Inventory reference rule and the
//! change-effect pure helpers (P4.D328 — v4 `lib/inventory/refs.ts` +
//! `lib/inventory/equip-effects.ts`' `describeInventoryChange` /
//! `recordPendingInventoryChange` / `strikeFromPendingInventoryChange`, at
//! `322a5488e`).
//!
//! Every NDJSON row carries its INPUT (the pool's item literals, the holder's
//! own ids, the equipped ids, the reference / options / steps) beside v4's
//! output; this test rebuilds the pool from the row, runs
//! `quilltap_core::inventory::refs` / `services::inventory_change_effects`,
//! and compares the outputs as `JSON.stringify` BYTES (`serde_json::to_string`
//! — key order and `null`s pin; memory note `serde-json-map-eq-ignores-order`).
//!
//! The pending map iterates in insertion order on both sides (v4's `Map`,
//! v5's `IndexMap` since P4.169 I-4 — R-6 converged), so the `record` /
//! `strike` rows compare their entries in v4's order, byte for byte.
//!
//! Generate the oracle:
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/inventory-refs.ts \
//!     > /tmp/oracle-inventory-refs.ndjson
//! Run:
//!   QT_ORACLE_INVENTORY_REFS=/tmp/oracle-inventory-refs.ndjson \
//!     cargo test -p quilltap-harness --test inventory_refs_equivalence

use std::collections::HashSet;

use quilltap_core::inventory::equip::{InventoryChange, InventoryGive, PendingInventoryMap};
use quilltap_core::inventory::refs::{
    equipped_kin_count, equipped_kin_count_by_name, inventory_context_from, pick_for_gesture,
    resolve_inventory_ref, state_of, AggregateScope, Gesture, InventoryContext, InventoryRefMatch,
    ResolveInventoryRefOptions, ResolvedInventoryPool,
};
use quilltap_core::inventory::types::ResolvedInventoryItem;
use quilltap_core::services::inventory_change_effects::{
    describe_inventory_change, record_pending_inventory_change,
    strike_from_pending_inventory_change, InventoryChangeProvenance, PendingWho,
};
use serde_json::{json, Map, Value};

/// The row count the oracle's corpus emits (asserted, so a silently truncated
/// regen cannot pass).
const EXPECTED_ROWS: usize = 87; // P4.169: +1 insertion-order row

fn item(v: &Value) -> ResolvedInventoryItem {
    ResolvedInventoryItem(v.as_object().expect("an item literal").clone())
}

fn items(v: &Value) -> Vec<ResolvedInventoryItem> {
    v.as_array()
        .map(|a| a.iter().map(item).collect())
        .unwrap_or_default()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn pool_of(spec: &Value) -> (ResolvedInventoryPool, HashSet<String>) {
    let own: HashSet<String> = strings(&spec["own"]).into_iter().collect();
    let pool = ResolvedInventoryPool {
        items: items(&spec["items"]),
        owns: Box::new(move |id| own.contains(id)),
    };
    let equipped: HashSet<String> = strings(&spec["equipped"]).into_iter().collect();
    (pool, equipped)
}

fn opts_of(v: &Value) -> ResolveInventoryRefOptions {
    ResolveInventoryRefOptions {
        include_archetypes: v.get("includeArchetypes").and_then(Value::as_bool) == Some(true),
        include_archived: v.get("includeArchived").and_then(Value::as_bool) == Some(true),
    }
}

fn match_value(m: &InventoryRefMatch) -> Value {
    json!({ "item": m.item.to_value(), "equipped": m.equipped, "own": m.own })
}

fn change_of(v: &Value) -> InventoryChange {
    InventoryChange {
        equipped: items(&v["equipped"]),
        unequipped: items(&v["unequipped"]),
        picked_up: v.get("pickedUp").map(items),
        given: v.get("given").map(|g| InventoryGive {
            item: item(&g["item"]),
            to_name: g["toName"].as_str().unwrap_or_default().to_string(),
        }),
    }
}

fn provenance_of(s: &str) -> InventoryChangeProvenance {
    match s {
        "operator" => InventoryChangeProvenance::Operator,
        "character" => InventoryChangeProvenance::Character,
        "pascal" => InventoryChangeProvenance::Pascal,
        other => panic!("unknown provenance {other}"),
    }
}

fn run_steps(steps: &Value) -> PendingInventoryMap {
    let mut pending = PendingInventoryMap::new();
    for step in steps.as_array().expect("steps") {
        let who = &step["who"];
        record_pending_inventory_change(
            &mut pending,
            PendingWho {
                user_id: who["userId"].as_str().unwrap(),
                chat_id: who["chatId"].as_str().unwrap(),
                character_id: who["characterId"].as_str().unwrap(),
            },
            &change_of(&step["change"]),
            provenance_of(step["provenance"].as_str().unwrap()),
        );
    }
    pending
}

/// `Array.from(map.entries())` — insertion order.
fn entries(pending: &PendingInventoryMap) -> Value {
    Value::Array(
        pending
            .iter()
            .map(|(k, v)| json!([k, serde_json::to_value(v).unwrap()]))
            .collect(),
    )
}

fn run_row(row: &Value) -> Value {
    let f = row["fn"].as_str().unwrap();
    match f {
        "resolve" => {
            let (pool, eq) = pool_of(&row["pool"]);
            let m = resolve_inventory_ref(
                row["ref"].as_str().unwrap(),
                &pool,
                &eq,
                opts_of(&row["opts"]),
            );
            Value::Array(m.iter().map(match_value).collect())
        }
        "state" => {
            let (pool, eq) = pool_of(&row["pool"]);
            let m = resolve_inventory_ref(
                row["ref"].as_str().unwrap(),
                &pool,
                &eq,
                opts_of(&row["opts"]),
            );
            Value::String(state_of(&m).as_str().to_string())
        }
        "kin" => {
            let (pool, eq) = pool_of(&row["pool"]);
            json!(equipped_kin_count(row["ref"].as_str().unwrap(), &pool, &eq))
        }
        "kinByName" => {
            let (pool, eq) = pool_of(&row["pool"]);
            json!(equipped_kin_count_by_name(
                row["name"].as_str().unwrap(),
                &pool,
                &eq
            ))
        }
        "pick" => {
            let (pool, eq) = pool_of(&row["pool"]);
            let m = resolve_inventory_ref(
                row["ref"].as_str().unwrap(),
                &pool,
                &eq,
                opts_of(&row["opts"]),
            );
            let gesture = match row["action"].as_str().unwrap() {
                "equip" => Gesture::Equip,
                _ => Gesture::Unequip,
            };
            pick_for_gesture(&m, gesture)
                .map(match_value)
                .unwrap_or(Value::Null)
        }
        "context" => {
            let (pool, eq) = pool_of(&row["pool"]);
            let chat_id = row["chatId"].as_str();
            let ctx = inventory_context_from("c", chat_id, pool, eq);
            let reads: Vec<Value> = strings(&row["refs"])
                .iter()
                .map(|r| {
                    json!({
                        "ref": r,
                        "state": ctx.state(r).as_str(),
                        "item": ctx.item(r).map(|i| i.to_value()).unwrap_or(Value::Null),
                        "kin": ctx.equipped_kin_count(r),
                    })
                })
                .collect();
            let mut scopes = Map::new();
            for scope in [
                AggregateScope::Equipped,
                AggregateScope::Owned,
                AggregateScope::Reachable,
            ] {
                scopes.insert(
                    scope.as_str().to_string(),
                    serde_json::to_value(ctx.scope_objects(scope)).unwrap(),
                );
            }
            json!({
                "characterId": ctx.character_id(),
                "chatId": ctx.chat_id(),
                "reads": reads,
                "scopes": Value::Object(scopes),
            })
        }
        "describe" => describe_inventory_change(
            row["characterName"].as_str().unwrap(),
            &change_of(&row["change"]),
        )
        .map(Value::String)
        .unwrap_or(Value::Null),
        "record" => entries(&run_steps(&row["steps"])),
        "strike" => {
            let mut pending = run_steps(&row["steps"]);
            strike_from_pending_inventory_change(
                &mut pending,
                row["key"].as_str().unwrap(),
                row["itemId"].as_str().unwrap(),
            );
            entries(&pending)
        }
        other => panic!("unknown fn {other}"),
    }
}

#[test]
fn inventory_refs_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_INVENTORY_REFS") else {
        eprintln!("SKIP: set QT_ORACLE_INVENTORY_REFS to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut count = 0usize;
    let mut red: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap_or("<no-id>").to_string();
        let f = row["fn"].as_str().unwrap_or_default();
        let got = run_row(&row);
        let want = row["out"].clone();
        let (g, w) = (
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&want).unwrap(),
        );
        if g != w {
            eprintln!("RED {f}/{id}\n  got:  {g}\n  want: {w}");
            red.push(format!("{f}/{id}"));
        }
        count += 1;
    }
    assert_eq!(
        count, EXPECTED_ROWS,
        "the oracle's row count moved — regenerate and update"
    );
    assert!(red.is_empty(), "{} of {count} rows red: {red:?}", red.len());
    eprintln!("OK: inventory-refs matched oracle ({count} rows).");
}
