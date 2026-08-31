//! `quilltap db` — the legacy flag path (v4 `bin/quilltap.js` `dbCommand`,
//! lines 790–1099) + the instance-lock CLI commands (`handleLockCommand`,
//! lines 465–685). The high-level verbs (`schema`, `find`, …) are recognized
//! (v4 `db-commands.js` `VERBS`) and exit loud until their round lands.

use serde_json::{Map, Value};

use quilltap_core::clock;
use quilltap_host::instances::InstanceRegistry;
use quilltap_host::lock::{
    acquire_write_lock, is_pid_alive, release_write_lock, verify_pid_is_quilltap,
};

use crate::nodefmt::{cell_to_js_value, json_stringify_pretty, node_join};
use crate::out;
use crate::resolve::{load_db_key, print_default_instance_hint, resolve_data_dir_and_passphrase};
use crate::vtable::console_table;

const DB_HELP: &str = include_str!("help/db_help.txt");

/// v4 `db-commands.js` `VERBS` — the high-level verb set. All recognized;
/// none shipped this round (they exit loud).
const DB_VERBS: &[&str] = &[
    "schema",
    "find",
    "chats",
    "messages",
    "logs",
    "message",
    "log",
    "memories",
    "characters",
    "optimize",
    "backup",
    "integrity",
];

