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

use super::base_path_availability::{assert_base_path_available, BasePathUnavailableError};
use super::chunker::{chunk_document, ChunkOptions};
use super::cloud_placeholder::{
    allocation_stat, is_evicted_cloud_placeholder, AllocationStat, Platform,
};
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
    /// v4 `bf7a457f6`: files left alone because a cloud-sync provider has
    /// evicted them to a placeholder. Reading one would download it; the index
    /// keeps whatever it had and v4's watcher catches the file when it comes
    /// back (v5's watcher is the standing deferral).
    pub files_in_cloud: i64,
    pub chunks_created: i64,
    pub errors: Vec<String>,
}

impl ScanResult {
    /// v4 `emptyScanResult(mountPointId, errors = [])` — every count at zero,
    /// for callers that build one by hand.
    pub fn empty(mount_point_id: &str, errors: Vec<String>) -> ScanResult {
        ScanResult {
            mount_point_id: mount_point_id.to_string(),
            files_scanned: 0,
            files_new: 0,
            files_modified: 0,
            files_deleted: 0,
            files_in_cloud: 0,
            chunks_created: 0,
            errors,
        }
    }

    /// v4's `ScanResult` key order (`filesInCloud` between `filesDeleted` and
    /// `chunksCreated`).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "mountPointId": self.mount_point_id,
            "filesScanned": self.files_scanned,
            "filesNew": self.files_new,
            "filesModified": self.files_modified,
            "filesDeleted": self.files_deleted,
            "filesInCloud": self.files_in_cloud,
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
    /// v4 `{status: 'in-cloud'}` (`bf7a457f6`): an evicted cloud placeholder,
    /// returned BEFORE anything reads the file.
    InCloud,
    New {
        chunks_created: i64,
    },
    Modified {
        chunks_created: i64,
    },
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
        Err(e) => {
            // v4 `walkTree`'s `onReadError` (`scanner.ts:141-146`, P4.D381
            // Tier 2): WARN and skip the directory. `path` is the absolute
            // directory (`path.join(basePath, relativeDir)`, or the base).
            tracing::warn!(
                target: "quilltap::mount_index",
                path = %current_absolute,
                error = %e,
                "Unable to read directory during walk"
            );
            return;
        }
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

/// The test seam over a file's allocation (P4.D381 item 13): production reads
/// [`HostAllocationProbe`] (the one `allocation_stat` reader, unix-only —
/// `PLACEHOLDER_CHECK_UNIX_ONLY`); the harness injects `blocks = 0` for named
/// paths to reproduce an evicted placeholder on a resident file.
pub trait AllocationProbe {
    fn allocation(&self, absolute_path: &str, meta: &std::fs::Metadata) -> Option<AllocationStat>;
}

/// The production [`AllocationProbe`]: the host's own `stat`.
pub struct HostAllocationProbe;

impl AllocationProbe for HostAllocationProbe {
    fn allocation(&self, _absolute_path: &str, meta: &std::fs::Metadata) -> Option<AllocationStat> {
        allocation_stat(meta)
    }
}

/// v4 `processMountFile` — hash, compare, convert, chunk, persist. Idempotent:
/// an unchanged sha returns `Unchanged` without touching the database.
///
/// A file evicted to a cloud placeholder returns `InCloud` before anything
/// reads it — hashing it would make iCloud or OneDrive download it. Its index
/// entry, if any, is left as it was.
pub fn process_mount_file(
    conn: &Connection,
    mount_point: &MountServiceInfo,
    absolute_path: &str,
    relative_path: &str,
    extractor: &dyn DocumentTextExtractor,
) -> Result<ProcessFileOutcome, DbError> {
    process_mount_file_with_probe(
        conn,
        mount_point,
        absolute_path,
        relative_path,
        extractor,
        &HostAllocationProbe,
    )
}

/// [`process_mount_file`] over an injected [`AllocationProbe`] (the
/// frozen-seam sibling idiom; production passes [`HostAllocationProbe`]).
pub fn process_mount_file_with_probe(
    conn: &Connection,
    mount_point: &MountServiceInfo,
    absolute_path: &str,
    relative_path: &str,
    extractor: &dyn DocumentTextExtractor,
    probe: &dyn AllocationProbe,
) -> Result<ProcessFileOutcome, DbError> {
    // v4 `bf7a457f6`'s order: the type check (no stat for an ignored type),
    // then the stat, then the placeholder check — all BEFORE the first read.
    let Some(file_type) = detect_file_type(relative_path) else {
        return Ok(ProcessFileOutcome::Unsupported);
    };

    let meta = std::fs::metadata(absolute_path).map_err(|e| DbError::Internal(e.to_string()))?;
    // No per-file log here (v4's why): a scan of an iCloud-synced folder meets
    // thousands of these and rolled the log over several times. The scan logs
    // one summary line.
    if probe
        .allocation(absolute_path, &meta)
        .is_some_and(|stat| is_evicted_cloud_placeholder(stat, Platform::current()))
    {
        return Ok(ProcessFileOutcome::InCloud);
    }

    let bytes = std::fs::read(absolute_path).map_err(|e| DbError::Internal(e.to_string()))?;
    let sha256 = sha256_hex(&bytes);

    let links = DocMountFileLinksRepository::new(conn);
    // v4 `scanner.ts:170-176`: the fallback path read — `null` → no
    // `unchanged` shortcut, no chunk delete, the upsert (P4.142 G2).
    let existing = links.find_by_mount_point_and_path_or_none(&mount_point.id, relative_path);

    if let Some(ex) = &existing {
        if ex.sha256 == sha256 {
            return Ok(ProcessFileOutcome::Unchanged);
        }
    }

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
    // v4 `scanner.ts:255-259`: the fallback path read — `null` → `false` (not
    // counted).
    let Some(existing) = links.find_by_mount_point_and_path_or_none(mount_point_id, relative_path)
    else {
        return Ok(false);
    };
    // P4.149 (5d #21, Ruling R-E): v4's `deleteWithGC` is a FALLBACK
    // (`scanner.ts:261`) and `removeMountFile` answers `true` regardless — a
    // swallowed delete is over-counted in `filesDeleted`, a reporting-only
    // divergence the next scan heals (the stale link is found again).
    let _ = links.delete_with_gc_or_false(&existing.id);
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
    scan_mount_point_with_probe(conn, mount_point, extractor, &HostAllocationProbe)
}

/// [`scan_mount_point`] over an injected [`AllocationProbe`] (the harness's
/// evicted-placeholder plant; production passes [`HostAllocationProbe`]).
///
/// Its lines (v4 service `MountIndex:Scanner` → target
/// `quilltap::mount_index`, the module family's shared target) run on the
/// WRITER thread when reached through `mount_scan` — a pin reads them through
/// a process-wide capture.
pub fn scan_mount_point_with_probe(
    conn: &Connection,
    mount_point: &MountServiceInfo,
    extractor: &dyn DocumentTextExtractor,
    probe: &dyn AllocationProbe,
) -> ScanResult {
    let mut result = ScanResult::empty(&mount_point.id, Vec::new());
    let points = DocMountPointsRepository::new(conn);

    if mount_point.mount_type == "database" {
        tracing::info!(
            target: "quilltap::mount_index",
            mountPointId = %mount_point.id,
            name = %mount_point.name,
            "Starting database-backed mount point rescan"
        );
        let _ = points.update_scan_status(&mount_point.id, "scanning", None);
        match rescan_database_mount_point(conn, mount_point, extractor) {
            Ok(count) => {
                result.files_scanned = count;
                result.files_modified = count;
                if let Err(e) = update_mount_point_totals(conn, &mount_point.id) {
                    let msg = e.to_string();
                    log_database_rescan_failed(&mount_point.id, &msg);
                    result.errors.push(format!("Rescan failed: {msg}"));
                    let _ = points.update_scan_status(&mount_point.id, "error", Some(&msg));
                }
            }
            Err(e) => {
                let msg = e.to_string();
                log_database_rescan_failed(&mount_point.id, &msg);
                let _ = points.update_scan_status(&mount_point.id, "error", Some(&msg));
                result.errors.push(format!("Rescan failed: {msg}"));
            }
        }
        return result;
    }

    // v4 logs `basePath` as the mount's value; an absent one is `undefined`
    // and OMITTED (§R.5) — `Option` renders nothing for `None`.
    tracing::info!(
        target: "quilltap::mount_index",
        mountPointId = %mount_point.id,
        basePath = mount_point.base_path.as_deref(),
        name = %mount_point.name,
        "Starting mount point scan"
    );

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
            // A cloud placeholder counts as seen, so its index entry survives
            // the stale sweep below (v4 `seenPaths.add` BEFORE processing).
            seen.insert(relative_path.as_str());
            match process_mount_file_with_probe(
                conn,
                mount_point,
                absolute_path,
                relative_path,
                extractor,
                probe,
            ) {
                Ok(ProcessFileOutcome::Unsupported)
                | Ok(ProcessFileOutcome::Unchanged)
                | Ok(ProcessFileOutcome::Empty) => {}
                Ok(ProcessFileOutcome::InCloud) => {
                    result.files_in_cloud += 1;
                }
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
                    tracing::warn!(
                        target: "quilltap::mount_index",
                        relativePath = %relative_path,
                        error = %msg,
                        "Error processing file during mount scan"
                    );
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

        tracing::info!(
            target: "quilltap::mount_index",
            mountPointId = %mount_point.id,
            filesScanned = result.files_scanned,
            filesNew = result.files_new,
            filesModified = result.files_modified,
            filesDeleted = result.files_deleted,
            filesInCloud = result.files_in_cloud,
            chunksCreated = result.chunks_created,
            errorCount = result.errors.len(),
            "Mount point scan completed"
        );
        if result.files_in_cloud > 0 {
            tracing::info!(
                target: "quilltap::mount_index",
                mountPointId = %mount_point.id,
                name = %mount_point.name,
                filesInCloud = result.files_in_cloud,
                "Left cloud-evicted files unread; they are indexed once downloaded"
            );
        }
        Ok(())
    })();

