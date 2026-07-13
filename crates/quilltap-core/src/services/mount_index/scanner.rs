//! Port of v4 `lib/mount-index/scanner.ts` — the mount-point scan orchestrator:
//! walk the filesystem under `basePath`, apply include/exclude patterns, detect
//! new/modified/deleted files, convert → chunk → persist. Embedding jobs are
//! NOT enqueued here (v4's scan runner does that afterward; the `mountScan`
//! handler composes `enqueue_embedding_jobs_for_mount_point`).
//!
//! `matchesPattern` / `listFilesystemFolders` live in [`super::list`] (landed
//! with the READ/LIST keystone) — shared, not duplicated.

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db::doc_mount_file_links::{
    policy_from_content, DocMountFileLinksRepository, LinkFilesystemFileInput, LinkUpdate,
    DEFAULT_DOCUMENT_POLICY,
};
use crate::db::doc_mount_files::DocMountFilesRepository;
use crate::db::doc_mount_points::{DocMountPointsRepository, MountServiceInfo};
use crate::db::DbError;

use super::chunker::{chunk_document, ChunkOptions};
use super::converters::{convert_to_plain_text, DocumentTextExtractor};
use super::list::matches_pattern;
use super::path_utils::path_extname;
use super::reindex_file::{insert_chunks, node_basename, reindex_single_file};

/// v4 `ScanResult`.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub mount_point_id: String,
    pub files_scanned: i64,
    pub files_new: i64,
    pub files_modified: i64,
    pub files_deleted: i64,
    pub chunks_created: i64,
    pub errors: Vec<String>,
}

impl ScanResult {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "mountPointId": self.mount_point_id,
            "filesScanned": self.files_scanned,
            "filesNew": self.files_new,
            "filesModified": self.files_modified,
            "filesDeleted": self.files_deleted,
            "chunksCreated": self.chunks_created,
            "errors": self.errors,
        })
    }
}

/// v4 `ProcessFileOutcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessFileOutcome {
    Unchanged,
    Unsupported,
    Empty,
    New { chunks_created: i64 },
    Modified { chunks_created: i64 },
}

/// v4 scanner `detectFileType` — the NARROW variant (no json/jsonl; the scan
/// deliberately skips them, unlike `reindex_file::detect_file_type_full`).
pub fn detect_file_type(file_path: &str) -> Option<&'static str> {
    match path_extname(file_path).to_lowercase().as_str() {
        ".pdf" => Some("pdf"),
        ".docx" => Some("docx"),
        ".md" | ".markdown" => Some("markdown"),
        ".txt" => Some("txt"),
        _ => None,
    }
}

/// Hex SHA-256 of raw bytes (v4 `computeSha256` reads the file; callers here
/// pass the bytes).
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// `stat.mtime.toISOString()` — millisecond-precision ISO like V8's.
pub(crate) fn mtime_iso(meta: &std::fs::Metadata) -> String {
    let ms = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    crate::clock::iso_from_unix_ms(ms)
}

/// v4 `walkDirectory`: recursive, symlinks skipped, exclude patterns tested on
/// every entry (dirs prune the recursion), include patterns on files (empty =
/// include all). Returns `(relative, absolute)` pairs. Directory order follows
/// `fs.readdir` (normalized downstream by the differentials).
fn walk_directory(
    base_path: &str,
    include_patterns: &[String],
    exclude_patterns: &[String],
    current_relative: &str,
    results: &mut Vec<(String, String)>,
) {
    let current_absolute = if current_relative.is_empty() {
        base_path.to_string()
    } else {
        format!("{base_path}/{current_relative}")
    };
    let entries = match std::fs::read_dir(&current_absolute) {
        Ok(e) => e,
        Err(_) => return, // v4 logs + returns [] on unreadable dirs.
    };
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        // Skip symlinks entirely.
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative_path = if current_relative.is_empty() {
            name.clone()
        } else {
            format!("{current_relative}/{name}")
        };
        if exclude_patterns
            .iter()
            .any(|p| matches_pattern(&relative_path, p))
        {
            continue;
        }
        if file_type.is_dir() {
            walk_directory(
                base_path,
                include_patterns,
                exclude_patterns,
                &relative_path,
                results,
            );
        } else if file_type.is_file()
            && (include_patterns.is_empty()
                || include_patterns
                    .iter()
                    .any(|p| matches_pattern(&relative_path, p)))
        {
            let absolute = format!("{base_path}/{relative_path}");
            results.push((relative_path, absolute));
        }
    }
}

