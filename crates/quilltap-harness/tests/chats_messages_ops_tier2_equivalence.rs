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
    // ── P4.D183: the rest of the funnel, plus the two must-NOT-bump arms ──
    #[serde(rename = "addMessage")]
    AddMessage {
        #[serde(rename = "chatId")]
        chat_id: String,
        message: Value,
    },
    #[serde(rename = "addMessages")]
    AddMessages {
        #[serde(rename = "chatId")]
        chat_id: String,
        messages: Vec<Value>,
    },
    #[serde(rename = "replaceInMessages")]
    ReplaceInMessages {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "searchText")]
        search_text: String,
        #[serde(rename = "replaceText")]
        replace_text: String,
    },
    #[serde(rename = "chatUpdate")]
    ChatUpdate {
        #[serde(rename = "chatId")]
        chat_id: String,
        data: Value,
    },
}

/// **A v4 BUG this port found, pinned in BOTH directions** (P4.D183).
///
/// v4's `deleteMessagesByIds` counts a requested id as removed whether or not
/// it existed:
///
/// ```js
/// const result = await messagesCollection.deleteOne({ id: messageId, chatId });
/// if (typeof result === 'number') { removed += result; }
/// else if (result) { removed += 1; }          // ← an OBJECT is always truthy
/// ```
///
/// The real SQLite backend returns `{ deletedCount: 0, acknowledged: true }`
/// for a miss (`backends/sqlite/backend.ts:289`) — never a number — so the
/// second branch fires and `removed` ends up as `messageIds.length`. v4 then
/// takes the `removed > 0` path: it rewrites `messageCount`/`lastMessageAt`
/// and, since `5029075bb`, BUMPS the transcript counter and publishes a hint
/// for a delete that deleted nothing.
///
/// v4's own unit suite cannot see this. `chats-messages-transcript-version.
/// test.ts:57` mocks `deleteOne` to return a NUMBER (`0` / `1`), which takes
/// the first branch and behaves correctly — so its "says nothing when nothing
/// was removed" passes against a mock that does not match its own production
/// backend. It took a real-DB oracle to expose it, and it only became VISIBLE
/// when the counter arrived: `messageCount` and `lastMessageAt` are recomputed
/// from what survives, so they land on the same values either way.
///
/// **v5 is left correct.** Reproducing this would mean regressing a v5 path
/// that works, and miscounting deletions for every caller of the return value.
/// The divergence is recorded here instead, in both directions, so it cannot
/// drift: the moment v4 fixes it the oracle's value becomes v5's and this
/// tripwire fires, naming itself.
///
/// FILED UPSTREAM: see the lane record.
const DELETE_MISS_DIVERGENCE: (&str, i64, i64) = (
    // (chat id, what v5 writes, what v4 writes)
    "c0000020-0000-4000-8000-000000000001",
    2,
    3,
);

/// The `addMessage` / `addMessages` ops MINT `updatedAt` / `lastMessageAt`
/// from the wall clock, so those two cells cannot be compared on the chat they
/// touch — the rest of the corpus keeps this family's zero-normalization
/// property (v4 `5029075bb` is the first change to put a minting op in it).
/// Both sides are asserted to be a plausible ISO timestamp rather than simply
/// dropped, so a NULL or an empty string still reds.
const ADD_MINTS_TIMESTAMPS_ON: &str = "c0000060-0000-4000-8000-000000000001";

/// Apply the two carve-outs above to a `chats` dump, asserting each one is
/// REAL (present and of the expected shape) before neutralizing it — a
/// carve-out nothing exercises is a hole, not an exemption.
fn apply_chats_carve_outs(got: &mut Value, oracle: &mut Value) {
    let (div_chat, v5_expected, v4_expected) = DELETE_MISS_DIVERGENCE;
    let mut saw_divergence = false;
    let mut saw_mint = false;
    for (side, dump, expected) in [
        ("rust", &mut *got, v5_expected),
        ("oracle", &mut *oracle, v4_expected),
    ] {
        let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) else {
            continue;
        };
        for row in rows {
            let Some(o) = row.as_object_mut() else {
                continue;
            };
            let id = o
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if id == div_chat {
                let actual = o.get("transcriptVersion").and_then(Value::as_i64);
                assert_eq!(
                    actual,
                    Some(expected),
                    "[{side}] the delete-miss divergence has MOVED on {div_chat}:                      expected {expected}, got {actual:?}. If v4 fixed the                      truthy-object bug, retire `DELETE_MISS_DIVERGENCE` to a plain                      equality; if v5 changed, that is a regression."
                );
                o.insert(
                    "transcriptVersion".into(),
                    Value::String("<divergent>".into()),
                );
                saw_divergence = true;
            }
            if id == ADD_MINTS_TIMESTAMPS_ON {
                for key in ["updatedAt", "lastMessageAt"] {
                    let v = o.get(key).and_then(Value::as_str).unwrap_or("");
                    assert!(
                        v.len() == 24 && v.ends_with('Z'),
                        "[{side}] {key} on the add chat should be a minted ISO                          timestamp, got {v:?} — the carve-out must not hide a NULL"
                    );
                    o.insert((*key).into(), Value::String("<minted>".into()));
                }
                saw_mint = true;
            }
        }
    }
    assert!(
        saw_divergence && saw_mint,
        "both carve-out rows must be PRESENT in the dump (divergence: \
         {saw_divergence}, mint: {saw_mint}) — a carve-out whose row has left \
         the corpus is measuring nothing"
    );
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
                Op::AddMessage { chat_id, message } => {
                    let e = serde_json::from_value(message.clone()).expect("parse add message");
                    repo.add_message(chat_id, &e).expect("add_message");
                }
                Op::AddMessages { chat_id, messages } => {
                    let es: Vec<_> = messages
                        .iter()
                        .map(|m| serde_json::from_value(m.clone()).expect("parse add messages"))
                        .collect();
                    repo.add_messages(chat_id, &es).expect("add_messages");
                }
                Op::ReplaceInMessages {
                    chat_id,
                    search_text,
                    replace_text,
                } => {
                    quilltap_core::db::chats_search::ChatSearchRepository::new(writer.connection())
                        .replace_in_messages(chat_id, search_text, replace_text)
                        .expect("replace_in_messages");
                }
                Op::ChatUpdate { chat_id, data } => {
                    // `ChatUpdate` is a hand-built setter struct, not a
                    // Deserialize target (the `double_option` tri-states make
                    // a derive the wrong shape), so the spec's two keys are
                    // read explicitly. Keep this in step with the spec's
                    // `chatUpdate` op if it grows a field.
                    let patch = quilltap_core::db::chats::ChatUpdate {
                        title: data
                            .get("title")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        message_count: data.get("messageCount").and_then(Value::as_f64),
                        ..Default::default()
                    };
                    assert!(
                        patch.title.is_some() && patch.message_count.is_some(),
                        "the chatUpdate op must actually carry both keys, or the \
                         must-NOT-bump arm proves nothing: {data}"
                    );
                    quilltap_core::db::chats::ChatsRepository::new(writer.connection())
                        .update(chat_id, &patch)
                        .expect("chats update");
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
    let mut got_chats = got_chats;
    let mut want_chats = oracle["chats"].clone();
    apply_chats_carve_outs(&mut got_chats, &mut want_chats);
    assert_dump_eq(&got_chats, &want_chats, "chats");

    eprintln!("OK: chats messages ops tier-2 matched oracle.");
}
