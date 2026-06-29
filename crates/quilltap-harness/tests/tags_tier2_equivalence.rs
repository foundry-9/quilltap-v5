//! Tier-2 differential test #2: the `tags` repo (Phase-2, repo #2 after
//! `folders`).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-tags-fixture.ts), run the SAME create, update,
//! and delete op sequence from the committed spec, dump the `tags` table
//! canonically, and assert the post-op state is identical. Ids and timestamps
//! are pinned on both sides, so the dumps must match with zero normalization.
//!
//! Beyond `folders` (all strings) this exercises a boolean column (`quickHide`
//! as INTEGER 0/1), a nullable JSON-object column (`visualStyle` as compact JSON
//! in schema field order), the `nameLower` derivation, and the `delete` op.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-tags-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tags-fixture.ts
//!   QT_FIXTURE_TAGS=/tmp/qt-tags-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tags-tier2.ts \
//!     > /tmp/oracle-tags.ndjson
//! Run:
//!   QT_ORACLE_TAGS=/tmp/oracle-tags.ndjson \
//!   QT_FIXTURE_TAGS=/tmp/qt-tags-fixture.db \
//!     cargo test -p quilltap-harness --test tags_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::tags::{CreateOptions, TagCreate, TagUpdate, TagVisualStyle};
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
    name: String,
    #[serde(rename = "nameLower")]
    name_lower: Option<String>,
    #[serde(rename = "quickHide")]
    quick_hide: Option<bool>,
    #[serde(rename = "visualStyle")]
    visual_style: Option<TagVisualStyle>,
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
    name: Option<String>,
    #[serde(rename = "quickHide")]
    quick_hide: Option<bool>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/tags-tier2.json")
}

#[test]
fn tags_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TAGS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TAGS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_TAGS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_TAGS to the seed fixture .db (see test header).");
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
    let work = std::env::temp_dir().join(format!("qt-tags-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let tags = writer.tags();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => tags
                    .create(
                        &TagCreate {
                            user_id: data.user_id.clone(),
                            name: data.name.clone(),
                            name_lower: data.name_lower.clone(),
                            quick_hide: data.quick_hide,
                            visual_style: data.visual_style.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("tags.create"),
                Op::Update { id, data } => {
                    let found = tags
                        .update(
                            id,
                            &TagUpdate {
                                name: data.name.clone(),
                                quick_hide: data.quick_hide,
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("tags.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = tags.delete(id).expect("tags.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer.dump_table_json("tags", "id").expect("dump tags");

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
    eprintln!("OK: tags tier-2 matched oracle ({n} rows).");
}
