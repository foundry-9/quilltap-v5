//! Tier-3 differential test: the **primary-stream / recovery / provider-failover**
//! services (`quilltap_core::services::{primary_stream, recovery,
//! provider_failover}` — v4 `primary-stream.service.ts` / `recovery.service.ts` /
//! `provider-failover.service.ts`), the model-calling stream services, verified
//! tier-3 → tier-2.
//!
//! Both sides pin the `streamMessage` seam identically: the jest oracle resolves
//! each real stream to a corpus rule and RECORDS the exact
//! `provider|model|temperature|messages` canned key it answered (as an ordered
//! per-key list of chunk sequences — a key hit twice by the tool-unsupported
//! retry records both in order); this test replays those recorded sequences
//! through a stateful [`CannedStreamingProvider`] wrapper (a per-key queue), so a
//! prompt/params divergence surfaces as a canned-miss. Then:
//!
//!   1. each call's ordered SSE-event trace (decoded to JSON) is compared
//!      byte-for-byte against v4's recorded frames;
//!   2. each call's result object is compared (minted recovery/static message ids
//!      tokenized on both sides);
//!   3. the `chat_messages` + `chats` dumps are compared in the minted-values
//!      normalization (message ids remapped to first-appearance tokens, minted
//!      timestamps placeholdered);
//!   4. **W4.11b:** the `llm_logs` CHAT_MESSAGE rows are compared. v4's log fires
//!      inside the REAL `streamMessage` wrapper (streaming.service.ts:407), so the
//!      oracle relocates its model mock DOWN to `createLLMProvider` (the wrapper
//!      itself runs) and un-mocks `logLLMCall`. The Rust primary path already logs
//!      via `run_primary_stream`; W4.11b adds the failover-leg logging. Both dump
//!      via the shared `common::{materialize,dump}_llm_logs` (id/createdAt/updatedAt
//!      placeholdered, sorted by canonical JSON; `requestHashes` diffed as a row
//!      column). `durationMs = Date.now() - startTime` is a PRESENCE marker on
//!      both sides — `common::normalize_duration_ms` collapses any non-NULL value
//!      to `"<ms>"` and keeps NULL NULL, so the oracle's frozen-clock 0 and the
//!      port's measured elapsed compare equal while a side that stopped writing a
//!      duration at all still fails. (The port hard-coded `Some(0.0)` until
//!      dogfood finding #100 — every streamed chat message logged a zero duration
//!      where v4 logs a real one.) Rows:
//!      clean_stream + tool_unsupported_retry_success (characterId set), the four
//!      failover legs (characterId SET — v4 `65f5021c8`); NONE for recovery (v4 passes no userId).
//!
//! ## ⚠ Measured blind spot — the credential-gate chain arm (P4.68)
//!
//! A chain candidate refused by `resolveConnectionProfileApiKey` records
//! `auth` with `no-api-key-configured` (v4 `provider-failover.service.ts:583-591`;
//! v5 `provider_failover.rs`'s `FallbackTrigger::Auth` arm). NO case in this
//! corpus reaches it — every call shares one chain whose three profiles all
//! carry keys. The shape that would: a per-call primary override plus a second
//! primary with `allowTierFallback: false` pointing at a KEYLESS understudy,
//! both `modelClass: null` so they stay out of every existing case's tier pool
//! (the P4.68 lane record spells it out). Until that lands, the `auth` bytes are
//! pinned by nothing here.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-primary-stream-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/primary-stream-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/primary-stream-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-primary-stream.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-primary-stream-fixture.ts
//!   QT_FIXTURE_PRIMARY_STREAM=/tmp/qt-primary-stream.db \
//!   QT_ORACLE_OUT=/tmp/oracle-primary-stream.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- primary-stream-tier3
//!   cp "$V5W/harness/oracle/cases/openai-chaining-fallback-tier3.test.ts" "$TMPO/cases/"
//!   QT_ORACLE_OUT=/tmp/oracle-openai-fallback.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- openai-chaining-fallback-tier3
//! Run:
//!   QT_ORACLE_PRIMARY_STREAM=/tmp/oracle-primary-stream.ndjson \
//!   QT_FIXTURE_PRIMARY_STREAM=/tmp/qt-primary-stream.db \
//!   QT_ORACLE_OPENAI_FALLBACK=/tmp/oracle-openai-fallback.ndjson \
//!     cargo test -p quilltap-harness --test primary_stream_tier3_equivalence

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamMessage, StreamParams,
    StreamUsage, StreamingCompletionProvider,
};
use quilltap_core::model::streaming_provider::{ProviderKeySource, WireStreamingProvider};
use quilltap_core::model::transport::{
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportPolicy, TransportRequest,
    TransportResponse,
};
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::primary_stream::{
    self, find_previous_response_id, EffectiveProfile, PreservePartialOnError,
    RunPrimaryStreamOptions, StreamingState,
};
use quilltap_core::services::provider_failover::{
    self, AttemptEmptyResponseRecoveryOptions, DangerSettings, DangerousContentRouter,
    FailoverLogCtx, RouteResult,
};
use quilltap_core::services::recovery::{self, RecoveryContext};
use serde::Deserialize;
use serde_json::{json, Value};

