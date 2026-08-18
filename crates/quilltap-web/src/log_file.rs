//! # The file log transport (P4.49) — `combined.log`, `error.log`, rotation
//!
//! A port of v4's `lib/logging/transports/file.ts`. Writes one JSON record per
//! line to `combined.log` (everything) and `error.log` (errors only). When an
//! active file would exceed `max_file_size` it rotates into `<stem>.0.log`
//! (newest backup) through `<stem>.<max_files-1>.log` (oldest).
//!
//! ## Why this is a checked port when P4.18 said logs are not
//!
//! P4.18 ruled log **records** operator output rather than data — no
//! differential applies to *which* events v5 emits, their wording, or their
//! field names, and that ruling stands. It does not extend to the file
//! behavior. The file names, the rotation sequence, the sweep's allowlist
//! predicate, the JSON envelope's key set and the "errors go to both files"
//! rule are a contract with the operator and with their existing greps — v4
//! pins them with 36 executable cases (`__tests__/unit/logging-transports.
//! test.ts`), and this module mirrors the file-transport half of that corpus
//! 1:1 (the way an SPA lane mirrors v4's component tests when the surface has
//! no NDJSON oracle).
//!
//! So: **checked** — file names, rotation order and naming, the sweep
//! predicate in both directions, size accounting + the rotate-before-append
//! threshold, the envelope keys, the error-level fan-out.
//! **Not checked, deliberately** — the content of `context`.
//!
//! ## Divergences from v4, recorded
//!
//! - v4's transport is async (`fs.promises`); v5's is synchronous, because a
//!   `tracing` layer's `on_event` is synchronous. The fmt layer writes to
//!   stderr the same way. One mutex serializes size accounting, rotation and
//!   the append so two threads cannot interleave a record.
//! - v4's `error.name` is the JS constructor name. Rust's `%e`-rendered errors
//!   carry no runtime type name, so a recorded `error` field becomes
//!   `{"name": "Error", "message": …}` with no `stack` (v4 marks `stack`
//!   optional). Envelope *keys* stay v4's; the value is what Rust can know.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

/// v4's `STEMS = ['combined', 'error'] as const`.
pub(crate) const STEMS: [&str; 2] = ["combined", "error"];

/// v4's `DEFAULT` transport ceiling: 10 MB (`file.ts:63`).
pub const DEFAULT_MAX_FILE_SIZE: u64 = 10_485_760;

/// v4's default backup count: 10 → `.0.log` through `.9.log` (`file.ts:64`).
pub const DEFAULT_MAX_FILES: usize = 10;

/// v4's `activeLogName`.
fn active_log_name(stem: &str) -> String {
    format!("{stem}.log")
}

/// v4's `rotatedLogName`.
fn rotated_log_name(stem: &str, rotation: usize) -> String {
    format!("{stem}.{rotation}.log")
}

/// v4's `belongsToStemFamily`: true if `entry` looks like it belongs to a log
/// stem's family — either an exact match for the stem (`combined`), or the stem
/// followed by a **non-alphanumeric** separator (`.`, ` `, `(`, `-`). Returning
/// true means the entry is a candidate for the allowlist check; words like
/// `errors.log` or `embedded-server.log` fall through and are never touched.
///
/// v4 indexes `entry[stem.length]` — a UTF-16 code unit — and tests it against
/// `/[a-zA-Z0-9]/`. The stems are ASCII, so the byte at that offset is the same
/// character; a multi-byte follower yields a non-ASCII byte here and a
/// non-alphanumeric code unit (or a lone surrogate) there. Both say "belongs".
pub(crate) fn belongs_to_stem_family(entry: &str) -> bool {
    for stem in STEMS {
        if !entry.starts_with(stem) {
            continue;
        }
        if entry.len() == stem.len() {
            return true;
        }
        let next = entry.as_bytes()[stem.len()];
        if !next.is_ascii_alphanumeric() {
            return true;
        }
    }
    false
}

/// The five levels of v4's `LogLevel` enum, in v4's lowercase spelling.
/// The numeric ordering (`error` 0 … `trace` 4) belongs to v4's `Logger`, which
/// v5 does not port — `RUST_LOG` is v5's level filter (P4.18).
pub fn level_name(level: &tracing::Level) -> &'static str {
    match *level {
        tracing::Level::ERROR => "error",
        tracing::Level::WARN => "warn",
        tracing::Level::INFO => "info",
        tracing::Level::DEBUG => "debug",
        tracing::Level::TRACE => "trace",
    }
}

/// One record in v4's envelope: `{timestamp, level, message, context, error?}`
/// (`lib/logging/transports/base.ts`). `context` is always present, even empty;
/// `error` is omitted entirely when absent (v4 sets it `undefined`, which
/// `JSON.stringify` drops).
///
/// The rendered line is `JSON.stringify(logData) + '\n'` — v4's key order is
/// literal object order, which `serde_json`'s `preserve_order` reproduces.
pub fn render_line(
    timestamp: &str,
    level: &str,
    message: &str,
    context: Map<String, Value>,
    error: Option<Value>,
) -> String {
    let mut record = Map::new();
    record.insert("timestamp".to_string(), json!(timestamp));
    record.insert("level".to_string(), json!(level));
    record.insert("message".to_string(), json!(message));
    record.insert("context".to_string(), Value::Object(context));
    if let Some(err) = error {
        record.insert("error".to_string(), err);
    }
    let mut line = Value::Object(record).to_string();
    line.push('\n');
    line
}

/// The mutable half of the transport, behind one lock: v4's
/// `fileSizes: Map<Stem, number>`. The lock also serializes the append and the
/// rotation, which v4 gets for free from its single-threaded event loop.
#[derive(Default)]
struct Sizes {
    by_stem: BTreeMap<&'static str, u64>,
}

/// v4's `FileTransport`.
pub struct LogFileWriter {
    log_dir: PathBuf,
    max_file_size: u64,
    max_files: usize,
    sizes: Mutex<Sizes>,
}

impl LogFileWriter {
    /// v4's constructor + `initializeDirectory()`: `mkdir -p`, sweep the stray
    /// logs, then `stat` each stem to seed its running size (missing → 0).
    ///
    /// v4 swallows an initialization failure with a `console.error`; so does
    /// this, on stderr, because the transport failing must not take the process
    /// with it (and a `tracing` event here would re-enter the layer).
    pub fn new(log_dir: PathBuf, max_file_size: u64, max_files: usize) -> Self {
        let writer = Self {
            log_dir,
            max_file_size,
            max_files,
            sizes: Mutex::new(Sizes::default()),
        };
        writer.initialize_directory();
        writer
    }

