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
//! equal the oracle's. One case is a MEASURED out-of-lane divergence, pinned
//! both ways (`OUT_OF_LANE_DIVERGENCES`).
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
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
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

/// Divergences this family MEASURED outside P4.D217's ownership, pinned both
/// ways (the `EXPECTED_DIVERGENCES` shape): each named case must diverge in
/// exactly the recorded way — a "VANISHED" failure when it matches v4 (remove
/// the row), a "WRONG SHAPE" failure when it diverges differently.
///
/// `doc_read_file`'s RESULT object serializes in a different KEY ORDER on v5
/// (`formattedText, path, uri, mtime, totalLines, truncated, mimeType,
/// content`) than v4's handler builds it (`formattedText, content, mimeType,
/// path, uri, mtime, totalLines, truncated`). The values are equal; the BYTES
/// differ — on the SSE `toolResult` frame and, through the threaded tool
/// message, in the continuation the model sees (so v5's next canned key misses
/// and the case ends in the "detained" frame). Pre-existing in the doc-edit
/// handlers (`tools/doc_edit/**` — P4.D216's this round; recorded for the
/// unifier in the P4.D217 lane record, not taken here).
const OUT_OF_LANE_DIVERGENCES: &[(&str, &str)] = &[(
    "inworld_read_notes_then_submit",
    "doc_read_file result key order",
)];

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
        return Ok(chunk);
    }
    if let Some(rc) = &c.reasoning_content {
        return Ok(StreamChunk {
            reasoning_content: Some(rc.clone()),
            ..Default::default()
        });
    }
    Ok(StreamChunk::content(c.content.clone().unwrap_or_default()))
}

type Queues = HashMap<String, VecDeque<Vec<StreamChunkResult>>>;

struct QueuedStreamingProvider {
    queues: Mutex<Queues>,
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
                q.push_back(seq.iter().map(chunk_to_result).collect());
            }
        }
        Self {
            queues: Mutex::new(queues),
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
    let mut exercised_divergences = 0usize;
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
        if let Some((_, what)) = OUT_OF_LANE_DIVERGENCES
            .iter()
            .find(|(name, _)| *name == case.name)
        {
            // The recorded shape: the first toolResult frame is Value-EQUAL and
            // byte-UNEQUAL, and everything after it diverges downstream.
            let pos = want_frames
                .iter()
                .position(|f| f.get("toolResult").is_some())
                .expect("the pinned case carries a toolResult frame");
            if bytes(&got_frames) == bytes(&want_frames) {
                failures.push(format!(
                    "{}: divergence VANISHED ({what}) — remove it from OUT_OF_LANE_DIVERGENCES",
                    case.name
                ));
            } else if got_frames.get(pos) != want_frames.get(pos)
                || bytes(&got_frames[pos..=pos]) == bytes(&want_frames[pos..=pos])
                || bytes(&got_frames[..pos]) != bytes(&want_frames[..pos])
            {
                failures.push(format!(
                    "{}: WRONG SHAPE for the recorded divergence ({what})\n  v4: {}\n  v5: {}",
                    case.name,
                    serde_json::to_string(&want_frames).unwrap(),
                    serde_json::to_string(&got_frames).unwrap()
                ));
            }
            // Downstream of the key-order split nothing else can agree; drain
            // this case's own records so the next case starts clean.
            served_so_far = streaming.served_tools.lock().unwrap().len();
            logged_so_far = db
                .read_llm_logs(|c| {
                    c.query_row("SELECT count(*) FROM llm_logs", [], |r| r.get::<_, i64>(0))
                        .map_err(Into::into)
                })
                .unwrap() as usize;
            exercised_divergences += 1;
            let _ = outcome;
            continue;
        }
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
    assert_eq!(
        exercised_divergences,
        OUT_OF_LANE_DIVERGENCES.len(),
        "every recorded divergence must be exercised by the corpus"
    );
    // A canned MISS is still a served call, so the counts agree even across
    // the pinned divergence.
    if served_so_far != canned.len() {
        failures.push(format!(
            "served stream count {served_so_far} != the oracle's canned-stream count {}",
            canned.len()
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
