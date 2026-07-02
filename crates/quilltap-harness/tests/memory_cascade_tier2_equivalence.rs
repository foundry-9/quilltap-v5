//! Tier-2 differential test: the memory-service **cascade-delete family** (v4
//! `lib/memory/memory-service.ts` `deleteMemoryWithVector` /
//! `deleteMemoriesBySourceMessageWithVectors` /
//! `deleteMemoriesBySourceMessagesWithVectors` /
//! `deleteMemoriesByChatIdWithVectors`), ported as
//! `quilltap_core::services::memory_service`.
//!
//! Both sides run the SAME op sequence (`memory-cascade-tier2.json`) on a fresh
//! copy of the pre-seeded fixture (eleven memories across six characters, with
//! cross-character `relatedMemoryIds` links and per-character vector stores —
//! including two memories with NO vector entry and one character with a metadata
//! row but no entries), then `memories` + `vector_indices` + `vector_entries` are
//! dumped canonically and diffed. Each op's RETURN value is also asserted against
//! the spec (both sides independently) — the ownership-mismatch / missing-row /
//! empty-cascade branches leave no trace in the final state, so the returns are
//! their proof.
//!
//! What the final state proves: the ownership gate, the chokepoint neighbour
//! scrub surviving on `mem-e2` (related [] + minted `updatedAt`), `hasVector`
//! counting (`vectorsRemoved` 2-of-3 on the chat wipe), the untouched-store
//! sentinel (char-C / char-E / char-F metadata `updatedAt` stays pinned — a store
//! whose sweep removed nothing must NOT be flushed), per-character grouping
//! across two DB tables, and full survivor scoping (mem-a4 / mem-e2 / mem-f1 rows
//! + entries intact).
//!
//! NORMALIZATION: sentinel-aware minted-timestamp placeholder on
//! `memories.updatedAt` (neighbour scrub mints) and `vector_indices.updatedAt`
//! (a flush that removed entries mints via `saveMeta`); everything else is pinned
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
use quilltap_core::db::runtime::Db;
use quilltap_core::services::memory_service::{
    delete_memories_by_chat_id_with_vectors, delete_memories_by_source_message_with_vectors,
    delete_memories_by_source_messages_with_vectors, delete_memory_with_vector,
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

const TABLES: &[&str] = &["memories", "vector_indices", "vector_entries"];

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

    let work = std::env::temp_dir().join(format!("qt-mem-cascade-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    for (i, op) in spec.ops.iter().enumerate() {
        match op {
            Op::OneWithVector {
                character_id,
                memory_id,
                expect,
            } => {
                let ok = delete_memory_with_vector(&db, character_id, memory_id)
                    .await
                    .unwrap_or_else(|e| panic!("op[{i}] deleteMemoryWithVector: {e:?}"));
                assert_eq!(&json!({ "ok": ok }), expect, "op[{i}] return");
            }
            Op::BySourceMessage {
                source_message_id,
                expect,
            } => {
                let r = delete_memories_by_source_message_with_vectors(&db, source_message_id)
                    .await
                    .unwrap_or_else(|e| panic!("op[{i}] bySourceMessage: {e:?}"));
                assert_eq!(
                    &json!({ "deleted": r.deleted, "vectorsRemoved": r.vectors_removed }),
                    expect,
                    "op[{i}] return"
                );
            }
            Op::BySourceMessages {
                source_message_ids,
                expect,
            } => {
                let r = delete_memories_by_source_messages_with_vectors(&db, source_message_ids)
                    .await
                    .unwrap_or_else(|e| panic!("op[{i}] bySourceMessages: {e:?}"));
                assert_eq!(
                    &json!({ "deleted": r.deleted, "vectorsRemoved": r.vectors_removed }),
                    expect,
                    "op[{i}] return"
                );
            }
            Op::ByChatId { chat_id, expect } => {
                let r = delete_memories_by_chat_id_with_vectors(&db, chat_id)
                    .await
                    .unwrap_or_else(|e| panic!("op[{i}] byChatId: {e:?}"));
                assert_eq!(
                    &json!({
                        "deleted": r.deleted,
                        "vectorsRemoved": r.vectors_removed,
                        "characterCount": r.character_count,
                    }),
                    expect,
                    "op[{i}] return"
                );
            }
        }
    }

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t, "id"))
                .unwrap_or_else(|e| panic!("dump {t}: {e:?}"))
        })
        .collect();
    drop(db);
    let _ = std::fs::remove_file(&work);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| oracle_table(&oracle_text, t))
        .collect();

    for (i, t) in TABLES.iter().enumerate() {
        // vector_entries has no updatedAt; the collapse is a no-op there.
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

    // Sanity: three survivors everywhere (mem-a4, mem-e2, mem-f1), six metas.
    assert_eq!(
        got[0]["rows"].as_array().unwrap().len(),
        3,
        "memories survivors"
    );
    assert_eq!(
        got[1]["rows"].as_array().unwrap().len(),
        6,
        "vector_indices rows"
    );
    assert_eq!(
        got[2]["rows"].as_array().unwrap().len(),
        3,
        "vector_entries survivors"
    );

    // The untouched stores' sentinel proof: char-C (no entries), char-E (swept but
    // held nothing matching), char-F (never touched) keep the pinned updatedAt.
    let metas = got[1]["rows"].as_array().unwrap();
    for ch in [
        "33333333-3333-4333-8333-333333333333", // char-C
        "55555555-5555-4555-8555-555555555555", // char-E
        "66666666-6666-4666-8666-666666666666", // char-F
    ] {
        let row = metas
            .iter()
            .find(|r| r["id"].as_str() == Some(ch))
            .unwrap_or_else(|| panic!("missing meta row {ch}"));
        assert_eq!(
            row["updatedAt"].as_str(),
            Some(spec.seed_timestamp.as_str()),
            "{ch} metadata must keep the seed sentinel"
        );
    }

    eprintln!(
        "OK: memory-cascade tier-2 matched oracle (memories + vector_indices + vector_entries)."
    );
}
