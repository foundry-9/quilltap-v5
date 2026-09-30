//! The production [`SaveImageSideEffects`] (P4.120) — what v4 does after a photo
//! write lands: `invalidateMountPoint(mountPointId)` then
//! `enqueueEmbeddingJobsForMountPoint(mountPointId)`.
//!
//! Until this order v5 had only [`NoSideEffects`]: every photo path — the
//! save-to-album doors, the `describe_image` tool's vision tier, the upload's
//! auto-describe — passed it literally, so a described or kept image's fresh
//! chunks never got an `EMBEDDING_GENERATE` job until the next full mount scan.
//!
//! - **`invalidate_mount_point`** stays a no-op, and it is a *measured* one: v5
//!   has no in-memory mount-chunk cache (every read of a chunk goes to SQLite,
//!   so the write that landed the rows IS the invalidation) — the same recorded
//!   no-counterpart `db::doc_mount_file_links` and `photos::avatar_rolls_service`
//!   name for the same v4 call.
//! - **`enqueue_embedding_jobs`** runs [`enqueue_embedding_jobs_for_mount_point`]
//!   on the single writer. The trait is synchronous and is called from INSIDE a
//!   writer closure by `save_image_to_album`, where a nested `Db::write` would
//!   deadlock — so the enqueue is handed to the armed background spawner
//!   ([`crate::background`]) and queues behind the closure that called it. That
//!   is also v4's shape for `save-image-to-album.ts:319` (fire-and-forget,
//!   `.catch` warn). Unarmed → a DEBUG line, no enqueue (the differential's
//!   `NoSideEffects` posture, which the harness relies on).

use crate::background::spawn_background;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::photos::save_image_to_album::SaveImageSideEffects;
use crate::services::mount_index::embedding_scheduler::enqueue_embedding_jobs_for_mount_point;

/// [`SaveImageSideEffects`] over a real database — see the module header.
#[derive(Clone)]
pub struct MountEmbeddingSideEffects {
    db: Db,
}

impl MountEmbeddingSideEffects {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl SaveImageSideEffects for MountEmbeddingSideEffects {
    // v4 `invalidateMountPoint` — no counterpart (module header). Deliberately
    // the trait default's no-op, spelled out so the absence is a decision.
    fn invalidate_mount_point(&self, _mount_point_id: &str) {}

    fn enqueue_embedding_jobs(&self, mount_point_id: &str) {
        let db = self.db.clone();
        let mount_point_id = mount_point_id.to_string();
        let mp = mount_point_id.clone();
        let spawned = spawn_background(Box::pin(async move {
            let mp_for_write = mp.clone();
            let result = db
                .write(move |ws| {
                    let main = ws.main().connection();
                    let mount = ws
                        .mount_index()
                        .ok_or_else(|| {
                            DbError::Internal(
                                "embedding enqueue requires the mount-index database".into(),
                            )
                        })?
                        .connection();
                    enqueue_embedding_jobs_for_mount_point(main, mount, &mp_for_write)
                })
                .await;
            if let Err(error) = result {
                tracing::warn!(
                    mount_point_id = %mp,
                    error = %error,
                    "failed to enqueue embedding jobs for mount"
                );
            }
        }));
        if !spawned {
            tracing::debug!(
                mount_point_id = %mount_point_id,
                "no background spawner armed; skipping the post-photo embedding enqueue"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::chat_media::ImageDescribeDriver;
    use crate::background::{
        arm_background_spawner_for_current_thread, disarm_background_spawner_for_current_thread,
    };
    use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkFilesystemFileInput};
    use crate::db::files::{CreateOptions, FileCreate, FilesRepository};
    use crate::db::runtime::DbPaths;
    use crate::photos::auto_describe_attachment::auto_describe_chat_image_attachment;
    use crate::photos::save_image_to_album::{FileBytesStore, IngestImageRequest};
    use crate::services::file_fallback::{FallbackFile, FallbackResult, FallbackType};
    use crate::test_support::captured_with;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    type Task = Pin<Box<dyn Future<Output = ()> + Send>>;

    const PEPPER: &str = "cXVpbGx0YXAtdGVzdC1wZXBwZXItMzItYnl0ZXMhIQ==";
    const USER: &str = "11111111-1111-4111-8111-111111111111";
    const TS: &str = "2026-08-01T00:00:00.000Z";
    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Bytes;
    impl FileBytesStore for Bytes {
        fn read_image_buffer(
            &self,
            _e: &crate::db::files::FileEntry,
        ) -> Result<Option<Vec<u8>>, String> {
            Ok(Some(vec![1, 2, 3]))
        }
        fn ingest_image_buffer(
            &self,
            _r: &IngestImageRequest,
        ) -> Result<crate::db::files::FileEntry, String> {
            Err("unused".into())
        }
    }

    struct Describe;
    impl ImageDescribeDriver for Describe {
        fn describe<'a>(
            &'a self,
            _f: FallbackFile,
            _c: Option<&'a str>,
        ) -> Pin<Box<dyn Future<Output = FallbackResult> + Send + 'a>> {
            Box::pin(async {
                FallbackResult {
                    type_: FallbackType::ImageDescription,
                    text_content: None,
                    image_description: Some("a copper kettle on a windowsill".into()),
                    processing_metadata: None,
                    error: None,
                }
            })
        }
    }

