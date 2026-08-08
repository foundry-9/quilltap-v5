//! P4.6ae FILES route-surface differential: `api::files::*` vs v4's REAL
//! files-family route handlers. Both sides read a FRESH copy of the committed
//! files fixture per case (baked ids identical → no remap except folder-create,
//! which mints ids — those are canonicalized by path). Success cases diff the
//! `data` body (+ a post-mutation `files`/`folders` table dump); error cases diff
//! status + message.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-files-routes.ndjson npx jest -- files-routes
//! Run:
//!   QT_ORACLE_FILES_ROUTES=/tmp/oracle-files-routes.ndjson \
//!     cargo test -p quilltap-harness --test files_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::chat_media::{self, ChatFileUploadInput};
use quilltap_core::api::files::{self, MovePid};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Map, Value};

const USER_A: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
const PROJECT: &str = "90000000-0000-4000-8000-000000000001";
const MISSING_PROJECT: &str = "90000000-0000-4000-8000-0000000000ff";
const F_ROOT: &str = "f0000000-0000-4000-8000-000000000001";
const F_DOCS: &str = "f0000000-0000-4000-8000-000000000002";
const F_UNLINKED: &str = "f0000000-0000-4000-8000-000000000021";
const F_STALE: &str = "f0000000-0000-4000-8000-000000000022";
const PF_1: &str = "f0000000-0000-4000-8000-000000000011";
const F_USERB: &str = "f0000000-0000-4000-8000-0000000000b1";
// P4.6ah
const F_ASSOC: &str = "f0000000-0000-4000-8000-000000000031";
const F_IMG1: &str = "f0000000-0000-4000-8000-000000000061";
const F_IMG2: &str = "f0000000-0000-4000-8000-000000000062";
const F_IMG_USERB: &str = "f0000000-0000-4000-8000-0000000000b2";
const CHAT_G: &str = "c0000000-0000-4000-8000-000000000001";
const CHAT_P: &str = "c0000000-0000-4000-8000-000000000002";
const PF_DUP: &str = "f0000000-0000-4000-8000-000000000041";

