//! The document-store refresh chain — v4 `scheduleDocumentStoreRefresh`
//! (`operator-doc-actions.ts:187`), which is byte-for-byte the same chain
//! `storeMountFile` fire-and-forgets after a native-text/extracted write:
//! `reindexSingleFile` → `enqueueEmbeddingJobsForMountPoint` + `refreshStats`.
//!
//! v4 runs it fire-and-forget for Node responsiveness. Under v5's
//! single-writer model the work serializes on the writer thread regardless, so
//! [`refresh_now`] executes synchronously inside the caller's write closure —
//! same end state, deterministic ordering (the correctness-not-workaround
//! rule); the production [`crate::documents::MountRefreshScheduler`] impl
//! (P4.6y unit J) queues it as its own write job, which is the fire-and-forget
//! shape with the writer channel as the scheduler.

use rusqlite::Connection;

use crate::db::doc_mount_points::DocMountPointsRepository;

use super::converters::DocumentTextExtractor;
use super::embedding_scheduler::enqueue_embedding_jobs_for_mount_point;
use super::reindex_file::reindex_single_file;

/// The synchronous refresh core: re-chunk one path, enqueue embeddings for the
/// whole mount, refresh the cached stats. Best-effort throughout (v4's
/// catch-warn — a failed refresh never fails the write that triggered it).
pub fn refresh_now(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    absolute_path: &str,
    extractor: &dyn DocumentTextExtractor,
) {
    // reindex_single_file is itself best-effort (logs, never errors).
    reindex_single_file(
        mount,
        mount_point_id,
        relative_path,
        absolute_path,
        extractor,
    );
    if let Err(e) = enqueue_embedding_jobs_for_mount_point(main, mount, mount_point_id) {
        eprintln!(
            "Background embedding enqueue failed after document save \
             (mount {mount_point_id}, path {relative_path}): {e}"
        );
    }
    if let Err(e) = DocMountPointsRepository::new(mount).refresh_stats(mount_point_id) {
        eprintln!(
            "Background stats refresh failed after document save \
             (mount {mount_point_id}): {e}"
        );
    }
}
