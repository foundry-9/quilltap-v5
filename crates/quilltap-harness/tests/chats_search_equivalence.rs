//! Mixed differential test: v4's `ChatSearchReplaceOps` (Phase-2, the
//! conversation capstone, sub-unit 6 — `countMessagesWithText` /
//! `findMessagesWithText` / `searchMessagesGlobal` / `replaceInMessages`).
//!
//! Both sides run the SAME read queries + replace ops (`chats-search.json`) on a
//! fresh copy of the seed fixture (three chats pre-seeded with a mix of message /
//! context-summary / system events via v4's real `addMessages`). The read methods
//! return VALUES (compared exactly); `replaceInMessages` returns a count AND
//! mutates `chat_messages` (which is then dumped). Because no op touches a
//! timestamp, the differential needs ZERO normalization.
//!
//! Banks: substring count/find across multiple messages; the >1000-char guard
//! (count=0 / []); global search with the role filter (USER/ASSISTANT only —
//! context-summary + system events excluded) + createdAt-DESC ordering + a small
//! `limit` truncation; a regex-special search text (`1.5`, `f(x)`) proving the
//! `$regex`→`LIKE` escape+translate path; and a replace that changes some messages
//! and leaves others (count + content diff).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chsearch-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-search-fixture.ts
//!   QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-search.ts > /tmp/oracle-chsearch.ndjson
//! Run:
//!   QT_ORACLE_CHSEARCH=/tmp/oracle-chsearch.ndjson \
//!   QT_FIXTURE_CHSEARCH=/tmp/qt-chsearch-fixture.db \
//!     cargo test -p quilltap-harness --test chats_search_equivalence

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// Sentinel for the >MAX_SEARCH_QUERY_LENGTH guard (expanded identically both
/// sides — see the oracle case).
const TOO_LONG_SENTINEL: &str = "TOOLONGSEARCHTEXT_REPLACE_AT_RUNTIME";

/// 1001 chars — one over v4's MAX_SEARCH_QUERY_LENGTH (1000).
fn expand_search_text(s: &str) -> String {
    if s == TOO_LONG_SENTINEL {
        "x".repeat(1001)
    } else {
        s.to_string()
    }
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    reads: Vec<ReadOp>,
    replace: Vec<ReplaceOp>,
}

#[derive(Deserialize)]
struct ReadOp {
    kind: String,
    #[serde(default, rename = "chatId")]
    chat_id: Option<String>,
    #[serde(default, rename = "chatIds")]
    chat_ids: Option<Vec<String>>,
    #[serde(rename = "searchText")]
    search_text: String,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct ReplaceOp {
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "searchText")]
    search_text: String,
    #[serde(rename = "replaceText")]
    replace_text: String,
}

#[derive(Deserialize)]
struct OracleRead {
    kind: String,
    result: Value,
}
#[derive(Deserialize)]
struct OracleReplace {
    #[allow(dead_code)]
    kind: String,
    count: i64,
}
#[derive(Deserialize)]
struct Oracle {
    reads: Vec<OracleRead>,
    replace: Vec<OracleReplace>,
    messages: Value,
}

fn run_read(writer: &Writer, op: &ReadOp) -> Value {
    let repo = writer.chat_search();
    let search = expand_search_text(&op.search_text);
    match op.kind.as_str() {
        "countMessagesWithText" => {
            let n = repo
                .count_messages_with_text(op.chat_id.as_deref().unwrap(), &search)
                .expect("count_messages_with_text");
            Value::from(n)
        }
        "findMessagesWithText" => Value::Array(
            repo.find_messages_with_text(op.chat_id.as_deref().unwrap(), &search)
                .expect("find_messages_with_text"),
        ),
        "searchMessagesGlobal" => Value::Array(
            repo.search_messages_global(
                op.chat_ids.as_deref().unwrap(),
                &search,
                op.limit.unwrap(),
            )
            .expect("search_messages_global"),
        ),
        other => panic!("unknown read kind: {other}"),
    }
}

fn assert_dump_eq(got: &Value, oracle: &Value, label: &str) {
    assert_eq!(got["table"], oracle["table"], "{label}: table name");
    assert_eq!(
        got["columns"], oracle["columns"],
        "{label}: column set / order"
    );
    assert_eq!(
        got["rows"], oracle["rows"],
        "{label}: row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );
}

#[test]
fn chats_search_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHSEARCH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHSEARCH to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHSEARCH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHSEARCH to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-search.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-chsearch-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // 1) Read methods (before any mutation).
    assert_eq!(
        spec.reads.len(),
        oracle.reads.len(),
        "read count: spec vs oracle"
    );
    for (i, op) in spec.reads.iter().enumerate() {
        let got = run_read(&writer, op);
        let oracle_read = &oracle.reads[i];
        assert_eq!(oracle_read.kind, op.kind, "read {i}: kind mismatch");
        assert_eq!(
            got,
            oracle_read.result,
            "read {i} ({}): result diverged\n  rust:   {}\n  oracle: {}",
            op.kind,
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&oracle_read.result).unwrap()
        );
    }

    // 2) Replace ops (mutate chat_messages; no timestamp touched).
    assert_eq!(
        spec.replace.len(),
        oracle.replace.len(),
        "replace count: spec vs oracle"
    );
    {
        let repo = writer.chat_search();
        for (i, op) in spec.replace.iter().enumerate() {
            let count = repo
                .replace_in_messages(&op.chat_id, &op.search_text, &op.replace_text)
                .expect("replace_in_messages");
            assert_eq!(
                count, oracle.replace[i].count,
                "replace {i}: count diverged"
            );
        }
    }

    // 3) Post-replace chat_messages dump.
    let got_messages = writer
        .dump_table_json("chat_messages", "id")
        .expect("dump chat_messages");
    let _ = std::fs::remove_file(&work);

    assert_dump_eq(&got_messages, &oracle.messages, "chat_messages");

    eprintln!(
        "OK: chats search matched oracle ({} reads, {} replaces).",
        spec.reads.len(),
        spec.replace.len()
    );
}
