//! Tier-1 differential test (chat-orchestration wave 1.4): the greedy
//! token-budget tail fit, `selectRecentMessages`. Pure function — exact
//! equality on every field.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/message-selector.ts \
//!     > /tmp/oracle-message-selector.ndjson
//! Run:
//!   QT_ORACLE_MESSAGE_SELECTOR=/tmp/oracle-message-selector.ndjson \
//!     cargo test -p quilltap-harness --test message_selector_equivalence

use quilltap_core::message_selector::{select_recent_messages, SelectableMessage};
use quilltap_core::token_estimation::DEFAULT_CHARS_PER_TOKEN;
use serde::Deserialize;

#[derive(Deserialize)]
struct WireMessage {
    role: String,
    content: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "thoughtSignature", default)]
    thought_signature: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "participantId", default)]
    participant_id: Option<String>,
}

impl WireMessage {
    fn into_input(self) -> SelectableMessage {
        SelectableMessage {
            role: self.role,
            content: self.content,
            id: self.id,
            thought_signature: self.thought_signature,
            name: self.name,
            participant_id: self.participant_id,
        }
    }
}

#[derive(Deserialize)]
struct WireResult {
    messages: Vec<WireMessage>,
    #[serde(rename = "tokenCount")]
    token_count: i64,
    truncated: bool,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    messages: Vec<WireMessage>,
    #[serde(rename = "maxTokens")]
    max_tokens: i64,
    out: WireResult,
}

#[test]
fn message_selector_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MESSAGE_SELECTOR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MESSAGE_SELECTOR to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let inputs: Vec<SelectableMessage> = row
            .messages
            .into_iter()
            .map(WireMessage::into_input)
            .collect();

        let got = select_recent_messages(&inputs, row.max_tokens, DEFAULT_CHARS_PER_TOKEN);

        assert_eq!(
            got.messages.len(),
            row.out.messages.len(),
            "message count '{}'",
            row.id
        );
        for (i, (got_msg, want_msg)) in got.messages.iter().zip(row.out.messages.iter()).enumerate()
        {
            assert_eq!(got_msg.role, want_msg.role, "'{}' msg[{i}].role", row.id);
            // a14a1811 (bug 95): the main-loop copy now carries the source row
            // id (the attachment anchor's tier-2 key); the force-include arm
            // still omits it. Compared since P4.D106 — the comparator was
            // id-blind before because the field was deliberately dropped.
            assert_eq!(got_msg.id, want_msg.id, "'{}' msg[{i}].id", row.id);
            assert_eq!(
                got_msg.content, want_msg.content,
                "'{}' msg[{i}].content",
                row.id
            );
            assert_eq!(
                got_msg.thought_signature, want_msg.thought_signature,
                "'{}' msg[{i}].thoughtSignature",
                row.id
            );
            assert_eq!(got_msg.name, want_msg.name, "'{}' msg[{i}].name", row.id);
            assert_eq!(
                got_msg.participant_id, want_msg.participant_id,
                "'{}' msg[{i}].participantId",
                row.id
            );
        }
        assert_eq!(
            got.token_count, row.out.token_count,
            "'{}' tokenCount",
            row.id
        );
        assert_eq!(got.truncated, row.out.truncated, "'{}' truncated", row.id);

        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: message-selector matched oracle ({count} rows).");
}
