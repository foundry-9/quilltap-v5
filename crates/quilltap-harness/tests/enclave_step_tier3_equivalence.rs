//! Tier-3 differential test: the **enclave `step()` + schedule tick** (U4.4,
//! the Phase-3 capstone — `quilltap_core::enclave::step`, v4
//! `handleAutonomousRoomTurn` + `handleAutonomousRoomScheduleTick`).
//!
//! Both sides copy the same three-DB seed fixture (main + mount-index +
//! llm-logs) and run the same 19-call sequence (17 turn calls across 16
//! autonomous rooms + 2 schedule ticks over 4 more). The oracle drives v4's
//! REAL handlers UNFORKED (direct-write — the U4.4 encoded decision) with ONLY
//! the model boundaries mocked at the `createLLMProvider` level, so v4's real
//! streaming wrapper writes its CHAT_MESSAGE `llm_logs` rows tagged with the
//! ambient `runWithAutonomousRunId` id; this test composes the ported `step`
//! over `process_message` with the run's `LogContext` threaded explicitly (the
//! B-proof) and replays the recorded canned streams/completions.
//!
//! Diffed state:
//!   1. `chats` — run-state machine outcomes (runState / runStateMessage /
//!      counters / milestone masks / cron slots) with pure-clock columns
//!      placeholdered and minted run ids remapped;
//!   2. `chat_messages` — the Host announcements byte-for-byte (banner /
//!      halfway / near-end / grace / end / paused reason texts — completing
//!      U4.1's composed-string proof) + the turn transcripts;
//!   3. `background_jobs` — the self-re-enqueues + the finalizer/tick enqueues
//!      (payload run/message ids remapped through the shared maps);
//!   4. `llm_logs` — the token-accounting substrate: the turn CHAT_MESSAGE +
//!      distill MEMORY_EXTRACTION rows tagged `autonomousRunId`, the 9c fold's
//!      SUMMARIZATION/TITLE_GENERATION rows UNtagged (the fold runs outside
//!      the run scope on both sides).
//!
//! **TZ=UTC is REQUIRED since P4.d26** (the server-local distill TODAY line; the
//! step passes the instance zone as `server_tz`), so the pin below is
//! load-bearing, not decoration.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-enclave-step-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/enclave-step-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/enclave-step-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-enclave-step-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-enclave-step-mount.db \
//!   QT_FIXTURE_LLMLOGS_OUT=/tmp/qt-enclave-step-llmlogs.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-enclave-step-fixture.ts
//!   QT_FIXTURE_ENCLAVE_STEP_MAIN=/tmp/qt-enclave-step-main.db \
//!   QT_FIXTURE_ENCLAVE_STEP_MOUNT=/tmp/qt-enclave-step-mount.db \
//!   QT_FIXTURE_ENCLAVE_STEP_LLMLOGS=/tmp/qt-enclave-step-llmlogs.db \
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-enclave-step.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- enclave-step-tier3
//! Run:
//!   QT_ORACLE_ENCLAVE_STEP=/tmp/oracle-enclave-step.ndjson \
//!   QT_FIXTURE_ENCLAVE_STEP_MAIN=/tmp/qt-enclave-step-main.db \
//!   QT_FIXTURE_ENCLAVE_STEP_MOUNT=/tmp/qt-enclave-step-mount.db \
//!   QT_FIXTURE_ENCLAVE_STEP_LLMLOGS=/tmp/qt-enclave-step-llmlogs.db \
//!     cargo test -p quilltap-harness --test enclave_step_tier3_equivalence

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::enclave::step::{
    schedule_tick, step, AutonomousRoomTurnPayload, StepDeps, TickDeps, TurnJobMeta,
};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::build_context::RealBuildContextSeams;
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::message_finalizer::{
    NoAnswerConfirmation, NoAsyncCompression, NoCostTracking,
};
use quilltap_core::services::orchestrator::{OrchestratorChatSettings, OrchestratorDeps};
use quilltap_core::services::pricing_fetcher::PricingFetcher;
use quilltap_core::tools::ask_carina::ErasedAskCarina;
use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Spec.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CallW {
    name: String,
    kind: String,
    #[serde(default)]
    chat_id: Option<String>,
    user_id: String,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    job_id: Option<String>,
    #[serde(default)]
    job_created_at: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    stream_label: Option<String>,
    anchor_ms: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatSeedW {
    id: String,
    columns: HashMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user1: String,
    #[allow(dead_code)]
    user2: String,
    chats: Vec<ChatSeedW>,
    calls: Vec<CallW>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/enclave-step-tier3.json")
}

// ---------------------------------------------------------------------------
// Oracle rows (the orchestrator-differential shapes).
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
struct CannedStreamW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    sequences: Vec<Vec<ChunkW>>,
}
#[derive(Deserialize)]
struct CannedCompletionW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    response: String,
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

