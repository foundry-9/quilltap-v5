//! The uploaded-blob text backfill — v4 `lib/startup/backfill-upload-blob-text.ts`
//! (`40fab1c54`, bugs 260 / 261). Contract C2.
//!
//! Uploads stored before text extraction (every chat upload is a blob — `.md`,
//! `.txt` and `.json` included — and the bridges used to pass `extractText:
//! false`) get what new uploads now get at ingest: their text extracted onto the
//! link, chunked, and their store's embeddings queued. Idempotent and
//! stampless: the worklist (`extractionStatus = 'none'`) IS the flag — an
//! extracted row leaves it, a failed one is marked `failed` and leaves it too.
//!
//! **Where it runs.** v4 chains it after PHASE 3.3's `scanAllMountPoints` in
//! `instrumentation.ts`; v5 has no boot mount scan, so the host calls it ONCE
//! from the mount-index boot write closure, after the built-in mounts exist
//! (`quilltap-host/src/host.rs`). Its lines therefore fire on the writer
//! thread, under `target: "quilltap::startup"` (v4's `Startup:*` service
//! loggers' twin, shared with `daily_db_optimize`).
//!
//! **PDF / DOCX under the refusing seam (P4.D354 R-1).** While the production
//! [`DocumentTextExtractor`] is the refusing default (`is_available() ==
//! false`), the worklist names the SIX native-text extensions only: v4's
//! one-pass write would stamp every PDF / DOCX upload `failed` (`Converter
//! produced no text`) and its `'none'`-only worklist would never revisit them
//! once a real extractor lands. A v5-only INFO names the skip (with its count)
//! once per boot when there are such rows.
//! The NAMED divergence `REFUSING_SEAM_LEAVES_PDF_UNEXTRACTED`.
//!
//! **v4 lines with no reachable v5 twin (recorded, not emitted):**
//! - `Chunking a backfilled blob failed; its text is still readable` — v5's
//!   [`reindex_single_file`] is best-effort and never returns an error.
//! - `Uploaded-blob text backfill failed` (the outer catch) — every read this
//!   body makes is a fallback and every write is caught per store, as in v4,
//!   where the catch is defensive only; nothing in v5 can reach it.
//! - `Uploaded-blob text backfill could not start` (`instrumentation.ts:718`)
//!   — there is no promise chain to fail; the Rust function cannot fail to
//!   start.

use rusqlite::Connection;
use serde::Serialize;

use super::converters::DocumentTextExtractor;
use super::embedding_scheduler::enqueue_embedding_jobs_for_mount_point;
use super::path_utils::EXTRACTABLE_BLOB_EXTENSIONS;
use super::reindex_file::reindex_single_file;
use super::store_file::{extract_blob_text, ExtractBlobTextArgs, RefusedConverterPolicy};
use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;

/// The extensions the refusing seam leaves out of the worklist (R-1).
pub const REFUSED_CONVERTER_EXTENSIONS: &[&str] = &[".pdf", ".docx"];

/// v4 `UploadBlobTextBackfillResult`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadBlobTextBackfillResult {
    pub candidates: usize,
    pub extracted: usize,
    pub failed: usize,
    pub stores_touched: usize,
}

