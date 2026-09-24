//! Tier-3 differential: the Scenario Builder SERVICE — v4 `runScenarioBuilder`
//! (`lib/services/scenario-builder/scenario-builder.service.ts`, `d1c06cd9d`)
//! against v5's `quilltap_core::services::scenario_builder::run_scenario_builder`.
//!
//! Cloned from `brahma_console_tier3_equivalence` (P4.D216's). Both sides pin
//! the model boundaries identically — the streamed calls by the exact
//! `provider|model|temperature|messages` key (so the system prompt, the
//! Scenario Builder's tool instructions, the user message and every tool
//! result threaded into a continuation are proven by the replay) and tool
//! detection by the raw response's `marker` — and run everything else REAL:
//! the slate builder, the shared one-shot loop, `process_tool_calls`, every
//! handler over the pre-built pool, and the mount pool itself, all over the
//! same doc-opacity fixture.
//!
//! **Comparand, per case:** (1) the ordered FRAME list the run emits — the
//! loop's `toolsDetected` / `status` / `toolResult` frames, the `reasoning`
//! frames, and exactly one terminal `done` / `error` frame, or none on an
//! abort — each frame's JSON byte-exact; (2) per stream call, the NAMES of the
//! tools passed (the slate is not in the canned key — this is where `search_web`
//! appearing only when the web is available is proven); (3) the log TYPE of
//! every `llm_logs` row the real writer put down, against the types v4's
//! `streamMessage` writes at each terminal chunk — every one `SCENARIO_BUILDER`;
//! (4) the `ScenarioBuilder`, `OneShotToolLoop` and `ScenarioBuilderMountPool`
//! services' log lines, field for field. The served canned-stream count must
//! equal the oracle's. P4.114 retired the one measured divergence
//! (`doc_read_file`'s result KEY ORDER on `inworld_read_notes_then_submit`)
//! by making v5's order v4's — it VANISHED red-first, and the case now runs
//! its full canned chain like every other. P4.114 also records every canned
//! row's per-message `thoughtSignatures` and compares them on each served call:
//! `inworld_grep_thought_signature_empty_after_real` streams a real signature,
//! then `""` on the terminal chunk, and v4's loop (truthiness) keeps the real one.
//!
//! **The curl tool is absent on BOTH sides** (the jest env registers no
//! plugins; v5 builds no plugin tools), so real mode's slate agrees; the
//! corpus never asks for `curl` or executes `search_web` (a network call).
//!
//! Determinism: the oracle fakes `Date` to `nowEpochMs` in `nowTz` and answers
//! `crypto.randomUUID` with `syntheticChatId` once per chat-less case; v5 is
//! handed the same `jiff::Zoned` and id.
//!
//! ⚠ PIN REQUIRED at the TARGET `d1c06cd9d` (the service does not exist at the
//! `00c290c9a` baseline — the import fails there). Fixture MINTED — rebuild,
//! regenerate, THEN `cargo test` against the same build.
//!
//!     N=~/.nvm/versions/node/v24.13.1/bin ; W=${V5W:-$(git rev-parse --show-toplevel)}
//!     STAGE=/tmp/qt-oracle-stage-sb-tier3
//!     rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!     cp $W/harness/oracle/cases/scenario-builder-tier3.test.ts $STAGE/harness/oracle/cases/
//!     cp $W/harness/oracle/fixtures/scenario-builder-tier3.json $STAGE/harness/oracle/fixtures/
//!     cd ~/source/quilltap-server
//!     rm -f /tmp/qt-sbt3-main.db /tmp/qt-sbt3-mount.db
//!     QT_FIXTURE_DOPA_MAIN=/tmp/qt-sbt3-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-sbt3-mount.db \
//!     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts
//!     QT_FIXTURE_SBT3_MAIN=/tmp/qt-sbt3-main.db QT_FIXTURE_SBT3_MOUNT=/tmp/qt-sbt3-mount.db \
//!     QT_ORACLE_OUT=/tmp/oracle-scenario-builder-tier3.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!     --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "scenario-builder-tier3\.test\.ts$"
//!
//! Then:
//!
//!     QT_ORACLE_SBT3=/tmp/oracle-scenario-builder-tier3.ndjson \
//!     QT_FIXTURE_SBT3_MAIN=/tmp/qt-sbt3-main.db QT_FIXTURE_SBT3_MOUNT=/tmp/qt-sbt3-mount.db \
//!     cargo test -p quilltap-harness --test scenario_builder_tier3_equivalence -- --nocapture