    /// A provisioned instance with one described-able image: a `files` row and a
    /// BLANK link (no extractedText) sharing its sha, in mount `mp-1`.
    async fn seeded_db(dir: &tempfile::TempDir) -> Db {
        let dpath = dir.path().to_path_buf();
        let db = tokio::task::spawn_blocking(move || {
            crate::services::provisioning::provision_fresh_instance(&dpath, PEPPER).unwrap();
            Db::open(
                DbPaths {
                    main: dpath.join("quilltap.db"),
                    mount_index: Some(dpath.join("quilltap-mount-index.db")),
                    llm_logs: None,
                },
                PEPPER,
            )
            .unwrap()
        })
        .await
        .unwrap();
        db.write(|ws| {
            FilesRepository::new(ws.main().connection()).create(
                &FileCreate {
                    user_id: USER.to_string(),
                    sha256: SHA.to_string(),
                    original_filename: "subject.webp".to_string(),
                    mime_type: "image/webp".to_string(),
                    size: 10.0,
                    width: None,
                    height: None,
                    is_plain_text: None,
                    linked_to: vec![],
                    source: "UPLOADED".to_string(),
                    category: "IMAGE".to_string(),
                    generation_prompt: None,
                    generation_model: None,
                    generation_revised_prompt: None,
                    generation_key: None,
                    description: None,
                    tags: vec![],
                    project_id: None,
                    folder_path: None,
                    storage_key: None,
                    file_status: "ok".to_string(),
                },
                &CreateOptions {
                    id: "f-img".to_string(),
                    created_at: TS.to_string(),
                    updated_at: TS.to_string(),
                },
            )?;
            let mount = ws.mount_index().unwrap().connection();
            mount.execute(
                "INSERT INTO doc_mount_points \
                   (id, name, basePath, mountType, storeType, createdAt, updatedAt) \
                 VALUES ('mp-1', 'Uploads', '', 'quilltap_native', 'documents', ?1, ?1)",
                rusqlite::params![TS],
            )?;
            DocMountFileLinksRepository::new(mount).link_filesystem_file(
                &LinkFilesystemFileInput {
                    mount_point_id: "mp-1".to_string(),
                    relative_path: "chat-uploads/subject.webp".to_string(),
                    file_name: "subject.webp".to_string(),
                    file_type: "blob".to_string(),
                    sha256: SHA.to_string(),
                    file_size_bytes: 10.0,
                    last_modified: TS.to_string(),
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .await
        .unwrap();
        db
    }

    fn pending_embedding_jobs(db: &Db) -> i64 {
        db.read_main(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM background_jobs \
                 WHERE type = 'EMBEDDING_GENERATE' AND status = 'PENDING'",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap()
    }

    /// Armed: the describe's `enqueue_embedding_jobs` becomes a PENDING
    /// `EMBEDDING_GENERATE` job for a chunk of the touched mount — the effect v4
    /// has and every photo path lacked while it passed `NoSideEffects`. The job
    /// is queued BEHIND the caller (a spawned writer task), never inside it.
    #[tokio::test]
    async fn an_armed_enqueue_lands_a_pending_embedding_job() {
        let dir = tempfile::tempdir().unwrap();
        let db = seeded_db(&dir).await;
        let queue: Arc<Mutex<Vec<Task>>> = Arc::new(Mutex::new(Vec::new()));
        let q = queue.clone();
        arm_background_spawner_for_current_thread(Arc::new(move |fut| {
            q.lock().unwrap().push(fut);
        }));

        let fx = MountEmbeddingSideEffects::new(db.clone());
        let out =
            auto_describe_chat_image_attachment(&db, &Bytes, &fx, Some(&Describe), "f-img", None)
                .await
                .unwrap();
        assert_eq!(out.links_updated, 1, "the blank link took the description");

        // The hook only HANDED the spawner a task; nothing has run yet.
        assert_eq!(queue.lock().unwrap().len(), 1);
        assert_eq!(pending_embedding_jobs(&db), 0, "the enqueue is deferred");

        let tasks: Vec<Task> = std::mem::take(&mut *queue.lock().unwrap());
        for t in tasks {
            t.await;
        }
        disarm_background_spawner_for_current_thread();
        assert!(
            pending_embedding_jobs(&db) >= 1,
            "no EMBEDDING_GENERATE job for the described mount's chunks"
        );
    }

    /// Unarmed: a DEBUG line and no enqueue — the harness/CLI posture.
    #[tokio::test]
    async fn an_unarmed_enqueue_is_a_debug_line_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let db = seeded_db(&dir).await;
        disarm_background_spawner_for_current_thread();
        let fx = MountEmbeddingSideEffects::new(db.clone());
        let (_, logs) = captured_with(|| {
            fx.invalidate_mount_point("mp-1");
            fx.enqueue_embedding_jobs("mp-1");
        });
        assert_eq!(pending_embedding_jobs(&db), 0);
        assert_eq!(logs.len(), 1, "{logs:?}");
        assert!(logs[0].starts_with("DEBUG "), "{}", logs[0]);
        assert!(
            logs[0].contains("no background spawner armed"),
            "{}",
            logs[0]
        );
    }
}
