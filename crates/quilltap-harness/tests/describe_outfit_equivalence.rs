//! P4.D312 tier-1 differential: v4's markdown outfit description (`lib/wardrobe/
//! outfit-description.ts` `describeOutfit(slots, { omit })`, the hair-styling
//! precedence note of v4 `7458ff94e`) vs
//! `quilltap_core::wardrobe::describe_outfit_with_omit`.
//!
//! Both sides read the SAME corpus (`harness/oracle/fixtures/describe-outfit
//! .json`) so the inputs cannot drift, and the rendered strings are compared
//! EXACTLY. Before this family the leaf had eight v5 callers and no direct
//! oracle — only composition through the wardrobe tools, the avatar / scene
//! prompts and Aurora — so a second caller's change could pass as "the note".
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx "$V5W/harness/oracle/cases/describe-outfit.ts" > /tmp/oracle-describe-outfit.ndjson
//! Run:
//!   QT_ORACLE_DESCRIBE_OUTFIT=/tmp/oracle-describe-outfit.ndjson \
//!     cargo test -p quilltap-harness --test describe_outfit_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::wardrobe::{
    describe_outfit_with_omit, OutfitSlotName, OutfitSlotValues, HAIR_STYLING_PRECEDENCE_NOTE,
};
use serde_json::Value;

fn corpus_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/describe-outfit.json")
}

fn titles(slots: &Value, slot: &str) -> Vec<String> {
    slots[slot]
        .as_array()
        .unwrap_or_else(|| panic!("corpus slot `{slot}` is not an array"))
        .iter()
        .map(|v| v.as_str().expect("a slot title is a string").to_string())
        .collect()
}

fn slot_name(s: &str) -> OutfitSlotName {
    match s {
        "top" => OutfitSlotName::Top,
        "bottom" => OutfitSlotName::Bottom,
        "footwear" => OutfitSlotName::Footwear,
        "accessories" => OutfitSlotName::Accessories,
        "hair" => OutfitSlotName::Hair,
        other => panic!("corpus omit names an unknown slot `{other}`"),
    }
}

#[test]
fn describe_outfit_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DESCRIBE_OUTFIT") else {
        eprintln!("SKIP: set QT_ORACLE_DESCRIBE_OUTFIT to the oracle NDJSON (see header).");
        return;
    };
    let body = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read corpus"))
            .expect("corpus is JSON");
    let rows = corpus["rows"].as_array().expect("corpus rows");

    let mut want: HashMap<String, String> = HashMap::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line is JSON");
        let name = v["name"].as_str().expect("name").to_string();
        let out = v["out"].as_str().expect("out").to_string();
        assert!(
            want.insert(name.clone(), out).is_none(),
            "duplicate oracle row {name}"
        );
    }
    assert_eq!(
        want.len(),
        rows.len(),
        "the oracle and the corpus disagree on the row count — regenerate the oracle"
    );

    let mut failures = Vec::new();
    let mut noted = 0usize;
    for row in rows {
        let name = row["name"].as_str().expect("row name");
        let s = &row["slots"];
        let slots = OutfitSlotValues {
            top: titles(s, "top"),
            bottom: titles(s, "bottom"),
            footwear: titles(s, "footwear"),
            accessories: titles(s, "accessories"),
            hair: titles(s, "hair"),
        };
        let omit: Vec<OutfitSlotName> = row["omit"]
            .as_array()
            .expect("row omit")
            .iter()
            .map(|v| slot_name(v.as_str().expect("omit entry is a string")))
            .collect();
        let got = describe_outfit_with_omit(&slots, &omit);
        let expected = want
            .get(name)
            .unwrap_or_else(|| panic!("no oracle row for corpus row {name}"));
        if &got != expected {
            failures.push(format!(
                "[{name}]\n  rust:   {got:?}\n  oracle: {expected:?}"
            ));
        }
        if expected.contains(HAIR_STYLING_PRECEDENCE_NOTE) {
            noted += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "{} describeOutfit difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The family must exercise the note, or it proves nothing about it.
    assert!(noted > 0, "no oracle row carries the hair precedence note");
    eprintln!(
        "OK: describe_outfit — {} rows equal, {noted} carrying the hair precedence note.",
        rows.len()
    );
}
