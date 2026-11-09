//! Tier-1 differential test: Pascal custom-tool RUN PRESETS — the naming
//! contract (P4.D379, v4 `fd165fce0` at the `624d0d25c` pin).
//!
//! Covers `parsePresetFromPath` / `parse_preset_from_path`,
//! `PRESET_NAME_PATTERN` and `PRESET_FILE_SUFFIX`. Pure, exact: every `parse`
//! row's `preset` (`null` or the name) and every `pattern` row's verdict.
//!
//! Generate the oracle output (from the PIN, Node 24):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/pascal-tool-presets.ts \
//!     > /tmp/oracle-pascal-tool-presets.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_TOOL_PRESETS=/tmp/oracle-pascal-tool-presets.ndjson \
//!     cargo test -p quilltap-harness --test pascal_tool_presets_equivalence

use quilltap_core::pascal::tool_presets::{
    parse_preset_from_path, PRESET_FILE_SUFFIX, PRESET_NAME_PATTERN,
};
use serde_json::Value;

#[test]
fn pascal_tool_presets_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_TOOL_PRESETS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PASCAL_TOOL_PRESETS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut constants, mut patterns, mut parses) = (0usize, 0usize, 0usize);
    let (mut saw_some, mut saw_overlap) = (false, false);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap_or("?").to_string();
        match row["kind"].as_str() {
            Some("constants") => {
                assert_eq!(
                    PRESET_NAME_PATTERN.as_str(),
                    row["patternSource"].as_str().unwrap(),
                    "PRESET_NAME_PATTERN source"
                );
                assert_eq!(
                    row["patternFlags"].as_str(),
                    Some(""),
                    "v4's pattern carries no flags (so `$` is end-of-text on both sides)"
                );
                assert_eq!(PRESET_FILE_SUFFIX, row["fileSuffix"].as_str().unwrap());
                constants += 1;
            }
            Some("pattern") => {
                let name = row["name"].as_str().unwrap();
                assert_eq!(
                    PRESET_NAME_PATTERN.is_match(name),
                    row["matches"].as_bool().unwrap(),
                    "pattern row '{id}' ({name:?})"
                );
                patterns += 1;
            }
            Some("parse") => {
                let tool = row["toolName"].as_str().unwrap();
                let rel = row["relativePath"].as_str().unwrap();
                let want = row["preset"].as_str().map(str::to_string);
                assert_eq!(
                    parse_preset_from_path(tool, rel),
                    want,
                    "parse row '{id}' ({tool:?}, {rel:?})"
                );
                saw_some |= want.is_some();
                saw_overlap |= id.starts_with("overlap-");
                parses += 1;
            }
            other => panic!("unknown row kind {other:?} in '{id}'"),
        }
    }
    assert_eq!(constants, 1, "exactly one constants row");
    assert!(patterns >= 20, "only {patterns} pattern rows");
    assert!(parses >= 40, "only {parses} parse rows");
    assert!(
        saw_some && saw_overlap,
        "corpus lost its positive or overlap rows"
    );
    eprintln!("OK: pascal tool presets matched oracle ({patterns} pattern, {parses} parse).");
}
