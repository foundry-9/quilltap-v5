//! `quilltap db integrity` (P4.D365) — v4 `packages/quilltap/lib/
//! db-commands.js` `cmdIntegrity` / `integrityOneDb` at v4 `624d0d25c`
//! (`70d2859d8`): `PRAGMA integrity_check` on each encrypted database,
//! read-only, safe beside a live instance. Exit 0 ok, 1 issues, 2 an open
//! failure — and both non-zero exits are SILENT (v4 throws with `silent =
//! true`, so `runVerb` prints no `Error:` line), which is why this verb has
//! its own result type rather than `db_characters::CmdError`.
//!
//! It lives in its own module (the `db_characters.rs` split precedent) and
//! opens through the CLI's ONE opener, `dbopen::open_encrypted` (v4's
//! `openEncryptedDb`).

use serde_json::{Map, Value};

use crate::db_characters::{as_bool, parse_sub_args, print_json, CmdError, Ctx};
use crate::dbopen::{open_encrypted, sqlite_msg, OpenOptions};
use crate::nodefmt::node_join;
use crate::out;

/// v4 `INTEGRITY_TARGETS` (= `OPTIMIZE_TARGETS`): `(key, filename,
/// friendlyName of the opener v4 picks for the key)`. The key IS the label.
const INTEGRITY_TARGETS: [(&str, &str, &str); 3] = [
    ("main", "quilltap.db", "main database"),
    ("llm-logs", "quilltap-llm-logs.db", "LLM logs database"),
    (
        "mount-points",
        "quilltap-mount-index.db",
        "mount index database",
    ),
];

/// How `cmdIntegrity` ends when it does not succeed.
pub(crate) enum IntegrityExit {
    /// v4's `err.silent = true` throws: no `Error:` line, this exit code.
    Silent(i32),
    /// An ordinary throw (`Unknown integrity target …`): `Error: <message>`.
    Loud(CmdError),
}

