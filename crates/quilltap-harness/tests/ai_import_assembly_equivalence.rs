//! Tier-1 differential: `generators::ai_import`'s pure leaf (v4
//! `lib/services/ai-import.service.ts` — `CHARACTER_BASICS_PROMPT`,
//! `assembleWardrobeItems`, `assembleQtapExport`, `restampStructuralFields`;
//! `p4.9k`, P4.9K2 unit 2) — exact against v4's REAL exports.
//!
//! Comparands are `JSON.stringify` BYTES with every uuid remapped `<minted-N>`
//! in first-seen order on BOTH sides (the assembler mints `crypto.randomUUID()`
//! per row; a duplicate-title pair sharing ONE id, or a composite's
//! `componentItemIds` pointing at its components' ids, survives the remap
//! because the SAME uuid maps to the same placeholder). The clock is frozen in
//! the oracle and a parameter here, so timestamps are compared verbatim — the
//! chat messages' `Date.now() + idx * 1000` included. A throw is an `ok:
//! false` row whose message is compared as bytes (the assembler's own sentence
//! and V8's TypeError wording for a malformed step result).
//! `restampStructuralFields` mutates its argument, so its row carries the
//! mutated bag beside the count.
//!
//! Coverage is asserted by SHAPE in both directions off the oracle's final
//! `coverage` row.
//!
//! Regenerate the oracle (the corpus is committed — nothing to build):
//!   cd ~/source/quilltap-server
//!   npx tsx $V5W/harness/oracle/cases/ai-import-assembly.ts > /tmp/oracle-ai-import-assembly.ndjson
//! Run:
//!   QT_ORACLE_AI_IMPORT_ASSEMBLY=/tmp/oracle-ai-import-assembly.ndjson cargo test -p quilltap-harness --test ai_import_assembly_equivalence
//!
//! While v4 HEAD is past the baseline the regen needs a worktree pinned at
//! the baseline — the sweep driver's job (`recipe_sweep.py --run
//! ai_import_assembly_equivalence --v4 <pin>`), never a path baked in here.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::generators::ai_import::{
    assemble_qtap_export, assemble_wardrobe_items, character_basics_prompt,
    restamp_structural_fields,
};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "prompt")]
    Prompt { out: String },
    #[serde(rename = "wardrobe")]
    Wardrobe { id: String, ok: bool, out: String },
    #[serde(rename = "export")]
    Export { id: String, ok: bool, out: String },
    #[serde(rename = "restamp")]
    Restamp { id: String, ok: bool, out: String },
    #[serde(rename = "coverage")]
    Coverage { ids: Vec<String> },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WardrobeRow {
    id: String,
    character_id: String,
    now: String,
    items: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRow {
    id: String,
    step_results: Value,
    include_memories: bool,
    include_chats: bool,
    app_version: String,
}

#[derive(Deserialize)]
struct RestampRow {
    id: String,
    now: String,
    data: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    frozen_now_ms: i64,
    wardrobe: Vec<WardrobeRow>,
    export: Vec<ExportRow>,
    restamp: Vec<RestampRow>,
}

fn corpus() -> Corpus {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/ai-import-assembly.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Every uuid in the serialized bytes → `<minted-N>` in first-seen order.
fn remap_minted(serialized: &str) -> String {
    let re =
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap();
    let mut map: HashMap<String, String> = HashMap::new();
    let mut next = 1usize;
    re.replace_all(serialized, |caps: &regex::Captures<'_>| {
        let key = caps[0].to_string();
        map.entry(key)
            .or_insert_with(|| {
                let p = format!("<minted-{next}>");
                next += 1;
                p
            })
            .clone()
    })
    .into_owned()
}

/// A Rust-side row in the oracle's `{ok, out}` shape.
fn outcome(r: Result<String, String>) -> (bool, String) {
    match r {
        Ok(out) => (true, out),
        Err(msg) => (false, msg),
    }
}

#[test]
fn ai_import_assembly_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_AI_IMPORT_ASSEMBLY") else {
        eprintln!("SKIP: set QT_ORACLE_AI_IMPORT_ASSEMBLY (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let corpus = corpus();
    let wardrobe_by_id: HashMap<&str, &WardrobeRow> =
        corpus.wardrobe.iter().map(|r| (r.id.as_str(), r)).collect();
    let export_by_id: HashMap<&str, &ExportRow> =
        corpus.export.iter().map(|r| (r.id.as_str(), r)).collect();
    let restamp_by_id: HashMap<&str, &RestampRow> =
        corpus.restamp.iter().map(|r| (r.id.as_str(), r)).collect();

    let mut driven: BTreeSet<String> = BTreeSet::new();
    let mut coverage: Option<Vec<String>> = None;
    let mut prompt_seen = false;
    let mut failed: Vec<String> = Vec::new();
    let mut throws = 0usize;

    let mut check = |label: String, got: (bool, String), want_ok: bool, want_out: &str| {
        let (got_ok, got_out) = got;
        let g = format!("ok={got_ok} {}", remap_minted(&got_out));
        let w = format!("ok={want_ok} {}", remap_minted(want_out));
        if g != w {
            failed.push(format!("{label}\n  GOT : {g}\n  WANT: {w}"));
        }
    };

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        match row {
            Row::Prompt { out } => {
                prompt_seen = true;
                check(
                    "CHARACTER_BASICS_PROMPT".to_string(),
                    (true, character_basics_prompt()),
                    true,
                    &out,
                );
            }
            Row::Wardrobe { id, ok, out } => {
                let r = wardrobe_by_id[id.as_str()];
                let items = r.items.as_array().map(Vec::as_slice);
                let got =
                    serde_json::to_string(&assemble_wardrobe_items(items, &r.character_id, &r.now))
                        .unwrap();
                check(format!("wardrobe:{id}"), (true, got), ok, &out);
                driven.insert(format!("wardrobe:{id}"));
            }
            Row::Export { id, ok, out } => {
                let r = export_by_id[id.as_str()];
                let got = outcome(
                    assemble_qtap_export(
                        &r.step_results,
                        r.include_memories,
                        r.include_chats,
                        &r.app_version,
                        corpus.frozen_now_ms,
                    )
                    .map(|v| serde_json::to_string(&v).unwrap()),
                );
                if !ok {
                    throws += 1;
                }
                check(format!("export:{id}"), got, ok, &out);
                driven.insert(format!("export:{id}"));
            }
            Row::Restamp { id, ok, out } => {
                let r = restamp_by_id[id.as_str()];
                let mut data = r.data.clone();
                let fixes = restamp_structural_fields(&mut data, &r.now);
                let got = serde_json::to_string(&json!({"fixes": fixes, "data": data})).unwrap();
                check(format!("restamp:{id}"), (true, got), ok, &out);
                driven.insert(format!("restamp:{id}"));
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
        .wardrobe
        .iter()
        .map(|r| format!("wardrobe:{}", r.id))
        .chain(corpus.export.iter().map(|r| format!("export:{}", r.id)))
        .chain(corpus.restamp.iter().map(|r| format!("restamp:{}", r.id)))
        .collect();
    assert_eq!(
        corpus_ids, coverage,
        "the corpus must equal the oracle's coverage"
    );
    assert!(
        throws >= 5,
        "the export throw arms were driven only {throws} times"
    );
    eprintln!(
        "ai_import_assembly_equivalence: {} wardrobe + {} export ({throws} throwing) + {} restamp rows + the prompt",
        corpus.wardrobe.len(),
        corpus.export.len(),
        corpus.restamp.len()
    );
    assert!(
        failed.is_empty(),
        "{} row(s) DIFFER:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
