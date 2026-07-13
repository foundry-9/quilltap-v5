//! Shared plumbing for the P4.6y mount-family tier-2 differentials
//! (`mount_index_equivalence` + `mount_ops_equivalence` + later suites): the
//! per-case fresh-fixture instance, the RAW eight-table dump (column lists
//! identical to the jest oracles\'), and the ONE normalization algorithm
//! applied to both sides (timestamps/error-text/abs-path blanking, the
//! canonical minted-UUID remap, job-row re-sort by remapped payload, 1e-6
//! float rounding).

#![allow(dead_code)]

use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use serde_json::{json, Map, Value};

pub const MP_DB: &str = "b6000000-0000-4000-8000-000000000001";
pub const MP_EMPTY: &str = "b6000000-0000-4000-8000-000000000002";
pub const MP_FS: &str = "b6000000-0000-4000-8000-000000000003";
pub const MP_OBS: &str = "b6000000-0000-4000-8000-000000000004";
pub const BOGUS: &str = "b6000000-0000-4000-8000-0000000000ee";
pub const SINGLE_USER_ID: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";

pub fn test_pepper() -> String {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/mounts.json");
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).expect("spec");
    spec["testPepperBase64"].as_str().unwrap().to_string()
}

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

pub fn load_oracle(env_var: &str) -> Option<std::collections::HashMap<String, Value>> {
    let path = match std::env::var(env_var) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set {env_var} to the oracle NDJSON (see test header).");
            return None;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut oracle = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        oracle.insert(row["name"].as_str().unwrap().to_string(), row);
    }
    Some(oracle)
}

pub fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).unwrap();
        }
    }
}