mod scenario_builder_capture;

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamMessage, StreamParams,
    StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::native_tool_loop::ToolCallDetector;
use quilltap_core::services::scenario_builder::request_schema::ScenarioBuilderMode;
use quilltap_core::services::scenario_builder::system_prompt::zoned_at;
use quilltap_core::services::scenario_builder::{
    run_scenario_builder, RunScenarioBuilderOptions, ScenarioBuilderChat, ScenarioBuilderDeps,
    ScenarioBuilderInput, ScenarioRunOutcome,
};
use quilltap_core::services::tool_execution::ToolCall;
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use scenario_builder_capture::{normalize, v4_lines, StructuralCapture};
use serde::Deserialize;
use serde_json::Value;

const TARGETS: &[&str] = &[
    "quilltap_core::services::scenario_builder",
    "quilltap_core::services::scenario_builder::mount_pool",
    "quilltap_core::services::agent_loop::one_shot_loop",
];

// ---------------------------------------------------------------------------
// Spec.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DetectionCall {
    name: String,
    arguments: Value,
    #[serde(default)]
    call_id: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatW {
    id: String,
    #[serde(default)]
    scenario_text: Option<String>,
    #[serde(default)]
    context_summary: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    name: String,
    user_id: String,
    mode: String,
    location: String,
    time: String,
    details: String,
    project_id: Option<String>,
    character_ids: Vec<String>,
    #[serde(default)]
    chat: Option<ChatW>,
    #[serde(default)]
    prior_draft: Option<String>,
    #[serde(default)]
    revision: Option<String>,
    web_search_configured: bool,
    profile: serde_json::Map<String, Value>,
    #[serde(default)]
    abort_on_reasoning: Option<String>,
    #[serde(default)]
    abort_on_marker: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    now_epoch_ms: i64,
    now_tz: String,
    synthetic_chat_id: String,
    profile_base: serde_json::Map<String, Value>,
    detection: HashMap<String, Vec<DetectionCall>>,
    cases: Vec<CaseW>,
}

// ---------------------------------------------------------------------------
// Oracle canned streams → a stateful per-key queue (the Brahma recipe).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CannedMsgW {
    role: String,
    content: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChunkW {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    done: Option<bool>,
    #[serde(default)]
    raw_response: Option<Value>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    usage: Option<UsageW>,
    /// P4.114: a Gemini-style thought signature on the chunk (`""` included).
    #[serde(default)]
    thought_signature: Option<String>,
}

#[derive(Deserialize, Clone)]
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
    /// P4.114: every message's `thoughtSignature` as v4 threaded it (`None` =
    /// absent) — the canned KEY is role + content only.
    #[serde(default, rename = "thoughtSignatures")]
    thought_signatures: Option<Vec<Option<String>>>,
}

fn chunk_to_result(c: &ChunkW) -> StreamChunkResult {
    if let Some(e) = &c.error {
        return Err(StreamError::new(e.clone()));
    }
    if c.done == Some(true) {
        let mut chunk = StreamChunk::done(c.usage.as_ref().map(|u| StreamUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }));
        chunk.raw_response = c.raw_response.clone();
        chunk.thought_signature = c.thought_signature.clone();
        return Ok(chunk);
    }
    if let Some(rc) = &c.reasoning_content {
        return Ok(StreamChunk {
            reasoning_content: Some(rc.clone()),
            ..Default::default()
        });
    }
    let mut chunk = StreamChunk::content(c.content.clone().unwrap_or_default());
    chunk.thought_signature = c.thought_signature.clone();
    Ok(chunk)
}

/// P4.114: every message's thought signature as v5 passes it (`None` for every
/// role but an assistant turn that carries one).
fn thought_signatures(messages: &[StreamMessage]) -> Vec<Option<String>> {
    messages
        .iter()
        .map(|m| match m {
            StreamMessage::Assistant {
                thought_signature, ..
            } => thought_signature.clone(),
            _ => None,
        })
        .collect()
}

