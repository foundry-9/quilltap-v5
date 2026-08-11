//! Port of v4 `lib/mount-index/embedding-scheduler.ts` +
//! `reindex.ts::enqueueEmbeddingJobsScoped`'s shared enqueue plumbing — the
//! EMBEDDING_GENERATE enqueue for un-embedded mount chunks, enforcing the
//! per-document `embed:false` policy (skip blocked links AND erase any vectors
//! a blocked link still carries).
//!
//! These run at the conn-pair level: chunk/link reads + embedding erasure hit
//! the **mount-index** connection; job rows land on the **main** connection —
//! both owned by the same `WriterSet` inside one `Db::write` closure. The v4
//! `ensureProcessorRunning()` wake fires once from the dispatch handler after
//! the write lands (idempotent — v4 wakes per enqueued job).

use rusqlite::Connection;
use serde_json::json;

use crate::db::background_jobs::{BackgroundJobsRepository, BjCreate, CreateOptions};
use crate::db::doc_mount_chunks::DocMountChunksRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::DbError;

/// v4 `EMBEDDING_ENTITY_PRIORITIES['MOUNT_CHUNK']` — batch indexing runs below
/// chat-related embeddings (MEMORY/CONVERSATION_CHUNK are 10).
const MOUNT_CHUNK_PRIORITY: f64 = 0.0;

