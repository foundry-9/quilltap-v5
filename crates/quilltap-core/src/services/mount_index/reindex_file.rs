//! Port of v4 `lib/doc-edit/reindex-file.ts` — single-file re-chunking after an
//! edit/write. Lives under `services::mount_index` in v5 (v4 keeps it in
//! `lib/doc-edit/`, but its only consumers here are the mount-index write /
//! rescan / refresh paths; `tools/doc_edit` is a frozen reference this round).
//!
//! Best-effort like v4: every failure is swallowed into a stderr warning — the
//! edit that triggered the reindex has already succeeded.

use rusqlite::Connection;

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_chunks::{
    CreateOptions as ChunkCreateOptions, DmcCreate, DocMountChunksRepository,
};
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::{
    policy_from_content, DocMountFileLinksRepository, LinkFilesystemFileInput, LinkUpdate,
    DEFAULT_DOCUMENT_POLICY,
};
use crate::db::doc_mount_points::DocMountPointsRepository;

use super::chunker::{chunk_document, ChunkOptions};
use super::converters::{convert_to_plain_text, DocumentTextExtractor};
use super::path_utils::path_extname;

/// v4 `detectFileType` (reindex-file.ts — the WIDE variant, incl. json/jsonl;
/// scanner.ts's narrow variant lives in `scanner::detect_file_type`).
pub fn detect_file_type_full(file_path: &str) -> Option<&'static str> {
    match path_extname(file_path).to_lowercase().as_str() {
        ".pdf" => Some("pdf"),
        ".docx" => Some("docx"),
        ".md" | ".markdown" => Some("markdown"),
        ".txt" => Some("txt"),
        ".json" => Some("json"),
        ".jsonl" | ".ndjson" => Some("jsonl"),
        _ => None,
    }
}

