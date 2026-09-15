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
//! to empty → filter); a with-context case (memories + project +
//! recent-conversations block folded into the augmented prompt); and — P4.D190,
//! v4 `f90144ac4` bug 141 — the two FAILURE shapes, where v4 rethrows out of
//! `generateGreetingMessage` and the comparand is the rejection's `name` +
//! message: a stalled provider (v4's REAL `LLMStreamStalledError`, thrown by the
//! mock so v4's own `instanceof` is what is measured) and an ordinary `502 Bad
//! Gateway`, the arm that proves the port does not over-classify a failure as a
//! silence.
//!
//! ⚠ The oracle imports `@/lib/llm/stream-watchdog`, which does not exist at the
//! `31436bae4` baseline — every regen of this family runs from a worktree pinned
//! at `ffb6b3119` or later.
//!
//! Build the oracle (Node 24, from the v4 checkout; jest ignores `.claude/`
//! venues, so the case stages through a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-greeting-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/initial-greeting.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/initial-greeting.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-greeting.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- initial-greeting
//! Run:
//!   QT_ORACLE_GREETING=/tmp/oracle-greeting.ndjson \
//!     cargo test -p quilltap-harness --test initial_greeting_equivalence

use std::collections::HashMap;

use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    CannedStreamingProvider, StreamChunk, StreamChunkResult, StreamError, StreamUsage,
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
    /// P4.D79: the CUMULATIVE `reasoningContent` values the canned stream emits
    /// (each is the full thinking-so-far, not a delta — which is why v4 assigns
    /// rather than concatenates).
    #[serde(rename = "cannedReasoning", default)]
    canned_reasoning: Vec<String>,
    /// P4.D190 (§C.4): pose a stalled provider. The canned sequence yields
    /// `chunks_received` content chunks and then the stalled `Err` the watchdog
    /// would have raised, so the count the error carries is the count the
    /// consumer saw.
    #[serde(default)]
    stall: Option<StallSpec>,
    /// P4.D190 (§C.4): pose an ORDINARY provider failure.
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct StallSpec {
    #[serde(rename = "budgetMs")]
    budget_ms: u64,
    #[serde(rename = "chunksReceived")]
    chunks_received: u64,
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
    /// `None` when v4 REJECTED (see `error`).
    result: Option<OracleResult>,
    /// P4.D190: v4's rejection, `{name, message}` — `name` is the byte v4's own
    /// greeting ladder reads with `instanceof`, and the port answers with
    /// [`StreamError::v4_name`].
    #[serde(default)]
    error: Option<OracleError>,
}

#[derive(Deserialize, PartialEq, Debug)]
struct OracleError {
    name: String,
    message: String,
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
    #[serde(rename = "reasoningContent")]
    reasoning_content: String,
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
        for r in &c.canned_reasoning {
            chunks.push(Ok(StreamChunk {
                reasoning_content: Some(r.clone()),
                ..Default::default()
            }));
        }
        // P4.D190 (§C.4): a failure case ENDS the sequence — the terminal item
        // is the error the watchdog (or the provider) would have raised, in the
        // shape the oracle's mock threw on v4's side.
        if let Some(st) = &c.stall {
            for i in 0..st.chunks_received {
                chunks.push(Ok(StreamChunk::content(format!("chunk-{i} "))));
            }
            chunks.push(Err(StreamError::stalled(
                st.budget_ms,
                st.chunks_received,
                Some(&c.provider),
                Some(&c.model),
            )));
        } else if let Some(e) = &c.error {
            chunks.push(Err(StreamError::new(e)));
        } else {
            if !c.canned_content.is_empty() {
                chunks.push(Ok(StreamChunk::content(&c.canned_content)));
            }
            let usage = c.canned_usage.as_ref().map(|u| StreamUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            });
            chunks.push(Ok(StreamChunk::done(usage)));
        }
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

        match generate_greeting_message(&provider, &req, None).await {
            Ok(result) => {
                assert!(
                    row.error.is_none(),
                    "case {}: v4 rejected with {:?} and the port resolved",
                    c.id,
                    row.error
                );
                let got = OracleResult {
                    content: result.content,
                    reasoning_content: result.reasoning_content,
                    content_filter_detected: result.content_filter_detected,
                };
                let want = row.result.as_ref().unwrap_or_else(|| {
                    panic!("case {}: oracle row has neither result nor error", c.id)
                });
                assert_eq!(&got, want, "case {}: rust != oracle", c.id);
            }
            Err(e) => {
                // P4.D190: v4 rethrows; the comparand is the CLASS (v4's
                // `error.name`, which its greeting ladder reads by
                // `instanceof`) plus the message bytes.
                let want = row.error.as_ref().unwrap_or_else(|| {
                    panic!(
                        "case {}: the port errored ({}) where v4 resolved",
                        c.id, e.message
                    )
                });
                let got = OracleError {
                    name: e.v4_name().to_string(),
                    message: e.message.clone(),
                };
                assert_eq!(&got, want, "case {}: rust error != oracle error", c.id);
                assert!(
                    row.result.is_none(),
                    "case {}: oracle carries both a result and an error",
                    c.id
                );
            }
        }
    }
    assert_eq!(oracle.len(), cases.len(), "oracle case count drifted");
    // P4.D79: a stale oracle predating the reasoning capture would carry no
    // `reasoningContent` key at all and every case would compare empty-to-empty.
    // At least one case must actually have captured some.
    assert!(
        oracle.values().any(|r| r
            .result
            .as_ref()
            .is_some_and(|x| !x.reasoning_content.is_empty())),
        "no case captured reasoning — regenerate the oracle"
    );
    // P4.D190: an oracle regenerated at the BASELINE cannot import
    // `stream-watchdog` at all, and one regenerated from a tree whose mock
    // still resolved the class across `jest.resetModules()` would record
    // `Error` for the stall. Both read as a stale oracle, not as a port bug.
    assert!(
        oracle
            .values()
            .any(|r| r.error.as_ref().is_some_and(|e| e.name == "LLMStreamStalledError")),
        "no case recorded a stalled rejection — regenerate the oracle from a pin at or past `ffb6b3119`"
    );
}
