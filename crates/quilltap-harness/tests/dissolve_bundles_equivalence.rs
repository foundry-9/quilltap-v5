//! Tier-1 differential (P4.D71 unit 1, re-aimed at v4 `9ecebf986` by
//! P4.D279): bundle dissolution — the pure half of v4 `61574563` "wearing a
//! bundled outfit breaks it apart automatically", now `lib/wardrobe/slot-ops.ts`.
//!
//! Diffs [`quilltap_core::slot_ops`] against v4's REAL `slot-ops.ts`,
//! field-for-field over the P4.D71 corpus (v4's old
//! unit suite case-for-case plus the shapes it leaves implicit: a
//! depth-truncated chain, a mutual cycle, a multi-slot leaf, an already-worn
//! leaf under `replace`, a repeated bundle in one slot) — grown with a
//! replacing LEAF and an out-of-order `types` list carrying an unrecognized
//! name, the two shapes `9ecebf986`'s `slotsOf` filter changes. (P4.D279
//! kept the old `dissolve_bundles` names + the three `wardrobe` mutators as
//! LANE-REMOVE shims proven here to be pure delegates; P4.D282 re-pointed
//! every row onto the `slot_ops` names and deleted the shims.)
//!
//! `slotsCoveredBy` is private at the pin: the shape row's `coveredSlots` is
//! DERIVED (the slots a lookup-less `replace` lands the id in, canonical
//! order) and compared against `slot_ops::slots_of` as a canonical-order set.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/dissolve-bundles.ts \
//!     > /tmp/oracle-dissolve-bundles.ndjson
//! Run:
//!   QT_ORACLE_DISSOLVE=/tmp/oracle-dissolve-bundles.ndjson \
//!     cargo test -p quilltap-harness --test dissolve_bundles_equivalence -- --nocapture
//!
//! P4.D262 (v4 `3ee3b1342`, the wear ledger) adds the credit widenings:
//! `slot_ops::dissolve_composites_in_slots` (every snapshot case),
//! `slot_ops::worn_bundles_for` (v4 `wornBundlesFor`) and
//! `services::outfit_selections::default_outfit_from_pool_with_credit` (v4
//! `buildDefaultOutfitWithCredit`), `wornBundles` compared whole.

use std::collections::HashMap;

use quilltap_core::db::wardrobe_wear_stats::WornBundle;
use quilltap_core::services::outfit_selections::default_outfit_from_pool_with_credit;
use quilltap_core::slot_ops::{self, DissolvedLeaf, WearableLookup, WearableNode};
use quilltap_core::wardrobe::{is_composite, Slots, WARDROBE_SLOT_TYPES};
use serde::Deserialize;
use serde_json::Value;

/// The corpus's own marker — a stale NDJSON from before this lane cannot pass
/// quietly (`oracle-regen-silent-stale-pass`).
const BASELINE: &str = "p4.d279-dissolve-bundles-shim";

#[derive(Deserialize)]
struct WireSlots {
    top: Vec<String>,
    bottom: Vec<String>,
    footwear: Vec<String>,
    accessories: Vec<String>,
    /// Absent on pre-hair corpus rows (reads as empty, matching v4's parse).
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
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "meta")]
    Meta { baseline: String, items: Vec<Value> },
    #[serde(rename = "shape")]
    Shape {
        id: String,
        #[serde(rename = "coveredSlots")]
        covered_slots: Vec<String>,
        #[serde(rename = "isBundle")]
        is_bundle: bool,
    },
    #[serde(rename = "shape_bare")]
    ShapeBare {
        id: String,
        node: Value,
        #[serde(rename = "coveredSlots")]
        covered_slots: Vec<String>,
        #[serde(rename = "isBundle")]
        is_bundle: bool,
    },
    #[serde(rename = "dissolve")]
    Dissolve {
        id: String,
        #[serde(rename = "itemId")]
        item_id: String,
        /// An item outside the universe, shipped on the row.
        #[serde(default)]
        node: Option<Value>,
        lookup: Option<Vec<String>>,
        out: Option<Vec<WireLeaf>>,
    },
    #[serde(rename = "lay")]
    Lay {
        id: String,
        #[serde(rename = "bundleId")]
        bundle_id: String,
        leaves: Vec<WireLeaf>,
        current: WireSlots,
        #[serde(rename = "clearCoveredSlots")]
        clear_covered_slots: bool,
        out: WireSlots,
    },
    #[serde(rename = "wear")]
    Wear {
        id: String,
        mode: String,
        #[serde(rename = "itemId")]
        item_id: String,
        slot: Option<String>,
        current: WireSlots,
        lookup: Option<Vec<String>>,
        out: WireSlots,
    },
    #[serde(rename = "snapshot")]
    Snapshot {
        id: String,
        current: WireSlots,
        lookup: Vec<String>,
        out: WireSlots,
    },
    // ── P4.D262 (v4 `3ee3b1342`): the wear ledger's credit widenings ──
    #[serde(rename = "snapshot_credit")]
    SnapshotCredit {
        id: String,
        current: WireSlots,
        lookup: Vec<String>,
        out: WireCredit,
    },
    #[serde(rename = "worn_bundles_for")]
    WornBundlesFor {
        id: String,
        #[serde(rename = "itemId")]
        item_id: String,
        lookup: Option<Vec<String>>,
        #[serde(rename = "onlySlot")]
        only_slot: Option<String>,
        out: Vec<WireWornBundle>,
    },
    #[serde(rename = "default_credit")]
    DefaultCredit {
        id: String,
        pool: Vec<Value>,
        out: WireCredit,
    },
}