/// better-sqlite3's `err.code` for a SQLite error: the name of the EXTENDED
/// result code, from the driver's `src/util/constants.cpp` (`CS::Code`),
/// transcribed WHOLE from `better-sqlite3-multiple-ciphers` 13.0.3 at v4
/// `624d0d25c` (the values from the vendored SQLite3MC 2.4.0 header). An
/// unlisted code is `UNKNOWN_SQLITE_ERROR_<n>`, as the driver answers.
/// `cli_sqlite_code_names_guard` pins this table against the pin's file.
pub(crate) const SQLITE_CODE_NAMES: &[(&str, i32)] = &[
    ("SQLITE_OK", 0),
    ("SQLITE_ERROR", 1),
    ("SQLITE_INTERNAL", 2),
    ("SQLITE_PERM", 3),
    ("SQLITE_ABORT", 4),
    ("SQLITE_BUSY", 5),
    ("SQLITE_LOCKED", 6),
    ("SQLITE_NOMEM", 7),
    ("SQLITE_READONLY", 8),
    ("SQLITE_INTERRUPT", 9),
    ("SQLITE_IOERR", 10),
    ("SQLITE_CORRUPT", 11),
    ("SQLITE_NOTFOUND", 12),
    ("SQLITE_FULL", 13),
    ("SQLITE_CANTOPEN", 14),
    ("SQLITE_PROTOCOL", 15),
    ("SQLITE_EMPTY", 16),
    ("SQLITE_SCHEMA", 17),
    ("SQLITE_TOOBIG", 18),
    ("SQLITE_CONSTRAINT", 19),
    ("SQLITE_MISMATCH", 20),
    ("SQLITE_MISUSE", 21),
    ("SQLITE_NOLFS", 22),
    ("SQLITE_AUTH", 23),
    ("SQLITE_FORMAT", 24),
    ("SQLITE_RANGE", 25),
    ("SQLITE_NOTADB", 26),
    ("SQLITE_NOTICE", 27),
    ("SQLITE_WARNING", 28),
    ("SQLITE_ROW", 100),
    ("SQLITE_DONE", 101),
    ("SQLITE_ERROR_MISSING_COLLSEQ", 257),
    ("SQLITE_ERROR_RETRY", 513),
    ("SQLITE_ERROR_SNAPSHOT", 769),
    ("SQLITE_IOERR_READ", 266),
    ("SQLITE_IOERR_SHORT_READ", 522),
    ("SQLITE_IOERR_WRITE", 778),
    ("SQLITE_IOERR_FSYNC", 1034),
    ("SQLITE_IOERR_DIR_FSYNC", 1290),
    ("SQLITE_IOERR_TRUNCATE", 1546),
    ("SQLITE_IOERR_FSTAT", 1802),
    ("SQLITE_IOERR_UNLOCK", 2058),
    ("SQLITE_IOERR_RDLOCK", 2314),
    ("SQLITE_IOERR_DELETE", 2570),
    ("SQLITE_IOERR_BLOCKED", 2826),
    ("SQLITE_IOERR_NOMEM", 3082),
    ("SQLITE_IOERR_ACCESS", 3338),
    ("SQLITE_IOERR_CHECKRESERVEDLOCK", 3594),
    ("SQLITE_IOERR_LOCK", 3850),
    ("SQLITE_IOERR_CLOSE", 4106),
    ("SQLITE_IOERR_DIR_CLOSE", 4362),
    ("SQLITE_IOERR_SHMOPEN", 4618),
    ("SQLITE_IOERR_SHMSIZE", 4874),
    ("SQLITE_IOERR_SHMLOCK", 5130),
    ("SQLITE_IOERR_SHMMAP", 5386),
    ("SQLITE_IOERR_SEEK", 5642),
    ("SQLITE_IOERR_DELETE_NOENT", 5898),
    ("SQLITE_IOERR_MMAP", 6154),
    ("SQLITE_IOERR_GETTEMPPATH", 6410),
    ("SQLITE_IOERR_CONVPATH", 6666),
    ("SQLITE_IOERR_VNODE", 6922),
    ("SQLITE_IOERR_AUTH", 7178),
    ("SQLITE_IOERR_BEGIN_ATOMIC", 7434),
    ("SQLITE_IOERR_COMMIT_ATOMIC", 7690),
    ("SQLITE_IOERR_ROLLBACK_ATOMIC", 7946),
    ("SQLITE_IOERR_DATA", 8202),
    ("SQLITE_IOERR_CORRUPTFS", 8458),
    ("SQLITE_IOERR_IN_PAGE", 8714),
    ("SQLITE_LOCKED_SHAREDCACHE", 262),
    ("SQLITE_LOCKED_VTAB", 518),
    ("SQLITE_BUSY_RECOVERY", 261),
    ("SQLITE_BUSY_SNAPSHOT", 517),
    ("SQLITE_CANTOPEN_NOTEMPDIR", 270),
    ("SQLITE_CANTOPEN_ISDIR", 526),
    ("SQLITE_CANTOPEN_FULLPATH", 782),
    ("SQLITE_CANTOPEN_CONVPATH", 1038),
    ("SQLITE_CANTOPEN_DIRTYWAL", 1294),
    ("SQLITE_CANTOPEN_SYMLINK", 1550),
    ("SQLITE_CORRUPT_VTAB", 267),
    ("SQLITE_CORRUPT_SEQUENCE", 523),
    ("SQLITE_CORRUPT_INDEX", 779),
    ("SQLITE_READONLY_RECOVERY", 264),
    ("SQLITE_READONLY_CANTLOCK", 520),
    ("SQLITE_READONLY_ROLLBACK", 776),
    ("SQLITE_READONLY_DBMOVED", 1032),
    ("SQLITE_READONLY_CANTINIT", 1288),
    ("SQLITE_READONLY_DIRECTORY", 1544),
    ("SQLITE_ABORT_ROLLBACK", 516),
    ("SQLITE_CONSTRAINT_CHECK", 275),
    ("SQLITE_CONSTRAINT_COMMITHOOK", 531),
    ("SQLITE_CONSTRAINT_FOREIGNKEY", 787),
    ("SQLITE_CONSTRAINT_FUNCTION", 1043),
    ("SQLITE_CONSTRAINT_NOTNULL", 1299),
    ("SQLITE_CONSTRAINT_PRIMARYKEY", 1555),
    ("SQLITE_CONSTRAINT_TRIGGER", 1811),
    ("SQLITE_CONSTRAINT_UNIQUE", 2067),
    ("SQLITE_CONSTRAINT_VTAB", 2323),
    ("SQLITE_CONSTRAINT_ROWID", 2579),
    ("SQLITE_CONSTRAINT_PINNED", 2835),
    ("SQLITE_CONSTRAINT_DATATYPE", 3091),
    ("SQLITE_NOTICE_RECOVER_WAL", 283),
    ("SQLITE_NOTICE_RECOVER_ROLLBACK", 539),
    ("SQLITE_NOTICE_RBU", 795),
    ("SQLITE_WARNING_AUTOINDEX", 284),
    ("SQLITE_AUTH_USER", 279),
    ("SQLITE_OK_LOAD_PERMANENTLY", 256),
    ("SQLITE_OK_SYMLINK", 512),
];

