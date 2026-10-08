//! P4.D264 — tier-1 EXACT differential for the PURE half of the `.qtap`
//! import's phase 7e: `quilltap_core::services::quilltap_import::wardrobe_wear::
//! {remap_wardrobe_wear_rows, build_imported_wardrobe_item_id_map}` against v4's
//! REAL `remapWardrobeWearRows` / `buildImportedWardrobeItemIdMap`
//! (`lib/import/quilltap-import/import-wardrobe-wear.ts`, `3ee3b1342` #81).
//!
//! Every oracle line carries its own input (the incoming rows RAW — malformed
//! ones included — the item map, the two resolution tables, the live rows,
//! the pinned `now`); both sides mint with the same counter
//! (`00000000-0000-4000-8000-<12 digits, from 1>`), so no normalization at all.
//! Compared: the result rows byte-for-byte (key order included) and the four
//! counters; the id map entry for entry, in insertion order.
//!
//! Regenerate (Node 24, from the v4 checkout at the pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx "$V5W/harness/oracle/cases/wardrobe-wear-import-remap.ts" \
//!     > /tmp/oracle-wardrobe-wear-import-remap.ndjson
//! then:
//!   QT_ORACLE_WARDROBE_WEAR_IMPORT_REMAP=/tmp/oracle-wardrobe-wear-import-remap.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_wear_import_remap_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::db::wardrobe_wear_stats::WardrobeWearStatsRow;
use quilltap_core::services::quilltap_import::wardrobe_wear::{
    build_imported_wardrobe_item_id_map, remap_wardrobe_wear_rows, RemapContext,
};
use serde_json::{json, Value};

fn pairs(v: &Value) -> Vec<(String, Option<String>)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p[0].as_str().unwrap().to_string(),
                p[1].as_str().map(str::to_string),
            )
        })
        .collect()
}

#[test]
fn wardrobe_wear_import_remap_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_WARDROBE_WEAR_IMPORT_REMAP") else {
        eprintln!("SKIP: QT_ORACLE_WARDROBE_WEAR_IMPORT_REMAP unset");
        return;
    };
    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(!lines.is_empty(), "the oracle wrote no lines");

    let mut failures = Vec::new();
    let (mut remaps, mut maps) = (0usize, 0usize);
    for line in &lines {
        let name = line["name"].as_str().unwrap();
        let input = &line["input"];
        match line["kind"].as_str().unwrap() {
            "remap" => {
                remaps += 1;
                let item_ids: HashMap<String, String> = pairs(&input["itemIds"])
                    .into_iter()
                    .map(|(k, v)| (k, v.unwrap()))
                    .collect();
                let wearers: HashMap<String, Option<String>> =
                    pairs(&input["wearers"]).into_iter().collect();
                let chats: HashMap<String, Option<String>> =
                    pairs(&input["chats"]).into_iter().collect();
                let existing: Vec<WardrobeWearStatsRow> =
                    serde_json::from_value(input["existing"].clone()).expect("live rows");
                let incoming = input["incoming"].as_array().unwrap().clone();
                let resolve_wearer = |id: &str| wearers.get(id).cloned().flatten();
                let resolve_chat = |id: &str| chats.get(id).cloned().flatten();
                let mut n = 0u64;
                let mut mint = || {
                    n += 1;
                    format!("00000000-0000-4000-8000-{n:012}")
                };
                let got = remap_wardrobe_wear_rows(
                    &incoming,
                    &mut RemapContext {
                        item_ids: &item_ids,
                        resolve_wearer: &resolve_wearer,
                        resolve_chat: &resolve_chat,
                        existing: &existing,
                        mint_id: &mut mint,
                        now: input["now"].as_str().unwrap(),
                    },
                );
                let got = json!({
                    "rows": got.rows,
                    "droppedMissingItem": got.dropped_missing_item,
                    "droppedInvalid": got.dropped_invalid,
                    "foldedWearers": got.folded_wearers,
                    "clearedChats": got.cleared_chats,
                });
                let (g, w) = (got.to_string(), line["result"].to_string());
                if g != w {
                    failures.push(format!("[{name}]\n  rust:   {g}\n  oracle: {w}"));
                } else {
                    println!("OK {name}");
                }
            }
            "map" => {
                maps += 1;
                let as_pairs = |v: &Value| -> Vec<(String, String)> {
                    pairs(v).into_iter().map(|(k, v)| (k, v.unwrap())).collect()
                };
                let got = build_imported_wardrobe_item_id_map(
                    input["documents"].as_array().unwrap(),
                    &as_pairs(&input["mountPoints"]),
                    &as_pairs(&input["wardrobeItems"]),
                );
                let g =
                    json!(got.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>()).to_string();
                let w = line["map"].to_string();
                if g != w {
                    failures.push(format!("[{name}]\n  rust:   {g}\n  oracle: {w}"));
                } else {
                    println!("OK {name}");
                }
            }
            other => panic!("unknown oracle kind {other}"),
        }
    }
    assert_eq!((remaps, maps), (11, 2), "the corpus's case counts");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
