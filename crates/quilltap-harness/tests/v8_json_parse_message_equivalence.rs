//! P4.154 tier-1 differential — `quilltap_core::generators::optimizer::
//! v8_json_parse_message` vs V8's REAL `JSON.parse` failure wording, recorded
//! on Node 24.13.1 (the Node v4 runs on). Every row's `message` is the thrown
//! `SyntaxError`'s `.message` — or `null` where V8 ACCEPTS the input, where the
//! twin must answer `None` — so nothing on either side is typed by hand.
//!
//! v4 renders this text wherever it catches a parse failure and logs
//! `err.message`: the document-store overlay's `properties.json unparseable:`
//! line (dogfood #146 — a `{` file reads `Expected property name or '}' in JSON
//! at position 1 (line 1 column 2)`), `parseLLMJson`'s propagated
//! `SyntaxError`, and the SDKs' body parse (`model/sdk_response_shape.rs`).
//!
//! Tier 1: exact string equality, every row. A row whose V8 message carries a
//! LONE surrogate (a reported token, or a context-window edge, that splits an
//! astral pair) is recorded with each lone unit as U+FFFD (`toWellFormed`) and
//! `wellFormed: false`: a Rust `String` cannot carry the lone unit, so the twin
//! renders it lossily and the comparison is on the lossy form. That is the ONE
//! recorded divergence class — pinned below by count so a corpus change that
//! adds or loses such a row is seen.
//!
//! Generate the oracle output (from the v4 checkout, or the pinned worktree
//! while v4 HEAD is past the baseline; the case imports nothing from v4 — the
//! Node is the instrument):
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/v8-json-parse-messages.ts \
//!     > /tmp/oracle-v8-json-parse-messages.ndjson
//! Run:
//!   QT_ORACLE_V8_JSON_PARSE_MESSAGES=/tmp/oracle-v8-json-parse-messages.ndjson \
//!     cargo test -p quilltap-harness --test v8_json_parse_message_equivalence

use quilltap_core::generators::optimizer::v8_json_parse_message;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    kind: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    input: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default = "yes")]
    #[serde(rename = "wellFormed")]
    well_formed: bool,
    #[serde(default)]
    version: Option<String>,
}

fn yes() -> bool {
    true
}

/// The rows whose V8 message carries a lone surrogate (the recorded lossy
/// class). A v4/Node change that moves the corpus trips this by name.
const LONE_SURROGATE_ROWS: &[&str] = &["astral-bare-token", "astral-value-token"];

/// The rows V8 ACCEPTS that serde REFUSES — the twin's whole `None` scope once
/// a caller has a serde failure in hand, so the callers' fallback to serde's
/// own text (a RECORDED divergence: v4 parses these and carries on) is reached
/// on exactly these shapes. Both-ways: a serde change that starts accepting
/// one, or a new corpus row serde refuses, trips this by name.
const SERDE_REFUSES_V8_ACCEPTS: &[&str] = &[
    "ok-number-past-f64",
    "ok-lone-surrogate-escape",
    "ok-deep-nesting",
];

#[test]
fn v8_json_parse_message_matches_node() {
    let Ok(path) = std::env::var("QT_ORACLE_V8_JSON_PARSE_MESSAGES") else {
        eprintln!(
            "SKIP: v8_json_parse_message_matches_node: set QT_ORACLE_V8_JSON_PARSE_MESSAGES \
             to run the differential"
        );
        return;
    };
    let text = std::fs::read_to_string(&path).expect("oracle ndjson");
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();

    let node = rows
        .iter()
        .find(|r| r.kind == "node")
        .and_then(|r| r.version.as_deref())
        .expect("the oracle names its Node");
    assert_eq!(
        node, "v24.13.1",
        "V8's wording is the recording Node's — regenerate under Node 24.13.1"
    );

    let parse: Vec<&Row> = rows.iter().filter(|r| r.kind == "parse").collect();
    assert!(
        parse.len() >= 100,
        "oracle corpus too small ({}) — regenerate {path}",
        parse.len()
    );

    let mut diverged = Vec::new();
    let mut lossy = Vec::new();
    for row in &parse {
        if !row.well_formed {
            lossy.push(row.id.as_str());
        }
        let got = v8_json_parse_message(&row.input);
        if got != row.message {
            diverged.push(format!(
                "  {} {:?}:\n    rust   {:?}\n    oracle {:?}",
                row.id, row.input, got, row.message
            ));
        }
    }
    assert!(
        diverged.is_empty(),
        "v8_json_parse_message diverged from V8 on {}/{} rows:\n{}",
        diverged.len(),
        parse.len(),
        diverged.join("\n")
    );
    assert_eq!(
        lossy, LONE_SURROGATE_ROWS,
        "the lone-surrogate (lossy) rows moved — re-read the recorded divergence"
    );

    let serde_refused: Vec<&str> = parse
        .iter()
        .filter(|r| r.message.is_none())
        .filter(|r| serde_json::from_str::<serde_json::Value>(&r.input).is_err())
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(
        serde_refused, SERDE_REFUSES_V8_ACCEPTS,
        "the V8-accepts / serde-refuses rows moved — the callers' serde fallback scope changed"
    );
    // And every row V8 refuses, serde refuses too — so the twin's `Some` is
    // what every caller renders on those.
    let serde_accepts_v8_refuses: Vec<&str> = parse
        .iter()
        .filter(|r| r.message.is_some())
        .filter(|r| serde_json::from_str::<serde_json::Value>(&r.input).is_ok())
        .map(|r| r.id.as_str())
        .collect();
    assert!(
        serde_accepts_v8_refuses.is_empty(),
        "serde accepts what V8 refuses: {serde_accepts_v8_refuses:?}"
    );

    let accepted = parse.iter().filter(|r| r.message.is_none()).count();
    eprintln!(
        "v8_json_parse_message: {} rows matched ({} accepted → None, {} lossy)",
        parse.len(),
        accepted,
        lossy.len()
    );
}
