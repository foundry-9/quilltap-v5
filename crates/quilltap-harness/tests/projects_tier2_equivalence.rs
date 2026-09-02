//! Tier-2 differential test: the `projects` STORE-BACKED entity (document-store
//! overlay slice, build step 4).
//!
//! Mirrors `groups_tier2_equivalence` — same two-DB store-backed machine, same
//! shared cross-db id-map remap form, seven tables (the slim `projects` row +
//! `doc_mount_points` / `_files` / `_documents` / `_file_links` / `_folders` +
//! `project_doc_mount_links`). What it adds: the larger **16-key `properties.json`
//! bag** (five Zod-default keys ALWAYS materialized in schema order, the rest
//! `skip_serializing_if`) and the **character-roster operations**
//! (`addToRoster` / `removeFromRoster` / `setAllowAnyCharacter`, each a properties
//! read-modify-write through `update`). v4's reindex `chunkCount` /
//! `doc_mount_chunks` artifact is pinned/excluded, exactly as for groups.
//!
//! The corpus banks: a rich create (roster + color + defaultImageProfileId +
//! backgroundDisplayMode, the optional keys interleaved with the materialized
//! defaults in schema order) and a minimal create (only the five defaults);
//! addToRoster + removeFromRoster (the `characterRoster` array RMW preserving the
//! other 15 keys); setAllowAnyCharacter (a bool RMW); and a DB-only `name` update.
//!
//! Since P4.D29 (v4 `dcd9440a`) it also banks the `read_properties` refusal/seed
//! arms through THREE more stores, proving the shared generic through the SECOND
//! `StoreEntity` (the group bag is 2 keys, this one 16 — a refused write here
//! would have flattened several keys at once). Gamma's `properties.json` is
//! overwritten with malformed bytes and Delta's with a schema-invalid body
//! (`characterRoster` a string — a TYPE mismatch both Zod and serde reject; see
//! the lane record for why v4's own `not-a-uuid` test body was NOT used);
//! Epsilon's file is DELETED, the one arm that may seed from the slim row. The
//! thrown messages are recorded oracle-side into an `errors` array and diffed
//! here, ids remapped through each side's own token map and only the
//! parse-detail tail elided (see `UNPARSEABLE_MARKER`).
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_PROJECTS_MAIN=/tmp/qt-projects-main.db \
//!   QT_FIXTURE_PROJECTS_MOUNT=/tmp/qt-projects-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-projects-tier2-fixture.ts
//!   QT_FIXTURE_PROJECTS_MAIN=/tmp/qt-projects-main.db \
//!   QT_FIXTURE_PROJECTS_MOUNT=/tmp/qt-projects-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/projects-tier2.ts > /tmp/oracle-projects.ndjson
//! Run:
//!   QT_ORACLE_PROJECTS=/tmp/oracle-projects.ndjson \
//!   QT_FIXTURE_PROJECTS_MAIN=/tmp/qt-projects-main.db \
//!   QT_FIXTURE_PROJECTS_MOUNT=/tmp/qt-projects-mount.db \
//!     cargo test -p quilltap-harness --test projects_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::projects::{
    find_official_mount_point_id_raw, ProjectCreateInput, ProjectCreateOptions, ProjectsRepository,
};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        label: String,
        input: Map<String, Value>,
    },
    #[serde(rename = "update")]
    Update {
        label: String,
        patch: Map<String, Value>,
    },
    #[serde(rename = "addToRoster")]
    AddToRoster {
        label: String,
        #[serde(rename = "characterId")]
        character_id: String,
    },
    #[serde(rename = "removeFromRoster")]
    RemoveFromRoster {
        label: String,
        #[serde(rename = "characterId")]
        character_id: String,
    },
    #[serde(rename = "setAllowAnyCharacter")]
    SetAllowAnyCharacter { label: String, value: bool },
    /// Overwrite `properties.json` with raw bytes, bypassing the overlay's
    /// serializer (the corrupt-store arms). v4 drives `writeDatabaseDocument`.
    #[serde(rename = "plantProperties")]
    PlantProperties { label: String, content: String },
    /// Remove `properties.json` entirely — v4's genuine-absence (NOT_FOUND) arm,
    /// the ONLY one that may seed defaults.
    #[serde(rename = "deleteProperties")]
    DeleteProperties { label: String },
    /// Run the update expecting a refusal; the message is recorded and diffed.
    #[serde(rename = "updateExpectError")]
    UpdateExpectError {
        label: String,
        patch: Map<String, Value>,
    },
}

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    from_mount: bool,
    pin_chunk_count: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "projects",
        oracle_key: "projects",
        order_by: "name",
        id_columns: &["id", "officialMountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: false,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
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
        from_mount: true,
        pin_chunk_count: false, // P4.6BK: v5 chunks on write — chunkCount now diffs exactly
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "project_doc_mount_links",
        oracle_key: "projectLinks",
        order_by: "createdAt",
        id_columns: &["id", "projectId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/projects-tier2.json")
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: row is not an object", spec.table));
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
        if spec.pin_chunk_count {
            obj.insert("chunkCount".to_string(), Value::String("<cc>".to_string()));
        }
    }
}

