//! Tier-2 differential: the five subprompt ROUTES (v4 `2f4254b42`,
//! `app/api/v1/characters/[id]/subprompts/route.ts` GET/POST +
//! `[subpromptId]/route.ts` GET/PUT/DELETE) — `quilltap_core::api::subprompts`,
//! P4.D163 unit 4.
//!
//! Both sides run each case on a FRESH copy of the committed
//! `subprompts-{main,mount}.db` pair and compare the STATUS (v4's HTTP status
//! vs the `ErrorKind` mapping — a 2xx on both is a success, the REST edge's
//! 201 lives in `quilltap-web` and is wire-tested there), the BODY byte for
//! byte (v4's `{subprompts}` / `{subprompt}` / `{success}` / `{error}` and the
//! Zod `{error: 'Validation error', details}` envelope — `details` compared
//! serialized), the RECORDED seams (the routes' own `publishRealtime(
//! 'characters', id)` + the fan-out's `chats` publishes + the recompile calls),
//! and a compact CENSUS of the vault links/folders, the characters' vault
//! links and the chats' participants after the mutating cases.
//!
//! The guard ladders were MEASURED on v4's real handlers by this oracle (the
//! `a6870c5a` class), not assumed: POST/PUT parse the body BEFORE the
//! character lookup (a bad body on a missing character is 400); GET/DELETE
//! validate the id first (a bad id on a missing character is 400); good
//! everything + a missing character is 404; the service's UTF-16 title rule
//! fires AFTER Zod's code-point rule and AFTER the character lookup (a blank
//! title on a missing character is 404).
//!
//! NORMALIZATION: an ISO timestamp equal to the seed sentinel is exact, any
//! other is `<ts>`; mount-point ids map to the fixture's vault keys.
//!
//! Generate the oracle (Node 24, from the v4 checkout — a pinned worktree
//! while v4 HEAD is past the baseline; cp to a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-subprompts-routes-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/subprompts-routes.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/subprompts.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/subprompts-main.db \
//!   QT_FIXTURE_SP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/subprompts-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-subprompts-routes.ndjson TZ=UTC \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- subprompts-routes
//! Run:
//!   QT_ORACLE_SUBPROMPTS_ROUTES=/tmp/oracle-subprompts-routes.ndjson \
//!     cargo test -p quilltap-harness --test subprompts_routes_equivalence -- --nocapture

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::subprompts as api;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::DbError;
use quilltap_core::subprompts::FanoutSeams;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    seed_timestamp: String,
    user_id: String,
    ids: BTreeMap<String, String>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/subprompts.json")
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

fn http_for(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked | ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// The recorder for BOTH publish topics and the recompile.
#[derive(Default)]
struct Recorder {
    compile: Mutex<Vec<(String, String)>>,
    publish: Mutex<Vec<(String, Option<String>)>>,
}
impl Recorder {
    fn recorded(&self) -> Value {
        let compile: Vec<Value> = self
            .compile
            .lock()
            .unwrap()
            .iter()
            .map(|(c, p)| json!([c, p]))
            .collect();
        let publish: Vec<Value> = self
            .publish
            .lock()
            .unwrap()
            .iter()
            .map(|(t, id)| json!([t, id]))
            .collect();
        json!({ "compile": compile, "publish": publish })
    }
}
impl FanoutSeams for Recorder {
    fn compile(
        &self,
        _m: &Connection,
        _mo: &Connection,
        chat: &Value,
        participant_id: &str,
    ) -> Result<(), DbError> {
        self.compile.lock().unwrap().push((
            chat.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            participant_id.to_string(),
        ));
        Ok(())
    }
    fn publish_chat(&self, chat_id: &str) {
        self.publish
            .lock()
            .unwrap()
            .push(("chats".to_string(), Some(chat_id.to_string())));
    }
    fn publish_character(&self, character_id: &str) {
        self.publish
            .lock()
            .unwrap()
            .push(("characters".to_string(), Some(character_id.to_string())));
    }
}

fn ts(v: Option<String>, sentinel: &str) -> Value {
    match v {
        Some(s)
            if s.len() >= 11
                && s.as_bytes()[10] == b'T'
                && s[..4].chars().all(|c| c.is_ascii_digit()) =>
        {
            Value::String(if s == sentinel { s } else { "<ts>".to_string() })
        }
        Some(s) => Value::String(s),
        None => Value::Null,
    }
}
fn norm(v: Value, sentinel: &str) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.into_iter().map(|x| norm(x, sentinel)).collect()),
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, val) in o {
                if k == "updatedAt" {
                    out.insert(k, ts(val.as_str().map(str::to_string), sentinel));
                } else {
                    out.insert(k, norm(val, sentinel));
                }
            }
            Value::Object(out)
        }
        other => other,
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
fn first_diff(a: &Value, b: &Value) -> String {
    let sa = serde_json::to_string_pretty(a).unwrap();
    let sb = serde_json::to_string_pretty(b).unwrap();
    let la: Vec<&str> = sa.lines().collect();
    let lb: Vec<&str> = sb.lines().collect();
    for i in 0..la.len().max(lb.len()) {
        let x = la.get(i).copied().unwrap_or("<eof>");
        let y = lb.get(i).copied().unwrap_or("<eof>");
        if x != y {
            let lo = i.saturating_sub(3);
            let ctx: Vec<String> = (lo..i)
                .map(|j| format!("   = {}", la.get(j).copied().unwrap_or("")))
                .collect();
            return format!("{}\n  GOT : {x}\n  WANT: {y}", ctx.join("\n"));
        }
    }
    String::new()
}
fn opt_str(row: &rusqlite::Row<'_>, idx: usize) -> Option<String> {
    row.get::<_, Option<String>>(idx).unwrap_or(None)
}

