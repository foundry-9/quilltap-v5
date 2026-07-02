//! Tier-3 differential test: the **context-compression service half**
//! (`quilltap_core::services::compression` — v4 `applyContextCompression` +
//! `compressConversationHistory`), the model-dependent compression path.
//!
//! `applyContextCompression` makes NO DB writes, so the diff is result-object
//! equivalence: the jest oracle drives v4's REAL `applyContextCompression` over
//! the committed corpus (mocking only the model/API-key seams) and emits each
//! call's full `ContextCompressionResult` JSON plus the exact
//! `provider|model|temperature|messages` canned keys the provider answered.
//! This test replays exactly those recorded entries through
//! `CannedCompletionProvider` (so a prompt/selection/temperature divergence
//! surfaces as a canned-miss), runs the Rust port for each call, and compares
//! the serialized result to the oracle's byte-for-byte.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-compression.ndjson \
//!     $N/npx jest --silent --watchman=false \
//!       --roots "$PWD" --roots "$V5/harness/oracle/cases" -- compression-tier3
//! Run:
//!   QT_ORACLE_COMPRESSION=/tmp/oracle-compression.ndjson \
//!     cargo test -p quilltap-harness --test compression_tier3_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::cheap_llm::{
    CheapLlmProfile, CheapLlmSelection, DangerousContentSettings, UncensoredFallbackOptions,
};
use quilltap_core::context_compression::CompressibleMessage;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::compression::{apply_context_compression, ContextCompressionOptions};
use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/compression-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MessageW {
    role: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionW {
    provider: String,
    model_name: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    connection_profile_id: Option<String>,
    is_local: bool,
    #[serde(default)]
    profile_parameters: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileW {
    id: String,
    provider: String,
    model_name: String,
    #[serde(default)]
    base_url: Option<String>,
    is_dangerous_compatible: bool,
    #[serde(default)]
    parameters: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DangerW {
    mode: String,
    #[serde(default)]
    uncensored_text_profile_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallW {
    name: String,
    messages: Vec<MessageW>,
    system_prompt: String,
    window_size: i64,
    compression_target_tokens: i64,
    character_name: String,
    user_name: String,
    #[allow(dead_code)]
    user_id: String,
    selection: SelectionW,
    #[serde(default)]
    danger_settings: Option<DangerW>,
    #[serde(default)]
    available_profiles: Option<Vec<ProfileW>>,
}

#[derive(Deserialize)]
struct Spec {
    calls: Vec<CallW>,
}

// ---------------------------------------------------------------------------
// Oracle rows.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CannedMessageW {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct CannedUsageW {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: i64,
}

#[derive(Deserialize)]
struct CannedRowW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMessageW>,
    /// Present (with `failure` null) for a response entry.
    #[serde(default)]
    response: Option<String>,
    /// Present (with `response` null) for a failure entry — the exact thrown
    /// error message the provider produced.
    #[serde(default)]
    failure: Option<String>,
    #[serde(default)]
    usage: Option<CannedUsageW>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/compression-tier3.json")
}

#[tokio::test]
async fn compression_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_COMPRESSION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_COMPRESSION to the oracle NDJSON (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_results: Vec<(String, Value)> = Vec::new();
    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("result") => {
                oracle_results.push((v["call"].as_str().unwrap().to_string(), v["result"].clone()))
            }
            Some("canned") => {
                oracle_canned.push(serde_json::from_value(v).expect("parse canned row"))
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Replay EXACTLY the entries the oracle recorded — an input the oracle never
    // sent is a surfaced canned-miss, not an answer.
    let mut completion = CannedCompletionProvider::new();
    for row in &oracle_canned {
        let messages: Vec<CompletionMessage> = row
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: match m.role.as_str() {
                    "system" => CompletionRole::System,
                    "user" => CompletionRole::User,
                    "assistant" => CompletionRole::Assistant,
                    other => panic!("unexpected role {other}"),
                },
                content: m.content.clone(),
            })
            .collect();
        completion = if let Some(failure) = &row.failure {
            // A thrown-error call — the executor's error-message inspection and
            // the "Failed to compress …" warning depend on this exact string.
            completion.with_failure(
                &row.provider,
                &row.model,
                row.temperature,
                &messages,
                failure,
            )
        } else {
            completion.with_response(
                &row.provider,
                &row.model,
                row.temperature,
                &messages,
                row.response.clone().unwrap_or_default(),
                row.usage.as_ref().map(|u| CompletionUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                }),
            )
        };
    }

    let executor = CheapLlmTaskExecutor::new();

    assert_eq!(
        spec.calls.len(),
        oracle_results.len(),
        "call-count mismatch — regenerate the oracle NDJSON"
    );

    for (call, (oracle_name, oracle_result)) in spec.calls.into_iter().zip(oracle_results) {
        assert_eq!(call.name, oracle_name, "call order mismatch");
        let name = call.name.clone();

        let messages: Vec<CompressibleMessage> = call
            .messages
            .iter()
            .map(|m| CompressibleMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        let selection = CheapLlmSelection {
            provider: call.selection.provider,
            model_name: call.selection.model_name,
            base_url: call.selection.base_url,
            connection_profile_id: call.selection.connection_profile_id,
            is_local: call.selection.is_local,
            profile_parameters: call.selection.profile_parameters,
        };

        // v4 builds the uncensored fallback from `dangerSettings &&
        // availableProfiles`; `isDangerousChat` is not part of the options, so
        // it is always None on this path.
        let danger = call.danger_settings.map(|d| DangerousContentSettings {
            mode: d.mode,
            uncensored_text_profile_id: d.uncensored_text_profile_id,
        });
        let profiles: Option<Vec<CheapLlmProfile>> = call.available_profiles.map(|ps| {
            ps.into_iter()
                .map(|p| CheapLlmProfile {
                    id: p.id,
                    provider: p.provider,
                    model_name: p.model_name,
                    base_url: p.base_url,
                    is_dangerous_compatible: p.is_dangerous_compatible,
                    parameters: p.parameters,
                    ..Default::default()
                })
                .collect()
        });
        let uncensored_fallback = match (&danger, &profiles) {
            (Some(d), Some(ps)) => Some(UncensoredFallbackOptions {
                danger_settings: d,
                available_profiles: ps,
                is_dangerous_chat: None,
            }),
            _ => None,
        };

        let options = ContextCompressionOptions {
            window_size: call.window_size,
            compression_target_tokens: call.compression_target_tokens,
            selection,
            character_name: call.character_name,
            user_name: call.user_name,
            uncensored_fallback,
        };

        let result = apply_context_compression(
            &completion,
            &executor,
            &messages,
            &call.system_prompt,
            &options,
        )
        .await;
        let got = serde_json::to_value(&result).expect("serialize result");
        assert_eq!(got, oracle_result, "{name}: result object diverges");
    }
}
