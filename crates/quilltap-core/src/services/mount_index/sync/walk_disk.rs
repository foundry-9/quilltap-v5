//! One read of the target directory, reduced to the same [`SyncEntry`] shape as
//! the store walk.
//!
//! Port of v4 `lib/mount-index/sync/walk-disk.ts` (`23da0b322`).
//!
//! Three rules do most of the work here:
//!
//!   - **Dot-entries are invisible.** A file or folder whose name begins with
//!     `.` is never read, never pushed, and never deleted — which covers
//!     `.DS_Store`, `.git`, the editor's swap files, and the verb's own
//!     manifest. The store walk applies the same rule, so the invisibility is
//!     symmetric: nothing in the store with a dot-path is written out either.
//!   - **Sidecars are not entries.** `<file>.description.md` is collected
//!     separately and attached to its partner's `description`. A sidecar with no
//!     partner is a warning.
//!   - **Text-native files are hashed as bytes**, exactly as the store hashes
//!     them, so the two sides' shas are comparable without decoding anything. A
//!     `.md` that is not valid UTF-8 cannot be held verbatim by a database store
//!     and is reported rather than mangled.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::sidecar::{is_sidecar_path, parse_sidecar, partner_path_for};
use super::types::{sha256_hex, EntryKind, SyncEntry, SyncEntryMap, DISK_TEMP_SUFFIX};
use crate::clock::iso_from_unix_ms;
use crate::services::mount_index::list::matches_pattern;
use crate::services::mount_index::path_utils::detect_native_text;

pub struct DiskWalkResult {
    pub entries: SyncEntryMap,
    pub warnings: Vec<String>,
    /// Disk paths whose own name ends in the sidecar suffix but match nothing.
    pub orphan_sidecars: Vec<String>,
    /// Disk paths that could not be read, or that a database store cannot hold.
    pub unreadable: Vec<String>,
}

struct PendingSidecar {
    partner_key: String,
    relative_path: String,
    text: String,
    mtime: String,
}

struct Walker<'a> {
    target_path: &'a Path,
    exclude_patterns: &'a [String],
    entries: SyncEntryMap,
    warnings: Vec<String>,
    orphan_sidecars: Vec<String>,
    unreadable: Vec<String>,
    sidecars: Vec<PendingSidecar>,
    /// Lower-cased key → the casing already claimed, so a collision is
    /// detectable.
    claimed: HashMap<String, String>,
}

pub fn walk_disk(target_path: &Path, exclude_patterns: &[String]) -> DiskWalkResult {
    let mut walker = Walker {
        target_path,
        exclude_patterns,
        entries: SyncEntryMap::new(),
        warnings: Vec::new(),
        orphan_sidecars: Vec::new(),
        unreadable: Vec::new(),
        sidecars: Vec::new(),
        claimed: HashMap::new(),
    };
    walker.walk("");

    // Attach every sidecar to its partner. One that matches nothing is reported
    // rather than deleted — it may belong to a file the operator is about to add.
    let sidecars = std::mem::take(&mut walker.sidecars);
    let sidecar_count = sidecars.len();
    for sidecar in sidecars {
        match walker.entries.get_mut(&sidecar.partner_key) {
            None => {
                walker.orphan_sidecars.push(sidecar.relative_path.clone());
                walker.warnings.push(format!(
                    "{} describes a file that is not here",
                    sidecar.relative_path
                ));
            }
            Some(partner) => {
                partner.description = Some(sidecar.text);
                partner.description_updated_at = Some(sidecar.mtime);
            }
        }
    }

    tracing::debug!(
        target: "quilltap::mount_index",
        target_path = %target_path.display(),
        entries = walker.entries.len(),
        sidecars = sidecar_count,
        orphan_sidecars = walker.orphan_sidecars.len(),
        unreadable = walker.unreadable.len(),
        "[Sync] Disk walk complete",
    );

    DiskWalkResult {
        entries: walker.entries,
        warnings: walker.warnings,
        orphan_sidecars: walker.orphan_sidecars,
        unreadable: walker.unreadable,
    }
}