/// v4 `backfillUploadBlobText()`. Never errors (every failure is logged as v4
/// logs it); returns the tallies.
pub fn backfill_upload_blob_text(
    main: &Connection,
    mount: &Connection,
    extractor: &dyn DocumentTextExtractor,
) -> UploadBlobTextBackfillResult {
    let mut result = UploadBlobTextBackfillResult::default();
    let blobs = DocMountBlobsRepository::new(mount);

    let extensions: Vec<&str> = if extractor.is_available() {
        EXTRACTABLE_BLOB_EXTENSIONS.to_vec()
    } else {
        // v5's own line (R-1): the refusing seam's pdf / docx carve, named once
        // per boot when there is something to leave (a steady-state boot stays
        // DEBUG-only).
        let skipped = blobs
            .list_unextracted_by_extensions(REFUSED_CONVERTER_EXTENSIONS)
            .len();
        if skipped > 0 {
            tracing::info!(
                target: "quilltap::startup",
                skippedExtensions = ?REFUSED_CONVERTER_EXTENSIONS,
                skipped,
                "Leaving PDF/DOCX uploads unextracted; no document text extractor is available"
            );
        }
        EXTRACTABLE_BLOB_EXTENSIONS
            .iter()
            .copied()
            .filter(|e| !REFUSED_CONVERTER_EXTENSIONS.contains(e))
            .collect()
    };
    let worklist = blobs.list_unextracted_by_extensions(&extensions);
    result.candidates = worklist.len();
    if worklist.is_empty() {
        tracing::debug!(
            target: "quilltap::startup",
            "No unextracted text blobs; nothing to backfill"
        );
        return result;
    }

    tracing::info!(
        target: "quilltap::startup",
        candidates = worklist.len(),
        "Extracting text from uploaded blobs"
    );

    // v4's `Set` iterates in insertion order — a `BTreeSet` would not.
    let mut touched_stores: Vec<String> = Vec::new();
    for blob in &worklist {
        let Some(data) = blobs.read_data_or_none(&blob.id) else {
            tracing::warn!(
                target: "quilltap::startup",
                mountPointId = %blob.mount_point_id,
                relativePath = %blob.relative_path,
                "Blob bytes missing; skipping text backfill"
            );
            result.failed += 1;
            continue;
        };

        let ok = extract_blob_text(
            mount,
            &ExtractBlobTextArgs {
                blob_id: &blob.id,
                link_id: &blob.link_id,
                relative_path: &blob.relative_path,
                data: &data,
                mount_point_id: &blob.mount_point_id,
            },
            extractor,
            RefusedConverterPolicy::LeaveUnextracted,
        );
        if !ok {
            result.failed += 1;
            continue;
        }
        result.extracted += 1;
        if !touched_stores.contains(&blob.mount_point_id) {
            touched_stores.push(blob.mount_point_id.clone());
        }

        // Best-effort: v4's `Chunking a backfilled blob failed` WARN has no
        // reachable twin (module docs).
        reindex_single_file(
            mount,
            &blob.mount_point_id,
            &blob.relative_path,
            "",
            extractor,
        );
    }

    for mount_point_id in &touched_stores {
        let queued = enqueue_embedding_jobs_for_mount_point(main, mount, mount_point_id)
            .and_then(|_| DocMountPointsRepository::new(mount).refresh_stats(mount_point_id));
        if let Err(e) = queued {
            tracing::warn!(
                target: "quilltap::startup",
                mountPointId = %mount_point_id,
                error = %e,
                "Queueing embeddings for a backfilled store failed"
            );
        }
    }
    result.stores_touched = touched_stores.len();

    tracing::info!(
        target: "quilltap::startup",
        candidates = result.candidates,
        extracted = result.extracted,
        failed = result.failed,
        storesTouched = result.stores_touched,
        "Uploaded-blob text backfill finished"
    );
    result
}

#[cfg(test)]
mod tests {
    //! The capture pins (P4.D354 item 18): every line `extract_blob_text`, the
    //! worklist read and the backfill emit, each with its silence leg — over a
    //! freshly provisioned instance's real schema. `global_capture` (the lines
    //! fire on this thread; in production the host runs them inside its
    //! `write_blocking` closure).
    use super::*;
    use crate::db::Writer;
    use crate::services::mount_index::blob_transcode::RefusingWebpTranscoder;
    use crate::services::mount_index::converters::RefusingTextExtractor;
    use crate::services::mount_index::store_file::{
        store_mount_file, AssetStorageMode, CollisionStrategy, StoreFileInput,
    };
    use crate::test_support::global_capture;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const MP: &str = "c3540000-0000-4000-8000-000000000001";