    fn initialize_directory(&self) {
        if let Err(e) = fs::create_dir_all(&self.log_dir) {
            eprintln!("Failed to initialize logging directory: {e}");
            return;
        }
        self.purge_stray_logs();

        let mut sizes = self.lock_sizes();
        for stem in STEMS {
            let size = fs::metadata(self.log_dir.join(active_log_name(stem)))
                .map(|m| m.len())
                .unwrap_or(0);
            sizes.by_stem.insert(stem, size);
        }
    }

    /// v4's `purgeStrayLogs`: delete directory entries that are NOT allowlisted
    /// AND belong to a stem family. Everything else — terminal transcripts,
    /// `quilltap-stdout.log`, `startup.log`, `embedded-server.log`, `logs/
    /// terminals/` — is user-owned and left alone.
    ///
    /// Every step is best-effort: a `readdir` failure returns, an `unlink`
    /// failure is ignored ("sync-locked or already gone is fine"). v4 unlinks a
    /// matching *directory* too and swallows the resulting EISDIR/EPERM; so
    /// does `remove_file` here.
    fn purge_stray_logs(&self) {
        let mut allowed: Vec<String> = Vec::new();
        for stem in STEMS {
            allowed.push(active_log_name(stem));
            for i in 0..self.max_files {
                allowed.push(rotated_log_name(stem, i));
            }
        }

        let entries = match fs::read_dir(&self.log_dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if allowed.iter().any(|a| a == name) {
                continue;
            }
            if !belongs_to_stem_family(name) {
                continue;
            }
            let _ = fs::remove_file(self.log_dir.join(name));
        }
    }

    /// v4's `write()`: render once, append to `combined` always, and to `error`
    /// only when the level is ERROR.
    pub fn write(&self, level: &str, line: &str) {
        self.write_to_file("combined", line);
        if level == "error" {
            self.write_to_file("error", line);
        }
    }

