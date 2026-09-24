//! The Scenario Builder — The Host researches and drafts a starting scene
//! (v4 `lib/services/scenario-builder/scenario-builder.service.ts`,
//! `d1c06cd9d`, P4.D217).
//!
//! v4's own header, carried: one builder run is one request → tool loop → one
//! scene. Stateless on the server: no chat row, no messages, no participant.
//! The only durable trace is the LLM log rows the loop writes, typed
//! `SCENARIO_BUILDER`. The run executes in the parent process inside the API
//! route (v5: behind the dispatch boundary, on the host's driver), streamed
//! over SSE (v5: the Event channel) — never a background job. Tool scope is
//! "what this chat could see": a pre-built mount pool from
//! [`mount_pool::resolve_scenario_builder_mount_pool`] (cast vaults, their
//! groups' stores, the project's stores, Quilltap General), never
//! `operator_surface`.
//!
//! ## The `buildTools` call — v4's 20 positionals, mapped
//!
//! | # | v4 positional            | v4 value                       | v5 [`BuildToolsInput`] field |
//! |---|--------------------------|--------------------------------|------------------------------|
//! | 1 | `connectionProfile`      | the profile                    | `provider`, `use_native_web_search`, `allow_tool_use`, `allow_web_search` (read off the row) |
//! | 2 | `imageProfileId`         | `null`                         | `image_profile_id: None` |
//! | 3 | `imageProfile`           | `null`                         | `image_provider_constraints: None` |
//! | 4 | `userId`                 | the caller                     | `build_tools`' own `user_id` argument |
//! | 5 | `projectId`              | `null` — no `project_info`; the pool carries the project tier | `project_id: None` |
//! | 6 | `requestFullContext`     | `false`                        | `request_full_context: false` |
//! | 7 | `disabledTools`          | `[]`                           | `disabled_tools: Some(&[])` (NOT `None` — that is v4's legacy "no tools" skip) |
//! | 8 | `disabledToolGroups`     | `[]`                           | `disabled_tool_groups: &[]` |
//! | 9 | `agentModeEnabled`       | `true`                         | `agent_mode_enabled: true` |
//! |10 | `isMultiCharacter`       | `false`                        | `is_multi_character: false` |
//! |11 | `helpToolsEnabled`       | `false`                        | `help_tools_enabled: false` |
//! |12 | `canDressThemselves`     | `false`                        | `can_dress_themselves: false` |
//! |13 | `canCreateOutfits`       | `false`                        | `can_create_outfits: false` |
//! |14 | `docToolsMode`           | `'read'`                       | `doc_tools_mode: DocToolsMode::Read` |
//! |15 | `askCarinaEnabled`       | `false`                        | `ask_carina_enabled: false` |
//! |16 | `includeWorkspaceTools`  | `false`                        | `include_workspace_tools: false` |
//! |17 | `excludeMemorySearch`    | `true`                         | `exclude_memory_search: true` |
//! |18 | `sqlAccess`              | `false`                        | `sql_access: false` |
//! |19 | `customToolContext`      | `null`                         | `custom_tool_context: None` |
//! |20 | `extras`                 | `{ documentsOnlySearch: true, webSearch: webAvailable, pluginToolAllowlist: real ? ['curl'] : [] }` | `extras: BuildToolsExtras { documents_only_search: true, web_search: Some(web_available), plugin_tool_allowlist: Some(…) }` |
//!
//! plus the two injected facts v5's builder takes in place of v4's runtime
//! reads (`model_supports_native_tools`, `provider_supports_web_search`). v5
//! builds NO plugin tools, so the `['curl']` allowlist admits nothing on v5 —
//! the recorded divergence ([`capabilities`]'s module header, §R.4(k)).
//!
//! ## The tool context — NO `operator_surface`
//!
//! v4 builds `{ chatId, userId, projectId, mountPool }` (no
//! `embeddingProfileId` → the user's default; no `operatorSurface`). The
//! executor refuses a context carrying BOTH the pool and the operator flag
//! (P4.D216), so [`scenario_tool_context`] building it with
//! `operator_surface: false` is what keeps that refusal from ever firing on
//! this path — pinned by `scenario_tool_context_never_carries_the_operator_flag`.
//!
//! ## The three outcomes and the seven log lines (logger `ScenarioBuilder`)
//!
//! DEBUG `Scenario Builder run starting` (the pool's four counts nested);
//! then the loop's result: `aborted` → DEBUG `Scenario Builder run aborted by
//! the client` and NOTHING enqueued; any other failure → DEBUG `Scenario
//! Builder run ended without a scene` + the "empty-handed" error frame; ok →
//! DEBUG `Scenario Builder run complete` + the hand-built done frame. A throw
//! (the loop's `Err`) → `signal.aborted` ? DEBUG `… aborted by the client
//! (during a throw)` : ERROR `Scenario Builder run failed` + the "detained"
//! error frame. (The seventh, `Resolved Scenario Builder capabilities`, is
//! [`capabilities`]'s.)