    if let Err(e) = scan {
        let msg = e.to_string();
        tracing::error!(
            target: "quilltap::mount_index",
            mountPointId = %mount_point.id,
            error = %msg,
            "Mount point scan failed"
        );
        let _ = points.update_scan_status(&mount_point.id, "error", Some(&msg));
        result.errors.push(format!("Scan failed: {msg}"));
    }

    result
}

/// v4 `scanner.ts:328-331`: ERROR `Database-backed rescan failed
/// {mountPointId, error}` (P4.D381 Tier 2).
fn log_database_rescan_failed(mount_point_id: &str, msg: &str) {
    tracing::error!(
        target: "quilltap::mount_index",
        mountPointId = %mount_point_id,
        error = %msg,
        "Database-backed rescan failed"
    );
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
        return Err(DbError::Internal(
            "rescanDatabaseMountPoint called on non-database mount point".to_string(),
        ));
    }
    // v4 `database-store.ts:639`: the fallback joined read — `[]` → a 0-document
    // rescan (P4.142 G3).
    let links =
        DocMountFileLinksRepository::new(conn).find_by_mount_point_id_or_empty(&mount_point.id);
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

/// How `create_filesystem_folder` can refuse. v4 throws three shapes and the
/// folders route discriminates on all three; a typed enum is how v5 says
/// `instanceof`.
#[derive(Debug, Clone)]
pub enum CreateFolderError {
    /// v4's `throw new Error('Folder path escapes mount point boundary')` —
    /// the traversal guard, which the route maps to a 400.
    Escapes,
    /// v4 `ed8934f1`: the store's own root is unreachable — a 409 carrying the
    /// diagnosis.
    BasePathUnavailable(BasePathUnavailableError),
    /// Anything the `mkdir -p` itself threw (the route's bare 500).
    Io(String),
}

