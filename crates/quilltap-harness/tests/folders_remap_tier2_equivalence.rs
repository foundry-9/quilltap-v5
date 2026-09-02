//! Tier-2 differential test: the `folders` repo MINTED-VALUES (remap) path —
//! the generated-UUID remap + timestamp-placeholder normalization machinery.
//!
//! `folders` / `tags` tier-2 so far pinned every id and timestamp (the
//! zero-normalization form). This case pins NOTHING: both v4 and the Rust port
//! independently mint random UUIDs and wall-clock timestamps, so the raw dumps
//! cannot match. They are reconciled by normalizing only the legitimately
//! nondeterministic fields, then structural-diffing the rest:
//!
//!   - **id remap.** Rows are dumped in natural-key (`path`) order — identical
//!     on both sides because paths are inputs, not generated. Walking that order,
//!     each id-column value (`id`, `parentFolderId`) gets a first-seen canonical
//!     token (`ID_0`, `ID_1`, …). A generated id referencing another generated
//!     id (the child's `parentFolderId` → the parent's `id`) thus verifies the
//!     FK RELATIONSHIP without pinning the literal id.
//!   - **timestamps.** `createdAt` / `updatedAt` → a `<ts>` placeholder. The
//!     create invariant `createdAt == updatedAt` is asserted per row BEFORE
//!     placeholdering, so the lever isn't silently dropped.
//!
//! The SAME normalization runs over both the oracle dump and the Rust dump (one
//! implementation, here), so the remap is provably consistent — the oracle stays
//! a raw emitter.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-folders-remap-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-folders-remap-fixture.ts
//!   QT_FIXTURE_FOLDERS_REMAP=/tmp/qt-folders-remap-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/folders-remap-tier2.ts \
//!     > /tmp/oracle-folders-remap.ndjson
//! Run:
//!   QT_ORACLE_FOLDERS_REMAP=/tmp/oracle-folders-remap.ndjson \
//!   QT_FIXTURE_FOLDERS_REMAP=/tmp/qt-folders-remap-fixture.db \
//!     cargo test -p quilltap-harness --test folders_remap_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::folders::{CreateOptions, FolderCreate};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Live `folders` row count, for the per-op "did the table grow" comparand.
fn row_count(writer: &Writer) -> usize {
    writer
        .connection()
        .query_row("SELECT COUNT(*) FROM folders", [], |r| r.get::<_, i64>(0))
        .expect("count folders") as usize
}

/// The committed fixture spec — the single source driving both ports.
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
    Create { data: CreateData },
    /// v4 `a5df98b3f` (bug 114) — the find-or-create chokepoint.
    #[serde(rename = "ensureByPath")]
    EnsureByPath { data: CreateData },
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "userId")]
    user_id: String,
    path: String,
    name: String,
    #[serde(rename = "parentFolderId")]
    parent_folder_id: Option<String>,
    /// "set parentFolderId to the id minted for op[N]".
    #[serde(rename = "parentFromOp")]
    parent_from_op: Option<usize>,
    /// ABSENT and explicit `null` are the same thing here (v4's
    /// `data.projectId ?? null` collapses `undefined` onto null before the read
    /// and the write), which is exactly what the `/etchings/` op pins.
    #[serde(rename = "projectId")]
    project_id: Option<String>,
}

/// Columns that hold a generated id (the PK + any FK to a generated id).
const ID_COLUMNS: &[&str] = &["id", "parentFolderId"];
/// Columns that hold a wall-clock timestamp minted at create time.
const TS_COLUMNS: &[&str] = &["createdAt", "updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/folders-remap-tier2.json")
}

