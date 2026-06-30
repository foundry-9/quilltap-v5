//! Tier-2 differential test: v4's `ChatMessagesOps` mutation path (Phase-2, the
//! conversation capstone, sub-unit 4b — `updateMessage` / `deleteMessagesByIds` /
//! `clearMessages`).
//!
//! Both sides run the SAME op sequence (`chats-messages-ops-tier2.json`) on a
//! fresh copy of the seed fixture (three chats pre-seeded with messages via v4's
//! real `addMessages`), then BOTH the `chat_messages` and `chats` tables are
//! dumped canonically and the post-op state is asserted identical.
//!
//! Exercises: `updateMessage` (a message — scalar + number + a freshly-added
//! `dangerFlags` JSON column whose defaults bake, while the untouched
//! `reasoningSegments` round-trips byte-for-byte; a context-summary's `context`
//! edit; and a not-found id that no-ops); `deleteMessagesByIds` (remove two of
//! three, recount messageCount; then a nonexistent id that removes nothing and
//! leaves metadata untouched); `clearMessages` (delete all, messageCount→0,
//! lastMessageAt→null, updatedAt preserved).
//!
//! NORMALIZATION: NONE. The seed's minted timestamps are baked once and read by
//! both sides, and no 4b op mints a new chat timestamp, so every cell is pinned.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chatsmsgops-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-messages-ops-fixture.ts
//!   QT_FIXTURE_CHATSMSGOPS=/tmp/qt-chatsmsgops-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-messages-ops-tier2.ts > /tmp/oracle-chatsmsgops.ndjson
//! Run:
//!   QT_ORACLE_CHATSMSGOPS=/tmp/oracle-chatsmsgops.ndjson \
//!   QT_FIXTURE_CHATSMSGOPS=/tmp/qt-chatsmsgops-fixture.db \
//!     cargo test -p quilltap-harness --test chats_messages_ops_tier2_equivalence

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "updateMessage")]
    UpdateMessage {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        updates: Value,
    },
    #[serde(rename = "deleteMessagesByIds")]
    DeleteMessagesByIds {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "messageIds")]
        message_ids: Vec<String>,
    },
    #[serde(rename = "clearMessages")]
    ClearMessages {
        #[serde(rename = "chatId")]
        chat_id: String,
    },
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
fn chats_messages_ops_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHATSMSGOPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHATSMSGOPS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHATSMSGOPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHATSMSGOPS to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-messages-ops-tier2.json"),
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

    let work = std::env::temp_dir().join(format!("qt-chatsmsgops-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.chat_messages();
        for op in &spec.ops {
            match op {
                Op::UpdateMessage {
                    chat_id,
                    message_id,
                    updates,
                } => {
                    repo.update_message(chat_id, message_id, updates)
                        .expect("update_message");
                }
                Op::DeleteMessagesByIds {
                    chat_id,
                    message_ids,
                } => {
                    repo.delete_messages_by_ids(chat_id, message_ids)
                        .expect("delete_messages_by_ids");
                }
                Op::ClearMessages { chat_id } => {
                    repo.clear_messages(chat_id).expect("clear_messages");
                }
            }
        }
    }

    let got_messages = writer
        .dump_table_json("chat_messages", "id")
        .expect("dump chat_messages");
    let got_chats = writer.dump_table_json("chats", "id").expect("dump chats");
    let _ = std::fs::remove_file(&work);

    assert_dump_eq(&got_messages, &oracle["messages"], "chat_messages");
    assert_dump_eq(&got_chats, &oracle["chats"], "chats");

    eprintln!("OK: chats messages ops tier-2 matched oracle.");
}
