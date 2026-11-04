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
//! 3. regenerable caches of *stale* chats (compression cache, compiled identity
//!    stacks, rendered HTML, raw provider payloads, thinking traces, memory-gate
//!    debug logs) — the ported
//!    [`crate::services::collapse_stale_chat_caches::collapse_stale_chat_caches`].
//!    Conversation-chunk embeddings are NEVER cold-tiered any more (v4
//!    `f7f3d7bf0`, "keep conversation embeddings warm");
//! 4. orphaned store children — mount-index rows whose STORE is gone (v4 bug 9,
//!    `3bb664f0`) — [`crate::db::doc_mount_file_links::sweep_orphaned_store_children`].
//!    Runs BEFORE the orphaned-files sweep so a reaped link can drop its file
//!    too; the boot hook runs it as well, healing a server that has not restarted
//!    in weeks;
//! 5. orphaned mount-index files (belt-and-suspenders after the collapse) —
//!    [`crate::db::doc_mount_file_links::sweep_orphaned_files`];
//! 6. closed terminal (Ariel) PTY sessions and their transcript files —
//!    [`cleanup_closed_sessions`], the transcript unlink behind the injected
//!    [`TranscriptStore`] seam (the fs lives host-side);
//! 7. orphaned thumbnails — `_thumbnails/{fileId}_{size}.webp` cache entries
//!    whose source file is gone (v4 bug 43, `7bcd8515`) —
//!    [`crate::services::file_storage::sweep_orphaned_thumbnails`], over the
//!    injected [`crate::services::file_storage::StorageBackend`].
//! 10. the Built-in (TF-IDF) vocabulary refit check (v4 `4ae0c4ca6`, P4.D292) —
//!     [`crate::services::builtin_refit_policy::check_builtin_refit`]: "AUTO
//!     memories never ask for a refit themselves, so the pass checks whether
//!     the corpus has outgrown the vocabulary (no-op on neural defaults)."
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
///
/// P4.D355 (R-6): the unlink answers v4's THREE outcomes — `Ok(true)` removed,
/// `Ok(false)` already gone (ENOENT — success, silent, not counted),
/// `Err(text)` any other failure (the orphan sweep's WARN carries the text;
/// `cleanup_closed_sessions` counts `Ok(true)` only, as before).
///
/// P4.D355 (contract C6, R-13): [`TranscriptStore::list_transcripts`] — the
/// orphan sweep's directory listing. The contract NAME is kept; the TYPE is
/// richer than the planner's `Vec<(session_id, path, mtime_ms)>` sketch because
/// core owns v4's loop (the `.log` filter, the id slice) and needs the
/// missing-directory arm: `Ok(None)` = the directory does not exist (v4's
/// ENOENT arm), `Ok(Some(entries))` = EVERY directory entry with its `stat`
/// folded in, `Err(text)` = a non-ENOENT listing failure.
pub trait TranscriptStore: Send + Sync {
    fn unlink_transcript(
        &self,
        session_id: &str,
        transcript_path: Option<&str>,
    ) -> Result<bool, String>;

    /// Every entry of `<logsDir>/terminals/` (v4 `fs.readdir` + the per-entry
    /// `fs.stat`), in listing order. Defaulted: no directory.
    fn list_transcripts(&self) -> Result<Option<Vec<TranscriptEntry>>, String> {
        Ok(None)
    }

    /// The listed directory as v4's `transcriptsDir` field renders it.
    fn transcripts_dir_display(&self) -> String {
        String::new()
    }
}

/// One `<logsDir>/terminals/` entry (contract C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    /// The entry's file name (`<id>.log`, `init-<id>.bashrc`, …).
    pub name: String,
    /// `path.join(transcriptsDir, name)`.
    pub path: std::path::PathBuf,
    /// `stat.isFile()` — `false` when the stat itself failed (recorded: v4
    /// stats inside its per-entry try, so a non-ENOENT stat failure there is
    /// the WARN; here the listing folds it into a silent skip).
    pub is_file: bool,
    /// `stat.mtimeMs`.
    pub mtime_ms: i64,
}

/// The no-fs default (tests / embedders without a logs dir): nothing is ever
/// removed, so `transcripts` counts 0 while rows still reap; no directory to
/// sweep.
pub struct NoTranscriptStore;

