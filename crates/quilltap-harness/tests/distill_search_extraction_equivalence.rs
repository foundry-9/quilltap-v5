//! Tier-1 differential test: the SEARCH-side memory extraction
//! (`quilltap_core::services::memory_recap::distill` vs v4
//! `extractMemorySearchKeywords`, lib/memory/cheap-llm-tasks/memory-tasks.ts) —
//! the prompt build (system-prompt bytes, the TODAY line from the
//! ExtractionClock, the conversation window slice/truncation) and the response
//! parse including the episodic signals (strict retrospective, the timeRange
//! validation + full-day normalization, the entities trim/cap).
//!
//! SPLIT from the memory-tasks tier-1 family (P4.d13): this family regenerates
//! at v4 `8bf3cb5f`+; the creation-side cases (`QT_ORACLE_MEMORY_TASKS`) stay
//! at their `7e6d13e5` vintage until round 3.
//!
//! Generate the oracle output (jest — the seam needs `jest.mock`; TZ=UTC):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-distill.ndjson \
//!     $N/npx jest --silent --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-search-extraction
//! Run:
//!   QT_ORACLE_DISTILL=/tmp/oracle-distill.ndjson \
//!     cargo test -p quilltap-harness distill_search_extraction
use quilltap_core::services::memory_recap::distill::{
    build_distill_messages, parse_extraction, DistillMessage, ExtractionClock,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct MessageW {
    role: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClockW {
    now_iso: String,
    timeline_mode: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    name: String,
    messages: Vec<MessageW>,
    character_name: String,
    clock: ClockW,
    response_text: String,
}

#[derive(Deserialize)]
struct OracleMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OracleCall {
    messages: Vec<OracleMessage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OracleRow {
    name: String,
    calls: Vec<OracleCall>,
    success: bool,
    has_usage: bool,
    result: Value,
}

#[test]
fn distill_search_extraction_equivalence() {
    let oracle_path = match std::env::var("QT_ORACLE_DISTILL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("QT_ORACLE_DISTILL not set — skipping search-extraction differential");
            return;
        }
    };
    let oracle_raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let oracle_rows: Vec<OracleRow> = oracle_raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();

    let corpus_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/memory-search-extraction.json"
    );
    let corpus_raw = std::fs::read_to_string(corpus_path).expect("read corpus fixture");
    #[derive(Deserialize)]
    struct Spec {
        cases: Vec<CaseW>,
    }
    let spec: Spec = serde_json::from_str(&corpus_raw).expect("parse corpus");

    assert_eq!(
        spec.cases.len(),
        oracle_rows.len(),
        "corpus/oracle case-count mismatch — regenerate the oracle NDJSON"
    );

    for (case, oracle) in spec.cases.into_iter().zip(oracle_rows) {
        assert_eq!(case.name, oracle.name, "case order mismatch");
        let name = &case.name;

        // The extractor always makes exactly one call and the mocked executor
        // always succeeds with usage.
        assert!(oracle.success, "{name}: oracle reported failure");
        assert!(oracle.has_usage, "{name}: oracle usage missing");
        assert_eq!(oracle.calls.len(), 1, "{name}: oracle call-count != 1");

        let recent: Vec<DistillMessage> = case
            .messages
            .iter()
            .map(|m| DistillMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();
        let clock = ExtractionClock {
            now_iso: case.clock.now_iso.clone(),
            timeline_mode: case.clock.timeline_mode.clone(),
        };
        let messages = build_distill_messages(&recent, &case.character_name, Some(&clock));

        let oracle_msgs = &oracle.calls[0].messages;
        assert_eq!(
            messages.len(),
            oracle_msgs.len(),
            "{name}: message-count mismatch"
        );
        for (mi, (rust, orc)) in messages.iter().zip(oracle_msgs).enumerate() {
            assert_eq!(rust.role.as_str(), orc.role, "{name}: message {mi} role");
            assert_eq!(
                rust.content, orc.content,
                "{name}: message {mi} content diverges"
            );
        }

        let parsed = parse_extraction(&case.response_text);
        assert_eq!(
            parsed.signals_json(),
            oracle.result,
            "{name}: parsed result diverges"
        );
    }
    println!("distill_search_extraction_equivalence: all cases green");
}
