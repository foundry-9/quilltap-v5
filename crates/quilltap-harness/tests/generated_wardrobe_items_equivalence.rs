//! Tier-1 differential: `generators::generated_items` (v4
//! `lib/wardrobe/generated-items.ts` — the shared LLM-wardrobe shape, prompt
//! and sanitizer every AI generator uses; `p4.9k`, landed under P4.9K1 as the
//! optimizer runner's first consumer) — exact against v4's REAL exports.
//!
//! Comparands are `JSON.stringify` BYTES: key order and the omission of
//! `undefined` keys (a cleared `components` / `replace`, a blank `imagePrompt`)
//! are part of the assertion, which is why the Rust items serialize through
//! `skip_serializing_if` in v4's literal order. The prompt is compared whole
//! (it reaches a paid model verbatim).
//!
//! Coverage is asserted by SHAPE in both directions off the oracle's final
//! `coverage` row.
//!
//! Regenerate the oracle (the corpus is committed — nothing to build):
//!   cd ~/source/quilltap-server
//!   npx tsx $V5W/harness/oracle/cases/generated-wardrobe-items.ts > /tmp/oracle-generated-wardrobe-items.ndjson
//! Run:
//!   QT_ORACLE_GENERATED_WARDROBE_ITEMS=/tmp/oracle-generated-wardrobe-items.ndjson cargo test -p quilltap-harness --test generated_wardrobe_items_equivalence
//!
//! While v4 HEAD is past the baseline the regen needs a worktree pinned at
//! the baseline — the sweep driver's job (`recipe_sweep.py --run
//! generated_wardrobe_items_equivalence --v4 <pin>`), never a path baked in here.

use std::collections::BTreeSet;
use std::path::PathBuf;

use quilltap_core::generators::generated_items::{
    order_json_leaf_first, sanitize_generated_wardrobe_items, wardrobe_items_generation_prompt,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "prompt")]
    Prompt { out: String },
    #[serde(rename = "sanitize")]
    Sanitize { id: String, ok: bool, out: String },
    #[serde(rename = "order")]
    Order { id: String, ok: bool, out: String },
    #[serde(rename = "coverage")]
    Coverage { ids: Vec<String> },
}

#[derive(Deserialize)]
struct CorpusRow {
    id: String,
    items: Value,
}

#[derive(Deserialize)]
struct Corpus {
    sanitize: Vec<CorpusRow>,
    order: Vec<CorpusRow>,
}

fn corpus() -> Corpus {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/generated-wardrobe-items.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn generated_wardrobe_items_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_GENERATED_WARDROBE_ITEMS") else {
        eprintln!("SKIP: set QT_ORACLE_GENERATED_WARDROBE_ITEMS (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let corpus = corpus();
    let sanitize_by_id: std::collections::HashMap<&str, &Value> = corpus
        .sanitize
        .iter()
        .map(|r| (r.id.as_str(), &r.items))
        .collect();
    let order_by_id: std::collections::HashMap<&str, &Value> = corpus
        .order
        .iter()
        .map(|r| (r.id.as_str(), &r.items))
        .collect();

    let mut driven: BTreeSet<String> = BTreeSet::new();
    let mut coverage: Option<Vec<String>> = None;
    let mut prompt_seen = false;
    let mut failed: Vec<String> = Vec::new();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        match row {
            Row::Prompt { out } => {
                prompt_seen = true;
                if wardrobe_items_generation_prompt() != out {
                    failed.push("WARDROBE_ITEMS_GENERATION_PROMPT differs".to_string());
                }
            }
            Row::Sanitize { id, ok, out } => {
                assert!(ok, "v4 threw on sanitize row {id}: {out}");
                let items = sanitize_by_id[id.as_str()];
                let got = serde_json::to_string(&sanitize_generated_wardrobe_items(items)).unwrap();
                if got != out {
                    failed.push(format!("sanitize:{id}\n  GOT : {got}\n  WANT: {out}"));
                }
                driven.insert(format!("sanitize:{id}"));
            }
            Row::Order { id, ok, out } => {
                assert!(ok, "v4 threw on order row {id}: {out}");
                let items = order_by_id[id.as_str()].as_array().unwrap();
                let got = serde_json::to_string(&order_json_leaf_first(items)).unwrap();
                if got != out {
                    failed.push(format!("order:{id}\n  GOT : {got}\n  WANT: {out}"));
                }
                driven.insert(format!("order:{id}"));
            }
            Row::Coverage { ids } => coverage = Some(ids),
        }
    }
    assert!(prompt_seen, "the oracle carried no prompt row");
    let coverage: BTreeSet<String> = coverage.expect("a coverage row").into_iter().collect();
    assert_eq!(
        driven, coverage,
        "the ids driven must equal the oracle's coverage"
    );
    let corpus_ids: BTreeSet<String> = corpus
        .sanitize
        .iter()
        .map(|r| format!("sanitize:{}", r.id))
        .chain(corpus.order.iter().map(|r| format!("order:{}", r.id)))
        .collect();
    assert_eq!(
        corpus_ids, coverage,
        "the corpus must equal the oracle's coverage"
    );
    eprintln!(
        "generated_wardrobe_items_equivalence: {} sanitize + {} order rows + the prompt",
        corpus.sanitize.len(),
        corpus.order.len()
    );
    assert!(
        failed.is_empty(),
        "{} row(s) DIFFER:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
