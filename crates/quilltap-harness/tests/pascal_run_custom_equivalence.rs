//! Tier-1 differential (P4.6ay unit 4, the pure half): the `run_custom`
//! description builder against v4's real `buildRunCustomDescription`. Each row
//! carries the raw definition JSON(s) forming the roster; the Rust side parses
//! them through `safe_parse`, rebuilds the description, and diffs byte-for-byte.
//!
//! The empty-roster form (the §2 byte-identity obligation) is pinned by an
//! in-crate test (`quilltap_core::tools::run_custom` →
//! `empty_roster_description_matches_data_rs`), not here.
//!
//! Generate the oracle (v4 @ d68638b4, Node 24):
//!   cd ~/source/quilltap-server
//!   npx tsx <V5W>/harness/oracle/cases/pascal-run-custom.ts > /tmp/oracle-pascal-run-custom.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_RUN_CUSTOM=/tmp/oracle-pascal-run-custom.ndjson \
//!     cargo test -p quilltap-harness --test pascal_run_custom_equivalence

use quilltap_core::db::tiered_mount_pool::MountTier;
use quilltap_core::pascal::custom_tool_types::safe_parse;
use quilltap_core::pascal::roster::DiscoveredCustomTool;
use quilltap_core::tools::run_custom::build_run_custom_description;
use serde_json::Value;

#[test]
fn run_custom_description_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_RUN_CUSTOM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_RUN_CUSTOM to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut rows = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap();

        let roster: Vec<DiscoveredCustomTool> = row["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|raw| {
                let definition = safe_parse(raw)
                    .unwrap_or_else(|e| panic!("case '{id}': definition failed to parse: {e:?}"));
                DiscoveredCustomTool {
                    definition,
                    tier: MountTier::Character,
                    mount_point_id: "m".to_string(),
                    mount_name: "M".to_string(),
                    definition_path: "Tools/x.tool.json".to_string(),
                }
            })
            .collect();

        assert_eq!(
            build_run_custom_description(&roster),
            row["description"].as_str().unwrap(),
            "case '{id}' description"
        );
        rows += 1;
    }

    assert!(rows > 0, "oracle empty");
    eprintln!("OK: run_custom descriptions matched oracle ({rows} rows).");
}