/// better-sqlite3 `CS::Code(code)`.
pub(crate) fn sqlite_code_name(code: i32) -> String {
    SQLITE_CODE_NAMES
        .iter()
        .find(|(_, c)| *c == code)
        .map(|(n, _)| (*n).to_string())
        .unwrap_or_else(|| format!("UNKNOWN_SQLITE_ERROR_{code}"))
}

/// v4's `err.code || 'error'` for a throw out of `db.pragma(...)`: a SQLite
/// failure carries its extended code's name; anything else is `error`.
fn err_code(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(f, _) => sqlite_code_name(f.extended_code),
        _ => "error".to_string(),
    }
}

/// v4 `cmdIntegrity`.
pub(crate) fn cmd_integrity(args: &[String], ctx: &Ctx) -> Result<(), IntegrityExit> {
    let (flags, positional) = parse_sub_args(args);
    let json = as_bool(flags.get("json"));

    let keys: Vec<&str> = INTEGRITY_TARGETS.iter().map(|t| t.0).collect();
    let targets: Vec<&(&str, &str, &str)> =
        if positional.is_empty() || (positional.len() == 1 && positional[0] == "all") {
            INTEGRITY_TARGETS.iter().collect()
        } else {
            let mut out = Vec::new();
            for p in &positional {
                match INTEGRITY_TARGETS.iter().find(|t| t.0 == p) {
                    Some(t) => out.push(t),
                    None => {
                        return Err(IntegrityExit::Loud(CmdError {
                            message: format!(
                                "Unknown integrity target '{p}'. Allowed: {} | all",
                                keys.join(" | ")
                            ),
                            exit_code: 1,
                        }))
                    }
                }
            }
            out
        };

    // Read-only — safe alongside a running instance. Log it once if active.
    if !json {
        if let Some(pid) = crate::db_cmd::live_instance_pid_part(&ctx.data_dir) {
            out::log(&format!(
                "Live instance detected{pid} — running read-only checks."
            ));
        }
    }

    let mut results: Vec<Value> = Vec::new();
    for (key, filename, friendly) in targets {
        let db_path = node_join(&ctx.data_dir, filename);
        if !std::path::Path::new(&db_path).exists() {
            if !json {
                out::log(&format!("Skipping {key}: {db_path} not found."));
            }
            let mut r = Map::new();
            r.insert("target".into(), Value::from(*key));
            r.insert("ok".into(), Value::Bool(false));
            r.insert("openable".into(), Value::Bool(false));
            r.insert("issues".into(), Value::Array(Vec::new()));
            r.insert("reason".into(), Value::from("not found"));
            results.push(Value::Object(r));
            continue;
        }
        results.push(integrity_one_db(key, &db_path, friendly, ctx));
    }

    // v4: an open failure is `openable === false` with any reason but
    // `'not found'`; an issue is `openable !== false && !ok`. So a MISSING
    // file fails neither test — it is skipped, not failed.
    let any_open_failed = results
        .iter()
        .any(|r| r["openable"] == Value::Bool(false) && r["reason"] != "not found");
    let any_issues = results
        .iter()
        .any(|r| r["openable"] != Value::Bool(false) && r["ok"] != Value::Bool(true));

    if json {
        let mut o = Map::new();
        o.insert("results".into(), Value::Array(results));
        print_json(&Value::Object(o));
    } else {
        out::log("");
        if !any_issues && !any_open_failed {
            out::log("All databases reported ok.");
        } else if any_open_failed {
            out::log("One or more databases could not be opened.");
        } else {
            out::log("One or more databases reported integrity issues \u{2014} see above.");
        }
    }

    if any_open_failed {
        return Err(IntegrityExit::Silent(2));
    }
    if any_issues {
        return Err(IntegrityExit::Silent(1));
    }
    Ok(())
}

