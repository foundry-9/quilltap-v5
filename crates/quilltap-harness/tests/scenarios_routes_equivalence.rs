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
    // P4.D295: bodies compare in KEY ORDER (v4's object-literal order is part
    // of the wire contract; `serde_json` map equality would be order-blind).
    serde_json::to_string_pretty(&v).unwrap()
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

/// A fixture copy in a scratch dir removed on drop (the `Db` drops first).
struct ScratchDb {
    db: Db,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for ScratchDb {
    type Target = Db;
    fn deref(&self) -> &Db {
        &self.db
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> ScratchDb {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-scenarios-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("groups-projects-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("groups-projects-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    // Nothing the open logs belongs to the case.
    let _ = take_lines();
    ScratchDb { db, _dir: scratch }
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

/// [P4.D295] The scenario lines a case logs — the v4 case captures the same
/// set (see its header).
const LINE_PREFIXES: &[&str] = &[
    "[ScenarioLocation] ",
    "[ScenariosEverywhere] ",
    "[General v1] ",
    "[Projects v1] ",
    "[Groups v1] ",
    "Scenario file body is empty; skipping",
    "Scenarios: multiple isDefault: true entries",
    "Failed to demote a previously-default scenario",
];

/// The rows whose v4 route param the oracle HTTP-ENCODES (L-6: v4's factory
/// decodes the route segment once more than v5's dispatch path needs). Their
/// L17 `scenarioPath` field is v4's raw route param — the ENCODED string —
/// where v5 logs the logical path, so the comparison encodes v5's field.
const ENCODED_PARAM_ROWS: &[&str] = &[
    "project_get_malformed_percent_500",
    "project_put_malformed_percent_500",
];

/// A PROCESS-GLOBAL capture (the `system_restore_state` precedent): every
/// handler here runs inside `with_both_conns`, on the WRITER thread, where a
/// thread-scoped subscriber sees nothing. Installed once per binary.
fn global_capture() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    static BUF: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<Vec<String>>>> =
        std::sync::OnceLock::new();
    BUF.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry()
            .with(quilltap_core::test_support::CaptureLayer(buf.clone()));
        tracing::subscriber::set_global_default(subscriber).expect("one global capture per binary");
        buf
    })
    .clone()
}

/// The scenario lines logged since the last call, `LEVEL message k=v` (the
/// v5 target dropped), and the buffer cleared.
fn take_lines() -> Vec<String> {
    let buf = global_capture();
    let mut b = buf.lock().unwrap();
    let out = b
        .iter()
        .filter_map(|l| {
            let (level, rest) = l.split_once(' ')?;
            let (_target, rest) = rest.split_once(' ')?;
            LINE_PREFIXES
                .iter()
                .any(|p| rest.starts_with(p))
                .then(|| format!("{level} {rest}"))
        })
        .collect();
    b.clear();
    out
}

/// JavaScript `encodeURIComponent`.
fn js_encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Replace a created entity's minted ids in its lines (the create hooks).
fn mask_entity(lines: Vec<String>, entity: &Value) -> Vec<String> {
    let mut out = lines;
    for (key, token) in [("id", "<id>"), ("officialMountPointId", "<mount>")] {
        if let Some(v) = entity.get(key).and_then(Value::as_str) {
            out = out.into_iter().map(|l| l.replace(v, token)).collect();
        }
    }
    out
}

/// Which of `Scenarios/` / `Knowledge/` a store holds — the v4 case's
/// `folders`.
fn folder_state(db: &Db, mp: &str) -> Value {
    let mp = mp.to_string();
    db.read_mount_index(move |conn| {
        let has = |f: &str| -> rusqlite::Result<bool> {
            conn.query_row(
                "SELECT COUNT(*) FROM doc_mount_folders WHERE mountPointId = ?1 AND LOWER(path) = ?2",
                rusqlite::params![mp, f],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
        };
        Ok(json!({ "scenarios": has("scenarios")?, "knowledge": has("knowledge")? }))
    })
    .expect("folder state")
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

    // [P4.D295] Every case's scenario lines, in order, against v4's.
    let lines_check = |name: &str, got: Vec<String>, failed: &mut Vec<String>| {
        let want: Vec<String> = oracle[name]["lines"]
            .as_array()
            .unwrap_or_else(|| panic!("[{name}] the oracle row carries no `lines`"))
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        let got: Vec<String> = if ENCODED_PARAM_ROWS.contains(&name) {
            got.into_iter()
                .map(|l| match l.split_once(" scenarioPath=") {
                    Some((head, rest)) => {
                        let (path, tail) = rest.split_once(" error=").unwrap_or((rest, ""));
                        let tail = if tail.is_empty() {
                            String::new()
                        } else {
                            format!(" error={tail}")
                        };
                        format!(
                            "{head} scenarioPath={}{tail}",
                            js_encode_uri_component(path)
                        )
                    }
                    None => l,
                })
                .collect()
        } else {
            got
        };
        if got != want {
            eprintln!("[{name}] LINES MISMATCH:\n  v4: {want:#?}\n  v5: {got:#?}");
            failed.push(format!("{name}(lines)"));
        }
    };
    // Success case: compare response_data (optionally ts-blanked) to oracle body.
    let ok = |name: &str, resp: &Response, blank: bool, failed: &mut Vec<String>| {
        lines_check(name, take_lines(), failed);
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
        lines_check(name, take_lines(), failed);
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
            &rt.block_on(groups::group_scenario_list(
                &db,
                GAMMA,
                false,
                &spec.user_id,
            )),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_get");
        ok(
            "group_get",
            &rt.block_on(groups::group_scenario_get(
                &db,
                GAMMA,
                "prologue.md",
                &spec.user_id,
            )),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_get_miss");
        err(
            "group_get_missing",
            &rt.block_on(groups::group_scenario_get(
                &db,
                GAMMA,
                "ghost.md",
                &spec.user_id,
            )),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "g_get_nest");
        err(
            "group_get_nested",
            &rt.block_on(groups::group_scenario_get(
                &db,
                GAMMA,
                "sub/deep.md",
                &spec.user_id,
            )),
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
            &spec.user_id,
        ));
        ok("group_create", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_collide");
        let resp = rt.block_on(groups::group_scenario_create(
            &db,
            GAMMA,
            json!({ "filename": "prologue", "body": "dup" }),
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
        ));
        ok("group_delete", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_delete_miss");
        let resp = rt.block_on(groups::group_scenario_delete(
            &db,
            GAMMA,
            "ghost.md",
            false,
            &spec.user_id,
        ));
        err("group_delete_missing", &resp, &mut failed);
    }
    // --- Participant-union ---
    {
        let db = fresh_db(&spec, "u_aria");
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![ARIA.to_string()],
            false,
            &spec.user_id,
        ));
        ok("union_aria", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_bram");
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![BRAM.to_string()],
            false,
            &spec.user_id,
        ));
        ok("union_bram", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_empty");
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![],
            false,
            &spec.user_id,
        ));
        ok("union_empty", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_unknown");
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![BOGUS.to_string()],
            false,
            &spec.user_id,
        ));
        ok("union_unknown", &resp, false, &mut failed);
    }

