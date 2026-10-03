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
/// number of NEW jobs enqueued (0 with v4's WARN when no default profile or no
/// user is configured — v4's warn-and-return-0 arms; P4.142 replaced the
/// `eprintln!`s with v4's lines, `embedding-scheduler.ts:80-127`).
pub fn enqueue_embedding_jobs_for_mount_point(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
) -> Result<i64, DbError> {
    // v4's `findByMountPointId` is the FALLBACK joined read (`[]` after its
    // `Error querying joined file links` ERROR), not a throw (P4.131).
    let links =
        DocMountFileLinksRepository::new(mount).find_by_mount_point_id_or_empty(mount_point_id);
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
    // v4's `clearEmbeddingsByLinkId` is a fallback WRITE (`0` after its own
    // `Error clearing embeddings by link ID`), so the `Failed to clear embeddings
    // for embed:false link` WARN around it (`embedding-scheduler.ts:47-53`) is
    // unreachable — the home's line is the only one (P4.142).
    let chunks_repo = DocMountChunksRepository::new(mount);
    for link_id in blocked {
        chunks_repo.clear_embeddings_by_link_id_or_zero(link_id);
    }

    // Un-embedded chunks whose link is not blocked (a link absent from the map
    // — e.g. its row was just deleted — defaults to allowed, v4's behavior).
    // v4's `findByMountPointId` falls back (`Error finding entities by filter`,
    // `[]`), so a failed read is "nothing to embed" — never an `Err` (P4.142).
    let all_chunks = chunks_repo.find_rows_by_mount_point_id_or_empty(mount_point_id);
    let unembedded: Vec<_> = all_chunks
        .iter()
        .filter(|c| !c.has_embedding && allow_by_link.get(c.link_id.as_str()) != Some(&false))
        .collect();
    if unembedded.is_empty() {
        return Ok(0);
    }

    // v4 `embeddingProfiles.findAll()` and `users.findAll()` are `_findAll`
    // fallbacks (`Error finding all entities`, `[]`), so a failed read takes the
    // no-default / no-user WARN arm below, never an `Err` (P4.142).
    let profiles =
        crate::db::fallback::find_all_or_empty("embedding_profiles", || all_profile_heads(main));
    let Some((profile_id, profile_name, _)) = profiles.into_iter().find(|(_, _, d)| *d) else {
        tracing::warn!(
            target: "quilltap::mount_index",
            mountPointId = %mount_point_id,
            unembeddedCount = unembedded.len(),
            "No default embedding profile configured, skipping mount chunk embedding"
        );
        return Ok(0);
    };
    let users = crate::db::fallback::find_all_or_empty("users", || {
        first_user_id(main).map(|u| u.into_iter().collect::<Vec<_>>())
    });
    let Some(user_id) = users.into_iter().next() else {
        tracing::warn!(
            target: "quilltap::mount_index",
            mountPointId = %mount_point_id,
            "No user found, skipping mount chunk embedding"
        );
        return Ok(0);
    };

    tracing::info!(
        target: "quilltap::mount_index",
        mountPointId = %mount_point_id,
        chunkCount = unembedded.len(),
        profileId = %profile_id,
        profileName = %profile_name,
        "Enqueuing embedding jobs for mount chunks"
    );
    let mut enqueued = 0i64;
    for chunk in &unembedded {
        match enqueue_mount_chunk_embedding(main, &user_id, &chunk.id, &profile_id) {
            Ok((_, true)) => enqueued += 1,
            Ok((_, false)) => {}
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::mount_index",
                    chunkId = %chunk.id,
                    error = %e,
                    "Failed to enqueue embedding job for mount chunk"
                );
            }
        }
    }
    tracing::info!(
        target: "quilltap::mount_index",
        mountPointId = %mount_point_id,
        totalChunks = unembedded.len(),
        enqueued,
        "Embedding job enqueueing complete"
    );
    Ok(enqueued)
}

