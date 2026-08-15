//! Tier-1 differential: the Ollama inline-`<think>` stream parser (P4.D78, the
//! port of v4 `d9c5a1c7`'s `plugins/dist/qtap-plugin-ollama/think-parser.ts`).
//!
//! Covers `ThinkTagStreamParser::push` / `flush` / `reasoning` and
//! `extract_think_blocks` against v4's REAL classes. Pure functions — exact
//! string equality on every step.
//!
//! Both sides read the SAME committed case table
//! (`harness/oracle/fixtures/ollama-think-parser/cases.json`), whose rows are
//! explicit `pushes: string[]` so no chop arithmetic has to agree across the
//! language boundary. The table enumerates every single split point of texts
//! carrying `<think>` / `</think>` (so each tag is straddled at every offset),
//! v4's own suite verbatim, the orphan-close rule with and without prior visible
//! output, unterminated blocks, held partial tags at flush, no-think
//! passthrough, and JS-vs-Rust whitespace/indexing edges.
//!
//! Regenerate the oracle:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/ollama-think-parser.ts ~/source/quilltap-v5/harness/oracle/fixtures/ollama-think-parser/cases.json > /tmp/oracle-ollama-think-parser.ndjson
//! Run:
//!   QT_ORACLE_OLLAMA_THINK_PARSER=/tmp/oracle-ollama-think-parser.ndjson cargo test -p quilltap-harness --test ollama_think_parser_equivalence
//!
//! Regenerate the case table itself (committed; a clean tree means no change):
//!   node ~/source/quilltap-v5/harness/oracle/fixtures/ollama-think-parser/gen-cases.mjs ~/source/quilltap-v5/harness/oracle/fixtures/ollama-think-parser/cases.json

use std::path::{Path, PathBuf};

use quilltap_core::model::ollama_think_parser::{extract_think_blocks, ThinkTagStreamParser};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    id: String,
    pushes: Vec<String>,
}

#[derive(Deserialize, PartialEq, Debug)]
struct Step {
    visible: String,
    reasoning: String,
}

#[derive(Deserialize)]
struct OneShot {
    content: String,
    reasoning: String,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    steps: Vec<Step>,
    flush: Step,
    #[serde(rename = "oneShot")]
    one_shot: OneShot,
}

fn cases_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/ollama-think-parser/cases.json")
}

#[test]
fn ollama_think_parser_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_OLLAMA_THINK_PARSER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_OLLAMA_THINK_PARSER to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let cases: Vec<Case> = serde_json::from_str(
        &std::fs::read_to_string(cases_path()).expect("committed case table readable"),
    )
    .expect("case table parses");

    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row parses"))
        .collect();

    assert_eq!(
        rows.len(),
        cases.len(),
        "oracle has {} rows for {} committed cases — regenerate the oracle",
        rows.len(),
        cases.len()
    );

    for (case, row) in cases.iter().zip(rows.iter()) {
        assert_eq!(case.id, row.id, "oracle row order diverged from the table");

        // The streaming half: one `push` per committed delta, then `flush`.
        let mut parser = ThinkTagStreamParser::new();
        let steps: Vec<Step> = case
            .pushes
            .iter()
            .map(|delta| {
                let visible = parser.push(delta);
                Step {
                    visible,
                    reasoning: parser.reasoning().to_string(),
                }
            })
            .collect();
        assert_eq!(steps, row.steps, "case '{}': push sequence", case.id);
        let flush = Step {
            visible: parser.flush(),
            reasoning: parser.reasoning().to_string(),
        };
        assert_eq!(flush, row.flush, "case '{}': flush", case.id);

        // The one-shot half (the non-streaming `sendMessage` split).
        let joined: String = case.pushes.concat();
        let got = extract_think_blocks(&joined);
        assert_eq!(
            got.content, row.one_shot.content,
            "case '{}': extract_think_blocks content",
            case.id
        );
        assert_eq!(
            got.reasoning, row.one_shot.reasoning,
            "case '{}': extract_think_blocks reasoning",
            case.id
        );
    }

    // Shape, not a hand count: the table must still carry each adversarial
    // family (a regenerated table that silently lost one would otherwise pass).
    for family in [
        "v4-",
        "straddle-basic-split-",
        "straddle-orphan-split-",
        "straddle-close-after-visible-split-",
        "pair-open-split-",
    ] {
        assert!(
            cases.iter().any(|c| c.id.starts_with(family)),
            "case table lost the '{family}' family"
        );
    }

    eprintln!(
        "OK: ollama-think-parser matched oracle ({} cases, {} pushes).",
        rows.len(),
        cases.iter().map(|c| c.pushes.len()).sum::<usize>()
    );
}
