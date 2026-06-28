//! Tier-1 differential test #17 (Wave 3 / B8): context-summary gating cadence.
//!
//! Covers evaluateSummarizationGate, calculateInterchangeCount,
//! shouldCheckTitleAtInterchange, and partitionMessagesIntoTurns. Pure
//! functions — exact equality on every field.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/context-summary.ts \
//!     > /tmp/oracle-context-summary.ndjson
//! Run:
//!   QT_ORACLE_CONTEXT_SUMMARY=/tmp/oracle-context-summary.ndjson cargo test -p quilltap-harness

use quilltap_core::context_summary::{
    calculate_interchange_count, evaluate_summarization_gate, partition_messages_into_turns,
    should_check_title_at_interchange, FoldedTurn, InterchangeMessage, PartitionInputMessage,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct WireMsg {
    id: Option<String>,
    role: Option<String>,
    #[serde(rename = "type")]
    message_type: Option<String>,
    #[serde(rename = "systemSender", default)]
    system_sender: Option<String>,
}

#[derive(Deserialize)]
struct WireTurn {
    #[serde(rename = "turnNumber")]
    turn_number: usize,
    ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "gate")]
    Gate {
        id: String,
        #[serde(rename = "currentTurn")]
        current_turn: i64,
        #[serde(rename = "lastFoldedTurn")]
        last_folded_turn: i64,
        #[serde(rename = "lastFullRebuildTurn")]
        last_full_rebuild_turn: i64,
        out: String,
    },
    #[serde(rename = "interchange")]
    Interchange {
        id: String,
        messages: Vec<WireMsg>,
        #[serde(rename = "chatType", default)]
        chat_type: Option<String>,
        out: i64,
    },
    #[serde(rename = "title")]
    Title {
        id: String,
        current: i64,
        #[serde(rename = "lastChecked")]
        last_checked: i64,
        #[serde(rename = "chatType", default)]
        chat_type: Option<String>,
        out: bool,
    },
    #[serde(rename = "partition")]
    Partition {
        id: String,
        messages: Vec<WireMsg>,
        #[serde(rename = "chatType", default)]
        chat_type: Option<String>,
        out: Vec<WireTurn>,
    },
}

#[test]
fn context_summary_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CONTEXT_SUMMARY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CONTEXT_SUMMARY to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Gate {
                id,
                current_turn,
                last_folded_turn,
                last_full_rebuild_turn,
                out,
            } => {
                let got = evaluate_summarization_gate(
                    current_turn,
                    last_folded_turn,
                    last_full_rebuild_turn,
                );
                assert_eq!(got.as_str(), out, "gate '{id}'");
            }
            Row::Interchange {
                id,
                messages,
                chat_type,
                out,
            } => {
                let msgs: Vec<InterchangeMessage> = messages
                    .iter()
                    .map(|m| InterchangeMessage {
                        role: m.role.clone(),
                        message_type: m.message_type.clone(),
                        system_sender: m.system_sender.clone(),
                    })
                    .collect();
                let got = calculate_interchange_count(&msgs, chat_type.as_deref());
                assert_eq!(got, out, "interchange '{id}'");
            }
            Row::Title {
                id,
                current,
                last_checked,
                chat_type,
                out,
            } => {
                let got =
                    should_check_title_at_interchange(current, last_checked, chat_type.as_deref());
                assert_eq!(got, out, "title '{id}'");
            }
            Row::Partition {
                id,
                messages,
                chat_type,
                out,
            } => {
                let msgs: Vec<PartitionInputMessage> = messages
                    .iter()
                    .map(|m| PartitionInputMessage {
                        id: m.id.clone().unwrap_or_default(),
                        role: m.role.clone(),
                        message_type: m.message_type.clone(),
                        system_sender: m.system_sender.clone(),
                    })
                    .collect();
                let got = partition_messages_into_turns(&msgs, chat_type.as_deref());
                let want: Vec<FoldedTurn> = out
                    .into_iter()
                    .map(|t| FoldedTurn {
                        turn_number: t.turn_number,
                        ids: t.ids,
                    })
                    .collect();
                assert_eq!(got, want, "partition '{id}'");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: context-summary matched oracle ({count} rows).");
}
