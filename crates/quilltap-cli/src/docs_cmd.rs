//! `quilltap docs` — the direct-mode read verbs (v4
//! `packages/quilltap/lib/docs-commands.js`): `list`, `show`, `ls`/`dir`,
//! `tree`, `read` (raw + `--rendered`). The remaining verbs (`files`,
//! `status`, `find`, `grep` — Tier B; the write/server-required set — P4.4)
//! are recognized and exit loud.
//!
//! ANSI fidelity: the launcher's coloring here is hand-rolled string
//! constants that do NOT auto-disable when piped — reproduced literally.

use serde_json::{Map, Value};

use quilltap_core::doc_edit::qtap_uri::{
    format_qtap_uri, is_qtap_uri, parse_qtap_uri, QtapUriParts,
};
use quilltap_core::doc_edit::DocEditScope;
use quilltap_host::instances::InstanceRegistry;

use crate::nodefmt::{cell_to_js_value, format_bytes, js_num_string, json_stringify_pretty};
use crate::out;
use crate::resolve::{load_db_key, print_default_instance_hint, resolve_data_dir_and_passphrase};
use crate::vtable::console_table;

const DOCS_HELP: &str = include_str!("help/docs_help.txt");

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";

const TEXT_FILE_TYPES: &[&str] = &["markdown", "txt", "json", "jsonl"];
const BINARY_FILE_TYPES: &[&str] = &["pdf", "docx", "blob"];

fn is_text_file_type(t: &str) -> bool {
    TEXT_FILE_TYPES.contains(&t)
}

/// v4 `textColumnMarker`.
fn text_column_marker(file_type: &str, extraction_status: &str) -> &'static str {
    if is_text_file_type(file_type) {
        return "=";
    }
    match extraction_status {
        "converted" => "T",
        "pending" => "~",
        "failed" => "!",
        _ => "-",
    }
}

/// v4 `embedColumnMarker`.
fn embed_column_marker(chunk_count: f64, embedded_chunk_count: f64) -> &'static str {
    // v4: `if (!chunkCount)` — falsy covers 0 and NaN.
    if chunk_count.is_nan() || chunk_count == 0.0 {
        return "-";
    }
    if embedded_chunk_count == chunk_count {
        return "Y";
    }
    "~"
}

#[derive(Default)]
struct Flags {
    data_dir: String,
    instance: String,
    passphrase: String,
    json: bool,
    uri: bool,
    rendered: bool,
    force: bool,
    links: bool,
    help: bool,
    names_only: bool,
    base64: bool,
    recursive: bool,
    sort: String,
    reverse: bool,
    depth: i64,
    max_nodes: i64,
}

fn parse_flags(args: &[String]) -> (Flags, Vec<String>) {
    let mut flags = Flags {
        sort: "name".to_string(),
        depth: 20,
        max_nodes: 1000,
        ..Flags::default()
    };
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
            "--port" => {
                let port = crate::nodefmt::js_parse_int(args.get(i + 1).map(String::as_str));
                i += 1;
                match port {
                    Some(p) if (1..=65535).contains(&p) => {}
                    _ => {
                        out::elog("Error: --port must be between 1 and 65535");
                        out::exit(1);
                    }
                }
            }
            "--json" => flags.json = true,
            "--uri" => flags.uri = true,
            "--rendered" => flags.rendered = true,
            "--folder" | "--mount" | "--type" | "--ext" | "--limit" | "--max" | "--context"
            | "--top" | "--sort" | "--threshold" => {
                // Value-taking flags; only --sort is consumed by a Tier-R verb.
                let value = args.get(i + 1).cloned().unwrap_or_default();
                if a == "--sort" {
                    flags.sort = if value.is_empty() {
                        "name".to_string()
                    } else {
                        value
                    };
                }
                i += 1;
            }
            "--force" => flags.force = true,
            "--links" => flags.links = true,
            "--ignore-case" | "-l" | "--wait" | "--long" | "--semantic" => {}
            "--base64" => flags.base64 = true,
            "-R" | "--recursive" => flags.recursive = true,
            "-r" | "--reverse" => flags.reverse = true,
            "--depth" => {
                i += 1;
                // v4: parseInt(args[++i], 10) || 20 (NaN and 0 both fall back).
                flags.depth = match crate::nodefmt::js_parse_int(args.get(i).map(String::as_str)) {
                    Some(0) | None => 20,
                    Some(x) => x,
                };
            }
            "--max-nodes" => {
                i += 1;
                // v4: parseInt(args[++i], 10) || 1000.
                flags.max_nodes =
                    match crate::nodefmt::js_parse_int(args.get(i).map(String::as_str)) {
                        Some(0) | None => 1000,
                        Some(x) => x,
                    };
            }
            "--names-only" => flags.names_only = true,
            // v4 `ed8934f1` (bug 56) added `--format args|json` for the
            // `docker-mounts` verb alone. Recognized here — and its value
            // consumed, exactly as v4's `args[++i]` does — so the flag never
            // shadows that verb's own loud refusal below with an
            // "Unknown option" exit. No other verb reads it.
            "--format" => i += 1,
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

pub fn run(args: &[String]) {
    if args.is_empty() {
        out::write_stdout(DOCS_HELP.as_bytes());
        out::exit(1);
    }
    let (flags, mut positional) = parse_flags(args);
    if flags.help {
        out::write_stdout(DOCS_HELP.as_bytes());
        out::exit(0);
    }
    if positional.is_empty() {
        out::write_stdout(DOCS_HELP.as_bytes());
        out::exit(1);
    }
    let verb = positional.remove(0);

    // Verbs whose positionals accept a single qtap:// URI in place of the
    // `<mount> <relativePath>` pair.
    const QTAP_POSITIONAL_VERBS: &[&str] = &[
        "read", "write", "delete", "mkdir", "ls", "dir", "tree", "files", "move", "copy", "link",
        "rmdir", "mvdir", "show", "scan",
    ];

    let result: Result<(), String> = (|| {
        if QTAP_POSITIONAL_VERBS.contains(&verb.as_str()) {
            positional = expand_qtap_positionals(&positional)?;
        }
        match verb.as_str() {
            "list" => handle_list(&flags),
            "show" => handle_show(&flags, positional.first().map(String::as_str)),
            "ls" | "dir" => handle_ls(
                &flags,
                positional.first().map(String::as_str),
                positional.get(1).map(String::as_str),
            ),
            "tree" => handle_tree(
                &flags,
                positional.first().map(String::as_str),
                positional.get(1).map(String::as_str),
            ),
            "read" => handle_read(
                &flags,
                positional.first().map(String::as_str),
                positional.get(1).map(String::as_str),
            ),
            // `docker-mounts` is v4 `ed8934f1`'s new verb (bug 56): it plans the
            // bind mounts an instance's filesystem stores need inside a
            // container. Its planner (`lib/docker-mounts.js`, the platform
            // arms + the nesting/collapse rules) is unported — recognized and
            // loud rather than silently unknown; the port is its own order.
            "files" | "status" | "find" | "grep" | "export" | "scan" | "write" | "delete"
            | "mkdir" | "move" | "copy" | "link" | "rmdir" | "mvdir" | "reindex" | "embed"
            | "docker-mounts" => {
                out::elog(&format!(
                    "Error: docs subcommand '{verb}' is recognized but not yet available in this build of the quilltap CLI."
                ));
                out::exit(1);
            }
            _ => {
                out::elog(&format!("Unknown docs subcommand: {verb}"));
                out::write_stdout(DOCS_HELP.as_bytes());
                out::exit(1);
            }
        }
    })();

    if let Err(msg) = result {
        out::elog(&format!("Error: {msg}"));
        out::exit(1);
    }
}

/// v4 `expandQtapArg`: a `qtap://…` arg becomes `[mountSpec, relPath]`.
fn expand_qtap_arg(arg: &str) -> Result<Vec<String>, String> {
    let p: QtapUriParts = parse_qtap_uri(arg).map_err(|e| e.message)?;
    if p.scope != DocEditScope::DocumentStore {
        return Err(
            "The CLI addresses document stores only; project/general scopes \
             (qtap://project/…, qtap://general/…) are not CLI-addressable. Pass a store name or UUID."
                .to_string(),
        );
    }
    let mount_point = p.mount_point.clone().unwrap_or_default();
    if !mount_point.is_empty() && mount_point.to_lowercase() == "self" {
        return Err(
            "\"self\" requires a character context and is not resolvable from the CLI; pass a store name or UUID."
                .to_string(),
        );
    }
    Ok(vec![mount_point, p.path])
}

fn expand_qtap_positionals(positional: &[String]) -> Result<Vec<String>, String> {
    let mut out_args = Vec::new();
    for arg in positional {
        if is_qtap_uri(arg) {
            out_args.extend(expand_qtap_arg(arg)?);
        } else {
            out_args.push(arg.clone());
        }
    }
    Ok(out_args)
}

// ----------------------------------------------------------------------------
// openDb + schema guard
// ----------------------------------------------------------------------------

struct DocsDb {
    conn: rusqlite::Connection,
    #[allow(dead_code)]
    data_dir: String,
}

fn open_docs_db(flags: &Flags) -> Result<DocsDb, String> {
    let registry = InstanceRegistry::at_default_location();
    let resolved = resolve_data_dir_and_passphrase(
        &flags.data_dir,
        &flags.instance,
        &flags.passphrase,
        &registry,
    )?;
    print_default_instance_hint(&resolved, &registry);
    let data_dir = resolved.data_dir.clone();
    let pepper = load_db_key(&data_dir, &resolved.passphrase)?;
    let db_path = crate::nodefmt::node_join(&data_dir, "quilltap-mount-index.db");
    if !std::path::Path::new(&db_path).exists() {
        return Err(format!("mount index database not found: {db_path}"));
    }
    let conn = open_readonly(&db_path, pepper.as_deref()).map_err(|e| {
        format!(
            "Cannot open mount index database: {e}\nThe database may be encrypted with a different key, or the .dbkey file may be missing."
        )
    })?;
    assert_docs_schema(&conn, &data_dir, &registry);
    Ok(DocsDb { conn, data_dir })
}

fn open_readonly(db_path: &str, pepper: Option<&str>) -> Result<rusqlite::Connection, String> {
    use rusqlite::OpenFlags;
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| sqlite_msg(&e))?;
    if let Some(pepper) = pepper {
        let key_hex =
            quilltap_core::dbkey::pepper_b64_to_key_hex(pepper).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "key", format!("x'{key_hex}'"))
            .map_err(|e| sqlite_msg(&e))?;
    }
    conn.query_row("SELECT 1", [], |_| Ok(()))
        .map_err(|e| sqlite_msg(&e))?;
    Ok(conn)
}

