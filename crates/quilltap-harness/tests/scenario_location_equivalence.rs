//! Tier-2 differential (P4.D295): THE SCENARIO LOCATION —
//! `scenarios::location::{resolve_scenario_location, classify_scenario_mount,
//! lookup_scenario_ref}`, the locations' `list` / `read` / `id_for_path`, the
//! store tiers' `create` / `update` / `rename` / `delete` / `set_default`, and
//! `scenarios::everywhere::list_scenarios_everywhere` — vs v4's REAL
//! `lib/scenarios/location.ts` + `everywhere.ts` (`545e299bb`, pin
//! `83334b9a5`). The raw finders behind them (L-2 — `projects` / `groups`
//! `find_all_raw` / `find_by_id_raw_or_none` / `find_by_official_mount_point_
//! id_raw`, `characters_read::find_by_document_mount_point_id_raw`) are proven
//! here too: the catalogue's `"beta"` / `"Beta"` tie keeps v4's rowid order,
//! and the nameless project / group rows v4's row schema refuses are dropped.
//!
//! Both sides open a COPY of the run-time `build-scenario-location-fixture.ts`
//! pair and replay the oracle's op list IN ORDER on that one copy (the writes
//! are sequential state). Each row compares the result (a thrown error by its
//! v4 class for the location's own errors, by message otherwise), the
//! `doc_mount_points` count, which mounts hold a `Scenarios/` folder row, and
//! the captured scenario lines L1–L15 (`LEVEL message k=v`, arrays as `kJson`).
//! Excerpts are compared in UTF-16 units under `EXCERPT_LONE_SURROGATE` (R-AA).
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout or the round's
//! pin worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   LANG=en_US.UTF-8 TZ=UTC QT_FIXTURE_SL_MAIN=/tmp/qt-scenario-location-main.db \
//!   QT_FIXTURE_SL_MOUNT=/tmp/qt-scenario-location-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-scenario-location-fixture.ts
//!   LANG=en_US.UTF-8 TZ=UTC QT_FIXTURE_SL_MAIN=/tmp/qt-scenario-location-main.db \
//!   QT_FIXTURE_SL_MOUNT=/tmp/qt-scenario-location-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/scenario-location.ts \
//!     > /tmp/oracle-scenario-location.ndjson
//! Run:
//!   QT_ORACLE_SCENARIO_LOCATION=/tmp/oracle-scenario-location.ndjson \
//!   QT_FIXTURE_SL_MAIN=/tmp/qt-scenario-location-main.db \
//!   QT_FIXTURE_SL_MOUNT=/tmp/qt-scenario-location-mount.db \
//!     cargo test -p quilltap-harness --test scenario_location_equivalence

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::chats::ScenarioRef;
use quilltap_core::db::scenarios::ParsedScenario;
use quilltap_core::db::Writer;
use quilltap_core::scenarios::everywhere::list_scenarios_everywhere;
use quilltap_core::scenarios::location::{
    classify_scenario_mount, lookup_scenario_ref, resolve_scenario_location, CreateScenarioInput,
    ScenarioLocation, ScenarioLocationError, ScenarioRefLookup, ScenarioScope, UpdateScenarioInput,
};
use quilltap_core::test_support::captured_with;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    ids: HashMap<String, String>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/scenario-location.json")
}

/// The scenario lines this family pins — the v4 case captures the same set.
const PINNED_MESSAGES: &[&str] = &[
    "[ScenarioLocation] Resolved scenario location",
    "[ScenarioLocation] Failed to ensure Scenarios folder",
    "[ScenarioLocation] Created scenario",
    "[ScenarioLocation] Updated scenario",
    "[ScenarioLocation] Renamed scenario",
    "[ScenarioLocation] Deleted scenario",
    "[ScenarioLocation] Mount belongs to an unavailable character",
    "[ScenarioLocation] Refused scenario ref: bad path",
    "[ScenarioLocation] Refused scenario ref: mount is not a scenario owner",
    "[ScenarioLocation] Refused scenario ref: owner did not resolve",
    "[ScenarioLocation] Scenario ref points at a missing file",
    "[ScenariosEverywhere] Listed scenario shelves",
    "Scenario file body is empty; skipping",
    "Scenarios: multiple isDefault: true entries",
    "Scenarios: built list",
    "Failed to demote a previously-default scenario; alphabetical tiebreaker will compensate",
];

