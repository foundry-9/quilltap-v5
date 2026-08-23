//! Auto-describe chat image attachments — port of v4
//! `lib/photos/auto-describe-attachment.ts` (P4.D108; the vision tier's
//! substrate for the `describe_image` tool, v4 `a14a1811` bug 92).
//!
//! When a user uploads an image into a chat, v4 runs it through the configured
//! vision profile (with the uncensored fallback wired up in
//! `generateImageDescription`) and persists the result in three places so the
//! description is later useful to:
//!
//!  - **`files.description`** — the legacy image-v2 row gains a free-form
//!    description that the UI can show on hover, in the gallery, and in
//!    exports.
//!  - **`doc_mount_file_links.description`** — every "blank" hard link to the
//!    same bytes (chat upload, project upload, etc.) gets the description as
//!    its per-mount text. Kept-image links carry rich Markdown and are left
//!    alone.
//!  - **chunks + embeddings** — the description is chunked, written into the
//!    mount-index chunks table, and queued for embedding so search ("the
//!    photo of the kettle on the windowsill") surfaces the image even though
//!    it's not a text document.
//!
//! In v4 the function is fire-and-forget from the upload path; in v5 the
//! upload-time call remains a named no-op (`services/chat_files.rs` — the
//! P4.D106 boundary), and the first live caller is the `describe_image`
//! tool's vision tier (`tools/executor.rs::run_describe_image`).
//!
//! ## Seams (mirroring the sibling `save_image_to_album` precedent)
//!
//! - **Bytes**: the injected [`FileBytesStore`] (v4
//!   `fileStorageManager.downloadFile`; a read failure is v4's warn + the
//!   `no-bytes` skip — `Ok(None)` folds there too, per the trait contract).
//! - **The vision call**: the injected [`ImageDescribeDriver`] (§C2 — the
//!   host's driver composes `file_fallback::generate_image_description` with
//!   the real codec + `logLLMCall`, so profile resolution, the refusal
//!   heuristics, and the uncensored retry all run behind it). `None` means no
//!   driver was assembled: the module behaves as v4 does when
//!   `generateImageDescription` answers a non-description result — the
//!   `describe-failed` skip.
//! - **The embedding enqueue**: the injected [`SaveImageSideEffects`] (the
//!   recorded seam the photo trio already uses; v4 calls
//!   `enqueueEmbeddingJobsForMountPoint` directly and the differential
//!   jest-mocks it to a no-op — `NoSideEffects` is the mock-identical
//!   default; production wires the host scheduler).

use std::collections::BTreeSet;

use base64::Engine as _;
use rusqlite::Connection;

use crate::api::chat_media::ImageDescribeDriver;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_files::DocMountFilesRepository;
use crate::db::files::{FileEntry, FileUpdate, FilesRepository};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::jsstr::{js_trim, utf16_len};
use crate::photos::keep_image_markdown::sha256_of_string;
use crate::photos::save_image_to_album::{
    chunk_and_insert_extracted_text, FileBytesStore, SaveImageSideEffects,
};
use crate::services::file_fallback::{FallbackFile, FallbackType};

/// v4 `AutoDescribeOutput.skipReason` — the reason a non-success result
/// returned without persisting anything. The kebab-case strings are v4's
/// exact vocabulary; `already-described` is load-bearing for the
/// `describe_image` handler's race arm (another writer won between the
/// handler's read and this call — re-read, don't report failure).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoDescribeSkipReason {
    NotFound,
    NotImage,
    NoSha,
    NoBytes,
    DescribeFailed,
    AlreadyDescribed,
}

impl AutoDescribeSkipReason {
    /// The v4 string (`'not-found' | 'not-image' | 'no-sha' | 'no-bytes' |
    /// 'describe-failed' | 'already-described'`).
    pub fn as_str(self) -> &'static str {
        match self {
            AutoDescribeSkipReason::NotFound => "not-found",
            AutoDescribeSkipReason::NotImage => "not-image",
            AutoDescribeSkipReason::NoSha => "no-sha",
            AutoDescribeSkipReason::NoBytes => "no-bytes",
            AutoDescribeSkipReason::DescribeFailed => "describe-failed",
            AutoDescribeSkipReason::AlreadyDescribed => "already-described",
        }
    }
}

