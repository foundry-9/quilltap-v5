//! Tier-1 differential test #25 (Wave 5 / B16): chat-task artifact strippers —
//! stripToolArtifacts, extractVisibleConversation, getCharacterChatPreview,
//! exact string / structural equality against the v4 oracle.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-tasks.ts \
//!     > /tmp/oracle-chat-tasks.ndjson
//! Run:
//!   QT_ORACLE_CHAT_TASKS=/tmp/oracle-chat-tasks.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::chat_tasks::{extract_visible_conversation, strip_tool_artifacts, RawMessage};
use quilltap_core::chat_utils::get_character_chat_preview;
use serde::Deserialize;

#[derive(Deserialize)]
struct WRawMsg {
    #[serde(rename = "type", default)]
    type_: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct WChatMsg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "strip")]
    Strip {
        id: String,
        content: String,
        out: Option<String>,
    },
    #[serde(rename = "visible")]
    Visible {
        id: String,
        messages: Vec<WRawMsg>,
        out: Vec<WChatMsg>,
    },
    #[serde(rename = "preview")]
    Preview {
        id: String,
        contents: Vec<String>,
        out: Option<String>,
    },
}

#[test]
fn chat_tasks_match_oracle() {
    let path = match std::env::var("QT_ORACLE_CHAT_TASKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_TASKS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Strip { id, content, out } => {
                assert_eq!(strip_tool_artifacts(&content), out, "strip '{id}'");
            }
            Row::Visible { id, messages, out } => {
                let raw: Vec<RawMessage> = messages
                    .into_iter()
                    .map(|m| RawMessage {
                        type_: m.type_,
                        role: m.role,
                        content: m.content,
                    })
                    .collect();
                let got = extract_visible_conversation(&raw);
                let got_pairs: Vec<WChatMsg> = got
                    .into_iter()
                    .map(|c| WChatMsg {
                        role: c.role,
                        content: c.content,
                    })
                    .collect();
                assert_eq!(got_pairs, out, "visible '{id}'");
            }
            Row::Preview { id, contents, out } => {
                assert_eq!(get_character_chat_preview(&contents), out, "preview '{id}'");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: chat-tasks matched oracle ({count} rows).");
}
