//! Tier-3 differential test: the **chat-message orchestrator**
//! (`quilltap_core::services::orchestrator` — v4 `processMessage` +
//! `executeTurnChain`), the send-path spine and the FINAL unit of Phase-3 wave 3.
//! The first end-to-end differential.
//!
//! Both sides copy the same two-DB seed fixture and run the same six-call
//! sequence. The oracle drives v4's REAL `handleSendMessage` (which calls
//! `processMessage` + `executeTurnChain`) with ONLY the model boundaries + the
//! out-of-scope subsystems mocked to match the Rust injected seams; this test
//! composes the ported services through `process_message` / `execute_turn_chain`,
//! replaying the recorded canned streams + summary completion. Then:
//!
//!   1. each call's ordered event trace (decoded to JSON) is compared
//!      byte-for-byte against v4's recorded SSE frames — INCLUDING the
//!      `turnStart` / `turnComplete` / `chainComplete` chain frames and the
//!      `debugLLMRequest` frame (which v4 emits but the Rust port does not: the
//!      harness DROPS every `debugLLMRequest` / `debugContext` / `keep-alive`
//!      frame + the many pre-send `status` frames v4's tool/validate/gather
//!      pipeline emits that the seamed Rust path does not — the diffed vocabulary
//!      is the load-bearing set: `status(initializing/resolving/sending/streaming/
//!      preparing/gathering)`, `turnStart`, `content`, `reasoning`, `done`,
//!      `turnComplete`, `chainComplete`, `error`);
//!   2. the `chats` / `chat_messages` / `background_jobs` dumps are compared in a
//!      minted-values normalization (message/job ids remapped to first-appearance
//!      tokens in a shared cross-table map; minted timestamps placeholdered).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-orch-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-orch-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-orchestrator-fixture.ts
//!   QT_FIXTURE_ORCH_MAIN=/tmp/qt-orch-main.db QT_FIXTURE_ORCH_MOUNT=/tmp/qt-orch-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-orchestrator.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- orchestrator-tier3
//! Run:
//!   QT_ORACLE_ORCHESTRATOR=/tmp/oracle-orchestrator.ndjson \
//!   QT_FIXTURE_ORCH_MAIN=/tmp/qt-orch-main.db QT_FIXTURE_ORCH_MOUNT=/tmp/qt-orch-mount.db \
//!     cargo test -p quilltap-harness --test orchestrator_tier3_equivalence

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    canned_completion_key, CannedCompletionProvider, CompletionMessage, CompletionRole,
    CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::build_context::RealBuildContextSeams;
use quilltap_core::services::carina_query::{CarinaQueryDeps, RealCarinaQuery};
use quilltap_core::services::carina_runner::ClosureProspero;
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::message_finalizer::{NoAnswerConfirmation, NoAsyncCompression};
use quilltap_core::services::native_tool_loop::{NoToolCallDetector, RegistryToolCallDetector};
use quilltap_core::services::orchestrator::{
    self, ExecuteTurnChainOptions, OrchestratorChatSettings, OrchestratorDeps, ProcessClock,
    ProcessMessageInput, SendMessageOptions,
};
use quilltap_core::services::tool_execution::CannedToolRunner;
use quilltap_core::services::turn_orchestrator::ChainConfig;
use quilltap_core::tools::ask_carina::{ErasedAskCarina, TypedAskCarina};
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::Value;

mod common;

// ---------------------------------------------------------------------------
// Spec.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CallW {
    name: String,
    /// `"single"` / `"chain"` — documentary; the harness always drives the chain
    /// after `process_message` (v4's `handleSendMessage` does the same), so the
    /// field is informational only.
    #[allow(dead_code)]
    kind: String,
    chat_id: String,
    #[serde(default)]
    content: String,
    continue_mode: bool,
    #[serde(default)]
    responding_participant: Option<String>,
    #[serde(rename = "cheapLLMSettings")]
    cheap_llm_settings: bool,
    #[serde(rename = "summaryCheck")]
    #[allow(dead_code)]
    summary_check: bool,
    /// The committed RNG byte stream (auto-detect); mirrors the oracle's
    /// crypto.randomBytes mock. Absent → empty.
    #[serde(default)]
    rng_bytes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    frozen_now_ms: i64,
    #[serde(default)]
    local_offset_minutes: i64,
    calls: Vec<CallW>,
    // W4.10a: the fixture builder seeds `api_keys` from the spec's `apiKeys` map;
    // the Rust harness no longer reads it (the real `DbApiKeys` resolver reads the
    // seeded table). Left off the struct — serde ignores the unknown key.
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/orchestrator-tier3.json")
}

// ---------------------------------------------------------------------------
// Oracle rows.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CannedMsgW {
    role: String,
    content: String,
}
#[derive(Deserialize)]
struct ChunkW {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    done: Option<bool>,
    #[serde(default)]
    usage: Option<UsageW>,
    #[serde(default)]
    error: Option<String>,
    /// W4.1g native-call case: the provider raw response carried on the terminal
    /// chunk (the canned detector keys on `raw_response.marker`).
    #[serde(default, rename = "rawResponse")]
    raw_response: Option<Value>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}
#[derive(Deserialize)]
struct CannedStreamW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    /// W4.1g: the tool slate the oracle recorded reaching `streamMessage` for this
    /// call key (`actualTools`; `[]` when the slate was empty). The Rust side must
    /// pass the identical array — proven per call.
    #[serde(default)]
    tools: Value,
    sequences: Vec<Vec<ChunkW>>,
}
#[derive(Deserialize)]
struct CannedCompletionW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    response: String,
    /// W4.11a: the usage v4's cheap-LLM completion returns — carried through so the
    /// executor's `llm_logs` row records the same `usage` v4 logs.
    #[serde(default)]
    usage: Option<UsageW>,
}