// W4.11b: shared `llm_logs` materialize/dump helpers (the W4.10b common module).
mod common;

// ---------------------------------------------------------------------------
// Spec.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProfileW {
    id: String,
    provider: String,
    model_name: String,
    #[serde(default)]
    base_url: Option<String>,
}

/// The three seeded connection-profile rows the chain reads (P4.D135). The
/// fixture holds them; this is only what the harness needs to NAME them.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChainProfileW {
    id: String,
}

/// A bare "is this profile in the fixture?" probe for the shape assertions.
fn fallback_repos_lookup(db: &Db, id: &str) -> Option<Value> {
    let owned = id.to_string();
    db.read_main(move |c| quilltap_core::db::connection_profiles::find_by_id(c, &owned))
        .ok()
        .flatten()
}

impl ProfileW {
    fn to_effective(&self) -> EffectiveProfile {
        EffectiveProfile {
            id: self.id.clone(),
            provider: self.provider.clone(),
            model_name: self.model_name.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

#[derive(Deserialize, Clone)]
struct CharacterW {
    id: String,
    name: String,
    aliases: Vec<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AttachedFileW {
    filename: String,
    mime_type: String,
    size: f64,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CallW {
    name: String,
    kind: String,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    participant_id: Option<String>,
    #[serde(default)]
    pre_generated_message_id: Option<String>,
    #[serde(default)]
    has_tools: bool,
    #[serde(default)]
    attached_files: Vec<AttachedFileW>,
    /// P4.D136 (v4 `a1d88aa3a`, bug 106): attachments planted on the message
    /// ARRAY. Independent of `attached_files` (what the user uploaded) — the
    /// two disagreeing is exactly what the new case measures.
    #[serde(default)]
    message_attachments: Vec<Value>,
    #[serde(default)]
    original_message: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    content_was_flagged_dangerous: bool,
    #[serde(default)]
    danger_mode: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    existing_messages: Vec<Value>,
    #[serde(default)]
    is_dangerous_routed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    sentinel: String,
    profile: ProfileW,
    uncensored_profile: ProfileW,
    #[serde(default)]
    understudy_profile: Option<ChainProfileW>,
    #[serde(default)]
    tier_spare_profile: Option<ChainProfileW>,
    user_id: String,
    character: CharacterW,
    calls: Vec<CallW>,
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
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

#[derive(Deserialize)]
struct CannedRowW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    sequences: Vec<Vec<ChunkW>>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/primary-stream-tier3.json")
}

// ---------------------------------------------------------------------------
// Stateful canned streaming provider (per-key queue of sequences).
// ---------------------------------------------------------------------------

struct QueuedStreamingProvider {
    // key -> queue of chunk sequences (popped front on each call).
    queues: Mutex<HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>>>,
}

impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedRowW]) -> Self {
        let mut queues: HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>> =
            HashMap::new();
        for row in rows {
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
            let key = canned_stream_key(&row.provider, &row.model, row.temperature, &messages);
            let q = queues.entry(key).or_default();
            for seq in &row.sequences {
                q.push_back(seq.iter().map(chunk_to_result).collect());
            }
        }
        Self {
            queues: Mutex::new(queues),
        }
    }
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
        return Ok(StreamChunk::done(usage));
    }
    Ok(StreamChunk::content(c.content.clone().unwrap_or_default()))
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
        let sequence: Vec<StreamChunkResult> = {
            let mut queues = self.queues.lock().unwrap();
            match queues.get_mut(&key).and_then(|q| q.pop_front()) {
                Some(seq) => seq,
                None => vec![Err(StreamError::new(format!(
                    "no canned stream queued for key ({provider}, model {}, temperature {:?}, {} msgs)",
                    params.model,
                    params.temperature,
                    params.messages.len(),
                )))],
            }
        };
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(sequence.len().max(1));
            for item in sequence {
                let _ = tx.send(item).await;
            }
            rx
        }
    }
}