    /// A text extractor that CAN convert (the future extractor's shape).
    struct Scripted(&'static str);
    impl DocumentTextExtractor for Scripted {
        fn extract(&self, _bytes: &[u8], _file_type: &str) -> String {
            self.0.to_string()
        }
    }

    fn instance() -> (Writer, Writer, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let main = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
        let mount = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
        mount
            .connection()
            .execute(
                "INSERT INTO doc_mount_points (id, name, mountType, createdAt, updatedAt) \
                 VALUES (?1, 'Uploads', 'database', '2026-01-01T00:00:00.000Z', \
                 '2026-01-01T00:00:00.000Z')",
                [MP],
            )
            .unwrap();
        (main, mount, dir)
    }

    /// Plant a pre-bug-260 upload (stored as a blob, NOT extracted).
    fn plant(main: &Writer, mount: &Writer, path: &str, bytes: &[u8]) -> (String, String) {
        let mut input = StoreFileInput::new(MP, path, bytes.to_vec());
        input.treat_native_text_as_document = false;
        input.extract_text = false;
        input.enqueue_embedding = false;
        input.collision_strategy = CollisionStrategy::UniqueSuffix;
        input.asset_storage = AssetStorageMode::Database;
        let r = store_mount_file(
            main.connection(),
            mount.connection(),
            &input,
            &RefusingTextExtractor,
            &RefusingWebpTranscoder,
        )
        .unwrap();
        (r.blob_id.unwrap(), r.link_id.unwrap())
    }

    fn status(mount: &Writer, path: &str) -> (Option<String>, Option<String>) {
        mount
            .connection()
            .query_row(
                "SELECT extractionStatus, extractionError FROM doc_mount_file_links \
                 WHERE relativePath = ?1",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }

    fn extract(
        mount: &Writer,
        ids: &(String, String),
        path: &str,
        data: &[u8],
        extractor: &dyn DocumentTextExtractor,
        policy: RefusedConverterPolicy,
    ) -> (bool, Vec<String>) {
        global_capture::capture(|| {
            extract_blob_text(
                mount.connection(),
                &ExtractBlobTextArgs {
                    blob_id: &ids.0,
                    link_id: &ids.1,
                    relative_path: path,
                    data,
                    mount_point_id: MP,
                },
                extractor,
                policy,
            )
        })
    }

    #[test]
    fn extract_blob_text_logs_the_stored_text_and_is_silent_otherwise() {
        global_capture::install();
        let (main, mount, _dir) = instance();
        let ids = plant(&main, &mount, "a/notes.md", b"# Notes\n\nhello");
        let (ok, lines) = extract(
            &mount,
            &ids,
            "a/notes.md",
            b"# Notes\n\nhello",
            &RefusingTextExtractor,
            RefusedConverterPolicy::LeaveUnextracted,
        );
        assert!(ok);
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::mount_index extractBlobText: stored extracted text \
                 mountPointId=c3540000-0000-4000-8000-000000000001 relativePath=a/notes.md \
                 blobType=blob textLength=14"
                    .to_string()
            ]
        );
        assert_eq!(status(&mount, "a/notes.md").0.as_deref(), Some("converted"));

        // The blank arm: `failed` written, NO line.
        let ids = plant(&main, &mount, "a/blank.txt", b" \n\t");
        let (ok, lines) = extract(
            &mount,
            &ids,
            "a/blank.txt",
            b" \n\t",
            &RefusingTextExtractor,
            RefusedConverterPolicy::LeaveUnextracted,
        );
        assert!(!ok);
        assert!(lines.is_empty(), "{lines:?}");
        assert_eq!(
            status(&mount, "a/blank.txt"),
            (
                Some("failed".to_string()),
                Some("Converter produced no text".to_string())
            )
        );

        // A non-extractable path: nothing written, NO line.
        let ids = plant(&main, &mount, "a/clip.mp3", b"ID3");
        let (ok, lines) = extract(
            &mount,
            &ids,
            "a/clip.mp3",
            b"ID3",
            &RefusingTextExtractor,
            RefusedConverterPolicy::WriteFailed,
        );
        assert!(!ok);
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// R-1: `LeaveUnextracted` writes nothing for a PDF under the refusing
    /// seam (v5's own DEBUG); `WriteFailed` keeps v4's bytes; an AVAILABLE
    /// extractor converts under either policy.
    #[test]
    fn the_refused_converter_policy_both_ways() {
        global_capture::install();
        let (main, mount, _dir) = instance();
        let ids = plant(&main, &mount, "r/one.pdf", b"%PDF-garbage");
        let (ok, lines) = extract(
            &mount,
            &ids,
            "r/one.pdf",
            b"%PDF-garbage",
            &RefusingTextExtractor,
            RefusedConverterPolicy::LeaveUnextracted,
        );
        assert!(!ok);
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::mount_index extractBlobText: converter unavailable; leaving the \
                 blob unextracted mountPointId=c3540000-0000-4000-8000-000000000001 \
                 relativePath=r/one.pdf blobType=pdf"
                    .to_string()
            ]
        );
        assert_eq!(
            status(&mount, "r/one.pdf"),
            (Some("none".to_string()), None)
        );

        let ids = plant(&main, &mount, "r/two.docx", b"PK-garbage");
        let (ok, _) = extract(
            &mount,
            &ids,
            "r/two.docx",
            b"PK-garbage",
            &RefusingTextExtractor,
            RefusedConverterPolicy::WriteFailed,
        );
        assert!(!ok);
        assert_eq!(
            status(&mount, "r/two.docx"),
            (
                Some("failed".to_string()),
                Some("Converter produced no text".to_string())
            )
        );