/// A queued canned sequence and the thought signatures v4 threaded into it.
type Queued = (Vec<StreamChunkResult>, Option<Vec<Option<String>>>);
type Queues = HashMap<String, VecDeque<Queued>>;

struct QueuedStreamingProvider {
    queues: Mutex<Queues>,
    /// P4.114: served calls whose messages' thought signatures differ from v4's.
    signature_mismatches: Mutex<Vec<String>>,
    /// Per served call: the tool NAMES the call carried.
    served_tools: Mutex<Vec<Vec<String>>>,
}

impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedStreamW]) -> Self {
        let mut queues: Queues = HashMap::new();
        for row in rows {
            let messages: Vec<CompletionMessage> = row
                .messages
                .iter()
                .map(|m| CompletionMessage {
                    role: CompletionRole::from_v4_wire(m.role.as_str())
                        .unwrap_or(CompletionRole::User),
                    content: m.content.clone(),
                })
                .collect();
            let key = canned_stream_key(&row.provider, &row.model, row.temperature, &messages);
            let q = queues.entry(key).or_default();
            for seq in &row.sequences {
                q.push_back((
                    seq.iter().map(chunk_to_result).collect(),
                    row.thought_signatures.clone(),
                ));
            }
        }
        Self {
            queues: Mutex::new(queues),
            signature_mismatches: Mutex::new(Vec::new()),
            served_tools: Mutex::new(Vec::new()),
        }
    }
}

