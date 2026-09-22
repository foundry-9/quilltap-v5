//! P4.D208 differential: the greeting's "Recent Conversations" block (v4 bug
//! 158, `da9c4f34f`) vs v4's REAL `buildRecentConversationsBlock`
//! (`lib/memory/memory-recap.ts`).
//!
//! Both sides read the SAME baked fixture — nine chats created by v4's real
//! `repos.chats.create` — and render the block for the same
//! (characterId, currentChatId, limit) calls. The rendered string compares
//! EXACTLY: nothing is mutated, so no timestamp is ever minted.
//!
//! Driving v4's real repository rather than a mock is the point. v4's own new
//! test file mocks `findRecentSummarizedByCharacter`, so it pins the render but
//! not the read; here the read's `contextSummary IS NOT NULL` filter is under
//! test too, which is the only thing that makes the empty-gist arm reachable —
//! the NULL-summary chat never arrives, the whitespace-only one does.
//!
//! The corpus covers every arm the fix introduced: the 445-character scenario
//! capped at 280 with an ellipsis (the bug's own shape), 280 exactly NOT capped
//! against 281 which is, a short summary left intact, a padded one trimmed
//! before measuring, a whitespace-only one rendering its heading ALONE, and the
//! closing `READ_CONVERSATION_CALL_NOTE`.
//!
//! `repoCalls` is the instrument for v4's `limit <= 0` short circuit, which is
//! otherwise indistinguishable from "the query came back empty". v5's analogue
//! is the WARN: the read is given a connection with no `chats` table at all, so
//! a v5 that reached the repository would log
//! `[Chats v1] Failed to build recent-conversations block for greeting` and
//! answer `''` — the same empty string by a different route. Silence proves the
//! short circuit.
//!
//! Generate the fixture + oracle (Node 24, from a v4 checkout pinned at the
//! TARGET — `buildRecentConversationsBlock` renders differently at the
//! baseline, which is this family's pin proof):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_RECENT_CONVS=/tmp/qt-recent-convs.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-recent-conversations-fixture.ts
//!   QT_FIXTURE_RECENT_CONVS=/tmp/qt-recent-convs.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/recent-conversations-block.ts \
//!     > /tmp/oracle-recent-conversations-block.ndjson
//! Run:
//!   QT_ORACLE_RECENT_CONVS=/tmp/oracle-recent-conversations-block.ndjson \
//!   QT_FIXTURE_RECENT_CONVS=/tmp/qt-recent-convs.db \
//!     cargo test -p quilltap-harness --test recent_conversations_block_equivalence

use std::path::PathBuf;

use quilltap_core::db::Writer;
use quilltap_core::services::chat_create::build_recent_conversations_block;
use serde::Deserialize;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

#[derive(Deserialize)]
struct Row {
    label: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "currentChatId")]
    current_chat_id: Option<String>,
    limit: i64,
    block: String,
    #[serde(rename = "repoCalls")]
    repo_calls: u32,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/recent-conversations-block.json")
}

#[test]
fn recent_conversations_block_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_RECENT_CONVS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_RECENT_CONVS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_RECENT_CONVS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_RECENT_CONVS to the fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"));

    let pid = std::process::id();
    let work = std::env::temp_dir().join(format!("qt-recent-convs-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open: {e}"));

    let mut count = 0usize;
    let mut capped = 0usize;
    let mut heading_only = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got = build_recent_conversations_block(
            writer.connection(),
            &row.character_id,
            row.current_chat_id.as_deref(),
            row.limit,
        );
        assert_eq!(
            got, row.block,
            "buildRecentConversationsBlock '{}' (limit={})",
            row.label, row.limit
        );
        if row.block.contains('…') {
            capped += 1;
        }
        // A heading whose next line is blank or the closing note is the
        // empty-gist arm — v4 renders NO body line at all, not an empty one.
        if row
            .block
            .contains("(`cc000003-0000-4000-8000-000000000003`)\n\n")
        {
            heading_only += 1;
        }
        // Every non-empty block closes with the note; no empty one carries it.
        if row.block.is_empty() {
            assert_eq!(row.repo_calls == 0, row.limit <= 0, "'{}'", row.label);
        } else {
            assert!(
                row.block.ends_with(
                    "_Pass any of the conversation IDs above (in backticks) to the \
                     `read_conversation` tool to revisit the full transcript._"
                ),
                "'{}' must close with READ_CONVERSATION_CALL_NOTE",
                row.label
            );
        }
        count += 1;
    }
    assert!(count > 0, "oracle file looks empty");
    assert!(capped > 0, "no corpus row exercised the 280-char cap");
    assert!(
        heading_only > 0,
        "no corpus row exercised the empty-gist arm — the whitespace-only chat \
         must survive the read's `contextSummary IS NOT NULL` filter"
    );

    let _ = std::fs::remove_file(&work);
    eprintln!("OK: recent-conversations block matched oracle ({count} calls).");
}

/// v4's `limit <= 0` short circuit: it returns `''` WITHOUT asking the
/// repository anything (`repoCalls: 0` in the oracle). v5's analogue is proven
/// by SILENCE — the read is handed a connection with no `chats` table, so a v5
/// that reached it would take the swallowed-read arm and warn. The same empty
/// string comes back either way, so the log line is the only witness.
#[test]
fn limit_zero_asks_the_repository_nothing() {
    use quilltap_core::test_support::captured_with;

    let conn = rusqlite::Connection::open_in_memory().expect("in-memory connection");

    for limit in [0i64, -3] {
        let (out, logs) = captured_with(|| {
            build_recent_conversations_block(&conn, "char-1", Some("chat-1"), limit)
        });
        assert_eq!(out, "", "limit {limit} must render nothing");
        assert!(
            logs.is_empty(),
            "limit {limit} reached the repository — it must short-circuit first: {logs:?}"
        );
    }

    // The firing pin: with a positive limit the SAME broken connection does
    // reach the read and does warn, so the silence above is a short circuit and
    // not a dead instrument.
    let (out, logs) = captured_with(|| build_recent_conversations_block(&conn, "char-1", None, 5));
    assert_eq!(out, "");
    assert!(
        logs.iter()
            .any(|l| l
                .contains("[Chats v1] Failed to build recent-conversations block for greeting")),
        "a positive limit over a broken connection must warn: {logs:?}"
    );
}
