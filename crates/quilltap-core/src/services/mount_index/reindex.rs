//! Port of v4 `lib/mount-index/reindex.ts` — the scoped re-extract + re-chunk
//! pass (**synchronous in-request**, deliberately: the scanner is synchronous
//! too, and a new background-job type was explicitly avoided) and the scoped
//! embedding enqueue (async via EMBEDDING_GENERATE jobs).

use rusqlite::Connection;

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::{
    sha256_of_string, DocMountFileLinksRepository, LinkRow, LinkUpdate,
};
use crate::db::doc_mount_points::MountServiceInfo;
use crate::db::DbError;
use crate::jsstr::{js_trim, utf16_len, utf16_truncate};

use super::chunker::{chunk_document, ChunkOptions};
use super::converters::{convert_buffer_to_plain_text, DocumentTextExtractor};
use super::embedding_scheduler::{
    default_profile_id, enqueue_mount_chunk_embedding, first_user_id,
};
use super::reindex_file::insert_chunks;

const TEXT_NATIVE: [&str; 4] = ["markdown", "txt", "json", "jsonl"];
const EXTRACTABLE: [&str; 6] = ["pdf", "docx", "markdown", "txt", "json", "jsonl"];

/// v4 `ReindexOptions`.
#[derive(Debug, Clone, Default)]
pub struct ReindexOptions {
    /// Path scope: an exact link path, or a folder prefix covering every link
    /// beneath it. Leading/trailing slashes normalised.
    pub path: Option<String>,
    /// Without `force`, text-native links only re-chunk when chunkless, and
    /// pdf/docx only in `none|pending|failed|skipped` extraction state.
    pub force: bool,
}

/// v4 `ReindexResult`.
#[derive(Debug, Clone)]
pub struct ReindexResult {
    pub processed: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub errors: Vec<(String, String)>, // (relativePath, error)
}

impl ReindexResult {
    pub fn errors_json(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.errors
                .iter()
                .map(|(p, e)| serde_json::json!({ "relativePath": p, "error": e }))
                .collect(),
        )
    }
}

/// v4 `normalisePathScope`.
fn normalise_path_scope(p: Option<&str>) -> Option<String> {
    let p = p?;
    let s = p.trim_start_matches('/').trim_end_matches('/');
    if s.is_empty() || s == "." {
        return None;
    }
    Some(s.to_string())
}

/// v4 `linkMatchesScope`.
fn link_matches_scope(link: &LinkRow, scope: Option<&str>) -> bool {
    let Some(scope) = scope else { return true };
    if link.relative_path == scope {
        return true;
    }
    link.relative_path.starts_with(&format!("{scope}/"))
}

/// v4 `shouldProcess`.
fn should_process(link: &LinkRow, force: bool) -> bool {
    if force {
        return EXTRACTABLE.contains(&link.file_type.as_str());
    }
    // Text-native: only if no chunks exist yet.
    if TEXT_NATIVE.contains(&link.file_type.as_str()) {
        return link.chunk_count == 0;
    }
    // Non-text-native: only pdf/docx get extraction.
    if link.file_type != "pdf" && link.file_type != "docx" {
        return false;
    }
    matches!(
        link.extraction_status.as_deref(),
        Some("none") | Some("pending") | Some("failed") | Some("skipped")
    )
}

/// v4 `readSourceBytes`: fs links read `path.join(basePath, relativePath)`
/// (deliberately NOT the boundary guard — indexed links are trusted);
/// database-backed links pull from documents (text) or blobs (binary).
fn read_source_bytes(
    conn: &Connection,
    mount_point: &MountServiceInfo,
    link: &LinkRow,
) -> Result<Vec<u8>, String> {
    if link.source == "filesystem" {
        let base = mount_point.base_path.as_deref().unwrap_or("");
        let abs = format!("{}/{}", base, link.relative_path);
        return std::fs::read(&abs).map_err(|e| e.to_string());
    }
    if TEXT_NATIVE.contains(&link.file_type.as_str()) {
        let doc = DocMountDocumentsRepository::new(conn)
            .find_content_by_file_id(&link.file_id)
            .map_err(|e| e.to_string())?;
        let Some(content) = doc else {
            return Err(format!("No document content for fileId {}", link.file_id));
        };
        return Ok(content.into_bytes());
    }
    let bytes = DocMountBlobsRepository::new(conn)
        .read_data_by_file_id(&link.file_id)
        .map_err(|e| e.to_string())?;
    bytes.ok_or_else(|| format!("No blob bytes for fileId {}", link.file_id))
}

