//! P4.D283 tier-1 differential: the physical-description pick (v4
//! `lib/characters/physical-description.ts` `pickPhysicalDescription` /
//! `hasPhysicalDescription`, v4 `9ecebf986`) vs
//! `quilltap_core::characters::physical_description`.
//!
//! Both sides read the SAME corpus (`harness/oracle/fixtures/physical-description
//! .json`) so the inputs cannot drift; every case runs under all FIVE profiles
//! and the picked text + the `has` verdict are compared EXACTLY. The corpus pins
//! the two semantics the ten pre-span inline chains disagreed on — a
//! whitespace-only variant is SKIPPED and the winner is TRIMMED (JS `trim`,
//! incl. NBSP / the ideographic space / a BOM / the line and paragraph
//! separators) — plus every single-variant shape, so each profile's lead AND its
//! tail show, and the head-and-shoulders crop alone, which `scene` /
//! `self-image` / `fullest` must never answer. Those identities are asserted
//! here as well as diffed.
//!
//! ⚠ The module this drives arrived at v4 `9ecebf986`, which is PAST the
//! `01a83539d` oracle baseline — so until the baseline moves, regenerate through
//! the sweep driver with a pin at or after that commit
//! (`recipe_sweep.py --v4 <pinned worktree> --run physical_description_equivalence`),
//! which rewrites the `cd` below. Against a checkout still at the baseline the
//! import fails outright; it cannot pass stale.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/physical-description.ts \
//!     > /tmp/oracle-physical-description.ndjson
//!
//! Run:
//!   QT_ORACLE_PHYSICAL_DESCRIPTION=/tmp/oracle-physical-description.ndjson \
//!     cargo test -p quilltap-harness --test physical_description_equivalence

use std::collections::HashMap;

use quilltap_core::characters::physical_description::{
    has_physical_description_value, pick_physical_description_value, PhysicalDescriptionProfile,
};
use serde_json::Value;

const PROFILES: [PhysicalDescriptionProfile; 5] = [
    PhysicalDescriptionProfile::HeadAndShoulders,
    PhysicalDescriptionProfile::FullLength,
    PhysicalDescriptionProfile::Scene,
    PhysicalDescriptionProfile::SelfImage,
    PhysicalDescriptionProfile::Fullest,
];

fn corpus_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/physical-description.json")
}

fn corpus() -> Vec<Value> {
    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read corpus"))
            .expect("parse corpus");
    corpus["cases"].as_array().expect("corpus cases").clone()
}

/// The corpus's `"__UNDEFINED__"` sentinel is JS `undefined`; on this side it
/// and `null` are both "no description" (`Option::None`).
fn desc_of(case: &Value) -> Option<&Value> {
    match &case["desc"] {
        Value::String(s) if s == "__UNDEFINED__" => None,
        Value::Null => None,
        v => Some(v),
    }
}

#[test]
fn physical_description_pick_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PHYSICAL_DESCRIPTION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PHYSICAL_DESCRIPTION to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let body = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let cases = corpus();
    let by_name: HashMap<String, Value> = cases
        .iter()
        .map(|c| {
            (
                c["name"].as_str().expect("case name").to_string(),
                c.clone(),
            )
        })
        .collect();
    assert_eq!(
        by_name.len(),
        cases.len(),
        "corpus case names must be unique (a duplicate would silently shadow)"
    );

    let mut rows = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let name = row["name"].as_str().expect("name");
        let tag = row["profile"].as_str().expect("profile");
        let profile = PhysicalDescriptionProfile::parse(tag)
            .unwrap_or_else(|| panic!("oracle row {name} names an unknown profile {tag:?}"));
        assert_eq!(profile.as_str(), tag, "the tag round-trips");
        let case = by_name
            .get(name)
            .unwrap_or_else(|| panic!("oracle row {name} has no corpus case"));
        let want_pick = row["pick"].as_str().expect("pick");
        let want_has = row["has"].as_bool().expect("has");
        let got_pick = pick_physical_description_value(desc_of(case), profile);
        let got_has = has_physical_description_value(desc_of(case), profile);
        if got_pick != want_pick || got_has != want_has {
            failures.push(format!(
                "{name} / {tag}: v5 pick={got_pick:?} has={got_has} ; v4 pick={want_pick:?} has={want_has}"
            ));
        }
        rows += 1;
    }
    assert_eq!(
        rows,
        cases.len() * PROFILES.len(),
        "the oracle must carry one row per (case, profile)"
    );
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The identities the corpus exists to pin, asserted on the v5 side alone (so
/// "both sides agree" and "the rule holds" are separate claims).
#[test]
fn the_pinned_identities_hold() {
    let cases = corpus();
    let case = |n: &str| cases.iter().find(|c| c["name"] == n).expect(n).clone();

    // The head-and-shoulders crop alone is NEVER a scene / self-image /
    // fullest answer — and IS the full-length tail.
    let crop = case("only-head-and-shoulders");
    for p in [
        PhysicalDescriptionProfile::Scene,
        PhysicalDescriptionProfile::SelfImage,
        PhysicalDescriptionProfile::Fullest,
    ] {
        assert_eq!(pick_physical_description_value(desc_of(&crop), p), "");
        assert!(!has_physical_description_value(desc_of(&crop), p));
    }
    assert_eq!(
        pick_physical_description_value(desc_of(&crop), PhysicalDescriptionProfile::FullLength),
        "H&S"
    );

    // Whitespace-only leads are SKIPPED, not taken-and-emptied.
    let ws = case("whitespace-leads-skipped");
    for p in PROFILES {
        assert_eq!(
            pick_physical_description_value(desc_of(&ws), p),
            "the full prose",
            "{p:?}"
        );
    }
    // Every-whitespace is nothing at all.
    let none = case("every-whitespace");
    for p in PROFILES {
        assert_eq!(pick_physical_description_value(desc_of(&none), p), "");
        assert!(!has_physical_description_value(desc_of(&none), p));
    }
    // The winner is trimmed with JS `trim` (NBSP + the ideographic space).
    let nbsp = case("nbsp-and-ideographic-space-trimmed");
    assert_eq!(
        pick_physical_description_value(desc_of(&nbsp), PhysicalDescriptionProfile::SelfImage),
        "copper hair"
    );
    assert_eq!(
        pick_physical_description_value(desc_of(&nbsp), PhysicalDescriptionProfile::FullLength),
        "ideographic"
    );
    // Interior whitespace is kept verbatim.
    let interior = case("interior-whitespace-kept");
    assert_eq!(
        pick_physical_description_value(desc_of(&interior), PhysicalDescriptionProfile::Scene),
        "two   words\tapart \n inside"
    );
}