/// v4 `processMountFile` — hash, compare, convert, chunk, persist. Idempotent:
/// an unchanged sha returns `Unchanged` without touching the database.
pub fn process_mount_file(
    conn: &Connection,
    mount_point: &MountServiceInfo,
    absolute_path: &str,
    relative_path: &str,
    extractor: &dyn DocumentTextExtractor,
) -> Result<ProcessFileOutcome, DbError> {
    let Some(file_type) = detect_file_type(relative_path) else {
        return Ok(ProcessFileOutcome::Unsupported);
    };

    let bytes = std::fs::read(absolute_path).map_err(|e| DbError::Key(e.to_string()))?;
    let sha256 = sha256_hex(&bytes);

    let links = DocMountFileLinksRepository::new(conn);
    let existing = links.find_by_mount_point_and_path(&mount_point.id, relative_path)?;

    if let Some(ex) = &existing {
        if ex.sha256 == sha256 {
            return Ok(ProcessFileOutcome::Unchanged);
        }
    }

    let meta = std::fs::metadata(absolute_path).map_err(|e| DbError::Key(e.to_string()))?;

    let plain_text = convert_to_plain_text(absolute_path, file_type, extractor);
    if crate::jsstr::js_trim(&plain_text).is_empty() {
        return Ok(ProcessFileOutcome::Empty);
    }

    let chunks = chunk_document(&plain_text, ChunkOptions::default());

    // Per-document policy lives in the markdown frontmatter, which
    // convert_to_plain_text has already stripped — parse it from the raw bytes.
    // Non-markdown files keep the permissive defaults.
    let mut policy = DEFAULT_DOCUMENT_POLICY;
    if file_type == "markdown" {
        policy = String::from_utf8(bytes.clone())
            .map(|raw| policy_from_content(&raw))
            .unwrap_or(DEFAULT_DOCUMENT_POLICY);
    }

    if let Some(ex) = &existing {
        links.delete_chunks_by_link_id(&ex.id)?;
    }

    // Upsert the (file, link) pair — linkFilesystemFile finds-or-creates the
    // content row by sha (keeping its UUID stable across rewrites) and
    // (re)points the link at the new content.
    let link_id = links.link_filesystem_file(&LinkFilesystemFileInput {
        mount_point_id: mount_point.id.clone(),
        relative_path: relative_path.to_string(),
        file_name: node_basename(relative_path).to_string(),
        file_type: file_type.to_string(),
        sha256,
        file_size_bytes: meta.len() as f64,
        last_modified: mtime_iso(&meta),
        source: None,
        conversion_status: Some("converted".to_string()),
        conversion_error: None,
        plain_text_length: Some(crate::jsstr::utf16_len(&plain_text) as f64),
        chunk_count: Some(chunks.len() as f64),
        allow_embed: Some(policy.embed),
        allow_character_read: Some(policy.character_read),
        allow_character_write: Some(policy.character_write),
    })?;

    if !chunks.is_empty() {
        insert_chunks(conn, &link_id, &mount_point.id, &chunks)?;
    }

    Ok(if existing.is_some() {
        ProcessFileOutcome::Modified {
            chunks_created: chunks.len() as i64,
        }
    } else {
        ProcessFileOutcome::New {
            chunks_created: chunks.len() as i64,
        }
    })
}

