//! P4.D87 tier-1 differential: the equipped-outfit hash (v4
//! `lib/wardrobe/outfit-hash.ts` `hashEquippedSlots` + `hasEquippedItems`) vs
//! `quilltap_core::wardrobe::{hash_equipped_slots, has_equipped_items}`.
//!
//! Pins the FIVE-key normalized preimage exactly (v4 `4423ad10` added `hair`
//! unconditionally — one accepted cache miss per chat, no conditional key
//! omission), including the identity that makes the miss a one-time event: a
//! four-key legacy row and its explicit `hair: []` equivalent hash IDENTICALLY,
//! and layering order within a slot stays significant. `hasEquippedItems` is
//! diffed alongside — a hair-only outfit now counts as equipped.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/outfit-hash.ts > /tmp/oracle-outfit-hash.ndjson
//! Run:
//!   QT_ORACLE_OUTFIT_HASH=/tmp/oracle-outfit-hash.ndjson \
//!     cargo test -p quilltap-harness --test outfit_hash_equivalence
use quilltap_core::wardrobe::{has_equipped_items, hash_equipped_slots, Slots};
use serde_json::Value;

const A: &str = "a0000000-0000-4000-8000-000000000001";
const B: &str = "a0000000-0000-4000-8000-000000000002";
const H: &str = "a0000000-0000-4000-8000-000000000003";

/// The same corpus the oracle case walks, by name (raw stored-shape JSON).
fn case_slots(name: &str) -> Option<Value> {
    let v = match name {
        "null" => return None,
        "empty-object" => serde_json::json!({}),
        "legacy-four-key" => {
            serde_json::json!({ "top": [A], "bottom": [], "footwear": [], "accessories": [] })
        }
        "five-key-equivalent" => serde_json::json!({
            "top": [A], "bottom": [], "footwear": [], "accessories": [], "hair": []
        }),
        "hair-only" => serde_json::json!({
            "top": [], "bottom": [], "footwear": [], "accessories": [], "hair": [H]
        }),
        "layered-ab" => serde_json::json!({
            "top": [A, B], "bottom": [], "footwear": [], "accessories": [], "hair": []
        }),
        "layered-ba" => serde_json::json!({
            "top": [B, A], "bottom": [], "footwear": [], "accessories": [], "hair": []
        }),
        "full-five-slot" => serde_json::json!({
            "top": [A], "bottom": [B], "footwear": [A], "accessories": [B], "hair": [H]
        }),
        other => panic!("unknown oracle case name: {other}"),
    };
    Some(v)
}

#[test]
fn outfit_hash_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_OUTFIT_HASH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_OUTFIT_HASH to the oracle NDJSON (see header).");
            return;
        }
    };
    let body = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let mut rows = 0usize;
    let mut hash_by_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let name = row["name"].as_str().expect("name").to_string();
        let want_hash = row["hash"].as_str().expect("hash").to_string();
        let want_has = row["has"].as_bool().expect("has");

        let raw = case_slots(&name);
        let slots = raw.as_ref().map(|v| Slots::from_value(Some(v)));
        let got_hash = hash_equipped_slots(slots.as_ref());
        let got_has = has_equipped_items(slots.as_ref());
        assert_eq!(got_hash, want_hash, "{name}: hash diverged");
        assert_eq!(got_has, want_has, "{name}: hasEquippedItems diverged");
        hash_by_name.insert(name, want_hash);
        rows += 1;
    }
    assert_eq!(rows, 8, "corpus shape moved — update the case table");

    // The one-time-miss identity: legacy four-key ≡ explicit five-key.
    assert_eq!(
        hash_by_name["legacy-four-key"], hash_by_name["five-key-equivalent"],
        "a pre-hair row must hash identically to its five-key equivalent"
    );
    // Layering order stays significant.
    assert_ne!(
        hash_by_name["layered-ab"], hash_by_name["layered-ba"],
        "slot layering order must stay significant"
    );

    eprintln!("OK: outfit-hash matched oracle ({rows} cases).");
}
