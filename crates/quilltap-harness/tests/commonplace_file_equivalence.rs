//! P4.D265 tier-1 differential: v4's Commonplace vault-file leaf
//! (`lib/memory/commonplace-file.ts`, `d58548051`) through its REAL exports —
//! `commonplacePathFor`, `commonplaceFileToCanonText`, `truncateToTokenBudget`,
//! `renderCommonplaceFile` and the four constants — against
//! `quilltap_core::commonplace_file`, exact.
//!
//! ⚠ Lone surrogates. A UTF-16 cut through a surrogate pair leaves JS a lone
//! surrogate no Rust `String` can hold; the oracle writes every string through
//! `toWellFormed()` (lone → U+FFFD, exactly v5's lossy cut) and lists each such
//! output under `loneSurrogates`. That list is pinned here BY VALUE — the one
//! representational divergence, recorded: the sanitizer's 100-unit slice of a
//! name with an astral emoji at units 99–100 (`paths[9]`).
//!
//! Regenerate + run (a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-commonplace-file.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/commonplace-file.ts > /tmp/oracle-commonplace-file.ndjson
//!   cd $V5W
//!   QT_ORACLE_COMMONPLACE_FILE=/tmp/oracle-commonplace-file.ndjson \
//!     cargo test -p quilltap-harness --test commonplace_file_equivalence -- --nocapture

use quilltap_core::commonplace_file::{
    commonplace_file_to_canon_text, commonplace_path_for, render_commonplace_file,
    truncate_to_token_budget, CommonplaceDigestEntry, CommonplaceFileInput, CommonplaceSubject,
    COMMONPLACE_FILE_TOKEN_BUDGET, COMMONPLACE_FOLDER, COMMONPLACE_FRONTMATTER_TYPE,
    COMMONPLACE_SELF_STEM,
};
use serde_json::{json, Value};

#[test]
fn commonplace_file_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_COMMONPLACE_FILE") else {
        eprintln!("SKIP: set QT_ORACLE_COMMONPLACE_FILE to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    let o: Value = serde_json::from_str(text.trim()).expect("oracle line");
    let mut failures: Vec<String> = Vec::new();

    assert_eq!(
        o["constants"],
        json!({
            "COMMONPLACE_FOLDER": COMMONPLACE_FOLDER,
            "COMMONPLACE_FRONTMATTER_TYPE": COMMONPLACE_FRONTMATTER_TYPE,
            "COMMONPLACE_SELF_STEM": COMMONPLACE_SELF_STEM,
            "COMMONPLACE_FILE_TOKEN_BUDGET": COMMONPLACE_FILE_TOKEN_BUDGET,
        }),
        "the constants"
    );

    let paths = o["paths"].as_array().unwrap();
    for (i, p) in paths.iter().enumerate() {
        let subject = match &p["in"] {
            Value::String(s) if s == "self" => CommonplaceSubject::SelfFile,
            obj => CommonplaceSubject::Other {
                id: obj["id"].as_str().unwrap().to_string(),
                name: obj["name"].as_str().unwrap().to_string(),
            },
        };
        let got = commonplace_path_for(&subject);
        if Value::String(got.clone()) != p["out"] {
            failures.push(format!("paths[{i}]: v5 {got:?} v4 {}", p["out"]));
        }
    }

    let canon = o["canon"].as_array().unwrap();
    for (i, c) in canon.iter().enumerate() {
        let got = commonplace_file_to_canon_text(c["in"].as_str());
        let got = got.map(Value::String).unwrap_or(Value::Null);
        if got != c["out"] {
            failures.push(format!("canon[{i}]: v5 {got} v4 {}", c["out"]));
        }
    }

    let truncate = o["truncate"].as_array().unwrap();
    for (i, t) in truncate.iter().enumerate() {
        let got =
            truncate_to_token_budget(t["text"].as_str().unwrap(), t["budget"].as_i64().unwrap())
                .map(Value::String)
                .unwrap_or(Value::Null);
        if got != t["out"] {
            failures.push(format!("truncate[{i}]: v5 {got} v4 {}", t["out"]));
        }
    }

    let render = o["render"].as_array().unwrap();
    for (i, r) in render.iter().enumerate() {
        let inp = &r["in"];
        let input = CommonplaceFileInput {
            subject_character_id: inp["subjectCharacterId"].as_str().unwrap().to_string(),
            subject_name: inp["subjectName"].as_str().unwrap().to_string(),
            is_self: inp["isSelf"].as_bool().unwrap(),
            digests: inp["digests"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| CommonplaceDigestEntry {
                    content: d["content"].as_str().unwrap().to_string(),
                    kind: d["kind"].as_str().unwrap().to_string(),
                    occurred_at: d["occurredAt"].as_str().map(str::to_string),
                    reinforced_importance: d["reinforcedImportance"].as_f64().unwrap(),
                })
                .collect(),
            updated_at: inp["updatedAt"].as_str().unwrap().to_string(),
        };
        let got = render_commonplace_file(&input);
        let got = json!({
            "content": got.content,
            "entriesWritten": got.entries_written,
            "entriesDropped": got.entries_dropped,
        });
        if got != r["out"] {
            failures.push(format!("render[{i}]: v5 {got} v4 {}", r["out"]));
        }
    }

    assert!(
        failures.is_empty(),
        "{} row(s) differ from v4:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        o["loneSurrogates"],
        json!(["$.paths[9].out"]),
        "the recorded lone-surrogate outputs (v5 cuts lossily to U+FFFD)"
    );
    assert!(
        paths.len() >= 12 && canon.len() >= 14 && truncate.len() >= 9 && render.len() >= 8,
        "the corpus shrank"
    );
    eprintln!(
        "OK: commonplace file matched oracle ({} paths, {} canon, {} truncate, {} render).",
        paths.len(),
        canon.len(),
        truncate.len(),
        render.len()
    );
}