/// Insert the chunk set for a link (v4 `docMountChunks.bulkInsert` — minted
/// ids, fresh timestamps, `embedding: null`).
pub(crate) fn insert_chunks(
    conn: &Connection,
    link_id: &str,
    mount_point_id: &str,
    chunks: &[super::chunker::ChunkResult],
) -> Result<(), crate::db::DbError> {
    let repo = DocMountChunksRepository::new(conn);
    let now = crate::clock::now_iso();
    for c in chunks {
        repo.create(
            &DmcCreate {
                link_id: link_id.to_string(),
                mount_point_id: mount_point_id.to_string(),
                chunk_index: c.chunk_index as f64,
                content: c.content.clone(),
                token_count: c.token_count as f64,
                heading_context: c.heading_context.clone(),
                embedding: None,
            },
            &ChunkCreateOptions {
                id: uuid::Uuid::new_v4().to_string(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )?;
    }
    Ok(())
}

/// v4's post-write chunk pass for a database-backed store — the single-file
/// half (P4.6BK): `reindexSingleFile(mountPointId, rel, '')`, wrapped so the
/// `db` write sites need no extractor. A database-backed store chunks from
/// `doc_mount_documents.content` (or a blob's already-extracted text), so the
/// pdf/docx extractor seam is never consulted. Should the mount somehow not be
/// database-backed, the fs branch fails on the empty `absolute_path` and warns
/// — the same outcome as v4's `fs.stat('')` throw landing in its catch-all.
///
/// Callers want [`reindex_after_database_write`], which runs this AND the
/// hard-link group pass; this is the half, exposed for the group pass's own
/// per-sibling use.
fn reindex_single_file_after_database_write(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) {
    reindex_single_file(
        conn,
        mount_point_id,
        relative_path,
        "",
        &super::converters::RefusingTextExtractor,
    );
}

/// v4 `reindexAfterDatabaseWrite` (`lib/mount-index/post-write-reindex.ts`,
/// `23da0b322`) — the ONE post-write block every database-store writer calls.
///
/// A write to a database store records the document row and repoints the link;
/// it does not build the chunks that semantic search, `doc_grep`'s fallback and
/// every character's RAG context read. Each writer used to carry its own copy of
/// the follow-up — and `file_ops::write_dest_bytes` carried none at all, which
/// is half of bug 156. This is the one block they all call:
///
///   1. chunk the just-written content, so it is immediately searchable;
///   2. re-chunk every member of its hard-link group — the write has already
///      repointed them at the new content row, but chunks are per LINK, so
///      without this a sibling path keeps serving the previous revision's
///      chunks to search and to character context.
///
/// **Never throws**, and in v5 it cannot: both halves are infallible by
/// signature, logging their own failures and returning. That is worth stating
/// because v4 wraps each call in its own try/catch with a warn sentence
/// (`Failed to chunk database document after write` / `Failed to re-index
/// hard-link group after database write`) — and those two arms are **measured
/// unreachable in v4 too**: `reindexSingleFile` swallows everything in its own
/// catch (`Failed to re-index file after edit`, which v5 emits from
/// [`reindex_single_file`]), and `reindexLinkGroupSiblings` swallows per
/// sibling. v4's outer catches can fire only if the dynamic `await import(...)`
/// of the helper module fails, which a statically linked v5 has no analogue
/// for. So the two sentences have no v5 counterpart by construction, not by
/// omission.
///
/// **The `QUILLTAP_JOB_CHILD` divergence, re-recorded at the hoist.** v4 opens
/// this helper with `if (process.env.QUILLTAP_JOB_CHILD === '1') return;`
/// because inside its forked job child a repository write is buffered until the
/// batch ships home, so `reindexSingleFile` would read back content that is not
/// committed yet; in-child writers leave chunking to the next database rescan,
/// which now finds them because `link_document_content` zeroes `chunk_count`
/// whenever it repoints a link. **v5's job runner is in-process by locked
/// decision** — no fork, no buffered writes, read-your-writes holds — so the
/// guard's precondition cannot occur and v5 always chunks. Measured on the one
/// path that matters: the chunk-on-write job runs on the same connection as the
/// writer it follows, so what it reads back IS committed. The net effect where
/// v4 runs `doc_write_file` inside a forked child (autonomous turns) is that v5
/// builds the same rows the rescan would have built, just sooner. This matches
/// the enclave-step oracle, which runs v4 UNFORKED.
pub fn reindex_after_database_write(conn: &Connection, mount_point_id: &str, relative_path: &str) {
    reindex_single_file_after_database_write(conn, mount_point_id, relative_path);
    super::link_groups::reindex_link_group_siblings_after_database_write(
        conn,
        mount_point_id,
        relative_path,
    );
}

/// v4 `reindexSingleFile(mountPointId, relativePath, absolutePath)` — re-read,
/// re-chunk, and re-persist one file's chunks + link rollups. `absolute_path`
/// is `""` for database-backed stores (bytes come from the mount-index DB).
/// Best-effort: errors are logged, never returned (v4's catch-all).
pub fn reindex_single_file(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    absolute_path: &str,
    extractor: &dyn DocumentTextExtractor,
) {
    if let Err(e) = reindex_inner(
        conn,
        mount_point_id,
        relative_path,
        absolute_path,
        extractor,
    ) {
        eprintln!(
            "Failed to re-index file after edit: mount={mount_point_id} path={relative_path}: {e}"
        );
    }
}

fn reindex_inner(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    absolute_path: &str,
    extractor: &dyn DocumentTextExtractor,
) -> Result<(), crate::db::DbError> {
    let Some(file_type) = detect_file_type_full(relative_path) else {
        return Ok(());
    };

    let points = DocMountPointsRepository::new(conn);
    let is_database_backed = points
        .find_service_info_by_id(mount_point_id)?
        .map(|mp| mp.mount_type == "database")
        .unwrap_or(false);

    let plain_text: String;
    let last_modified_iso: String;
    let mut raw_for_policy: Option<String> = None;

    if is_database_backed {
        // Text source 1: doc_mount_documents.content (native text); source 2:
        // doc_mount_blobs.extractedText (derived pdf/docx plaintext).
        let docs = DocMountDocumentsRepository::new(conn);
        match docs.find_content_and_mtime_by_mount_point_and_path(mount_point_id, relative_path)? {
            Some((content, last_modified)) => {
                plain_text = content;
                last_modified_iso = last_modified;
                // Database stores: `plainText` IS the raw markdown (frontmatter
                // intact) — v4 parses policy from it directly.
                raw_for_policy = Some(plain_text.clone());
            }
            None => {
                let blobs = DocMountBlobsRepository::new(conn);
                let Some(blob) =
                    blobs.find_by_mount_point_and_path(mount_point_id, relative_path)?
                else {
                    return Ok(());
                };
                let Some(extracted) = blob
                    .extracted_text
                    .filter(|t| !crate::jsstr::js_trim(t).is_empty())
                else {
                    return Ok(());
                };
                plain_text = extracted;
                last_modified_iso = blob.updated_at;
                raw_for_policy = Some(plain_text.clone());
            }
        }
    } else {
        let meta = match std::fs::metadata(absolute_path) {
            Ok(m) => m,
            Err(e) => return Err(crate::db::DbError::Internal(e.to_string())),
        };
        let converted = convert_to_plain_text(absolute_path, file_type, extractor);
        if crate::jsstr::js_trim(&converted).is_empty() {
            return Ok(());
        }
        plain_text = converted;
        last_modified_iso = super::scanner::mtime_iso(&meta);
    }

    let chunks = chunk_document(&plain_text, ChunkOptions::default());

    // Per-document policy lives in markdown frontmatter. For fs stores the
    // converted text is frontmatter-stripped, so re-read the raw bytes.
    let mut policy = DEFAULT_DOCUMENT_POLICY;
    if file_type == "markdown" {
        let raw = match raw_for_policy {
            Some(r) => Some(r),
            None => std::fs::read_to_string(absolute_path).ok(),
        };
        policy = raw
            .map(|r| policy_from_content(&r))
            .unwrap_or(DEFAULT_DOCUMENT_POLICY);
    }

    let links = DocMountFileLinksRepository::new(conn);
    let existing_link = links.find_by_mount_point_and_path(mount_point_id, relative_path)?;

    let link_id: String = if is_database_backed {
        // Database stores refresh chunk metadata on the EXISTING link in place —
        // routing through linkFilesystemFile would fork a content-less file row
        // (resolved by (sha256, source='database') finding nothing) and sever
        // the document from its content row. See v4's why-comment.
        let Some(existing) = existing_link else {
            eprintln!(
                "Skipping reindex: database-backed file has no link row \
                 (mount={mount_point_id} path={relative_path})"
            );
            return Ok(());
        };
        links.delete_chunks_by_link_id(&existing.id)?;
        links.update(
            &existing.id,
            &LinkUpdate {
                conversion_status: Some("converted".to_string()),
                plain_text_length: Some(Some(crate::jsstr::utf16_len(&plain_text) as f64)),
                chunk_count: Some(chunks.len() as f64),
                last_modified: Some(last_modified_iso),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            },
        )?;
        // Only markdown carries policy frontmatter; non-markdown links keep
        // their permissive defaults untouched.
        if file_type == "markdown" {
            links.update_policy_flags(&existing.id, &policy)?;
        }
        existing.id
    } else {
        if let Some(existing) = &existing_link {
            links.delete_chunks_by_link_id(&existing.id)?;
        }
        let sha256 = match std::fs::read(absolute_path) {
            Ok(bytes) => super::scanner::sha256_hex(&bytes),
            Err(e) => return Err(crate::db::DbError::Internal(e.to_string())),
        };
        let file_size = std::fs::metadata(absolute_path)
            .map(|m| m.len() as f64)
            .unwrap_or(0.0);
        links.link_filesystem_file(&LinkFilesystemFileInput {
            mount_point_id: mount_point_id.to_string(),
            relative_path: relative_path.to_string(),
            file_name: node_basename(relative_path).to_string(),
            file_type: file_type.to_string(),
            sha256,
            file_size_bytes: file_size,
            last_modified: last_modified_iso,
            source: Some("filesystem".to_string()),
            conversion_status: Some("converted".to_string()),
            conversion_error: None,
            plain_text_length: Some(crate::jsstr::utf16_len(&plain_text) as f64),
            chunk_count: Some(chunks.len() as f64),
            allow_embed: Some(policy.embed),
            allow_character_read: Some(policy.character_read),
            allow_character_write: Some(policy.character_write),
        })?
    };

    if !chunks.is_empty() {
        insert_chunks(conn, &link_id, mount_point_id, &chunks)?;
    }
    Ok(())
}

/// Node `path.basename` for posix relative paths.
pub(crate) fn node_basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}
