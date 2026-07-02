//! Tier-1 differential test: turn-manager pure predicates
//! (chat-orchestration wave 1.1, Part B). Covers isUsersTurn,
//! isParticipantsTurn, getSelectionExplanation — pure functions, exact
//! equality.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-predicates.ts \
//!     > /tmp/oracle-turn-predicates.ndjson
//! Run:
//!   QT_ORACLE_TURN_PREDICATES=/tmp/oracle-turn-predicates.ndjson \
//!     cargo test -p quilltap-harness --test turn_predicates_equivalence

use quilltap_core::select_speaker::{
    get_selection_explanation, is_participants_turn, is_users_turn, SelectionResult,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "users_turn")]
    UsersTurn {
        id: String,
        #[serde(rename = "nextSpeakerId")]
        next_speaker_id: Option<String>,
        reason: String,
        out: bool,
    },
    #[serde(rename = "participants_turn")]
    ParticipantsTurn {
        id: String,
        #[serde(rename = "nextSpeakerId")]
        next_speaker_id: Option<String>,
        reason: String,
        #[serde(rename = "participantId")]
        participant_id: String,
        out: bool,
    },
    #[serde(rename = "explanation")]
    Explanation {
        id: String,
        reason: String,
        out: String,
    },
}

/// The port's `SelectionResult.reason` is `&'static str`; the oracle emits
/// arbitrary reason strings (incl. unknowns for the default branch), so the
/// test leaks the wire string to a `'static` slice. Test-only, tiny corpus.
fn mk_result(next_speaker_id: Option<String>, reason: &str) -> SelectionResult {
    SelectionResult {
        next_speaker_id,
        reason: Box::leak(reason.to_string().into_boxed_str()),
        cycle_complete: false,
        debug: None,
    }
}

#[test]
fn turn_predicates_match_oracle() {
    let path = match std::env::var("QT_ORACLE_TURN_PREDICATES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TURN_PREDICATES to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::UsersTurn {
                id,
                next_speaker_id,
                reason,
                out,
            } => {
                let result = mk_result(next_speaker_id, &reason);
                assert_eq!(is_users_turn(&result), out, "users_turn '{id}'");
            }
            Row::ParticipantsTurn {
                id,
                next_speaker_id,
                reason,
                participant_id,
                out,
            } => {
                let result = mk_result(next_speaker_id, &reason);
                assert_eq!(
                    is_participants_turn(&result, &participant_id),
                    out,
                    "participants_turn '{id}'"
                );
            }
            Row::Explanation { id, reason, out } => {
                let result = mk_result(Some("p1".to_string()), &reason);
                assert_eq!(
                    get_selection_explanation(&result),
                    out,
                    "explanation '{id}'"
                );
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: turn-predicates matched oracle ({count} rows).");
}