/// v4 `reindexLinks(mountPoint, options)` — synchronous in-request.
pub fn reindex_links(
    conn: &Connection,
    mount_point: &MountServiceInfo,
    options: &ReindexOptions,
    extractor: &dyn DocumentTextExtractor,
) -> Result<ReindexResult, DbError> {
    let links_repo = DocMountFileLinksRepository::new(conn);
    let scope = normalise_path_scope(options.path.as_deref());
    let force = options.force;

    let all_links = links_repo.find_by_mount_point_id(&mount_point.id)?;
    let in_scope: Vec<&LinkRow> = all_links
        .iter()
        .filter(|l| link_matches_scope(l, scope.as_deref()))
        .collect();

    let mut result = ReindexResult {
        processed: 0,
        succeeded: 0,
        failed: 0,
        skipped: 0,
        errors: Vec::new(),
    };

    for link in in_scope {
        if !should_process(link, force) {
            result.skipped += 1;
            continue;
        }
        result.processed += 1;

        let is_text_native = TEXT_NATIVE.contains(&link.file_type.as_str());
        let step = (|| -> Result<Result<(), String>, DbError> {
            let buf = match read_source_bytes(conn, mount_point, link) {
                Ok(b) => b,
                Err(msg) => return Ok(Err(msg)),
            };
            let plain_text = convert_buffer_to_plain_text(&buf, &link.file_type, extractor);
            let trimmed_empty = js_trim(&plain_text).is_empty();

            if trimmed_empty {
                links_repo.update(
                    &link.id,
                    &LinkUpdate {
                        extraction_status: Some("failed".to_string()),
                        extracted_text: Some(None),
                        extracted_text_sha256: Some(None),
                        extraction_error: Some(Some("extractor returned empty text".to_string())),
                        chunk_count: Some(0.0),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    },
                )?;
                links_repo.delete_chunks_by_link_id(&link.id)?;
                return Ok(Err("extractor returned empty text".to_string()));
            }

            let chunks = chunk_document(&plain_text, ChunkOptions::default());
            links_repo.delete_chunks_by_link_id(&link.id)?;
            if !chunks.is_empty() {
                insert_chunks(conn, &link.id, &mount_point.id, &chunks)?;
            }
            links_repo.update(
                &link.id,
                &LinkUpdate {
                    extracted_text: Some(if is_text_native {
                        None
                    } else {
                        Some(plain_text.clone())
                    }),
                    extracted_text_sha256: Some(if is_text_native {
                        None
                    } else {
                        Some(sha256_of_string(&plain_text))
                    }),
                    extraction_status: Some(
                        if is_text_native { "none" } else { "converted" }.to_string(),
                    ),
                    extraction_error: Some(None),
                    plain_text_length: Some(Some(utf16_len(&plain_text) as f64)),
                    chunk_count: Some(chunks.len() as f64),
                    updated_at: crate::clock::now_iso(),
                    ..Default::default()
                },
            )?;
            Ok(Ok(()))
        })();

        match step {
            Ok(Ok(())) => result.succeeded += 1,
            Ok(Err(msg)) => {
                // The empty-text arm already wrote its bookkeeping; the
                // read-failure arm takes v4's catch path (best-effort status).
                if msg != "extractor returned empty text" {
                    let _ = links_repo.update(
                        &link.id,
                        &LinkUpdate {
                            extraction_status: Some("failed".to_string()),
                            extraction_error: Some(Some(utf16_truncate(&msg, 500))),
                            updated_at: crate::clock::now_iso(),
                            ..Default::default()
                        },
                    );
                }
                result.failed += 1;
                result.errors.push((link.relative_path.clone(), msg));
            }
            Err(e) => {
                // A DB failure mid-link: v4's catch — best-effort status update,
                // then keep going.
                let msg = e.to_string();
                let _ = links_repo.update(
                    &link.id,
                    &LinkUpdate {
                        extraction_status: Some("failed".to_string()),
                        extraction_error: Some(Some(utf16_truncate(&msg, 500))),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    },
                );
                result.failed += 1;
                result.errors.push((link.relative_path.clone(), msg));
            }
        }
    }

    Ok(result)
}

