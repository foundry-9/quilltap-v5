//! Tier-2 differential test: the memory **cascade deletes**, re-pointed by
//! P4.D288 (C1-7, the `83334b9a5` pin) onto the ONE retire path. v4
//! `af48f6cc2` deleted `lib/memory/memory-service.ts`'s four `…WithVectors`
//! functions; the committed op list (`memory-cascade-tier2.json`, §R.7
//! unchanged) is replayed through each one's successor at the pin, the same
//! way on both sides (the oracle header carries the mapping) —
//! `quilltap_core::services::memory_retire::retire_and_announce` /
//! `retire_chat_memories` here.
//!
//! Both sides run the SAME ops on a fresh copy of the pre-seeded fixture
//! (eleven memories across six characters, cross-character `relatedMemoryIds`
//! links, two rows with NO embedding), then `memories` is dumped canonically
//! and diffed. Each op's RETURN is asserted against the spec's `expect` (both
//! sides independently) — the ownership-mismatch / missing-row /
//! empty-cascade branches leave no trace in the final state, so the returns
//! are their proof. `vectorsRemoved` is the LOADED index's count on both sides
//! (L-2, `MEMORY_INDEX_ALWAYS_LOADED`: the oracle loads v4's store first).
//!
//! NORMALIZATION: sentinel-aware minted-timestamp placeholder on
//! `memories.updatedAt` (the neighbour scrub mints); everything else is pinned
//! by the fixture builder, so ids and all other cells diff exactly.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-mem-cascade-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-cascade-fixture.ts
//!   QT_FIXTURE_MEMCASCADE=/tmp/qt-mem-cascade-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-cascade-tier2.ts > /tmp/oracle-mem-cascade.ndjson
//! Run:
//!   QT_ORACLE_MEMCASCADE=/tmp/oracle-mem-cascade.ndjson \
//!   QT_FIXTURE_MEMCASCADE=/tmp/qt-mem-cascade-fixture.db \
//!     cargo test -p quilltap-harness --test memory_cascade_tier2_equivalence

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::memories_read;
use quilltap_core::db::runtime::Db;
use quilltap_core::services::memory_retire::{
    announce_retirement, retire_and_announce, retire_chat_memories, RetireOptions, RetireReason,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "seedTimestamp")]
    seed_timestamp: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "deleteMemoryWithVector")]
    OneWithVector {
        #[serde(rename = "characterId")]
        character_id: String,
        #[serde(rename = "memoryId")]
        memory_id: String,
        expect: Value,
    },
    #[serde(rename = "deleteMemoriesBySourceMessage")]
    BySourceMessage {
        #[serde(rename = "sourceMessageId")]
        source_message_id: String,
        expect: Value,
    },
    #[serde(rename = "deleteMemoriesBySourceMessages")]
    BySourceMessages {
        #[serde(rename = "sourceMessageIds")]
        source_message_ids: Vec<String>,
        expect: Value,
    },
    #[serde(rename = "deleteMemoriesByChatId")]
    ByChatId {
        #[serde(rename = "chatId")]
        chat_id: String,
        expect: Value,
    },
}

/// Collapse a non-sentinel `updatedAt` to `<ts>`, in place (sentinel = seed value).
fn normalize(dump: &mut Value, sentinel: &str, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        let bump = matches!(obj.get("updatedAt"), Some(Value::String(s)) if s != sentinel);
        if bump {
            obj.insert("updatedAt".to_string(), Value::String("<ts>".to_string()));
        }
    }
}

/// Pick the NDJSON line whose `table` field matches from the multi-line oracle.
fn oracle_table(oracle_text: &str, table: &str) -> Value {
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if v.get("table").and_then(Value::as_str) == Some(table) {
            return v;
        }
    }
    panic!("oracle ndjson missing table {table}");
}

const TABLES: &[&str] = &["memories"];