/// v4's `{ id, leafIds }` worn-bundle element.
#[derive(Deserialize)]
struct WireWornBundle {
    id: String,
    #[serde(rename = "leafIds")]
    leaf_ids: Vec<String>,
}

/// v4's `{ slots, wornBundles }`.
#[derive(Deserialize)]
struct WireCredit {
    slots: WireSlots,
    #[serde(rename = "wornBundles")]
    worn_bundles: Vec<WireWornBundle>,
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

/// Every slot, hair included.
fn assert_all_slots(got: &Slots, want: &WireSlots, label: &str) {
    assert_eq!(got.top, want.top, "{label}: top");
    assert_eq!(got.bottom, want.bottom, "{label}: bottom");
    assert_eq!(got.footwear, want.footwear, "{label}: footwear");
    assert_eq!(got.accessories, want.accessories, "{label}: accessories");
    assert_eq!(got.hair, want.hair, "{label}: hair");
}

/// `slotsCoveredBy` as the oracle can see it: the recognized slots, in
/// canonical order, deduped.
fn covered_set(node: &WearableNode) -> Vec<String> {
    let covered = slot_ops::slots_of(node);
    WARDROBE_SLOT_TYPES
        .iter()
        .filter(|s| covered.iter().any(|c| c == *s))
        .map(|s| s.to_string())
        .collect()
}

/// Rebuild the case's lookup from the corpus's item universe. `None` = v4 passed
/// no lookup at all (`undefined`), which is a distinct arm from an empty map.
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
fn dissolve_bundles_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_DISSOLVE") else {
        eprintln!("SKIP: set QT_ORACLE_DISSOLVE to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut universe: HashMap<String, Value> = HashMap::new();
    let mut saw_meta = false;
    let (mut shapes, mut dissolves, mut lays, mut wears, mut snapshots) = (0, 0, 0, 0, 0);
    let (mut snapshot_credits, mut worn_bundles_fors, mut default_credits) = (0, 0, 0);
    // The credit rows must exercise a non-empty claim, not only `[]`.
    let mut non_empty_claims = 0;
    // Both dissolution outcomes must be present: leaves, and the store-whole
    // fail-safe. A corpus carrying only one is blind to half the contract.
    let (mut dissolved_to_leaves, mut stored_whole) = (0, 0);
    // The two `9ecebf986` shapes: a replacing leaf, an unrecognized type.
    let (mut replacing_leaf_rows, mut odd_type_rows) = (0, 0);

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("oracle row") {
            Row::Meta { baseline, items } => {
                assert_eq!(
                    baseline, BASELINE,
                    "the oracle NDJSON predates this lane — regenerate it"
                );
                for it in items {
                    let id = it
                        .get("id")
                        .and_then(Value::as_str)
                        .expect("corpus item id")
                        .to_string();
                    universe.insert(id, it);
                }
                saw_meta = true;
            }

            Row::Shape {
                id,
                covered_slots: want_slots,
                is_bundle: want_bundle,
            } => {
                let node = node_of(&universe, &id);
                assert_eq!(covered_set(&node), want_slots, "coveredSlots '{id}'");
                assert_eq!(
                    is_composite(&node.component_item_ids),
                    want_bundle,
                    "isBundle '{id}'"
                );
                if node
                    .types
                    .iter()
                    .any(|t| !WARDROBE_SLOT_TYPES.contains(&t.as_str()))
                {
                    odd_type_rows += 1;
                }
                shapes += 1;
            }

            Row::ShapeBare {
                id,
                node,
                covered_slots: want_slots,
                is_bundle: want_bundle,
            } => {
                // A node with no `componentItemIds` key at all — v4's structural
                // `WearableNode`, which must read as a non-bundle rather than
                // throwing on the missing field.
                let node = WearableNode::from_value(&node);
                assert_eq!(covered_set(&node), want_slots, "coveredSlots '{id}'");
                assert_eq!(
                    is_composite(&node.component_item_ids),
                    want_bundle,
                    "isBundle '{id}'"
                );
                shapes += 1;
            }

            Row::Dissolve {
                id,
                item_id,
                node,
                lookup,
                out,
            } => {
                let map = lookup_of(&universe, lookup.as_ref());
                let node = match node {
                    Some(n) => WearableNode::from_value(&n),
                    None => node_of(&universe, &item_id),
                };
                let got = slot_ops::dissolve_composite_to_leaves(&node, map.as_ref());
                match (&got, &out) {
                    (None, None) => stored_whole += 1,
                    (Some(g), Some(w)) => {
                        let want: Vec<DissolvedLeaf> = w.iter().map(WireLeaf::to_leaf).collect();
                        assert_eq!(*g, want, "dissolve '{id}'");
                        dissolved_to_leaves += 1;
                    }
                    _ => panic!("dissolve '{id}': null-vs-leaves mismatch (got {got:?})"),
                }
                dissolves += 1;
            }

            Row::Lay {
                id,
                bundle_id,
                leaves,
                current,
                clear_covered_slots,
                out,
            } => {
                let leaves: Vec<DissolvedLeaf> = leaves.iter().map(WireLeaf::to_leaf).collect();
                let node = node_of(&universe, &bundle_id);
                let got = slot_ops::lay_leaves_into_slots(
                    &current.to_slots(),
                    &node,
                    &leaves,
                    clear_covered_slots,
                );
                assert_all_slots(&got, &out, &format!("lay '{id}'"));
                lays += 1;
            }

            Row::Wear {
                id,
                mode,
                item_id,
                slot,
                current,
                lookup,
                out,
            } => {
                let map = lookup_of(&universe, lookup.as_ref());
                // `from_value` reads `replace` off the item itself, as v4 does.
                let node = node_of(&universe, &item_id);
                let current = current.to_slots();
                // Each mode's primitive, and the ONE mode → primitive table
                // (`compute_displaced_slots`) over the same gesture.
                let displaced = |mode: slot_ops::DisplacementMode, slot: Option<&str>| {
                    slot_ops::compute_displaced_slots(
                        &current,
                        &slot_ops::ComputeDisplacedOptions {
                            mode,
                            item: Some(&node),
                            slot,
                            item_id: None,
                            items_by_id: map.as_ref(),
                        },
                    )
                };
                let (got, via_table) = match mode.as_str() {
                    "wear" => (
                        slot_ops::wear_item_into_slots(&current, &node, map.as_ref()),
                        displaced(slot_ops::DisplacementMode::Wear, None),
                    ),
                    "replace" => (
                        slot_ops::replace_item_into_slots(&current, &node, map.as_ref()),
                        displaced(slot_ops::DisplacementMode::Replace, None),
                    ),
                    "add_to_slot" => {
                        let slot = slot.as_deref().expect("add_to_slot needs a slot");
                        (
                            slot_ops::add_item_to_slot(&current, slot, &node, map.as_ref()),
                            displaced(slot_ops::DisplacementMode::AddToSlot, Some(slot)),
                        )
                    }
                    other => panic!("unknown wear mode '{other}'"),
                };
                assert_all_slots(&got, &out, &format!("wear '{id}' ({mode})"));
                assert_eq!(got, via_table, "wear '{id}': primitive ≠ the mode table");
                if node.replace && node.component_item_ids.is_empty() {
                    replacing_leaf_rows += 1;
                }
                if node
                    .types
                    .iter()
                    .any(|t| !WARDROBE_SLOT_TYPES.contains(&t.as_str()))
                {
                    odd_type_rows += 1;
                }
                wears += 1;
            }

            Row::Snapshot {
                id,
                current,
                lookup,
                out,
            } => {
                let map = lookup_of(&universe, Some(&lookup)).expect("snapshot lookup");
                let (got, _) =
                    slot_ops::dissolve_composites_in_slots(&current.to_slots(), &map, None);
                assert_all_slots(&got, &out, &format!("snapshot '{id}'"));
                snapshots += 1;
            }

            Row::SnapshotCredit {
                id,
                current,
                lookup,
                out,
            } => {
                let map = lookup_of(&universe, Some(&lookup)).expect("snapshot lookup");
                let (got_slots, got_bundles) =
                    slot_ops::dissolve_composites_in_slots(&current.to_slots(), &map, None);
                let label = format!("snapshot_credit '{id}'");
                assert_all_slots(&got_slots, &out.slots, &label);
                assert_worn_bundles(&got_bundles, &out.worn_bundles, &label);
                if !got_bundles.is_empty() {
                    non_empty_claims += 1;
                }
                snapshot_credits += 1;
            }

            Row::WornBundlesFor {
                id,
                item_id,
                lookup,
                only_slot,
                out,
            } => {
                let map = lookup_of(&universe, lookup.as_ref());
                let node = node_of(&universe, &item_id);
                let got = slot_ops::worn_bundles_for(&node, map.as_ref(), only_slot.as_deref());
                assert_worn_bundles(&got, &out, &format!("worn_bundles_for '{id}'"));
                if !got.is_empty() {
                    non_empty_claims += 1;
                }
                worn_bundles_fors += 1;
            }

            Row::DefaultCredit { id, pool, out } => {
                let got = default_outfit_from_pool_with_credit(&pool);
                let label = format!("default_credit '{id}'");
                assert_all_slots(&got.slots, &out.slots, &label);
                assert_worn_bundles(&got.worn_bundles, &out.worn_bundles, &label);
                if !got.worn_bundles.is_empty() {
                    non_empty_claims += 1;
                }
                default_credits += 1;
            }
        }
    }