impl TranscriptStore for NoTranscriptStore {
    fn unlink_transcript(
        &self,
        _session_id: &str,
        _transcript_path: Option<&str>,
    ) -> Result<bool, String> {
        Ok(false)
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
    /// v4 `orphanedStoreChildrenSwept` (bug 9, `3bb664f0`): links / folders /
    /// documents reaped because their mount point vanished. Runs BEFORE the
    /// orphaned-files sweep so a reaped link can drop its file too.
    ///
    /// ⚠ The reaper this maps from ([`crate::db::doc_mount_file_links::
    /// sweep_orphaned_store_children`]) is STRICTLY BROADER than v4's: v5 also
    /// reaps orphaned `doc_mount_chunks` and collected content, where v4 reaps
    /// only documents. On this maintenance fixture there are no orphaned store
    /// children, so `documents` and every value are 0 and the shapes agree; the
    /// chunk-reaping divergence is exercised (and pinned) in
    /// `store_delete_equivalence`'s reaper arm instead.
    pub orphaned_store_children_swept: OrphanedStoreChildrenSwept,
    /// Orphaned `doc_mount_files` rows swept.
    pub orphaned_files_swept: usize,
    /// v4 `orphanedThumbnailsSwept` (bug 43, `7bcd8515`).
    pub orphaned_thumbnails_swept: crate::services::file_storage::OrphanedThumbnailSweep,
    /// Terminal-session rows reaped / transcript files removed.
    pub terminal_rows: usize,
    pub terminal_transcripts: usize,
    /// P4.D355 (v4 `85c002822`, `scheduled-maintenance.ts:92` at `236c04e35`):
    /// `orphanedTranscriptsSwept` — step 6b's `{ scanned, deleted }`. v4's slot
    /// is between `terminals` and `orphanedThumbnailsSwept`; the completion
    /// log carries it there (the struct order is not the wire).
    pub orphaned_transcripts_swept: OrphanedTranscriptSweep,
    /// v4 `foldOtherCatchup`: the fold-grain OTHER idle catch-up sweep's
    /// `{ enqueued, deferred }`.
    pub fold_other_catchup: crate::services::extraction::fold_catchup::FoldCatchupSweepSummary,
    /// v4 `foldEpisodeCatchup` (`ac6348c5f`, bug 232): the episode idle
    /// catch-up sweep's `{ enqueued, deferred }`.
    pub fold_episode_catchup: crate::services::extraction::fold_catchup::FoldCatchupSweepSummary,
    // === P4.D292: the step-10 refit check's summary key ===
    /// v4 `builtinRefit` (`4ae0c4ca6`): "Why the Built-in (TF-IDF)
    /// vocabulary was refit this pass, if it was" — `{ reason, enqueued }`,
    /// initialised `{ null, false }`.
    pub builtin_refit: BuiltinRefitSweep,
    // === end P4.D292 ===
    /// Sweeps that threw (and were swallowed so the rest could run): `jobs` /
    /// `assets` / `caches` / `orphan-store-children` / `orphans` / `terminals` /
    /// `orphan-transcripts` (P4.D355 — in v4's vocabulary but unreachable: the sweep is a fallback read) / `orphan-thumbnails` / `fold-other-catchup` / `fold-episode-catchup` (v4's
    /// failure vocabulary).
    pub failures: Vec<String>,
}

/// P4.D355 — v4 `MaintenanceSweepSummary.orphanedTranscriptsSwept`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OrphanedTranscriptSweep {
    pub scanned: usize,
    pub deleted: usize,
}

// === P4.D292: the step-10 summary shape ===
/// v4 `MaintenanceSweepSummary.builtinRefit` — `{ reason: string | null;
/// enqueued: boolean }`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuiltinRefitSweep {
    pub reason: Option<&'static str>,
    pub enqueued: bool,
}

impl BuiltinRefitSweep {
    /// v4's `{ reason, enqueued }`.
    pub fn to_json(self) -> serde_json::Value {
        serde_json::json!({ "reason": self.reason, "enqueued": self.enqueued })
    }
}
// === end P4.D292 ===

/// v4 `orphanedStoreChildrenSwept` summary shape (bug 9): `{links, folders,
/// documents}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrphanedStoreChildrenSwept {
    pub links: usize,
    pub folders: usize,
    pub documents: usize,
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
        if transcripts.unlink_transcript(&session.id, session.transcript_path.as_deref())
            == Ok(true)
        {
            removed += 1;
        }
    }
    Ok((rows, removed))
}

/// P4.D355 — v4 `TerminalSessionsRepository.sweepOrphanedTranscripts`
/// (`85c002822`, `terminal-sessions.repository.ts:186-275` at `236c04e35`),
/// v4's loop verbatim (R-5 — CORE code over the C6 seam): "Remove transcript
/// files in `<logsDir>/terminals/` that no row names. … Only `<id>.log` files
/// are considered — the shell-init artifacts beside them (`init-<id>.bashrc`,
/// `zdotdir-<id>/`) are cleaned on PTY exit. A file is spared when a row names
/// it (by id or stored `transcriptPath`), when `isLive(id)` says the PTY is
/// still running (a chat deleted under a live terminal leaves the PTY writing
/// with no row), or when it was modified within `graceMs` (the transcript is
/// opened before the row is created)."
///
/// A missing directory answers `(0, 0)` WITHOUT reading the rows (v4's test
/// pins it). `path.resolve` is a LEXICAL absolutize here
/// ([`std::path::absolute`]), never `canonicalize` (which follows symlinks and
/// fails on a missing file). Answers `(scanned, deleted)`.
pub async fn sweep_orphaned_transcripts(
    db: &Db,
    transcripts: &dyn TranscriptStore,
    is_live: &(dyn Fn(&str) -> bool + Sync),
    grace_ms: i64,
    now_ms: i64,
) -> Result<(usize, usize), String> {
    let Some(entries) = transcripts.list_transcripts()? else {
        tracing::debug!(
            target: "quilltap::maintenance",
            transcriptsDir = %transcripts.transcripts_dir_display(),
            "No terminal transcript directory; nothing to sweep"
        );
        return Ok((0, 0));
    };
    let rows = db
        .read_main(terminal_sessions::find_all)
        .map_err(|e| e.to_string())?;
    let resolve = |p: &std::path::Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let named_ids: std::collections::HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    let named_paths: std::collections::HashSet<std::path::PathBuf> = rows
        .iter()
        .filter_map(|r| r.transcript_path.as_deref().filter(|p| !p.is_empty()))
        .map(|p| resolve(std::path::Path::new(p)))
        .collect();
    let cutoff_ms = now_ms - grace_ms;

    let mut scanned = 0usize;
    let mut deleted = 0usize;
    for entry in &entries {
        if !entry.name.ends_with(".log") {
            continue;
        }
        scanned += 1;
        let session_id = &entry.name[..entry.name.len() - ".log".len()];
        if named_ids.contains(session_id) || named_paths.contains(&resolve(&entry.path)) {
            continue;
        }
        if is_live(session_id) {
            tracing::debug!(
                target: "quilltap::maintenance",
                sessionId = %session_id,
                "Sparing transcript of a live PTY with no row"
            );
            continue;
        }
        if !entry.is_file {
            continue;
        }
        if entry.mtime_ms > cutoff_ms {
            tracing::debug!(
                target: "quilltap::maintenance",
                sessionId = %session_id,
                "Sparing recently written orphan transcript"
            );
            continue;
        }
        let file_path = entry.path.to_string_lossy();
        match transcripts.unlink_transcript(session_id, Some(&file_path)) {
            Ok(true) => {
                deleted += 1;
                tracing::debug!(
                    target: "quilltap::maintenance",
                    sessionId = %session_id,
                    filePath = %file_path,
                    "Removed orphaned terminal transcript"
                );
            }
            // ENOENT — the file vanished between the listing and the unlink:
            // silent, not counted.
            Ok(false) => {}
            Err(error) => tracing::warn!(
                target: "quilltap::maintenance",
                sessionId = %session_id,
                filePath = %file_path,
                error = %error,
                "Failed to remove orphaned terminal transcript"
            ),
        }
    }
    tracing::info!(
        target: "quilltap::maintenance",
        scanned = scanned,
        deleted = deleted,
        "Swept orphaned terminal transcripts"
    );
    Ok((scanned, deleted))
}

