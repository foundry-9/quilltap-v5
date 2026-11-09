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

// === P4.D380 === (v4 `a3d53eae1` — `afterDocumentWrite`'s chain, `core/write.ts:38-61`)

/// The ONE chain after a document write (round ruling R-2): `reindexSingleFile`
/// → `reindexLinkGroupSiblings` → `enqueueEmbeddingJobsForMountPoint` +
/// `refreshStats`. The two re-index steps are best-effort themselves (they log
/// and never fail, as v4's do); the FIRST failure of the last two is the
/// chain's error. v4 runs those two concurrently under `Promise.all` and
/// rejects with the first to settle; here they run in order, BOTH always run,
/// and the embedding error wins when both fail (which settles first in v4 is
/// unmeasurable from the corpus — recorded).
pub fn refresh_after_document_write(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    absolute_path: &str,
    extractor: &dyn DocumentTextExtractor,
) -> Result<(), String> {
    reindex_single_file(
        mount,
        mount_point_id,
        relative_path,
        absolute_path,
        extractor,
    );
    super::link_groups::reindex_link_group_siblings(
        mount,
        mount_point_id,
        relative_path,
        extractor,
    );
    let embed = enqueue_embedding_jobs_for_mount_point(main, mount, mount_point_id)
        .map(|_| ())
        .map_err(|e| e.to_string());
    let stats = DocMountPointsRepository::new(mount)
        .refresh_stats(mount_point_id)
        .map_err(|e| e.to_string());
    embed.and(stats)
}

/// [`refresh_after_document_write`] with v4's catch: ONE WARN `Background
/// re-index, embedding, or stats refresh failed after a document write`
/// `{mountPointId, path, error}`; never fails the write that triggered it.
pub fn refresh_after_document_write_logged(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    absolute_path: &str,
    extractor: &dyn DocumentTextExtractor,
) {
    if let Err(error) = refresh_after_document_write(
        main,
        mount,
        mount_point_id,
        relative_path,
        absolute_path,
        extractor,
    ) {
        tracing::warn!(
            target: "quilltap::documents",
            mountPointId = %mount_point_id,
            path = %relative_path,
            error = %error,
            "Background re-index, embedding, or stats refresh failed after a document write"
        );
    }
}
// === end P4.D380 ===

/// Refresh with the mount looked up (resolves the fs absolute path for
/// filesystem mounts; `path: None` = the whole-mount form — no single-file
/// reindex, just the embedding enqueue + stats).
pub fn run_refresh(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
    path: Option<&str>,
) {
    let extractor = super::converters::default_text_extractor();
    let mp = match DocMountPointsRepository::new(mount).find_service_info_by_id(mount_point_id) {
        Ok(Some(mp)) => mp,
        Ok(None) => {
            eprintln!("Refresh skipped: mount point {mount_point_id} not found");
            return;
        }
        Err(e) => {
            eprintln!("Refresh skipped: mount lookup failed for {mount_point_id}: {e}");
            return;
        }
    };
    match path {
        Some(rel) => {
            let abs = if mp.is_filesystem_mount() {
                format!("{}/{}", mp.base_path.as_deref().unwrap_or(""), rel)
            } else {
                String::new()
            };
            // P4.D380: the Document Mode cadence of the ONE chain (R-2) — the
            // link-group siblings step and v4's WARN, where `refresh_now`'s
            // three `eprintln!`s were.
            refresh_after_document_write_logged(
                main,
                mount,
                mount_point_id,
                rel,
                &abs,
                extractor.as_ref(),
            );
        }
        None => {
            if let Err(e) = super::embedding_scheduler::enqueue_embedding_jobs_for_mount_point(
                main,
                mount,
                mount_point_id,
            ) {
                eprintln!("Whole-mount embedding enqueue failed ({mount_point_id}): {e}");
            }
            if let Err(e) = DocMountPointsRepository::new(mount).refresh_stats(mount_point_id) {
                eprintln!("Whole-mount stats refresh failed ({mount_point_id}): {e}");
            }
        }
    }
}

/// The PRODUCTION [`crate::documents::MountRefreshScheduler`] (P4.6y unit J —
/// the P4.6w seam wire): fire-and-forget with the writer channel as the
/// scheduler. The seam's call sites run ON the writer thread (inside the write
/// closure that performed the document save), so the impl spawns a plain OS
/// thread that enqueues its OWN write job — it runs after the triggering write
/// commits, and never re-enters the busy writer. Tests that need determinism
/// call [`run_refresh`] synchronously instead.
pub struct DbMountRefreshScheduler {
    db: crate::db::runtime::Db,
}

impl DbMountRefreshScheduler {
    pub fn new(db: crate::db::runtime::Db) -> Self {
        Self { db }
    }
}

impl crate::documents::MountRefreshScheduler for DbMountRefreshScheduler {
    fn schedule_refresh(&self, mount_point_id: &str, path: Option<&str>) {
        let db = self.db.clone();
        let id = mount_point_id.to_string();
        let p = path.map(|s| s.to_string());
        std::thread::spawn(move || {
            let idc = id.clone();
            let res = db.write_blocking(move |ws| {
                let Some(mount_writer) = ws.mount_index() else {
                    eprintln!("Refresh skipped: no mount-index database (mount {idc})");
                    return Ok(());
                };
                let mount = mount_writer.connection();
                let main = ws.main().connection();
                run_refresh(main, mount, &idc, p.as_deref());
                Ok(())
            });
            if let Err(e) = res {
                eprintln!(
                    "Background document-store refresh failed to reach the writer                      (mount {id}): {e}"
                );
            }
        });
    }
}