    assert!(saw_meta, "oracle file carries no meta row: {path}");
    // Shape assertions, not hand counts (`harness-corpus-shape-constants-rot`):
    // every arm must have been exercised, and the corpus must cover both the
    // dissolve-to-leaves and store-whole outcomes.
    assert!(shapes >= 41, "shape cases: {shapes}");
    assert!(dissolves >= 15, "dissolve cases: {dissolves}");
    assert!(lays >= 8, "lay cases: {lays}");
    assert!(wears >= 22, "wear cases: {wears}");
    assert!(snapshots >= 13, "snapshot cases: {snapshots}");
    assert!(
        snapshot_credits >= 13,
        "snapshot_credit cases: {snapshot_credits}"
    );
    assert!(
        worn_bundles_fors >= 12,
        "worn_bundles_for cases: {worn_bundles_fors}"
    );
    assert!(
        default_credits >= 8,
        "default_credit cases: {default_credits}"
    );
    assert!(
        non_empty_claims >= 10,
        "non-empty credit claims: {non_empty_claims}"
    );
    assert!(
        dissolved_to_leaves >= 7,
        "dissolve→leaves arms: {dissolved_to_leaves}"
    );
    assert!(
        stored_whole >= 5,
        "dissolve→store-whole arms: {stored_whole}"
    );
    assert!(
        replacing_leaf_rows >= 1,
        "replacing-leaf wear rows: {replacing_leaf_rows}"
    );
    assert!(
        odd_type_rows >= 3,
        "unrecognized-type rows: {odd_type_rows}"
    );
    eprintln!(
        "OK: dissolve-bundles (slot_ops) matched oracle ({shapes} shape / \
         {dissolves} dissolve / {lays} lay / {wears} wear / {snapshots} snapshot / \
         {snapshot_credits} credit / {worn_bundles_fors} worn_bundles_for / \
         {default_credits} default_credit cases)."
    );
}