/// Normalize every table and return the shared id-map, so the recorded error
/// messages can be remapped with the SAME first-seen tokens.
fn normalize_all(dumps: &mut [Value]) -> HashMap<String, String> {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
    id_map
}

/// The one seam the thrown messages cannot cross: the parse detail. v4's tail is
/// V8's `JSON.parse` text or a Zod issue array; v5's is serde's. Everything up to
/// and including `properties.json unparseable: ` IS compared byte-for-byte (that
/// is v4's `ProjectStoreUnavailableError` message, ids remapped); the tail is
/// replaced with a placeholder and separately asserted non-empty on both sides.
/// This is the standing "`is not valid JSON:` wording" seam, not a normalization
/// of convenience. Mirrors `groups_tier2_equivalence`.
const UNPARSEABLE_MARKER: &str = "properties.json unparseable: ";

/// Remap minted ids to the shared tokens, then elide the parse-detail tail.
fn normalize_error_message(message: &str, id_map: &HashMap<String, String>, side: &str) -> String {
    let mut out = message.to_string();
    for (raw, token) in id_map {
        if out.contains(raw.as_str()) {
            out = out.replace(raw.as_str(), token);
        }
    }
    if let Some(idx) = out.find(UNPARSEABLE_MARKER) {
        let head_end = idx + UNPARSEABLE_MARKER.len();
        assert!(
            !out[head_end..].trim().is_empty(),
            "{side}: empty parse detail in {out:?}"
        );
        out.truncate(head_end);
        out.push_str("<parse-detail>");
    }
    out
}

/// `[{label, message}]` → normalized, order preserved. A `null` message means the
/// update did NOT throw and is kept as-is, so a side that silently succeeds where
/// the other refuses goes RED.
fn normalize_errors(errors: &Value, id_map: &HashMap<String, String>, side: &str) -> Vec<Value> {
    errors
        .as_array()
        .unwrap_or_else(|| panic!("{side}: errors is not an array: {errors}"))
        .iter()
        .map(|e| {
            let label = e["label"].clone();
            let message = match e.get("message") {
                Some(Value::String(m)) => Value::String(normalize_error_message(m, id_map, side)),
                _ => Value::Null,
            };
            json!({ "label": label, "message": message })
        })
        .collect()
}

/// Split a flat create input into (name, description, instructions, state, properties).
/// The property bag is everything except the four top-level store/slim fields —
/// exactly what v4's create payload routes to `properties.json`.
fn split_create_input(input: &Map<String, Value>) -> ProjectCreateInput {
    let mut properties = input.clone();
    properties.remove("name");
    properties.remove("description");
    properties.remove("instructions");
    properties.remove("state");
    ProjectCreateInput {
        name: input["name"].as_str().expect("name").to_string(),
        description: input
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        instructions: input
            .get("instructions")
            .and_then(Value::as_str)
            .map(str::to_string),
        state: input
            .get("state")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
        properties: Value::Object(properties),
    }
}

