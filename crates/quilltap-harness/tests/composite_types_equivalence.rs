//! Tier-1 differential (P4.D279, the wardrobe keystone — v4 `4a38222e6`): the
//! composite type union and the four pure slot/composite helpers.
//!
//! Diffs [`quilltap_core::wardrobe`]'s `union_types` / `build_composite_types`
//! against v4's REAL `lib/wardrobe/composite-types.ts`, and `is_composite` /
//! `add_id_to_slot` / `remove_id_from_slot` / `equipped_slots_equal` against
//! the REAL `lib/schemas/wardrobe.types.ts`, exact (string arrays by value, slot
//! bags byte-for-byte as v4's `JSON.stringify`). The corpus carries the
//! `buildCompositeTypes` describe of v4's `item-mutations.test.ts` case-for-case
//! plus the implicit shapes: WIDEN rows (designated ⊂ union; designated adding
//! a slot), empty components, duplicate ids through add/remove, unknown slot
//! names filtered to the canonical five, a pre-hair four-key bag, and
//! order-sensitive equality.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/composite-types.ts \
//!     > /tmp/oracle-composite-types.ndjson
//! Run:
//!   QT_ORACLE_COMPOSITE_TYPES=/tmp/oracle-composite-types.ndjson \
//!     cargo test -p quilltap-harness --test composite_types_equivalence -- --nocapture

use quilltap_core::wardrobe::{
    add_id_to_slot, build_composite_types, equipped_slots_equal, is_composite, remove_id_from_slot,
    union_types, Slots, WARDROBE_SLOT_TYPES,
};
use serde::Deserialize;
use serde_json::Value;

/// The corpus's own marker — a stale NDJSON from before this lane cannot pass
/// quietly (`oracle-regen-silent-stale-pass`).
const BASELINE: &str = "p4.d279-composite-types";

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "meta")]
    Meta {
        baseline: String,
        #[serde(rename = "slotTypes")]
        slot_types: Vec<String>,
    },
    #[serde(rename = "union")]
    Union {
        id: String,
        components: Vec<Vec<String>>,
        out: Vec<String>,
    },
    #[serde(rename = "build")]
    Build {
        id: String,
        components: Vec<Vec<String>>,
        /// `null` = v4's omitted argument (the `[]` default) — a distinct arm
        /// from an explicit empty list, driven as such on the oracle side.
        designated: Option<Vec<String>>,
        out: Vec<String>,
    },
    #[serde(rename = "is_composite")]
    IsComposite {
        id: String,
        #[serde(rename = "componentItemIds")]
        component_item_ids: Option<Vec<String>>,
        out: bool,
    },
    #[serde(rename = "add")]
    Add {
        id: String,
        slots: Value,
        slot: String,
        #[serde(rename = "itemId")]
        item_id: String,
        out: Value,
    },
    #[serde(rename = "remove")]
    Remove {
        id: String,
        slots: Value,
        slot: String,
        #[serde(rename = "itemId")]
        item_id: Option<String>,
        out: Value,
    },
    #[serde(rename = "equal")]
    Equal {
        id: String,
        a: Value,
        b: Value,
        out: bool,
    },
}

/// Byte-compare a slot bag against v4's `JSON.stringify` — `serde_json::Value`
/// equality is key-order-blind, and the five-key canonical order is part of
/// the contract (`serde-json-map-eq-ignores-order`).
fn assert_bag(got: &Slots, want: &Value, label: &str) {
    assert_eq!(
        serde_json::to_string(&got.to_value()).unwrap(),
        serde_json::to_string(want).unwrap(),
        "{label}"
    );
}

