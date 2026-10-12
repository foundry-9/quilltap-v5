//! v4 `markMemoriesAccessed` (`lib/memory/memory-service.ts:1373-1385`, v4
//! `3f7320138`) — bump `lastAccessedAt` for the memories a turn actually
//! DELIVERED, so housekeeping's recent-access protection reflects real use
//! rather than every search hit.
//!
//! P4.D267 deletes the semantic search's internal `bump_access_times`; P4.D268
//! calls this at every delivery site (this round, §R.14). Non-fatal: a failure
//! never reaches the caller.
//!
//! ## Lines
//!
//! - DEBUG `[Memory] Marking memories accessed {characterId, count}` — on the
//!   CALLER thread, before the write (memory note `capture-rig-writer-thread`:
//!   a line inside a `Db::write` closure is invisible to the thread-scoped rig).
//! - On a failed write, v4's REPOSITORY lines — `updateAccessTimeBulk` is a
//!   FALLBACK `safeQuery` (`memories.repository.ts:715-731`) over the base
//!   `updateMany` RETHROW — so `Error updating entities {collection}` then
//!   `Error bulk-updating memory access times {collection, characterId, count,
//!   error}`, through `db::fallback::memory_access_times_or_zero` on the
//!   caller thread.
//! - v4's own `.catch` WARN `[Memory] Failed to bump lastAccessedAt for used
//!   memories` is UNREACHABLE through v4's real code (the fallback never
//!   rejects) and is pinned NEGATIVE (§R.5) — never emitted here.

use crate::db::runtime::Db;

/// v4 `markMemoriesAccessed(characterId, memoryIds)`: the non-empty string ids,
/// de-duplicated in first-seen order (`Array.from(new Set(…filter(id =>
/// typeof id === 'string' && id.length > 0)))`); none → return silently.
pub async fn mark_memories_accessed(db: &Db, character_id: &str, memory_ids: &[String]) {
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<String> = memory_ids
        .iter()
        .filter(|id| !id.is_empty() && seen.insert(id.as_str()))
        .cloned()
        .collect();
    if ids.is_empty() {
        return;
    }
    tracing::debug!(
        target: "quilltap::memories",
        characterId = %character_id,
        count = ids.len(),
        "[Memory] Marking memories accessed"
    );
    let count = ids.len();
    let cid = character_id.to_string();
    let result = db
        .write(move |writers| {
            writers
                .main()
                .memories()
                .update_access_time_bulk(&cid, &ids)
        })
        .await;
    // The repository's fallback lines, on THIS thread; the answer (rows or 0)
    // is unused, as v4's fire-and-forget never reads it.
    let _ = crate::db::fallback::memory_access_times_or_zero(character_id, count, || result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Writer;
    use tempfile::tempdir;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    async fn db_with(rows: &[&str]) -> (tempfile::TempDir, Db) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("quilltap.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE memories (id TEXT PRIMARY KEY, characterId TEXT, \
                     lastAccessedAt TEXT, updatedAt TEXT)",
                )
                .unwrap();
            for id in rows {
                w.connection()
                    .execute(
                        "INSERT INTO memories (id, characterId) VALUES (?1, 'c1')",
                        [id],
                    )
                    .unwrap();
            }
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn accessed(db: &Db) -> Vec<(String, bool)> {
        db.read_main(|c| {
            let mut s = c
                .prepare("SELECT id, lastAccessedAt IS NOT NULL FROM memories ORDER BY id")
                .unwrap();
            let rows = s
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            Ok(rows)
        })
        .unwrap()
    }

    #[tokio::test]
    async fn dedups_drops_empty_ids_and_logs_the_deduped_count() {
        let (_dir, db) = db_with(&["m1", "m2", "m3"]).await;
        let ids: Vec<String> = ["m1", "", "m2", "m1"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (_, lines) = crate::test_support::global_capture::capture_async(async {
            mark_memories_accessed(&db, "c1", &ids).await
        })
        .await;
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::memories [Memory] Marking memories accessed characterId=c1 count=2"
                    .to_string()
            ]
        );
        assert_eq!(
            accessed(&db),
            vec![
                ("m1".into(), true),
                ("m2".into(), true),
                ("m3".into(), false)
            ]
        );
    }

    #[tokio::test]
    async fn an_empty_list_is_silent_and_a_failure_logs_v4s_repository_lines() {
        let (_dir, db) = db_with(&[]).await;
        let (_, lines) = crate::test_support::global_capture::capture_async(async {
            mark_memories_accessed(&db, "c1", &[String::new()]).await
        })
        .await;
        assert!(lines.is_empty(), "{lines:?}");

        db.write(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE memories")
                .map_err(Into::into)
        })
        .await
        .unwrap();
        let (_, lines) = crate::test_support::global_capture::capture_async(async {
            mark_memories_accessed(&db, "c1", &["m1".to_string()]).await
        })
        .await;
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines[1].starts_with(
            "ERROR quilltap::db Error updating entities collection=memories error=no such table: memories"
        ));
        assert!(lines[2].starts_with(
            "ERROR quilltap::db Error bulk-updating memory access times collection=memories \
             characterId=c1 count=1 error=no such table: memories"
        ));
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Failed to bump lastAccessedAt")),
            "v4's own WARN is unreachable — pinned negative: {lines:?}"
        );
    }
}