fn tool_names(tools: &Option<Value>) -> Vec<String> {
    tools
        .as_ref()
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|t| {
                    t.get("name")
                        .or_else(|| t.get("function").and_then(|f| f.get("name")))
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
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
        self.served_tools
            .lock()
            .unwrap()
            .push(tool_names(&params.tools));
        let sequence: Vec<StreamChunkResult> = {
            let mut queues = self.queues.lock().unwrap();
            match queues.get_mut(&key).and_then(|q| q.pop_front()) {
                Some((seq, want_sigs)) => {
                    let got_sigs = thought_signatures(&params.messages);
                    if want_sigs.is_some_and(|w| w != got_sigs) {
                        self.signature_mismatches.lock().unwrap().push(format!(
                            "thought signatures diverge (model {}, {} msgs): v5 {got_sigs:?}",
                            params.model,
                            params.messages.len()
                        ));
                    }
                    seq
                }
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

struct MarkerDetector {
    by_marker: HashMap<String, Vec<ToolCall>>,
    abort_on: Mutex<Option<(String, Arc<AtomicBool>)>>,
}

impl ToolCallDetector for MarkerDetector {
    fn detect(&self, raw_response: &Value, _provider: &str) -> Vec<ToolCall> {
        let marker = raw_response.get("marker").and_then(Value::as_str);
        if let (Some(m), Some((trip, flag))) = (marker, self.abort_on.lock().unwrap().as_ref()) {
            if m == trip {
                flag.store(true, Ordering::SeqCst);
            }
        }
        marker
            .and_then(|m| self.by_marker.get(m))
            .cloned()
            .unwrap_or_default()
    }
}

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

#[tokio::test]
async fn scenario_builder_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SBT3") else {
        eprintln!("SKIP: set QT_ORACLE_SBT3 to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_SBT3_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_SBT3_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_SBT3_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_SBT3_MOUNT to the seed mount-index .db (see header).");
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/scenario-builder-tier3.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    assert!(!text.trim().is_empty(), "{oracle_path} is EMPTY");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    let oracle_cases: HashMap<String, Value> = rows
        .iter()
        .filter(|r| r["kind"] == "case")
        .map(|r| (r["name"].as_str().unwrap().to_string(), r.clone()))
        .collect();
    let canned: Vec<CannedStreamW> = rows
        .iter()
        .filter(|r| r["kind"] == "cannedStream")
        .map(|r| serde_json::from_value(r.clone()).expect("canned row"))
        .collect();
    assert_eq!(oracle_cases.len(), spec.cases.len(), "oracle case count");
    assert!(
        spec.cases.len() >= 10,
        "the order's corpus floor is 10 cases"
    );

    // The fixture pair + a provisioned llm-logs partition beside it.
    let scratch = tempfile::tempdir().unwrap();
    let work_main = scratch.path().join("main.db");
    let work_mount = scratch.path().join("mount.db");
    std::fs::copy(&fixture_main, &work_main).expect("copy main");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount");
    let llm_data = scratch.path().join("llm").join("data");
    std::fs::create_dir_all(&llm_data).unwrap();
    quilltap_core::services::provisioning::provision_fresh_instance(
        &llm_data,
        &spec.test_pepper_base64,
    )
    .expect("provision an llm-logs partition");
    let db = Db::open(
        DbPaths {
            main: work_main,
            mount_index: Some(work_mount),
            llm_logs: Some(llm_data.join("quilltap-llm-logs.db")),
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture copies");

    let streaming = QueuedStreamingProvider::from_oracle(&canned);
    let runner = BuiltInToolRunner::new(db.clone(), dummy_env());
    let detector = MarkerDetector {
        by_marker: spec
            .detection
            .iter()
            .map(|(k, calls)| {
                (
                    k.clone(),
                    calls
                        .iter()
                        .map(|c| ToolCall {
                            name: c.name.clone(),
                            arguments: c.arguments.clone(),
                            call_id: c.call_id.clone(),
                        })
                        .collect(),
                )
            })
            .collect(),
        abort_on: Mutex::new(None),
    };

    use tracing_subscriber::layer::SubscriberExt;
    let (layer, captured) = StructuralCapture::new(TARGETS);
    let _guard = tracing::subscriber::set_default(tracing_subscriber::registry().with(layer));

    let now = zoned_at(spec.now_epoch_ms, &spec.now_tz).expect("the fixed instant");
    let mut failures: Vec<String> = Vec::new();
    let mut logged_so_far = 0usize;
    let mut served_so_far = 0usize;
    for case in &spec.cases {
        let want = &oracle_cases[&case.name];
        captured.lock().unwrap().clear();

        let mut profile = spec.profile_base.clone();
        for (k, v) in &case.profile {
            profile.insert(k.clone(), v.clone());
        }
        let profile = Value::Object(profile);
        let input = ScenarioBuilderInput {
            mode: if case.mode == "real" {
                ScenarioBuilderMode::Real
            } else {
                ScenarioBuilderMode::InWorld
            },
            location: case.location.clone(),
            time: case.time.clone(),
            details: case.details.clone(),
            project_id: case.project_id.clone(),
            character_ids: case.character_ids.clone(),
            chat: case.chat.as_ref().map(|c| ScenarioBuilderChat {
                id: c.id.clone(),
                scenario_text: c.scenario_text.clone(),
                context_summary: c.context_summary.clone(),
            }),
            prior_draft: case.prior_draft.clone(),
            revision: case.revision.clone(),
        };

        let flag = Arc::new(AtomicBool::new(false));
        *detector.abort_on.lock().unwrap() =
            case.abort_on_marker.clone().map(|m| (m, flag.clone()));
        let frames: Mutex<Vec<Value>> = Mutex::new(Vec::new());
        let abort_on_reasoning = case.abort_on_reasoning.clone();
        let on_frame = |frame: Value| {
            if let (Some(trip), Some(r)) = (
                abort_on_reasoning.as_deref(),
                frame.get("reasoning").and_then(Value::as_str),
            ) {
                if r == trip {
                    flag.store(true, Ordering::SeqCst);
                }
            }
            frames.lock().unwrap().push(frame);
        };
        let deps = ScenarioBuilderDeps {
            db: &db,
            streaming: &streaming,
            tool_runner: &runner,
            tool_detector: &detector,
            model_supports_native_tools: true,
            provider_supports_web_search: false,
            web_search_configured: case.web_search_configured,
        };
        let outcome = run_scenario_builder(
            &deps,
            RunScenarioBuilderOptions {
                user_id: &case.user_id,
                connection_profile: &profile,
                input: &input,
                now: now.clone(),
                synthetic_chat_id: Some(spec.synthetic_chat_id.clone()),
            },
            &on_frame,
            Some(&flag),
        )
        .await;

        // (1) the frames — compared as BYTES (key order is on the SSE wire).
        let got_frames = frames.lock().unwrap().clone();
        let want_frames: Vec<Value> = want["frames"].as_array().cloned().unwrap_or_default();
        let bytes = |fs: &[Value]| fs.iter().map(Value::to_string).collect::<Vec<_>>();
        if bytes(&got_frames) != bytes(&want_frames) {
            failures.push(format!(
                "{}: FRAMES differ\n  v4: {}\n  v5: {}",
                case.name,
                serde_json::to_string(&want_frames).unwrap(),
                serde_json::to_string(&got_frames).unwrap()
            ));
        }
        // The dispatch outcome agrees with the frames (the terminal frame, or
        // none on an abort).
        let last = want_frames.last();
        let terminal = last.filter(|f| f.get("done").is_some() || f.get("error").is_some());
        match (&outcome, terminal) {
            (ScenarioRunOutcome::Done(v), Some(t)) | (ScenarioRunOutcome::Failed(v), Some(t))
                if v == t => {}
            (ScenarioRunOutcome::Aborted, None) => {}
            (o, t) => failures.push(format!(
                "{}: OUTCOME {o:?} does not answer the terminal frame {t:?}",
                case.name
            )),
        }

        // (2) the tool names per stream call.
        let served: Vec<Vec<String>> =
            streaming.served_tools.lock().unwrap()[served_so_far..].to_vec();
        served_so_far += served.len();
        let want_tools: Vec<Vec<String>> = want["streams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| serde_json::from_value(s["toolNames"].clone()).unwrap())
            .collect();
        let sorted = |v: &[Vec<String>]| -> Vec<Vec<String>> {
            v.iter()
                .map(|t| {
                    let mut t = t.clone();
                    t.sort();
                    t
                })
                .collect()
        };
        if sorted(&served) != sorted(&want_tools) {
            failures.push(format!(
                "{}: TOOL SLATES differ\n  v4: {want_tools:?}\n  v5: {served:?}",
                case.name
            ));
        }
        for s in want["streams"].as_array().unwrap() {
            assert_eq!(
                s["logType"], "SCENARIO_BUILDER",
                "{}: v4 passed a non-Scenario-Builder log type",
                case.name
            );
        }

        // (3) the llm_logs rows' types (the real writer's).
        let all_types: Vec<String> = db
            .read_llm_logs(|c| {
                let mut st = c.prepare("SELECT type FROM llm_logs ORDER BY rowid")?;
                let rows = st
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        let got_types = all_types[logged_so_far..].to_vec();
        logged_so_far = all_types.len();
        let want_types: Vec<String> =
            serde_json::from_value(want["loggedTypes"].clone()).expect("loggedTypes");
        if got_types != want_types {
            failures.push(format!(
                "{}: llm_logs types differ\n  v4: {want_types:?}\n  v5: {got_types:?}",
                case.name
            ));
        }

        // (4) the log lines.
        let got_lines = captured.lock().unwrap().clone();
        let (gl, wl) = (
            normalize(&got_lines, &["error"]),
            normalize(&v4_lines(&want["lines"]), &["error"]),
        );
        if gl != wl {
            failures.push(format!(
                "{}: LOG lines differ\n  v4: {wl:#?}\n  v5: {gl:#?}",
                case.name
            ));
        }
    }
    // A canned MISS is still a served call, so the counts agree even on a
    // diverging case.
    if served_so_far != canned.len() {
        failures.push(format!(
            "served stream count {served_so_far} != the oracle's canned-stream count {}",
            canned.len()
        ));
    }
    // P4.114: the thought signature each served call's messages carried,
    // against the one v4 threaded (`inworld_grep_thought_signature_empty_after_
    // real` is the arm — an EMPTY signature after a real one must not overwrite
    // it; v4 `one-shot-loop.ts:241` reads it by truthiness).
    failures.extend(streaming.signature_mismatches.lock().unwrap().iter().cloned());
    let signed_rows = canned
        .iter()
        .filter(|r| {
            r.thought_signatures
                .as_ref()
                .expect("every cannedStream row records `thoughtSignatures` (regenerate from THIS tree's oracle case)")
                .iter()
                .any(Option::is_some)
        })
        .count();
    if signed_rows != 1 {
        failures.push(format!(
            "{signed_rows} canned continuation(s) carry a threaded thought signature; the corpus plants exactly one"
        ));
    }
    eprintln!(
        "scenario_builder_tier3: {} cases, {served_so_far} stream calls",
        spec.cases.len()
    );
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
