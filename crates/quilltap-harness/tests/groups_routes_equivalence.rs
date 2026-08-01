//! P4.6k GROUPS route-surface differential: `api::groups::*` vs v4's REAL groups
//! route handlers. Both sides read a FRESH copy of the committed groups-projects
//! fixture (baked ids identical → no remap, except the CREATE case which mints
//! fresh ids — those keys are blanked). Reads carry the full body; mutations
//! carry the body + a post-op table dump (matching the oracle's shape).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-groups-routes.ndjson npx jest -- groups-routes
//! Run:
//!   QT_ORACLE_GROUPS_ROUTES=/tmp/oracle-groups-routes.ndjson \
//!     cargo test -p quilltap-harness --test groups_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::groups;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::groups::find_official_mount_point_id_raw;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const GAMMA: &str = "a2000000-0000-4000-8000-000000000001";
const DELTA: &str = "a2000000-0000-4000-8000-000000000002";
const BRAM: &str = "a1000000-0000-4000-8000-000000000002";
const CLEO: &str = "a1000000-0000-4000-8000-000000000003";
const GAMMA_EXTRA_MP: &str = "b0000000-0000-4000-8000-000000000001";

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
/// Blank the CREATE-minted keys (group id + timestamps + the store FK).
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
    let scratch = std::env::temp_dir().join(format!("qt-gp-groups-{}-{}", tag, std::process::id()));
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