/// Ruled divergence `EXCERPT_LONE_SURROGATE` (R-AA) over the catalogue: the
/// `(row, source mount key, scenario path)` whose v4 excerpt carries a lone
/// high surrogate (the fixture's `Scenarios/long.md` in ALPHA), where v5 emits
/// U+FFFD. Both ways, as in `scenario_pure_equivalence`.
const EXCERPT_LONE_SURROGATE: &[&str] = &["Scenarios/long.md"];

/// Ops the corpus must carry (a truncated oracle must not pass).
const MIN_ROWS: usize = 85;

struct Env<'a> {
    main: &'a Connection,
    mount: &'a Connection,
    user_id: String,
    ids: HashMap<String, String>,
    mounts: HashMap<String, String>,
    known_mounts: HashSet<String>,
    known_ts: HashSet<String>,
}

impl Env<'_> {
    fn remap(&self, s: &str) -> String {
        let mut out = s.to_string();
        for id in all_mounts(self.mount) {
            if !self.known_mounts.contains(&id) {
                out = out.replace(&id, "<minted>");
            }
        }
        out
    }
    fn ts(&self, t: &str) -> String {
        if self.known_ts.contains(t) {
            t.to_string()
        } else {
            "<fresh>".to_string()
        }
    }
    fn id_of(&self, key: &Value) -> Option<String> {
        match key {
            Value::Null => None,
            Value::String(s) if s.is_empty() => Some(String::new()),
            Value::String(s) => Some(self.ids[s].clone()),
            other => panic!("id key {other}"),
        }
    }
    fn scen(&self, s: &ParsedScenario) -> Value {
        let mut m = Map::new();
        m.insert("path".into(), json!(s.path));
        m.insert("filename".into(), json!(s.filename));
        m.insert("name".into(), json!(s.name));
        if let Some(d) = &s.description {
            m.insert("description".into(), json!(d));
        }
        m.insert("isDefault".into(), json!(s.is_default));
        m.insert("rawIsDefault".into(), json!(s.raw_is_default));
        m.insert("archived".into(), json!(s.archived));
        m.insert("body".into(), json!(s.body));
        m.insert("lastModified".into(), json!(self.ts(&s.last_modified)));
        m.insert("createdAt".into(), json!(self.ts(&s.created_at)));
        m.insert("updatedAt".into(), json!(self.ts(&s.updated_at)));
        Value::Object(m)
    }
    fn shape(&self, l: Option<&ScenarioLocation>) -> Value {
        match l {
            None => Value::Null,
            Some(l) => json!({
                "scope": l.scope().as_str(),
                "id": l.id(),
                "mountPointId": self.remap(l.mount_point_id()),
                "ownerName": l.owner_name(),
            }),
        }
    }
}

fn all_mounts(mount: &Connection) -> Vec<String> {
    let mut stmt = mount
        .prepare("SELECT id FROM doc_mount_points")
        .expect("prepare");
    stmt.query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect()
}

fn scope_of(s: &str) -> ScenarioScope {
    match s {
        "character" => ScenarioScope::Character,
        "project" => ScenarioScope::Project,
        "group" => ScenarioScope::Group,
        "general" => ScenarioScope::General,
        other => panic!("scope {other}"),
    }
}

/// A v5 error as the v4 row records it: `{ threw: { name, message } }`. The
/// location's own errors carry v4's class name; anything else is compared by
/// message alone (v4's name is whatever the driver threw).
fn threw(e: &ScenarioLocationError, env: &Env<'_>) -> Value {
    let name = match e {
        ScenarioLocationError::NotFound(_) => "ScenarioNotFoundError",
        ScenarioLocationError::Conflict(_) => "ScenarioConflictError",
        ScenarioLocationError::Filename(_) => "ScenarioFilenameError",
        ScenarioLocationError::Uri(_) => "URIError",
        ScenarioLocationError::Other(_) => "<other>",
    };
    json!({ "threw": { "name": name, "message": env.remap(&e.to_string()) } })
}

fn same_threw(v4: &Value, v5: &Value) -> bool {
    let (Some(a), Some(b)) = (v4.get("threw"), v5.get("threw")) else {
        return false;
    };
    let names_agree = b["name"] == "<other>" || a["name"] == b["name"];
    names_agree && a["message"] == b["message"]
}