// ---------------------------------------------------------------------------
// Canned providers (per-key queue — the orchestrator-harness shape).
// ---------------------------------------------------------------------------

struct QueuedStreamingProvider {
    queues: Mutex<HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>>>,
}
impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedStreamW]) -> Self {
        let mut queues: HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>> =
            HashMap::new();
        for row in rows {
            let messages = to_completion_messages(&row.messages);
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
                None => {
                    // Loud canned-miss diagnostics: the step swallows stream
                    // errors (v4's shell), so a prompt divergence would
                    // otherwise be silent.
                    eprintln!("CANNED MISS:\n{key}\n--- queued keys ---");
                    for k in queues.keys() {
                        eprintln!("{}...", &k[..k.len().min(400)]);
                    }
                    vec![Err(StreamError::new(format!(
                        "no canned stream queued for key ({provider}, model {}, {} msgs)",
                        params.model,
                        params.messages.len(),
                    )))]
                }
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

struct NoCarina;
impl quilltap_core::services::carina_runner::RunCarinaQuery for NoCarina {
    #[allow(clippy::manual_async_fn)]
    fn run(
        &mut self,
        _o: quilltap_core::services::carina_runner::RunCarinaQueryOptions,
    ) -> impl Future<
        Output = Result<
            quilltap_core::services::carina_runner::CarinaResult,
            quilltap_core::services::carina_runner::CarinaRunError,
        >,
    > + Send {
        async {
            Err(quilltap_core::services::carina_runner::CarinaRunError(
                "no carina markup in the corpus".into(),
            ))
        }
    }
}

/// The orchestrator seams mirroring the fixture's single u1 `chat_settings` row
/// (cheapLLM PROVIDER_CHEAPEST / compression off / RNG off / confirmation off /
/// danger DETECT_ONLY with the Zod defaults / agent-mode defaults).
struct HarnessOrchestratorSeams;
impl quilltap_core::services::orchestrator::OrchestratorSeams for HarnessOrchestratorSeams {
    fn chat_settings(&self, _user_id: &str) -> Option<OrchestratorChatSettings> {
        Some(OrchestratorChatSettings {
            cheap_llm_settings_present: true,
            compression_enabled: false,
            project_context_reinject_interval: 5,
            auto_detect_rng: false,
            custom_tools: true,
            answer_confirmation_global_enabled: false,
            autonomous_destructive_policy: "opt_in_per_room".to_string(),
            agent_mode_default_enabled: false,
            agent_mode_max_turns: 10,
            danger_settings: Some(quilltap_core::db::chat_settings::DangerousContentSettings {
                mode: "DETECT_ONLY".to_string(),
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
            cheap_llm_strategy: "PROVIDER_CHEAPEST".to_string(),
            cheap_llm_user_defined_profile_id: None,
            cheap_llm_default_cheap_profile_id: None,
            cheap_llm_fallback_to_local: false,
        })
    }
}

// ---------------------------------------------------------------------------
// Oracle NDJSON.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OracleLine {
    kind: String,
    #[serde(flatten)]
    rest: Value,
}

fn dump_table(db: &Db, table: &str) -> Value {
    let table = table.to_string();
    db.read_main(move |c| quilltap_core::db::dump_table_json_conn(c, &table, "id"))
        .expect("table dump")
}

fn dump_llm_logs_table(db: &Db) -> Value {
    db.read_llm_logs(|c| quilltap_core::db::dump_table_json_conn(c, "llm_logs", "id"))
        .expect("llm_logs dump")
}

// ---------------------------------------------------------------------------
// The test.
// ---------------------------------------------------------------------------

#[test]
fn enclave_step_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_ENCLAVE_STEP") else {
        eprintln!("QT_ORACLE_ENCLAVE_STEP not set; skipping");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_ENCLAVE_STEP_MAIN") else {
        eprintln!("QT_FIXTURE_ENCLAVE_STEP_MAIN not set; skipping");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_ENCLAVE_STEP_MOUNT") else {
        eprintln!("QT_FIXTURE_ENCLAVE_STEP_MOUNT not set; skipping");
        return;
    };
    let Ok(fixture_ll) = std::env::var("QT_FIXTURE_ENCLAVE_STEP_LLMLOGS") else {
        eprintln!("QT_FIXTURE_ENCLAVE_STEP_LLMLOGS not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec readable"))
            .expect("spec parses");

    // Parse the oracle NDJSON.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let mut canned_streams: Vec<CannedStreamW> = Vec::new();
    let mut canned_completions: Vec<CannedCompletionW> = Vec::new();
    let mut want_tables: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let parsed: OracleLine = serde_json::from_str(line).expect("oracle line parses");
        match parsed.kind.as_str() {
            "call" => {
                // Sanity: v4's handlers never threw over this corpus.
                let threw = parsed.rest.get("threw").cloned().unwrap_or(Value::Null);
                assert!(
                    threw.is_null(),
                    "oracle call threw: {} — {threw}",
                    parsed
                        .rest
                        .get("call")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                );
            }
            "cannedStream" => {
                canned_streams.push(serde_json::from_value(parsed.rest).expect("cannedStream"))
            }
            "cannedCompletion" => canned_completions
                .push(serde_json::from_value(parsed.rest).expect("cannedCompletion")),
            "table" => {
                let table = parsed
                    .rest
                    .get("table")
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_string();
                want_tables.insert(table, parsed.rest);
            }
            other => panic!("unknown oracle line kind: {other}"),
        }
    }

    // Copy the fixture DBs to a scratch dir.
    let scratch =
        std::env::temp_dir().join(format!("qt-enclave-step-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("main.db");
    let work_mount = scratch.join("mount.db");
    let work_ll = scratch.join("llm-logs.db");
    for (src, dst) in [
        (&fixture_main, &work_main),
        (&fixture_mount, &work_mount),
        (&fixture_ll, &work_ll),
    ] {
        let _ = std::fs::remove_file(dst);
        std::fs::copy(src, dst).expect("copy fixture");
    }

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

    // Canned providers.
    let streaming = QueuedStreamingProvider::from_oracle(&canned_streams);
    let mut completion = CannedCompletionProvider::new();
    for row in &canned_completions {
        let messages = to_completion_messages(&row.messages);
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
    let embedding = CannedEmbeddingProvider::new();
    let bc_seams = RealBuildContextSeams { db: &db };
    let router =
        quilltap_core::services::dangerous_content::provider_routing::DangerContentRouter::new(
            db.clone(),
            quilltap_core::services::dangerous_content::provider_routing::DbApiKeys(db.clone()),
        );
    let pricing = PricingFetcher::new(NoPricingFetch);

    // The deterministic uuid mint (uuid-SHAPED so the banner-content remap regex
    // catches both sides' minted ids).
    let mint_counter = Arc::new(AtomicI64::new(0));
    let mint = {
        let c = mint_counter.clone();
        move || {
            let n = c.fetch_add(1, Ordering::SeqCst);
            format!("12121212-0000-4000-8000-{n:012x}")
        }
    };

    for call in &spec.calls {
        // The per-call re-anchored +1 ms/read clock (mirrors the oracle's Date
        // mock; every clock-derived byte is placeholdered or margin-safe).
        let tick = AtomicI64::new(call.anchor_ms);
        let now_ms = move || tick.fetch_add(1, Ordering::SeqCst);

        if call.kind == "tick" {
            let deps = TickDeps {
                now_ms: &now_ms,
                mint_uuid: &mint,
                tz: "UTC",
            };
            rt.block_on(schedule_tick(&db, &deps, &call.user_id))
                .unwrap_or_else(|e| panic!("schedule_tick({}) failed: {e:?}", call.name));
            continue;
        }

        // A turn call: the run-tagged executor (the explicit LogContext replacing
        // v4's ambient runWithAutonomousRunId) + the UNtagged fold executor.
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: spec.user1.clone(),
            chat_id: call.chat_id.clone(),
            message_id: None,
            ctx: LogContext {
                autonomous_run_id: call.run_id.clone(),
            },
        });
        let fold_executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: spec.user1.clone(),
            chat_id: call.chat_id.clone(),
            message_id: None,
            ctx: LogContext::none(),
        });

        let sink = RecordingSink::new();
        let orchestrator_seams = HarnessOrchestratorSeams;
        let mut confirmation = NoAnswerConfirmation;
        let mut compression = NoAsyncCompression;
        let mut cost = NoCostTracking;
        let mut carina = NoCarina;
        let prospero_fn: fn(
            quilltap_core::services::carina_runner::ProsperoCarinaErrorArgs,
        )
            -> Result<(), quilltap_core::services::carina_runner::CarinaRunError> = |_a| Ok(());
        let mut prospero = quilltap_core::services::carina_runner::ClosureProspero(prospero_fn);
        let mut rng_bytes = quilltap_core::tools::rng::FixedBytes::new(vec![]);
        let ask_carina = ErasedAskCarina::not_available();

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
            file_bytes: &quilltap_core::services::chat_files::NotConfiguredBytes,
            image_transcoder: &quilltap_core::files::image_processing::NotConfiguredTranscoder,
            danger_router: &router,
            confirmation: &mut confirmation,
            compression: &mut compression,
            cost: &mut cost,
            carina_query: &mut carina,
            prospero: &mut prospero,
            rng_bytes: &mut rng_bytes,
        };
        let sdeps = StepDeps {
            now_ms: &now_ms,
            mint_uuid: &mint,
            tz: "UTC",
            random01: 0.0,
            fold_executor: &fold_executor,
            model_context_limit: 200_000,
            timestamp_config: None,
            timezone: Some("UTC".to_string()),
            local_offset_minutes: 0,
            provider_supports_web_search: false,
        };
        let meta = TurnJobMeta {
            job_id: call.job_id.as_deref().unwrap_or_default(),
            job_created_at: call.job_created_at.as_deref().unwrap_or_default(),
            user_id: &call.user_id,
        };
        let payload = AutonomousRoomTurnPayload {
            chat_id: call.chat_id.clone(),
            run_id: call.run_id.clone(),
        };
        let outcome = rt
            .block_on(step(&mut deps, &sdeps, &meta, &payload))
            .unwrap_or_else(|e| panic!("step({}) failed: {e:?}", call.name));
        eprintln!("step({}) -> {outcome:?}", call.name);
    }

    // --- Dump + normalize + diff -------------------------------------------
    let seeded_run_started: HashMap<String, String> = spec
        .chats
        .iter()
        .filter_map(|c| {
            c.columns
                .get("runStartedAt")
                .and_then(Value::as_str)
                .map(|v| (c.id.clone(), v.to_string()))
        })
        .collect();

    let mut got_chats = dump_table(&db, "chats");
    let mut got_msgs = dump_table(&db, "chat_messages");
    let mut got_jobs = dump_table(&db, "background_jobs");
    let mut got_logs = dump_llm_logs_table(&db);
    let mut want_chats = want_tables.remove("chats").expect("oracle chats");
    let mut want_msgs = want_tables.remove("chat_messages").expect("oracle msgs");
    let mut want_jobs = want_tables.remove("background_jobs").expect("oracle jobs");
    let mut want_logs = want_tables.remove("llm_logs").expect("oracle llm_logs");

    // Shared per-side remap: minted run ids (chats.currentRunId), then message
    // ids, then the job payload refs + announcement-content uuids.
    let mut got_ids: HashMap<String, String> = HashMap::new();
    let mut want_ids: HashMap<String, String> = HashMap::new();
    normalize_chats(&mut got_chats, &seeded_run_started, &mut got_ids);
    normalize_chats(&mut want_chats, &seeded_run_started, &mut want_ids);
    normalize_messages(&mut got_msgs, &mut got_ids);
    normalize_messages(&mut want_msgs, &mut want_ids);
    normalize_jobs(&mut got_jobs, &got_ids);
    normalize_jobs(&mut want_jobs, &want_ids);
    normalize_llm_logs(&mut got_logs, &got_ids);
    normalize_llm_logs(&mut want_logs, &want_ids);

    assert_table_eq("chats", &got_chats, &want_chats);
    assert_table_eq("chat_messages", &got_msgs, &want_msgs);
    assert_table_eq("background_jobs", &got_jobs, &want_jobs);
    assert_table_eq("llm_logs", &got_logs, &want_logs);

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
}

