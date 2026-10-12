//! Tier-1 differential (P4.D279 keystone, C1 §6): [`quilltap_core::slot_ops`]
//! against v4's REAL `lib/wardrobe/slot-ops.ts` (`9ecebf986`) — the pure math
//! of putting things on and taking them off.
//!
//! The corpus is v4's own `__tests__/unit/lib/wardrobe/slot-ops.test.ts`
//! case-for-case (the pure describes — the persisted `applyDisplacement` block
//! rides `outfit_displacement_tier2_equivalence`), plus every mode through
//! `computeDisplacedSlots` with and without its required `item` / `slot`,
//! `wornBundlesForMode` per mode, `analyzeComposite`'s `{cycles, truncated,
//! unresolved}` on a cycle / a truncated chain / an orphan / a leaf, and
//! `dissolveCompositesInSlots`'s `only` filter (one composite, both, a leaf,
//! the empty set).
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/slot-ops.ts > /tmp/oracle-slot-ops.ndjson
//! Run:
//!   QT_ORACLE_SLOT_OPS=/tmp/oracle-slot-ops.ndjson \
//!     cargo test -p quilltap-harness --test slot_ops_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};

use quilltap_core::db::wardrobe_wear_stats::WornBundle;
use quilltap_core::slot_ops::{
    add_item_to_slot, analyze_composite, compute_displaced_slots, dissolve_composite_to_leaves,
    dissolve_composites_in_slots, lay_leaves_into_slots, wear_item_into_slots, worn_bundles_for,
    worn_bundles_for_mode, ComputeDisplacedOptions, DisplacementMode, DissolvedLeaf,
    WearableLookup, WearableNode,
};
use quilltap_core::wardrobe::{is_composite, Slots};
use serde::Deserialize;
use serde_json::Value;

const BASELINE: &str = "p4.d279-slot-ops";

#[derive(Deserialize)]
struct WireSlots {
    top: Vec<String>,
    bottom: Vec<String>,
    footwear: Vec<String>,
    accessories: Vec<String>,
    #[serde(default)]
    hair: Vec<String>,
}

impl WireSlots {
    fn to_slots(&self) -> Slots {
        Slots {
            top: self.top.clone(),
            bottom: self.bottom.clone(),
            footwear: self.footwear.clone(),
            accessories: self.accessories.clone(),
            hair: self.hair.clone(),
        }
    }
}

#[derive(Deserialize)]
struct WireLeaf {
    id: String,
    slots: Vec<String>,
}

impl WireLeaf {
    fn to_leaf(&self) -> DissolvedLeaf {
        DissolvedLeaf {
            id: self.id.clone(),
            slots: self.slots.clone(),
        }
    }
}

#[derive(Deserialize)]
struct WireWornBundle {
    id: String,
    #[serde(rename = "leafIds")]
    leaf_ids: Vec<String>,
}

#[derive(Deserialize)]
struct WireAnalysis {
    leaves: Option<Vec<WireLeaf>>,
    cycles: usize,
    truncated: bool,
    unresolved: bool,
}

