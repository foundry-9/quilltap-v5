//! Tier-2 differential test: the `files` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-files-fixture.ts), run the SAME create / update
//! / delete op sequence from the committed spec, dump the `files` table
//! canonically, and assert the post-op state is identical. Ids and timestamps
//! are pinned on both sides, so the dumps must match with zero normalization.
//!
//! files is the WIDEST repo to date (~23 columns) and carries the Taggable
//! lineage (`userId` + a JSON `tags` array). It banks a SECOND JSON array column
//! (`linkedTo`), a REAL number column (`size`) plus two nullable REAL columns
//! (`width`/`height`), an OPTIONAL-NO-DEFAULT boolean (`isPlainText`: 0/1 when
//! present, NULL when absent — one create OMITS it to bank the NULL case), enum
//! TEXT columns (`source`/`category`/`fileStatus`), a 64-char `sha256`, and a
//! batch of nullable string columns.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-fl-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-files-fixture.ts
//!   QT_FIXTURE_FL=/tmp/qt-fl-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/files-tier2.ts \
//!     > /tmp/oracle-fl.ndjson
//! Run:
//!   QT_ORACLE_FILES=/tmp/oracle-fl.ndjson \
//!   QT_FIXTURE_FILES=/tmp/qt-fl-fixture.db \
//!     cargo test -p quilltap-harness --test files_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::files::{CreateOptions, FileCreate, FileUpdate};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

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
    Create {
        data: CreateData,
        options: CreateOpts,
    },
    #[serde(rename = "update")]
    Update { id: String, data: UpdateData },
    #[serde(rename = "delete")]
    Delete { id: String },
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "userId")]
    user_id: String,
    sha256: String,
    #[serde(rename = "originalFilename")]
    original_filename: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    size: f64,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    /// Absent in the spec => `None` => bound as SQL NULL (the no-default boolean).
    #[serde(default, rename = "isPlainText")]
    is_plain_text: Option<bool>,
    #[serde(rename = "linkedTo")]
    linked_to: Vec<String>,
    source: String,
    category: String,
    #[serde(default, rename = "generationPrompt")]
    generation_prompt: Option<String>,
    #[serde(default, rename = "generationModel")]
    generation_model: Option<String>,
    #[serde(default, rename = "generationRevisedPrompt")]
    generation_revised_prompt: Option<String>,
    #[serde(default)]
    description: Option<String>,
    tags: Vec<String>,
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
    #[serde(default, rename = "folderPath")]
    folder_path: Option<String>,
    #[serde(default, rename = "storageKey")]
    storage_key: Option<String>,
    #[serde(rename = "fileStatus")]
    file_status: String,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
struct UpdateData {
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default, rename = "originalFilename")]
    original_filename: Option<String>,
    #[serde(default, rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(default)]
    size: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default, rename = "isPlainText")]
    is_plain_text: Option<bool>,
    #[serde(default, rename = "linkedTo")]
    linked_to: Option<Vec<String>>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default, rename = "generationPrompt")]
    generation_prompt: Option<String>,
    #[serde(default, rename = "generationModel")]
    generation_model: Option<String>,
    #[serde(default, rename = "generationRevisedPrompt")]
    generation_revised_prompt: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
    #[serde(default, rename = "folderPath")]
    folder_path: Option<String>,
    #[serde(default, rename = "storageKey")]
    storage_key: Option<String>,
    #[serde(default, rename = "fileStatus")]
    file_status: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/files-tier2.json")
}

#[test]
fn files_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_FILES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_FILES to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_FILES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_FILES to the seed fixture .db (see test header).");
            return;
        }
    };

    // Parse the committed spec (pepper + op sequence) — same file the oracle used.
    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    // Parse the oracle's expected post-op dump (one NDJSON object).
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work = std::env::temp_dir().join(format!("qt-fl-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.files();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &FileCreate {
                            user_id: data.user_id.clone(),
                            sha256: data.sha256.clone(),
                            original_filename: data.original_filename.clone(),
                            mime_type: data.mime_type.clone(),
                            size: data.size,
                            width: data.width,
                            height: data.height,
                            is_plain_text: data.is_plain_text,
                            linked_to: data.linked_to.clone(),
                            source: data.source.clone(),
                            category: data.category.clone(),
                            generation_prompt: data.generation_prompt.clone(),
                            generation_model: data.generation_model.clone(),
                            generation_revised_prompt: data.generation_revised_prompt.clone(),
                            description: data.description.clone(),
                            tags: data.tags.clone(),
                            project_id: data.project_id.clone(),
                            folder_path: data.folder_path.clone(),
                            storage_key: data.storage_key.clone(),
                            file_status: data.file_status.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("files.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &FileUpdate {
                                sha256: data.sha256.clone(),
                                original_filename: data.original_filename.clone(),
                                mime_type: data.mime_type.clone(),
                                size: data.size,
                                width: data.width,
                                height: data.height,
                                is_plain_text: data.is_plain_text,
                                linked_to: data.linked_to.clone(),
                                source: data.source.clone(),
                                category: data.category.clone(),
                                generation_prompt: data.generation_prompt.clone(),
                                generation_model: data.generation_model.clone(),
                                generation_revised_prompt: data.generation_revised_prompt.clone(),
                                description: data.description.clone(),
                                tags: data.tags.clone(),
                                project_id: data.project_id.clone(),
                                folder_path: data.folder_path.clone(),
                                storage_key: data.storage_key.clone(),
                                file_status: data.file_status.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("files.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("files.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer.dump_table_json("files", "id").expect("dump files");

    let _ = std::fs::remove_file(&work);

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label). assert_eq on serde_json::Value is order-independent for
    // object keys and exact for the row arrays (both sides sorted by id).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: files tier-2 matched oracle ({n} rows).");
}