/// Every embedding profile's `(id, name, isDefault)`, in table order — the
/// scheduler's view of v4's `embeddingProfiles.findAll()` (it logs the default
/// profile's NAME, `embedding-scheduler.ts:97-102`).
fn all_profile_heads(main: &Connection) -> Result<Vec<(String, String, bool)>, DbError> {
    let mut stmt = main.prepare("SELECT id, name, isDefault FROM embedding_profiles")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?.unwrap_or(0) != 0,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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

#[cfg(test)]
mod fallback_read_tests {
    use super::*;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A provisioned instance's two writable connections (healthy schema, no
    /// rows).
    fn instance(dir: &tempfile::TempDir) -> (crate::db::Writer, crate::db::Writer) {
        crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
        (
            crate::db::Writer::open_writable(&dir.path().join("quilltap.db"), PEPPER).unwrap(),
            crate::db::Writer::open_writable(&dir.path().join("quilltap-mount-index.db"), PEPPER)
                .unwrap(),
        )
    }

    /// P4.142: v4's chunks read (`embedding-scheduler.ts:60`, `findByMountPointId`)
    /// falls back — `Error finding entities by filter {collection:
    /// doc_mount_chunks}` and `[]` — so a broken chunks table is "nothing to
    /// embed", `Ok(0)`, never an `Err` (which used to reach the caller's WARN).
    #[test]
    fn a_failed_chunks_read_logs_v4s_line_and_enqueues_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = instance(&dir);
        mount
            .connection()
            .execute_batch("DROP TABLE doc_mount_chunks")
            .unwrap();
        let (got, lines) = crate::test_support::captured_with(|| {
            enqueue_embedding_jobs_for_mount_point(main.connection(), mount.connection(), "mp-1")
        });
        assert_eq!(got.unwrap(), 0);
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error finding entities by filter collection=doc_mount_chunks error=no such table: doc_mount_chunks".to_string()]
        );
    }

    /// One un-embedded chunk on `mp-1` (its link absent from the allow map —
    /// v4's "defaults to allowed").
    fn plant_chunk(mount: &Connection) {
        mount
            .execute(
                "INSERT INTO doc_mount_chunks (id, linkId, mountPointId, chunkIndex, content, \
                 tokenCount, headingContext, embedding, createdAt, updatedAt) VALUES \
                 ('ch-1', 'l-x', 'mp-1', 0, 'text', 1, NULL, NULL, 't', 't')",
                [],
            )
            .unwrap();
    }

    fn run(main: &crate::db::Writer, mount: &crate::db::Writer) -> (i64, Vec<String>) {
        let (got, lines) = crate::test_support::captured_with(|| {
            enqueue_embedding_jobs_for_mount_point(main.connection(), mount.connection(), "mp-1")
        });
        (got.unwrap(), lines)
    }

    /// P4.142 Tier 2 (survey §A6): v4's INFO pair around a real enqueue, its
    /// fields in v4's order (`embedding-scheduler.ts:97-127`), where v5 had
    /// printed nothing.
    #[test]
    fn a_real_enqueue_logs_v4s_info_pair() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = instance(&dir);
        plant_chunk(mount.connection());
        let (pid, pname): (String, String) = main
            .connection()
            .query_row(
                "SELECT id, name FROM embedding_profiles WHERE isDefault = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("the provisioned instance marks a default profile");
        let (n, lines) = run(&main, &mount);
        assert_eq!(n, 1);
        assert_eq!(
            lines,
            vec![
                format!("INFO quilltap::mount_index Enqueuing embedding jobs for mount chunks mountPointId=mp-1 chunkCount=1 profileId={pid} profileName={pname}"),
                "INFO quilltap::mount_index Embedding job enqueueing complete mountPointId=mp-1 totalChunks=1 enqueued=1".to_string(),
            ]
        );
    }

    /// v4's two warn-and-return-0 arms, and a failed profile read taking the
    /// first of them through `_findAll`'s fallback (`Error finding all
    /// entities {collection: embedding_profiles}`, `[]`) rather than an `Err`.
    #[test]
    fn the_no_default_and_no_user_arms_warn_as_v4() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = instance(&dir);
        plant_chunk(mount.connection());
        main.connection()
            .execute_batch("UPDATE embedding_profiles SET isDefault = 0")
            .unwrap();
        let (n, lines) = run(&main, &mount);
        assert_eq!(n, 0);
        assert_eq!(
            lines,
            vec!["WARN quilltap::mount_index No default embedding profile configured, skipping mount chunk embedding mountPointId=mp-1 unembeddedCount=1".to_string()]
        );

        main.connection()
            .execute_batch("ALTER TABLE embedding_profiles RENAME COLUMN isDefault TO isDefault_x")
            .unwrap();
        let (n, lines) = run(&main, &mount);
        assert_eq!(n, 0);
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error finding all entities collection=embedding_profiles error=no such column: isDefault".to_string(),
                "WARN quilltap::mount_index No default embedding profile configured, skipping mount chunk embedding mountPointId=mp-1 unembeddedCount=1".to_string(),
            ]
        );

        main.connection()
            .execute_batch(
                "ALTER TABLE embedding_profiles RENAME COLUMN isDefault_x TO isDefault; \
                 UPDATE embedding_profiles SET isDefault = 1 WHERE rowid = (SELECT MIN(rowid) FROM embedding_profiles); \
                 DELETE FROM users",
            )
            .unwrap();
        let (n, lines) = run(&main, &mount);
        assert_eq!(n, 0);
        assert_eq!(
            lines,
            vec!["WARN quilltap::mount_index No user found, skipping mount chunk embedding mountPointId=mp-1".to_string()]
        );
    }

    /// v4's per-chunk catch (`:116-119`): a failed enqueue WARNs `{chunkId,
    /// error}` and the run goes on — the INFO pair still closes it.
    #[test]
    fn a_failed_enqueue_warns_and_the_run_completes() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = instance(&dir);
        plant_chunk(mount.connection());
        main.connection()
            .execute_batch("DROP TABLE background_jobs")
            .unwrap();
        let (n, lines) = run(&main, &mount);
        assert_eq!(n, 0);
        assert_eq!(lines.len(), 3, "{lines:#?}");
        assert!(
            lines[1].starts_with(
                "WARN quilltap::mount_index Failed to enqueue embedding job for mount chunk chunkId=ch-1 error="
            ),
            "{}",
            lines[1]
        );
        assert!(
            lines[2].ends_with("totalChunks=1 enqueued=0"),
            "{}",
            lines[2]
        );
    }

    /// The blocked-link clear is v4's fallback WRITE: a failure logs `Error
    /// clearing embeddings by link ID` and the run goes on (v4's own `Failed to
    /// clear embeddings for embed:false link` WARN is unreachable).
    #[test]
    fn a_failed_clear_logs_the_homes_line_only() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = instance(&dir);
        let mp: String = mount
            .connection()
            .query_row(
                "SELECT id FROM doc_mount_points WHERE mountType = 'database' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount.connection())
            .write_database_document(&mp, "notes/a.md", "hello")
            .unwrap();
        mount
            .connection()
            .execute_batch(
                "UPDATE doc_mount_file_links SET allowEmbed = 0; \
                 ALTER TABLE doc_mount_chunks RENAME COLUMN embedding TO embedding_x",
            )
            .unwrap();
        let link_id: String = mount
            .connection()
            .query_row(
                "SELECT id FROM doc_mount_file_links WHERE mountPointId = ?1",
                [&mp],
                |r| r.get(0),
            )
            .unwrap();
        let (got, lines) = crate::test_support::captured_with(|| {
            enqueue_embedding_jobs_for_mount_point(main.connection(), mount.connection(), &mp)
        });
        assert_eq!(got.unwrap(), 0);
        assert_eq!(
            lines,
            vec![
                format!("ERROR quilltap::db Error clearing embeddings by link ID collection=doc_mount_chunks linkId={link_id} error=no such column: embedding"),
                "ERROR quilltap::db Error finding entities by filter collection=doc_mount_chunks error=no such column: embedding".to_string(),
            ]
        );
    }

    /// The silence leg: a healthy, empty mount — `Ok(0)` and no line.
    #[test]
    fn a_healthy_empty_mount_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = instance(&dir);
        let (got, lines) = crate::test_support::captured_with(|| {
            enqueue_embedding_jobs_for_mount_point(main.connection(), mount.connection(), "mp-1")
        });
        assert_eq!(got.unwrap(), 0);
        assert!(lines.is_empty(), "{lines:?}");
    }
}
