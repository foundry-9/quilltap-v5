//! P4.D83 sampling-params differential (tier-1, EXACT): v5
//! `quilltap_core::sampling_params::resolve_sampling_params` vs v4's REAL
//! `resolveSamplingParams` (`lib/llm/sampling-params.ts`, `d89babc4`).
//!
//! The oracle case runs the identical corpus through v4's function; a key that
//! `JSON.stringify` dropped is an "absent knob" (`undefined` there, `None`
//! here), and a present key must equal the Rust `f64` exactly.
//!
//! Generate the oracle (Node 24, from a v4 checkout pinned at the baseline):
//!   cd /tmp/qt-v4-pin-p4d83-93ed8abf
//!   npx tsx <worktree>/harness/oracle/cases/sampling-params.ts \
//!     > /tmp/oracle-sampling-params.ndjson
//! Run:
//!   QT_ORACLE_SAMPLING_PARAMS=/tmp/oracle-sampling-params.ndjson \
//!     cargo test -p quilltap-harness --test sampling_params_equivalence

use quilltap_core::sampling_params::resolve_sampling_params;
use serde_json::{json, Value};

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// The corpus, name-for-name with `harness/oracle/cases/sampling-params.ts`.
/// `None` is the case whose `params` is `undefined` there.
fn corpus() -> Vec<(&'static str, Option<Value>)> {
    vec![
        // --- v4's own suite -------------------------------------------------
        (
            "editor-snake-case",
            Some(json!({ "temperature": 1, "max_tokens": 16384, "top_p": 0.95 })),
        ),
        (
            "imported-camel-case",
            Some(json!({ "temperature": 0.7, "maxTokens": 4096, "topP": 0.9 })),
        ),
        (
            "both-spellings-snake-wins",
            Some(json!({ "max_tokens": 16384, "maxTokens": 4096, "top_p": 0.95, "topP": 0.5 })),
        ),
        (
            "numeric-strings",
            Some(json!({ "temperature": "0.8", "max_tokens": "8192", "top_p": "1" })),
        ),
        ("only-temperature", Some(json!({ "temperature": 0.7 }))),
        ("empty-bag", Some(json!({}))),
        ("undefined-bag", None),
        (
            "unparseable",
            Some(json!({ "temperature": "warm", "max_tokens": null, "top_p": {} })),
        ),
        (
            "deliberate-zero",
            Some(json!({ "temperature": 0, "top_p": 0 })),
        ),
        (
            "ignores-non-sampling-keys",
            Some(json!({
                "temperature": 1,
                "max_tokens": 16384,
                "top_p": 0.95,
                "enable_thinking": true,
                "request_timeout_seconds": 900,
                "num_ctx": 65536,
            })),
        ),
        // --- the fall-through -----------------------------------------------
        (
            "fallthrough-snake-garbage",
            Some(json!({ "max_tokens": "warm", "maxTokens": 4096 })),
        ),
        (
            "fallthrough-snake-null",
            Some(json!({ "top_p": null, "topP": 0.4 })),
        ),
        (
            "fallthrough-snake-object",
            Some(json!({ "max_tokens": {}, "maxTokens": "2048" })),
        ),
        (
            "fallthrough-both-garbage",
            Some(json!({ "max_tokens": "a", "maxTokens": "b" })),
        ),
        // --- JS Number() string coercion -------------------------------------
        (
            "string-padded",
            Some(json!({ "temperature": "  0.5  ", "max_tokens": "\t512\n" })),
        ),
        ("string-empty-is-zero", Some(json!({ "temperature": "" }))),
        ("string-whitespace-is-zero", Some(json!({ "top_p": "   " }))),
        ("string-hex", Some(json!({ "max_tokens": "0x800" }))),
        (
            "string-binary-octal",
            Some(json!({ "max_tokens": "0b101", "top_p": "0o17" })),
        ),
        ("string-exponent", Some(json!({ "max_tokens": "1e3" }))),
        (
            "string-plus-signed",
            Some(json!({ "temperature": "+0.25" })),
        ),
        (
            "string-trailing-garbage",
            Some(json!({ "temperature": "0.5abc" })),
        ),
        ("string-infinity", Some(json!({ "max_tokens": "Infinity" }))),
        (
            "string-lowercase-infinity",
            Some(json!({ "max_tokens": "infinity" })),
        ),
        (
            "string-negative-infinity",
            Some(json!({ "temperature": "-Infinity" })),
        ),
        ("string-nan-literal", Some(json!({ "temperature": "NaN" }))),
        // --- non-finite / odd numbers ----------------------------------------
        (
            "negative-values",
            Some(json!({ "temperature": -0.5, "max_tokens": -1, "top_p": -0.0 })),
        ),
        (
            "fractional-max-tokens",
            Some(json!({ "max_tokens": 1000.5 })),
        ),
        ("huge-max-tokens", Some(json!({ "max_tokens": 1e21 }))),
        // --- non-number, non-string values -----------------------------------
        (
            "boolean-values",
            Some(json!({ "temperature": true, "max_tokens": false, "top_p": true })),
        ),
        ("array-value", Some(json!({ "max_tokens": [4096] }))),
        (
            "nested-object-value",
            Some(json!({ "temperature": { "value": 0.7 } })),
        ),
        // --- non-object blobs -------------------------------------------------
        ("array-bag", Some(json!([]))),
        ("string-bag", Some(json!("nope"))),
        ("number-bag", Some(json!(42))),
        ("null-bag", Some(Value::Null)),
    ]
}

/// One oracle line's knob: absent (the key JSON.stringify dropped) or a number.
fn oracle_knob(row: &Value, key: &str) -> Option<f64> {
    match row.get(key) {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_f64()
                .unwrap_or_else(|| panic!("non-numeric {key}: {v}")),
        ),
    }
}

#[test]
fn sampling_params_match_v4() {
    let Some(path) = env_or_skip("QT_ORACLE_SAMPLING_PARAMS") else {
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();

    let corpus = corpus();
    assert_eq!(
        rows.len(),
        corpus.len(),
        "oracle row count {} != corpus {} — regenerate the oracle at the pin",
        rows.len(),
        corpus.len()
    );

    let mut diffs: Vec<String> = Vec::new();
    for (row, (name, params)) in rows.iter().zip(corpus.iter()) {
        assert_eq!(
            row["case"].as_str(),
            Some(*name),
            "case order drifted: oracle {:?} vs corpus {name}",
            row["case"]
        );
        let got = resolve_sampling_params(params.as_ref());
        for (key, want, have) in [
            (
                "temperature",
                oracle_knob(row, "temperature"),
                got.temperature,
            ),
            ("maxTokens", oracle_knob(row, "maxTokens"), got.max_tokens),
            ("topP", oracle_knob(row, "topP"), got.top_p),
        ] {
            // Exact equality: every value in the corpus is representable, and
            // `-0.0 == 0.0` matches JSON.stringify's own rendering of -0.
            let same = match (want, have) {
                (None, None) => true,
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            if !same {
                diffs.push(format!("{name}.{key}: v4 {want:?} != v5 {have:?}"));
            }
        }
    }

    assert!(
        diffs.is_empty(),
        "sampling-params diffs:\n{}",
        diffs.join("\n")
    );
    eprintln!("sampling_params: {} cases matched v4 exactly", corpus.len());
}
