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
            Err(e) => return Err(crate::db::DbError::Key(e.to_string())),
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
            Err(e) => return Err(crate::db::DbError::Key(e.to_string())),
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