// The canned dangerous-content router (mirrors the oracle's mock: reroute to the
// uncensored profile with a canned key).
struct CannedRouter {
    profile: EffectiveProfile,
    key: String,
}
impl DangerousContentRouter for CannedRouter {
    async fn resolve(
        &self,
        _original_profile: &EffectiveProfile,
        _original_api_key: &str,
        _settings: &DangerSettings,
        _user_id: &str,
        _turn_attachment_mime_types: &[String],
    ) -> RouteResult {
        RouteResult {
            rerouted: true,
            connection_profile: self.profile.clone(),
            api_key: self.key.clone(),
            profile_row: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Table normalization.
// ---------------------------------------------------------------------------

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
}

/// Remap `id` to first-appearance tokens (shared with the result normalization),
/// placeholder minted timestamps; leave payload columns literal. `participantId`
/// is pinned (from the fixture) so it is remapped too (identically both sides).
fn normalize_chat_messages(dump: &mut Value, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("chat_messages dump has no rows");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().unwrap();
        for col in ["id", "participantId"] {
            if let Some(Value::String(raw)) = obj.get(col) {
                let t = token_for(id_map, raw);
                obj.insert(col.to_string(), Value::String(t));
            }
        }
        for col in ["createdAt"] {
            if obj.get(col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert(col.to_string(), Value::String("<ts>".into()));
            }
        }
    }
}

const CHATS_TS: &[&str] = &["lastMessageAt", "updatedAt"];

/// Collapse minted chat timestamps to `<ts>` only when they differ from the seed
/// sentinel (a chat that received nothing keeps its sentinel — proving no stray
/// mint).
fn normalize_chats(dump: &mut Value, sentinel: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("chats dump has no rows");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().unwrap();
        for col in CHATS_TS {
            let mint = obj
                .get(*col)
                .and_then(Value::as_str)
                .is_some_and(|s| s != sentinel);
            if mint {
                obj.insert((*col).to_string(), Value::String("<ts>".into()));
            }
        }
    }
}

/// Tokenize a minted message id inside a result object (both sides), through the
/// SHARED id map so a message id in the result maps to the same token as its
/// chat_messages row.
fn normalize_result_ids(result: &mut Value, id_map: &mut HashMap<String, String>) {
    if let Some(obj) = result.as_object_mut() {
        if let Some(Value::String(raw)) = obj.get("messageId") {
            let t = token_for(id_map, raw);
            obj.insert("messageId".into(), Value::String(t));
        }
        // The primary earlyReturn carries a messageId too.
        if let Some(Value::Object(er)) = obj.get_mut("earlyReturn") {
            if let Some(Value::String(raw)) = er.get("messageId") {
                let t = token_for(id_map, raw);
                er.insert("messageId".into(), Value::String(t));
            }
        }
    }
}

