//! P4.D119 — the four `?action=instructions` GET/POST surfaces (v4 `b86bb1a5`)
//! against v5's eight dispatch verbs.
//!
//! Drives `api::characters::character_wardrobe_instructions_{get,set}`,
//! `api::groups::group_wardrobe_instructions_{get,set}`,
//! `api::projects::project_wardrobe_instructions_{get,set}` and
//! `api::wardrobe::wardrobe_instructions_{get,set}` through the SAME
//! `routeCases` corpus the oracle drives v4's REAL route handlers with, over a
//! FRESH copy of the committed `wardrobe-instructions-{main,mount}.db` pair per
//! case. Compared: the HTTP status (through the `ErrorKind` mapping — v5's
//! surfaces are dispatch verbs, so the status is derived exactly as
//! `quilltap-web`'s `unwrap_to_http` derives it), the body, and a whole-table
//! dump of the mount index, which is what proves an error arm wrote NOTHING and
//! a clear actually removed the file.
//!
//! ⚠ **The bodies are decoded THROUGH the `Request` enum**, not built by hand:
//! `instructions` is v4's `z.string().nullable()` — REQUIRED but nullable — so
//! the absent / explicit-`null` / value tri-state has to survive the edge or the
//! `{}` → flat `Validation error` arm becomes untestable (memories:
//! `edge-must-decode-through-the-request-enum.md`,
//! `harness-spec-typed-option-collapses-tristate.md`).
//!
//! Zod's `details` issue array is NOT ported (the standing project-wide
//! deferral, P4.6ay unit 12): only the decision and the `error` sentence are
//! compared, and `details` is dropped from the oracle's body before the diff.
//!
//! Generate the oracle: see the `.ts` case header
//! (`harness/oracle/cases/wardrobe-instructions-routes.test.ts`) — jest ignores
//! `.claude/` venues, so the case + spec are copied to a /tmp mirror.
//!   … QT_ORACLE_OUT=/tmp/oracle-wardrobe-instructions-routes.ndjson npx jest -- wardrobe-instructions-routes
//!
//! Run:
//!   QT_ORACLE_WARDROBE_INSTRUCTIONS_ROUTES=/tmp/oracle-wardrobe-instructions-routes.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_instructions_routes_equivalence
//!
//! Skips (does not fail) when the env var is unset — the standing gated-
//! differential discipline.

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Request, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "vaultlessCharacterId")]
    vaultless_character_id: String,
    #[serde(rename = "archivedCharacterId")]
    archived_character_id: String,
    #[serde(rename = "missingCharacterId")]
    missing_character_id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "groupId")]
    group_id: String,
    #[serde(rename = "missingProjectId")]
    missing_project_id: String,
    #[serde(rename = "missingGroupId")]
    missing_group_id: String,
    #[serde(rename = "generalMountPointId")]
    general_mount_point_id: String,
    #[serde(rename = "extraStores")]
    extra_stores: Vec<ExtraStore>,
    #[serde(rename = "routeCases")]
    route_cases: Vec<Value>,
}

#[derive(Deserialize)]
struct ExtraStore {
    label: String,
    id: String,
}

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "doc_mount_file_links",
        oracle_key: "links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-instructions.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// `quilltap-web`'s `unwrap_to_http` error mapping.
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