    /// v4's `writeToFile()`: the content's **UTF-8 byte length** (v4's
    /// `Buffer.byteLength(content, 'utf-8')`, not its character count), rotate
    /// **before** appending when `currentSize + contentSize > maxFileSize`,
    /// then append and add the bytes to the running size. Every failure is a
    /// `console.error` and a swallow.
    fn write_to_file(&self, stem: &'static str, content: &str) {
        let mut sizes = self.lock_sizes();
        let content_size = content.len() as u64;
        let current_size = sizes.by_stem.get(stem).copied().unwrap_or(0);
        if current_size + content_size > self.max_file_size {
            self.rotate_file(stem, &mut sizes);
        }

        let path = self.log_dir.join(active_log_name(stem));
        let appended = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| f.write_all(content.as_bytes()));
        match appended {
            Ok(()) => {
                let current = sizes.by_stem.get(stem).copied().unwrap_or(0);
                sizes.by_stem.insert(stem, current + content_size);
            }
            Err(e) => eprintln!("Failed to write to {}: {e}", active_log_name(stem)),
        }
    }

    /// v4's `rotateFile()`: drop the oldest backup, shift each existing backup
    /// one slot older (.0 → .1, .1 → .2, …), then rename the active file to .0,
    /// then reset the running size to 0.
    ///
    /// **Every step is individually best-effort** — "not present is fine" is
    /// v4's own comment at all three sites, and it is load-bearing: a fresh
    /// directory has none of these files.
    ///
    /// ⚠ The degenerate `max_files` cases. v4's shift loop runs `i` from
    /// `maxFiles - 2` down to `0`, which for `maxFiles = 1` starts at `-1` and
    /// simply never executes. The obvious Rust spelling (`for i in
    /// (0..=max_files - 2)`) underflows a `usize` and panics where v4 merely
    /// skips — so the range is written `0..max_files.saturating_sub(1)`,
    /// reversed, which is empty for both `max_files = 1` and `max_files = 0`.
    /// (At `max_files = 0` v4 unlinks a nonexistent `<stem>.-1.log` and then
    /// renames active → `.0.log`; the saturating form unlinks `.0.log` first
    /// and renames onto the same name, which POSIX rename would have replaced
    /// anyway. Same end state.)
    fn rotate_file(&self, stem: &'static str, sizes: &mut Sizes) {
        let oldest = self
            .log_dir
            .join(rotated_log_name(stem, self.max_files.saturating_sub(1)));
        let _ = fs::remove_file(&oldest);

        for i in (0..self.max_files.saturating_sub(1)).rev() {
            let old_path = self.log_dir.join(rotated_log_name(stem, i));
            let new_path = self.log_dir.join(rotated_log_name(stem, i + 1));
            let _ = fs::rename(&old_path, &new_path);
        }

        let active = self.log_dir.join(active_log_name(stem));
        let first_backup = self.log_dir.join(rotated_log_name(stem, 0));
        let _ = fs::rename(&active, &first_backup);

        sizes.by_stem.insert(stem, 0);
    }

    /// The size map's lock, which must never panic a logging path: a poisoned
    /// mutex (a thread that panicked mid-write) yields its inner value rather
    /// than propagating.
    fn lock_sizes(&self) -> std::sync::MutexGuard<'_, Sizes> {
        self.sizes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The directory this transport writes into (for the boot banner).
    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as stdfs;
    use tempfile::TempDir;

    fn read(dir: &Path, name: &str) -> String {
        stdfs::read_to_string(dir.join(name)).unwrap_or_default()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = stdfs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        v.sort();
        v
    }

    // ── Unit 1 — the record shape ──────────────────────────────────────────

    /// v4's "should include all log data in JSON format": the envelope keys are
    /// exactly `timestamp`, `level`, `message`, `context` — in that order — and
    /// `error` is absent when there is no error.
    #[test]
    fn envelope_keys_match_v4() {
        let mut ctx = Map::new();
        ctx.insert("userId".into(), json!("user-123"));
        ctx.insert("action".into(), json!("create"));
        let line = render_line(
            "2025-12-01T00:00:00.000Z",
            "info",
            "Test message",
            ctx,
            None,
        );

        assert!(line.ends_with('\n'), "v4 appends a newline to every record");
        let parsed: Value = serde_json::from_str(line.trim()).unwrap();
        let keys: Vec<&str> = parsed.as_object().unwrap().keys().map(|k| &**k).collect();
        assert_eq!(keys, vec!["timestamp", "level", "message", "context"]);
        assert_eq!(parsed["timestamp"], json!("2025-12-01T00:00:00.000Z"));
        assert_eq!(parsed["level"], json!("info"));
        assert_eq!(parsed["message"], json!("Test message"));
        assert_eq!(parsed["context"]["userId"], json!("user-123"));
        assert_eq!(parsed["context"]["action"], json!("create"));
    }

    /// v4's `error` key rides beside `context` when present, and `context` is
    /// serialized even when empty (`JSON.stringify` keeps `{}`, drops
    /// `undefined`).
    #[test]
    fn error_key_present_only_when_set() {
        let line = render_line(
            "2025-12-01T00:00:00.000Z",
            "error",
            "Error occurred",
            Map::new(),
            Some(json!({"name": "Error", "message": "Something went wrong"})),
        );
        let parsed: Value = serde_json::from_str(line.trim()).unwrap();
        let keys: Vec<&str> = parsed.as_object().unwrap().keys().map(|k| &**k).collect();
        assert_eq!(
            keys,
            vec!["timestamp", "level", "message", "context", "error"]
        );
        assert_eq!(parsed["context"], json!({}));
        assert_eq!(parsed["error"]["message"], json!("Something went wrong"));
    }

    /// The five level names are v4's `LogLevel` values verbatim.
    #[test]
    fn level_names_match_v4_enum() {
        assert_eq!(level_name(&tracing::Level::ERROR), "error");
        assert_eq!(level_name(&tracing::Level::WARN), "warn");
        assert_eq!(level_name(&tracing::Level::INFO), "info");
        assert_eq!(level_name(&tracing::Level::DEBUG), "debug");
        assert_eq!(level_name(&tracing::Level::TRACE), "trace");
    }

    // ── Unit 1 — the fan-out ───────────────────────────────────────────────

    /// v4's "should create directory on initialization".
    #[test]
    fn creates_the_directory_on_initialization() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("nested").join("logs");
        assert!(!dir.exists());
        let _writer = LogFileWriter::new(dir.clone(), DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_FILES);
        assert!(dir.is_dir());
    }

    /// v4's "should write to combined.log for all log levels" +
    /// "should not write to error.log for non-ERROR levels".
    #[test]
    fn non_error_levels_write_only_combined() {
        let tmp = TempDir::new().unwrap();
        let w = LogFileWriter::new(
            tmp.path().to_path_buf(),
            DEFAULT_MAX_FILE_SIZE,
            DEFAULT_MAX_FILES,
        );
        for level in ["warn", "info", "debug", "trace"] {
            w.write(
                level,
                &render_line("2025-12-01T00:00:00.000Z", level, "Test", Map::new(), None),
            );
        }
        assert_eq!(read(tmp.path(), "combined.log").lines().count(), 4);
        assert_eq!(
            names(tmp.path()),
            vec!["combined.log"],
            "error.log is never created by non-error records"
        );
    }

    /// v4's "should write to error.log only for ERROR level" — the error
    /// record lands in BOTH files, byte-identical.
    #[test]
    fn error_level_writes_both_files() {
        let tmp = TempDir::new().unwrap();
        let w = LogFileWriter::new(
            tmp.path().to_path_buf(),
            DEFAULT_MAX_FILE_SIZE,
            DEFAULT_MAX_FILES,
        );
        let line = render_line(
            "2025-12-01T00:00:00.000Z",
            "error",
            "Test error",
            Map::new(),
            None,
        );
        w.write("error", &line);

        assert_eq!(read(tmp.path(), "combined.log"), line);
        assert_eq!(read(tmp.path(), "error.log"), line);
    }

    /// v4's "should write to appropriate files based on level": one error plus
    /// three non-errors = 5 appends (2 + 1 + 1 + 1).
    #[test]
    fn mixed_levels_land_in_v4s_counts() {
        let tmp = TempDir::new().unwrap();
        let w = LogFileWriter::new(
            tmp.path().to_path_buf(),
            DEFAULT_MAX_FILE_SIZE,
            DEFAULT_MAX_FILES,
        );
        for level in ["error", "warn", "info", "debug"] {
            w.write(
                level,
                &render_line("2025-12-01T00:00:00.000Z", level, level, Map::new(), None),
            );
        }
        assert_eq!(read(tmp.path(), "combined.log").lines().count(), 4);
        assert_eq!(read(tmp.path(), "error.log").lines().count(), 1);
    }

    /// v4's "should initialize file size tracking for combined.log/error.log":
    /// an existing active file seeds the running size, so the FIRST write after
    /// a restart still rotates at the right byte.
    #[test]
    fn existing_active_file_seeds_the_size() {
        let tmp = TempDir::new().unwrap();
        stdfs::write(tmp.path().join("combined.log"), "x".repeat(90)).unwrap();
        // 90 seeded + a 20-byte record > 100 → rotate before appending.
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 100, DEFAULT_MAX_FILES);
        w.write("info", &"y".repeat(20));

        assert_eq!(read(tmp.path(), "combined.0.log").len(), 90);
        assert_eq!(read(tmp.path(), "combined.log").len(), 20);
    }

    /// v4's "should handle initialization errors gracefully": a `mkdir` that
    /// cannot succeed is reported and swallowed, never a panic.
    #[test]
    fn initialization_failure_is_swallowed() {
        let tmp = TempDir::new().unwrap();
        // A regular file where the directory should be: create_dir_all fails.
        let blocked = tmp.path().join("not-a-dir");
        stdfs::write(&blocked, b"occupied").unwrap();
        let w = LogFileWriter::new(
            blocked.join("logs"),
            DEFAULT_MAX_FILE_SIZE,
            DEFAULT_MAX_FILES,
        );
        // And a write against the dead transport is still a no-op, not a panic
        // (v4's `writeToFile` catch).
        w.write("error", "{}\n");
    }

    // ── Unit 2 — rotation ──────────────────────────────────────────────────

    /// v4's "should rotate file when size exceeds maxFileSize" + "should rename
    /// rotated files in sequence": the active file becomes `.0.log`.
    #[test]
    fn rotates_active_into_zero() {
        let tmp = TempDir::new().unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 100, 3);
        w.write("info", &format!("{}\n", "A".repeat(60)));
        w.write("info", &format!("{}\n", "B".repeat(60)));

        assert_eq!(read(tmp.path(), "combined.0.log").trim(), "A".repeat(60));
        assert_eq!(read(tmp.path(), "combined.log").trim(), "B".repeat(60));
    }

    /// The threshold is v4's: rotate when `current + content > max`, so a write
    /// landing exactly ON the ceiling does NOT rotate.
    #[test]
    fn rotation_threshold_is_strictly_greater() {
        let tmp = TempDir::new().unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 100, 3);
        w.write("info", &"A".repeat(50));
        w.write("info", &"B".repeat(50)); // 50 + 50 = 100, not > 100
        assert_eq!(read(tmp.path(), "combined.log").len(), 100);
        assert!(!tmp.path().join("combined.0.log").exists());

        w.write("info", "C"); // 100 + 1 > 100
        assert_eq!(read(tmp.path(), "combined.0.log").len(), 100);
        assert_eq!(read(tmp.path(), "combined.log"), "C");
    }

    /// v4 measures with `Buffer.byteLength(content, 'utf-8')` — **bytes**, not
    /// characters. A 30-character message of 3-byte characters is 90 bytes and
    /// rotates a 100-byte file on the second write; counting characters would
    /// not.
    #[test]
    fn size_is_utf8_bytes_not_characters() {
        let tmp = TempDir::new().unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 100, 3);
        let multibyte = "空".repeat(30); // 30 chars, 90 bytes
        assert_eq!(multibyte.chars().count(), 30);
        assert_eq!(multibyte.len(), 90);

        w.write("info", &multibyte);
        assert!(!tmp.path().join("combined.0.log").exists());
        w.write("info", &multibyte); // 90 + 90 > 100 by bytes; 60 chars is not
        assert_eq!(read(tmp.path(), "combined.0.log").chars().count(), 30);
    }

    /// v4's "should remove oldest file when maxFiles limit is reached" +
    /// the downward shift: `.0 → .1`, `.1 → .2`, and the old `.<maxFiles-1>`
    /// is gone.
    #[test]
    fn shift_drops_the_oldest_and_moves_each_backup_down() {
        let tmp = TempDir::new().unwrap();
        stdfs::write(tmp.path().join("combined.0.log"), "zero").unwrap();
        stdfs::write(tmp.path().join("combined.1.log"), "one").unwrap();
        stdfs::write(tmp.path().join("combined.2.log"), "two-the-oldest").unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 10, 3);
        w.write("info", "active-content");

        assert_eq!(read(tmp.path(), "combined.1.log"), "zero");
        assert_eq!(read(tmp.path(), "combined.2.log"), "one");
        assert_eq!(
            read(tmp.path(), "combined.log"),
            "active-content",
            "the rotated-away active file was empty (never created), so .0 is absent"
        );
        assert!(
            !tmp.path().join("combined.3.log").exists(),
            "maxFiles = 3 keeps .0 .1 .2 and nothing beyond"
        );
    }

    /// Gaps in the backup sequence are fine — each rename is independently
    /// best-effort ("not present is fine"), so a missing `.1.log` does not stop
    /// `.2.log` from shifting.
    #[test]
    fn rotation_tolerates_gaps_in_the_backup_sequence() {
        let tmp = TempDir::new().unwrap();
        stdfs::write(tmp.path().join("combined.log"), "active").unwrap();
        stdfs::write(tmp.path().join("combined.0.log"), "zero").unwrap();
        // .1.log deliberately absent
        stdfs::write(tmp.path().join("combined.2.log"), "two").unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 5, 5);
        w.write("info", "new-record");

        assert_eq!(read(tmp.path(), "combined.0.log"), "active");
        assert_eq!(read(tmp.path(), "combined.1.log"), "zero");
        assert!(!tmp.path().join("combined.2.log").exists());
        assert_eq!(read(tmp.path(), "combined.3.log"), "two");
        assert_eq!(read(tmp.path(), "combined.log"), "new-record");
    }

    /// The degenerate `maxFiles = 1`: v4's shift loop starts at `-1` and never
    /// runs, so the active file becomes `.0.log` and there is nothing to shift.
    /// The naive `0..=max_files - 2` spelling would panic on `usize` underflow.
    #[test]
    fn max_files_one_keeps_a_single_backup_without_underflow() {
        let tmp = TempDir::new().unwrap();
        stdfs::write(tmp.path().join("combined.log"), "first").unwrap();
        stdfs::write(tmp.path().join("combined.0.log"), "older").unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 5, 1);
        w.write("info", "second");

        assert_eq!(read(tmp.path(), "combined.0.log"), "first");
        assert_eq!(read(tmp.path(), "combined.log"), "second");
        assert!(!tmp.path().join("combined.1.log").exists());
    }

    /// `maxFiles = 0` likewise must not underflow. v4 unlinks a nonexistent
    /// `.-1.log`, skips the loop, and renames the active file onto `.0.log`.
    #[test]
    fn max_files_zero_does_not_underflow() {
        let tmp = TempDir::new().unwrap();
        stdfs::write(tmp.path().join("combined.log"), "first").unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 5, 0);
        w.write("info", "second");
        assert_eq!(read(tmp.path(), "combined.0.log"), "first");
        assert_eq!(read(tmp.path(), "combined.log"), "second");
    }

    /// v4's "should handle errors in rotateFile without throwing": with every
    /// filesystem step failing (the directory is gone), the write is a no-op
    /// rather than a panic, and the running size still resets.
    #[test]
    fn rotation_failure_is_swallowed() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("logs");
        let w = LogFileWriter::new(dir.clone(), 5, 3);
        stdfs::remove_dir_all(&dir).unwrap();
        w.write("error", "a record long enough to rotate");
    }

    /// v4's "should track file sizes after writes": consecutive writes
    /// accumulate rather than each measuring from zero.
    #[test]
    fn sizes_accumulate_across_writes() {
        let tmp = TempDir::new().unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 100, 3);
        for _ in 0..4 {
            w.write("info", &"A".repeat(30)); // 30, 60, 90, then 90+30 > 100
        }
        assert_eq!(read(tmp.path(), "combined.0.log").len(), 90);
        assert_eq!(read(tmp.path(), "combined.log").len(), 30);
    }

    /// The two stems rotate independently — an error storm rotating `error.log`
    /// must not reset `combined.log`'s accounting.
    #[test]
    fn stems_rotate_independently() {
        let tmp = TempDir::new().unwrap();
        let w = LogFileWriter::new(tmp.path().to_path_buf(), 100, 3);
        w.write("info", &"A".repeat(80)); // combined: 80
        w.write("error", &"B".repeat(30)); // combined: 80+30 > 100 → rotate; error: 30
        assert_eq!(read(tmp.path(), "combined.0.log").len(), 80);
        assert_eq!(read(tmp.path(), "combined.log").len(), 30);
        assert_eq!(read(tmp.path(), "error.log").len(), 30);
        assert!(!tmp.path().join("error.0.log").exists());
    }
}