/// v4 `createFilesystemFolder` (scanner.ts:496) — mkdir -p inside the mount,
/// refusing targets that escape `basePath` (the traversal guard; the folders
/// route maps the "escapes" message to a 400) and refusing outright when the
/// mount's own base path is unreachable (`ed8934f1`).
pub fn create_filesystem_folder(
    base_path: &str,
    _mount_point_id: &str,
    relative_path: &str,
) -> Result<(), CreateFolderError> {
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
        return Err(CreateFolderError::Escapes);
    }

    // The mkdir below is recursive, which is what makes this check load-bearing
    // rather than merely polite. Without it, a mount whose basePath is absent —
    // a store pointing at an unmounted volume, or at a host path that was never
    // bound into this container — sends mkdir walking all the way up to the
    // topmost missing ancestor and creating the entire chain. Where the process
    // has the privileges to do that, the folder is reported as created while
    // living in a fabricated tree that has nothing to do with the user's store.
    // Fail loudly against the absent base instead.
    assert_base_path_available(base_path).map_err(CreateFolderError::BasePathUnavailable)?;

    std::fs::create_dir_all(&resolved).map_err(|e| CreateFolderError::Io(e.to_string()))
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
        assert!(matches!(err, CreateFolderError::Escapes));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// [ed8934f1] The escape guard runs BEFORE the availability assert (v4's
    /// order), and an absent base refuses instead of `mkdir -p` fabricating the
    /// whole ancestor chain.
    #[test]
    fn create_filesystem_folder_refuses_an_absent_base() {
        let tmp = std::env::temp_dir().join(format!("qt-scan-absent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let base = tmp.join("never/created");
        let base = base.to_str().unwrap();

        let err = create_filesystem_folder(base, "m1", "a/b").unwrap_err();
        match err {
            CreateFolderError::BasePathUnavailable(e) => {
                assert_eq!(e.message, format!("The path '{base}' does not exist."));
            }
            other => panic!("expected BasePathUnavailable, got {other:?}"),
        }
        assert!(!tmp.exists(), "the absent base must not have been created");

        // The traversal guard still wins over the availability check.
        let err = create_filesystem_folder(base, "m1", "../escape").unwrap_err();
        assert!(matches!(err, CreateFolderError::Escapes));
    }
}

#[cfg(test)]
mod g2g3_fallback_tests {
    use super::*;
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A provisioned instance's mount-index writer, one of its database
    /// stores (service info), and `notes/a.md` written into it.
    fn store(
        dir: &tempfile::TempDir,
    ) -> (
        crate::db::Writer,
        crate::db::doc_mount_points::MountServiceInfo,
    ) {
        crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
        let w =
            crate::db::Writer::open_writable(&dir.path().join("quilltap-mount-index.db"), PEPPER)
                .unwrap();
        let id: String = w
            .connection()
            .query_row(
                "SELECT id FROM doc_mount_points WHERE mountType = 'database' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(w.connection())
            .write_database_document(&id, "notes/a.md", "hello")
            .unwrap();
        let info = crate::db::doc_mount_points::DocMountPointsRepository::new(w.connection())
            .find_service_info_by_id(&id)
            .unwrap()
            .unwrap();
        (w, info)
    }

    /// Breaks every joined link read (named by `queryJoined`, by no overlay read).
    fn plant_links(w: &crate::db::Writer) {
        w.connection()
            .execute_batch(
                "ALTER TABLE doc_mount_file_links RENAME COLUMN originalMimeType TO originalMimeType_x",
            )
            .unwrap();
    }

    const PATH_LINE: &str = "ERROR quilltap::db Error querying joined file links collection=doc_mount_file_links whereClause=WHERE l.mountPointId = ? AND LOWER(l.relativePath) = LOWER(?) error=no such column: l.originalMimeType";
    const ID_LINE: &str = "ERROR quilltap::db Error querying joined file links collection=doc_mount_file_links whereClause=WHERE l.mountPointId = ? error=no such column: l.originalMimeType";

    /// P4.142 G3: `removeMountFile`'s lookup (v4 `scanner.ts:255-259`) — a
    /// failed read is `null` → `false` (not counted), never an `Err`; the
    /// healthy lookup removes and is silent.
    #[test]
    fn remove_mount_file_falls_back_to_not_counted() {
        let dir = tempfile::tempdir().unwrap();
        let (w, mp) = store(&dir);
        let (got, lines) = crate::test_support::captured_with(|| {
            remove_mount_file(w.connection(), &mp.id, "notes/missing.md")
        });
        assert!(!got.unwrap());
        assert!(lines.is_empty(), "{lines:?}");
        plant_links(&w);
        let (got, lines) = crate::test_support::captured_with(|| {
            remove_mount_file(w.connection(), &mp.id, "notes/a.md")
        });
        assert!(!got.unwrap());
        assert_eq!(lines, vec![PATH_LINE.to_string()]);
    }

    /// P4.149 (5d #21, Ruling R-E): `removeMountFile`'s `deleteWithGC` is a
    /// FALLBACK (`scanner.ts:261`) and the function answers `true` regardless —
    /// a swallowed delete logs v4's line and is COUNTED (over-counted in
    /// `filesDeleted`; the next scan finds the link again).
    #[test]
    fn a_failed_remove_delete_logs_v4s_line_and_still_counts() {
        let dir = tempfile::tempdir().unwrap();
        let (w, mp) = store(&dir);
        w.connection()
            .execute_batch(
                "CREATE TRIGGER qt_no_link_delete BEFORE DELETE ON doc_mount_file_links \
                 BEGIN SELECT RAISE(ABORT, 'planted'); END",
            )
            .unwrap();
        let (got, lines) = crate::test_support::captured_with(|| {
            remove_mount_file(w.connection(), &mp.id, "notes/a.md")
        });
        assert!(got.unwrap(), "v4 answers true after a swallowed delete");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error deleting file link with GC collection=doc_mount_file_links linkId="
            ) && lines[0].ends_with(" error=planted"),
            "{}",
            lines[0]
        );
    }

    /// P4.142 G3: `rescanDatabaseMountPoint`'s link read (v4 `database-store.ts:
    /// 639`) — a failed read is `[]` → a 0-document rescan.
    #[test]
    fn rescan_falls_back_to_zero_documents() {
        let dir = tempfile::tempdir().unwrap();
        let (w, mp) = store(&dir);
        plant_links(&w);
        let extractor = super::super::converters::RefusingTextExtractor;
        let (got, lines) = crate::test_support::captured_with(|| {
            rescan_database_mount_point(w.connection(), &mp, &extractor)
        });
        assert_eq!(got.unwrap(), 0);
        assert_eq!(
            lines.first().map(String::as_str),
            Some(ID_LINE),
            "{lines:#?}"
        );
    }
}

