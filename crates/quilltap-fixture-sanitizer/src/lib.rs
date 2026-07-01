//! The real-snapshot fixture sanitizer — core library.
//!
//! [`sanitize_db`] copies one decrypted source database into a fresh
//! destination database, **deterministically scrubbing** every free-text / BLOB
//! cell while **preserving structural shape**: the schema is replayed verbatim
//! from the source's own `sqlite_master` (so tables/indexes/triggers — and the
//! frozen on-disk schema — carry over byte-for-byte without reproducing v4's
//! DDL here), every row is copied (row counts + the FK-id graph preserved),
//! numbers / booleans / enum tokens / timestamps / ids are kept, and the
//! document store's content↔sha invariant is recomputed so a scrubbed file's
//! `sha256` still matches its (scrubbed) bytes.
//!
//! ## What is preserved vs scrubbed (the policy)
//!
//! - **Preserved:** every INTEGER / REAL cell (numbers, 0/1 booleans, counters,
//!   flags); id-like columns (`id`, `*Id`, `*Ids`, `*_id`) and any UUID-valued
//!   TEXT (the FK graph); timestamp-like columns (`*At`, `*_at`) and any
//!   ISO-8601 TEXT; and a conservative set of enum/control columns (by name and
//!   by the `*Type`/`*Status`/`*Kind`/`*Mode`/`*Role` suffixes) so enum/flag
//!   distributions survive and the fixture stays readable by the ported repos.
//! - **Scrubbed:** all other TEXT (names, titles, descriptions, message
//!   content, prose) → deterministic pseudo-text of the same character length;
//!   TEXT that parses as a JSON object/array → deep-scrubbed (keys, numbers,
//!   bools, uuids, and enum/id/ts-keyed leaves kept; free-text string leaves
//!   scrubbed) so it stays valid JSON; all BLOBs → deterministic same-length
//!   bytes.
//!
//! The scrub is keyed on `SHA-256(column ‖ original)`, so it is **one-way** (the
//! original never appears in the output) and **equality-preserving** (identical
//! originals map to identical output, keeping content-dedup relationships).
//!
//! ## Documented limits (a breadth tool, not a differential artifact)
//!
//! The enum allowlist is conservative and extended empirically against real
//! data; a column holding a short controlled vocabulary that is neither on the
//! list nor suffix-matched would be scrubbed (caught by `--verify` failing to
//! read it, then added). Scrubbed prose is not semantically meaningful — the
//! tool exercises the ported **marshaling** against real-shaped rows, not the
//! content itself.

use std::collections::HashMap;
use std::path::Path;

use quilltap_core::db::doc_mount_file_links::sha256_of_string;
// Link-only: keep the cipher-correct sqlite3 in this crate's link graph.
use quilltap_sqlite3mc_sys as _;

use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// The committed throwaway TEST pepper (base64). Safe to embed — it never keys
/// real data; the same value the synthetic tier-2 fixtures use.
pub const TEST_PEPPER_B64: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";

/// The three database files that make up a Quilltap instance's `data/` dir.
pub const INSTANCE_DB_FILES: &[&str] = &[
    "quilltap.db",
    "quilltap-mount-index.db",
    "quilltap-llm-logs.db",
];

/// Errors from the sanitizer.
#[derive(Debug)]
pub enum SanitizeError {
    Sqlite(rusqlite::Error),
    Key(String),
    Json(String),
}

impl std::fmt::Display for SanitizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SanitizeError::Sqlite(e) => write!(f, "sqlite error: {e}"),
            SanitizeError::Key(m) => write!(f, "key error: {m}"),
            SanitizeError::Json(m) => write!(f, "json error: {m}"),
        }
    }
}
impl std::error::Error for SanitizeError {}
impl From<rusqlite::Error> for SanitizeError {
    fn from(e: rusqlite::Error) -> Self {
        SanitizeError::Sqlite(e)
    }
}

type Result<T> = std::result::Result<T, SanitizeError>;

