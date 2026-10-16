//! Tier-1 differential test: Pascal TRIGGER PHRASES, the pure scanner (P4.D305,
//! v4 `362426c8f`).
//!
//! Covers `scanTriggerPhrases` / `scan_trigger_phrases`, `triggersContainedIn`
//! / `triggers_contained_in`, and `MAX_TRIGGER_RUNS_PER_MESSAGE`. Pure — every
//! `{ hits, dropped }` compared as `serde_json::to_string` BYTES (key order and
//! the UTF-16 `index` included).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/pascal-trigger-phrases.ts \
//!     > /tmp/oracle-pascal-trigger-phrases.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_TRIGGER_PHRASES=/tmp/oracle-pascal-trigger-phrases.ndjson \
//!     cargo test -p quilltap-harness --test pascal_trigger_phrases_equivalence

use quilltap_core::pascal::trigger_phrases::{
    scan_trigger_phrases, triggers_contained_in, MAX_TRIGGER_RUNS_PER_MESSAGE,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct WireTool {
    name: String,
    #[serde(default)]
    triggers: Option<Vec<String>>,
}

#[test]
fn pascal_trigger_phrases_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_TRIGGER_PHRASES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PASCAL_TRIGGER_PHRASES to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut scans, mut contained, mut constants) = (0usize, 0usize, 0usize);
    let (mut saw_dropped, mut saw_nonzero_index) = (false, false);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap_or("?").to_string();
        match row["kind"].as_str() {
            Some("scan") => {
                let content = row["content"].as_str().unwrap();
                let tools: Vec<WireTool> = serde_json::from_value(row["tools"].clone()).unwrap();
                let got = scan_trigger_phrases(
                    content,
                    tools
                        .iter()
                        .map(|t| (t.name.as_str(), t.triggers.as_deref())),
                );
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&row["out"]).unwrap(),
                    "scan case '{id}'"
                );
                saw_dropped |= !got.dropped.is_empty();
                saw_nonzero_index |= got.hits.iter().any(|h| h.index > 0);
                scans += 1;
            }
            Some("contained") => {
                let text = row["text"].as_str().unwrap();
                let triggers: Option<Vec<String>> = row
                    .get("triggers")
                    .map(|t| serde_json::from_value(t.clone()).unwrap());
                let got = triggers_contained_in(text, triggers.as_deref());
                assert_eq!(
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(&row["out"]).unwrap(),
                    "contained case '{id}'"
                );
                contained += 1;
            }
            Some("constant") => {
                assert_eq!(
                    row["out"].as_u64(),
                    Some(MAX_TRIGGER_RUNS_PER_MESSAGE as u64),
                    "MAX_TRIGGER_RUNS_PER_MESSAGE"
                );
                constants += 1;
            }
            other => panic!("case '{id}': unknown row kind {other:?}"),
        }
    }

    assert!(
        scans > 0 && contained > 0 && constants == 1,
        "oracle file looks incomplete: {scans} scans, {contained} contained, {constants} constants"
    );
    assert!(saw_dropped, "no scan row exercised the per-message cap");
    assert!(saw_nonzero_index, "no scan row matched past offset 0");
    eprintln!("OK: pascal trigger phrases matched oracle ({scans} scans, {contained} contained).");
}