fn b64(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

/// A backend whose disk-key existence check ERRORS — matching v4's real
/// `LocalFileStorageBackend` over the oracle's absent `files/` dir (its `exists`
/// throws "Unknown existence check error", so the disk-key file lands in v4's
/// per-file `errors`, NOT `stale`). The exact message is the enumerated fs leg —
/// the differential strips the `errors` array and compares `stale`/`staleFiles`.
/// The dispatch engine passes `NotConfigured` (an enumerated production
/// degradation for legacy disk keys).
struct DiskErrorBackend;
impl quilltap_core::services::file_storage::StorageBackend for DiskErrorBackend {
    fn upload(&self, _k: &str, _c: &[u8], _t: &str) -> Result<(), String> {
        Ok(())
    }
    fn download(&self, _k: &str) -> Result<Vec<u8>, String> {
        Err("missing".into())
    }
    fn delete(&self, _k: &str) -> Result<(), String> {
        Ok(())
    }
    fn exists(&self, _k: &str) -> Result<bool, String> {
        Err("Unknown existence check error".into())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    #[allow(dead_code)]
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/files-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// Integer-valued floats → integers (JS-number parity).
fn canon_numbers(v: &mut Value) {
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
fn sorted(v: &Value) -> Value {
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
fn norm(v: &Value) -> Value {
    let mut c = v.clone();
    canon_numbers(&mut c);
    sorted(&c)
}
/// Replace the named keys (wherever they appear) with a placeholder — minted ids /
/// timestamps whose exact value legitimately differs between the two sides.
fn blank_keys(v: &mut Value, keys: &[&str]) {
    if let Value::Object(o) = v {
        for k in keys {
            if o.contains_key(*k) {
                o.insert((*k).to_string(), Value::String(format!("<{k}>")));
            }
        }
        o.iter_mut().for_each(|(_, x)| blank_keys(x, keys));
    } else if let Value::Array(a) = v {
        a.iter_mut().for_each(|x| blank_keys(x, keys));
    }
}

/// Canonicalize a folders table dump: rewrite `id`→`path` and
/// `parentFolderId`→(referenced folder's path, or null), then sort by path — so
/// minted folder ids (parent-chain create) compare structurally.
fn canon_folders_dump(rows: &Value) -> Value {
    let arr = rows.as_array().cloned().unwrap_or_default();
    let id_to_path: HashMap<String, String> = arr
        .iter()
        .filter_map(|r| {
            Some((
                r.get("id")?.as_str()?.to_string(),
                r.get("path")?.as_str()?.to_string(),
            ))
        })
        .collect();
    let mut out: Vec<Value> = arr
        .iter()
        .map(|r| {
            let parent = r
                .get("parentFolderId")
                .and_then(Value::as_str)
                .and_then(|id| id_to_path.get(id))
                .cloned();
            json!({
                "path": r.get("path").cloned().unwrap_or(Value::Null),
                "name": r.get("name").cloned().unwrap_or(Value::Null),
                "parentPath": parent.map(Value::String).unwrap_or(Value::Null),
                "projectId": r.get("projectId").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.get("path")
            .and_then(Value::as_str)
            .cmp(&b.get("path").and_then(Value::as_str))
    });
    Value::Array(out)
}

/// Normalize a `{files, folders}` table dump for comparison.
fn norm_tables(t: &Value) -> Value {
    json!({
        "files": norm(t.get("files").unwrap_or(&Value::Null)),
        "folders": norm(&canon_folders_dump(t.get("folders").unwrap_or(&Value::Null))),
    })
}

fn response_data(r: &Response) -> Value {
    serde_json::to_value(r)
        .unwrap()
        .get("data")
        .cloned()
        .unwrap_or(Value::Null)
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-files-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("files-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("files-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// Dump the files + folders tables in the oracle's `dumpTables` shape.
fn dump_tables(db: &Db) -> Value {
    let files = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, projectId, folderPath, linkedTo, fileStatus FROM files ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "projectId": r.get::<_, Option<String>>(1)?,
                    "folderPath": r.get::<_, Option<String>>(2)?,
                    "linkedTo": r.get::<_, Option<String>>(3)?,
                    "fileStatus": r.get::<_, Option<String>>(4)?,
                }))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    let folders = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, path, name, parentFolderId, projectId FROM folders ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "path": r.get::<_, String>(1)?,
                    "name": r.get::<_, String>(2)?,
                    "parentFolderId": r.get::<_, Option<String>>(3)?,
                    "projectId": r.get::<_, Option<String>>(4)?,
                }))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    json!({ "files": files, "folders": folders })
}

/// Dump characters (image refs) + chat_messages (attachments) + files for the
/// dissociate/force arms, normalizing the minted deletion-note timestamp in
/// message content (`… deleted <ts>]` → `… deleted <ts>]`).
fn dump_assoc(db: &Db) -> Value {
    let characters = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, defaultImageId, avatarOverrides FROM characters ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "defaultImageId": r.get::<_, Option<String>>(1)?,
                    "avatarOverrides": r.get::<_, Option<String>>(2)?,
                }))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    let messages = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, chatId, content, attachments FROM chat_messages ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "chatId": r.get::<_, String>(1)?,
                    "content": normalize_deletion_ts(&r.get::<_, String>(2)?),
                    "attachments": r.get::<_, Option<String>>(3)?,
                }))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    let files = db
        .read_main(|c| {
            let mut stmt = c.prepare("SELECT id FROM files ORDER BY id")?;
            let rows = stmt.query_map([], |r| Ok(json!({ "id": r.get::<_, String>(0)? })))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    json!({ "characters": characters, "messages": messages, "files": files })
}

/// Normalize an assoc dump's message-content timestamps (applied to the oracle
/// side, whose messages carry v4's minted clock).
fn normalize_assoc(v: &Value) -> Value {
    let mut out = v.clone();
    if let Some(msgs) = out.get_mut("messages").and_then(Value::as_array_mut) {
        for m in msgs {
            if let Some(c) = m.get("content").and_then(Value::as_str) {
                let n = normalize_deletion_ts(c);
                m["content"] = Value::String(n);
            }
        }
    }
    out
}

/// Compare a dissociate/force assoc dump to the oracle's `assoc` (normalizing the
/// minted deletion-note timestamp on both sides).
fn check_assoc(name: &str, got: &Value, oracle: &HashMap<String, Value>, failed: &mut Vec<String>) {
    let want = oracle[name].get("assoc").cloned().unwrap_or(Value::Null);
    if norm(&normalize_assoc(got)) == norm(&normalize_assoc(&want)) {
        eprintln!("[{name} assoc] OK.");
    } else {
        eprintln!(
            "[{name} assoc] MISMATCH:\n got {}\n want {}",
            norm(&normalize_assoc(got)),
            norm(&normalize_assoc(&want))
        );
        failed.push(name.to_string());
    }
}

/// Replace the ISO timestamp inside a `[Attachment "…" deleted <ts>]` note with a
/// fixed sentinel (both sides mint their own clock).
fn normalize_deletion_ts(content: &str) -> String {
    match content.find(" deleted ") {
        Some(i) => {
            let head = &content[..i + " deleted ".len()];
            // The rest is `<ISO>]` — keep the trailing `]`.
            match content[i..].find(']') {
                Some(_) => format!("{head}<ts>]"),
                None => content.to_string(),
            }
        }
        None => content.to_string(),
    }
}

#[test]
fn files_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_FILES_ROUTES") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    // A success-body check with optional minted-key blanking + optional table dump.
    let check_ok = |name: &str,
                    got_body: Value,
                    blank: &[&str],
                    tables: Option<Value>,
                    failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_u64().unwrap_or(0);
        if !(200..300).contains(&want_status) {
            eprintln!("[{name}] expected success, oracle status {want_status}");
            failed.push(name.to_string());
            return;
        }
        let mut got = got_body;
        let mut want = rec["body"].clone();
        blank_keys(&mut got, blank);
        blank_keys(&mut want, blank);
        let mut ok = norm(&got) == norm(&want);
        if let Some(t) = &tables {
            let wt = rec.get("tables").cloned().unwrap_or(Value::Null);
            if norm_tables(t) != norm_tables(&wt) {
                ok = false;
                eprintln!(
                    "[{name} tables] MISMATCH:\n got {}\n want {}",
                    norm_tables(t),
                    norm_tables(&wt)
                );
            }
        }
        if ok {
            eprintln!("[{name}] OK.");
        } else {
            eprintln!(
                "[{name}] MISMATCH:\n got {}\n want {}",
                norm(&got),
                norm(&want)
            );
            failed.push(name.to_string());
        }
    };

    // An error check: status + message (v4 `{error}` body).
    let check_err = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_u64().unwrap_or(0) as u16;
        match got {
            Response::Error(e) => {
                let want_msg = rec["body"]["error"].as_str().unwrap_or("");
                if status_of(e.kind) == want_status && e.message == want_msg {
                    eprintln!("[{name}] OK ({want_status}).");
                } else {
                    eprintln!(
                        "[{name}] MISMATCH: got {} {:?} / want {want_status} {want_msg:?}",
                        status_of(e.kind),
                        e.message
                    );
                    failed.push(name.to_string());
                }
            }
            other => {
                eprintln!("[{name}] expected error, got {:?}", response_data(other));
                failed.push(name.to_string());
            }
        }
    };

    // ── reads ──
    check_ok(
        "list_general",
        response_data(&files::files_list(
            &fresh_db(&spec, "lg"),
            USER_A,
            None,
            None,
            Some("general"),
        )),
        &[],
        None,
        &mut failed,
    );
    check_ok(
        "list_project",
        response_data(&files::files_list(
            &fresh_db(&spec, "lp"),
            USER_A,
            Some(PROJECT),
            None,
            None,
        )),
        &[],
        None,
        &mut failed,
    );
    check_ok(
        "list_folder",
        response_data(&files::files_list(
            &fresh_db(&spec, "lf"),
            USER_A,
            None,
            Some("/docs/"),
            None,
        )),
        &[],
        None,
        &mut failed,
    );
    check_ok(
        "folders_general",
        response_data(&files::files_folders_list(
            &fresh_db(&spec, "fg"),
            USER_A,
            None,
        )),
        &[],
        None,
        &mut failed,
    );
    check_ok(
        "folders_project",
        response_data(&files::files_folders_list(
            &fresh_db(&spec, "fp"),
            USER_A,
            Some(PROJECT),
        )),
        &[],
        None,
        &mut failed,
    );

    // ── move / promote (blank the minted updatedAt; dump the file rows) ──
    let move_case =
        |tag: &str, id: &str, folder: Option<String>, filename: Option<String>, pid: MovePid| {
            let db = fresh_db(&spec, tag);
            let resp = rt.block_on(files::file_move(&db, USER_A, id, folder, filename, pid));
            (response_data(&resp), dump_tables(&db))
        };
    {
        let (b, t) = move_case("mf", F_DOCS, Some("/moved/".into()), None, MovePid::Keep);
        check_ok("move_folder", b, &["updatedAt"], Some(t), &mut failed);
    }
    {
        let (b, t) = move_case(
            "mr",
            F_DOCS,
            None,
            Some("renamed.txt".into()),
            MovePid::Keep,
        );
        check_ok("move_filename", b, &["updatedAt"], Some(t), &mut failed);
    }
    {
        let (b, t) = move_case("mp", F_ROOT, None, None, MovePid::Set(PROJECT.into()));
        check_ok("move_to_project", b, &["updatedAt"], Some(t), &mut failed);
    }
    {
        let (b, t) = move_case("mg", PF_1, None, None, MovePid::Clear);
        check_ok("move_to_general", b, &["updatedAt"], Some(t), &mut failed);
    }
    check_err(
        "move_project_missing",
        &rt.block_on(files::file_move(
            &fresh_db(&spec, "mm"),
            USER_A,
            F_ROOT,
            None,
            None,
            MovePid::Set(MISSING_PROJECT.into()),
        )),
        &mut failed,
    );
    check_err(
        "move_none",
        &rt.block_on(files::file_move(
            &fresh_db(&spec, "mn"),
            USER_A,
            F_ROOT,
            None,
            None,
            MovePid::Keep,
        )),
        &mut failed,
    );
    {
        let db = fresh_db(&spec, "pp");
        let resp = rt.block_on(files::file_promote(
            &db,
            USER_A,
            F_ROOT,
            Some(PROJECT.into()),
            None,
        ));
        check_ok(
            "promote_to_project",
            response_data(&resp),
            &["updatedAt"],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "pg");
        let resp = rt.block_on(files::file_promote(&db, USER_A, PF_1, None, None));
        check_ok(
            "promote_to_general",
            response_data(&resp),
            &["updatedAt"],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }

    // ── delete ──
    {
        let db = fresh_db(&spec, "dh");
        let resp = rt.block_on(files::file_delete(
            &db, USER_A, F_UNLINKED, false, false, None,
        ));
        check_ok(
            "delete_happy",
            response_data(&resp),
            &[],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "ds");
        let resp = rt.block_on(files::file_delete(&db, USER_A, F_STALE, false, false, None));
        check_ok(
            "delete_stale",
            response_data(&resp),
            &[],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }
    check_err(
        "delete_forbidden",
        &rt.block_on(files::file_delete(
            &fresh_db(&spec, "df"),
            USER_A,
            F_USERB,
            false,
            false,
            None,
        )),
        &mut failed,
    );

    // ── folders ──
    {
        let db = fresh_db(&spec, "fcn");
        let resp = rt.block_on(files::files_folder_create(&db, USER_A, "/new/", None));
        check_ok(
            "folder_create_new",
            response_data(&resp),
            &["id", "createdAt", "updatedAt", "parentFolderId"],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "fci");
        let resp = rt.block_on(files::files_folder_create(&db, USER_A, "/docs/", None));
        check_ok(
            "folder_create_idempotent",
            response_data(&resp),
            &["id", "createdAt", "updatedAt", "parentFolderId"],
            None,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "fcc");
        let resp = rt.block_on(files::files_folder_create(&db, USER_A, "/a/b/c/", None));
        check_ok(
            "folder_create_parent_chain",
            response_data(&resp),
            &["id", "createdAt", "updatedAt", "parentFolderId"],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "frn");
        let resp = rt.block_on(files::files_folder_rename(
            &db,
            USER_A,
            "/docs/",
            "documents",
            None,
        ));
        check_ok(
            "folder_rename",
            response_data(&resp),
            &[],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }
    check_err(
        "folder_rename_root",
        &rt.block_on(files::files_folder_rename(
            &fresh_db(&spec, "frr"),
            USER_A,
            "/",
            "x",
            None,
        )),
        &mut failed,
    );
    {
        let db = fresh_db(&spec, "fdh");
        let resp = rt.block_on(files::files_folder_delete(&db, USER_A, "/empty/", None));
        check_ok(
            "folder_delete_happy",
            response_data(&resp),
            &[],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }
    check_err(
        "folder_delete_has_files",
        &rt.block_on(files::files_folder_delete(
            &fresh_db(&spec, "fdf"),
            USER_A,
            "/trash/",
            None,
        )),
        &mut failed,
    );
    check_err(
        "folder_delete_files_before_subfolders",
        &rt.block_on(files::files_folder_delete(
            &fresh_db(&spec, "fdb"),
            USER_A,
            "/docs/",
            None,
        )),
        &mut failed,
    );
    check_err(
        "folder_delete_has_subfolders",
        &rt.block_on(files::files_folder_delete(
            &fresh_db(&spec, "fds"),
            USER_A,
            "/sub/",
            None,
        )),
        &mut failed,
    );

    // ── P4.6ah: delete envelope (associations / force / dissociate) ──
    // The itemized FILE_HAS_ASSOCIATIONS refusal. v4 nests `{error, details:{code,
    // associations}}`; the shared CoreError carries `associations` (top-level, per
    // the contract — lane C tolerates both), so we verify the associations CONTENT
    // against v4's `details.associations`.
    {
        let db = fresh_db(&spec, "das");
        let resp = rt.block_on(files::file_delete(&db, USER_A, F_ASSOC, false, false, None));
        let rec = &oracle["delete_associations"];
        match &resp {
            Response::Error(e) => {
                let want_msg = rec["body"]["error"].as_str().unwrap_or("");
                let want_assoc = &rec["body"]["details"]["associations"];
                let got_assoc = serde_json::to_value(&e.associations).unwrap_or(Value::Null);
                if status_of(e.kind) == 400
                    && e.message == want_msg
                    && e.code.as_deref() == Some("FILE_HAS_ASSOCIATIONS")
                    && norm(&got_assoc) == norm(want_assoc)
                {
                    eprintln!("[delete_associations] OK.");
                } else {
                    eprintln!(
                        "[delete_associations] MISMATCH:\n got {} {:?} {:?}\n want 400 {want_msg:?} {}",
                        status_of(e.kind),
                        e.message,
                        norm(&got_assoc),
                        norm(want_assoc)
                    );
                    failed.push("delete_associations".to_string());
                }
            }
            other => {
                eprintln!(
                    "[delete_associations] expected error, got {:?}",
                    response_data(other)
                );
                failed.push("delete_associations".to_string());
            }
        }
    }
    // force override → skips the association check, just deletes.
    {
        let db = fresh_db(&spec, "dfo");
        let resp = rt.block_on(files::file_delete(&db, USER_A, F_ASSOC, true, false, None));
        check_ok("delete_force", response_data(&resp), &[], None, &mut failed);
        check_assoc("delete_force", &dump_assoc(&db), &oracle, &mut failed);
    }
    // dissociate → strips message attachments + character refs, then deletes.
    {
        let db = fresh_db(&spec, "ddi");
        let resp = rt.block_on(files::file_delete(&db, USER_A, F_ASSOC, false, true, None));
        check_ok(
            "delete_dissociate",
            response_data(&resp),
            &[],
            None,
            &mut failed,
        );
        check_assoc("delete_dissociate", &dump_assoc(&db), &oracle, &mut failed);
    }

    // ── P4.6ah: maintenance ──
    // thumbnails: only the DB-provable `total` (owned+resizable filter) is pinned;
    // generated/cached/errors are the enumerated codec leg (v4's counts are
    // backend-quirk-dependent).
    {
        let ids = vec![
            F_IMG1.to_string(),
            F_IMG2.to_string(),
            F_ROOT.to_string(),
            F_IMG_USERB.to_string(),
        ];
        let resp = files::files_generate_thumbnails(&fresh_db(&spec, "th"), USER_A, &ids);
        let got_total = response_data(&resp)
            .get("total")
            .cloned()
            .unwrap_or(Value::Null);
        let want_total = oracle["thumbnails"]["body"]["total"].clone();
        if norm(&got_total) == norm(&want_total) {
            eprintln!("[thumbnails total] OK.");
        } else {
            eprintln!("[thumbnails total] MISMATCH: got {got_total} want {want_total}");
            failed.push("thumbnails".to_string());
        }
    }
    {
        // The `errors` array carries the backend-specific disk-key existence
        // failure (the enumerated fs leg) — strip it and diff stale/staleFiles.
        let mut got = response_data(&rt.block_on(files::files_cleanup_stale(
            &fresh_db(&spec, "cs"),
            &DiskErrorBackend,
            USER_A,
            true,
        )));
        let mut want = oracle["cleanup_stale_dryrun"]["body"].clone();
        if let Value::Object(m) = &mut got {
            m.remove("errors");
        }
        if let Value::Object(m) = &mut want {
            m.remove("errors");
        }
        if norm(&got) == norm(&want) {
            eprintln!("[cleanup_stale_dryrun] OK.");
        } else {
            eprintln!(
                "[cleanup_stale_dryrun] MISMATCH:\n got {}\n want {}",
                norm(&got),
                norm(&want)
            );
            failed.push("cleanup_stale_dryrun".to_string());
        }
    }
    check_ok(
        "cleanup_orphans_dryrun",
        response_data(&rt.block_on(files::files_cleanup_orphans(
            &fresh_db(&spec, "cod"),
            USER_A,
            "delete",
            true,
        ))),
        &[],
        None,
        &mut failed,
    );
    {
        let db = fresh_db(&spec, "com");
        let resp = rt.block_on(files::files_cleanup_orphans(&db, USER_A, "move", false));
        check_ok(
            "cleanup_orphans_move",
            response_data(&resp),
            &[],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }

    // ── P4.6ah: general upload (saveFileEntry) ──
    {
        let db = fresh_db(&spec, "unp");
        let resp = rt.block_on(files::file_upload(
            &db,
            USER_A,
            "newdoc.txt",
            "text/plain",
            b"a fresh document body".to_vec(),
            vec![],
            Some(PROJECT.to_string()),
            None,
            None,
        ));
        check_ok(
            "upload_new_project",
            response_data(&resp),
            &["id", "filepath", "createdAt", "updatedAt"],
            None,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "uov");
        let resp = rt.block_on(files::file_upload(
            &db,
            USER_A,
            "p1.txt",
            "text/plain",
            b"overwritten p1 body".to_vec(),
            vec![],
            Some(PROJECT.to_string()),
            None,
            None,
        ));
        check_ok(
            "upload_overwrite_project",
            response_data(&resp),
            &["updatedAt"],
            Some(dump_tables(&db)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "ung");
        let resp = rt.block_on(files::file_upload(
            &db,
            USER_A,
            "gen.txt",
            "text/plain",
            b"general upload body".to_vec(),
            vec![],
            None,
            None,
            None,
        ));
        check_ok(
            "upload_new_general",
            response_data(&resp),
            &["id", "filepath", "createdAt", "updatedAt"],
            None,
            &mut failed,
        );
    }

    // ── P4.6ah: chat upload (uploadChatFile) + link ──
    let chat_upload = |tag: &str,
                       chat: &str,
                       name: &str,
                       content: &str,
                       res: Option<&str>,
                       cfid: Option<&str>| {
        let db = fresh_db(&spec, tag);
        let resp = rt.block_on(chat_media::chat_file_upload(
            &db,
            USER_A,
            chat,
            ChatFileUploadInput {
                filename: name.to_string(),
                content_type: "text/plain".to_string(),
                data: b64(content),
                resolution: res.map(str::to_string),
                conflicting_file_id: cfid.map(str::to_string),
            },
        ));
        response_data(&resp)
    };
    check_ok(
        "chat_upload_nonproject",
        chat_upload("cun", CHAT_G, "note.txt", "a chat note", None, None),
        &["id", "filepath", "url"],
        None,
        &mut failed,
    );
    check_ok(
        "chat_upload_conflict",
        chat_upload("cuc", CHAT_P, "dup.txt", "new dup content", None, None),
        &[],
        None,
        &mut failed,
    );
    check_ok(
        "chat_upload_skip",
        chat_upload("cus", CHAT_P, "dup.txt", "skip me", Some("skip"), None),
        &[],
        None,
        &mut failed,
    );
    check_ok(
        "chat_upload_keepboth",
        chat_upload(
            "cuk",
            CHAT_P,
            "dup.txt",
            "kept both body",
            Some("keepBoth"),
            None,
        ),
        &["id", "filepath", "url"],
        None,
        &mut failed,
    );
    check_ok(
        "chat_upload_replace",
        chat_upload(
            "cur",
            CHAT_P,
            "dup.txt",
            "replacement body",
            Some("replace"),
            Some(PF_DUP),
        ),
        &["id", "filepath", "url"],
        None,
        &mut failed,
    );
    check_ok(
        "chat_upload_link",
        response_data(&rt.block_on(chat_media::chat_file_link(
            &fresh_db(&spec, "cul"),
            USER_A,
            CHAT_G,
            F_UNLINKED,
        ))),
        &[],
        None,
        &mut failed,
    );

    assert!(failed.is_empty(), "files-routes mismatches: {failed:?}");
}
