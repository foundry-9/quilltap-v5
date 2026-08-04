//! The daily maintenance pass (P4.1d, v4
//! `lib/background-jobs/scheduled-maintenance.ts` `runScheduledMaintenance`) —
//! the single parent-side tick that reaps data with no bearing on characters,
//! stories, or memories:
//!
//! 1. finished background jobs (COMPLETED after a short window, DEAD after a
//!    longer one) — the ported [`queue_service::cleanup_finished_jobs`];
//! 2. superseded generated story-backgrounds & wardrobe avatars of *stale*
//!    chats — the ported [`crate::services::maintenance::collapse_stale_chat_assets`]
//!    (its storage-bytes half is the standing host FsSeam);
//! 3. regenerable caches of *stale* chats (compression cache, rendered
//!    markdown/HTML, raw provider payloads, thinking traces, memory-gate debug
//!    logs) plus cold-tiering of their conversation-chunk embeddings — the ported
//!    [`crate::services::collapse_stale_chat_caches::collapse_stale_chat_caches`];
//! 4. orphaned mount-index files (belt-and-suspenders after the collapse) —
//!    [`crate::db::doc_mount_file_links::sweep_orphaned_files`];
//!    (4b, v5-only, P4.31) mount-index rows whose STORE is gone —
//!    [`crate::db::doc_mount_file_links::sweep_orphaned_store_children`]. The
//!    boot hook runs it too; this is the arm that heals a server which has not
//!    restarted in weeks;
//! 5. closed terminal (Ariel) PTY sessions and their transcript files —
//!    [`cleanup_closed_sessions`], the transcript unlink behind the injected
//!    [`TranscriptStore`] seam (the fs lives host-side).
//!
//! Each sweep runs in order and independently try/caught (a failure in one
//! cannot abort the rest); `lastMaintenanceSweepAt` is recorded at the end
//! REGARDLESS of per-sweep failures ("last attempted pass", so the host's
//! dev-restart startup short-circuit works). The 24 h cadence + the 5-minute
//! startup grace + the 20 h recent-run window live in the host driver
//! (`quilltap-host`), consulting [`crate::services::job_scheduler`].
//!
//! ## v4 error-shape note (documented micro-seam)
//!
//! v4's `sweepOrphanedFiles` / `cleanupClosedSessions` are `safeQuery`-wrapped
//! (repo-level swallow to a `0` default), so v4's per-sweep try/catch around
//! them is effectively unreachable — its `failures` list can only carry
//! `'jobs'`/`'assets'`. The port surfaces repo errors to this orchestration and
//! records them in `failures` (a strictly more informative shape on a path v4
//! cannot observably reach); an ABSENT mount-index partition reproduces v4's
//! `getRawMountIndexDatabase() === null → 0` (no failure entry).

use crate::clock::iso_from_unix_ms;
use crate::db::runtime::Db;
use crate::db::{instance_settings, terminal_sessions, DbError};
use crate::services::collapse_stale_chat_caches::collapse_stale_chat_caches;
use crate::services::maintenance::collapse_stale_chat_assets;
use crate::services::queue_service::{
    self, cleanup_finished_jobs, retention_cutoff_iso, CLOSED_TERMINAL_RETENTION_DAYS,
};

/// The transcript-file half of the terminal cleanup (v4 unlinks
/// `<logsDir>/terminals/<id>.log` — or the row's explicit `transcriptPath` —
/// best-effort, swallowing ENOENT). The fs lives host-side; the core calls
/// through this seam. Returns `true` when a file was actually removed (v4
/// counts only successful unlinks — an already-gone transcript is success but
/// NOT counted).
pub trait TranscriptStore: Send + Sync {
    fn unlink_transcript(&self, session_id: &str, transcript_path: Option<&str>) -> bool;
}

/// The no-fs default (tests / embedders without a logs dir): nothing is ever
/// removed, so `transcripts` counts 0 while rows still reap.
pub struct NoTranscriptStore;

impl TranscriptStore for NoTranscriptStore {
    fn unlink_transcript(&self, _session_id: &str, _transcript_path: Option<&str>) -> bool {
        false
    }
}

