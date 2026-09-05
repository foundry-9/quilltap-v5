//! The local disk storage backend (P4.1b) — v4
//! `lib/file-storage/backends/local/index.ts` + its `retry.ts`, implementing
//! the core [`StorageBackend`] seam over the legacy `<base>/files/**` layout —
//! plus the help-doc disk walker (v4 `lib/help/help-doc-sync.ts`
//! `findMarkdownFiles`, the host half of the core sync's injected file list).

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use quilltap_core::services::file_storage::StorageBackend;
use quilltap_core::services::help_doc_sync::HelpSourceFile;

// ============================================================================
// withFsRetry (v4 backends/local/retry.ts)
// ============================================================================

/// v4 `RETRY_BACKOFF_MS`.
const RETRY_BACKOFF_MS: &[u64] = &[50, 150, 400, 800, 1500];

/// v4's transient-error set: EAGAIN / EBUSY / EWOULDBLOCK / EINTR / EDEADLK
/// (symbolic + the numeric errnos libuv reports — the Docker-on-iCloud
/// VirtioFS case). Rust surfaces these as raw OS errors; `WouldBlock` /
/// `Interrupted` cover the kind-mapped ones.
fn is_transient_fs_error(err: &io::Error) -> bool {
    if matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    ) {
        return true;
    }
    // Numeric errnos across macOS/Linux: EINTR(4), EAGAIN/EDEADLK(11/35 by
    // platform), EBUSY(16) — the union of v4's TRANSIENT_FS_ERRNOS.
    matches!(err.raw_os_error(), Some(4 | 11 | 16 | 35))
}

/// v4 `withFsRetry` — retry a filesystem op on transient errors with the fixed
/// backoff schedule; non-transient errors (ENOENT, EACCES, …) rethrow
/// immediately so callers keying on codes keep working.
fn with_fs_retry<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut last: Option<io::Error> = None;
    for attempt in 0..=RETRY_BACKOFF_MS.len() {
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if !is_transient_fs_error(&e) {
                    return Err(e);
                }
                last = Some(e);
                if let Some(ms) = RETRY_BACKOFF_MS.get(attempt) {
                    std::thread::sleep(Duration::from_millis(*ms));
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("exhausted fs retries")))
}

// ============================================================================
// LocalStorageBackend (v4 LocalFileStorageBackend)
// ============================================================================

/// The local filesystem backend: files stored under `base_path` (v4:
/// `getFilesDir()` = `<base>/files`), with tilde expansion, the
/// directory-traversal guard, ENOENT-tolerant deletes (+ the legacy
/// `.meta.json` sidecar unlink), and the transient-error retry.
pub struct LocalStorageBackend {
    base_path: PathBuf,
}

/// Node `path.normalize` for a POSIX-relative key: collapse separators,
/// resolve `.` / `..` lexically (leading `..`s survive for relative paths).
fn node_normalize(key: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in key.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if matches!(stack.last(), Some(&s) if s != "..") {
                    stack.pop();
                } else {
                    stack.push("..");
                }
            }
            s => stack.push(s),
        }
    }
    if stack.is_empty() {
        ".".to_string()
    } else {
        stack.join("/")
    }
}

impl LocalStorageBackend {
    /// v4 constructor: tilde-expand + normalize the base path.
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        let raw: PathBuf = base_path.into();
        let s = raw.to_string_lossy();
        let expanded = if s == "~" {
            std::env::home_dir().unwrap_or(raw.clone())
        } else if let Some(rest) = s.strip_prefix("~/") {
            std::env::home_dir()
                .map(|h| h.join(rest))
                .unwrap_or(raw.clone())
        } else {
            raw.clone()
        };
        LocalStorageBackend {
            base_path: expanded,
        }
    }

    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    /// v4 `buildSafePath` (`local/index.ts:121`): normalize the key, strip any
    /// remaining `..` TEXT (v4's `normalize(key).replace(/\.\./g, '')` — note
    /// it also eats `..` inside names like `a..b` → `ab`, faithfully kept),
    /// join under the base, and verify the result stays under the base.
    fn build_safe_path(&self, key: &str) -> Result<PathBuf, String> {
        let normalized = node_normalize(key).replace("..", "");
        // Node `path.join(base, k)` CONCATENATES (an absolute k does not
        // replace the base as Rust's `Path::join` would).
        let relative = normalized.trim_start_matches('/');
        let full = self.base_path.join(relative);
        // The traversal guard (post-strip nothing can escape, but keep v4's
        // final check).
        if !full.starts_with(&self.base_path) {
            return Err(format!(
                "Invalid file path: would escape base directory. Key: {key}"
            ));
        }
        Ok(full)
    }
}