pub fn run(args: &[String]) -> i32 {
    // ---- Strip --data-dir / --passphrase / --instance (anywhere on the line).
    let mut cleaned: Vec<String> = Vec::new();
    let mut data_dir_override = String::new();
    let mut passphrase = String::new();
    let mut instance_name = String::new();
    let mut j = 0;
    while j < args.len() {
        let a = args[j].as_str();
        match a {
            "--data-dir" | "-d" => {
                j += 1;
                data_dir_override = args.get(j).cloned().unwrap_or_default();
            }
            "--passphrase" => {
                j += 1;
                passphrase = args.get(j).cloned().unwrap_or_default();
            }
            "--instance" | "-i" => {
                j += 1;
                instance_name = args.get(j).cloned().unwrap_or_default();
            }
            _ => cleaned.push(args[j].clone()),
        }
        j += 1;
    }

    // ---- Verb pre-flight (v4 `dbCommand`: the first BARE token decides
    //      whether this is the verb path; the verb itself is then `args[0]`,
    //      so a leading flag makes `runVerb` throw "Unknown db subcommand").
    if let Some(first_positional) = cleaned.iter().find(|a| !a.starts_with('-')) {
        if DB_VERBS.contains(&first_positional.as_str()) {
            return run_verb_path(&cleaned, &data_dir_override, &instance_name, &passphrase);
        }
    }

    // ---- Legacy flag-based path.
    let mut use_llm_logs = false;
    let mut use_mount_points = false;
    let mut show_tables = false;
    let mut count_table = String::new();
    let mut repl = false;
    let mut writable = false;
    let mut sql = String::new();
    let mut show_help = false;
    let mut lock_status = false;
    let mut lock_clean = false;
    let mut lock_override = false;
    let mut as_json = false;

    let mut i = 0;
    while i < cleaned.len() {
        match cleaned[i].as_str() {
            "--llm-logs" => use_llm_logs = true,
            "--mount-points" => use_mount_points = true,
            "--tables" => show_tables = true,
            "--count" => {
                i += 1;
                count_table = cleaned.get(i).cloned().unwrap_or_default();
            }
            "--repl" => repl = true,
            "--write" => writable = true,
            "--json" => as_json = true,
            "--help" | "-h" => show_help = true,
            "--lock-status" => lock_status = true,
            "--lock-clean" => lock_clean = true,
            "--lock-override" => lock_override = true,
            other => {
                if other.starts_with('-') {
                    out::elog(&format!("Unknown option: {other}"));
                    out::exit(1);
                }
                sql = other.to_string();
            }
        }
        i += 1;
    }

    if show_help {
        out::write_stdout(DB_HELP.as_bytes());
        out::exit(0);
    }

    if repl {
        out::elog(
            "Error: db --repl is recognized but not yet available in this build of the quilltap CLI.",
        );
        out::exit(1);
    }

    let registry = InstanceRegistry::at_default_location();
    let resolved = match resolve_data_dir_and_passphrase(
        &data_dir_override,
        &instance_name,
        &passphrase,
        &registry,
    ) {
        Ok(r) => r,
        Err(e) => {
            out::elog(&format!("Error: {e}"));
            out::exit(1);
        }
    };
    print_default_instance_hint(&resolved, &registry);
    let data_dir = resolved.data_dir.clone();
    let passphrase = resolved.passphrase.clone();

    // ---- Instance lock commands (no database open required).
    if lock_status || lock_clean || lock_override {
        handle_lock_command(&data_dir, lock_status, lock_clean, lock_override);
        return 0;
    }

    if use_llm_logs && use_mount_points {
        out::elog("Error: --llm-logs and --mount-points are mutually exclusive");
        out::exit(1);
    }

    let db_filename = if use_llm_logs {
        "quilltap-llm-logs.db"
    } else if use_mount_points {
        "quilltap-mount-index.db"
    } else {
        "quilltap.db"
    };
    let db_path = node_join(&data_dir, db_filename);

    if !std::path::Path::new(&db_path).exists() {
        out::elog(&format!("Database not found: {db_path}"));
        out::exit(1);
    }

    let pepper = match load_db_key(&data_dir, &passphrase) {
        Ok(p) => p,
        Err(e) => {
            out::elog(&format!("Error: {e}"));
            out::exit(1);
        }
    };

    // Read-write opens must claim the instance lock first (no override).
    if writable {
        if let Err(e) = acquire_write_lock(std::path::Path::new(&data_dir)) {
            out::elog(&e.to_string());
            out::exit(1);
        }
    }

    let conn = match open_encrypted(&db_path, pepper.as_deref(), writable) {
        Ok(c) => c,
        Err(OpenError::Open(e)) => {
            // better-sqlite3's constructor throw (e.g. unreadable file).
            out::elog(&format!("Error: {e}"));
            if writable {
                release_write_lock(std::path::Path::new(&data_dir));
            }
            out::exit(1);
        }
        Err(OpenError::Verify(e)) => {
            out::elog(&format!("Cannot open database: {e}"));
            out::elog("The database may be encrypted with a different key, or the .dbkey file may be missing.");
            if writable {
                release_write_lock(std::path::Path::new(&data_dir));
            }
            out::exit(1);
        }
    };

    // ---- Dispatch (v4's try/catch → readonly hint or `Error:` + exitCode 1).
    let result = dispatch(&conn, show_tables, &count_table, &sql, as_json);
    let mut exit_code = 0;
    if let Err(msg) = result {
        let lower = msg.to_lowercase();
        if !writable && (lower.contains("readonly") || lower.contains("read-only")) {
            out::elog("This database was opened read-only, so it cannot be modified.");
            out::elog(
                "Re-run with --write to make changes — it claims the instance lock while it runs",
            );
            out::elog(
                "and refuses if another Quilltap instance is using the database. For example:",
            );
            let example = if sql.is_empty() {
                "\"UPDATE ...\"".to_string()
            } else {
                serde_json::to_string(&sql).unwrap_or_default()
            };
            out::elog(&format!("  quilltap db --write {example}"));
        } else {
            out::elog(&format!("Error: {msg}"));
        }
        exit_code = 1;
    }

    drop(conn);
    if writable {
        release_write_lock(std::path::Path::new(&data_dir));
    }
    exit_code
}

