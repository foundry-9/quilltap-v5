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
//! P4.D265 (v4 `d58548051` / `3f7320138`): the tier corpus — a third character
//! (cold rows, digests, the F4 tie rows, two planted NULL rows) — the five new
//! reads, the paginated `tier`, and per query the raw-path lines (`Fetched most
//! important memories`, `Memory failed validation in <context>`), compared
//! line for line through the thread-scoped capture rig.
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
    // P4.D265
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default, rename = "olderThan")]
    older_than: Option<String>,
}

#[derive(Deserialize)]
struct OracleQuery {
    kind: String,
    result: Value,
    /// P4.D265: v4's raw-path lines for this query (`Fetched most important
    /// memories`, `Memory failed validation in <context>`).
    #[serde(default)]
    lines: Vec<OracleLine>,
}

#[derive(Deserialize)]
struct OracleLine {
    level: String,
    message: String,
    fields: serde_json::Map<String, Value>,
}

/// The messages the oracle captures (its `CAPTURED_PREFIXES`).
const CAPTURED_PREFIXES: [&str; 2] = [
    "Fetched most important memories",
    "Memory failed validation in ",
];

/// v4's line rendered as v5's capture rig renders the port's: `LEVEL
/// quilltap::db <message> k=v …`, fields in v4's key order, strings bare,
/// numbers as JSON (JS) renders them.
fn expected_line(l: &OracleLine) -> String {
    let mut out = format!("{} quilltap::db {}", l.level.to_uppercase(), l.message);
    for (k, v) in &l.fields {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {k}={rendered}"));
    }
    out
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
                tier: q.tier.as_deref(),
            };
            let (page, total) = mr::find_by_character_id_paginated(conn, cid(), &opts)
                .expect("findByCharacterIdPaginated");
            json!({ "memories": page, "totalCount": total })
        }
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
        // P4.d27 / v4 `7391404e`. Fail-soft (no `Result`) because v4 wraps it in
        // `safeQuery` with `[]` — see the port's doc comment.
        "findDistinctCharacterIds" => Value::Array(
            mr::find_distinct_character_ids(conn)
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
        // === P4.D265 (v4 `d58548051`) ===
        "findHotDigests" => {
            let subject = match q.subject.as_deref().unwrap() {
                "self" => mr::DigestSubject::SelfSubject,
                "any" => mr::DigestSubject::Any,
                id => mr::DigestSubject::About(id.to_string()),
            };
            Value::Array(
                mr::find_hot_digests(conn, cid(), subject, q.limit.unwrap())
                    .expect("findHotDigests"),
            )
        }
        "countHotByCharacterId" => {
            Value::from(mr::count_hot_by_character_id(conn, cid()).expect("countHotByCharacterId"))
        }
        "countUnconsideredHot" => {
            Value::from(mr::count_unconsidered_hot(conn, cid()).expect("countUnconsideredHot"))
        }
        "findColdIdsByCharacterId" => Value::from(
            mr::find_cold_ids_by_character_id(conn, cid()).expect("findColdIdsByCharacterId"),
        ),
        "findExpiredColdIds" => Value::from(
            mr::find_expired_cold_ids(conn, cid(), q.older_than.as_deref().unwrap())
                .expect("findExpiredColdIds"),
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

    let work_scratch = tempfile::Builder::new()
        .prefix("qt-memread-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open: {e}"));

    // v4 `ad1c4c37f` deleted `findByKeywords` + `findByAboutCharacterId` as
    // dead code (v5 never had a production caller either — retired, not
    // ported). The committed fixture keeps their rows unchanged; skip them
    // here, matching the oracle case's own filter, rather than call the
    // retired methods.
    let retired = ["findByKeywords", "findByAboutCharacterId"];
    let queries: Vec<&Query> = spec
        .queries
        .iter()
        .filter(|q| !retired.contains(&q.kind.as_str()))
        .collect();

    assert_eq!(
        queries.len(),
        oracle.queries.len(),
        "query count: spec vs oracle"
    );

    let mut lines_compared = 0usize;
    for (i, q) in queries.iter().enumerate() {
        let (got, lines) = quilltap_core::test_support::captured_with(|| run_query(&writer, q));
        let oq = &oracle.queries[i];
        // P4.D265: the raw-path lines, query by query.
        let got_lines: Vec<String> = lines
            .into_iter()
            .filter(|l| CAPTURED_PREFIXES.iter().any(|p| l.contains(p)))
            .collect();
        let want_lines: Vec<String> = oq.lines.iter().map(expected_line).collect();
        assert_eq!(
            got_lines, want_lines,
            "query {i} ({}): lines diverged",
            q.kind
        );
        lines_compared += want_lines.len();
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

    assert!(
        lines_compared > 0,
        "the oracle carried no raw-path line — the capture never compared anything"
    );
    eprintln!(
        "OK: memories read matched oracle ({} queries, {lines_compared} lines).",
        queries.len()
    );
}
