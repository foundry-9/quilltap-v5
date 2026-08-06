//! P4.37 tier-1 ALMANACK RENDER differential: deserializes the exact
//! `AlmanackReportData` v4 rendered and byte-compares
//! [`quilltap_core::almanack::render_almanack_markdown`] against v4's REAL
//! `renderAlmanackMarkdown` output.
//!
//! EXACT tier: no normalization at all. The renderer is pure by construction on
//! both sides (no clock, no I/O, no globals), so any difference is a port
//! defect. The variants in the oracle case flip the branches v4's own fixture
//! only ever takes one side of — empty collections, an unreachable Scriptorium,
//! approximate attribution, `cell()` escaping, the numeric/`null` edges.
//!
//! Generate the oracle (see the .test.ts header — it pins `TZ=UTC`), then run:
//!   QT_ORACLE_ALMANACK_RENDER=/tmp/oracle-almanack-render.ndjson \
//!     cargo test -p quilltap-harness --test almanack_render_equivalence -- --nocapture

use quilltap_core::almanack::render_almanack_markdown;
use quilltap_core::almanack::types::AlmanackReportData;
use serde_json::Value;

/// Every variant the oracle case emits. A missing name is a truncated oracle,
/// not a pass (the P4.D19 census lesson): the assertion below is on the SET,
/// not merely on each line that happens to be present.
const EXPECTED_CASES: [&str; 8] = [
    "base",
    "all_empty",
    "scriptorium_unavailable",
    "approximate_attribution",
    "pipes_and_newlines",
    "numeric_edges",
    "null_dates_and_zero_tables",
    // The V8 legacy space-form stamps (SQLite `datetime('now')` vintage) —
    // the P4.37 §3-review arm: parsed as local(=UTC) where strict ISO says N/A.
    "space_form_date_stamps",
];

/// The v4 commit the committed recipe pins. A stale NDJSON regenerated against
/// a different baseline fails loudly rather than passing silently
/// (`oracle-regen-silent-stale-pass`).
const BASELINE: &str = "f7f1a956";

#[test]
fn almanack_render_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_ALMANACK_RENDER") else {
        eprintln!("SKIP: set QT_ORACLE_ALMANACK_RENDER to the oracle NDJSON (see test header).");
        return;
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));

    let mut seen: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        let name = v["name"].as_str().expect("case name").to_string();
        assert_eq!(
            v["baseline"].as_str(),
            Some(BASELINE),
            "case {name}: oracle regenerated against a different v4 baseline"
        );

        let data: AlmanackReportData = serde_json::from_value(v["data"].clone())
            .unwrap_or_else(|e| panic!("case {name}: deserialize report data: {e}"));

        // The round trip must also be lossless: a field the Rust model dropped
        // would otherwise be invisible to a render that never mentions it.
        // Numbers are compared as `f64` on both sides — v4's JSON writes an
        // integral count as `3` and the Rust model (which holds every count as
        // a JS `number`, i.e. `f64`) writes `3.0`. That is a representation
        // difference in `serde_json::Number`, not a value difference.
        let round_tripped = serde_json::to_value(&data).expect("serialize report data");
        assert_eq!(
            numbers_as_f64(&round_tripped),
            numbers_as_f64(&v["data"]),
            "case {name}: report data did not round-trip through the Rust model"
        );

        let want = v["markdown"].as_str().expect("markdown");
        let got = render_almanack_markdown(&data);
        if got != want {
            panic!(
                "case {name}: rendered markdown differs\n{}",
                first_diff(want, &got)
            );
        }
        seen.push(name);
        checked += 1;
    }

    for expected in EXPECTED_CASES {
        assert!(
            seen.iter().any(|n| n == expected),
            "oracle is missing case `{expected}` (saw {seen:?})"
        );
    }
    assert_eq!(
        checked,
        EXPECTED_CASES.len(),
        "oracle case count moved — update EXPECTED_CASES deliberately"
    );
    eprintln!("almanack_render_equivalence: {checked} cases byte-identical");
}

/// Rewrite every number in a JSON tree as an `f64`, so an integral count
/// compares equal however the writer happened to encode it.
fn numbers_as_f64(v: &Value) -> Value {
    match v {
        Value::Number(n) => Value::from(n.as_f64().expect("finite JSON number")),
        Value::Array(items) => Value::Array(items.iter().map(numbers_as_f64).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, val)| (k.clone(), numbers_as_f64(val)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The first differing line, with a little context — a 1,000-line diff dump is
/// unreadable in a test failure.
fn first_diff(want: &str, got: &str) -> String {
    let w: Vec<&str> = want.lines().collect();
    let g: Vec<&str> = got.lines().collect();
    for i in 0..w.len().max(g.len()) {
        let a = w.get(i);
        let b = g.get(i);
        if a != b {
            return format!(
                "  first difference at line {}\n   v4: {:?}\n   v5: {:?}\n  (v4 has {} lines, v5 has {})",
                i + 1,
                a,
                b,
                w.len(),
                g.len()
            );
        }
    }
    "  (line-equal but byte-different — trailing newline?)".to_string()
}
