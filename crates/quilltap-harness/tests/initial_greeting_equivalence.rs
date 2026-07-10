//! Tier-3 differential (P4.4 unit 2, sub-unit 5): the initial-greeting core
//! (`services::initial_greeting::generate_greeting_message`) vs v4's REAL
//! `generateGreetingMessage`. DB-free: the oracle drives v4 with the streaming
//! provider + `logLLMCall` mocked, recording the request `messages` it passes to
//! `streamMessage`; this side registers a canned stream keyed by those RECORDED
//! messages and runs the port, comparing `{content, contentFilterDetected}`
//! exactly. A prompt-byte divergence misses the canned key (→ an error / empty
//! content) and surfaces.
//!
//! Cases: a plain success; content-filter (empty content + completion tokens);
//! empty content with no usage (not a filter); a whitespace-only response (trims
//! to empty → filter); and a with-context case (memories + project +
//! recent-conversations block folded into the augmented prompt).
//!
//! Build the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   QT_ORACLE_OUT=/tmp/oracle-greeting.ndjson npx jest ... initial-greeting
//! Run:
//!   QT_ORACLE_GREETING=/tmp/oracle-greeting.ndjson \
//!     cargo test -p quilltap-harness --test initial_greeting_equivalence

use std::collections::HashMap;

use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    CannedStreamingProvider, StreamChunk, StreamChunkResult, StreamUsage,
};
use quilltap_core::services::initial_greeting::{
    generate_greeting_message, GreetingRequest, ParticipantMemory, ProjectContext,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct CaseSpec {
    id: String,
    #[serde(rename = "systemPrompt")]
    system_prompt: String,
    #[serde(rename = "characterName")]
    character_name: String,
    provider: String,
    model: String,
    temperature: Option<f64>,
    memories: Vec<MemSpec>,
    project: Option<ProjSpec>,
    #[serde(rename = "recentConversationsBlock")]
    recent_conversations_block: Option<String>,
    #[serde(rename = "cannedContent")]
    canned_content: String,
    #[serde(rename = "cannedUsage")]
    canned_usage: Option<UsageSpec>,
}

#[derive(Deserialize)]
struct MemSpec {
    #[serde(rename = "aboutCharacterName")]
    about_character_name: String,
    summary: String,
}
#[derive(Deserialize)]
struct ProjSpec {
    name: String,
    description: Option<String>,
    instructions: Option<String>,
}
#[derive(Deserialize)]
struct UsageSpec {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: i64,
}

#[derive(Deserialize)]
struct OracleRow {
    id: String,
    request: OracleRequest,
    result: OracleResult,
}
#[derive(Deserialize)]
struct OracleRequest {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<OracleMessage>,
}
#[derive(Deserialize)]
struct OracleMessage {
    role: String,
    content: String,
}
#[derive(Deserialize, PartialEq, Debug)]
struct OracleResult {
    content: String,
    #[serde(rename = "contentFilterDetected")]
    content_filter_detected: bool,
}

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/initial-greeting.json")
}

#[tokio::test(flavor = "current_thread")]
async fn initial_greeting_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_GREETING") else {
        eprintln!("SKIP: set QT_ORACLE_GREETING (see test header).");
        return;
    };

    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let cases: Vec<CaseSpec> = serde_json::from_value(spec["cases"].clone()).expect("parse cases");

    let mut oracle: HashMap<String, OracleRow> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: OracleRow = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(row.id.clone(), row);
    }

    for c in &cases {
        let row = oracle
            .get(&c.id)
            .unwrap_or_else(|| panic!("oracle missing {}", c.id));

        // Register the canned stream keyed by the RECORDED (v4) messages.
        let recorded_msgs: Vec<CompletionMessage> = row
            .request
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: match m.role.as_str() {
                    "system" => CompletionRole::System,
                    "assistant" => CompletionRole::Assistant,
                    _ => CompletionRole::User,
                },
                content: m.content.clone(),
            })
            .collect();
        let mut chunks: Vec<StreamChunkResult> = Vec::new();
        if !c.canned_content.is_empty() {
            chunks.push(Ok(StreamChunk::content(&c.canned_content)));
        }
        let usage = c.canned_usage.as_ref().map(|u| StreamUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });
        chunks.push(Ok(StreamChunk::done(usage)));
        let provider = CannedStreamingProvider::new().with_stream(
            &row.request.provider,
            &row.request.model,
            row.request.temperature,
            &recorded_msgs,
            chunks,
        );

        let req = GreetingRequest {
            system_prompt: c.system_prompt.clone(),
            character_name: c.character_name.clone(),
            provider: c.provider.clone(),
            model_name: c.model.clone(),
            api_key: "test-key".to_string(),
            base_url: None,
            temperature: c.temperature,
            max_tokens: None,
            top_p: None,
            profile_parameters: None,
            participant_memories: c
                .memories
                .iter()
                .map(|m| ParticipantMemory {
                    about_character_name: m.about_character_name.clone(),
                    summary: m.summary.clone(),
                })
                .collect(),
            project_context: c.project.as_ref().map(|p| ProjectContext {
                name: p.name.clone(),
                description: p.description.clone(),
                instructions: p.instructions.clone(),
            }),
            recent_conversations_block: c.recent_conversations_block.clone(),
            character_id: Some("char-1".to_string()),
        };

        let result = generate_greeting_message(&provider, &req, None)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "case {}: greeting errored (canned miss?): {}",
                    c.id, e.message
                )
            });

        let got = OracleResult {
            content: result.content,
            content_filter_detected: result.content_filter_detected,
        };
        assert_eq!(got, row.result, "case {}: rust != oracle", c.id);
    }
    assert_eq!(oracle.len(), cases.len(), "oracle case count drifted");
}
