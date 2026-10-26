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

/// P4.D328 (R-10 / R-H): catalog keys v4's `ALL_TOOLS` carries at `322a5488e`
/// that SIBLING lanes of the round add — the Log's three (P4.D331) and
/// `editCustomTool` (P4.D332). The enumerator imports none of them on this
/// branch, so today the lift is a no-op; it stays so the unifier's full regen
/// at the pin is green with this const DELETED (the regen happens once, on the
/// union, with the four imports added).
const SIBLING_KEYS_UNTIL_UNION: &[&str] =
    &["logCreateEntry", "logGrep", "logRead", "editCustomTool"];

/// P4.D328: rows whose bytes MOVED at the pin under a SIBLING lane's commit —
/// `runSql` (v4 `e4a60ad9e`, the Log — P4.D331) and `terminalRead` (v4
/// `a6292f169`, bug 255 — P4.D337). This branch keeps the committed bytes; the
/// owners land the pin's. Both ways: a carved row whose bytes already EQUAL the
/// pin's fails, so a stale carve cannot outlive the union.
const SIBLING_BYTES_UNTIL_UNION: &[(&str, &str)] =
    &[("runSql", "run_sql"), ("terminalRead", "terminal_read")];

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
        .map(|l| serde_json::from_str::<DefRow>(l).unwrap())
        .filter(|r| !SIBLING_KEYS_UNTIL_UNION.contains(&r.key.as_str()))
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
        if SIBLING_BYTES_UNTIL_UNION.iter().any(|(k, _)| *k == d.key) {
            assert_ne!(
                d.json, oracle.def_json,
                "{}'s bytes now equal the pin's — delete it from SIBLING_BYTES_UNTIL_UNION",
                d.key
            );
            eprintln!("CARVED (sibling bytes until union): {}", d.key);
            continue;
        }
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
    // The sibling-bytes carve applies here too: both sides drop the carved
    // names (the canonical array is otherwise compared whole).
    let carved = |v: &Value| -> Value {
        Value::Array(
            v.as_array()
                .expect("canonicalized array")
                .iter()
                .filter(|t| {
                    let name = t
                        .pointer("/function/name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    !SIBLING_BYTES_UNTIL_UNION.iter().any(|(_, n)| *n == name)
                })
                .cloned()
                .collect(),
        )
    };
    assert_eq!(
        carved(&got_value),
        carved(want),
        "canonicalize_universal_tools diverges from v4"
    );

    eprintln!("OK: canonicalize spot-check over full catalog matched v4.");
}
