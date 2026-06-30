//! Read-differential test: v4's message read surface (`ChatMessagesOps`:
//! getMessages / getMessageCount / findChatIdForMessage). Phase-2, the chats repo
//! — the conversation capstone, sub-unit 3 (chat_messages read path).
//!
//! Both sides READ the SAME baked fixture (one chat + twelve messages added by
//! v4's real `repos.chats.addMessages`, ids + timestamps pinned), run the SAME
//! query list, and the results compare **exactly** — NO normalization, because
//! nothing is mutated. Every message's Zod defaults are baked at write time, so
//! the hydrated `getMessages` array is the per-member parsed event (JSON columns
//! parsed, numbers JS-rendered, nullable-optionals dropped).
//!
//! The Rust port drives [`chats_messages_read`]'s functions over one connection;
//! v4 drives the real repository methods (see the oracle).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHATSMSGREAD=/tmp/qt-chatsmsgread.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-messages-read-fixture.ts
//!   QT_FIXTURE_CHATSMSGREAD=/tmp/qt-chatsmsgread.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-messages-read.ts > /tmp/oracle-chatsmsgread.ndjson
//! Run:
//!   QT_ORACLE_CHATSMSGREAD=/tmp/oracle-chatsmsgread.ndjson \
//!   QT_FIXTURE_CHATSMSGREAD=/tmp/qt-chatsmsgread.db \
//!     cargo test -p quilltap-harness --test chats_messages_read_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::chats_messages_read as cmr;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Query {
    kind: String,
    #[serde(default, rename = "chatId")]
    chat_id: Option<String>,
    #[serde(default, rename = "messageId")]
    message_id: Option<String>,
}

#[derive(Deserialize)]
struct OracleQuery {
    kind: String,
    result: Value,
}
#[derive(Deserialize)]
struct Oracle {
    queries: Vec<OracleQuery>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chats-messages-read-tier2.json")
}

fn run_query(writer: &Writer, q: &Query) -> Value {
    let conn = writer.connection();
    match q.kind.as_str() {
        "getMessages" => Value::Array(
            cmr::get_messages(conn, q.chat_id.as_deref().unwrap()).expect("getMessages"),
        ),
        "getMessageCount" => Value::from(
            cmr::get_message_count(conn, q.chat_id.as_deref().unwrap()).expect("getMessageCount"),
        ),
        "findChatIdForMessage" => {
            cmr::find_chat_id_for_message(conn, q.message_id.as_deref().unwrap())
                .expect("findChatIdForMessage")
                .map(Value::String)
                .unwrap_or(Value::Null)
        }
        other => panic!("unknown query kind: {other}"),
    }
}

#[test]
fn chats_messages_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHATSMSGREAD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHATSMSGREAD to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHATSMSGREAD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHATSMSGREAD to the fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let pid = std::process::id();
    let work = std::env::temp_dir().join(format!("qt-chatsmsgread-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open: {e}"));

    assert_eq!(
        spec.queries.len(),
        oracle.queries.len(),
        "query count: spec vs oracle"
    );

    for (i, q) in spec.queries.iter().enumerate() {
        let got = run_query(&writer, q);
        let oq = &oracle.queries[i];
        assert_eq!(oq.kind, q.kind, "query {i}: kind mismatch");
        assert_eq!(
            got,
            oq.result,
            "query {i} ({}): result diverged\n  rust:   {}\n  oracle: {}",
            q.kind,
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&oq.result).unwrap()
        );
    }

    let _ = std::fs::remove_file(&work);
    eprintln!(
        "OK: chats messages read matched oracle ({} queries).",
        spec.queries.len()
    );
}