fn to_completion_messages(m: &[CannedMsgW]) -> Vec<CompletionMessage> {
    m.iter()
        .map(|m| CompletionMessage {
            role: match m.role.as_str() {
                "system" => CompletionRole::System,
                "assistant" => CompletionRole::Assistant,
                _ => CompletionRole::User,
            },
            content: m.content.clone(),
        })
        .collect()
}

fn chunk_to_result(c: &ChunkW) -> StreamChunkResult {
    if let Some(err) = &c.error {
        return Err(StreamError::new(err.clone()));
    }
    if let Some(r) = &c.reasoning {
        return Ok(StreamChunk {
            reasoning_content: Some(r.clone()),
            ..Default::default()
        });
    }
    if c.done == Some(true) {
        let usage = c.usage.map(|u| StreamUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });
        let mut chunk = StreamChunk::done(usage);
        chunk.raw_response = c.raw_response.clone();
        return Ok(chunk);
    }
    Ok(StreamChunk::content(c.content.clone().unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Stateful canned streaming provider (per-key queue of sequences).
// ---------------------------------------------------------------------------

struct QueuedStreamingProvider {
    queues: Mutex<HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>>>,
    /// The tool slate the ORACLE recorded reaching the wire, per call key (W4.1g).
    expected_tools: HashMap<String, Value>,
    /// The tool slate the RUST side actually passed, per call key — recorded here
    /// as each stream fires, then diffed against `expected_tools` after the run.
    recorded_tools: Mutex<HashMap<String, Value>>,
}
impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedStreamW]) -> Self {
        let mut queues: HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>> =
            HashMap::new();
        let mut expected_tools: HashMap<String, Value> = HashMap::new();
        for row in rows {
            let messages = to_completion_messages(&row.messages);
            let key = canned_stream_key(&row.provider, &row.model, row.temperature, &messages);
            let q = queues.entry(key.clone()).or_default();
            for seq in &row.sequences {
                q.push_back(seq.iter().map(chunk_to_result).collect());
            }
            // Normalize a missing/null recorded tools to `[]` (v4 records `[]` for
            // an empty slate).
            let tools = if row.tools.is_array() {
                row.tools.clone()
            } else {
                Value::Array(Vec::new())
            };
            expected_tools.insert(key, tools);
        }
        Self {
            queues: Mutex::new(queues),
            expected_tools,
            recorded_tools: Mutex::new(HashMap::new()),
        }
    }
}
impl StreamingCompletionProvider for QueuedStreamingProvider {
    fn stream_message(
        &self,
        provider: &str,
        _base_url: Option<&str>,
        params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        let key = canned_stream_key(
            provider,
            &params.model,
            params.temperature,
            &params.messages,
        );
        // Record the tool slate this call passed at the wire (W4.1g). v4 records
        // `[]` for an empty slate; normalize `None` → `[]`.
        {
            let tools = params.tools.clone().unwrap_or(Value::Array(Vec::new()));
            self.recorded_tools
                .lock()
                .unwrap()
                .entry(key.clone())
                .or_insert(tools);
        }
        let sequence: Vec<StreamChunkResult> = {
            let mut queues = self.queues.lock().unwrap();
            match queues.get_mut(&key).and_then(|q| q.pop_front()) {
                Some(seq) => seq,
                None => vec![Err(StreamError::new(format!(
                    "no canned stream queued for key ({provider}, model {}, {} msgs)",
                    params.model,
                    params.messages.len(),
                )))],
            }
        };
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(sequence.len().max(1) + 1);
            for item in sequence {
                let _ = tx.send(item).await;
            }
            rx
        }
    }
}

// ---------------------------------------------------------------------------
// W4.2u/W4.10a: the REAL uncensored-reroute router. The differential runs v4's
// REAL danger resolution (global mode AUTO_ROUTE, no `uncensoredTextProfileId`, so
// only the FIRST-branch reroute on an actively-dangerous chat fires), so the Rust
// side wires the REAL `DangerContentRouter` (scans `connection_profiles` for an
// `isDangerousCompatible` profile off the read pool). W4.10a swaps the prior canned
// key map for the REAL DB-backed `DbApiKeys` resolver, reading the fixture-seeded
// `api_keys` table (the oracle reads its own seeded rows).
//
// The never-called pricing fetch backing the in-spine `check_model_supports_tools`
// (W4.10a): an empty fetch → OPENROUTER cache empty → v4's "default to native
// tools"; every non-OPENROUTER provider answers from the static fallback table.
// ---------------------------------------------------------------------------

struct NoPricingFetch;
impl quilltap_core::services::pricing_fetcher::PricingFetch for NoPricingFetch {
    fn openrouter_public_models(&self) -> Option<Value> {
        None
    }
    fn openrouter_sdk_models(&self, _api_key: &str) -> Option<Value> {
        None
    }
    fn ollama_tags(&self, _base_url: &str) -> Option<Value> {
        None
    }
}

/// A throwaway [`SelfInventoryEnv`] for the tool runners the carina / ask_carina /
/// Brahma engines carry (the corpus's carina / ask_carina cases never call
/// `self_inventory`).
fn dummy_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: String::new(),
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Browser,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