        let ids = plant(&main, &mount, "r/three.pdf", b"%PDF");
        let (ok, _) = extract(
            &mount,
            &ids,
            "r/three.pdf",
            b"%PDF",
            &Scripted("Page one text"),
            RefusedConverterPolicy::LeaveUnextracted,
        );
        assert!(ok);
        assert_eq!(
            status(&mount, "r/three.pdf").0.as_deref(),
            Some("converted")
        );
    }

    /// v4's catch arm, reachable in v5 through a failed write: WARN `text
    /// extraction failed`, then the failure record fails too → WARN `could not
    /// record the failure`.
    #[test]
    fn a_failed_write_takes_v4s_catch_arm() {
        global_capture::install();
        let (main, mount, _dir) = instance();
        let ids = plant(&main, &mount, "c/x.md", b"text");
        mount
            .connection()
            .execute_batch(
                "CREATE TRIGGER no_writes BEFORE UPDATE ON doc_mount_file_links \
                 BEGIN SELECT RAISE(ABORT, 'planted write failure'); END;",
            )
            .unwrap();
        let (ok, lines) = extract(
            &mount,
            &ids,
            "c/x.md",
            b"text",
            &RefusingTextExtractor,
            RefusedConverterPolicy::WriteFailed,
        );
        assert!(!ok);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with(
            "WARN quilltap::mount_index extractBlobText: text extraction failed \
             mountPointId=c3540000-0000-4000-8000-000000000001 relativePath=c/x.md blobType=blob error="
        ));
        assert!(lines[1].starts_with(
            "WARN quilltap::mount_index extractBlobText: could not record the failure \
             mountPointId=c3540000-0000-4000-8000-000000000001 relativePath=c/x.md error="
        ));
    }

    #[test]
    fn the_worklist_read_warns_and_answers_empty_on_failure_and_is_silent_otherwise() {
        global_capture::install();
        let (main, mount, _dir) = instance();
        plant(&main, &mount, "w/a.md", b"a");
        let blobs = DocMountBlobsRepository::new(mount.connection());
        let (rows, lines) =
            global_capture::capture(|| blobs.list_unextracted_by_extensions(&[".MD", ".txt"]));
        assert_eq!(rows.len(), 1);
        assert!(lines.is_empty(), "{lines:?}");
        let (rows, lines) = global_capture::capture(|| blobs.list_unextracted_by_extensions(&[]));
        assert!(rows.is_empty());
        assert!(lines.is_empty(), "{lines:?}");

        mount
            .connection()
            .execute_batch("DROP TABLE doc_mount_points;")
            .unwrap();
        let (rows, lines) =
            global_capture::capture(|| blobs.list_unextracted_by_extensions(&[".md"]));
        assert!(rows.is_empty());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "WARN quilltap::db Failed to list unextracted blobs extensions=[\".md\"] error="
            ),
            "{lines:?}"
        );
    }

    /// The steady-state boot is DEBUG-only: no worklist, no PDF to leave.
    #[test]
    fn a_steady_state_backfill_logs_one_debug_line() {
        global_capture::install();
        let (main, mount, _dir) = instance();
        let (result, lines) = global_capture::capture(|| {
            backfill_upload_blob_text(
                main.connection(),
                mount.connection(),
                &RefusingTextExtractor,
            )
        });
        assert_eq!(result, UploadBlobTextBackfillResult::default());
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::startup No unextracted text blobs; nothing to backfill"
                    .to_string()
            ]
        );
    }

    /// An available extractor lists PDF / DOCX too (no v5-only INFO).
    #[test]
    fn an_available_extractor_backfills_pdfs_without_the_skip_line() {
        global_capture::install();
        let (main, mount, _dir) = instance();
        plant(&main, &mount, "p/one.pdf", b"%PDF");
        let (result, lines) = global_capture::capture(|| {
            backfill_upload_blob_text(
                main.connection(),
                mount.connection(),
                &Scripted("Extracted words"),
            )
        });
        assert_eq!(
            result,
            UploadBlobTextBackfillResult {
                candidates: 1,
                extracted: 1,
                failed: 0,
                stores_touched: 1
            }
        );
        assert!(
            !lines.iter().any(|l| l.contains("Leaving PDF/DOCX")),
            "{lines:?}"
        );
        assert_eq!(status(&mount, "p/one.pdf").0.as_deref(), Some("converted"));
    }
}