/// The compact census (the SAME queries and shapes as the oracle's).
fn census(
    main: &Connection,
    mount: &Connection,
    sentinel: &str,
    vaults: &HashMap<String, String>,
) -> Value {
    let point = |id: Option<String>| -> Value {
        match id {
            Some(id) => Value::String(
                vaults
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "<new-point>".into()),
            ),
            None => Value::Null,
        }
    };
    let s_of = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let mut links: Vec<Value> = mount
        .prepare(
            "SELECT l.mountPointId, l.relativePath, fo.path, l.lastModified, d.content \
             FROM doc_mount_file_links l LEFT JOIN doc_mount_folders fo ON fo.id = l.folderId \
             LEFT JOIN doc_mount_documents d ON d.fileId = l.fileId",
        )
        .unwrap()
        .query_map([], |r| {
            Ok(json!({
                "point": point(opt_str(r, 0)), "relativePath": opt_str(r, 1),
                "folderPath": opt_str(r, 2), "lastModified": ts(opt_str(r, 3), sentinel),
                "content": opt_str(r, 4),
            }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    links.sort_by(|a, b| {
        s_of(&a["point"])
            .cmp(&s_of(&b["point"]))
            .then(s_of(&a["relativePath"]).cmp(&s_of(&b["relativePath"])))
    });
    let mut folders: Vec<Value> = mount
        .prepare("SELECT mountPointId, path FROM doc_mount_folders")
        .unwrap()
        .query_map([], |r| {
            Ok(json!({ "point": point(opt_str(r, 0)), "path": opt_str(r, 1) }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    folders.sort_by(|a, b| {
        s_of(&a["point"])
            .cmp(&s_of(&b["point"]))
            .then(s_of(&a["path"]).cmp(&s_of(&b["path"])))
    });
    let characters: Vec<Value> = main
        .prepare("SELECT id, characterDocumentMountPointId FROM characters ORDER BY id")
        .unwrap()
        .query_map([], |r| {
            Ok(json!({ "id": opt_str(r, 0), "vault": point(opt_str(r, 1)) }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let chats: Vec<Value> = main
        .prepare("SELECT id, participants FROM chats ORDER BY id")
        .unwrap()
        .query_map([], |r| {
            let parts: Value = opt_str(r, 1)
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::from_str(&s).unwrap())
                .unwrap_or(Value::Null);
            let parts = match parts {
                Value::Array(a) => Value::Array(
                    a.into_iter()
                        .map(|p| {
                            let mut o = p.as_object().cloned().unwrap_or_default();
                            for k in ["createdAt", "updatedAt"] {
                                let v = o.get(k).and_then(Value::as_str).map(str::to_string);
                                if v.is_some() {
                                    o.insert(k.to_string(), ts(v, sentinel));
                                }
                            }
                            Value::Object(o)
                        })
                        .collect(),
                ),
                other => other,
            };
            Ok(json!({ "id": opt_str(r, 0), "participants": parts }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    json!({ "links": links, "folders": folders, "characters": characters, "chats": chats })
}

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    status: u16,
    body: Value,
    recorded: Value,
    #[serde(default)]
    census: Option<Value>,
}

struct Case {
    name: &'static str,
    method: &'static str,
    character: &'static str,
    subprompt: Option<&'static str>,
    body: Option<Value>,
}
fn c(
    name: &'static str,
    method: &'static str,
    character: &'static str,
    subprompt: Option<&'static str>,
    body: Option<Value>,
) -> Case {
    Case {
        name,
        method,
        character,
        subprompt,
        body,
    }
}

fn cases() -> Vec<Case> {
    let x = |n: usize| "x".repeat(n);
    vec![
        c("list_a", "GET", "charA", None, None),
        c("list_b_no_folder", "GET", "charB", None, None),
        c("list_c_no_vault", "GET", "charC", None, None),
        c("list_d_archived_reads", "GET", "charD", None, None),
        c("list_missing_404", "GET", "missing", None, None),
        c(
            "create_a_201",
            "POST",
            "charA",
            None,
            Some(json!({"title": "New one", "content": "Say less."})),
        ),
        c(
            "create_a_collision_201",
            "POST",
            "charA",
            None,
            Some(json!({"title": "Be terse", "content": "again"})),
        ),
        c(
            "create_b_ensures_folder_201",
            "POST",
            "charB",
            None,
            Some(json!({"title": "First", "content": "x"})),
        ),
        c(
            "create_c_provisions_201",
            "POST",
            "charC",
            None,
            Some(json!({"title": "Fresh", "content": "x"})),
        ),
        c(
            "create_d_archived_409",
            "POST",
            "charD",
            None,
            Some(json!({"title": "Nope", "content": "x"})),
        ),
        c(
            "create_missing_404",
            "POST",
            "missing",
            None,
            Some(json!({"title": "T", "content": "x"})),
        ),
        c(
            "create_zod_title_101_code_points_400",
            "POST",
            "charA",
            None,
            Some(json!({"title": x(101), "content": "x"})),
        ),
        c(
            "create_zod_title_empty_400",
            "POST",
            "charA",
            None,
            Some(json!({"title": "", "content": "x"})),
        ),
        c(
            "create_zod_content_missing_400",
            "POST",
            "charA",
            None,
            Some(json!({"title": "T"})),
        ),
        c(
            "create_zod_title_null_400",
            "POST",
            "charA",
            None,
            Some(json!({"title": null, "content": "x"})),
        ),
        c(
            "create_zod_body_null_400",
            "POST",
            "charA",
            None,
            Some(Value::Null),
        ),
        c(
            "create_zod_both_bad_two_issues_400",
            "POST",
            "charA",
            None,
            Some(json!({"title": "", "content": ""})),
        ),
        c(
            "create_zod_unknown_key_stripped_201",
            "POST",
            "charA",
            None,
            Some(json!({"title": "Stripped", "content": "x", "bogus": 1})),
        ),
        c(
            "create_bad_body_missing_character_400_not_404",
            "POST",
            "missing",
            None,
            Some(json!({"title": "", "content": "x"})),
        ),
        c(
            "create_service_title_100_astral_400",
            "POST",
            "charA",
            None,
            Some(json!({"title": "😀".repeat(100), "content": "x"})),
        ),
        c(
            "create_service_title_blank_400",
            "POST",
            "charA",
            None,
            Some(json!({"title": "   ", "content": "x"})),
        ),
        c(
            "create_service_content_blank_400",
            "POST",
            "charA",
            None,
            Some(json!({"title": "T", "content": " \n "})),
        ),
        c(
            "create_service_title_blank_missing_character_404",
            "POST",
            "missing",
            None,
            Some(json!({"title": "   ", "content": "x"})),
        ),
        c("get_a_terse", "GET", "charA", Some("terse"), None),
        c(
            "get_a_verse_lower_case",
            "GET",
            "charA",
            Some("verse"),
            None,
        ),
        c("get_a_gone_404", "GET", "charA", Some("gone"), None),
        c("get_a_bad_id_400", "GET", "charA", Some("a/b"), None),
        c("get_a_bad_id_dotdot_400", "GET", "charA", Some(".."), None),
        c(
            "get_bad_id_missing_character_400_not_404",
            "GET",
            "missing",
            Some("a/b"),
            None,
        ),
        c(
            "get_good_id_missing_character_404",
            "GET",
            "missing",
            Some("terse"),
            None,
        ),
        c(
            "get_d_keep_archived_reads",
            "GET",
            "charD",
            Some("keep"),
            None,
        ),
        c("get_c_no_vault_404", "GET", "charC", Some("terse"), None),
        c(
            "update_a_title_only",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({"title": "Be brief"})),
        ),
        c(
            "update_a_content_only",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({"content": "Two lines."})),
        ),
        c(
            "update_a_both",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({"title": "Brief", "content": "Short."})),
        ),
        c(
            "update_a_empty_body_noop_rewrite",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({})),
        ),
        c(
            "update_a_gone_404",
            "PUT",
            "charA",
            Some("gone"),
            Some(json!({"title": "x"})),
        ),
        c(
            "update_a_bad_id_400",
            "PUT",
            "charA",
            Some("a/b"),
            Some(json!({"title": "x"})),
        ),
        c(
            "update_d_archived_409",
            "PUT",
            "charD",
            Some("keep"),
            Some(json!({"title": "x"})),
        ),
        c(
            "update_missing_404",
            "PUT",
            "missing",
            Some("terse"),
            Some(json!({"title": "x"})),
        ),
        c(
            "update_zod_title_null_400",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({"title": null})),
        ),
        c(
            "update_zod_title_101_400",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({"title": x(101)})),
        ),
        c(
            "update_zod_content_empty_400",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({"content": ""})),
        ),
        c(
            "update_zod_body_null_400",
            "PUT",
            "charA",
            Some("terse"),
            Some(Value::Null),
        ),
        c(
            "update_bad_body_bad_id_400_zod_first",
            "PUT",
            "charA",
            Some("a/b"),
            Some(json!({"title": null})),
        ),
        c(
            "update_bad_body_missing_character_400_not_404",
            "PUT",
            "missing",
            Some("terse"),
            Some(json!({"title": null})),
        ),
        c(
            "update_good_body_bad_id_missing_character_400",
            "PUT",
            "missing",
            Some("a/b"),
            Some(json!({"title": "x"})),
        ),
        c(
            "update_service_title_100_astral_400",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({"title": "😀".repeat(100)})),
        ),
        c(
            "update_service_title_blank_400",
            "PUT",
            "charA",
            Some("terse"),
            Some(json!({"title": "  "})),
        ),
        c(
            "update_a_verse_lower_case_writes_lower_path",
            "PUT",
            "charA",
            Some("verse"),
            Some(json!({"title": "In verse"})),
        ),
        c(
            "delete_a_terse_fans_out",
            "DELETE",
            "charA",
            Some("terse"),
            None,
        ),
        c(
            "delete_a_VERSE_case_insensitive_strip",
            "DELETE",
            "charA",
            Some("Verse"),
            None,
        ),
        c(
            "delete_a_gone_404_no_fanout",
            "DELETE",
            "charA",
            Some("gone"),
            None,
        ),
        c("delete_a_bad_id_400", "DELETE", "charA", Some("a/b"), None),
        c(
            "delete_bad_id_missing_character_400_not_404",
            "DELETE",
            "missing",
            Some("a/b"),
            None,
        ),
        c(
            "delete_good_id_missing_character_404",
            "DELETE",
            "missing",
            Some("terse"),
            None,
        ),
        c(
            "delete_d_archived_409",
            "DELETE",
            "charD",
            Some("keep"),
            None,
        ),
        c(
            "delete_c_no_vault_provisions_then_404",
            "DELETE",
            "charC",
            Some("terse"),
            None,
        ),
    ]
}

/// `(status, body)` from a dispatch `Response`: the data for a success (2xx —
/// the exact code is the transport's), `{error[, details]}` for an error.
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::Error(e) => {
            let body = e
                .validation_wire_body()
                .unwrap_or_else(|| json!({ "error": e.message }));
            (http_for(e.kind), body)
        }
        other => {
            let v = serde_json::to_value(other).unwrap();
            (200, v.get("data").cloned().unwrap_or(Value::Null))
        }
    }
}

#[test]
fn subprompts_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SUBPROMPTS_ROUTES") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let meta: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("subprompts-main.db.meta.json")).unwrap(),
    )
    .unwrap();
    let vaults: HashMap<String, String> = meta["vaults"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (v.as_str().unwrap().to_string(), k.clone()))
        .collect();
    let oracle: HashMap<String, OracleRow> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let r: OracleRow = serde_json::from_str(l).expect("oracle row");
            (r.name.clone(), r)
        })
        .collect();
    let all = cases();
    assert!(!oracle.is_empty(), "empty oracle (the empty-file trap)");
    assert_eq!(
        oracle.len(),
        all.len(),
        "oracle case count != corpus (shape)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    for case in &all {
        let name = case.name;
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("oracle missing {name}"));
        let scratch =
            std::env::temp_dir().join(format!("qt-spr-rust-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let main_work = scratch.join("main.db");
        let mount_work = scratch.join("mount.db");
        std::fs::copy(fixtures_dir().join("subprompts-main.db"), &main_work).unwrap();
        std::fs::copy(fixtures_dir().join("subprompts-mount.db"), &mount_work).unwrap();
        let db = Db::open(
            DbPaths {
                main: main_work,
                mount_index: Some(mount_work),
                llm_logs: None,
            },
            &spec.test_pepper_base64,
        )
        .expect("open db");
        let rec: Arc<Recorder> = Arc::new(Recorder::default());
        let seams: Arc<dyn FanoutSeams> = rec.clone();
        let cid = spec.ids.get(case.character).unwrap().as_str();
        let uid = spec.user_id.as_str();
        let body = case.body.clone().unwrap_or(Value::Null);
        let resp = match (case.method, case.subprompt) {
            ("GET", None) => rt.block_on(api::character_subprompt_list(&db, cid)),
            ("POST", None) => rt.block_on(api::character_subprompt_create(
                &db,
                uid,
                cid,
                body,
                seams.clone(),
            )),
            ("GET", Some(sid)) => rt.block_on(api::character_subprompt_get(&db, cid, sid)),
            ("PUT", Some(sid)) => rt.block_on(api::character_subprompt_update(
                &db,
                uid,
                cid,
                sid,
                body,
                seams.clone(),
            )),
            ("DELETE", Some(sid)) => rt.block_on(api::character_subprompt_delete(
                &db,
                uid,
                cid,
                sid,
                seams.clone(),
            )),
            other => panic!("unknown case shape {other:?}"),
        };
        let (status, body) = status_body(&resp);
        let want_ok = (200..300).contains(&want.status);
        let got_ok = (200..300).contains(&status);
        if want_ok != got_ok || (!want_ok && status != want.status) {
            failed.push(format!("{name} STATUS: v5={status} v4={}", want.status));
        }
        let got_body = sorted(&norm(body, &spec.seed_timestamp));
        let want_body = sorted(&want.body);
        if got_body != want_body {
            failed.push(format!(
                "{name} BODY:\n{}",
                first_diff(&got_body, &want_body)
            ));
        }
        let got_rec = sorted(&rec.recorded());
        let want_rec = sorted(&want.recorded);
        if got_rec != want_rec {
            failed.push(format!(
                "{name} RECORDED:\n{}",
                first_diff(&got_rec, &want_rec)
            ));
        }
        if let Some(want_census) = &want.census {
            let got_census = db
                .read_main(|main| {
                    db.read_mount_index(|mount| {
                        Ok(census(main, mount, &spec.seed_timestamp, &vaults))
                    })
                })
                .unwrap();
            if sorted(&got_census) != sorted(want_census) {
                failed.push(format!(
                    "{name} CENSUS:\n{}",
                    first_diff(&sorted(&got_census), &sorted(want_census))
                ));
            }
        }
        eprintln!(
            "[{name}] {}",
            if failed.last().is_some_and(|f| f.starts_with(name)) {
                "MISMATCH"
            } else {
                "OK"
            }
        );
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
    }
    assert!(
        failed.is_empty(),
        "{} case(s) differ:\n{}",
        failed.len(),
        failed.join("\n\n")
    );
    eprintln!(
        "OK: subprompts routes matched oracle ({} cases).",
        all.len()
    );
}
