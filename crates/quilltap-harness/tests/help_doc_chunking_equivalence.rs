//! P4.D77 differential — help-document chunking (v4 `lib/help/help-doc-chunking.ts`
//! `buildHelpDocChunks` / `helpChunkEmbeddingText` vs
//! `quilltap_core::services::help_doc_chunking`).
//!
//! Tier 1: exact equality over the oracle's fixed corpus. The oracle drives v4's
//! REAL module, which pulls in v4's REAL Scriptorium chunker beneath it — so
//! this diff covers the reuse as well as the wrapper.
//!
//! What the corpus pins (the reasoning lives in the oracle case's header):
//! the 400/700/100 size targets against the chunker's 800/1200/200 defaults
//! (`over-help-under-scriptorium` is chosen to split under one regime and stay
//! whole under the other — a port that forgot the options fails ONLY there),
//! the overlap prefix, heading tracking across sections and before the first
//! heading, the hard-split path for one oversized paragraph, the empty arms
//! yielding NOTHING, and — for the embedding text — the U+203A separator and
//! v4's JS-TRUTHINESS heading guard, where an empty-string heading takes the
//! title-only branch.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this tree>
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5/harness/oracle/cases/help-doc-chunking.ts \
//!     > /tmp/oracle-help-doc-chunking.ndjson
//! Run:
//!   QT_ORACLE_HELP_DOC_CHUNKING=/tmp/oracle-help-doc-chunking.ndjson \
//!     cargo test -p quilltap-harness --test help_doc_chunking_equivalence

use serde::Deserialize;
use serde_json::Value;

use quilltap_core::services::help_doc_chunking::{
    build_help_doc_chunks, help_chunk_embedding_text,
};

#[derive(Deserialize)]
struct ChunkCase {
    name: String,
    input: String,
    output: Vec<OracleChunk>,
}

#[derive(Deserialize)]
struct OracleChunk {
    #[serde(rename = "chunkIndex")]
    chunk_index: i64,
    heading: Option<String>,
    content: String,
}

#[derive(Deserialize)]
struct EmbedCase {
    name: String,
    #[serde(rename = "docTitle")]
    doc_title: String,
    heading: Option<String>,
    #[serde(rename = "headingUndefined")]
    heading_undefined: bool,
    content: String,
    output: String,
}

fn lines_of(kind: &str, text: &str) -> Vec<Value> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("oracle line"))
        .filter(|v| v.get("kind").and_then(Value::as_str) == Some(kind))
        .collect()
}

#[test]
fn help_doc_chunking_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_DOC_CHUNKING") else {
        eprintln!(
            "skipping help_doc_chunking_matches_oracle: set QT_ORACLE_HELP_DOC_CHUNKING to run \
             the differential"
        );
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).expect("oracle ndjson");

    let chunk_cases: Vec<ChunkCase> = lines_of("chunks", &text)
        .into_iter()
        .map(|v| serde_json::from_value(v).expect("chunks case"))
        .collect();
    let embed_cases: Vec<EmbedCase> = lines_of("embedText", &text)
        .into_iter()
        .map(|v| serde_json::from_value(v).expect("embedText case"))
        .collect();

    // Shape assertions, not hand counts (`harness-corpus-shape-constants-rot`):
    // the corpus must still carry the arms whose absence would make the diff
    // vacuous.
    assert!(
        chunk_cases.len() >= 16 && embed_cases.len() >= 10,
        "oracle corpus shrank — regenerate {oracle_path} ({} chunk / {} embed rows)",
        chunk_cases.len(),
        embed_cases.len()
    );
    let named = |cases: &[String], want: &str| cases.iter().any(|n| n == want);
    let chunk_names: Vec<String> = chunk_cases.iter().map(|c| c.name.clone()).collect();
    for want in [
        "empty",
        "over-help-under-scriptorium",
        "six-sections",
        "preamble-then-headings",
        "single-huge-paragraph",
    ] {
        assert!(named(&chunk_names, want), "corpus lost the {want} arm");
    }
    let embed_names: Vec<String> = embed_cases.iter().map(|c| c.name.clone()).collect();
    assert!(
        named(&embed_names, "empty-heading"),
        "corpus lost the JS-truthiness empty-heading arm"
    );
    // The size-target arm is only meaningful while it actually splits.
    let splitter = chunk_cases
        .iter()
        .find(|c| c.name == "over-help-under-scriptorium")
        .expect("the size-target arm");
    assert!(
        splitter.output.len() > 1,
        "the size-target arm no longer splits under v4's own options — it can no \
         longer catch a port that ignored HELP_CHUNK_OPTIONS"
    );

    let mut diverged: Vec<String> = Vec::new();

    for case in &chunk_cases {
        let got = build_help_doc_chunks(&case.input);
        if got.len() != case.output.len() {
            diverged.push(format!(
                "  chunks[{}]: rust {} chunks vs oracle {}",
                case.name,
                got.len(),
                case.output.len()
            ));
            continue;
        }
        for (i, (rust, oracle)) in got.iter().zip(case.output.iter()).enumerate() {
            if rust.chunk_index != oracle.chunk_index {
                diverged.push(format!(
                    "  chunks[{}][{i}].chunkIndex: rust {} vs oracle {}",
                    case.name, rust.chunk_index, oracle.chunk_index
                ));
            }
            if rust.heading != oracle.heading {
                diverged.push(format!(
                    "  chunks[{}][{i}].heading: rust {:?} vs oracle {:?}",
                    case.name, rust.heading, oracle.heading
                ));
            }
            if rust.content != oracle.content {
                diverged.push(format!(
                    "  chunks[{}][{i}].content: rust {:?} vs oracle {:?}",
                    case.name, rust.content, oracle.content
                ));
            }
        }
    }

    for case in &embed_cases {
        // `undefined` and `null` are the same input on this side (both `None`);
        // the oracle tags which one it sent so the two rows stay distinguishable
        // in a failure message.
        let heading = if case.heading_undefined {
            None
        } else {
            case.heading.as_deref()
        };
        let got = help_chunk_embedding_text(&case.doc_title, heading, &case.content);
        if got != case.output {
            diverged.push(format!(
                "  embedText[{}]: rust {:?} vs oracle {:?}",
                case.name, got, case.output
            ));
        }
    }

    assert!(
        diverged.is_empty(),
        "help-doc chunking diverged from the v4 oracle:\n{}",
        diverged.join("\n")
    );
}