    // --- Projects (Iota: opening[default], climax) ---
    {
        let db = fresh_db(&spec, "p_list");
        ok(
            "project_list",
            &rt.block_on(projects::project_scenario_list(
                &db,
                IOTA,
                false,
                &spec.user_id,
            )),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_get");
        ok(
            "project_get",
            &rt.block_on(projects::project_scenario_get(
                &db,
                IOTA,
                "opening.md",
                &spec.user_id,
            )),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_get_miss");
        err(
            "project_get_missing",
            &rt.block_on(projects::project_scenario_get(
                &db,
                IOTA,
                "ghost.md",
                &spec.user_id,
            )),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_get_nest");
        err(
            "project_get_nested",
            &rt.block_on(projects::project_scenario_get(
                &db,
                IOTA,
                "sub/deep.md",
                &spec.user_id,
            )),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_create");
        let resp = rt.block_on(projects::project_scenario_create(
            &db,
            IOTA,
            json!({ "filename": "Act Two", "description": "The middle", "isDefault": true, "body": "It deepens." }),
            &spec.user_id,
        ));
        ok("project_create", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_collide");
        let resp = rt.block_on(projects::project_scenario_create(
            &db,
            IOTA,
            json!({ "filename": "opening", "body": "dup" }),
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
        ));
        ok("project_delete", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_delete_miss");
        let resp = rt.block_on(projects::project_scenario_delete(
            &db,
            IOTA,
            "ghost.md",
            false,
            &spec.user_id,
        ));
        err("project_delete_missing", &resp, &mut failed);
    }

    // --- General (aurora[default] + dusk[default] → default-conflict) ---
    {
        let db = fresh_db(&spec, "gen_list");
        ok(
            "general_list",
            &rt.block_on(scenarios::scenario_list(&db, false, &spec.user_id)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_get");
        ok(
            "general_get",
            &rt.block_on(scenarios::scenario_get(
                &db,
                "aurora.md".into(),
                &spec.user_id,
            )),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_get_miss");
        err(
            "general_get_missing",
            &rt.block_on(scenarios::scenario_get(
                &db,
                "ghost.md".into(),
                &spec.user_id,
            )),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_create");
        let resp = rt.block_on(scenarios::scenario_create(
            &db,
            json!({ "filename": "Twilight", "isDefault": true, "body": "Between." }),
            &spec.user_id,
        ));
        ok("general_create", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_collide");
        let resp = rt.block_on(scenarios::scenario_create(
            &db,
            json!({ "filename": "aurora", "body": "dup" }),
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
        ));
        ok("general_rename", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_delete");
        let resp = rt.block_on(scenarios::scenario_delete(
            &db,
            "dusk.md".into(),
            false,
            &spec.user_id,
        ));
        ok("general_delete", &resp, false, &mut failed);
    }
    // --- General: pre-provision race arms ---
    {
        let db = fresh_db(&spec, "gen_list_unprov");
        unprovision_general(&db);
        ok(
            "general_list_unprov",
            &rt.block_on(scenarios::scenario_list(&db, false, &spec.user_id)),
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
            &spec.user_id,
        ));
        err("general_create_unprov", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_get_unprov");
        unprovision_general(&db);
        let resp = rt.block_on(scenarios::scenario_get(
            &db,
            "aurora.md".into(),
            &spec.user_id,
        ));
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
            &rt.block_on(groups::group_scenario_list(&db, GAMMA, include_archived, &spec.user_id)),
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
                &spec.user_id,
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
            &rt.block_on(projects::project_scenario_list(
                &db,
                IOTA,
                include_archived,
                &spec.user_id,
            )),
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
            &spec.user_id,
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
            &rt.block_on(scenarios::scenario_list(
                &db,
                include_archived,
                &spec.user_id,
            )),
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "gen_create_archived");
        let resp = rt.block_on(scenarios::scenario_create(
            &db,
            json!({ "filename": "Put Away", "name": "Put Away", "archived": true, "body": "Stored." }),
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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
            &spec.user_id,
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

    // ── P4.D295 (v4 `545e299bb`): the location's route edges ───────────────
    let raw_mount = |db: &Db, sql: &'static str, args: Vec<String>| {
        db.write_blocking(move |w| {
            let conn = w.mount_index().expect("mount writer").connection();
            conn.execute(sql, rusqlite::params_from_iter(args.iter()))?;
            Ok(())
        })
        .expect("raw mount statement");
    };
    let drop_folder = |db: &Db, scope: &str, folder: &str| {
        let mp = scope_mount(db, scope);
        raw_mount(
            db,
            "DELETE FROM \"doc_mount_folders\" WHERE \"mountPointId\" = ?1 AND LOWER(\"path\") = ?2",
            vec![mp, folder.to_lowercase()],
        );
    };
    let plant_folder_failure =
        |db: &Db| {
            db.write_blocking(|w| {
                w.mount_index().expect("mount writer").connection().execute_batch(
                "CREATE TRIGGER \"qt_p4d295_folder_failure\" BEFORE INSERT ON \"doc_mount_folders\"
                 WHEN LOWER(NEW.\"path\") = 'scenarios'
                 BEGIN SELECT RAISE(ABORT, 'planted folder failure'); END",
            )?;
                Ok(())
            })
            .expect("plant folder failure");
        };
    let folders_match = |name: &str, db: &Db, scope: &str, failed: &mut Vec<String>| {
        let got = folder_state(db, &scope_mount(db, scope));
        if got != oracle[name]["folders"] {
            eprintln!(
                "[{name}] FOLDERS MISMATCH: v4 {} v5 {got}",
                oracle[name]["folders"]
            );
            failed.push(format!("{name}(folders)"));
        }
    };
    const MISSING_PROJECT: &str = "a3000000-0000-4000-8000-0000000000ff";
    {
        let db = fresh_db(&spec, "p_ws_body");
        let resp = rt.block_on(projects::project_scenario_create(
            &db,
            IOTA,
            json!({ "filename": "ws", "body": "   " }),
            &spec.user_id,
        ));
        err(
            "project_create_whitespace_body_written_then_404",
            &resp,
            &mut failed,
        );
        bytes_match(
            "project_create_whitespace_body_written_then_404",
            &db,
            "project",
            "ws.md",
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_bad_body_missing");
        let resp = rt.block_on(projects::project_scenario_create(
            &db,
            MISSING_PROJECT,
            json!({ "filename": "x", "body": "" }),
            &spec.user_id,
        ));
        err(
            "project_create_invalid_body_missing_project_is_400",
            &resp,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_ok_body_missing");
        let resp = rt.block_on(projects::project_scenario_create(
            &db,
            MISSING_PROJECT,
            json!({ "filename": "x", "body": "y" }),
            &spec.user_id,
        ));
        err(
            "project_create_valid_body_missing_project_is_404",
            &resp,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_rename_same_missing");
        let resp = rt.block_on(projects::project_scenario_rename(
            &db,
            IOTA,
            "ghost.md",
            "ghost",
            false,
            &spec.user_id,
        ));
        err(
            "project_rename_same_name_missing_is_404",
            &resp,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_rename_same_existing");
        let resp = rt.block_on(projects::project_scenario_rename(
            &db,
            IOTA,
            "opening.md",
            "opening",
            false,
            &spec.user_id,
        ));
        ok(
            "project_rename_same_name_existing_logs",
            &resp,
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_rename_empty");
        let resp = rt.block_on(projects::project_scenario_rename(
            &db,
            IOTA,
            "opening.md",
            "",
            false,
            &spec.user_id,
        ));
        err(
            "project_rename_empty_new_filename_is_400",
            &resp,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_get_no_folder");
        drop_folder(&db, "project", "Scenarios");
        let resp = rt.block_on(projects::project_scenario_get(
            &db,
            IOTA,
            "opening.md",
            &spec.user_id,
        ));
        ok(
            "project_get_no_folder_never_heals",
            &resp,
            false,
            &mut failed,
        );
        folders_match(
            "project_get_no_folder_never_heals",
            &db,
            "project",
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "p_put_heals");
        drop_folder(&db, "project", "Scenarios");
        let resp = rt.block_on(projects::project_scenario_update(
            &db,
            IOTA,
            "opening.md",
            json!({ "name": "Opening", "body": "Healed." }),
            false,
            &spec.user_id,
        ));
        ok("project_put_heals_folder", &resp, true, &mut failed);
        folders_match("project_put_heals_folder", &db, "project", &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_put_knowledge");
        drop_folder(&db, "group", "Knowledge");
        let resp = rt.block_on(groups::group_scenario_update(
            &db,
            GAMMA,
            "prologue.md",
            json!({ "name": "Prologue", "body": "Again." }),
            false,
            &spec.user_id,
        ));
        ok("group_put_heals_knowledge", &resp, true, &mut failed);
        folders_match("group_put_heals_knowledge", &db, "group", &mut failed);
    }
    {
        let db = fresh_db(&spec, "g_get_knowledge");
        drop_folder(&db, "group", "Knowledge");
        let resp = rt.block_on(groups::group_scenario_get(
            &db,
            GAMMA,
            "prologue.md",
            &spec.user_id,
        ));
        ok("group_get_never_heals_knowledge", &resp, false, &mut failed);
        folders_match("group_get_never_heals_knowledge", &db, "group", &mut failed);
    }
    {
        // L-6: the dispatch string `a%20b.md` — v5 decodes it once, inside the
        // location; v4 got its HTTP encoding as the route param.
        let db = fresh_db(&spec, "p_get_pct");
        seed_scenario(
            &db,
            "project",
            "a b.md",
            "---\nname: Spaced\n---\n\nA spaced file.",
        );
        let resp = rt.block_on(projects::project_scenario_get(
            &db,
            IOTA,
            "a%20b.md",
            &spec.user_id,
        ));
        ok("project_get_percent_decoded_once", &resp, true, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_get_bad_pct");
        let resp = rt.block_on(projects::project_scenario_get(
            &db,
            IOTA,
            "%E0.md",
            &spec.user_id,
        ));
        err("project_get_malformed_percent_500", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_put_bad_pct");
        let resp = rt.block_on(projects::project_scenario_update(
            &db,
            IOTA,
            "%E0.md",
            json!({ "body": "x" }),
            false,
            &spec.user_id,
        ));
        err("project_put_malformed_percent_500", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "p_list_folder_fail");
        drop_folder(&db, "project", "Scenarios");
        plant_folder_failure(&db);
        let resp = rt.block_on(projects::project_scenario_list(
            &db,
            IOTA,
            false,
            &spec.user_id,
        ));
        err("project_list_folder_failure_500", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "u_folder_fail");
        drop_folder(&db, "group", "Scenarios");
        drop_folder(&db, "beacon", "Scenarios");
        plant_folder_failure(&db);
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![ARIA.to_string()],
            false,
            &spec.user_id,
        ));
        ok(
            "union_folder_failure_skips_with_warn",
            &resp,
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "u_empty_name");
        db.write_blocking(|w| {
            w.main().connection().execute(
                "UPDATE \"groups\" SET \"name\" = '' WHERE \"id\" = ?1",
                [BEACON],
            )?;
            Ok(())
        })
        .expect("blank the group name");
        let resp = rt.block_on(groups::group_scenarios_union(
            &db,
            vec![ARIA.to_string()],
            false,
            &spec.user_id,
        ));
        ok("union_empty_group_name_dropped", &resp, false, &mut failed);
    }
    // --- the catalogue + the by-ref read ---
    {
        let db = fresh_db(&spec, "gen_everywhere");
        let resp = rt.block_on(scenarios::scenario_everywhere(&db, false, &spec.user_id));
        ok("general_everywhere", &resp, false, &mut failed);
    }
    {
        let db = fresh_db(&spec, "gen_everywhere_arch");
        seed_scenario(&db, "project", "mothballed.md", ARCHIVED_FILE);
        let resp = rt.block_on(scenarios::scenario_everywhere(&db, true, &spec.user_id));
        ok("general_everywhere_archived", &resp, true, &mut failed);
    }
    let general_mp = |db: &Db| scope_mount(db, "general");
    let read = |db: &Db, mp: Option<String>, path: Option<&str>| {
        rt.block_on(scenarios::scenario_read_by_ref(
            db,
            mp,
            path.map(str::to_string),
            &spec.user_id,
        ))
    };
    {
        let db = fresh_db(&spec, "read_no_mp");
        err(
            "general_read_missing_mount_point_id",
            &read(&db, None, Some("Scenarios/aurora.md")),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "read_no_path");
        err(
            "general_read_missing_path",
            &read(&db, Some(general_mp(&db)), None),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "read_bad_path");
        err(
            "general_read_refused_bad_path",
            &read(&db, Some(general_mp(&db)), Some("../x.md")),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "read_not_owner");
        err(
            "general_read_refused_not_an_owner",
            &read(
                &db,
                Some("a9000000-0000-4000-8000-0000000000ff".into()),
                Some("Scenarios/x.md"),
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "read_missing");
        err(
            "general_read_missing_file",
            &read(&db, Some(general_mp(&db)), Some("Scenarios/ghost.md")),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "read_general");
        ok(
            "general_read_ok_general_bare_name",
            &read(&db, Some(general_mp(&db)), Some("aurora")),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "read_project");
        ok(
            "general_read_ok_project",
            &read(
                &db,
                Some(scope_mount(&db, "project")),
                Some("Scenarios/opening.md"),
            ),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "read_character");
        let vault: String = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                    [ARIA],
                    |r| r.get::<_, String>(0),
                )?)
            })
            .expect("ARIA's vault");
        {
            let vault = vault.clone();
            db.write_blocking(move |w| {
                w.mount_index()
                    .expect("mount writer")
                    .doc_mount_file_links()
                    .write_database_document(
                        &vault,
                        "Scenarios/Arrival.md",
                        "# The Arrival\n\nDawn at the station.",
                    )?;
                Ok(())
            })
            .expect("seed ARIA's vault");
        }
        let _ = take_lines();
        ok(
            "general_read_ok_character",
            &read(&db, Some(vault), Some("Scenarios/Arrival.md")),
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "read_bad_pct");
        err(
            "general_read_malformed_percent",
            &read(&db, Some(general_mp(&db)), Some("Scenarios/%E0.md")),
            &mut failed,
        );
    }
    // --- the create hooks: the status, and the lines with the minted ids masked ---
    for (name, kind, planted) in [
        ("project_create_hook", "project", false),
        ("project_create_hook_folder_failure", "project", true),
        ("group_create_hook", "group", false),
        ("group_create_hook_folder_failure", "group", true),
    ] {
        let db = fresh_db(&spec, name);
        if planted {
            plant_folder_failure(&db);
        }
        let resp = if kind == "project" {
            rt.block_on(projects::project_create(&db, json!({ "name": "Hooked" })))
        } else {
            rt.block_on(groups::group_create(
                &db,
                "Hooked".to_string(),
                None,
                None,
                None,
                None,
            ))
        };
        let data = response_data(&resp);
        let lines = mask_entity(take_lines(), &data[kind]);
        if matches!(resp, Response::Error(_)) || oracle[name]["status"] != 201 {
            eprintln!("[{name}] expected a created {kind}: {resp:?}");
            failed.push(name.to_string());
        }
        lines_check(name, lines, &mut failed);
    }

    assert!(failed.is_empty(), "scenarios-routes FAILED: {failed:?}");
}
