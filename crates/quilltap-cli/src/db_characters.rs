//! `quilltap db characters` — the five-subcommand family (v4
//! `packages/quilltap/lib/db-commands.js` `cmdCharacters`, v4 `ed8934f1`):
//! `status`, `archives`, `archive`, `rehydrate`, `export`.
//!
//! Three of the five are read-only direct-mode work over the encrypted
//! databases (`status`, `archives`, and `export`'s ARCHIVED leg, which
//! decrypts the bundle straight off the disk — the only way to reach a packed
//! character's mail, photographs and summaries without waking them). The other
//! two — `archive` / `rehydrate` — and `export`'s LIVE leg are HTTP proxies
//! into the running server, because the export pipeline and the unlocked
//! passphrase live in the server process; v5 posts the same v4 URLs
//! (`POST /api/v1/characters/{id}?action=archive|rehydrate`,
//! `POST /api/v1/system/tools?action=export`), so the wire is v4's byte for
//! byte and one stub answers both CLIs in the differential.
//!
//! The bundle crypto is NOT reimplemented here: the decrypt links
//! `quilltap_core::services::character_archive::crypto`. What IS local is the
//! CLI's own passphrase ladder — v4's launcher does **not** call
//! `resolveArchivePassphrase`; it tries the internal sentinel, then
//! `QUILLTAP_DB_PASSPHRASE`, then an interactive prompt, and carries its own
//! sentences. Ported as written.

use serde_json::{Map, Value};

use quilltap_core::services::character_archive::crypto;

use crate::dbopen::{open_readonly, sqlite_msg};
use crate::nodefmt::{
    cell_to_js_value, js_num_string, js_parse_int, json_stringify_pretty, node_join, node_resolve,
    slice_utf16, utf16_len,
};
use crate::out;
use crate::resolve::prompt_passphrase;
use crate::vtable::console_table;

/// v4's `INTERNAL_PASSPHRASE` under its archive-side name
/// (`ARCHIVE_INTERNAL_PASSPHRASE` in `db-commands.js`).
const ARCHIVE_INTERNAL_PASSPHRASE: &str = "__quilltap_no_passphrase__";

/// An error on its way to `dbCommand`'s catch: `Error: <message>` on stderr
/// and `err.ambiguous ? 2 : 1` as the exit code.
#[derive(Debug)]
pub struct CmdError {
    pub message: String,
    pub exit_code: i32,
}

impl CmdError {
    fn new(message: impl Into<String>) -> Self {
        CmdError {
            message: message.into(),
            exit_code: 1,
        }
    }
}

impl From<String> for CmdError {
    fn from(message: String) -> Self {
        CmdError::new(message)
    }
}

/// v4 `makeCtx(dataDir, pepper)` — the per-verb database openers plus the
/// data dir the export leg walks up from to reach `files/`.
pub struct Ctx {
    pub data_dir: String,
    pub pepper: Option<String>,
}

impl Ctx {
    fn open(&self, filename: &str, friendly: &str) -> Result<rusqlite::Connection, String> {
        let db_path = node_join(&self.data_dir, filename);
        if !std::path::Path::new(&db_path).exists() {
            return Err(format!("{friendly} not found: {db_path}"));
        }
        open_readonly(&db_path, self.pepper.as_deref()).map_err(|e| {
            format!(
                "Cannot open {friendly}: {e}\nThe database may be encrypted with a different key, or the .dbkey file may be missing."
            )
        })
    }

    fn open_main(&self) -> Result<rusqlite::Connection, String> {
        self.open("quilltap.db", "main database")
    }

    fn open_mounts(&self) -> Result<rusqlite::Connection, String> {
        self.open("quilltap-mount-index.db", "mount index database")
    }
}

// ---------------------------------------------------------------------------
// arg parsing / output (v4 db-commands.js shared helpers)
// ---------------------------------------------------------------------------

/// v4 `parseSubArgs`: `--k=v`, `--k v` (only when `v` does not itself start
/// with `--`), otherwise `--k` → boolean true. Single-dash tokens are
/// positional.
fn parse_sub_args(args: &[String]) -> (Map<String, Value>, Vec<String>) {
    let mut flags = Map::new();
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if let Some(body) = a.strip_prefix("--") {
            match body.find('=') {
                Some(eq) => {
                    flags.insert(body[..eq].to_string(), Value::from(&body[eq + 1..]));
                }
                None => match args.get(i + 1) {
                    Some(next) if !next.starts_with("--") => {
                        flags.insert(body.to_string(), Value::from(next.as_str()));
                        i += 1;
                    }
                    _ => {
                        flags.insert(body.to_string(), Value::Bool(true));
                    }
                },
            }
        } else {
            positional.push(a.to_string());
        }
        i += 1;
    }
    (flags, positional)
}

/// v4 `asBool(v)`: `v === true || v === 'true' || v === '1'`.
fn as_bool(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(true)) => true,
        Some(Value::String(s)) => s == "true" || s == "1",
        _ => false,
    }
}

/// v4 `asInt(v, dflt)`: absent or bare-flag `true` → the default; otherwise
/// `parseInt(v, 10)` with NaN falling back to the default.
fn as_int(v: Option<&Value>, dflt: i64) -> i64 {
    match v {
        None | Some(Value::Bool(_)) => dflt,
        Some(Value::String(s)) => js_parse_int(Some(s.as_str())).unwrap_or(dflt),
        Some(other) => js_parse_int(Some(&js_to_string(other))).unwrap_or(dflt),
    }
}

/// JS `String(value)` for the shapes the CLI interpolates (SQLite cells and
/// parsed JSON): `null`/absent render as the empty string ONLY where v4
/// applies `?? ''` — this is the raw `String()`, so `null` → `"null"`.
fn js_to_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_num_string(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// v4 `truncate(s, n)`: `null`/absent → `''`; longer than `n` UTF-16 units →
/// the first `n` plus the ` … (+K chars)` tail.
fn truncate(v: &Value, n: usize) -> String {
    if v.is_null() {
        return String::new();
    }
    let s = js_to_string(v);
    let len = utf16_len(&s);
    if len <= n {
        return s;
    }
    format!("{}… (+{} chars)", slice_utf16(&s, n), len - n)
}

fn truncate_str(s: &str, n: usize) -> String {
    truncate(&Value::from(s), n)
}

/// v4 `printTable(rows)`.
fn print_table(rows: &[Map<String, Value>]) {
    if rows.is_empty() {
        out::log("(no results)");
        return;
    }
    out::write_stdout(console_table(rows).as_bytes());
}

/// v4 `printJson(data)`.
fn print_json(v: &Value) {
    out::write_stdout(format!("{}\n", json_stringify_pretty(v)).as_bytes());
}

fn row_to_map(row: &rusqlite::Row<'_>, cols: &[String]) -> rusqlite::Result<Map<String, Value>> {
    let mut m = Map::new();
    for (i, name) in cols.iter().enumerate() {
        m.insert(name.clone(), cell_to_js_value(row.get_ref(i)?));
    }
    Ok(m)
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
    let mut rows = stmt
        .query(rusqlite::params_from_iter(params.iter()))
        .map_err(|e| sqlite_msg(&e))?;
    let mut out_rows = Vec::new();
    while let Some(row) = rows.next().map_err(|e| sqlite_msg(&e))? {
        out_rows.push(row_to_map(row, &cols).map_err(|e| sqlite_msg(&e))?);
    }
    Ok(out_rows)
}

fn table_columns(conn: &rusqlite::Connection, table: &str) -> Result<Vec<String>, String> {
    let rows = query_maps(conn, &format!("PRAGMA table_info({table})"), &[])?;
    Ok(rows
        .iter()
        .filter_map(|r| r.get("name").map(js_to_string))
        .collect())
}

// ---------------------------------------------------------------------------
// name / id resolution (v4 db-helpers.js)
// ---------------------------------------------------------------------------

/// v4 `UUID_RE`.
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    let hex = |c: u8| c.is_ascii_hexdigit();
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            _ => {
                if !hex(c) {
                    return false;
                }
            }
        }
    }
    true
}

