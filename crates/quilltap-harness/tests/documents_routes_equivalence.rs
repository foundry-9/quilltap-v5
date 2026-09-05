//! P4.6w DOCUMENT MODE route-surface differential: `api::documents::*` vs v4's
//! REAL chat-scoped + standalone document handlers. Both sides read a FRESH copy
//! of the committed documents-{main,mount}.db fixture per case (baked ids
//! identical → no remap). Reads carry the full body; write cases additionally
//! diff the chat_documents / documentMode state (id-free). Minted fields (the
//! open/write mtime, the reactivated/created row id, the Librarian message
//! id/createdAt) are blanked; everything else — including the Librarian body
//! text — is compared byte-for-byte.
//!
//! Generate the oracle (Node 24, from the v4 checkout — cp to a /tmp mirror; jest
//! ignores .claude/):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-doc-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/documents-routes.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/documents-web.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DOC_MAIN=$V5W/crates/quilltap-web/tests/fixtures/documents-main.db \
//!   QT_FIXTURE_DOC_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/documents-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-documents-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- documents-routes
//! Run:
//!   QT_ORACLE_DOCUMENTS_ROUTES=/tmp/oracle-documents-routes.ndjson \
//!     cargo test -p quilltap-harness --test documents_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::documents;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const CHAT_A: &str = "c1000000-0000-4000-8000-00000000000a";
const R1: &str = "f1000000-0000-4000-8000-000000000001";
const R3: &str = "f1000000-0000-4000-8000-000000000003";
const STANDALONE: &str = "00000000-0000-0000-0000-000000000000";
const GEN: &str = "Quilltap General";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    #[allow(dead_code)]
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/documents-web.json")
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

fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
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

/// Blank the minted / re-generated fields: the top-level `mtime`, the
/// `document.id`, and the `librarianMessage.{id,createdAt}`.
fn blank_minted(v: &mut Value) {
    if let Some(obj) = v.as_object_mut() {
        if obj.contains_key("mtime") {
            obj.insert("mtime".into(), json!("<mtime>"));
        }
        if let Some(doc) = obj.get_mut("document").and_then(|d| d.as_object_mut()) {
            if doc.contains_key("id") {
                doc.insert("id".into(), json!("<id>"));
            }
        }
        if let Some(lm) = obj.get_mut("librarianMessage") {
            if let Some(lm) = lm.as_object_mut() {
                if lm.contains_key("id") {
                    lm.insert("id".into(), json!("<id>"));
                }
                if lm.contains_key("createdAt") {
                    lm.insert("createdAt".into(), json!("<createdAt>"));
                }
            }
        }
    }
}

fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_minted(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 423,
        // The store-unavailable refusal (P4.23) — v4's deliberate
        // contextful 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}
fn response_data(r: &Response) -> Value {
    serde_json::to_value(r)
        .unwrap()
        .get("data")
        .cloned()
        .unwrap_or(Value::Null)
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-doc-routes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("documents-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("documents-mount.db"), &mount).unwrap();
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

/// The id-free chat_documents state for a chat + its documentMode (the oracle's
/// `tables` shape).
fn dump_chat_docs(db: &Db, chat_id: &str) -> Value {
    use quilltap_core::db::chat_documents::ChatDocumentsRepository;
    let cid = chat_id.to_string();
    db.read_main(move |main| {
        let repo = ChatDocumentsRepository::new(main);
        let mut rows = repo.find_by_chat_id_full(&cid)?;
        rows.sort_by(|a, b| {
            (a.scope.as_str(), a.file_path.as_str()).cmp(&(b.scope.as_str(), b.file_path.as_str()))
        });
        let docs: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({
                    "filePath": r.file_path,
                    "scope": r.scope,
                    "mountPoint": r.mount_point,
                    "displayTitle": r.display_title,
                    "isActive": r.is_active,
                })
            })
            .collect();
        let mode = quilltap_core::db::chats_read::find_by_id(main, &cid)?.and_then(|c| {
            c.get("documentMode")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        Ok(json!({ "documentMode": mode, "docs": docs }))
    })
    .unwrap()
}

#[test]
fn documents_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_DOCUMENTS_ROUTES") else {
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

    // Compare a 200 body (+ optional tables) or an error (status + message).
    let check = |name: &str, got: &Response, tables: Option<Value>, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_u64().unwrap_or(0) as u16;
        if want_status == 200 {
            if let Response::Error(e) = got {
                eprintln!(
                    "[{name}] expected 200, got error {}: {}",
                    status_of(e.kind),
                    e.message
                );
                failed.push(name.to_string());
                return;
            }
            let mut got_body = response_data(got);
            if let Some(t) = tables {
                if let Some(obj) = got_body.as_object_mut() {
                    obj.insert("__tables__".into(), t);
                }
            }
            let mut want = rec["body"].clone();
            if let Some(t) = rec.get("tables") {
                if let Some(obj) = want.as_object_mut() {
                    obj.insert("__tables__".into(), t.clone());
                }
            }
            if norm(&got_body) != norm(&want) {
                eprintln!(
                    "[{name}] MISMATCH:\n{}",
                    first_diff(&norm(&got_body), &norm(&want))
                );
                failed.push(name.to_string());
            } else {
                eprintln!("[{name}] OK.");
            }
        } else {
            // Error arm: status + message (v4 `{error}` body). Tables still diffed.
            match got {
                Response::Error(e) => {
                    let got_status = status_of(e.kind);
                    let want_msg = rec["body"]["error"].as_str().unwrap_or("");
                    let mut ok = got_status == want_status && e.message == want_msg;
                    if let (Some(gt), Some(wt)) = (&tables, rec.get("tables")) {
                        ok = ok && norm(gt) == norm(wt);
                    }
                    if !ok {
                        eprintln!(
                            "[{name}] MISMATCH: got {got_status} {:?} / want {want_status} {want_msg:?}",
                            e.message
                        );
                        failed.push(name.to_string());
                    } else {
                        eprintln!("[{name}] OK ({got_status}).");
                    }
                }
                other => {
                    eprintln!("[{name}] expected error, got {:?}", response_data(other));
                    failed.push(name.to_string());
                }
            }
        }
    };

    // ── reads ──────────────────────────────────────────────────────────────
    let db = fresh_db(&spec, "r");
    check(
        "chat_active_document",
        &documents::chat_active_document(&db, CHAT_A),
        None,
        &mut failed,
    );
    check(
        "chat_open_documents",
        &documents::chat_open_documents(&db, CHAT_A),
        None,
        &mut failed,
    );
    check(
        "chat_recent_documents",
        &documents::chat_recent_documents(&db, CHAT_A),
        None,
        &mut failed,
    );
    check(
        "chat_accessible_stores_all",
        &documents::chat_accessible_stores(&db, CHAT_A, true),
        None,
        &mut failed,
    );
    check(
        "chat_accessible_stores_default",
        &documents::chat_accessible_stores(&db, CHAT_A, false),
        None,
        &mut failed,
    );
    let read_bag =
        |fp: &str| json!({ "filePath": fp, "scope": "document_store", "mountPoint": GEN });
    check(
        "chat_read_document_existing",
        &documents::chat_document_read(&db, CHAT_A, read_bag("notes/alpha.md"), None),
        None,
        &mut failed,
    );
    check(
        "chat_read_document_missing",
        &documents::chat_document_read(&db, CHAT_A, read_bag("notes/missing.md"), None),
        None,
        &mut failed,
    );
    check(
        "chat_resolve_document_exists",
        &documents::chat_document_resolve(&db, CHAT_A, read_bag("notes/alpha.md"), None),
        None,
        &mut failed,
    );
    check(
        "chat_resolve_document_missing",
        &documents::chat_document_resolve(&db, CHAT_A, read_bag("notes/missing.md"), None),
        None,
        &mut failed,
    );
    check(
        "standalone_stores",
        &documents::document_stores(&db),
        None,
        &mut failed,
    );
    check(
        "standalone_recent",
        &documents::documents_recent(&db),
        None,
        &mut failed,
    );

    // ── chat-scoped writes (fresh db each) ─────────────────────────────────
    {
        let db = fresh_db(&spec, "open-react");
        let r = rt.block_on(documents::chat_document_open(
            &db,
            CHAT_A,
            json!({ "filePath": "notes/beta.md", "scope": "document_store", "mountPoint": GEN, "mode": "split" }),
            None,
        ));
        let t = dump_chat_docs(&db, CHAT_A);
        check("chat_open_document_reactivate", &r, Some(t), &mut failed);
    }
    {
        let db = fresh_db(&spec, "open-new");
        let r = rt.block_on(documents::chat_document_open(
            &db,
            CHAT_A,
            json!({ "scope": "document_store", "mountPoint": GEN, "targetFolder": "notes", "mode": "split" }),
            None,
        ));
        let t = dump_chat_docs(&db, CHAT_A);
        check("chat_open_document_new_blank", &r, Some(t), &mut failed);
    }
    {
        let db = fresh_db(&spec, "close");
        let r = rt.block_on(documents::chat_document_close(
            &db,
            CHAT_A,
            Some(R1.to_string()),
        ));
        let t = dump_chat_docs(&db, CHAT_A);
        check("chat_close_document", &r, Some(t), &mut failed);
    }
    {
        let db = fresh_db(&spec, "write");
        let r = rt.block_on(documents::chat_document_write(
            &db,
            CHAT_A,
            json!({ "filePath": "notes/alpha.md", "scope": "document_store", "mountPoint": GEN, "content": "# Alpha\n\nEdited body.\n", "diffContent": "I've made changes to \"alpha.md\":\n- edited" }),
            None,
            None,
        ));
        check("chat_write_document", &r, None, &mut failed);
    }
    {
        let db = fresh_db(&spec, "write-conflict");
        let r = rt.block_on(documents::chat_document_write(
            &db,
            CHAT_A,
            json!({ "filePath": "notes/alpha.md", "scope": "document_store", "mountPoint": GEN, "content": "x", "mtime": 111 }),
            None,
            None,
        ));
        check("chat_write_document_conflict", &r, None, &mut failed);
    }
    {
        let db = fresh_db(&spec, "rename");
        let r = rt.block_on(documents::chat_document_rename(
            &db,
            CHAT_A,
            json!({ "newTitle": "renamed", "chatDocumentId": R1 }),
            None,
            None,
        ));
        let t = json!({ "chatA": dump_chat_docs(&db, CHAT_A), "standalone": dump_chat_docs(&db, STANDALONE) });
        check("chat_rename_document", &r, Some(t), &mut failed);
    }
    {
        let db = fresh_db(&spec, "rename-conflict");
        let r = rt.block_on(documents::chat_document_rename(
            &db,
            CHAT_A,
            json!({ "newTitle": "beta", "chatDocumentId": R1 }),
            None,
            None,
        ));
        check("chat_rename_document_conflict", &r, None, &mut failed);
    }
    {
        let db = fresh_db(&spec, "delete");
        let r = rt.block_on(documents::chat_document_delete(
            &db,
            CHAT_A,
            Some(R3.to_string()),
            None,
        ));
        let t = dump_chat_docs(&db, CHAT_A);
        check("chat_delete_document", &r, Some(t), &mut failed);
    }
    {
        // v4 `0506517d3` correction (c): the chat-scoped delete's 404 body reads
        // "File not found", not "File not found not found". Reached by deleting
        // the SAME row twice — `resolve_target_document` looks a named id up by
        // id alone (no active filter), so the second call finds the closed row
        // and the database store answers "no such document". An id absent from
        // the fixture answers 400 "No active document to delete" instead.
        let db = fresh_db(&spec, "delete-missing");
        let _ = rt.block_on(documents::chat_document_delete(
            &db,
            CHAT_A,
            Some(R1.to_string()),
            None,
        ));
        let r = rt.block_on(documents::chat_document_delete(
            &db,
            CHAT_A,
            Some(R1.to_string()),
            None,
        ));
        let t = dump_chat_docs(&db, CHAT_A);
        check("chat_delete_document_missing", &r, Some(t), &mut failed);
    }

    // ── standalone writes ──────────────────────────────────────────────────
    {
        let db = fresh_db(&spec, "sa-open");
        let r = rt.block_on(documents::document_open(
            &db,
            json!({ "filePath": "notes/alpha.md", "scope": "document_store", "mountPoint": GEN }),
            None,
        ));
        check("standalone_open_existing", &r, None, &mut failed);
    }
    {
        let db = fresh_db(&spec, "sa-read");
        let r = documents::document_read(
            &db,
            json!({ "filePath": "notes/gamma.md", "scope": "document_store", "mountPoint": GEN }),
            None,
        );
        check("standalone_read", &r, None, &mut failed);
    }
    {
        let db = fresh_db(&spec, "sa-write");
        let r = rt.block_on(documents::document_write(
            &db,
            json!({ "filePath": "notes/gamma.md", "scope": "document_store", "mountPoint": GEN, "content": "# Gamma\n\nEdited.\n" }),
            None,
            None,
        ));
        check("standalone_write", &r, None, &mut failed);
    }
    {
        let db = fresh_db(&spec, "sa-rename");
        let r = rt.block_on(documents::document_rename(
            &db,
            json!({ "filePath": "notes/gamma.md", "scope": "document_store", "mountPoint": GEN, "newTitle": "delta" }),
            None,
            None,
        ));
        check("standalone_rename", &r, None, &mut failed);
    }
    {
        let db = fresh_db(&spec, "sa-delete");
        let r = rt.block_on(documents::document_delete(
            &db,
            json!({ "filePath": "notes/gamma.md", "scope": "document_store", "mountPoint": GEN }),
            None,
        ));
        check("standalone_delete", &r, None, &mut failed);
    }

    assert!(failed.is_empty(), "documents route mismatches: {failed:?}");
}