fn sqlite_msg(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(m)) => m.clone(),
        other => other.to_string(),
    }
}

/// v4 `assertDocsSchema` — refuse a pre-link-table mount index with the exact
/// operator guidance (stderr, exit 1).
fn assert_docs_schema(conn: &rusqlite::Connection, data_dir: &str, registry: &InstanceRegistry) {
    let ok: Option<i64> = conn
        .query_row(
            "SELECT 1 AS ok FROM sqlite_master WHERE type='table' AND name='doc_mount_file_links'",
            [],
            |row| row.get(0),
        )
        .ok();
    if ok == Some(1) {
        return;
    }
    let registered = registry.list_instances().unwrap_or_default();
    let mut lines = vec![
        "Error: this instance has not been migrated to the post-link-table mount-index schema."
            .to_string(),
        format!(
            "  Database: {}",
            crate::nodefmt::node_join(data_dir, "quilltap-mount-index.db")
        ),
        "  Missing table: doc_mount_file_links".to_string(),
        String::new(),
        "Start Quilltap against this data directory once (npm run dev / npx quilltap)".to_string(),
        "to run the pending migrations, or target a different instance:".to_string(),
    ];
    if !registered.is_empty() {
        for inst in &registered {
            lines.push(format!("  --instance {}    ({})", inst.name, inst.path));
        }
    } else {
        lines.push("  (no instances registered — see `quilltap instances --help`)".to_string());
    }
    out::write_stderr(&format!("{}\n", lines.join("\n")));
    out::exit(1);
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

/// A row as an insertion-ordered JS-valued map (column order preserved).
fn row_to_map(row: &rusqlite::Row<'_>, cols: &[String]) -> Map<String, Value> {
    let mut m = Map::new();
    for (idx, name) in cols.iter().enumerate() {
        m.insert(
            name.clone(),
            row.get_ref(idx)
                .map(cell_to_js_value)
                .unwrap_or(Value::Null),
        );
    }
    m
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

fn s<'a>(m: &'a Map<String, Value>, key: &str) -> &'a str {
    m.get(key).and_then(Value::as_str).unwrap_or("")
}

fn n(m: &Map<String, Value>, key: &str) -> f64 {
    m.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn truthy(m: &Map<String, Value>, key: &str) -> bool {
    match m.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(x)) => x.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(x)) => !x.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

