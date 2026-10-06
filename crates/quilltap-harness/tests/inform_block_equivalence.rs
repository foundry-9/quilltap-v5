//! Tier-1 differential: `buildInformBlock` (P4.D205, v4 `e7d77bb60`).
//!
//! v4's module takes its `repos` as a parameter, so the oracle injects a
//! recording stub rather than building a database — v4's own test seam, driven
//! against v4's REAL `lib/chat/context/inform-block.ts`. That separability is
//! what lets the port split the same way: [`is_swipe_request`] chooses the set,
//! [`assemble_inform_block`] builds the block, and `build_inform_block` is the
//! two plus the query.
//!
//! Four comparands per row:
//!   - **`content`** — the joined block, or absent.
//!   - **`rowIds`** — what the finalizer would consume. Empty on every swipe.
//!   - **`calledMethod`** — which read v4 chose, which pins the `isSwipe`
//!     predicate including v4's `Array.isArray(ids) && ids.length > 0` (an EMPTY
//!     list falls back to pending).
//!   - **`writeCalls`** — v4's five write methods, every one of them recorded.
//!     **Zero on every row**: selection is not delivery. This is the row-count
//!     assert the order asks for, and it is checked against v4's own answer
//!     rather than asserted only of v5.
//!
//! **P4.D249 (v4 `52d6e7ecd`, standing informs)** moves three things this
//! family now pins: v4 reads the seat's in-force set on EVERY call, so a swipe
//! makes TWO reads (`readCalls` replaces the single-read `calledMethod`
//! comparand, which is now always the in-force read); a swipe's rows are the
//! standing rows first, then the re-applied ones de-duplicated
//! ([`merge_for_swipe`]); and the `[Inform] …` debug line's
//! `pending`/`reapplied`/`standing`/`passages` ([`inform_counts`]). Red-first
//! from the oracle at both pins: 15 of 16 cases move between `e5c6bd0c0` and
//! `52d6e7ecd` (every built block's debug gains `standing`, every swipe gains
//! its second read, and the five new standing cases).
//!
//! The separator constant is pinned from v4's exported `INFORM_BLOCK_SEPARATOR`,
//! so the Rust constant is compared against v4's value, not a transcription.
//!
//! **P4.D254 (v4 `94fbb1ae3`)** gives every built block ONE vouching header
//! (`INFORM_BLOCK_HEADER`, `\n\n` before the passages; the empty-is-absent
//! arms unchanged), pinned the same way from v4's exported constant. Red-first
//! at the target pin: unported `main` diverged on the first built case (`the
//! pending passage, verbatim, …` — content without the header); the target
//! oracle carries the header on all 13 built cases.
//!
//! Generate the oracle (Node 24, from the TARGET-pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/inform-block.ts \
//!     > /tmp/oracle-inform-block.ndjson
//! Run:
//!   QT_ORACLE_INFORM_BLOCK=/tmp/oracle-inform-block.ndjson \
//!     cargo test -p quilltap-harness --test inform_block_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::chat_informs::ChatInformRow;
use quilltap_core::services::inform_block::{
    assemble_inform_block, inform_counts, is_swipe_request, merge_for_swipe, INFORM_BLOCK_HEADER,
    INFORM_BLOCK_SEPARATOR,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    label: String,
    #[serde(rename = "chatId")]
    _chat_id: String,
    #[serde(rename = "participantId")]
    _participant_id: String,
    #[serde(default, rename = "regenerationOfMessageIds")]
    regeneration_of_message_ids: Option<Vec<String>>,
    pending: Vec<SpecRow>,
    consumed: Vec<SpecRow>,
}

#[derive(Deserialize, Clone)]
struct SpecRow {
    id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "batchId")]
    batch_id: String,
    #[serde(rename = "participantId")]
    participant_id: String,
    #[serde(rename = "contentMarkdown")]
    content_markdown: String,
    #[serde(rename = "recordMessageId")]
    record_message_id: Option<String>,
    #[serde(default)]
    permanent: bool,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    #[serde(rename = "consumedAt")]
    consumed_at: Option<String>,
    #[serde(rename = "consumedByMessageId")]
    consumed_by_message_id: Option<String>,
}

impl From<&SpecRow> for ChatInformRow {
    fn from(r: &SpecRow) -> Self {
        ChatInformRow {
            id: r.id.clone(),
            chat_id: r.chat_id.clone(),
            batch_id: r.batch_id.clone(),
            participant_id: r.participant_id.clone(),
            content_markdown: r.content_markdown.clone(),
            record_message_id: r.record_message_id.clone(),
            permanent: r.permanent,
            created_at: r.created_at.clone(),
            updated_at: r.updated_at.clone(),
            consumed_at: r.consumed_at.clone(),
            consumed_by_message_id: r.consumed_by_message_id.clone(),
        }
    }
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/inform-block.json")
}

