//! Tier-2 differential test (P4.d3): the cold-chunk RE-EMBED enqueue (v4
//! `lib/scriptorium/cold-chunk-reembed.ts` `maybeEnqueueColdChunkReembed`).
//!
//! Both sides copy the SAME seed fixture (built by
//! `harness/oracle/fixtures/build-cold-reembed-fixture.ts`), reset the debounce,
//! run `maybe_enqueue_cold_chunk_reembed` for the cold / warm / chunkless chats,
//! and diff:
//!   - the per-chat return counts (`cold`=2, `warm`=0, `chunkless`=0), and
//!   - the enqueued `background_jobs` projection (count + type / priority (10 for
//!     CONVERSATION_CHUNK) / maxAttempts / entityType / entityId / chatId /
//!     profileId, sorted by entityId). Minted job ids + timestamps are omitted;
//!     `status` is omitted (a nondeterministic processor artifact).
//!
//! The embedded chunk and the empty-content chunk are correctly skipped (only the
//! two cold-with-content chunks enqueue), and the priority + per-entity dedup come
//! from the shared `queue_service::enqueue_embedding_generate`.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-cold-reembed.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-cold-reembed-fixture.ts
//!   QT_FIXTURE_COLD_REEMBED=/tmp/qt-cold-reembed.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/cold-chunk-reembed-tier2.ts \
//!     > /tmp/oracle-cold-reembed.ndjson
//! Run:
//!   QT_ORACLE_COLD_REEMBED=/tmp/oracle-cold-reembed.ndjson \
//!   QT_FIXTURE_COLD_REEMBED=/tmp/qt-cold-reembed.db \
//!     cargo test -p quilltap-harness --test cold_chunk_reembed_tier2_equivalence

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::cold_chunk_reembed::{
    maybe_enqueue_cold_chunk_reembed, reset_debounce_for_testing,
};
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER_ID: &str = "11111111-1111-4111-8111-111111111111";
const NOW_ISO: &str = "2026-06-01T00:00:00.000Z";
const COLD_CHAT: &str = "c0100000-0000-4000-8000-000000000001";
const WARM_CHAT: &str = "c0200000-0000-4000-8000-000000000002";
const CHUNKLESS_CHAT: &str = "c0300000-0000-4000-8000-000000000003";

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
fn cold_chunk_reembed_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_COLD_REEMBED") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_COLD_REEMBED / QT_FIXTURE_COLD_REEMBED (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_COLD_REEMBED") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_COLD_REEMBED (see header).");
            return;
        }
    };
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let pid = std::process::id();
    let work = std::env::temp_dir().join(format!("qt-cold-reembed-rust-{pid}.db"));
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

    reset_debounce_for_testing();
    let cold = rt
        .block_on(maybe_enqueue_cold_chunk_reembed(
            &db, USER_ID, COLD_CHAT, now_ms,
        ))
        .expect("cold");
    // A second immediate scan (same clock) is debounced.
    let debounced = rt
        .block_on(maybe_enqueue_cold_chunk_reembed(
            &db, USER_ID, COLD_CHAT, now_ms,
        ))
        .expect("debounced");
    let warm = rt
        .block_on(maybe_enqueue_cold_chunk_reembed(
            &db, USER_ID, WARM_CHAT, now_ms,
        ))
        .expect("warm");
    let chunkless = rt
        .block_on(maybe_enqueue_cold_chunk_reembed(
            &db,
            USER_ID,
            CHUNKLESS_CHAT,
            now_ms,
        ))
        .expect("chunkless");

    // 2) Enqueued background_jobs projection (status omitted; priority/maxAttempts
    // read as integers to match the oracle's JSON numbers).
    let mut got_rows = db
        .read_main(|c| {
            let mut stmt =
                c.prepare("SELECT type, priority, maxAttempts, payload FROM background_jobs")?;
            let rows = stmt
                .query_map([], |r| {
                    let payload: String = r.get(3)?;
                    let p: Value = serde_json::from_str(&payload).unwrap_or(Value::Null);
                    Ok(json!({
                        "type": r.get::<_, String>(0)?,
                        // priority / maxAttempts are REAL columns; the oracle
                        // (better-sqlite3) surfaces the whole values as JSON
                        // integers, so collapse the f64 to i64 to match.
                        "priority": r.get::<_, f64>(1)? as i64,
                        "maxAttempts": r.get::<_, f64>(2)? as i64,
                        "entityType": p.get("entityType").cloned().unwrap_or(Value::Null),
                        "entityId": p.get("entityId").cloned().unwrap_or(Value::Null),
                        "chatId": p.get("chatId").cloned().unwrap_or(Value::Null),
                        "profileId": p.get("profileId").cloned().unwrap_or(Value::Null),
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read jobs");
    got_rows.sort_by(|a, b| {
        a["entityId"]
            .as_str()
            .unwrap_or("")
            .cmp(b["entityId"].as_str().unwrap_or(""))
    });

    let want_jobs = oracle_line(&oracle_text, "jobs");
    assert_eq!(
        got_rows.len() as u64,
        want_jobs["count"].as_u64().unwrap(),
        "job count diverged"
    );
    assert_eq!(
        &Value::Array(got_rows),
        want_jobs.get("rows").unwrap(),
        "jobs projection diverged"
    );

    // No-profile arm: drop every embedding profile, reset the debounce, and
    // rescan the still-cold chat — nothing is enqueued (and no jobs added, the
    // dump above already captured the two).
    reset_debounce_for_testing();
    rt.block_on(db.write(|w| {
        w.main()
            .connection()
            .execute("DELETE FROM embedding_profiles", [])?;
        Ok(())
    }))
    .expect("delete profiles");
    let no_profile = rt
        .block_on(maybe_enqueue_cold_chunk_reembed(
            &db, USER_ID, COLD_CHAT, now_ms,
        ))
        .expect("no_profile");

    // Per-chat return counts (asserted last so the no-profile arm is included).
    let got_counts = json!({
        "cold": cold,
        "debounced": debounced,
        "warm": warm,
        "chunkless": chunkless,
        "noProfile": no_profile,
    });
    let want_counts = oracle_line(&oracle_text, "counts");
    assert_eq!(
        got_counts,
        json!({
            "cold": want_counts["cold"],
            "debounced": want_counts["debounced"],
            "warm": want_counts["warm"],
            "chunkless": want_counts["chunkless"],
            "noProfile": want_counts["noProfile"],
        }),
        "counts diverged"
    );

    eprintln!("OK: cold-chunk-reembed matched oracle (counts incl. debounce/no-profile + jobs).");
}