/// A summary of one sanitized database.
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub tables: usize,
    pub rows: usize,
    pub files_rekeyed: usize,
}

/// Open an existing encrypted DB for **reading**. Per the read-path rule, `key`
/// is the first and only pragma — no `journal_mode`/`foreign_keys` on a read
/// path (those would force header writes that race the cipher).
pub fn open_read(path: &Path, pepper_b64: &str) -> Result<Connection> {
    let hex = quilltap_core::dbkey::pepper_b64_to_key_hex(pepper_b64)
        .map_err(|e| SanitizeError::Key(e.to_string()))?;
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "key", format!("x'{hex}'"))?;
    Ok(conn)
}

/// Open/create a **fresh** encrypted DB for writing under `pepper_b64` (the
/// default cipher is sqleet/ChaCha20 — no `cipher=` pragma). `key` is set first.
pub fn open_write_fresh(path: &Path, pepper_b64: &str) -> Result<Connection> {
    let hex = quilltap_core::dbkey::pepper_b64_to_key_hex(pepper_b64)
        .map_err(|e| SanitizeError::Key(e.to_string()))?;
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "key", format!("x'{hex}'"))?;
    Ok(conn)
}

/// Sanitize `src` into `dst` (a fresh, already-keyed connection). Replays the
/// schema, copies every user table scrubbing per the policy, and recomputes the
/// document-store content↔sha invariant.
pub fn sanitize_db(src: &Connection, dst: &Connection) -> Result<Report> {
    // Bulk-copy exact rows → the FK graph is internally consistent already, so
    // enforcement only gets in the way during the copy.
    dst.execute_batch("PRAGMA foreign_keys=OFF;")?;

    // 1. Replay the schema verbatim (tables, indexes, triggers), in rowid order
    //    so a table precedes its indexes.
    let ddl: Vec<String> = {
        let mut st = src.prepare(
            "SELECT sql FROM sqlite_master \
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY rowid",
        )?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    for sql in &ddl {
        dst.execute_batch(&format!("{sql};"))?;
    }

    // 2. The document-store content↔sha overrides (mount-index db only).
    let overrides = build_file_overrides(src)?;

    // 3. Copy every user table.
    let tables: Vec<String> = {
        let mut st = src.prepare(
            "SELECT name FROM sqlite_master \
             WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY rowid",
        )?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<std::result::Result<_, _>>()?
    };

    let mut report = Report {
        tables: tables.len(),
        files_rekeyed: overrides.sha_by_file.len(),
        ..Report::default()
    };
    dst.execute_batch("BEGIN;")?;
    for t in &tables {
        report.rows += copy_table(src, dst, t, &overrides)?;
    }
    dst.execute_batch("COMMIT;")?;
    Ok(report)
}

/// Scrubbed document-store bytes + recomputed content-address, keyed by fileId.
#[derive(Default)]
struct FileOverrides {
    /// fileId → scrubbed text content (for `doc_mount_documents`).
    content_by_file: HashMap<String, String>,
    /// fileId → scrubbed binary data (for `doc_mount_blobs`).
    data_by_file: HashMap<String, Vec<u8>>,
    /// fileId → (recomputed sha256 hex, byte size) for the parent file row.
    sha_by_file: HashMap<String, (String, i64)>,
}

fn build_file_overrides(src: &Connection) -> Result<FileOverrides> {
    let mut ov = FileOverrides::default();

    if table_has_column(src, "doc_mount_documents", "content")? {
        let mut st = src.prepare("SELECT fileId, content FROM doc_mount_documents")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (file_id, content) = row?;
            let content = content.unwrap_or_default();
            let scrubbed = scrub_text_value("content", &content);
            let sha = sha256_of_string(&scrubbed);
            let size = scrubbed.len() as i64;
            ov.sha_by_file.insert(file_id.clone(), (sha, size));
            ov.content_by_file.insert(file_id, scrubbed);
        }
    }

    if table_has_column(src, "doc_mount_blobs", "data")? {
        let mut st = src.prepare("SELECT fileId, data FROM doc_mount_blobs")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))
        })?;
        for row in rows {
            let (file_id, data) = row?;
            let data = data.unwrap_or_default();
            let scrubbed = scrub_blob("data", &data);
            let sha = hex_sha256(&scrubbed);
            let size = scrubbed.len() as i64;
            ov.sha_by_file.insert(file_id.clone(), (sha, size));
            ov.data_by_file.insert(file_id, scrubbed);
        }
    }

    Ok(ov)
}

