//! Tier-2 differential test: the `image_profiles` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-image-profiles-fixture.ts), run the SAME create
//! / update / delete op sequence from the committed spec, dump the
//! `image_profiles` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! This banks the Taggable lineage (`userId` + a JSON `tags` array), the first
//! OPEN / arbitrary-JSON object column (`parameters`), two boolean columns
//! (`isDefault`, `isDangerousCompatible`), and two nullable string columns
//! (`apiKeyId`, `baseUrl`). The `parameters` corpus is constrained to `{}` or
//! single-key objects so v4's insertion-order `JSON.stringify` and Rust's
//! key-sorting `serde_json::Value` serialize byte-identically (the multi-key
//! key-order seam is a tracked deferral — see image_profiles.rs).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ip-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-image-profiles-fixture.ts
//!   QT_FIXTURE_IP=/tmp/qt-ip-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/image-profiles-tier2.ts \
//!     > /tmp/oracle-ip.ndjson
//! Run:
//!   QT_ORACLE_IMAGE_PROFILES=/tmp/oracle-ip.ndjson \
//!   QT_FIXTURE_IMAGE_PROFILES=/tmp/qt-ip-fixture.db \
//!     cargo test -p quilltap-harness --test image_profiles_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::image_profiles::{CreateOptions, IpCreate, IpUpdate};
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
    parameters: Value,
    #[serde(rename = "isDefault")]
    is_default: bool,
    #[serde(rename = "isDangerousCompatible")]
    is_dangerous_compatible: bool,
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
    #[serde(default, rename = "modelName")]
    model_name: Option<String>,
    #[serde(default)]
    parameters: Option<Value>,
    #[serde(default, rename = "isDefault")]
    is_default: Option<bool>,
    #[serde(default, rename = "isDangerousCompatible")]
    is_dangerous_compatible: Option<bool>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/image-profiles-tier2.json")
}

#[test]
fn image_profiles_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_IMAGE_PROFILES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_IMAGE_PROFILES to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_IMAGE_PROFILES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_IMAGE_PROFILES to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-ip-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.image_profiles();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &IpCreate {
                            user_id: data.user_id.clone(),
                            name: data.name.clone(),
                            provider: data.provider.clone(),
                            api_key_id: data.api_key_id.clone(),
                            base_url: data.base_url.clone(),
                            model_name: data.model_name.clone(),
                            parameters: data.parameters.clone(),
                            is_default: data.is_default,
                            is_dangerous_compatible: data.is_dangerous_compatible,
                            tags: data.tags.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("image_profiles.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &IpUpdate {
                                name: data.name.clone(),
                                provider: data.provider.clone(),
                                model_name: data.model_name.clone(),
                                parameters: data.parameters.clone(),
                                is_default: data.is_default,
                                is_dangerous_compatible: data.is_dangerous_compatible,
                                tags: data.tags.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("image_profiles.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("image_profiles.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("image_profiles", "id")
        .expect("dump image_profiles");

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
    eprintln!("OK: image_profiles tier-2 matched oracle ({n} rows).");
}
