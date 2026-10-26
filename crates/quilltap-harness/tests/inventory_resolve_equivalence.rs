//! Tier-1 differential (P4.D324, the Inventory keystone — v4 `4fbdad186`):
//! resolution and the carried-map normaliser.
//!
//! Diffs [`quilltap_core::inventory::resolve`] (`with_inventory_defaults`,
//! `overridden_fields_of`, `resolve_item`, `inventory_display_name`,
//! `inventory_label`, `is_kin`, `is_equippable`, `kit_state`) and
//! [`quilltap_core::db::chats_inventory::normalize_equipped_inventory`]
//! against v4's REAL `lib/inventory/resolve.ts` + `normalizeEquippedInventory`
//! over `cases/inventory-resolve.ts`'s corpus. KEY ORDER is the contract: every
//! object result is compared as `serde_json::to_string` bytes against v4's
//! `JSON.stringify` (a `preserve_order` map's `==` is order-blind).
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/inventory-resolve.ts \
//!     > /tmp/oracle-inv-resolve.ndjson
//! Run:
//!   QT_ORACLE_INV_RESOLVE=/tmp/oracle-inv-resolve.ndjson \
//!     cargo test -p quilltap-harness --test inventory_resolve_equivalence -- --nocapture

use quilltap_core::db::chats_inventory::normalize_equipped_inventory;
use quilltap_core::inventory::resolve::{
    inventory_display_name, inventory_label, is_equippable, is_kin, kit_state,
    overridden_fields_of, resolve_item, with_inventory_defaults,
};
use quilltap_core::inventory::types::{InventoryRecord, KitState};
use serde_json::{json, Map, Value};

const BASELINE: &str = "p4.d324-inventory-resolve";

fn record(v: &Value) -> InventoryRecord {
    InventoryRecord(v.as_object().cloned().unwrap_or_default())
}

fn kit_state_str(s: KitState) -> &'static str {
    match s {
        KitState::None => "none",
        KitState::Some => "some",
        KitState::All => "all",
    }
}

#[test]
fn inventory_resolve_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_INV_RESOLVE") else {
        eprintln!("SKIP: set QT_ORACLE_INV_RESOLVE to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut failures = Vec::new();
    let (mut rows, mut saw_meta) = (0usize, false);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap();
        let id = format!("{kind}/{}", row["id"].as_str().unwrap_or(""));
        let (got, want): (String, String) = match kind {
            "meta" => {
                saw_meta = true;
                assert_eq!(row["baseline"], BASELINE, "stale oracle");
                continue;
            }
            "defaults" => (
                serde_json::to_string(&with_inventory_defaults(&record(&row["record"]))).unwrap(),
                row["out"].as_str().unwrap().to_string(),
            ),
            "overridden" => (
                serde_json::to_string(&overridden_fields_of(&record(&row["record"]))).unwrap(),
                row["out"].as_str().unwrap().to_string(),
            ),
            "resolve" => {
                let parent = (!row["parent"].is_null()).then(|| record(&row["parent"]));
                let resolved = resolve_item(&record(&row["record"]), |pid| {
                    parent
                        .as_ref()
                        .filter(|p| p.0.get("id").and_then(Value::as_str) == Some(pid))
                });
                (
                    serde_json::to_string(&resolved).unwrap(),
                    row["out"].as_str().unwrap().to_string(),
                )
            }
            "display" => {
                let item = record(&row["item"]);
                (
                    json!({"display": inventory_display_name(&item), "label": inventory_label(&item)}).to_string(),
                    json!({"display": row["display"], "label": row["label"]}).to_string(),
                )
            }
            "kin" => {
                let a = record(&json!({"name": row["a"]}));
                let b = record(&json!({"name": row["b"]}));
                (json!(is_kin(&a, &b)).to_string(), row["out"].to_string())
            }
            "equippable" => (
                json!(is_equippable(&record(&row["item"]))).to_string(),
                row["out"].to_string(),
            ),
            "kit_state" => {
                let item_ids: Vec<String> = serde_json::from_value(row["itemIds"].clone()).unwrap();
                let equipped: Vec<InventoryRecord> = row["equipped"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(record)
                    .collect();
                let members = row["members"].as_object().unwrap().clone();
                let got = kit_state(&item_ids, &equipped, |m| members.get(m).map(record));
                (
                    json!(kit_state_str(got)).to_string(),
                    row["out"].to_string(),
                )
            }
            "normalize" => {
                let raw = row["raw"].get("value");
                let got: Map<String, Value> = normalize_equipped_inventory(raw)
                    .into_iter()
                    .map(|(k, ids)| (k, json!(ids)))
                    .collect();
                (
                    serde_json::to_string(&Value::Object(got)).unwrap(),
                    row["out"].as_str().unwrap().to_string(),
                )
            }
            other => panic!("unknown row kind {other}"),
        };
        rows += 1;
        if got != want {
            failures.push(format!("{id}\n  v5: {got}\n  v4: {want}"));
        }
    }
    assert!(saw_meta, "no meta row");
    eprintln!("inventory resolve: {rows} rows");
    assert!(rows > 90, "corpus too small");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