/// The Prospero-carina-error post seam for the ask_carina engine's `TypedAskCarina`
/// (a no-op — its errors never fire in the corpus). A `Clone` unit struct because
/// `TypedAskCarina` requires `P: PostProsperoCarinaError + Clone` (it clones the
/// writer per call).
#[derive(Clone)]
struct HarnessProspero;
impl quilltap_core::services::carina_runner::PostProsperoCarinaError for HarnessProspero {
    fn post(
        &mut self,
        _args: quilltap_core::services::carina_runner::ProsperoCarinaErrorArgs,
    ) -> Result<(), quilltap_core::services::carina_runner::CarinaRunError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Event trace normalization: drop the frames the seamed Rust path does not emit.
// ---------------------------------------------------------------------------

/// The load-bearing event vocabulary the differential compares. v4 emits many
/// pre-send `status` frames (loading_tools / validating / warning / …) + a
/// `debugLLMRequest` frame that the seamed Rust orchestrator does not; the Rust
/// path emits the initializing/resolving/gathering/preparing/sending/streaming
/// statuses + the turn/content/done/chain frames. To compare the two, both traces
/// are filtered to the shared vocabulary: every non-status frame is kept; a
/// `status` frame is kept ONLY when its `stage` is in the shared set.
fn shared_status_stage(stage: &str) -> bool {
    matches!(
        stage,
        "initializing"
            | "resolving"
            | "gathering"
            | "preparing"
            | "sending"
            | "streaming"
            | "retrying"
            | "rerouting"
    )
}

fn filter_events(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter(|e| {
            let obj = match e.as_object() {
                Some(o) => o,
                None => return false,
            };
            // Drop debug frames + keep-alives (not modeled by the Rust EventSink)
            // and the transport-shell `error` frame (v4's `handleStreamError`; the
            // Rust `process_message` propagates the error instead of emitting a
            // frame — the frame is a Phase-4 transport concern).
            if obj.contains_key("debugLLMRequest")
                || obj.contains_key("debugContext")
                || obj.contains_key("fallbackInfo")
                || obj.contains_key("error")
            {
                return false;
            }
            if let Some(status) = obj.get("status").and_then(Value::as_object) {
                let stage = status.get("stage").and_then(Value::as_str).unwrap_or("");
                return shared_status_stage(stage);
            }
            true
        })
        .map(|e| {
            // Normalize the minted assistant-message id in `done` / `turnComplete`
            // frames (both sides mint a fresh UUID) to a stable placeholder. The
            // `nextSpeakerId` / `participantId` are SEEDED participant ids (identical
            // on both sides), so they stay literal.
            let mut e = e.clone();
            if let Some(obj) = e.as_object_mut() {
                // The Courier `pendingExternalTurn` frame also carries the minted
                // placeholder id (W4.4a4).
                if obj.contains_key("done")
                    || obj.contains_key("turnComplete")
                    || obj.contains_key("pendingExternalTurn")
                {
                    if let Some(v) = obj.get_mut("messageId") {
                        if v.is_string() {
                            *v = Value::String("<msgid>".into());
                        }
                    }
                }
                // W4.10a: the `carinaAnswer` frame (v4's `onPosted` → the engine's
                // live emit) carries the posted Carina message — a fresh minted
                // `id` + `createdAt` on each side. Placeholder both; the rest
                // (content / answererId / systemSender / carinaMeta) is deterministic.
                if let Some(msg) = obj.get_mut("carinaAnswer").and_then(Value::as_object_mut) {
                    if let Some(v) = msg.get_mut("id") {
                        if v.is_string() {
                            *v = Value::String("<msgid>".into());
                        }
                    }
                    if let Some(v) = msg.get_mut("createdAt") {
                        if v.is_string() {
                            *v = Value::String("<ts>".into());
                        }
                    }
                }
            }
            e
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Oracle NDJSON.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OracleLine {
    kind: String,
    #[serde(default)]
    call: Option<String>,
    #[serde(flatten)]
    rest: Value,
}

fn dump_table(db: &Db, table: &str) -> Value {
    let table = table.to_string();
    db.read_main(move |c| quilltap_core::db::dump_table_json_conn(c, &table, "id"))
        .expect("table dump")
}

// ---------------------------------------------------------------------------
// The test.
// ---------------------------------------------------------------------------

#[test]
fn orchestrator_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_ORCHESTRATOR") else {
        eprintln!("QT_ORACLE_ORCHESTRATOR not set; skipping");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_ORCH_MAIN") else {
        eprintln!("QT_FIXTURE_ORCH_MAIN not set; skipping");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_ORCH_MOUNT") else {
        eprintln!("QT_FIXTURE_ORCH_MOUNT not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec readable"))
            .expect("spec parses");

    // Parse the oracle NDJSON.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let mut want_events: HashMap<String, Vec<Value>> = HashMap::new();
    let mut canned_streams: Vec<CannedStreamW> = Vec::new();
    let mut canned_completions: Vec<CannedCompletionW> = Vec::new();
    let mut want_tables: HashMap<String, Value> = HashMap::new();
    let mut want_llm_logs: Option<Vec<Value>> = None;
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let parsed: OracleLine = serde_json::from_str(line).expect("oracle line parses");
        match parsed.kind.as_str() {
            "events" => {
                let evs = parsed.rest.get("events").cloned().unwrap_or(Value::Null);
                let arr = evs.as_array().cloned().unwrap_or_default();
                want_events.insert(parsed.call.clone().unwrap(), filter_events(&arr));
            }
            "threw" => { /* recorded alongside the events line; the events line carries `threw` */ }
            "cannedStream" => {
                canned_streams.push(serde_json::from_value(parsed.rest).expect("cannedStream"))
            }
            "cannedCompletion" => canned_completions
                .push(serde_json::from_value(parsed.rest).expect("cannedCompletion")),
            "compression" | "cost" => { /* recorded; the corpus keeps these empty/no-op */ }
            "table" => {
                let table = parsed
                    .rest
                    .get("table")
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_string();
                want_tables.insert(table, parsed.rest);
            }
            "llmlogs" => want_llm_logs = Some(common::oracle_llm_logs(&parsed.rest)),
            other => panic!("unknown oracle line kind: {other}"),
        }
    }

    // Copy the fixture DBs to a scratch dir.
    let scratch = std::env::temp_dir().join(format!("qt-orch-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("orch-main.db");
    let work_mount = scratch.join("orch-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    // W4.11a: materialize the llm-logs partition so the per-call `with_logging`
    // executor + the primary stream can write their `llm_logs` rows (the oracle
    // does the same via `SQLITE_LLM_LOGS_PATH`).
    let work_ll = scratch.join("orch-llm-logs.db");
    let _ = std::fs::remove_file(&work_ll);
    common::materialize_llm_logs(&work_ll, &spec.test_pepper_base64);

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: Some(work_ll.clone()),
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture instance");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    // The providers: canned streams (per-key queue) + canned summary completion.
    // W4.11a: the STATEFUL streaming provider (+ the embedding provider) are shared
    // by value across the borrowed spine deps AND the owned, effectively-`'static`
    // `ask_carina` / Brahma engine seams via `Arc` (the blanket impls). This lets
    // the inner ask_carina query / Brahma console draw from the SAME per-key queues
    // the single v4 mock served — the whole point of the Arc ownership work.
    let streaming = Arc::new(QueuedStreamingProvider::from_oracle(&canned_streams));
    let mut completion = CannedCompletionProvider::new();
    for row in &canned_completions {
        let messages = to_completion_messages(&row.messages);
        let _key = canned_completion_key(&row.provider, &row.model, row.temperature, &messages);
        completion = completion.with_response(
            &row.provider,
            &row.model,
            row.temperature,
            &messages,
            &row.response,
            row.usage.as_ref().map(|u| CompletionUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
        );
    }
    let embedding = Arc::new(CannedEmbeddingProvider::new());
    // Round-3 unification (Group 2): the buildContext whisper writers run LIVE
    // (RealBuildContextSeams). The oracle un-mocks the same writers; the resulting
    // whisper rows appear in the diffed chat_messages dump.
    let bc_seams = RealBuildContextSeams { db: &db };
    // W4.10a: the REAL DB-backed ApiKeyResolver (reads the fixture-seeded `api_keys`
    // table — the oracle un-monkey-patches `findApiKeyByIdAndUserId` to read its own
    // seeded rows). Closes the W4.7d→W4.4b handoff: the danger-reroute key material
    // now comes off the real table end to end.
    let router =
        quilltap_core::services::dangerous_content::provider_routing::DangerContentRouter::new(
            db.clone(),
            quilltap_core::services::dangerous_content::provider_routing::DbApiKeys(db.clone()),
        );
    // W4.10a: `model_supports_native_tools` is now sourced in-spine from the REAL
    // `check_model_supports_tools` over this fetcher. The fetch is a seam: an empty
    // fetch → OPENROUTER cache empty → v4's "default to native tools" (model not
    // found); every non-OPENROUTER provider answers from the static fallback table.
    // The oracle un-mocks `checkModelSupportsTools` + mocks `getPricingCache` empty,
    // matching this.
    let pricing = quilltap_core::services::pricing_fetcher::PricingFetcher::new(NoPricingFetch);

    // W4.10a: the REAL Carina query engine backing the finalizer's `@Name:` markup
    // runner (replacing the `NoCarina` no-op). Its inner model call replays a
    // recorded canned stream (proving the engine's system-prompt bytes), the tool
    // loop is empty (no carina-answer tool calls in the corpus), and Brahma is
    // never consulted (the answerer is a regular character). Constructed per call
    // (below) so it can hold that call's live SSE sink for the `carinaAnswer` emit.
    let carina_tool_runner = CannedToolRunner::new();
    let carina_detector = NoToolCallDetector;
    // W4.11a: the REAL Brahma console over the SHARED Arc streaming provider — so a
    // `@Name:` answerer resolving to Brahma (`brahma_maxdepth` case) drives the
    // console's one-shot stream out of the same per-key queues v4's mock served.
    // A real `BuiltInToolRunner` + the registry detector make the console's tool
    // loop real (the case keeps it a plain answer). Inert for every non-Brahma case.
    let carina_brahma = quilltap_core::services::brahma_console::RealBrahmaConsole::new(
        db.clone(),
        Arc::clone(&streaming),
        BuiltInToolRunner::new(db.clone(), dummy_env()),
        RegistryToolCallDetector::built_in(),
        true,
    );

    // W4.11a: the `ask_carina` TOOL engine — a `TypedAskCarina` over Arc clones of
    // the shared providers, erased into the `ErasedAskCarina` seam the spine wires
    // into its per-turn `BuiltInToolRunner`. Its `tool_runner` is a SEPARATE
    // `BuiltInToolRunner` (no ask_carina seam — the type-level cycle note); its
    // `brahma` is its own console (never reached — the ask_carina answerer is a
    // regular character). Constructed once (sink-agnostic; the per-turn sink is a
    // `run` argument). Inert until a native `ask_carina` call dispatches
    // (`ask_carina_tool` case).
    let ask_carina = ErasedAskCarina::new(TypedAskCarina {
        db: db.clone(),
        embedding: Arc::clone(&embedding),
        streaming: Arc::clone(&streaming),
        tool_runner: BuiltInToolRunner::new(db.clone(), dummy_env()),
        tool_detector: RegistryToolCallDetector::built_in(),
        brahma: quilltap_core::services::brahma_console::RealBrahmaConsole::new(
            db.clone(),
            Arc::clone(&streaming),
            BuiltInToolRunner::new(db.clone(), dummy_env()),
            RegistryToolCallDetector::built_in(),
            true,
        ),
        prospero: HarnessProspero,
        model_supports_native_tools: true,
        now_ms: spec.frozen_now_ms as f64,
    });

    let mut got_events: Vec<(String, Vec<Value>)> = Vec::new();

    for call in &spec.calls {
        let sink = RecordingSink::new();
        // W4.11a: a per-call executor that logs each cheap-LLM provider call into
        // `llm_logs` (v4's un-mocked `logLLMCall`). The distill (memory-keyword-
        // extraction → MEMORY_EXTRACTION, per-call characterId) and the summary
        // fold/title (SUMMARIZATION / TITLE_GENERATION, characterId null) carry
        // chatId = this call's chat; v4's cheap-LLM log passes messageId = undefined
        // for both, so `message_id: None`.
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: spec.user_id.clone(),
            chat_id: Some(call.chat_id.clone()),
            message_id: None,
            ctx: LogContext::none(),
        });
        // Fresh finalizer seams per call.
        let mut confirmation = NoAnswerConfirmation;
        let mut compression = NoAsyncCompression;
        let mut cost = quilltap_core::services::message_finalizer::NoCostTracking;
        let mut carina_query = RealCarinaQuery::new(CarinaQueryDeps {
            db: &db,
            embedding: &embedding,
            streaming: &streaming,
            tool_runner: &carina_tool_runner,
            tool_detector: &carina_detector,
            sink: &sink,
            brahma: &carina_brahma,
            // Diff-irrelevant: carina's system prompt is fixed before its tool build.
            model_supports_native_tools: true,
            now_ms: spec.frozen_now_ms as f64,
        });
        let mut prospero = ClosureProspero(|_a| Ok(()));
        let mut rng_bytes = quilltap_core::tools::rng::FixedBytes::new(call.rng_bytes.clone());
        let orchestrator_seams = HarnessOrchestratorSeams {
            cheap_llm_settings: call.cheap_llm_settings,
        };

        let mut deps = OrchestratorDeps {
            db: &db,
            embedding: &embedding,
            completion: &completion,
            streaming: &streaming,
            executor: &executor,
            ask_carina: &ask_carina,
            sink: &sink,
            pricing: &pricing,
            build_context_seams: &bc_seams,
            orchestrator_seams: &orchestrator_seams,
            // The attachment subsystem (W4.4b): the corpus keeps `fileIds` empty
            // and carries no prior-image message attachments, so `loadAndProcessFiles`
            // early-returns and the Lantern K-seam is never invoked — these are inert.
            file_bytes: &quilltap_core::services::chat_files::NotConfiguredBytes,
            image_transcoder: &quilltap_core::files::image_processing::NotConfiguredTranscoder,
            danger_router: &router,
            confirmation: &mut confirmation,
            compression: &mut compression,
            cost: &mut cost,
            carina_query: &mut carina_query,
            prospero: &mut prospero,
            rng_bytes: &mut rng_bytes,
        };
        // The spine now constructs the real registry-backed tool detector +
        // provider-text strategy internally (W4.7c). The corpus carries no native
        // tool calls (the canned streams' raw_response has no tool_use blocks) and
        // no provider text markers, so both passes no-op after the real (now
        // provider-reshaped) slate reaches the wire — proven by the tools-at-wire
        // assertion. A native tool CALL end-to-end is proven separately by
        // `native_tool_loop_tier3` (v4's REAL `runNativeToolLoop` + threading).

        let make_input = |chat_id: &str, content: &str, continue_mode: bool, resp: Option<&str>| {
            ProcessMessageInput {
                chat_id: chat_id.to_string(),
                user_id: spec.user_id.clone(),
                options: SendMessageOptions {
                    continue_mode,
                    content: content.to_string(),
                    responding_participant_id: resp.map(String::from),
                    ..Default::default()
                },
                clock: ProcessClock {
                    now_ms: spec.frozen_now_ms,
                    local_offset_minutes: spec.local_offset_minutes,
                    random01: 0.0,
                },
                model_context_limit: 200_000,
                timestamp_config: None,
                timezone: Some("UTC".to_string()),
                provider_supports_web_search: false,
            }
        };

        // Initial processMessage.
        let initial = rt.block_on(orchestrator::process_message(
            &mut deps,
            &make_input(
                &call.chat_id,
                &call.content,
                call.continue_mode,
                call.responding_participant.as_deref(),
            ),
        ));

        match initial {
            Ok(result) => {
                // v4's `handleSendMessage` ALWAYS drives `executeTurnChain` after
                // `processMessage` (the chain's own guard decides whether it fires
                // a model turn). So the harness drives it for every case — the
                // single-character cases either stop at `user_turn` (with a real
                // user participant) or max-depth (the `single_basic` depth-guard).
                {
                    // Drive executeTurnChain: each chained turn re-enters
                    // process_message in continue mode with the resolved responder.
                    let chat_id = call.chat_id.clone();
                    let user_id = spec.user_id.clone();
                    let frozen = spec.frozen_now_ms;
                    let offset = spec.local_offset_minutes;
                    let make_chain_input = move |pid: String| ProcessMessageInput {
                        chat_id: chat_id.clone(),
                        user_id: user_id.clone(),
                        options: SendMessageOptions {
                            continue_mode: true,
                            content: String::new(),
                            responding_participant_id: Some(pid),
                            ..Default::default()
                        },
                        clock: ProcessClock {
                            now_ms: frozen,
                            local_offset_minutes: offset,
                            random01: 0.0,
                        },
                        model_context_limit: 200_000,
                        timestamp_config: None,
                        timezone: Some("UTC".to_string()),
                        provider_supports_web_search: false,
                    };
                    rt.block_on(orchestrator::execute_turn_chain(
                        &mut deps,
                        ExecuteTurnChainOptions {
                            chat_id: call.chat_id.clone(),
                            initial_result: result,
                            initial_continue_mode: call.continue_mode,
                            never_pause_for_user: false,
                            single_turn: false,
                            chain_start_time_ms: frozen,
                            config: ChainConfig::default(),
                        },
                        frozen,
                        0.0,
                        make_chain_input,
                    ))
                    .expect("chain");
                }
            }
            Err(_e) => {
                eprintln!("process_message({}) returned Err: {:?}", call.name, _e);
                // The mid-stream-error case surfaces the stream error to the caller;
                // v4's `handleSendMessage` catch emits an `error` frame at the
                // transport shell. The Rust `process_message` propagates the error
                // rather than emitting the frame (the frame belongs to the transport
                // layer, Phase 4) — so the harness drops v4's `error` frame from the
                // compared vocabulary for this case (see `filter_events`).
            }
        }

        got_events.push((call.name.clone(), filter_events(&sink.events_json())));
    }

    // --- events ---
    for (name, got) in &got_events {
        let want = want_events
            .get(name)
            .unwrap_or_else(|| panic!("oracle events missing for {name}"));
        assert_events_eq(name, got, want);
    }

    // --- tool slate AT THE WIRE (W4.1g) ---
    // Every `streamMessage` call the Rust spine made must have passed the exact
    // tool array v4 passed for the same call key. This proves the real buildTools
    // slate reaches the provider on every case (not just that the tables match).
    {
        let recorded = streaming.recorded_tools.lock().unwrap();
        for (key, got_tools) in recorded.iter() {
            let want_tools = streaming.expected_tools.get(key).unwrap_or_else(|| {
                panic!("Rust made a stream call with no oracle-recorded tools for key:\n{key}")
            });
            if got_tools != want_tools {
                let gn = wire_tool_names(got_tools);
                let wn = wire_tool_names(want_tools);
                panic!(
                    "tool slate at wire mismatch for key:\n{key}\n  got:  {gn:?}\n  want: {wn:?}"
                );
            }
        }
    }

    // --- table dumps (minted-values remap) ---
    let mut idmap: HashMap<String, String> = HashMap::new();
    let mut got_chats = dump_table(&db, "chats");
    let mut got_msgs = dump_table(&db, "chat_messages");
    let mut got_jobs = dump_table(&db, "background_jobs");
    let mut want_chats = want_tables.remove("chats").expect("oracle chats");
    let mut want_msgs = want_tables.remove("chat_messages").expect("oracle msgs");
    let mut want_jobs = want_tables.remove("background_jobs").expect("oracle jobs");

    // Normalize both sides with a shared remap (minted message/job ids →
    // first-appearance tokens; minted timestamps → <ts>). Seeded ids stay literal.
    let ctx = Normalizer::new();
    ctx.normalize_chats(&mut got_chats);
    ctx.normalize_chats(&mut want_chats);
    ctx.normalize_messages(&mut got_msgs, &mut idmap);
    let mut idmap2: HashMap<String, String> = HashMap::new();
    ctx.normalize_messages(&mut want_msgs, &mut idmap2);
    // Jobs are normalized AFTER messages so the shared message idmap is populated:
    // a job payload's `turnOpenerMessageId` / `extractionAnchorMessageId` are message
    // ids (minted fresh for a non-continue send, seeded otherwise), remapped through
    // the same per-side map so they verify by relationship (a seeded id → the same
    // token both sides; a minted id → matching tokens).
    ctx.normalize_jobs(&mut got_jobs, &idmap);
    ctx.normalize_jobs(&mut want_jobs, &idmap2);

    assert_table_eq("chats", &got_chats, &want_chats);
    assert_table_eq("chat_messages", &got_msgs, &want_msgs);
    assert_table_eq("background_jobs", &got_jobs, &want_jobs);

    // --- llm_logs (W4.11a) ---
    // The per-call `with_logging` executor wrote the cheap-LLM rows (distill
    // MEMORY_EXTRACTION, summary-fold SUMMARIZATION, title TITLE_GENERATION);
    // primary_stream wrote CHAT_MESSAGE rows. TWO row families are documented
    // seam/mock artifacts filtered from BOTH sides:
    //   * `CHAT_MESSAGE` — the Rust primary_stream logs these, but v4's
    //     service-level `streamMessage` mock swallows its own CHAT_MESSAGE log
    //     (`streaming.service.ts:405` lives INSIDE the mocked wrapper), so v4
    //     writes none. That row shape is proven byte-exact by `primary_stream_tier3`
    //     (W4.11b); relocating the oracle's stream mock to the provider level is
    //     out of scope (it would re-risk the 24-case corpus).
    //   * `DANGER_CLASSIFICATION` — v4's `resolveMessageDangerState` classifies the
    //     user message INLINE (a cheap-LLM call → one log row) for non-off / not-
    //     already-dangerous chats; that classify is a documented seam in the Rust
    //     spine (behaviorally inert here — the canned cheap response resolves
    //     non-dangerous → no reroute — so the diffed tables/events already match).
    //     The Rust side writes no such row; filtered on both sides. (The danger
    //     logging seam proper is W4.11c.)
    let strip_seam_rows = |rows: Vec<Value>| -> Vec<Value> {
        rows.into_iter()
            .filter(|r| {
                let t = r["type"].as_str().unwrap_or("");
                t != "CHAT_MESSAGE" && t != "DANGER_CLASSIFICATION"
            })
            .collect()
    };
    let got_logs = strip_seam_rows(common::dump_llm_logs(&db));
    let want_logs = strip_seam_rows(want_llm_logs.expect("oracle emitted no llmlogs row"));
    assert_eq!(
        got_logs.len(),
        want_logs.len(),
        "llm_logs row count diverges (got {} vs oracle {})\n got: {:#?}\n want: {:#?}",
        got_logs.len(),
        want_logs.len(),
        got_logs,
        want_logs,
    );
    assert_eq!(got_logs, want_logs, "llm_logs rows diverge");

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
}

// ---------------------------------------------------------------------------
// Normalization.
// ---------------------------------------------------------------------------

struct Normalizer;
impl Normalizer {
    fn new() -> Self {
        Normalizer
    }

    /// chats: minted `updatedAt` / `lastMessageAt` / the assistant message ids
    /// inside participants stay pinned (participants are seeded). Placeholder the
    /// timestamp columns unconditionally (both sides mint the same frozen/real
    /// wall clock; the frozen v4 side + the real Rust side differ, so collapse).
    fn normalize_chats(&self, dump: &mut Value) {
        if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
            for row in rows {
                if let Some(obj) = row.as_object_mut() {
                    for c in [
                        "updatedAt",
                        "lastMessageAt",
                        "contextSummary",
                        "compactionGeneration",
                        "lastSummaryTurn",
                        "lastRenameCheckInterchange",
                        "messageCount",
                        "totalPromptTokens",
                        "totalCompletionTokens",
                        "estimatedCostUSD",
                    ] {
                        if let Some(v) = obj.get(c) {
                            if !v.is_null() {
                                obj.insert(c.to_string(), Value::String(format!("<{c}>")));
                            }
                        }
                    }
                }
            }
        }
    }

    /// chat_messages: seeded rows keep their ids; minted rows (createdAt-minted)
    /// placeholder `id` (remapped) + `createdAt` + the volatile token columns.
    fn normalize_messages(&self, dump: &mut Value, idmap: &mut HashMap<String, String>) {
        if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
            // Sort by (chatId, type, role, content) — NOT createdAt: v4's frozen
            // clock collapses every minted `createdAt` to one value (so it sorts
            // by role/content) while the Rust real clock makes them distinct, so a
            // createdAt-keyed sort would order the two sides differently. The
            // (chatId, type, role, content) tuple is stable + identical on both.
            rows.sort_by_key(|r| {
                (
                    r.get("chatId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    r.get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    r.get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    r.get("content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            });
            for row in rows {
                if let Some(obj) = row.as_object_mut() {
                    // A seeded id is 36-char and present in the fixture; a minted id
                    // is remapped. We cannot know seeded vs minted structurally, so
                    // remap EVERY id to a first-appearance token (seeded ids on both
                    // sides are identical → same token).
                    if let Some(id) = obj.get("id").and_then(Value::as_str) {
                        let n = idmap.len();
                        let tok = idmap
                            .entry(id.to_string())
                            .or_insert_with(|| format!("<m{n}>"))
                            .clone();
                        obj.insert("id".into(), Value::String(tok));
                    }
                    for c in [
                        "createdAt",
                        "tokenCount",
                        "promptTokens",
                        "completionTokens",
                    ] {
                        if let Some(v) = obj.get(c) {
                            if !v.is_null() {
                                obj.insert(c.to_string(), Value::String(format!("<{c}>")));
                            }
                        }
                    }
                }
            }
        }
    }

    /// background_jobs: all minted — remap the payload's message-id fields through
    /// the shared message idmap, re-sort by (type, payload) then placeholder `id` +
    /// the timestamp/attempt columns. The `type` + payload chatId are pinned so the
    /// sort is stable across sides.
    fn normalize_jobs(&self, dump: &mut Value, idmap: &HashMap<String, String>) {
        // Remap a message-id-valued payload field through the shared idmap: a seeded
        // id → the same token both sides; a minted id (non-continue turn opener /
        // extraction anchor) → matching tokens; an unknown id → a stable placeholder
        // (never leaks a raw minted UUID into the diff).
        let remap = |v: &Value| -> Option<Value> {
            let s = v.as_str()?;
            Some(Value::String(
                idmap.get(s).cloned().unwrap_or_else(|| "<msgref>".into()),
            ))
        };
        if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
            // Remap payload message-id fields FIRST so the (type, payload) sort key
            // is deterministic across sides (minted ids would otherwise differ).
            for row in rows.iter_mut() {
                if let Some(obj) = row.as_object_mut() {
                    if let Some(payload_str) = obj.get("payload").and_then(Value::as_str) {
                        if let Ok(mut payload) = serde_json::from_str::<Value>(payload_str) {
                            if let Some(pobj) = payload.as_object_mut() {
                                // `carinaMessageId` (W4.10a) is the posted Carina
                                // message's minted id — a `chat_messages` row, so it
                                // is in the shared idmap and remaps to the matching
                                // token.
                                for f in [
                                    "turnOpenerMessageId",
                                    "extractionAnchorMessageId",
                                    "carinaMessageId",
                                ] {
                                    if let Some(v) = pobj.get(f) {
                                        if !v.is_null() {
                                            if let Some(mapped) = remap(v) {
                                                pobj.insert(f.to_string(), mapped);
                                            }
                                        }
                                    }
                                }
                                obj.insert(
                                    "payload".into(),
                                    Value::String(serde_json::to_string(&payload).unwrap()),
                                );
                            }
                        }
                    }
                }
            }
            rows.sort_by_key(|r| {
                (
                    r.get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    r.get("payload")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            });
            for row in rows {
                if let Some(obj) = row.as_object_mut() {
                    obj.insert("id".into(), Value::String("<id>".into()));
                    for c in ["scheduledAt", "createdAt", "updatedAt", "startedAt"] {
                        if let Some(v) = obj.get(c) {
                            if !v.is_null() {
                                obj.insert(c.to_string(), Value::String(format!("<{c}>")));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn wire_tool_names(tools: &Value) -> Vec<String> {
    tools
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    t.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn assert_events_eq(name: &str, got: &[Value], want: &[Value]) {
    if got != want {
        let g = serde_json::to_string_pretty(got).unwrap();
        let w = serde_json::to_string_pretty(want).unwrap();
        panic!("event trace mismatch for {name}\n--- got ---\n{g}\n--- want ---\n{w}");
    }
}

fn assert_table_eq(name: &str, got: &Value, want: &Value) {
    let got_rows = got.get("rows").cloned().unwrap_or(Value::Null);
    let want_rows = want.get("rows").cloned().unwrap_or(Value::Null);
    if got_rows != want_rows {
        let g = serde_json::to_string_pretty(&got_rows).unwrap();
        let w = serde_json::to_string_pretty(&want_rows).unwrap();
        panic!("table {name} mismatch\n--- got ---\n{g}\n--- want ---\n{w}");
    }
}

// ---------------------------------------------------------------------------
// The harness orchestrator seams (mirroring the oracle mocks).
// ---------------------------------------------------------------------------

struct HarnessOrchestratorSeams {
    cheap_llm_settings: bool,
}
impl orchestrator::OrchestratorSeams for HarnessOrchestratorSeams {
    fn chat_settings(&self, _user_id: &str) -> Option<OrchestratorChatSettings> {
        // The fixture's SINGLE chat_settings row is shared by every chat — it has
        // `cheapLLMSettings` present, compression off, autoDetectRng ON (W4.1a),
        // and answer-confirmation off (interval 5). `cheap_llm_settings_present` is
        // therefore true for EVERY call (it gates memory extraction — which v4
        // fires for every turn — and the summary check, which additionally needs
        // interchange > 10, reached only by `summary_fold`). The per-call
        // `cheap_llm_settings` corpus flag is documentary (which case exercises a
        // fold); the settings row itself is identical across calls.
        let _ = self.cheap_llm_settings;
        Some(OrchestratorChatSettings {
            cheap_llm_settings_present: true,
            compression_enabled: false,
            project_context_reinject_interval: 5,
            // autoDetectRng is a per-USER setting (the fixture's single row); flipped
            // true in W4.1a. Existing corpus content carries no RNG patterns, so it
            // fires only for the three rng_* cases.
            auto_detect_rng: true,
            answer_confirmation_global_enabled: false,
            autonomous_destructive_policy: "opt_in_per_room".to_string(),
            // Agent mode (W4.4): the fixture's single chat_settings row sets
            // `agentModeSettings = { maxTurns: 15, defaultEnabled: false }` (a
            // NON-default maxTurns so the `agent_mode_on` case banks custom-maxTurns
            // propagation into the injected instruction). `defaultEnabled` stays
            // false, so every non-opted-in chat resolves agent mode OFF; the
            // `agent_mode_on` chat opts in at the Chat level (`agentModeEnabled`).
            agent_mode_default_enabled: false,
            agent_mode_max_turns: 15,
            // W4.2u: the fixture's single chat_settings row sets
            // `dangerousContentSettings = { mode: AUTO_ROUTE }` (no
            // `uncensoredTextProfileId`, so the empty-response uncensored failover
            // stays inert — only the FIRST-branch reroute on an actively-dangerous
            // chat fires; every salon chat is not dangerous → no-op). The resolver
            // reads this global + the chat's `conciergeOverride`/`chatType`.
            danger_settings: Some(quilltap_core::db::chat_settings::DangerousContentSettings {
                mode: "AUTO_ROUTE".to_string(),
                threshold: 0.7,
                scan_text_chat: true,
                scan_image_prompts: true,
                scan_image_generation: false,
                uncensored_text_profile_id: None,
                uncensored_image_profile_id: None,
                display_mode: "SHOW".to_string(),
                show_warning_badges: true,
                custom_classification_prompt: None,
            }),
            // Round-3 Group 8: the fixture's `cheapLLMSettings =
            // { strategy: PROVIDER_CHEAPEST, fallbackToLocal: false }`. The spine
            // resolves the cheap-LLM selection from this + the connection profiles,
            // and threads it into buildContext (activating the recap/distill feeders).
            cheap_llm_strategy: "PROVIDER_CHEAPEST".to_string(),
            cheap_llm_user_defined_profile_id: None,
            cheap_llm_default_cheap_profile_id: None,
            cheap_llm_fallback_to_local: false,
        })
    }
}
