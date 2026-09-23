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
//! **TZ=UTC is REQUIRED since P4.d26**: the distill TODAY line renders in the
//! SERVER-LOCAL zone, so this oracle is TZ-sensitive (the harness pins
//! `server_tz` to "UTC"). The pin below is load-bearing, not decoration.
//!
//! **P4.90** added `failover_then_native_tool_call`: the seat's profile
//! (`FailoverPrimary`, ANTHROPIC `claude-falls-over`) throws a hard provider
//! error, `attemptHardErrorFailover` hands the turn to `FailoverUnderstudy`
//! (OPENAI `gpt-stands-in`), and the understudy's reply carries a native tool
//! call — so the tool loop's re-stream is the first call in this corpus that
//! happens AFTER a cross-provider recovery. The canned key is
//! `provider|model|temperature|messages`, so that one row is the only comparand
//! in the tree that can see which profile the re-stream was keyed to: before the
//! fix the Rust side asked for `(OPENAI, claude-falls-over)` — the understudy's
//! provider with the primary's model — and had no canned answer.
//!
//! **P4.D199** (v4 `bcd7e4852`, bug 151) adds the five `lantern_budget_*` arms:
//! a seat on the `LanternVision` profile (ANTHROPIC with `supportsImageUpload`,
//! so `image/webp` keeps its RAW bytes) is handed unseen Lantern announcements
//! carrying `image/webp` files sized in base64 against the 2 MiB per-turn
//! budget, and the recorded attachment slate below is what says which images
//! reached the wire and in what order. The image bytes come from the new
//! `fsmBytesFill` spec key (a run of one byte — an `fsmBytes` array cannot carry
//! a megabyte), and they are JUNK, which is what makes this family independent
//! of the loader half: v5's `NotConfiguredTranscoder` and v4's real `sharp` both
//! fail to shrink junk and pass the stored bytes through, MEASURED by the
//! recorded base64 lengths (307,200 in, 307,200 on the wire).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-orch-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
//!   cp "$V5W/harness/oracle/cases/orchestrator-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/lib/pinned-draws.ts" "$TMPO/lib/"
//!   cp "$V5W/harness/oracle/fixtures/orchestrator-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-orch-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-orch-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-orchestrator-fixture.ts
//!   QT_FIXTURE_ORCH_MAIN=/tmp/qt-orch-main.db QT_FIXTURE_ORCH_MOUNT=/tmp/qt-orch-mount.db \
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-orchestrator.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- orchestrator-tier3
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
use quilltap_core::weighted_random::DrawSource;
use serde::Deserialize;
use serde_json::{json, Value};

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
    /// P4.d2: the Nudge flag (b90cd1f5) — a summoned turn withholds the
    /// "nothing to add" offer.
    #[serde(default)]
    nudge: Option<bool>,
    /// P4.6c: user-initiated tool results pre-inserted as TOOL messages before
    /// the user message (orchestrator.service.ts:601–624).
    #[serde(default)]
    pending_tool_results: Vec<PtrW>,
    /// P4.D186 (v4 bug 137): attachment ids on the send. A held post still
    /// persists them (the seam sits below the attachment work), so the corpus
    /// needed a way to hang one on a call.
    #[serde(default)]
    file_ids: Vec<String>,
    /// P4.D186 (v4 bug 137): `options.neverPauseForUser` — the autonomous-room
    /// opt-out the hold predicate consults.
    #[serde(default)]
    never_pause_for_user: bool,
    /// P4.87: this case's ordered `Math.random()` pin, mirroring the oracle's
    /// per-case `pinned.reset(call.draws ?? [0])`. Absent → `[0]`, which is the
    /// frozen zero every pre-rotation row was written against.
    #[serde(default)]
    draws: Option<Vec<f64>>,
}

/// A corpus `pendingToolResults` element.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PtrW {
    tool: String,
    success: bool,
    result: String,
    prompt: String,
    arguments: serde_json::Map<String, serde_json::Value>,
    created_at: String,
}

/// P4.D154 (bug 121): a corpus `files` row. The fixture builder seeds the DB
/// row; the BYTES live here and back the canned host byte layer on both sides
/// (the oracle mocks `fileStorageManager.downloadFile` from the same table).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileSpecW {
    id: String,
    #[serde(default)]
    fsm_bytes: Vec<u8>,
    #[serde(default)]
    fsm_bytes_utf8: Option<String>,
    /// P4.D199 (bug 151): `len` copies of `byte`, expanded identically by the
    /// oracle's `fileStorageManager.downloadFile` mock. The Lantern byte budget
    /// is 2 MiB of base64, so its arms need megabyte-scale files and a literal
    /// `fsmBytes` array cannot carry one.
    #[serde(default)]
    fsm_bytes_fill: Option<FsmFillW>,
}

/// P4.D199: the `fsmBytesFill` run-length spec.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsmFillW {
    byte: u8,
    len: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    frozen_now_ms: i64,
    #[serde(default)]
    local_offset_minutes: i64,
    #[serde(default)]
    files: Vec<FileSpecW>,
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
    /// chunk.
    ///
    /// The two sides read it DIFFERENTLY and the corpus row must satisfy both:
    /// the oracle's `detectToolCallsInResponse` is mocked to key on
    /// `rawResponse.marker` (→ `spec.detection[marker]`), while the Rust spine
    /// runs the REAL `RegistryToolCallDetector` over the provider's own wire
    /// shape. P4.90's `failover_then_native_tool_call` row therefore carries
    /// BOTH a `marker` and a genuine OPENAI `tool_calls` array that parse to the
    /// same call.
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
    /// P4.D154 (bug 121): the per-message attachment slate at the wire,
    /// positionally aligned with `messages`. v4 stamps the merged slate onto the
    /// anchor message only, so this is the only comparand that can see
    /// `mergedAttachmentsToSend` — the call key projects role+content alone.
    #[serde(default)]
    attachments: Value,
    /// P4.D79: the whole `modelParams` bag v4 passed to `streamMessage` for this
    /// call key (v4 `profileParams(effectiveProfile) ?? {}`). Recorded because
    /// the key only carries the temperature — v5 was passing NO profile
    /// parameters at all, and no assertion could see it.
    #[serde(rename = "modelParams", default)]
    model_params: Value,
    /// P4.D83: the three sampling knobs v4's REAL `resolveSamplingParams`
    /// derived from that bag inside `streamMessage` — the function this family
    /// mocks, which is where v4 resolves them. Keys `JSON.stringify` dropped are
    /// the knobs v4 left undefined. The corpus's Primary profile deliberately
    /// mixes spellings (`max_tokens` snake, `topP` camel) so both arms of the
    /// resolver are measured here, not just asserted at tier 1.
    #[serde(default)]
    sampling: Value,
    /// P4.92: the OpenAI Responses-API chaining token v4 passed for this call
    /// key (`null` where v4 passed none). v4's ONE `streamMessage` funnel takes
    /// it as an OPTION, and only the PRIMARY call site supplies it — the native
    /// re-stream, the native force-final and the text continuation all omit it,
    /// with `provider-failover.service.ts:464` spelling out why ("handing it to
    /// a different account — never mind a different provider — is meaningless at
    /// best"). v5 hands the loops the primary's whole `StreamParams`, so it rode
    /// in; the call key could never see it.
    #[serde(rename = "previousResponseId", default)]
    previous_response_id: Option<String>,
    /// P4.92: the provider stop sequences v4 passed for this call key (`[]` where
    /// v4 passed none). The primary passes `initialStopSequences`
    /// (simple-json's `</tool_call>` pair, else undefined) and the TEXT
    /// continuation passes `strategy.stopSequences`; the native loop's two
    /// re-streams pass NEITHER. Since v4 runs `runNativeToolLoop`
    /// unconditionally — even under simple-json, where `actualTools` is `[]` —
    /// a simple-json seat whose reply carries a native call reaches the native
    /// re-stream, and that is where the carry-over is observable.
    #[serde(default)]
    stop: Vec<String>,
    /// P4.95: the per-character prompt-cache key v4 derived for this call key
    /// (`null` where the call site passed no `characterId`). v4's funnel takes a
    /// `characterId` and runs `buildCharacterCacheKey` on it itself
    /// (`streaming.service.ts:392`), so "which legs cache" is really "which legs
    /// pass a characterId" — and the mock calls v4's REAL derivation, because it
    /// stands IN PLACE OF the funnel and the funnel's own body never runs.
    /// Measured at `1fefadb9a`: the primary and the native loop's FIRST
    /// re-stream pass one; the native force-final, the text continuation and the
    /// primary's tool-unsupported retry do not.
    #[serde(rename = "cacheKey", default)]
    cache_key: Option<String>,
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
            // P4.90 found this mapper filing the oracle's `tool` role as `user`
            // (the canned key renders the role, so a tool-loop re-stream's
            // expected key could never match v5's); P4.93 gave `CompletionRole`
            // its one inverse and repointed every hand-rolled mapper onto it —
            // this file last, at the `1fefadb9a` round's unification (§R.10 (b)),
            // because P4.92 owned it that round. The `User` default is the
            // conscious choice `from_v4_wire`'s doc asks for: this corpus can
            // carry only the four wire spellings.
            role: CompletionRole::from_v4_wire(m.role.as_str()).unwrap_or(CompletionRole::User),
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
    /// The `modelParams` bag the ORACLE recorded reaching the wire, per call key
    /// (P4.D79).
    expected_model_params: HashMap<String, Value>,
    /// The tool slate the RUST side actually passed, per call key — recorded here
    /// as each stream fires, then diffed against `expected_tools` after the run.
    recorded_tools: Mutex<HashMap<String, Value>>,
    /// The `profileParameters` bag the RUST side actually passed, per call key.
    recorded_model_params: Mutex<HashMap<String, Value>>,
    /// P4.D83: the sampling knobs the ORACLE recorded / the RUST side passed.
    expected_sampling: HashMap<String, Value>,
    recorded_sampling: Mutex<HashMap<String, Value>>,
    /// P4.D154 (bug 121): the per-message attachment slate the ORACLE recorded
    /// at the wire, per call key.
    expected_attachments: HashMap<String, Value>,
    recorded_attachments: Mutex<HashMap<String, Value>>,
    /// P4.92: the chaining token / stop sequences the ORACLE recorded at the
    /// wire, per call key, and what the RUST side actually passed. Neither is in
    /// the key (the key is `provider|model|temperature|messages`), so these are
    /// the only comparands that can see either field.
    expected_previous_response_id: HashMap<String, Option<String>>,
    recorded_previous_response_id: Mutex<HashMap<String, Option<String>>>,
    expected_stop: HashMap<String, Vec<String>>,
    recorded_stop: Mutex<HashMap<String, Vec<String>>>,
    /// P4.95: the prompt-cache key the ORACLE recorded at the wire, per call
    /// key, and what the RUST side actually passed. Like the two above, it is
    /// NOT in the key, so this is the only comparand that can see it.
    expected_cache_key: HashMap<String, Option<String>>,
    recorded_cache_key: Mutex<HashMap<String, Option<String>>>,
}

