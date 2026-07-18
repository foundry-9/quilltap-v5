//! P4.6k PROJECTS route-surface differential: `api::projects::*` vs v4's REAL
//! projects route handlers. Both sides read a FRESH copy of the committed
//! groups-projects fixture (baked ids identical → no remap, except CREATE and the
//! route-minted `updatedAt` on update/link, which are blanked). Reads carry the
//! full body; mutations carry the body + a post-op table dump.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-projects-routes.ndjson npx jest -- projects-routes
//! Run:
//!   QT_ORACLE_PROJECTS_ROUTES=/tmp/oracle-projects-routes.ndjson \
//!     cargo test -p quilltap-harness --test projects_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::projects;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const IOTA: &str = "a3000000-0000-4000-8000-000000000001";
const LAMBDA: &str = "a3000000-0000-4000-8000-000000000002";
const KAPPA: &str = "a3000000-0000-4000-8000-000000000003";
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const BRAM: &str = "a1000000-0000-4000-8000-000000000002";
const GAMMA_EXTRA_MP: &str = "b0000000-0000-4000-8000-000000000001";
const IOTA_DANGLING_MP: &str = "b0000000-0000-4000-8000-0000000000df";
const CHAT_A: &str = "c1000000-0000-4000-8000-000000000001";
const CLOAK: &str = "aa000000-0000-4000-8000-000000000001";
const ENSEMBLE: &str = "aa000000-0000-4000-8000-000000000002";
const BG_FILE: &str = "f0000001-0000-4000-8000-000000000001";
const LAMBDA_FILE_1: &str = "f0000002-0000-4000-8000-000000000002";
const MISSING_FILE: &str = "f0000009-0000-4000-8000-000000000009";

