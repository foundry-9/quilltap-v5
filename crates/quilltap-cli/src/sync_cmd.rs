//! `quilltap sync <store> <path>` — keep a database-backed document store and a
//! directory on disk in step with each other.
//!
//! Port of v4 `packages/quilltap/lib/sync-command.js` (`23da0b322`).
//!
//! This file is a thin client. Every decision the sync makes — which side wins,
//! what counts as a deletion, when to refuse — is made by the engine in the
//! server (`quilltap_core::services::mount_index::sync`), and every flag is
//! validated by the route's own schema, which is the single source of truth. The
//! CLI resolves the store name to a UUID (the one thing it opens the database
//! for, read-only), posts the request, and prints what came back.
//!
//! The engine lives in the server rather than here because the sync writes
//! through the store's own chokepoints — `link_document_content`, the folder-row
//! helper, the hard-link fan-out, the post-write re-chunk — and reaching them
//! needs the writer, not a read-only opener. `docs write` on a database store
//! already requires the server for exactly this reason; `deconvert` already
//! takes a server-local target path. This is that shape, not a new one.
//!
//! `<path>` is therefore resolved on the SERVER. Running under Docker, it must
//! sit inside a bind mount — `quilltap docs docker-mounts` plans those.

use serde_json::{Map, Value};

use quilltap_core::doc_edit::qtap_uri::{is_qtap_uri, parse_qtap_uri};
use quilltap_core::doc_edit::DocEditScope;
use quilltap_host::instances::InstanceRegistry;

use crate::dbopen::{open_encrypted, sqlite_msg, OpenOptions};
use crate::nodefmt::{js_parse_int, json_stringify_pretty, node_join, node_resolve};
use crate::out;
use crate::resolve::{load_db_key, print_default_instance_hint, resolve_data_dir_and_passphrase};
use crate::sync_report::{exit_code_for, format_action_lines, format_summary, SyncReportView};

const SYNC_HELP: &str = include_str!("help/sync_help.txt");

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const YELLOW: &str = "\x1b[33m";

#[derive(Debug)]
struct Flags {
    data_dir: String,
    instance: String,
    passphrase: String,
    port: i64,
    json: bool,
    dry_run: bool,
    direction: String,
    prefer: String,
    no_delete: bool,
    no_manifest: bool,
    help: bool,
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            data_dir: String::new(),
            instance: String::new(),
            passphrase: String::new(),
            port: 3000,
            json: false,
            dry_run: false,
            // v4's defaults for the two enums are the STRINGS, not the schema's
            // defaults: the CLI sends whatever it holds and the route judges it,
            // so a `--direction` with no value sends `''` and is refused by the
            // server with the server's own wording.
            direction: "both".to_string(),
            prefer: "newer".to_string(),
            no_delete: false,
            no_manifest: false,
            help: false,
        }
    }
}

fn parse_flags(args: &[String]) -> (Flags, Vec<String>) {
    let mut flags = Flags::default();
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-d" | "--data-dir" => {
                i += 1;
                flags.data_dir = args.get(i).cloned().unwrap_or_default();
            }
            "-i" | "--instance" => {
                i += 1;
                flags.instance = args.get(i).cloned().unwrap_or_default();
            }
            "--passphrase" => {
                i += 1;
                flags.passphrase = args.get(i).cloned().unwrap_or_default();
            }
            "-p" | "--port" => {
                // v4 `parseInt(args[++i], 10) || 3000` — a NaN, a zero and an
                // absent value all fall back to 3000.
                i += 1;
                flags.port = match js_parse_int(args.get(i).map(String::as_str)) {
                    Some(0) | None => 3000,
                    Some(p) => p,
                };
            }
            "--json" => flags.json = true,
            "--dry-run" => flags.dry_run = true,
            "--direction" => {
                i += 1;
                flags.direction = args.get(i).cloned().unwrap_or_default();
            }
            "--prefer" => {
                i += 1;
                flags.prefer = args.get(i).cloned().unwrap_or_default();
            }
            "--no-delete" => flags.no_delete = true,
            "--no-manifest" => flags.no_manifest = true,
            "-h" | "--help" => flags.help = true,
            other => {
                if other.starts_with('-') {
                    out::elog(&format!("Unknown option: {other}"));
                    out::exit(1);
                }
                positional.push(other.to_string());
            }
        }
        i += 1;
    }
    (flags, positional)
}

/// v4 `expandPath` — `~/x` becomes an absolute path. The server sees only what
/// we send it.
fn expand_path(input: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let expanded = if input == "~" {
        home
    } else if let Some(rest) = input.strip_prefix("~/") {
        node_join(&home, rest)
    } else {
        input.to_string()
    };
    node_resolve(&expanded)
}