/// v4's verb path (`dbCommand`'s first branch): resolve the data dir, print
/// the default-instance hint, unlock the pepper, build the ctx, then
/// `runVerb`. Errors surface as `Error: <message>` with v4's exit code
/// (`err.exitCode ?? (err.ambiguous ? 2 : 1)`).
fn run_verb_path(
    cleaned: &[String],
    data_dir_override: &str,
    instance_name: &str,
    passphrase: &str,
) -> i32 {
    let registry = InstanceRegistry::at_default_location();
    let resolved = match resolve_data_dir_and_passphrase(
        data_dir_override,
        instance_name,
        passphrase,
        &registry,
    ) {
        Ok(r) => r,
        Err(e) => {
            out::elog(&format!("Error: {e}"));
            out::exit(1);
        }
    };
    print_default_instance_hint(&resolved, &registry);
    let pepper = match load_db_key(&resolved.data_dir, &resolved.passphrase) {
        Ok(p) => p,
        Err(e) => {
            out::elog(&format!("Error: {e}"));
            out::exit(1);
        }
    };
    let ctx = crate::db_characters::Ctx {
        data_dir: resolved.data_dir.clone(),
        pepper,
    };

    // v4 `runVerb(args, ctx)`: `const [verb, ...rest] = args`.
    let verb = cleaned.first().map(String::as_str).unwrap_or("");
    let rest: Vec<String> = cleaned.iter().skip(1).cloned().collect();
    let result = match verb {
        "characters" => crate::db_characters::run(&rest, &ctx),
        other if DB_VERBS.contains(&other) => {
            out::elog(&format!(
                "Error: db subcommand '{other}' is recognized but not yet available in this build of the quilltap CLI."
            ));
            out::exit(1);
        }
        // The bare token that qualified this as the verb path was NOT first —
        // v4 looks up `args[0]` and throws on the miss.
        other => Err(crate::db_characters::CmdError {
            message: format!("Unknown db subcommand: {other}"),
            exit_code: 1,
        }),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            out::elog(&format!("Error: {}", e.message));
            e.exit_code
        }
    }
}

enum OpenError {
    Open(String),
    Verify(String),
}

/// The rusqlite error text better-sqlite3 would surface (`err.message`): the
/// engine message for a SQLite failure, the display form otherwise.
fn sqlite_msg(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(m)) => m.clone(),
        other => other.to_string(),
    }
}

/// Open an encrypted database exactly as v4's launcher does: read-only unless
/// `--write`; `PRAGMA key` (raw-hex) as the first and only pragma; verified
/// with `SELECT 1` before use (the read-path rule).
fn open_encrypted(
    db_path: &str,
    pepper: Option<&str>,
    writable: bool,
) -> Result<rusqlite::Connection, OpenError> {
    use rusqlite::OpenFlags;
    let flags = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let conn = rusqlite::Connection::open_with_flags(db_path, flags)
        .map_err(|e| OpenError::Open(sqlite_msg(&e)))?;
    if let Some(pepper) = pepper {
        let key_hex = quilltap_core::dbkey::pepper_b64_to_key_hex(pepper)
            .map_err(|e| OpenError::Open(e.to_string()))?;
        conn.pragma_update(None, "key", format!("x'{key_hex}'"))
            .map_err(|e| OpenError::Verify(sqlite_msg(&e)))?;
    }
    conn.query_row("SELECT 1", [], |_| Ok(()))
        .map_err(|e| OpenError::Verify(sqlite_msg(&e)))?;
    Ok(conn)
}