/// One maintenance pass (v4 `runScheduledMaintenance`). `now_ms` anchors every
/// retention cutoff AND the recorded `lastMaintenanceSweepAt` stamp (v4 uses
/// the wall clock at each site; the single anchor is deterministic and
/// semantically identical — "when the pass ran").
///
/// P4.D355 (R-7): `probe` is the host's live-PTY manager — step 6b's `isLive`
/// (v4 `ptyManager.get(id) !== undefined`); `None` is v4's empty map.
pub async fn run_scheduled_maintenance(
    db: &Db,
    now_ms: i64,
    transcripts: &dyn TranscriptStore,
    backend: &dyn crate::services::file_storage::StorageBackend,
    probe: Option<&dyn crate::services::ariel_notifications::TerminalLivenessProbe>,
) -> MaintenanceSweepSummary {
    // v4 `scheduled-maintenance.ts:182`. The pair of bookend lines is what makes
    // a pass legible in `combined.log`: without the opener an operator cannot
    // tell a pass that is still running from one that never started.
    tracing::info!(
        target: "quilltap::maintenance",
        "Starting scheduled maintenance pass",
    );

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
        orphaned_store_children_swept: OrphanedStoreChildrenSwept::default(),
        orphaned_files_swept: 0,
        orphaned_thumbnails_swept: Default::default(),
        terminal_rows: 0,
        terminal_transcripts: 0,
        orphaned_transcripts_swept: OrphanedTranscriptSweep::default(), // P4.D355
        fold_other_catchup: Default::default(),
        fold_episode_catchup: Default::default(),
        builtin_refit: BuiltinRefitSweep::default(), // P4.D292
        failures: Vec::new(),
    };

    // 1. Finished background jobs (COMPLETED short window, DEAD longer window).
    match cleanup_finished_jobs(db, now_ms).await {
        Ok((completed, dead)) => {
            summary.jobs_completed = completed;
            summary.jobs_dead = dead;
        }
        Err(e) => {
            tracing::warn!(
                target: "quilltap::maintenance",
                error = %e,
                "Job cleanup sweep failed — continuing",
            );
            summary.failures.push("jobs".to_string());
        }
    }

    // 2. Stale-chat asset collapse.
    match collapse_stale_chat_assets(db, now_ms).await {
        Ok(r) => {
            summary.assets_stale_chats = r.stale_chats;
            summary.assets_chats_collapsed = r.chats_collapsed;
            summary.assets_files_deleted = r.files_deleted;
        }
        Err(e) => {
            tracing::warn!(
                target: "quilltap::maintenance",
                error = %e,
                "Stale-chat asset collapse failed — continuing",
            );
            summary.failures.push("assets".to_string());
        }
    }

    // 3. Stale-chat cache collapse.
    match collapse_stale_chat_caches(db, now_ms).await {
        Ok(r) => {
            summary.caches_stale_chats = r.stale_chats;
            summary.caches_chats_collapsed = r.chats_collapsed;
            summary.caches_chat_rows_cleared = r.chat_rows_cleared;
            summary.caches_message_rows_cleared = r.message_rows_cleared;
        }
        Err(e) => {
            tracing::warn!(
                target: "quilltap::maintenance",
                error = %e,
                "Stale-chat cache collapse failed — continuing",
            );
            summary.failures.push("caches".to_string());
        }
    }

    // 4. Orphaned store children — v4 bug 9 (`3bb664f0`). Runs BEFORE the
    // orphaned-files sweep so a reaped link can drop its file too. v5's reaper is
    // STRICTLY BROADER (it also reaps orphaned chunks + collected content — see
    // the summary field's note); the v4-shaped `{links, folders, documents}`
    // summary maps `documents` from v5's content count. `ensure_builtin_mounts`
    // runs the reaper at boot too (healing a desktop instance on next launch); a
    // long-running server may not restart for weeks, so it rides the daily sweep.
    // An absent mount-index partition is v4's `null → 0` (not a failure).
    match db
        .write(|writers| match writers.mount_index() {
            Some(w) => {
                crate::db::doc_mount_file_links::sweep_orphaned_store_children(w.connection())
            }
            None => Ok(Default::default()),
        })
        .await
    {
        Ok(r) => {
            summary.orphaned_store_children_swept = OrphanedStoreChildrenSwept {
                links: r.links,
                folders: r.folders,
                // ⚠ A DIFFERENT quantity than v4's `documents`: v4 counts
                // `doc_mount_documents` rows deleted by a partition-wide
                // orphaned-documents DELETE; v5's reaper counts
                // `doc_mount_files` rows (with their document/blob payload)
                // GC'd for the reaped links specifically. A blob-backed orphan
                // counts here but not in v4, and document rows orphaned by
                // OTHER causes count in v4 but are `sweep_orphaned_link_
                // content`'s job in v5. Equal only at zero (the fixture's
                // steady state); the summary is a log surface, not a wire.
                documents: r.content,
            }
        }
        Err(e) => {
            // The swallow-site rule (P4.9G3's lesson): a failure that only
            // becomes a summary key leaves nothing to diagnose with.
            // v4 `scheduled-maintenance.ts:233` (runSweep's warnMessage). The
            // swallow-site rule that put a line here stands; the SENTENCE and
            // the level are now v4's (dogfood finding #110) rather than the
            // port's own words.
            tracing::warn!(
                target: "quilltap::maintenance",
                error = %e,
                "Orphaned store-children sweep failed — continuing",
            );
            summary.failures.push("orphan-store-children".to_string());
        }
    }

    // 5. Orphaned mount-index files — AFTER the store-children reaper, mopping up
    // stragglers. An absent mount-index partition is v4's `null → 0`.
    match db
        .write(|writers| match writers.mount_index() {
            Some(w) => crate::db::doc_mount_file_links::sweep_orphaned_files(w.connection()),
            None => Ok(0),
        })
        .await
    {
        Ok(n) => summary.orphaned_files_swept = n,
        Err(e) => {
            tracing::warn!(
                target: "quilltap::maintenance",
                error = %e,
                "Orphaned-file sweep failed — continuing",
            );
            summary.failures.push("orphans".to_string());
        }
    }

    // 6. Closed terminal sessions + transcript files.
    let cutoff = retention_cutoff_iso(CLOSED_TERMINAL_RETENTION_DAYS, now_ms);
    match cleanup_closed_sessions(db, &cutoff, transcripts).await {
        Ok((rows, files)) => {
            summary.terminal_rows = rows;
            summary.terminal_transcripts = files;
        }
        Err(e) => {
            tracing::warn!(
                target: "quilltap::maintenance",
                error = %e,
                "Terminal-session cleanup failed — continuing",
            );
            summary.failures.push("terminals".to_string());
        }
    }

    // 6b. P4.D355 (v4 `85c002822`, `scheduled-maintenance.ts:268-279`):
    // "Transcripts no row names. Chat deletes cascade `terminal_sessions` away
    // without touching the files, so step 6 can never see them. A live PTY
    // whose row went with its chat is spared."
    //
    // v4's sweep is a FALLBACK `safeQuery` (`terminal-sessions.repository.ts:
    // 203-275`): a thrown listing / row read logs ERROR `Error sweeping
    // orphaned terminal transcripts` `{collection, error}` and answers
    // `{scanned: 0, deleted: 0}` — the step never adds a failure, and v4's
    // `orphan-transcripts` push (`scheduled-maintenance.ts:268-279`) is
    // unreachable. Ported as that fallback.
    let is_live = |id: &str| probe.is_some_and(|p| p.is_live(id));
    match sweep_orphaned_transcripts(
        db,
        transcripts,
        &is_live,
        queue_service::ORPHAN_TERMINAL_TRANSCRIPT_GRACE_MS,
        now_ms,
    )
    .await
    {
        Ok((scanned, deleted)) => {
            summary.orphaned_transcripts_swept = OrphanedTranscriptSweep { scanned, deleted }
        }
        Err(e) => {
            // v4 `safeQuery`'s fallback arm: `{collection, ...context, error}`.
            tracing::error!(
                target: "quilltap::db",
                collection = "terminal_sessions",
                error = %e,
                "Error sweeping orphaned terminal transcripts"
            );
            summary.orphaned_transcripts_swept = OrphanedTranscriptSweep {
                scanned: 0,
                deleted: 0,
            };
        }
    }

    // 7. Orphaned thumbnails — v4 bug 43 (`7bcd8515`). v4's sweep propagates a
    // LISTING or existence-check failure (no try/catch around
    // `listRaw`/`findByIds` despite its doc-comment) and THIS catch is what
    // pushes `orphan-thumbnails`; per-entry delete failures are swallowed
    // inside the sweep.
    match crate::services::file_storage::sweep_orphaned_thumbnails(db, backend) {
        Ok(r) => summary.orphaned_thumbnails_swept = r,
        Err(e) => {
            // The swallow-site rule: log the real error where it happened.
            // v4 `scheduled-maintenance.ts:251` (runSweep's warnMessage) — see
            // the store-children arm above.
            tracing::warn!(
                target: "quilltap::maintenance",
                error = %e,
                "Orphaned-thumbnail sweep failed — continuing",
            );
            summary.failures.push("orphan-thumbnails".to_string());
        }
    }

    // === P4.D296: fold catch-ups (steps 8–9) ===
    // v4 `scheduled-maintenance.ts:267-291`. Steps 8 and 9 select from the
    // same chat list and cursor table; read each once for both. ONE `try`
    // spans BOTH reads: a cursor-read failure after a good chat read leaves
    // the chats SET and the cursors unset, and each step then reads only its
    // own cursors. `now_ms` is the pass's one anchor (v4 passes no `now`, so
    // its sweep reads the wall clock — the same instant).
    let mut catchup_chats: Option<Vec<serde_json::Value>> = None;
    let mut catchup_cursors: Option<
        Vec<(
            String,
            crate::db::memory_extraction_cursors::ExtractionCursorMap,
        )>,
    > = None;
    let shared = (|| -> Result<(), crate::db::DbError> {
        // v4 `repos.chats.findAll()` falls back to `[]` (`_findAll`'s
        // `safeQuery`), so only the cursor read can reach the WARN below.
        catchup_chats = Some(crate::db::fallback::find_all_or_empty("chats", || {
            db.read_main(crate::db::chats_read::find_all)
        }));
        catchup_cursors = Some(db.read_main(|c| {
            crate::db::memory_extraction_cursors::MemoryExtractionCursorsRepository::new(c)
                .find_all_by_chat()
        })?);
        Ok(())
    })();
    if let Err(e) = shared {
        tracing::warn!(
            target: "quilltap::maintenance",
            error = %crate::db::fallback::error_text(&e),
            "Could not list chats for the fold catch-ups; each will read its own",
        );
    }
    for (pass, failure, warn) in [
        (
            "other-fold",
            "fold-other-catchup",
            "Fold-grain OTHER catch-up failed — continuing",
        ),
        (
            "episode-fold",
            "fold-episode-catchup",
            "Fold episode catch-up failed — continuing",
        ),
    ] {
        // 8. Fold-grain OTHER catch-up: a MEMORY_EXTRACTION job for each chat
        //    idle > 2 h whose tail is past its other-fold cursor.
        // 9. Fold episode catch-up: one for each chat idle > 2 h whose
        //    episode-fold cursor is behind its fold boundary, so a capped
        //    backlog drains without waiting for more folds (bug 232).
        match crate::services::extraction::fold_catchup::enqueue_fold_catchups(
            db,
            pass,
            Some(now_ms as f64),
            None,
            catchup_chats.as_deref(),
            catchup_cursors.as_deref(),
        )
        .await
        {
            Ok(r) if pass == "other-fold" => summary.fold_other_catchup = r,
            Ok(r) => summary.fold_episode_catchup = r,
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::maintenance",
                    error = %crate::db::fallback::error_text(&e),
                    "{}",
                    warn,
                );
                summary.failures.push(failure.to_string());
            }
        }
    }
    // === end P4.D296 ===

    // === P4.D292: step 10, the Built-in (TF-IDF) vocabulary refit check ===
    // v4 `scheduled-maintenance.ts:293-297`: "AUTO memories never ask for a
    // refit themselves, so the pass checks whether the corpus has outgrown the
    // vocabulary (no-op on neural defaults)." v4 wraps it in `runSweep` with
    // the failure key `builtin-refit` and the WARN `TF-IDF refit check failed —
    // continuing`, unreachable there and absent here: `check_builtin_refit`
    // never fails (it answers its all-null struct after its own WARN).
    let refit = crate::services::builtin_refit_policy::check_builtin_refit(db).await;
    summary.builtin_refit = BuiltinRefitSweep {
        reason: refit.reason.map(|r| r.as_str()),
        enqueued: refit.enqueued,
    };
    // === end P4.D292 ===

    // Record the pass regardless of per-sweep failures ("last attempted pass").
    // A record failure is warned-and-swallowed in v4; same here.
    let stamp = iso_from_unix_ms(now_ms);
    if let Err(e) = db
        .write(move |writers| {
            instance_settings::set_last_maintenance_sweep_at(writers.main().connection(), &stamp)
        })
        .await
    {
        // v4 `scheduled-maintenance.ts:258`. Still swallowed — but a failure to
        // record the stamp makes the NEXT startup tick run again, and that is
        // only diagnosable if it is said out loud.
        tracing::warn!(
            target: "quilltap::maintenance",
            error = %e,
            "Failed to record lastMaintenanceSweepAt",
        );
    }

    // v4 `scheduled-maintenance.ts:270` — `moduleLogger.info('Scheduled
    // maintenance pass complete', summary)`, the summary object spread: v4's
    // camelCase keys in v4's order, each object / array through the `…Json`
    // file-layer convention (P4.D270 R-F: the line gained `foldOtherCatchup`,
    // so it moved whole — the snake_case flattened keys and the omitted
    // `orphanedThumbnailsSwept` were pre-existing). Dogfood finding #110: this
    // pass DELETES the operator's generated images (the stale-chat asset
    // collapse), and before this line nothing anywhere said so.
    let json = |v: serde_json::Value| v.to_string();
    tracing::info!(
        target: "quilltap::maintenance",
        jobsJson = %json(serde_json::json!({
            "completed": summary.jobs_completed,
            "dead": summary.jobs_dead,
        })),
        assetsJson = %json(serde_json::json!({
            "staleChats": summary.assets_stale_chats,
            "chatsCollapsed": summary.assets_chats_collapsed,
            "filesDeleted": summary.assets_files_deleted,
        })),
        cachesJson = %json(serde_json::json!({
            "staleChats": summary.caches_stale_chats,
            "chatsCollapsed": summary.caches_chats_collapsed,
            "chatRowsCleared": summary.caches_chat_rows_cleared,
            "messageRowsCleared": summary.caches_message_rows_cleared,
        })),
        orphanedFilesSwept = summary.orphaned_files_swept,
        orphanedStoreChildrenSweptJson = %json(serde_json::json!({
            "links": summary.orphaned_store_children_swept.links,
            "folders": summary.orphaned_store_children_swept.folders,
            "documents": summary.orphaned_store_children_swept.documents,
        })),
        terminalsJson = %json(serde_json::json!({
            "rows": summary.terminal_rows,
            "transcripts": summary.terminal_transcripts,
        })),
        // P4.D355: v4's slot, between `terminals` and `orphanedThumbnailsSwept`.
        orphanedTranscriptsSweptJson = %json(serde_json::json!({
            "scanned": summary.orphaned_transcripts_swept.scanned,
            "deleted": summary.orphaned_transcripts_swept.deleted,
        })),
        orphanedThumbnailsSweptJson = %json(serde_json::json!({
            "scanned": summary.orphaned_thumbnails_swept.scanned,
            "deleted": summary.orphaned_thumbnails_swept.deleted,
            "unparseable": summary.orphaned_thumbnails_swept.unparseable,
        })),
        foldOtherCatchupJson = %json(serde_json::json!({
            "enqueued": summary.fold_other_catchup.enqueued,
            "deferred": summary.fold_other_catchup.deferred,
        })),
        foldEpisodeCatchupJson = %json(serde_json::json!({
            "enqueued": summary.fold_episode_catchup.enqueued,
            "deferred": summary.fold_episode_catchup.deferred,
        })),
        builtinRefitJson = %json(summary.builtin_refit.to_json()), // P4.D292
        failuresJson = %json(serde_json::json!(summary.failures)),
        "Scheduled maintenance pass complete",
    );

    summary
}