/// Unit 3 — the stray-file sweep. Its own module because this predicate is the
/// only part of the transport that **deletes**, and it has to be pinned in both
/// directions: what it eats, and what it must never eat.
///
/// v4's own comment names the three shapes it exists for — leftovers from an
/// older `<stem>.log.<N>` rotation, iCloud sync conflicts (`combined 2.log`,
/// `combined.log.9 2`) and Finder duplicates (`combined(2).log`) — all of which
/// accumulate in an instance directory that lives in iCloud or Dropbox.
#[cfg(test)]
mod sweep_tests {
    use super::*;
    use std::fs as stdfs;
    use tempfile::TempDir;

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = stdfs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        v.sort();
        v
    }

    /// v4's `belongsToStemFamily` as a table: the exact stem, a
    /// non-alphanumeric follower, and — the subtle half — an **alphanumeric**
    /// follower which makes the entry somebody else's file.
    #[test]
    fn stem_family_predicate_matches_v4() {
        for belongs in [
            "combined",
            "error",
            "combined.log",
            "combined 2.log",
            "combined(2).log",
            "combined-old.log",
            "combined.log.3",
            "error 2.log",
            "error.log.10",
        ] {
            assert!(
                belongs_to_stem_family(belongs),
                "{belongs} belongs to a stem family"
            );
        }
        for foreign in [
            "errors.log",
            "combined2.log",
            "error2",
            "embedded-server.log",
            "startup.log",
            "quilltap-stdout.log",
            "quilltap-stderr.log",
            "stdout.log",
            "terminals",
            "",
        ] {
            assert!(
                !belongs_to_stem_family(foreign),
                "{foreign} is not ours to delete"
            );
        }
    }

    /// A multi-byte follower is not ASCII-alphanumeric, so it belongs — the
    /// same answer v4's `/[a-zA-Z0-9]/` gives the UTF-16 code unit at that
    /// offset. Pinned because the byte-vs-code-unit indexing is a real
    /// difference in mechanism reaching the same verdict.
    #[test]
    fn multibyte_follower_belongs_like_v4() {
        assert!(belongs_to_stem_family("combined空.log"));
        assert!(belongs_to_stem_family("error—2.log"));
    }

    /// v4's "should delete files that do not match the allowed combined/error
    /// names", entry for entry — its exact directory listing, run against the
    /// real filesystem rather than a mocked `readdir`.
    #[test]
    fn sweep_deletes_conflicts_and_spares_neighbours() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let allowed = [
            "combined.log",
            "combined.0.log",
            "combined.9.log",
            "error.log",
            "error.3.log",
        ];
        let garbage = [
            "combined 2.log",
            "combined 11.log",
            "combined(2).log",
            "combined.log.5",
            "combined.log.6 2",
            "combined.log(1).3",
            "error.log.10",
        ];
        let foreign = [
            "quilltap-stderr.log",
            "quilltap-stdout.log",
            "startup.log",
            "stdout.log",
            "embedded-server.log",
            // starts with "error" but the next char is alphanumeric
            "errors.log",
        ];
        for name in allowed.iter().chain(garbage.iter()).chain(foreign.iter()) {
            stdfs::write(dir.join(name), b"x").unwrap();
        }
        // v4's listing carries `terminals` — in v5 it is the live PTY
        // transcript directory (`host.rs`'s `logs/terminals`), the one thing in
        // this directory another subsystem owns. It must survive with its
        // contents.
        stdfs::create_dir(dir.join("terminals")).unwrap();
        stdfs::write(dir.join("terminals").join("session.log"), b"pty").unwrap();

        let _w = LogFileWriter::new(dir.to_path_buf(), DEFAULT_MAX_FILE_SIZE, 10);

        for name in garbage {
            assert!(!dir.join(name).exists(), "{name} should have been swept");
        }
        for name in allowed.iter().chain(foreign.iter()) {
            assert!(dir.join(name).exists(), "{name} must not be touched");
        }
        assert!(dir.join("terminals").is_dir());
        assert_eq!(
            stdfs::read_to_string(dir.join("terminals").join("session.log")).unwrap(),
            "pty"
        );
    }

    /// The allowlist is `maxFiles`-relative: at `maxFiles = 3`, `.0`–`.2` are
    /// kept and `.3` upward is a leftover from a larger setting — exactly what
    /// the sweep exists to clear.
    #[test]
    fn allowlist_follows_max_files() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        for name in [
            "combined.0.log",
            "combined.2.log",
            "combined.3.log",
            "error.9.log",
        ] {
            stdfs::write(dir.join(name), b"x").unwrap();
        }
        let _w = LogFileWriter::new(dir.to_path_buf(), DEFAULT_MAX_FILE_SIZE, 3);
        assert_eq!(
            names(dir),
            vec!["combined.0.log", "combined.2.log"],
            ".3 and .9 are out of range at maxFiles = 3"
        );
    }

    /// A **directory** whose name belongs to a stem family: v4 calls `unlink`
    /// on it and swallows the EISDIR/EPERM, so the directory survives.
    /// `remove_file` behaves the same. Recorded because it is the one way a
    /// belonging name is spared.
    #[test]
    fn a_directory_in_the_family_survives_the_unlink() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        stdfs::create_dir(dir.join("combined.old")).unwrap();
        stdfs::write(dir.join("combined.old").join("inner"), b"x").unwrap();
        assert!(belongs_to_stem_family("combined.old"));

        let _w = LogFileWriter::new(dir.to_path_buf(), DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_FILES);
        assert!(dir.join("combined.old").is_dir());
    }

    /// v4's "should tolerate readdir failures during purge": an unreadable
    /// directory returns early and initialization still seeds the sizes.
    /// Reproduced by clearing the parent's search bit — `chmod 000` on the
    /// target alone does not deny access on macOS
    /// (`planted-fs-conditions-in-differentials`).
    #[test]
    #[cfg(unix)]
    fn readdir_failure_is_tolerated() {
        use std::os::unix::fs::PermissionsExt as _;
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("logs");
        stdfs::create_dir(&dir).unwrap();
        stdfs::set_permissions(&dir, stdfs::Permissions::from_mode(0o000)).unwrap();

        let w = LogFileWriter::new(dir.clone(), DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_FILES);
        w.write("info", "{}\n");

        stdfs::set_permissions(&dir, stdfs::Permissions::from_mode(0o755)).unwrap();
    }

    /// v4's "should tolerate unlink failures during purge": a stray file that
    /// cannot be removed leaves the rest of the sweep — and the size seeding —
    /// intact.
    #[test]
    #[cfg(unix)]
    fn unlink_failure_is_tolerated() {
        use std::os::unix::fs::PermissionsExt as _;
        let tmp = TempDir::new().unwrap();
        let outer = tmp.path().join("outer");
        stdfs::create_dir(&outer).unwrap();
        stdfs::write(outer.join("combined.log.5"), b"stray").unwrap();
        stdfs::write(outer.join("combined 2.log"), b"stray").unwrap();
        // Read + execute but not write: the entries list, the unlinks fail.
        stdfs::set_permissions(&outer, stdfs::Permissions::from_mode(0o555)).unwrap();

        let _w = LogFileWriter::new(outer.clone(), DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_FILES);
        assert!(outer.join("combined.log.5").exists());

        stdfs::set_permissions(&outer, stdfs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The sweep runs **once, at initialization** — v4 calls it from
    /// `initializeDirectory`, never from `write`. A stray file that lands while
    /// the process is running survives until the next boot.
    #[test]
    fn sweep_runs_only_at_initialization() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        let w = LogFileWriter::new(dir.to_path_buf(), DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_FILES);
        stdfs::write(dir.join("combined 2.log"), b"arrived later").unwrap();
        w.write("info", "{}\n");
        assert!(dir.join("combined 2.log").exists());
    }
}

