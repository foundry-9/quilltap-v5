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

use crate::dbopen::{open_encrypted, sqlite_msg, OpenOptions};
use crate::nodefmt::{
    cell_to_js_value, json_stringify_pretty, node_join, raw_sql_cell_to_js_value,
};
use crate::out;
use crate::resolve::{load_db_key, print_default_instance_hint, resolve_data_dir_and_passphrase};
use crate::vtable::console_table;

const DB_HELP: &str = include_str!("help/db_help.txt");

/// v4 `db-commands.js` `VERBS` — the high-level verb set. All recognized;
/// `characters` (P4.D66) and `optimize` (P4.D259) ship, the rest exit loud.
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

    // BANKED (P4.D240): v4's `--repl` SQL arm runs the SAME bug-173 decode
    // (`decodeCompressedTextInRows(rows)` at `bin/quilltap.js:1116`, a second
    // site beside the raw-SQL one); whoever ports the REPL must carry it
    // through `nodefmt::raw_sql_cell_to_js_value`.
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

    // v4 `a2db63da7`: the filename AND the name the opener's refusals use.
    let (db_filename, db_friendly_name) = if use_llm_logs {
        ("quilltap-llm-logs.db", "LLM logs database")
    } else if use_mount_points {
        ("quilltap-mount-index.db", "mount index database")
    } else {
        ("quilltap.db", "main database")
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

    // Open through the ONE shared opener (v4 `a2db63da7`, bug 162): it keys,
    // verifies and registers `qt_text()`. Its failure message is printed BARE
    // — v4's catch is `console.error(err.message)`, no `Error:` prefix, no hint
    // of its own (the probe arm's message already carries the hint line).
    let conn = match open_encrypted(
        &db_path,
        pepper.as_deref(),
        OpenOptions {
            readonly: !writable,
            friendly_name: db_friendly_name,
        },
    ) {
        Ok(c) => c,
        Err(e) => {
            out::elog(&e.message);
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
        // P4.D259 (Tier 2): the optimize verb, over core's own step runner —
        // the one the server's daily PHASE 0.75 pass runs.
        "optimize" => cmd_optimize(&rest, &ctx),
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
                    // Compressed text columns arrive as Buffers; print them as
                    // text (v4 bug 173, `decodeCompressedTextInRows` at
                    // `bin/quilltap.js:1047`, before the `--json` / `(no
                    // results)` / table printers). Scoped to THIS site (v4
                    // `ddf942635`; it retires P4.D203's Buffer pin): the
                    // other `cell_to_js_value` callers keep the Buffer form.
                    obj.insert(
                        name.clone(),
                        raw_sql_cell_to_js_value(row.get_ref(idx).map_err(|e| sqlite_msg(&e))?),
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

/// v4 `assessLock(lock, hostname)` — the one shared decision `--lock-status`
/// and `--lock-clean` both read (v4 `25f534c0b`, bug 126).
///
/// A hostname that differs from ours does NOT mean a different machine: macOS
/// derives `gethostname()` dynamically when `scutil --get HostName` is unset,
/// so one Mac reports e.g. "MacBook-Pro.local" and "Mac" at different times.
/// So `alive` checks PID liveness **regardless of the recorded name** (it was
/// `sameHost && isPidAlive(...)` before), and the fallback is heartbeat
/// freshness for any environment rather than only for containers.
///
/// v4's returned object also carries `live = (alive && isNode) ||
/// heartbeatFresh`; measured at the port, NEITHER verb destructures it, so it
/// has no v5 counterpart (a field nothing reads is a `dead_code` warning here).
struct LockAssessment {
    same_host: bool,
    alive: bool,
    is_node: bool,
    heartbeat_age_ms: f64,
    heartbeat_fresh: bool,
}

/// How long a heartbeat stays "fresh". A lock whose heartbeat is younger than
/// this counts as HELD whatever its process is doing — see
/// [`lock_clean_refusal_lines`] for why that distinction is load-bearing in the
/// wording.
const FRESH_MS: f64 = 5.0 * 60.0 * 1000.0;

/// The freshness window in words, derived from the constant the check actually
/// uses — so the sentence `--lock-clean` prints cannot drift away from the
/// behaviour it describes. v4 `23abc1ba1`'s `describeFreshWindow()`
/// (`packages/quilltap/bin/quilltap.js:489-498`), arm for arm.
///
/// The window is a parameter so the JS arithmetic can be pinned at values
/// [`FRESH_MS`] never takes ([`describe_fresh_window`] is the production call).
/// `Math.round` first, then `% 60` on the ROUNDED number — v4's order, which is
/// what makes `0` answer `0 minutes` rather than `0 seconds`.
fn describe_fresh_window_ms(ms: f64) -> String {
    let seconds = quilltap_core::jsnum::math_round(ms / 1000.0);
    if seconds % 60.0 != 0.0 {
        return format!("{} seconds", crate::nodefmt::js_num_string(seconds));
    }
    let minutes = seconds / 60.0;
    if minutes == 1.0 {
        "1 minute".to_string()
    } else {
        format!("{} minutes", crate::nodefmt::js_num_string(minutes))
    }
}

fn describe_fresh_window() -> String {
    describe_fresh_window_ms(FRESH_MS)
}

/// The two lines `--lock-clean` prints when it refuses, or `None` when the lock
/// is cleanable. Pure so the WORDING can be pinned against v4's launcher
/// (`packages/quilltap/bin/quilltap.js:636-656` at v4 `23abc1ba1`), which is
/// this verb's oracle.
///
/// The two arms say different things because they tested different things:
///
///  - `alive && is_node` — a real Quilltap process is running. "Held by a live
///    process" is TRUE and stopping it IS the remedy, so this arm keeps v4's
///    *"Stop the running instance first…"*.
///  - `heartbeat_fresh` alone — reached ONLY once [`assess_lock`] has computed
///    `alive = false` (the arm above claims every confirmed-live case), so it
///    must not assert that anything is running. Freshness alone is bug 126's
///    deliberate fallback for every environment — a PID check is not reliable
///    everywhere, and cleaning a LIVE instance's lock is the worse failure —
///    so the refusal is right, and what it says is simply what was tested,
///    plus the remedy the old text omitted entirely: waiting.
///
/// ⚠ **This arm is a CONVERGENCE, and its history is the point.** Until v4
/// `23abc1ba1` it announced *"its holder is alive"* over a PID the line above
/// had just proved dead — and the BOOT path contradicted it outright, logging
/// `PID <n> is no longer running` as it reclaimed the same lock. v5 carried
/// that byte for byte. The 2026-09-15 dogfood walk (finding #119) "fixed" it
/// here on a false premise, Tier R caught the divergence, and the revert was
/// pinned *asserting the false claim* so that v4's own fix would redden it by
/// design. v4 landed the fix (bug 144, `23abc1ba1`); at the target pin exactly
/// **four** `lock clean …` Tier R cases moved — four, not the five the walk's
/// record claimed — and these are v4's post-fix bytes.
fn lock_clean_refusal_lines(
    alive: bool,
    is_node: bool,
    pid: f64,
    heartbeat_age_ms: f64,
    heartbeat_fresh: bool,
) -> Option<[String; 2]> {
    if alive && is_node {
        return Some([
            format!(
                "Lock is held by a live Quilltap process (PID {}). Cannot clean.",
                crate::nodefmt::js_num_string(pid)
            ),
            "Stop the running instance first, or use --lock-override to force.".to_string(),
        ]);
    }
    if heartbeat_fresh {
        return Some([
            format!(
                "Lock heartbeat is still fresh ({}s ago). Cannot clean.",
                quilltap_core::jsnum::math_round(heartbeat_age_ms / 1000.0) as i64
            ),
            format!(
                "A lock counts as held until its heartbeat is {} stale, even if its process has \
                 gone. Wait it out, or use --lock-override to force.",
                describe_fresh_window()
            ),
        ]);
    }
    None
}

fn assess_lock(lock: &Map<String, Value>, hostname: &str) -> LockAssessment {
    let pid = lock_num(lock, "pid");
    let alive = pid.is_finite() && is_pid_alive(pid as u32);
    let heartbeat_age_ms = heartbeat_age_ms(lock);
    LockAssessment {
        same_host: lock_str(lock, "hostname") == hostname,
        alive,
        is_node: alive && verify_pid_is_quilltap(pid as u32),
        heartbeat_age_ms,
        heartbeat_fresh: heartbeat_age_ms < FRESH_MS,
    }
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
        let LockAssessment {
            same_host,
            alive,
            is_node,
            heartbeat_age_ms: age_ms,
            heartbeat_fresh,
        } = assess_lock(&lock, &hostname);

        // v4's branch ORDER: a confirmed process, then ANY fresh heartbeat,
        // then the reused-PID suspicion, then the two stale arms. The
        // container-only window and the bare `STALE (different host)` arm are
        // gone (bug 126); note the reset code now precedes the parenthesis on
        // the heartbeat arm, where it used to follow the whole phrase.
        let status = if alive && is_node {
            "\x1b[32mACTIVE\x1b[0m (process confirmed running)".to_string()
        } else if heartbeat_fresh {
            let age_str = format!(
                "{}s",
                quilltap_core::jsnum::math_round(age_ms / 1000.0) as i64
            );
            format!(
                "\x1b[32mACTIVE\x1b[0m ({}, heartbeat {} ago)",
                non_empty_or(lock_str(&lock, "environment"), "unknown"),
                age_str
            )
        } else if alive && !is_node {
            "\x1b[33mSUSPECT\x1b[0m (PID alive but does not look like Quilltap — possible PID reuse)"
                .to_string()
        } else if !same_host {
            format!(
                "\x1b[33mSTALE ({} on {}, no recent heartbeat)\x1b[0m — will be auto-claimed on next startup",
                non_empty_or(lock_str(&lock, "environment"), "unknown"),
                lock_str(&lock, "hostname")
            )
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
                " (this host)".to_string()
            } else {
                format!(" (recorded name differs from ours: {hostname})")
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
            let age_s = quilltap_core::jsnum::math_round(heartbeat_age_ms(&lock) / 1000.0);
            let mut display = if age_s < 120.0 {
                format!("{}s ago", crate::nodefmt::js_num_string(age_s))
            } else if age_s < 7200.0 {
                format!(
                    "{}m ago",
                    crate::nodefmt::js_num_string(quilltap_core::jsnum::math_round(age_s / 60.0))
                )
            } else {
                format!(
                    "{}h ago",
                    crate::nodefmt::js_num_string(quilltap_core::jsnum::math_round(age_s / 3600.0))
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
        let LockAssessment {
            same_host,
            alive,
            is_node,
            heartbeat_age_ms: age_ms,
            heartbeat_fresh,
        } = assess_lock(&lock, &hostname);

        // v4's branch order, matching `--lock-status` above. The NEW second arm
        // refuses to delete a lock that is still being refreshed whatever its
        // recorded name says; the old `Lock is held by a live <env> instance`
        // and `Lock was held by a different host` arms are gone (bug 126).
        if let Some([first, second]) =
            lock_clean_refusal_lines(alive, is_node, pid, age_ms, heartbeat_fresh)
        {
            out::log(&first);
            out::log(&second);
            out::exit(1);
        } else if alive && !is_node {
            out::log(&format!(
                "Lock references PID {} which is alive but does NOT look like a Quilltap process.",
                crate::nodefmt::js_num_string(pid)
            ));
            out::log("This is likely a stale lock with a reused PID. Removing.");
        } else if !same_host {
            out::log(&format!(
                "Lock was held by {} on {} with no recent heartbeat. Removing stale lock.",
                non_empty_or(lock_str(&lock, "environment"), "unknown"),
                lock_str(&lock, "hostname")
            ));
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

// ---------- verb: optimize (P4.D259 Tier 2; v4 `db-commands.js:1330-1480`) ----------

/// v4 `OPTIMIZE_TARGETS` — `(key, filename, friendly name)`; the key is also
/// the label. The friendly name is v4's `openMainDb` & co.
const OPTIMIZE_TARGETS: [(&str, &str, &str); 3] = [
    ("main", "quilltap.db", "main database"),
    ("llm-logs", "quilltap-llm-logs.db", "LLM logs database"),
    (
        "mount-points",
        "quilltap-mount-index.db",
        "mount index database",
    ),
];

/// v4 `formatBytes`.
fn format_bytes(n: f64) -> String {
    use quilltap_core::jsnum::to_fixed;
    if n < 1024.0 {
        return format!("{} B", crate::nodefmt::js_num_string(n));
    }
    if n < 1024.0 * 1024.0 {
        return format!("{} KB", to_fixed(n / 1024.0, 1));
    }
    if n < 1024.0 * 1024.0 * 1024.0 {
        return format!("{} MB", to_fixed(n / (1024.0 * 1024.0), 1));
    }
    format!("{} GB", to_fixed(n / (1024.0 * 1024.0 * 1024.0), 2))
}

/// v4 `formatDuration`.
fn format_duration(ms: u64) -> String {
    use quilltap_core::jsnum::to_fixed;
    if ms < 1000 {
        return format!("{ms} ms");
    }
    if ms < 60_000 {
        return format!("{} s", to_fixed(ms as f64 / 1000.0, 2));
    }
    format!("{} min", to_fixed(ms as f64 / 60_000.0, 2))
}

/// v4 `fileSize` — `fs.statSync(p).size`, 0 on error.
fn optimize_file_size(path: &str) -> f64 {
    std::fs::metadata(path)
        .map(|m| m.len() as f64)
        .unwrap_or(0.0)
}

/// v4 launcher `getLockStatus` (`lock-helpers.js:54-107`) — the read-only
/// decision a maintenance verb refuses on. `Ok(())` for `absent` / `stale`;
/// the refusal sentence otherwise.
fn optimize_lock_refusal(data_dir: &str) -> Result<(), String> {
    let lock_path = node_join(data_dir, "quilltap.lock");
    if !std::path::Path::new(&lock_path).exists() {
        return Ok(());
    }
    // `JSON.parse(readFileSync)` — only a read or parse failure is `corrupt`;
    // a non-object parses fine and simply matches no host (stale).
    let parsed = std::fs::read_to_string(&lock_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let Some(parsed) = parsed else {
        return Err(format!(
            "Lock file at {lock_path} is corrupt. Inspect it manually or clean with `quilltap db --lock-clean`, then retry."
        ));
    };
    let lock = match parsed {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    let hostname = quilltap_host::lock::hostname();
    let pid = lock_num(&lock, "pid");
    let active = |reason: String| {
        Err(format!(
            "Database is currently in use — {reason}.\nStop the running Quilltap instance before optimizing, then try again.\n(See `quilltap db --lock-status` for details.)"
        ))
    };
    if lock_str(&lock, "hostname") == hostname {
        let alive = pid.is_finite() && is_pid_alive(pid as u32);
        if !alive {
            return Ok(()); // stale: PID no longer running
        }
        if !verify_pid_is_quilltap(pid as u32) {
            let reason = format!(
                "PID {} is alive but does not look like a Quilltap process",
                crate::nodefmt::js_num_string(pid)
            );
            return Err(format!(
                "Lock file {reason}.\nThis may be a stale lock from a reused PID. Inspect it with\n`quilltap db --lock-status` and clean it up with `quilltap db --lock-clean` if safe."
            ));
        }
        return active(format!(
            "held by PID {} on this host",
            crate::nodefmt::js_num_string(pid)
        ));
    }
    // Different hostname — could be a VM/container sharing the data dir.
    let environment = lock_str(&lock, "environment");
    let age_ms = heartbeat_age_ms(&lock);
    if environment == "docker" && age_ms < FRESH_MS {
        let age = format!(
            "{}s",
            crate::nodefmt::js_num_string(quilltap_core::jsnum::math_round(age_ms / 1000.0))
        );
        return active(format!(
            "held by {environment} instance on {} (heartbeat {age} ago)",
            lock_str(&lock, "hostname")
        ));
    }
    Ok(())
}

/// v4 `cmdOptimize` — `VACUUM`, `ANALYZE`, `PRAGMA optimize` per target with
/// the instance lock checked first (the server holds its connections open, so
/// a running instance refuses). The steps are core's
/// [`run_optimize_steps`](quilltap_core::services::daily_db_optimize::run_optimize_steps),
/// the server's daily pass's own — v4's "keep the two step lists in step".
fn cmd_optimize(
    args: &[String],
    ctx: &crate::db_characters::Ctx,
) -> Result<(), crate::db_characters::CmdError> {
    use crate::db_characters::{as_bool, parse_sub_args, print_json, CmdError};
    let (flags, positional) = parse_sub_args(args);
    let json = as_bool(flags.get("json"));

    let keys: Vec<&str> = OPTIMIZE_TARGETS.iter().map(|t| t.0).collect();
    let targets: Vec<&(&str, &str, &str)> =
        if positional.is_empty() || (positional.len() == 1 && positional[0] == "all") {
            OPTIMIZE_TARGETS.iter().collect()
        } else {
            let mut out = Vec::new();
            for p in &positional {
                match OPTIMIZE_TARGETS.iter().find(|t| t.0 == p) {
                    Some(t) => out.push(t),
                    None => {
                        return Err(CmdError {
                            message: format!(
                                "Unknown optimize target '{p}'. Allowed: {} | all",
                                keys.join(" | ")
                            ),
                            exit_code: 1,
                        })
                    }
                }
            }
            out
        };

    // Refuse to proceed if any instance is actively holding the lock.
    optimize_lock_refusal(&ctx.data_dir).map_err(|message| CmdError {
        message,
        exit_code: 1,
    })?;

    let mut results: Vec<Value> = Vec::new();
    for (key, filename, friendly) in targets {
        let db_path = node_join(&ctx.data_dir, filename);
        if !std::path::Path::new(&db_path).exists() {
            if !json {
                out::log(&format!("Skipping {key}: {db_path} not found."));
            }
            let mut r = Map::new();
            r.insert("target".into(), Value::from(*key));
            r.insert("skipped".into(), Value::Bool(true));
            r.insert("reason".into(), Value::from("not found"));
            results.push(Value::Object(r));
            continue;
        }
        results.push(optimize_one_db(key, &db_path, friendly, ctx));
    }

    if json {
        let mut o = Map::new();
        o.insert("results".into(), Value::Array(results));
        print_json(&Value::Object(o));
        return Ok(());
    }

    // Final summary
    let total_saved: f64 = results
        .iter()
        .filter(|r| r["skipped"] == Value::Bool(false))
        .filter_map(|r| Some(r.get("sizeBefore")?.as_f64()? - r.get("sizeAfter")?.as_f64()?))
        .sum();
    if total_saved > 0.0 {
        out::log("");
        out::log(&format!("Total reclaimed: {}", format_bytes(total_saved)));
    }
    Ok(())
}

/// v4 `optimizeOneDb` — prints its block whether or not `--json` was given
/// (v4 does), and answers the result object in v4's key order.
fn optimize_one_db(
    label: &str,
    db_path: &str,
    friendly: &str,
    ctx: &crate::db_characters::Ctx,
) -> Value {
    use quilltap_core::services::daily_db_optimize::run_optimize_steps;
    out::log("");
    out::log(&format!("── {label}  ({db_path}) ──"));
    let size_before = optimize_file_size(db_path);
    out::log(&format!("  size before: {}", format_bytes(size_before)));

    let mut r = Map::new();
    r.insert("target".into(), Value::from(label));
    let conn = match open_encrypted(
        db_path,
        ctx.pepper.as_deref(),
        OpenOptions {
            readonly: false,
            friendly_name: friendly,
        },
    ) {
        Ok(c) => c,
        Err(e) => {
            out::log(&format!("  open failed: {}", e.message));
            r.insert("skipped".into(), Value::Bool(true));
            r.insert("reason".into(), Value::from(e.message));
            return Value::Object(r);
        }
    };

    let outcome = run_optimize_steps(&conn);
    let mut steps = Vec::new();
    for step in &outcome.steps {
        let mut s = Map::new();
        s.insert("name".into(), Value::from(step.name));
        s.insert("ok".into(), Value::Bool(step.ok));
        s.insert("ms".into(), Value::from(step.ms));
        match &step.error {
            None => out::log(&format!("  {}: {}", step.name, format_duration(step.ms))),
            Some(error) => {
                out::log(&format!(
                    "  {}: FAILED after {} — {error}",
                    step.name,
                    format_duration(step.ms)
                ));
                s.insert("error".into(), Value::from(error.as_str()));
            }
        }
        steps.push(Value::Object(s));
    }
    drop(conn);

    let size_after = optimize_file_size(db_path);
    r.insert("skipped".into(), Value::Bool(false));
    r.insert("sizeBefore".into(), Value::from(size_before as u64));
    r.insert("sizeAfter".into(), Value::from(size_after as u64));
    r.insert("steps".into(), Value::Array(steps));
    if outcome.ok {
        let delta = size_before - size_after;
        let delta_str = if delta == 0.0 {
            "no change".to_string()
        } else if delta > 0.0 {
            format!("reclaimed {}", format_bytes(delta))
        } else {
            format!("grew by {}", format_bytes(-delta))
        };
        out::log(&format!(
            "  size after:  {}  ({delta_str})",
            format_bytes(size_after)
        ));
    }
    Value::Object(r)
}

#[cfg(test)]
mod lock_clean_wording_tests {
    use super::*;

    /// v4's launcher, `packages/quilltap/bin/quilltap.js:636-656` at v4
    /// `23abc1ba1`, transcribed. These are the bytes Tier R compares against;
    /// every arm is v4's.
    const V4_LIVE_PROCESS: &str =
        "Lock is held by a live Quilltap process (PID 4242). Cannot clean.";
    const V4_FRESH_HEARTBEAT: &str = "Lock heartbeat is still fresh (82s ago). Cannot clean.";
    /// The live-process arm's second line — v4's ONLY surviving use of it. The
    /// fresh-heartbeat arm stopped saying this at `23abc1ba1`, which is half of
    /// bug 144's fix (the remedy was wrong, not only the diagnosis).
    const V4_LIVE_SECOND_LINE: &str =
        "Stop the running instance first, or use --lock-override to force.";
    const V4_FRESH_SECOND_LINE: &str =
        "A lock counts as held until its heartbeat is 5 minutes stale, even if its process has \
         gone. Wait it out, or use --lock-override to force.";

    #[test]
    fn a_live_quilltap_process_is_named_as_such() {
        let [first, second] =
            lock_clean_refusal_lines(true, true, 4242.0, 1_000.0, true).expect("refuses");
        assert_eq!(first, V4_LIVE_PROCESS);
        assert_eq!(second, V4_LIVE_SECOND_LINE);
    }

    /// The 2026-09-15 walk's exact scenario: the server was SIGKILLed 82 s ago,
    /// so the PID is DEAD but the heartbeat is still young.
    ///
    /// This is the CONVERGED text (v4 `23abc1ba1`, bug 144). Its predecessor,
    /// `a_fresh_heartbeat_keeps_v4s_false_liveness_claim`, asserted
    /// `first.contains("its holder is alive")` **on purpose** so that v4's fix
    /// would redden it by design; it did, together with four `lock clean …`
    /// Tier R cases, and this is the retirement. The two `!contains` guards
    /// below are what stops the old bytes coming back: BOTH lines moved — the
    /// first stopped claiming liveness, and the second stopped prescribing a
    /// remedy for a process that is gone.
    #[test]
    fn a_fresh_heartbeat_says_what_it_tested_and_offers_waiting() {
        let [first, second] =
            lock_clean_refusal_lines(false, false, 24346.0, 82_000.0, true).expect("refuses");
        assert_eq!(first, V4_FRESH_HEARTBEAT);
        assert_eq!(second, V4_FRESH_SECOND_LINE);
        assert!(!first.contains("its holder is alive"));
        assert!(!second.contains("Stop the running instance first"));
        // The window in the sentence IS the window the check uses.
        assert!(second.contains(&describe_fresh_window()));
    }

    /// v4's `describeFreshWindow()`, arm for arm, over an injected window —
    /// including values `FRESH_MS` never takes, because the arithmetic is JS's
    /// and not English. `0` is the tell: `0 % 60 === 0`, `0 / 60 === 0`, and
    /// `0 === 1` is false, so v4 answers `0 minutes`.
    #[test]
    fn the_fresh_window_is_worded_as_v4_words_it() {
        assert_eq!(describe_fresh_window_ms(300.0 * 1000.0), "5 minutes");
        assert_eq!(describe_fresh_window_ms(60.0 * 1000.0), "1 minute");
        assert_eq!(describe_fresh_window_ms(90.0 * 1000.0), "90 seconds");
        assert_eq!(describe_fresh_window_ms(0.0), "0 minutes");
        // The production call reads `FRESH_MS` — which
        // `the_freshness_window_is_v4s_five_minutes` pins at v4's value.
        assert_eq!(describe_fresh_window(), "5 minutes");
    }

    /// v4's branch ORDER: a confirmed live process is named before the
    /// heartbeat arm, even when both hold.
    #[test]
    fn a_live_process_outranks_a_fresh_heartbeat() {
        let [first, _] = lock_clean_refusal_lines(true, true, 7.0, 1_000.0, true).expect("refuses");
        assert_eq!(
            first,
            "Lock is held by a live Quilltap process (PID 7). Cannot clean."
        );
        assert!(!first.contains("still being refreshed"));
    }

    /// A stale heartbeat over a dead PID is exactly what `--lock-clean` exists
    /// for — no refusal, so the caller proceeds to remove it.
    #[test]
    fn a_stale_lock_is_not_refused() {
        assert!(lock_clean_refusal_lines(false, false, 9.0, 6.0 * 60.0 * 1000.0, false).is_none());
    }

    /// An alive PID that is NOT Quilltap falls through to the reused-PID arm
    /// below, not to either refusal.
    #[test]
    fn a_reused_pid_is_not_refused_when_the_heartbeat_is_stale() {
        assert!(lock_clean_refusal_lines(true, false, 11.0, 6.0 * 60.0 * 1000.0, false).is_none());
    }

    /// The freshness window the refusal rests on is v4's five minutes.
    #[test]
    fn the_freshness_window_is_v4s_five_minutes() {
        assert_eq!(FRESH_MS, 5.0 * 60.0 * 1000.0);
        // 4m59s still refuses; 5m01s does not.
        assert!(
            lock_clean_refusal_lines(false, false, 1.0, 299_000.0, 299_000.0 < FRESH_MS).is_some()
        );
        assert!(
            lock_clean_refusal_lines(false, false, 1.0, 301_000.0, 301_000.0 < FRESH_MS).is_none()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::types::ValueRef;

    // ---- Table-mode seam (P4.D240 item 3, option (b)): NAMED divergences from
    // Node's `console.table`, now operator-reachable because decoded message
    // text meets embedding Buffers in one `SELECT *`. Each pins v5's value.

    #[test]
    fn table_mode_divergences_buffer_cell_is_compact_json() {
        // Node prints `<Buffer eb 01 02 03>`.
        let v = crate::nodefmt::cell_to_js_value(ValueRef::Blob(&[0xeb, 1, 2, 3]));
        assert_eq!(
            crate::vtable::cell_text(&v),
            r#"{"type":"Buffer","data":[235,1,2,3]}"#
        );
    }

    #[test]
    fn table_mode_divergences_long_strings_are_not_truncated() {
        // Node truncates a string over 10,000 UTF-16 units with
        // `'…'... N more characters`.
        let s = "y".repeat(10_050);
        let cell = crate::vtable::cell_text(&Value::from(s.clone()));
        assert_eq!(cell, crate::nodefmt::inspect_quote(&s));
        assert_eq!(cell.chars().count(), 10_052);
    }

    #[test]
    fn table_mode_divergences_width_counts_chars_not_columns() {
        // Node counts an East-Asian wide char as two columns; v5 counts one.
        let mut row = Map::new();
        row.insert("k".into(), Value::from("\u{4e16}\u{754c}"));
        let table = crate::vtable::console_table(&[row]);
        // `'世界'` is 4 chars: the data row's cell is `| '世界' |`.
        assert!(table.contains("│ '\u{4e16}\u{754c}' │"), "{table}");
        let rule = table.lines().next().unwrap();
        assert_eq!(rule.chars().filter(|c| *c == '─').count(), 4 + 2 + 7 + 2);
    }

    #[test]
    fn table_mode_divergences_c1_controls_are_not_escaped() {
        // Node escapes C1 controls (`\x85`); v5 prints the char through.
        assert_eq!(crate::nodefmt::inspect_quote("a\u{85}b"), "'a\u{85}b'");
    }
}