pub mod capabilities;
pub mod mount_pool;
pub mod request_schema;
pub mod system_prompt;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::tiered_mount_pool::TieredMountPool;
use crate::db::DbError;
use crate::model::stream::StreamingCompletionProvider;
use crate::services::agent_loop::one_shot_loop::{
    build_one_shot_tool_instructions, run_one_shot_tool_loop, OneShotLoopDeps, OneShotLoopResult,
    RunOneShotToolLoopOptions,
};
use crate::services::chat_events::{ChatEvent, EventSink};
use crate::services::llm_logging::log_type;
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::pseudo_tool::TextBlockEnabledToolOptions;
use crate::services::tool_build::{build_tools, BuildToolsExtras, BuildToolsInput, DocToolsMode};
use crate::services::tool_execution::{StatusContext, ToolExecutionContext, ToolRunner};

use request_schema::ScenarioBuilderMode;
use system_prompt::{
    build_scenario_builder_system_prompt, build_scenario_builder_user_message,
    ScenarioBuilderUserMessageInput,
};

/// v4 `SCENARIO_BUILDER_MAX_AGENT_TURNS` — the turn budget for one builder run
/// (a constant in v1; the spec's Deferred list).
pub const SCENARIO_BUILDER_MAX_AGENT_TURNS: i64 = 25;

/// The loop's log label (v4 `logLabel: 'Scenario Builder'`).
pub const SCENARIO_BUILDER_LOG_LABEL: &str = "Scenario Builder";

/// The "empty-handed" frame's sentence (the loop returned `ok: false`).
pub const EMPTY_HANDED: &str = "The Host returned from his enquiries empty-handed, I regret to report. Do try again, or try another model.";

/// The "detained" frame's sentence (anything THREW).
pub const DETAINED: &str = "The Host has been detained by circumstances beyond his control — the model would not answer. Do try again, or try another model.";

/// Both error frames' `errorType`.
pub const SCENARIO_BUILDER_FAILED: &str = "scenario_builder_failed";

/// v4 `ScenarioBuilderInput.chat` — the in-chat run's chat, already
/// ownership-checked by the caller.
#[derive(Debug, Clone, Default)]
pub struct ScenarioBuilderChat {
    pub id: String,
    pub scenario_text: Option<String>,
    pub context_summary: Option<String>,
}

/// v4 `ScenarioBuilderInput`.
#[derive(Debug, Clone)]
pub struct ScenarioBuilderInput {
    pub mode: ScenarioBuilderMode,
    pub location: String,
    pub time: String,
    pub details: String,
    pub project_id: Option<String>,
    /// Cast ids already vetted by the caller (readable by this user).
    pub character_ids: Vec<String>,
    pub chat: Option<ScenarioBuilderChat>,
    pub prior_draft: Option<String>,
    pub revision: Option<String>,
}

/// The model boundaries and host facts one run composes (the Brahma one-shot's
/// precedent: only the composing host can construct them).
pub struct ScenarioBuilderDeps<'a, STR, TR, TD> {
    pub db: &'a Db,
    pub streaming: &'a STR,
    pub tool_runner: &'a TR,
    pub tool_detector: &'a TD,
    /// Injected `checkModelSupportsTools(provider, model, userId)`.
    pub model_supports_native_tools: bool,
    /// Injected `provider.supportsWebSearch` (the registry's capability).
    pub provider_supports_web_search: bool,
    /// v4 `isWebSearchConfigured()` — the engine's host fact.
    pub web_search_configured: bool,
}