#[test]
fn composite_types_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_COMPOSITE_TYPES") else {
        eprintln!("SKIP: set QT_ORACLE_COMPOSITE_TYPES to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut saw_meta = false;
    let (mut unions, mut builds, mut composites, mut adds, mut removes, mut equals) =
        (0, 0, 0, 0, 0, 0);
    // The WIDEN rule needs both directions on the corpus: a designated set the
    // union already covers, and one that adds a slot.
    let (mut widen_subset, mut widen_adds) = (0, 0);
    // Both arms of `removeIdFromSlot`: an id filtered, and the slot cleared.
    let (mut remove_by_id, mut remove_clear) = (0, 0);
    // Both verdicts of the equality.
    let (mut equal_true, mut equal_false) = (0, 0);
    // The pre-hair four-key bag must reach the slot helpers.
    let mut pre_hair_bags = 0;

    let canonical: Vec<String> = WARDROBE_SLOT_TYPES.iter().map(|s| s.to_string()).collect();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("oracle row") {
            Row::Meta {
                baseline,
                slot_types,
            } => {
                assert_eq!(
                    baseline, BASELINE,
                    "the oracle NDJSON predates this lane — regenerate it"
                );
                assert_eq!(slot_types, canonical, "WARDROBE_SLOT_TYPES");
                saw_meta = true;
            }

            Row::Union {
                id,
                components,
                out,
            } => {
                let got = union_types(components.iter().map(Vec::as_slice));
                assert_eq!(got, out, "union '{id}'");
                unions += 1;
            }

            Row::Build {
                id,
                components,
                designated,
                out,
            } => {
                let designated_slice = designated.as_deref().unwrap_or(&[]);
                let got = build_composite_types(&components, designated_slice);
                assert_eq!(got, out, "build '{id}'");
                // WIDEN, never narrow: every component slot and every canonical
                // designated slot survives.
                let union = union_types(components.iter().map(Vec::as_slice));
                for s in union.iter().chain(designated_slice.iter()) {
                    if canonical.contains(s) {
                        assert!(got.contains(s), "build '{id}': dropped slot {s}");
                    }
                }
                if let Some(d) = &designated {
                    let known: Vec<&String> = d.iter().filter(|s| canonical.contains(*s)).collect();
                    if !known.is_empty() && known.iter().all(|s| union.contains(*s)) {
                        widen_subset += 1;
                    }
                    if known.iter().any(|s| !union.contains(*s)) {
                        widen_adds += 1;
                    }
                }
                builds += 1;
            }

            Row::IsComposite {
                id,
                component_item_ids,
                out,
            } => {
                let ids = component_item_ids.unwrap_or_default();
                assert_eq!(is_composite(&ids), out, "is_composite '{id}'");
                composites += 1;
            }

            Row::Add {
                id,
                slots,
                slot,
                item_id,
                out,
            } => {
                if slots.get("hair").is_none() {
                    pre_hair_bags += 1;
                }
                let current = Slots::from_value(Some(&slots));
                let got = add_id_to_slot(&current, &slot, &item_id);
                assert_bag(&got, &out, &format!("add '{id}'"));
                // A copy, never a mutation of the input.
                assert_eq!(
                    current,
                    Slots::from_value(Some(&slots)),
                    "add '{id}': input mutated"
                );
                adds += 1;
            }

            Row::Remove {
                id,
                slots,
                slot,
                item_id,
                out,
            } => {
                if slots.get("hair").is_none() {
                    pre_hair_bags += 1;
                }
                let current = Slots::from_value(Some(&slots));
                let got = remove_id_from_slot(&current, &slot, item_id.as_deref());
                assert_bag(&got, &out, &format!("remove '{id}'"));
                assert_eq!(
                    current,
                    Slots::from_value(Some(&slots)),
                    "remove '{id}': input mutated"
                );
                match item_id {
                    Some(_) => remove_by_id += 1,
                    None => remove_clear += 1,
                }
                removes += 1;
            }

            Row::Equal { id, a, b, out } => {
                if a.get("hair").is_none() || b.get("hair").is_none() {
                    pre_hair_bags += 1;
                }
                let (a, b) = (Slots::from_value(Some(&a)), Slots::from_value(Some(&b)));
                assert_eq!(equipped_slots_equal(&a, &b), out, "equal '{id}'");
                // Symmetric, as v4's slot-by-slot walk is.
                assert_eq!(equipped_slots_equal(&b, &a), out, "equal '{id}' (reversed)");
                if out {
                    equal_true += 1;
                } else {
                    equal_false += 1;
                }
                equals += 1;
            }
        }
    }

    assert!(saw_meta, "oracle file carries no meta row: {path}");
    // Shape assertions, not hand counts (`harness-corpus-shape-constants-rot`).
    assert!(unions >= 7, "union cases: {unions}");
    assert!(builds >= 10, "build cases: {builds}");
    assert!(
        widen_subset >= 2,
        "WIDEN designated-subset rows: {widen_subset}"
    );
    assert!(widen_adds >= 3, "WIDEN designated-adds rows: {widen_adds}");
    assert!(composites >= 5, "is_composite cases: {composites}");
    assert!(adds >= 6, "add cases: {adds}");
    assert!(removes >= 8, "remove cases: {removes}");
    assert!(remove_by_id >= 4, "remove-by-id arms: {remove_by_id}");
    assert!(remove_clear >= 3, "remove-clear arms: {remove_clear}");
    assert!(equals >= 8, "equal cases: {equals}");
    assert!(equal_true >= 3, "equal=true verdicts: {equal_true}");
    assert!(equal_false >= 4, "equal=false verdicts: {equal_false}");
    assert!(
        pre_hair_bags >= 5,
        "pre-hair four-key bags: {pre_hair_bags}"
    );
    eprintln!(
        "OK: composite-types matched oracle ({unions} union / {builds} build / {composites} \
         is_composite / {adds} add / {removes} remove / {equals} equal cases)."
    );
}