fn dispatch(
    conn: &rusqlite::Connection,
    show_tables: bool,
    count_table: &str,
    sql: &str,
    as_json: bool,
) -> Result<(), String> {
    if show_tables {
        let names = list_tables(conn)?;
        if as_json {
            out::log(&json_stringify_pretty(&Value::from(names)));
        } else {
            for name in names {
                out::log(&name);
            }
        }
        return Ok(());
    }
    if !count_table.is_empty() {
        let count: Value = conn
            .query_row(
                &format!("SELECT count(*) as count FROM \"{count_table}\""),
                [],
                |row| Ok(cell_to_js_value(row.get_ref(0).unwrap())),
            )
            .map_err(|e| sqlite_msg(&e))?;
        if as_json {
            let mut obj = Map::new();
            obj.insert("table".into(), Value::from(count_table));
            obj.insert("count".into(), count);
            out::log(&json_stringify_pretty(&Value::Object(obj)));
        } else {
            out::log(&serde_json::to_string(&count).unwrap_or_default());
        }
        return Ok(());
    }
    if !sql.is_empty() {
        let mut stmt = conn.prepare(sql).map_err(|e| sqlite_msg(&e))?;
        if stmt.column_count() > 0 {
            // Reader (v4 `stmt.reader` → `.all()`).
            let col_names: Vec<String> = stmt
                .column_names()
                .into_iter()
                .map(str::to_string)
                .collect();
            let mut rows_out: Vec<Map<String, Value>> = Vec::new();
            let mut rows = stmt.query([]).map_err(|e| sqlite_msg(&e))?;
            while let Some(row) = rows.next().map_err(|e| sqlite_msg(&e))? {
                let mut obj = Map::new();
                for (idx, name) in col_names.iter().enumerate() {
                    obj.insert(
                        name.clone(),
                        cell_to_js_value(row.get_ref(idx).map_err(|e| sqlite_msg(&e))?),
                    );
                }
                rows_out.push(obj);
            }
            if as_json {
                out::log(&json_stringify_pretty(&Value::from(
                    rows_out.into_iter().map(Value::Object).collect::<Vec<_>>(),
                )));
            } else if rows_out.is_empty() {
                out::log("(no results)");
            } else {
                out::write_stdout(console_table(&rows_out).as_bytes());
            }
        } else {
            // Writer (v4 `stmt.run()`).
            let changes = stmt.execute([]).map_err(|e| sqlite_msg(&e))?;
            if as_json {
                let mut obj = Map::new();
                obj.insert("changes".into(), Value::from(changes as i64));
                obj.insert(
                    "lastInsertRowid".into(),
                    Value::from(conn.last_insert_rowid()),
                );
                out::log(&json_stringify_pretty(&Value::Object(obj)));
            } else {
                out::log(&format!("Changes: {changes}"));
            }
        }
        return Ok(());
    }
    // No flags at all → the help (v4 prints it after opening the DB).
    out::write_stdout(DB_HELP.as_bytes());
    Ok(())
}

fn list_tables(conn: &rusqlite::Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .map_err(|e| sqlite_msg(&e))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| sqlite_msg(&e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| sqlite_msg(&e))?;
    Ok(names)
}

// ============================================================================
// Instance Lock CLI Commands (v4 bin/quilltap.js `handleLockCommand`)
// ============================================================================

/// The raw lock JSON, preserving unknown fields and key order (v4 mutates and
/// re-serializes the parsed object).
enum LockRead {
    Missing,
    Corrupt,
    Parsed(Map<String, Value>),
}

fn read_lock_value(lock_path: &str) -> LockRead {
    if !std::path::Path::new(lock_path).exists() {
        return LockRead::Missing;
    }
    match std::fs::read_to_string(lock_path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(obj)) => LockRead::Parsed(obj),
            Ok(_) | Err(_) => LockRead::Corrupt,
        },
        Err(_) => LockRead::Corrupt,
    }
}

fn lock_num(lock: &Map<String, Value>, key: &str) -> f64 {
    lock.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN)
}

fn lock_str<'a>(lock: &'a Map<String, Value>, key: &str) -> &'a str {
    lock.get(key).and_then(Value::as_str).unwrap_or("")
}

/// `lock.lastHeartbeat ? Date.now() - parse(...) : Infinity` in ms.
fn heartbeat_age_ms(lock: &Map<String, Value>) -> f64 {
    let hb = lock_str(lock, "lastHeartbeat");
    if hb.is_empty() {
        return f64::INFINITY;
    }
    match clock::iso_to_ms(hb) {
        Some(ms) => (clock::now_unix_ms() - ms) as f64,
        None => f64::NAN,
    }
}

/// v4 `lock-helpers.js`'s `VM_ENVIRONMENTS` set, which `1560bd43b` narrowed to
/// the single `docker` entry when the managed Lima/WSL2 modes were retired. A
/// lock still carrying a retired value takes the plain different-host arm.
fn is_vm_environment(lock: &Map<String, Value>) -> bool {
    lock_str(lock, "environment") == "docker"
}

