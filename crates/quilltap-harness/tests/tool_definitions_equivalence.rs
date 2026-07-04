//! Byte-exact differential: the tool-definition catalog (wave 4 / W4.1b.3).
//!
//! For every stored catalog constant, prove that parsing it with the workspace's
//! preserve-order `serde_json` and re-serializing reproduces v4's `JSON.stringify`
//! output byte-for-byte (this is the round-trip the canonicalize / provider paths
//! perform). Also assert catalog completeness (every oracle tool present, no
//! extras) and spot-check `canonicalize_universal_tools` over the full real
//! catalog.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-definitions.ts \
//!     > /tmp/oracle-tool-definitions.ndjson
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-definitions-canonical.ts \
//!     > /tmp/oracle-tool-definitions-canonical.ndjson
//! Run:
//!   QT_ORACLE_TOOL_DEFINITIONS=/tmp/oracle-tool-definitions.ndjson \
//!   QT_ORACLE_TOOL_DEFINITIONS_CANONICAL=/tmp/oracle-tool-definitions-canonical.ndjson \
//!     cargo test -p quilltap-harness --test tool_definitions_equivalence

use std::collections::HashMap;

use quilltap_core::canonicalize::canonicalize_universal_tools;
use quilltap_core::tools::definitions::{all_universal_tools, TOOL_DEFINITIONS};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct DefRow {
    key: String,
    name: String,
    #[serde(rename = "defJson")]
    def_json: String,
}

#[test]
fn tool_definitions_match_oracle() {
    let path = match std::env::var("QT_ORACLE_TOOL_DEFINITIONS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TOOL_DEFINITIONS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let rows: Vec<DefRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // Completeness: same key set, no extras / omissions.
    let oracle_keys: HashMap<&str, &DefRow> = rows.iter().map(|r| (r.key.as_str(), r)).collect();
    assert_eq!(
        TOOL_DEFINITIONS.len(),
        rows.len(),
        "catalog size {} != oracle size {}",
        TOOL_DEFINITIONS.len(),
        rows.len()
    );
    for d in TOOL_DEFINITIONS {
        assert!(
            oracle_keys.contains_key(d.key),
            "extra catalog key {}",
            d.key
        );
    }

    // Byte-exact round-trip per definition.
    for d in TOOL_DEFINITIONS {
        let oracle = oracle_keys[d.key];
        assert_eq!(d.name, oracle.name, "name mismatch for {}", d.key);
        // The stored constant IS the v4 bytes.
        assert_eq!(d.json, oracle.def_json, "stored bytes drift for {}", d.key);
        // Parse with preserve-order serde and re-serialize; must reproduce JS.
        let parsed: Value = serde_json::from_str(d.json).unwrap();
        let reserialized = serde_json::to_string(&parsed).unwrap();
        assert_eq!(
            reserialized, oracle.def_json,
            "serde round-trip diverges from JS JSON.stringify for {}",
            d.key
        );
    }

    eprintln!("OK: tool-definitions byte-exact for {} tools.", rows.len());

    // Canonicalize spot-check over the full real catalog.
    let canon_path = match std::env::var("QT_ORACLE_TOOL_DEFINITIONS_CANONICAL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP canonicalize spot-check: set QT_ORACLE_TOOL_DEFINITIONS_CANONICAL.");
            return;
        }
    };
    let canon_text = std::fs::read_to_string(&canon_path).unwrap();
    let canon_row: Value = serde_json::from_str(canon_text.lines().next().unwrap()).unwrap();
    let want = &canon_row["canonicalized"];

    let got = canonicalize_universal_tools(&all_universal_tools());
    let got_value = serde_json::to_value(&got).unwrap();
    assert_eq!(
        &got_value, want,
        "canonicalize_universal_tools diverges from v4"
    );

    eprintln!("OK: canonicalize spot-check over full catalog matched v4.");
}
