//! Tier-1 differential: the wardrobe YAML emitter (Decision A — the only
//! eemeli/yaml site). For each corpus case, the Rust `build_wardrobe_item_file`
//! must reproduce v4's `buildWardrobeItemFile` (over `buildSlugByItemIdMap`)
//! byte-for-byte — the emitted bytes feed the content-dedup SHA, so a quoting
//! mismatch is a correctness bug, not a test gap.
//!
//! The corpus probes the full eemeli stringify surface: plain/single/double quote
//! selection, the core-schema reparse-safety quoting (numbers/bools/null), line
//! folding past width 80, block scalars (`|`/`|-`/`>`) for multiline values, the
//! slug-vs-UUID `componentItems` map, every `buildWardrobeItemFile` flag branch,
//! and the description body.
//!
//! Build the oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-emit.ts \
//!     > /tmp/oracle-vault-wardrobe-emit.ndjson
//! Run:
//!   QT_ORACLE_VAULT_WARDROBE_EMIT=/tmp/oracle-vault-wardrobe-emit.ndjson \
//!     cargo test -p quilltap-harness --test vault_wardrobe_emit_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::vault_overlay::{
    build_slug_by_item_id_map, build_wardrobe_item_file, WardrobeItem,
};
use serde_json::Value;

#[derive(serde::Deserialize)]
struct Oracle {
    results: Vec<Vec<String>>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/vault-wardrobe-emit.json")
}

/// JSON string → `Some(Some(s))`; null/absent → `None`. The emitter treats null
/// and absent identically, so this lossless-enough mapping suffices.
fn opt_opt(v: Option<&Value>) -> Option<Option<String>> {
    match v {
        Some(Value::String(s)) => Some(Some(s.clone())),
        _ => None,
    }
}

fn str_field(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn str_array(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn item_from_json(v: &Value) -> WardrobeItem {
    WardrobeItem {
        id: str_field(v, "id"),
        character_id: opt_opt(v.get("characterId")),
        title: str_field(v, "title"),
        description: opt_opt(v.get("description")),
        image_prompt: opt_opt(v.get("imagePrompt")),
        types: str_array(v, "types"),
        component_item_ids: str_array(v, "componentItemIds"),
        appropriateness: opt_opt(v.get("appropriateness")),
        is_default: v.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
        replace: v.get("replace").and_then(Value::as_bool).unwrap_or(false),
        migrated_from_clothing_record_id: opt_opt(v.get("migratedFromClothingRecordId")),
        archived_at: opt_opt(v.get("archivedAt")),
        created_at: str_field(v, "createdAt"),
        updated_at: str_field(v, "updatedAt"),
    }
}

#[test]
fn vault_wardrobe_emit_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_WARDROBE_EMIT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_VAULT_WARDROBE_EMIT to the oracle NDJSON (see header).");
            return;
        }
    };

    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let cases = spec
        .get("cases")
        .and_then(Value::as_array)
        .expect("cases array");

    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");

    assert_eq!(
        cases.len(),
        oracle.results.len(),
        "case count vs oracle diverged"
    );

    let mut total = 0usize;
    for (ci, (case, want_case)) in cases.iter().zip(oracle.results.iter()).enumerate() {
        let items_json = case.as_array().expect("case is an array");
        let items: Vec<WardrobeItem> = items_json.iter().map(item_from_json).collect();
        let id_titles: Vec<(String, String)> = items
            .iter()
            .map(|i| (i.id.clone(), i.title.clone()))
            .collect();
        let slug_map: HashMap<String, String> =
            build_slug_by_item_id_map(&id_titles).into_iter().collect();

        assert_eq!(
            items.len(),
            want_case.len(),
            "case {ci}: item count vs oracle diverged"
        );
        for (ii, (item, want)) in items.iter().zip(want_case.iter()).enumerate() {
            let got = build_wardrobe_item_file(item, &slug_map);
            assert_eq!(
                &got, want,
                "case {ci} item {ii} (title {:?}) diverged:\n--- got ---\n{got}\n--- want ---\n{want}",
                item.title
            );
            total += 1;
        }
    }

    eprintln!(
        "OK: wardrobe emit matched oracle on {total} items across {} cases.",
        cases.len()
    );
}
