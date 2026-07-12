//! P4.6n SCENARIOS route-surface differential: `api::groups`/`api::projects`/
//! `api::scenarios` scenario handlers vs v4's REAL scenario route handlers. Both
//! sides read a FRESH copy of the committed groups-projects fixture per case
//! (baked ids/timestamps identical → no remap on reads). Create/update mint fresh
//! document timestamps → those (`lastModified`/`createdAt`/`updatedAt`) are blanked
//! on both sides. Error cases assert the Error kind's HTTP status + message match
//! v4's `{status, body:{error}}`.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-scenarios-routes.ndjson npx jest -- scenarios-routes
//! Run:
//!   QT_ORACLE_SCENARIOS_ROUTES=/tmp/oracle-scenarios-routes.ndjson \
//!     cargo test -p quilltap-harness --test scenarios_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::api::{groups, projects, scenarios};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const BRAM: &str = "a1000000-0000-4000-8000-000000000002";
const GAMMA: &str = "a2000000-0000-4000-8000-000000000001";
const IOTA: &str = "a3000000-0000-4000-8000-000000000001";
const BOGUS: &str = "a1000000-0000-4000-8000-0000000000ff";

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
/// Blank the mint-time document timestamps on any scenario object (create/update
/// rewrite files → fresh wall-clock ts that differ v4-vs-v5).
fn blank_ts(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for k in ["lastModified", "createdAt", "updatedAt"] {
                if o.contains_key(k) {
                    o.insert(k.to_string(), Value::String(format!("<{k}>")));
                }
            }
            o.iter_mut().for_each(|(_, x)| blank_ts(x));
        }
        Value::Array(a) => a.iter_mut().for_each(blank_ts),
        _ => {}
    }
}
fn norm(v: &Value, blank: bool) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    if blank {
        blank_ts(&mut v);
    }
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
fn http_for(kind: ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::NotFound => 404,
        ErrorKind::Locked => 503,
        ErrorKind::Internal => 500,
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-scenarios-{}-{}", tag, std::process::id()));
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

/// Delete the general mount pointer to exercise the pre-provision race arms
/// (both differential sides drop `instance_settings.generalMountPointId`).
fn unprovision_general(db: &Db) {
    db.write_blocking(|ws| {
        ws.main().connection().execute(
            "DELETE FROM \"instance_settings\" WHERE \"key\" = 'generalMountPointId'",
            [],
        )?;
        Ok(())
    })
    .expect("unprovision general");
}

#[test]
fn scenarios_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SCENARIOS_ROUTES") else {
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

    // Success case: compare response_data (optionally ts-blanked) to oracle body.
    let ok = |name: &str, resp: &Response, blank: bool, failed: &mut Vec<String>| {
        if let Response::Error(e) = resp {
            eprintln!(
                "[{name}] expected success, got error {:?}: {}",
                e.kind, e.message
            );
            failed.push(name.to_string());
            return;
        }
        let want = &oracle[name]["body"];
        let got = response_data(resp);
        if norm(&got, blank) != norm(want, blank) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm(&got, blank), &norm(want, blank))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };
    // Error case: compare the Error kind's HTTP status + message.
    let err = |name: &str, resp: &Response, failed: &mut Vec<String>| {
        let want = &oracle[name];
        let want_status = want["status"].as_i64().unwrap();
        let want_msg = want["body"]["error"].as_str().unwrap_or("");
        match resp {
            Response::Error(e) => {
                if http_for(e.kind) != want_status || e.message != want_msg {
                    eprintln!(
                        "[{name}] ERR MISMATCH: got {}/{:?}:'{}' want {}/'{}'",
                        http_for(e.kind),
                        e.kind,
                        e.message,
                        want_status,
                        want_msg
                    );
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK (err {want_status}).");
                }
            }
            _ => {
                eprintln!("[{name}] expected error {want_status}, got success");
                failed.push(name.to_string());
            }
        }
    };

    // --- Groups reads ---
    {
        let db = fresh_db(&spec, "g_list");
        ok(
            "group_list",
            &rt.block_on(groups::group_scenario_list(&db, GAMMA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_get");
        ok(
            "group_get",
            &rt.block_on(groups::group_scenario_get(&db, GAMMA, "prologue.md")),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_get_miss");
        err(
            "group_get_missing",
            &rt.block_on(groups::group_scenario_get(&db, GAMMA, "ghost.md")),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_get_nest");
        err(
            "group_get_nested",
            &rt.block_on(groups::group_scenario_get(&db, GAMMA, "sub/deep.md")),
            &mut failed,
        );
    }
    // --- Groups create ---
    {
        let db = fresh_db(&spec, "g_create");
        let resp = rt.block_on(groups::group_scenario_create(
            &db,
            GAMMA,
            json!({ "filename": "Fresh: Take", "name": "Fresh Take", "isDefault": true, "body": "Anew." }),
        ));
        ok("group_create", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_collide");
        let resp = rt.block_on(groups::group_scenario_create(
            &db,
            GAMMA,
            json!({ "filename": "prologue", "body": "dup" }),
        ));
        err("group_create_collision", &resp, &mut failed);
    }
    // --- Groups update ---
    {
        let db = fresh_db(&spec, "g_update");
        let resp = rt.block_on(groups::group_scenario_update(
            &db,
            GAMMA,
            "interlude.md",
            json!({ "name": "Interlude II", "body": "Changed." }),
        ));
        ok("group_update", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_update_empty");
        let resp = rt.block_on(groups::group_scenario_update(
            &db,
            GAMMA,
            "interlude.md",
            json!({ "body": "" }),
        ));
        err("group_update_emptybody", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_update_miss");
        let resp = rt.block_on(groups::group_scenario_update(
            &db,
            GAMMA,
            "ghost.md",
            json!({ "body": "x" }),
        ));
        err("group_update_missing", &resp, &mut failed);
    }
    // --- Groups rename (no ts mint → no blank) ---
    {
        let db = fresh_db(&spec, "g_rename");
        let resp = rt.block_on(groups::group_scenario_rename(
            &db,
            GAMMA,
            "interlude.md",
            "interlude-2",
        ));
        ok("group_rename", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_rename_noop");
        let resp = rt.block_on(groups::group_scenario_rename(
            &db,
            GAMMA,
            "interlude.md",
            "interlude",
        ));
        ok("group_rename_noop", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_rename_conf");
        let resp = rt.block_on(groups::group_scenario_rename(
            &db,
            GAMMA,
            "interlude.md",
            "prologue",
        ));
        err("group_rename_conflict", &resp, &mut failed);
    }
    // --- Groups delete ---
    {
        let db = fresh_db(&spec, "g_delete");
        let resp = rt.block_on(groups::group_scenario_delete(&db, GAMMA, "interlude.md"));
        ok("group_delete", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_delete_miss");
        let resp = rt.block_on(groups::group_scenario_delete(&db, GAMMA, "ghost.md"));
        err("group_delete_missing", &resp, &mut failed);
    }
    // --- Participant-union ---
    {
        let db = fresh_db(&spec, "u_aria");
        let resp = rt.block_on(groups::group_scenarios_union(&db, vec![ARIA.to_string()]));
        ok("union_aria", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_bram");
        let resp = rt.block_on(groups::group_scenarios_union(&db, vec![BRAM.to_string()]));
        ok("union_bram", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_empty");
        let resp = rt.block_on(groups::group_scenarios_union(&db, vec![]));
        ok("union_empty", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_unknown");
        let resp = rt.block_on(groups::group_scenarios_union(&db, vec![BOGUS.to_string()]));
        ok("union_unknown", &resp, false, &mut failed);
    }

    // --- Projects (Iota: opening[default], climax) ---
    {
        let db = fresh_db(&spec, "p_list");
        ok(
            "project_list",
            &rt.block_on(projects::project_scenario_list(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_get");
        ok(
            "project_get",
            &rt.block_on(projects::project_scenario_get(&db, IOTA, "opening.md")),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_get_miss");
        err(
            "project_get_missing",
            &rt.block_on(projects::project_scenario_get(&db, IOTA, "ghost.md")),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_get_nest");
        err(
            "project_get_nested",
            &rt.block_on(projects::project_scenario_get(&db, IOTA, "sub/deep.md")),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_create");
        let resp = rt.block_on(projects::project_scenario_create(
            &db,
            IOTA,
            json!({ "filename": "Act Two", "description": "The middle", "isDefault": true, "body": "It deepens." }),
        ));
        ok("project_create", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_collide");
        let resp = rt.block_on(projects::project_scenario_create(
            &db,
            IOTA,
            json!({ "filename": "opening", "body": "dup" }),
        ));
        err("project_create_collision", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_update");
        let resp = rt.block_on(projects::project_scenario_update(
            &db,
            IOTA,
            "climax.md",
            json!({ "name": "Climax!", "body": "Peak." }),
        ));
        ok("project_update", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_update_empty");
        let resp = rt.block_on(projects::project_scenario_update(
            &db,
            IOTA,
            "climax.md",
            json!({ "body": "" }),
        ));
        err("project_update_emptybody", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_rename");
        let resp = rt.block_on(projects::project_scenario_rename(
            &db,
            IOTA,
            "climax.md",
            "climax-2",
        ));
        ok("project_rename", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_rename_conf");
        let resp = rt.block_on(projects::project_scenario_rename(
            &db,
            IOTA,
            "climax.md",
            "opening",
        ));
        err("project_rename_conflict", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_delete");
        let resp = rt.block_on(projects::project_scenario_delete(&db, IOTA, "climax.md"));
        ok("project_delete", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_delete_miss");
        let resp = rt.block_on(projects::project_scenario_delete(&db, IOTA, "ghost.md"));
        err("project_delete_missing", &resp, &mut failed);
    }

    // --- General (aurora[default] + dusk[default] → default-conflict) ---
    {
        let db = fresh_db(&spec, "gen_list");
        ok(
            "general_list",
            &rt.block_on(scenarios::scenario_list(&db)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_get");
        ok(
            "general_get",
            &rt.block_on(scenarios::scenario_get(&db, "aurora.md".into())),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_get_miss");
        err(
            "general_get_missing",
            &rt.block_on(scenarios::scenario_get(&db, "ghost.md".into())),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_create");
        let resp = rt.block_on(scenarios::scenario_create(
            &db,
            json!({ "filename": "Twilight", "isDefault": true, "body": "Between." }),
        ));
        ok("general_create", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_collide");
        let resp = rt.block_on(scenarios::scenario_create(
            &db,
            json!({ "filename": "aurora", "body": "dup" }),
        ));
        err("general_create_collision", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_update");
        let resp = rt.block_on(scenarios::scenario_update(
            &db,
            "aurora.md".into(),
            json!({ "name": "Aurora II", "body": "Brighter." }),
        ));
        ok("general_update", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_rename");
        let resp = rt.block_on(scenarios::scenario_rename(
            &db,
            "dusk.md".into(),
            "evening".into(),
        ));
        ok("general_rename", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_delete");
        let resp = rt.block_on(scenarios::scenario_delete(&db, "dusk.md".into()));
        ok("general_delete", &resp, false, &mut failed);
    }
    // --- General: pre-provision race arms ---
    {
        let db = fresh_db(&spec, "gen_list_unprov");
        unprovision_general(&db);
        ok(
            "general_list_unprov",
            &rt.block_on(scenarios::scenario_list(&db)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_create_unprov");
        unprovision_general(&db);
        let resp = rt.block_on(scenarios::scenario_create(
            &db,
            json!({ "filename": "x", "body": "y" }),
        ));
        err("general_create_unprov", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_get_unprov");
        unprovision_general(&db);
        let resp = rt.block_on(scenarios::scenario_get(&db, "aurora.md".into()));
        err("general_get_unprov", &resp, &mut failed);
    }

    assert!(failed.is_empty(), "scenarios-routes FAILED: {failed:?}");
}
