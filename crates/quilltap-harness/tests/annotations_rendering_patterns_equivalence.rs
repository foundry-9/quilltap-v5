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
//!
//! P4.D351 (v4 `778b97b2b`): a row may carry `options` (`unmarkedStyle`,
//! `markdownAsterisks`) — the generator's third argument — or `viaTemplate` (a
//! template object driven through `generate_template_rendering_patterns`). The
//! compare is BYTE-level as well as key-sorted: the stored `renderingPatterns`
//! text is what a template row persists, so key order is part of the contract.
//! `novel-whole` / `template-fn-novel` must also equal the keystone's seeded
//! Novel `renderingPatterns` (`services/builtin_templates.json`) byte for byte.

use quilltap_core::db::roleplay_templates::{StringOrPair, TemplateDelimiter, UnmarkedStyle};
use quilltap_core::services::annotations::{
    generate_rendering_patterns, generate_template_rendering_patterns, TemplateRenderingOptions,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Options {
    #[serde(rename = "unmarkedStyle", default)]
    unmarked_style: Option<UnmarkedStyle>,
    #[serde(rename = "markdownAsterisks", default)]
    markdown_asterisks: Option<bool>,
}

/// `generateTemplateRenderingPatterns`'s argument (every key optional; a
/// `null` `delimiters` is v4's `?? []`).
#[derive(Deserialize)]
struct ViaTemplate {
    #[serde(default)]
    delimiters: Option<Vec<TemplateDelimiter>>,
    #[serde(rename = "narrationDelimiters", default)]
    narration_delimiters: Option<StringOrPair>,
    #[serde(rename = "unmarkedStyle", default)]
    unmarked_style: Option<UnmarkedStyle>,
    #[serde(rename = "markdownAsterisks", default)]
    markdown_asterisks: Option<bool>,
}

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    delimiters: Vec<TemplateDelimiter>,
    #[serde(rename = "narrationDelimiters")]
    narration_delimiters: Option<StringOrPair>,
    #[serde(default)]
    options: Option<Options>,
    #[serde(rename = "viaTemplate", default)]
    via_template: Option<ViaTemplate>,
    output: Value,
}

/// The keystone's seeded Novel `renderingPatterns`, as compact JSON text.
fn seeded_novel_patterns() -> String {
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "../../quilltap-core/src/services/builtin_templates.json"
    ))
    .unwrap();
    let novel = rows.iter().find(|r| r["name"] == "Novel").unwrap();
    serde_json::to_string(&novel["renderingPatterns"]).unwrap()
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
        let got = match &row.via_template {
            Some(t) => generate_template_rendering_patterns(
                t.delimiters.as_deref(),
                t.narration_delimiters.as_ref(),
                t.unmarked_style.as_ref(),
                t.markdown_asterisks,
            ),
            None => {
                let opts = row.options.as_ref();
                generate_rendering_patterns(
                    &row.delimiters,
                    row.narration_delimiters.as_ref(),
                    TemplateRenderingOptions {
                        unmarked_style: opts.and_then(|o| o.unmarked_style.as_ref()),
                        markdown_asterisks: opts.and_then(|o| o.markdown_asterisks),
                    },
                )
            }
        };
        let got_v = serde_json::to_value(&got).unwrap();
        let got_bytes = serde_json::to_string(&got).unwrap();
        let want_bytes = serde_json::to_string(&row.output).unwrap();
        if row.name == "novel-whole" || row.name == "template-fn-novel" {
            if got_bytes != seeded_novel_patterns() {
                eprintln!(
                    "[{}] differs from the seeded Novel row: {got_bytes}",
                    row.name
                );
                failed.push(format!("{} (seed)", row.name));
            }
            if want_bytes != seeded_novel_patterns() {
                eprintln!(
                    "[{}] v4's output differs from the seeded Novel row",
                    row.name
                );
                failed.push(format!("{} (v4 vs seed)", row.name));
            }
        }
        if norm(&got_v) != norm(&row.output) || got_bytes != want_bytes {
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
    eprintln!("OK-COUNT: {count} rows, {} failed", failed.len());
    assert!(failed.is_empty(), "annotations FAILED: {failed:?}");
}