#[test]
fn projects_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PROJECTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PROJECTS to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_PROJECTS_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PROJECTS_MAIN (see header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_PROJECTS_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PROJECTS_MOUNT (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-projects-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-projects-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    let mut got_errors: Vec<Value> = Vec::new();
    {
        let repo = ProjectsRepository::new(main.connection(), mount.connection());
        let mut id_by_label: HashMap<String, String> = HashMap::new();
        let lookup = |map: &HashMap<String, String>, label: &str| -> String {
            map.get(label)
                .unwrap_or_else(|| panic!("op references unknown label {label}"))
                .clone()
        };
        // The label's official store, read RAW — a broken overlay must not block
        // it (v4 reads it through `projects.findByIdRaw`).
        let mount_point_id = |map: &HashMap<String, String>, label: &str| -> String {
            find_official_mount_point_id_raw(main.connection(), &lookup(map, label))
                .unwrap_or_else(|e| panic!("raw mount lookup {label}: {e}"))
                .flatten()
                .unwrap_or_else(|| panic!("project {label} has no officialMountPointId"))
        };
        for op in &spec.ops {
            match op {
                Op::PlantProperties { label, content } => {
                    let mp = mount_point_id(&id_by_label, label);
                    DocMountFileLinksRepository::new(mount.connection())
                        .write_database_document(&mp, "properties.json", content)
                        .unwrap_or_else(|e| panic!("plant {label}: {e}"));
                }
                Op::DeleteProperties { label } => {
                    let mp = mount_point_id(&id_by_label, label);
                    DocMountFileLinksRepository::new(mount.connection())
                        .delete_database_document(&mp, "properties.json")
                        .unwrap_or_else(|e| panic!("delete properties {label}: {e}"));
                }
                Op::UpdateExpectError { label, patch } => {
                    let message = match repo.update(&lookup(&id_by_label, label), patch) {
                        Ok(_) => Value::Null,
                        Err(e) => Value::String(e.to_string()),
                    };
                    got_errors.push(json!({ "label": label, "message": message }));
                }
                Op::Create { label, input } => {
                    let created = repo
                        .create(&split_create_input(input), &ProjectCreateOptions::default())
                        .unwrap_or_else(|e| panic!("create {label}: {e}"));
                    id_by_label.insert(label.clone(), created["id"].as_str().unwrap().to_string());
                }
                Op::Update { label, patch } => {
                    repo.update(&lookup(&id_by_label, label), patch)
                        .unwrap_or_else(|e| panic!("update {label}: {e}"));
                }
                Op::AddToRoster {
                    label,
                    character_id,
                } => {
                    repo.add_to_roster(&lookup(&id_by_label, label), character_id)
                        .unwrap_or_else(|e| panic!("addToRoster {label}: {e}"));
                }
                Op::RemoveFromRoster {
                    label,
                    character_id,
                } => {
                    repo.remove_from_roster(&lookup(&id_by_label, label), character_id)
                        .unwrap_or_else(|e| panic!("removeFromRoster {label}: {e}"));
                }
                Op::SetAllowAnyCharacter { label, value } => {
                    repo.set_allow_any_character(&lookup(&id_by_label, label), *value)
                        .unwrap_or_else(|e| panic!("setAllowAnyCharacter {label}: {e}"));
                }
            }
        }
    }

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            let w = if s.from_mount { &mount } else { &main };
            w.dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect();
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            oracle
                .get(s.oracle_key)
                .cloned()
                .unwrap_or_else(|| panic!("oracle missing dump for {}", s.oracle_key))
        })
        .collect();

    let got_id_map = normalize_all(&mut got);
    let want_id_map = normalize_all(&mut want);

    // The P4.D29 refusal arms: v4's thrown message vs v5's, ids remapped through
    // each side's OWN first-seen map and only the parse-detail tail elided.
    let got_errs = normalize_errors(&Value::Array(got_errors.clone()), &got_id_map, "rust");
    let want_errs = normalize_errors(
        oracle
            .get("errors")
            .unwrap_or_else(|| panic!("oracle has no `errors` array — regenerate it (see header)")),
        &want_id_map,
        "oracle",
    );
    assert_eq!(
        got_errs, want_errs,
        "refusal-arm messages diverged\n  rust:   {got_errs:?}\n  oracle: {want_errs:?}"
    );

    for (i, s) in TABLES.iter().enumerate() {
        assert_eq!(got[i]["table"], want[i]["table"], "{}: table name", s.table);
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{}: column set / order",
            s.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            s.table, got[i]["rows"], want[i]["rows"]
        );
    }

    let rows = |key: &str| {
        let i = TABLES.iter().position(|t| t.oracle_key == key).unwrap();
        got[i]["rows"].as_array().unwrap().clone()
    };
    // [P4.D146] Zeta is the sixth — the planted-retired-mode arm.
    assert_eq!(rows("projects").len(), 6, "6 project rows");
    assert_eq!(rows("points").len(), 6, "6 mount-point rows");
    assert_eq!(rows("projectLinks").len(), 6, "6 project→store links");

    // The minimal project's properties.json = the five materialized defaults,
    // in schema order, with backgroundDisplayMode 'theme' (Beta after the
    // allowAnyCharacter RMW → true).
    let docs = rows("documents");
    let beta_props =
        "{\n  \"allowAnyCharacter\": true,\n  \"characterRoster\": [],\n  \"defaultDisabledTools\": [],\n  \"defaultDisabledToolGroups\": [],\n  \"backgroundDisplayMode\": \"theme\"\n}";
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String(beta_props.into())),
        "Beta materialized-defaults properties.json not found; documents: {docs:?}"
    );
    // Alpha's final roster after add char-2 then remove char-1 = [char-2], with
    // the optional keys (color / defaultImageProfileId / answerConfirmationOverride /
    // backgroundDisplayMode) preserved through the RMWs and interleaved with the
    // defaults in schema order.
    //
    // [P4.D146 / v4 70505745a] Alpha's create asks for `backgroundDisplayMode:
    // 'project'` and lands **'theme'**: writes route through the properties
    // parse, which coerces the retired modes, so a retired value can no longer
    // reach disk at all. This literal was `"project"` before the fix — it is the
    // write-side proof, pinned by bytes.
    let alpha_props =
        "{\n  \"allowAnyCharacter\": false,\n  \"characterRoster\": [\n    \"aaaaaaaa-0000-4000-8000-000000000002\"\n  ],\n  \"color\": \"#778899\",\n  \"defaultDisabledTools\": [],\n  \"defaultDisabledToolGroups\": [],\n  \"defaultImageProfileId\": \"11111111-1111-4111-8111-111111111111\",\n  \"answerConfirmationOverride\": \"ON\",\n  \"backgroundDisplayMode\": \"theme\"\n}";
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String(alpha_props.into())),
        "Alpha RMW-preserved properties.json not found; documents: {docs:?}"
    );

    // [P4.D146 / v4 70505745a] The READ side. Zeta's `properties.json` was
    // PLANTED on disk carrying `"backgroundDisplayMode": "static"` — the shape a
    // pre-4.9 instance is full of, and one the post-fix create can no longer
    // produce — then touched with an unrelated `icon` patch. The write overlay's
    // read-modify-write seeds from the PARSED bag, so what lands back on disk is
    // the normalized 'theme'. Without the coercion in `parse_properties` the
    // planted `"static"` would have been read back and rewritten verbatim.
    let zeta_props =
        "{\n  \"allowAnyCharacter\": false,\n  \"characterRoster\": [],\n  \"color\": \"#dd0000\",\n  \"icon\": \"anchor\",\n  \"defaultDisabledTools\": [],\n  \"defaultDisabledToolGroups\": [],\n  \"backgroundDisplayMode\": \"theme\"\n}";
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String(zeta_props.into())),
        "Zeta planted-retired-mode properties.json did not normalize; documents: {docs:?}"
    );

    // ── P4.D29: the refusal arms wrote NOTHING ────────────────────────────
    // Gamma's malformed bytes and Delta's schema-invalid body are still their
    // stores' `properties.json` after both patches were refused. Before
    // `dcd9440a` each would have been REPLACED by a defaults-seeded bag — which
    // for the 16-key project bag means SEVERAL keys silently lost per write, the
    // exact damage the drift describes.
    let has_doc = |content: &str| {
        docs.iter()
            .any(|d| d["content"] == Value::String(content.into()))
    };
    assert!(
        has_doc("{ not json"),
        "gamma's planted malformed properties.json was overwritten — the refusal did not hold"
    );
    assert!(
        has_doc(
            "{\n  \"allowAnyCharacter\": false,\n  \"characterRoster\": \"not-an-array\",\n  \"color\": \"#bb0000\"\n}"
        ),
        "delta's planted schema-invalid properties.json was overwritten"
    );
    // Neither refused patch's key reached a bag anywhere in the store.
    assert!(
        !docs.iter().any(|d| d["content"]
            .as_str()
            .is_some_and(|c| c.contains("\"answerConfirmationOverride\": \"ON\"")
                && !c.contains("\"color\": \"#778899\""))),
        "gamma's refused patch reached the store as a defaults-seeded bag"
    );
    assert!(
        !docs.iter().any(|d| d["content"]
            .as_str()
            .is_some_and(|c| c.contains("\"defaultAgentModeEnabled\": true")
                && !c.contains("\"color\": \"#aa0000\""))),
        "delta's refused patch reached the store as a defaults-seeded bag"
    );
    // Epsilon's `properties.json` was genuinely ABSENT — the one arm that may
    // seed from the slim row, so its `icon` legitimately does not survive.
    assert!(
        has_doc(
            "{\n  \"allowAnyCharacter\": false,\n  \"characterRoster\": [],\n  \"color\": \"#0e0e0e\",\n  \"defaultDisabledTools\": [],\n  \"defaultDisabledToolGroups\": [],\n  \"backgroundDisplayMode\": \"theme\"\n}"
        ),
        "the genuine-absence seed arm did not write its defaults-seeded bag; documents: {docs:?}"
    );

    eprintln!(
        "OK: projects store-backed tier-2 matched oracle (7 tables, 2 DBs, {} refusal arms).",
        got_errs.len()
    );
}
