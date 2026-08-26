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
            &rt.block_on(groups::group_scenario_list(&db, GAMMA, false)),
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
        ));
        err("group_rename_conflict", &resp, &mut failed);
    }
    // --- Groups delete ---
    {
        let db = fresh_db(&spec, "g_delete");
        let resp = rt.block_on(groups::group_scenario_delete(
            &db,
            GAMMA,
            "interlude.md",
            false,
        ));
        ok("group_delete", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_delete_miss");
        let resp = rt.block_on(groups::group_scenario_delete(&db, GAMMA, "ghost.md", false));
        err("group_delete_missing", &resp, &mut failed);
    }
    // --- Participant-union ---
    {
        let db = fresh_db(&spec, "u_aria");
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![ARIA.to_string()],
            false,
        ));
        ok("union_aria", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_bram");
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![BRAM.to_string()],
            false,
        ));
        ok("union_bram", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_empty");
        let resp = rt.block_on(groups::group_scenarios_union(&db, vec![], false));
        ok("union_empty", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_unknown");
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![BOGUS.to_string()],
            false,
        ));
        ok("union_unknown", &resp, false, &mut failed);
    }

    // --- Projects (Iota: opening[default], climax) ---
    {
        let db = fresh_db(&spec, "p_list");
        ok(
            "project_list",
            &rt.block_on(projects::project_scenario_list(&db, IOTA, false)),
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
            false,
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
            false,
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
            false,
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
            false,
        ));
        err("project_rename_conflict", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_delete");
        let resp = rt.block_on(projects::project_scenario_delete(
            &db,
            IOTA,
            "climax.md",
            false,
        ));
        ok("project_delete", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_delete_miss");
        let resp = rt.block_on(projects::project_scenario_delete(
            &db, IOTA, "ghost.md", false,
        ));
        err("project_delete_missing", &resp, &mut failed);
    }

    // --- General (aurora[default] + dusk[default] → default-conflict) ---
    {
        let db = fresh_db(&spec, "gen_list");
        ok(
            "general_list",
            &rt.block_on(scenarios::scenario_list(&db, false)),
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
            false,
        ));
        ok("general_update", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_rename");
        let resp = rt.block_on(scenarios::scenario_rename(
            &db,
            "dusk.md".into(),
            "evening".into(),
            false,
        ));
        ok("general_rename", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_delete");
        let resp = rt.block_on(scenarios::scenario_delete(&db, "dusk.md".into(), false));
        ok("general_delete", &resp, false, &mut failed);
    }
    // --- General: pre-provision race arms ---
    {
        let db = fresh_db(&spec, "gen_list_unprov");
        unprovision_general(&db);
        ok(
            "general_list_unprov",
            &rt.block_on(scenarios::scenario_list(&db, false)),
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

    // ── P4.D120 / v4 `d25dacc1` — archived scenarios ─────────────────────────
    // Every case seeds its own file on the FRESH copy through the RAW
    // document-store write (never the route under test); the committed
    // `groups-projects-*` pair is untouched, so no sibling family is disturbed.
    const BEACON: &str = "a2000000-0000-4000-8000-000000000003";
    const ARCHIVED_FILE: &str = "---\nname: Mothballed\narchived: true\n---\n\nA scene put away.";

    /// label → the store the case seeds into, resolved exactly as the oracle does.
    fn scope_mount(db: &Db, scope: &str) -> String {
        db.read_main(|conn| {
            let one = |sql: &str, id: &str| -> Option<String> {
                conn.query_row(sql, [id], |r| r.get::<_, Option<String>>(0))
                    .ok()
                    .flatten()
            };
            Ok(match scope {
                "general" => {
                    quilltap_core::db::instance_settings::get_general_mount_point_id(conn)?
                        .expect("general mount unprovisioned")
                }
                "group" => one(
                    "SELECT officialMountPointId FROM groups WHERE id = ?1",
                    GAMMA,
                )
                .expect("no official store for group"),
                "beacon" => one(
                    "SELECT officialMountPointId FROM groups WHERE id = ?1",
                    BEACON,
                )
                .expect("no official store for beacon"),
                "project" => one(
                    "SELECT officialMountPointId FROM projects WHERE id = ?1",
                    IOTA,
                )
                .expect("no official store for project"),
                other => panic!("unknown seed scope {other}"),
            })
        })
        .expect("resolve scope mount")
    }

    fn seed_scenario(db: &Db, scope: &str, file: &str, content: &str) {
        let mp = scope_mount(db, scope);
        let rel = format!("Scenarios/{file}");
        let content = content.to_string();
        db.write_blocking(move |w| {
            let links = w
                .mount_index()
                .expect("mount writer")
                .doc_mount_file_links();
            links.ensure_folder_path(&mp, "Scenarios")?;
            links.write_database_document(&mp, &rel, &content)?;
            Ok(())
        })
        .expect("seed scenario");
    }

    // The BYTES on disk after the case — v4 emits them as `fileBytes`, so a
    // write is pinned by its frontmatter key order, not just by the body.
    let bytes_match = |name: &str, db: &Db, scope: &str, file: &str, failed: &mut Vec<String>| {
        let Some(want) = oracle.get(name).and_then(|c| c.get("fileBytes")) else {
            return;
        };
        let mp = scope_mount(db, scope);
        let rel = format!("Scenarios/{file}");
        let got = db
            .read_mount_index(|conn| {
                quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository::new(conn)
                    .find_by_mount_point_and_path(&mp, &rel)
            })
            .expect("read back");
        let got = match got {
            Some(c) => Value::String(c),
            None => Value::Null,
        };
        if &got != want {
            eprintln!("[{name}] FILE BYTES MISMATCH:\n  rust:   {got}\n  oracle: {want}");
            failed.push(format!("{name}(bytes)"));
        } else {
            eprintln!("[{name}] file bytes OK.");
        }
    };

    // group list arms
    for (name, include_archived, seed) in [
        ("group_list_hides_archived", false, ("group", "mothballed.md", ARCHIVED_FILE)),
        ("group_list_shows_archived_with_the_flag", true, ("group", "mothballed.md", ARCHIVED_FILE)),
        // ⚠ The bare `?includeArchived` spelling and the rejected `=1` spelling
        // live at v4's URL reader. v5's scenario surfaces are dispatch verbs, so
        // the SPELLING is not reachable here; what IS ported is the resolved
        // boolean, and the two rows below drive it directly. The spelling itself
        // is pinned at the two wardrobe REST edges that exist
        // (`quilltap-web::wardrobe_routes::read_include_archived`).
        ("group_list_bare_include_archived_spelling", true, ("group", "mothballed.md", ARCHIVED_FILE)),
        ("group_list_rejects_other_include_archived_spellings", false, ("group", "mothballed.md", ARCHIVED_FILE)),
        (
            "group_list_string_true_frontmatter_is_archived",
            false,
            ("group", "stringy.md", "---\nname: Stringy\narchived: \"true\"\n---\n\nA quoted flag."),
        ),
        (
            "group_list_other_archived_values_are_active",
            false,
            ("group", "yesish.md", "---\nname: Yesish\narchived: yes\n---\n\nNot the flag."),
        ),
        (
            "group_list_archived_cannot_win_the_default",
            true,
            (
                "group",
                "aardvark.md",
                "---\nname: Aardvark\nisDefault: true\narchived: true\n---\n\nFirst alphabetically.",
            ),
        ),
    ] {
        let db = fresh_db(&spec, name);
        seed_scenario(&db, seed.0, seed.1, seed.2);
        ok(
            name,
            &rt.block_on(groups::group_scenario_list(&db, GAMMA, include_archived)),
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_create_archived");
        let resp = rt.block_on(groups::group_scenario_create(
            &db,
            GAMMA,
            json!({
                "filename": "Put Away", "name": "Put Away",
                "description": "Filed for later.", "archived": true, "body": "Stored."
            }),
        ));
        ok(
            "group_create_archived_writes_the_flag",
            &resp,
            true,
            &mut failed,
        );
        bytes_match(
            "group_create_archived_writes_the_flag",
            &db,
            "group",
            "Put Away.md",
            &mut failed,
        );
    }
    {
        // The collection-POST quirk: the fresh list is refreshed with the BODY's
        // `archived`, not the query param — so an ACTIVE create returns an
        // archived-free list even with "Show archived" ticked.
        let db = fresh_db(&spec, "g_create_active_flag");
        seed_scenario(&db, "group", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(groups::group_scenario_create(
            &db,
            GAMMA,
            json!({ "filename": "Still Here", "body": "Active." }),
        ));
        ok(
            "group_create_active_ignores_the_query_flag",
            &resp,
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_update_preserve");
        seed_scenario(&db, "group", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(groups::group_scenario_update(
            &db,
            GAMMA,
            "mothballed.md",
            json!({ "name": "Mothballed", "body": "Edited while archived." }),
            true,
        ));
        ok(
            "group_update_omitting_archived_preserves_it",
            &resp,
            true,
            &mut failed,
        );
        bytes_match(
            "group_update_omitting_archived_preserves_it",
            &db,
            "group",
            "mothballed.md",
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_update_restore");
        seed_scenario(&db, "group", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(groups::group_scenario_update(
            &db,
            GAMMA,
            "mothballed.md",
            json!({ "name": "Mothballed", "archived": false, "body": "Back in play." }),
            false,
        ));
        ok(
            "group_update_restoring_deletes_the_key",
            &resp,
            true,
            &mut failed,
        );
        bytes_match(
            "group_update_restoring_deletes_the_key",
            &db,
            "group",
            "mothballed.md",
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_update_archive");
        let resp = rt.block_on(groups::group_scenario_update(
            &db,
            GAMMA,
            "interlude.md",
            json!({ "name": "Interlude", "archived": true, "body": "Put away." }),
            false,
        ));
        ok(
            "group_update_archiving_an_active_scenario",
            &resp,
            true,
            &mut failed,
        );
        bytes_match(
            "group_update_archiving_an_active_scenario",
            &db,
            "group",
            "interlude.md",
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_delete_flag");
        seed_scenario(&db, "group", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(groups::group_scenario_delete(
            &db,
            GAMMA,
            "interlude.md",
            true,
        ));
        ok(
            "group_delete_fresh_list_honours_the_flag",
            &resp,
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_rename_flag");
        seed_scenario(&db, "group", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(groups::group_scenario_rename(
            &db,
            GAMMA,
            "interlude.md",
            "interlude-3",
            true,
        ));
        ok(
            "group_rename_fresh_list_honours_the_flag",
            &resp,
            true,
            &mut failed,
        );
    }
    for (name, include_archived) in [
        ("union_honours_the_flag", true),
        ("union_hides_archived_by_default", false),
    ] {
        let db = fresh_db(&spec, name);
        seed_scenario(&db, "beacon", "mothballed.md", ARCHIVED_FILE);
        ok(
            name,
            &rt.block_on(groups::group_scenarios_union(
                &db,
                vec![ARIA.to_string()],
                include_archived,
            )),
            true,
            &mut failed,
        );
    }
    for (name, include_archived) in [
        ("project_list_hides_archived", false),
        ("project_list_shows_archived_with_the_flag", true),
    ] {
        let db = fresh_db(&spec, name);
        seed_scenario(&db, "project", "mothballed.md", ARCHIVED_FILE);
        ok(
            name,
            &rt.block_on(projects::project_scenario_list(&db, IOTA, include_archived)),
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_update_preserve");
        seed_scenario(&db, "project", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(projects::project_scenario_update(
            &db,
            IOTA,
            "mothballed.md",
            json!({ "name": "Mothballed", "body": "Edited while archived." }),
            true,
        ));
        ok(
            "project_update_omitting_archived_preserves_it",
            &resp,
            true,
            &mut failed,
        );
        bytes_match(
            "project_update_omitting_archived_preserves_it",
            &db,
            "project",
            "mothballed.md",
            &mut failed,
        );
    }
    for (name, include_archived) in [
        ("general_list_hides_archived", false),
        ("general_list_shows_archived_with_the_flag", true),
    ] {
        let db = fresh_db(&spec, name);
        seed_scenario(&db, "general", "mothballed.md", ARCHIVED_FILE);
        ok(
            name,
            &rt.block_on(scenarios::scenario_list(&db, include_archived)),
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_create_archived");
        let resp = rt.block_on(scenarios::scenario_create(
            &db,
            json!({ "filename": "Put Away", "name": "Put Away", "archived": true, "body": "Stored." }),
        ));
        ok(
            "general_create_archived_writes_the_flag",
            &resp,
            true,
            &mut failed,
        );
        bytes_match(
            "general_create_archived_writes_the_flag",
            &db,
            "general",
            "Put Away.md",
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_update_preserve");
        seed_scenario(&db, "general", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(scenarios::scenario_update(
            &db,
            "mothballed.md".into(),
            json!({ "name": "Mothballed", "body": "Edited while archived." }),
            true,
        ));
        ok(
            "general_update_omitting_archived_preserves_it",
            &resp,
            true,
            &mut failed,
        );
        bytes_match(
            "general_update_omitting_archived_preserves_it",
            &db,
            "general",
            "mothballed.md",
            &mut failed,
        );
    }
    // ── The explicit-null / wrong-type `archived` refusals (unify §3): Zod's
    // `.optional()` accepts an ABSENT key, never null — both answer v4's 400
    // with Zod 4's own sentence and WRITE NOTHING (the bytes leg pins the
    // pre-fix silent-keep, which answered 200 and preserved the stored flag).
    {
        let db = fresh_db(&spec, "gen_update_null_arch");
        seed_scenario(&db, "general", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(scenarios::scenario_update(
            &db,
            "mothballed.md".into(),
            json!({ "name": "Mothballed", "body": "Never lands.", "archived": null }),
            false,
        ));
        err("general_update_null_archived_refuses", &resp, &mut failed);
        bytes_match(
            "general_update_null_archived_refuses",
            &db,
            "general",
            "mothballed.md",
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_update_str_arch");
        seed_scenario(&db, "general", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(scenarios::scenario_update(
            &db,
            "mothballed.md".into(),
            json!({ "name": "Mothballed", "body": "Never lands.", "archived": "yes" }),
            false,
        ));
        err("general_update_string_archived_refuses", &resp, &mut failed);
        bytes_match(
            "general_update_string_archived_refuses",
            &db,
            "general",
            "mothballed.md",
            &mut failed,
        );
    }

    assert!(failed.is_empty(), "scenarios-routes FAILED: {failed:?}");
}