// ── Unit 4 — the knobs, and the `tracing` layer that feeds the transport ──

/// v4's `LOG_OUTPUT` (`lib/env.ts:31`): where log records are sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogOutput {
    Console,
    File,
    Both,
}

impl LogOutput {
    pub fn wants_console(self) -> bool {
        matches!(self, LogOutput::Console | LogOutput::Both)
    }
    pub fn wants_file(self) -> bool {
        matches!(self, LogOutput::File | LogOutput::Both)
    }
}

/// **v5's default is `both` — a recorded divergence from v4's code default of
/// `console`** (ruled by the human, 2026-08-18, work order P4.49 unit 6).
///
/// The reasoning, recorded so it is never re-litigated as an accident: v4's own
/// `.env.local` runs `LOG_OUTPUT="file"`, so no deployment in actual use relies
/// on the code default; P4.18 already ruled log records non-differential, so a
/// default cannot break an oracle; and a lane whose entire purpose is that v5's
/// output evaporates must not depend on someone remembering an env var. `both`
/// rather than v4's lived `file` because v5's stderr is genuinely useful when
/// the binary runs in a terminal, which is how the dogfood server runs.
///
/// **Scoped to the port.** The ruling names its own expiry: once primary
/// development moves to Rust/Angular in this repo, the default is expected to
/// become environment-dependent (production vs development) rather than a flat
/// `both`. That conditional is deliberately NOT built now — there is no
/// environment concept here to hang it on. The four knobs below are honoured in
/// full so the change is a one-liner when the time comes; treat it as a planned
/// step, not drift.
pub const DEFAULT_LOG_OUTPUT: LogOutput = LogOutput::Both;

