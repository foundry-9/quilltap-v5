//! Tier-1 differential (P4.D279, keystone C1 §5): the pure half of v4's
//! `lib/wardrobe/wear-ops.ts` + `lib/wardrobe/wearable.ts` at `4a38222e6`.
//!
//! Diffs [`quilltap_core::wear_ops`] (`find_in_pool`, `resolve_wearable`,
//! `wardrobe_item_not_found_message`) and [`quilltap_core::wearable`]
//! (`archived_wear_message`, `wear_refusal`, `newly_worn_archived_items`)
//! against v4's REAL exports over a pool built by both sides from the SAME
//! layers (the oracle's `meta` row carries them), field-for-field: the three
//! refusal reasons and their sentences byte-exact, every mode × an archived
//! item, the covered / uncovered slot, the empty / whitespace references, an
//! archived OWN item found by title where an archived SHARED one is not.
//!
//! The persisted half (`wear_item` / `take_off_item`) is one
//! `apply_displacement` call each — `outfit_displacement_tier2_equivalence`
//! pins the write; the effect rule is unit-pinned in `wear_ops.rs`.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/wear-ops.ts > /tmp/oracle-wear-ops.ndjson
//! Run:
//!   QT_ORACLE_WEAR_OPS=/tmp/oracle-wear-ops.ndjson \
//!     cargo test -p quilltap-harness --test wear_ops_equivalence -- --nocapture

use quilltap_core::wardrobe::Slots;
use quilltap_core::wear_ops::{
    find_in_pool, resolve_wearable, wardrobe_item_not_found_message, PutOnMode, WearableRef,
    WearableResolution,
};
use quilltap_core::wearable::{archived_wear_message, newly_worn_archived_items, wear_refusal};
use quilltap_core::wearable_pool::{build_wearable_pool, WardrobeTierMounts, WearablePool};
use serde_json::{json, Value};

const BASELINE: &str = "p4.d279-wear-ops";

fn str_opt(v: &Value) -> Option<&str> {
    v.as_str()
}

fn item_ref(v: &Value) -> WearableRef<'_> {
    WearableRef {
        item_id: str_opt(&v["itemId"]),
        item_title: str_opt(&v["itemTitle"]),
    }
}

fn pool_from_meta(meta: &Value) -> WearablePool {
    let layer =
        |k: &str| -> Vec<Value> { meta["layers"][k].as_array().cloned().unwrap_or_default() };
    build_wearable_pool(
        "char-1",
        WardrobeTierMounts::default(),
        layer("own"),
        layer("group"),
        layer("project"),
        layer("general"),
    )
}

#[test]
fn wear_ops_match_oracle() {
    let path = match std::env::var("QT_ORACLE_WEAR_OPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_WEAR_OPS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    assert!(!rows.is_empty(), "empty oracle");
    let meta = rows.iter().find(|r| r["kind"] == "meta").expect("meta row");
    assert_eq!(
        meta["baseline"], BASELINE,
        "stale oracle — regenerate from the pin"
    );
    let pool = pool_from_meta(meta);

    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for row in &rows {
        let kind = row["kind"].as_str().unwrap_or_default();
        let id = row["id"].as_str().unwrap_or_default();
        let got: Value = match kind {
            "meta" => continue,
            "not_found_message" => json!(wardrobe_item_not_found_message(
                str_opt(&row["itemId"]),
                str_opt(&row["itemTitle"])
            )),
            "find_in_pool" => find_in_pool(&pool, item_ref(&row["ref"]))
                .map(|i| i["id"].clone())
                .unwrap_or(Value::Null),
            "resolve_wearable" => {
                let mode = PutOnMode::parse(row["mode"].as_str().unwrap()).expect("mode");
                match resolve_wearable(&pool, item_ref(&row["ref"]), mode, str_opt(&row["slot"])) {
                    WearableResolution::Ok(item) => json!({ "ok": true, "item": item["id"] }),
                    WearableResolution::Refused { reason, message } => {
                        json!({ "ok": false, "reason": reason.as_str(), "message": message })
                    }
                }
            }
            "archived_wear_message" => json!(archived_wear_message(row["title"].as_str().unwrap())),
            "wear_refusal" => wear_refusal(&row["item"])
                .map(Value::String)
                .unwrap_or(Value::Null),
            "newly_worn_archived" => {
                let items: Vec<Value> = vec![
                    meta["layers"]["own"][0].clone(),
                    meta["layers"]["own"][1].clone(),
                    meta["layers"]["own"][2].clone(),
                    meta["layers"]["general"][1].clone(),
                ];
                let slots =
                    (!row["slots"].is_null()).then(|| Slots::from_value(Some(&row["slots"])));
                json!(newly_worn_archived_items(&items, slots.as_ref())
                    .iter()
                    .map(|i| i["id"].clone())
                    .collect::<Vec<_>>())
            }
            other => panic!("unknown row kind {other}"),
        };
        assert_eq!(got, row["out"], "[{kind}:{id}] diverged");
        *counts.entry(kind.to_string()).or_insert(0) += 1;
    }
    eprintln!("OK: wear_ops matched oracle — {counts:?}");
    assert!(counts.get("resolve_wearable").copied().unwrap_or(0) >= 30);
}
