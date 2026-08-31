//! Tier-1 differential: the moderation finish-reason recogniser (P4.D106; v4
//! `a14a1811` bug 93, `lib/llm/moderation-finish-reason.ts`), ported as
//! `quilltap_core::moderation_finish_reason` — plus its `getEmptyResponseReason`
//! wiring (`provider-failover.service.ts`), ported as
//! `quilltap_core::services::provider_failover::get_empty_response_reason`.
//!
//! The oracle drives v4's REAL module over a fixed corpus (all ten literals,
//! case/whitespace variants incl. JS-only whitespace, ordinary stops, the
//! substring traps, null/empty, Unicode case-folding) and records
//! `isModerationFinishReason` + `describeModerationRefusal` for each row
//! (`kind: "reason"`); a second section drives v4's REAL
//! `getEmptyResponseReason` over the flag × finish-reason matrix (`kind:
//! "empty"` — the moderation first branch beating each pre-existing sentence,
//! the uncensored-retry suffix, the provider/model defaults, and the five
//! pre-existing sentences byte-unchanged). The Rust side recomputes and
//! compares exactly (strings byte-for-byte).
//!
//! Regenerate the oracle (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   npx tsx $V5W/harness/oracle/cases/moderation-finish-reason.ts > /tmp/oracle-moderation-finish-reason.ndjson
//! Run:
//!   QT_ORACLE_MODERATION_FINISH_REASON=/tmp/oracle-moderation-finish-reason.ndjson cargo test -p quilltap-harness --test moderation_finish_reason_equivalence -- --nocapture

use quilltap_core::moderation_finish_reason::{
    describe_moderation_refusal, is_moderation_finish_reason,
};
use quilltap_core::services::provider_failover::get_empty_response_reason;
use serde::Deserialize;

#[derive(Deserialize)]
struct ReasonRow {
    label: String,
    reason: Option<String>,
    provider: String,
    #[serde(rename = "modelName")]
    model_name: String,
    #[serde(rename = "isModeration")]
    is_moderation: bool,
    description: Option<String>,
}

#[derive(Deserialize)]
struct EmptyRow {
    label: String,
    uncensored: bool,
    same: bool,
    flagged: bool,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
    provider: Option<String>,
    #[serde(rename = "modelName")]
    model_name: Option<String>,
    result: String,
}

#[test]
fn moderation_finish_reason_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_MODERATION_FINISH_REASON") else {
        eprintln!("QT_ORACLE_MODERATION_FINISH_REASON not set; skipping");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let mut rows = 0usize;
    let mut moderation_rows = 0usize;
    let mut empty_rows = 0usize;
    let mut empty_moderation_rows = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(line).expect("parse oracle row");
        match v.get("kind").and_then(serde_json::Value::as_str) {
            Some("reason") => {
                let row: ReasonRow = serde_json::from_value(v).expect("parse reason row");
                let got_is = is_moderation_finish_reason(row.reason.as_deref());
                assert_eq!(
                    got_is, row.is_moderation,
                    "isModeration mismatch for {}: got {got_is}, oracle {}",
                    row.label, row.is_moderation
                );
                let got_desc = describe_moderation_refusal(
                    row.reason.as_deref(),
                    &row.provider,
                    &row.model_name,
                );
                assert_eq!(
                    got_desc, row.description,
                    "description mismatch for {}",
                    row.label
                );
                rows += 1;
                if row.is_moderation {
                    moderation_rows += 1;
                }
            }
            Some("empty") => {
                let row: EmptyRow = serde_json::from_value(v).expect("parse empty row");
                let got = get_empty_response_reason(
                    row.uncensored,
                    row.same,
                    row.flagged,
                    // P4.D135: this family's corpus predates the fallback chain
                    // and never walks one, so the trail is empty and every
                    // sentence renders without the understudy roll — which is
                    // exactly what its recorded strings are.
                    &[],
                    row.finish_reason.as_deref(),
                    row.provider.as_deref(),
                    row.model_name.as_deref(),
                );
                assert_eq!(got, row.result, "empty-reason mismatch for {}", row.label);
                empty_rows += 1;
                if got.contains("refused this turn on content grounds") {
                    empty_moderation_rows += 1;
                }
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }
    // Corpus-shape floors: the ten literals + their variant rows must actually
    // be present (a truncated or stale oracle must not pass vacuously), at
    // least one row must exercise the negative arm, and the empty-reason
    // matrix must carry both moderation-first-branch rows and fall-through
    // rows for the five pre-existing sentences.
    assert!(
        rows >= 30,
        "expected the full reason corpus, got {rows} rows"
    );
    assert!(
        moderation_rows >= 15,
        "expected the recognised rows, got {moderation_rows}"
    );
    assert!(
        moderation_rows < rows,
        "expected at least one unrecognised row"
    );
    assert!(
        empty_rows >= 12,
        "expected the empty-reason matrix, got {empty_rows} rows"
    );
    assert!(
        empty_moderation_rows >= 5 && empty_moderation_rows < empty_rows,
        "expected both moderation and fall-through empty rows, got {empty_moderation_rows}/{empty_rows}"
    );
    println!(
        "moderation_finish_reason_equivalence: {rows} reason rows + {empty_rows} empty rows OK"
    );
}