/// Re-export so the host's sweep loops read one module for the terminal window
/// (the other windows are already on `queue_service`).
pub use queue_service::DAY_MS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::Db;
    // The three tests here reach the same callsites; a thread-scoped capture
    // in one loses lines when a sibling reaches a callsite first with no
    // subscriber (tracing caches its `Interest` as "never" — memory note
    // `capture-rig-writer-thread`, second trap). Every test installs the
    // process-global capture, and the capturing one arms its own thread.
    use crate::test_support::global_capture;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A recording store that "removes" every transcript except a named one
    /// (simulating an already-gone file → not counted).
    struct RecordingStore {
        missing_id: String,
        calls: AtomicUsize,
    }
    impl TranscriptStore for RecordingStore {
        fn unlink_transcript(&self, session_id: &str, _path: Option<&str>) -> Result<bool, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(session_id != self.missing_id)
        }
    }

    // The tracing capture layer is `crate::test_support` (P4.77 — formerly the
    // `cost_events.rs` idiom copy-pasted here; thread-scoped via
    // `set_default`). Every field this module logs is either bare numeric
    // (Debug/Display render identically) or carried with `%`/`?`, whose
    // wrapper's own `Debug` forwards to the inner value's own formatting —
    // so the shared visitor's explicit per-type handling changes no byte any
    // assertion here checks.

    /// A storage backend that lists nothing — the thumbnail sweep needs one and
    /// this test is not about it.
    struct NoBackend;
    impl crate::services::file_storage::StorageBackend for NoBackend {
        fn upload(&self, _k: &str, _c: &[u8], _t: &str) -> Result<(), String> {
            Err("no backend".into())
        }
        fn download(&self, _k: &str) -> Result<Vec<u8>, String> {
            Err("no backend".into())
        }
        fn delete(&self, _k: &str) -> Result<(), String> {
            Err("no backend".into())
        }
        fn exists(&self, _k: &str) -> Result<bool, String> {
            Ok(false)
        }
    }

    struct NoTranscripts;
    impl TranscriptStore for NoTranscripts {
        fn unlink_transcript(&self, _id: &str, _p: Option<&str>) -> Result<bool, String> {
            Ok(false)
        }
    }

    /// Dogfood finding #110 — v4 `scheduled-maintenance.ts:182/:258/:263` plus
    /// `runSweep`'s per-sweep `warnMessage`. The pass had been running in total
    /// silence, and it is the pass that DELETES the operator's generated images.
    /// The DB here carries only `terminal_sessions`, so every other sweep AND
    /// the `lastMaintenanceSweepAt` write fail — which is how one test reaches
    /// most of v4's sentences at once without inventing a single error.
    #[test]
    fn the_pass_announces_itself_its_failures_and_its_summary() {
        let (db, _scratch) = test_db();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        global_capture::install();
        let (summary, lines) = global_capture::capture(|| {
            rt.block_on(run_scheduled_maintenance(
                &db,
                1_900_000_000_000,
                &NoTranscripts,
                &NoBackend,
                None,
            ))
        });

        let has = |needle: &str| lines.iter().any(|l| l.contains(needle));

        // The bookends.
        assert!(has("Starting scheduled maintenance pass"), "{lines:?}");
        assert!(has("Scheduled maintenance pass complete"), "{lines:?}");

        // v4's runSweep warns, for the sweeps this DB makes fail.
        assert!(has("Job cleanup sweep failed — continuing"), "{lines:?}");
        assert!(
            has("Stale-chat asset collapse failed — continuing"),
            "{lines:?}"
        );
        assert!(
            has("Stale-chat cache collapse failed — continuing"),
            "{lines:?}"
        );

        // The stamp write has no `instance_settings` table to write to.
        assert!(has("Failed to record lastMaintenanceSweepAt"), "{lines:?}");

        // Every warn names its error and rides the maintenance target.
        for l in lines.iter().filter(|l| l.contains("— continuing")) {
            assert!(l.starts_with("WARN quilltap::maintenance"), "{l:?}");
            assert!(l.contains("error="), "{l:?}");
        }

        // The summary line carries the numbers, not just a sentence.
        let done = lines
            .iter()
            .find(|l| l.contains("Scheduled maintenance pass complete"))
            .unwrap();
        assert!(done.starts_with("INFO quilltap::maintenance"), "{done:?}");
        // v4's spread keys (P4.D270 R-F — the snake_case pin moved with the line).
        assert!(
            done.contains(r#"assetsJson={"staleChats":0,"chatsCollapsed":0,"filesDeleted":0}"#),
            "{done:?}"
        );
        assert!(done.contains("foldOtherCatchupJson="), "{done:?}");
        // === P4.D355: step 6b's key in v4's slot ===
        let terminals_at = done.find("terminalsJson=").expect("terminalsJson");
        let orphans_at = done
            .find(r#"orphanedTranscriptsSweptJson={"scanned":0,"deleted":0}"#)
            .expect("orphanedTranscriptsSweptJson");
        let thumbs_at = done
            .find("orphanedThumbnailsSweptJson=")
            .expect("orphanedThumbnailsSweptJson");
        assert!(
            terminals_at < orphans_at && orphans_at < thumbs_at,
            "{done:?}"
        );
        // No transcript directory → the DEBUG, and no failure word.
        assert!(
            has("No terminal transcript directory; nothing to sweep"),
            "{lines:?}"
        );
        assert!(!summary.failures.contains(&"orphan-transcripts".to_string()));
        // === end P4.D355 ===
        assert!(done.contains("failuresJson="), "{done:?}");
        assert!(!done.contains("assets_files_deleted"), "{done:?}");
        // === P4.D292: step 10's key, after the fold catch-ups, before failures ===
        // No `embedding_profiles` table here: the instance-default read's base
        // fallback answers none, so the check is the all-false no-op.
        let refit_at = done
            .find(r#"builtinRefitJson={"reason":null,"enqueued":false}"#)
            .expect("builtinRefitJson");
        assert!(
            done.find("foldOtherCatchupJson=").unwrap() < refit_at,
            "{done:?}"
        );
        assert!(refit_at < done.find("failuresJson=").unwrap(), "{done:?}");
        assert!(!has("TF-IDF refit check failed"), "{lines:?}");
        // === end P4.D292 ===

        // And the failures the summary reports are the ones that were warned.
        assert!(summary.failures.contains(&"jobs".to_string()));
        assert!(summary.failures.contains(&"assets".to_string()));
    }

    /// The scratch dir rides out with the handle — bind it for the test's life.
    fn test_db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::Builder::new()
            .prefix("qt-sched-maint-")
            .tempdir()
            .expect("tempdir");
        let db = Db::open_main(
            dir.path().join("main.db"),
            "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
        )
        .unwrap();
        db.write_blocking(|writers| {
            writers.main().connection().execute_batch(
                "CREATE TABLE terminal_sessions (
                    id TEXT PRIMARY KEY, chatId TEXT NOT NULL, label TEXT,
                    shell TEXT NOT NULL, cwd TEXT NOT NULL, startedAt TEXT NOT NULL,
                    exitedAt TEXT, exitCode REAL, transcriptPath TEXT,
                    createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL,
                    cols REAL, rows REAL);",
            )?;
            Ok(())
        })
        .unwrap();
        (db, dir)
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
        global_capture::install();
        let (db, _scratch) = test_db();
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
        global_capture::install();
        let (db, _scratch) = test_db();
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
        let backend = crate::services::file_storage::NotConfiguredStorageBackend;
        let summary = rt.block_on(run_scheduled_maintenance(
            &db,
            now_ms,
            &NoTranscriptStore,
            &backend,
            None,
        ));

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

    // === P4.D355 — v4 `terminal-transcript-orphan-sweep.test.ts` (`85c002822`)
    // by name, over a `TranscriptStore` double ===

    struct DirStore {
        listing: Result<Option<Vec<TranscriptEntry>>, String>,
        /// Names whose unlink answers ENOENT (`Ok(false)`) / another error.
        vanished: Vec<String>,
        failing: Vec<String>,
        unlinked: std::sync::Mutex<Vec<String>>,
    }
    impl TranscriptStore for DirStore {
        fn unlink_transcript(&self, _id: &str, path: Option<&str>) -> Result<bool, String> {
            let path = path.unwrap().to_string();
            if self.vanished.iter().any(|v| path.ends_with(v.as_str())) {
                return Ok(false);
            }
            if self.failing.iter().any(|v| path.ends_with(v.as_str())) {
                return Err("EACCES: permission denied".to_string());
            }
            self.unlinked.lock().unwrap().push(path);
            Ok(true)
        }
        fn list_transcripts(&self) -> Result<Option<Vec<TranscriptEntry>>, String> {
            self.listing.clone()
        }
        fn transcripts_dir_display(&self) -> String {
            "/logs/terminals".to_string()
        }
    }

    const NOW: i64 = 1_900_000_000_000;

    fn entry(name: &str, age_ms: i64, is_file: bool) -> TranscriptEntry {
        TranscriptEntry {
            name: name.to_string(),
            path: std::path::PathBuf::from(format!("/logs/terminals/{name}")),
            is_file,
            mtime_ms: NOW - age_ms,
        }
    }

    fn store(listing: Result<Option<Vec<TranscriptEntry>>, String>) -> DirStore {
        DirStore {
            listing,
            vanished: Vec::new(),
            failing: Vec::new(),
            unlinked: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn sweep(
        db: &Db,
        store: &DirStore,
        live: &[&str],
    ) -> (Result<(usize, usize), String>, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        global_capture::install();
        let live: Vec<String> = live.iter().map(|s| s.to_string()).collect();
        let is_live = move |id: &str| live.iter().any(|l| l == id);
        global_capture::capture(|| {
            rt.block_on(sweep_orphaned_transcripts(
                db,
                store,
                &is_live,
                queue_service::ORPHAN_TERMINAL_TRANSCRIPT_GRACE_MS,
                NOW,
            ))
        })
    }

    const HOUR: i64 = 60 * 60 * 1000;

    #[test]
    fn removes_only_old_unnamed_non_live_log_files() {
        let (db, _scratch) = test_db();
        seed_session(&db, "kept", None, None);
        seed_session(
            &db,
            "named-by-path",
            None,
            Some("/logs/terminals/custom-name.log"),
        );
        let s = store(Ok(Some(vec![
            entry("kept.log", 48 * HOUR, true),
            entry("custom-name.log", 48 * HOUR, true),
            entry("deleted-chat-session.log", 48 * HOUR, true),
            entry("live-no-row.log", 48 * HOUR, true),
            entry("spawning.log", 5 * 60 * 1000, true),
            entry("init-deleted-chat-session.bashrc", 48 * HOUR, true),
        ])));
        let (out, lines) = sweep(&db, &s, &["live-no-row"]);
        assert_eq!(out, Ok((5, 1)));
        assert_eq!(
            *s.unlinked.lock().unwrap(),
            ["/logs/terminals/deleted-chat-session.log"]
        );
        let mine: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("quilltap::maintenance"))
            .collect();
        assert_eq!(
            mine,
            [
                "DEBUG quilltap::maintenance Removed orphaned terminal transcript sessionId=deleted-chat-session \
                 filePath=/logs/terminals/deleted-chat-session.log",
                "DEBUG quilltap::maintenance Sparing transcript of a live PTY with no row sessionId=live-no-row",
                "DEBUG quilltap::maintenance Sparing recently written orphan transcript sessionId=spawning",
                "INFO quilltap::maintenance Swept orphaned terminal transcripts scanned=5 deleted=1",
            ],
            "{lines:?}"
        );
    }

    #[test]
    fn answers_zeros_without_reading_rows_when_the_directory_is_missing() {
        // No `terminal_sessions` table at all: a rows read would FAIL, so a
        // `(0, 0)` proves the rows were never read (v4: no `findAll` call).
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_main(
            dir.path().join("main.db"),
            "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
        )
        .unwrap();
        let (out, lines) = sweep(&db, &store(Ok(None)), &[]);
        assert_eq!(out, Ok((0, 0)));
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("quilltap::maintenance"))
                .collect::<Vec<_>>(),
            [
                "DEBUG quilltap::maintenance No terminal transcript directory; nothing to sweep \
              transcriptsDir=/logs/terminals"
            ]
        );
    }

    #[test]
    fn a_file_vanishing_between_listing_and_unlink_is_silent_and_uncounted() {
        let (db, _scratch) = test_db();
        let mut s = store(Ok(Some(vec![
            entry("gone.log", 48 * HOUR, true),
            entry("locked.log", 48 * HOUR, true),
        ])));
        s.vanished = vec!["gone.log".into()];
        s.failing = vec!["locked.log".into()];
        let (out, lines) = sweep(&db, &s, &[]);
        assert_eq!(out, Ok((2, 0)));
        assert!(
            !lines.iter().any(|l| l.contains("sessionId=gone ")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l
                == "WARN quilltap::maintenance Failed to remove orphaned terminal transcript \
                    sessionId=locked filePath=/logs/terminals/locked.log error=EACCES: permission denied"),
            "{lines:?}"
        );
    }

    /// Tier 3 item 27 — v4's `sweepOrphanedTranscripts` is a FALLBACK
    /// `safeQuery` (`terminal-sessions.repository.ts:203-275`): a thrown listing
    /// logs ERROR `Error sweeping orphaned terminal transcripts` `{collection,
    /// error}` and answers `{scanned: 0, deleted: 0}` — so the step adds no
    /// `orphan-transcripts` failure and v5 ports exactly that.
    #[test]
    fn a_failed_listing_logs_v4s_fallback_line_and_sweeps_nothing() {
        let (db, _scratch) = test_db();
        let s = store(Err("EACCES".into()));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        global_capture::install();
        let (summary, lines) = global_capture::capture(|| {
            rt.block_on(run_scheduled_maintenance(&db, NOW, &s, &NoBackend, None))
        });
        assert!(
            !summary.failures.contains(&"orphan-transcripts".to_string()),
            "{:?}",
            summary.failures
        );
        assert_eq!(summary.orphaned_transcripts_swept.scanned, 0);
        assert_eq!(summary.orphaned_transcripts_swept.deleted, 0);
        assert!(
            lines.iter().any(|l| l.starts_with("ERROR ")
                && l.contains("Error sweeping orphaned terminal transcripts")
                && l.contains("collection=terminal_sessions")
                && l.contains("error=EACCES")),
            "{lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Orphaned-transcript sweep failed")),
            "{lines:?}"
        );
    }
}