/// The scoped-enqueue error split: `Config` carries v4's thrown message
/// verbatim (`No default embedding profile configured` / `No user found`) so the route
/// 500 body matches byte-for-byte; `Db` is an infrastructure failure.
#[derive(Debug)]
pub enum ScopedEnqueueError {
    Config(String),
    Db(DbError),
}
impl From<DbError> for ScopedEnqueueError {
    fn from(e: DbError) -> Self {
        ScopedEnqueueError::Db(e)
    }
}
impl std::fmt::Display for ScopedEnqueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScopedEnqueueError::Config(m) => write!(f, "{m}"),
            ScopedEnqueueError::Db(e) => write!(f, "{e}"),
        }
    }
}

/// v4 `enqueueEmbeddingJobsScoped` — EMBEDDING_GENERATE jobs for null-embedding
/// chunks under the path scope (`force` re-queues embedded ones too). Returns
/// `(jobs, queued, skipped)`; each job is `{id, kind:'embed', status:'queued'}`.
pub fn enqueue_embedding_jobs_scoped(
    main: &Connection,
    mount: &Connection,
    mount_point: &MountServiceInfo,
    options: &ReindexOptions,
) -> Result<(Vec<serde_json::Value>, i64, i64), ScopedEnqueueError> {
    let links_repo = DocMountFileLinksRepository::new(mount);
    let scope = normalise_path_scope(options.path.as_deref());
    let force = options.force;

    let all_links = links_repo.find_by_mount_point_id(&mount_point.id)?;
    let in_scope_link_ids: std::collections::HashSet<String> = all_links
        .iter()
        .filter(|l| link_matches_scope(l, scope.as_deref()))
        .map(|l| l.id.clone())
        .collect();

    let all_chunks = crate::db::doc_mount_chunks::DocMountChunksRepository::new(mount)
        .find_rows_by_mount_point_id(&mount_point.id)?;
    let candidates: Vec<_> = all_chunks
        .iter()
        .filter(|c| in_scope_link_ids.contains(&c.link_id))
        .collect();
    let need_embedding: Vec<_> = if force {
        candidates.clone()
    } else {
        candidates
            .iter()
            .filter(|c| !c.has_embedding)
            .copied()
            .collect()
    };

    if need_embedding.is_empty() {
        return Ok((Vec::new(), 0, candidates.len() as i64));
    }

    let profile_id = default_profile_id(main)?.ok_or_else(|| {
        ScopedEnqueueError::Config("No default embedding profile configured".to_string())
    })?;
    let user_id = first_user_id(main)?
        .ok_or_else(|| ScopedEnqueueError::Config("No user found".to_string()))?;

    let mut jobs = Vec::new();
    let mut queued = 0i64;
    let mut skipped = (candidates.len() - need_embedding.len()) as i64;

    for chunk in need_embedding {
        match enqueue_mount_chunk_embedding(main, &user_id, &chunk.id, &profile_id) {
            Ok((job_id, true)) => {
                queued += 1;
                jobs.push(serde_json::json!({
                    "id": job_id,
                    "kind": "embed",
                    "status": "queued",
                }));
            }
            Ok((_, false)) => skipped += 1,
            Err(e) => {
                eprintln!(
                    "Failed to enqueue embedding job for chunk {}: {e}",
                    chunk.id
                );
                skipped += 1;
            }
        }
    }

    Ok((jobs, queued, skipped))
}