/// The resolved four knobs.
#[derive(Debug, Clone)]
pub struct LogSettings {
    pub output: LogOutput,
    /// `LOG_FILE_PATH`, **after v4's quirk** — `None` means "use the instance's
    /// own logs dir".
    pub explicit_path: Option<PathBuf>,
    pub max_file_size: u64,
    pub max_files: usize,
}

/// Resolve v4's four `LOG_*` knobs from their raw env values. Pure — the
/// process environment is read by the caller, because mutating it in a test
/// races the parallel test threads (the same reason `tracing_filter_directive`
/// is pulled out pure).
///
/// Returns the settings plus any complaints about unusable values. v4 rejects
/// those at the zod schema and refuses to boot; v5 falls back to the default
/// and says so **after** the subscriber exists — refusing to start a server
/// because a logging knob is misspelled is the wrong trade for an operability
/// surface.
///
/// The `LOG_FILE_PATH` quirk (`lib/logger.ts:55-57`) is ported, not simplified:
/// v4's zod schema *defaults* the variable to `'./logs'`, so the value is
/// always set, and the transport therefore honours it **only if it is set AND
/// is not the literal default**. A deployment that spells out `./logs` gets the
/// instance's logs dir, not a relative directory beside the process's cwd.
pub fn resolve_log_settings(
    output: Option<&str>,
    file_path: Option<&str>,
    max_file_size: Option<&str>,
    max_files: Option<&str>,
) -> (LogSettings, Vec<String>) {
    let mut complaints = Vec::new();

    let output = match output.map(str::trim) {
        None | Some("") => DEFAULT_LOG_OUTPUT,
        Some("console") => LogOutput::Console,
        Some("file") => LogOutput::File,
        Some("both") => LogOutput::Both,
        Some(other) => {
            complaints.push(format!(
                "LOG_OUTPUT={other:?} is not one of console/file/both; using the default"
            ));
            DEFAULT_LOG_OUTPUT
        }
    };

    let explicit_path = match file_path.map(str::trim) {
        None | Some("") => None,
        // v4: `env.LOG_FILE_PATH && env.LOG_FILE_PATH !== './logs'`.
        Some("./logs") => None,
        Some(p) => Some(PathBuf::from(p)),
    };

    let max_file_size = match max_file_size.map(str::trim) {
        None | Some("") => DEFAULT_MAX_FILE_SIZE,
        Some(v) => v.parse::<u64>().unwrap_or_else(|_| {
            complaints.push(format!(
                "LOG_FILE_MAX_SIZE={v:?} is not a byte count; using the default"
            ));
            DEFAULT_MAX_FILE_SIZE
        }),
    };

    let max_files = match max_files.map(str::trim) {
        None | Some("") => DEFAULT_MAX_FILES,
        Some(v) => v.parse::<usize>().unwrap_or_else(|_| {
            complaints.push(format!(
                "LOG_FILE_MAX_FILES={v:?} is not a count; using the default"
            ));
            DEFAULT_MAX_FILES
        }),
    };

    (
        LogSettings {
            output,
            explicit_path,
            max_file_size,
            max_files,
        },
        complaints,
    )
}