/// The three knobs as `{temperature?, maxTokens?, topP?}` with absent knobs
/// OMITTED — the shape `JSON.stringify` gives v4's `SamplingParams`.
fn sampling_object(temperature: Option<f64>, max_tokens: Option<i64>, top_p: Option<f64>) -> Value {
    let mut o = serde_json::Map::new();
    if let Some(t) = temperature {
        o.insert("temperature".into(), json!(t));
    }
    if let Some(m) = max_tokens {
        o.insert("maxTokens".into(), json!(m));
    }
    if let Some(p) = top_p {
        o.insert("topP".into(), json!(p));
    }
    Value::Object(o)
}
impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedStreamW]) -> Self {
        let mut queues: HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>> =
            HashMap::new();
        let mut expected_tools: HashMap<String, Value> = HashMap::new();
        let mut expected_model_params: HashMap<String, Value> = HashMap::new();
        let mut expected_sampling: HashMap<String, Value> = HashMap::new();
        let mut expected_attachments: HashMap<String, Value> = HashMap::new();
        let mut expected_previous_response_id: HashMap<String, Option<String>> = HashMap::new();
        let mut expected_stop: HashMap<String, Vec<String>> = HashMap::new();
        let mut expected_cache_key: HashMap<String, Option<String>> = HashMap::new();
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
            expected_tools.insert(key.clone(), tools);
            // A missing/null recorded bag normalizes to `{}` (v4's `?? {}`).
            let mp = if row.model_params.is_object() || row.model_params.is_array() {
                row.model_params.clone()
            } else {
                Value::Object(serde_json::Map::new())
            };
            expected_model_params.insert(key.clone(), mp);
            let sampling = if row.sampling.is_object() {
                row.sampling.clone()
            } else {
                Value::Object(serde_json::Map::new())
            };
            expected_sampling.insert(key.clone(), sampling);
            // A missing/null recorded slate normalizes to `[]` (an oracle from
            // before this comparand existed); the floor below catches that case
            // loudly rather than letting it pass as "no attachments anywhere".
            let atts = if row.attachments.is_array() {
                row.attachments.clone()
            } else {
                Value::Array(Vec::new())
            };
            expected_attachments.insert(key.clone(), atts);
            expected_previous_response_id.insert(key.clone(), row.previous_response_id.clone());
            expected_stop.insert(key.clone(), row.stop.clone());
            expected_cache_key.insert(key, row.cache_key.clone());
        }
        Self {
            queues: Mutex::new(queues),
            expected_tools,
            expected_model_params,
            recorded_tools: Mutex::new(HashMap::new()),
            recorded_model_params: Mutex::new(HashMap::new()),
            expected_sampling,
            recorded_sampling: Mutex::new(HashMap::new()),
            expected_attachments,
            recorded_attachments: Mutex::new(HashMap::new()),
            expected_previous_response_id,
            recorded_previous_response_id: Mutex::new(HashMap::new()),
            expected_stop,
            recorded_stop: Mutex::new(HashMap::new()),
            expected_cache_key,
            recorded_cache_key: Mutex::new(HashMap::new()),
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
            let mp = params
                .profile_parameters
                .clone()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
            self.recorded_model_params
                .lock()
                .unwrap()
                .entry(key.clone())
                .or_insert(mp);
            self.recorded_sampling
                .lock()
                .unwrap()
                .entry(key.clone())
                .or_insert_with(|| {
                    sampling_object(params.temperature, params.max_tokens, params.top_p)
                });
            // P4.D154 (bug 121): the per-message attachment slate, positionally
            // aligned with `params.messages` — only `StreamMessage::User` can
            // carry one, so every other role contributes `[]`.
            self.recorded_attachments
                .lock()
                .unwrap()
                .entry(key.clone())
                .or_insert_with(|| {
                    Value::Array(
                        params
                            .messages
                            .iter()
                            .map(|m| match m {
                                quilltap_core::model::stream::StreamMessage::User {
                                    attachments,
                                    ..
                                } => Value::Array(attachments.clone()),
                                _ => Value::Array(Vec::new()),
                            })
                            .collect(),
                    )
                });
            // P4.92: the two primary-only options. Recorded per key, never in
            // it — the same side-channel treatment `tools` / `modelParams` /
            // `sampling` / `attachments` get above.
            self.recorded_previous_response_id
                .lock()
                .unwrap()
                .entry(key.clone())
                .or_insert_with(|| params.previous_response_id.clone());
            self.recorded_stop
                .lock()
                .unwrap()
                .entry(key.clone())
                .or_insert_with(|| params.stop.clone());
            // P4.95: the third primary-only-ish option. v5 already carries the
            // DERIVED key in `StreamParams.cache_key` (v4 derives inside the
            // funnel from `characterId`), so the two comparands meet on the
            // string that reaches the provider builder.
            self.recorded_cache_key
                .lock()
                .unwrap()
                .entry(key.clone())
                .or_insert_with(|| params.cache_key.clone());
        }
        let sequence: Vec<StreamChunkResult> = {
            let mut queues = self.queues.lock().unwrap();
            match queues.get_mut(&key).and_then(|q| q.pop_front()) {
                Some(seq) => seq,
                // P4.90: name the roles + a content prefix of every message on a
                // miss. The counts alone said "5 msgs" for both halves of a
                // two-step diagnosis (first the model was wrong, then a message
                // body was), and neither told which.
                None => vec![Err(StreamError::new(format!(
                    "no canned stream queued for key ({provider}, model {}, temperature {:?}, \
                     {} msgs)\n{}",
                    params.model,
                    params.temperature,
                    params.messages.len(),
                    params
                        .messages
                        .iter()
                        .map(|m| {
                            let c = m.content();
                            let head: String = c.chars().take(120).collect();
                            format!(
                                "    {:>9} | {} chars | {head:?}",
                                m.role_str(),
                                c.chars().count()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
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
            // §3 of the 93ed8abf round: v5 now emits v4's pre-send
            // `validating` (unconditional, streaming path) and `warning`
            // (overage) statuses, so both joined the shared vocabulary and are
            // sequence-compared like the rest.
            | "validating"
            | "warning"
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
                // P4.d2: the `hostAnnouncement` frame (the turn-pass note) carries
                // the full posted MessageEvent — a fresh minted `id` + `createdAt`
                // on each side. Placeholder both; the rest (content / opaqueContent
                // / systemSender / systemKind / hostEvent.participantId) is
                // deterministic.
                if let Some(msg) = obj
                    .get_mut("hostAnnouncement")
                    .and_then(Value::as_object_mut)
                {
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
// The canned host byte layer (P4.D154, bug 121).
// ---------------------------------------------------------------------------

/// fileId → the spec's bytes, mirroring the oracle's
/// `fileStorageManager.downloadFile` mock. A file id the spec does not carry
/// errors exactly as a storage miss does — `load_chat_files_for_llm` logs and
/// skips it, as v4's own catch does.
struct CannedBytes {
    by_file_id: HashMap<String, Vec<u8>>,
}
impl quilltap_core::services::chat_files::FileBytesStore for CannedBytes {
    fn download_file(
        &self,
        entry: &quilltap_core::db::files::FileEntry,
    ) -> Result<Vec<u8>, String> {
        match self.by_file_id.get(&entry.id) {
            Some(b) => Ok(b.clone()),
            None => Err(format!("no canned bytes for fileId {}", entry.id)),
        }
    }
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
                // P4.92 item 8: EVERY row must carry both new keys. `serde`'s
                // `default` makes a missing key parse as "v4 passed none", which
                // is also the common real answer — so a regen from a stale
                // oracle case would drop the recording and leave every
                // comparison reading `None == None` / `[] == []`. Presence is
                // checked on the raw object, before the typed parse erases the
                // difference.
                for k in ["previousResponseId", "stop"] {
                    assert!(
                        parsed.rest.get(k).is_some(),
                        "oracle cannedStream row is missing `{k}`: regenerate the oracle from THIS tree's case file (P4.92 widened the streamMessage mock's recording)"
                    );
                }
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
    // P4.81 item 4: the three `execute_turn_chain` chain-stop log lines,
    // captured per case (thread-scoped — `rt` is current-thread, so the whole
    // `block_on` below runs on this thread and cannot steal a sibling test's
    // subscriber). `multi_chain`'s chained turn scripts an empty second
    // stream (`"content": ""`) and `chain_error_pause`'s scripts a stream
    // error — the ONLY two corpus cases that reach the chain loop's two
    // stop-with-a-line branches; every other case must stay silent on both.
    let mut chain_logs: HashMap<String, Vec<String>> = HashMap::new();
    // P4.D186: the INITIAL `process_message`'s tracing output, per case.
    let mut initial_logs: HashMap<String, Vec<String>> = HashMap::new();

    // P4.D154 (bug 121): the host byte layer the re-hydration reads through.
    //
    // P4.D199 (bug 151) adds `image/webp` rows for the Lantern byte budget, so
    // image-shaped bytes DO reach the loaders now — but they are junk (a run of
    // one byte, `fsmBytesFill`), and junk is what makes the two sides agree
    // without a codec: v5's `NotConfiguredTranscoder` answers `Err` to every
    // transcode, and v4's real `sharp` throws on a buffer that is not an image
    // (its `Could not shrink image for LLM transport; sending stored bytes` warn
    // in the oracle's stderr is the proof it ran), so BOTH sides put the stored
    // bytes on the wire. Every row's base64 also sits under the ANTHROPIC
    // 5 MiB per-image ceiling, so neither side's provider backstop resize fires
    // either. That is what makes this family independent of P4.D198's loader
    // half; the unifier re-runs it over the union.
    let canned_bytes = CannedBytes {
        by_file_id: spec
            .files
            .iter()
            .map(|f| {
                let bytes = match (&f.fsm_bytes_fill, &f.fsm_bytes_utf8) {
                    (Some(fill), _) => vec![fill.byte; fill.len],
                    (None, Some(t)) => t.as_bytes().to_vec(),
                    (None, None) => f.fsm_bytes.clone(),
                };
                (f.id.clone(), bytes)
            })
            .collect(),
    };

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
            // The attachment subsystem (W4.4b). The corpus keeps request `fileIds`
            // empty, so `load_and_process_files` still early-returns; but since
            // P4.D154 a planted USER message DOES carry attachments, so the
            // bug-121 re-hydration seam reads real `files` rows out of the fixture
            // through the canned byte store above (the oracle mocks the same layer
            // and runs v4's REAL loader + fallback over it).
            file_bytes: &canned_bytes,
            image_transcoder: &quilltap_core::files::image_processing::NotConfiguredTranscoder,
            danger_router: &router,
            confirmation: &mut confirmation,
            compression: &mut compression,
            cost: &mut cost,
            carina_query: &mut carina_query,
            prospero: &mut prospero,
            rng_bytes: &mut rng_bytes,
            web_search: None,
            image_describe: None,
            photo_bytes: None,
        };
        // The spine now constructs the real registry-backed tool detector +
        // provider-text strategy internally (W4.7c). Exactly ONE corpus case
        // carries a native tool call — P4.90's `failover_then_native_tool_call`,
        // whose understudy answers with an OPENAI `tool_calls` array; every other
        // case's `raw_response` has no tool blocks and no provider text markers,
        // so both passes no-op after the real (now provider-reshaped) slate
        // reaches the wire — proven by the tools-at-wire assertion. The loop's
        // own internals stay proven by `native_tool_loop_tier3` (v4's REAL
        // `runNativeToolLoop` + threading); what THIS family adds is the one
        // thing that family cannot see — which profile the re-stream is keyed to
        // after the spine has failed over underneath it.

        // P4.87: ONE draw source per case, cloned into every carrier this case
        // reaches — the initial `ProcessClock`, each chained turn's, and
        // `execute_turn_chain`'s own. Clones share the cursor (the `Arc`'d
        // closure holds it), so the draws arrive in one order exactly as v4's
        // single pinned `Math.random` delivers them across `handleSendMessage`.
        // Building a fresh sequence per carrier restarted at index 0, which was
        // invisible only while the array had one element.
        let case_draws = DrawSource::sequence(call.draws.clone().unwrap_or_else(|| vec![0.0]));

        let call_nudge = call.nudge;
        // P4.D186: the send's attachments and the autonomous-room opt-out.
        let call_file_ids = call.file_ids.clone();
        let call_never_pause = call.never_pause_for_user;
        let call_ptrs: Vec<orchestrator::PendingToolResult> = call
            .pending_tool_results
            .iter()
            .map(|p| orchestrator::PendingToolResult {
                tool: p.tool.clone(),
                success: p.success,
                result: p.result.clone(),
                prompt: p.prompt.clone(),
                arguments: p.arguments.clone(),
                created_at: p.created_at.clone(),
            })
            .collect();
        let make_input = |chat_id: &str, content: &str, continue_mode: bool, resp: Option<&str>| {
            ProcessMessageInput {
                // U4.4: the request-path default (none) — inert on this corpus.
                log_context: LogContext::none(),
                chat_id: chat_id.to_string(),
                user_id: spec.user_id.clone(),
                options: SendMessageOptions {
                    continue_mode,
                    content: content.to_string(),
                    responding_participant_id: resp.map(String::from),
                    // P4.d2: the Nudge flag (summoned → the skip offer is withheld).
                    nudge: call_nudge,
                    // P4.6c: pending tool results pre-inserted before the user msg
                    // (only on the INITIAL non-continue turn — the chain passes none).
                    pending_tool_results: if continue_mode {
                        Vec::new()
                    } else {
                        call_ptrs.clone()
                    },
                    // P4.D186: attachments ride only the INITIAL non-continue
                    // turn, as v4's chained `processMessage` passes none.
                    file_ids: if continue_mode {
                        Vec::new()
                    } else {
                        call_file_ids.clone()
                    },
                    // P4.D186: v4's `handleSendMessage` forwards
                    // `neverPauseForUser` to EVERY chained turn too
                    // (`orchestrator.service.ts:218`), so this one is not gated
                    // on `continue_mode`.
                    never_pause_for_user: call_never_pause,
                    ..Default::default()
                },
                clock: ProcessClock {
                    now_ms: spec.frozen_now_ms,
                    local_offset_minutes: spec.local_offset_minutes,
                    // P4.87: the case's own sequence, shared with the chain.
                    random01: case_draws.clone(),
                },
                model_context_limit: 200_000,
                timestamp_config: None,
                timezone: Some("UTC".to_string()),
                server_tz: Some("UTC".to_string()),
                provider_supports_web_search: false,
            }
        };

        // Initial processMessage. P4.D186: captured, so the paused-hold info line
        // (a log-only change every frame and row diff is blind to) is pinned per
        // case — on its own branch AND silent on every sibling.
        let initial = {
            let mut out = None;
            let initial_lines = quilltap_core::test_support::captured(|| {
                out = Some(rt.block_on(orchestrator::process_message(
                    &mut deps,
                    &make_input(
                        &call.chat_id,
                        &call.content,
                        call.continue_mode,
                        call.responding_participant.as_deref(),
                    ),
                )));
            });
            initial_logs.insert(call.name.clone(), initial_lines);
            out.expect("process_message ran")
        };

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
                    let chain_draws = case_draws.clone();
                    let chain_never_pause = call_never_pause;
                    let make_chain_input = move |pid: String| ProcessMessageInput {
                        log_context: LogContext::none(),
                        chat_id: chat_id.clone(),
                        user_id: user_id.clone(),
                        options: SendMessageOptions {
                            continue_mode: true,
                            content: String::new(),
                            responding_participant_id: Some(pid),
                            // P4.D186: v4 spreads the autonomous-room flags into
                            // every chained turn (`orchestrator.service.ts:216–222`).
                            never_pause_for_user: chain_never_pause,
                            ..Default::default()
                        },
                        clock: ProcessClock {
                            now_ms: frozen,
                            local_offset_minutes: offset,
                            // P4.87: the case's own sequence, shared with the
                            // initial turn and the chain's own carrier.
                            random01: chain_draws.clone(),
                        },
                        model_context_limit: 200_000,
                        timestamp_config: None,
                        timezone: Some("UTC".to_string()),
                        server_tz: Some("UTC".to_string()),
                        provider_supports_web_search: false,
                    };
                    // P4.6BM: v4 gates its post-cycle render trigger on the
                    // INITIAL result's `hasContent`, which the chain options
                    // take by value below.
                    let initial_had_content = result.has_content;
                    let lines = quilltap_core::test_support::captured(|| {
                        rt.block_on(orchestrator::execute_turn_chain(
                            &mut deps,
                            ExecuteTurnChainOptions {
                                chat_id: call.chat_id.clone(),
                                user_id: spec.user_id.clone(),
                                initial_result: result,
                                initial_continue_mode: call.continue_mode,
                                // P4.D186: v4's `handleSendMessage` passes
                                // `options.neverPauseForUser === true` straight
                                // into `executeTurnChain` (`:226`).
                                never_pause_for_user: call_never_pause,
                                single_turn: false,
                                chain_start_time_ms: frozen,
                                config: ChainConfig::default(),
                            },
                            frozen,
                            // P4.87: the case's own sequence, shared with both
                            // input carriers above.
                            &case_draws,
                            make_chain_input,
                        ))
                        .expect("chain");
                    });
                    chain_logs.insert(call.name.clone(), lines);
                    // P4.6BM: v4's post-cycle Scriptorium trigger, at v4's own
                    // placement (`orchestrator.service.ts:236-247` — after the
                    // chain, "every turn with content"). The production analog
                    // is in `quilltap-host`'s spine; this composition mirrors
                    // `handleSendMessage`, so the trigger belongs here too.
                    if initial_had_content {
                        rt.block_on(
                            quilltap_core::services::conversation_render_job::trigger_conversation_render(
                                &db,
                                &spec.user_id,
                                &call.chat_id,
                            ),
                        );
                    }
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

    // --- P4.90: the failover-then-tool-call arm must have FAILED OVER on v4 ---
    // The frame + table diffs compare `failover_then_native_tool_call` end to
    // end, and the pre-fix v5 reddens on the truncated trace (the loop's
    // re-stream keyed to the primary's model has no canned answer). What that
    // cannot say is that the ORACLE still measures a failover: if a future regen
    // quietly stopped failing over (the mock's `502` reclassified, say), both
    // sides would re-stream on the primary and the case would go green having
    // measured nothing. So the arm is pinned against v4's own recorded calls: at
    // least one canned stream keyed to the UNDERSTUDY (`OPENAI` /
    // `gpt-stands-in`) whose messages carry a `tool` role — the re-stream after
    // the tool call, on the provider the chain fell over to. (The `2075242f9`
    // round's §3 review.)
    assert!(
        canned_streams.iter().any(|s| s.provider == "OPENAI"
            && s.model == "gpt-stands-in"
            && s.messages.iter().any(|m| m.role == "tool")),
        "failover_then_native_tool_call: v4 recorded no understudy-keyed (OPENAI / \
         gpt-stands-in) re-stream carrying a `tool` message — the oracle no longer \
         fails over before the tool loop, so the arm measures nothing"
    );

    // --- P4.92: the two carry-over cases must have RE-STREAMED on v4 ---
    // The per-call arms compare v5's `previous_response_id` / `stop` against
    // the recorded row at every canned lookup, and the stale-oracle floors
    // guard the PRIMARY rows (a non-null id, a non-empty stop). What neither
    // can say is that the RE-STREAM happened: if detection silently stopped on
    // both sides (a marker rename, say), each case would record one primary
    // row, the primary floors would still pass, and the strip's arms would
    // compare nothing. So each case is pinned against v4's own recorded calls
    // in the P4.90 idiom above — a re-stream row (one carrying a `tool`
    // message) on the case's seat, showing the field v4 DROPPED. (The
    // `1fefadb9a` round's §3 review.)
    assert!(
        canned_streams.iter().any(|s| s.provider == "OPENAI"
            && s.model == "gpt-chains"
            && s.previous_response_id.is_none()
            && s.messages.iter().any(|m| m.role == "tool")),
        "openai_chained_then_native_tool_call: v4 recorded no re-stream (OPENAI / \
         gpt-chains, carrying a `tool` message) WITHOUT a previousResponseId — the \
         case no longer reaches the native loop, so the previous_response_id strip \
         measures nothing"
    );
    assert!(
        canned_streams.iter().any(|s| s.provider == "OPENAI"
            && s.model == "o1-mini"
            && s.stop.is_empty()
            && s.messages.iter().any(|m| m.role == "tool")),
        "simple_json_then_native_tool_call: v4 recorded no re-stream (OPENAI / \
         o1-mini, carrying a `tool` message) with EMPTY stop sequences — the case \
         no longer reaches the native loop, so the stop strip measures nothing"
    );

    // --- P4.D186 (v4 `31436bae4` bug 137): the held user turn ---
    // The frame + table diffs above already compare these cases end to end. What
    // they cannot do alone is prove the case is MEANINGFUL: if both sides failed
    // the same way (v5 with no hold would ask the canned provider for a key the
    // corpus deliberately does not carry, and the oracle's `streamMessage` mock
    // throws `no streams for <label>` for the same reason), a mutual failure
    // would compare equal. So each held case is pinned against the ORACLE's own
    // recorded frames: v4 must have completed the call and emitted exactly the
    // six-key held frame, and must NOT have streamed a reply.
    //
    // Measured on v4's own code at both pins: at the baseline (`f4ad2c8d1`, before
    // the fix) every one of these seven cases runs a model turn, `ed000005`'s
    // `requestFullContextOnNextMessage` is reset 1 → 0, `ed000006` gains a
    // `group-context` Prospero whisper, and `ed000007`'s fair-rotation guard fires
    // (`lastTurnParticipantId` = the next seat, a `user_turn` frame). At the
    // target all four answers flip. That is what makes the three `!hold` conjuncts
    // non-vacuous rather than three guards whose siblings happen to be false.
    let held_cases = [
        "paused_hold_basic",
        "paused_hold_attachment",
        "paused_hold_tool_result",
        "paused_hold_rng",
        "paused_hold_keeps_full_context_flag",
        "paused_hold_cadence_boundary",
        "paused_hold_two_user_seats",
    ];
    let held_frame = json!({
        "chainComplete": true,
        "reason": "paused",
        "nextSpeakerId": null,
        "chainDepth": 0,
        "paused": true,
        "heldUserTurn": true
    });
    for name in held_cases {
        let want = want_events
            .get(name)
            .unwrap_or_else(|| panic!("oracle events missing for {name}"));
        let chain_frames: Vec<&Value> = want
            .iter()
            .filter(|e| e.get("chainComplete").is_some())
            .collect();
        assert_eq!(
            chain_frames.len(),
            1,
            "{name}: v4 must emit exactly one chainComplete on a held turn, got {chain_frames:?}"
        );
        assert_eq!(
            *chain_frames[0], held_frame,
            "{name}: v4's held frame is not the six-key shape"
        );
        for forbidden in ["content", "done", "turnComplete"] {
            assert!(
                !want.iter().any(|e| e.get(forbidden).is_some()),
                "{name}: v4 must not emit a `{forbidden}` frame on a held turn — the \
                 model was called, so this case is measuring the wrong thing"
            );
        }
    }
    // The two paused cases that RUN keep v4's older frame: `paused` with no
    // `heldUserTurn` key at all (the chain's own early return, P4.D160).
    for name in [
        "paused_continue_summons_runs",
        "paused_never_pause_for_user_runs",
    ] {
        let want = want_events
            .get(name)
            .unwrap_or_else(|| panic!("oracle events missing for {name}"));
        assert!(
            want.iter().any(|e| e.get("content").is_some()),
            "{name}: a summons into a paused room must still stream a reply"
        );
        let chain_frames: Vec<&Value> = want
            .iter()
            .filter(|e| e.get("chainComplete").is_some())
            .collect();
        assert_eq!(chain_frames.len(), 1, "{name}: one chainComplete expected");
        assert!(
            chain_frames[0].get("heldUserTurn").is_none(),
            "{name}: `heldUserTurn` must be ABSENT on a frame v4 did not hold: {:?}",
            chain_frames[0]
        );
    }

    // --- P4.D186: the ONE info line, capture-pinned ---
    // v4 `logger.info('[Orchestrator] Chat paused — recording the user message
    // without a reply', { chatId, userId, hasContent, attachmentCount })`. A
    // log-only line is invisible to every frame and row comparand above, and its
    // silence on the sibling branches is half the contract.
    for call in &spec.calls {
        let lines = initial_logs
            .get(&call.name)
            .unwrap_or_else(|| panic!("no captured initial log for {}", call.name));
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[Orchestrator] Chat paused"))
            .collect();
        if held_cases.contains(&call.name.as_str()) {
            assert_eq!(
                hits.len(),
                1,
                "{}: the paused-hold line must fire exactly once: {lines:?}",
                call.name
            );
            let want_line = format!(
                "INFO quilltap_core::services::orchestrator [Orchestrator] Chat paused — \
                 recording the user message without a reply chat_id={} user_id={} \
                 has_content={} attachment_count={}",
                call.chat_id,
                spec.user_id,
                !call.content.is_empty(),
                call.file_ids.len(),
            );
            assert_eq!(*hits[0], want_line, "{}: info-line shape", call.name);
        } else {
            assert!(
                hits.is_empty(),
                "{}: a turn that is not held must not log the paused-hold line: {lines:?}",
                call.name
            );
        }
    }

    // --- P4.81 item 4: the chain-stop log lines, pinned per case ---
    // v4 `turn-orchestrator.service.ts`'s `logger.info('[TurnOrchestrator] Chain
    // stopped: empty response', { chatId, chainDepth, userId })` and
    // `logger.error('[TurnOrchestrator] Chain error, stopping', { chatId,
    // chainDepth, userId, error })`. Only `multi_chain` (an empty second
    // stream) and `chain_error_pause` (a scripted stream error) reach these
    // branches; every other case's chain either never loops or stops silently
    // (`user_turn` / `max_depth` / `paused`, already pinned elsewhere).
    for (name, lines) in &chain_logs {
        let empty_response_hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[TurnOrchestrator] Chain stopped: empty response"))
            .collect();
        let chain_error_hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[TurnOrchestrator] Chain error, stopping"))
            .collect();
        match name.as_str() {
            "multi_chain" => {
                assert_eq!(
                    empty_response_hits.len(),
                    1,
                    "multi_chain's empty second stream must log the stop line \
                     exactly once: {lines:?}"
                );
                assert_eq!(
                    empty_response_hits[0],
                    "INFO quilltap_core::services::orchestrator [TurnOrchestrator] \
                     Chain stopped: empty response chat_id=c89ee722-b555-40db-9e02-bbd0e82ee56b \
                     chain_depth=1 user_id=e18e05bc-63e8-4539-8a85-719b7a508850",
                );
                assert!(
                    chain_error_hits.is_empty(),
                    "multi_chain must not also log the error line: {lines:?}"
                );
            }
            "chain_error_pause" => {
                assert_eq!(
                    chain_error_hits.len(),
                    1,
                    "chain_error_pause's scripted stream error must log the error \
                     line exactly once: {lines:?}"
                );
                assert!(
                    chain_error_hits[0].starts_with(
                        "ERROR quilltap_core::services::orchestrator [TurnOrchestrator] \
                         Chain error, stopping chat_id=9c8a0002-0000-4000-8000-0000000000e1 \
                         chain_depth=1 user_id=e18e05bc-63e8-4539-8a85-719b7a508850 error="
                    ),
                    "unexpected error-line shape for chain_error_pause: {:?}",
                    chain_error_hits.first()
                );
                assert!(
                    empty_response_hits.is_empty(),
                    "chain_error_pause must not also log the empty-response line: {lines:?}"
                );
            }
            _ => {
                assert!(
                    empty_response_hits.is_empty() && chain_error_hits.is_empty(),
                    "case {name} must not log either chain-stop line: {lines:?}"
                );
            }
        }
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

    // --- modelParams AT THE WIRE (P4.D79) ---
    // v4 builds `modelParams = profileParams(effectiveProfile) ?? {}` and
    // forwards it as `profileParameters`; the call key only carries its
    // temperature, so until this assertion existed the corpus could not tell
    // that v5 was passing nothing at all. The corpus's Primary profile now
    // carries a real parameters bag, which makes both halves measurable.
    {
        let recorded = streaming.recorded_model_params.lock().unwrap();
        for (key, got_mp) in recorded.iter() {
            let want_mp = streaming.expected_model_params.get(key).unwrap_or_else(|| {
                panic!(
                    "Rust made a stream call with no oracle-recorded modelParams for key:\n{key}"
                )
            });
            assert_eq!(
                got_mp, want_mp,
                "profileParameters at wire mismatch for key:\n{key}"
            );
        }
    }

    // --- the SAMPLING knobs AT THE WIRE (P4.D83, v4 `d89babc4`) ---
    // v4 resolves them inside `streamMessage` (mocked here, so the mock calls
    // v4's REAL `resolveSamplingParams`); v5's orchestrator resolves them before
    // its narrower provider seam. Both sides must reach the same three numbers.
    // Before this lane v5 read `modelParams.maxTokens` / `.topP`, spellings the
    // profile editor never writes — so Max Tokens and Top P were dropped on
    // every Salon turn.
    {
        let recorded = streaming.recorded_sampling.lock().unwrap();
        assert!(
            !recorded.is_empty(),
            "no stream call recorded — the sampling assertion would be vacuous"
        );
        for (key, got) in recorded.iter() {
            let want = streaming.expected_sampling.get(key).unwrap_or_else(|| {
                panic!("Rust made a stream call with no oracle-recorded sampling for key:\n{key}")
            });
            assert_eq!(got, want, "sampling at wire mismatch for key:\n{key}");
        }
        // The symmetric direction (§3 of the 93ed8abf round): every call the
        // oracle recorded must have been made here too.
        for key in streaming.expected_sampling.keys() {
            assert!(
                recorded.contains_key(key),
                "the oracle recorded a stream call v5 never made:\n{key}"
            );
        }
    }

    // --- the ATTACHMENT slate AT THE WIRE (P4.D154, v4 `e288ae2ec` / bug 121) ---
    // v4 stamps `mergedAttachmentsToSend` onto the anchor message only, and the
    // canned call key projects role + content alone — so this is the ONLY
    // comparand that can see the merge. Without it a v5 that dropped
    // `rehydrated_attachments_to_keep` (or kept an errored `unsupported` file)
    // would diff clean.
    {
        let recorded = streaming.recorded_attachments.lock().unwrap();
        for (key, got) in recorded.iter() {
            let want = streaming.expected_attachments.get(key).unwrap_or_else(|| {
                panic!(
                    "Rust made a stream call with no oracle-recorded attachments for key:\n{key}"
                )
            });
            assert_eq!(
                got, want,
                "attachment slate at wire mismatch for key:\n{key}"
            );
        }
        // Stale-oracle floor, now also the bug-151 budget's OUTCOME pin
        // (P4.D199): every call that reached the wire with attachments, named by
        // the ordered filenames it carried. The per-key equality above compares
        // v4 against v5; THIS says what the corpus is supposed to be exercising,
        // so an oracle regenerated from a pre-fix tree, a fixture that lost the
        // image rows, or a v5 that stopped budgeting cannot pass vacuously.
        //
        //   dossier.pdf                  — the bug-121 re-hydration row.
        //   lb_fit_a + lb_fit_b          — `lantern_budget_all_fit`: 0.6 MiB of
        //                                  base64 against a 2 MiB budget.
        //   lb_small + lb_new            — `lantern_budget_drops_oldest`: the
        //                                  1.5 MiB OLDEST portrait is the one
        //                                  dropped, and the survivors are
        //                                  CHRONOLOGICAL. An oldest-first spend
        //                                  would read `lb_big + lb_small`, and a
        //                                  missing restore `lb_new + lb_small`.
        //   lb_half1 + lb_half2          — `lantern_budget_exact_fit`: two halves
        //                                  summing to exactly 2,097,152, both
        //                                  kept (`>` not `>=`).
        //
        // `lantern_budget_single_over` (one image over the whole budget) and
        // `lantern_budget_nonvision_seat_unbudgeted` (every image described
        // instead of transported, so the budget never engages) deliberately
        // carry NOTHING and are absent from this list.
        let mut carried: Vec<Vec<String>> = recorded
            .values()
            .flat_map(|v| {
                v.as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|per_msg| {
                        let atts = per_msg.as_array()?;
                        if atts.is_empty() {
                            return None;
                        }
                        Some(
                            atts.iter()
                                .map(|a| {
                                    a.get("filename")
                                        .and_then(Value::as_str)
                                        .unwrap_or("<no filename>")
                                        .to_string()
                                })
                                .collect::<Vec<String>>(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        carried.sort();
        assert_eq!(
            carried,
            vec![
                vec!["dossier.pdf".to_string()],
                vec!["lb_fit_a.webp".to_string(), "lb_fit_b.webp".to_string()],
                vec!["lb_half1.webp".to_string(), "lb_half2.webp".to_string()],
                vec!["lb_small.webp".to_string(), "lb_new.webp".to_string()],
            ],
            "the attachment slates reaching the wire are not the corpus's; the \
             fixture or the oracle has gone stale (or the Lantern byte budget \
             moved)"
        );
    }

    // --- the CHAINING TOKEN and the STOP SEQUENCES at the wire (P4.92) ---
    // v4 has ONE `streamMessage` funnel (`streaming.service.ts:400-412`) whose
    // options each call site fills in by hand, and the four Salon-turn sites do
    // not agree:
    //
    //   option               | primary | native re-stream | force-final | text cont.
    //   previousResponseId   |  yes    |       no         |     no      |    no
    //   stop                 |  yes    |       no         |     no      |    yes
    //   characterId→cacheKey |  yes    |       yes        |     no      |    no
    //
    // The third row is P4.95's: v4's funnel does not TAKE a cacheKey, it takes a
    // `characterId` and derives one (`streaming.service.ts:392`), so the leg rule
    // is which sites pass the id — measured line by line at `1fefadb9a`
    // (`primary-stream.service.ts:207` yes, `native-tool-loop.service.ts:350`
    // yes, `:421-431` no, `text-tool-loop.service.ts:390-400` no). It is the one
    // option whose per-leg answer is NOT uniform across the loop clones, which
    // is why its clears live in the two loops rather than in `loop_base_params`.
    //
    // v5 hands the loops `base_params: params.clone()` — the primary's whole
    // `StreamParams` — so both fields rode into every re-stream. Neither is in
    // the canned call key (`provider|model|temperature|messages`), so the corpus
    // was structurally blind to the divergence until the oracle mock started
    // recording them alongside `tools` / `modelParams` / `sampling` /
    // `attachments`. Fixed caller-side at the three `base_params:` sites
    // (`orchestrator.rs`), and pinned here per call.
    {
        let recorded = streaming.recorded_previous_response_id.lock().unwrap();
        for (key, got) in recorded.iter() {
            let want = streaming
                .expected_previous_response_id
                .get(key)
                .unwrap_or_else(|| {
                    panic!(
                        "Rust made a stream call with no oracle-recorded previousResponseId for key:\n{key}"
                    )
                });
            assert_eq!(
                got, want,
                "previousResponseId at wire mismatch for key:\n{key}"
            );
        }
        let recorded_stop = streaming.recorded_stop.lock().unwrap();
        for (key, got) in recorded_stop.iter() {
            let want = streaming.expected_stop.get(key).unwrap_or_else(|| {
                panic!("Rust made a stream call with no oracle-recorded stop for key:\n{key}")
            });
            assert_eq!(got, want, "stop at wire mismatch for key:\n{key}");
        }
        // P4.95: the same per-call compare for the prompt-cache key.
        let recorded_cache_key = streaming.recorded_cache_key.lock().unwrap();
        for (key, got) in recorded_cache_key.iter() {
            let want = streaming.expected_cache_key.get(key).unwrap_or_else(|| {
                panic!("Rust made a stream call with no oracle-recorded cacheKey for key:\n{key}")
            });
            assert_eq!(got, want, "cacheKey at wire mismatch for key:\n{key}");
        }
        // Stale-oracle floors. Both fields default to the "v4 passed none"
        // value, so an oracle regenerated from a tree WITHOUT the recording
        // would make every arm above compare `None == None` / `[] == []` and
        // pass having measured nothing (`a-case-added-only-to-the-oracle-is-
        // never-run`). The corpus guarantees at least one of each:
        //   - `openai_chained_then_native_tool_call` seeds an assistant message
        //     carrying `rawResponse.id: "resp_prior"` on an OPENAI seat, so v4's
        //     `findPreviousResponseId` returns it on the PRIMARY call;
        //   - `textblock_mode` / `simple_json_then_native_tool_call` seat the
        //     OPENAI `o1-mini` profile, whose `supportsTools: false` in
        //     FALLBACK_PRICING resolves `simple-json` — so v4 passes
        //     `SIMPLE_JSON_STOP` on the primary.
        assert!(
            streaming
                .expected_previous_response_id
                .values()
                .any(|v| v.is_some()),
            "no oracle row recorded a previousResponseId: the oracle predates the P4.92 recording, or the chaining case stopped chaining; every previousResponseId arm above is vacuous"
        );
        assert!(
            streaming.expected_stop.values().any(|v| !v.is_empty()),
            "no oracle row recorded a non-empty stop: the oracle predates the P4.92 recording, or no case resolves simple-json any more; every stop arm above is vacuous"
        );
        // ...and the TEXT continuation's own `stop` override
        // (`text_tool_loop.rs`, v4 `text-tool-loop.service.ts:400`) needs its
        // own floor. Found by a mutation that SURVIVED: deleting that override
        // left the family green, because the only rows reaching the text
        // continuation were the ANTHROPIC ones, whose text-block strategy
        // returns NO stop sequences at all — so the arm compared `[] == []`.
        // The simple-json strategy is the only one with sequences to lose, and
        // `simple_json_text_tool_continuation` is the case that poses it.
        //
        // A continuation row is identified by the tool-result LEDGER ENTRY in
        // its slate, and the two strategies spell that differently: simple-json
        // writes `<tool_result name="…">` (v4 `formatSimpleJsonToolResult`) and
        // text-block writes `[Tool Result: …]`. Both are named so the floor
        // survives whichever strategy a future case uses.
        let text_continuations_with_stop = canned_streams
            .iter()
            .filter(|r| {
                !r.stop.is_empty()
                    && r.messages.iter().any(|m| {
                        m.content.contains("<tool_result name=")
                            || m.content.contains("[Tool Result: ")
                    })
            })
            .count();
        assert!(
            text_continuations_with_stop > 0,
            "no oracle row is a text continuation carrying stop sequences: text_tool_loop's `params.stop = strategy.stop_sequences()` is untested, and a mutation deleting it would survive"
        );

        // --- P4.95's floors, the same idiom ---
        //
        // `cacheKey` defaults to `None` on BOTH sides, so an oracle regenerated
        // from a tree without the recording — or a corpus that stopped reaching
        // a leg — would make every arm above compare `None == None` and pass
        // having measured nothing. Three floors, one per direction of the rule:
        //
        //  (a) SOMETHING carries a key at all (the primary rows);
        //  (b) a RE-STREAM carries one — the native loop's first re-stream is
        //      the only leg where "inherit" is the correct answer, so if no case
        //      reaches it, a mutation clearing the key in `loop_base_params`
        //      would survive;
        //  (c) a re-stream carries `null` — the force-final and the text
        //      continuation are where v4 drops the id, so if neither is reached,
        //      a mutation deleting the two `params.cache_key = None` lines in the
        //      loops would survive.
        assert!(
            streaming.expected_cache_key.values().any(|v| v.is_some()),
            "no oracle row recorded a cacheKey: the oracle predates the P4.95 recording, or the mock stopped calling v4's buildCharacterCacheKey; every cacheKey arm above is vacuous"
        );
        // A RE-STREAM row is one whose slate ENDS in a tool result: the loops
        // append the results and re-stream immediately, so the result is the
        // last thing in the body — the native loop threads a `tool` role
        // message (`buildToolResultMessages`), the text loop writes a user-role
        // ledger entry (`<tool_result name="…">` for simple-json,
        // `[Tool Result: …]` for text-block).
        //
        // ⚠ "carries a tool result ANYWHERE" is NOT the discriminator, and the
        // difference is load-bearing: a PRIMARY call whose chat HISTORY already
        // holds tool messages satisfies it. Measured on this corpus — 13 rows
        // carry one somewhere, only 5 end in one — so the loose spelling let
        // eight primary rows stand in for a re-stream, and floor (b) would have
        // passed with the native loop's re-stream unreached. Found by posing
        // floor (b)'s own scenario (P4.95's F-b proof) and watching the per-call
        // arm fire instead of the floor.
        let is_restream = |r: &CannedStreamW| {
            r.messages.last().is_some_and(|m| {
                m.role == "tool"
                    || m.content.contains("<tool_result name=")
                    || m.content.contains("[Tool Result: ")
            })
        };
        let restreams_with_key = canned_streams
            .iter()
            .filter(|r| is_restream(r) && r.cache_key.is_some())
            .count();
        assert!(
            restreams_with_key > 0,
            "no oracle row is a re-stream CARRYING a cacheKey: the native loop's first re-stream (v4 `native-tool-loop.service.ts:350`, which passes `characterId`) is unreached, so a mutation clearing the key on every loop clone would survive"
        );
        // The force-final's own slate is not a tool-result row (v4 appends the
        // assistant prose + the force-final USER nudge), so it is named by that
        // nudge; the text continuation is named by its ledger entry above.
        // ⚠ The nudge scan below reads the WHOLE slate, not `.last()`: correct
        // while the nudge is a transient loop message never persisted to history
        // (so no later primary row can carry it); a change that persisted it
        // would silently inflate this count.
        let keyless_restreams = canned_streams
            .iter()
            .filter(|r| {
                r.cache_key.is_none()
                    && (is_restream(r)
                        || r.messages.iter().any(|m| {
                            m.content
                                .contains("You have reached the maximum number of agent turns")
                        }))
            })
            .count();
        assert!(
            keyless_restreams >= 2,
            "fewer than two oracle rows are KEYLESS re-streams: v4 drops the characterId on the native force-final (`:421-431`) and on the text continuation (`text-tool-loop.service.ts:390-400`), and if neither leg is reached the two `params.cache_key = None` clears in the loops are untested — a mutation deleting them would survive (got {keyless_restreams})"
        );
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

    // P4.106 item 2: `chat_informs` — the consumption comparand. Normalized
    // AFTER messages so `consumedByMessageId` verifies by relationship through
    // each side's message idmap (a minted assistant id → the matching token; the
    // seeded consumer → the same token both sides).
    let mut got_informs = dump_table(&db, "chat_informs");
    let mut want_informs = want_tables
        .remove("chat_informs")
        .expect("oracle chat_informs — regenerate the oracle");
    normalize_informs(&mut got_informs, &idmap);
    normalize_informs(&mut want_informs, &idmap2);

    assert_table_eq("chats", &got_chats, &want_chats);
    assert_table_eq("chat_messages", &got_msgs, &want_msgs);
    assert_table_eq("background_jobs", &got_jobs, &want_jobs);
    assert_table_eq("chat_informs", &got_informs, &want_informs);
    // Non-vacuous by construction: the corpus plants seven rows and three calls
    // consume; the dump must carry them and at least one consumption.
    {
        let rows = got_informs["rows"].as_array().expect("chat_informs rows");
        assert_eq!(rows.len(), 7, "the planted chat_informs rows went missing");
        let consumed_by_turns = rows
            .iter()
            .filter(|r| r["consumedAt"] == Value::String("<ts>".into()))
            .count();
        assert_eq!(
            consumed_by_turns, 4,
            "the saved turn consumes two rows, the preserved partial one, and the \
             poisoned turn its FIRST row (e6) before the trigger aborts the second"
        );
        // P4.106 Tier 2 — the failed consume, measured: v4 `markConsumed` loops
        // `update` per id inside `safeQuery(…, 0)`, so a write that fails on the
        // SECOND row leaves the FIRST consumed, the second pending, and the turn
        // saved. v5 matches all three (the table equality above) — its per-id
        // loop's first UPDATE is not rolled back by the second's error.
        let poisoned = rows
            .iter()
            .find(|r| r["id"] == "1a000000-0000-4000-8000-0000000000e7")
            .expect("the poisoned row");
        assert!(
            poisoned["consumedAt"].is_null(),
            "the poisoned row must stay pending: {poisoned}"
        );
    }
    // …and v5's line for it, at v4's level and wording (`safeQuery`'s error
    // message is `'Error marking informs consumed'`), on that call ONLY.
    for name in [
        "inform_consume_fails_on_a_poisoned_row",
        "inform_consumed_by_saved_turn",
        "inform_consumed_by_preserved_partial",
        "inform_already_consumed_is_left_alone",
    ] {
        let lines = &initial_logs[name];
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Error marking informs consumed"))
            .collect();
        if name == "inform_consume_fails_on_a_poisoned_row" {
            assert_eq!(hits.len(), 1, "{name}: one failed-consume line: {lines:#?}");
            assert!(
                hits[0].starts_with("ERROR quilltap::inform"),
                "{name}: v4's safeQuery logs at ERROR: {}",
                hits[0]
            );
            assert!(
                hits[0].contains("P4.106 poisoned inform row"),
                "{name}: the trigger's error must ride the line: {}",
                hits[0]
            );
        } else {
            assert!(hits.is_empty(), "{name}: no failed-consume line: {hits:?}");
        }
    }

    // --- P4.D186 tier 2: the held result's SHAPE, read off the jobs it did not
    // enqueue ---
    // `finish_held_user_turn` returns `hasContent: false`, and v4's
    // `handleSendMessage` gates BOTH the scene-state trigger and the Scriptorium
    // render on `result.hasContent`. Nothing about the returned struct is a
    // comparand on either side, so the only observable consequence is the absence
    // of those job rows — asserted on V5's OWN dump (an assertion on the oracle
    // could not catch a v5 regression), with the table equality above carrying it
    // to v4. Non-vacuous by construction: the same corpus enqueues 38
    // CONVERSATION_RENDER rows for the chats that DO take a turn.
    {
        let held_chat_ids: Vec<String> = spec
            .calls
            .iter()
            .filter(|c| held_cases.contains(&c.name.as_str()))
            .map(|c| c.chat_id.clone())
            .collect();
        assert_eq!(held_chat_ids.len(), held_cases.len(), "held chat ids");
        let rows = got_jobs
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut render_rows = 0usize;
        for row in &rows {
            let kind = row.get("type").and_then(Value::as_str).unwrap_or_default();
            if kind == "CONVERSATION_RENDER" {
                render_rows += 1;
            }
            // `SCENE_STATE_TRACKING` is a FORWARD guard: nothing in v5 enqueues
            // that kind yet (the trigger is unported — `spine.rs` says so), so
            // only the `CONVERSATION_RENDER` half is live and only that half
            // has the non-vacuity floor below. The kind stays named so the day
            // the trigger lands, a held turn cannot fire it unnoticed.
            if kind != "CONVERSATION_RENDER" && kind != "SCENE_STATE_TRACKING" {
                continue;
            }
            let payload = row
                .get("payload")
                .and_then(Value::as_str)
                .unwrap_or_default();
            for held in &held_chat_ids {
                assert!(
                    !payload.contains(held.as_str()),
                    "a held turn enqueued a {kind} job for {held} — `hasContent` \
                     must be false so neither scene tracking nor the Scriptorium \
                     render fires: {payload}"
                );
            }
        }
        assert!(
            render_rows > 0,
            "no CONVERSATION_RENDER rows at all — the absence assertion above \
             would be vacuous"
        );
    }

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
    let all_got_logs = common::dump_llm_logs(&db);

    // --- the failover-logging WIRING pin (P4.68) ---
    //
    // v4 `orchestrator.service.ts:1572` hands `preGeneratedAssistantMessageId`
    // to `attemptEmptyResponseRecovery`, and every `restreamInto` leg passes it
    // to `streamMessage` as `messageId` — so a turn whose primary answers empty
    // logs the primary's `CHAT_MESSAGE` row AND one per retry leg, all sharing
    // that one message id. v5 called a no-logging entry point here until P4.68,
    // so the retry legs wrote nothing.
    //
    // The `strip_seam_rows` filter below removes `CHAT_MESSAGE` from BOTH sides
    // (v4's service-level `streamMessage` mock swallows its own log, so the
    // oracle has none to compare against), which means the differential CANNOT
    // see this wiring — measured: 57 rows with the log unwired, 59 with it, and
    // the family is green either way. Hence a v5-side census here. It is not an
    // equivalence claim; the failover row SHAPE is proven byte-exact by
    // `primary_stream_tier3`.
    //
    // The discriminator is structural rather than a magic count, so the corpus
    // can grow: a repeated `(chatId, messageId)` pair is a re-stream into the
    // same pre-generated assistant message, which only a failover leg does.
    {
        // A re-stream into the same pre-generated assistant message is either a
        // FAILOVER leg (v4 `restreamInto` — `characterId` SET since `65f5021c8`)
        // or the tool-unsupported RETRY (v4 `primary-stream.service.ts:246-256`,
        // which passes NO `characterId`; v5 reproduces that at
        // `primary_stream.rs`'s retry ctx). Only the former is this wiring's
        // evidence, so the census counts repeated-pair rows WITH a character —
        // the §3 unification review found the first draft counting every
        // repeat and asserting NULL-free characters on every row, which a
        // retry case landing in the corpus would have reddened on a correct
        // tree.
        let mut by_key: HashMap<(String, String), (usize, usize)> = HashMap::new();
        for row in all_got_logs
            .iter()
            .filter(|r| r["type"].as_str() == Some("CHAT_MESSAGE"))
        {
            let chat = row["chatId"].as_str().unwrap_or_default().to_string();
            let msg = row["messageId"].as_str().unwrap_or_default().to_string();
            let e = by_key.entry((chat, msg)).or_default();
            e.0 += 1;
            if row["characterId"].as_str().is_some() {
                e.1 += 1;
            }
        }
        let failover_legs: usize = by_key
            .values()
            .filter(|(total, _)| *total > 1)
            .map(|(_, with_character)| with_character.saturating_sub(1))
            .sum();
        assert!(
            failover_legs > 0,
            "no CHAT_MESSAGE row with a characterId shares a (chatId, messageId) \
             with another, so no failover leg logged one. The orchestrator's \
             empty-response recovery is not being handed its `FailoverLogCtx` \
             (v4 `orchestrator.service.ts:1572`), or the corpus lost its \
             empty-primary case. Pairs seen: {}",
            by_key.len()
        );
    }

    let got_logs = strip_seam_rows(all_got_logs);
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

/// Whole-frame compare — P4.D173's `routeTrail` on the `done` frame is a
/// comparand of its own now that both stacked lanes are on one branch (the
/// P4.D172-era subtraction and its run-scoped tripwire retired at unification).
fn assert_events_eq(name: &str, got: &[Value], want: &[Value]) {
    if got != want {
        let g = serde_json::to_string_pretty(got).unwrap();
        let w = serde_json::to_string_pretty(want).unwrap();
        panic!("event trace mismatch for {name}\n--- got ---\n{g}\n--- want ---\n{w}");
    }
}

// The `pin_summary_fold_last_turn_divergence` that stood here from P4.D172 to
// the `a2db63da7` unification asserted that v4 LOST its own `lastTurnParticipantId`
// write on the `summary_fold` row while v5 persisted what v4's frame announced.
// At `a2db63da7` v4 persists it too (measured by P4.D212 across two regens; the
// only code commit in range touching that path is `e7821606f`'s fold rewrite —
// the fold's new `await resolveSpeakerNames` moves the interleaving that used to
// lose the write). The divergence CONVERGED, so the row-wise `chats` compare now
// sees the field raw on both sides; the candidate v4 filing that pin carried is
// moot./// P4.106 item 2: `chat_informs` rows (seeded ids, sorted by id). A row the
/// run touched (`updatedAt` moved off `createdAt`) has its `updatedAt` and
/// `consumedAt` placeholdered — v4's frozen clock vs v5's real one — and every
/// `consumedByMessageId` goes through the side's message idmap.
fn normalize_informs(dump: &mut Value, idmap: &HashMap<String, String>) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| r["id"].as_str().unwrap_or("").to_string());
        for row in rows {
            let Some(obj) = row.as_object_mut() else {
                continue;
            };
            let touched = obj.get("updatedAt") != obj.get("createdAt");
            if touched {
                obj.insert("updatedAt".into(), Value::String("<ts>".into()));
                if obj.get("consumedAt").is_some_and(|v| !v.is_null()) {
                    obj.insert("consumedAt".into(), Value::String("<ts>".into()));
                }
            }
            if let Some(m) = obj.get("consumedByMessageId").and_then(Value::as_str) {
                let tok = idmap
                    .get(m)
                    .cloned()
                    .unwrap_or_else(|| format!("<unknown message {m}>"));
                obj.insert("consumedByMessageId".into(), Value::String(tok));
            }
        }
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
            custom_tools: true,
            answer_confirmation_global_enabled: false,
            autonomous_destructive_policy: "opt_in_per_room".to_string(),
            // Agent mode (W4.4): the fixture's single chat_settings row sets
            // `agentModeSettings = { maxTurns: 1, defaultEnabled: false }` (a
            // NON-default maxTurns — the default is 10 — so the `agent_mode_on`
            // case still banks custom-maxTurns propagation into the injected
            // instruction). `defaultEnabled` stays false, so every non-opted-in
            // chat resolves agent mode OFF; the `agent_mode_on` chat opts in at
            // the Chat level (`agentModeEnabled`).
            //
            // P4.95 lowered it 15 → 1. `maxTurns` has NO per-chat level (v4's
            // `resolveAgentModeSetting` reads it from the global row alone), and
            // the native loop's force-final branch is gated on `toolIterations >=
            // effectiveMaxTurns` — so one shared cap of 1 is the only way to reach
            // that branch without seeding fifteen tool rounds. `agent_force_final`
            // is the case that walks it; `agent_mode_on` is unaffected in which LEGS
            // it reaches (its stream carries no tool call, so `toolIterations`
            // never leaves 0) — its prompt BYTES do move, on both sides at once:
            // `buildAgentModeInstructions(maxTurns)` interpolates the number
            // (v4 `agent-mode-resolver.service.ts:113`, v5 `agent_mode.rs:168`),
            // so that case now reads "up to 1 tool iterations", still a
            // non-default value and still a discriminator.
            agent_mode_default_enabled: false,
            agent_mode_max_turns: 1,
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
            // { strategy: PROVIDER_CHEAPEST, fallbackToLocal: false,
            //   defaultCheapProfileId: <CheapDefault> }`. The spine resolves the
            // cheap-LLM selection from this + the connection profiles, and threads it
            // into buildContext (activating the recap/distill feeders).
            // finding #27: the `defaultCheapProfileId` (priority 1) is what the fixed
            // summary check honours — the fold's cheap-LLM `llm_logs` row records the
            // CheapDefault profile (OPENAI/cheap-configured-model), NOT
            // `getCheapestModel(ANTHROPIC)`. This value MUST mirror the fixture's
            // `chatSettings.cheapLLMSettings.defaultCheapProfileId`.
            cheap_llm_strategy: "PROVIDER_CHEAPEST".to_string(),
            cheap_llm_user_defined_profile_id: None,
            cheap_llm_default_cheap_profile_id: Some(
                "f0000006-0000-4000-8000-000000000006".to_string(),
            ),
            cheap_llm_fallback_to_local: false,
        })
    }
}