/// v4 `enqueueEmbeddingGenerate(userId, {entityType:'MOUNT_CHUNK', entityId,
/// profileId})`: de-dupe against in-flight EMBEDDING_GENERATE jobs for the same
/// entity, else mint a PENDING job at the entity-type priority. Returns
/// `(job_id, is_new)`.
pub fn enqueue_mount_chunk_embedding(
    main: &Connection,
    user_id: &str,
    chunk_id: &str,
    profile_id: &str,
) -> Result<(String, bool), DbError> {
    let jobs = BackgroundJobsRepository::new(main);
    let pending = jobs.find_pending_for_entity(chunk_id)?;
    if let Some(existing) = pending.iter().find(|j| j.job_type == "EMBEDDING_GENERATE") {
        return Ok((existing.id.clone(), false));
    }
    let now = crate::clock::now_iso();
    let id = uuid::Uuid::new_v4().to_string();
    jobs.create(
        &BjCreate {
            user_id: user_id.to_string(),
            job_type: "EMBEDDING_GENERATE".to_string(),
            status: Some("PENDING".to_string()),
            // v4's caller-literal key order.
            payload: json!({
                "entityType": "MOUNT_CHUNK",
                "entityId": chunk_id,
                "profileId": profile_id,
            }),
            priority: MOUNT_CHUNK_PRIORITY,
            attempts: 0.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: now.clone(),
            started_at: None,
            completed_at: None,
        },
        &CreateOptions {
            id: id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;
    Ok((id, true))
}

/// The user's default embedding profile id — v4 `profiles.findAll()` then
/// `find(p => p.isDefault)` and NOTHING else (v4 `d553f72a` dropped the old
/// `|| profiles[0]` fallback at its five embedding sites; NOT user-scoped).
/// ⚠ Help-doc sync is deliberately NOT a consumer: v4's
/// `lib/help/help-doc-sync.ts:359` KEPT the first-row fallback, so it has its
/// own resolver, [`default_or_first_profile_id`].
pub fn default_profile_id(main: &Connection) -> Result<Option<String>, DbError> {
    let mut stmt = main.prepare("SELECT id, isDefault FROM embedding_profiles")?;
    let rows: Vec<(String, bool)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or(0) != 0,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // The **default** embedding profile, and only the default (v4 `d553f72a`):
    // every vector in the instance — memories, conversation chunks, mount
    // chunks — must come from the same profile, or semantic search silently
    // compares apples to oranges. No fallback to an arbitrary profile: with
    // none marked, these chunks WAIT (the startup reconcile re-enqueues them
    // once one is configured), exactly as memories do.
    Ok(rows
        .iter()
        .find(|(_, is_default)| *is_default)
        .map(|(id, _)| id.clone()))
}

/// v4 `lib/help/help-doc-sync.ts:359` — `find(p => p.isDefault) || profiles[0]`.
/// Help-doc sync is the ONE consumer v4's `d553f72a` one-default sweep did NOT
/// touch: it still falls back to the first profile row when none is marked
/// default. Caught by the round-1 unification review (the shared helper above
/// had silently changed this sixth site along with the ordered five); keep the
/// two resolvers separate until v4 itself converges them.
pub fn default_or_first_profile_id(main: &Connection) -> Result<Option<String>, DbError> {
    let mut stmt = main.prepare("SELECT id, isDefault FROM embedding_profiles")?;
    let rows: Vec<(String, bool)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or(0) != 0,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .iter()
        .find(|(_, is_default)| *is_default)
        .map(|(id, _)| id.clone())
        .or_else(|| rows.first().map(|(id, _)| id.clone())))
}

/// v4 `users.findAll()[0]?.id` — the single-user id.
pub fn first_user_id(main: &Connection) -> Result<Option<String>, DbError> {
    main.query_row("SELECT id FROM users LIMIT 1", [], |row| {
        row.get::<_, String>(0)
    })
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// v4 `enqueueEmbeddingJobsForMountPoint(mountPointId)` — the whole-mount
/// enqueue (the scan runner's follow-up): erase embeddings on `embed:false`
/// links, then enqueue for every un-embedded, un-blocked chunk. Returns the
/// number of NEW jobs enqueued (0 with a stderr warning when no profile or no
/// user is configured, matching v4's warn-and-return-0 arms).
pub fn enqueue_embedding_jobs_for_mount_point(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
) -> Result<i64, DbError> {
    let links = DocMountFileLinksRepository::new(mount).find_by_mount_point_id(mount_point_id)?;
    let mut allow_by_link: std::collections::HashMap<&str, bool> = std::collections::HashMap::new();
    let mut blocked: Vec<&str> = Vec::new();
    for l in &links {
        allow_by_link.insert(l.id.as_str(), l.allow_embed);
        if !l.allow_embed {
            blocked.push(l.id.as_str());
        }
    }
    // Erase lingering embeddings for blocked links (NULL, don't delete — the
    // chunk text survives so re-embedding stays possible if the flag flips).
    let chunks_repo = DocMountChunksRepository::new(mount);
    for link_id in blocked {
        if let Err(e) = chunks_repo.clear_embeddings_by_link_id(link_id) {
            eprintln!("Failed to clear embeddings for embed:false link {link_id}: {e}");
        }
    }

    // Un-embedded chunks whose link is not blocked (a link absent from the map
    // — e.g. its row was just deleted — defaults to allowed, v4's behavior).
    let all_chunks = chunks_repo.find_rows_by_mount_point_id(mount_point_id)?;
    let unembedded: Vec<_> = all_chunks
        .iter()
        .filter(|c| !c.has_embedding && allow_by_link.get(c.link_id.as_str()) != Some(&false))
        .collect();
    if unembedded.is_empty() {
        return Ok(0);
    }

    let Some(profile_id) = default_profile_id(main)? else {
        eprintln!(
            "No embedding profile configured, skipping mount chunk embedding \
             (mount {mount_point_id}, {} un-embedded chunks)",
            unembedded.len()
        );
        return Ok(0);
    };
    let Some(user_id) = first_user_id(main)? else {
        eprintln!("No user found, skipping mount chunk embedding (mount {mount_point_id})");
        return Ok(0);
    };

    let mut enqueued = 0i64;
    for chunk in unembedded {
        match enqueue_mount_chunk_embedding(main, &user_id, &chunk.id, &profile_id) {
            Ok((_, true)) => enqueued += 1,
            Ok((_, false)) => {}
            Err(e) => {
                eprintln!(
                    "Failed to enqueue embedding job for mount chunk {}: {e}",
                    chunk.id
                );
            }
        }
    }
    Ok(enqueued)
}

#[cfg(test)]
mod resolver_split_tests {
    use super::*;
    use rusqlite::Connection;

    fn db_with_profiles(rows: &[(&str, i64)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE embedding_profiles (id TEXT PRIMARY KEY, isDefault INTEGER);",
        )
        .unwrap();
        for (id, is_default) in rows {
            conn.execute(
                "INSERT INTO embedding_profiles (id, isDefault) VALUES (?1, ?2)",
                rusqlite::params![id, is_default],
            )
            .unwrap();
        }
        conn
    }

    /// The round-1 unification review's finding: v4 `d553f72a` dropped the
    /// first-row fallback at five embedding sites but KEPT it in help-doc sync
    /// (`help-doc-sync.ts:359`). The two resolvers must stay split — a shared
    /// helper silently changing the sixth site is exactly what shipped to the
    /// review and was caught there.
    #[test]
    fn no_default_marked_splits_the_two_resolvers() {
        let conn = db_with_profiles(&[("first", 0), ("second", 0)]);
        assert_eq!(default_profile_id(&conn).unwrap(), None);
        assert_eq!(
            default_or_first_profile_id(&conn).unwrap(),
            Some("first".to_string())
        );
    }

    #[test]
    fn a_marked_default_wins_in_both_resolvers() {
        let conn = db_with_profiles(&[("first", 0), ("second", 1)]);
        assert_eq!(
            default_profile_id(&conn).unwrap(),
            Some("second".to_string())
        );
        assert_eq!(
            default_or_first_profile_id(&conn).unwrap(),
            Some("second".to_string())
        );
    }

    #[test]
    fn an_empty_table_resolves_to_none_in_both() {
        let conn = db_with_profiles(&[]);
        assert_eq!(default_profile_id(&conn).unwrap(), None);
        assert_eq!(default_or_first_profile_id(&conn).unwrap(), None);
    }
}
