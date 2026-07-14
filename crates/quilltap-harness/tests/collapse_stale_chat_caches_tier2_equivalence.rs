//! Tier-2 differential test (P4.d3): the stale-chat CACHE collapse (v4
//! `lib/background-jobs/maintenance/collapse-stale-chat-caches.ts`
//! `collapseStaleChatCaches`).
//!
//! Both sides copy the SAME seed fixture (built by
//! `harness/oracle/fixtures/build-retention-caches-fixture.ts`), run
//! `collapse_stale_chat_caches(db, nowMs)` with the SAME fixed `nowMs`, and diff:
//!
//!   - the summary (chatsScanned / staleChats / chatsCollapsed / chatRowsCleared
//!     / messageRowsCleared / chunkEmbeddingsCleared),
//!   - a `chats` projection (the two cleared columns + `updatedAt` — proving the
//!     stale chat's caches NULL, the active chat's survive, neither updatedAt
//!     bumped),
//!   - a `messages` projection (the five discardable columns + content — the
//!     stale chat's laden message all-NULL keeping content; the guard skips the
//!     bare second message; the active chat's message survives),
//!   - a `chunks` projection (embeddingNull + a boolean updatedAtChanged vs the
//!     seed timestamp — the stale chat's embedded chunk cold-tiered with a bumped
//!     updatedAt; the already-cold chunk skipped by the guard; the active chunk
//!     survives).
//!
//! The chunk `updatedAt` VALUE is minted (v4 stamps `new Date()`, the Rust sweep
//! injects `nowMs`), so it is compared only as the `updatedAtChanged` boolean.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-retention-caches.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-retention-caches-fixture.ts
//!   QT_FIXTURE_RETENTION_CACHES=/tmp/qt-retention-caches.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/collapse-stale-chat-caches-tier2.ts \
//!     > /tmp/oracle-retention-caches.ndjson
//! Run:
//!   QT_ORACLE_RETENTION_CACHES=/tmp/oracle-retention-caches.ndjson \
//!   QT_FIXTURE_RETENTION_CACHES=/tmp/qt-retention-caches.db \
//!     cargo test -p quilltap-harness --test collapse_stale_chat_caches_tier2_equivalence

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::collapse_stale_chat_caches::collapse_stale_chat_caches;
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const NOW_ISO: &str = "2026-06-01T00:00:00.000Z";
const SEED_TS: &str = "2026-01-01T00:00:00.000Z";

fn oracle_line(text: &str, kind: &str) -> Value {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("kind").and_then(Value::as_str) == Some(kind) {
            return v;
        }
    }
    panic!("oracle ndjson missing kind {kind}");
}

#[test]
fn collapse_stale_chat_caches_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_RETENTION_CACHES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_RETENTION_CACHES / QT_FIXTURE_RETENTION_CACHES (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_RETENTION_CACHES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_RETENTION_CACHES (see header).");
            return;
        }
    };
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let pid = std::process::id();
    let work = std::env::temp_dir().join(format!("qt-retention-caches-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let db = Db::open(
        DbPaths {
            main: work.clone(),
            mount_index: None,
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .unwrap_or_else(|e| panic!("open db: {e}"));

    let now_ms = quilltap_core::clock::iso_to_ms(NOW_ISO).expect("parse nowIso");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let summary = rt
        .block_on(collapse_stale_chat_caches(&db, now_ms))
        .expect("collapse");

    // 1) Summary.
    let got_summary = json!({
        "chatsScanned": summary.chats_scanned,
        "staleChats": summary.stale_chats,
        "chatsCollapsed": summary.chats_collapsed,
        "chatRowsCleared": summary.chat_rows_cleared,
        "messageRowsCleared": summary.message_rows_cleared,
        "chunkEmbeddingsCleared": summary.chunk_embeddings_cleared,
    });
    assert_eq!(
        &got_summary,
        oracle_line(&oracle_text, "summary").get("summary").unwrap(),
        "summary diverged"
    );

    // 2) chats projection.
    let got_chats = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, compressionCache, renderedMarkdown, updatedAt FROM chats ORDER BY id ASC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "compressionCache": r.get::<_, Option<String>>(1)?,
                        "renderedMarkdown": r.get::<_, Option<String>>(2)?,
                        "updatedAt": r.get::<_, String>(3)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read chats");
    assert_eq!(
        &Value::Array(got_chats),
        oracle_line(&oracle_text, "chats").get("rows").unwrap(),
        "chats projection diverged"
    );

    // 3) messages projection.
    let got_messages = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, chatId, rawResponse, reasoningContent, reasoningSegments, \
                        renderedHtml, debugMemoryLogs, content \
                 FROM chat_messages ORDER BY id ASC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "chatId": r.get::<_, String>(1)?,
                        "rawResponse": r.get::<_, Option<String>>(2)?,
                        "reasoningContent": r.get::<_, Option<String>>(3)?,
                        "reasoningSegments": r.get::<_, Option<String>>(4)?,
                        "renderedHtml": r.get::<_, Option<String>>(5)?,
                        "debugMemoryLogs": r.get::<_, Option<String>>(6)?,
                        "content": r.get::<_, String>(7)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read messages");
    assert_eq!(
        &Value::Array(got_messages),
        oracle_line(&oracle_text, "messages").get("rows").unwrap(),
        "messages projection diverged"
    );

    // 4) chunks projection (updatedAt compared only as the changed-vs-seed bool).
    let got_chunks = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, chatId, content, \
                        CASE WHEN embedding IS NULL THEN 1 ELSE 0 END AS embeddingNull, \
                        CASE WHEN updatedAt != ?1 THEN 1 ELSE 0 END AS updatedAtChanged \
                 FROM conversation_chunks ORDER BY id ASC",
            )?;
            let rows = stmt
                .query_map([SEED_TS], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "chatId": r.get::<_, String>(1)?,
                        "content": r.get::<_, String>(2)?,
                        "embeddingNull": r.get::<_, i64>(3)?,
                        "updatedAtChanged": r.get::<_, i64>(4)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read chunks");
    assert_eq!(
        &Value::Array(got_chunks),
        oracle_line(&oracle_text, "chunks").get("rows").unwrap(),
        "chunks projection diverged"
    );

    eprintln!("OK: collapse-stale-chat-caches matched oracle (summary + 3 projections).");
}