#[test]
fn inform_block_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_INFORM_BLOCK") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_INFORM_BLOCK to the oracle NDJSON (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let rows: Vec<Value> = oracle_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    assert!(!rows.is_empty(), "oracle file is EMPTY — the regen failed");

    // The separator, compared against v4's exported constant.
    let sep_row = rows
        .iter()
        .find(|r| r["case"] == "inform-block-separator")
        .expect("oracle carries the separator row");
    assert_eq!(
        sep_row["separator"].as_str().expect("separator string"),
        INFORM_BLOCK_SEPARATOR,
        "INFORM_BLOCK_SEPARATOR diverged from v4's exported constant"
    );
    // P4.D254: the header, compared against v4's exported constant.
    let header_row = rows
        .iter()
        .find(|r| r["case"] == "inform-block-header")
        .expect("oracle carries the header row — regenerate it at `94fbb1ae3` or later");
    assert_eq!(
        header_row["header"].as_str().expect("header string"),
        INFORM_BLOCK_HEADER,
        "INFORM_BLOCK_HEADER diverged from v4's exported constant"
    );

    let case_rows: Vec<&Value> = rows
        .iter()
        .filter(|r| r["case"] == "inform-block")
        .collect();
    assert_eq!(
        case_rows.len(),
        spec.cases.len(),
        "oracle row count does not match the corpus"
    );

    let mut checked = 0usize;
    for (case, want) in spec.cases.iter().zip(case_rows.iter()) {
        assert_eq!(
            want["label"].as_str().unwrap_or(""),
            case.label,
            "corpus and oracle fell out of step"
        );

        let ids = case.regeneration_of_message_ids.as_deref();
        let is_swipe = is_swipe_request(ids);

        // The predicate, pinned against the reads v4 actually performed:
        // the in-force read always, the consumed read on a swipe too — the
        // order `build_inform_block` issues them.
        let got_reads: Vec<&str> = if is_swipe {
            vec!["findPendingForParticipant", "findConsumedByMessages"]
        } else {
            vec!["findPendingForParticipant"]
        };
        let want_reads: Vec<&str> = want["readCalls"]
            .as_array()
            .expect("readCalls array")
            .iter()
            .map(|v| v.as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            got_reads, want_reads,
            "[{}] the reads diverged from v4's",
            case.label
        );

        // v4's selection over the stub's two answers: the in-force set off a
        // swipe; on a swipe the standing rows of the in-force set merged ahead
        // of the consumed ones.
        let in_force: Vec<ChatInformRow> = case.pending.iter().map(ChatInformRow::from).collect();
        let rows: Vec<ChatInformRow> = if is_swipe {
            merge_for_swipe(
                case.consumed.iter().map(ChatInformRow::from).collect(),
                in_force.into_iter().filter(|r| r.permanent).collect(),
            )
        } else {
            in_force
        };
        let got = assemble_inform_block(&rows, is_swipe);

        // The debug line's counts (absent when v4 logged no `[Inform]` line —
        // the all-whitespace arm returns EMPTY silently, as v5 does).
        match &want["debug"] {
            Value::Null => assert!(
                got.content.is_none() && !rows.is_empty(),
                "[{}] v4 logged nothing, which only the all-whitespace arm does",
                case.label
            ),
            d => {
                let f = &d["fields"];
                let (pending, reapplied, standing) = if rows.is_empty() {
                    assert_eq!(d["message"], "[Inform] No inform block for this turn");
                    (0, 0, 0)
                } else {
                    assert_eq!(d["message"], "[Inform] Built inform block");
                    let c = inform_counts(&rows, is_swipe);
                    assert_eq!(
                        f["passages"], got.passage_count,
                        "[{}] passages",
                        case.label
                    );
                    (c.pending, c.reapplied, c.standing)
                };
                assert_eq!(
                    (
                        f["pending"].clone(),
                        f["reapplied"].clone(),
                        f["standing"].clone()
                    ),
                    (pending.into(), reapplied.into(), standing.into()),
                    "[{}] debug counts diverged",
                    case.label
                );
                // v4's field order on the line.
                let keys: Vec<&str> = f.as_object().unwrap().keys().map(String::as_str).collect();
                let want_keys: &[&str] = if rows.is_empty() {
                    &[
                        "chatId",
                        "participantId",
                        "pending",
                        "reapplied",
                        "standing",
                    ]
                } else {
                    &[
                        "chatId",
                        "participantId",
                        "pending",
                        "reapplied",
                        "standing",
                        "passages",
                    ]
                };
                assert_eq!(keys, want_keys, "[{}] debug field order", case.label);
            }
        }

        let want_content = match &want["content"] {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            other => panic!("[{}] unexpected content shape: {other}", case.label),
        };
        assert_eq!(
            got.content, want_content,
            "[{}] block content diverged",
            case.label
        );

        let want_ids: Vec<String> = want["rowIds"]
            .as_array()
            .expect("rowIds array")
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(got.row_ids, want_ids, "[{}] row ids diverged", case.label);

        // Selection is not delivery — v4's own write methods, none of them called.
        let writes = want["writeCalls"].as_array().expect("writeCalls array");
        assert!(
            writes.is_empty(),
            "[{}] v4 WROTE during block assembly ({writes:?}) — the oracle's own \
             never-writes rule broke",
            case.label
        );

        checked += 1;
    }

    // Guard the corpus itself: the swipe arm and the empty-list fallback must
    // both be present, or the predicate is untested in one direction.
    assert!(
        spec.cases.iter().any(|c| c
            .regeneration_of_message_ids
            .as_deref()
            .is_some_and(|i| !i.is_empty())),
        "corpus has no swipe case"
    );
    assert!(
        spec.cases
            .iter()
            .any(|c| c.regeneration_of_message_ids.as_deref() == Some(&[])),
        "corpus has no EMPTY-regeneration-list case — v4's Array.isArray && length > 0 \
         fallback is untested"
    );

    assert!(
        spec.cases.iter().any(|c| c
            .pending
            .iter()
            .any(|r| r.permanent && r.consumed_at.is_some())),
        "corpus has no delivered standing row — the stamped-row exclusion is untested"
    );

    eprintln!("OK: inform_block matched oracle ({checked} cases).");
}
