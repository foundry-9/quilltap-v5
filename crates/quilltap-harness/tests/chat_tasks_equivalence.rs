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
//!     cargo test -p quilltap-harness --test chat_tasks_equivalence

use quilltap_core::chat_tasks::{extract_visible_conversation, strip_tool_artifacts, RawMessage};
use quilltap_core::chat_utils::get_character_chat_preview;
use serde::Deserialize;

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
        // P4.D372: the raw rows as v4 read them, through the ONE
        // `RawMessage::from_event` (R-L1) — so the read itself is compared.
        messages: Vec<serde_json::Value>,
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
                let raw: Vec<RawMessage> = messages.iter().map(RawMessage::from_event).collect();
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

// === P4.D372 === R-L1's census: the raw-row construction of `RawMessage`
// has ONE home (`RawMessage::from_event`). Before P4.D372 there were FIVE
// production literals (`build_context.rs`, `orchestrator.rs`,
// `chat_admin.rs`, `story_background_job.rs`, `title_update_job.rs`), each a
// hand-kept field spill (P4.D205 / P4.D347 — how a field goes missing). After:
// `from_event`'s own literal + the typed `ExistingMessage` arm in
// `build_context.rs` (not a `Value` read). A sixth literal is a new spill.
#[test]
fn raw_message_sites_census() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../quilltap-core/src");
    let mut sites = Vec::new();
    let mut stack = vec![src.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                // Production only: everything before the file's first
                // `#[cfg(test)]` module (the two test-only literals live in
                // `pascal/intervention/mod.rs`'s test module).
                let prod = match text.find("\n#[cfg(test)]\nmod ") {
                    Some(i) => &text[..i],
                    None => &text[..],
                };
                for (n, line) in prod.lines().enumerate() {
                    let l = line.trim_start();
                    if l.contains("RawMessage {")
                        && !l.starts_with("pub struct RawMessage")
                        && !l.starts_with("impl RawMessage")
                        && !l.starts_with("//")
                    {
                        let rel = p.strip_prefix(&src).unwrap().display().to_string();
                        sites.push(format!("{rel}:{}", n + 1));
                    }
                }
            }
        }
    }
    sites.sort();
    let files: Vec<&str> = sites.iter().map(|s| s.split(':').next().unwrap()).collect();
    assert_eq!(
        files,
        ["chat_tasks.rs", "services/build_context.rs"],
        "RawMessage production literals moved: {sites:?}"
    );
}
