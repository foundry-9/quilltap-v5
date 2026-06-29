//! Tier-2 differential test: the `character_plugin_data` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-character-plugin-data-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `character_plugin_data` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! This banks the open / arbitrary-JSON VALUE column `data` (`z.unknown()`,
//! stored as compact JSON text); the rest of the row is plain strings
//! (`characterId`, `pluginName`) plus the timestamps. The `data` corpus is
//! constrained to `{}` or single-key objects so v4's insertion-order
//! `JSON.stringify` and Rust's key-sorting `serde_json::Value` serialize
//! byte-identically (the multi-key key-order seam is a tracked deferral — see
//! character_plugin_data.rs).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-cpd-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-character-plugin-data-fixture.ts
//!   QT_FIXTURE_CPD=/tmp/qt-cpd-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/character-plugin-data-tier2.ts \
//!     > /tmp/oracle-cpd.ndjson
//! Run:
//!   QT_ORACLE_CHARACTER_PLUGIN_DATA=/tmp/oracle-cpd.ndjson \
//!   QT_FIXTURE_CHARACTER_PLUGIN_DATA=/tmp/qt-cpd-fixture.db \
//!     cargo test -p quilltap-harness --test character_plugin_data_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::character_plugin_data::{CpdCreate, CpdUpdate, CreateOptions};
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
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "pluginName")]
    plugin_name: String,
    data: Value,
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
    #[serde(default, rename = "characterId")]
    character_id: Option<String>,
    #[serde(default, rename = "pluginName")]
    plugin_name: Option<String>,
    #[serde(default)]
    data: Option<Value>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/character-plugin-data-tier2.json")
}

#[test]
fn character_plugin_data_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARACTER_PLUGIN_DATA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CHARACTER_PLUGIN_DATA to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHARACTER_PLUGIN_DATA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CHARACTER_PLUGIN_DATA to the seed fixture .db (see test header)."
            );
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
    let work = std::env::temp_dir().join(format!("qt-cpd-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.character_plugin_data();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &CpdCreate {
                            character_id: data.character_id.clone(),
                            plugin_name: data.plugin_name.clone(),
                            data: data.data.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("character_plugin_data.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &CpdUpdate {
                                character_id: data.character_id.clone(),
                                plugin_name: data.plugin_name.clone(),
                                data: data.data.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("character_plugin_data.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("character_plugin_data.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("character_plugin_data", "id")
        .expect("dump character_plugin_data");

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
    eprintln!("OK: character_plugin_data tier-2 matched oracle ({n} rows).");
}
