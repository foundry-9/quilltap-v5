//! Read-differential test: v4's `MemoriesRepository` findBy* / count* queries
//! (Phase-2, the memories repo).
//!
//! Both sides READ the SAME baked fixture (six memories created by v4's real
//! `repos.memories.create`, ids + timestamps pinned), run the SAME query list,
//! and the results compare **exactly** — NO normalization, because nothing is
//! mutated so no timestamp is ever minted. A returned memory's `embedding` (only
//! `M1` has one) is the `Float32Array` `{"0":…}` object the read marshaling
//! rebuilds from the BLOB.
//!
//! The Rust port drives [`memories_read`]'s query functions over one connection;
//! v4 drives the real repository methods (see the oracle). Each query yields a
//! single JSON `result` Value of the kind-appropriate shape.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MEMREAD=/tmp/qt-memread.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memories-read-fixture.ts
//!   QT_FIXTURE_MEMREAD=/tmp/qt-memread.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memories-read.ts > /tmp/oracle-memread.ndjson
//! Run:
//!   QT_ORACLE_MEMREAD=/tmp/oracle-memread.ndjson \
//!   QT_FIXTURE_MEMREAD=/tmp/qt-memread.db \
//!     cargo test -p quilltap-harness --test memories_read_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::memories_read as mr;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Query {
    kind: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "characterId")]
    character_id: Option<String>,
    #[serde(default, rename = "memoryId")]
    memory_id: Option<String>,
    #[serde(default, rename = "chatId")]
    chat_id: Option<String>,
    #[serde(default, rename = "aboutCharacterId")]
    about_character_id: Option<String>,
    #[serde(default, rename = "aboutCharacterIds")]
    about_character_ids: Option<Vec<String>>,
    #[serde(default, rename = "sourceMessageId")]
    source_message_id: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    ids: Option<Vec<String>>,
    #[serde(default)]
    keywords: Option<Vec<String>>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default, rename = "searchText")]
    search_text: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default, rename = "minImportance")]
    min_importance: Option<f64>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default, rename = "sortBy")]
    sort_by: Option<String>,
    #[serde(default, rename = "sortOrder")]
    sort_order: Option<String>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default, rename = "batchSize")]
    batch_size: Option<i64>,
    #[serde(default, rename = "limitPerCharacter")]
    limit_per_character: Option<i64>,
    #[serde(default)]
    high: Option<i64>,
    #[serde(default)]
    medium: Option<i64>,
    #[serde(default)]
    low: Option<i64>,
    #[serde(default, rename = "chatIds")]
    chat_ids: Option<Vec<String>>,
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
        .join("../../harness/oracle/fixtures/memories-read-tier2.json")
}