impl StorageBackend for LocalStorageBackend {
    /// v4 `upload`: ensure the parent directory, write the bytes (no sidecar —
    /// metadata lives in the DB; `content_type` is carried for interface
    /// parity only).
    fn upload(&self, key: &str, content: &[u8], _content_type: &str) -> Result<(), String> {
        let path = self.build_safe_path(key)?;
        let run = || -> io::Result<()> {
            if let Some(dir) = path.parent() {
                with_fs_retry(|| std::fs::create_dir_all(dir))?;
            }
            with_fs_retry(|| std::fs::write(&path, content))?;
            Ok(())
        };
        run().map_err(|e| format!("Failed to upload file '{key}': {e}"))
    }

    fn download(&self, key: &str) -> Result<Vec<u8>, String> {
        let path = self.build_safe_path(key)?;
        with_fs_retry(|| std::fs::read(&path)).map_err(|e| {
            // [Bug 55, v4 `d553f72a`] Nothing at that path is a "gone"
            // condition, not a read fault — the caller needs to tell the two
            // apart to answer 404 instead of 500. v4 throws
            // `FileContentMissingError(key, "No file at storage key '<key>'")`
            // here; the trait carries `String`, so the marker does
            // (`quilltap_core::services::file_storage::CONTENT_MISSING_SENTINEL`).
            if e.kind() == io::ErrorKind::NotFound {
                tracing::warn!(key = %key, "Download found no file at the storage key");
                return quilltap_core::services::file_storage::content_missing(format!(
                    "No file at storage key '{key}'"
                ));
            }
            format!("Failed to download file '{key}': {e}")
        })
    }

    /// v4 `delete`: idempotent (ENOENT succeeds) + the legacy `.meta.json`
    /// sidecar unlink (best-effort).
    fn delete(&self, key: &str) -> Result<(), String> {
        let path = self.build_safe_path(key)?;
        match with_fs_retry(|| std::fs::remove_file(&path)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Failed to delete file '{key}': {e}")),
        }
        // Legacy sidecar cleanup — failures never throw.
        let mut sidecar = path.into_os_string();
        sidecar.push(".meta.json");
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool, String> {
        let path = self.build_safe_path(key)?;
        match with_fs_retry(|| std::fs::metadata(&path)) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("Failed to check if file '{key}' exists: {e}")),
        }
    }

    /// v4 `LocalFileStorageBackend.getMetadata().capabilities.list` = true.
    fn supports_list(&self) -> bool {
        true
    }

    /// v4 `LocalFileStorageBackend.list` (`local/index.ts:492`): recurse the
    /// prefix directory, skipping hidden files (`.`-prefixed) and legacy
    /// `.meta.json` sidecars, returning `{currentPrefix}/{name}` keys. A missing
    /// prefix directory (ENOENT) yields the keys collected so far rather than
    /// throwing; any other read error propagates.
    fn list(&self, prefix: &str, max_keys: Option<usize>) -> Result<Vec<String>, String> {
        let prefix_path = self.build_safe_path(prefix)?;
        let start_prefix = prefix.trim_end_matches('/').to_string();
        let mut results: Vec<String> = Vec::new();
        list_dir(&prefix_path, &start_prefix, max_keys, &mut results)
            .map_err(|e| format!("Failed to list files with prefix '{prefix}': {e}"))?;
        Ok(results)
    }
}

/// Recursive helper for [`LocalStorageBackend::list`] — v4's inner `listDir`.
fn list_dir(
    dir_path: &Path,
    current_prefix: &str,
    max_keys: Option<usize>,
    results: &mut Vec<String>,
) -> io::Result<()> {
    if max_keys.is_some_and(|m| results.len() >= m) {
        return Ok(());
    }
    let entries = match with_fs_retry(|| std::fs::read_dir(dir_path)) {
        Ok(rd) => rd,
        // A missing / inaccessible directory is not an error (v4 returns).
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        if max_keys.is_some_and(|m| results.len() >= m) {
            break;
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Skip hidden files and legacy .meta.json sidecars (v4).
        if name.starts_with('.') || name.ends_with(".meta.json") {
            continue;
        }
        let full_key = if current_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{current_prefix}/{name}")
        };
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            list_dir(&entry.path(), &full_key, max_keys, results)?;
        } else {
            results.push(full_key);
        }
    }
    Ok(())
}

