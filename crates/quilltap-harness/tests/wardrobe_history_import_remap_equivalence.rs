//! P4.D317 (Tier 2, R-12) — tier-1 EXACT differential for the PURE half of the
//! `.qtap` import's phase 7f: `quilltap_core::services::quilltap_import::
//! wardrobe_history::remap_wardrobe_history_rows` (and with it the whole-row
//! `parse_wardrobe_history_row`) against v4's REAL `remapWardrobeHistoryRows`
//! (`lib/import/quilltap-import/import-wardrobe-history.ts`, `7953da3a0` +
//! `4ae20977f`) — so v4's REAL `ChatWardrobeHistoryRowSchema` and
//! `hashEquippedSlots` too.
//!
//! Every oracle line carries its own input (the raw rows, malformed ones
//! included, and the three id tables); both sides mint with the same counter
//! (`00000000-0000-4000-8000-<12 digits, from 1>`). Compared: the result
//! (`{ rows, droppedInvalid }`) as BYTES — v4's `JSON.stringify` against
//! `serde_json::to_string`, key order included.
//!
//! Regenerate (Node 24, from the v4 checkout at the pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx "$V5W/harness/oracle/cases/wardrobe-history-import-remap.ts" \
//!     > /tmp/oracle-wardrobe-history-import-remap.ndjson
//! then:
//!   QT_ORACLE_WARDROBE_HISTORY_IMPORT_REMAP=/tmp/oracle-wardrobe-history-import-remap.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_history_import_remap_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::services::quilltap_import::wardrobe_history::{
    remap_wardrobe_history_rows, HistoryRemapContext,
};
use serde_json::{json, Value};

fn table(v: &Value) -> HashMap<String, String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p[0].as_str().unwrap().to_string(),
                p[1].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn wardrobe_history_import_remap_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_WARDROBE_HISTORY_IMPORT_REMAP") else {
        eprintln!("SKIP: QT_ORACLE_WARDROBE_HISTORY_IMPORT_REMAP unset");
        return;
    };
    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(!lines.is_empty(), "the oracle wrote no lines");

    let mut failures = Vec::new();
    let (mut valid_rows, mut dropped) = (0usize, 0usize);
    for line in &lines {
        let name = line["name"].as_str().unwrap();
        let input = &line["input"];
        let (chats, characters, items) = (
            table(&input["chats"]),
            table(&input["characters"]),
            table(&input["items"]),
        );
        let mut n = 0u64;
        let mut mint = || {
            n += 1;
            format!("00000000-0000-4000-8000-{n:012}")
        };
        let got = remap_wardrobe_history_rows(
            input["incoming"].as_array().unwrap(),
            &mut HistoryRemapContext {
                chat_ids: &chats,
                character_ids: &characters,
                item_ids: &items,
                mint_id: &mut mint,
            },
        );
        valid_rows += got.rows.len();
        dropped += got.dropped_invalid;
        let g = serde_json::to_string(&json!({
            "rows": got.rows,
            "droppedInvalid": got.dropped_invalid,
        }))
        .unwrap();
        let w = line["resultText"].as_str().unwrap();
        if g != w {
            failures.push(format!("[{name}]\n  rust:   {g}\n  oracle: {w}"));
        } else {
            println!("OK {name}");
        }
    }
    assert_eq!(lines.len(), 56, "the corpus's case count");
    // Non-vacuity: both arms are walked.
    assert!(
        valid_rows > 0 && dropped > 0,
        "{valid_rows} rows / {dropped} dropped"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