// === P4.D381 (v4 bf7a457f6): the cloud-placeholder scan + the scanner lines ===
#[cfg(all(test, unix))]
mod p4d381_cloud_tests {
    use super::*;
    use crate::services::mount_index::cloud_placeholder::AllocationStat;
    use std::os::unix::fs::PermissionsExt;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// Reports `blocks: 0` (an evicted placeholder) for the named absolute
    /// paths, the host's own stat for every other.
    struct Evict(Vec<String>);
    impl AllocationProbe for Evict {
        fn allocation(&self, p: &str, meta: &std::fs::Metadata) -> Option<AllocationStat> {
            if self.0.iter().any(|x| x == p) {
                Some(AllocationStat {
                    size: meta.len(),
                    blocks: 0,
                })
            } else {
                HostAllocationProbe.allocation(p, meta)
            }
        }
    }

    /// Panics if consulted — "does not stat a file type the scanner ignores".
    struct NeverConsulted;
    impl AllocationProbe for NeverConsulted {
        fn allocation(&self, p: &str, _m: &std::fs::Metadata) -> Option<AllocationStat> {
            panic!("the probe was consulted for {p}");
        }
    }

    struct Fx {
        _dir: tempfile::TempDir,
        base: std::path::PathBuf,
        w: crate::db::Writer,
        fs_mp: MountServiceInfo,
        db_mp: MountServiceInfo,
    }