#[derive(Deserialize)]
struct WireInSlots {
    slots: WireSlots,
    #[serde(rename = "wornBundles")]
    worn_bundles: Vec<WireWornBundle>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "meta")]
    Meta { baseline: String, items: Vec<Value> },
    #[serde(rename = "is_composite")]
    IsComposite { id: String, out: bool },
    #[serde(rename = "is_composite_bare")]
    IsCompositeBare { node: Value, out: bool },
    #[serde(rename = "analyze")]
    Analyze {
        id: String,
        #[serde(rename = "itemId")]
        item_id: String,
        lookup: Option<Vec<String>>,
        out: WireAnalysis,
        leaves: Option<Vec<WireLeaf>>,
    },
    #[serde(rename = "lay")]
    Lay {
        id: String,
        #[serde(rename = "compositeId")]
        composite_id: String,
        leaves: Vec<WireLeaf>,
        current: WireSlots,
        #[serde(rename = "clearCoveredSlots")]
        clear_covered_slots: bool,
        out: WireSlots,
    },
    #[serde(rename = "wear")]
    Wear {
        id: String,
        #[serde(rename = "itemId")]
        item_id: String,
        current: WireSlots,
        lookup: Option<Vec<String>>,
        out: WireSlots,
    },
    #[serde(rename = "add")]
    Add {
        id: String,
        #[serde(rename = "itemId")]
        item_id: String,
        slot: String,
        current: WireSlots,
        lookup: Option<Vec<String>>,
        out: WireSlots,
    },
    #[serde(rename = "worn_for")]
    WornFor {
        id: String,
        #[serde(rename = "itemId")]
        item_id: String,
        lookup: Option<Vec<String>>,
        #[serde(rename = "onlySlot")]
        only_slot: Option<String>,
        out: Vec<WireWornBundle>,
    },
    #[serde(rename = "compute")]
    Compute {
        id: String,
        mode: String,
        #[serde(rename = "itemId")]
        item_id: Option<String>,
        slot: Option<String>,
        #[serde(rename = "removeId")]
        remove_id: Option<String>,
        lookup: Option<Vec<String>>,
        current: WireSlots,
        out: WireSlots,
        #[serde(rename = "inputUntouched")]
        input_untouched: bool,
        #[serde(rename = "wornBundles")]
        worn_bundles: Vec<WireWornBundle>,
    },
    #[serde(rename = "in_slots")]
    InSlots {
        id: String,
        current: WireSlots,
        lookup: Vec<String>,
        only: Option<Vec<String>>,
        out: WireInSlots,
        #[serde(rename = "sameObject")]
        same_object: bool,
    },
}

fn assert_slots(got: &Slots, want: &WireSlots, label: &str) {
    assert_eq!(got.top, want.top, "{label}: top");
    assert_eq!(got.bottom, want.bottom, "{label}: bottom");
    assert_eq!(got.footwear, want.footwear, "{label}: footwear");
    assert_eq!(got.accessories, want.accessories, "{label}: accessories");
    assert_eq!(got.hair, want.hair, "{label}: hair");
}

fn assert_worn_bundles(got: &[WornBundle], want: &[WireWornBundle], label: &str) {
    let got: Vec<(&str, &[String])> = got
        .iter()
        .map(|b| (b.id.as_str(), b.leaf_ids.as_slice()))
        .collect();
    let want: Vec<(&str, &[String])> = want
        .iter()
        .map(|b| (b.id.as_str(), b.leaf_ids.as_slice()))
        .collect();
    assert_eq!(got, want, "{label}: wornBundles");
}

fn lookup_of(
    universe: &HashMap<String, Value>,
    ids: Option<&Vec<String>>,
) -> Option<WearableLookup> {
    ids.map(|ids| {
        ids.iter()
            .map(|id| {
                (
                    id.clone(),
                    universe
                        .get(id)
                        .unwrap_or_else(|| panic!("corpus names an unknown item: {id}"))
                        .clone(),
                )
            })
            .collect()
    })
}

fn node_of(universe: &HashMap<String, Value>, id: &str) -> WearableNode {
    WearableNode::from_value(
        universe
            .get(id)
            .unwrap_or_else(|| panic!("corpus names an unknown item: {id}")),
    )
}

