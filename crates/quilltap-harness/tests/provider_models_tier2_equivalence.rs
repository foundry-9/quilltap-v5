//! Tier-2 differential test #5: the `provider_models` repo (Phase-2, repo #5
//! after `folders`, `tags`, `text_replacement_rules`, and `prompt_templates`).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-provider-models-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `provider_models` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! Beyond the earlier repos this exercises two NULLABLE REAL number columns
//! (`contextWindow`, `maxOutputTokens`, bound as `Option<f64>`), two boolean
//! columns (`deprecated`, `experimental`), a nullable string column (`baseUrl`),
//! and enum TEXT columns (`provider`, `modelType`). An integer-valued REAL
//! (e.g. `128000.0`) must render as `128000` in the dump, matching v4 via
//! `js_number_to_json`. Scope is `create` / `update` / `delete` only.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-pm-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-provider-models-fixture.ts
//!   QT_FIXTURE_PM=/tmp/qt-pm-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/provider-models-tier2.ts \
//!     > /tmp/oracle-pm.ndjson
//! Run:
//!   QT_ORACLE_PROVIDER_MODELS=/tmp/oracle-pm.ndjson \
//!   QT_FIXTURE_PROVIDER_MODELS=/tmp/qt-pm-fixture.db \
//!     cargo test -p quilltap-harness --test provider_models_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::provider_models::{CreateOptions, PmCreate, PmUpdate};
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
    provider: String,
    #[serde(rename = "modelId")]
    model_id: String,
    #[serde(rename = "modelType")]
    model_type: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(rename = "contextWindow")]
    context_window: Option<f64>,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: Option<f64>,
    deprecated: bool,
    experimental: bool,
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
    #[serde(default, rename = "displayName")]
    display_name: Option<String>,
    #[serde(default, rename = "contextWindow")]
    context_window: Option<f64>,
    #[serde(default, rename = "maxOutputTokens")]
    max_output_tokens: Option<f64>,
    #[serde(default)]
    deprecated: Option<bool>,
    #[serde(default)]
    experimental: Option<bool>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/provider-models-tier2.json")
}

#[test]
fn provider_models_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PROVIDER_MODELS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PROVIDER_MODELS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_PROVIDER_MODELS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_PROVIDER_MODELS to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-pm-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.provider_models();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &PmCreate {
                            provider: data.provider.clone(),
                            model_id: data.model_id.clone(),
                            model_type: data.model_type.clone(),
                            display_name: data.display_name.clone(),
                            base_url: data.base_url.clone(),
                            context_window: data.context_window,
                            max_output_tokens: data.max_output_tokens,
                            deprecated: data.deprecated,
                            experimental: data.experimental,
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .unwrap_or_else(|e| panic!("create {} failed: {e}", options.id));
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &PmUpdate {
                                display_name: data.display_name.clone(),
                                context_window: data.context_window,
                                max_output_tokens: data.max_output_tokens,
                                deprecated: data.deprecated,
                                experimental: data.experimental,
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("provider_models.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("provider_models.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("provider_models", "id")
        .expect("dump provider_models");

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
    eprintln!("OK: provider_models tier-2 matched oracle ({n} rows).");
}
