//! Tier-1 differential test (W4.1a sub-unit 1): the RNG pattern detector —
//! exact field-by-field equivalence against the v4 oracle. Both the detected
//! patterns and the converted tool calls are diffed. Exercises the three regexes
//! (dice `\b(\d+)?d(\d+)\b`, coin `\bflip.{1,3}coin\b`, bottle
//! `\bspin\b.{0,50}\bbottle\b`) with the JS→Rust fidelity traps: ASCII `\b`/`\d`,
//! the JS-`.` line-terminator exclusion, the "flip the coin" 1–3-char quirk, the
//! spin-bottle `{0,50}` length bound, bounds rejections, non-ASCII adjacency, and
//! the ReDoS adversarial string.
//!
//! Generate the oracle output (LOG_LEVEL=error silences the service logger, which
//! writes to stdout):
//!   cd ~/source/quilltap-server
//!   LOG_LEVEL=error npx tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/rng-patterns.ts \
//!     > /tmp/oracle-rng-patterns.ndjson
//! Run:
//!   QT_ORACLE_RNG_PATTERNS=/tmp/oracle-rng-patterns.ndjson \
//!     cargo test -p quilltap-harness --test rng_patterns_equivalence

use quilltap_core::rng_patterns::{detect_and_convert_rng_patterns, detect_rng_patterns};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Row {
    kind: String,
    id: String,
    content: String,
    patterns: Value,
    #[serde(rename = "toolCalls")]
    tool_calls: Value,
}

#[test]
fn rng_patterns_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_RNG_PATTERNS") else {
        eprintln!("SKIP: set QT_ORACLE_RNG_PATTERNS to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("oracle line parses");
        assert_eq!(row.kind, "rng");

        let got_patterns = serde_json::to_value(detect_rng_patterns(&row.content)).unwrap();
        assert_eq!(
            got_patterns, row.patterns,
            "detectRngPatterns mismatch for '{}' (content {:?})",
            row.id, row.content
        );

        let got_calls =
            serde_json::to_value(detect_and_convert_rng_patterns(&row.content)).unwrap();
        assert_eq!(
            got_calls, row.tool_calls,
            "detectAndConvertRngPatterns mismatch for '{}' (content {:?})",
            row.id, row.content
        );
        count += 1;
    }

    assert!(count >= 40, "oracle corpus too small: {count} rows");
    eprintln!("OK: rng-patterns matched oracle ({count} rows).");
}
