//! Cloud placeholder detection — the port of v4's `lib/files/cloud-placeholder.ts`
//! (v4 `bf7a457f6`, P4.D381): the one place that decides, from a file's
//! `stat`, whether a cloud-sync provider (iCloud Drive, OneDrive Files
//! On-Demand) has evicted it to a placeholder.
//!
//! Reading a placeholder makes the provider download it. That is what v4's
//! boot step in `lib/startup/materialize-cloud-files.ts` wants for the database
//! files, and exactly what the document-store scanner must not do: a store
//! pointed at an iCloud-synced `~/Documents` faulted in tens of thousands of
//! files one at a time and held v4's startup gate for its whole five-minute cap.
//!
//! Pure and dependency-free. v4's module is ported whole (R-P6): the scanner
//! (`scanner::process_mount_file`) is the only v5 caller of
//! [`is_evicted_cloud_placeholder`]; [`is_not_fully_materialized`] has NO v5
//! caller, because v4's boot materialize step is NO-COUNTERPART in v5 — it is
//! kept `pub` so the tier-1 family (`cloud_placeholder_equivalence`) covers the
//! module whole.
//!
//! ## `PLACEHOLDER_CHECK_UNIX_ONLY` (R-3 — a ruled divergence)
//!
//! v4 reads `fs.Stats.blocks` on every platform: libuv reports the allocation
//! size on Windows, so OneDrive placeholders read `blocks === 0` there. Rust's
//! `std::fs::Metadata` exposes `blocks()` only through the unix
//! `MetadataExt`, so v5's ONE production reader [`allocation_stat`] returns
//! `None` off unix and Windows never detects a placeholder — the scanner reads
//! the file (which downloads it), where v4 leaves it `in-cloud`. The
//! divergence is the one row of [`PLACEHOLDER_CHECK_UNIX_ONLY`]. Re-open
//! trigger: Tauri-Windows — read `GetFileAttributesEx` and test
//! `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`.

/// Block size that `fs.Stats.blocks` is denominated in (POSIX `st_blocks`).
pub const STAT_BLOCK_BYTES: u64 = 512;

/// The two `stat` fields both rules read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocationStat {
    pub size: u64,
    pub blocks: u64,
}

/// The `process.platform` values v4's rule distinguishes. `Other` stands for
/// every platform v4 treats as "no placeholders" (`linux`, `freebsd`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Darwin,
    Win32,
    Linux,
    Other,
}

impl Platform {
    /// The build's platform — v4's `process.platform` default argument.
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Darwin
        } else if cfg!(target_os = "windows") {
            Platform::Win32
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            Platform::Other
        }
    }
}

/// A real file (`size > 0`) whose allocated blocks hold fewer bytes than its
/// size has not finished downloading. Covers both cloud states with one test:
///   - fully evicted placeholder → `blocks == 0` (mirrors macOS SF_DATALESS)
///   - partially materialized     → `blocks > 0` but `blocks * 512 < size`
///
/// Broad on purpose: it is the "must fault this in before SQLite opens it"
/// rule. A filesystem that compresses transparently (APFS decmpfs, ZFS, btrfs)
/// can also report fewer blocks than bytes, which costs that caller only a
/// redundant read. Never use it to decide what to skip — that is
/// [`is_evicted_cloud_placeholder`].
pub fn is_not_fully_materialized(stat: AllocationStat) -> bool {
    // v4 multiplies JS numbers; saturate so a hostile `blocks` cannot wrap.
    stat.size > 0 && stat.blocks.saturating_mul(STAT_BLOCK_BYTES) < stat.size
}

/// True when the file is a fully evicted cloud placeholder: it has content
/// (`size > 0`) and no local allocation at all (`blocks == 0`).
///
/// Narrow on purpose: it decides what a scan leaves alone, so a false positive
/// silently drops a document from the index. A partially downloaded file is
/// already in flight, and a compressed one has blocks, so neither is caught.
/// Only macOS (iCloud Drive) and Windows (OneDrive, where libuv reports the
/// allocation size) have placeholders; elsewhere this is always false.
pub fn is_evicted_cloud_placeholder(stat: AllocationStat, platform: Platform) -> bool {
    if platform != Platform::Darwin && platform != Platform::Win32 {
        return false;
    }
    stat.size > 0 && stat.blocks == 0
}