fn str_input(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn units(s: &str) -> Value {
    Value::Array(s.encode_utf16().map(|u| json!(u)).collect())
}

/// v5's catalogue in the oracle's rendering (excerpt → `excerptUnits`,
/// timestamps normalized), with R-AA's U+FFFD mapped back to v4's lone
/// surrogate for the named rows — and every named row CHECKED to carry one.
fn everywhere_shape(sources: Vec<Value>, env: &Env<'_>, ruled_seen: &mut Vec<String>) -> Value {
    Value::Array(
        sources
            .into_iter()
            .map(|mut src| {
                let mp = src["mountPointId"].as_str().unwrap().to_string();
                src["mountPointId"] = json!(env.remap(&mp));
                let scenarios = src["scenarios"].as_array().unwrap().clone();
                src["scenarios"] = Value::Array(
                    scenarios
                        .into_iter()
                        .map(|s| {
                            let mut out = Map::new();
                            for (k, v) in s.as_object().unwrap() {
                                match k.as_str() {
                                    "excerpt" => {}
                                    "updatedAt" => {
                                        out.insert("excerptUnits".into(), units(s["excerpt"].as_str().unwrap()));
                                        out.insert(k.clone(), json!(env.ts(v.as_str().unwrap())));
                                    }
                                    _ => {
                                        out.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                            let path = s["path"].as_str().unwrap();
                            if EXCERPT_LONE_SURROGATE.contains(&path) {
                                let u = out.get_mut("excerptUnits").unwrap().as_array_mut().unwrap();
                                let k = u.iter().position(|x| x == 0xFFFD).unwrap_or_else(|| {
                                    panic!("{path}: ruled EXCERPT_LONE_SURROGATE but v5 emits no U+FFFD")
                                });
                                // v4's lone high surrogate is the first half of
                                // the pair the cut split — the fixture's 🎭.
                                u[k] = json!(0xD83Cu16);
                                ruled_seen.push(path.to_string());
                            }
                            Value::Object(out)
                        })
                        .collect(),
                );
                src
            })
            .collect(),
    )
}

fn normalize_lines(lines: Vec<String>, env: &Env<'_>) -> Vec<String> {
    lines
        .into_iter()
        .filter_map(|l| {
            let (level, rest) = l.split_once(' ')?;
            let (_target, rest) = rest.split_once(' ')?;
            if !PINNED_MESSAGES.iter().any(|m| rest.starts_with(m)) {
                return None;
            }
            Some(format!("{level} {}", env.remap(rest)))
        })
        .collect()
}

fn run_op(op: &Value, env: &Env<'_>, ruled_seen: &mut Vec<String>) -> Value {
    let kind = op["op"].as_str().unwrap();
    let (main, mount, uid) = (env.main, env.mount, env.user_id.as_str());
    let resolve = |op: &Value, ensure: bool| {
        resolve_scenario_location(
            main,
            mount,
            scope_of(op["scope"].as_str().unwrap()),
            env.id_of(&op["id"]).as_deref(),
            uid,
            ensure,
        )
    };
    let unwrap_loc = |op: &Value| resolve(op, false).expect("resolve").expect("location");
    match kind {
        "resolve" => match resolve(op, op["ensure"].as_bool().unwrap()) {
            Ok(l) => env.shape(l.as_ref()),
            Err(e) => threw(&e, env),
        },
        "classify" => {
            match classify_scenario_mount(main, &env.mounts[op["mount"].as_str().unwrap()], uid) {
                Ok(None) => Value::Null,
                Ok(Some((scope, id))) => json!({ "scope": scope.as_str(), "id": id }),
                Err(e) => threw(&e, env),
            }
        }
        "lookup" => {
            let r = ScenarioRef {
                mount_point_id: env.mounts[op["mount"].as_str().unwrap()].clone(),
                path: op["path"].as_str().unwrap().to_string(),
            };
            match lookup_scenario_ref(main, mount, &r, uid) {
                Ok(ScenarioRefLookup::Refused(reason)) => {
                    json!({ "status": "refused", "reason": reason })
                }
                Ok(ScenarioRefLookup::Missing { location, path }) => {
                    json!({ "status": "missing", "location": env.shape(Some(&location)), "path": path })
                }
                Ok(ScenarioRefLookup::Ok { location, scenario }) => json!({
                    "status": "ok",
                    "location": env.shape(Some(&location)),
                    "scenario": env.scen(&scenario),
                    "scenarioId": location.id_for_path(&scenario.path),
                }),
                Err(e) => threw(&e, env),
            }
        }
        "list" => {
            let l = unwrap_loc(op);
            match l.list(mount, op["includeArchived"].as_bool().unwrap()) {
                Ok(r) => json!({
                    "scenarios": r.scenarios.iter().map(|s| env.scen(s)).collect::<Vec<_>>(),
                    "warnings": r.warnings,
                }),
                Err(e) => threw(&e, env),
            }
        }
        "read" => {
            let l = unwrap_loc(op);
            match l.read(mount, op["path"].as_str().unwrap()) {
                Ok(s) => s.map_or(Value::Null, |s| env.scen(&s)),
                Err(e) => threw(&e, env),
            }
        }
        "idForPath" => json!(unwrap_loc(op).id_for_path(op["path"].as_str().unwrap())),
        "create" | "update" | "rename" | "delete" | "setDefault" => {
            let l = unwrap_loc(op);
            let store = l.as_store().expect("a store location");
            let out = match kind {
                "create" => {
                    let i = &op["input"];
                    store
                        .create(
                            mount,
                            &CreateScenarioInput {
                                filename: str_input(i, "filename").unwrap(),
                                name: str_input(i, "name"),
                                description: str_input(i, "description"),
                                is_default: i.get("isDefault").and_then(Value::as_bool),
                                archived: i.get("archived").and_then(Value::as_bool),
                                body: str_input(i, "body").unwrap(),
                            },
                        )
                        .map(|s| env.scen(&s))
                }
                "update" => {
                    let p = &op["patch"];
                    store
                        .update(
                            mount,
                            op["path"].as_str().unwrap(),
                            &UpdateScenarioInput {
                                name: str_input(p, "name"),
                                description: str_input(p, "description"),
                                is_default: p.get("isDefault").and_then(Value::as_bool),
                                archived: p.get("archived").and_then(Value::as_bool),
                                body: str_input(p, "body").unwrap(),
                            },
                        )
                        .map(|s| env.scen(&s))
                }
                "rename" => store
                    .rename(
                        mount,
                        op["path"].as_str().unwrap(),
                        op["filename"].as_str().unwrap(),
                    )
                    .map(|s| env.scen(&s)),
                "delete" => store
                    .delete(mount, op["path"].as_str().unwrap())
                    .map(|()| Value::Null),
                _ => store
                    .set_default(mount, op["path"].as_str())
                    .map(|()| Value::Null),
            };
            out.unwrap_or_else(|e| threw(&e, env))
        }
        "everywhere" => match list_scenarios_everywhere(
            main,
            mount,
            uid,
            op["includeArchived"].as_bool().unwrap(),
        ) {
            Ok(sources) => everywhere_shape(sources, env, ruled_seen),
            Err(e) => json!({ "threw": { "name": "<other>", "message": e.to_string() } }),
        },
        "breakFolders" => {
            mount
                .execute_batch("DROP TABLE doc_mount_folders")
                .expect("break folders");
            Value::Null
        }
        other => panic!("unknown op {other}"),
    }
}

fn scenario_folders(env: &Env<'_>, broken: bool) -> Value {
    if broken {
        return json!(["<broken>"]);
    }
    let mut stmt = env
        .mount
        .prepare("SELECT mountPointId FROM doc_mount_folders WHERE LOWER(path) = 'scenarios'")
        .expect("prepare");
    let mut out: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .map(|r| env.remap(&r.expect("row")))
        .collect();
    out.sort();
    json!(out)
}

#[test]
fn scenario_location_matches_oracle() {
    let Some(oracle_path) = std::env::var("QT_ORACLE_SCENARIO_LOCATION").ok() else {
        eprintln!("SKIP: set QT_ORACLE_SCENARIO_LOCATION to the oracle NDJSON (see header).");
        return;
    };
    let (Some(main_fixture), Some(mount_fixture)) = (
        std::env::var("QT_FIXTURE_SL_MAIN").ok(),
        std::env::var("QT_FIXTURE_SL_MOUNT").ok(),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_SL_MAIN / QT_FIXTURE_SL_MOUNT (see header).");
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec"))
            .expect("spec");
    let rows: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    assert!(
        rows.len() >= MIN_ROWS,
        "{} rows, floor {MIN_ROWS} — truncated oracle?",
        rows.len()
    );

    let scratch = tempfile::Builder::new()
        .prefix("qt-sl-rust-")
        .tempdir()
        .expect("tempdir");
    let main_work = scratch.path().join("main.db");
    let mount_work = scratch.path().join("mount.db");
    std::fs::copy(&main_fixture, &main_work).expect("copy main");
    std::fs::copy(&mount_fixture, &mount_work).expect("copy mount");
    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64).expect("open main");
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64).expect("open mount");
    let (main, mount) = (main_w.connection(), mount_w.connection());

    let col = |table: &str, column: &str, key: &str| -> String {
        main.query_row(
            &format!("SELECT \"{column}\" FROM \"{table}\" WHERE id = ?1"),
            [&spec.ids[key]],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|e| panic!("{table}.{column} of {key}: {e}"))
    };
    let mounts: HashMap<String, String> = [
        ("general".to_string(), spec.ids["general"].clone()),
        ("linked".to_string(), spec.ids["linked"].clone()),
        ("arbitrary".to_string(), spec.ids["arbitrary"].clone()),
        ("unknown".to_string(), spec.ids["unknown"].clone()),
        (
            "ada.vault".to_string(),
            col("characters", "characterDocumentMountPointId", "ada"),
        ),
        (
            "zed.vault".to_string(),
            col("characters", "characterDocumentMountPointId", "zed"),
        ),
        (
            "otto.vault".to_string(),
            col("characters", "characterDocumentMountPointId", "otto"),
        ),
        (
            "alpha.store".to_string(),
            col("projects", "officialMountPointId", "alpha"),
        ),
        (
            "guild.store".to_string(),
            col("groups", "officialMountPointId", "guild"),
        ),
        (
            "nameless.store".to_string(),
            col("projects", "officialMountPointId", "nameless"),
        ),
    ]
    .into_iter()
    .collect();
    let mut known_ts = HashSet::new();
    {
        let mut stmt = mount
            .prepare("SELECT d.createdAt, d.updatedAt, l.lastModified FROM doc_mount_documents d JOIN doc_mount_file_links l ON l.fileId = d.fileId")
            .expect("prepare");
        for r in stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .expect("query")
        {
            let (c, u, m) = r.expect("row");
            known_ts.extend([c, u, m]);
        }
    }
    let env = Env {
        main,
        mount,
        user_id: spec.user_id.clone(),
        ids: spec.ids.clone(),
        mounts,
        known_mounts: all_mounts(mount).into_iter().collect(),
        known_ts,
    };

    let mut failures: Vec<String> = Vec::new();
    let mut ruled_seen: Vec<String> = Vec::new();
    let mut broken = false;
    for row in &rows {
        let name = row["name"].as_str().unwrap();
        let op = &row["op"];
        let (result, lines) = captured_with(|| run_op(op, &env, &mut ruled_seen));
        if op["op"] == "breakFolders" {
            broken = true;
        }
        let lines = normalize_lines(lines, &env);
        let want = &row["result"];
        let results_agree = if want.get("threw").is_some() {
            same_threw(want, &result)
        } else {
            serde_json::to_string(want).unwrap() == serde_json::to_string(&result).unwrap()
        };
        if !results_agree {
            failures.push(format!("[{name}] result\n  v4: {want}\n  v5: {result}"));
        }
        let points: i64 = mount
            .query_row("SELECT COUNT(*) FROM doc_mount_points", [], |r| r.get(0))
            .unwrap();
        if json!(points) != row["points"] {
            failures.push(format!(
                "[{name}] doc_mount_points v4 {} v5 {points}",
                row["points"]
            ));
        }
        let folders = scenario_folders(&env, broken);
        if folders != row["scenarioFolders"] {
            failures.push(format!(
                "[{name}] Scenarios/ folders\n  v4: {}\n  v5: {folders}",
                row["scenarioFolders"]
            ));
        }
        let want_lines: Vec<String> = row["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        if want_lines != lines {
            failures.push(format!(
                "[{name}] lines\n  v4: {want_lines:#?}\n  v5: {lines:#?}"
            ));
        }
    }
    for path in EXCERPT_LONE_SURROGATE {
        assert!(
            ruled_seen.iter().any(|p| p == path),
            "EXCERPT_LONE_SURROGATE names {path}, never seen"
        );
    }
    assert!(
        failures.is_empty(),
        "{} mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    println!("scenario_location: {} rows match v4", rows.len());
}
