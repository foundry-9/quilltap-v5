//! Tier-1 differential test: the vault wardrobe-component pure leaves
//! (`parseComponentItemsField`, `parseWardrobeTypesField`, `detectComponentCycles`).
//!
//! Exact-equality check against the v4 oracle for every corpus case — no YAML,
//! no ICU/case-mapping, so these are decision-free vault (Family B) leaves ported
//! ahead of the stateful vault overlay. Covers: component-field coercion
//! (non-arrays → `[]`, trim, drop empty/non-string), wardrobe-type validation
//! (all-or-nothing enum check, dedup, `None` on empty/invalid), and component
//! cycle detection (direct self-ref, indirect, sub-cycle, diamond-safe,
//! unknown-ref, deep chain).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-component-leaves.ts \
//!     > /tmp/oracle-vault-component-leaves.ndjson
//! Run:
//!   QT_ORACLE_VAULT_COMPONENT_LEAVES=/tmp/oracle-vault-component-leaves.ndjson \
//!     cargo test -p quilltap-harness --test vault_component_leaves_equivalence

use std::collections::HashMap;

use quilltap_core::vault_overlay::{
    detect_component_cycles, parse_component_items_field, parse_wardrobe_types_field,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "componentItems")]
    ComponentItems {
        id: String,
        raw: Value,
        out: Vec<String>,
    },
    #[serde(rename = "wardrobeTypes")]
    WardrobeTypes {
        id: String,
        raw: Value,
        out: Option<Vec<String>>,
    },
    #[serde(rename = "cycles")]
    Cycles {
        id: String,
        #[serde(rename = "selfId")]
        self_id: String,
        #[serde(rename = "componentItemIds")]
        component_item_ids: Vec<String>,
        graph: HashMap<String, Vec<String>>,
        out: Vec<Vec<String>>,
    },
}

#[test]
fn vault_component_leaves_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_COMPONENT_LEAVES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_VAULT_COMPONENT_LEAVES to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        match row {
            Row::ComponentItems { id, raw, out } => {
                assert_eq!(
                    parse_component_items_field(&raw),
                    out,
                    "[{id}] parse_component_items_field"
                );
            }
            Row::WardrobeTypes { id, raw, out } => {
                assert_eq!(
                    parse_wardrobe_types_field(&raw),
                    out,
                    "[{id}] parse_wardrobe_types_field"
                );
            }
            Row::Cycles {
                id,
                self_id,
                component_item_ids,
                graph,
                out,
            } => {
                assert_eq!(
                    detect_component_cycles(&self_id, &component_item_ids, &graph),
                    out,
                    "[{id}] detect_component_cycles"
                );
            }
        }
        n += 1;
    }
    assert!(n >= 20, "expected the full corpus, saw {n} rows");
    eprintln!("OK: vault component leaves matched oracle on {n} cases.");
}