/// v4 `resolveStoreSpec` — a store name or UUID, or a `qtap://store/` URI with
/// an EMPTY path. This is the only thing the verb opens the database for, and it
/// opens it read-only.
fn resolve_store_spec(spec: &str) -> Result<String, String> {
    if !is_qtap_uri(spec) {
        return Ok(spec.to_string());
    }
    let parsed = parse_qtap_uri(spec).map_err(|e| e.message)?;
    if parsed.scope != DocEditScope::DocumentStore {
        return Err(
            "sync addresses document stores only; qtap://project/… and qtap://general/… are not CLI-addressable."
                .to_string(),
        );
    }
    if !parsed.path.is_empty() {
        return Err(format!(
            "sync takes a whole store, not a path inside one: {spec}"
        ));
    }
    let mount_point = parsed.mount_point.clone().unwrap_or_default();
    // v4 `!parsed.mountPoint || … === 'self'` — JS truthiness, so an empty
    // store name takes the same refusal as `self`.
    if mount_point.is_empty() || mount_point.to_lowercase() == "self" {
        return Err(
            "\"self\" requires a character context and is not resolvable from the CLI.".to_string(),
        );
    }
    Ok(mount_point)
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

fn row_to_map(row: &rusqlite::Row<'_>, cols: &[String]) -> Map<String, Value> {
    let mut out_map = Map::new();
    for (i, name) in cols.iter().enumerate() {
        let value = row.get_ref(i).map(crate::nodefmt::cell_to_js_value);
        out_map.insert(name.clone(), value.unwrap_or(Value::Null));
    }
    out_map
}

fn query_maps(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Map<String, Value>>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| sqlite_msg(&e))?;
    let cols: Vec<String> = stmt
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut rows = stmt.query(params).map_err(|e| sqlite_msg(&e))?;
    let mut out_rows = Vec::new();
    while let Some(row) = rows.next().map_err(|e| sqlite_msg(&e))? {
        out_rows.push(row_to_map(row, &cols));
    }
    Ok(out_rows)
}

fn s(row: &Map<String, Value>, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(v)) => v.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => crate::nodefmt::json_stringify_pretty(other),
    }
}

/// v4 `requireMount(db, spec)` — UUID first, then a case-insensitive name.
/// Errors go to stderr and exit 1 (they are not thrown).
fn require_mount(conn: &rusqlite::Connection, spec: &str) -> Map<String, Value> {
    if is_uuid(spec) {
        let rows = query_maps(
            conn,
            "SELECT * FROM doc_mount_points WHERE id = ?",
            &[&spec],
        )
        .unwrap_or_default();
        let Some(row) = rows.into_iter().next() else {
            out::elog(&format!("No document store found with id {spec}"));
            out::exit(1);
        };
        return row;
    }
    let rows = query_maps(
        conn,
        "SELECT * FROM doc_mount_points WHERE LOWER(name) = LOWER(?) ORDER BY name COLLATE NOCASE",
        &[&spec],
    )
    .unwrap_or_default();
    if rows.is_empty() {
        out::elog(&format!("No document store found with name \"{spec}\""));
        out::exit(1);
    }
    if rows.len() > 1 {
        out::elog(&format!(
            "Ambiguous store name \"{spec}\" matches multiple stores:"
        ));
        for r in &rows {
            out::elog(&format!(
                "  {}  {}  ({})",
                s(r, "id"),
                s(r, "name"),
                s(r, "mountType")
            ));
        }
        out::elog("Pass the UUID instead.");
        out::exit(1);
    }
    rows.into_iter().next().unwrap()
}

/// v4 `encodeURIComponent`.
fn encode_uri_component(s: &str) -> String {
    let mut out_s = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        let c = *byte as char;
        if c.is_ascii_alphanumeric() || "-_.!~*'()".contains(c) {
            out_s.push(c);
        } else {
            out_s.push_str(&format!("%{byte:02X}"));
        }
    }
    out_s
}

