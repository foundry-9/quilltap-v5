//! Read-differential test: v4's `ChatsRepository` findBy* queries (Phase-2, the
//! chats repo — the conversation capstone, sub-unit 2, read path).
//!
//! Both sides READ the SAME baked fixture (seven chats created by v4's real
//! `repos.chats.create`, ids + timestamps pinned), run the SAME query list, and
//! the results compare **exactly** — NO normalization, because nothing is mutated
//! so no timestamp is ever minted. The hydrated chat's `participants` is the
//! per-element Zod-parsed array (defaults materialized, nullable-optionals
//! dropped); JSON columns are the parsed objects; numbers render the JS way.
//!
//! The Rust port drives [`chats_read`]'s query functions over one connection; v4
//! drives the real repository methods (see the oracle).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHATSREAD=/tmp/qt-chatsread.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-read-fixture.ts
//!   QT_FIXTURE_CHATSREAD=/tmp/qt-chatsread.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-read.ts > /tmp/oracle-chatsread.ndjson
//! Run:
//!   QT_ORACLE_CHATSREAD=/tmp/oracle-chatsread.ndjson \
//!   QT_FIXTURE_CHATSREAD=/tmp/qt-chatsread.db \
//!     cargo test -p quilltap-harness --test chats_read_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::chats_read as cr;
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
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default, rename = "characterId")]
    character_id: Option<String>,
    #[serde(default, rename = "chatType")]
    chat_type: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default, rename = "excludeChatId")]
    exclude_chat_id: Option<String>,
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
        .join("../../harness/oracle/fixtures/chats-read-tier2.json")
}

fn run_query(writer: &Writer, q: &Query) -> Value {
    let conn = writer.connection();
    match q.kind.as_str() {
        "findById" => cr::find_by_id(conn, q.id.as_deref().unwrap())
            .expect("findById")
            .unwrap_or(Value::Null),
        "findAll" => Value::Array(cr::find_all(conn).expect("findAll")),
        "findByUserId" => Value::Array(
            cr::find_by_user_id(conn, q.user_id.as_deref().unwrap()).expect("findByUserId"),
        ),
        "findByCharacterId" => Value::Array(
            cr::find_by_character_id(conn, q.character_id.as_deref().unwrap())
                .expect("findByCharacterId"),
        ),
        "findByType" => Value::Array(
            cr::find_by_type(
                conn,
                q.user_id.as_deref().unwrap(),
                q.chat_type.as_deref().unwrap(),
            )
            .expect("findByType"),
        ),
        "findRecentSummarizedByCharacter" => Value::Array(
            cr::find_recent_summarized_by_character(
                conn,
                q.character_id.as_deref().unwrap(),
                q.limit.unwrap(),
                q.exclude_chat_id.as_deref(),
            )
            .expect("findRecentSummarizedByCharacter"),
        ),
        other => panic!("unknown query kind: {other}"),
    }
}

#[test]
fn chats_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHATSREAD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHATSREAD to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHATSREAD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHATSREAD to the fixture .db (header).");
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
    let work = std::env::temp_dir().join(format!("qt-chatsread-rust-{pid}.db"));
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
        "OK: chats read matched oracle ({} queries).",
        spec.queries.len()
    );
}