impl LogSettings {
    /// Read the four knobs straight out of the process environment.
    pub fn from_env() -> (Self, Vec<String>) {
        let get = |k: &str| std::env::var(k).ok();
        let (output, path, size, files) = (
            get("LOG_OUTPUT"),
            get("LOG_FILE_PATH"),
            get("LOG_FILE_MAX_SIZE"),
            get("LOG_FILE_MAX_FILES"),
        );
        resolve_log_settings(
            output.as_deref(),
            path.as_deref(),
            size.as_deref(),
            files.as_deref(),
        )
    }

    /// The directory the file transport would write into: an explicit
    /// `LOG_FILE_PATH` wins, else the instance's own `logs/` — the same
    /// `<base>/logs` `quilltap-host` already uses for `logs/terminals`
    /// (v4's `getLogsDir()`).
    pub fn resolve_dir(&self, instance_base_dir: Option<&Path>) -> Option<PathBuf> {
        self.explicit_path
            .clone()
            .or_else(|| instance_base_dir.map(|d| d.join("logs")))
    }
}

/// Which destinations a given set of knobs actually resolves to. Pulled out
/// pure so the "never leave a process with **no** destination" rule can be
/// pinned without installing a process-global subscriber (`try_init` succeeds
/// exactly once per process, so the decision cannot be tested through it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogDestinations {
    pub console: bool,
    pub file_dir: Option<PathBuf>,
    /// Set when files were asked for and no directory is known — the caller
    /// emits it once the subscriber exists.
    pub complaint: Option<String>,
}

/// Resolve [`LogSettings`] against the instance dir into concrete
/// destinations. Console is installed when asked for **and** whenever the file
/// half was asked for but could not be built — a `LOG_OUTPUT=file` process with
/// nowhere to write must not fall silent, which is the exact failure this lane
/// exists to end.
pub fn resolve_destinations(
    settings: &LogSettings,
    instance_base_dir: Option<&Path>,
) -> LogDestinations {
    let mut complaint = None;
    let file_dir = if settings.output.wants_file() {
        match settings.resolve_dir(instance_base_dir) {
            Some(dir) => Some(dir),
            None => {
                complaint = Some(
                    "LOG_OUTPUT asks for files but no log directory is known (no instance \
                     resolved and no LOG_FILE_PATH); logging to stderr only"
                        .to_string(),
                );
                None
            }
        }
    } else {
        None
    };
    LogDestinations {
        console: settings.output.wants_console() || file_dir.is_none(),
        file_dir,
        complaint,
    }
}

/// Collects a `tracing` event's fields into v4's record shape.
///
/// The split: the `message` field is v4's `message`; a field literally named
/// `error` becomes v4's `error` object (the codebase's established convention
/// is `tracing::warn!(error = %e, "…")`); everything else is `context`.
/// Per P4.18 the *contents* of `context` are v5's own and carry no fidelity
/// obligation — only the envelope's keys do.
#[derive(Default)]
struct FieldVisitor {
    message: String,
    context: Map<String, Value>,
    error: Option<Value>,
}

impl FieldVisitor {
    fn put(&mut self, name: &str, value: Value) {
        match name {
            "message" => {
                self.message = match value {
                    Value::String(s) => s,
                    other => other.to_string(),
                }
            }
            "error" => {
                let message = match value {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                // v4's `error.name` is the JS constructor name; a Rust error
                // rendered through `%`/`?` has no runtime type name, so the
                // generic base name stands in and `stack` (optional in v4) is
                // absent.
                self.error = Some(json!({ "name": "Error", "message": message }));
            }
            other => {
                self.context.insert(other.to_string(), value);
            }
        }
    }
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.put(field.name(), Value::String(format!("{value:?}")));
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.put(field.name(), Value::String(value.to_string()));
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.put(field.name(), json!(value));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.put(field.name(), json!(value));
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.put(field.name(), json!(value));
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        // `JSON.stringify` writes NaN/Infinity as `null`; so does this.
        let v = serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null);
        self.put(field.name(), v);
    }
}

/// The `tracing_subscriber` layer that turns events into v4-shaped file
/// records. Mounted **beside** the stderr fmt layer under the one `EnvFilter`,
/// so `RUST_LOG` governs both destinations identically.
pub struct LogFileLayer {
    writer: Arc<LogFileWriter>,
}

impl LogFileLayer {
    pub fn new(writer: Arc<LogFileWriter>) -> Self {
        Self { writer }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogFileLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let level = level_name(metadata.level());

        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        // v5's context: the event target first (the analog of v4's singleton
        // `{service, environment}` prelude), then the event's own fields.
        let mut context = Map::new();
        context.insert("module".to_string(), json!(metadata.target()));
        for (k, v) in visitor.context {
            context.insert(k, v);
        }

        let line = render_line(
            &quilltap_core::clock::now_iso(),
            level,
            &visitor.message,
            context,
            visitor.error,
        );
        self.writer.write(level, &line);
    }
}

#[cfg(test)]
mod knob_tests {
    use super::*;

    /// The four defaults. `LOG_OUTPUT` is v5's ruled `both`, NOT v4's code
    /// default of `console` — see [`DEFAULT_LOG_OUTPUT`] for the ruling.
    #[test]
    fn defaults_match_the_ruling_and_v4s_numbers() {
        let (s, complaints) = resolve_log_settings(None, None, None, None);
        assert_eq!(s.output, LogOutput::Both);
        assert_eq!(s.explicit_path, None);
        assert_eq!(s.max_file_size, 10_485_760);
        assert_eq!(s.max_files, 10);
        assert!(complaints.is_empty());
    }

    /// v4's three `LOG_OUTPUT` values, and the fan-out each one asks for.
    #[test]
    fn output_values_match_v4s_enum() {
        for (raw, expect, console, file) in [
            ("console", LogOutput::Console, true, false),
            ("file", LogOutput::File, false, true),
            ("both", LogOutput::Both, true, true),
        ] {
            let (s, complaints) = resolve_log_settings(Some(raw), None, None, None);
            assert_eq!(s.output, expect);
            assert_eq!(s.output.wants_console(), console);
            assert_eq!(s.output.wants_file(), file);
            assert!(complaints.is_empty());
        }
    }