pub fn run(args: &[String]) -> i32 {
    let (flags, positional) = parse_flags(args);
    // v4 prints the help for `--help` AND for no positionals, and the exit code
    // is the only difference.
    if flags.help || positional.is_empty() {
        out::write_stdout(SYNC_HELP.as_bytes());
        out::exit(if flags.help { 0 } else { 1 });
    }

    let store_spec = positional[0].clone();
    let Some(target_spec) = positional.get(1) else {
        out::elog("Usage: quilltap sync <store> <path> [options]");
        out::elog("Run 'quilltap sync --help' for the full list of options.");
        out::exit(1);
    };
    if positional.len() > 2 {
        out::elog(&format!(
            "sync takes one store and one path; got {} arguments.",
            positional.len()
        ));
        out::exit(1);
    }

    let store_name = match resolve_store_spec(&store_spec) {
        Ok(name) => name,
        Err(message) => {
            out::elog(&format!("Error: {message}"));
            out::exit(1);
        }
    };

    let target_path = expand_path(target_spec);

    // Resolve the store from the local database — read-only, and the only thing
    // the CLI opens it for.
    let registry = InstanceRegistry::at_default_location();
    let resolved = match resolve_data_dir_and_passphrase(
        &flags.data_dir,
        &flags.instance,
        &flags.passphrase,
        &registry,
    ) {
        Ok(r) => r,
        Err(message) => {
            out::elog(&format!("Error: {message}"));
            out::exit(1);
        }
    };
    print_default_instance_hint(&resolved, &registry);
    let pepper = match load_db_key(&resolved.data_dir, &resolved.passphrase) {
        Ok(p) => p,
        Err(message) => {
            out::elog(&format!("Error: {message}"));
            out::exit(1);
        }
    };
    let db_path = node_join(&resolved.data_dir, "quilltap-mount-index.db");
    let mount = {
        // v4 `openMountIndexDb(...)` throws to `syncCommand`'s top-level
        // catch, which prints `Error: ${err.message}` — so the opener's
        // composed two-liner arrives WITH the prefix (P4.D214, Tier R `sync
        // wrong key on the mount index`).
        let conn = match open_encrypted(
            &db_path,
            pepper.as_deref(),
            OpenOptions {
                readonly: true,
                friendly_name: "mount index database",
            },
        ) {
            Ok(c) => c,
            Err(e) => {
                out::elog(&format!("Error: {}", e.message));
                out::exit(1);
            }
        };
        let mount = require_mount(&conn, &store_name);
        // v4 closes the handle the moment the row is in hand; the sync itself
        // runs in the server, which holds the writer.
        drop(conn);
        mount
    };

    let mount_type = s(&mount, "mountType");
    if mount_type != "database" {
        let base_path = s(&mount, "basePath");
        // v4 `mount.basePath ? ` (${mount.basePath})` : ''` — JS truthiness.
        let suffix = if base_path.is_empty() {
            String::new()
        } else {
            format!(" ({base_path})")
        };
        out::elog(&format!(
            "\"{}\" is a {mount_type} store — it already IS a directory{suffix}.",
            s(&mount, "name")
        ));
        out::elog("sync mirrors database-backed stores only.");
        out::exit(1);
    }

    let path = format!(
        "/api/v1/mount-points/{}?action=sync",
        encode_uri_component(&s(&mount, "id"))
    );
    // The route's schema is the single source of truth for these; the CLI does
    // not re-validate, so a bad --direction is refused by the server with the
    // server's own wording.
    let mut body = Map::new();
    body.insert("targetPath".into(), Value::String(target_path));
    body.insert("dryRun".into(), Value::Bool(flags.dry_run));
    body.insert("direction".into(), Value::String(flags.direction.clone()));
    body.insert("prefer".into(), Value::String(flags.prefer.clone()));
    body.insert("propagateDeletes".into(), Value::Bool(!flags.no_delete));
    body.insert("useManifest".into(), Value::Bool(!flags.no_manifest));
    let body_text = serde_json::to_string(&Value::Object(body)).expect("serialize the sync body");

    let (status, response) = match crate::http::post(flags.port, &path, Some(&body_text)) {
        Ok(r) => r,
        Err(_) => {
            // v4 distinguishes a connection refusal from any other fetch
            // failure; the Rust client's only failure mode at this seam IS the
            // connect, so both arms land here with v4's connection wording.
            out::elog(&format!(
                "Cannot sync database-backed store \"{}\" without the Quilltap server.",
                s(&mount, "name")
            ));
            out::elog("Start the server (`quilltap`) or pass --port to match a non-default port.");
            out::exit(1);
        }
    };

    // v4 `await res.json().catch(() => null)`.
    let payload: Option<Value> = serde_json::from_str(&response).ok();
    if !(200..300).contains(&status) {
        let message = payload
            .as_ref()
            .and_then(|p| p.get("error"))
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("HTTP {status}"));
        out::elog(&format!("Error: {message}"));
        out::exit(1);
    }

    // v4 `payload && payload.data !== undefined ? payload.data : payload`.
    let report_value = match payload {
        Some(Value::Object(ref o)) if o.contains_key("data") => o.get("data").cloned().unwrap(),
        Some(other) => other,
        None => Value::Null,
    };
    let report = SyncReportView::from_value(&report_value);

    if flags.json {
        out::write_stdout(format!("{}\n", json_stringify_pretty(&report_value)).as_bytes());
        return exit_code_for(&report.summary);
    }

    let colour = out::stdout_is_tty();
    for line in format_action_lines(&report.actions, colour) {
        out::log(&line);
    }

    // Advisory text goes to stderr so stdout stays greppable.
    for warning in &report.warnings {
        out::elog(&if colour {
            format!("{YELLOW}warning:{RESET} {warning}")
        } else {
            format!("warning: {warning}")
        });
    }
    let summary = format_summary(&report.summary, report.elapsed_ms, report.dry_run);
    out::elog(&if colour {
        format!("{DIM}{summary}{RESET}")
    } else {
        summary
    });
    if report.summary.conflicts > 0 {
        out::elog(&if colour {
            format!(
                "{DIM}Re-run with --prefer store or --prefer disk to resolve the conflicts.{RESET}"
            )
        } else {
            "Re-run with --prefer store or --prefer disk to resolve the conflicts.".to_string()
        });
    }

    exit_code_for(&report.summary)
}