    /// A provisioned instance's mount-index writer with one FILESYSTEM store
    /// over `<tmp>/tree` holding `notes.md` and `plan.txt`, plus the
    /// provisioned database store.
    fn fixture() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
        let w =
            crate::db::Writer::open_writable(&dir.path().join("quilltap-mount-index.db"), PEPPER)
                .unwrap();
        let base = dir.path().join("tree");
        std::fs::create_dir_all(base.join("sub")).unwrap();
        std::fs::write(base.join("notes.md"), "# Notes\n\nResident text.\n").unwrap();
        std::fs::write(base.join("plan.txt"), "A plan, in plain text.\n").unwrap();
        std::fs::write(base.join("sub/deep.md"), "# Deep\n\nDown here.\n").unwrap();
        let db_id: String = w
            .connection()
            .query_row(
                "SELECT id FROM doc_mount_points WHERE mountType = 'database' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        w.connection()
            .execute_batch(&format!(
                "CREATE TEMP TABLE p4d381_mp AS SELECT * FROM doc_mount_points WHERE id = '{db_id}'; \
                 UPDATE p4d381_mp SET id = 'p4d381-fs', name = 'P4D381 FS', mountType = 'filesystem', \
                   basePath = '{}'; \
                 INSERT INTO doc_mount_points SELECT * FROM p4d381_mp;",
                base.display()
            ))
            .unwrap();
        let points = crate::db::doc_mount_points::DocMountPointsRepository::new(w.connection());
        let fs_mp = points
            .find_service_info_by_id("p4d381-fs")
            .unwrap()
            .unwrap();
        let db_mp = points.find_service_info_by_id(&db_id).unwrap().unwrap();
        Fx {
            _dir: dir,
            base,
            w,
            fs_mp,
            db_mp,
        }
    }

