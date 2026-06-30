//! Tier-2 differential test: v4's `MemoriesRepository` write / mutation surface
//! (Phase-2, main DB).
//!
//! Both sides run the SAME op sequence (`memories-tier2.json`) on a fresh copy of
//! the empty-table seed fixture, then the `memories` table is dumped canonically
//! and the post-op state is asserted identical. Exercises `create` (rich +
//! minimal, embedding BLOB + null), `update`, `updateForCharacter` (owned +
//! not-owned no-op), `updateAccessTime{,Bulk}`, `replaceInMemories`,
//! `deleteForCharacter` (not-owned no-op), `bulkDelete`, `delete`,
//! `deleteByChatId`, `deleteBySourceMessageId{,s}`.
//!
//! NORMALIZATION: minted-timestamp placeholder. ids + createdAt + every payload
//! column are pinned; only `updatedAt` (bumped by every mutator) and
//! `lastAccessedAt` (set by updateAccessTime{,Bulk}) are minted, so both are
//! collapsed to `<ts>` on BOTH dumps. The `embedding` BLOB is dumped as lowercase
//! hex on both sides (a text-only update leaves it untouched).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-mem-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memories-fixture.ts
//!   QT_FIXTURE_MEM=/tmp/qt-mem-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memories-tier2.ts > /tmp/oracle-mem.ndjson
//! Run:
//!   QT_ORACLE_MEM=/tmp/oracle-mem.ndjson \
//!   QT_FIXTURE_MEM=/tmp/qt-mem-fixture.db \
//!     cargo test -p quilltap-harness --test memories_tier2_equivalence

use quilltap_core::db::memories::{CreateOptions, MemCreate, MemUpdate};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The two minted-timestamp columns collapsed to `<ts>` on both dumps.
const TS_COLUMNS: &[&str] = &["updatedAt", "lastAccessedAt"];

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create { options: Opts, data: Box<CreateData> },
    #[serde(rename = "update")]
    Update { id: String, data: PatchData },
    #[serde(rename = "updateForCharacter")]
    UpdateForCharacter {
        #[serde(rename = "characterId")]
        character_id: String,
        #[serde(rename = "memoryId")]
        memory_id: String,
        data: PatchData,
    },
    #[serde(rename = "updateAccessTime")]
    UpdateAccessTime {
        #[serde(rename = "characterId")]
        character_id: String,
        #[serde(rename = "memoryId")]
        memory_id: String,
    },
    #[serde(rename = "updateAccessTimeBulk")]
    UpdateAccessTimeBulk {
        #[serde(rename = "characterId")]
        character_id: String,
        ids: Vec<String>,
    },
    #[serde(rename = "replaceInMemories")]
    ReplaceInMemories {
        ids: Vec<String>,
        search: String,
        replace: String,
    },
    #[serde(rename = "delete")]
    Delete { id: String },
    #[serde(rename = "deleteForCharacter")]
    DeleteForCharacter {
        #[serde(rename = "characterId")]
        character_id: String,
        #[serde(rename = "memoryId")]
        memory_id: String,
    },
    #[serde(rename = "bulkDelete")]
    BulkDelete {
        #[serde(rename = "characterId")]
        character_id: String,
        ids: Vec<String>,
    },
    #[serde(rename = "deleteByChatId")]
    DeleteByChatId {
        #[serde(rename = "chatId")]
        chat_id: String,
    },
    #[serde(rename = "deleteBySourceMessageId")]
    DeleteBySourceMessageId {
        #[serde(rename = "sourceMessageId")]
        source_message_id: String,
    },
    #[serde(rename = "deleteBySourceMessageIds")]
    DeleteBySourceMessageIds { ids: Vec<String> },
}