fn response_data(resp: &Response) -> Value {
    match resp {
        Response::Character(v)
        | Response::Group(v)
        | Response::Project(v)
        | Response::Wardrobe(v) => v.clone(),
        other => panic!("unexpected response variant: {other:?}"),
    }
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row object");
        for col in spec.id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

#[test]
fn wardrobe_instructions_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WARDROBE_INSTRUCTIONS_ROUTES") else {
        eprintln!(
            "SKIP: set QT_ORACLE_WARDROBE_INSTRUCTIONS_ROUTES to the oracle NDJSON (see header)."
        );
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse oracle line");
            (s(&v, "name").expect("oracle row name"), v)
        })
        .collect();
    assert_eq!(
        oracle.len(),
        spec.route_cases.len(),
        "oracle row count must equal the shared corpus's case count"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    for case in &spec.route_cases {
        let name = s(case, "name").expect("case name");
        let scope = s(case, "scope").expect("case scope");
        let method = s(case, "method").expect("case method");
        let target = s(case, "target");
        let want = oracle
            .get(&name)
            .unwrap_or_else(|| panic!("oracle missing case {name}"));

        // An unknown `?action=` is answered by v4's DISPATCHER, which v5 has no
        // analogue for: its verbs are named, so an unknown verb never reaches a
        // wardrobe handler at all. Recorded, not silently skipped.
        if case.get("action").is_some() {
            assert_eq!(
                want["status"].as_i64(),
                Some(400),
                "{name}: the unknown-action row must still be v4's 400"
            );
            continue;
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let main = dir.path().join("main.db");
        let mount = dir.path().join("mount.db");
        std::fs::copy(fixtures_dir().join("wardrobe-instructions-main.db"), &main)
            .expect("copy main");
        std::fs::copy(
            fixtures_dir().join("wardrobe-instructions-mount.db"),
            &mount,
        )
        .expect("copy mount");
        let db = Db::open(
            DbPaths {
                main,
                mount_index: Some(mount.clone()),
                llm_logs: None,
            },
            &spec.test_pepper_base64,
        )
        .expect("open db");

        // Per-case seeding through the RAW document-store write (the helpers
        // under test never seed), then the General un-provisioning.
        if let Some(seed) = case.get("seed").and_then(Value::as_object) {
            let labels = mount_labels(&db, &spec);
            for (label, content) in seed {
                let mp = labels
                    .get(label)
                    .unwrap_or_else(|| panic!("{name}: no mount for seed label {label}"))
                    .clone();
                let content = content.as_str().unwrap_or_default().to_string();
                db.write_blocking(move |w| {
                    let links = w
                        .mount_index()
                        .expect("mount writer")
                        .doc_mount_file_links();
                    links.ensure_folder_path(&mp, "Wardrobe")?;
                    links.write_database_document(&mp, "Wardrobe/instructions.md", &content)?;
                    Ok(())
                })
                .expect("seed");
            }
        }
        if case
            .get("unprovisionGeneral")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            db.write_blocking(|w| {
                w.main().connection().execute(
                    "DELETE FROM \"instance_settings\" WHERE \"key\" = ?1",
                    ["generalMountPointId"],
                )?;
                Ok(())
            })
            .expect("unprovision general");
        }

        let id = match scope.as_str() {
            "character" => match target.as_deref() {
                Some("missing") => spec.missing_character_id.clone(),
                Some("archived") => spec.archived_character_id.clone(),
                Some("vaultless") => spec.vaultless_character_id.clone(),
                _ => spec.character_id.clone(),
            },
            "group" => match target.as_deref() {
                Some("missing") => spec.missing_group_id.clone(),
                _ => spec.group_id.clone(),
            },
            "project" => match target.as_deref() {
                Some("missing") => spec.missing_project_id.clone(),
                _ => spec.project_id.clone(),
            },
            _ => String::new(),
        };

        let resp = if method == "GET" {
            match scope.as_str() {
                "character" => {
                    quilltap_core::api::characters::character_wardrobe_instructions_get(&db, &id)
                }
                "group" => quilltap_core::api::groups::group_wardrobe_instructions_get(&db, &id),
                "project" => {
                    quilltap_core::api::projects::project_wardrobe_instructions_get(&db, &id)
                }
                _ => quilltap_core::api::wardrobe::wardrobe_instructions_get(&db),
            }
        } else {
            // Decode through the Request enum so the tri-state survives.
            let body = case.get("body").cloned().unwrap_or(Value::Null);
            let (tag, key) = match scope.as_str() {
                "character" => ("characterWardrobeInstructionsSet", Some("characterId")),
                "group" => ("groupWardrobeInstructionsSet", Some("groupId")),
                "project" => ("projectWardrobeInstructionsSet", Some("projectId")),
                _ => ("wardrobeInstructionsSet", None),
            };
            let req = quilltap_core::api::types::wardrobe_instructions_set_request(
                &body,
                tag,
                key.map(|k| (k, id.as_str())),
            )
            .unwrap_or_else(|| panic!("{name}: the SET body did not decode into its variant"));
            let instructions = match &req {
                Request::CharacterWardrobeInstructionsSet { instructions, .. }
                | Request::GroupWardrobeInstructionsSet { instructions, .. }
                | Request::ProjectWardrobeInstructionsSet { instructions, .. }
                | Request::WardrobeInstructionsSet { instructions } => instructions.clone(),
                other => panic!("{name}: unexpected variant {other:?}"),
            };
            match scope.as_str() {
                "character" => rt.block_on(
                    quilltap_core::api::characters::character_wardrobe_instructions_set(
                        &db,
                        &id,
                        &instructions,
                    ),
                ),
                "group" => {
                    rt.block_on(quilltap_core::api::groups::group_wardrobe_instructions_set(
                        &db,
                        &id,
                        &instructions,
                    ))
                }
                "project" => rt.block_on(
                    quilltap_core::api::projects::project_wardrobe_instructions_set(
                        &db,
                        &id,
                        &instructions,
                    ),
                ),
                _ => rt.block_on(quilltap_core::api::wardrobe::wardrobe_instructions_set(
                    &db,
                    &instructions,
                )),
            }
        };

        let want_status = want["status"].as_i64().expect("oracle status");
        match &resp {
            Response::Error(e) => {
                // Zod's `details` array is the standing project-wide deferral:
                // only the decision and the sentence are compared.
                assert_eq!(
                    (http_for(e.kind), e.message.as_str()),
                    (want_status, want["body"]["error"].as_str().unwrap_or("")),
                    "{name}: error shape"
                );
            }
            ok => {
                assert_eq!(
                    want_status, 200,
                    "{name}: rust succeeded but oracle refused"
                );
                let got_body = response_data(ok);
                let want_body = want["body"].clone();
                assert_eq!(
                    got_body, want_body,
                    "{name}: body diverged\n  rust:   {got_body}\n  oracle: {want_body}"
                );
            }
        }

        // The tables prove the write (or its absence). Read them back through a
        // fresh open so the writer's transaction is committed.
        let mut got: Vec<Value> = TABLES
            .iter()
            .map(|t| {
                db.read_mount_index(|conn| {
                    quilltap_core::db::dump_table_json_conn(conn, t.table, t.order_by)
                })
                .unwrap_or_else(|e| panic!("{name}: dump {}: {e}", t.table))
            })
            .collect();
        let mut expected: Vec<Value> = TABLES
            .iter()
            .map(|t| {
                want.get("tables")
                    .and_then(|v| v.get(t.oracle_key))
                    .cloned()
                    .unwrap_or_else(|| panic!("{name}: oracle missing dump for {}", t.oracle_key))
            })
            .collect();
        normalize_all(&mut got);
        normalize_all(&mut expected);
        for (i, t) in TABLES.iter().enumerate() {
            assert_eq!(
                got[i]["rows"], expected[i]["rows"],
                "{name}/{}: row state diverged\n  rust:   {}\n  oracle: {}",
                t.table, got[i]["rows"], expected[i]["rows"]
            );
        }
    }

    eprintln!(
        "OK: wardrobe-instructions routes matched oracle ({} cases).",
        spec.route_cases.len()
    );
}

