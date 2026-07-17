//! Tier-1 differential (P4.6ay units 5 + 6): the Pascal outcome body and
//! Prospero's custom-tool-error body, against v4's real pure builders
//! (`buildPascalResultContent`, `buildCustomToolErrorContent`/opaque) plus the
//! reason normalization inside `postProsperoCustomToolError`.
//!
//! Only the pure builders are diffed; the `post*` writers persist through the
//! repos and are exercised by the run_custom handler + route differentials
//! (units 4 / 7).
//!
//! Generate the oracle output (v4 @ d68638b4, Node 24):
//!   cd ~/source/quilltap-server
//!   npx tsx <V5W>/harness/oracle/cases/pascal-writers.ts > /tmp/oracle-pascal-writers.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_WRITERS=/tmp/oracle-pascal-writers.ndjson \
//!     cargo test -p quilltap-harness --test pascal_writers_equivalence

use quilltap_core::services::pascal_writer::build_pascal_result_content;
use quilltap_core::services::prospero_notifications::{
    build_custom_tool_error_content, build_custom_tool_error_opaque_content,
    normalize_custom_tool_error_reason,
};
use serde_json::Value;

#[test]
fn pascal_writers_match_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_WRITERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_WRITERS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut pascal = 0usize;
    let mut error = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap();
        match row["kind"].as_str().unwrap() {
            "pascalBody" => {
                let got = build_pascal_result_content(
                    row["toolTitle"].as_str().unwrap(),
                    row["message"].as_str().unwrap(),
                );
                assert_eq!(
                    got.content,
                    row["content"].as_str().unwrap(),
                    "pascalBody '{id}' content"
                );
                assert_eq!(
                    got.opaque_content,
                    row["opaqueContent"].as_str().unwrap(),
                    "pascalBody '{id}' opaqueContent"
                );
                pascal += 1;
            }
            "customToolError" => {
                let tool = row["toolName"].as_str().unwrap();
                let normalized =
                    normalize_custom_tool_error_reason(row["reason"].as_str().unwrap());
                assert_eq!(
                    normalized,
                    row["normalized"].as_str().unwrap(),
                    "error '{id}' normalized"
                );
                assert_eq!(
                    build_custom_tool_error_content(tool, &normalized),
                    row["content"].as_str().unwrap(),
                    "error '{id}' content"
                );
                assert_eq!(
                    build_custom_tool_error_opaque_content(tool, &normalized),
                    row["opaqueContent"].as_str().unwrap(),
                    "error '{id}' opaqueContent"
                );
                error += 1;
            }
            other => panic!("unknown row kind '{other}'"),
        }
    }

    assert!(
        pascal > 0 && error > 0,
        "oracle short: {pascal} pascal, {error} error rows"
    );
    eprintln!("OK: pascal writers matched oracle ({pascal} pascal bodies, {error} error bodies).");
}
