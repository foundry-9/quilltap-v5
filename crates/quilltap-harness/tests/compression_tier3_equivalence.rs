//! Tier-3 differential test: the **context-compression service half**
//! (`quilltap_core::services::compression` — v4 `applyContextCompression` +
//! `compressConversationHistory`), the model-dependent compression path.
//!
//! `applyContextCompression` makes NO DB writes of its own, so the RESULT diff is
//! result-object equivalence: the jest oracle drives v4's REAL
//! `applyContextCompression` over the committed corpus (mocking only the
//! model/API-key seams) and emits each call's full `ContextCompressionResult` JSON
//! plus the exact `provider|model|temperature|messages` canned keys the provider
//! answered. This test replays exactly those recorded entries through
//! `CannedCompletionProvider` (so a prompt/selection/temperature divergence
//! surfaces as a canned-miss), runs the Rust port for each call, and compares the
//! serialized result to the oracle's byte-for-byte.
//!
//! `executeCheapLLMTask`'s `sendToProvider` ALSO fires `logLLMCall` after every
//! successful provider call (W4.7e3), so both sides run against a REAL llm-logs DB
//! and diff the written `llm_logs` rows (`CONTEXT_COMPRESSION`; id/createdAt/
//! updatedAt placeholdered, sorted by canonical JSON — see `common::dump_llm_logs`).
//!
//! Generate the oracle output (Node 24, from the v4 checkout). The oracle is now a
//! real-DB jest test, so stage a copy of the case + fixture to /tmp (jest ignores
//! `.claude/` worktree paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-compression-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/compression-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/compression-tier3.json"  "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-compression.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- compression-tier3
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
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::compression::{apply_context_compression, ContextCompressionOptions};
use quilltap_core::services::llm_logging::LogContext;
use serde::Deserialize;
use serde_json::Value;

mod common;

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
    let mut oracle_llm_logs: Option<Vec<Value>> = None;
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
            Some("llmlogs") => oracle_llm_logs = Some(common::oracle_llm_logs(&v)),
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }
    let oracle_llm_logs = oracle_llm_logs.expect("oracle emitted no llmlogs row");

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

    // All corpus calls share one userId; the executor carries one log config +
    // writes into the real llm-logs partition, exactly as the oracle's single DB
    // accumulates rows across the run.
    let user_id = spec.calls[0].user_id.clone();
    assert!(
        spec.calls.iter().all(|c| c.user_id == user_id),
        "corpus mixes userIds — the logging executor carries one"
    );
    let (db, _db_dir) = common::open_main_and_llm_logs_db(common::TEST_PEPPER);
    let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
        db: db.clone(),
        user_id,
        // Compression passes no chatId/messageId to `executeCheapLLMTask`.
        chat_id: None,
        message_id: None,
        ctx: LogContext::none(),
    });

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

    // Every successful cheap-LLM provider call wrote one `llm_logs` row (v4's
    // `sendToProvider` `logLLMCall`, task type CONTEXT_COMPRESSION). Diff the
    // written rows against v4's byte-for-byte (id/createdAt/updatedAt
    // placeholdered, sorted by canonical JSON).
    //
    // The corpus contains a failing provider arm, so v5 ALSO writes P4.13 unit
    // 6's ruled error row, which v4 never writes. That row is split off and
    // asserted in both directions (`assert_ruled_failed_call_divergence`) — it
    // must be present on the v5 side and absent on v4's — and everything else
    // is diffed byte-for-byte. Two sweeps counted this family's 7-vs-6 as an
    // unexplained content divergence (P4.D32, P4.D42).
    let got_logs = common::assert_ruled_failed_call_divergence(
        common::dump_llm_logs(&db),
        &oracle_llm_logs,
        "compression_tier3",
    );
    assert_eq!(
        got_logs,
        oracle_llm_logs,
        "llm_logs rows diverge (got {} vs oracle {})",
        got_logs.len(),
        oracle_llm_logs.len()
    );
}