/// Normalize a `{ table, columns, rows }` dump in place: first-seen id remap
/// over the rows in their given order, then timestamp placeholdering. Asserts
/// the `createdAt == updatedAt` create-invariant per row before collapsing it.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));

    let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));

        // createdAt == updatedAt invariant (both are the same `now` on create).
        let created = obj.get("createdAt").cloned();
        let updated = obj.get("updatedAt").cloned();
        assert_eq!(
            created, updated,
            "{label}: createdAt != updatedAt in row {obj:?}"
        );

        for col in ID_COLUMNS {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

#[test]
fn folders_remap_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_FOLDERS_REMAP") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_FOLDERS_REMAP to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_FOLDERS_REMAP") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_FOLDERS_REMAP to the seed fixture .db (see header).");
            return;
        }
    };

    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Fresh copy so the shared seed fixture stays pristine.
    let work =
        std::env::temp_dir().join(format!("qt-folders-remap-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port, minting our own ids/ts.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    // The fixture must carry v4's unique index — without it the constraint arm
    // below silently inserts a duplicate and measures nothing (the two-vintage
    // seam: `ensureCollection`/generateDDL cannot express a COALESCE index, so
    // the builder creates it explicitly).
    assert!(
        quilltap_core::db::folders_unique_path_repair::index_exists(writer.connection())
            .expect("index probe"),
        "the remap fixture is stale: rebuild it with build-folders-remap-fixture.ts \
         (it must create idx_folders_userId_projectId_path)"
    );
    let mut op_results: Vec<Value> = Vec::new();
    {
        let folders = writer.folders();
        let mut minted: Vec<String> = Vec::new();
        for op in &spec.ops {
            let (data, is_ensure) = match op {
                Op::Create { data } => (data, false),
                Op::EnsureByPath { data } => (data, true),
            };
            let parent = match data.parent_from_op {
                Some(n) => Some(
                    minted
                        .get(n)
                        .cloned()
                        .unwrap_or_else(|| panic!("parentFromOp {n} out of range")),
                ),
                None => data.parent_folder_id.clone(),
            };
            let create = FolderCreate {
                user_id: data.user_id.clone(),
                path: data.path.clone(),
                name: data.name.clone(),
                parent_folder_id: parent,
                project_id: data.project_id.clone(),
            };
            if is_ensure {
                let before = row_count(&writer);
                match folders.ensure_by_path(&create) {
                    Ok(folder) => {
                        minted.push(folder.id.clone());
                        op_results.push(json!({
                            "kind": "ensureByPath",
                            "threw": false,
                            "returnedPath": folder.path,
                            "returnedProjectId": folder.project_id,
                            "createdNew": row_count(&writer) > before,
                        }));
                    }
                    Err(e) => {
                        minted.push(String::new());
                        op_results.push(json!({
                            "kind": "ensureByPath",
                            "threw": true,
                            "uniqueConstraint":
                                quilltap_core::db::sqlite_errors::is_unique_constraint_error(&e),
                            "rowsAdded": row_count(&writer) as i64 - before as i64,
                        }));
                    }
                }
                continue;
            }
            let id = folders
                // Unpinned: the repo mints id + timestamps.
                .create(&create, &CreateOptions::default())
                .expect("folders.create");
            minted.push(id);
            op_results.push(json!({ "kind": "create", "threw": false }));
        }
    }

    let mut got = writer
        .dump_table_json("folders", "path")
        .expect("dump folders");

    let _ = std::fs::remove_file(&work);

    // One normalization, applied to both dumps.
    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "remapped row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    // The per-op outcomes: what the chokepoint DID, which the table dump alone
    // cannot show (an insert the index collapses looks the same as a read).
    assert_eq!(
        Value::Array(op_results.clone()),
        oracle["ops"],
        "per-op ensureByPath outcomes diverged"
    );
    assert!(
        op_results.iter().any(|r| r["threw"] == Value::Bool(true)),
        "the constraint arm never fired — the fixture's index or the ''-projectId \
         seed row is missing"
    );
    assert!(
        op_results
            .iter()
            .any(|r| r["createdNew"] == Value::Bool(false)),
        "no ensureByPath op resolved to an EXISTING row"
    );
    assert!(
        op_results
            .iter()
            .any(|r| r["createdNew"] == Value::Bool(true)),
        "no ensureByPath op created a row"
    );

    // Sanity: the remap actually fired (the child points at the parent).
    let rows = got["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        5,
        "expected the ''-project seed + /a/ + /a/b/ + /plates/ + /etchings/"
    );
    let child = rows
        .iter()
        .find(|r| r["path"] == Value::String("/a/b/".into()))
        .expect("child row");
    assert_eq!(
        child["parentFolderId"],
        Value::String("ID_0".into()),
        "child should reference the parent's remapped id"
    );
    // Guard against a no-op normalization: ids must be tokens, not raw UUIDs.
    let m: &Map<String, Value> = rows[0].as_object().unwrap();
    assert!(
        m["id"].as_str().unwrap().starts_with("ID_"),
        "id was not remapped"
    );

    eprintln!(
        "OK: folders remap tier-2 matched oracle ({} rows).",
        rows.len()
    );
}