/// The group slim rows + memberships + links + mount-point names, in the oracle's
/// `dumpGroupTables` shape.
fn dump_group_tables(db: &Db) -> Value {
    let groups = db
        .read_main(|main| {
            let mut stmt = main.prepare("SELECT id, name FROM groups ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)? }))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    db.read_mount_index(|mount| {
        let mut mstmt = mount.prepare(
            "SELECT groupId, characterId FROM group_character_members ORDER BY groupId, characterId",
        )?;
        let members = mstmt
            .query_map([], |r| {
                Ok(json!({ "groupId": r.get::<_, String>(0)?, "characterId": r.get::<_, String>(1)? }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut lstmt = mount.prepare(
            "SELECT groupId, mountPointId FROM group_doc_mount_links ORDER BY groupId, mountPointId",
        )?;
        let links = lstmt
            .query_map([], |r| {
                Ok(json!({ "groupId": r.get::<_, String>(0)?, "mountPointId": r.get::<_, String>(1)? }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut nstmt = mount.prepare("SELECT name FROM doc_mount_points ORDER BY name")?;
        let names = nstmt
            .query_map([], |r| Ok(json!({ "name": r.get::<_, String>(0)? })))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({
            "groups": groups,
            "members": members,
            "links": links,
            "mountPointNames": names,
        }))
    })
    .unwrap()
}

/// The created group's official-store folder paths (the folder-ensure side effect).
fn dump_created_folders(db: &Db, mount_point_id: &str) -> Value {
    let mp = mount_point_id.to_string();
    db.read_mount_index(move |mount| {
        let mut stmt = mount
            .prepare("SELECT path FROM doc_mount_folders WHERE mountPointId = ?1 ORDER BY path")?;
        let rows = stmt
            .query_map(rusqlite::params![mp], |r| {
                Ok(json!({ "path": r.get::<_, String>(0)? }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "folders": rows }))
    })
    .unwrap()
}

#[test]
fn groups_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_GROUPS_ROUTES") else {
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

    // A read/simple body check (baked ids identical → no blanking).
    let check = |name: &str, got: &Value, failed: &mut Vec<String>| {
        let want = &oracle[name]["body"];
        if norm(got) != norm(want) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm(got), &norm(want))
            );
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
            &response_data(&groups::group_list(&db)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "get_g");
        check(
            "get_gamma",
            &response_data(&groups::group_get(&db, GAMMA)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "get_d");
        check(
            "get_delta",
            &response_data(&groups::group_get(&db, DELTA)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "mem_g");
        check(
            "members_gamma",
            &response_data(&groups::group_members(&db, GAMMA)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "mem_d");
        check(
            "members_delta",
            &response_data(&groups::group_members(&db, DELTA)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "mp_g");
        check(
            "mount_points_gamma",
            &response_data(&groups::group_mount_point_list(&db, GAMMA)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "mp_d");
        check(
            "mount_points_delta",
            &response_data(&groups::group_mount_point_list(&db, DELTA)),
            &mut failed,
        );
    }

    // --- Mutations ---
    {
        // create — minted ids blanked on both sides; folder-ensure dumped.
        let db = fresh_db(&spec, "create");
        let resp = rt.block_on(groups::group_create(
            &db,
            "Epsilon".into(),
            Some("A new group".into()),
            Some("#abcdef".into()),
            Some("gear".into()),
        ));
        let got = response_data(&resp);
        let want = &oracle["create"]["body"];
        if norm_blanked(&got) != norm_blanked(want) {
            eprintln!(
                "[create] MISMATCH:\n{}",
                first_diff(&norm_blanked(&got), &norm_blanked(want))
            );
            failed.push("create".into());
        } else {
            eprintln!("[create] OK.");
        }
        let mp = got
            .get("group")
            .and_then(|g| g.get("officialMountPointId"))
            .and_then(Value::as_str)
            .expect("created mount fk");
        check_tables("create", &dump_created_folders(&db, mp), &mut failed);
    }
    {
        let db = fresh_db(&spec, "update");
        let resp = rt.block_on(groups::group_update(
            &db,
            GAMMA,
            json!({ "name": "Gamma Renamed", "color": "#010203" }),
        ));
        // update mints a fresh updatedAt on both sides → blank it.
        let got = response_data(&resp);
        let want = &oracle["update"]["body"];
        if norm_blanked(&got) != norm_blanked(want) {
            eprintln!(
                "[update] MISMATCH:\n{}",
                first_diff(&norm_blanked(&got), &norm_blanked(want))
            );
            failed.push("update".into());
        } else {
            eprintln!("[update] OK.");
        }
    }
    {
        let db = fresh_db(&spec, "delete");
        let resp = rt.block_on(groups::group_delete(&db, GAMMA));
        check("delete", &response_data(&resp), &mut failed);
        check_tables("delete", &dump_group_tables(&db), &mut failed);
    }
    {
        let db = fresh_db(&spec, "addm");
        let resp = rt.block_on(groups::group_member_add(&db, GAMMA, CLEO));
        check("add_member", &response_data(&resp), &mut failed);
        check_tables("add_member", &dump_group_tables(&db), &mut failed);
    }
    {
        let db = fresh_db(&spec, "remm");
        let resp = rt.block_on(groups::group_member_remove(&db, GAMMA, BRAM));
        check("remove_member", &response_data(&resp), &mut failed);
        check_tables("remove_member", &dump_group_tables(&db), &mut failed);
    }
    {
        let db = fresh_db(&spec, "mlink");
        let resp = rt.block_on(groups::group_mount_point_link(&db, GAMMA, GAMMA_EXTRA_MP));
        check("mount_link", &response_data(&resp), &mut failed);
    }
    {
        let db = fresh_db(&spec, "munlink");
        let resp = rt.block_on(groups::group_mount_point_unlink(&db, GAMMA, GAMMA_EXTRA_MP));
        check("mount_unlink", &response_data(&resp), &mut failed);
        check_tables("mount_unlink", &dump_group_tables(&db), &mut failed);
    }

    // --- P4.23: the corrupted-store 503 envelope arm ---
    // Malformed bytes planted through the REAL write_database_document; the
    // GET's hydrating find_by_id refuses and the api layer answers v4's
    // deliberate contextful 503. Status AND body byte-compare against v4's
    // REAL route (the middleware envelope, context.ts:176-205) — the body via
    // raw to_string so KEY ORDER is pinned too (preserve_order both sides).
    // Mutation-proven: collapsing `overlay_to_db` back to `DbError::Key`
    // reds this arm on kind AND body.
    {
        let db = fresh_db(&spec, "corrupt");
        let mp = db
            .read_main(|main| find_official_mount_point_id_raw(main, GAMMA))
            .expect("read gamma store fk")
            .flatten()
            .expect("gamma has an officialMountPointId");
        rt.block_on(db.write(move |w| {
            let mount = w
                .mount_index()
                .expect("fixture has a mount-index partition");
            DocMountFileLinksRepository::new(mount.connection()).write_database_document(
                &mp,
                "properties.json",
                "{",
            )?;
            Ok(())
        }))
        .expect("plant corrupt properties.json");

        let want = &oracle["get_store_corrupt"];
        assert_eq!(
            want["status"].as_i64(),
            Some(503),
            "oracle corrupt arm did not answer 503 — did v4's middleware envelope move?"
        );
        match groups::group_get(&db, GAMMA) {
            Response::Error(e) => {
                if !matches!(e.kind, ErrorKind::Unavailable) {
                    eprintln!(
                        "[get_store_corrupt] kind {:?} (want Unavailable / HTTP 503)",
                        e.kind
                    );
                    failed.push("get_store_corrupt_kind".into());
                }
                match e.unavailable_wire_body() {
                    Some(got_body) => {
                        let got = serde_json::to_string(&got_body).unwrap();
                        let want_body = serde_json::to_string(&want["body"]).unwrap();
                        if got != want_body {
                            eprintln!(
                                "[get_store_corrupt] MISMATCH:\n  GOT : {got}\n  WANT: {want_body}"
                            );
                            failed.push("get_store_corrupt".into());
                        } else {
                            eprintln!("[get_store_corrupt] OK.");
                        }
                    }
                    None => {
                        eprintln!(
                            "[get_store_corrupt] refusal carries no entity (wire body absent)"
                        );
                        failed.push("get_store_corrupt_body".into());
                    }
                }
            }
            other => {
                eprintln!(
                    "[get_store_corrupt] expected the 503 refusal, got: {}",
                    serde_json::to_string(&other).unwrap_or_default()
                );
                failed.push("get_store_corrupt_not_error".into());
            }
        }
    }

    assert!(failed.is_empty(), "groups-routes FAILED: {failed:?}");
}
