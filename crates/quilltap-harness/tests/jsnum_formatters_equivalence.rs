//! Tier-1 differential test #27 (Wave 6 / B18): the JS `toFixed` kernel and the
//! display formatters built on it — exact string equality against the v4 oracle.
//! The toFixed rows pin V8's half-away-from-zero rounding on the f64's exact
//! value (Rust's `{:.N}` rounds half-to-even and would diverge on ties).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/jsnum-formatters.ts \
//!     > /tmp/oracle-jsnum-formatters.ndjson
//! Run:
//!   QT_ORACLE_JSNUM_FORMATTERS=/tmp/oracle-jsnum-formatters.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::format_bytes::format_bytes;
use quilltap_core::format_tokens::{format_cost_for_display, format_token_count};
use quilltap_core::jsnum::to_fixed;
use quilltap_core::token_estimation::format_token_count as format_token_count_lower;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "tofixed")]
    ToFixed {
        value: serde_json::Value,
        digits: u32,
        out: String,
    },
    #[serde(rename = "bytes")]
    Bytes { bytes: f64, out: String },
    #[serde(rename = "cost")]
    Cost { cost: Option<f64>, out: String },
    #[serde(rename = "tokK")]
    TokK { tokens: f64, out: String },
    #[serde(rename = "tokLower")]
    TokLower { tokens: f64, out: String },
}

/// Map the oracle's `value` (a JSON number, or a tag for what JSON can't carry)
/// to the f64 it denotes.
fn decode_value(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap(),
        serde_json::Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            other => panic!("unexpected value tag: {other}"),
        },
        other => panic!("unexpected value json: {other}"),
    }
}

#[test]
fn jsnum_formatters_match_oracle() {
    let path = match std::env::var("QT_ORACLE_JSNUM_FORMATTERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_JSNUM_FORMATTERS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::ToFixed { value, digits, out } => {
                let x = decode_value(&value);
                assert_eq!(to_fixed(x, digits), out, "toFixed({value}, {digits})");
            }
            Row::Bytes { bytes, out } => {
                assert_eq!(format_bytes(bytes), out, "formatBytes({bytes})");
            }
            Row::Cost { cost, out } => {
                assert_eq!(format_cost_for_display(cost), out, "formatCost({cost:?})");
            }
            Row::TokK { tokens, out } => {
                assert_eq!(
                    format_token_count(tokens),
                    out,
                    "formatTokenCount/K({tokens})"
                );
            }
            Row::TokLower { tokens, out } => {
                assert_eq!(
                    format_token_count_lower(tokens),
                    out,
                    "formatTokenCount/k({tokens})"
                );
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: jsnum-formatters matched oracle ({count} rows).");
}
