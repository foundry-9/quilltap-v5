//! Tier-1 differential test: the SEARCH-side memory extraction
//! (`quilltap_core::services::memory_recap::distill` vs v4
//! `extractMemorySearchKeywords`, lib/memory/cheap-llm-tasks/memory-tasks.ts) —
//! the prompt build (system-prompt bytes, the TODAY line from the
//! ExtractionClock, the conversation window slice/truncation), the response
//! parse including the episodic signals (strict retrospective, the timeRange
//! validation + full-day normalization, the entities trim/cap), and — since v4
//! `505dcb1f` (P4.d26) — the deterministic day-reference MERGE that overrides
//! the model's own range.
//!
//! **TWO ZONE LEGS.** The TODAY line and the day-reference scan resolve their
//! calendar in the SERVER-LOCAL zone, and under `TZ=UTC` local *is* UTC — so the
//! UTC leg alone cannot tell v4's fix from the bug it fixes. The same corpus is
//! therefore driven twice, `TZ=UTC` and `TZ=America/Chicago`, and v5 is fed the
//! matching IANA name through `ExtractionClock::local_tz` (core never reads the
//! process zone). Both legs are REQUIRED: with `QT_ORACLE_DISTILL` set, a
//! missing `QT_ORACLE_DISTILL_CHICAGO` FAILS rather than quietly halving the
//! evidence.
//!
//! The merge is exercised through the production helpers
//! (`resolve_distill_day_reference` → `parse_extraction` → `merge_day_reference`),
//! which is exactly the pipeline `distill_memory_search` runs inside its parse
//! closure — the differential never re-derives it.
//!
//! SPLIT from the memory-tasks tier-1 family (P4.d13). Regenerate BOTH legs
//! (jest — the seam needs `jest.mock`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
//!   M=/tmp/qt-p4d26-mirror; rm -rf $M; mkdir -p $M/cases $M/fixtures
//!   cp $W/harness/oracle/cases/memory-search-extraction.test.ts $M/cases/
//!   cp $W/harness/oracle/fixtures/memory-search-extraction.json $M/fixtures/
//!   cd ~/source/quilltap-server   # or a worktree pinned at the baseline
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-distill.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$M/cases" \
//!       -- memory-search-extraction
//!   TZ=America/Chicago QT_ORACLE_OUT=/tmp/oracle-distill-chicago.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$M/cases" \
//!       -- memory-search-extraction
//! Run:
//!   QT_ORACLE_DISTILL=/tmp/oracle-distill.ndjson \
//!   QT_ORACLE_DISTILL_CHICAGO=/tmp/oracle-distill-chicago.ndjson \
//!     cargo test -p quilltap-harness --test distill_search_extraction_equivalence \
//!       -- --nocapture
use quilltap_core::services::memory_recap::distill::{
    build_distill_messages, merge_day_reference, parse_extraction, resolve_distill_day_reference,
    DistillMessage, ExtractionClock,
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

#[derive(Deserialize)]
struct Spec {
    cases: Vec<CaseW>,
}

fn load_oracle(path: &str) -> Vec<OracleRow> {
    let raw = std::fs::read_to_string(path).expect("read oracle NDJSON");
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect()
}

fn load_corpus() -> Spec {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/memory-search-extraction.json"
    ))
    .expect("read corpus fixture");
    serde_json::from_str(&raw).expect("parse corpus")
}

/// One zone leg: drive every corpus case through v5's production pipeline with
/// `local_tz = zone` and diff against that leg's NDJSON. Returns the per-case
/// results so the caller can prove the legs actually disagree.
fn run_leg(zone: &str, oracle_path: &str) -> Vec<(String, Value)> {
    let oracle_rows = load_oracle(oracle_path);
    let spec = load_corpus();
    assert_eq!(
        spec.cases.len(),
        oracle_rows.len(),
        "{zone}: corpus/oracle case-count mismatch — regenerate the oracle NDJSON"
    );

    let mut out = Vec::new();
    for (case, oracle) in spec.cases.into_iter().zip(oracle_rows) {
        assert_eq!(case.name, oracle.name, "case order mismatch");
        let name = format!("{} @ {zone}", case.name);

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
            local_tz: Some(zone.to_string()),
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

        // The production pipeline, in production order: resolve the day
        // reference from the same window, parse, merge inside the success arm.
        let day_reference = resolve_distill_day_reference(&recent, Some(&clock));
        let parsed = merge_day_reference(
            parse_extraction(&case.response_text),
            day_reference.as_ref(),
        );
        let got = parsed.signals_json();
        assert_eq!(got, oracle.result, "{name}: parsed result diverges");
        out.push((case.name, got));
    }
    out
}

#[test]
fn distill_search_extraction_equivalence() {
    let Ok(utc_path) = std::env::var("QT_ORACLE_DISTILL") else {
        eprintln!("QT_ORACLE_DISTILL not set — skipping search-extraction differential");
        return;
    };
    // Required, not optional: the local-calendar behavior this family now covers
    // is INVISIBLE under TZ=UTC (local == UTC there), so a UTC-only run would
    // pass on half the evidence.
    let chicago_path = std::env::var("QT_ORACLE_DISTILL_CHICAGO").expect(
        "QT_ORACLE_DISTILL_CHICAGO must be set alongside QT_ORACLE_DISTILL — the \
         TZ=America/Chicago leg is what proves the TODAY line and the day-reference \
         scan resolve a LOCAL calendar (see the test header for the regen recipe)",
    );

    let utc = run_leg("UTC", &utc_path);
    let chicago = run_leg("America/Chicago", &chicago_path);

    // The legs must actually disagree: the day-reference cases resolve different
    // windows, and every finite-clock case renders a different TODAY line.
    let differing = utc
        .iter()
        .zip(&chicago)
        .filter(|((_, a), (_, b))| a != b)
        .count();
    assert!(
        differing >= 5,
        "only {differing} cases differ between the UTC and America/Chicago legs — \
         the corpus is blind to the local-calendar crux"
    );
    println!(
        "distill_search_extraction_equivalence: {} cases x 2 zone legs green \
         ({differing} zone-sensitive)",
        utc.len()
    );
}