#[tokio::test]
async fn primary_stream_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PRIMARY_STREAM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PRIMARY_STREAM to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_PRIMARY_STREAM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PRIMARY_STREAM to the seed .db (see header).");
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
    let mut oracle_events: HashMap<String, Value> = HashMap::new();
    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    let mut oracle_tables: HashMap<String, Value> = HashMap::new();
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
            Some("events") => {
                oracle_events.insert(v["call"].as_str().unwrap().to_string(), v["events"].clone());
            }
            Some("canned") => oracle_canned.push(serde_json::from_value(v).expect("parse canned")),
            Some("table") => {
                oracle_tables.insert(v["table"].as_str().unwrap().to_string(), v);
            }
            Some("llmlogs") => oracle_llm_logs = Some(common::oracle_llm_logs(&v)),
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }
    let oracle_llm_logs = oracle_llm_logs.expect("oracle emitted no llmlogs row");

    // Fresh copy so the seed fixture stays pristine.
    let work =
        std::env::temp_dir().join(format!("qt-primary-stream-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let provider = QueuedStreamingProvider::from_oracle(&oracle_canned);
    let router = CannedRouter {
        profile: spec.uncensored_profile.to_effective(),
        key: "uncensored-key".into(),
    };

    // W4.11b: attach a fresh llm-logs partition so the primary + failover
    // CHAT_MESSAGE `logLLMCall` rows land somewhere we can dump.
    let ll_work = std::env::temp_dir().join(format!(
        "qt-primary-stream-rust-ll-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&ll_work);
    common::materialize_llm_logs(&ll_work, &spec.test_pepper_base64);
    let db = Db::open(
        DbPaths {
            main: work.clone(),
            mount_index: None,
            llm_logs: Some(ll_work.clone()),
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // P4.D135: the chain's read seam, over the same fixture copy v4's `repos`
    // read. `fallback_primary` is the failing profile as a full row — v4 walks
    // `state.effectiveProfile`, which IS the whole object; v5's state carries
    // the four-field projection, so the row comes from the table.
    let fallback_repos = quilltap_core::services::fallback_repos::DbFallbackRepos::new(&db);
    let fallback_primary = {
        let id = spec.profile.id.clone();
        db.read_main(move |c| quilltap_core::db::connection_profiles::find_by_id(c, &id))
            .ok()
            .flatten()
            .as_ref()
            .and_then(quilltap_core::llm_fallback::FallbackProfile::from_value)
    };
    assert!(
        fallback_primary.is_some(),
        "the fixture must carry the primary connection profile — the chain reads REAL rows \
         (rebuild it: the recipe's `build-primary-stream-fixture.ts` step)"
    );
    // Shape assertion, not decoration: a fixture that lost the understudy or
    // the spare would leave every chain case measuring a chain of one.
    if let Some(p) = fallback_primary.as_ref() {
        assert_eq!(
            p.fallback_profile_id.as_deref(),
            spec.understudy_profile.as_ref().map(|u| u.id.as_str()),
            "the primary must name the understudy the spec does"
        );
        assert!(
            p.allow_tier_fallback,
            "the primary must opt in to the tier pick, or no case reaches the spare"
        );
    }
    assert!(
        spec.tier_spare_profile
            .as_ref()
            .and_then(|sp| fallback_repos_lookup(&db, &sp.id))
            .is_some(),
        "the fixture must carry the tier spare"
    );

    // The messages the oracle plants for primary/failover calls.
    let user_messages = |marker: &str| -> Vec<CompletionMessage> {
        vec![
            CompletionMessage::system(format!("You are {}.", spec.character.name)),
            CompletionMessage::user(marker.to_string()),
        ]
    };
    // P4.D136 (v4 `a1d88aa3a`, bug 106): the attachments a call plants ON THE
    // ARRAY. `needsVision` is read from here now, not from `attachedFiles`.
    let base_params_with_attachments =
        |messages: Vec<CompletionMessage>, attachments: &[Value]| StreamParams {
            // The canned registration keeps the oracle-recorded `[{role, content}]`
            // shape; the params carry the equivalent StreamMessage projection
            // (identical canned key bytes).
            messages: messages
                .iter()
                .map(|m| match m.role {
                    CompletionRole::System => StreamMessage::system(m.content.clone()),
                    CompletionRole::Assistant => StreamMessage::assistant(m.content.clone()),
                    _ => StreamMessage::User {
                        content: m.content.clone(),
                        cache_control: None,
                        attachments: attachments.to_vec(),
                    },
                })
                .collect(),
            model: spec.profile.model_name.clone(),
            temperature: Some(1.0),
            max_tokens: Some(4096),
            top_p: None,
            tools: None,
            web_search_enabled: false,
            profile_parameters: None,
            cache_key: None,
            previous_response_id: None,
            stop: Vec::new(),
            request_timeout_ms: None,
        };

    assert_eq!(
        spec.calls.len(),
        oracle_results.len(),
        "call-count mismatch — regenerate the oracle"
    );

    // Accumulate (got, want) result + event pairs to normalize together with the
    // shared id map at the end (the `done` event's messageId is minted, so it must
    // ride the same first-appearance token map as the results + table rows).
    let mut result_pairs: Vec<(String, Value, Value)> = Vec::new();
    let mut event_pairs: Vec<(String, Value, Value)> = Vec::new();

    for (call, (oracle_name, oracle_result)) in spec.calls.iter().zip(oracle_results.into_iter()) {
        assert_eq!(call.name, oracle_name, "call order mismatch");
        let name = call.name.clone();
        let sink = RecordingSink::new();

        let got_result: Value = match call.kind.as_str() {
            "findPrevResponseId" => {
                let provider = call.provider.clone().unwrap_or_default();
                let out = find_previous_response_id(&provider, &call.existing_messages);
                json!(out)
            }
            "primary" => {
                let marker = call.original_message.clone().unwrap_or_default();
                let mut params =
                    base_params_with_attachments(user_messages(&marker), &call.message_attachments);
                if call.has_tools {
                    params.tools = Some(json!([{ "function": { "name": "noop" } }]));
                }
                let mut state = StreamingState {
                    effective_profile: Some(spec.profile.to_effective()),
                    effective_api_key: "primary-key".into(),
                    ..Default::default()
                };
                let mut preserve = PreservePartialOnError::new(
                    call.chat_id.clone().unwrap(),
                    spec.character.id.clone(),
                    spec.character.name.clone(),
                    spec.character.aliases.clone(),
                    call.participant_id.clone().unwrap(),
                    None,
                    call.pre_generated_message_id.clone().unwrap(),
                );
                let attached = call
                    .attached_files
                    .iter()
                    .map(|f| recovery::AttachedFile {
                        filename: f.filename.clone(),
                        mime_type: f.mime_type.clone(),
                        size: f.size,
                    })
                    .collect();
                let opts = RunPrimaryStreamOptions {
                    // U4.4: the request-path default (none) — inert on this corpus.
                    log_context: LogContext::none(),
                    chat_id: call.chat_id.clone().unwrap(),
                    user_id: spec.user_id.clone(),
                    chat: primary_stream::PrimaryStreamChat { is_paused: false },
                    character_id: spec.character.id.clone(),
                    character_name: spec.character.name.clone(),
                    character_aliases: spec.character.aliases.clone(),
                    participant_id: call.participant_id.clone().unwrap(),
                    participant_status: None,
                    user_participant_id: None,
                    is_multi_character: false,
                    params,
                    attached_files: attached,
                    original_message: call.original_message.clone(),
                    pre_generated_assistant_message_id: call
                        .pre_generated_message_id
                        .clone()
                        .unwrap(),
                    // P4.D135: no `primary` case is danger-routed or carries an
                    // image; the fallback shapes get their own `hardFailover`
                    // cases below, where the flags matter.
                    is_dangerous_routed: false,
                    fallback_profile: fallback_primary.clone(),
                    state: &mut state,
                };
                match primary_stream::run_primary_stream(
                    &db,
                    &provider,
                    &sink,
                    &mut preserve,
                    Some(&fallback_repos),
                    opts,
                )
                .await
                {
                    Ok(res) => {
                        let early = res.early_return.map(|e| {
                            json!({
                                "isMultiCharacter": e.is_multi_character,
                                "hasContent": e.has_content,
                                "messageId": e.message_id,
                                "userParticipantId": e.user_participant_id,
                                "isPaused": e.is_paused,
                            })
                        });
                        json!({ "earlyReturn": early, "fullResponse": state.full_response })
                    }
                    Err(e) => json!({ "threw": e.message, "fullResponse": state.full_response }),
                }
            }
            "recovery" => {
                let ctx = RecoveryContext {
                    chat_id: call.chat_id.clone().unwrap(),
                    character_name: spec.character.name.clone(),
                    connection_profile: Some(spec.profile.to_effective()),
                    api_key: "primary-key".into(),
                    attached_files: call
                        .attached_files
                        .iter()
                        .map(|f| recovery::AttachedFile {
                            filename: f.filename.clone(),
                            mime_type: f.mime_type.clone(),
                            size: f.size,
                        })
                        .collect(),
                    original_message: call.original_message.clone(),
                    error_message: call.error_message.clone().unwrap_or_default(),
                    character_participant_id: call.participant_id.clone().unwrap(),
                };
                let rr = recovery::attempt_request_limit_recovery(&db, &provider, &sink, ctx).await;
                json!({
                    "success": rr.success,
                    "messageId": rr.message_id,
                    "isStaticFallback": rr.is_static_fallback,
                    "response": rr.response,
                })
            }
            "failover" => {
                let marker = call.original_message.clone().unwrap_or_default();
                let mut state = StreamingState {
                    effective_profile: Some(spec.profile.to_effective()),
                    effective_api_key: "primary-key".into(),
                    ..Default::default()
                };
                let danger_mode = call.danger_mode.clone().unwrap_or_else(|| "OFF".into());
                let uncensored_id = if danger_mode == "AUTO_ROUTE" {
                    Some(spec.uncensored_profile.id.clone())
                } else {
                    None
                };
                let opts = AttemptEmptyResponseRecoveryOptions {
                    state: &mut state,
                    tool_messages_length: 0,
                    content_was_flagged_dangerous: call.content_was_flagged_dangerous,
                    danger_settings: DangerSettings {
                        mode: danger_mode,
                        uncensored_text_profile_id: uncensored_id,
                    },
                    connection_profile: spec.profile.to_effective(),
                    params: base_params_with_attachments(
                        user_messages(&marker),
                        &call.message_attachments,
                    ),
                    user_id: spec.user_id.clone(),
                    chat_id: call.chat_id.clone().unwrap(),
                    character_id: spec.character.id.clone(),
                    character_name: spec.character.name.clone(),
                    // P4.D135: present `repos` + `fallback_context` is what
                    // enables the THIRD recovery; the pre-existing cases whose
                    // second step produced content never reach it.
                    fallback_context: Some(provider_failover::ChainCapabilities {
                        dangerous: false,
                        needs_vision: false,
                        needs_tools: false,
                    }),
                };
                // W4.11b: drive the recovery with its log context so the failover
                // CHAT_MESSAGE rows are written (messageId = the pre-generated id).
                // ⚠ `characterId` is NOT null: v4 `65f5021c8` added
                // `characterId: opts.character.id` to `restreamInto`'s
                // `streamMessage` call, and `attempt_empty_response_recovery`
                // passes `Some(&character_id)` to match. (This comment claimed the
                // opposite until P4.68 — the code four lines into
                // `provider_failover` had said so all along.)
                let msg_id = call.pre_generated_message_id.clone().unwrap();
                let log_ctx = quilltap_core::services::llm_logging::LogContext::none();
                let flags = provider_failover::attempt_empty_response_recovery::<
                    _,
                    _,
                    _,
                    _,
                    quilltap_core::model::completion::CannedCompletionProvider,
                >(
                    &provider,
                    &sink,
                    &router,
                    Some(&fallback_repos),
                    // The reroute's attachment re-decide (v4 `a1d88aa3a`, bug
                    // 106) is exercised by `file_attachment_tier3`'s (E) family,
                    // over the fixture that HAS a describer and image bytes.
                    // This family's arrays carry no attachments at all, so the
                    // adapter would answer "unchanged" for every case here;
                    // `None` is v4's own falsy-`repos` arm and keeps the canned
                    // router (which has no profile row to hand over) honest.
                    None,
                    opts,
                    Some(FailoverLogCtx {
                        db: &db,
                        message_id: &msg_id,
                        log_context: &log_ctx,
                    }),
                )
                .await;
                json!({
                    "uncensoredRetryAttempted": flags.uncensored_retry_attempted,
                    "sameProviderRetryAttempted": flags.same_provider_retry_attempted,
                    "chainFallbackAttempted": flags.chain_fallback_attempted,
                    "chainAttempts": flags
                        .chain_attempts
                        .iter()
                        .map(|a| json!({
                            "profileId": a.profile_id,
                            "profileName": a.profile_name,
                            "provider": a.provider,
                            "modelName": a.model_name,
                            "trigger": a.trigger.as_str(),
                            "error": a.error,
                        }))
                        .collect::<Vec<_>>(),
                    "fullResponse": state.full_response,
                    "effectiveProfileId": state.effective_profile.as_ref().map(|p| p.id.clone()),
                })
            }
            "hardFailover" => {
                // P4.D135: `run_primary_stream`'s catch-all, which since
                // `65f5021c8` walks the chain before it rethrows. Driven through
                // the WHOLE `run_primary_stream` rather than
                // `attempt_hard_error_failover` directly, because the two things
                // worth pinning are exactly the ones only the whole path shows:
                // that the chain runs AFTER the tool-unsupported and
                // request-limit branches have declined, and that an exhausted
                // chain's summary reaches the rethrown error's message.
                let mut state = StreamingState {
                    effective_profile: Some(spec.profile.to_effective()),
                    effective_api_key: "primary-key".into(),
                    ..Default::default()
                };
                let mut preserve = PreservePartialOnError::new(
                    call.chat_id.clone().unwrap(),
                    spec.character.id.clone(),
                    spec.character.name.clone(),
                    spec.character.aliases.clone(),
                    call.participant_id.clone().unwrap(),
                    None,
                    call.pre_generated_message_id.clone().unwrap(),
                );
                let marker = call.original_message.clone().unwrap_or_default();
                let params =
                    base_params_with_attachments(user_messages(&marker), &call.message_attachments);
                let opts = RunPrimaryStreamOptions {
                    log_context: LogContext::none(),
                    chat_id: call.chat_id.clone().unwrap(),
                    user_id: spec.user_id.clone(),
                    chat: primary_stream::PrimaryStreamChat { is_paused: false },
                    character_id: spec.character.id.clone(),
                    character_name: spec.character.name.clone(),
                    character_aliases: spec.character.aliases.clone(),
                    participant_id: call.participant_id.clone().unwrap(),
                    participant_status: None,
                    user_participant_id: None,
                    is_multi_character: false,
                    params,
                    attached_files: Vec::new(),
                    original_message: call.original_message.clone(),
                    pre_generated_assistant_message_id: call
                        .pre_generated_message_id
                        .clone()
                        .unwrap(),
                    is_dangerous_routed: call.is_dangerous_routed,
                    fallback_profile: fallback_primary.clone(),
                    state: &mut state,
                };
                match primary_stream::run_primary_stream(
                    &db,
                    &provider,
                    &sink,
                    &mut preserve,
                    Some(&fallback_repos),
                    opts,
                )
                .await
                {
                    Ok(res) => json!({
                        "earlyReturn": res.early_return.map(|e| json!({
                            "isMultiCharacter": e.is_multi_character,
                            "hasContent": e.has_content,
                            "messageId": e.message_id,
                            "userParticipantId": e.user_participant_id,
                            "isPaused": e.is_paused,
                        })),
                        "fullResponse": state.full_response,
                        "effectiveProfileId": state.effective_profile.as_ref().map(|p| p.id.clone()),
                        // The swap's buffer reset is only measurable against a
                        // DIRTY state — a failed attempt that left reasoning
                        // behind before it died.
                        "reasoningContent": state.reasoning_content,
                        "reasoningSegmentCount": state.reasoning_segments.len(),
                    }),
                    Err(e) => json!({
                        "threw": e.message,
                        "fullResponse": state.full_response,
                        "effectiveProfileId": state.effective_profile.as_ref().map(|p| p.id.clone()),
                        "reasoningContent": state.reasoning_content,
                        "reasoningSegmentCount": state.reasoning_segments.len(),
                    }),
                }
            }
            other => panic!("unknown call kind {other}"),
        };

        // Defer the event-trace compare to the end: the `done` event's messageId
        // is minted, so it rides the shared first-appearance id map.
        let got_events = Value::Array(sink.events_json());
        let want_events = oracle_events
            .remove(&name)
            .unwrap_or_else(|| panic!("oracle missing events for {name}"));
        event_pairs.push((name.clone(), got_events, want_events));
        result_pairs.push((name, got_result, oracle_result));
    }

    // Now dump the tables.
    let mut got_msgs = db
        .read_main(|c| quilltap_core::db::dump_table_json_conn(c, "chat_messages", "content"))
        .expect("dump chat_messages");
    let mut got_chats = db
        .read_main(|c| quilltap_core::db::dump_table_json_conn(c, "chats", "id"))
        .expect("dump chats");
    // W4.11b: the CHAT_MESSAGE `llm_logs` rows — the primary + tool-retry rows
    // (characterId set) and the failover-leg rows (characterId SET — v4 `65f5021c8`); none for
    // recovery (v4 passes no userId there). requestHashes are part of the row diff.
    let got_logs = common::dump_llm_logs(&db);
    drop(db);
    let _ = std::fs::remove_file(&work);
    let _ = std::fs::remove_file(&ll_work);

    let mut want_msgs = oracle_tables
        .remove("chat_messages")
        .expect("oracle chat_messages");
    let mut want_chats = oracle_tables.remove("chats").expect("oracle chats");
    for v in [&mut want_msgs, &mut want_chats] {
        v.as_object_mut().unwrap().remove("kind");
    }

    // The oracle dumps chat_messages ordered by id; re-sort both by content so the
    // first-appearance id remap is stable across sides.
    sort_rows_by(&mut want_msgs, "content");
    sort_rows_by(&mut got_msgs, "content");

    // Shared id map per side: normalize results (minted messageIds), then event
    // traces (the `done` messageId), then the tables — in ONE fixed order per side
    // so a minted id maps to the same first-appearance token everywhere it appears.
    let mut got_map: HashMap<String, String> = HashMap::new();
    let mut want_map: HashMap<String, String> = HashMap::new();
    for (name, got_result, want_result) in &mut result_pairs {
        let mut g = got_result.clone();
        let mut w = want_result.clone();
        normalize_result_ids(&mut g, &mut got_map);
        normalize_result_ids(&mut w, &mut want_map);
        assert_eq!(g, w, "{name}: result object diverges");
    }
    for (name, got_events, want_events) in &mut event_pairs {
        let mut g = got_events.clone();
        let mut w = want_events.clone();
        normalize_event_ids(&mut g, &mut got_map);
        normalize_event_ids(&mut w, &mut want_map);
        assert_events_eq(name, &g, &w);
    }
    normalize_chat_messages(&mut got_msgs, &mut got_map);
    normalize_chat_messages(&mut want_msgs, &mut want_map);
    normalize_chats(&mut got_chats, &spec.sentinel);
    normalize_chats(&mut want_chats, &spec.sentinel);

    assert_eq!(
        got_msgs["columns"], want_msgs["columns"],
        "chat_messages column set / order"
    );
    assert_eq!(
        got_msgs["rows"], want_msgs["rows"],
        "chat_messages rows diverge"
    );
    assert_eq!(
        got_chats["columns"], want_chats["columns"],
        "chats column set"
    );
    assert_eq!(got_chats["rows"], want_chats["rows"], "chats rows diverge");

    // W4.11b: the `llm_logs` CHAT_MESSAGE rows (id/createdAt/updatedAt placeholdered
    // by the shared dump; durationMs collapsed to the `"<ms>"` presence marker on
    // both sides; requestHashes asserted as part of the row). Both sides sorted by
    // canonical JSON.
    assert_eq!(
        got_logs,
        oracle_llm_logs,
        "llm_logs CHAT_MESSAGE rows diverge (got {} vs oracle {})",
        got_logs.len(),
        oracle_llm_logs.len()
    );

    eprintln!(
        "OK: primary-stream tier-3 matched oracle ({} calls, {} llm_logs rows).",
        result_pairs.len(),
        got_logs.len()
    );
}

/// Tokenize the minted `messageId` in each `done` event through the shared map.
fn normalize_event_ids(events: &mut Value, id_map: &mut HashMap<String, String>) {
    if let Some(arr) = events.as_array_mut() {
        for ev in arr.iter_mut() {
            if let Some(obj) = ev.as_object_mut() {
                if obj.get("done").and_then(Value::as_bool) == Some(true) {
                    if let Some(Value::String(raw)) = obj.get("messageId") {
                        let t = token_for(id_map, raw);
                        obj.insert("messageId".into(), Value::String(t));
                    }
                }
            }
        }
    }
}

fn assert_events_eq(name: &str, got: &Value, want: &Value) {
    assert_eq!(
        got, want,
        "{name}: SSE event trace diverges\n  rust:   {got}\n  oracle: {want}"
    );
}

fn sort_rows_by(dump: &mut Value, col: &str) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by(|a, b| {
            let av = a.get(col).and_then(Value::as_str).unwrap_or("");
            let bv = b.get(col).and_then(Value::as_str).unwrap_or("");
            av.cmp(bv)
        });
    }
}

// ===========================================================================
// P4.41 — the OpenAI conversation-chaining fallback (dogfood #69), tier-3.
//
// A SEPARATE oracle from the primary-stream family above: that family mocks
// `createLLMProvider().streamMessage` wholesale (ABOVE the provider), so v4's
// fallback — which lives INSIDE `OpenAIProvider.streamMessage` — never runs
// there. This case drives the REAL provider on v4 (SDK mocked fail-then-succeed,
// `harness/oracle/cases/openai-chaining-fallback-tier3.test.ts`) and the REAL
// `WireStreamingProvider` here (transport mocked fail-then-succeed), and diffs
// the recovered chunk sequence + the fallback fingerprint (two attempts, the
// first chained, the retry NOT). The byte proof that the retry drops the id and
// carries the full input is the unit + wire tests; this proves turn-level
// outcome parity against v4's real fallback.
// ===========================================================================

/// Fails the FIRST `execute_stream` (the chained attempt) and serves the scripted
/// Responses-API SSE on the retry, recording every request body.
struct FallbackTransport {
    calls: Mutex<usize>,
    seen: Mutex<Vec<TransportRequest>>,
    retry_frame: Vec<u8>,
}

impl ProviderTransport for FallbackTransport {
    fn execute<'a>(
        &'a self,
        _request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        Box::pin(async move {
            Err(TransportError {
                message: "non-streaming not scripted".to_string(),
                status: None,
            })
        })
    }
    fn execute_stream<'a>(
        &'a self,
        request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        self.seen.lock().unwrap().push(request.clone());
        let n = {
            let mut c = self.calls.lock().unwrap();
            *c += 1;
            *c
        };
        let frame = self.retry_frame.clone();
        Box::pin(async move {
            if n == 1 {
                Err(TransportError {
                    message: "400 previous_response_not_found".to_string(),
                    status: None,
                })
            } else {
                let (tx, rx) = tokio::sync::mpsc::channel(1);
                let _ = tx.send(Ok(frame)).await;
                Ok(rx)
            }
        })
    }
}

struct FallbackKeys;
impl ProviderKeySource for FallbackKeys {
    fn key_for(&self, _provider: &str) -> Option<String> {
        Some("test-key".to_string())
    }
}

/// Normalize a decoded [`StreamChunk`] to the oracle's `(content, done, usage)`
/// shape (camelCase usage keys matching the TS oracle).
fn normalize_fallback_chunk(c: &StreamChunk) -> Value {
    json!({
        "content": c.content,
        "done": c.done,
        "usage": c.usage.as_ref().map(|u| json!({
            "promptTokens": u.prompt_tokens,
            "completionTokens": u.completion_tokens,
            "totalTokens": u.total_tokens,
        })),
    })
}

#[tokio::test]
async fn openai_chaining_fallback_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_OPENAI_FALLBACK") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_OPENAI_FALLBACK to the oracle NDJSON (see \
                 harness/oracle/cases/openai-chaining-fallback-tier3.test.ts)."
            );
            return;
        }
    };
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let oracle: Value =
        serde_json::from_str(oracle_text.trim().lines().next().expect("oracle line"))
            .expect("parse oracle line");
    assert_eq!(oracle["kind"], "fallback", "unexpected oracle row kind");

    // The retry SSE the mocked SDK yields on v4's full-input attempt, mirrored
    // byte-for-byte so both sides decode the same recovered stream.
    let retry_frame = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"model\":\"gpt-4o\",\"output_text\":\"Hi\",\"output\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":2,\"total_tokens\":12}}}\n\n",
    )
    .as_bytes()
    .to_vec();

    let params = StreamParams {
        messages: vec![
            StreamMessage::system("You are Byron."),
            StreamMessage::user("Roll a die."),
            StreamMessage::assistant("I rolled a 4."),
            StreamMessage::user("Roll again."),
        ],
        model: "gpt-4o".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(1024),
        top_p: None,
        tools: None,
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: Some("resp_dead".to_string()),
        stop: Vec::new(),
        request_timeout_ms: None,
    };

    let wire = WireStreamingProvider::new(
        FallbackTransport {
            calls: Mutex::new(0),
            seen: Mutex::new(Vec::new()),
            retry_frame,
        },
        FallbackKeys,
        TransportPolicy::default(),
        "Quilltap/test".to_string(),
    );

    let mut rx = wire.stream_message("OPENAI", None, &params).await;
    let mut got_chunks: Vec<Value> = Vec::new();
    while let Some(item) = rx.recv().await {
        let chunk = item.expect("the fallback must recover the turn, not error");
        got_chunks.push(normalize_fallback_chunk(&chunk));
    }

    // The recovered chunk sequence matches v4's real fallback output.
    assert_eq!(
        Value::Array(got_chunks),
        oracle["chunks"],
        "the recovered chunk sequence diverges from v4's fallback"
    );

    // The fallback fingerprint: two attempts, the first chained, the retry not.
    let seen = wire.transport_ref().seen.lock().unwrap().clone();
    assert_eq!(
        seen.len() as i64,
        oracle["sdkCreateCount"].as_i64().unwrap(),
        "attempt count diverges (v4 issued two SDK creates)"
    );
    let has_prev = |req: &TransportRequest| -> bool {
        serde_json::from_slice::<Value>(&req.body)
            .ok()
            .and_then(|b| b.get("previous_response_id").cloned())
            .is_some()
    };
    assert_eq!(
        has_prev(&seen[0]),
        oracle["firstChained"].as_bool().unwrap(),
        "the first attempt's chaining differs from v4"
    );
    assert_eq!(
        has_prev(&seen[1]),
        oracle["secondChained"].as_bool().unwrap(),
        "the retry's chaining differs from v4 (it must drop the id)"
    );
}