// ============================================================================
// The help-doc disk walker (v4 help-doc-sync.ts findMarkdownFiles)
// ============================================================================

/// v4 `findMarkdownFiles` — recurse `dir` in `readdirSync` order (files and
/// directories interleaved, subdirectory contents inlined at the directory's
/// position). A directory read error yields the entries collected so far for
/// that directory (v4's try/catch around the whole loop).
///
/// ## The order is SORTED, not the raw syscall order (P4.9I2A correction)
///
/// Node's `fs.readdirSync` goes through libuv's `uv_fs_scandir`, which sorts
/// every directory's entries with `strcmp` on `d_name` (`src/unix/fs.c`,
/// `uv__fs_scandir_sort`) — so v4 walks each directory in **byte order of the
/// name**, on every platform. Rust's `read_dir` is the raw syscall order (APFS
/// hands back a hash order; ext4 an htree order), and this walker used to
/// return THAT, under a doc comment claiming the two agreed. No differential
/// could see it: the `help-sync-*` families compare the changed docs as SORTED
/// paths precisely because "walk order is readdir-dependent". The order became
/// load-bearing with the shipped tree (P4.9I2A): it is the `help_docs` rowid
/// order, hence `findAll`'s order, the Guide's list order, and the help-chat
/// context resolver's "first match wins" — and the content oracle
/// (`help_tree_equivalence`) compares it. Measured on the vendored tree: Node
/// `readdirSync('help')` is sorted, `ls -f` / Python `listdir` are not.
///
/// `sort_unstable_by` over the UTF-8 name bytes IS `strcmp` on `d_name` (both
/// compare unsigned bytes; UTF-8 byte order equals code-point order). The
/// build script (`build.rs`) mirrors this function verbatim.
pub fn find_markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    // libuv `scandir` collects the whole directory, then sorts by `strcmp`.
    let mut collected: Vec<std::fs::DirEntry> = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            // v4: a read error aborts this directory's loop (caught) — with
            // libuv's collect-then-sort, a failed scandir yields NO entries.
            return files;
        };
        collected.push(entry);
    }
    collected.sort_unstable_by(|a, b| {
        a.file_name()
            .as_encoded_bytes()
            .cmp(b.file_name().as_encoded_bytes())
    });
    for entry in collected {
        let path = entry.path();
        let Ok(meta) = std::fs::metadata(&path) else {
            break;
        };
        if meta.is_dir() {
            files.extend(find_markdown_files(&path));
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with(".md"))
            .unwrap_or(false)
        {
            files.push(path);
        }
    }
    files
}

/// Read the help tree under `cwd/help` into the core sync's input list —
/// `rel_path` is v4's `relative(process.cwd(), filePath)` (`help/...`), the
/// content read as UTF-8 (lossy, matching Node's `readFileSync(_, 'utf-8')`
/// replacement behavior). A missing `help/` directory yields an empty list
/// (v4's `existsSync` guard).
pub fn load_help_source_files(cwd: &Path) -> Vec<HelpSourceFile> {
    let help_dir = cwd.join("help");
    if !help_dir.is_dir() {
        return Vec::new();
    }
    find_markdown_files(&help_dir)
        .into_iter()
        .filter_map(|path| {
            let rel = path.strip_prefix(cwd).ok()?.to_string_lossy().to_string();
            let bytes = std::fs::read(&path).ok()?;
            Some(HelpSourceFile {
                rel_path: rel,
                raw_content: String::from_utf8_lossy(&bytes).to_string(),
            })
        })
        .collect()
}