fn copy_table(
    src: &Connection,
    dst: &Connection,
    table: &str,
    ov: &FileOverrides,
) -> Result<usize> {
    let cols = column_names(src, table)?;
    if cols.is_empty() {
        return Ok(0);
    }
    let col_list = cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = cols.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let insert_sql = format!("INSERT INTO \"{table}\" ({col_list}) VALUES ({placeholders})");
    let mut ins = dst.prepare(&insert_sql)?;

    let key_col = key_column_for(table, &cols);

    let mut select = src.prepare(&format!("SELECT * FROM \"{table}\""))?;
    let ncols = cols.len();
    let mut rows = select.query([])?;
    let mut n = 0usize;
    while let Some(row) = rows.next()? {
        // Read the row as owned SQLite values.
        let raw: Vec<SqlValue> = (0..ncols)
            .map(|i| row.get::<usize, SqlValue>(i))
            .collect::<std::result::Result<_, _>>()?;

        // The row's key value (fileId / id) for document-store overrides.
        let key_val = key_col
            .and_then(|kc| cols.iter().position(|c| c == kc))
            .and_then(|i| match &raw[i] {
                SqlValue::Text(s) => Some(s.clone()),
                _ => None,
            });

        let sanitized: Vec<SqlValue> = cols
            .iter()
            .zip(raw)
            .map(|(col, val)| sanitize_cell(table, col, val, key_val.as_deref(), ov))
            .collect();

        ins.execute(rusqlite::params_from_iter(sanitized.iter()))?;
        n += 1;
    }
    Ok(n)
}

/// Which column holds the document-store key for a table (so overrides can be
/// looked up): `fileId` for the byte tables, `id` for `doc_mount_files`.
fn key_column_for<'a>(table: &str, cols: &'a [String]) -> Option<&'a str> {
    let want = match table {
        "doc_mount_documents" | "doc_mount_blobs" => "fileId",
        "doc_mount_files" => "id",
        _ => return None,
    };
    cols.iter().find(|c| c.as_str() == want).map(|c| c.as_str())
}

/// Sanitize one cell, applying the document-store overrides where relevant.
fn sanitize_cell(
    table: &str,
    col: &str,
    val: SqlValue,
    key_val: Option<&str>,
    ov: &FileOverrides,
) -> SqlValue {
    // Document store: patch content/data + the recomputed sha/size, so the
    // content-addressed invariant holds after scrubbing.
    match (table, col) {
        ("doc_mount_documents", "content") => {
            if let Some(k) = key_val {
                if let Some(c) = ov.content_by_file.get(k) {
                    return SqlValue::Text(c.clone());
                }
            }
        }
        ("doc_mount_documents", "plainTextLength") => {
            if let Some(k) = key_val {
                if let Some(c) = ov.content_by_file.get(k) {
                    return SqlValue::Integer(c.chars().count() as i64);
                }
            }
        }
        ("doc_mount_blobs", "data") => {
            if let Some(k) = key_val {
                if let Some(d) = ov.data_by_file.get(k) {
                    return SqlValue::Blob(d.clone());
                }
            }
        }
        ("doc_mount_blobs", "sha256") => {
            if let Some((sha, _)) = key_val.and_then(|k| ov.sha_by_file.get(k)) {
                return SqlValue::Text(sha.clone());
            }
        }
        ("doc_mount_blobs", "sizeBytes") => {
            if let Some((_, size)) = key_val.and_then(|k| ov.sha_by_file.get(k)) {
                return SqlValue::Integer(*size);
            }
        }
        ("doc_mount_files", "sha256") => {
            if let Some((sha, _)) = key_val.and_then(|k| ov.sha_by_file.get(k)) {
                return SqlValue::Text(sha.clone());
            }
        }
        ("doc_mount_files", "fileSizeBytes") => {
            if let Some((_, size)) = key_val.and_then(|k| ov.sha_by_file.get(k)) {
                return SqlValue::Integer(*size);
            }
        }
        _ => {}
    }

    // Generic per-column scrub.
    match val {
        SqlValue::Null => SqlValue::Null,
        SqlValue::Integer(i) => SqlValue::Integer(i), // numbers/flags preserved
        SqlValue::Real(f) => SqlValue::Real(f),
        SqlValue::Blob(b) => SqlValue::Blob(scrub_blob(col, &b)),
        SqlValue::Text(s) => SqlValue::Text(scrub_text_value(col, &s)),
    }
}

