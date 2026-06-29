//! Tier-2 differential test: the `connection_profiles` repo (Phase-2, the
//! workhorse profile repo).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-connection-profiles-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `connection_profiles` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! This is the widest marshaling surface the tier-2 ports have hit: three enum
//! TEXT columns (provider, transport, pseudoToolMode), many boolean columns, two
//! nullable REAL int-override columns (maxContext, maxTokens), five REAL token
//! counters, three nullable string columns, a JSON array column (tags), and the
//! open-JSON object column (parameters, corpus-constrained to {}/single-key).
//! There is no conflict or built-in guard in scope — every op succeeds.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-cp-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-connection-profiles-fixture.ts
//!   QT_FIXTURE_CP=/tmp/qt-cp-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/connection-profiles-tier2.ts \
//!     > /tmp/oracle-cp.ndjson
//! Run:
//!   QT_ORACLE_CONNECTION_PROFILES=/tmp/oracle-cp.ndjson \
//!   QT_FIXTURE_CONNECTION_PROFILES=/tmp/qt-cp-fixture.db \
//!     cargo test -p quilltap-harness --test connection_profiles_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::connection_profiles::{CpCreate, CpUpdate, CreateOptions};
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
    transport: String,
    #[serde(rename = "courierDeltaMode")]
    courier_delta_mode: bool,
    #[serde(rename = "apiKeyId")]
    api_key_id: Option<String>,
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(rename = "modelName")]
    model_name: String,
    parameters: Value,
    #[serde(rename = "isDefault")]
    is_default: bool,
    #[serde(rename = "isCheap")]
    is_cheap: bool,
    #[serde(rename = "allowWebSearch")]
    allow_web_search: bool,
    #[serde(rename = "useNativeWebSearch")]
    use_native_web_search: bool,
    #[serde(rename = "allowToolUse")]
    allow_tool_use: bool,
    #[serde(rename = "pseudoToolMode")]
    pseudo_tool_mode: String,
    #[serde(rename = "modelClass")]
    model_class: Option<String>,
    #[serde(rename = "maxContext")]
    max_context: Option<f64>,
    #[serde(rename = "maxTokens")]
    max_tokens: Option<f64>,
    #[serde(rename = "isDangerousCompatible")]
    is_dangerous_compatible: bool,
    #[serde(rename = "supportsImageUpload")]
    supports_image_upload: bool,
    tags: Vec<String>,
    #[serde(rename = "sortIndex")]
    sort_index: f64,
    #[serde(rename = "totalTokens")]
    total_tokens: f64,
    #[serde(rename = "totalPromptTokens")]
    total_prompt_tokens: f64,
    #[serde(rename = "totalCompletionTokens")]
    total_completion_tokens: f64,
    #[serde(rename = "messageCount")]
    message_count: f64,
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
    #[serde(default)]
    transport: Option<String>,
    #[serde(default, rename = "courierDeltaMode")]
    courier_delta_mode: Option<bool>,
    #[serde(default, rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(default, rename = "modelName")]
    model_name: Option<String>,
    #[serde(default, rename = "isDefault")]
    is_default: Option<bool>,
    #[serde(default, rename = "isCheap")]
    is_cheap: Option<bool>,
    #[serde(default, rename = "allowWebSearch")]
    allow_web_search: Option<bool>,
    #[serde(default, rename = "useNativeWebSearch")]
    use_native_web_search: Option<bool>,
    #[serde(default, rename = "allowToolUse")]
    allow_tool_use: Option<bool>,
    #[serde(default, rename = "pseudoToolMode")]
    pseudo_tool_mode: Option<String>,
    #[serde(default, rename = "modelClass")]
    model_class: Option<String>,
    #[serde(default, rename = "maxContext")]
    max_context: Option<f64>,
    #[serde(default, rename = "maxTokens")]
    max_tokens: Option<f64>,
    #[serde(default, rename = "isDangerousCompatible")]
    is_dangerous_compatible: Option<bool>,
    #[serde(default, rename = "supportsImageUpload")]
    supports_image_upload: Option<bool>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default, rename = "sortIndex")]
    sort_index: Option<f64>,
    #[serde(default, rename = "totalTokens")]
    total_tokens: Option<f64>,
    #[serde(default, rename = "totalPromptTokens")]
    total_prompt_tokens: Option<f64>,
    #[serde(default, rename = "totalCompletionTokens")]
    total_completion_tokens: Option<f64>,
    #[serde(default, rename = "messageCount")]
    message_count: Option<f64>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/connection-profiles-tier2.json")
}

#[test]
fn connection_profiles_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CONNECTION_PROFILES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CONNECTION_PROFILES to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CONNECTION_PROFILES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CONNECTION_PROFILES to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-cp-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.connection_profiles();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &CpCreate {
                            user_id: data.user_id.clone(),
                            name: data.name.clone(),
                            provider: data.provider.clone(),
                            transport: data.transport.clone(),
                            courier_delta_mode: data.courier_delta_mode,
                            api_key_id: data.api_key_id.clone(),
                            base_url: data.base_url.clone(),
                            model_name: data.model_name.clone(),
                            parameters: data.parameters.clone(),
                            is_default: data.is_default,
                            is_cheap: data.is_cheap,
                            allow_web_search: data.allow_web_search,
                            use_native_web_search: data.use_native_web_search,
                            allow_tool_use: data.allow_tool_use,
                            pseudo_tool_mode: data.pseudo_tool_mode.clone(),
                            model_class: data.model_class.clone(),
                            max_context: data.max_context,
                            max_tokens: data.max_tokens,
                            is_dangerous_compatible: data.is_dangerous_compatible,
                            supports_image_upload: data.supports_image_upload,
                            tags: data.tags.clone(),
                            sort_index: data.sort_index,
                            total_tokens: data.total_tokens,
                            total_prompt_tokens: data.total_prompt_tokens,
                            total_completion_tokens: data.total_completion_tokens,
                            message_count: data.message_count,
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .unwrap_or_else(|e| panic!("create {}: {e}", options.id));
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &CpUpdate {
                                name: data.name.clone(),
                                provider: data.provider.clone(),
                                transport: data.transport.clone(),
                                courier_delta_mode: data.courier_delta_mode,
                                base_url: data.base_url.clone(),
                                model_name: data.model_name.clone(),
                                is_default: data.is_default,
                                is_cheap: data.is_cheap,
                                allow_web_search: data.allow_web_search,
                                use_native_web_search: data.use_native_web_search,
                                allow_tool_use: data.allow_tool_use,
                                pseudo_tool_mode: data.pseudo_tool_mode.clone(),
                                model_class: data.model_class.clone(),
                                max_context: data.max_context,
                                max_tokens: data.max_tokens,
                                is_dangerous_compatible: data.is_dangerous_compatible,
                                supports_image_upload: data.supports_image_upload,
                                tags: data.tags.clone(),
                                sort_index: data.sort_index,
                                total_tokens: data.total_tokens,
                                total_prompt_tokens: data.total_prompt_tokens,
                                total_completion_tokens: data.total_completion_tokens,
                                message_count: data.message_count,
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("cp.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("cp.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("connection_profiles", "id")
        .expect("dump connection_profiles");

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
    eprintln!("OK: connection_profiles tier-2 matched oracle ({n} rows).");
}