/// v4 `MaintenanceSweepSummary`.
#[derive(Debug, Clone, PartialEq)]
pub struct MaintenanceSweepSummary {
    /// COMPLETED jobs reaped.
    pub jobs_completed: usize,
    /// DEAD jobs reaped.
    pub jobs_dead: usize,
    /// The asset collapse's `staleChats` / `chatsCollapsed` / `filesDeleted`.
    pub assets_stale_chats: usize,
    pub assets_chats_collapsed: usize,
    pub assets_files_deleted: usize,
    /// The cache collapse's summary (v4 `caches`): stale chats, chats collapsed,
    /// and the `chats` / `chat_messages` / `conversation_chunks` rows cleared.
    pub caches_stale_chats: usize,
    pub caches_chats_collapsed: usize,
    pub caches_chat_rows_cleared: usize,
    pub caches_message_rows_cleared: usize,
    pub caches_chunk_embeddings_cleared: usize,
    /// Orphaned `doc_mount_files` rows swept.
    pub orphaned_files_swept: usize,
    /// P4.31 (v5-only): rows reaped because their mount point no longer exists
    /// — links / folders / chunks / collected content, summed.
    pub parentless_store_rows_reaped: usize,
    /// Terminal-session rows reaped / transcript files removed.
    pub terminal_rows: usize,
    pub terminal_transcripts: usize,
    /// Sweeps that threw (and were swallowed so the rest could run):
    /// `jobs` / `assets` / `orphans` / `parentless` / `terminals`.
    pub failures: Vec<String>,
}

/// v4 `terminalSessions.cleanupClosedSessions(olderThan)`: reap closed terminal
/// sessions older than the cutoff, along with their transcript files.
///
/// A session is eligible only when it has actually exited (`exitedAt` non-null)
/// AND exited before the cutoff — sessions still running are NEVER selected,
/// regardless of how old `startedAt` is (the [`terminal_sessions::
/// find_closed_before`] read carries both v4 layers of that guard). Per
/// candidate: delete the row (a no-op delete skips the transcript, faithful to
/// v4's `if (!deleted) continue`), then unlink the transcript through the seam
/// (the row's `transcriptPath`, else the store's default `<id>.log` location).
/// Returns `(rows, transcripts)`.
pub async fn cleanup_closed_sessions(
    db: &Db,
    cutoff_iso: &str,
    transcripts: &dyn TranscriptStore,
) -> Result<(usize, usize), DbError> {
    let cutoff = cutoff_iso.to_string();
    let candidates =
        db.read_main(move |conn| terminal_sessions::find_closed_before(conn, &cutoff))?;

    let mut rows = 0usize;
    let mut removed = 0usize;
    for session in candidates {
        let sid = session.id.clone();
        let deleted = db
            .write(move |writers| {
                terminal_sessions::TerminalSessionsRepository::new(writers.main().connection())
                    .delete(&sid)
            })
            .await?;
        if !deleted {
            continue;
        }
        rows += 1;
        if transcripts.unlink_transcript(&session.id, session.transcript_path.as_deref()) {
            removed += 1;
        }
    }
    Ok((rows, removed))
}

