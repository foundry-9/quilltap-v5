//! P4.D281 WARDROBE-FOLDERS-ROUTES differential (v4 `4a38222e6`): the five
//! folder actions — `GET ?action=folders`, `POST ?action=create-folder /
//! rename-folder / delete-folder / move-items` — and `folder` on the
//! collection POST / item PUT, through `api::wardrobe_route_factory` on all
//! four tiers, vs v4's REAL route factory behind the four registered route
//! files (`harness/oracle/cases/wardrobe-folders-routes.test.ts`).
//!
//! THE CASE LIST LIVES IN `wardrobe-folders-routes.json#cases` — the corpus
//! the oracle reads too; this test asserts its case count against the NDJSON's
//! row count, so a half-regenerated oracle cannot partial-pass. Each case is a
//! list of steps over ONE fresh copy of the keystone's four-tier seed (built
//! from `wardrobe-keystone-tier2.json` by `build-wardrobe-keystone-fixture.ts`
//! into this family's OWN paths — the keystone's seed consumed read-only).
//!
//! Compared per step: the status, and for a success the body BYTES (key
//! order included — `serde_json`'s `preserve_order` map equality is
//! order-blind, so both sides are rendered with `to_string`) after blanking
//! every `createdAt` / `updatedAt` (a move restamps `updatedAt`) and a
//! create's minted `wardrobeItem.id`; for a refusal the status and v4's
//! sentence (Zod's `details` array is the standing project-wide deferral).
//! Then the case's mount-index write-set: every `doc_mount_file_links`
//! relative path and every `doc_mount_folders` path, per mount.
//!
//! Generate the fixture + oracle (see the .ts header), then:
//!   QT_ORACLE_WARDROBE_FOLDERS_ROUTES=/tmp/oracle-wardrobe-folders-routes.ndjson \
//!   QT_FIXTURE_WFR_MAIN=/tmp/qt-wfr-main.db QT_FIXTURE_WFR_MOUNT=/tmp/qt-wfr-mount.db \
//!     cargo test -p quilltap-web --test wardrobe_folders_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response, WardrobeContainerScope};
use quilltap_core::api::wardrobe_route_factory::{
    self as factory, FolderAction, WardrobeRouteConfig, CHARACTER_WARDROBE, GENERAL_WARDROBE,
    GROUP_WARDROBE, PROJECT_WARDROBE,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Step {
    op: String,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    item: Option<String>,
    #[serde(default)]
    items: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Plant {
    sql: String,
    params: Vec<Value>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    tier: String,
    #[serde(default)]
    owner: Option<String>,
    steps: Vec<Step>,
    #[serde(default)]
    plants: Vec<Plant>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    owner_ids: HashMap<String, String>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Seed {
    test_pepper_base64: String,
    user_a_id: String,
    items: HashMap<String, SeedItem>,
}

#[derive(Deserialize)]
struct SeedItem {
    id: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

fn http_for(kind: ErrorKind) -> i64 {
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

/// `(status, body)` as v4's transport would answer: a create is 201.
fn answer(op: &str, resp: &Response) -> (i64, Value) {
    match resp {
        Response::Error(e) => (http_for(e.kind), json!({ "error": e.message })),
        Response::Character(v)
        | Response::Wardrobe(v)
        | Response::Project(v)
        | Response::Group(v) => (if op == "create" { 201 } else { 200 }, v.clone()),
        other => panic!("unexpected response variant: {other:?}"),
    }
}

/// Blank every `createdAt` / `updatedAt` (a move restamps `updatedAt`; a
/// create mints both) and, for a create, the minted id.
fn normalize(v: &Value, op: &str) -> Value {
    fn walk(v: &mut Value) {
        match v {
            Value::Object(m) => {
                for (k, x) in m.iter_mut() {
                    if (k == "createdAt" || k == "updatedAt") && x.is_string() {
                        *x = Value::String("<ts>".into());
                    } else {
                        walk(x);
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(walk),
            _ => {}
        }
    }
    let mut out = v.clone();
    walk(&mut out);
    if op == "create" {
        if let Some(id) = out.pointer_mut("/wardrobeItem/id") {
            *id = Value::String("<minted>".into());
        }
    }
    out
}

fn config(tier: &str) -> WardrobeRouteConfig {
    match tier {
        "character" => CHARACTER_WARDROBE,
        "general" => GENERAL_WARDROBE,
        "project" => PROJECT_WARDROBE,
        "group" => GROUP_WARDROBE,
        other => panic!("unknown tier {other}"),
    }
}

fn dump(mount_path: &std::path::Path, pepper: &str) -> (Value, Value) {
    let w = Writer::open_writable(mount_path, pepper).expect("open mount for the dump");
    let c = w.connection();
    let rows = |sql: &str, a: &str, b: &str| -> Value {
        let mut stmt = c.prepare(sql).expect("prepare dump");
        let out: Vec<Value> = stmt
            .query_map([], |r| {
                Ok(json!({ a: r.get::<_, String>(0)?, b: r.get::<_, String>(1)? }))
            })
            .expect("dump query")
            .map(|r| r.expect("dump row"))
            .collect();
        Value::Array(out)
    };
    (
        rows(
            "SELECT \"mountPointId\", \"relativePath\" FROM \"doc_mount_file_links\" \
             ORDER BY \"mountPointId\", \"relativePath\"",
            "mountPointId",
            "relativePath",
        ),
        rows(
            "SELECT \"mountPointId\", \"path\" FROM \"doc_mount_folders\" \
             ORDER BY \"mountPointId\", \"path\"",
            "mountPointId",
            "path",
        ),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn wardrobe_folders_routes_match_oracle() {
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_WARDROBE_FOLDERS_ROUTES"),
        std::env::var("QT_FIXTURE_WFR_MAIN"),
        std::env::var("QT_FIXTURE_WFR_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_WARDROBE_FOLDERS_ROUTES + QT_FIXTURE_WFR_MAIN + QT_FIXTURE_WFR_MOUNT (see header)."
        );
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("wardrobe-folders-routes.json")).unwrap(),
    )
    .expect("parse corpus");
    let seed: Seed = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("wardrobe-keystone-tier2.json")).unwrap(),
    )
    .expect("parse seed");
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    assert_eq!(
        oracle.len(),
        spec.cases.len(),
        "oracle row count != corpus case count — regenerate the NDJSON"
    );

    let user = seed.user_a_id.as_str();
    let item_id = |r: &str| {
        seed.items
            .get(r)
            .map(|i| i.id.clone())
            .unwrap_or(r.to_string())
    };
    let mut failed: Vec<String> = Vec::new();

    for case in &spec.cases {
        let want = oracle
            .get(&case.name)
            .unwrap_or_else(|| panic!("{}: no oracle row", case.name));
        let scratch = tempfile::tempdir().unwrap();
        let main_path = scratch.path().join("main.db");
        let mount_path = scratch.path().join("mount.db");
        std::fs::copy(&main_fixture, &main_path).unwrap();
        std::fs::copy(&mount_fixture, &mount_path).unwrap();
        if !case.plants.is_empty() {
            let w = Writer::open_writable(&main_path, &seed.test_pepper_base64).unwrap();
            for p in &case.plants {
                let params: Vec<rusqlite::types::Value> = p
                    .params
                    .iter()
                    .map(|v| match v {
                        Value::Null => rusqlite::types::Value::Null,
                        Value::Number(n) => rusqlite::types::Value::Integer(n.as_i64().unwrap()),
                        other => rusqlite::types::Value::Text(
                            other
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or(other.to_string()),
                        ),
                    })
                    .collect();
                w.connection()
                    .execute(&p.sql, rusqlite::params_from_iter(params))
                    .unwrap();
            }
        }
        let db = Db::open(
            DbPaths {
                main: main_path.clone(),
                mount_index: Some(mount_path.clone()),
                llm_logs: None,
            },
            &seed.test_pepper_base64,
        )
        .expect("open the case's copy");

        let cfg = config(&case.tier);
        let scope: WardrobeContainerScope =
            serde_json::from_value(Value::String(case.tier.clone())).unwrap();
        let owner = if case.tier == "general" {
            None
        } else {
            let r = case.owner.as_deref().unwrap_or_default();
            Some(spec.owner_ids.get(r).cloned().unwrap_or(r.to_string()))
        };
        let owner = owner.as_deref();

        let want_steps = want["steps"].as_array().expect("oracle steps");
        assert_eq!(
            want_steps.len(),
            case.steps.len(),
            "{}: step count",
            case.name
        );
        for (i, (step, want_step)) in case.steps.iter().zip(want_steps).enumerate() {
            let body = step.body.clone().unwrap_or(Value::Null);
            let resp = match step.op.as_str() {
                "folders" => {
                    factory::folder_action(&db, FolderAction::List, scope, owner, user, &body).await
                }
                "list" => {
                    if case.tier == "character" {
                        factory::list_character_items(&db, user, owner.unwrap(), None, false)
                    } else {
                        factory::list_store_items(&db, cfg, owner, user, false).await
                    }
                }
                "create-folder" => {
                    factory::folder_action(&db, FolderAction::Create, scope, owner, user, &body)
                        .await
                }
                "rename-folder" => {
                    factory::folder_action(&db, FolderAction::Rename, scope, owner, user, &body)
                        .await
                }
                "delete-folder" => {
                    factory::folder_action(&db, FolderAction::Delete, scope, owner, user, &body)
                        .await
                }
                "move-items" => {
                    let mut body = body.clone();
                    if let Some(items) = &step.items {
                        if !body.is_object() {
                            body = json!({});
                        }
                        body["itemIds"] =
                            Value::Array(items.iter().map(|r| Value::String(item_id(r))).collect());
                    }
                    factory::folder_action(&db, FolderAction::MoveItems, scope, owner, user, &body)
                        .await
                }
                "create" => factory::create(&db, cfg, owner, user, &body).await,
                "update" => {
                    let iid = item_id(step.item.as_deref().unwrap_or_default());
                    factory::item_update(&db, cfg, owner, user, &iid, body.clone()).await
                }
                other => panic!("unknown step op {other}"),
            };
            let (status, got) = answer(&step.op, &resp);
            let want_status = want_step["status"].as_i64().unwrap();
            let label = format!("{} step {i} ({})", case.name, step.op);
            if status != want_status {
                failed.push(format!(
                    "{label}: status {status} vs oracle {want_status} — {got}"
                ));
                continue;
            }
            if status >= 400 {
                if got["error"] != want_step["body"]["error"] {
                    failed.push(format!(
                        "{label}: error {} vs oracle {}",
                        got["error"], want_step["body"]["error"]
                    ));
                }
                continue;
            }
            let (g, w) = (
                serde_json::to_string(&normalize(&got, &step.op)).unwrap(),
                serde_json::to_string(&normalize(&want_step["body"], &step.op)).unwrap(),
            );
            if g != w {
                failed.push(format!(
                    "{label}: body diverged\n  rust:   {g}\n  oracle: {w}"
                ));
            }
        }
        drop(db);

        let (links, folders) = dump(&mount_path, &seed.test_pepper_base64);
        if links != want["links"] {
            failed.push(format!(
                "{}: links diverged\n  rust:   {links}\n  oracle: {}",
                case.name, want["links"]
            ));
        }
        if folders != want["folders"] {
            failed.push(format!(
                "{}: folders diverged\n  rust:   {folders}\n  oracle: {}",
                case.name, want["folders"]
            ));
        }
    }

    assert!(
        failed.is_empty(),
        "{} divergence(s):\n{}",
        failed.len(),
        failed.join("\n")
    );
}