/// v4 `AutoDescribeOutput`.
#[derive(Clone, Debug)]
pub struct AutoDescribeOutput {
    /// v4 `describedFileEntry` — the `files` row gained the description.
    pub described_file_entry: bool,
    /// v4 `linksUpdated` — how many blank links took the description.
    pub links_updated: i64,
    /// The trimmed description that was persisted (`None` on any skip).
    pub description: Option<String>,
    /// v4 `skipReason` — set exactly when nothing was persisted.
    pub skip_reason: Option<AutoDescribeSkipReason>,
}

impl AutoDescribeOutput {
    /// v4 `{ ...EMPTY_RESULT, skipReason }`.
    pub fn skipped(reason: AutoDescribeSkipReason) -> Self {
        AutoDescribeOutput {
            described_file_entry: false,
            links_updated: 0,
            description: None,
            skip_reason: Some(reason),
        }
    }
}

/// The precheck outcome — the four cheap arms v4 runs before touching bytes.
pub enum AutoDescribePrecheck {
    /// One of the skip arms fired (`not-found` / `not-image` / `no-sha` /
    /// `already-described`).
    Skip(AutoDescribeSkipReason),
    /// The entry is an undescribed image with a sha — proceed to bytes +
    /// vision.
    Proceed(Box<FileEntry>),
}

/// The four pre-bytes arms of v4 `autoDescribeChatImageAttachment`, in v4's
/// order: not-found → not-image → no-sha → already-described.
pub fn auto_describe_precheck(
    main: &Connection,
    file_entry_id: &str,
) -> Result<AutoDescribePrecheck, DbError> {
    let files = FilesRepository::new(main);
    let Some(entry) = files.find_by_id(file_entry_id)? else {
        return Ok(AutoDescribePrecheck::Skip(AutoDescribeSkipReason::NotFound));
    };
    if !entry.mime_type.starts_with("image/") {
        return Ok(AutoDescribePrecheck::Skip(AutoDescribeSkipReason::NotImage));
    }
    if entry.sha256.is_empty() {
        return Ok(AutoDescribePrecheck::Skip(AutoDescribeSkipReason::NoSha));
    }
    // v4: `entry.description && entry.description.trim().length > 0`.
    if entry
        .description
        .as_deref()
        .is_some_and(|d| !js_trim(d).is_empty())
    {
        return Ok(AutoDescribePrecheck::Skip(
            AutoDescribeSkipReason::AlreadyDescribed,
        ));
    }
    Ok(AutoDescribePrecheck::Proceed(Box::new(entry)))
}

/// The persist pass: write the trimmed description onto the `files` row, then
/// update every "blank" hard link to these bytes (per-mount `description` +
/// `extractedText` + the chunk pass), collecting the touched mounts for the
/// embedding enqueue. Returns `(described_file_entry, links_updated,
/// touched_mount_points)`.
///
/// v4 computes ONE `now = new Date().toISOString()` before the link loop (the
/// links' `descriptionUpdatedAt`); each link failure is warn-and-continue.
/// Kept-image links (a non-blank `extractedText`) are skipped untouched.
pub fn auto_describe_persist(
    main: &Connection,
    mount: &Connection,
    entry: &FileEntry,
    description: &str,
    now_iso: &str,
) -> Result<(bool, i64, BTreeSet<String>), DbError> {
    let files = FilesRepository::new(main);
    let described_file_entry = files.update(
        &entry.id,
        &FileUpdate {
            description: Some(description.to_string()),
            updated_at: now_iso.to_string(),
            ..Default::default()
        },
    )?;

    // Update every "blank" hard link to these bytes — chat uploads, project
    // uploads, etc. — with the description as both per-mount metadata
    // (`description`) and per-mount text (`extractedText`). Kept-image links
    // already carry rich Markdown and are left untouched.
    let dmf = DocMountFilesRepository::new(mount);
    let links_repo = DocMountFileLinksRepository::new(mount);
    let mut links_updated: i64 = 0;
    let mut touched: BTreeSet<String> = BTreeSet::new();
    if let Some(file_id) = dmf.find_by_sha256(&entry.sha256)? {
        let links = links_repo.find_by_file_id(&file_id)?;
        let extracted_text_sha256 = sha256_of_string(description);
        for link in links {
            let has_extracted_text =
                !js_trim(link.extracted_text.as_deref().unwrap_or("")).is_empty();
            if has_extracted_text {
                continue;
            }
            let applied = links_repo
                .apply_auto_description(
                    &link.id,
                    description,
                    &extracted_text_sha256,
                    now_iso,
                    now_iso,
                )
                .and_then(|_| {
                    chunk_and_insert_extracted_text(
                        mount,
                        &link.id,
                        &link.mount_point_id,
                        description,
                        now_iso,
                    )
                });
            match applied {
                Ok(()) => {
                    touched.insert(link.mount_point_id.clone());
                    links_updated += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        file_entry_id = %entry.id,
                        link_id = %link.id,
                        error = %e,
                        "auto-describe: failed to update link"
                    );
                }
            }
        }
    }
    Ok((described_file_entry, links_updated, touched))
}