#[tokio::test]
async fn memory_cascade_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MEMCASCADE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEMCASCADE to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_MEMCASCADE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MEMCASCADE to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/memory-cascade-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let work_scratch = tempfile::Builder::new()
        .prefix("qt-mem-cascade-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // The messages route's shape: every memory the message(s) sourced, then
    // ONE retirement (`message-cascade`).
    async fn by_source_messages(db: &Db, ids: &[String]) -> Value {
        let mut memory_ids: Vec<String> = Vec::new();
        for id in ids {
            let smid = id.clone();
            let rows = db
                .read_main(move |c| memories_read::find_by_source_message_id(c, &smid))
                .expect("findBySourceMessageId");
            memory_ids.extend(
                rows.iter()
                    .filter_map(|m| m["id"].as_str().map(str::to_string)),
            );
        }
        let r = retire_and_announce(
            db,
            memory_ids,
            RetireOptions::new(RetireReason::MessageCascade),
        )
        .await
        .expect("retire");
        json!({ "deleted": r.retired, "vectorsRemoved": r.vectors_removed })
    }

    let mut returns: Vec<Value> = Vec::new();
    for (i, op) in spec.ops.iter().enumerate() {
        let got = match op {
            Op::OneWithVector {
                character_id,
                memory_id,
                expect,
            } => {
                // The item route's shape: the owner checked, then the retire.
                let mid = memory_id.clone();
                let owner = db
                    .read_main(move |c| memories_read::find_by_id(c, &mid))
                    .expect("findById")
                    .and_then(|m| m["characterId"].as_str().map(str::to_string));
                let ok = if owner.as_deref() == Some(character_id.as_str()) {
                    retire_and_announce(
                        &db,
                        vec![memory_id.clone()],
                        RetireOptions::new(RetireReason::ItemDelete),
                    )
                    .await
                    .unwrap_or_else(|e| panic!("op[{i}] retire: {e:?}"))
                    .retired
                        > 0
                } else {
                    false
                };
                let got = json!({ "ok": ok });
                assert_eq!(&got, expect, "op[{i}] return");
                got
            }
            Op::BySourceMessage {
                source_message_id,
                expect,
            } => {
                let got = by_source_messages(&db, std::slice::from_ref(source_message_id)).await;
                assert_eq!(&got, expect, "op[{i}] return");
                got
            }
            Op::BySourceMessages {
                source_message_ids,
                expect,
            } => {
                let got = by_source_messages(&db, source_message_ids).await;
                assert_eq!(&got, expect, "op[{i}] return");
                got
            }
            Op::ByChatId { chat_id, expect } => {
                let cid = chat_id.clone();
                let r = db
                    .write(move |w| retire_chat_memories(w.main().connection(), &cid))
                    .await
                    .unwrap_or_else(|e| panic!("op[{i}] retireChatMemories: {e:?}"));
                announce_retirement(&db, r.announcement.as_ref()).await;
                let got = json!({
                    "deleted": r.retired,
                    "vectorsRemoved": r.vectors_removed,
                    "characterCount": r.characters_affected,
                });
                assert_eq!(&got, expect, "op[{i}] return");
                got
            }
        };
        returns.push(got);
    }
    let oracle_returns = oracle_text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .find_map(|v| v.get("returns").cloned())
        .expect("the oracle carries no `returns` line");
    assert_eq!(Value::Array(returns), oracle_returns, "the per-op returns");

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t, "id"))
                .unwrap_or_else(|e| panic!("dump {t}: {e:?}"))
        })
        .collect();
    drop(db);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| oracle_table(&oracle_text, t))
        .collect();

    for (i, t) in TABLES.iter().enumerate() {
        normalize(&mut got[i], &spec.seed_timestamp, &format!("rust {t}"));
        normalize(&mut want[i], &spec.seed_timestamp, &format!("oracle {t}"));
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{t} column set / order"
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{t} row state diverged\n  rust:   {}\n  oracle: {}",
            got[i]["rows"], want[i]["rows"]
        );
    }

    // Sanity: three survivors (mem-a4, mem-e2, mem-f1).
    assert_eq!(
        got[0]["rows"].as_array().unwrap().len(),
        3,
        "memories survivors"
    );

    eprintln!("OK: memory-cascade tier-2 matched oracle (memories + the per-op returns).");
}
