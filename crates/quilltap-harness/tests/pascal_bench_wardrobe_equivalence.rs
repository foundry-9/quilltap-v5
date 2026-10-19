//! Tier-1 differential (P4.D319, v4 `5cb0e0190`): the proving bench's wardrobe
//! lens and the one wearable-reference rule, against v4's REAL
//! `lib/pascal/bench-wardrobe.ts` + `lib/wardrobe/wearable-ref.ts` at the
//! `e8e17b2d9` pin.
//!
//! - `lens` rows: a deterministic definition (`min === max`, no draw) dealt by
//!   BOTH sides' execution core with the row's `character` override, then
//!   [`resolve_bench_wardrobe_against`] over the RENDERED effects, against a
//!   pool both sides build from the SAME layers (the oracle's `meta` row). The
//!   effects and the resolution are compared as JSON TEXT — key order is the
//!   wire (`ref, found, reason, message` on a miss; `id, title, types, origin,
//!   imageFileId` on a hit). The not-found sentence is v4's re-spelled
//!   `Wardrobe item not found with title "<ref>"`; `take_off` resolves through
//!   `find_in_pool` (archived found), put-ons through `resolve_wearable`;
//!   `clear_slot` and non-wardrobe effects contribute nothing and keep their
//!   definition indices.
//! - `wearable_ref` rows: v4's seven `wearable-ref.test.ts` `it`s and two
//!   `matchWearableTitle` corners through [`quilltap_core::wardrobe::wearable_ref`].
//!
//! The lens's READ half (`resolve_bench_wardrobe` — the pool load, the WARN on a
//! failed read, the DEBUG) is pinned by the route family and the in-crate
//! tests; v4's fail-soft arm is reachable only by mock (its real
//! `loadWearablePool` never throws), so no oracle row exists for it.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/pascal-bench-wardrobe.ts > /tmp/oracle-pascal-bench-wardrobe.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_BENCH_WARDROBE=/tmp/oracle-pascal-bench-wardrobe.ndjson \
//!     cargo test -p quilltap-harness --test pascal_bench_wardrobe_equivalence -- --nocapture

use quilltap_core::pascal::bench_wardrobe::resolve_bench_wardrobe_against;
use quilltap_core::pascal::custom_tool_types::safe_parse;
use quilltap_core::pascal::custom_tools::execute_custom_tool;
use quilltap_core::pascal::dice::FixedBytes;
use quilltap_core::wardrobe::wearable_ref::{
    is_templated_wearable_ref, match_wearable_title, resolve_wearable_ref, RefTarget,
};
use quilltap_core::wearable_pool::{build_wearable_pool, WardrobeTierMounts, WearablePool};
use serde_json::Value;

/// The oracle's `NOW` — no lens row renders `{{now}}`, but the core takes one.
const NOW_MS: i64 = 1_788_004_800_000;

fn pool_from_meta(meta: &Value) -> WearablePool {
    let layers: Value =
        serde_json::from_str(meta["layers"].as_str().expect("layers text")).expect("layers JSON");
    let layer = |k: &str| -> Vec<Value> { layers[k].as_array().cloned().unwrap_or_default() };
    build_wearable_pool(
        meta["characterId"].as_str().expect("characterId"),
        WardrobeTierMounts::default(),
        layer("own"),
        layer("group"),
        layer("project"),
        layer("general"),
    )
}

#[test]
fn pascal_bench_wardrobe_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_BENCH_WARDROBE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PASCAL_BENCH_WARDROBE to the oracle NDJSON (see header)."
            );
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

    let meta = rows
        .iter()
        .find(|r| r["kind"] == "meta")
        .expect("a meta row");
    let pool = pool_from_meta(meta);

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let (mut lens, mut refs) = (0usize, 0usize);
    let (mut hits, mut misses) = (0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();

    for row in &rows {
        let id = row["id"].as_str().unwrap_or_default();
        match row["kind"].as_str() {
            Some("meta") => {}
            Some("lens") => {
                lens += 1;
                let raw: Value =
                    serde_json::from_str(row["inputJson"].as_str().unwrap()).expect("definition");
                let definition = safe_parse(&raw)
                    .unwrap_or_else(|e| panic!("lens '{id}': definition refused: {e:?}"));
                let result = rt
                    .block_on(execute_custom_tool(
                        &definition,
                        None,
                        None,
                        None,
                        None,
                        &mut FixedBytes::new(vec![]),
                        None,
                        None,
                        NOW_MS,
                        row["character"].as_str(),
                    ))
                    .unwrap_or_else(|e| panic!("lens '{id}': deal refused: {}", e.0));
                let effects = result.effects.unwrap_or_default();
                let got_effects = serde_json::to_string(&effects).unwrap();
                if got_effects != row["effects"].as_str().unwrap() {
                    failures.push(format!(
                        "lens '{id}' effects\n  v5: {got_effects}\n  v4: {}",
                        row["effects"]
                    ));
                }
                let resolution = resolve_bench_wardrobe_against(&pool, &effects);
                for entry in &resolution {
                    for item in entry["items"].as_array().into_iter().flatten() {
                        if item["found"].is_null() {
                            misses += 1;
                        } else {
                            hits += 1;
                        }
                    }
                }
                let got = serde_json::to_string(&resolution).unwrap();
                if got != row["resolution"].as_str().unwrap() {
                    failures.push(format!(
                        "lens '{id}' resolution\n  v5: {got}\n  v4: {}",
                        row["resolution"]
                    ));
                }
            }
            Some("wearable_ref") => {
                refs += 1;
                let pool: Vec<Value> = row["pool"].as_array().cloned().unwrap_or_default();
                let r = row["ref"].as_str().unwrap();
                let got = match row["fn"].as_str() {
                    Some("resolve") => resolve_wearable_ref(&pool, r)
                        .map(|v| Value::String(v.ref_id().to_string()))
                        .unwrap_or(Value::Null),
                    Some("matchTitle") => match_wearable_title(&pool, r)
                        .map(|v| Value::String(v.ref_id().to_string()))
                        .unwrap_or(Value::Null),
                    Some("isTemplated") => Value::Bool(is_templated_wearable_ref(r)),
                    other => panic!("wearable_ref '{id}': unknown fn {other:?}"),
                };
                if got != row["out"] {
                    failures.push(format!(
                        "wearable_ref '{id}'\n  v5: {got}\n  v4: {}",
                        row["out"]
                    ));
                }
            }
            other => panic!("unknown row kind {other:?}"),
        }
    }

    assert!(
        failures.is_empty(),
        "{} row(s) differ from the oracle:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Coverage, not a hand count: both arms of the lens and the ref rule.
    assert!(lens >= 20, "lens coverage thinned: {lens}");
    assert!(refs >= 9, "wearable_ref coverage thinned: {refs}");
    assert!(
        hits >= 5 && misses >= 5,
        "the lens corpus must see both hits and misses ({hits} / {misses})"
    );
    eprintln!("OK: bench wardrobe matched oracle ({lens} lens, {refs} wearable_ref rows).");
}