/// One run's options.
pub struct RunScenarioBuilderOptions<'a> {
    pub user_id: &'a str,
    /// The connection profile ROW (`provider`, `modelName`, `id`,
    /// `allowWebSearch`, … are read off it).
    pub connection_profile: &'a Value,
    pub input: &'a ScenarioBuilderInput,
    /// v4 `new Date()` rendered in the server's zone — injected.
    pub now: jiff::Zoned,
    /// v4 `input.chat?.id ?? randomUUID()` — `None` mints a v4 uuid; the
    /// tier-3 family passes a fixed id so its log lines compare.
    pub synthetic_chat_id: Option<String>,
}

/// What a run ended as — the dispatch reply's body source. `Done` / `Failed`
/// carry the terminal frame (also emitted); `Aborted` emitted NO terminal
/// frame (v4's rule: the client is gone).
#[derive(Debug, Clone, PartialEq)]
pub enum ScenarioRunOutcome {
    Done(Value),
    Failed(Value),
    Aborted,
}

/// v4 `isScenarioWebAvailable(mode, profile)` — web search reaches this run
/// only in real mode, on a profile that allows it, with a provider configured.
pub fn is_scenario_web_available(
    mode: ScenarioBuilderMode,
    profile: &Value,
    web_search_configured: bool,
) -> bool {
    mode == ScenarioBuilderMode::Real
        && profile
            .get("allowWebSearch")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && web_search_configured
}

/// The tool context v4 builds for a run: `{ chatId, userId, projectId,
/// mountPool }` — no `operatorSurface`, no `embeddingProfileId`.
pub fn scenario_tool_context(
    chat_id: &str,
    user_id: &str,
    project_id: Option<&str>,
    mount_pool: TieredMountPool,
) -> ToolExecutionContext {
    ToolExecutionContext {
        chat_id: chat_id.to_string(),
        user_id: user_id.to_string(),
        project_id: project_id.map(str::to_string),
        mount_pool: Some(mount_pool),
        operator_surface: false,
        pending_wardrobe_announcements: Arc::new(Mutex::new(std::collections::HashSet::new())),
        ..Default::default()
    }
}

/// The loop's `controller`: every [`ChatEvent`] it (and `processToolCalls`)
/// writes becomes one v4 SSE payload object, serialized with its key order
/// intact, handed to the run's frame callback.
struct FrameSink<'a, F: Fn(Value) + Sync>(&'a F);

impl<F: Fn(Value) + Sync> EventSink for FrameSink<'_, F> {
    fn emit(&self, event: ChatEvent) {
        if let Ok(v) = serde_json::to_value(&event) {
            (self.0)(v);
        }
    }
}

fn error_frame(sentence: &str, details: &str) -> Value {
    serde_json::to_value(ChatEvent::error(sentence, SCENARIO_BUILDER_FAILED, details))
        .unwrap_or(Value::Null)
}

fn s<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str)
}