/// v4 `autoDescribeChatImageAttachment` — the whole pipeline. Safe to call
/// repeatedly: when the FileEntry already carries a description, the call
/// short-circuits (`already-described`) before invoking the vision LLM.
pub async fn auto_describe_chat_image_attachment(
    db: &Db,
    file_bytes: &dyn FileBytesStore,
    side_effects: &dyn SaveImageSideEffects,
    describe: Option<&dyn ImageDescribeDriver>,
    file_entry_id: &str,
) -> AutoDescribeOutput {
    // Arms 1–4 (cheap reads, no bytes).
    let id = file_entry_id.to_string();
    let precheck = db.read_main(move |c| auto_describe_precheck(c, &id));
    let entry = match precheck {
        Ok(AutoDescribePrecheck::Skip(reason)) => return AutoDescribeOutput::skipped(reason),
        Ok(AutoDescribePrecheck::Proceed(entry)) => *entry,
        Err(e) => {
            tracing::warn!(
                file_entry_id = %file_entry_id,
                error = %e,
                "auto-describe: precheck read failed"
            );
            return AutoDescribeOutput::skipped(AutoDescribeSkipReason::NotFound);
        }
    };

    // Bytes (v4 `fileStorageManager.downloadFile`; a throw is warn + no-bytes;
    // the trait's `Ok(None)` missing-bytes arm folds there too).
    let buffer = match file_bytes.read_image_buffer(&entry) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return AutoDescribeOutput::skipped(AutoDescribeSkipReason::NoBytes);
        }
        Err(error) => {
            tracing::warn!(
                file_entry_id = %entry.id,
                error = %error,
                "auto-describe: failed to read bytes for FileEntry"
            );
            return AutoDescribeOutput::skipped(AutoDescribeSkipReason::NoBytes);
        }
    };

    let file = FallbackFile {
        id: entry.id.clone(),
        filename: entry.original_filename.clone(),
        mime_type: entry.mime_type.clone(),
        data: Some(base64::engine::general_purpose::STANDARD.encode(&buffer)),
    };

    // The vision call (behind the §C2 driver; the host's driver runs
    // `generate_image_description` whole — profile resolution, refusals, the
    // uncensored retry, `logLLMCall`).
    let result = match describe {
        Some(driver) => Some(driver.describe(file).await),
        None => None,
    };
    // v4: `result.type !== 'image_description' || !result.imageDescription`
    // — the truthiness check, so an EMPTY string fails here but a
    // whitespace-only one passes (and persists as `''` after the trim below).
    // Carried as-is.
    let described = result.as_ref().and_then(|r| {
        if r.type_ == FallbackType::ImageDescription {
            r.image_description.clone().filter(|d| !d.is_empty())
        } else {
            None
        }
    });
    let Some(raw_description) = described else {
        tracing::info!(
            file_entry_id = %entry.id,
            "auto-describe: vision describe did not produce a description"
        );
        return AutoDescribeOutput::skipped(AutoDescribeSkipReason::DescribeFailed);
    };

    let description = js_trim(&raw_description).to_string();
    let now_iso = crate::clock::now_iso();
    let entry_for_write = entry.clone();
    let desc_for_write = description.clone();
    let persisted = db
        .write(move |writers| {
            let Some(mount_w) = writers.mount_index() else {
                // No mount-index partition: the files-row write still lands;
                // there are no links to update.
                let files = FilesRepository::new(writers.main().connection());
                let described = files.update(
                    &entry_for_write.id,
                    &FileUpdate {
                        description: Some(desc_for_write.clone()),
                        updated_at: now_iso.clone(),
                        ..Default::default()
                    },
                )?;
                return Ok((described, 0i64, BTreeSet::new()));
            };
            let mount = mount_w.connection();
            let main = writers.main().connection();
            auto_describe_persist(main, mount, &entry_for_write, &desc_for_write, &now_iso)
        })
        .await;

    let (described_file_entry, links_updated, touched) = match persisted {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(
                file_entry_id = %entry.id,
                error = %e,
                "auto-describe: persist pass failed"
            );
            return AutoDescribeOutput::skipped(AutoDescribeSkipReason::DescribeFailed);
        }
    };

    // Queue embedding generation for every mount we wrote chunks into so the
    // new text becomes searchable (the recorded seam — v4
    // `enqueueEmbeddingJobsForMountPoint`, per-mount warn-and-continue there;
    // the differential jest-mocks it to a no-op, `NoSideEffects` here).
    for mount_point_id in &touched {
        side_effects.enqueue_embedding_jobs(mount_point_id);
    }

    tracing::info!(
        file_entry_id = %entry.id,
        links_updated,
        description_length = utf16_len(&description),
        "auto-describe: completed"
    );

    AutoDescribeOutput {
        described_file_entry,
        links_updated,
        description: Some(description),
        skip_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use super::*;
    use crate::db::doc_mount_file_links::LinkFilesystemFileInput;
    use crate::db::files::{CreateOptions, FileCreate};
    use crate::db::runtime::{Db, DbPaths};
    use crate::services::file_fallback::FallbackResult;

    const TEST_PEPPER: &str = "cXVpbGx0YXAtdGVzdC1wZXBwZXItMzItYnl0ZXMhIQ==";
    const USER: &str = "11111111-1111-4111-8111-111111111111";
    const TS: &str = "2026-08-01T00:00:00.000Z";
    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    /// A canned bytes store: fixed bytes for every entry, or a hard failure.
    struct CannedBytes(Option<Vec<u8>>);
    impl FileBytesStore for CannedBytes {
        fn read_image_buffer(&self, _entry: &FileEntry) -> Result<Option<Vec<u8>>, String> {
            match &self.0 {
                Some(b) => Ok(Some(b.clone())),
                None => Err("canned read failure".into()),
            }
        }
        fn ingest_image_buffer(
            &self,
            _req: &crate::photos::save_image_to_album::IngestImageRequest,
        ) -> Result<FileEntry, String> {
            Err("not used".into())
        }
    }

    /// A canned describe driver returning a fixed [`FallbackResult`]; counts
    /// its calls so the short-circuit arms can prove the LLM leg never ran
    /// (the tier3-mock-level rule: mock at the §C2 seam, everything above
    /// real).
    struct CannedDescribe {
        result: FallbackResult,
        calls: AtomicUsize,
    }
    impl CannedDescribe {
        fn description(text: &str) -> Self {
            CannedDescribe {
                result: FallbackResult {
                    type_: FallbackType::ImageDescription,
                    text_content: None,
                    image_description: Some(text.to_string()),
                    processing_metadata: None,
                    error: None,
                },
                calls: AtomicUsize::new(0),
            }
        }
        fn unsupported() -> Self {
            CannedDescribe {
                result: FallbackResult {
                    type_: FallbackType::Unsupported,
                    text_content: None,
                    image_description: None,
                    processing_metadata: None,
                    error: Some("No image description profile available. Configure one in Settings → Chat Settings → Image Description Profile".into()),
                },
                calls: AtomicUsize::new(0),
            }
        }
    }
    impl ImageDescribeDriver for CannedDescribe {
        fn describe<'a>(
            &'a self,
            _file: FallbackFile,
        ) -> Pin<Box<dyn Future<Output = FallbackResult> + Send + 'a>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let r = self.result.clone();
            Box::pin(async move { r })
        }
    }

    /// Records the enqueued mount points (the recorded seam).
    #[derive(Default)]
    struct RecordingSideEffects(Mutex<Vec<String>>);
    impl SaveImageSideEffects for RecordingSideEffects {
        fn enqueue_embedding_jobs(&self, mount_point_id: &str) {
            self.0.lock().unwrap().push(mount_point_id.to_string());
        }
    }

    /// Provision a fresh two-partition instance in a temp dir and open it.
    async fn scratch_db(dir: &tempfile::TempDir) -> Db {
        let dpath = dir.path().to_path_buf();
        tokio::task::spawn_blocking(move || {
            crate::services::provisioning::provision_fresh_instance(&dpath, TEST_PEPPER).unwrap();
            Db::open(
                DbPaths {
                    main: dpath.join("quilltap.db"),
                    mount_index: Some(dpath.join("quilltap-mount-index.db")),
                    llm_logs: None,
                },
                TEST_PEPPER,
            )
            .unwrap()
        })
        .await
        .unwrap()
    }

    fn file_create(sha: &str, mime: &str, description: Option<&str>) -> FileCreate {
        FileCreate {
            user_id: USER.to_string(),
            sha256: sha.to_string(),
            original_filename: "subject.webp".to_string(),
            mime_type: mime.to_string(),
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
            description: description.map(str::to_string),
            tags: vec![],
            project_id: None,
            folder_path: None,
            storage_key: None,
            file_status: "ok".to_string(),
        }
    }

    /// Seed one files row (main) and, when `link` is set, a mount point + a
    /// filesystem link sharing the sha (a "blank" link — no extractedText).
    async fn seed(db: &Db, id: &str, sha: &str, mime: &str, description: Option<&str>, link: bool) {
        let id = id.to_string();
        let sha = sha.to_string();
        let mime = mime.to_string();
        let description = description.map(str::to_string);
        db.write(move |ws| {
            let main = ws.main().connection();
            FilesRepository::new(main).create(
                &file_create(&sha, &mime, description.as_deref()),
                &CreateOptions {
                    id: id.clone(),
                    created_at: TS.to_string(),
                    updated_at: TS.to_string(),
                },
            )?;
            if link {
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
                        sha256: sha.clone(),
                        file_size_bytes: 10.0,
                        last_modified: TS.to_string(),
                        ..Default::default()
                    },
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn skip_arms_fire_in_v4_order_without_a_vision_call() {
        let dir = tempfile::tempdir().unwrap();
        let db = scratch_db(&dir).await;
        let bytes = CannedBytes(Some(vec![1, 2, 3]));
        let effects = RecordingSideEffects::default();
        let driver = CannedDescribe::description("never used");

        // not-found.
        let out = auto_describe_chat_image_attachment(
            &db,
            &bytes,
            &effects,
            Some(&driver),
            "99999999-9999-4999-8999-999999999999",
        )
        .await;
        assert_eq!(out.skip_reason, Some(AutoDescribeSkipReason::NotFound));

        // not-image.
        seed(&db, "f-text", SHA_A, "text/plain", None, false).await;
        let out =
            auto_describe_chat_image_attachment(&db, &bytes, &effects, Some(&driver), "f-text")
                .await;
        assert_eq!(out.skip_reason, Some(AutoDescribeSkipReason::NotImage));

        // no-sha (an empty sha256 — v4's `!entry.sha256` falsy check).
        seed(&db, "f-nosha", "", "image/webp", None, false).await;
        let out =
            auto_describe_chat_image_attachment(&db, &bytes, &effects, Some(&driver), "f-nosha")
                .await;
        assert_eq!(out.skip_reason, Some(AutoDescribeSkipReason::NoSha));

        // already-described (a whitespace-only description does NOT count —
        // v4 trims before the length check).
        seed(
            &db,
            "f-done",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "image/webp",
            Some("already here"),
            false,
        )
        .await;
        let out =
            auto_describe_chat_image_attachment(&db, &bytes, &effects, Some(&driver), "f-done")
                .await;
        assert_eq!(
            out.skip_reason,
            Some(AutoDescribeSkipReason::AlreadyDescribed)
        );

        // None of the arms reached the vision driver.
        assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
        assert!(effects.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn no_bytes_and_no_driver_arms() {
        let dir = tempfile::tempdir().unwrap();
        let db = scratch_db(&dir).await;
        seed(&db, "f-img", SHA_A, "image/webp", None, false).await;
        let effects = RecordingSideEffects::default();

        // A bytes read failure → no-bytes (v4's downloadFile catch).
        let failing = CannedBytes(None);
        let driver = CannedDescribe::description("never used");
        let out =
            auto_describe_chat_image_attachment(&db, &failing, &effects, Some(&driver), "f-img")
                .await;
        assert_eq!(out.skip_reason, Some(AutoDescribeSkipReason::NoBytes));
        assert_eq!(driver.calls.load(Ordering::SeqCst), 0);

        // No driver assembled → describe-failed (v4's non-description result).
        let bytes = CannedBytes(Some(vec![1, 2, 3]));
        let out = auto_describe_chat_image_attachment(&db, &bytes, &effects, None, "f-img").await;
        assert_eq!(
            out.skip_reason,
            Some(AutoDescribeSkipReason::DescribeFailed)
        );

        // A driver answering a non-description result → describe-failed.
        let refusing = CannedDescribe::unsupported();
        let out =
            auto_describe_chat_image_attachment(&db, &bytes, &effects, Some(&refusing), "f-img")
                .await;
        assert_eq!(
            out.skip_reason,
            Some(AutoDescribeSkipReason::DescribeFailed)
        );
        assert_eq!(refusing.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn success_persists_to_files_blank_links_and_chunks_and_enqueues() {
        let dir = tempfile::tempdir().unwrap();
        let db = scratch_db(&dir).await;
        seed(&db, "f-img", SHA_A, "image/webp", None, true).await;
        let bytes = CannedBytes(Some(vec![1, 2, 3]));
        let effects = RecordingSideEffects::default();
        // Surrounding whitespace pins the v4 trim before persist.
        let driver = CannedDescribe::description("  A copper kettle steams on a windowsill.  ");

        let out =
            auto_describe_chat_image_attachment(&db, &bytes, &effects, Some(&driver), "f-img")
                .await;
        assert_eq!(out.skip_reason, None);
        assert!(out.described_file_entry);
        assert_eq!(out.links_updated, 1);
        assert_eq!(
            out.description.as_deref(),
            Some("A copper kettle steams on a windowsill.")
        );
        assert_eq!(effects.0.lock().unwrap().as_slice(), ["mp-1"]);

        // The three sinks: files.description, the link row, the chunk rows.
        let (file_desc, link_row, chunk_count): (Option<String>, (String, String, String), i64) =
            db.write(|ws| {
                let main = ws.main().connection();
                let file_desc = FilesRepository::new(main)
                    .find_by_id("f-img")?
                    .unwrap()
                    .description;
                let mount = ws.mount_index().unwrap().connection();
                let link_row = mount.query_row(
                    "SELECT description, extractedText, extractionStatus \
                     FROM doc_mount_file_links",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                let chunk_count =
                    mount.query_row("SELECT COUNT(*) FROM doc_mount_chunks", [], |r| r.get(0))?;
                Ok((file_desc, link_row, chunk_count))
            })
            .await
            .unwrap();
        assert_eq!(
            file_desc.as_deref(),
            Some("A copper kettle steams on a windowsill.")
        );
        assert_eq!(link_row.0, "A copper kettle steams on a windowsill.");
        assert_eq!(link_row.1, "A copper kettle steams on a windowsill.");
        assert_eq!(link_row.2, "converted");
        assert!(chunk_count > 0, "the chunk pass must write rows");

        // A second run now short-circuits: already-described, no vision call.
        let out2 =
            auto_describe_chat_image_attachment(&db, &bytes, &effects, Some(&driver), "f-img")
                .await;
        assert_eq!(
            out2.skip_reason,
            Some(AutoDescribeSkipReason::AlreadyDescribed)
        );
        assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn kept_links_with_extracted_text_are_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let db = scratch_db(&dir).await;
        seed(&db, "f-img", SHA_A, "image/webp", None, true).await;
        // Give the link kept-image markdown (a non-blank extractedText).
        db.write(|ws| {
            let mount = ws.mount_index().unwrap().connection();
            mount.execute(
                "UPDATE doc_mount_file_links SET extractedText = '## Original prompt\n\nkept'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let bytes = CannedBytes(Some(vec![1, 2, 3]));
        let effects = RecordingSideEffects::default();
        let driver = CannedDescribe::description("A description.");
        let out =
            auto_describe_chat_image_attachment(&db, &bytes, &effects, Some(&driver), "f-img")
                .await;
        // The files row is described, but the kept link is skipped whole.
        assert!(out.described_file_entry);
        assert_eq!(out.links_updated, 0);
        assert!(effects.0.lock().unwrap().is_empty());
        let kept: String = db
            .write(|ws| {
                let mount = ws.mount_index().unwrap().connection();
                Ok(
                    mount.query_row("SELECT extractedText FROM doc_mount_file_links", [], |r| {
                        r.get(0)
                    })?,
                )
            })
            .await
            .unwrap();
        assert_eq!(kept, "## Original prompt\n\nkept");
    }
}