/// v4 `ambiguous(kind, rows)` — `.ambiguous = true` becomes exit code 2.
fn ambiguous(kind: &str, rows: &[Map<String, Value>]) -> CmdError {
    let list = rows
        .iter()
        .take(10)
        .map(|r| {
            let label = r
                .get("name")
                .filter(|v| !v.is_null())
                .or_else(|| r.get("title").filter(|v| !v.is_null()))
                .map(js_to_string)
                .unwrap_or_default();
            format!(
                "  {}  {}",
                r.get("id").map(js_to_string).unwrap_or_default(),
                label
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let more = if rows.len() > 10 {
        format!("\n  … and {} more", rows.len() - 10)
    } else {
        String::new()
    };
    CmdError {
        message: format!(
            "Multiple {kind}s match. Use a UUID or a more specific name:\n{list}{more}"
        ),
        exit_code: 2,
    }
}

/// v4 `readVaultAliases(mounts, mountPointId)` — the `aliases` array out of a
/// vault's `properties.json`; `[]` for missing / empty / unparseable.
fn read_vault_aliases(mounts: &rusqlite::Connection, mount_point_id: &str) -> Vec<String> {
    let rows = query_maps(
        mounts,
        "SELECT d.content AS content
           FROM doc_mount_file_links l
           JOIN doc_mount_documents d ON d.fileId = l.fileId
          WHERE l.mountPointId = ? AND LOWER(l.relativePath) = 'properties.json'
          LIMIT 1",
        &[&mount_point_id],
    )
    .unwrap_or_default();
    let Some(content) = rows
        .first()
        .and_then(|r| r.get("content"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return Vec::new();
    };
    let Ok(props) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };
    match props.get("aliases") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// v4 `resolveCharactersByAlias(db, mounts, query)`.
fn resolve_characters_by_alias(
    db: &rusqlite::Connection,
    mounts: &rusqlite::Connection,
    query: &str,
) -> Vec<Map<String, Value>> {
    let needle = query.to_lowercase();
    let chars = query_maps(
        db,
        "SELECT id, name, characterDocumentMountPointId AS mp FROM characters WHERE characterDocumentMountPointId IS NOT NULL",
        &[],
    )
    .unwrap_or_default();
    let mut matches = Vec::new();
    for c in &chars {
        let mp = c.get("mp").map(js_to_string).unwrap_or_default();
        let aliases = read_vault_aliases(mounts, &mp);
        if aliases.iter().any(|a| a.to_lowercase().contains(&needle)) {
            let mut m = Map::new();
            m.insert("id".into(), c.get("id").cloned().unwrap_or(Value::Null));
            m.insert("name".into(), c.get("name").cloned().unwrap_or(Value::Null));
            matches.push(m);
        }
    }
    matches
}

/// v4 `resolveCharacter(db, query, openMounts)` — UUID, then exact name, then
/// the fuzzy LIKE fallback folded with vault aliases.
fn resolve_character(
    db: &rusqlite::Connection,
    query: &str,
    ctx: &Ctx,
) -> Result<Map<String, Value>, CmdError> {
    if is_uuid(query) {
        let rows = query_maps(
            db,
            "SELECT id, name FROM characters WHERE id = ?",
            &[&query],
        )?;
        return rows
            .into_iter()
            .next()
            .ok_or_else(|| CmdError::new(format!("No character with id {query}")));
    }
    let exact = query_maps(
        db,
        "SELECT id, name FROM characters WHERE LOWER(name) = LOWER(?)",
        &[&query],
    )?;
    if exact.len() == 1 {
        return Ok(exact.into_iter().next().unwrap());
    }
    if exact.len() > 1 {
        return Err(ambiguous("character", &exact));
    }

    let mut candidates = query_maps(
        db,
        "SELECT id, name FROM characters WHERE LOWER(name) LIKE LOWER(?) ORDER BY name",
        &[&format!("%{query}%")],
    )?;

    // v4 opens the mount index here and degrades to name-only matching when
    // it cannot be opened.
    if let Ok(mounts) = ctx.open_mounts() {
        let mut seen: Vec<String> = candidates
            .iter()
            .map(|r| r.get("id").map(js_to_string).unwrap_or_default())
            .collect();
        for m in resolve_characters_by_alias(db, &mounts, query) {
            let id = m.get("id").map(js_to_string).unwrap_or_default();
            if !seen.contains(&id) {
                seen.push(id);
                candidates.push(m);
            }
        }
    }

    if candidates.is_empty() {
        return Err(CmdError::new(format!("No character matching '{query}'")));
    }
    if candidates.len() > 1 {
        return Err(ambiguous("character", &candidates));
    }
    Ok(candidates.into_iter().next().unwrap())
}

// ---------------------------------------------------------------------------
// dispatch
// ---------------------------------------------------------------------------

pub fn run(args: &[String], ctx: &Ctx) -> Result<(), CmdError> {
    let (flags, positional) = parse_sub_args(args);
    let sub = positional.first().map(String::as_str).unwrap_or("status");
    match sub {
        "status" => cmd_status(&flags, ctx),
        "archives" => cmd_archives(&flags, ctx),
        "archive" => cmd_archive_verb("archive", positional.get(1), &flags, ctx),
        "rehydrate" => cmd_archive_verb("rehydrate", positional.get(1), &flags, ctx),
        "export" => cmd_export(positional.get(1), &flags, ctx),
        other => Err(CmdError::new(format!(
            "Unknown characters subcommand: {other}. Try: status, archives, archive, rehydrate, export"
        ))),
    }
}

// ---------------------------------------------------------------------------
// characters status
// ---------------------------------------------------------------------------

/// v4 `REQUIRED_VAULT_SINGLE_FILES` — written unconditionally by the vault
/// projection writer, so a healthy character has all five.
const REQUIRED_VAULT_SINGLE_FILES: &[&str] = &[
    "properties.json",
    "identity.md",
    "description.md",
    "personality.md",
    "example-dialogues.md",
];
/// v4 `OPTIONAL_VAULT_SINGLE_FILES` — manifesto is nullable; its absence is
/// surfaced, never counted as missing.
const OPTIONAL_VAULT_SINGLE_FILES: &[&str] = &["manifesto.md"];
/// v4 `PHYSICAL_VAULT_FILES` — written as a pair, so exactly one is an
/// inconsistency worth flagging.
const PHYSICAL_VAULT_FILES: &[&str] = &["physical-description.md", "physical-prompts.json"];

struct VaultStatus {
    id: Value,
    name: Value,
    flag: Value,
    mount_point_id: Value,
    vault: &'static str,
    present_single_files: usize,
    expected_single_files: usize,
    missing_single_files: Vec<String>,
    manifesto_present: bool,
    physical_files_present: usize,
    physical_inconsistent: bool,
    prompts_vault: usize,
    prompts_db: usize,
    scenarios_vault: usize,
    scenarios_db: usize,
    wardrobe_vault: usize,
    diverged: Vec<String>,
    issue: Option<String>,
    pre_cutover: bool,
}

impl VaultStatus {
    /// The JSON projection — key order is v4's object-literal order.
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), self.id.clone());
        m.insert("name".into(), self.name.clone());
        m.insert("flag".into(), self.flag.clone());
        m.insert("mountPointId".into(), self.mount_point_id.clone());
        m.insert("vault".into(), Value::from(self.vault));
        m.insert(
            "presentSingleFiles".into(),
            Value::from(self.present_single_files),
        );
        m.insert(
            "expectedSingleFiles".into(),
            Value::from(self.expected_single_files),
        );
        m.insert(
            "missingSingleFiles".into(),
            Value::from(self.missing_single_files.clone()),
        );
        m.insert(
            "manifestoPresent".into(),
            Value::from(self.manifesto_present),
        );
        m.insert(
            "physicalFilesPresent".into(),
            Value::from(self.physical_files_present),
        );
        m.insert(
            "physicalInconsistent".into(),
            Value::from(self.physical_inconsistent),
        );
        m.insert("promptsVault".into(), Value::from(self.prompts_vault));
        m.insert("promptsDb".into(), Value::from(self.prompts_db));
        m.insert("scenariosVault".into(), Value::from(self.scenarios_vault));
        m.insert("scenariosDb".into(), Value::from(self.scenarios_db));
        m.insert("wardrobeVault".into(), Value::from(self.wardrobe_vault));
        m.insert("diverged".into(), Value::from(self.diverged.clone()));
        m.insert(
            "issue".into(),
            match &self.issue {
                Some(s) => Value::from(s.as_str()),
                None => Value::Null,
            },
        );
        m.insert("preCutover".into(), Value::from(self.pre_cutover));
        Value::Object(m)
    }
}

/// v4 `safeJsonArray(raw)`.
fn safe_json_array(raw: Option<&Value>) -> Vec<Value> {
    let Some(v) = raw else { return Vec::new() };
    let s = match v {
        Value::Null => return Vec::new(),
        Value::String(s) if s.is_empty() => return Vec::new(),
        Value::String(s) => s.clone(),
        other => js_to_string(other),
    };
    match serde_json::from_str::<Value>(&s) {
        Ok(Value::Array(a)) => a,
        _ => Vec::new(),
    }
}

/// v4 `normalizeEmpty(v)` — `null`/absent become `''`, everything else passes
/// through unchanged (so the `!==` below stays type-sensitive).
fn normalize_empty(v: Option<&Value>) -> Value {
    match v {
        None | Some(Value::Null) => Value::from(""),
        Some(other) => other.clone(),
    }
}

/// JS `!==` for the values that reach these comparisons: primitives compare by
/// type + value; two freshly-parsed objects/arrays are never reference-equal,
/// so they always differ.
fn js_strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::String(x), Value::String(y)) => x == y,
        _ => false,
    }
}