    fn abs(fx: &Fx, rel: &str) -> String {
        format!("{}/{rel}", fx.fs_mp.base_path.as_deref().unwrap())
    }

    fn link_count(fx: &Fx) -> i64 {
        fx.w.connection()
            .query_row(
                "SELECT COUNT(*) FROM doc_mount_file_links WHERE mountPointId = 'p4d381-fs'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn lines_with<'a>(lines: &'a [String], msg: &str) -> Vec<&'a String> {
        lines.iter().filter(|l| l.contains(msg)).collect()
    }

    // v4 `scanner-cloud-placeholder.test.ts` it 1: in-cloud, nothing read. The
    // file is ALSO mode 000, so any read would error.
    #[test]
    fn returns_in_cloud_without_reading_an_evicted_file() {
        let fx = fixture();
        let p = abs(&fx, "notes.md");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
        let ex = crate::services::mount_index::converters::default_text_extractor();
        let out = process_mount_file_with_probe(
            fx.w.connection(),
            &fx.fs_mp,
            &p,
            "notes.md",
            ex.as_ref(),
            &Evict(vec![p.clone()]),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(out, ProcessFileOutcome::InCloud);
        assert_eq!(link_count(&fx), 0, "an in-cloud file writes nothing");
    }

    // it 2: a resident file is hashed and indexed as before.
    #[test]
    fn hashes_a_resident_file_as_before() {
        let fx = fixture();
        let p = abs(&fx, "notes.md");
        let ex = crate::services::mount_index::converters::default_text_extractor();
        let out = process_mount_file_with_probe(
            fx.w.connection(),
            &fx.fs_mp,
            &p,
            "notes.md",
            ex.as_ref(),
            &Evict(vec![]),
        )
        .unwrap();
        assert!(matches!(out, ProcessFileOutcome::New { .. }), "{out:?}");
    }

    // it 3: an ignored type is never stat'd — the path does not even exist, so
    // a stat would error, and the probe panics if consulted.
    #[test]
    fn does_not_stat_a_file_type_the_scanner_ignores() {
        let fx = fixture();
        let ex = crate::services::mount_index::converters::default_text_extractor();
        let out = process_mount_file_with_probe(
            fx.w.connection(),
            &fx.fs_mp,
            &abs(&fx, "song.m4a"),
            "song.m4a",
            ex.as_ref(),
            &NeverConsulted,
        )
        .unwrap();
        assert_eq!(out, ProcessFileOutcome::Unsupported);
    }

    // it 4 (scanMountPoint): evicted files are counted, kept in the index, and
    // never deleted — plus the summary lines (and the silence leg).
    #[test]
    fn counts_evicted_files_and_keeps_their_index_entries() {
        let fx = fixture();
        let ex = crate::services::mount_index::converters::default_text_extractor();
        let (first, lines) = crate::test_support::captured_with(|| {
            scan_mount_point(fx.w.connection(), &fx.fs_mp, ex.as_ref())
        });
        assert_eq!((first.files_new, first.files_in_cloud), (3, 0));
        assert_eq!(link_count(&fx), 3);
        let start = format!(
            "INFO quilltap::mount_index Starting mount point scan mountPointId=p4d381-fs basePath={} name=P4D381 FS",
            fx.base.display()
        );
        assert_eq!(
            lines_with(&lines, "Starting mount point scan"),
            vec![&start]
        );
        assert_eq!(
            lines_with(&lines, "Mount point scan completed"),
            vec!["INFO quilltap::mount_index Mount point scan completed mountPointId=p4d381-fs filesScanned=3 filesNew=3 filesModified=0 filesDeleted=0 filesInCloud=0 chunksCreated=3 errorCount=0"]
        );
        assert!(
            lines_with(&lines, "Left cloud-evicted files unread").is_empty(),
            "the silence leg: no in-cloud file, no line"
        );

        let probe = Evict(vec![abs(&fx, "notes.md"), abs(&fx, "plan.txt")]);
        let (second, lines) = crate::test_support::captured_with(|| {
            scan_mount_point_with_probe(fx.w.connection(), &fx.fs_mp, ex.as_ref(), &probe)
        });
        assert_eq!(second.files_scanned, 3);
        assert_eq!(second.files_in_cloud, 2);
        assert_eq!(second.files_deleted, 0);
        assert!(second.errors.is_empty(), "{:?}", second.errors);
        assert_eq!(link_count(&fx), 3, "the evicted files' entries survive");
        assert_eq!(
            lines_with(&lines, "Left cloud-evicted files unread"),
            vec!["INFO quilltap::mount_index Left cloud-evicted files unread; they are indexed once downloaded mountPointId=p4d381-fs name=P4D381 FS filesInCloud=2"]
        );
        let json = second.to_json().to_string();
        assert!(
            json.contains(r#""filesDeleted":0,"filesInCloud":2,"chunksCreated":0"#),
            "v4's key order: {json}"
        );
    }

    // Tier 2 item 16: the per-file WARN (an unreadable, resident file) and the
    // walk WARN (an unreadable directory). Silence: the healthy scan above logs
    // neither.
    #[test]
    fn the_per_file_and_walk_warns() {
        let fx = fixture();
        let ex = crate::services::mount_index::converters::default_text_extractor();
        let p = abs(&fx, "notes.md");
        let sub = fx.base.join("sub");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).unwrap();
        let (r, lines) = crate::test_support::captured_with(|| {
            scan_mount_point(fx.w.connection(), &fx.fs_mp, ex.as_ref())
        });
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
        let warn = lines_with(&lines, "Error processing file during mount scan");
        assert_eq!(warn.len(), 1, "{lines:?}");
        assert!(warn[0].starts_with(
            "WARN quilltap::mount_index Error processing file during mount scan relativePath=notes.md error="
        ));
        let walk = lines_with(&lines, "Unable to read directory during walk");
        assert_eq!(walk.len(), 1, "{lines:?}");
        assert!(walk[0].starts_with(&format!(
            "WARN quilltap::mount_index Unable to read directory during walk path={} error=",
            sub.display()
        )));
    }

    // Tier 2 item 16: the database-backed rescan's INFO, its failure ERROR, and
    // the filesystem scan's failure ERROR (a broken `doc_mount_files` column
    // fails the totals read / the existing-files read).
    #[test]
    fn the_rescan_and_scan_failure_lines() {
        let fx = fixture();
        let ex = crate::services::mount_index::converters::default_text_extractor();
        let (_, lines) = crate::test_support::captured_with(|| {
            scan_mount_point(fx.w.connection(), &fx.db_mp, ex.as_ref())
        });
        assert_eq!(
            lines_with(&lines, "Starting database-backed mount point rescan"),
            vec![&format!(
                "INFO quilltap::mount_index Starting database-backed mount point rescan mountPointId={} name={}",
                fx.db_mp.id, fx.db_mp.name
            )]
        );
        assert!(
            lines_with(&lines, "rescan failed").is_empty(),
            "silence leg"
        );
        assert!(lines_with(&lines, "Mount point scan failed").is_empty());

        fx.w.connection()
            .execute_batch(
                "ALTER TABLE doc_mount_files RENAME COLUMN fileSizeBytes TO fileSizeBytes_x",
            )
            .unwrap();
        let (r, lines) = crate::test_support::captured_with(|| {
            scan_mount_point(fx.w.connection(), &fx.db_mp, ex.as_ref())
        });
        assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
        let failed = lines_with(&lines, "Database-backed rescan failed");
        assert_eq!(failed.len(), 1, "{lines:?}");
        assert!(failed[0].starts_with(&format!(
            "ERROR quilltap::mount_index Database-backed rescan failed mountPointId={} error=",
            fx.db_mp.id
        )));

        let (r, lines) = crate::test_support::captured_with(|| {
            scan_mount_point(fx.w.connection(), &fx.fs_mp, ex.as_ref())
        });
        assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
        assert!(r.errors[0].starts_with("Scan failed: "), "{:?}", r.errors);
        let failed = lines_with(&lines, "Mount point scan failed");
        assert_eq!(failed.len(), 1, "{lines:?}");
        assert!(failed[0].starts_with(
            "ERROR quilltap::mount_index Mount point scan failed mountPointId=p4d381-fs error="
        ));
        assert!(lines_with(&lines, "Mount point scan completed").is_empty());
    }

    #[test]
    fn empty_is_every_count_at_zero() {
        let r = ScanResult::empty("mp-1", vec!["boom".into()]);
        assert_eq!(
            r.to_json().to_string(),
            r#"{"mountPointId":"mp-1","filesScanned":0,"filesNew":0,"filesModified":0,"filesDeleted":0,"filesInCloud":0,"chunksCreated":0,"errors":["boom"]}"#
        );
    }
}
// === /P4.D381 ===
