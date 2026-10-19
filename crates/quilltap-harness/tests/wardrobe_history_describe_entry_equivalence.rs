//! P4.D315 Tier 1 item 14 — tier-1 differential: `quilltap_core::
//! wardrobe_history` (`resolve_history_slots`, `stale_items_of`,
//! `summarize_history_slots`, `changed_by_label`) against v4's REAL
//! `lib/wardrobe/history/describe-entry.ts` at the `e8e17b2d9` pin.
//!
//! Both sides build their pools from the SAME layers in
//! `harness/oracle/fixtures/wardrobe-history-describe-entry.json` — v4 through
//! its REAL `buildWearablePool`, v5 through `build_wearable_pool` — so no stub
//! pool stands in for either. Per slot case: the resolved slots compared as
//! JSON BYTES (key order — `serde_json::to_string` both sides), the stale
//! items as bytes, the summary exact (its JS-whitespace edges: U+FEFF trimmed,
//! U+0085 kept). Per label case (every source × actor × actorName × toolName ×
//! selfLabel): the label exact — all thirteen labels and v4's own eleven
//! expectations (`describe-entry.test.ts:43`) among them.
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   $N/npx tsx $V5W/harness/oracle/cases/wardrobe-history-describe-entry.ts \
//!     > /tmp/oracle-wh-describe.ndjson
//! Run:
//!   QT_ORACLE_WH_DESCRIBE=/tmp/oracle-wh-describe.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_history_describe_entry_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use quilltap_core::db::chat_wardrobe_history::{ChatWardrobeHistoryRow, WardrobeHistorySource};
use quilltap_core::wardrobe::Slots;
use quilltap_core::wardrobe_history::{
    changed_by_label, resolve_history_slots, stale_items_of, summarize_history_slots,
};
use quilltap_core::wearable_pool::{build_wearable_pool, WardrobeTierMounts, WearablePool};
use serde_json::Value;

fn layer(v: &Value, k: &str) -> Vec<Value> {
    v[k].as_array().cloned().unwrap_or_default()
}

#[test]
fn describe_entry_matches_oracle() {
    let Some(oracle_path) = std::env::var_os("QT_ORACLE_WH_DESCRIBE") else {
        eprintln!("SKIP: set QT_ORACLE_WH_DESCRIBE (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/wardrobe-history-describe-entry.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let character_id = spec["characterId"].as_str().unwrap();
    let pools: HashMap<String, WearablePool> = spec["pools"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, layers)| {
            (
                name.clone(),
                build_wearable_pool(
                    character_id,
                    WardrobeTierMounts::default(),
                    layer(layers, "own"),
                    layer(layers, "group"),
                    layer(layers, "project"),
                    layer(layers, "general"),
                ),
            )
        })
        .collect();
    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    let slot_cases = spec["slotCases"].as_array().unwrap();
    let theirs_slots: Vec<&Value> = lines.iter().filter(|l| l["kind"] == "slots").collect();
    assert_eq!(theirs_slots.len(), slot_cases.len(), "slot case count");
    for (c, v4) in slot_cases.iter().zip(theirs_slots) {
        let name = c["name"].as_str().unwrap();
        assert_eq!(v4["name"], c["name"]);
        let pool = &pools[c["pool"].as_str().unwrap()];
        let resolved = resolve_history_slots(pool, &Slots::from_value(Some(&c["slots"])));
        assert_eq!(
            serde_json::to_string(&resolved).unwrap(),
            serde_json::to_string(&v4["resolved"]).unwrap(),
            "[{name}] resolved slots"
        );
        assert_eq!(
            serde_json::to_string(&stale_items_of(&resolved)).unwrap(),
            serde_json::to_string(&v4["stale"]).unwrap(),
            "[{name}] stale items"
        );
        assert_eq!(
            summarize_history_slots(&resolved),
            v4["summary"].as_str().unwrap(),
            "[{name}] summary"
        );
    }

    let label_cases = spec["labelCases"].as_array().unwrap();
    let theirs_labels: Vec<&Value> = lines.iter().filter(|l| l["kind"] == "label").collect();
    assert_eq!(theirs_labels.len(), label_cases.len(), "label case count");
    let mut seen = BTreeSet::new();
    for (i, (c, v4)) in label_cases.iter().zip(theirs_labels).enumerate() {
        let entry = &c["entry"];
        let source: WardrobeHistorySource =
            serde_json::from_value(entry["source"].clone()).unwrap();
        let row = ChatWardrobeHistoryRow {
            id: String::new(),
            chat_id: String::new(),
            character_id: entry["characterId"].as_str().unwrap().to_string(),
            entry_index: 0,
            slots: Slots::fresh(),
            slots_hash: String::new(),
            source: source.as_str().to_string(),
            actor_character_id: entry["actorCharacterId"].as_str().map(str::to_string),
            actor_tool_name: entry["actorToolName"].as_str().map(str::to_string),
            is_current: false,
            created_at: String::new(),
        };
        let label = changed_by_label(
            &row,
            c["actorName"].as_str(),
            c["selfLabel"].as_str().unwrap(),
        );
        assert_eq!(label, v4["label"].as_str().unwrap(), "[label #{i}] {c}");
        seen.insert(label);
    }
    // Every one of the thirteen label FORMS is exercised (the parameterised
    // ones by a representative).
    for want in [
        "the operator",
        "the operator, as Madeline",
        "the operator, as Jeeves",
        "Pascal",
        "Pascal's dress_for_dinner",
        "Pascal (rolled by Jeeves)",
        "Pascal's dress_for_dinner (rolled by Jeeves)",
        "Madeline",
        "the character",
        "Jeeves",
        "another character",
        "chat opened",
        "joined the chat",
        "merged",
        "before history was kept",
    ] {
        assert!(seen.contains(want), "label form never produced: {want}");
    }
    eprintln!(
        "OK: describe-entry matched v4 on {} slot cases and {} labels.",
        slot_cases.len(),
        label_cases.len()
    );
}
