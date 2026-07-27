//! Tier-1 EXACT differential — the Scriptorium conversation renderer (P4.6BM:
//! `quilltap_core::services::conversation_markdown::render_conversation_markdown`
//! vs v4's real `renderConversationMarkdown`,
//! `lib/scriptorium/markdown-renderer.ts`).
//!
//! Both sides run the SAME committed corpus
//! (`harness/oracle/fixtures/conversation-markdown.json`) and the full
//! `{ markdown, interchanges }` return compares **byte-for-byte** — no
//! normalization anywhere. The renderer's one impure read (the header's
//! `Current time:` line) is the corpus `nowIso`, frozen on the oracle side by
//! replacing `globalThis.Date` and injected on the Rust side as a parameter.
//!
//! `TZ=UTC` is load-bearing on BOTH sides: v4 formats through
//! `toLocaleDateString('en-US')` / `toLocaleTimeString('en-US')` with no
//! explicit zone, so the timestamps follow the host zone; the port computes in
//! UTC.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx $V5/harness/oracle/cases/conversation-markdown.ts \
//!     > /tmp/oracle-conversation-markdown.ndjson
//! Run:
//!   QT_ORACLE_CONVERSATION_MARKDOWN=/tmp/oracle-conversation-markdown.ndjson \
//!     cargo test -p quilltap-harness --test conversation_markdown_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::services::conversation_markdown::{
    render_conversation_markdown, ConversationMetadata, RenderEvent,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageSeed {
    id: String,
    #[serde(rename = "type")]
    type_: String,
    role: String,
    content: String,
    participant_id: Option<String>,
    created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataSeed {
    conversation_id: String,
    title: String,
    created_at: String,
    last_updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseSeed {
    name: String,
    character_names: Vec<(String, String)>,
    metadata: Option<MetadataSeed>,
    messages: Vec<MessageSeed>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    now_iso: String,
    cases: Vec<CaseSeed>,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct OracleInterchange {
    index: usize,
    message_ids: Vec<String>,
    participant_names: Vec<String>,
    content: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct OracleRow {
    name: String,
    markdown: String,
    interchanges: Vec<OracleInterchange>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/conversation-markdown.json")
}

#[test]
fn conversation_markdown_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CONVERSATION_MARKDOWN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CONVERSATION_MARKDOWN to the oracle NDJSON (see header)."
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let rows: Vec<OracleRow> = oracle_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();

    // Corpus-shape assertion (not a hand count): the oracle ran EXACTLY this
    // corpus, case-for-case in order — a truncated or stale NDJSON fails here
    // rather than passing on a prefix.
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        spec.cases
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        "oracle case set/order diverges from the corpus — regenerate"
    );

    for (case, want) in spec.cases.iter().zip(rows.iter()) {
        let messages: Vec<RenderEvent> = case
            .messages
            .iter()
            .map(|m| RenderEvent {
                id: m.id.clone(),
                type_: Some(m.type_.clone()),
                role: Some(m.role.clone()),
                content: Some(m.content.clone()),
                participant_id: m.participant_id.clone(),
                created_at: m.created_at.clone(),
            })
            .collect();
        let metadata = case.metadata.as_ref().map(|m| ConversationMetadata {
            conversation_id: m.conversation_id.clone(),
            title: m.title.clone(),
            created_at: m.created_at.clone(),
            last_updated_at: m.last_updated_at.clone(),
        });
        let got = render_conversation_markdown(
            &messages,
            &case.character_names,
            metadata.as_ref(),
            &spec.now_iso,
        );

        assert_eq!(
            got.markdown, want.markdown,
            "case {}: markdown diverges\n got: {:?}\nwant: {:?}",
            case.name, got.markdown, want.markdown
        );
        assert_eq!(
            got.interchanges.len(),
            want.interchanges.len(),
            "case {}: interchange count diverges",
            case.name
        );
        for (i, (g, w)) in got
            .interchanges
            .iter()
            .zip(want.interchanges.iter())
            .enumerate()
        {
            assert_eq!(
                g.index, w.index,
                "case {}: interchange {i} index",
                case.name
            );
            assert_eq!(
                g.message_ids, w.message_ids,
                "case {}: interchange {i} messageIds",
                case.name
            );
            assert_eq!(
                g.participant_names, w.participant_names,
                "case {}: interchange {i} participantNames",
                case.name
            );
            assert_eq!(
                g.content, w.content,
                "case {}: interchange {i} content diverges\n got: {:?}\nwant: {:?}",
                case.name, g.content, w.content
            );
        }
    }

    println!(
        "conversation_markdown: {} cases OK ({} interchanges compared)",
        rows.len(),
        rows.iter().map(|r| r.interchanges.len()).sum::<usize>()
    );
}
