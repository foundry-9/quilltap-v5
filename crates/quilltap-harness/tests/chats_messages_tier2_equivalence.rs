//! Tier-2 differential test: v4's `ChatMessagesOps` WRITE path (Phase-2, the
//! conversation capstone, sub-unit 4a — `addMessage` / `addMessages`).
//!
//! Both sides run the SAME op sequence (`chats-messages-tier2.json`) on a fresh
//! copy of the seed fixture (three chats pre-created, `chat_messages` table
//! materialized empty), then BOTH the `chat_messages` table (the write marshaling)
//! and the `chats` table (the metadata side-effect) are dumped canonically and the
//! post-op state is asserted identical.
//!
//! Exercises: the kitchen-sink message marshaling (every JSON column — rawResponse
//! [single-key], the typed nested objects in schema order, the baked DangerFlag
//! defaults, the integer-valued nested numbers rendered bare); a context-summary
//! event (non-actual: no lastMessageAt bump, updatedAt preserved, messageCount 0);
//! a mixed `addMessages` batch (whisper + system event + public message: the
//! folded spokenThisCycle, the visible-message count, the actual-message bump).
//!
//! NORMALIZATION: `chat_messages` — none (ids + createdAt pinned). `chats` —
//! `lastMessageAt`/`updatedAt` collapsed to `<ts>` ONLY when they differ from the
//! seed sentinel (a chat that received only a context-summary keeps its sentinel
//! `updatedAt`, so a stray mint would be caught).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chatsmsg-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-messages-fixture.ts
//!   QT_FIXTURE_CHATSMSG=/tmp/qt-chatsmsg-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-messages-tier2.ts > /tmp/oracle-chatsmsg.ndjson
//! Run:
//!   QT_ORACLE_CHATSMSG=/tmp/oracle-chatsmsg.ndjson \
//!   QT_FIXTURE_CHATSMSG=/tmp/qt-chatsmsg-fixture.db \
//!     cargo test -p quilltap-harness --test chats_messages_tier2_equivalence

use quilltap_core::db::chats_messages::ChatEventInput;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The minted-timestamp columns on the `chats` row (collapsed to `<ts>` when not
/// the sentinel).
const TS_COLUMNS: &[&str] = &["lastMessageAt", "updatedAt"];

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    sentinel: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "addMessage")]
    AddMessage {
        #[serde(rename = "chatId")]
        chat_id: String,
        message: ChatEventInput,
    },
    #[serde(rename = "addMessages")]
    AddMessages {
        #[serde(rename = "chatId")]
        chat_id: String,
        messages: Vec<ChatEventInput>,
    },
}

/// Collapse the minted-timestamp columns to `<ts>` (non-null, non-sentinel only).
fn normalize_chats(dump: &mut Value, sentinel: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("chats dump has no rows array");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row is not an object");
        for col in TS_COLUMNS {
            let mint = obj
                .get(*col)
                .and_then(Value::as_str)
                .is_some_and(|s| s != sentinel);
            if mint {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
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
fn chats_messages_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHATSMSG") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHATSMSG to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHATSMSG") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHATSMSG to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-messages-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-chatsmsg-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.chat_messages();
        for op in &spec.ops {
            match op {
                Op::AddMessage { chat_id, message } => {
                    repo.add_message(chat_id, message).expect("add_message");
                }
                Op::AddMessages { chat_id, messages } => {
                    repo.add_messages(chat_id, messages).expect("add_messages");
                }
            }
        }
    }

    let got_messages = writer
        .dump_table_json("chat_messages", "id")
        .expect("dump chat_messages");
    let mut got_chats = writer.dump_table_json("chats", "id").expect("dump chats");
    let _ = std::fs::remove_file(&work);

    normalize_chats(&mut got_chats, &spec.sentinel);
    let mut oracle_chats = oracle["chats"].clone();
    normalize_chats(&mut oracle_chats, &spec.sentinel);

    assert_dump_eq(&got_messages, &oracle["messages"], "chat_messages");
    assert_dump_eq(&got_chats, &oracle_chats, "chats");

    let nm = got_messages["rows"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(nm > 0, "chat_messages dump looks empty");
    eprintln!("OK: chats messages tier-2 matched oracle ({nm} message rows).");
}
