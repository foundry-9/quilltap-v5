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
use quilltap_core::db::characters_read;
use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::projects::find_official_mount_point_id_raw;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const IOTA: &str = "a3000000-0000-4000-8000-000000000001";
const LAMBDA: &str = "a3000000-0000-4000-8000-000000000002";
const KAPPA: &str = "a3000000-0000-4000-8000-000000000003";
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const BRAM: &str = "a1000000-0000-4000-8000-000000000002";
/// P4.D63: the archived character (fixture extension) — add-character must refuse.
const EDDA: &str = "a1000000-0000-4000-8000-000000000005";
const GAMMA_EXTRA_MP: &str = "b0000000-0000-4000-8000-000000000001";
const IOTA_DANGLING_MP: &str = "b0000000-0000-4000-8000-0000000000df";
const CHAT_A: &str = "c1000000-0000-4000-8000-000000000001";
/// P4.D140: the never-spoken-in project chat (`lastMessageAt` NULL).
const CHAT_B: &str = "c1000000-0000-4000-8000-000000000002";
const CLOAK: &str = "aa000000-0000-4000-8000-000000000001";
const ENSEMBLE: &str = "aa000000-0000-4000-8000-000000000002";
const BG_FILE: &str = "f0000001-0000-4000-8000-000000000001";
const LAMBDA_FILE_1: &str = "f0000002-0000-4000-8000-000000000002";
const MISSING_FILE: &str = "f0000009-0000-4000-8000-000000000009";
/// P4.148: a well-formed character uuid with no row behind it.
const MISSING_CHARACTER: &str = "a1000000-0000-4000-8000-0000000000ff";

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

/// P4.D246 Tier 2 (order item 15): v4's four `[Projects v1]` INFO lines the
/// handlers never logged, pinned through the thread-scoped capture rig. The
/// rig renders `<LEVEL> <target> <message> <field>=<value> …` — the message
/// FIRST and unquoted (a `fmt::Arguments` Debug), then the fields in callsite
/// order (`%` → Display, unquoted — the memory note on the sigil) — so a pin
/// here is BYTE-exact and ORDER-sensitive.
const PROJECTS_TARGET: &str = "INFO quilltap_core::api::projects";

fn projects_v1_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.contains("[Projects v1]"))
        .cloned()
        .collect()
}

/// [P4.148] v4's `[Projects v1]` records (`withLogs`: `{level, message,
/// fields, error}`) against v5's captured lines: each side rendered as
/// `<LEVEL> quilltap_core::api::projects <message> k=v …`, v5's `error=` tail
/// split off and compared to v4's `error` separately — VERBATIM (the
/// JSON-parse wording too, since the `94fbb1ae3` smalls unification). `Err`
/// names the first difference.
fn compare_projects_v1(v4: &Value, v5: &[String]) -> Result<(), String> {
    // The `94fbb1ae3` smalls unification: the JSON-parse tail after
    // `properties.json unparseable: ` is compared VERBATIM — both overlay
    // paths render V8's sentence through the measured twin (P4.154).
    let elide = |e: &str| e.to_string();
    let want: Vec<(String, Option<String>)> = v4
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains("[Projects v1]"))
        })
        .map(|r| {
            let mut line = format!(
                "{} quilltap_core::api::projects {}",
                r["level"].as_str().unwrap_or_default().to_uppercase(),
                r["message"].as_str().unwrap_or_default()
            );
            for pair in r["fields"].as_array().cloned().unwrap_or_default() {
                line.push_str(&format!(
                    " {}={}",
                    pair[0].as_str().unwrap_or_default(),
                    pair[1].as_str().unwrap_or_default()
                ));
            }
            (line, r["error"].as_str().map(elide))
        })
        .collect();
    let got: Vec<(String, Option<String>)> = projects_v1_lines(v5)
        .iter()
        .map(|l| match l.find(" error=") {
            Some(i) => (l[..i].to_string(), Some(elide(&l[i + " error=".len()..]))),
            None => (l.clone(), None),
        })
        .collect();
    if got == want {
        Ok(())
    } else {
        Err(format!("\n  rust:   {got:?}\n  oracle: {want:?}"))
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
        // [P4.D120] `archivedAt` is CLASSIFIED, not blanked: a fresh stamp
        // differs between the two runs, but `null` vs stamped is the whole
        // point of the archive arms and must survive normalization.
        if o.get("archivedAt")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            o.insert("archivedAt".to_string(), Value::String("<stamped>".into()));
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

/// Raw SQL against a case's own copy (the home-routes idiom), so a case can
/// stage a shape the committed fixture cannot express.
fn mutate(db: &Db, sql: &'static str, params: Vec<String>) {
    db.write_blocking(move |ws| {
        let bound: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        ws.main().connection().execute(sql, bound.as_slice())?;
        Ok(())
    })
    .expect("mutation SQL");
}

/// A fresh Db over a private scratch dir; the dir is deleted when this drops
/// (the `Db` field drops first).
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
        .prefix(&format!("qt-gp-proj-{tag}-"))
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
    ScratchDb { db, _dir: scratch }
}