// ---------------------------------------------------------------------------
// Normalization.
// ---------------------------------------------------------------------------

/// Remap a (possibly minted) uuid to a first-seen token. Seeded/pinned ids are
/// identical on both sides, so they map to the same token in the same walk
/// order; minted ids map to matching tokens by position.
fn map_id(ids: &mut HashMap<String, String>, id: &str) -> String {
    let n = ids.len();
    ids.entry(id.to_string())
        .or_insert_with(|| format!("<u{n}>"))
        .clone()
}

/// Replace every uuid-shaped substring through the map (unknown → `<uuid>`).
fn remap_uuids_in_text(ids: &HashMap<String, String>, text: &str) -> String {
    // 8-4-4-4-12 hex (both sides' minted ids are uuid-shaped).
    let re = regex::Regex::new(
        r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
    )
    .unwrap();
    re.replace_all(text, |c: &regex::Captures| {
        ids.get(&c[0]).cloned().unwrap_or_else(|| "<uuid>".into())
    })
    .into_owned()
}

/// chats: rows are pinned-id; placeholder the pure-clock columns
/// (sentinel-aware for `runStartedAt` — a SEEDED value must survive untouched,
/// proving no stray write), remap minted `currentRunId`s, and keep everything
/// else EXACT (runState / runStateMessage / counters / masks / the
/// cron-computed `scheduleNextRunAt`, which is minute-aligned and therefore
/// tick-invariant).
fn normalize_chats(
    dump: &mut Value,
    seeded_run_started: &HashMap<String, String>,
    ids: &mut HashMap<String, String>,
) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows {
            let Some(obj) = row.as_object_mut() else {
                continue;
            };
            let chat_id = obj
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(Value::String(rid)) = obj.get("currentRunId") {
                let tok = map_id(ids, &rid.clone());
                obj.insert("currentRunId".into(), Value::String(tok));
            }
            // runStartedAt: sentinel-aware.
            if let Some(Value::String(s)) = obj.get("runStartedAt") {
                let seeded = seeded_run_started.get(&chat_id);
                if seeded != Some(s) {
                    obj.insert("runStartedAt".into(), Value::String("<ts>".into()));
                }
            }
            for c in [
                "updatedAt",
                "lastMessageAt",
                "runEndedAt",
                "runPausedAt",
                "scheduleLastRunAt",
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

/// chat_messages: remap uuids inside content (the run-start banner carries the
/// minted run id), remap row ids to first-seen tokens, placeholder minted
/// timestamps/token counts. Sort by (chatId, type, role, content) AFTER the
/// content remap (the createdAt orderings differ across the two clocks).
fn normalize_messages(dump: &mut Value, ids: &mut HashMap<String, String>) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            if let Some(obj) = row.as_object_mut() {
                if let Some(Value::String(content)) = obj.get("content") {
                    let mapped = remap_uuids_in_text(ids, &content.clone());
                    obj.insert("content".into(), Value::String(mapped));
                }
            }
        }
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
                r.get("context")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        });
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                if let Some(id) = obj.get("id").and_then(Value::as_str).map(str::to_string) {
                    let tok = map_id(ids, &id);
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

/// background_jobs: remap the payload's run/message ids through the shared map,
/// sort by (type, payload), placeholder ids + timestamps + attempts-volatile
/// columns. The seeded PROCESSING sibling keeps its pinned payload (its run id
/// is corpus-pinned → same token both sides).
fn normalize_jobs(dump: &mut Value, ids: &HashMap<String, String>) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            if let Some(obj) = row.as_object_mut() {
                if let Some(payload_str) = obj.get("payload").and_then(Value::as_str) {
                    if let Ok(mut payload) = serde_json::from_str::<Value>(payload_str) {
                        if let Some(pobj) = payload.as_object_mut() {
                            for f in ["runId", "turnOpenerMessageId", "extractionAnchorMessageId"] {
                                if let Some(Value::String(v)) = pobj.get(f) {
                                    let mapped = ids
                                        .get(v)
                                        .cloned()
                                        .unwrap_or_else(|| "<uuidref>".to_string());
                                    pobj.insert(f.to_string(), Value::String(mapped));
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
                r.get("status")
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

/// llm_logs: placeholder id / timestamps / durationMs (v4's ticking clock makes
/// its stream duration nonzero while the port emits 0 — a documented stream-
/// clock follow-up), remap `messageId` / `autonomousRunId` through the shared
/// map (pinned run ids + seeded rows → same tokens; minted assistant message
/// ids → matching tokens), then sort canonically.
fn normalize_llm_logs(dump: &mut Value, ids: &HashMap<String, String>) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            if let Some(obj) = row.as_object_mut() {
                obj.insert("id".into(), Value::String("<id>".into()));
                for c in ["createdAt", "updatedAt", "durationMs"] {
                    if let Some(v) = obj.get(c) {
                        if !v.is_null() {
                            obj.insert(c.to_string(), Value::String(format!("<{c}>")));
                        }
                    }
                }
                for c in ["messageId", "autonomousRunId"] {
                    if let Some(Value::String(v)) = obj.get(c) {
                        let mapped = ids
                            .get(v)
                            .cloned()
                            .unwrap_or_else(|| "<uuidref>".to_string());
                        obj.insert(c.to_string(), Value::String(mapped));
                    }
                }
            }
        }
        rows.sort_by_key(|r| serde_json::to_string(r).unwrap());
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