/// v4 `requireMount(db, spec)` — errors go to stderr + exit 1 (not thrown).
fn require_mount(conn: &rusqlite::Connection, spec: Option<&str>) -> Map<String, Value> {
    let Some(spec) = spec.filter(|s| !s.is_empty()) else {
        out::elog("Error: mount name or id is required");
        out::exit(1);
    };
    if is_uuid(spec) {
        let rows = query_maps(
            conn,
            "SELECT * FROM doc_mount_points WHERE id = ?",
            &[&spec],
        )
        .unwrap_or_default();
        let Some(row) = rows.into_iter().next() else {
            out::elog(&format!("No mount point found with id {spec}"));
            out::exit(1);
        };
        return row;
    }
    let rows = query_maps(
        conn,
        "SELECT * FROM doc_mount_points\n     WHERE LOWER(name) = LOWER(?)\n     ORDER BY name COLLATE NOCASE",
        &[&spec],
    )
    .unwrap_or_default();
    if rows.is_empty() {
        out::elog(&format!("No mount point found with name \"{spec}\""));
        out::exit(1);
    }
    if rows.len() > 1 {
        out::elog(&format!(
            "Ambiguous mount name \"{spec}\" matches multiple mounts:"
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

/// v4 `loadAmbiguousMountNames`.
fn load_ambiguous_mount_names(conn: &rusqlite::Connection) -> std::collections::HashSet<String> {
    let rows = query_maps(
        conn,
        "SELECT name FROM doc_mount_points WHERE enabled = 1",
        &[],
    )
    .unwrap_or_default();
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for r in rows {
        let key = s(&r, "name").trim().to_lowercase();
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .filter(|(_, c)| *c > 1)
        .map(|(k, _)| k)
        .collect()
}

/// v4 `rowQtapUri` (launcher `formatDocStoreUri(authority, path)`).
fn row_qtap_uri(
    mount_name: &str,
    mount_id: &str,
    relative_path: &str,
    ambiguous: &std::collections::HashSet<String>,
) -> String {
    let use_id = mount_name.is_empty() || ambiguous.contains(&mount_name.trim().to_lowercase());
    let authority = if use_id { mount_id } else { mount_name };
    format_qtap_uri(&QtapUriParts {
        scope: DocEditScope::DocumentStore,
        mount_point: Some(authority.to_string()),
        path: relative_path.to_string(),
        heading: None,
        level: None,
        query: None,
    })
}

// ----------------------------------------------------------------------------
// list
// ----------------------------------------------------------------------------

fn handle_list(flags: &Flags) -> Result<(), String> {
    let db = open_docs_db(flags)?;
    let rows = query_maps(
        &db.conn,
        "\n      SELECT id, name, mountType, storeType, basePath, enabled,\n             fileCount, chunkCount, totalSizeBytes, scanStatus\n      FROM doc_mount_points\n      ORDER BY name COLLATE NOCASE\n    ",
        &[],
    )?;
    if flags.names_only {
        for row in &rows {
            out::log(s(row, "name"));
        }
        return Ok(());
    }
    if flags.json {
        out::write_stdout(
            format!(
                "{}\n",
                json_stringify_pretty(&Value::from(
                    rows.into_iter().map(Value::Object).collect::<Vec<_>>()
                ))
            )
            .as_bytes(),
        );
        return Ok(());
    }
    if rows.is_empty() {
        out::log("(no mount points)");
        return Ok(());
    }
    let display: Vec<Map<String, Value>> = rows
        .iter()
        .map(|r| {
            let mut m = Map::new();
            m.insert("id".into(), r.get("id").cloned().unwrap_or(Value::Null));
            m.insert("name".into(), r.get("name").cloned().unwrap_or(Value::Null));
            m.insert(
                "type".into(),
                r.get("mountType").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "store".into(),
                r.get("storeType").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "enabled".into(),
                Value::from(if truthy(r, "enabled") { "yes" } else { "no" }),
            );
            m.insert(
                "files".into(),
                r.get("fileCount").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "chunks".into(),
                r.get("chunkCount").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "size".into(),
                Value::from(format_bytes(n(r, "totalSizeBytes"))),
            );
            m.insert(
                "status".into(),
                r.get("scanStatus").cloned().unwrap_or(Value::Null),
            );
            m
        })
        .collect();
    out::write_stdout(console_table(&display).as_bytes());
    Ok(())
}

// ----------------------------------------------------------------------------
// show
// ----------------------------------------------------------------------------

fn handle_show(flags: &Flags, id: Option<&str>) -> Result<(), String> {
    if id.is_none() {
        out::elog("Usage: quilltap docs show <mount-id>");
        out::exit(1);
    }
    let db = open_docs_db(flags)?;
    let mount = require_mount(&db.conn, id);
    let mount_id = s(&mount, "id").to_string();

    let count = |sql: &str| -> Result<f64, String> {
        db.conn
            .query_row(sql, [&mount_id], |row| row.get::<_, f64>(0))
            .map_err(|e| sqlite_msg(&e))
    };
    let file_count =
        count("SELECT COUNT(*) AS n FROM doc_mount_file_links WHERE mountPointId = ?")?;
    let chunk_count = count("SELECT COUNT(*) AS n FROM doc_mount_chunks WHERE mountPointId = ?")?;
    let blob_count = count(
        "\n      SELECT COUNT(*) AS n FROM doc_mount_file_links l\n      JOIN doc_mount_blobs b ON b.fileId = l.fileId\n      WHERE l.mountPointId = ?\n    ",
    )?;
    let doc_count = count(
        "\n      SELECT COUNT(*) AS n FROM doc_mount_file_links l\n      JOIN doc_mount_documents d ON d.fileId = l.fileId\n      WHERE l.mountPointId = ?\n    ",
    )?;

    if flags.json {
        let mut obj = mount.clone();
        obj.insert(
            "filesActual".into(),
            crate::nodefmt::js_number_to_json(file_count),
        );
        obj.insert(
            "chunksActual".into(),
            crate::nodefmt::js_number_to_json(chunk_count),
        );
        obj.insert(
            "blobsActual".into(),
            crate::nodefmt::js_number_to_json(blob_count),
        );
        obj.insert(
            "documentsActual".into(),
            crate::nodefmt::js_number_to_json(doc_count),
        );
        out::write_stdout(format!("{}\n", json_stringify_pretty(&Value::Object(obj))).as_bytes());
        return Ok(());
    }

    out::log(&format!(
        "{BOLD}{}{RESET}  {DIM}{}{RESET}",
        s(&mount, "name"),
        mount_id
    ));
    out::log(&format!(
        "  Type:           {} / {}",
        s(&mount, "mountType"),
        s(&mount, "storeType")
    ));
    let base_path = s(&mount, "basePath");
    out::log(&format!(
        "  Base path:      {}",
        if base_path.is_empty() {
            format!("{DIM}(database-backed){RESET}")
        } else {
            base_path.to_string()
        }
    ));
    out::log(&format!(
        "  Enabled:        {}",
        if truthy(&mount, "enabled") {
            format!("{GREEN}yes{RESET}")
        } else {
            format!("{RED}no{RESET}")
        }
    ));
    out::log(&format!(
        "  Scan status:    {}{}",
        s(&mount, "scanStatus"),
        if s(&mount, "lastScanError").is_empty() {
            String::new()
        } else {
            format!("  {RED}{}{RESET}", s(&mount, "lastScanError"))
        }
    ));
    out::log(&format!(
        "  Conversion:     {}{}",
        s(&mount, "conversionStatus"),
        if s(&mount, "conversionError").is_empty() {
            String::new()
        } else {
            format!("  {RED}{}{RESET}", s(&mount, "conversionError"))
        }
    ));
    out::log(&format!(
        "  Last scanned:   {}",
        if s(&mount, "lastScannedAt").is_empty() {
            format!("{DIM}never{RESET}")
        } else {
            s(&mount, "lastScannedAt").to_string()
        }
    ));
    out::log(&format!(
        "  Cached counts:  {} files, {} chunks, {}",
        js_num_string(n(&mount, "fileCount")),
        js_num_string(n(&mount, "chunkCount")),
        format_bytes(n(&mount, "totalSizeBytes"))
    ));
    out::log(&format!(
        "  Live counts:    {} files, {} chunks, {} blobs, {} db-docs",
        js_num_string(file_count),
        js_num_string(chunk_count),
        js_num_string(blob_count),
        js_num_string(doc_count)
    ));
    if file_count != n(&mount, "fileCount") || chunk_count != n(&mount, "chunkCount") {
        out::log(&format!(
            "  {YELLOW}Note: cached counts disagree with live counts; consider 'docs scan'{RESET}"
        ));
    }
    out::log(&format!("  Created:        {}", s(&mount, "createdAt")));
    out::log(&format!("  Updated:        {}", s(&mount, "updatedAt")));
    Ok(())
}

// ----------------------------------------------------------------------------
// ls / dir
// ----------------------------------------------------------------------------

/// v4 `normalizeLsPath`.
fn normalize_ls_path(p: Option<&str>) -> String {
    let Some(p) = p else {
        return String::new();
    };
    let trimmed = p.trim_matches('/');
    if trimmed == "." || trimmed.is_empty() {
        return String::new();
    }
    trimmed.to_string()
}

/// v4 `formatLsDate` — LOCAL-time `YYYY-MM-DD HH:MM` (the process TZ; both
/// diff sides share it); invalid dates fall back to `iso.slice(0, 16)`
/// (UTF-16 slice).
fn format_ls_date(iso: &str) -> String {
    if iso.is_empty() {
        return "-".to_string();
    }
    let Some(ms) = quilltap_core::clock::iso_to_ms(iso) else {
        return quilltap_core::jsstr::utf16_truncate(iso, 16);
    };
    let Ok(ts) = jiff::Timestamp::from_millisecond(ms) else {
        return quilltap_core::jsstr::utf16_truncate(iso, 16);
    };
    let zoned = ts.to_zoned(jiff::tz::TimeZone::system());
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        zoned.year(),
        zoned.month(),
        zoned.day(),
        zoned.hour(),
        zoned.minute()
    )
}

/// Cached per process: one PRAGMA per run, not one per prepared statement
/// (v4 `docs-commands.js`'s module-level `linkGroupColumnPresent`).
static LINK_GROUP_COLUMN_PRESENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn has_link_group_column(conn: &rusqlite::Connection) -> bool {
    *LINK_GROUP_COLUMN_PRESENT.get_or_init(|| {
        let probe = || -> rusqlite::Result<bool> {
            let mut stmt = conn.prepare("PRAGMA table_info(\"doc_mount_file_links\")")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                if row.get::<_, String>(1)? == "linkGroupId" {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        probe().unwrap_or(false)
    })
}

/// The `links` column counts deliberate hard links — members of this file's
/// `linkGroupId` — NOT rows sharing a `fileId` (v4 `40319484`). Content rows are
/// addressed by sha256, so a boilerplate or empty file collects dozens of
/// unrelated links that share its bytes purely by coincidence; reporting those
/// as links told the operator a file was linked into 36 stores when nothing had
/// been linked at all. An instance that has not gained the `linkGroupId` column
/// yet degrades to `1` rather than failing the whole listing.
fn ls_file_columns(conn: &rusqlite::Connection) -> String {
    let present = has_link_group_column(conn);
    let link_count = if present {
        "(CASE WHEN l.linkGroupId IS NULL THEN 1 ELSE\n         (SELECT COUNT(*) FROM doc_mount_file_links g WHERE g.linkGroupId = l.linkGroupId) END)"
    } else {
        "1"
    };
    let link_group_id = if present { "l.linkGroupId" } else { "NULL" };
    format!(
        "\n  {link_group_id} AS linkGroupId,\n  l.id AS linkId, l.fileId, l.relativePath, l.fileName, l.lastModified,\n  l.extractionStatus, l.extractedTextSha256, l.chunkCount,\n  f.fileType, f.fileSizeBytes, f.source, f.sha256,\n  {link_count} AS linkCount,\n  (SELECT COUNT(*) FROM doc_mount_chunks\n    WHERE linkId = l.id AND embedding IS NOT NULL) AS embeddedChunkCount\n"
    )
}

enum LsTarget {
    Root,
    File(Box<Map<String, Value>>),
    Folder { path: String },
    None,
}

fn resolve_ls_target(
    conn: &rusqlite::Connection,
    mount_id: &str,
    normalized_path: &str,
) -> Result<LsTarget, String> {
    if normalized_path.is_empty() {
        return Ok(LsTarget::Root);
    }
    let cols = ls_file_columns(conn);
    let file = query_maps(
        conn,
        &format!(
            "\n    SELECT {cols}\n    FROM doc_mount_file_links l\n    JOIN doc_mount_files f ON f.id = l.fileId\n    WHERE l.mountPointId = ? AND l.relativePath = ?\n  "
        ),
        &[&mount_id, &normalized_path],
    )?;
    if let Some(f) = file.into_iter().next() {
        return Ok(LsTarget::File(Box::new(f)));
    }
    let folder = query_maps(
        conn,
        "SELECT id, name, path, parentId, createdAt, updatedAt\n     FROM doc_mount_folders WHERE mountPointId = ? AND path = ?",
        &[&mount_id, &normalized_path],
    )?;
    if !folder.is_empty() {
        return Ok(LsTarget::Folder {
            path: normalized_path.to_string(),
        });
    }
    let like = format!("{normalized_path}/%");
    let has_files: bool = conn
        .query_row(
            "SELECT 1 FROM doc_mount_file_links\n    WHERE mountPointId = ? AND relativePath LIKE ? LIMIT 1",
            rusqlite::params![mount_id, like],
            |_| Ok(true),
        )
        .unwrap_or(false);
    let has_folders: bool = conn
        .query_row(
            "SELECT 1 FROM doc_mount_folders\n    WHERE mountPointId = ? AND path LIKE ? LIMIT 1",
            rusqlite::params![mount_id, like],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if has_files || has_folders {
        return Ok(LsTarget::Folder {
            path: normalized_path.to_string(),
        });
    }
    Ok(LsTarget::None)
}

#[allow(clippy::type_complexity)]
fn fetch_ls_rows(
    conn: &rusqlite::Connection,
    mount_id: &str,
    parent_path: &str,
) -> Result<(Vec<Map<String, Value>>, Vec<Map<String, Value>>), String> {
    let folders = if parent_path.is_empty() {
        query_maps(
            conn,
            "\n        SELECT id, name, path, createdAt, updatedAt\n        FROM doc_mount_folders\n        WHERE mountPointId = ?\n          AND path NOT LIKE '%/%'\n        ORDER BY name COLLATE NOCASE\n      ",
            &[&mount_id],
        )?
    } else {
        let like = format!("{parent_path}/%");
        let not_like = format!("{parent_path}/%/%");
        query_maps(
            conn,
            "\n        SELECT id, name, path, createdAt, updatedAt\n        FROM doc_mount_folders\n        WHERE mountPointId = ?\n          AND path LIKE ?\n          AND path NOT LIKE ?\n        ORDER BY name COLLATE NOCASE\n      ",
            &[&mount_id, &like, &not_like],
        )?
    };
    let cols = ls_file_columns(conn);
    let files = if parent_path.is_empty() {
        query_maps(
            conn,
            &format!(
                "\n        SELECT {cols}\n        FROM doc_mount_file_links l\n        JOIN doc_mount_files f ON f.id = l.fileId\n        WHERE l.mountPointId = ?\n          AND l.relativePath NOT LIKE '%/%'\n        ORDER BY l.fileName COLLATE NOCASE\n      "
            ),
            &[&mount_id],
        )?
    } else {
        let like = format!("{parent_path}/%");
        let not_like = format!("{parent_path}/%/%");
        query_maps(
            conn,
            &format!(
                "\n        SELECT {cols}\n        FROM doc_mount_file_links l\n        JOIN doc_mount_files f ON f.id = l.fileId\n        WHERE l.mountPointId = ?\n          AND l.relativePath LIKE ?\n          AND l.relativePath NOT LIKE ?\n        ORDER BY l.fileName COLLATE NOCASE\n      "
            ),
            &[&mount_id, &like, &not_like],
        )?
    };
    Ok((folders, files))
}

/// Members of each named hard-link group, keyed by `linkGroupId` — v4
/// `fetchLinkGroupMembers` (`40319484`). Grouping is by `linkGroupId` rather
/// than `fileId` on purpose (see [`ls_file_columns`]): a shared `fileId` only
/// means "identical bytes", which is not a link.
fn fetch_link_group_members(
    conn: &rusqlite::Connection,
    group_ids: &[String],
) -> Result<std::collections::HashMap<String, Vec<Map<String, Value>>>, String> {
    let mut by_group: std::collections::HashMap<String, Vec<Map<String, Value>>> =
        std::collections::HashMap::new();
    if group_ids.is_empty() {
        return Ok(by_group);
    }
    let placeholders = group_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let params: Vec<&dyn rusqlite::ToSql> = group_ids
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    let rows = query_maps(
        conn,
        &format!(
            "\n    SELECT l.linkGroupId, l.relativePath, l.mountPointId, m.name AS mountName\n    FROM doc_mount_file_links l\n    JOIN doc_mount_points m ON m.id = l.mountPointId\n    WHERE l.linkGroupId IN ({placeholders})\n    ORDER BY m.name COLLATE NOCASE, l.relativePath COLLATE NOCASE\n  "
        ),
        &params,
    )?;
    for r in rows {
        let group_id = s(&r, "linkGroupId").to_string();
        let mut link = Map::new();
        link.insert(
            "mountPointId".into(),
            r.get("mountPointId").cloned().unwrap_or(Value::Null),
        );
        link.insert(
            "mountName".into(),
            r.get("mountName").cloned().unwrap_or(Value::Null),
        );
        link.insert(
            "relativePath".into(),
            r.get("relativePath").cloned().unwrap_or(Value::Null),
        );
        by_group.entry(group_id).or_default().push(link);
    }
    Ok(by_group)
}

/// v4 `sortLsFiles` — stable sorts; `name` uses `localeCompare(...,
/// {sensitivity:'base'})` (ported via the ICU tertiary collator; a
/// case/accent-only-different pair is a documented seam the corpus avoids).
fn sort_ls_files(
    files: Vec<Map<String, Value>>,
    sort_type: &str,
    reverse: bool,
) -> Vec<Map<String, Value>> {
    let mut sorted = files;
    match sort_type {
        "time" => sorted.sort_by(|a, b| {
            let at = ls_time_ms(a);
            let bt = ls_time_ms(b);
            bt.partial_cmp(&at).unwrap_or(std::cmp::Ordering::Equal)
        }),
        "size" => sorted.sort_by(|a, b| {
            n(b, "fileSizeBytes")
                .partial_cmp(&n(a, "fileSizeBytes"))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "links" => sorted.sort_by(|a, b| {
            n(b, "linkCount")
                .partial_cmp(&n(a, "linkCount"))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        _ => sorted.sort_by(|a, b| {
            quilltap_core::collation::locale_compare(s(a, "fileName"), s(b, "fileName"))
        }),
    }
    if reverse {
        sorted.reverse();
    }
    sorted
}

fn ls_time_ms(f: &Map<String, Value>) -> f64 {
    let lm = s(f, "lastModified");
    if lm.is_empty() {
        return 0.0; // new Date(0)
    }
    quilltap_core::clock::iso_to_ms(lm)
        .map(|ms| ms as f64)
        .unwrap_or(f64::NAN)
}

fn handle_ls(
    flags: &Flags,
    mount_spec: Option<&str>,
    raw_path: Option<&str>,
) -> Result<(), String> {
    if mount_spec.is_none() {
        out::elog("Usage: quilltap docs ls <mount> [path] [--recursive|-R] [--sort name|time|size|links] [-r] [--links]");
        out::exit(1);
    }
    let db = open_docs_db(flags)?;
    let mount = require_mount(&db.conn, mount_spec);
    let mount_id = s(&mount, "id").to_string();
    let mount_name = s(&mount, "name").to_string();
    let normalized_path = normalize_ls_path(raw_path);

    let mut folders: Vec<Map<String, Value>> = Vec::new();
    let mut files: Vec<Map<String, Value>>;
    let mut single_file = false;
    let mut all_files_by_folder: std::collections::BTreeMap<String, Vec<Map<String, Value>>> =
        std::collections::BTreeMap::new();

    if flags.recursive {
        let cols = ls_file_columns(&db.conn);
        let all_files = if normalized_path.is_empty() {
            query_maps(
                &db.conn,
                &format!(
                    "\n            SELECT {cols}\n            FROM doc_mount_file_links l\n            JOIN doc_mount_files f ON f.id = l.fileId\n            WHERE l.mountPointId = ?\n            ORDER BY l.relativePath\n          "
                ),
                &[&mount_id],
            )?
        } else {
            let prefix = format!("{}/", normalized_path.trim_end_matches('/'));
            let like = format!("{prefix}%");
            query_maps(
                &db.conn,
                &format!(
                    "\n            SELECT {cols}\n            FROM doc_mount_file_links l\n            JOIN doc_mount_files f ON f.id = l.fileId\n            WHERE l.mountPointId = ? AND l.relativePath LIKE ?\n            ORDER BY l.relativePath\n          "
                ),
                &[&mount_id, &like],
            )?
        };
        for file in &all_files {
            let rel = s(file, "relativePath");
            let folder_path = match rel.rfind('/') {
                Some(idx) => rel[..idx].to_string(),
                None => String::new(),
            };
            all_files_by_folder
                .entry(folder_path)
                .or_default()
                .push(file.clone());
        }
        files = all_files;
    } else {
        match resolve_ls_target(&db.conn, &mount_id, &normalized_path)? {
            LsTarget::None => {
                out::elog(&format!(
                    "No file or folder at \"{}\" in mount {}",
                    if normalized_path.is_empty() {
                        "/"
                    } else {
                        &normalized_path
                    },
                    mount_name
                ));
                out::exit(1);
            }
            LsTarget::Root => {
                let (fo, fi) = fetch_ls_rows(&db.conn, &mount_id, "")?;
                folders = fo;
                files = fi;
            }
            LsTarget::Folder { path } => {
                let (fo, fi) = fetch_ls_rows(&db.conn, &mount_id, &path)?;
                folders = fo;
                files = fi;
            }
            LsTarget::File(f) => {
                files = vec![*f];
                single_file = true;
            }
        }
    }

    files = sort_ls_files(files, &flags.sort, flags.reverse);

    let want_links = flags.json || flags.links;
    let linked_group_ids: Vec<String> = if want_links {
        files
            .iter()
            .filter(|f| n(f, "linkCount") > 1.0 && !s(f, "linkGroupId").is_empty())
            .map(|f| s(f, "linkGroupId").to_string())
            .collect()
    } else {
        Vec::new()
    };
    let links_by_group = fetch_link_group_members(&db.conn, &linked_group_ids)?;

    // JSON output
    if flags.json {
        let ambiguous = load_ambiguous_mount_names(&db.conn);
        let mut out_rows: Vec<Value> = Vec::new();
        if !flags.recursive {
            for folder in &folders {
                let mut m = Map::new();
                m.insert("type".into(), Value::from("folder"));
                m.insert(
                    "name".into(),
                    folder.get("name").cloned().unwrap_or(Value::Null),
                );
                m.insert(
                    "path".into(),
                    folder.get("path").cloned().unwrap_or(Value::Null),
                );
                m.insert(
                    "uri".into(),
                    Value::from(row_qtap_uri(
                        &mount_name,
                        &mount_id,
                        s(folder, "path"),
                        &ambiguous,
                    )),
                );
                m.insert(
                    "createdAt".into(),
                    folder.get("createdAt").cloned().unwrap_or(Value::Null),
                );
                m.insert(
                    "updatedAt".into(),
                    folder.get("updatedAt").cloned().unwrap_or(Value::Null),
                );
                out_rows.push(Value::Object(m));
            }
        }
        for file in &files {
            let group_id = s(file, "linkGroupId");
            let others = if group_id.is_empty() {
                None
            } else {
                links_by_group.get(group_id)
            };
            let links: Vec<Value> = match others.filter(|o| !o.is_empty()) {
                Some(o) => o.iter().cloned().map(Value::Object).collect(),
                None => {
                    let mut link = Map::new();
                    link.insert("mountPointId".into(), Value::from(mount_id.clone()));
                    link.insert("mountName".into(), Value::from(mount_name.clone()));
                    link.insert(
                        "relativePath".into(),
                        file.get("relativePath").cloned().unwrap_or(Value::Null),
                    );
                    vec![Value::Object(link)]
                }
            };
            let mut m = Map::new();
            m.insert("type".into(), Value::from("file"));
            m.insert(
                "name".into(),
                file.get("fileName").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "relativePath".into(),
                file.get("relativePath").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "uri".into(),
                Value::from(row_qtap_uri(
                    &mount_name,
                    &mount_id,
                    s(file, "relativePath"),
                    &ambiguous,
                )),
            );
            m.insert(
                "fileType".into(),
                file.get("fileType").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "source".into(),
                file.get("source").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "fileSizeBytes".into(),
                file.get("fileSizeBytes").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "sha256".into(),
                file.get("sha256").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "lastModified".into(),
                file.get("lastModified").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "linkCount".into(),
                file.get("linkCount").cloned().unwrap_or(Value::Null),
            );
            let mut text_repr = Map::new();
            text_repr.insert(
                "kind".into(),
                Value::from(if is_text_file_type(s(file, "fileType")) {
                    "inline"
                } else {
                    "extracted"
                }),
            );
            text_repr.insert(
                "extractionStatus".into(),
                file.get("extractionStatus").cloned().unwrap_or(Value::Null),
            );
            text_repr.insert(
                "hasExtractedText".into(),
                Value::from(truthy(file, "extractedTextSha256")),
            );
            m.insert("textRepresentation".into(), Value::Object(text_repr));
            let chunk_count = n(file, "chunkCount");
            let embedded = n(file, "embeddedChunkCount");
            let mut emb = Map::new();
            emb.insert(
                "chunkCount".into(),
                crate::nodefmt::js_number_to_json(chunk_count),
            );
            emb.insert(
                "embeddedChunkCount".into(),
                crate::nodefmt::js_number_to_json(embedded),
            );
            emb.insert(
                "fullyEmbedded".into(),
                Value::from(chunk_count > 0.0 && chunk_count == embedded),
            );
            m.insert("embedding".into(), Value::Object(emb));
            m.insert("links".into(), Value::from(links));
            out_rows.push(Value::Object(m));
        }
        out::write_stdout(
            format!("{}\n", json_stringify_pretty(&Value::from(out_rows))).as_bytes(),
        );
        return Ok(());
    }

    // Recursive text output: grouped by folder
    if flags.recursive {
        let mut any_output = false;
        for (folder_path, folder_files) in &all_files_by_folder {
            let mut folder_files = folder_files.clone();
            folder_files.sort_by(|a, b| {
                quilltap_core::collation::locale_compare(s(a, "fileName"), s(b, "fileName"))
            });
            if !folder_files.is_empty() {
                any_output = true;
                let display_path = if folder_path.is_empty() {
                    "/"
                } else {
                    folder_path
                };
                out::log(&format!("{BOLD}{display_path}{RESET}"));
                for file in &folder_files {
                    let cells = [
                        "-".to_string(),
                        format!("{:>5}", js_num_string(n(file, "linkCount"))),
                        format!("{:>8}", format_bytes(n(file, "fileSizeBytes"))),
                        format_ls_date(s(file, "lastModified")),
                        text_column_marker(s(file, "fileType"), s(file, "extractionStatus"))
                            .to_string(),
                        embed_column_marker(n(file, "chunkCount"), n(file, "embeddedChunkCount"))
                            .to_string(),
                        s(file, "fileName").to_string(),
                    ];
                    out::log(&format!("  {}", cells.join("  ")));
                }
            }
        }
        if !any_output {
            out::log("(no files)");
        }
        return Ok(());
    }

    // Non-recursive text output
    if !single_file && folders.is_empty() && files.is_empty() {
        out::log("(empty)");
        return Ok(());
    }

    struct LsRow {
        r#type: String,
        links: String,
        size: String,
        modified: String,
        text: String,
        emb: String,
        name: String,
        /// v4 `40319484`: the `--links` sibling expansion keys on the deliberate
        /// hard-link GROUP, not on a shared (content-addressed) `fileId`.
        link_group_id: String,
        relative_path: String,
    }
    let header = LsRow {
        r#type: "T".to_string(),
        links: "links".to_string(),
        size: "size".to_string(),
        modified: "modified".to_string(),
        text: "text".to_string(),
        emb: "emb".to_string(),
        name: "name".to_string(),
        link_group_id: String::new(),
        relative_path: String::new(),
    };
    let mut data_rows: Vec<LsRow> = Vec::new();
    for folder in &folders {
        let modified = if s(folder, "updatedAt").is_empty() {
            s(folder, "createdAt")
        } else {
            s(folder, "updatedAt")
        };
        data_rows.push(LsRow {
            r#type: "d".to_string(),
            links: "-".to_string(),
            size: "-".to_string(),
            modified: format_ls_date(modified),
            text: "-".to_string(),
            emb: "-".to_string(),
            name: format!("{}/", s(folder, "name")),
            link_group_id: String::new(),
            relative_path: String::new(),
        });
    }
    for file in &files {
        data_rows.push(LsRow {
            r#type: "-".to_string(),
            links: js_num_string(n(file, "linkCount")),
            size: format_bytes(n(file, "fileSizeBytes")),
            modified: format_ls_date(s(file, "lastModified")),
            text: text_column_marker(s(file, "fileType"), s(file, "extractionStatus")).to_string(),
            emb: embed_column_marker(n(file, "chunkCount"), n(file, "embeddedChunkCount"))
                .to_string(),
            name: if single_file {
                s(file, "relativePath").to_string()
            } else {
                s(file, "fileName").to_string()
            },
            link_group_id: s(file, "linkGroupId").to_string(),
            relative_path: s(file, "relativePath").to_string(),
        });
    }

    let w_links = std::iter::once(header.links.len())
        .chain(data_rows.iter().map(|r| r.links.chars().count()))
        .max()
        .unwrap_or(0);
    let w_size = std::iter::once(header.size.len())
        .chain(data_rows.iter().map(|r| r.size.chars().count()))
        .max()
        .unwrap_or(0);
    let w_modified = std::iter::once(header.modified.len())
        .chain(data_rows.iter().map(|r| r.modified.chars().count()))
        .max()
        .unwrap_or(0);
    let w_text = std::iter::once(header.text.len())
        .chain(data_rows.iter().map(|r| r.text.chars().count()))
        .max()
        .unwrap_or(0);
    let w_emb = std::iter::once(header.emb.len())
        .chain(data_rows.iter().map(|r| r.emb.chars().count()))
        .max()
        .unwrap_or(0);
    let w_type = 1usize;

    let render_line = |r: &LsRow, dim: bool| -> String {
        let cells = [
            r.r#type.clone(),
            format!("{:>w$}", r.links, w = w_links),
            format!("{:>w$}", r.size, w = w_size),
            format!("{:<w$}", r.modified, w = w_modified),
            format!("{:>w$}", r.text, w = w_text),
            format!("{:>w$}", r.emb, w = w_emb),
            r.name.clone(),
        ];
        let line = cells.join("  ");
        if dim {
            format!("{DIM}{line}{RESET}")
        } else {
            line
        }
    };

    out::log(&render_line(&header, true));
    for r in &data_rows {
        out::log(&render_line(r, false));
        if flags.links && r.r#type == "-" {
            let empty = Vec::new();
            let others: Vec<&Map<String, Value>> = (if r.link_group_id.is_empty() {
                None
            } else {
                links_by_group.get(&r.link_group_id)
            })
            .unwrap_or(&empty)
            .iter()
            .filter(|l| {
                !(s(l, "mountPointId") == mount_id && s(l, "relativePath") == r.relative_path)
            })
            .collect();
            if !others.is_empty() {
                let indent_width = w_type + w_links + w_size + w_modified + w_text + w_emb + 6 * 2;
                let indent = " ".repeat(indent_width);
                for link in others {
                    let same_mount = s(link, "mountPointId") == mount_id;
                    let display = if same_mount {
                        s(link, "relativePath").to_string()
                    } else {
                        format!("{}:{}", s(link, "mountName"), s(link, "relativePath"))
                    };
                    out::log(&format!("{indent}{DIM}→ {display}{RESET}"));
                }
            }
        }
    }
    Ok(())
}

// ----------------------------------------------------------------------------
// tree
// ----------------------------------------------------------------------------

struct TreeNode {
    name: String,
    is_folder: bool,
    path: Option<String>,
    relative_path: Option<String>,
    size: Option<f64>,
    chunk_count: Option<f64>,
    children: Vec<TreeNode>,
}

fn build_folder_tree(
    conn: &rusqlite::Connection,
    mount_id: &str,
    start_path: &str,
    max_depth: i64,
) -> Result<TreeNode, String> {
    fn add_children(
        conn: &rusqlite::Connection,
        mount_id: &str,
        node: &mut TreeNode,
        parent_path: &str,
        current_depth: i64,
        max_depth: i64,
    ) -> Result<(), String> {
        if current_depth >= max_depth {
            return Ok(());
        }
        let folders = if parent_path.is_empty() {
            query_maps(
                conn,
                "SELECT id, name, path FROM doc_mount_folders\n         WHERE mountPointId = ? AND path NOT LIKE '%/%'\n         ORDER BY name COLLATE NOCASE",
                &[&mount_id],
            )?
        } else {
            let like = format!("{parent_path}/%");
            let not_like = format!("{parent_path}/%/%");
            query_maps(
                conn,
                "SELECT id, name, path FROM doc_mount_folders\n         WHERE mountPointId = ? AND path LIKE ? AND path NOT LIKE ?\n         ORDER BY name COLLATE NOCASE",
                &[&mount_id, &like, &not_like],
            )?
        };
        let files = if parent_path.is_empty() {
            query_maps(
                conn,
                "SELECT fileName, relativePath, fileSizeBytes, chunkCount, extractedText\n         FROM doc_mount_file_links l\n         JOIN doc_mount_files f ON f.id = l.fileId\n         WHERE l.mountPointId = ? AND l.relativePath NOT LIKE '%/%'\n         ORDER BY fileName COLLATE NOCASE",
                &[&mount_id],
            )?
        } else {
            let like = format!("{parent_path}/%");
            let not_like = format!("{parent_path}/%/%");
            query_maps(
                conn,
                "SELECT fileName, relativePath, fileSizeBytes, chunkCount, extractedText\n         FROM doc_mount_file_links l\n         JOIN doc_mount_files f ON f.id = l.fileId\n         WHERE l.mountPointId = ? AND l.relativePath LIKE ? AND l.relativePath NOT LIKE ?\n         ORDER BY fileName COLLATE NOCASE",
                &[&mount_id, &like, &not_like],
            )?
        };
        for folder in &folders {
            let mut child = TreeNode {
                name: s(folder, "name").to_string(),
                is_folder: true,
                path: Some(s(folder, "path").to_string()),
                relative_path: None,
                size: None,
                chunk_count: None,
                children: Vec::new(),
            };
            add_children(
                conn,
                mount_id,
                &mut child,
                s(folder, "path"),
                current_depth + 1,
                max_depth,
            )?;
            node.children.push(child);
        }
        for file in &files {
            node.children.push(TreeNode {
                name: s(file, "fileName").to_string(),
                is_folder: false,
                path: None,
                relative_path: Some(s(file, "relativePath").to_string()),
                size: Some(n(file, "fileSizeBytes")),
                chunk_count: Some(n(file, "chunkCount")),
                children: Vec::new(),
            });
        }
        Ok(())
    }

    let mut tree = TreeNode {
        name: start_path.to_string(),
        is_folder: true,
        path: None,
        relative_path: None,
        size: None,
        chunk_count: None,
        children: Vec::new(),
    };
    add_children(conn, mount_id, &mut tree, start_path, 0, max_depth)?;
    Ok(tree)
}

struct NodeCount {
    count: i64,
    truncated: bool,
}

fn render_tree_ascii(
    node: &TreeNode,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    max_nodes: i64,
    node_count: &mut NodeCount,
) {
    if node_count.count >= max_nodes {
        node_count.truncated = true;
        return;
    }
    let mut prefix = prefix.to_string();
    if !is_root {
        node_count.count += 1;
        if node_count.count > max_nodes {
            node_count.truncated = true;
            return;
        }
        let connector = if is_last { "└─" } else { "├─" };
        let type_marker = if node.is_folder { "d" } else { "-" };
        let display_name = if node.is_folder {
            format!("{}/", node.name)
        } else {
            node.name.clone()
        };
        out::log(&format!("{prefix}{connector} {type_marker} {display_name}"));
        prefix = format!("{prefix}{}", if is_last { "   " } else { "│  " });
    }
    let children = &node.children;
    for (i, child) in children.iter().enumerate() {
        if node_count.count >= max_nodes {
            node_count.truncated = true;
            break;
        }
        let last = i == children.len() - 1;
        render_tree_ascii(child, &prefix, last, false, max_nodes, node_count);
    }
}

struct TreeJsonCtx {
    mount_name: String,
    mount_id: String,
    ambiguous: std::collections::HashSet<String>,
}

fn tree_to_json(node: &TreeNode, ctx: &TreeJsonCtx) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::from(node.name.clone()));
    m.insert(
        "type".into(),
        Value::from(if node.is_folder { "folder" } else { "file" }),
    );
    if let Some(path) = &node.path {
        m.insert("path".into(), Value::from(path.clone()));
    }
    if let Some(rel) = &node.relative_path {
        m.insert("relativePath".into(), Value::from(rel.clone()));
    }
    let uri_path = node
        .relative_path
        .clone()
        .or_else(|| node.path.clone())
        .unwrap_or_default();
    m.insert(
        "uri".into(),
        Value::from(row_qtap_uri(
            &ctx.mount_name,
            &ctx.mount_id,
            &uri_path,
            &ctx.ambiguous,
        )),
    );
    if let Some(size) = node.size {
        m.insert("size".into(), crate::nodefmt::js_number_to_json(size));
    }
    if let Some(cc) = node.chunk_count {
        m.insert("chunkCount".into(), crate::nodefmt::js_number_to_json(cc));
    }
    if !node.children.is_empty() {
        m.insert(
            "children".into(),
            Value::from(
                node.children
                    .iter()
                    .map(|c| tree_to_json(c, ctx))
                    .collect::<Vec<_>>(),
            ),
        );
    }
    Value::Object(m)
}

fn handle_tree(
    flags: &Flags,
    mount_spec: Option<&str>,
    raw_path: Option<&str>,
) -> Result<(), String> {
    if mount_spec.is_none() {
        out::elog("Usage: quilltap docs tree <mount> [path] [--depth N] [--max-nodes N] [--long] [--json]");
        out::exit(1);
    }
    let db = open_docs_db(flags)?;
    let mount = require_mount(&db.conn, mount_spec);
    let mount_id = s(&mount, "id").to_string();
    let mount_name = s(&mount, "name").to_string();
    let normalized_path = normalize_ls_path(raw_path);

    let target = resolve_ls_target(&db.conn, &mount_id, &normalized_path)?;
    let start_path = match target {
        LsTarget::None => {
            out::elog(&format!(
                "No file or folder at \"{}\" in mount {}",
                if normalized_path.is_empty() {
                    "/"
                } else {
                    &normalized_path
                },
                mount_name
            ));
            out::exit(1);
        }
        LsTarget::File(_) => {
            out::elog(&format!("\"{normalized_path}\" is a file, not a folder"));
            out::exit(1);
        }
        LsTarget::Folder { path } => path,
        LsTarget::Root => String::new(),
    };

    let max_depth = flags.depth.min(50);
    let max_nodes = flags.max_nodes.max(1);
    let tree = build_folder_tree(&db.conn, &mount_id, &start_path, max_depth)?;

    if flags.json {
        let ctx = TreeJsonCtx {
            mount_name,
            mount_id,
            ambiguous: load_ambiguous_mount_names(&db.conn),
        };
        let output = tree_to_json(&tree, &ctx);
        out::write_stdout(format!("{}\n", json_stringify_pretty(&output)).as_bytes());
        return Ok(());
    }

    let mut node_count = NodeCount {
        count: 0,
        truncated: false,
    };
    let display_root = if start_path.is_empty() {
        "/"
    } else {
        &start_path
    };
    out::log(&format!("{BOLD}{display_root}{RESET}"));
    if !tree.children.is_empty() {
        for (i, child) in tree.children.iter().enumerate() {
            if node_count.count >= max_nodes {
                node_count.truncated = true;
                break;
            }
            let last = i == tree.children.len() - 1;
            render_tree_ascii(child, "", last, false, max_nodes, &mut node_count);
        }
    }
    if node_count.truncated {
        out::log(&format!(
            "{DIM}… (truncated at {max_nodes} nodes; use --max-nodes <N> for more){RESET}"
        ));
    }
    Ok(())
}

// ----------------------------------------------------------------------------
// read
// ----------------------------------------------------------------------------

fn is_binary_file_type(t: &str) -> bool {
    BINARY_FILE_TYPES.contains(&t)
}

fn tty_guard(file_type: &str, flags: &Flags, label: &str) {
    if !out::stdout_is_tty() {
        return;
    }
    if !is_binary_file_type(file_type) {
        return;
    }
    if flags.force {
        return;
    }
    out::elog(&format!(
        "{label} is binary ({file_type}); redirect to a file (e.g. > out.{file_type}) or pass --force."
    ));
    out::exit(1);
}

fn handle_read(flags: &Flags, id: Option<&str>, relative_path: Option<&str>) -> Result<(), String> {
    let (Some(id), Some(relative_path)) = (id, relative_path) else {
        out::elog("Usage: quilltap docs read [--rendered] [--base64] <mount-id> <relativePath>");
        out::exit(1);
    };

    if flags.base64 {
        // Server-required (GET .../files/{path}?encoding=base64) — P4.4.
        out::elog(
            "Error: docs read --base64 is recognized but not yet available in this build of the quilltap CLI.",
        );
        out::exit(1);
    }

    let db = open_docs_db(flags)?;
    let mount = require_mount(&db.conn, Some(id));

    if flags.rendered {
        return read_rendered(&db.conn, &mount, relative_path);
    }
    read_raw(&db.conn, &mount, relative_path, flags)
}

fn read_raw(
    conn: &rusqlite::Connection,
    mount: &Map<String, Value>,
    relative_path: &str,
    flags: &Flags,
) -> Result<(), String> {
    let mount_id = s(mount, "id").to_string();
    let rows = query_maps(
        conn,
        "\n    SELECT l.fileId, f.source, f.fileType\n    FROM doc_mount_file_links l\n    JOIN doc_mount_files f ON f.id = l.fileId\n    WHERE l.mountPointId = ? AND l.relativePath = ?\n  ",
        &[&mount_id, &relative_path],
    )?;
    let Some(file) = rows.into_iter().next() else {
        out::elog(&format!(
            "No file at {relative_path} in mount {}",
            s(mount, "name")
        ));
        out::exit(1);
    };

    tty_guard(s(&file, "fileType"), flags, relative_path);

    let source = s(&file, "source");
    if source == "filesystem" {
        let full_path = crate::nodefmt::node_join(s(mount, "basePath"), relative_path);
        if !std::path::Path::new(&full_path).exists() {
            out::elog(&format!("File missing on disk: {full_path}"));
            out::exit(1);
        }
        let bytes = std::fs::read(&full_path).map_err(|e| e.to_string())?;
        out::write_stdout(&bytes);
        return Ok(());
    }

    if source == "database" {
        let file_id = s(&file, "fileId").to_string();
        if is_text_file_type(s(&file, "fileType")) {
            let content: Option<String> = conn
                .query_row(
                    "SELECT content FROM doc_mount_documents WHERE fileId = ?",
                    [&file_id],
                    |row| row.get(0),
                )
                .ok();
            let Some(content) = content else {
                out::elog(&format!(
                    "File row exists but no document content for {relative_path}"
                ));
                out::exit(1);
            };
            out::write_stdout(content.as_bytes());
            return Ok(());
        }
        let data: Option<Vec<u8>> = conn
            .query_row(
                "SELECT data FROM doc_mount_blobs WHERE fileId = ?",
                [&file_id],
                |row| row.get(0),
            )
            .ok();
        let Some(data) = data else {
            out::elog(&format!(
                "File row exists but no blob bytes for {relative_path}"
            ));
            out::exit(1);
        };
        out::write_stdout(&data);
        return Ok(());
    }

    out::elog(&format!("Unknown file source: {source}"));
    out::exit(1);
}

fn read_rendered(
    conn: &rusqlite::Connection,
    mount: &Map<String, Value>,
    relative_path: &str,
) -> Result<(), String> {
    let mount_id = s(mount, "id").to_string();
    let rows = query_maps(
        conn,
        "\n    SELECT l.id AS linkId, l.fileId, l.extractedText,\n           f.source, f.fileType\n    FROM doc_mount_file_links l\n    JOIN doc_mount_files f ON f.id = l.fileId\n    WHERE l.mountPointId = ? AND l.relativePath = ?\n  ",
        &[&mount_id, &relative_path],
    )?;
    let Some(file) = rows.into_iter().next() else {
        out::elog(&format!(
            "No file at {relative_path} in mount {}",
            s(mount, "name")
        ));
        out::exit(1);
    };

    // 2. Extracted text on the link wins.
    let extracted = s(&file, "extractedText");
    if !extracted.is_empty() {
        out::write_stdout(extracted.as_bytes());
        return Ok(());
    }

    // 3. Database-backed text doc — content IS the rendered form.
    if s(&file, "source") == "database" && is_text_file_type(s(&file, "fileType")) {
        let file_id = s(&file, "fileId").to_string();
        let content: Option<String> = conn
            .query_row(
                "SELECT content FROM doc_mount_documents WHERE fileId = ?",
                [&file_id],
                |row| row.get(0),
            )
            .ok();
        if let Some(content) = content {
            out::write_stdout(content.as_bytes());
            return Ok(());
        }
    }

    // 4. Plain-text filesystem files render to themselves.
    if s(&file, "source") == "filesystem" && is_text_file_type(s(&file, "fileType")) {
        let full_path = crate::nodefmt::node_join(s(mount, "basePath"), relative_path);
        if std::path::Path::new(&full_path).exists() {
            let content = std::fs::read_to_string(&full_path).map_err(|e| e.to_string())?;
            out::write_stdout(content.as_bytes());
            return Ok(());
        }
    }

    // 5. Fall back to concatenated chunks (keyed by linkId).
    let link_id = s(&file, "linkId").to_string();
    let chunks = query_maps(
        conn,
        "\n    SELECT content FROM doc_mount_chunks\n    WHERE linkId = ?\n    ORDER BY chunkIndex\n  ",
        &[&link_id],
    )?;
    if chunks.is_empty() {
        out::elog(&format!(
            "No rendered text available for {relative_path}. Run 'quilltap docs scan {mount_id}' to extract and embed."
        ));
        out::exit(1);
    }
    let joined = chunks
        .iter()
        .map(|c| s(c, "content").to_string())
        .collect::<Vec<_>>()
        .join("\n\n");
    out::write_stdout(joined.as_bytes());
    Ok(())
}