/// P4.142 (G1) — v4's `{level, message, fields}` plant records rendered as v5's
/// `quilltap::db` capture lines, and v5's ERROR/WARN lines with the `error=` tail
/// dropped (each stack's own driver sentence).
fn plant_lines(v4: &Value, v5: &[String]) -> (Vec<String>, Vec<String>) {
    let want = v4
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|rec| {
            let mut line = format!(
                "{} quilltap::db {}",
                rec["level"].as_str().unwrap_or_default().to_uppercase(),
                rec["message"].as_str().unwrap_or_default()
            );
            for pair in rec["fields"].as_array().cloned().unwrap_or_default() {
                line.push_str(&format!(
                    " {}={}",
                    pair[0].as_str().unwrap_or_default(),
                    pair[1].as_str().unwrap_or_default()
                ));
            }
            line
        })
        .collect();
    let got = v5
        .iter()
        .filter(|l| l.starts_with("ERROR ") || l.starts_with("WARN "))
        .map(|l| match l.find(" error=") {
            Some(i) => l[..i].to_string(),
            None => l.clone(),
        })
        .collect();
    (got, want)
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
    // P4.55: an error-arm check — HTTP status + v4's `{error}` message.
    let check_error = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_i64().unwrap_or(0);
        let want_msg = rec["body"]["error"].as_str().unwrap_or("");
        match got {
            Response::Error(e) => {
                let got_status = http_for(e.kind);
                if got_status != want_status || e.message != want_msg {
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
                eprintln!("[{name}] expected Error, got {:?}", response_data(other));
                failed.push(name.to_string());
            }
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

    // [P4.148 item 14] v4 answers a create with `created(...)` = 201; v5's status
    // lives at the web edge, not in `Response`, so the pin is "v4 says 201 and
    // v5 answered a non-`Error` variant" — at EVERY success-create row (the
    // `:1302` 200 precedent), held whole by the census after the run.
    //
    // [P4.155 Tier 2 item 7] ⚠ RECORDED TRANSPORT DIVERGENCE: v5 has NO REST
    // route for projects — every project op reaches the core through the
    // engine dispatch (`api/engine.rs` → `projects::project_*`), whose success
    // reply is always HTTP 200. So where v4 answers 201, v5's wire answers 200.
    // This pin asserts v4's status only; it is not a claim that v5 says 201.
    let pinned_201: std::cell::RefCell<std::collections::BTreeSet<String>> = Default::default();
    let pin_201 = |name: &str, resp: &Response, failed: &mut Vec<String>| {
        assert_eq!(
            oracle[name]["status"].as_i64(),
            Some(201),
            "{name}: v4's status"
        );
        if matches!(resp, Response::Error(_)) {
            eprintln!(
                "[{name}] v4 answered 201, v5 an Error: {:?}",
                response_data(resp)
            );
            failed.push(format!("{name}_201"));
        }
        pinned_201.borrow_mut().insert(name.to_string());
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
        // P4.D140 / v4 `735d9408c`: push the never-spoken-in chat's `updatedAt`
        // past the other chat's activity. The OLD `lastMessageAt ?? updatedAt`
        // spelling floats it to the top; `chatActivityAt` leaves it below.
        let db = fresh_db(&spec, "lchaf");
        mutate(
            &db,
            r#"UPDATE "chats" SET "updatedAt" = ?1 WHERE "id" = ?2"#,
            vec!["2026-12-01T00:00:00.000Z".into(), CHAT_B.into()],
        );
        check(
            "list_chats_activity_fallback",
            &response_data(&projects::project_chat_list(&db, IOTA, None, None)),
            false,
            &mut failed,
        );
    }
    // P4.D143 (v4 `c43d3b1b4`): the project chats row carries the DERIVED
    // `conciergeState` + `dangerCategories`, never the raw label. Both project
    // chats painted at once — Chat A Vouched over a TRUE label, Chat B
    // Uncensored over a FALSE one — plus a Flagged pass with categories, the
    // labels set the wrong way round on the operator rows so a leaked
    // `isDangerousChat` would be visibly wrong. `list_chats` above is the
    // Monitored arm. Mirrors the oracle's raw UPDATEs exactly.
    {
        let db = fresh_db(&spec, "lcos");
        mutate(
            &db,
            // P4.D227 (v4 `3b463d6b1`, #76): `conciergeOverride` is DROPPED.
            r#"UPDATE "chats" SET "isDangerousChat" = 1 WHERE "id" = ?1"#,
            vec![CHAT_A.into()],
        );
        mutate(
            &db,
            r#"UPDATE "chats" SET "isDangerousChat" = 0 WHERE "id" = ?1"#,
            vec![CHAT_B.into()],
        );
        // P4.D226 (v4 `4d370a90f`): the states the rows derive from.
        mutate(
            &db,
            r#"UPDATE "chats" SET "conciergeMode" = 'locked', "conciergeModeSetBy" = 'operator', "conciergeModeReason" = 'migration' WHERE "id" = ?1"#,
            vec![CHAT_A.into()],
        );
        mutate(
            &db,
            r#"UPDATE "chats" SET "conciergeMode" = 'unmoderated', "conciergeModeSetBy" = 'operator', "conciergeModeReason" = 'manual' WHERE "id" = ?1"#,
            vec![CHAT_B.into()],
        );
        check(
            "list_chats_operator_states",
            &response_data(&projects::project_chat_list(&db, IOTA, None, None)),
            false,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lcfc");
        mutate(
            &db,
            r#"UPDATE "chats" SET "isDangerousChat" = 1, "dangerCategories" = ?1 WHERE "id" = ?2"#,
            vec![r#"["Violence","Substance Use"]"#.into(), CHAT_A.into()],
        );
        mutate(
            &db,
            r#"UPDATE "chats" SET "conciergeMode" = 'unmoderated', "conciergeModeSetBy" = 'concierge', "conciergeModeReason" = 'classifier' WHERE "id" = ?1"#,
            vec![CHAT_A.into()],
        );
        check(
            "list_chats_flagged_categories",
            &response_data(&projects::project_chat_list(&db, IOTA, None, None)),
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
        // P4.70: the latest-chat BRANCH, which neither standing arm reaches —
        // Iota is stored in the retired `'project'` mode (folded to `'theme'`
        // by the GET's normalize; that fold is what `background_iota` pins) and
        // Kappa in `'theme'`. `'latest_chat'` is the surviving write mode, so
        // the PUT is the way in without disturbing the committed fixture. Iota
        // owns two chats, one carrying `storyBackgroundImageId` and one not, so
        // the filter is exercised and `sourceChatId` must name the right one.
        // The sort-by-`updatedAt`-desc is NOT exercised (one candidate) —
        // recorded rather than faked.
        let db = fresh_db(&spec, "bglc");
        let resp = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "backgroundDisplayMode": "latest_chat" }),
        ));
        assert!(
            !matches!(resp, Response::Error(_)),
            "the latest_chat PUT must succeed before the GET can reach its branch"
        );
        check(
            "background_iota_latest_chat",
            &response_data(&projects::project_background_get(&db, IOTA)),
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
        pin_201("create", &resp, &mut failed);
    }
    // ---- P4.D114 / v4 bug 98 (`c93ec7ff`) ----
    // v4's create schema moved into `schemas.ts` and the four presentational
    // fields became `.nullable().optional()`. MEASURED against v4's real
    // schema (old vs new): ONLY the four null legs moved. Everything else here
    // is unchanged in v4 and exists because v5's hand-rolled create validated
    // nothing but the name — and refused a whitespace-only name v4 accepts.
    for (name, tag, body) in [
        (
            "create_blank_description",
            "cbd",
            json!({ "name": "Nu", "description": null, "instructions": null,
                    "color": "#abcdef", "icon": "rocket" }),
        ),
        (
            // `.min(1)` on the RAW string — no `.trim()` in this schema.
            "create_whitespace_name",
            "cwn",
            json!({ "name": "   ", "color": "#abcdef", "icon": "rocket" }),
        ),
        (
            "create_unknown_key_stripped",
            "cuk",
            json!({ "name": "Omicron", "color": "#abcdef", "icon": "rocket",
                    "notAField": "should vanish" }),
        ),
        (
            // Zod ≥ 4.5.4 (v4 `6e1a64ea6`): the `.max(100)` counts CODE POINTS
            // once the UTF-16 count overflows — 51 top hats are 102 units but 51
            // code points, ACCEPTED (`jsstr::zod_len_max_ok`). Was the 400 row
            // `create_name_astral_over_max` under 4.4.3's UTF-16 rule.
            "create_name_astral_within_max",
            "cnawm",
            json!({ "name": "\u{1F3A9}".repeat(51), "color": "#abcdef", "icon": "rocket" }),
        ),
    ] {
        let db = fresh_db(&spec, tag);
        let resp = rt.block_on(projects::project_create(&db, body));
        check(name, &response_data(&resp), true, &mut failed);
        pin_201(name, &resp, &mut failed);
    }
    // ---- P4.146 (dogfood #136): the `|| null` arm, compared whole ----
    // Siblings of the value-arm rows (`create`, `create_blank_description`,
    // `create_whitespace_name`) with NO colour/icon — v4's route stores
    // `color: null, icon: null` — plus an empty icon (`'' || null`).
    for (name, tag, body) in [
        (
            "create_no_colour",
            "cnc",
            json!({ "name": "Mu", "description": "A new project", "allowAnyCharacter": true, "characterRoster": [ARIA] }),
        ),
        (
            "create_blank_description_no_colour",
            "cbdnc",
            json!({ "name": "Nu", "description": null, "instructions": null }),
        ),
        (
            "create_whitespace_name_no_colour",
            "cwnnc",
            json!({ "name": "   " }),
        ),
        (
            "create_empty_icon",
            "cei",
            json!({ "name": "Sigma", "icon": "" }),
        ),
    ] {
        let db = fresh_db(&spec, tag);
        let resp = rt.block_on(projects::project_create(&db, body));
        check(name, &response_data(&resp), true, &mut failed);
        pin_201(name, &resp, &mut failed);
        assert_eq!(
            response_data(&resp)["project"].get("icon"),
            Some(&Value::Null),
            "{name}: v4 stores and echoes an explicit null icon"
        );
    }
    {
        // MEASURED at the pin: an empty COLOUR never reaches `|| null` —
        // `HexColorSchema` refuses `''` first.
        let db = fresh_db(&spec, "cec400");
        let resp = rt.block_on(projects::project_create(
            &db,
            json!({ "name": "Rho", "color": "" }),
        ));
        check_error("create_empty_colour_400", &resp, &mut failed);
    }
    {
        // The PUT entry point: `color: null` is written as `null`, not absent.
        let db = fresh_db(&spec, "unc");
        let resp = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "color": null, "icon": null }),
        ));
        check(
            "update_null_color",
            &response_data(&resp),
            true,
            &mut failed,
        );
        // And the read wire after it: `projectList` carries Iota's two nulls.
        let db = fresh_db(&spec, "lanc");
        let _ = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "color": null, "icon": null }),
        ));
        check(
            "list_after_null_color",
            &response_data(&projects::project_list(&db)),
            true,
            &mut failed,
        );
    }
    {
        // `color`/`icon` null: bug 98's other two legs — v4 REFUSED this body
        // before `c93ec7ff`. P4.146 (dogfood #136) LIFTED the mask this arm
        // carried: v4's route stores `color: null, icon: null` (`|| null`) and
        // echoes them, and v5's three-state bag now does too, so the whole echo
        // is the comparand.
        let db = fresh_db(&spec, "cnci");
        let resp = rt.block_on(projects::project_create(
            &db,
            json!({ "name": "Xi", "color": null, "icon": null }),
        ));
        check(
            "create_null_color_and_icon",
            &response_data(&resp),
            true,
            &mut failed,
        );
        pin_201("create_null_color_and_icon", &resp, &mut failed);
        let project = &response_data(&resp)["project"];
        assert_eq!(
            project.get("color"),
            Some(&Value::Null),
            "an explicit null colour"
        );
        assert_eq!(
            project.get("icon"),
            Some(&Value::Null),
            "an explicit null icon"
        );
    }
    {
        // P4.D246 (v4 `9753d0eb2`): `allowAnyCharacter: z.boolean().prefault(true)`
        // — a project created with the flag ABSENT is OPEN, stated by itself on a
        // minimal `{name}` body. Since P4.146 (dogfood #136) the whole echo is the
        // comparand — `color: null, icon: null` (v4's `|| null` on an absent key)
        // included; the mask this arm carried is LIFTED.
        let name = "create_flag_absent_defaults_open";
        let db = fresh_db(&spec, "cfado");
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_create(&db, json!({ "name": "Pi" })))
        });
        // P4.D246 Tier 2: v4 `route.ts:82-85` — `{ projectId, name }`, the id
        // minted, so the line is pinned around it.
        let created = projects_v1_lines(&lines);
        assert_eq!(
            created.len(),
            1,
            "{name}: exactly one [Projects v1] line: {lines:?}"
        );
        assert!(
            created[0].starts_with(&format!(
                "{PROJECTS_TARGET} [Projects v1] Project created projectId="
            )) && created[0].ends_with(" name=Pi"),
            "{name}: v4's create INFO line: {}",
            created[0]
        );
        check(name, &response_data(&resp), true, &mut failed);
        pin_201(name, &resp, &mut failed);
        assert_eq!(
            response_data(&resp)["project"].get("color"),
            Some(&Value::Null),
            "{name}: v4's `|| null` on an absent colour"
        );
        // The rule by name, so a regression reads as itself and not as one
        // line of a body diff: the flag-less create answers OPEN.
        assert_eq!(
            response_data(&resp)["project"]["allowAnyCharacter"],
            Value::Bool(true),
            "{name}: a project created without the flag opens (v4 9753d0eb2)"
        );
        assert_eq!(
            oracle[name]["body"]["project"]["allowAnyCharacter"],
            Value::Bool(true),
            "{name}: the oracle was not recorded at a pin past 9753d0eb2"
        );
    }
    for (name, tag, body) in [
        ("create_missing_name", "cmn", json!({})),
        ("create_empty_name", "cen", json!({ "name": "" })),
        (
            "create_name_over_max",
            "cnom",
            json!({ "name": "x".repeat(101) }),
        ),
        ("create_name_wrong_type", "cnwt", json!({ "name": 42 })),
        (
            "create_description_over_max",
            "cdom",
            json!({ "name": "P", "description": "x".repeat(2001) }),
        ),
        (
            "create_instructions_over_max",
            "ciom",
            json!({ "name": "P", "instructions": "x".repeat(10001) }),
        ),
        (
            "create_icon_over_max",
            "ciom2",
            json!({ "name": "P", "icon": "x".repeat(51) }),
        ),
        (
            "create_bad_color",
            "cbc",
            json!({ "name": "P", "color": "blue" }),
        ),
        (
            "create_roster_non_uuid",
            "crnu",
            json!({ "name": "P", "characterRoster": ["not-a-uuid"] }),
        ),
        (
            // `.optional().prefault([])` is NOT `.nullable()`.
            "create_roster_null",
            "crn",
            json!({ "name": "P", "characterRoster": null }),
        ),
        (
            "create_allow_any_wrong_type",
            "caawt",
            json!({ "name": "P", "allowAnyCharacter": "yes" }),
        ),
        (
            "create_allow_any_null",
            "caan",
            json!({ "name": "P", "allowAnyCharacter": null }),
        ),
        // Zod's root `invalid_type` reaches the same flat sentence.
        ("create_non_object_body", "cnob", json!("hello")),
    ] {
        let db = fresh_db(&spec, tag);
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_create(&db, body))
        });
        check_error(name, &resp, &mut failed);
        // P4.D246 Tier 2, the silence leg: a refused create logs no `[Projects
        // v1]` line (v4's parse throws before the handler's INFO).
        assert!(
            projects_v1_lines(&lines).is_empty(),
            "{name}: a refusal must log no [Projects v1] line: {lines:?}"
        );
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
    // P4.55 (the merge-verb silent-keep sweep): v4 parses the body through
    // `updateProjectSchema` and hands the repository the PARSED data, so an
    // invalid field 400s and an unknown key is STRIPPED. v5 passed the raw body
    // through with no validation at all.
    for (name, tag, patch) in [
        (
            "update_invalid_type",
            "uit",
            json!({ "allowAnyCharacter": "yes" }),
        ),
        ("update_over_max", "uom", json!({ "name": "x".repeat(101) })),
        (
            // `backgroundDisplayMode` is `.optional()` but NOT `.nullable()`.
            "update_null_non_nullable",
            "unn",
            json!({ "backgroundDisplayMode": null }),
        ),
        // [P4.D146 / v4 70505745a] The update enum narrows to
        // ['latest_chat','theme']. The two retired modes are refused at the
        // write gate — the coercion in `ProjectEntity::parse_properties` is for
        // values already on disk; a write must not be able to put one there
        // afresh.
        (
            "update_retired_mode_project",
            "urp",
            json!({ "backgroundDisplayMode": "project" }),
        ),
        (
            "update_retired_mode_static",
            "urs",
            json!({ "backgroundDisplayMode": "static" }),
        ),
    ] {
        let db = fresh_db(&spec, tag);
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_update(&db, IOTA, patch))
        });
        check_error(name, &resp, &mut failed);
        // P4.D246 Tier 2, the silence leg: a refused PUT logs no `[Projects v1]`
        // line (v4's parse throws before the write and the INFO).
        assert!(
            projects_v1_lines(&lines).is_empty(),
            "{name}: a refusal must log no [Projects v1] line: {lines:?}"
        );
    }
    {
        // …and the surviving mode still passes, so the narrowing is not a
        // blanket refusal.
        let db = fresh_db(&spec, "usm");
        let resp = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "backgroundDisplayMode": "latest_chat" }),
        ));
        check(
            "update_surviving_mode_latest_chat",
            &response_data(&resp),
            true,
            &mut failed,
        );
    }
    {
        // The unknown key is stripped, not refused: 200, absent from the echo,
        // and absent from the row.
        let db = fresh_db(&spec, "uuk");
        let resp = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "name": "Iota Stripped", "notAField": "should vanish" }),
        ));
        check(
            "update_unknown_key_stripped",
            &response_data(&resp),
            true,
            &mut failed,
        );
        check_tables(
            "update_unknown_key_stripped",
            &dump_project_tables(&db),
            &mut failed,
        );
    }
    {
        // P4.55, the P4.D85 cleared-null residue: `description` is
        // `.nullable()`, so this CLEARS it. v4's store-backed `update` answers
        // `_update`'s in-memory merge overlaid; v5 re-reads. This arm is the
        // measurement of whether the two echoes agree on a cleared
        // store-resident key.
        let db = fresh_db(&spec, "ucd");
        let resp = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "description": null }),
        ));
        check(
            "update_clear_description",
            &response_data(&resp),
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "delete");
        let resp = rt.block_on(projects::project_delete(&db, IOTA));
        check("delete", &response_data(&resp), false, &mut failed);
        check_tables("delete", &dump_project_tables(&db), &mut failed);
    }
    {
        let db = fresh_db(&spec, "addc");
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_character_add(&db, KAPPA, BRAM))
        });
        check("add_character", &response_data(&resp), false, &mut failed);
        // P4.D246 Tier 2: v4 `roster.ts:87` — `{ projectId, characterId }`.
        assert_eq!(
            projects_v1_lines(&lines),
            vec![format!(
                "{PROJECTS_TARGET} [Projects v1] Character added to project projectId={KAPPA} characterId={BRAM}"
            )],
            "add_character: v4's add INFO line"
        );
    }
    {
        let db = fresh_db(&spec, "remc");
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_character_remove(&db, IOTA, ARIA))
        });
        check(
            "remove_character",
            &response_data(&resp),
            false,
            &mut failed,
        );
        // P4.D246 Tier 2: v4 `roster.ts:113` — `{ projectId, characterId }`.
        assert_eq!(
            projects_v1_lines(&lines),
            vec![format!(
                "{PROJECTS_TARGET} [Projects v1] Character removed from project projectId={IOTA} characterId={ARIA}"
            )],
            "remove_character: v4's remove INFO line"
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
        pin_201("mount_link", &resp, &mut failed);
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
            &response_data(&projects::project_wardrobe_list(&db, IOTA, false)),
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
        pin_201("wardrobe_create", &resp, &mut failed);
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
    // ── P4.D120 / v4 `d25dacc1` ──────────────────────────────────────────
    // The list's hard-coded `true` is gone; each of these archives the Cloak
    // first, because a fresh fixture holds no archived garment and the flag
    // could not otherwise discriminate.
    for (name, include_archived) in [
        ("wardrobe_list_hides_an_archived_garment", false),
        (
            "wardrobe_list_shows_an_archived_garment_with_the_flag",
            true,
        ),
    ] {
        let db = fresh_db(&spec, name);
        let _ = rt.block_on(projects::project_wardrobe_update(
            &db,
            IOTA,
            CLOAK,
            json!({ "archived": true }),
        ));
        check(
            name,
            &response_data(&projects::project_wardrobe_list(
                &db,
                IOTA,
                include_archived,
            )),
            true,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "w_arch");
        let resp = rt.block_on(projects::project_wardrobe_update(
            &db,
            IOTA,
            CLOAK,
            json!({ "archived": true }),
        ));
        check(
            "wardrobe_update_archives",
            &response_data(&resp),
            true,
            &mut failed,
        );
    }
    {
        // The NEW 404, reachable only with `archived` in the body.
        let db = fresh_db(&spec, "w_arch_miss");
        let resp = rt.block_on(projects::project_wardrobe_update(
            &db,
            IOTA,
            "eeeeeeee-eeee-4eee-8eee-eeeeeeeeee01",
            json!({ "archived": true }),
        ));
        check_error(
            "wardrobe_update_archived_missing_item_404",
            &resp,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "wd");
        let resp = rt.block_on(projects::project_wardrobe_delete(&db, IOTA, ENSEMBLE));
        let mut body = response_data(&resp);
        let remaining: Vec<Value> =
            response_data(&projects::project_wardrobe_list(&db, IOTA, false))
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
        // P4.142 (G1): the links plant — `doc_mount_file_links.originalMimeType`
        // renamed (named by v4's `queryLinks`, never by the project overlay's
        // batch reads). v4's Branch A read is the FILES repository's fallback
        // `findByMountPointId`: 200 `{files: [], count: 0}` + `Error finding files
        // by mount point ID {collection: doc_mount_files, mountPointId}`.
        let name = "list_files_iota_links_plant";
        let db = fresh_db(&spec, "lf_iota_plant");
        db.write_blocking(|ws| {
            ws.mount_index().expect("mount present").connection().execute_batch(
                "ALTER TABLE \"doc_mount_file_links\" RENAME COLUMN \"originalMimeType\" TO \"originalMimeType_x\"",
            )?;
            Ok(())
        })
        .expect("plant the links rename");
        let (resp, lines) =
            quilltap_core::test_support::captured_with(|| projects::project_file_list(&db, IOTA));
        if let Response::Error(e) = &resp {
            eprintln!("[{name}] STATUS: v5 answered {:?} {}", e.kind, e.message);
            failed.push(format!("{name}:status"));
        }
        assert_eq!(
            oracle[name]["status"].as_i64(),
            Some(200),
            "{name}: v4's status"
        );
        check(name, &response_data(&resp), false, &mut failed);
        let (got, want) = plant_lines(&oracle[name]["logs"], &lines);
        if got != want {
            eprintln!("[{name}] LINES:\n  v5: {got:#?}\n  v4: {want:#?}");
            failed.push(format!("{name}:lines"));
        }
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
        // P4.D63: the archived add-to-roster refusal (Shared contract rule 7).
        // Placed here because `check_err` — the error-shape comparand — comes
        // into scope with the files section.
        let db = fresh_db(&spec, "addc_arch");
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_character_add(&db, KAPPA, EDDA))
        });
        check_err("add_character_archived", &resp, &mut failed);
        // P4.D246 Tier 2, the silence leg: v4 returns the 400 BEFORE its INFO.
        assert!(
            projects_v1_lines(&lines).is_empty(),
            "add_character_archived: a refusal must log no [Projects v1] line: {lines:?}"
        );
    }
    {
        let db = fresh_db(&spec, "rm_file");
        let resp = rt.block_on(projects::project_file_remove(&db, LAMBDA, LAMBDA_FILE_1));
        check("remove_file", &response_data(&resp), false, &mut failed);
        check_tables("remove_file", &dump_project_tables(&db), &mut failed);
    }

    // --- P4.23: the corrupted-store 503 envelope arms ---
    // Malformed bytes planted through the REAL write_database_document; the
    // hydrating find_by_id refuses and the api layer answers v4's deliberate
    // contextful 503. Status AND body byte-compare against v4's REAL route
    // (the middleware envelope, context.ts:176-205) — the body via raw
    // to_string so KEY ORDER is pinned too. The PUT arm proves the WRITE
    // route refuses on the same read-path throw (it hydrates before writing).
    // Mutation-proven: collapsing `overlay_to_db` back to `DbError::Internal`
    // reds these arms on kind AND body.
    let plant_corrupt = |db: &Db| {
        let mp = db
            .read_main(|main| find_official_mount_point_id_raw(main, IOTA))
            .expect("read iota store fk")
            .flatten()
            .expect("iota has an officialMountPointId");
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
    };
    let check_unavailable = |name: &str, resp: &Response, failed: &mut Vec<String>| {
        let want = &oracle[name];
        assert_eq!(
            want["status"].as_i64(),
            Some(503),
            "oracle {name} did not answer 503 — did v4's middleware envelope move?"
        );
        match resp {
            Response::Error(e) => {
                if !matches!(e.kind, ErrorKind::Unavailable) {
                    eprintln!("[{name}] kind {:?} (want Unavailable / HTTP 503)", e.kind);
                    failed.push(format!("{name}_kind"));
                }
                match e.unavailable_wire_body() {
                    Some(got_body) => {
                        let got = serde_json::to_string(&got_body).unwrap();
                        let want_body = serde_json::to_string(&want["body"]).unwrap();
                        if got != want_body {
                            eprintln!("[{name}] MISMATCH:\n  GOT : {got}\n  WANT: {want_body}");
                            failed.push(name.to_string());
                        } else {
                            eprintln!("[{name}] OK.");
                        }
                    }
                    None => {
                        eprintln!("[{name}] refusal carries no entity (wire body absent)");
                        failed.push(format!("{name}_body"));
                    }
                }
            }
            other => {
                eprintln!(
                    "[{name}] expected the 503 refusal, got: {}",
                    serde_json::to_string(other).unwrap_or_default()
                );
                failed.push(format!("{name}_not_error"));
            }
        }
    };
    {
        // v4's project GET wraps its whole body in a LOCAL try/catch → a FIXED
        // `serverError('Failed to fetch project')` — the middleware's 503
        // never fires here, unlike the PUT below. The oracle measured it
        // (500 + this exact body), and this arm pins v5 to the same.
        let db = fresh_db(&spec, "corrupt_get");
        plant_corrupt(&db);
        let want = &oracle["get_store_corrupt"];
        let (resp, lines) =
            quilltap_core::test_support::captured_with(|| projects::project_get(&db, IOTA));
        // [P4.148] The catch's line, in v4's bytes (was `project GET failed`).
        if let Err(diff) = compare_projects_v1(&want["logs"], &lines) {
            eprintln!("[get_store_corrupt] [Projects v1] lines MISMATCH:{diff}");
            failed.push("get_store_corrupt_line".into());
        }
        assert!(
            !lines.iter().any(|l| l.contains("sqlite error:")),
            "get_store_corrupt: the error field is the BARE message: {lines:?}"
        );
        match resp {
            Response::Error(e) => {
                let got_status = http_for(e.kind);
                let got_body = serde_json::to_string(&json!({ "error": e.message })).unwrap();
                let want_body = serde_json::to_string(&want["body"]).unwrap();
                if Some(got_status) != want["status"].as_i64() || got_body != want_body {
                    eprintln!(
                        "[get_store_corrupt] MISMATCH:\n  GOT : {got_status} {got_body}\n  WANT: {} {want_body}",
                        want["status"]
                    );
                    failed.push("get_store_corrupt".into());
                } else {
                    eprintln!("[get_store_corrupt] OK.");
                }
            }
            other => {
                eprintln!(
                    "[get_store_corrupt] expected the local-catch 500, got: {}",
                    serde_json::to_string(&other).unwrap_or_default()
                );
                failed.push("get_store_corrupt_not_error".into());
            }
        }
    }
    {
        let db = fresh_db(&spec, "corrupt_put");
        plant_corrupt(&db);
        let resp = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "name": "Iota Should Not Rename" }),
        ));
        check_unavailable("update_store_corrupt", &resp, &mut failed);
    }

    // ── P4.D246 (v4 `9753d0eb2`): the PUT answers the ENRICHED project ──────
    // `handlePutDefault` now runs the GET's `enrichProject` over the stored
    // project AFTER the write. The four standing PUT rows (`update`,
    // `update_surviving_mode_latest_chat`, `update_unknown_key_stripped`,
    // `update_clear_description`) pin it on Iota's rich roster; these three
    // state what they cannot.
    {
        // An EMPTY roster: `characterRoster: []` + `_count.characters: 0`.
        let name = "update_on_empty_roster";
        let db = fresh_db(&spec, "uoer");
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_update(
                &db,
                KAPPA,
                json!({ "description": "Kappa, re-described" }),
            ))
        });
        check(name, &response_data(&resp), true, &mut failed);
        // P4.D246 Tier 2: v4 `project-crud.ts:113` — `{ projectId, userId }`,
        // `userId` the engine's single user, logged after the write.
        assert_eq!(
            projects_v1_lines(&lines),
            vec![format!(
                "{PROJECTS_TARGET} [Projects v1] Project updated projectId={KAPPA} userId={}",
                quilltap_core::api::SINGLE_USER_ID
            )],
            "{name}: v4's update INFO line"
        );
        let project = &response_data(&resp)["project"];
        assert_eq!(
            project["characterRoster"],
            json!([]),
            "{name}: the enriched empty roster"
        );
        assert_eq!(
            project["_count"]["characters"],
            json!(0),
            "{name}: `_count` is appended even for an empty roster"
        );
    }
    {
        // The "same shape as GET" claim made a row: a PUT on Iota, then the
        // GET, both bodies recorded on both sides — and v5's two bodies must be
        // IDENTICAL apart from the minted `updatedAt` (every other byte,
        // character ids included, compared raw).
        let name = "update_then_get_agree";
        let db = fresh_db(&spec, "utga");
        let put = response_data(&rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "name": "Iota Agreed" }),
        )));
        let get = response_data(&projects::project_get(&db, IOTA));
        check(name, &json!({ "put": put, "get": get }), true, &mut failed);
        let strip_updated_at = |v: &Value| {
            let mut v = v.clone();
            if let Some(p) = v.get_mut("project").and_then(Value::as_object_mut) {
                p.remove("updatedAt");
            }
            v
        };
        assert_eq!(
            norm(&strip_updated_at(&put)),
            norm(&strip_updated_at(&get)),
            "{name}: the PUT's body must be the GET's body (v4 9753d0eb2 — one `enrichProject`)"
        );
        assert!(
            put["project"]["characterRoster"][0].is_object(),
            "{name}: the PUT's roster entries are objects, not ids"
        );
    }
    {
        // M5's arm: the enrichment FAILING after the write. Aria (on Iota's
        // roster) loses her vault keystone through the REAL
        // delete_database_document, so `characters_read::find_by_id` refuses
        // `StoreUnavailable` inside `enrich_project` — after `repo.update`
        // committed. v4's `handlePutDefault` has no local try/catch (the GET
        // does), so the throw is the middleware's contextful 503
        // `{error, characterId}`, and the dump proves the rename LANDED. A port
        // that routed the PUT's enrichment through the GET's fixed
        // `internal("Failed to fetch project")` answers 500 here and reddens.
        let name = "update_enrich_store_corrupt";
        let db = fresh_db(&spec, "corrupt_put_enrich");
        let aria_vault = db
            .read_main(|main| characters_read::find_by_id_raw(main, ARIA))
            .expect("read aria raw")
            .and_then(|c| {
                c.get("characterDocumentMountPointId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .expect("aria has a characterDocumentMountPointId");
        rt.block_on(db.write(move |w| {
            let mount = w
                .mount_index()
                .expect("fixture has a mount-index partition");
            DocMountFileLinksRepository::new(mount.connection())
                .delete_database_document(&aria_vault, "properties.json")?;
            Ok(())
        }))
        .expect("delete aria's vault keystone");
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_update(
                &db,
                IOTA,
                json!({ "name": "Iota Renamed Behind A Broken Vault" }),
            ))
        });
        check_unavailable(name, &resp, &mut failed);
        // v4 `project-crud.ts:113` logs the update BEFORE `enrichProject`
        // runs, so the INFO fires even though the enrichment then 503s (pinned
        // at the `e5c6bd0c0` unification — a port that logged only after a
        // successful enrichment stayed green everywhere else).
        assert_eq!(
            projects_v1_lines(&lines),
            vec![format!(
                "{PROJECTS_TARGET} [Projects v1] Project updated projectId={IOTA} userId={}",
                quilltap_core::api::SINGLE_USER_ID
            )],
            "{name}: v4's update INFO line fires before the failing enrichment"
        );
        check_tables(name, &dump_project_tables(&db), &mut failed);
        let renamed = dump_project_tables(&db)["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == IOTA && p["name"] == "Iota Renamed Behind A Broken Vault");
        assert!(
            renamed,
            "{name}: the write must have LANDED before the enrichment failed"
        );
    }

    // ---- P4.148 (P4.D246 item 16): the six `z.uuid()` gates, after the
    // project's existence check. Body AND `details` compared; silence leg: no
    // `[Projects v1]` line (each handler's INFO follows its write).
    for name in [
        "add_character_bad_uuid",
        "remove_character_bad_uuid",
        "add_chat_bad_uuid",
        "remove_chat_bad_uuid",
        "add_file_bad_uuid",
        "remove_file_bad_uuid",
    ] {
        let db = fresh_db(&spec, name);
        let bad = "not-a-uuid";
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(async {
                match name {
                    "add_character_bad_uuid" => {
                        projects::project_character_add(&db, KAPPA, bad).await
                    }
                    "remove_character_bad_uuid" => {
                        projects::project_character_remove(&db, KAPPA, bad).await
                    }
                    "add_chat_bad_uuid" => projects::project_chat_add(&db, KAPPA, bad).await,
                    "remove_chat_bad_uuid" => projects::project_chat_remove(&db, KAPPA, bad).await,
                    "add_file_bad_uuid" => projects::project_file_add(&db, KAPPA, bad).await,
                    _ => projects::project_file_remove(&db, KAPPA, bad).await,
                }
            })
        });
        let want = &oracle[name];
        assert_eq!(want["status"].as_i64(), Some(400), "{name}: v4's status");
        match &resp {
            Response::Error(e) => {
                let got = json!({
                    "error": e.message,
                    "details": e.details.as_deref().cloned().unwrap_or(Value::Null),
                });
                if http_for(e.kind) != 400 || norm(&got) != norm(&want["body"]) {
                    eprintln!(
                        "[{name}] MISMATCH:\n{}",
                        first_diff(&norm(&got), &norm(&want["body"]))
                    );
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK (400).");
                }
            }
            other => {
                eprintln!("[{name}] expected the 400, got {:?}", response_data(other));
                failed.push(format!("{name}_not_error"));
            }
        }
        // The silence leg — a failure, not a panic, so a red run names every
        // gate it reaches.
        if !projects_v1_lines(&lines).is_empty() {
            eprintln!("[{name}] a refusal logged [Projects v1]: {lines:?}");
            failed.push(format!("{name}_silence"));
        }
        assert_eq!(
            want["logs"].as_array().map(Vec::len),
            Some(0),
            "{name}: v4 logged nothing either"
        );
    }

    // P4.148 (item 9): the Scenarios/ ensure failing on create — a trigger
    // refusing every `doc_mount_folders` INSERT, planted on both copies. v4:
    // INFO `Project created`, THEN WARN `Failed to ensure project Scenarios
    // folder on create {projectId, error}`, the 201 still answered.
    {
        let name = "create_scenarios_ensure_fails";
        let db = fresh_db(&spec, "cs_ensure");
        db.write_blocking(|ws| {
            ws.mount_index()
                .expect("mount present")
                .connection()
                .execute_batch(
                    "CREATE TRIGGER qt_p4148_no_folders BEFORE INSERT ON doc_mount_folders \
                 BEGIN SELECT RAISE(ABORT, 'planted: folder inserts refused'); END",
                )?;
            Ok(())
        })
        .expect("plant the folder trigger");
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(projects::project_create(
                &db,
                json!({ "name": "Tau", "color": "#abcdef", "icon": "rocket" }),
            ))
        });
        assert_eq!(
            oracle[name]["status"].as_i64(),
            Some(201),
            "{name}: v4's status"
        );
        pin_201(name, &resp, &mut failed);
        check(name, &response_data(&resp), true, &mut failed);
        // The project id is minted on each side — compare with it blanked.
        let blank_id = |l: String| match l.find("projectId=") {
            Some(i) => {
                let end = l[i..].find(' ').map_or(l.len(), |e| i + e);
                format!("{}projectId=<id>{}", &l[..i], &l[end..])
            }
            None => l,
        };
        let v4_logs: Vec<Value> = oracle[name]["logs"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut r| {
                if let Some(fields) = r["fields"].as_array_mut() {
                    for f in fields.iter_mut() {
                        if f[0] == "projectId" {
                            f[1] = json!("<id>");
                        }
                    }
                }
                r
            })
            .collect();
        let v5_lines: Vec<String> = lines.into_iter().map(blank_id).collect();
        if let Err(diff) = compare_projects_v1(&Value::Array(v4_logs), &v5_lines) {
            eprintln!("[{name}] [Projects v1] lines MISMATCH:{diff}");
            failed.push(format!("{name}_lines"));
        } else {
            eprintln!("[{name}] lines OK.");
        }
    }

    // P4.148 (R-F, item 10): a roster member's NULL `tags` cell reads `[]`.
    {
        let name = "get_iota_null_tags";
        let db = fresh_db(&spec, "null_tags");
        mutate(
            &db,
            "UPDATE characters SET tags = NULL WHERE id = ?1",
            vec![ARIA.to_string()],
        );
        check(
            name,
            &response_data(&projects::project_get(&db, IOTA)),
            false,
            &mut failed,
        );
    }
    // P4.155 (R-C): v4's other two `char.tags || []` sites over the same NULL
    // cell — the roster list and the list-chats participants. REGRESSION
    // PINS, not red-first: GREEN on unported core (measured), since
    // `characters_read` materializes a NULL `tags` cell as `[]` first.
    for (name, tag) in [
        ("list_characters_null_tags", "lc_null_tags"),
        ("list_chats_null_tags", "lch_null_tags"),
    ] {
        let db = fresh_db(&spec, tag);
        mutate(
            &db,
            "UPDATE characters SET tags = NULL WHERE id = ?1",
            vec![ARIA.to_string()],
        );
        let got = if name == "list_characters_null_tags" {
            projects::project_character_list(&db, IOTA)
        } else {
            projects::project_chat_list(&db, IOTA, None, None)
        };
        check(name, &response_data(&got), false, &mut failed);
    }

    // P4.148 (item 11): a roster naming a well-formed uuid with no character —
    // `_count.characters` stays the raw length (2); the enriched roster and
    // list-characters carry the one real member.
    {
        let name = "roster_missing_character";
        let db = fresh_db(&spec, "roster_missing");
        let put = rt.block_on(projects::project_update(
            &db,
            IOTA,
            json!({ "characterRoster": [ARIA, MISSING_CHARACTER] }),
        ));
        let get = projects::project_get(&db, IOTA);
        let list = projects::project_character_list(&db, IOTA);
        let got = json!({
            "put": response_data(&put),
            "get": response_data(&get),
            "list": response_data(&list),
        });
        check(name, &got, true, &mut failed);
        assert_eq!(
            oracle[name]["body"]["get"]["project"]["_count"]["characters"],
            json!(2),
            "{name}: v4 counts the raw roster"
        );
    }

    // Every row v4 answered 201 was pinned (a new success-create row that
    // skipped `pin_201` reddens here).
    let mut want_201: Vec<&String> = oracle
        .iter()
        .filter(|(_, r)| r["status"].as_i64() == Some(201))
        .map(|(n, _)| n)
        .collect();
    want_201.sort();
    let got_201 = pinned_201.borrow();
    assert_eq!(
        got_201.iter().collect::<Vec<_>>(),
        want_201,
        "every 201 row pinned"
    );

    assert!(failed.is_empty(), "projects-routes FAILED: {failed:?}");
}