// ---- classification -------------------------------------------------------

fn is_id_col(col: &str) -> bool {
    col == "id"
        || col.ends_with("Id")
        || col.ends_with("Ids")
        || col.ends_with("_id")
        || col.ends_with("_ids")
}

fn is_ts_col(col: &str) -> bool {
    col.ends_with("At") || col.ends_with("_at") || col == "timestamp"
}

/// Conservative enum/control-column allowlist (by exact name) + structural
/// suffixes. Kept readable by the ported repos; extended empirically.
fn is_enum_col(col: &str) -> bool {
    const NAMES: &[&str] = &[
        "type",
        "role",
        "status",
        "controlledBy",
        "storeType",
        "source",
        "fileType",
        "kind",
        "provider",
        "systemKind",
        "systemSender",
        "recoveryType",
        "backgroundDisplayMode",
        "mode",
        "level",
        "environment",
        "category",
        "modelName",
        "modelHint",
        "model",
        "cipher",
        "algorithm",
        "kdfDigest",
        "digest",
        "scope",
        "visibility",
        "sourceType",
        "entityType",
        "jobType",
        "jobStatus",
        "swipeIndex",
        "provider_type",
    ];
    NAMES.contains(&col)
        || col.ends_with("Type")
        || col.ends_with("Status")
        || col.ends_with("Kind")
        || col.ends_with("Mode")
        || col.ends_with("Role")
}