/// A per-case fresh instance: fixture copies + per-case fs-tree copy + the
/// sentinel basePath rewrite (the mount-read recipe).
pub fn fresh_db(pepper: &str, tag: &str) -> (Db, PathBuf) {
    let scratch = std::env::temp_dir().join(format!("qt-mindex-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("mounts-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("mounts-mount.db"), &mount).unwrap();
    let tree = scratch.join("fs-tree");
    copy_dir(&fixtures_dir().join("mounts-fs-tree"), &tree);
    {
        let w = Writer::open_writable(&mount, pepper).expect("open mount writer");
        w.connection()
            .execute(
                "UPDATE doc_mount_points SET basePath = ?1 WHERE id IN (?2, ?3)",
                rusqlite::params![tree.to_str().unwrap(), MP_FS, MP_OBS],
            )
            .expect("rewrite basePath");
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open db");
    (db, scratch)
}

// ─── raw dumps (column lists identical to the oracle's) ─────────────────────

pub const MOUNT_TABLES: &[(&str, &str)] = &[
    (
        "points",
        "SELECT id, name, basePath, mountType, storeType, includePatterns, excludePatterns, enabled, \
         lastScannedAt, scanStatus, lastScanError, conversionStatus, conversionError, fileCount, \
         chunkCount, totalSizeBytes, createdAt, updatedAt FROM doc_mount_points ORDER BY id",
    ),
    (
        "files",
        "SELECT id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt \
         FROM doc_mount_files ORDER BY sha256, source",
    ),
    (
        "links",
        "SELECT id, fileId, mountPointId, relativePath, fileName, folderId, originalFileName, \
         originalMimeType, description, descriptionUpdatedAt, conversionStatus, conversionError, \
         plainTextLength, extractedText, extractedTextSha256, extractionStatus, extractionError, \
         chunkCount, allowEmbed, allowCharacterRead, allowCharacterWrite, lastModified, createdAt, \
         updatedAt FROM doc_mount_file_links ORDER BY mountPointId, relativePath",
    ),
    (
        "documents",
        "SELECT id, fileId, content, contentSha256, plainTextLength, createdAt, updatedAt \
         FROM doc_mount_documents ORDER BY contentSha256",
    ),
    (
        "folders",
        "SELECT id, mountPointId, parentId, name, path, createdAt, updatedAt \
         FROM doc_mount_folders ORDER BY mountPointId, path",
    ),
    (
        "chunks",
        "SELECT c.id, c.linkId, c.mountPointId, c.chunkIndex, c.content, c.tokenCount, \
         c.headingContext, (CASE WHEN c.embedding IS NULL THEN NULL ELSE length(c.embedding) END) \
         AS embeddingLength, c.createdAt, c.updatedAt, l.relativePath AS sortPath \
         FROM doc_mount_chunks c LEFT JOIN doc_mount_file_links l ON l.id = c.linkId \
         ORDER BY c.mountPointId, sortPath, c.chunkIndex",
    ),
    (
        "blobs",
        "SELECT id, fileId, sha256, sizeBytes, storedMimeType, length(data) AS dataLength, \
         createdAt, updatedAt FROM doc_mount_blobs ORDER BY sha256",
    ),
];

pub const JOBS_SQL: &str =
    "SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, \
     scheduledAt, startedAt, completedAt, createdAt, updatedAt FROM background_jobs ORDER BY payload";

pub fn rows_to_json(conn: &rusqlite::Connection, sql: &str) -> Vec<Value> {
    let mut stmt = conn.prepare(sql).unwrap();
    let names: Vec<String> = stmt
        .column_names()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let rows = stmt
        .query_map([], |row| {
            let mut obj = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Real(f) => json!(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        Value::String(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(b) => json!(b.len()),
                };
                obj.insert(name.clone(), v);
            }
            Ok(Value::Object(obj))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    rows
}

pub fn dump_tables(db: &Db) -> Value {
    let mut out = Map::new();
    for (name, sql) in MOUNT_TABLES {
        let sql = sql.to_string();
        let rows = db.read_mount_index(move |conn| Ok(rows_to_json(conn, &sql)));
        out.insert((*name).to_string(), Value::Array(rows.unwrap()));
    }
    let jobs = db
        .read_main(|conn| Ok(rows_to_json(conn, JOBS_SQL)))
        .unwrap();
    out.insert("jobs".to_string(), Value::Array(jobs));
    Value::Object(out)
}

// ─── the shared normalization (applied to BOTH sides) ───────────────────────

pub fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.as_bytes().iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

pub const TS_KEYS: &[&str] = &[
    "createdAt",
    "updatedAt",
    "lastScannedAt",
    "descriptionUpdatedAt",
    "lastModified",
    "scheduledAt",
    "startedAt",
    "completedAt",
];

/// Collapse ints stored as floats; round floats (none expected here).
pub fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

/// Normalize one case's `{status, body, tables}`:
///  1. blank timestamp-class values,
///  2. collapse extraction/conversion error TEXT to a presence marker (the v5
///     refusing extractor's message differs from v4 pdf-parse's),
///  3. remap every UUID string via first-seen order over a canonical traversal
///     (tables in fixed order, rows in their SQL sort, keys in a fixed order,
///     then the body, then job payloads),
///  4. parse job `payload` JSON strings (v4 and v5 may serialize key order
///     differently is NOT expected — both preserve insertion order — but the
///     entityId inside must ride the remap).
pub fn normalize(case: &Value) -> Value {
    let mut v = case.clone();
    canon_numbers(&mut v);

    // 1+2: walk everything: blank timestamps + error text.
    fn scrub(v: &mut Value) {
        match v {
            Value::Object(o) => {
                for (k, val) in o.iter_mut() {
                    if TS_KEYS.contains(&k.as_str()) {
                        if let Value::String(_) = val {
                            *val = Value::String("<ts>".to_string());
                        }
                    } else if k == "mtime" && val.is_number() {
                        // Write bodies mint a wall-clock epoch-ms mtime.
                        *val = Value::String("<ts-ms>".to_string());
                    } else if (k == "extractionError" || k == "conversionError" || k == "lastError")
                        && val.is_string()
                    {
                        *val = Value::String("<err>".to_string());
                    } else if k == "basePath"
                        && val.as_str().map(|s| s.starts_with('/')).unwrap_or(false)
                    {
                        // The per-side temp fs-tree rewrite (the sentinel recipe).
                        *val = Value::String("<abs>".to_string());
                    } else {
                        scrub(val);
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(scrub),
            _ => {}
        }
    }
    scrub(&mut v);

    // Parse job payload strings so their UUIDs join the remap.
    if let Some(jobs) = v
        .get_mut("tables")
        .and_then(|t| t.get_mut("jobs"))
        .and_then(|j| j.as_array_mut())
    {
        for job in jobs {
            if let Some(p) = job.get("payload").and_then(|p| p.as_str()) {
                if let Ok(parsed) = serde_json::from_str::<Value>(p) {
                    job["payload"] = parsed;
                }
            }
            // Job ids are minted in enqueue order (differs per side) and
            // nothing references them — blank rather than remap so token
            // numbering stays deterministic.
            if job.get("id").is_some() {
                job["id"] = Value::String("<jobid>".to_string());
            }
        }
    }

    // 3: the UUID remap. Canonical traversal: tables in dump order (serde_json
    // preserves insertion), rows in order, object keys in a FIXED priority so
    // both sides assign identical tokens.
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    fn assign(map: &mut std::collections::HashMap<String, String>, s: &str) -> String {
        if let Some(t) = map.get(s) {
            return t.clone();
        }
        let t = format!("<u{:03}>", map.len());
        map.insert(s.to_string(), t.clone());
        t
    }
    fn remap(v: &mut Value, map: &mut std::collections::HashMap<String, String>) {
        match v {
            Value::String(s) => {
                if is_uuid(s) {
                    *v = Value::String(assign(map, s));
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| remap(x, map)),
            Value::Object(o) => {
                // Fixed key-priority pass so token numbering is deterministic:
                // identity keys first, then the rest in sorted-key order.
                const PRIORITY: &[&str] = &[
                    "id",
                    "fileId",
                    "linkId",
                    "mountPointId",
                    "folderId",
                    "parentId",
                    "userId",
                    "profileId",
                    "entityId",
                ];
                for k in PRIORITY {
                    if let Some(val) = o.get_mut(*k) {
                        remap(val, map);
                    }
                }
                let mut rest: Vec<String> = o
                    .keys()
                    .filter(|k| !PRIORITY.contains(&k.as_str()))
                    .cloned()
                    .collect();
                rest.sort();
                for k in rest {
                    if let Some(val) = o.get_mut(&k) {
                        remap(val, map);
                    }
                }
            }
            _ => {}
        }
    }
    // Tables first (their SQL orderings are the canonical spine), then body.
    if let Some(tables) = v.get_mut("tables") {
        remap(tables, &mut map);
    }
    if let Some(body) = v.get_mut("body") {
        remap(body, &mut map);
    }

    // Embed bodies carry minted job ids ({id, kind, status} rows) — blank them
    // (nothing references a job id).
    if let Some(jobs) = v
        .get_mut("body")
        .and_then(|b| b.get_mut("jobs"))
        .and_then(|j| j.as_array_mut())
    {
        for job in jobs {
            if job.get("id").is_some() {
                job["id"] = Value::String("<jobid>".to_string());
            }
        }
    }

    // Search scores ride the f32-storage + ln-ULP seams — round every float to
    // 6 decimals (the P4.6s recipe); integer-valued floats already collapsed.
    fn round_floats(v: &mut Value) {
        match v {
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    if f.fract() != 0.0 {
                        let r = (f * 1_000_000.0).round() / 1_000_000.0;
                        if let Some(num) = serde_json::Number::from_f64(r) {
                            *v = Value::Number(num);
                        }
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(round_floats),
            Value::Object(o) => o.iter_mut().for_each(|(_, x)| round_floats(x)),
            _ => {}
        }
    }
    round_floats(&mut v);

    // Job rows were SQL-sorted by their RAW payload, which embeds per-side
    // minted chunk uuids — re-sort by the REMAPPED payload so rows align.
    if let Some(jobs) = v
        .get_mut("tables")
        .and_then(|t| t.get_mut("jobs"))
        .and_then(|j| j.as_array_mut())
    {
        jobs.sort_by_key(|job| {
            let ty = job.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let payload = job
                .get("payload")
                .map(|p| serde_json::to_string(&sorted(p)).unwrap_or_default())
                .unwrap_or_default();
            format!("{ty}\u{0}{payload}")
        });
    }

    v
}

pub fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

pub fn norm(case: &Value) -> String {
    serde_json::to_string_pretty(&sorted(&normalize(case))).unwrap()
}

pub fn diff_first_line(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  line {}\n  GOT : {gi}\n  WANT: {wi}", i + 1);
        }
    }
    "(identical)".to_string()
}

// ─── the response → (status, body) mapping (v4 envelope semantics) ──────────

pub fn status_of_kind(kind: &ErrorKind) -> i64 {
    match kind {
        ErrorKind::NotFound => 404,
        ErrorKind::BadRequest => 400,
        ErrorKind::Conflict => 409,
        ErrorKind::Forbidden => 403,
        _ => 500,
    }
}

pub fn response_to_status_body(r: Response) -> (i64, Value) {
    match r {
        Response::MountFile(v) => (200, v),
        Response::Error(e) => {
            let status = status_of_kind(&e.kind);
            // v4's error bodies: `{error}` plain, or `{error, code}` for the
            // typed file-op failures (CoreError.code, P4.6y).
            let mut body = Map::new();
            body.insert("error".to_string(), Value::String(e.message));
            if let Some(code) = e.code {
                body.insert("code".to_string(), Value::String(code));
            }
            (status, Value::Object(body))
        }
        other => panic!("unexpected response shape: {other:?}"),
    }
}