/// label → minted mount id, resolved the way the oracle resolves it.
fn mount_labels(db: &Db, spec: &Spec) -> HashMap<String, String> {
    let mut labels: HashMap<String, String> = HashMap::new();
    db.read_main(|conn| {
        let one = |sql: &str, id: &str| -> Option<String> {
            conn.query_row(sql, [id], |r| r.get::<_, Option<String>>(0))
                .ok()
                .flatten()
        };
        if let Some(v) = one(
            "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
            &spec.character_id,
        ) {
            labels.insert("charA".into(), v);
        }
        if let Some(v) = one(
            "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
            &spec.archived_character_id,
        ) {
            labels.insert("charC".into(), v);
        }
        if let Some(v) = one(
            "SELECT officialMountPointId FROM projects WHERE id = ?1",
            &spec.project_id,
        ) {
            labels.insert("project".into(), v);
        }
        if let Some(v) = one(
            "SELECT officialMountPointId FROM groups WHERE id = ?1",
            &spec.group_id,
        ) {
            labels.insert("group".into(), v);
        }
        Ok(())
    })
    .expect("resolve labels");
    labels.insert("general".into(), spec.general_mount_point_id.clone());
    for st in &spec.extra_stores {
        labels.insert(st.label.clone(), st.id.clone());
    }
    let _ = json!(null);
    labels
}
