//! Tier-1 differential (P4.D71 unit 1): bundle dissolution — the pure half of
//! v4 `61574563` "wearing a bundled outfit breaks it apart automatically".
//!
//! Diffs [`quilltap_core::dissolve_bundles`] + the widened pure primitives in
//! [`quilltap_core::wardrobe`] against v4's REAL `lib/wardrobe/dissolve-bundles.ts`
//! and `lib/wardrobe/outfit-displacement.ts`, field-for-field over a corpus that
//! carries v4's own unit suite case-for-case plus the shapes it leaves implicit
//! (a depth-truncated chain, a mutual cycle, a multi-slot leaf, an already-worn
//! leaf under `replace`, a repeated bundle in one slot).
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5/harness/oracle/cases/dissolve-bundles.ts \
//!     > /tmp/oracle-dissolve-bundles.ndjson
//! Run:
//!   QT_ORACLE_DISSOLVE=/tmp/oracle-dissolve-bundles.ndjson \
//!     cargo test -p quilltap-harness --test dissolve_bundles_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::dissolve_bundles::{
    dissolve_bundle_to_leaves, dissolve_bundles_in_slots, is_bundle, lay_leaves_into_slots,
    slots_covered_by, DissolvedLeaf, WearableLookup, WearableNode,
};
use quilltap_core::wardrobe::{
    add_item_to_slot, replace_item_into_slots, wear_item_into_slots, Slots,
};
use serde::Deserialize;
use serde_json::Value;

/// The corpus's own marker — a stale NDJSON from before this lane cannot pass
/// quietly (`oracle-regen-silent-stale-pass`).
const BASELINE: &str = "p4.d71-dissolve-bundles";

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
        #[serde(rename = "slotsCoveredBy")]
        slots_covered_by: Vec<String>,
        #[serde(rename = "isBundle")]
        is_bundle: bool,
    },
    #[serde(rename = "shape_bare")]
    ShapeBare {
        id: String,
        node: Value,
        #[serde(rename = "slotsCoveredBy")]
        slots_covered_by: Vec<String>,
        #[serde(rename = "isBundle")]
        is_bundle: bool,
    },
    #[serde(rename = "dissolve")]
    Dissolve {
        id: String,
        #[serde(rename = "itemId")]
        item_id: String,
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

fn assert_slots(got: &Slots, want: &WireSlots, label: &str) {
    assert_eq!(got.top, want.top, "{label}: top");
    assert_eq!(got.bottom, want.bottom, "{label}: bottom");
    assert_eq!(got.footwear, want.footwear, "{label}: footwear");
    assert_eq!(got.accessories, want.accessories, "{label}: accessories");
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
    // Both dissolution outcomes must be present: leaves, and the store-whole
    // fail-safe. A corpus carrying only one is blind to half the contract.
    let (mut dissolved_to_leaves, mut stored_whole) = (0, 0);

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
                slots_covered_by: want_slots,
                is_bundle: want_bundle,
            } => {
                let node = node_of(&universe, &id);
                assert_eq!(slots_covered_by(&node), want_slots, "slotsCoveredBy '{id}'");
                assert_eq!(is_bundle(&node), want_bundle, "isBundle '{id}'");
                shapes += 1;
            }

            Row::ShapeBare {
                id,
                node,
                slots_covered_by: want_slots,
                is_bundle: want_bundle,
            } => {
                // A node with no `componentItemIds` key at all — v4's structural
                // `WearableNode`, which must read as a non-bundle rather than
                // throwing on the missing field.
                let node = WearableNode::from_value(&node);
                assert_eq!(slots_covered_by(&node), want_slots, "slotsCoveredBy '{id}'");
                assert_eq!(is_bundle(&node), want_bundle, "isBundle '{id}'");
                shapes += 1;
            }

            Row::Dissolve {
                id,
                item_id,
                lookup,
                out,
            } => {
                let map = lookup_of(&universe, lookup.as_ref());
                let got = dissolve_bundle_to_leaves(&node_of(&universe, &item_id), map.as_ref());
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
                let got = lay_leaves_into_slots(
                    &current.to_slots(),
                    &node_of(&universe, &bundle_id),
                    &leaves,
                    clear_covered_slots,
                );
                assert_slots(&got, &out, &format!("lay '{id}'"));
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
                let node = node_of(&universe, &item_id);
                let current = current.to_slots();
                let got = match mode.as_str() {
                    "wear" => {
                        // v4 reads `replace` off the item itself.
                        let replace = universe
                            .get(&item_id)
                            .and_then(|v| v.get("replace"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        wear_item_into_slots(&current, &node, replace, map.as_ref())
                    }
                    "replace" => replace_item_into_slots(&current, &node, map.as_ref()),
                    "add_to_slot" => add_item_to_slot(
                        &current,
                        slot.as_deref().expect("add_to_slot needs a slot"),
                        &node,
                        map.as_ref(),
                    ),
                    other => panic!("unknown wear mode '{other}'"),
                };
                assert_slots(&got, &out, &format!("wear '{id}' ({mode})"));
                wears += 1;
            }

            Row::Snapshot {
                id,
                current,
                lookup,
                out,
            } => {
                let map = lookup_of(&universe, Some(&lookup)).expect("snapshot lookup");
                let got = dissolve_bundles_in_slots(&current.to_slots(), &map);
                assert_slots(&got, &out, &format!("snapshot '{id}'"));
                snapshots += 1;
            }
        }
    }

    assert!(saw_meta, "oracle file carries no meta row: {path}");
    // Shape assertions, not hand counts (`harness-corpus-shape-constants-rot`):
    // every arm must have been exercised, and the corpus must cover both the
    // dissolve-to-leaves and store-whole outcomes.
    assert!(shapes >= 39, "shape cases: {shapes}");
    assert!(dissolves >= 14, "dissolve cases: {dissolves}");
    assert!(lays >= 7, "lay cases: {lays}");
    assert!(wears >= 19, "wear cases: {wears}");
    assert!(snapshots >= 13, "snapshot cases: {snapshots}");
    assert!(
        dissolved_to_leaves >= 6,
        "dissolve→leaves arms: {dissolved_to_leaves}"
    );
    assert!(
        stored_whole >= 5,
        "dissolve→store-whole arms: {stored_whole}"
    );
    eprintln!(
        "OK: dissolve-bundles matched oracle ({shapes} shape / {dissolves} dissolve / \
         {lays} lay / {wears} wear / {snapshots} snapshot cases)."
    );
}