fn inspect_character_vault(row: &Map<String, Value>, mounts: &rusqlite::Connection) -> VaultStatus {
    // Pre-4.6-cutover instances still carry the content columns; post-cutover
    // the vault is the only source of truth and there is nothing to diverge
    // FROM. `undefined` in v4 is "the column was not selected".
    let pre_cutover = row.contains_key("identity")
        || row.contains_key("description")
        || row.contains_key("systemPrompts");

    let mut status = VaultStatus {
        id: row.get("id").cloned().unwrap_or(Value::Null),
        name: row.get("name").cloned().unwrap_or(Value::Null),
        flag: match row.get("readPropertiesFromDocumentStore") {
            None | Some(Value::Null) => Value::Null,
            Some(v) => crate::nodefmt::js_number_to_json(js_number(v)),
        },
        mount_point_id: match row.get("characterDocumentMountPointId") {
            Some(Value::String(s)) if !s.is_empty() => Value::from(s.as_str()),
            Some(other) if !other.is_null() && js_truthy(other) => other.clone(),
            _ => Value::Null,
        },
        vault: "missing",
        present_single_files: 0,
        expected_single_files: REQUIRED_VAULT_SINGLE_FILES.len(),
        missing_single_files: Vec::new(),
        manifesto_present: false,
        physical_files_present: 0,
        physical_inconsistent: false,
        prompts_vault: 0,
        prompts_db: 0,
        scenarios_vault: 0,
        scenarios_db: 0,
        wardrobe_vault: 0,
        diverged: Vec::new(),
        issue: None,
        pre_cutover,
    };

    if pre_cutover {
        status.prompts_db = safe_json_array(row.get("systemPrompts")).len();
        status.scenarios_db = safe_json_array(row.get("scenarios")).len();
    }

    let mount_point_id = match row.get("characterDocumentMountPointId") {
        Some(v) if js_truthy(v) => js_to_string(v),
        _ => {
            status.issue = Some("no vault".to_string());
            return status;
        }
    };

    status.vault = "present";

    // One listing of every link for this vault; everything else is lookups.
    let links = query_maps(
        mounts,
        "SELECT relativePath, fileId FROM doc_mount_file_links WHERE mountPointId = ?",
        &[&mount_point_id],
    )
    .unwrap_or_default();
    // A JS `Map` keyed by the lowercased path: insertion-ordered, last write
    // wins on a collision.
    let mut by_path: Vec<(String, String)> = Vec::new();
    for link in &links {
        let key = link
            .get("relativePath")
            .map(js_to_string)
            .unwrap_or_default()
            .to_lowercase();
        let file_id = link.get("fileId").map(js_to_string).unwrap_or_default();
        match by_path.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = file_id,
            None => by_path.push((key, file_id)),
        }
    }
    let get_link =
        |p: &str| -> Option<&String> { by_path.iter().find(|(k, _)| k == p).map(|(_, v)| v) };

    for p in REQUIRED_VAULT_SINGLE_FILES {
        if get_link(p).is_some() {
            status.present_single_files += 1;
        } else {
            status.missing_single_files.push((*p).to_string());
        }
    }
    status.manifesto_present = OPTIONAL_VAULT_SINGLE_FILES
        .iter()
        .all(|p| get_link(p).is_some());
    status.physical_files_present = PHYSICAL_VAULT_FILES
        .iter()
        .filter(|p| get_link(p).is_some())
        .count();
    status.physical_inconsistent = status.physical_files_present == 1;

    for (p, _) in &by_path {
        if p.starts_with("prompts/") && p.ends_with(".md") {
            status.prompts_vault += 1;
        } else if p.starts_with("scenarios/") && p.ends_with(".md") {
            status.scenarios_vault += 1;
        } else if p.starts_with("wardrobe/") && p.ends_with(".md") {
            status.wardrobe_vault += 1;
        }
    }

    if pre_cutover {
        let read_vault = |rel: &str| -> Option<String> {
            let file_id = get_link(rel)?;
            let rows = query_maps(
                mounts,
                "SELECT content FROM doc_mount_documents WHERE fileId = ?",
                &[file_id],
            )
            .unwrap_or_default();
            rows.first()
                .map(|r| r.get("content").map(js_to_string).unwrap_or_default())
        };

        for (vault_path, db_field) in [
            ("identity.md", "identity"),
            ("description.md", "description"),
            ("manifesto.md", "manifesto"),
            ("personality.md", "personality"),
            ("example-dialogues.md", "exampleDialogues"),
        ] {
            let Some(vault) = read_vault(vault_path) else {
                continue;
            };
            // v4: `row[dbField] ?? ''`.
            let db = match row.get(db_field) {
                None | Some(Value::Null) => String::new(),
                Some(other) => js_to_string(other),
            };
            if vault != db {
                status.diverged.push(db_field.to_string());
            }
        }

        if let Some(props_raw) = read_vault("properties.json") {
            match serde_json::from_str::<Value>(&props_raw) {
                Ok(props) => {
                    for (k, db_val) in [
                        ("pronouns", row.get("pronouns")),
                        ("title", row.get("title")),
                        ("firstMessage", row.get("firstMessage")),
                        ("talkativeness", row.get("talkativeness")),
                    ] {
                        if !js_strict_eq(&normalize_empty(props.get(k)), &normalize_empty(db_val)) {
                            status.diverged.push(k.to_string());
                        }
                    }
                    let vault_aliases = match props.get("aliases") {
                        Some(Value::Array(a)) => Value::Array(a.clone()),
                        _ => Value::Array(Vec::new()),
                    };
                    let db_aliases = Value::Array(safe_json_array(row.get("aliases")));
                    // v4 compares the SERIALIZED arrays, not the values —
                    // keep that (it collapses `1` and `1.0`, which a structural
                    // `Value` comparison would not).
                    if serde_json::to_string(&vault_aliases).ok()
                        != serde_json::to_string(&db_aliases).ok()
                    {
                        status.diverged.push("aliases".to_string());
                    }
                    // Tristate, reported only when the vault carries the key.
                    if props.get("systemTransparency").is_some() {
                        let v = props
                            .get("systemTransparency")
                            .filter(|v| !v.is_null())
                            .cloned()
                            .unwrap_or(Value::Null);
                        let d = row
                            .get("systemTransparency")
                            .filter(|v| !v.is_null())
                            .cloned()
                            .unwrap_or(Value::Null);
                        if !js_strict_eq(&v, &d) {
                            status.diverged.push("systemTransparency".to_string());
                        }
                    }
                }
                Err(_) => status
                    .diverged
                    .push("properties.json:unparseable".to_string()),
            }
        }

        let phys_arr = safe_json_array(row.get("physicalDescriptions"));
        let primary = phys_arr.first().filter(|v| js_truthy(v));
        if let Some(phys_md) = read_vault("physical-description.md") {
            let db_val = match primary.and_then(|p| p.get("fullDescription")) {
                Some(v) if !v.is_null() => js_to_string(v),
                _ => String::new(),
            };
            if phys_md != db_val {
                status
                    .diverged
                    .push("physicalDescription.fullDescription".to_string());
            }
        }
        if let Some(phys_json_raw) = read_vault("physical-prompts.json") {
            match serde_json::from_str::<Value>(&phys_json_raw) {
                Ok(phys_json) => {
                    for (k, field) in [
                        ("short", "shortPrompt"),
                        ("medium", "mediumPrompt"),
                        ("long", "longPrompt"),
                        ("complete", "completePrompt"),
                    ] {
                        let db_val = primary.and_then(|p| p.get(field));
                        if !js_strict_eq(
                            &normalize_empty(phys_json.get(k)),
                            &normalize_empty(db_val),
                        ) {
                            status.diverged.push(format!("physical.{k}Prompt"));
                        }
                    }
                }
                Err(_) => status
                    .diverged
                    .push("physical-prompts.json:unparseable".to_string()),
            }
        }

        if status.prompts_vault != status.prompts_db {
            status.diverged.push(format!(
                "systemPrompts:count(vault={},db={})",
                status.prompts_vault, status.prompts_db
            ));
        }
        if status.scenarios_vault != status.scenarios_db {
            status.diverged.push(format!(
                "scenarios:count(vault={},db={})",
                status.scenarios_vault, status.scenarios_db
            ));
        }
    }

    let any_content = status.present_single_files > 0
        || status.manifesto_present
        || status.physical_files_present > 0
        || status.prompts_vault > 0
        || status.scenarios_vault > 0
        || status.wardrobe_vault > 0;
    status.issue = Some(if !any_content {
        "vault empty".to_string()
    } else if !status.missing_single_files.is_empty() {
        format!(
            "{} required files missing",
            status.missing_single_files.len()
        )
    } else if status.physical_inconsistent {
        "physical files incomplete (1 of 2)".to_string()
    } else if !status.diverged.is_empty() {
        format!("diverged ({})", status.diverged.len())
    } else if !pre_cutover {
        "ok (post-cutover, vault is canonical)".to_string()
    } else if js_strict_eq(&status.flag, &Value::from(1)) {
        "ok (vault authoritative)".to_string()
    } else {
        "ok (db matches vault)".to_string()
    });
    status
}