/// One maintenance pass (v4 `runScheduledMaintenance`). `now_ms` anchors every
/// retention cutoff AND the recorded `lastMaintenanceSweepAt` stamp (v4 uses
/// the wall clock at each site; the single anchor is deterministic and
/// semantically identical — "when the pass ran").
pub async fn run_scheduled_maintenance(
    db: &Db,
    now_ms: i64,
    transcripts: &dyn TranscriptStore,
) -> MaintenanceSweepSummary {
    let mut summary = MaintenanceSweepSummary {
        jobs_completed: 0,
        jobs_dead: 0,
        assets_stale_chats: 0,
        assets_chats_collapsed: 0,
        assets_files_deleted: 0,
        caches_stale_chats: 0,
        caches_chats_collapsed: 0,
        caches_chat_rows_cleared: 0,
        caches_message_rows_cleared: 0,
        caches_chunk_embeddings_cleared: 0,
        orphaned_files_swept: 0,
        parentless_store_rows_reaped: 0,
        terminal_rows: 0,
        terminal_transcripts: 0,
        failures: Vec::new(),
    };

    // 1. Finished background jobs (COMPLETED short window, DEAD longer window).
    match cleanup_finished_jobs(db, now_ms).await {
        Ok((completed, dead)) => {
            summary.jobs_completed = completed;
            summary.jobs_dead = dead;
        }
        Err(_) => summary.failures.push("jobs".to_string()),
    }

    // 2. Stale-chat asset collapse.
    match collapse_stale_chat_assets(db, now_ms).await {
        Ok(r) => {
            summary.assets_stale_chats = r.stale_chats;
            summary.assets_chats_collapsed = r.chats_collapsed;
            summary.assets_files_deleted = r.files_deleted;
        }
        Err(_) => summary.failures.push("assets".to_string()),
    }

    // 3. Stale-chat cache collapse + conversation-chunk cold-tiering.
    match collapse_stale_chat_caches(db, now_ms).await {
        Ok(r) => {
            summary.caches_stale_chats = r.stale_chats;
            summary.caches_chats_collapsed = r.chats_collapsed;
            summary.caches_chat_rows_cleared = r.chat_rows_cleared;
            summary.caches_message_rows_cleared = r.message_rows_cleared;
            summary.caches_chunk_embeddings_cleared = r.chunk_embeddings_cleared;
        }
        Err(_) => summary.failures.push("caches".to_string()),
    }

    // 4. Orphaned mount-index files — AFTER the collapse, mopping up stragglers.
    // An absent mount-index partition is v4's `getRawMountIndexDatabase() ===
    // null → 0` (not a failure).
    match db
        .write(|writers| match writers.mount_index() {
            Some(w) => crate::db::doc_mount_file_links::sweep_orphaned_files(w.connection()),
            None => Ok(0),
        })
        .await
    {
        Ok(n) => summary.orphaned_files_swept = n,
        Err(_) => summary.failures.push("orphans".to_string()),
    }

    // 4b. P4.31 (v5-only, dogfood finding #58): the parentless-store reaper.
    // `ensure_builtin_mounts` runs it at boot, which heals a desktop instance on
    // its next launch; a long-running server (the Docker deployment) may not
    // restart for weeks, so it rides the daily sweep too. Same failure shape as
    // 4 above: an absent mount-index partition is v4's `null → 0`, not a
    // failure. v4 has no counterpart, so this is reported under its own key and
    // never joins `failures`' v4-shaped list under an existing name.
    match db
        .write(|writers| match writers.mount_index() {
            Some(w) => {
                crate::db::doc_mount_file_links::sweep_orphaned_store_children(w.connection())
            }
            None => Ok(Default::default()),
        })
        .await
    {
        Ok(r) => summary.parentless_store_rows_reaped = r.links + r.folders + r.chunks + r.content,
        Err(e) => {
            // The swallow-site rule (P4.9G3's lesson): a failure that only
            // becomes a summary key leaves nothing to diagnose with.
            tracing::error!(
                target: "quilltap::maintenance",
                error = %e,
                "The parentless-store-children reaper failed during the maintenance sweep",
            );
            summary.failures.push("parentless".to_string());
        }
    }

    // 5. Closed terminal sessions + transcript files.
    let cutoff = retention_cutoff_iso(CLOSED_TERMINAL_RETENTION_DAYS, now_ms);
    match cleanup_closed_sessions(db, &cutoff, transcripts).await {
        Ok((rows, files)) => {
            summary.terminal_rows = rows;
            summary.terminal_transcripts = files;
        }
        Err(_) => summary.failures.push("terminals".to_string()),
    }

    // Record the pass regardless of per-sweep failures ("last attempted pass").
    // A record failure is warned-and-swallowed in v4; same here.
    let stamp = iso_from_unix_ms(now_ms);
    let _ = db
        .write(move |writers| {
            instance_settings::set_last_maintenance_sweep_at(writers.main().connection(), &stamp)
        })
        .await;

    summary
}