fn run_query(writer: &Writer, q: &Query) -> Value {
    let conn = writer.connection();
    let cid = || q.character_id.as_deref().expect("characterId");
    match q.kind.as_str() {
        "findById" => single(mr::find_by_id(conn, q.id.as_deref().unwrap()).expect("findById")),
        "findByIdForCharacter" => single(
            mr::find_by_id_for_character(conn, cid(), q.memory_id.as_deref().unwrap())
                .expect("findByIdForCharacter"),
        ),
        "findAll" => Value::Array(mr::find_all(conn).expect("findAll")),
        "findByCharacterId" => {
            Value::Array(mr::find_by_character_id(conn, cid()).expect("findByCharacterId"))
        }
        "findByCharacterIdInBatches" => {
            let batches = mr::find_by_character_id_in_batches(conn, cid(), q.batch_size.unwrap())
                .expect("findByCharacterIdInBatches");
            Value::Array(batches.into_iter().map(Value::Array).collect())
        }
        "findByIds" => {
            Value::Array(mr::find_by_ids(conn, q.ids.as_ref().unwrap()).expect("findByIds"))
        }
        "findByCharacterIdPaginated" => {
            let opts = mr::PaginateOptions {
                limit: q.limit.unwrap(),
                offset: q.offset.unwrap(),
                sort_by: q.sort_by.as_deref().unwrap_or("createdAt"),
                sort_order: q.sort_order.as_deref().unwrap_or("desc"),
                search: q.search.as_deref(),
                source: q.source.as_deref(),
                min_importance: q.min_importance,
            };
            let (page, total) = mr::find_by_character_id_paginated(conn, cid(), &opts)
                .expect("findByCharacterIdPaginated");
            json!({ "memories": page, "totalCount": total })
        }
        "findByKeywords" => Value::Array(
            mr::find_by_keywords(conn, cid(), q.keywords.as_ref().unwrap())
                .expect("findByKeywords"),
        ),
        "searchByContent" => Value::Array(
            mr::search_by_content(conn, cid(), q.query.as_deref().unwrap())
                .expect("searchByContent"),
        ),
        "findByImportance" => Value::Array(
            mr::find_by_importance(conn, cid(), q.min_importance.unwrap())
                .expect("findByImportance"),
        ),
        "findBySource" => Value::Array(
            mr::find_by_source(conn, cid(), q.source.as_deref().unwrap()).expect("findBySource"),
        ),
        "findRecent" => {
            Value::Array(mr::find_recent(conn, cid(), q.limit.unwrap()).expect("findRecent"))
        }
        "findMostImportant" => Value::Array(
            mr::find_most_important(conn, cid(), q.limit.unwrap()).expect("findMostImportant"),
        ),
        "findRecentByImportanceTier" => {
            let (high, medium, low) = mr::find_recent_by_importance_tier(
                conn,
                cid(),
                q.high.unwrap(),
                q.medium.unwrap(),
                q.low.unwrap(),
            )
            .expect("findRecentByImportanceTier");
            json!({ "high": high, "medium": medium, "low": low })
        }
        "findByCharacterAboutCharacter" => Value::Array(
            mr::find_by_character_about_character(
                conn,
                cid(),
                q.about_character_id.as_deref().unwrap(),
            )
            .expect("findByCharacterAboutCharacter"),
        ),
        "findByCharacterAboutCharacters" => Value::Array(
            mr::find_by_character_about_characters(
                conn,
                cid(),
                q.about_character_ids.as_ref().unwrap(),
                q.limit_per_character.unwrap(),
            )
            .expect("findByCharacterAboutCharacters"),
        ),
        "findByChatId" => Value::Array(
            mr::find_by_chat_id(conn, q.chat_id.as_deref().unwrap()).expect("findByChatId"),
        ),
        "findBySourceMessageId" => Value::Array(
            mr::find_by_source_message_id(conn, q.source_message_id.as_deref().unwrap())
                .expect("findBySourceMessageId"),
        ),
        "findByAboutCharacterId" => Value::Array(
            mr::find_by_about_character_id(conn, q.about_character_id.as_deref().unwrap())
                .expect("findByAboutCharacterId"),
        ),
        "findMemoriesWithText" => Value::Array(
            mr::find_memories_with_text(
                conn,
                q.character_id.as_deref(),
                q.chat_id.as_deref(),
                q.search_text.as_deref().unwrap(),
            )
            .expect("findMemoriesWithText"),
        ),
        "countMemoriesWithText" => Value::from(
            mr::count_memories_with_text(
                conn,
                q.character_id.as_deref(),
                q.chat_id.as_deref(),
                q.search_text.as_deref().unwrap(),
            )
            .expect("countMemoriesWithText"),
        ),
        "countByCharacterId" => {
            Value::from(mr::count_by_character_id(conn, cid()).expect("countByCharacterId"))
        }
        "countCreatedSince" => Value::from(
            mr::count_created_since(conn, cid(), q.since.as_deref().unwrap())
                .expect("countCreatedSince"),
        ),
        "countWithoutEmbedding" => Value::from(
            mr::count_without_embedding(conn, q.character_id.as_deref())
                .expect("countWithoutEmbedding"),
        ),
        "findIdsWithoutEmbedding" => Value::Array(
            mr::find_ids_without_embedding(conn, q.character_id.as_deref(), q.limit)
                .expect("findIdsWithoutEmbedding"),
        ),
        "countByChatId" => Value::from(
            mr::count_by_chat_id(conn, q.chat_id.as_deref().unwrap()).expect("countByChatId"),
        ),
        "countBySourceMessageId" => Value::from(
            mr::count_by_source_message_id(conn, q.source_message_id.as_deref().unwrap())
                .expect("countBySourceMessageId"),
        ),
        "countBySourceMessageIds" => Value::from(
            mr::count_by_source_message_ids(conn, q.ids.as_ref().unwrap())
                .expect("countBySourceMessageIds"),
        ),
        "countByChatIds" => {
            mr::count_by_chat_ids(conn, q.chat_ids.as_ref().unwrap()).expect("countByChatIds")
        }
        "findDistinctChatIds" => Value::Array(
            mr::find_distinct_chat_ids(conn)
                .expect("findDistinctChatIds")
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
        "searchByContentAboutCharacter" => Value::Array(
            mr::search_by_content_about_character(
                conn,
                cid(),
                q.about_character_id.as_deref().unwrap(),
                q.query.as_deref().unwrap(),
            )
            .expect("searchByContentAboutCharacter"),
        ),
        other => panic!("unknown query kind: {other}"),
    }
}

/// `Option<Value>` → the JSON the oracle emits for a single-result query
/// (Memory object, or `null`).
fn single(v: Option<Value>) -> Value {
    v.unwrap_or(Value::Null)
}

#[test]
fn memories_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MEMREAD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEMREAD to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_MEMREAD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MEMREAD to the fixture .db (header).");
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
    let work = std::env::temp_dir().join(format!("qt-memread-rust-{pid}.db"));
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
        "OK: memories read matched oracle ({} queries).",
        spec.queries.len()
    );
}