impl Walker<'_> {
    fn walk(&mut self, relative_dir: &str) {
        let absolute_dir: PathBuf = if relative_dir.is_empty() {
            self.target_path.to_path_buf()
        } else {
            self.target_path.join(relative_dir)
        };

        let dirents = match read_dir_sorted(&absolute_dir) {
            Ok(d) => d,
            Err(e) => {
                // A target that does not exist yet is not an error — it is an
                // empty side, which is exactly what `--dry-run` against a new
                // path should see. (A real run has already created it by this
                // point.)
                if relative_dir.is_empty() && e.kind() == std::io::ErrorKind::NotFound {
                    return;
                }
                let shown = if relative_dir.is_empty() {
                    ".".to_string()
                } else {
                    relative_dir.to_string()
                };
                self.unreadable.push(shown);
                self.warnings.push(format!(
                    "Could not read {}: {}",
                    if relative_dir.is_empty() {
                        "the target directory"
                    } else {
                        relative_dir
                    },
                    node_error_message(&e, "scandir", &absolute_dir),
                ));
                return;
            }
        };

        for (name, file_type) in dirents {
            // Dot-entries — and their whole subtree — are as good as absent.
            if name.starts_with('.') {
                continue;
            }
            // A previous run's interrupted write, not a document.
            if name.ends_with(DISK_TEMP_SUFFIX) {
                continue;
            }

            let relative_path = if relative_dir.is_empty() {
                name.clone()
            } else {
                format!("{relative_dir}/{name}")
            };

            if self
                .exclude_patterns
                .iter()
                .any(|pattern| matches_pattern(&relative_path, pattern))
            {
                continue;
            }

            // A symlink is neither followed nor copied: resolving it would let a
            // link inside the target pull bytes from anywhere on the host into
            // the store, and copying it would put the wrong thing on the other
            // side.
            if file_type.is_symlink() {
                self.warnings
                    .push(format!("{relative_path} is a symbolic link and is skipped"));
                continue;
            }

            let absolute_path = self.target_path.join(&relative_path);
            let key = relative_path.to_lowercase();

            if file_type.is_dir() {
                let Some(stat) = stat_or_none(&absolute_path) else {
                    self.unreadable.push(relative_path);
                    continue;
                };
                if self.note_collision(&key, &relative_path) {
                    continue;
                }
                self.entries.insert(
                    key,
                    SyncEntry {
                        relative_path: relative_path.clone(),
                        kind: Some(EntryKind::Folder),
                        last_modified: iso_from_unix_ms(stat.mtime_ms),
                        created_at: birthtime_of(&stat),
                        ..SyncEntry::default()
                    },
                );
                self.walk(&relative_path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let Some(stat) = stat_or_none(&absolute_path) else {
                self.unreadable.push(relative_path);
                continue;
            };

            let bytes = match std::fs::read(&absolute_path) {
                Ok(b) => b,
                Err(e) => {
                    self.unreadable.push(relative_path.clone());
                    self.warnings.push(format!(
                        "Could not read {relative_path}: {}",
                        node_error_message(&e, "open", &absolute_path)
                    ));
                    continue;
                }
            };

            if is_sidecar_path(&relative_path) {
                if let Some(partner) = partner_path_for(&relative_path) {
                    self.sidecars.push(PendingSidecar {
                        partner_key: partner.to_lowercase(),
                        relative_path: relative_path.clone(),
                        // v4 `bytes.toString('utf-8')` — lossy, so invalid bytes
                        // become U+FFFD rather than failing the read.
                        text: parse_sidecar(&String::from_utf8_lossy(&bytes)),
                        mtime: iso_from_unix_ms(stat.mtime_ms),
                    });
                }
                continue;
            }

            // A database store holds text-native content as a string, so a `.md`
            // that is not valid UTF-8 could not be stored verbatim — and a sync
            // that is not byte-preserving never converges.
            if detect_native_text(&relative_path).is_some() && !is_valid_utf8(&bytes) {
                self.unreadable.push(relative_path.clone());
                self.warnings.push(format!(
                    "{relative_path} has a text extension but is not valid UTF-8; \
                     a database store cannot hold it verbatim"
                ));
                continue;
            }

            if self.note_collision(&key, &relative_path) {
                continue;
            }

            self.entries.insert(
                key,
                SyncEntry {
                    relative_path: relative_path.clone(),
                    kind: Some(EntryKind::File),
                    sha256: Some(sha256_hex(&bytes)),
                    size_bytes: Some(bytes.len() as i64),
                    last_modified: iso_from_unix_ms(stat.mtime_ms),
                    created_at: birthtime_of(&stat),
                    ..SyncEntry::default()
                },
            );
        }
    }

    /// On a case-sensitive filesystem the target may hold `Notes.md` and
    /// `notes.md` side by side. The store's index is NOCASE and cannot, so there
    /// is no answer the sync could apply — both are refused, loudly.
    fn note_collision(&mut self, key: &str, relative_path: &str) -> bool {
        match self.claimed.get(key) {
            None => {
                self.claimed
                    .insert(key.to_string(), relative_path.to_string());
                false
            }
            Some(already) => {
                self.warnings.push(format!(
                    "{relative_path} and {already} differ only by case; \
                     a database store cannot hold both, so neither is synced"
                ));
                self.entries.remove(key);
                true
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Node's `fs.readdir(dir, { withFileTypes: true })`.
///
/// libuv's `uv_fs_scandir` sorts every directory's entries with `strcmp` on
/// `d_name`; Rust's `read_dir` yields the raw syscall order (APFS hands back a
/// hash order). The walk order is load-bearing here — it is the disk map's
/// insertion order, which is the planner's key order, which breaks ties in the
/// plan's STABLE sort — so the sort is applied per directory, on the name bytes.
fn read_dir_sorted(dir: &Path) -> std::io::Result<Vec<(String, std::fs::FileType)>> {
    let mut out: Vec<(String, std::fs::FileType)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        out.push((entry.file_name().to_string_lossy().into_owned(), file_type));
    }
    out.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    Ok(out)
}

/// The fields v4 reads off a `Stats`.
///
/// `mtime_ms` is `new Date(mtimeMs)` — truncated to whole milliseconds, which is
/// what `toISOString()` then renders. `birthtime_ms` and `ctime_ms` are kept as
/// FLOATS with sub-millisecond precision, because Node's are and because
/// [`birthtime_of`] compares them against each other with a tolerance of one
/// millisecond. Truncating the birthtime first turns a pair that is 0.002 ms
/// apart into a pair 1.001 ms apart, and the guard then answers the opposite —
/// which is exactly the disagreement `sync_engine_equivalence` caught on a
/// freshly created directory (`nested/a-disk-tree-is-adopted-whole`: v4 reported
/// no creation date for `x`, v5 reported one).
struct Stat {
    mtime_ms: i64,
    birthtime_ms: f64,
    ctime_ms: f64,
}

fn stat_or_none(absolute_path: &Path) -> Option<Stat> {
    let meta = std::fs::metadata(absolute_path).ok()?;
    Some(stat_from_metadata(&meta))
}

#[cfg(unix)]
fn stat_from_metadata(meta: &std::fs::Metadata) -> Stat {
    use std::os::unix::fs::MetadataExt;
    let mtime_ms = meta.mtime() * 1000 + i64::from(meta.mtime_nsec() as i32) / 1_000_000;
    // Node reports `birthtimeMs` as 0 where the platform has no birthtime, and
    // as a sub-millisecond float where it has one.
    let birthtime_ms = meta
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    let ctime_ms = meta.ctime() as f64 * 1000.0 + meta.ctime_nsec() as f64 / 1_000_000.0;
    Stat {
        mtime_ms,
        birthtime_ms,
        ctime_ms,
    }
}

#[cfg(not(unix))]
fn stat_from_metadata(meta: &std::fs::Metadata) -> Stat {
    let ms = |t: std::io::Result<std::time::SystemTime>| -> f64 {
        t.ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    };
    let mtime_ms = ms(meta.modified());
    Stat {
        mtime_ms: mtime_ms as i64,
        birthtime_ms: ms(meta.created()),
        ctime_ms: mtime_ms,
    }
}

/// v4 `birthtimeOf` — the creation date, where the platform keeps one.
///
/// Linux reports `birthtime` as the epoch (or as `ctime`, on filesystems that
/// fake it) rather than admitting it does not know; either answer is worse than
/// none, because it would make every first run plan a `touch` that cannot
/// succeed. The manifest carries the real value in those cases.
fn birthtime_of(stat: &Stat) -> Option<String> {
    let birth = stat.birthtime_ms;
    // v4 `if (!birth || birth <= 0)` (`walk-disk.ts:200`) — 0 and negative both
    // fall out, and the `!birth` half ALSO catches NaN, which is falsy in JS
    // while `NaN <= 0` is false. `is_finite` is the Rust spelling of that half
    // (it also drops ±∞, which `<= 0` would let through as a positive).
    if !birth.is_finite() || birth <= 0.0 {
        return None;
    }
    // The comparison is between the two FLOATS, as Node's is.
    if (birth - stat.ctime_ms).abs() < 1.0 {
        return None;
    }
    // …and only the RENDERING truncates: `new Date(birthtimeMs)` does
    // `ToInteger`, which for a positive value is a truncation toward zero.
    Some(iso_from_unix_ms(birth as i64))
}

/// v4 `isValidUtf8` — round-trips through UTF-8 unchanged.
fn is_valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// v4 reports `err.message` from a Node `fs` error, which is
/// `"<code>: <syscall-message>, <syscall> '<path>'"`.
fn node_error_message(e: &std::io::Error, syscall: &str, path: &Path) -> String {
    let (code, text) = match e.kind() {
        std::io::ErrorKind::NotFound => ("ENOENT", "no such file or directory"),
        std::io::ErrorKind::PermissionDenied => ("EACCES", "permission denied"),
        std::io::ErrorKind::NotADirectory => ("ENOTDIR", "not a directory"),
        _ => ("EIO", "i/o error"),
    };
    format!("{code}: {text}, {syscall} \'{}\'", path.display())
}