/// Re-export so the host's sweep loops read one module for the terminal window
/// (the other windows are already on `queue_service`).
pub use queue_service::DAY_MS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::Db;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A recording store that "removes" every transcript except a named one
    /// (simulating an already-gone file → not counted).
    struct RecordingStore {
        missing_id: String,
        calls: AtomicUsize,
    }
    impl TranscriptStore for RecordingStore {
        fn unlink_transcript(&self, session_id: &str, _path: Option<&str>) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            session_id != self.missing_id
        }
    }

    fn test_db() -> Db {
        let dir = std::env::temp_dir().join(format!(
            "qt-sched-maint-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open_main(
            dir.join("main.db"),
            "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
        )
        .unwrap();
        db.write_blocking(|writers| {
            writers.main().connection().execute_batch(
                "CREATE TABLE terminal_sessions (
                    id TEXT PRIMARY KEY, chatId TEXT NOT NULL, label TEXT,
                    shell TEXT NOT NULL, cwd TEXT NOT NULL, startedAt TEXT NOT NULL,
                    exitedAt TEXT, exitCode REAL, transcriptPath TEXT,
                    createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
            )?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn seed_session(db: &Db, id: &str, exited_at: Option<&str>, transcript: Option<&str>) {
        let row = format!(
            "INSERT INTO terminal_sessions (id, chatId, shell, cwd, startedAt, exitedAt, transcriptPath, createdAt, updatedAt) \
             VALUES ('{id}', 'c1', 'zsh', '/tmp', '2020-01-01T00:00:00.000Z', {}, {}, '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
            exited_at.map(|e| format!("'{e}'")).unwrap_or("NULL".into()),
            transcript.map(|t| format!("'{t}'")).unwrap_or("NULL".into()),
        );
        db.write_blocking(move |writers| {
            writers.main().connection().execute_batch(&row)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn cleanup_reaps_only_closed_old_sessions() {
        let db = test_db();
        // Old + closed → reaped (transcript removed).
        seed_session(&db, "s-old", Some("2020-06-01T00:00:00.000Z"), None);
        // Old + closed, transcript "already gone" → row reaped, not counted.
        seed_session(&db, "s-gone", Some("2020-06-02T00:00:00.000Z"), None);
        // Closed but recent → kept.
        seed_session(&db, "s-recent", Some("2026-01-01T00:00:00.000Z"), None);
        // Still LIVE (exitedAt NULL), ancient startedAt → NEVER touched.
        seed_session(&db, "s-live", None, None);

        let store = RecordingStore {
            missing_id: "s-gone".to_string(),
            calls: AtomicUsize::new(0),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (rows, transcripts) = rt
            .block_on(cleanup_closed_sessions(
                &db,
                "2025-01-01T00:00:00.000Z",
                &store,
            ))
            .unwrap();
        assert_eq!(rows, 2);
        assert_eq!(transcripts, 1);
        assert_eq!(store.calls.load(Ordering::SeqCst), 2);

        // The survivors.
        let left: Vec<String> = db
            .read_main(|c| {
                let mut stmt = c.prepare("SELECT id FROM terminal_sessions ORDER BY id")?;
                let rows = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(left, vec!["s-live".to_string(), "s-recent".to_string()]);
    }

    #[test]
    fn full_pass_records_stamp_and_isolates_failures() {
        let db = test_db();
        // instance_settings for the stamp; NO background_jobs / chats / files
        // tables → the jobs + assets sweeps fail and are isolated.
        db.write_blocking(|writers| {
            writers.main().connection().execute_batch(
                "CREATE TABLE instance_settings (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
            )?;
            Ok(())
        })
        .unwrap();
        seed_session(&db, "s-old", Some("2020-06-01T00:00:00.000Z"), None);

        let now_ms = crate::clock::iso_to_ms("2026-07-08T00:00:00.000Z").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let summary = rt.block_on(run_scheduled_maintenance(&db, now_ms, &NoTranscriptStore));

        // jobs + assets + caches failed (missing chats/background_jobs tables);
        // orphans hit the absent mount-index partition → 0, NOT a failure;
        // terminals ran.
        assert_eq!(
            summary.failures,
            vec![
                "jobs".to_string(),
                "assets".to_string(),
                "caches".to_string()
            ]
        );
        assert_eq!(summary.orphaned_files_swept, 0);
        assert_eq!(summary.terminal_rows, 1);
        assert_eq!(summary.terminal_transcripts, 0); // NoTranscriptStore

        // The stamp was recorded at the pass anchor despite the failures.
        let stamp = db
            .read_main(crate::db::instance_settings::get_last_maintenance_sweep_at)
            .unwrap();
        assert_eq!(stamp, Some(now_ms));
    }
}
