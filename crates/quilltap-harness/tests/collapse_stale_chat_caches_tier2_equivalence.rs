//! Tier-2 differential test (P4.d3): the stale-chat CACHE collapse (v4
//! `lib/background-jobs/maintenance/collapse-stale-chat-caches.ts`
//! `collapseStaleChatCaches`).
//!
//! Both sides copy the SAME seed fixture (built by
//! `harness/oracle/fixtures/build-retention-caches-fixture.ts`), run
//! `collapse_stale_chat_caches(db, nowMs)` with the SAME fixed `nowMs`, and diff:
//!
//!   - the summary (chatsScanned / staleChats / chatsCollapsed / chatRowsCleared
//!     / messageRowsCleared — `chunkEmbeddingsCleared` left it, v4 `f7f3d7bf0`),
//!   - a `chats` projection (the two cleared columns + `updatedAt` — proving the
//!     stale chat's caches NULL, the active chat's survive, neither updatedAt
//!     bumped),
//!   - a `messages` projection (the five discardable columns + content — the
//!     stale chat's laden message all-NULL keeping content; the guard skips the
//!     bare second message; the active chat's message survives),
//!   - a `chunks` projection (embeddingNull + the raw `updatedAt`).
//!
//! P4.D235 (v4 `f7f3d7bf0`, "keep conversation embeddings warm"): the sweep no
//! longer cold-tiers ANY chunk. P4.D25's warmth window (v4 `f7cc887b` — clear
//! only embeddings older than the cutoff) is moot: the stale chat's OLD
//! embedded chunk, which the old sweep NULLed with a minted `updatedAt`, is now
//! SPARED like its two warm siblings. A stamp is still placeholdered exactly
//! when it differs from the row's SEEDED value, so a port that cold-tiered
//! anything shows up byte-level; the corpus-shape guard below pins that all
//! three embedded stale-chat chunks survive and nothing was re-stamped.
//! Measured at both pins over one baseline-built fixture: `acadcc7cd` NULLs
//! `cc000001` (`chunkEmbeddingsCleared: 1`), `f7f3d7bf0` spares it (no key).
//!
//! The `chats` projection no longer names `renderedMarkdown`: the column is
//! DROPPED at the target pin and the builder no longer writes it, so the
//! fixture builds at either pin (it is built at the TARGET pin).
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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::collapse_stale_chat_caches::collapse_stale_chat_caches;
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const NOW_ISO: &str = "2026-06-01T00:00:00.000Z";

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/retention-caches.json")
}

/// chunk id -> the `updatedAt` the fixture SEEDED it with. A dumped stamp that
/// differs from its row's seeded value was minted by the collapse (v4 stamps
/// `new Date()`, this side injects `nowMs`), so it is placeholdered on both
/// sides — and a stamp that survives unchanged is proof the age guard SPARED
/// that row.
fn seeded_chunk_stamps() -> BTreeMap<String, String> {
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    spec["chunks"]
        .as_array()
        .expect("spec.chunks")
        .iter()
        .map(|c| {
            (
                c["id"].as_str().expect("chunk id").to_string(),
                c["updatedAt"]
                    .as_str()
                    .expect("chunk updatedAt")
                    .to_string(),
            )
        })
        .collect()
}

/// Placeholder every chunk row whose `updatedAt` is not its seeded value.
fn normalize_chunks(rows: &mut [Value], seeded: &BTreeMap<String, String>) {
    for row in rows.iter_mut() {
        let id = row["id"].as_str().unwrap_or_default().to_string();
        let minted = match (seeded.get(&id), row.get("updatedAt")) {
            (Some(want), Some(Value::String(got))) => want != got,
            _ => true,
        };
        if minted {
            row.as_object_mut()
                .expect("chunk row object")
                .insert("updatedAt".to_string(), Value::String("<ts>".to_string()));
        }
    }
}

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
                "SELECT id, compressionCache, compiledIdentityStacks, \
                        updatedAt FROM chats ORDER BY id ASC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "compressionCache": r.get::<_, Option<String>>(1)?,
                        // v4 bug 160 (`186eb09cb`): the second discardable
                        // `chats` cache column. The fixture's THIRD chat
                        // carries ONLY this one, so it is the row that
                        // distinguishes the guard's new disjunct from the SET.
                        "compiledIdentityStacks": r.get::<_, Option<String>>(2)?,
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

    // 4) chunks projection. The raw `updatedAt` rides along: a stamp that still
    //    equals the row's SEEDED value proves the age guard spared it; a minted
    //    one (placeholdered on both sides) proves it was cold-tiered.
    let got_chunks = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, chatId, content, updatedAt, \
                        CASE WHEN embedding IS NULL THEN 1 ELSE 0 END AS embeddingNull \
                 FROM conversation_chunks ORDER BY id ASC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "chatId": r.get::<_, String>(1)?,
                        "content": r.get::<_, String>(2)?,
                        "updatedAt": r.get::<_, String>(3)?,
                        "embeddingNull": r.get::<_, i64>(4)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read chunks");

    let seeded = seeded_chunk_stamps();
    let mut got_chunks = got_chunks;
    let mut want_chunks = oracle_line(&oracle_text, "chunks")["rows"]
        .as_array()
        .expect("oracle chunks rows")
        .clone();
    normalize_chunks(&mut got_chunks, &seeded);
    normalize_chunks(&mut want_chunks, &seeded);

    // Corpus shape (P4.D235): ALL THREE embedded stale-chat chunks survive —
    // the old one included, which the pre-`f7f3d7bf0` sweep cold-tiered — and
    // no row was re-stamped, or a port still cold-tiering the old chunk could
    // not be told apart by the count alone.
    let spared = want_chunks
        .iter()
        .filter(|r| {
            r["chatId"].as_str() == Some("aaaaaaa1-0000-4000-8000-000000000001")
                && r["embeddingNull"].as_i64() == Some(0)
        })
        .count();
    assert_eq!(
        spared, 3,
        "every embedded stale-chat chunk keeps its vector (nothing is cold-tiered)"
    );
    assert!(
        want_chunks.iter().all(|r| r["updatedAt"] != "<ts>"),
        "no chunk was re-stamped: {want_chunks:?}"
    );

    assert_eq!(
        &Value::Array(got_chunks),
        &Value::Array(want_chunks),
        "chunks projection diverged"
    );

    eprintln!("OK: collapse-stale-chat-caches matched oracle (summary + 3 projections).");
}