/// The help tree the BINARY ships (P4.9I2A) — the compile-time table
/// `help_content::EMBEDDED_HELP` (`build.rs`) as the core sync's input list.
/// `rel_path` is v4's `relative(process.cwd(), filePath)` (`help/<name>.md`);
/// the order is the runtime walker's ([`find_markdown_files`] mirrored in the
/// build script). This is the ONE source both the boot-time
/// `ensure_help_docs_synced` and the `EMBEDDING_REINDEX_ALL` handler read, so
/// the two cannot disagree. [`load_help_source_files`] keeps the filesystem
/// walk for the differential fixture trees (`help-sync-*`, `help-ensure`),
/// which are NOT the shipped tree.
pub fn embedded_help_source_files() -> Vec<HelpSourceFile> {
    crate::help_content::EMBEDDED_HELP
        .iter()
        .map(|(rel_path, content)| HelpSourceFile {
            rel_path: (*rel_path).to_string(),
            raw_content: (*content).to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> (LocalStorageBackend, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (LocalStorageBackend::new(dir.path()), dir)
    }

    #[test]
    fn roundtrip_upload_download_exists_delete() {
        let (b, _dir) = backend();
        let key = "p1/folder/My File.png";
        assert!(!b.exists(key).unwrap());
        b.upload(key, b"bytes!", "image/png").unwrap();
        assert!(b.exists(key).unwrap());
        assert_eq!(b.download(key).unwrap(), b"bytes!");
        b.delete(key).unwrap();
        assert!(!b.exists(key).unwrap());
        // Idempotent delete.
        b.delete(key).unwrap();
    }

    #[test]
    fn delete_unlinks_legacy_sidecar() {
        let (b, dir) = backend();
        b.upload("a/x.bin", b"x", "application/octet-stream")
            .unwrap();
        let sidecar = dir.path().join("a/x.bin.meta.json");
        std::fs::write(&sidecar, b"{}").unwrap();
        b.delete("a/x.bin").unwrap();
        assert!(!sidecar.exists());
    }

    #[test]
    fn traversal_guard() {
        let (b, dir) = backend();
        // `..` segments are normalized away then text-stripped — the write
        // lands INSIDE the base, never at the escape target.
        b.upload("../escape.txt", b"x", "text/plain").unwrap();
        assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
        // An absolute key concatenates under the base (Node join semantics).
        b.upload("/abs.txt", b"x", "text/plain").unwrap();
        assert!(dir.path().join("abs.txt").exists());
        // `..` inside a NAME is eaten too (v4's `.replace(/\.\./g,'')`).
        b.upload("a..b.txt", b"x", "text/plain").unwrap();
        assert!(dir.path().join("ab.txt").exists());
    }

    #[test]
    fn node_normalize_shapes() {
        assert_eq!(node_normalize("a/./b//c"), "a/b/c");
        assert_eq!(node_normalize("a/../b"), "b");
        assert_eq!(node_normalize("../a"), "../a");
        assert_eq!(node_normalize(""), ".");
    }

    #[test]
    fn help_walker_reads_tree() {
        let dir = tempfile::tempdir().unwrap();
        let help = dir.path().join("help");
        std::fs::create_dir_all(help.join("nested")).unwrap();
        std::fs::write(help.join("a.md"), "# A").unwrap();
        std::fs::write(help.join("skip.txt"), "no").unwrap();
        std::fs::write(help.join("nested/b.md"), "# B").unwrap();
        let files = load_help_source_files(dir.path());
        let mut paths: Vec<&str> = files.iter().map(|f| f.rel_path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["help/a.md", "help/nested/b.md"]);
        // P4.9I2A: the ORDER is Node's `readdirSync` order — each directory's
        // entries sorted by name bytes, a subdirectory's contents inlined at its
        // position. Written in a scrambled creation order so a raw-syscall walk
        // (APFS hash order) cannot pass by luck.
        std::fs::write(help.join("z.md"), "# Z").unwrap();
        std::fs::write(help.join("m.md"), "# M").unwrap();
        std::fs::create_dir_all(help.join("b-dir")).unwrap();
        std::fs::write(help.join("b-dir/y.md"), "# Y").unwrap();
        std::fs::write(help.join("b-dir/a.md"), "# A2").unwrap();
        let ordered: Vec<String> = load_help_source_files(dir.path())
            .into_iter()
            .map(|f| f.rel_path)
            .collect();
        assert_eq!(
            ordered,
            vec![
                "help/a.md",
                "help/b-dir/a.md",
                "help/b-dir/y.md",
                "help/m.md",
                "help/nested/b.md",
                "help/z.md",
            ]
        );
        // Missing help dir → empty.
        let empty = tempfile::tempdir().unwrap();
        assert!(load_help_source_files(empty.path()).is_empty());
    }
}