/// v4 `integrityOneDb` — prints its block whether or not `--json` was given
/// (v4 does; the JSON follows the blocks), and answers the result object in
/// v4's key order.
fn integrity_one_db(label: &str, db_path: &str, friendly: &str, ctx: &Ctx) -> Value {
    out::log("");
    out::log(&format!(
        "\u{2500}\u{2500} {label}  ({db_path}) \u{2500}\u{2500}"
    ));

    let mut r = Map::new();
    r.insert("target".into(), Value::from(label));
    let conn = match open_encrypted(
        db_path,
        ctx.pepper.as_deref(),
        OpenOptions {
            readonly: true,
            friendly_name: friendly,
        },
    ) {
        Ok(c) => c,
        Err(e) => {
            out::log(&format!("  open failed: {}", e.message));
            r.insert("ok".into(), Value::Bool(false));
            r.insert("openable".into(), Value::Bool(false));
            r.insert("issues".into(), Value::Array(Vec::new()));
            r.insert("reason".into(), Value::from(e.message));
            return Value::Object(r);
        }
    };

    // v4's why (`db-commands.js:1744-1749`): SQLite3 Multiple Ciphers
    // (ChaCha20-Poly1305) has no SQLCipher-style `cipher_integrity_check`
    // pragma — an unknown pragma returns no rows, so asking for it reported
    // "ok" without checking anything. The page-level check is integrity_check
    // itself: every page it reads must decrypt and pass its Poly1305 tag, and
    // a page that fails THROWS (SQLITE_CORRUPT / SQLITE_NOTADB) rather than
    // returning a row, so that is caught as an issue.
    let started = std::time::Instant::now();
    let read: Result<Vec<String>, rusqlite::Error> = (|| {
        let mut st = conn.prepare("PRAGMA integrity_check")?;
        let rows = st.query_map([], |row| row.get::<_, Option<String>>(0))?;
        let mut lines = Vec::new();
        for row in rows {
            // `r.integrity_check || … || ''` then `.filter(Boolean)`.
            if let Some(line) = row?.filter(|l| !l.is_empty()) {
                lines.push(line);
            }
        }
        Ok(lines)
    })();
    let lines = match read {
        Ok(lines) => lines,
        Err(e) => vec![format!(
            "page read failed ({}): {}",
            err_code(&e),
            sqlite_msg(&e)
        )],
    };
    let mut issues = Vec::new();
    let integrity_check = if lines.len() == 1 && lines[0] == "ok" {
        out::log("  integrity_check: ok");
        "ok".to_string()
    } else {
        for line in &lines {
            out::log(&format!("  integrity_check: {line}"));
            let mut issue = Map::new();
            issue.insert("pragma".into(), Value::from("integrity_check"));
            issue.insert("message".into(), Value::from(line.as_str()));
            issues.push(Value::Object(issue));
        }
        lines.join("; ")
    };
    drop(conn);
    let duration_ms = started.elapsed().as_millis() as u64;
    out::log(&format!(
        "  duration: {}",
        crate::db_cmd::format_duration(duration_ms)
    ));

    r.insert("ok".into(), Value::Bool(integrity_check == "ok"));
    r.insert("openable".into(), Value::Bool(true));
    r.insert("integrityCheck".into(), Value::from(integrity_check));
    r.insert("issues".into(), Value::Array(issues));
    r.insert("durationMs".into(), Value::from(duration_ms));
    Value::Object(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_names_follow_the_driver() {
        assert_eq!(sqlite_code_name(11), "SQLITE_CORRUPT");
        assert_eq!(sqlite_code_name(26), "SQLITE_NOTADB");
        assert_eq!(sqlite_code_name(266), "SQLITE_IOERR_READ");
        assert_eq!(sqlite_code_name(9999), "UNKNOWN_SQLITE_ERROR_9999");
        assert_eq!(SQLITE_CODE_NAMES.len(), 107);
    }

    /// The `#define` value of `name` in the vendored SQLite3MC header, with
    /// the extended-code form `(SQLITE_BASE | (n<<8))` resolved.
    fn header_value(header: &str, name: &str) -> Option<i32> {
        let line = header.lines().find(|l| {
            l.strip_prefix("#define ")
                .and_then(|r| r.strip_prefix(name))
                .is_some_and(|r| r.starts_with([' ', '\t']))
        })?;
        let raw = line["#define ".len() + name.len()..].trim();
        let raw = raw.split("/*").next().unwrap().trim();
        if let Ok(n) = raw.parse::<i32>() {
            return Some(n);
        }
        let inner = raw.strip_prefix('(')?.strip_suffix(')')?;
        let (base, shift) = inner.split_once('|')?;
        let shift = shift.trim().strip_prefix('(')?.strip_suffix("<<8)")?;
        Some(header_value(header, base.trim())? | (shift.trim().parse::<i32>().ok()? << 8))
    }

    /// `cli_sqlite_code_names_guard` (P4.D365): [`SQLITE_CODE_NAMES`] is
    /// better-sqlite3's `constants.cpp` code table WHOLE — the same names in
    /// the same order — read from the round pin's install
    /// (`QT_V4_CHECKOUT/node_modules/better-sqlite3/src/util/constants.cpp`),
    /// and every value equals the vendored 2.4.0 header's. Skips loudly
    /// without `QT_V4_CHECKOUT` (the header half still runs).
    #[test]
    fn cli_sqlite_code_names_guard() {
        let header_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../quilltap-sqlite3mc-sys/vendor/sqlite3mc_amalgamation.h");
        let header = std::fs::read_to_string(&header_path).expect("the vendored header");
        for (name, code) in SQLITE_CODE_NAMES {
            assert_eq!(header_value(&header, name), Some(*code), "{name}");
        }
        let Ok(v4) = std::env::var("QT_V4_CHECKOUT") else {
            eprintln!("SKIP: cli_sqlite_code_names_guard: QT_V4_CHECKOUT unset (the pin's constants.cpp half)");
            return;
        };
        let cpp_path =
            std::path::Path::new(&v4).join("node_modules/better-sqlite3/src/util/constants.cpp");
        let cpp = std::fs::read_to_string(&cpp_path)
            .unwrap_or_else(|e| panic!("{}: {e}", cpp_path.display()));
        let names: Vec<&str> = cpp
            .lines()
            .filter_map(|l| l.trim().strip_prefix("SetCode(env, "))
            .map(|r| {
                let quoted = r.split('"').nth(1).expect("a quoted name");
                assert!(
                    r.starts_with(&format!("{quoted},")),
                    "{r}: constant and name differ"
                );
                quoted
            })
            .collect();
        let ours: Vec<&str> = SQLITE_CODE_NAMES.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            ours,
            names,
            "SQLITE_CODE_NAMES drifted from {}",
            cpp_path.display()
        );
        assert!(
            cpp.contains("std::string(\"UNKNOWN_SQLITE_ERROR_\") + std::to_string(code)"),
            "the driver's unknown-code fallback moved"
        );
        eprintln!(
            "OK: cli_sqlite_code_names_guard — {} codes match the pin",
            names.len()
        );
    }
}