/// v4 `runScenarioBuilder(opts, controller, signal)`: stream tool events to
/// `frames`, ending with a `done` frame carrying the scene or an `error` frame
/// in the Host's voice. Never fails; an abort ends quietly.
pub async fn run_scenario_builder<STR, TR, TD, F>(
    deps: &ScenarioBuilderDeps<'_, STR, TR, TD>,
    opts: RunScenarioBuilderOptions<'_>,
    frames: &F,
    signal: Option<&AtomicBool>,
) -> ScenarioRunOutcome
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    F: Fn(Value) + Sync,
{
    let RunScenarioBuilderOptions {
        user_id,
        connection_profile,
        input,
        now,
        synthetic_chat_id,
    } = opts;
    // Tools need a chat id only for scoping; nothing reads or writes a chat row
    // with a synthetic one (the read-only slate posts no Librarian notices).
    let chat_id = input
        .chat
        .as_ref()
        .map(|c| c.id.clone())
        .or(synthetic_chat_id)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let web_available =
        is_scenario_web_available(input.mode, connection_profile, deps.web_search_configured);
    let aborted = || signal.is_some_and(|s| s.load(Ordering::SeqCst));
    // v4's `safeController.enqueue`: `if (closed || signal.aborted) return`
    // (`route.ts:118-127`) — once the client has gone, nothing more is sent.
    // ONE gate for every frame this run produces: the loop's controller
    // ([`FrameSink`]), its reasoning callback (which calls `frames` directly,
    // not through the sink) and the terminal frames below all go through this
    // shadowed `frames`, so the host's publish closure needs no gate of its
    // own (P4.115 item 1).
    let frames = &|v: Value| {
        if !aborted() {
            frames(v);
        }
    };

    // v4's ONE try/catch spans everything below: a throw anywhere ends in the
    // catch's two arms. v5's throwing steps are the DB reads (the pool, the
    // slate) and the loop's `Err`.
    let run = async {
        let pool_user = user_id.to_string();
        let pool_project = input.project_id.clone();
        let pool_cast = input.character_ids.clone();
        let mount_pool = deps.db.read_main(|main| {
            deps.db.read_mount_index(|mount| {
                Ok::<_, DbError>(mount_pool::resolve_scenario_builder_mount_pool(
                    main,
                    mount,
                    &pool_user,
                    pool_project.as_deref(),
                    &pool_cast,
                ))
            })
        });
        let mount_pool = mount_pool.map_err(|e| e.to_string())?;

        let pool_summary = json!({
            "participants": mount_pool.participant_mount_point_ids.len(),
            "groups": mount_pool.group_mount_point_ids.len(),
            "projects": mount_pool.project_mount_point_ids.len(),
            "hasGlobal": mount_pool.global_mount_point_id.is_some(),
        });
        tracing::debug!(
            mode = input.mode.as_str(),
            castCount = input.character_ids.len(),
            inChat = input.chat.is_some(),
            revising = input.revision.is_some(),
            profileId = s(connection_profile, "id").unwrap_or_default(),
            provider = s(connection_profile, "provider").unwrap_or_default(),
            model = s(connection_profile, "modelName").unwrap_or_default(),
            webAvailable = web_available,
            pool = %pool_summary,
            "Scenario Builder run starting"
        );

        // The slate: search (documents/knowledge), the read-only doc_* five,
        // submit_final_response; plus search_web (and, on v4, curl) in real
        // mode. The 20 positionals are the module header's table.
        let provider = s(connection_profile, "provider").unwrap_or_default();
        let no_disabled: [String; 0] = [];
        let no_groups: [String; 0] = [];
        let allowlist: Vec<String> = match input.mode {
            ScenarioBuilderMode::Real => vec!["curl".to_string()],
            ScenarioBuilderMode::InWorld => Vec::new(),
        };
        let built = build_tools(
            deps.db,
            user_id,
            &BuildToolsInput {
                provider,
                use_native_web_search: connection_profile
                    .get("useNativeWebSearch")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                allow_tool_use: connection_profile
                    .get("allowToolUse")
                    .and_then(Value::as_bool),
                allow_web_search: connection_profile
                    .get("allowWebSearch")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                image_profile_id: None,
                image_provider_constraints: None,
                project_id: None,
                request_full_context: false,
                disabled_tools: Some(&no_disabled),
                disabled_tool_groups: &no_groups,
                agent_mode_enabled: true,
                is_multi_character: false,
                help_tools_enabled: false,
                can_dress_themselves: false,
                can_create_outfits: false,
                doc_tools_mode: DocToolsMode::Read,
                ask_carina_enabled: false,
                include_workspace_tools: false,
                exclude_memory_search: true,
                sql_access: false,
                model_supports_native_tools: deps.model_supports_native_tools,
                provider_supports_web_search: deps.provider_supports_web_search,
                custom_tool_context: None,
                extras: BuildToolsExtras {
                    documents_only_search: true,
                    web_search: Some(web_available),
                    plugin_tool_allowlist: Some(allowlist),
                },
            },
        )
        .map_err(|e| e.to_string())?;

        let tool_instructions = build_one_shot_tool_instructions(
            connection_profile,
            &built,
            &TextBlockEnabledToolOptions {
                image_generation: false,
                search: true,
                web_search: web_available,
                whisper: false,
                state: false,
                rng: false,
                project_info: false,
                help_search: false,
                help_settings: false,
                help_navigate: false,
                create_note: false,
                wardrobe_list: false,
                wardrobe_read: false,
                wardrobe_wear: false,
                wardrobe_take_off: false,
                wardrobe_create: false,
                wardrobe_update: false,
                wardrobe_archive: false,
            },
            SCENARIO_BUILDER_MAX_AGENT_TURNS,
        );

        let system_prompt = build_scenario_builder_system_prompt(
            input.mode,
            web_available,
            &tool_instructions,
            &now,
        );
        let user_message = build_scenario_builder_user_message(&ScenarioBuilderUserMessageInput {
            mode: input.mode,
            location: &input.location,
            time: &input.time,
            details: &input.details,
            current_scenario: input.chat.as_ref().and_then(|c| c.scenario_text.as_deref()),
            context_summary: input
                .chat
                .as_ref()
                .and_then(|c| c.context_summary.as_deref()),
            prior_draft: input.prior_draft.as_deref(),
            revision: input.revision.as_deref(),
        });

        let tool_context =
            scenario_tool_context(&chat_id, user_id, input.project_id.as_deref(), mount_pool);

        let sink = FrameSink(frames);
        let mut on_reasoning = |reasoning: &str| {
            if let Ok(v) = serde_json::to_value(ChatEvent::reasoning(reasoning)) {
                frames(v);
            }
        };
        let result = run_one_shot_tool_loop(
            &OneShotLoopDeps {
                db: deps.db,
                streaming: deps.streaming,
                tool_runner: deps.tool_runner,
                tool_detector: deps.tool_detector,
            },
            RunOneShotToolLoopOptions {
                user_id,
                chat_id: &chat_id,
                connection_profile,
                system_prompt: &system_prompt,
                user_message: &user_message,
                tools: &built,
                tool_context: &tool_context,
                max_agent_turns: SCENARIO_BUILDER_MAX_AGENT_TURNS,
                controller: &sink,
                signal,
                log_type: Some(log_type::SCENARIO_BUILDER),
                // `characterId: ''` — the EMPTY string, not null (§R.4(e)).
                status_context: Some(StatusContext {
                    character_name: "The Host".to_string(),
                    character_id: String::new(),
                }),
                on_reasoning: Some(&mut on_reasoning),
                log_label: Some(SCENARIO_BUILDER_LOG_LABEL),
            },
        )
        .await
        .map_err(|e| e.message)?;
        Ok::<_, String>(result)
    };

    match run.await {
        Ok(OneShotLoopResult::Failed { detail }) if detail == "aborted" => {
            tracing::debug!(chatId = %chat_id, "Scenario Builder run aborted by the client");
            ScenarioRunOutcome::Aborted
        }
        Ok(OneShotLoopResult::Failed { detail }) => {
            tracing::debug!(
                chatId = %chat_id,
                detail = %detail,
                "Scenario Builder run ended without a scene"
            );
            let frame = error_frame(EMPTY_HANDED, &detail);
            frames(frame.clone());
            ScenarioRunOutcome::Failed(frame)
        }
        Ok(OneShotLoopResult::Ok {
            answer,
            tools_executed,
            usage,
        }) => {
            let scenario = crate::jsstr::js_trim(&answer).to_string();
            let usage = json!({
                "promptTokens": usage.prompt_tokens,
                "completionTokens": usage.completion_tokens,
                "totalTokens": usage.total_tokens,
            });
            tracing::debug!(
                chatId = %chat_id,
                scenarioLength = crate::jsstr::utf16_len(&scenario),
                toolsExecuted = tools_executed,
                usage = %usage,
                "Scenario Builder run complete"
            );
            // v4's hand-built object literal — this key order IS the wire.
            let frame = json!({
                "done": true,
                "scenario": scenario,
                "provider": connection_profile.get("provider").cloned().unwrap_or(Value::Null),
                "modelName": connection_profile.get("modelName").cloned().unwrap_or(Value::Null),
                "usage": usage,
                "toolsExecuted": tools_executed,
                "webAvailable": web_available,
            });
            frames(frame.clone());
            ScenarioRunOutcome::Done(frame)
        }
        Err(detail) => {
            if aborted() {
                tracing::debug!(
                    chatId = %chat_id,
                    "Scenario Builder run aborted by the client (during a throw)"
                );
                return ScenarioRunOutcome::Aborted;
            }
            tracing::error!(chatId = %chat_id, error = %detail, "Scenario Builder run failed");
            let frame = error_frame(DETAINED, &detail);
            frames(frame.clone());
            ScenarioRunOutcome::Failed(frame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The executor refuses a context carrying BOTH a pool and the operator
    /// flag (P4.D216); the Scenario Builder's context must never set the flag,
    /// so that refusal can never fire from this path.
    #[test]
    fn scenario_tool_context_never_carries_the_operator_flag() {
        let ctx = scenario_tool_context("c", "u", Some("p"), TieredMountPool::default());
        assert!(!ctx.operator_surface);
        assert!(ctx.mount_pool.is_some());
        assert_eq!(ctx.project_id.as_deref(), Some("p"));
        assert!(ctx.embedding_profile_id.is_none());
        assert!(ctx.character_id.is_none());
    }

    // === P4.115 ===
    /// Turn 1: a raw response the [`OneToolCall`] detector reads as ONE
    /// `search` call; any later turn: a plain-text scene (never reached when
    /// the abort lands mid-tool).
    struct ToolThenScene(std::sync::Mutex<usize>);
    impl crate::model::stream::StreamingCompletionProvider for ToolThenScene {
        fn stream_message(
            &self,
            _provider: &str,
            _base_url: Option<&str>,
            _params: &crate::model::stream::StreamParams,
        ) -> impl std::future::Future<
            Output = tokio::sync::mpsc::Receiver<crate::model::stream::StreamChunkResult>,
        > + Send {
            let turn = {
                let mut n = self.0.lock().unwrap();
                *n += 1;
                *n
            };
            async move {
                use crate::model::stream::StreamChunk;
                let (tx, rx) = tokio::sync::mpsc::channel(4);
                let chunk = if turn == 1 {
                    StreamChunk {
                        raw_response: Some(json!({ "tool": "search" })),
                        done: true,
                        ..Default::default()
                    }
                } else {
                    StreamChunk {
                        content: "The quay at dusk.".to_string(),
                        done: true,
                        ..Default::default()
                    }
                };
                let _ = tx.send(Ok(chunk)).await;
                rx
            }
        }
    }

    struct OneToolCall;
    impl crate::services::native_tool_loop::ToolCallDetector for OneToolCall {
        fn detect(
            &self,
            raw: &Value,
            _provider: &str,
        ) -> Vec<crate::services::tool_execution::ToolCall> {
            if raw.get("tool").is_none() {
                return Vec::new();
            }
            vec![crate::services::tool_execution::ToolCall {
                name: "search".to_string(),
                arguments: json!({ "query": "quay" }),
                call_id: Some("call-1".to_string()),
            }]
        }
    }

    /// The client's abort lands WHILE the tool runs: the runner trips the
    /// run's token, then answers — so the loop's `tool_result` frame is the
    /// first one produced after the abort point.
    struct AbortingRunner(Arc<AtomicBool>);
    impl crate::services::tool_execution::ToolRunner for AbortingRunner {
        fn run(
            &self,
            tool_call: &crate::services::tool_execution::ToolCall,
            _ctx: &ToolExecutionContext,
        ) -> impl std::future::Future<Output = crate::services::tool_execution::ToolResult> + Send
        {
            self.0.store(true, Ordering::SeqCst);
            let name = tool_call.name.clone();
            async move {
                crate::services::tool_execution::ToolResult {
                    tool_name: name,
                    success: true,
                    result: json!({ "found": "the quay" }),
                    error: None,
                    message: None,
                    metadata: None,
                }
            }
        }
    }

    /// P4.115 item 1 — **no frame after an abort** (v4 `route.ts:118-127`:
    /// `safeController.enqueue` returns when `closed || signal.aborted`).
    /// The REAL service over a fresh provisioned instance; the abort trips
    /// mid-tool, so the loop still emits the `tool_result` frame (the loop
    /// reads the token only between turns and per chunk) — which must never
    /// reach the run's frame callback. Every frame is recorded with the
    /// token's state at the moment it was emitted.
    ///
    /// Mutation M1: remove the gate at the top of [`run_scenario_builder`] →
    /// the post-abort `tool_result` frame is published and this is RED.
    #[tokio::test]
    async fn no_frame_is_published_after_the_run_is_aborted() {
        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();

        let token = Arc::new(AtomicBool::new(false));
        let streaming = ToolThenScene(std::sync::Mutex::new(0));
        let runner = AbortingRunner(Arc::clone(&token));
        let deps = ScenarioBuilderDeps {
            db: &db,
            streaming: &streaming,
            tool_runner: &runner,
            tool_detector: &OneToolCall,
            model_supports_native_tools: true,
            provider_supports_web_search: false,
            web_search_configured: false,
        };
        let profile = json!({
            "id": "5b170000-0000-4000-8000-0000000000a1",
            "name": "Host OK",
            "provider": "OLLAMA",
            "modelName": "host-model",
            "allowToolUse": true,
        });
        let input = ScenarioBuilderInput {
            mode: ScenarioBuilderMode::InWorld,
            location: "The quay".to_string(),
            time: "dusk".to_string(),
            details: String::new(),
            project_id: None,
            character_ids: Vec::new(),
            chat: None,
            prior_draft: None,
            revision: None,
        };
        let emitted: std::sync::Mutex<Vec<(bool, Value)>> = std::sync::Mutex::new(Vec::new());
        let frames = |v: Value| {
            emitted
                .lock()
                .unwrap()
                .push((token.load(Ordering::SeqCst), v));
        };
        let outcome = run_scenario_builder(
            &deps,
            RunScenarioBuilderOptions {
                user_id: crate::api::SINGLE_USER_ID,
                connection_profile: &profile,
                input: &input,
                now: jiff::Zoned::now(),
                synthetic_chat_id: Some("c0000000-0000-4000-8000-0000000000b1".to_string()),
            },
            &frames,
            Some(&*token),
        )
        .await;

        assert!(
            matches!(outcome, ScenarioRunOutcome::Aborted),
            "the run ends aborted between turns: {outcome:?}"
        );
        let emitted = emitted.into_inner().unwrap();
        assert!(
            emitted.iter().any(|(after, _)| !after),
            "the pre-abort frames still reach the callback: {emitted:#?}"
        );
        let after: Vec<&Value> = emitted
            .iter()
            .filter(|(after, _)| *after)
            .map(|(_, v)| v)
            .collect();
        assert!(
            after.is_empty(),
            "ZERO frames may be published after the abort point; got {after:#?}"
        );
    }
    // === end P4.115 ===

    #[test]
    fn web_is_available_only_in_real_mode_with_both_facts() {
        let allow = json!({ "allowWebSearch": true });
        let deny = json!({ "allowWebSearch": false });
        let absent = json!({});
        assert!(is_scenario_web_available(
            ScenarioBuilderMode::Real,
            &allow,
            true
        ));
        assert!(!is_scenario_web_available(
            ScenarioBuilderMode::Real,
            &allow,
            false
        ));
        assert!(!is_scenario_web_available(
            ScenarioBuilderMode::Real,
            &deny,
            true
        ));
        assert!(!is_scenario_web_available(
            ScenarioBuilderMode::Real,
            &absent,
            true
        ));
        assert!(!is_scenario_web_available(
            ScenarioBuilderMode::InWorld,
            &allow,
            true
        ));
    }
}