/// A 36-char canonical UUID (`8-4-4-4-12` hex). Preserved as a ref.
fn looks_uuid(s: &str) -> bool {
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

/// An ISO-8601 timestamp like `2026-02-01T00:00:00.000Z`. Preserved.
fn looks_iso_datetime(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 20
        && s.ends_with('Z')
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[..4].iter().all(u8::is_ascii_digit)
}

fn preserve_text(col: &str, s: &str) -> bool {
    is_id_col(col) || is_ts_col(col) || is_enum_col(col) || looks_uuid(s) || looks_iso_datetime(s)
}

// ---- scrubbing ------------------------------------------------------------

/// Scrub a TEXT cell per the policy: preserve structural values, keep the
/// structural skeleton of document-store paths, deep-scrub JSON to keep it
/// valid, otherwise replace prose with same-length pseudo-text.
fn scrub_text_value(col: &str, s: &str) -> String {
    if preserve_text(col, s) {
        return s.to_string();
    }
    // Document-store paths embed private titles in their variable segments, but
    // the fixed segments (folder names + the managed vault filenames) are
    // non-private structural constants the read overlay locates files by — keep
    // those so a sanitized vault still resolves (e.g. `properties.json`,
    // `Prompts/`), scrub only the title stems.
    if is_path_col(col) {
        return scrub_path(s);
    }
    if let Some(json) = try_parse_json(s) {
        let scrubbed = deep_scrub_json(json, col);
        return serde_json::to_string(&scrubbed).unwrap_or_else(|_| scrub_free_text(col, s));
    }
    scrub_free_text(col, s)
}

fn is_path_col(col: &str) -> bool {
    col == "path" || col == "relativePath" || col.ends_with("Path")
}

/// The managed vault filenames + folder names that are structural constants
/// (not private) and that the read overlay locates files by. Preserved verbatim
/// as path segments; every other segment has its stem scrubbed (extension kept).
const STRUCTURAL_PATH_SEGMENTS: &[&str] = &[
    "properties.json",
    "description.md",
    "manifesto.md",
    "personality.md",
    "identity.md",
    "example-dialogues.md",
    "physical-description.md",
    "physical-prompts.json",
    "instructions.md",
    "state.json",
    "wardrobe.json",
    "Prompts",
    "Scenarios",
    "Wardrobe",
];

/// Scrub a `/`-separated path: keep empty segments (leading `/`) and structural
/// constants verbatim; for a variable segment, scrub the stem but keep the
/// extension (`Reginald.md` → `<scrubbed>.md`).
fn scrub_path(s: &str) -> String {
    s.split('/')
        .map(|seg| {
            if seg.is_empty() || STRUCTURAL_PATH_SEGMENTS.contains(&seg) {
                seg.to_string()
            } else if let Some(dot) = seg.rfind('.') {
                let (stem, ext) = seg.split_at(dot);
                format!("{}{}", scrub_free_text("path_segment", stem), ext)
            } else {
                scrub_free_text("path_segment", seg)
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Only treat a string as JSON when it is an object/array (not a bare
/// number/string/bool that `serde_json` would also accept).
fn try_parse_json(s: &str) -> Option<Value> {
    let t = s.trim_start();
    if !(t.starts_with('{') || t.starts_with('[')) {
        return None;
    }
    serde_json::from_str::<Value>(s).ok()
}

fn deep_scrub_json(v: Value, key: &str) -> Value {
    match v {
        Value::String(s) => {
            if preserve_text(key, &s) {
                Value::String(s)
            } else {
                Value::String(scrub_free_text(key, &s))
            }
        }
        Value::Array(a) => Value::Array(a.into_iter().map(|x| deep_scrub_json(x, key)).collect()),
        Value::Object(m) => {
            let mut o = Map::new();
            for (k, val) in m {
                let scrubbed = deep_scrub_json(val, &k);
                o.insert(k, scrubbed);
            }
            Value::Object(o)
        }
        other => other, // numbers, bools, null preserved
    }
}

/// Deterministic, one-way, length-preserving pseudo-text. Keyed on
/// `SHA-256(col ‖ 0 ‖ original)`; identical originals map identically (keeps
/// content-dedup relationships), the original never appears in the output.
fn scrub_free_text(col: &str, s: &str) -> String {
    let target = s.chars().count();
    if target == 0 {
        return String::new();
    }
    let mut h = Sha256::new();
    h.update(col.as_bytes());
    h.update([0u8]);
    h.update(s.as_bytes());
    let seed = h.finalize();

    const ALPHA: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";
    let mut out = String::with_capacity(target);
    let mut i = 0usize;
    while out.chars().count() < target {
        // A space every 7th position (never leading) keeps it from being one
        // giant token; harmless for marshaling.
        if i % 7 == 6 {
            out.push(' ');
        } else {
            let b = seed[i % seed.len()] ^ (i as u8);
            out.push(ALPHA[(b as usize) % 26] as char);
        }
        i += 1;
    }
    out.chars().take(target).collect()
}

/// Deterministic same-length replacement bytes for a BLOB (SHA-256 counter
/// mode keyed on the column + original).
fn scrub_blob(col: &str, b: &[u8]) -> Vec<u8> {
    if b.is_empty() {
        return Vec::new();
    }
    let mut seed = Sha256::new();
    seed.update(col.as_bytes());
    seed.update([1u8]);
    seed.update(b);
    let seed = seed.finalize();

    let mut out = Vec::with_capacity(b.len());
    let mut ctr = 0u64;
    while out.len() < b.len() {
        let mut h = Sha256::new();
        h.update(seed);
        h.update(ctr.to_le_bytes());
        out.extend_from_slice(&h.finalize());
        ctr += 1;
    }
    out.truncate(b.len());
    out
}

fn hex_sha256(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for byte in d {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

// ---- schema introspection -------------------------------------------------

fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut st = conn.prepare("SELECT name FROM pragma_table_info(?1)")?;
    let rows = st.query_map([table], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn table_has_column(conn: &Connection, table: &str, col: &str) -> Result<bool> {
    Ok(column_names(conn, table)?.iter().any(|c| c == col))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // A second valid 32-byte pepper (base64 of bytes 0x00..0x1F) so the test
    // proves a genuine re-key from key A (source) to key B (dest).
    const SRC_PEPPER: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

    fn cell_text(conn: &Connection, sql: &str) -> Option<String> {
        conn.query_row(sql, [], |r| r.get::<_, Option<String>>(0))
            .unwrap()
    }

    #[test]
    fn round_trips_a_synthetic_snapshot() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("src.db");
        let dst_path = dir.path().join("dst.db");

        // Build a synthetic source under SRC_PEPPER: a plain table + a mini
        // document store whose sha matches its content.
        let content = "The quick brown fox jumps over the lazy dog. Secret name: Aurora.";
        let sha = sha256_of_string(content);
        {
            let src = open_write_fresh(&src_path, SRC_PEPPER).unwrap();
            src.execute_batch(
                "CREATE TABLE things (id TEXT, name TEXT, count INTEGER, meta TEXT, \
                   controlledBy TEXT, createdAt TEXT, blobCol BLOB);
                 CREATE TABLE doc_mount_files (id TEXT, sha256 TEXT, fileSizeBytes INTEGER);
                 CREATE TABLE doc_mount_documents (fileId TEXT, content TEXT, plainTextLength INTEGER);",
            )
            .unwrap();
            src.execute(
                "INSERT INTO things VALUES (?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![
                    "11111111-2222-4333-8444-555555555555",
                    "Reginald Featherstonehaugh",
                    42i64,
                    r#"{"title":"My Private Title","controlledBy":"user","n":7}"#,
                    "user",
                    "2026-02-01T00:00:00.000Z",
                    vec![9u8, 8, 7, 6, 5, 4, 3, 2, 1, 0],
                ],
            )
            .unwrap();
            src.execute(
                "INSERT INTO doc_mount_files VALUES (?1,?2,?3)",
                rusqlite::params!["file-1", sha, content.len() as i64],
            )
            .unwrap();
            src.execute(
                "INSERT INTO doc_mount_documents VALUES (?1,?2,?3)",
                rusqlite::params!["file-1", content, content.chars().count() as i64],
            )
            .unwrap();
        }

        // Sanitize into a fresh dest under the TEST pepper (re-key A→B).
        {
            let src = open_read(&src_path, SRC_PEPPER).unwrap();
            let dst = open_write_fresh(&dst_path, TEST_PEPPER_B64).unwrap();
            let report = sanitize_db(&src, &dst).unwrap();
            assert_eq!(report.tables, 3);
            assert_eq!(report.rows, 3);
            assert_eq!(report.files_rekeyed, 1);
        }

        // Re-open the dest under the TEST pepper and assert the policy held.
        let dst = open_read(&dst_path, TEST_PEPPER_B64).unwrap();

        // Structure preserved.
        assert_eq!(
            cell_text(&dst, "SELECT id FROM things").as_deref(),
            Some("11111111-2222-4333-8444-555555555555"),
            "uuid id preserved"
        );
        assert_eq!(
            cell_text(&dst, "SELECT controlledBy FROM things").as_deref(),
            Some("user"),
            "enum column preserved"
        );
        assert_eq!(
            cell_text(&dst, "SELECT createdAt FROM things").as_deref(),
            Some("2026-02-01T00:00:00.000Z"),
            "timestamp preserved"
        );
        let count: i64 = dst
            .query_row("SELECT count FROM things", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 42, "integer preserved");

        // Free text scrubbed (changed) but same char length.
        let name = cell_text(&dst, "SELECT name FROM things").unwrap();
        assert_ne!(name, "Reginald Featherstonehaugh", "name scrubbed");
        assert_eq!(
            name.chars().count(),
            "Reginald Featherstonehaugh".chars().count(),
            "name length preserved"
        );
        assert!(!name.contains("Reginald"));

        // JSON column: still valid JSON, keys preserved, enum leaf preserved,
        // number preserved, free-text leaf scrubbed.
        let meta = cell_text(&dst, "SELECT meta FROM things").unwrap();
        let mj: Value = serde_json::from_str(&meta).expect("meta still valid JSON");
        assert_eq!(mj["controlledBy"], serde_json::json!("user"));
        assert_eq!(mj["n"], serde_json::json!(7));
        assert_ne!(mj["title"], serde_json::json!("My Private Title"));

        // BLOB scrubbed but same length.
        let blob: Vec<u8> = dst
            .query_row("SELECT blobCol FROM things", [], |r| r.get(0))
            .unwrap();
        assert_eq!(blob.len(), 10, "blob length preserved");
        assert_ne!(blob, vec![9u8, 8, 7, 6, 5, 4, 3, 2, 1, 0], "blob scrubbed");

        // Document store content↔sha recomputed and consistent.
        let new_content = cell_text(&dst, "SELECT content FROM doc_mount_documents").unwrap();
        assert!(!new_content.contains("Aurora"), "document content scrubbed");
        let stored_sha = cell_text(&dst, "SELECT sha256 FROM doc_mount_files").unwrap();
        assert_eq!(
            stored_sha,
            sha256_of_string(&new_content),
            "files.sha256 matches the SCRUBBED content (invariant recomputed)"
        );
        let stored_size: i64 = dst
            .query_row("SELECT fileSizeBytes FROM doc_mount_files", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stored_size, new_content.len() as i64, "size recomputed");
    }

    #[test]
    fn scrub_is_deterministic_and_equality_preserving() {
        assert_eq!(
            scrub_free_text("name", "hello"),
            scrub_free_text("name", "hello")
        );
        // Different column salt → different output for the same input.
        assert_ne!(
            scrub_free_text("name", "hello"),
            scrub_free_text("title", "hello")
        );
    }

    #[test]
    fn classification_rules() {
        assert!(is_id_col("id") && is_id_col("characterId") && is_id_col("related_ids"));
        assert!(is_ts_col("createdAt") && is_ts_col("last_seen_at"));
        assert!(
            is_enum_col("controlledBy") && is_enum_col("jobStatus") && is_enum_col("displayMode")
        );
        assert!(looks_uuid("11111111-2222-4333-8444-555555555555"));
        assert!(!looks_uuid("not-a-uuid"));
        assert!(looks_iso_datetime("2026-02-01T00:00:00.000Z"));
    }

    #[test]
    fn path_scrub_keeps_structural_segments() {
        // A managed root file is a structural constant → preserved verbatim, so
        // the read overlay can still locate it.
        assert_eq!(scrub_path("properties.json"), "properties.json");
        assert_eq!(scrub_path("description.md"), "description.md");
        // A title-bearing file under a structural folder: folder + extension
        // kept, only the stem scrubbed.
        let p = scrub_path("Prompts/Reginald.md");
        assert!(p.starts_with("Prompts/") && p.ends_with(".md"));
        assert!(!p.contains("Reginald"));
        assert_ne!(p, "Prompts/Reginald.md");
        // Leading slash (a folder path) keeps its empty root segment + structure.
        let f = scrub_path("/SecretFolder");
        assert!(f.starts_with('/') && !f.contains("SecretFolder"));
    }
}
