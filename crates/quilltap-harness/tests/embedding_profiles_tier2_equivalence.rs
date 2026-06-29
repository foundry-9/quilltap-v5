//! Tier-2 differential test: the `embedding_profiles` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-embedding-profiles-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `embedding_profiles` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! This banks the Taggable lineage (`userId` + a JSON `tags` array), two nullable
//! REAL number columns (`dimensions`, `truncateToDimensions` — integer-valued
//! cells dump as JSON integers), two boolean columns (`normalizeL2`,
//! `isDefault`), two nullable string columns (`apiKeyId`, `baseUrl`), and an enum
//! TEXT column (`provider`).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ep-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-embedding-profiles-fixture.ts
//!   QT_FIXTURE_EP=/tmp/qt-ep-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/embedding-profiles-tier2.ts \
//!     > /tmp/oracle-ep.ndjson
//! Run:
//!   QT_ORACLE_EMBEDDING_PROFILES=/tmp/oracle-ep.ndjson \
//!   QT_FIXTURE_EMBEDDING_PROFILES=/tmp/qt-ep-fixture.db \
//!     cargo test -p quilltap-harness --test embedding_profiles_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::embedding_profiles::{CreateOptions, EpCreate, EpUpdate};
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
    provider: String,
    #[serde(default, rename = "apiKeyId")]
    api_key_id: Option<String>,
    #[serde(default, rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(rename = "modelName")]
    model_name: String,
    #[serde(default)]
    dimensions: Option<f64>,
    #[serde(default, rename = "truncateToDimensions")]
    truncate_to_dimensions: Option<f64>,
    #[serde(rename = "normalizeL2")]
    normalize_l2: bool,
    #[serde(rename = "isDefault")]
    is_default: bool,
    tags: Vec<String>,
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
    name: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default, rename = "apiKeyId")]
    api_key_id: Option<String>,
    #[serde(default, rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(default, rename = "modelName")]
    model_name: Option<String>,
    #[serde(default)]
    dimensions: Option<f64>,
    #[serde(default, rename = "truncateToDimensions")]
    truncate_to_dimensions: Option<f64>,
    #[serde(default, rename = "normalizeL2")]
    normalize_l2: Option<bool>,
    #[serde(default, rename = "isDefault")]
    is_default: Option<bool>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/embedding-profiles-tier2.json")
}

#[test]
fn embedding_profiles_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_EMBEDDING_PROFILES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_EMBEDDING_PROFILES to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_EMBEDDING_PROFILES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_EMBEDDING_PROFILES to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-ep-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.embedding_profiles();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &EpCreate {
                            user_id: data.user_id.clone(),
                            name: data.name.clone(),
                            provider: data.provider.clone(),
                            api_key_id: data.api_key_id.clone(),
                            base_url: data.base_url.clone(),
                            model_name: data.model_name.clone(),
                            dimensions: data.dimensions,
                            truncate_to_dimensions: data.truncate_to_dimensions,
                            normalize_l2: data.normalize_l2,
                            is_default: data.is_default,
                            tags: data.tags.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("embedding_profiles.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &EpUpdate {
                                name: data.name.clone(),
                                provider: data.provider.clone(),
                                api_key_id: data.api_key_id.clone(),
                                base_url: data.base_url.clone(),
                                model_name: data.model_name.clone(),
                                dimensions: data.dimensions,
                                truncate_to_dimensions: data.truncate_to_dimensions,
                                normalize_l2: data.normalize_l2,
                                is_default: data.is_default,
                                tags: data.tags.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("embedding_profiles.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("embedding_profiles.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("embedding_profiles", "id")
        .expect("dump embedding_profiles");

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
    eprintln!("OK: embedding_profiles tier-2 matched oracle ({n} rows).");
}