    /// **The `LOG_FILE_PATH` quirk** (`lib/logger.ts:55-57`): the value is
    /// honoured only if set AND not the literal default `'./logs'` — v4's zod
    /// schema always sets it, so the literal is how a deployment says "no
    /// override". A custom path IS honoured.
    #[test]
    fn log_file_path_quirk_is_ported_not_simplified() {
        let instance = PathBuf::from("/srv/quilltap/Friday");

        let (s, _) = resolve_log_settings(None, Some("./logs"), None, None);
        assert_eq!(s.explicit_path, None);
        assert_eq!(
            s.resolve_dir(Some(&instance)),
            Some(PathBuf::from("/srv/quilltap/Friday/logs")),
            "the literal default means the instance's own logs dir"
        );

        let (s, _) = resolve_log_settings(None, Some("/var/log/quilltap"), None, None);
        assert_eq!(s.explicit_path, Some(PathBuf::from("/var/log/quilltap")));
        assert_eq!(
            s.resolve_dir(Some(&instance)),
            Some(PathBuf::from("/var/log/quilltap")),
            "an explicit non-default path wins over the instance dir"
        );

        // Unset behaves as the default does.
        let (s, _) = resolve_log_settings(None, None, None, None);
        assert_eq!(
            s.resolve_dir(Some(&instance)),
            Some(PathBuf::from("/srv/quilltap/Friday/logs"))
        );
        assert_eq!(s.resolve_dir(None), None);
    }

    /// The numeric knobs parse, and an unusable value falls back **loudly** —
    /// a complaint the caller emits once the subscriber exists.
    #[test]
    fn numeric_knobs_parse_and_complain() {
        let (s, complaints) = resolve_log_settings(None, None, Some("2097152"), Some("4"));
        assert_eq!(s.max_file_size, 2_097_152);
        assert_eq!(s.max_files, 4);
        assert!(complaints.is_empty());

        let (s, complaints) = resolve_log_settings(
            Some("everywhere"),
            None,
            Some("ten megabytes"),
            Some("lots"),
        );
        assert_eq!(s.output, DEFAULT_LOG_OUTPUT);
        assert_eq!(s.max_file_size, DEFAULT_MAX_FILE_SIZE);
        assert_eq!(s.max_files, DEFAULT_MAX_FILES);
        assert_eq!(
            complaints.len(),
            3,
            "each unusable knob complains: {complaints:?}"
        );
    }

    /// The destination table. The load-bearing row is the last one: files
    /// asked for with nowhere to put them still gets stderr, loudly.
    #[test]
    fn destinations_never_leave_a_process_silent() {
        let instance = PathBuf::from("/srv/Friday");
        let settings = |raw: &str| resolve_log_settings(Some(raw), None, None, None).0;

        let d = resolve_destinations(&settings("console"), Some(&instance));
        assert!(d.console);
        assert_eq!(d.file_dir, None);
        assert_eq!(d.complaint, None);

        let d = resolve_destinations(&settings("file"), Some(&instance));
        assert!(!d.console, "file-only means file-only when a dir is known");
        assert_eq!(d.file_dir, Some(PathBuf::from("/srv/Friday/logs")));
        assert_eq!(d.complaint, None);

        let d = resolve_destinations(&settings("both"), Some(&instance));
        assert!(d.console);
        assert_eq!(d.file_dir, Some(PathBuf::from("/srv/Friday/logs")));

        // The default, with no instance: console, and no complaint (`both`
        // already wanted console).
        let d = resolve_destinations(&resolve_log_settings(None, None, None, None).0, None);
        assert!(d.console);
        assert_eq!(d.file_dir, None);
        assert!(
            d.complaint.is_some(),
            "the missing file half is still said out loud"
        );

        // File-only with nowhere to write: stderr anyway, and a complaint.
        let d = resolve_destinations(&settings("file"), None);
        assert!(
            d.console,
            "a file-only process with no directory must not fall silent"
        );
        assert_eq!(d.file_dir, None);
        assert!(d.complaint.is_some());
    }

    /// An event's fields land in v4's envelope: `message` is the message,
    /// `error` becomes the error object, everything else is context — with the
    /// event target as `module`.
    #[test]
    fn the_layer_writes_v4_shaped_records() {
        use tracing_subscriber::prelude::*;

        let tmp = tempfile::TempDir::new().unwrap();
        let writer = Arc::new(LogFileWriter::new(
            tmp.path().to_path_buf(),
            DEFAULT_MAX_FILE_SIZE,
            DEFAULT_MAX_FILES,
        ));
        let subscriber =
            tracing_subscriber::registry().with(LogFileLayer::new(Arc::clone(&writer)));

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                chat_id = "c-1",
                turn = 3u64,
                ratio = 0.5,
                "context assembled"
            );
            tracing::error!(error = "disk full", "write failed");
        });

        let combined = std::fs::read_to_string(tmp.path().join("combined.log")).unwrap();
        let lines: Vec<&str> = combined.lines().collect();
        assert_eq!(lines.len(), 2);

        let first: Value = serde_json::from_str(lines[0]).unwrap();
        let keys: Vec<&str> = first.as_object().unwrap().keys().map(|k| &**k).collect();
        assert_eq!(keys, vec!["timestamp", "level", "message", "context"]);
        assert_eq!(first["level"], json!("info"));
        assert_eq!(first["message"], json!("context assembled"));
        assert_eq!(
            first["context"]["module"],
            json!("quilltap_web::log_file::knob_tests")
        );
        assert_eq!(first["context"]["chat_id"], json!("c-1"));
        assert_eq!(first["context"]["turn"], json!(3));
        assert_eq!(first["context"]["ratio"], json!(0.5));
        // v4's `toISOString()` shape.
        let ts = first["timestamp"].as_str().unwrap();
        assert_eq!(ts.len(), 24, "{ts}");
        assert!(ts.ends_with('Z') && ts.contains('T'));

        let second: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["level"], json!("error"));
        assert_eq!(second["message"], json!("write failed"));
        assert_eq!(second["error"]["message"], json!("disk full"));
        assert!(second["context"].get("error").is_none());

        // The error record went to BOTH files; the info record to combined only.
        let errors = std::fs::read_to_string(tmp.path().join("error.log")).unwrap();
        assert_eq!(errors.lines().count(), 1);
        assert!(errors.contains("write failed"));
    }
}
