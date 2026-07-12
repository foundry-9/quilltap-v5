//! P4.6p tier-1 EXACT differential: `services::annotations::generate_rendering_patterns`
//! vs v4's REAL `generateRenderingPatterns` (`lib/chat/annotations.ts`). The
//! corpus input (`delimiters` + `narrationDelimiters`) rides the NDJSON, so both
//! sides run identical input; the produced `RenderingPattern[]` is diffed
//! byte-for-byte (object keys order-insensitive, array order preserved).
//!
//! Generate the oracle (Node 24, from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     <this-worktree>/harness/oracle/cases/annotations-rendering-patterns.ts \
//!     > /tmp/oracle-annotations.ndjson
//! Run:
//!   QT_ORACLE_ANNOTATIONS=/tmp/oracle-annotations.ndjson \
//!     cargo test -p quilltap-harness --test annotations_rendering_patterns_equivalence

use quilltap_core::db::roleplay_templates::{StringOrPair, TemplateDelimiter};
use quilltap_core::services::annotations::generate_rendering_patterns;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    delimiters: Vec<TemplateDelimiter>,
    #[serde(rename = "narrationDelimiters")]
    narration_delimiters: Option<StringOrPair>,
    output: Value,
}

/// Recursively sort object keys (arrays keep their order — pattern order matters).
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}

#[test]
fn annotations_match_oracle() {
    let path = match std::env::var("QT_ORACLE_ANNOTATIONS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ANNOTATIONS (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let mut failed: Vec<String> = Vec::new();
    let mut count = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: OracleRow = serde_json::from_str(line).expect("parse oracle row");
        count += 1;
        let got = generate_rendering_patterns(&row.delimiters, row.narration_delimiters.as_ref());
        let got_v = serde_json::to_value(&got).unwrap();
        if norm(&got_v) != norm(&row.output) {
            eprintln!(
                "[{}] MISMATCH:\n  GOT : {}\n  WANT: {}",
                row.name,
                norm(&got_v),
                norm(&row.output)
            );
            failed.push(row.name);
        } else {
            eprintln!("[{}] OK.", row.name);
        }
    }
    assert!(count > 0, "oracle NDJSON was empty");
    assert!(failed.is_empty(), "annotations FAILED: {failed:?}");
}