fn push_history(lock: &mut Map<String, Value>, event: &str, detail: String) {
    let mut entry = Map::new();
    entry.insert("event".into(), Value::from(event));
    entry.insert("pid".into(), Value::from(std::process::id() as i64));
    entry.insert(
        "hostname".into(),
        Value::from(quilltap_host::lock::hostname()),
    );
    entry.insert("timestamp".into(), Value::from(clock::now_iso()));
    entry.insert("detail".into(), Value::from(detail));
    match lock.get_mut("history") {
        Some(Value::Array(arr)) => arr.push(Value::Object(entry)),
        _ => {
            lock.insert("history".into(), Value::from(vec![Value::Object(entry)]));
        }
    }
}

fn write_lock_value(lock_path: &str, lock: &Map<String, Value>) {
    // v4: JSON.stringify(lock, null, 2) + '\n' (best effort).
    let _ = std::fs::write(
        lock_path,
        format!("{}\n", json_stringify_pretty(&Value::Object(lock.clone()))),
    );
}

fn handle_lock_command(data_dir: &str, lock_status: bool, lock_clean: bool, lock_override: bool) {
    let lock_path = node_join(data_dir, "quilltap.lock");
    let hostname = quilltap_host::lock::hostname();

    let lock = match read_lock_value(&lock_path) {
        LockRead::Corrupt => {
            // v4's catch: only --lock-status reports (and only removes when
            // --lock-clean was ALSO passed); a bare --lock-clean or
            // --lock-override on a corrupt lock silently returns.
            if lock_status {
                out::log("Lock file exists but is corrupt or unreadable.");
                out::log(&format!("  Path: {lock_path}"));
                if lock_clean {
                    let _ = std::fs::remove_file(&lock_path);
                    out::log("  Removed corrupt lock file.");
                }
            }
            return;
        }
        LockRead::Missing => None,
        LockRead::Parsed(obj) => Some(obj),
    };

    // ---- --lock-status
    if lock_status {
        let Some(lock) = lock else {
            out::log("No instance lock found. Database is not currently claimed.");
            out::log(&format!("  Lock path: {lock_path}"));
            return;
        };
        let pid = lock_num(&lock, "pid");
        let same_host = lock_str(&lock, "hostname") == hostname;
        let alive = same_host && pid.is_finite() && is_pid_alive(pid as u32);
        let is_node = alive && verify_pid_is_quilltap(pid as u32);

        let status = if alive && is_node {
            "\x1b[32mACTIVE\x1b[0m (process confirmed running)".to_string()
        } else if alive && !is_node {
            "\x1b[33mSUSPECT\x1b[0m (PID alive but does not look like Quilltap — possible PID reuse)"
                .to_string()
        } else if !same_host {
            let is_vm = is_vm_environment(&lock);
            let age_ms = heartbeat_age_ms(&lock);
            const FRESH_MS: f64 = 5.0 * 60.0 * 1000.0;
            if is_vm && age_ms < FRESH_MS {
                let age_str = format!("{}s", (age_ms / 1000.0).round() as i64);
                format!(
                    "\x1b[32mACTIVE ({}, heartbeat {} ago)\x1b[0m",
                    lock_str(&lock, "environment"),
                    age_str
                )
            } else if is_vm {
                format!(
                    "\x1b[33mSTALE ({}, no recent heartbeat)\x1b[0m — will be auto-claimed on next startup",
                    lock_str(&lock, "environment")
                )
            } else {
                "\x1b[33mSTALE (different host)\x1b[0m — will be auto-claimed on next startup"
                    .to_string()
            }
        } else {
            "\x1b[31mSTALE (process dead)\x1b[0m — will be auto-claimed on next startup".to_string()
        };

        out::log(&format!("Instance Lock Status: {status}"));
        out::log("");
        out::log(&format!(
            "  PID:          {}",
            crate::nodefmt::js_num_string(pid)
        ));
        out::log(&format!(
            "  Hostname:     {}{}",
            lock_str(&lock, "hostname"),
            if same_host {
                " (this host)"
            } else {
                " (different host)"
            }
        ));
        out::log(&format!(
            "  Environment:  {}",
            non_empty_or(lock_str(&lock, "environment"), "unknown")
        ));
        out::log(&format!(
            "  Process:      {}",
            non_empty_or(lock_str(&lock, "processTitle"), "unknown")
        ));
        out::log(&format!(
            "  Started:      {}",
            non_empty_or(lock_str(&lock, "startedAt"), "unknown")
        ));

        if !lock_str(&lock, "lastHeartbeat").is_empty() {
            let age_s = (heartbeat_age_ms(&lock) / 1000.0).round();
            let mut display = if age_s < 120.0 {
                format!("{}s ago", crate::nodefmt::js_num_string(age_s))
            } else if age_s < 7200.0 {
                format!(
                    "{}m ago",
                    crate::nodefmt::js_num_string((age_s / 60.0).round())
                )
            } else {
                format!(
                    "{}h ago",
                    crate::nodefmt::js_num_string((age_s / 3600.0).round())
                )
            };
            if alive && age_s > 300.0 {
                display = format!("\x1b[33m{display} (stale — process may be hung)\x1b[0m");
            }
            out::log(&format!("  Heartbeat:    {display}"));
        }

        out::log(&format!("  Lock file:    {lock_path}"));

        let history = lock.get("history").and_then(Value::as_array);
        if let Some(history) = history.filter(|h| !h.is_empty()) {
            out::log("");
            out::log(&format!("  Recent history ({} entries):", history.len()));
            let start = history.len().saturating_sub(10);
            for entry in &history[start..] {
                let ts_raw = entry.get("timestamp").and_then(Value::as_str).unwrap_or("");
                let ts = if ts_raw.is_empty() {
                    "?".to_string()
                } else {
                    format_history_ts(ts_raw)
                };
                let detail = entry
                    .get("detail")
                    .and_then(Value::as_str)
                    .filter(|d| !d.is_empty())
                    .map(|d| format!(" — {d}"))
                    .unwrap_or_default();
                let event = entry.get("event").and_then(Value::as_str).unwrap_or("");
                let pid = entry
                    .get("pid")
                    .and_then(Value::as_f64)
                    .map(crate::nodefmt::js_num_string)
                    .unwrap_or_else(|| "undefined".to_string());
                out::log(&format!("    [{ts}] {event} (PID {pid}){detail}"));
            }
            if history.len() > 10 {
                out::log(&format!(
                    "    ... and {} earlier entries",
                    history.len() - 10
                ));
            }
        }
        return;
    }

    // ---- --lock-clean
    if lock_clean {
        let Some(mut lock) = lock else {
            out::log("No lock file found. Nothing to clean.");
            return;
        };
        let pid = lock_num(&lock, "pid");
        let same_host = lock_str(&lock, "hostname") == hostname;
        let alive = same_host && pid.is_finite() && is_pid_alive(pid as u32);

        if alive {
            let is_node = verify_pid_is_quilltap(pid as u32);
            if is_node {
                out::log(&format!(
                    "Lock is held by a live Quilltap process (PID {}). Cannot clean.",
                    crate::nodefmt::js_num_string(pid)
                ));
                out::log("Stop the running instance first, or use --lock-override to force.");
                out::exit(1);
            } else {
                out::log(&format!(
                    "Lock references PID {} which is alive but does NOT look like a Quilltap process.",
                    crate::nodefmt::js_num_string(pid)
                ));
                out::log("This is likely a stale lock with a reused PID. Removing.");
            }
        } else if !same_host {
            let is_vm = is_vm_environment(&lock);
            let age_ms = heartbeat_age_ms(&lock);
            const FRESH_MS: f64 = 5.0 * 60.0 * 1000.0;
            if is_vm && age_ms < FRESH_MS {
                out::log(&format!(
                    "Lock is held by a live {} instance (heartbeat {}s ago). Cannot clean.",
                    lock_str(&lock, "environment"),
                    (age_ms / 1000.0).round() as i64
                ));
                out::log("Stop the other instance first, or use --lock-override to force.");
                out::exit(1);
            } else if is_vm {
                out::log(&format!(
                    "Lock was held by {} ({}) with no recent heartbeat. Removing stale lock.",
                    lock_str(&lock, "environment"),
                    lock_str(&lock, "hostname")
                ));
            } else {
                out::log(&format!(
                    "Lock was held by a different host ({}). Removing stale lock.",
                    lock_str(&lock, "hostname")
                ));
            }
        } else {
            out::log(&format!(
                "Lock was held by PID {} which is no longer running. Removing stale lock.",
                crate::nodefmt::js_num_string(pid)
            ));
        }

        push_history(
            &mut lock,
            "stale-claimed",
            "Cleaned via CLI (quilltap db --lock-clean)".to_string(),
        );
        write_lock_value(&lock_path, &lock);
        match std::fs::remove_file(&lock_path) {
            Ok(()) => out::log("Lock file removed."),
            Err(e) => {
                out::elog(&format!("Failed to remove lock file: {e}"));
                out::exit(1);
            }
        }
        return;
    }

    // ---- --lock-override
    if lock_override {
        let Some(mut lock) = lock else {
            out::log("No lock file found. Nothing to override.");
            return;
        };
        let pid = lock_num(&lock, "pid");
        let same_host = lock_str(&lock, "hostname") == hostname;
        let alive = same_host && pid.is_finite() && is_pid_alive(pid as u32);

        if alive {
            let is_node = verify_pid_is_quilltap(pid as u32);
            if !is_node {
                out::elog(&format!(
                    "Lock override rejected: PID {} is alive but does not appear to be",
                    crate::nodefmt::js_num_string(pid)
                ));
                out::elog(
                    "a Quilltap/Node process. The PID may have been reused. Verify manually.",
                );
                out::exit(1);
            }
            out::log(&format!(
                "WARNING: Overriding lock held by live process (PID {}, {}).",
                crate::nodefmt::js_num_string(pid),
                non_empty_or(lock_str(&lock, "environment"), "unknown")
            ));
            out::log("The other instance may corrupt the database if it is still writing.");
        } else {
            out::log(&format!(
                "Overriding stale lock (PID {} is no longer running).",
                crate::nodefmt::js_num_string(pid)
            ));
        }

        let detail = format!(
            "Manual override via CLI (quilltap db --lock-override){}",
            if alive {
                format!(
                    " — overriding live PID {}",
                    crate::nodefmt::js_num_string(pid)
                )
            } else {
                format!(" — PID {} was dead", crate::nodefmt::js_num_string(pid))
            }
        );
        push_history(&mut lock, "override", detail);
        write_lock_value(&lock_path, &lock);
        match std::fs::remove_file(&lock_path) {
            Ok(()) => {
                out::log("Lock file removed. Next Quilltap startup will acquire a fresh lock.")
            }
            Err(e) => {
                out::elog(&format!("Failed to remove lock file: {e}"));
                out::exit(1);
            }
        }
    }
}

fn non_empty_or<'a>(s: &'a str, fallback: &'a str) -> &'a str {
    if s.is_empty() {
        fallback
    } else {
        s
    }
}

/// v4: `entry.timestamp.replace('T', ' ').replace(/\.\d+Z$/, 'Z')` — the
/// FIRST `T` only (JS string-arg replace).
fn format_history_ts(ts: &str) -> String {
    let spaced = match ts.find('T') {
        Some(idx) => format!("{} {}", &ts[..idx], &ts[idx + 1..]),
        None => ts.to_string(),
    };
    // Strip a trailing `.digits` before a final Z.
    if let Some(dot) = spaced.rfind('.') {
        let tail = &spaced[dot + 1..];
        if tail.len() >= 2
            && tail.ends_with('Z')
            && tail[..tail.len() - 1].chars().all(|c| c.is_ascii_digit())
            && tail.len() > 1
        {
            return format!("{}Z", &spaced[..dot]);
        }
    }
    spaced
}