/// JS truthiness for the SQLite/JSON cells these paths see.
fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// JS `Number(v)` for the cells `flag` can hold.
fn js_number(v: &Value) -> f64 {
    match v {
        Value::Null => 0.0,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    }
}

struct Counts {
    ok: usize,
    diverged: usize,
    missing_files: usize,
    no_vault: usize,
    empty: usize,
    phys_incomplete: usize,
}

/// v4 `summarizeCharacterStatuses`.
fn summarize(all: &[VaultStatus]) -> Counts {
    let mut c = Counts {
        ok: 0,
        diverged: 0,
        missing_files: 0,
        no_vault: 0,
        empty: 0,
        phys_incomplete: 0,
    };
    for s in all {
        let Some(issue) = &s.issue else { continue };
        if issue.is_empty() {
            continue;
        }
        if issue.starts_with("ok") {
            c.ok += 1;
        } else if issue == "no vault" {
            c.no_vault += 1;
        } else if issue == "vault empty" {
            c.empty += 1;
        } else if issue.ends_with(" files missing") {
            c.missing_files += 1;
        } else if issue.starts_with("physical files incomplete") {
            c.phys_incomplete += 1;
        } else if issue.starts_with("diverged") {
            c.diverged += 1;
        }
    }
    c
}

fn cmd_status(flags: &Map<String, Value>, ctx: &Ctx) -> Result<(), CmdError> {
    let json = as_bool(flags.get("json"));
    let limit = as_int(flags.get("limit"), 0);
    let only_diverged = as_bool(flags.get("diverged"));
    let only_blocked = as_bool(flags.get("blocked"));
    let id_query = flags.get("id").filter(|v| js_truthy(v)).map(js_to_string);

    let main = ctx.open_main()?;
    let mounts = ctx.open_mounts()?;

    // Probe the schema so the verb works both pre- and post-cutover: after the
    // 4.6 migration the content columns are gone, so ask only for what's there.
    let existing = table_columns(&main, "characters")?;
    const WANTED: &[&str] = &[
        "id",
        "name",
        "characterDocumentMountPointId",
        "systemTransparency",
        "readPropertiesFromDocumentStore",
        "identity",
        "description",
        "manifesto",
        "personality",
        "exampleDialogues",
        "pronouns",
        "aliases",
        "title",
        "firstMessage",
        "talkativeness",
        "physicalDescriptions",
        "systemPrompts",
        "scenarios",
    ];
    let cols: Vec<&str> = WANTED
        .iter()
        .copied()
        .filter(|c| existing.iter().any(|e| e == c))
        .collect();
    let mut sql = format!("SELECT {} FROM characters", cols.join(", "));
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(q) = &id_query {
        let c = resolve_character(&main, q, ctx)?;
        sql.push_str(" WHERE id = ?");
        params.push(rusqlite::types::Value::Text(
            c.get("id").map(js_to_string).unwrap_or_default(),
        ));
    } else {
        sql.push_str(" ORDER BY name");
        if limit > 0 {
            sql.push_str(" LIMIT ?");
            params.push(rusqlite::types::Value::Integer(limit));
        }
    }
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let rows = query_maps(&main, &sql, &param_refs)?;

    let all: Vec<VaultStatus> = rows
        .iter()
        .map(|r| inspect_character_vault(r, &mounts))
        .collect();
    let filtered: Vec<&VaultStatus> = all
        .iter()
        .filter(|s| {
            let issue = s.issue.as_deref().unwrap_or("");
            if only_blocked
                && !(!issue.is_empty()
                    && (issue == "no vault"
                        || issue == "vault empty"
                        || issue.ends_with(" files missing")))
            {
                return false;
            }
            if only_diverged && s.diverged.is_empty() && s.missing_single_files.is_empty() {
                return false;
            }
            true
        })
        .collect();

    let summary = summarize(&all);
    if json {
        let mut counts = Map::new();
        counts.insert("ok".into(), Value::from(summary.ok));
        counts.insert("diverged".into(), Value::from(summary.diverged));
        counts.insert("missingFiles".into(), Value::from(summary.missing_files));
        counts.insert("noVault".into(), Value::from(summary.no_vault));
        counts.insert("empty".into(), Value::from(summary.empty));
        counts.insert(
            "physIncomplete".into(),
            Value::from(summary.phys_incomplete),
        );
        let mut m = Map::new();
        m.insert("totalScanned".into(), Value::from(all.len()));
        m.insert("returned".into(), Value::from(filtered.len()));
        m.insert("counts".into(), Value::Object(counts));
        m.insert(
            "characters".into(),
            Value::from(filtered.iter().map(|s| s.to_json()).collect::<Vec<_>>()),
        );
        print_json(&Value::Object(m));
        return Ok(());
    }

    let mut headline = format!(
        "Scanned {} character{}: {} ok, {} diverged, {} with missing files, {} with no vault, {} empty",
        all.len(),
        if all.len() == 1 { "" } else { "s" },
        summary.ok,
        summary.diverged,
        summary.missing_files,
        summary.no_vault,
        summary.empty
    );
    if summary.phys_incomplete > 0 {
        headline.push_str(&format!(
            ", {} with incomplete physical files",
            summary.phys_incomplete
        ));
    }
    headline.push('.');
    out::log(&headline);
    out::log("");

    // The `flag` column and the DB side of the prompts/scenarios counts only
    // exist pre-cutover; post-cutover show vault-only counts instead of a
    // misleading `vault/db`.
    let any_pre_cutover = all.iter().any(|s| s.pre_cutover);
    let table: Vec<Map<String, Value>> = filtered
        .iter()
        .map(|s| {
            let missing = s.vault == "missing";
            let mut row = Map::new();
            row.insert(
                "id".into(),
                Value::from(slice_utf16(&js_to_string(&s.id), 8)),
            );
            row.insert("name".into(), Value::from(truncate(&s.name, 28)));
            if any_pre_cutover {
                row.insert(
                    "flag".into(),
                    if s.flag.is_null() {
                        Value::from("-")
                    } else {
                        s.flag.clone()
                    },
                );
            }
            row.insert("vault".into(), Value::from(s.vault));
            row.insert(
                "files".into(),
                Value::from(if missing {
                    "-".to_string()
                } else {
                    format!("{}/{}", s.present_single_files, s.expected_single_files)
                }),
            );
            row.insert(
                "manifesto".into(),
                Value::from(if missing {
                    "-"
                } else if s.manifesto_present {
                    "yes"
                } else {
                    "no"
                }),
            );
            row.insert(
                "phys".into(),
                Value::from(if missing {
                    "-".to_string()
                } else {
                    format!("{}/2", s.physical_files_present)
                }),
            );
            row.insert(
                "prompts".into(),
                Value::from(if missing {
                    "-".to_string()
                } else if any_pre_cutover {
                    format!("{}/{}", s.prompts_vault, s.prompts_db)
                } else {
                    s.prompts_vault.to_string()
                }),
            );
            row.insert(
                "scenarios".into(),
                Value::from(if missing {
                    "-".to_string()
                } else if any_pre_cutover {
                    format!("{}/{}", s.scenarios_vault, s.scenarios_db)
                } else {
                    s.scenarios_vault.to_string()
                }),
            );
            // v4 leaves this one a NUMBER when the vault is present — the
            // console.table cell is unquoted for it and quoted for the '-'.
            row.insert(
                "wardrobe".into(),
                if missing {
                    Value::from("-")
                } else {
                    Value::from(s.wardrobe_vault)
                },
            );
            row.insert(
                "status".into(),
                Value::from(match &s.issue {
                    Some(i) => truncate_str(i, 60),
                    None => String::new(),
                }),
            );
            row
        })
        .collect();
    print_table(&table);

    if !filtered.is_empty() && filtered.iter().any(|s| !s.diverged.is_empty()) {
        out::log("");
        out::log("Run with --json to see the full diverged-field list per character.");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// characters archives
// ---------------------------------------------------------------------------

/// v4 `cmdCharactersArchives` — archived characters plus every ARCHIVE bundle
/// on the shelf, loose ones (the survivors of a "keep archived bundles" wipe)
/// included. Read-only.
fn cmd_archives(flags: &Map<String, Value>, ctx: &Ctx) -> Result<(), CmdError> {
    let json = as_bool(flags.get("json"));
    let main = ctx.open_main()?;

    let cols = table_columns(&main, "characters")?;
    if !cols.iter().any(|c| c == "archivedAt") {
        out::log("This database predates character archiving (no archivedAt column).");
        return Ok(());
    }
    let archived = query_maps(
        &main,
        "SELECT id, name, archivedAt, archiveFileId FROM characters WHERE archivedAt IS NOT NULL ORDER BY archivedAt DESC",
        &[],
    )?;
    let bundles = query_maps(
        &main,
        "SELECT id, originalFilename, storageKey, size, createdAt FROM files WHERE category = 'ARCHIVE' ORDER BY createdAt DESC",
        &[],
    )?;

    let referenced: Vec<String> = archived
        .iter()
        .filter_map(|c| c.get("archiveFileId"))
        .filter(|v| js_truthy(v))
        .map(js_to_string)
        .collect();
    let is_referenced = |b: &Map<String, Value>| {
        referenced.contains(&b.get("id").map(js_to_string).unwrap_or_default())
    };
    let loose: Vec<&Map<String, Value>> = bundles.iter().filter(|b| !is_referenced(b)).collect();

    if json {
        let mut m = Map::new();
        m.insert(
            "archivedCharacters".into(),
            Value::from(
                archived
                    .iter()
                    .cloned()
                    .map(Value::Object)
                    .collect::<Vec<_>>(),
            ),
        );
        m.insert(
            "bundles".into(),
            Value::from(
                bundles
                    .iter()
                    .cloned()
                    .map(Value::Object)
                    .collect::<Vec<_>>(),
            ),
        );
        m.insert(
            "looseBundles".into(),
            Value::from(
                loose
                    .iter()
                    .map(|b| Value::Object((*b).clone()))
                    .collect::<Vec<_>>(),
            ),
        );
        print_json(&Value::Object(m));
        return Ok(());
    }

    if archived.is_empty() && bundles.is_empty() {
        out::log("The archive shelf stands empty — no archived characters, no bundles.");
        return Ok(());
    }

    if !archived.is_empty() {
        out::log(&format!("Archived characters ({}):", archived.len()));
        let rows: Vec<Map<String, Value>> = archived
            .iter()
            .map(|c| {
                let mut r = Map::new();
                r.insert(
                    "id".into(),
                    Value::from(slice_utf16(
                        &c.get("id").map(js_to_string).unwrap_or_default(),
                        8,
                    )),
                );
                r.insert(
                    "name".into(),
                    Value::from(truncate(c.get("name").unwrap_or(&Value::Null), 28)),
                );
                r.insert(
                    "archivedAt".into(),
                    c.get("archivedAt").cloned().unwrap_or(Value::Null),
                );
                r.insert(
                    "bundle".into(),
                    Value::from(match c.get("archiveFileId") {
                        Some(v) if js_truthy(v) => slice_utf16(&js_to_string(v), 8),
                        _ => "(none — pre-bundle tombstone)".to_string(),
                    }),
                );
                r
            })
            .collect();
        print_table(&rows);
    }
    if !bundles.is_empty() {
        out::log("");
        out::log(&format!(
            "Archive bundles ({}{}):",
            bundles.len(),
            if !loose.is_empty() {
                format!(", {} loose", loose.len())
            } else {
                String::new()
            }
        ));
        let rows: Vec<Map<String, Value>> = bundles
            .iter()
            .map(|b| {
                let mut r = Map::new();
                r.insert(
                    "id".into(),
                    Value::from(slice_utf16(
                        &b.get("id").map(js_to_string).unwrap_or_default(),
                        8,
                    )),
                );
                r.insert(
                    "file".into(),
                    Value::from(truncate(
                        b.get("originalFilename").unwrap_or(&Value::Null),
                        44,
                    )),
                );
                r.insert(
                    "bytes".into(),
                    b.get("size").cloned().unwrap_or(Value::Null),
                );
                r.insert(
                    "createdAt".into(),
                    b.get("createdAt").cloned().unwrap_or(Value::Null),
                );
                r.insert(
                    "state".into(),
                    Value::from(if is_referenced(b) {
                        "held by character"
                    } else {
                        "loose (importable only)"
                    }),
                );
                r
            })
            .collect();
        print_table(&rows);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// characters archive / rehydrate  (through the RUNNING server)
// ---------------------------------------------------------------------------

/// v4 `cmdCharactersArchiveVerb` — the archive pipeline (export, encryption,
/// prune) and the passphrase cache live in the server process, so the CLI
/// cannot run them against the raw database; the server must be up and it is
/// the server that holds the instance lock. `--write` stays the explicit
/// opt-in to a write.
fn cmd_archive_verb(
    verb: &str,
    query: Option<&String>,
    flags: &Map<String, Value>,
    ctx: &Ctx,
) -> Result<(), CmdError> {
    let Some(query) = query else {
        return Err(CmdError::new(format!(
            "Usage: characters {verb} <name|id> --write [--port N]"
        )));
    };
    if !as_bool(flags.get("write")) {
        return Err(CmdError::new(format!(
            "characters {verb} changes data; add --write to proceed."
        )));
    }
    let port = as_int(flags.get("port"), 3000);

    let character = {
        let main = ctx.open_main()?;
        resolve_character(&main, query, ctx)?
    };
    let character_id = character.get("id").map(js_to_string).unwrap_or_default();
    let character_name = character.get("name").cloned().unwrap_or(Value::Null);

    let path = format!(
        "/api/v1/characters/{}?action={verb}",
        encode_uri_component(&character_id)
    );
    let (status, body_text) = match crate::http::post(port, &path, None) {
        Ok(r) => r,
        Err(message) => {
            return Err(CmdError::new(format!(
                "Could not reach the Quilltap server at http://localhost:{port}: {message}\nThe {verb} operation runs inside the server (it needs the export pipeline and the unlocked passphrase), so start the server first."
            )))
        }
    };
    // v4: `await res.json().catch(() => ({}))`.
    let body: Value =
        serde_json::from_str(&body_text).unwrap_or_else(|_| Value::Object(Map::new()));
    if !(200..300).contains(&status) {
        let message = body
            .get("error")
            .filter(|v| js_truthy(v))
            .map(js_to_string)
            .unwrap_or_else(|| format!("{verb} failed with HTTP {status}"));
        return Err(CmdError::new(message));
    }

    if as_bool(flags.get("json")) {
        print_json(&body);
        return Ok(());
    }
    let name = js_to_string(&character_name);
    if verb == "archive" {
        if body.get("pruneComplete") == Some(&Value::Bool(false)) {
            out::log(&format!(
                "{name} is archived, but the prune did not finish — run the same command again to complete it."
            ));
        } else {
            let file_id = body
                .get("archiveFileId")
                .filter(|v| js_truthy(v))
                .map(js_to_string)
                .unwrap_or_else(|| "(none)".to_string());
            out::log(&format!(
                "{name} rests in the archive. Bundle file: {file_id}."
            ));
        }
    } else {
        match body.get("restored").filter(|v| js_truthy(v)) {
            Some(r) => out::log(&format!(
                "{name} is awake again — {} memories, {} documents, {} blobs restored.",
                js_undefined_aware(r.get("memories")),
                js_undefined_aware(r.get("documents")),
                js_undefined_aware(r.get("blobs"))
            )),
            None => out::log(&format!("{name} is awake again.")),
        }
        if let Some(bundle) = body.get("archiveBundleFileId").filter(|v| js_truthy(v)) {
            out::log(&format!(
                "The archive bundle stays in the file library (file {}); delete it there if you no longer want the spare copy.",
                js_to_string(bundle)
            ));
        }
        if let Some(Value::Array(warnings)) = body.get("warnings") {
            for w in warnings {
                out::log(&format!("warning: {}", js_to_string(w)));
            }
        }
    }
    Ok(())
}

/// JS template-string interpolation of a possibly-absent property.
fn js_undefined_aware(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_string(),
        Some(v) => js_to_string(v),
    }
}

/// `encodeURIComponent` for the character-id path segment.
fn encode_uri_component(s: &str) -> String {
    const UNRESERVED: &str = "-_.!~*'()";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || UNRESERVED.contains(c) {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// characters export
// ---------------------------------------------------------------------------

/// v4 `tryDecryptArchiveBundle(data, passphrase)` — `None` on a wrong
/// passphrase (the caller tries the next one), an error on a malformed
/// bundle. The crypto itself is core's; only the truncation sentence and the
/// wrong-passphrase-is-not-an-error convention are the launcher's own.
fn try_decrypt_archive_bundle(data: &[u8], passphrase: &str) -> Result<Option<Vec<u8>>, CmdError> {
    // v4 reads the header length itself and throws its own sentence before
    // any parsing — a shorter, launcher-specific check than core's.
    const MAGIC_LEN: usize = 8;
    if data.len() < MAGIC_LEN + 4 {
        return Err(CmdError::new(
            "Archive bundle is truncated (bad header or missing auth tag).",
        ));
    }
    let header_length = u32::from_be_bytes([
        data[MAGIC_LEN],
        data[MAGIC_LEN + 1],
        data[MAGIC_LEN + 2],
        data[MAGIC_LEN + 3],
    ]) as usize;
    let body_start = MAGIC_LEN + 4 + header_length;
    if header_length == 0 || data.len() < body_start + 16 {
        return Err(CmdError::new(
            "Archive bundle is truncated (bad header or missing auth tag).",
        ));
    }
    match crypto::decrypt_archive(data, passphrase) {
        Ok(plaintext) => Ok(Some(plaintext)),
        // v4's own decrypt returns null when the key hash disagrees.
        Err(crypto::ArchiveCryptoError::PassphraseMismatch { .. }) => Ok(None),
        // The launcher calls `crypto.createDecipheriv` directly, so a failed
        // tag escapes as NODE's message — not as the service layer's
        // `ArchiveIntegrityError` sentence, which no launcher path can print.
        // (Same string `loadDbKey` already carries for a bad `.dbkey`.)
        Err(crypto::ArchiveCryptoError::Integrity { .. }) => Err(CmdError::new(
            "Unsupported state or unable to authenticate data",
        )),
        // Everything else is a malformed header. v4 would surface Node's own
        // `JSON.parse` sentence here (the deferred `is not valid JSON:`
        // wording seam) and does not check `version`/`kdf` at all — an edge no
        // bundle either app writes can reach, so core's sentence stands.
        Err(e) => Err(CmdError::new(e.to_string())),
    }
}

/// v4 `cmdCharactersExport` — the interchange escape hatch. Archived
/// characters decrypt offline from their bundle (the only way to reach their
/// packed-away material without rehydrating); live characters proxy to the
/// server's export pipeline. Read-only either way.
fn cmd_export(
    query: Option<&String>,
    flags: &Map<String, Value>,
    ctx: &Ctx,
) -> Result<(), CmdError> {
    let Some(query) = query else {
        return Err(CmdError::new(
            "Usage: characters export <name|id> [--out <path>] [--port N]",
        ));
    };

    let character;
    let mut archive_file: Option<Map<String, Value>> = None;
    {
        let main = ctx.open_main()?;
        character = resolve_character(&main, query, ctx)?;
        let character_id = character.get("id").map(js_to_string).unwrap_or_default();
        let character_name = character.get("name").cloned().unwrap_or(Value::Null);
        let cols = table_columns(&main, "characters")?;
        if cols.iter().any(|c| c == "archivedAt") {
            let row = query_maps(
                &main,
                "SELECT archivedAt, archiveFileId FROM characters WHERE id = ?",
                &[&character_id],
            )?
            .into_iter()
            .next();
            let archived_at = row.as_ref().and_then(|r| r.get("archivedAt")).cloned();
            let archive_file_id = row.as_ref().and_then(|r| r.get("archiveFileId")).cloned();
            if row.is_some()
                && archived_at.as_ref().is_some_and(js_truthy)
                && archive_file_id.as_ref().is_some_and(js_truthy)
            {
                let file_id = js_to_string(archive_file_id.as_ref().unwrap());
                archive_file = query_maps(
                    &main,
                    "SELECT id, storageKey, sha256 FROM files WHERE id = ?",
                    &[&file_id],
                )?
                .into_iter()
                .next();
                if archive_file.is_none() {
                    return Err(CmdError::new(format!(
                        "{} is archived but their bundle file row ({file_id}) is missing.",
                        js_to_string(&character_name)
                    )));
                }
            } else if row.is_some() && archived_at.as_ref().is_some_and(js_truthy) {
                return Err(CmdError::new(format!(
                    "{} is a pre-bundle tombstone (no archive bundle exists to export).",
                    js_to_string(&character_name)
                )));
            }
        }
    }

    let character_id = character.get("id").map(js_to_string).unwrap_or_default();
    let character_name = character.get("name").cloned().unwrap_or(Value::Null);
    // v4: `String(character.name || character.id)` with the illegal-filename
    // set replaced.
    let safe_name: String = {
        let base = if js_truthy(&character_name) {
            js_to_string(&character_name)
        } else {
            character_id.clone()
        };
        base.chars()
            .map(|c| {
                if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    '_'
                } else {
                    c
                }
            })
            .collect()
    };
    let out_path = node_resolve(&match flags.get("out") {
        Some(v) if js_truthy(v) => js_to_string(v),
        _ => format!("{safe_name}.qtap"),
    });

    let plaintext: Vec<u8> = if let Some(file) = &archive_file {
        let storage_key = file.get("storageKey").filter(|v| js_truthy(v));
        let Some(storage_key) = storage_key else {
            return Err(CmdError::new(
                "The bundle row has no storage key; the file was never written.",
            ));
        };
        let bundle_path = node_join(
            &node_join(&ctx.data_dir, ".."),
            &format!("files/{}", js_to_string(storage_key)),
        );
        if !std::path::Path::new(&bundle_path).exists() {
            return Err(CmdError::new(format!(
                "Bundle bytes not found on disk: {bundle_path}"
            )));
        }
        let data = std::fs::read(&bundle_path).map_err(|e| CmdError::new(e.to_string()))?;

        if !crypto::is_encrypted_archive(&data) {
            // Pre-encryption plaintext bundle — pass it through untouched.
            data
        } else {
            let mut plaintext = try_decrypt_archive_bundle(&data, ARCHIVE_INTERNAL_PASSPHRASE)?;
            if plaintext.is_none() {
                if let Ok(env) = std::env::var("QUILLTAP_DB_PASSPHRASE") {
                    if !env.is_empty() {
                        plaintext = try_decrypt_archive_bundle(&data, &env)?;
                    }
                }
            }
            if plaintext.is_none() {
                let pass = prompt_passphrase("Archive passphrase: ").map_err(CmdError::new)?;
                if !pass.is_empty() {
                    plaintext = try_decrypt_archive_bundle(&data, &pass)?;
                }
            }
            match plaintext {
                Some(p) => p,
                None => {
                    return Err(CmdError::new(
                        "That passphrase does not open this archive. If you changed your passphrase and this bundle was reported left behind, it still wants the old one.",
                    ))
                }
            }
        }
    } else {
        // Live character: the export pipeline lives in the server.
        let port = as_int(flags.get("port"), 3000);
        let body = serde_json::json!({
            "type": "characters",
            "scope": "selected",
            "selectedIds": [character_id],
            "includeMemories": true,
        })
        .to_string();
        // The response is the `.qtap` bytes themselves (v4 reads
        // `res.arrayBuffer()`), so this leg takes the raw-bytes client.
        let (status, payload) = match crate::http::post_bytes(
            port,
            "/api/v1/system/tools?action=export",
            Some(&body),
        ) {
            Ok(r) => r,
            Err(message) => {
                return Err(CmdError::new(format!(
                    "Could not reach the Quilltap server at http://localhost:{port}: {message}\nExporting a live character runs the server's export pipeline, so start the server first. (Archived characters export offline from their bundle.)"
                )))
            }
        };
        if !(200..300).contains(&status) {
            let parsed: Value =
                serde_json::from_slice(&payload).unwrap_or_else(|_| Value::Object(Map::new()));
            let message = parsed
                .get("error")
                .filter(|v| js_truthy(v))
                .map(js_to_string)
                .unwrap_or_else(|| format!("Export failed with HTTP {status}"));
            return Err(CmdError::new(message));
        }
        payload
    };

    std::fs::write(&out_path, &plaintext).map_err(|e| CmdError::new(e.to_string()))?;
    out::log(&format!(
        "Wrote {} bytes to {out_path}",
        js_num_string(plaintext.len() as f64)
    ));
    if archive_file.is_some() {
        out::log("This is the decrypted archive bundle — a plaintext .qtap. Guard it accordingly.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sub_args_matches_v4() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // `--k v` consumes the value; `--k --j` does not.
        let (flags, pos) = parse_sub_args(&v(&["status", "--limit", "5", "--json"]));
        assert_eq!(flags.get("limit"), Some(&Value::from("5")));
        assert_eq!(flags.get("json"), Some(&Value::Bool(true)));
        assert_eq!(pos, vec!["status"]);
        // `--json status` swallows the subcommand as the flag's VALUE — v4's
        // quirk, so the sub falls back to the default 'status' with json OFF.
        let (flags, pos) = parse_sub_args(&v(&["--json", "status"]));
        assert_eq!(flags.get("json"), Some(&Value::from("status")));
        assert!(pos.is_empty());
        assert!(!as_bool(flags.get("json")));
        // `--k=v`.
        let (flags, _) = parse_sub_args(&v(&["--limit=3"]));
        assert_eq!(flags.get("limit"), Some(&Value::from("3")));
        // A single-dash token is positional.
        let (_, pos) = parse_sub_args(&v(&["-x", "archives"]));
        assert_eq!(pos, vec!["-x", "archives"]);
    }

    #[test]
    fn as_int_and_as_bool_match_v4() {
        assert_eq!(as_int(None, 7), 7);
        assert_eq!(as_int(Some(&Value::Bool(true)), 7), 7);
        assert_eq!(as_int(Some(&Value::from("12x")), 7), 12);
        assert_eq!(as_int(Some(&Value::from("x")), 7), 7);
        assert!(as_bool(Some(&Value::Bool(true))));
        assert!(as_bool(Some(&Value::from("1"))));
        assert!(as_bool(Some(&Value::from("true"))));
        assert!(!as_bool(Some(&Value::from("yes"))));
    }

    #[test]
    fn truncate_counts_utf16_units() {
        assert_eq!(truncate(&Value::Null, 4), "");
        assert_eq!(truncate(&Value::from("abcd"), 4), "abcd");
        assert_eq!(truncate(&Value::from("abcde"), 4), "abcd… (+1 chars)");
        // An astral char is TWO UTF-16 units, as v4's `.length` counts it —
        // all four probed against Node 24's own `truncate`.
        assert_eq!(truncate(&Value::from("𝄞x"), 3), "𝄞x");
        assert_eq!(truncate(&Value::from("𝄞x"), 2), "𝄞… (+1 chars)");
        assert_eq!(truncate(&Value::from("𝄞xy"), 2), "𝄞… (+2 chars)");
    }

    #[test]
    fn uuid_regex_matches_v4() {
        assert!(is_uuid("11111111-1111-4111-8111-111111111111"));
        assert!(is_uuid("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"));
        assert!(!is_uuid("11111111-1111-4111-8111-11111111111"));
        assert!(!is_uuid("zzzzzzzz-1111-4111-8111-111111111111"));
    }

    #[test]
    fn encode_uri_component_matches_js() {
        assert_eq!(encode_uri_component("abc-123"), "abc-123");
        assert_eq!(encode_uri_component("a b/c"), "a%20b%2Fc");
        assert_eq!(encode_uri_component("~!*'()_."), "~!*'()_.");
    }

    /// The launcher's decrypt differs from core's in exactly two places: a
    /// wrong passphrase is `None` (not an error) so the ladder can try the
    /// next one, and truncation carries the launcher's own sentence.
    #[test]
    fn bundle_decrypt_ladder_semantics() {
        let bundle =
            crypto::encrypt_archive(b"payload", ARCHIVE_INTERNAL_PASSPHRASE, None).unwrap();
        assert_eq!(
            try_decrypt_archive_bundle(&bundle, ARCHIVE_INTERNAL_PASSPHRASE)
                .unwrap()
                .unwrap(),
            b"payload"
        );
        assert!(try_decrypt_archive_bundle(&bundle, "wrong")
            .unwrap()
            .is_none());
        let truncated = &bundle[..bundle.len() - 20];
        assert_eq!(
            try_decrypt_archive_bundle(truncated, ARCHIVE_INTERNAL_PASSPHRASE)
                .unwrap_err()
                .message,
            "Archive bundle is truncated (bad header or missing auth tag)."
        );
    }
}
