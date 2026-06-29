//! Tier-2 differential test: the `wardrobe` repo (Phase-2) over the
//! `wardrobe_items` table.
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-wardrobe-fixture.ts), run the SAME create /
//! update / delete op sequence from the committed spec, dump the `wardrobe_items`
//! table canonically, and assert the post-op state is identical. Ids and
//! timestamps are pinned on both sides, so the dumps must match with zero
//! normalization.
//!
//! WHY THE BASE SQL CRUD: v4's `WardrobeRepository` overrides create/update/delete
//! to be vault-only (no SQL write mirror — they throw without a document-store
//! mount), so the oracle drives v4's REAL base-repository `_create`/`_update`/
//! `_delete` against `wardrobe_items` via a thin test subclass. The Rust port
//! mirrors that base-CRUD marshaling for the table.
//!
//! This banks two JSON ARRAY columns (`types` — enum strings; `componentItemIds`),
//! two boolean columns (`isDefault`/`replace` -> 0/1), a nullable soft-delete
//! TIMESTAMP column (`archivedAt` — 'date' affinity -> TEXT, exercised null and
//! set-to-non-null on update via the `archive` shape), and several nullable
//! string/UUID columns. No conflict/guard, so no expectThrow/Noop path.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-wardrobe-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-wardrobe-fixture.ts
//!   QT_FIXTURE_WARDROBE=/tmp/qt-wardrobe-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/wardrobe-tier2.ts \
//!     > /tmp/oracle-wardrobe.ndjson
//! Run:
//!   QT_ORACLE_WARDROBE=/tmp/oracle-wardrobe.ndjson \
//!   QT_FIXTURE_WARDROBE=/tmp/qt-wardrobe-fixture.db \
//!     cargo test -p quilltap-harness --test wardrobe_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::wardrobe::{CreateOptions, WardrobeCreate, WardrobeUpdate};
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
    #[serde(default, rename = "characterId")]
    character_id: Option<String>,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "imagePrompt")]
    image_prompt: Option<String>,
    types: Vec<String>,
    #[serde(rename = "componentItemIds")]
    component_item_ids: Vec<String>,
    #[serde(default)]
    appropriateness: Option<String>,
    #[serde(rename = "isDefault")]
    is_default: bool,
    replace: bool,
    #[serde(default, rename = "migratedFromClothingRecordId")]
    migrated_from_clothing_record_id: Option<String>,
    #[serde(default, rename = "archivedAt")]
    archived_at: Option<String>,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

/// The update patch. `archivedAt` uses `Option<Option<String>>` so the JSON
/// distinguishes "absent" (leave untouched) from "present, non-null" (set). The
/// corpus only ever sets it to a non-null timestamp (the `archive` shape); the
/// unarchive `Some(None)` -> SQL NULL form is deferred (not in the corpus).
#[derive(Deserialize)]
struct UpdateData {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "imagePrompt")]
    image_prompt: Option<String>,
    #[serde(default)]
    types: Option<Vec<String>>,
    #[serde(default, rename = "componentItemIds")]
    component_item_ids: Option<Vec<String>>,
    #[serde(default)]
    appropriateness: Option<String>,
    #[serde(default, rename = "isDefault")]
    is_default: Option<bool>,
    #[serde(default)]
    replace: Option<bool>,
    /// Absent in the JSON -> `None` (column untouched); present with a value ->
    /// `Some(Some(value))` (set the column).
    #[serde(default, rename = "archivedAt")]
    archived_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/wardrobe-tier2.json")
}

#[test]
fn wardrobe_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_WARDROBE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_WARDROBE to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_WARDROBE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WARDROBE to the seed fixture .db (see test header).");
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
    let work = std::env::temp_dir().join(format!("qt-wardrobe-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.wardrobe();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &WardrobeCreate {
                            character_id: data.character_id.clone(),
                            title: data.title.clone(),
                            description: data.description.clone(),
                            image_prompt: data.image_prompt.clone(),
                            types: data.types.clone(),
                            component_item_ids: data.component_item_ids.clone(),
                            appropriateness: data.appropriateness.clone(),
                            is_default: data.is_default,
                            replace: data.replace,
                            migrated_from_clothing_record_id: data
                                .migrated_from_clothing_record_id
                                .clone(),
                            archived_at: data.archived_at.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("wardrobe.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &WardrobeUpdate {
                                title: data.title.clone(),
                                description: data.description.clone(),
                                image_prompt: data.image_prompt.clone(),
                                types: data.types.clone(),
                                component_item_ids: data.component_item_ids.clone(),
                                appropriateness: data.appropriateness.clone(),
                                is_default: data.is_default,
                                replace: data.replace,
                                // Present non-null timestamp -> set the column.
                                archived_at: data.archived_at.clone().map(Some),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("wardrobe.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("wardrobe.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("wardrobe_items", "id")
        .expect("dump wardrobe_items");

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
    eprintln!("OK: wardrobe tier-2 matched oracle ({n} rows).");
}