/// v4 `removeMountFile` — unlink (with GC) one indexed path. `false` when no
/// record existed.
pub fn remove_mount_file(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<bool, DbError> {
    let links = DocMountFileLinksRepository::new(conn);
    let Some(existing) = links.find_by_mount_point_and_path(mount_point_id, relative_path)? else {
        return Ok(false);
    };
    links.delete_with_gc(&existing.id)?;
    Ok(true)
}

/// v4 `updateMountPointTotals` — recompute the aggregate rollups from the link
/// view (Σ chunkCount, Σ fileSizeBytes) and stamp `lastScannedAt`/idle.
pub fn update_mount_point_totals(conn: &Connection, mount_point_id: &str) -> Result<(), DbError> {
    let files = DocMountFilesRepository::new(conn)
        .find_links_with_content_json_by_mount_point_id(mount_point_id)?;
    let total_chunks: f64 = files
        .iter()
        .map(|f| f.get("chunkCount").and_then(|v| v.as_f64()).unwrap_or(0.0))
        .sum();
    let total_size: f64 = files
        .iter()
        .map(|f| {
            f.get("fileSizeBytes")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
        })
        .sum();
    DocMountPointsRepository::new(conn).update_last_scanned(
        mount_point_id,
        files.len() as f64,
        total_chunks,
        total_size,
    )
}

/// v4 `scanMountPoint` — the full scan. Database-backed mounts delegate to
/// `rescan_database_mount_point` (re-chunk under-indexed documents); fs mounts
/// walk the disk. Errors are accumulated in the result, never thrown (v4's
/// catch-all sets scanStatus 'error').
pub fn scan_mount_point(
    conn: &Connection,
    mount_point: &MountServiceInfo,
    extractor: &dyn DocumentTextExtractor,
) -> ScanResult {
    let mut result = ScanResult {
        mount_point_id: mount_point.id.clone(),
        files_scanned: 0,
        files_new: 0,
        files_modified: 0,
        files_deleted: 0,
        chunks_created: 0,
        errors: Vec::new(),
    };
    let points = DocMountPointsRepository::new(conn);

    if mount_point.mount_type == "database" {
        let _ = points.update_scan_status(&mount_point.id, "scanning", None);
        match rescan_database_mount_point(conn, mount_point, extractor) {
            Ok(count) => {
                result.files_scanned = count;
                result.files_modified = count;
                if let Err(e) = update_mount_point_totals(conn, &mount_point.id) {
                    result.errors.push(format!("Rescan failed: {e}"));
                    let _ =
                        points.update_scan_status(&mount_point.id, "error", Some(&e.to_string()));
                }
            }
            Err(e) => {
                let msg = e.to_string();
                let _ = points.update_scan_status(&mount_point.id, "error", Some(&msg));
                result.errors.push(format!("Rescan failed: {msg}"));
            }
        }
        return result;
    }

    let _ = points.update_scan_status(&mount_point.id, "scanning", None);

    let scan = (|| -> Result<(), DbError> {
        let base = mount_point.base_path.as_deref().unwrap_or("");
        let mut files_on_disk = Vec::new();
        walk_directory(
            base,
            &mount_point.include_patterns,
            &mount_point.exclude_patterns,
            "",
            &mut files_on_disk,
        );
        result.files_scanned = files_on_disk.len() as i64;

        let existing_files = DocMountFilesRepository::new(conn)
            .find_links_with_content_json_by_mount_point_id(&mount_point.id)?;
        let existing_paths: Vec<(String, String)> = existing_files
            .iter()
            .filter_map(|f| {
                Some((
                    f.get("relativePath")?.as_str()?.to_string(),
                    f.get("id")?.as_str()?.to_string(),
                ))
            })
            .collect();

        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for (relative_path, absolute_path) in &files_on_disk {
            seen.insert(relative_path.as_str());
            match process_mount_file(conn, mount_point, absolute_path, relative_path, extractor) {
                Ok(ProcessFileOutcome::Unsupported)
                | Ok(ProcessFileOutcome::Unchanged)
                | Ok(ProcessFileOutcome::Empty) => {}
                Ok(ProcessFileOutcome::New { chunks_created }) => {
                    result.files_new += 1;
                    result.chunks_created += chunks_created;
                }
                Ok(ProcessFileOutcome::Modified { chunks_created }) => {
                    result.files_modified += 1;
                    result.chunks_created += chunks_created;
                }
                Err(e) => {
                    let msg = e.to_string();
                    result.errors.push(format!("{relative_path}: {msg}"));
                    // Best-effort: mark the link failed when one exists.
                    if let Some((_, link_id)) =
                        existing_paths.iter().find(|(p, _)| p == relative_path)
                    {
                        let _ = DocMountFileLinksRepository::new(conn).update(
                            link_id,
                            &LinkUpdate {
                                conversion_status: Some("failed".to_string()),
                                conversion_error: Some(Some(msg)),
                                updated_at: crate::clock::now_iso(),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }

        // Deleted files: indexed but no longer on disk.
        for (existing_path, _) in &existing_paths {
            if !seen.contains(existing_path.as_str())
                && remove_mount_file(conn, &mount_point.id, existing_path)?
            {
                result.files_deleted += 1;
            }
        }

        update_mount_point_totals(conn, &mount_point.id)?;
        Ok(())
    })();

    if let Err(e) = scan {
        let msg = e.to_string();
        let _ = points.update_scan_status(&mount_point.id, "error", Some(&msg));
        result.errors.push(format!("Scan failed: {msg}"));
    }

    result
}

/// v4 `rescanDatabaseMountPoint` (`database-store.ts:618`) — walk the mount's
/// non-blob links and `reindexSingleFile` any that are under-chunked or not
/// yet converted. Returns the document-link count.
pub fn rescan_database_mount_point(
    conn: &Connection,
    mount_point: &MountServiceInfo,
    extractor: &dyn DocumentTextExtractor,
) -> Result<i64, DbError> {
    if mount_point.mount_type != "database" {
        return Err(DbError::Key(
            "rescanDatabaseMountPoint called on non-database mount point".to_string(),
        ));
    }
    let links = DocMountFileLinksRepository::new(conn).find_by_mount_point_id(&mount_point.id)?;
    let doc_links: Vec<_> = links
        .into_iter()
        .filter(|l| l.file_type != "blob")
        .collect();
    for link in &doc_links {
        let needs_rechunk = link.chunk_count == 0 || link.conversion_status != "converted";
        if needs_rechunk {
            // reindex_single_file is itself best-effort (v4 catches + warns).
            reindex_single_file(conn, &mount_point.id, &link.relative_path, "", extractor);
        }
        // v4 emits document-written per link (the watcher seam) — the event
        // emitter is not a ported surface; the refresh scheduler covers it.
    }
    Ok(doc_links.len() as i64)
}

/// v4 `verifyBasePath` — is the path accessible?
pub fn verify_base_path(base_path: &str) -> bool {
    std::path::Path::new(base_path).exists()
}

/// v4 `createFilesystemFolder` (scanner.ts:496) — mkdir -p inside the mount,
/// refusing targets that escape `basePath` (the traversal guard; the folders
/// route maps the "escapes" message to a 400).
pub fn create_filesystem_folder(
    base_path: &str,
    _mount_point_id: &str,
    relative_path: &str,
) -> Result<(), String> {
    use std::path::{Component, PathBuf};
    // path.resolve(basePath, relativePath) — normalize `.`/`..` lexically the
    // way Node does (no symlink resolution).
    let mut resolved = PathBuf::from(base_path);
    for comp in std::path::Path::new(relative_path).components() {
        match comp {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::CurDir => {}
            other => resolved.push(other),
        }
    }
    let base_resolved = PathBuf::from(base_path);
    let base_str = base_resolved.to_string_lossy().to_string();
    let target_str = resolved.to_string_lossy().to_string();
    let base_with_sep = if base_str.ends_with('/') {
        base_str.clone()
    } else {
        format!("{base_str}/")
    };
    if !(target_str == base_str || target_str.starts_with(&base_with_sep)) {
        return Err("Folder path escapes mount point boundary".to_string());
    }
    std::fs::create_dir_all(&resolved).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_file_type_is_the_narrow_scan_variant() {
        assert_eq!(detect_file_type("a/b.md"), Some("markdown"));
        assert_eq!(detect_file_type("a/b.markdown"), Some("markdown"));
        assert_eq!(detect_file_type("x.txt"), Some("txt"));
        assert_eq!(detect_file_type("x.pdf"), Some("pdf"));
        assert_eq!(detect_file_type("x.docx"), Some("docx"));
        // json/jsonl are deliberately NOT scanned (reindex-file's wide variant
        // handles them for database stores).
        assert_eq!(detect_file_type("x.json"), None);
        assert_eq!(detect_file_type("x.jsonl"), None);
        assert_eq!(detect_file_type("x.bin"), None);
    }

    #[test]
    fn create_filesystem_folder_guards_escapes() {
        let tmp = std::env::temp_dir().join(format!("qt-scan-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let base = tmp.to_str().unwrap();
        create_filesystem_folder(base, "m1", "a/b").unwrap();
        assert!(tmp.join("a/b").is_dir());
        let err = create_filesystem_folder(base, "m1", "../escape").unwrap_err();
        assert!(err.contains("escapes"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