#[test]
fn slot_ops_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_SLOT_OPS") else {
        eprintln!("SKIP: set QT_ORACLE_SLOT_OPS to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut universe: HashMap<String, Value> = HashMap::new();
    let mut saw_meta = false;
    let (mut composites, mut analyzes, mut lays, mut wears, mut adds, mut worn_fors) =
        (0, 0, 0, 0, 0, 0);
    let (mut computes, mut in_slots) = (0, 0);
    let mut modes_seen: HashSet<String> = HashSet::new();
    let (mut fallback_rows, mut only_rows, mut flagged_analyses, mut non_empty_claims) =
        (0, 0, 0, 0);

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("oracle row") {
            Row::Meta { baseline, items } => {
                assert_eq!(
                    baseline, BASELINE,
                    "the oracle NDJSON predates this lane — regenerate it"
                );
                for it in items {
                    let id = it["id"].as_str().expect("corpus item id").to_string();
                    universe.insert(id, it);
                }
                saw_meta = true;
            }

            Row::IsComposite { id, out } => {
                let node = node_of(&universe, &id);
                assert_eq!(
                    is_composite(&node.component_item_ids),
                    out,
                    "isComposite '{id}'"
                );
                composites += 1;
            }

            Row::IsCompositeBare { node, out } => {
                let node = WearableNode::from_value(&node);
                assert_eq!(
                    is_composite(&node.component_item_ids),
                    out,
                    "isComposite bare"
                );
                composites += 1;
            }

            Row::Analyze {
                id,
                item_id,
                lookup,
                out,
                leaves,
            } => {
                let map = lookup_of(&universe, lookup.as_ref());
                let node = node_of(&universe, &item_id);
                let got = analyze_composite(&node, map.as_ref());
                let want_leaves: Option<Vec<DissolvedLeaf>> = out
                    .leaves
                    .map(|l| l.iter().map(WireLeaf::to_leaf).collect());
                assert_eq!(got.leaves, want_leaves, "analyze '{id}': leaves");
                assert_eq!(got.cycles, out.cycles, "analyze '{id}': cycles");
                assert_eq!(got.truncated, out.truncated, "analyze '{id}': truncated");
                assert_eq!(got.unresolved, out.unresolved, "analyze '{id}': unresolved");
                // `dissolveCompositeToLeaves` IS the analysis' leaves.
                let want_direct: Option<Vec<DissolvedLeaf>> =
                    leaves.map(|l| l.iter().map(WireLeaf::to_leaf).collect());
                assert_eq!(
                    dissolve_composite_to_leaves(&node, map.as_ref()),
                    want_direct,
                    "dissolve '{id}'"
                );
                if got.cycles > 0 || got.truncated || got.unresolved {
                    flagged_analyses += 1;
                }
                analyzes += 1;
            }

            Row::Lay {
                id,
                composite_id,
                leaves,
                current,
                clear_covered_slots,
                out,
            } => {
                let leaves: Vec<DissolvedLeaf> = leaves.iter().map(WireLeaf::to_leaf).collect();
                let got = lay_leaves_into_slots(
                    &current.to_slots(),
                    &node_of(&universe, &composite_id),
                    &leaves,
                    clear_covered_slots,
                );
                assert_slots(&got, &out, &format!("lay '{id}'"));
                lays += 1;
            }

            Row::Wear {
                id,
                item_id,
                current,
                lookup,
                out,
            } => {
                let map = lookup_of(&universe, lookup.as_ref());
                let got = wear_item_into_slots(
                    &current.to_slots(),
                    &node_of(&universe, &item_id),
                    map.as_ref(),
                );
                assert_slots(&got, &out, &format!("wear '{id}'"));
                wears += 1;
            }

            Row::Add {
                id,
                item_id,
                slot,
                current,
                lookup,
                out,
            } => {
                let map = lookup_of(&universe, lookup.as_ref());
                let got = add_item_to_slot(
                    &current.to_slots(),
                    &slot,
                    &node_of(&universe, &item_id),
                    map.as_ref(),
                );
                assert_slots(&got, &out, &format!("add '{id}'"));
                adds += 1;
            }

            Row::WornFor {
                id,
                item_id,
                lookup,
                only_slot,
                out,
            } => {
                let map = lookup_of(&universe, lookup.as_ref());
                let got = worn_bundles_for(
                    &node_of(&universe, &item_id),
                    map.as_ref(),
                    only_slot.as_deref(),
                );
                assert_worn_bundles(&got, &out, &format!("worn_for '{id}'"));
                if !got.is_empty() {
                    non_empty_claims += 1;
                }
                worn_fors += 1;
            }

            Row::Compute {
                id,
                mode,
                item_id,
                slot,
                remove_id,
                lookup,
                current,
                out,
                input_untouched,
                worn_bundles,
            } => {
                assert!(input_untouched, "compute '{id}': v4 mutated its input?");
                let map = lookup_of(&universe, lookup.as_ref());
                let node = item_id.as_deref().map(|i| node_of(&universe, i));
                let mode_enum = DisplacementMode::parse(&mode)
                    .unwrap_or_else(|| panic!("compute '{id}': unknown mode {mode}"));
                assert_eq!(mode_enum.as_str(), mode);
                let options = ComputeDisplacedOptions {
                    mode: mode_enum,
                    item: node.as_ref(),
                    slot: slot.as_deref(),
                    item_id: remove_id.as_deref(),
                    items_by_id: map.as_ref(),
                };
                let current = current.to_slots();
                let before = current.clone();
                let got = compute_displaced_slots(&current, &options);
                assert_slots(&got, &out, &format!("compute '{id}' ({mode})"));
                assert_eq!(current, before, "compute '{id}': input mutated");
                assert_worn_bundles(
                    &worn_bundles_for_mode(&options),
                    &worn_bundles,
                    &format!("compute '{id}' ({mode})"),
                );
                if !worn_bundles.is_empty() {
                    non_empty_claims += 1;
                }
                let needs_item = matches!(
                    mode_enum,
                    DisplacementMode::Wear
                        | DisplacementMode::Replace
                        | DisplacementMode::AddToSlot
                );
                let needs_slot = matches!(
                    mode_enum,
                    DisplacementMode::AddToSlot
                        | DisplacementMode::RemoveFromSlot
                        | DisplacementMode::ClearSlot
                );
                if (needs_item && node.is_none()) || (needs_slot && slot.is_none()) {
                    fallback_rows += 1;
                }
                modes_seen.insert(mode);
                computes += 1;
            }

            Row::InSlots {
                id,
                current,
                lookup,
                only,
                out,
                same_object,
            } => {
                let map = lookup_of(&universe, Some(&lookup)).expect("in_slots lookup");
                let only_set: Option<HashSet<String>> =
                    only.as_ref().map(|o| o.iter().cloned().collect());
                let (slots, bundles) =
                    dissolve_composites_in_slots(&current.to_slots(), &map, only_set.as_ref());
                let label = format!("in_slots '{id}'");
                assert_slots(&slots, &out.slots, &label);
                assert_worn_bundles(&bundles, &out.worn_bundles, &label);
                // v4 hands the input back untouched exactly when nothing
                // dissolved; v5 has no identity, so the observable twin is
                // "no bundles ⇔ slots unchanged".
                assert_eq!(
                    same_object,
                    bundles.is_empty(),
                    "{label}: same-object ⇔ nothing dissolved"
                );
                if same_object {
                    assert_eq!(slots, current.to_slots(), "{label}: untouched");
                }
                if only.is_some() {
                    only_rows += 1;
                }
                if !bundles.is_empty() {
                    non_empty_claims += 1;
                }
                in_slots += 1;
            }
        }
    }

    assert!(saw_meta, "oracle file carries no meta row: {path}");
    assert!(composites >= 28, "isComposite cases: {composites}");
    assert!(analyzes >= 11, "analyze cases: {analyzes}");
    assert!(
        flagged_analyses >= 4,
        "flagged analyses: {flagged_analyses}"
    );
    assert!(lays >= 4, "lay cases: {lays}");
    assert!(wears >= 5, "wear cases: {wears}");
    assert!(adds >= 2, "add cases: {adds}");
    assert!(worn_fors >= 5, "worn_for cases: {worn_fors}");
    assert!(computes >= 23, "compute cases: {computes}");
    assert_eq!(
        modes_seen.len(),
        5,
        "every mode through the table: {modes_seen:?}"
    );
    assert!(
        fallback_rows >= 7,
        "missing item/slot fallbacks: {fallback_rows}"
    );
    assert!(in_slots >= 11, "in_slots cases: {in_slots}");
    assert!(only_rows >= 4, "`only` rows: {only_rows}");
    assert!(
        non_empty_claims >= 8,
        "non-empty claims: {non_empty_claims}"
    );
    eprintln!(
        "OK: slot-ops matched oracle ({composites} isComposite / {analyzes} analyze / {lays} lay / \
         {wears} wear / {adds} add / {worn_fors} worn_for / {computes} compute / {in_slots} in_slots)."
    );
}
