//! Tier-1 differential for the W4.6b Aurora OUTFIT builders — the byte-exact
//! persona / opaque announcement bodies, diffed against v4's REAL exports
//! (`harness/oracle/cases/post-office-aurora.ts`).
//!
//! Covers `buildOpeningOutfitContent` / `buildOpeningOutfitOpaqueContent` /
//! `buildOutfitChangeContent` / `buildOutfitChangeOpaqueContent` over the
//! describeOutfit branches (empty / one item / full / accessories-only /
//! multi-per-slot) and a Unicode/em-dash character name. The Core-whisper content
//! builders are proven in `context_feeders_leaves_equivalence`; the POST
//! functions' `chat_messages` rows are covered by the parent's central tier-3.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5/harness/oracle/cases/post-office-aurora.ts \
//!       > /tmp/oracle-po-aurora.ndjson
//! Run:
//!   QT_ORACLE_PO_AURORA=/tmp/oracle-po-aurora.ndjson \
//!     cargo test -p quilltap-harness --test post_office_aurora_equivalence

use serde_json::Value;

use quilltap_core::services::aurora_notifications::{
    build_opening_outfit_content, build_opening_outfit_opaque_content, build_outfit_change_content,
    build_outfit_change_opaque_content,
};
use quilltap_core::wardrobe::OutfitSlotValues;

fn slots(
    top: &[&str],
    bottom: &[&str],
    footwear: &[&str],
    accessories: &[&str],
) -> OutfitSlotValues {
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect();
    OutfitSlotValues {
        top: v(top),
        bottom: v(bottom),
        footwear: v(footwear),
        accessories: v(accessories),
        hair: Vec::new(),
    }
}

/// Reconstruct the `(characterName, outfit)` for one case id (mirrors the oracle).
fn case(id: &str) -> (&'static str, OutfitSlotValues) {
    match id {
        "empty" => ("Ada", slots(&[], &[], &[], &[])),
        "one-top" => ("Bea", slots(&["a blue silk blouse"], &[], &[], &[])),
        "full" => (
            "Cyrus",
            slots(
                &["a herringbone waistcoat"],
                &["grey flannel trousers"],
                &["oxford brogues"],
                &["a pocket watch", "a signet ring"],
            ),
        ),
        "accessories-only" => ("Dot", slots(&[], &[], &[], &["a long string of pearls"])),
        "multi-per-slot" => (
            "Ezra",
            slots(
                &["a greatcoat", "a woollen scarf"],
                &["a long tweed skirt"],
                &[],
                &[],
            ),
        ),
        "unicode-name" => (
            "Élodie — the Frankish",
            slots(&["une chemise blanche"], &[], &[], &[]),
        ),
        other => panic!("unknown aurora outfit case {other}"),
    }
}

fn rust_value(kind: &str, id: &str) -> Value {
    let (name, outfit) = case(id);
    let s = match kind {
        "opening_content" => build_opening_outfit_content(name, &outfit),
        "opening_opaque" => build_opening_outfit_opaque_content(name, &outfit),
        "change_content" => build_outfit_change_content(name, &outfit),
        "change_opaque" => build_outfit_change_opaque_content(name, &outfit),
        other => panic!("unknown aurora outfit kind {other}"),
    };
    Value::String(s)
}

#[test]
fn post_office_aurora_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PO_AURORA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("QT_ORACLE_PO_AURORA unset; skipping");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        let want = &row["value"];
        let got = rust_value(kind, id);
        assert_eq!(&got, want, "aurora outfit builder {kind}/{id} diverges");
        count += 1;
    }
    assert!(count > 0, "oracle had no rows");
    eprintln!("post-office-aurora: {count} rows matched");
}