fn http_for(kind: ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        ErrorKind::Internal => 500,
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/groups-projects.json")
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
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
fn blank_minted(v: &mut Value) {
    if let Value::Object(o) = v {
        for k in ["id", "createdAt", "updatedAt", "officialMountPointId"] {
            if o.contains_key(k) {
                o.insert(k.to_string(), Value::String(format!("<{k}>")));
            }
        }
        o.iter_mut().for_each(|(_, x)| blank_minted(x));
    } else if let Value::Array(a) = v {
        a.iter_mut().for_each(blank_minted);
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn norm_blanked(v: &Value) -> String {
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
fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-gp-proj-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("groups-projects-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("groups-projects-mount.db"), &mount).unwrap();
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

/// The project slim rows + chats/files projectId + project links, in the oracle's
/// `dumpProjectTables` shape.
fn dump_project_tables(db: &Db) -> Value {
    let main_part = db
        .read_main(|main| {
            let mut ps = main.prepare("SELECT id, name FROM projects ORDER BY id")?;
            let projects = ps
                .query_map([], |r| {
                    Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)? }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut cs = main.prepare("SELECT id, projectId FROM chats ORDER BY id")?;
            let chats = cs
                .query_map([], |r| {
                    Ok(json!({ "id": r.get::<_, String>(0)?, "projectId": r.get::<_, Option<String>>(1)? }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut fs = main.prepare("SELECT id, projectId FROM files ORDER BY id")?;
            let files = fs
                .query_map([], |r| {
                    Ok(json!({ "id": r.get::<_, String>(0)?, "projectId": r.get::<_, Option<String>>(1)? }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({ "projects": projects, "chats": chats, "files": files }))
        })
        .unwrap();
    let links = db
        .read_mount_index(|mount| {
            let mut ls = mount.prepare(
                "SELECT projectId, mountPointId FROM project_doc_mount_links ORDER BY projectId, mountPointId",
            )?;
            let rows: Vec<Value> = ls
                .query_map([], |r| {
                    Ok(json!({ "projectId": r.get::<_, String>(0)?, "mountPointId": r.get::<_, String>(1)? }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    json!({
        "projects": main_part["projects"],
        "chats": main_part["chats"],
        "files": main_part["files"],
        "links": links,
    })
}

#[test]
fn projects_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_PROJECTS_ROUTES") else {
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

    let check = |name: &str, got: &Value, blank: bool, failed: &mut Vec<String>| {
        let want = &oracle[name]["body"];
        let (g, w) = if blank {
            (norm_blanked(got), norm_blanked(want))
        } else {
            (norm(got), norm(want))
        };
        if g != w {
            eprintln!("[{name}] MISMATCH:\n{}", first_diff(&g, &w));
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };
    let check_tables = |name: &str, got: &Value, failed: &mut Vec<String>| {
        let want = &oracle[name]["tables"];
        if norm(got) != norm(want) {
            eprintln!(
                "[{name} tables] MISMATCH:\n{}",
                first_diff(&norm(got), &norm(want))
            );
            failed.push(format!("{name}_tables"));
        } else {
            eprintln!("[{name} tables] OK.");
        }
    };

    // --- Reads ---
    {
        let db = fresh_db(&spec, "list");
        check(
            "list",
            &response_data(&projects::project_list(&db)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "get_i");
        check(
            "get_iota",
            &response_data(&projects::project_get(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "get_k");
        check(
            "get_kappa",
            &response_data(&projects::project_get(&db, KAPPA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lc");
        check(
            "list_characters",
            &response_data(&projects::project_character_list(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lch");
        check(
            "list_chats",
            &response_data(&projects::project_chat_list(&db, IOTA, None, None)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lchp");
        check(
            "list_chats_page",
            &response_data(&projects::project_chat_list(&db, IOTA, Some(1), Some(0))),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gs");
        check(
            "get_state",
            &response_data(&projects::project_state_get(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "mpi");
        check(
            "mount_points_iota",
            &response_data(&projects::project_mount_point_list(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "mpk");
        check(
            "mount_points_kappa",
            &response_data(&projects::project_mount_point_list(&db, KAPPA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "bgi");
        check(
            "background_iota",
            &response_data(&projects::project_background_get(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "bgk");
        check(
            "background_kappa",
            &response_data(&projects::project_background_get(&db, KAPPA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "agl");
        check(
            "aesthetic_get_lantern",
            &response_data(&projects::project_aesthetic_get(&db, IOTA, "lantern")),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "aga");
        check(
            "aesthetic_get_aurora",
            &response_data(&projects::project_aesthetic_get(&db, IOTA, "aurora")),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "age");
        check(
            "aesthetic_get_empty",
            &response_data(&projects::project_aesthetic_get(&db, KAPPA, "lantern")),
            false,
            &mut failed,
        );
    }

    // --- Mutations ---
    {
        let db = fresh_db(&spec, "create");
        let resp = rt.block_on(projects::project_create(
            &db,
            json!({ "name": "Mu", "description": "A new project", "allowAnyCharacter": true, "characterRoster": [ARIA], "color": "#abcdef", "icon": "rocket" }),
        ));
        check("create", &response_data(&resp), true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "update");
        let resp = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "name": "Iota Renamed", "backgroundDisplayMode": "theme" }),
        ));
        check("update", &response_data(&resp), true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "delete");
        let resp = rt.block_on(projects::project_delete(&db, IOTA));
        check("delete", &response_data(&resp), false, &mut failed);
        check_tables("delete", &dump_project_tables(&db), &mut failed);
    }
    {
        let db = fresh_db(&spec, "addc");
        let resp = rt.block_on(projects::project_character_add(&db, KAPPA, BRAM));
        check("add_character", &response_data(&resp), false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "remc");
        let resp = rt.block_on(projects::project_character_remove(&db, IOTA, ARIA));
        check(
            "remove_character",
            &response_data(&resp),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "addch");
        let resp = rt.block_on(projects::project_chat_add(&db, KAPPA, CHAT_A));
        check("add_chat", &response_data(&resp), false, &mut failed);
        check_tables("add_chat", &dump_project_tables(&db), &mut failed);
    }
    {
        let db = fresh_db(&spec, "remch");
        let resp = rt.block_on(projects::project_chat_remove(&db, IOTA, CHAT_A));
        check("remove_chat", &response_data(&resp), false, &mut failed);
        check_tables("remove_chat", &dump_project_tables(&db), &mut failed);
    }
    {
        let db = fresh_db(&spec, "ss");
        let resp = rt.block_on(projects::project_state_set(
            &db,
            IOTA,
            json!({ "mood": "tense", "turns": 9 }),
        ));
        check("set_state", &response_data(&resp), false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "rs");
        let resp = rt.block_on(projects::project_state_reset(&db, IOTA));
        check("reset_state", &response_data(&resp), false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "ts");
        let resp = rt.block_on(projects::project_tool_settings_update(
            &db,
            IOTA,
            vec!["web_search".into()],
            vec!["danger".into()],
        ));
        check("tool_settings", &response_data(&resp), false, &mut failed);
    }
    {
        // aesthetic set (write) + GET readback (trimmed) — body {success, readback}.
        let db = fresh_db(&spec, "aset");
        let resp = rt.block_on(projects::project_aesthetic_set(
            &db,
            IOTA,
            "aurora",
            Some("  A brand new aurora palette.  ".into()),
        ));
        let mut body = response_data(&resp);
        let readback = response_data(&projects::project_aesthetic_get(&db, IOTA, "aurora"))
            .get("content")
            .cloned()
            .unwrap_or(Value::Null);
        if let Value::Object(o) = &mut body {
            o.insert("readback".into(), readback);
        }
        check("aesthetic_set", &body, false, &mut failed);
    }
    {
        // aesthetic clear (empty → delete) + GET readback ('').
        let db = fresh_db(&spec, "aclr");
        let resp = rt.block_on(projects::project_aesthetic_set(
            &db,
            IOTA,
            "lantern",
            Some("   ".into()),
        ));
        let mut body = response_data(&resp);
        let readback = response_data(&projects::project_aesthetic_get(&db, IOTA, "lantern"))
            .get("content")
            .cloned()
            .unwrap_or(Value::Null);
        if let Value::Object(o) = &mut body {
            o.insert("readback".into(), readback);
        }
        check("aesthetic_clear", &body, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "ml");
        let resp = rt.block_on(projects::project_mount_point_link(
            &db,
            IOTA,
            GAMMA_EXTRA_MP,
        ));
        check("mount_link", &response_data(&resp), true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "mu");
        let resp = rt.block_on(projects::project_mount_point_unlink(
            &db,
            IOTA,
            IOTA_DANGLING_MP,
        ));
        check("mount_unlink", &response_data(&resp), false, &mut failed);
        check_tables("mount_unlink", &dump_project_tables(&db), &mut failed);
    }

    // --- Wardrobe (Unit 4) ---
    {
        let db = fresh_db(&spec, "wl");
        check(
            "wardrobe_list",
            &response_data(&projects::project_wardrobe_list(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "wg");
        check(
            "wardrobe_get",
            &response_data(&projects::project_wardrobe_get(&db, IOTA, CLOAK)),
            false,
            &mut failed,
        );
    }
    {
        // create mints a fresh id + timestamps (both sides) → blank.
        let db = fresh_db(&spec, "wc");
        let resp = rt.block_on(projects::project_wardrobe_create(
            &db,
            IOTA,
            json!({ "title": "Rain Boots", "description": "For puddles.", "imagePrompt": "yellow rubber boots", "types": ["footwear"], "isDefault": false }),
        ));
        check("wardrobe_create", &response_data(&resp), true, &mut failed);
    }
    {
        // update mints a fresh updatedAt (both sides) → blank.
        let db = fresh_db(&spec, "wu");
        let resp = rt.block_on(projects::project_wardrobe_update(
            &db,
            IOTA,
            CLOAK,
            json!({ "title": "Weathered Cloak", "description": null }),
        ));
        check("wardrobe_update", &response_data(&resp), true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "wd");
        let resp = rt.block_on(projects::project_wardrobe_delete(&db, IOTA, ENSEMBLE));
        let mut body = response_data(&resp);
        let remaining: Vec<Value> = response_data(&projects::project_wardrobe_list(&db, IOTA))
            .get("wardrobeItems")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|i| {
                        json!({
                            "id": i.get("id").cloned().unwrap_or(Value::Null),
                            "title": i.get("title").cloned().unwrap_or(Value::Null),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Value::Object(o) = &mut body {
            o.insert("remaining".into(), Value::Array(remaining));
        }
        check("wardrobe_delete", &body, false, &mut failed);
    }

    // --- Files (Unit 5): list-files two-branch + add/remove ---
    let check_err = |name: &str, resp: &Response, failed: &mut Vec<String>| {
        let want = &oracle[name];
        let want_status = want["status"].as_i64().unwrap();
        let want_msg = want["body"]["error"].as_str().unwrap_or("");
        match resp {
            Response::Error(e) if http_for(e.kind) == want_status && e.message == want_msg => {
                eprintln!("[{name}] OK (err {want_status}).");
            }
            Response::Error(e) => {
                eprintln!(
                    "[{name}] ERR MISMATCH: got {}:'{}' want {want_status}:'{want_msg}'",
                    http_for(e.kind),
                    e.message
                );
                failed.push(name.to_string());
            }
            _ => {
                eprintln!("[{name}] expected error, got success");
                failed.push(name.to_string());
            }
        }
    };
    {
        // Branch A: Iota's primary store (baked/committed ts → no blank).
        let db = fresh_db(&spec, "lf_iota");
        check(
            "list_files_iota",
            &response_data(&projects::project_file_list(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    {
        // Branch B: Lambda's legacy files.
        let db = fresh_db(&spec, "lf_lambda");
        check(
            "list_files_lambda",
            &response_data(&projects::project_file_list(&db, LAMBDA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lf_kappa");
        check(
            "list_files_kappa",
            &response_data(&projects::project_file_list(&db, KAPPA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "add_file");
        let resp = rt.block_on(projects::project_file_add(&db, KAPPA, BG_FILE));
        check("add_file", &response_data(&resp), false, &mut failed);
        check_tables("add_file", &dump_project_tables(&db), &mut failed);
    }
    {
        let db = fresh_db(&spec, "add_file_miss");
        let resp = rt.block_on(projects::project_file_add(&db, KAPPA, MISSING_FILE));
        check_err("add_file_missing", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "rm_file");
        let resp = rt.block_on(projects::project_file_remove(&db, LAMBDA, LAMBDA_FILE_1));
        check("remove_file", &response_data(&resp), false, &mut failed);
        check_tables("remove_file", &dump_project_tables(&db), &mut failed);
    }

    assert!(failed.is_empty(), "projects-routes FAILED: {failed:?}");
}