/// The named divergence table (R-3): the one `(platform, stat)` shape where v4
/// and v5 disagree. Read by [`allocation_stat`]'s doc and pinned by a
/// source-text guard so a non-unix arm cannot land without updating it.
pub const PLACEHOLDER_CHECK_UNIX_ONLY: &[&str] =
    &["win32 + blocks 0 + size > 0: v4 reads `in-cloud`; v5 reads the file (no allocation stat off unix)"];

/// The ONE production reader of a file's allocation (`PLACEHOLDER_CHECK_UNIX_ONLY`).
/// On unix it is `st_size` / `st_blocks`; off unix it is `None`, so the scanner
/// never treats a file as a placeholder there (R-3; re-open on Tauri-Windows).
#[cfg(unix)]
pub(crate) fn allocation_stat(meta: &std::fs::Metadata) -> Option<AllocationStat> {
    use std::os::unix::fs::MetadataExt;
    Some(AllocationStat {
        size: meta.size(),
        blocks: meta.blocks(),
    })
}

/// The non-unix arm of [`allocation_stat`] — see `PLACEHOLDER_CHECK_UNIX_ONLY`.
#[cfg(not(unix))]
pub(crate) fn allocation_stat(_meta: &std::fs::Metadata) -> Option<AllocationStat> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(size: u64, blocks: u64) -> AllocationStat {
        AllocationStat { size, blocks }
    }

    // v4's ten `cloud-placeholder.test.ts` vectors.
    #[test]
    fn evicted_vectors() {
        assert!(is_evicted_cloud_placeholder(
            st(118035, 0),
            Platform::Darwin
        ));
        assert!(is_evicted_cloud_placeholder(st(4013, 0), Platform::Win32));
        assert!(!is_evicted_cloud_placeholder(
            st(80241, 160),
            Platform::Darwin
        ));
        assert!(!is_evicted_cloud_placeholder(
            st(100_000, 8),
            Platform::Darwin
        ));
        assert!(!is_evicted_cloud_placeholder(st(0, 0), Platform::Darwin));
        assert!(!is_evicted_cloud_placeholder(st(4096, 0), Platform::Linux));
    }

    #[test]
    fn materialized_vectors() {
        assert!(is_not_fully_materialized(st(4096, 0)));
        assert!(is_not_fully_materialized(st(100_000, 8)));
        assert!(!is_not_fully_materialized(st(1000, 8)));
        assert!(!is_not_fully_materialized(st(0, 0)));
    }

    #[cfg(unix)]
    #[test]
    fn allocation_stat_reads_a_resident_file() {
        let dir = std::env::temp_dir().join(format!("qt-cloud-ph-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.txt");
        std::fs::write(&p, vec![b'x'; 5000]).unwrap();
        let s = allocation_stat(&std::fs::metadata(&p).unwrap()).unwrap();
        assert_eq!(s.size, 5000);
        assert!(s.blocks > 0, "a freshly written file is resident");
        assert!(!is_evicted_cloud_placeholder(s, Platform::current()));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The source-text guard on `PLACEHOLDER_CHECK_UNIX_ONLY`: the non-unix arm
    /// must stay the `None` arm the table records. Adding real detection off
    /// unix without retiring the table row reds here.
    #[test]
    fn non_unix_arm_matches_the_divergence_table() {
        let src = include_str!("cloud_placeholder.rs");
        let arm = "#[cfg(not(unix))]\npub(crate) fn allocation_stat(_meta: &std::fs::Metadata) -> Option<AllocationStat> {\n    None\n}";
        assert!(
            src.contains(arm),
            "the non-unix allocation_stat arm changed: update PLACEHOLDER_CHECK_UNIX_ONLY (R-3) with it"
        );
        assert_eq!(PLACEHOLDER_CHECK_UNIX_ONLY.len(), 1);
        assert!(PLACEHOLDER_CHECK_UNIX_ONLY[0].starts_with("win32 + blocks 0"));
    }
}