#[derive(Deserialize)]
struct Opts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateData {
    character_id: String,
    about_character_id: Option<String>,
    chat_id: Option<String>,
    project_id: Option<String>,
    content: String,
    summary: String,
    keywords: Vec<String>,
    tags: Vec<String>,
    importance: f64,
    embedding: Option<Vec<f32>>,
    source: String,
    witnessed_context: Option<String>,
    source_message_id: Option<String>,
    last_accessed_at: Option<String>,
    reinforcement_count: f64,
    last_reinforced_at: Option<String>,
    related_memory_ids: Vec<String>,
    reinforced_importance: f64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PatchData {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    importance: Option<f64>,
    #[serde(default)]
    keywords: Option<Vec<String>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    source: Option<String>,
}

impl CreateData {
    fn to_mem_create(&self) -> MemCreate {
        MemCreate {
            character_id: self.character_id.clone(),
            about_character_id: self.about_character_id.clone(),
            chat_id: self.chat_id.clone(),
            project_id: self.project_id.clone(),
            content: self.content.clone(),
            summary: self.summary.clone(),
            keywords: self.keywords.clone(),
            tags: self.tags.clone(),
            importance: self.importance,
            embedding: self.embedding.clone(),
            source: self.source.clone(),
            witnessed_context: self.witnessed_context.clone(),
            source_message_id: self.source_message_id.clone(),
            last_accessed_at: self.last_accessed_at.clone(),
            reinforcement_count: self.reinforcement_count,
            last_reinforced_at: self.last_reinforced_at.clone(),
            related_memory_ids: self.related_memory_ids.clone(),
            reinforced_importance: self.reinforced_importance,
        }
    }
}

impl PatchData {
    fn to_mem_update(&self) -> MemUpdate {
        MemUpdate {
            content: self.content.clone(),
            summary: self.summary.clone(),
            importance: self.importance,
            keywords: self.keywords.clone(),
            tags: self.tags.clone(),
            source: self.source.clone(),
            ..Default::default()
        }
    }
}

/// Collapse the two minted-timestamp columns to `<ts>` (non-null only), in place.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

#[test]
fn memories_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MEM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEM to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_MEM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MEM to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/memories-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-mem-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.memories();
        for op in &spec.ops {
            match op {
                Op::Create { options, data } => {
                    repo.create(
                        &data.to_mem_create(),
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("create");
                }
                Op::Update { id, data } => {
                    repo.update(id, &data.to_mem_update()).expect("update");
                }
                Op::UpdateForCharacter {
                    character_id,
                    memory_id,
                    data,
                } => {
                    repo.update_for_character(character_id, memory_id, &data.to_mem_update())
                        .expect("updateForCharacter");
                }
                Op::UpdateAccessTime {
                    character_id,
                    memory_id,
                } => {
                    repo.update_access_time(character_id, memory_id)
                        .expect("updateAccessTime");
                }
                Op::UpdateAccessTimeBulk { character_id, ids } => {
                    repo.update_access_time_bulk(character_id, ids)
                        .expect("updateAccessTimeBulk");
                }
                Op::ReplaceInMemories {
                    ids,
                    search,
                    replace,
                } => {
                    repo.replace_in_memories(ids, search, replace)
                        .expect("replaceInMemories");
                }
                Op::Delete { id } => {
                    repo.delete(id).expect("delete");
                }
                Op::DeleteForCharacter {
                    character_id,
                    memory_id,
                } => {
                    repo.delete_for_character(character_id, memory_id)
                        .expect("deleteForCharacter");
                }
                Op::BulkDelete { character_id, ids } => {
                    repo.bulk_delete(character_id, ids).expect("bulkDelete");
                }
                Op::DeleteByChatId { chat_id } => {
                    repo.delete_by_chat_id(chat_id).expect("deleteByChatId");
                }
                Op::DeleteBySourceMessageId { source_message_id } => {
                    repo.delete_by_source_message_id(source_message_id)
                        .expect("deleteBySourceMessageId");
                }
                Op::DeleteBySourceMessageIds { ids } => {
                    repo.delete_by_source_message_ids(ids)
                        .expect("deleteBySourceMessageIds");
                }
            }
        }
    }

    let mut got = writer
        .dump_table_json("memories", "id")
        .expect("dump memories");

    let _ = std::fs::remove_file(&work);

    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: memories tier-2 matched oracle ({n} rows).");
}
