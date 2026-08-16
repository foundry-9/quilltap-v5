//! The Carina query engine (W4.5) — v4 `lib/services/carina/carina.service.ts`
//! (`runCarinaQuery`), the isolated reference-answer engine the markup runner,
//! the orchestrator/finalizer, and the `ask_carina` tool all drive.
//!
//! `run_carina_query` resolves a designated answerer character and produces a
//! minimal, ISOLATED reference answer. A Carina line opens when EITHER side is
//! `canBeCarina`: a Carina answerer is reachable by anyone, and a Carina-enabled
//! asker can reach anyone (even a non-answerer). The answer is built from:
//!   - the answerer's identity stack (NOT the per-turn `buildSystemPrompt`) + their
//!     default scenario + a surface-level identity card for whoever is asking,
//!   - the answerer's OWN relevant memories, recalled against the question and
//!     whispered in by the Commonplace Book (recall only — no live conversation),
//!   - prior Carina exchanges for THIS answerer in THIS chat (follow-up continuity),
//!   - the question as a user-role message.
//!
//! It runs the chat's enabled tools (minus `ask_carina`, a recursion guard) through
//! the standard detect→execute→re-stream loop, with a forced-text final turn if the
//! budget runs dry while the model is still only emitting tool calls. The answer is
//! posted as a `systemSender: 'carina'` message and surfaced live; the answerer's
//! memories of the exchange form through the dedicated `CARINA_MEMORY_EXTRACTION`
//! job enqueued after the answer posts. Failures are RETURNED (never thrown) so the
//! caller can route them through Prospero.
//!
//! ## The tier-3 seams
//!
//! The whole engine is async (it composes the async tool subsystem + streaming +
//! memory search). It is driven directly by `carina_query_tier3_equivalence`. The
//! [`RunCarinaQuery`](super::carina_runner::RunCarinaQuery) seam is itself async
//! (RPITIT `-> impl Future + Send` — the frozen constraint is its behavior +
//! argument SHAPE, not sync-ness; the sync signature was an artifact of the canned
//! test impl, and this codebase has taken the same seams async before), so a real
//! impl at the spine composition points simply awaits this function — no
//! sync↔async bridge.
//!
//! - **`RunBrahmaConsole`** — v4's `runBrahmaQuery` (the 358-line Brahma one-shot
//!   console) is an injected seam (W4.5b follow-up). The gate (`brahmaIsReachable`)
//!   and the sentinel-id post path ARE ported here; the console engine is canned.
//!   The default impl produces the `llm-failed` shape v4 yields on a console error.
//! - **`onPosted`** — v4's live-emit callback. The engine owns it: after posting it
//!   emits [`ChatEvent::carina_answer`](super::chat_events::ChatEvent) via the
//!   captured sink (the spine passes the SSE sink; a caller wanting no emit passes a
//!   no-op sink).

use std::future::Future;

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{api_keys, chats_read, connection_profiles};
use crate::model::embedding::EmbeddingProvider;
use crate::model::stream::{StreamMessage, ToolCallFunction, ToolCallPayload};
use crate::model::stream::{StreamParams, StreamingCompletionProvider};
use crate::provider_manifest::{Capability, Registry};
use crate::services::carina_runner::writer::{
    post_carina_response, PostCarinaResponseParams, PostedCarinaMessage,
};
use crate::services::carina_runner::{
    CarinaError, CarinaErrorKind, CarinaResult, CarinaRunError, RunCarinaQuery,
    RunCarinaQueryOptions,
};
use crate::services::chat_events::{ChatEvent, EventSink};
use crate::services::memory_service::{search_memories_semantic, SemanticSearchOptions};
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::tool_build::{build_tools, BuildToolsInput};
use crate::services::tool_execution::{
    create_tool_context, process_tool_calls, StatusContext, ToolRunner,
};

pub use crate::services::carina_runner::writer::build_carina_message;

/// Maximum detect→execute→re-stream iterations within a single Carina answer.
pub const MAX_TOOL_ITERATIONS: usize = 5;
/// Token budget for the answerer's recalled memories injected into the call.
const CARINA_MEMORY_TOKEN_BUDGET: i64 = 1200;
/// Upper bound on how many recalled memories to consider for the budget.
const CARINA_MEMORY_LIMIT: usize = 12;
/// Floor on memory importance for Carina recall (mirrors the per-turn head).
const CARINA_MEMORY_MIN_IMPORTANCE: f64 = 0.3;

/// Reserved `carinaMeta.answererId` for Brahma Console answers — v4
/// `BRAHMA_CARINA_ANSWERER_ID`. A fixed RFC-4122 v4 UUID chosen so it can never
/// collide with a real `character.id` (NOT the nil UUID).
pub const BRAHMA_CARINA_ANSWERER_ID: &str = "b4a4c0de-0000-4000-8000-000000000001";

/// True when a requested Carina answerer name refers to the Brahma Console — v4
/// `isBrahmaName`.
pub fn is_brahma_name(name: &str) -> bool {
    name.trim().to_lowercase() == "brahma"
}

// ===========================================================================
// The Brahma one-shot console seam (v4 runBrahmaQuery — W4.5b)
// ===========================================================================

/// v4's `runBrahmaQuery` result subset the answerer path consumes. `ok=false`
/// with `detail == Some("no-profile")` maps to the `no-profile` error; any other
/// `ok=false` (or a thrown console error) maps to `llm-failed` with the detail.
#[derive(Debug, Clone)]
pub struct BrahmaConsoleResult {
    pub ok: bool,
    /// The reference answer (only when `ok`).
    pub answer: String,
    /// `'no-profile'` or a short failure summary (only when `!ok`).
    pub detail: Option<String>,
}

/// The Brahma one-shot console — v4 `lib/services/brahma-console/one-shot.service`.
/// Its 358-line engine (Brahma system prompt + operator SQL surface) is out of
/// scope for W4.5 (tracked as W4.5b), so it is injected; the gate + sentinel-id
/// post path ARE ported. Async, generic-consumed (no boxing, the future stays
/// `Send`) — the [`EmbeddingProvider`]/[`ToolRunner`] precedent.
pub trait RunBrahmaConsole {
    fn run(
        &self,
        user_id: &str,
        chat_id: &str,
        question: &str,
    ) -> impl Future<Output = BrahmaConsoleResult> + Send;
}

/// The default Brahma console: the port has no console engine yet (W4.5b), so it
/// returns the `llm-failed` shape v4 produces on a console failure. A production
/// build without the console wired reaches this — an authorized `@Brahma:` query
/// then fails gracefully through Prospero, never a panic.
#[derive(Clone, Copy, Default)]
pub struct UnavailableBrahmaConsole;

impl RunBrahmaConsole for UnavailableBrahmaConsole {
    #[allow(clippy::manual_async_fn)]
    fn run(
        &self,
        _user_id: &str,
        _chat_id: &str,
        _question: &str,
    ) -> impl Future<Output = BrahmaConsoleResult> + Send {
        async {
            BrahmaConsoleResult {
                ok: false,
                answer: String::new(),
                detail: Some("Brahma console unavailable".to_string()),
            }
        }
    }
}

// ===========================================================================
// Deps
// ===========================================================================

/// The injected model boundaries + seams the engine composes. Borrowed for the
/// life of one query. `now_ms` is the `Date.now()` seam the memory-recall blend
/// reads (injected so the differential is deterministic).
pub struct CarinaQueryDeps<'a, EMB, STR, TR, TD, SNK, BRA>
where
    EMB: EmbeddingProvider,
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    SNK: EventSink,
    BRA: RunBrahmaConsole,
{
    pub db: &'a Db,
    pub embedding: &'a EMB,
    pub streaming: &'a STR,
    /// The tool executor boundary (v4 `executeToolCallWithContext`) driving the
    /// tool loop. `ask_carina` is filtered out of the slate, so no recursion.
    pub tool_runner: &'a TR,
    /// v4 `detectToolCallsInResponse` — reads provider-native tool calls off the
    /// raw response.
    pub tool_detector: &'a TD,
    /// The live SSE sink — the engine emits [`ChatEvent::carina_answer`] here after
    /// posting (v4's `onPosted`). A caller wanting no emit passes a no-op sink.
    pub sink: &'a SNK,
    /// The Brahma one-shot console seam (W4.5b).
    pub brahma: &'a BRA,
    /// Injected `checkModelSupportsTools(provider, model, userId)` for the resolved
    /// profile (registry-seam pattern; feeds `build_tools`). Does not affect the
    /// diffed output (tools do not enter the canned stream key).
    pub model_supports_native_tools: bool,
    /// The `Date.now()` (ms) seam for the memory-recall blend.
    pub now_ms: f64,
}

// ===========================================================================
// The RunCarinaQuery adapter (spine composition — W4.10a)
// ===========================================================================

/// The real implementor of the frozen [`RunCarinaQuery`] seam over the engine
/// [`run_carina_query`]. Holds the [`CarinaQueryDeps`] bundle and forwards each
/// `run` to the engine (which never throws — its failure is the
/// [`CarinaResult::Err`] arm — so `run` always returns `Ok(result)`). Constructed
/// at the orchestrator/finalizer composition points, replacing the test-only
/// canned seams. The seam is `&mut self` (RPITIT) while the engine is `&`-only;
/// the adapter reads its deps immutably.
pub struct RealCarinaQuery<'a, EMB, STR, TR, TD, SNK, BRA>
where
    EMB: EmbeddingProvider,
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    SNK: EventSink,
    BRA: RunBrahmaConsole,
{
    pub deps: CarinaQueryDeps<'a, EMB, STR, TR, TD, SNK, BRA>,
}

impl<'a, EMB, STR, TR, TD, SNK, BRA> RealCarinaQuery<'a, EMB, STR, TR, TD, SNK, BRA>
where
    EMB: EmbeddingProvider,
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    SNK: EventSink,
    BRA: RunBrahmaConsole,
{
    pub fn new(deps: CarinaQueryDeps<'a, EMB, STR, TR, TD, SNK, BRA>) -> Self {
        Self { deps }
    }
}

impl<EMB, STR, TR, TD, SNK, BRA> RunCarinaQuery for RealCarinaQuery<'_, EMB, STR, TR, TD, SNK, BRA>
where
    EMB: EmbeddingProvider + Sync,
    STR: StreamingCompletionProvider + Sync,
    TR: ToolRunner + Sync,
    TD: ToolCallDetector + Sync,
    SNK: EventSink + Sync,
    BRA: RunBrahmaConsole + Sync,
{
    #[allow(clippy::manual_async_fn)]
    fn run(
        &mut self,
        opts: RunCarinaQueryOptions,
    ) -> impl std::future::Future<Output = Result<CarinaResult, CarinaRunError>> + Send {
        async move { Ok(run_carina_query(&self.deps, &opts).await) }
    }
}

// ===========================================================================
// Small JSON accessors
// ===========================================================================

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}
fn b(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}
fn f(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}
fn str_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ===========================================================================
// The engine
// ===========================================================================

/// v4 `runCarinaQuery`. Resolves the answerer, builds the isolated context, runs
/// the tool loop, posts the answer, and enqueues memory extraction. Never throws:
/// failures are the [`CarinaResult::Err`] arm.
#[allow(clippy::too_many_lines)]
pub async fn run_carina_query<EMB, STR, TR, TD, SNK, BRA>(
    deps: &CarinaQueryDeps<'_, EMB, STR, TR, TD, SNK, BRA>,
    opts: &RunCarinaQueryOptions,
) -> CarinaResult
where
    EMB: EmbeddingProvider,
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    SNK: EventSink,
    BRA: RunBrahmaConsole,
{
    let db = deps.db;
    let user_id = opts.user_id.clone();
    let chat_id = opts.chat_id.clone();

    // 1. Resolve the answerer by name (case-insensitive, all matches oldest-first).
    let name = opts.character_name.clone();
    let uid = user_id.clone();
    let name_matches = db
        .read_main(|main| {
            db.read_mount_index(|mount| {
                crate::db::character_resolver::find_characters_by_name(main, mount, &uid, &name)
            })
        })
        .unwrap_or_default();

    // Prefer a Carina-enabled answerer (reachable by anyone). Only when none of the
    // name matches is itself an answerer do we consult the asker side.
    let mut answerer: Option<Value> = name_matches
        .iter()
        .find(|c| b(c, "canBeCarina") == Some(true))
        .cloned();
    if answerer.is_none() && !name_matches.is_empty() {
        let asker_opens = asker_opens_carina_line(
            db,
            &chat_id,
            opts.asker_participant_id.as_deref(),
            opts.operator_initiated,
        );
        if asker_opens {
            answerer = name_matches.first().cloned();
        }
    }

    // The Brahma Console pseudocharacter. Only when NO real character carries the
    // name "Brahma" do we treat the name as the Console. Reachability is gated to
    // the operator, user-controlled personas, and `systemTransparency` characters.
    if name_matches.is_empty() && is_brahma_name(&opts.character_name) {
        let reachable = brahma_is_reachable(
            db,
            &chat_id,
            opts.asker_participant_id.as_deref(),
            opts.operator_initiated,
        );
        if reachable {
            return answer_as_brahma(deps, opts).await;
        }
        // An unauthorized asker falls through to not-found — Brahma stays invisible.
    }

    let Some(answerer) = answerer else {
        return CarinaResult::Err {
            error: CarinaError {
                kind: CarinaErrorKind::NotFound,
                detail: None,
                character_name: Some(opts.character_name.clone()),
            },
        };
    };
    let answerer_id = s(&answerer, "id").unwrap_or_default();
    let answerer_name = s(&answerer, "name").unwrap_or_default();

    // Everything past here is v4's guarded region — on any DB/read failure we
    // return an llm-failed result rather than propagating.
    let llm_failed = |detail: String| CarinaResult::Err {
        error: CarinaError {
            kind: CarinaErrorKind::LlmFailed,
            detail: Some(detail),
            character_name: Some(answerer_name.clone()),
        },
    };

    // 2. Resolve the connection profile (custom chain) + API key.
    let Some(connection_profile) = resolve_carina_profile(db, &user_id, &answerer) else {
        return CarinaResult::Err {
            error: CarinaError {
                kind: CarinaErrorKind::NoProfile,
                detail: None,
                character_name: Some(answerer_name.clone()),
            },
        };
    };
    let provider = s(&connection_profile, "provider").unwrap_or_default();
    let base_url = s(&connection_profile, "baseUrl");
    let model_name = s(&connection_profile, "modelName").unwrap_or_default();
    let profile_id = s(&connection_profile, "id").unwrap_or_default();

    // v4 reads the api key (`findApiKeyById`) and passes it to `streamMessage`.
    // The streaming SEAM resolves keys above itself (production) / is canned (the
    // differential), so the value is not consumed here; the read is kept for
    // fidelity with v4's DB access and to surface a real key-lookup error.
    let _api_key = if let Some(api_key_id) = s(&connection_profile, "apiKeyId") {
        db.read_main(|c| api_keys::find_by_id(c, &api_key_id))
            .ok()
            .flatten()
            .map(|k| k.key_value)
            .unwrap_or_default()
    } else {
        String::new()
    };

    // 3. Load the chat for its tool slate + image profile.
    let chat_id_r = chat_id.clone();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &chat_id_r)) {
        Ok(Some(chat)) => chat,
        Ok(None) => return llm_failed("chat not found".to_string()),
        Err(e) => return llm_failed(format!("{e:?}")),
    };

    // 4. Build the minimal, isolated context.
    let scenario_text = resolve_default_scenario(&answerer);
    let sys_char = to_carina_character(&answerer);
    let mut system_prompt = crate::system_prompt::build_identity_stack(
        &crate::system_prompt::BuildIdentityStackOptions {
            character: &sys_char,
            user_character: None,
            selected_system_prompt_id: s(&answerer, "defaultSystemPromptId").as_deref(),
            scenario_text: scenario_text.as_deref(),
        },
    );
    if let Some(sc) = &scenario_text {
        system_prompt.push_str(&format!("\n\n## Scenario\n{sc}"));
    }

    // Tell the answerer who is consulting them (the surface-level card).
    let asker_card = load_asker_identity(db, &chat, opts.asker_participant_id.as_deref());
    if let Some(card) = &asker_card {
        system_prompt.push_str(&format!(
            "\n\n## Reference Query\nYou are being consulted for a quick, standalone question by the \
             individual described below. Use this to know who is addressing you, then answer them \
             directly and concisely from your own knowledge and the tools available to you. You do not \
             have the surrounding conversation in view.\n\n### Who Is Asking\n{card}"
        ));
    } else {
        system_prompt.push_str(
            "\n\n## Reference Query\nYou are being consulted for a quick, standalone question. \
             Answer it directly and concisely from your own knowledge and the tools available to you. \
             You do not have the surrounding conversation in view.",
        );
    }

    // The Commonplace Book whispers the answerer's own relevant memories.
    if let Some(memory_recall) =
        load_carina_memory_recall(deps, &answerer_id, &opts.question, &user_id, deps.now_ms).await
    {
        system_prompt.push_str(&format!("\n\n{memory_recall}"));
    }

    let prior_exchanges = load_prior_carina_exchanges(db, &chat_id, &answerer_id);
    let mut current_messages: Vec<StreamMessage> = Vec::new();
    current_messages.push(StreamMessage::system(system_prompt));
    current_messages.extend(prior_exchanges);
    current_messages.push(StreamMessage::user(opts.question.clone()));

    // 5. Build the chat's tool slate, minus `ask_carina` (recursion guard).
    let image_profile_id = s(&chat, "imageProfileId");
    let project_id = s(&chat, "projectId");
    let char_participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|p| s(p, "type").as_deref() == Some("CHARACTER"))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let is_multi_character = char_participants.len() > 1;

    let mut document_editing_enabled = false;
    if let Some(pid) = &project_id {
        let pid = pid.clone();
        if let Ok(links) = db.read_mount_index(move |c| {
            crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(c)
                .find_by_project_id(&pid)
        }) {
            document_editing_enabled = !links.is_empty();
        }
    }

    let provider_supports_web_search =
        Registry::built_in().supports_capability(&provider, Capability::WebSearch);
    let disabled_tools = str_array(&chat, "disabledTools");
    let disabled_tool_groups = str_array(&chat, "disabledToolGroups");
    let built = match build_tools(
        db,
        &user_id,
        &BuildToolsInput {
            provider: &provider,
            use_native_web_search: b(&connection_profile, "useNativeWebSearch").unwrap_or(false),
            allow_tool_use: b(&connection_profile, "allowToolUse"),
            allow_web_search: b(&connection_profile, "allowWebSearch").unwrap_or(false),
            image_profile_id: image_profile_id.as_deref(),
            image_provider_constraints: None,
            project_id: project_id.as_deref(),
            request_full_context: false,
            disabled_tools: Some(&disabled_tools),
            disabled_tool_groups: &disabled_tool_groups,
            agent_mode_enabled: false,
            is_multi_character,
            help_tools_enabled: b(&answerer, "defaultHelpToolsEnabled") == Some(true),
            can_dress_themselves: false,
            can_create_outfits: false,
            document_editing_enabled,
            ask_carina_enabled: false,
            include_workspace_tools: true,
            exclude_memory_search: false,
            sql_access: false,
            model_supports_native_tools: deps.model_supports_native_tools,
            provider_supports_web_search,
            // Carina's answerer is never offered `run_custom` (v4's `buildTools`
            // call omits `customToolContext`).
            custom_tool_context: None,
        },
    ) {
        Ok(built) => built,
        Err(e) => return llm_failed(format!("{e:?}")),
    };
    // Recursion guard: drop `ask_carina` from the answerer's own slate.
    let tools: Vec<Value> = built
        .tools
        .into_iter()
        .filter(|t| tool_name(t).as_deref() != Some("ask_carina"))
        .collect();
    let use_native_web_search = built.use_native_web_search;
    // v4 `carina.service.ts:606`: `const modelParams = (connectionProfile
    // .parameters ?? {})`, handed to `streamMessage`, which reads the request's
    // temperature / maxTokens / topP off it and forwards it as
    // `profileParameters`.
    //
    // P4.D79: v5 read `temperature` off a TOP-LEVEL `connection_profile
    // .temperature` key that connection profiles do not have — always `None`.
    // Invisible until the orchestrator tier-3 corpus gave its Primary profile a
    // real parameters bag, at which point the Carina answer stopped matching
    // v4's canned key entirely. The same class as the orchestrator's own
    // missing `modelParams` twin, found by the same corpus change.
    let model_params = connection_profile
        .get("parameters")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| json!({}));
    // P4.D83 (v4 `d89babc4`): a FIFTH site reached through v4's converted
    // function. v4's `d89babc4` names four call sites, but Carina is not one of
    // them — it hands its bag to `streamMessage`, which is itself one of the
    // four, so Carina's sampling moved with it. v5 resolves before its own
    // narrower seam, so the resolver has to be called here too; the orchestrator
    // corpus caught this the moment the sampling knobs became a comparand.
    let sampling = crate::sampling_params::resolve_sampling_params(Some(&model_params));
    let temperature = sampling.temperature;
    let max_tokens = sampling.max_tokens.map(|n| n as i64);
    let top_p = sampling.top_p;
    let profile_parameters = Some(model_params);

    // 6. Run the LLM call + tool loop server-side (no live client streaming).
    let answerer_participant_id = char_participants
        .iter()
        .find(|p| s(p, "characterId").as_deref() == Some(answerer_id.as_str()))
        .and_then(|p| s(p, "id"));
    let tool_context = create_tool_context(
        chat_id.clone(),
        user_id.clone(),
        answerer_id.clone(),
        answerer_participant_id.clone().unwrap_or_default(),
        image_profile_id.clone(),
        None,
        project_id.clone(),
        None,
        None,
    );
    let status = StatusContext {
        character_name: answerer_name.clone(),
        character_id: answerer_id.clone(),
    };

    let stream_ctx = StreamCtx {
        provider: &provider,
        base_url: base_url.as_deref(),
        model: &model_name,
        temperature,
        max_tokens,
        top_p,
        profile_parameters: profile_parameters.as_ref(),
    };

    let (mut answer, mut raw_response) = run_stream(
        deps.streaming,
        &stream_ctx,
        &current_messages,
        &tools,
        use_native_web_search,
    )
    .await;

    let mut iterations = 0usize;
    while raw_response.is_some() && iterations < MAX_TOOL_ITERATIONS {
        let raw = raw_response.clone().unwrap();
        let tool_calls = deps.tool_detector.detect(&raw, &provider);
        if tool_calls.is_empty() {
            break;
        }
        iterations += 1;

        // No-op sink for the tool loop (v4 uses a swallowing StreamController).
        let results = process_tool_calls(
            &tool_calls,
            &tool_context,
            &NoopSink,
            deps.tool_runner,
            Some(&status),
        )
        .await;

        // Assistant tool-call message (content = the accumulated prose, or
        // empty), carrying its `toolCalls` when any call has a provider id —
        // v4 carina.service.ts:663–681; the linkage rides to the wire (P4.13).
        let has_call_ids = tool_calls
            .iter()
            .any(|tc| tc.call_id.as_deref().is_some_and(|id| !id.is_empty()));
        let assistant_tool_calls: Vec<ToolCallPayload> = if has_call_ids {
            tool_calls
                .iter()
                .filter(|tc| tc.call_id.as_deref().is_some_and(|id| !id.is_empty()))
                .map(|tc| ToolCallPayload {
                    id: tc.call_id.clone().unwrap(),
                    kind: "function",
                    function: ToolCallFunction {
                        name: tc.name.clone(),
                        // v4 `JSON.stringify(tc.arguments)` (the threading
                        // module's normalizing stringify — integer-valued
                        // floats render bare, as JS does).
                        arguments: super::tool_call_threading::stringify_arguments(&tc.arguments),
                    },
                })
                .collect()
        } else {
            Vec::new()
        };
        current_messages.push(StreamMessage::Assistant {
            content: if answer.trim().is_empty() {
                String::new()
            } else {
                answer.clone()
            },
            tool_calls: assistant_tool_calls,
            reasoning_content: None,
            thought_signature: None,
            cache_control: None,
        });
        for tm in &results.tool_messages {
            match tm.call_id.as_deref().filter(|id| !id.is_empty()) {
                Some(call_id) => current_messages.push(StreamMessage::Tool {
                    call_id: call_id.to_string(),
                    name: Some(tm.tool_name.clone()),
                    content: tm.content.clone(),
                }),
                None => current_messages.push(StreamMessage::tool_result_fallback(
                    &tm.tool_name,
                    &tm.content,
                )),
            }
        }

        let next = run_stream(
            deps.streaming,
            &stream_ctx,
            &current_messages,
            &tools,
            use_native_web_search,
        )
        .await;
        answer = next.0;
        raw_response = next.1;
    }

    // Forced-text final turn — the budget ran dry while the model still emitted
    // tool calls, so stream once with NO tools offered.
    if answer.trim().is_empty() {
        if let Some(raw) = &raw_response {
            let pending = deps.tool_detector.detect(raw, &provider);
            if !pending.is_empty() {
                let next =
                    run_stream(deps.streaming, &stream_ctx, &current_messages, &[], false).await;
                answer = next.0;
                raw_response = next.1;
            }
        }
    }
    let _ = raw_response;

    let final_answer = answer.trim().to_string();
    if final_answer.is_empty() {
        return llm_failed("empty response".to_string());
    }

    // 7. Post the answer + surface it live (v4's onPosted → carinaAnswer emit).
    let posted = match post_carina_response(
        db,
        PostCarinaResponseParams {
            chat_id: chat_id.clone(),
            answer: final_answer.clone(),
            answerer_id: answerer_id.clone(),
            question: opts.question.clone(),
            participant_id: answerer_participant_id.clone(),
            whisper: opts.whisper,
            asker_participant_id: opts.asker_participant_id.clone(),
        },
    )
    .await
    {
        Some(p) => p,
        None => return llm_failed("failed to persist answer".to_string()),
    };
    emit_carina_answer(deps.sink, &posted);

    // 8. Form the answerer's SELF memories from this exchange, off the hot path.
    let _ = super::queue_service::enqueue_carina_memory_extraction(
        db,
        &user_id,
        &chat_id,
        &posted.id,
        &answerer_id,
        &profile_id,
    )
    .await;

    CarinaResult::Ok {
        answer: final_answer,
        message_id: posted.id.clone(),
        message: posted,
        answerer_id,
        answerer_name,
    }
}

// ===========================================================================
// Brahma answer path (gate + sentinel-id post; console is the seam)
// ===========================================================================

async fn answer_as_brahma<EMB, STR, TR, TD, SNK, BRA>(
    deps: &CarinaQueryDeps<'_, EMB, STR, TR, TD, SNK, BRA>,
    opts: &RunCarinaQueryOptions,
) -> CarinaResult
where
    EMB: EmbeddingProvider,
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    SNK: EventSink,
    BRA: RunBrahmaConsole,
{
    let result = deps
        .brahma
        .run(&opts.user_id, &opts.chat_id, &opts.question)
        .await;
    if !result.ok {
        if result.detail.as_deref() == Some("no-profile") {
            return CarinaResult::Err {
                error: CarinaError {
                    kind: CarinaErrorKind::NoProfile,
                    detail: None,
                    character_name: Some("Brahma".to_string()),
                },
            };
        }
        return CarinaResult::Err {
            error: CarinaError {
                kind: CarinaErrorKind::LlmFailed,
                detail: result.detail,
                character_name: Some("Brahma".to_string()),
            },
        };
    }

    let posted = match post_carina_response(
        deps.db,
        PostCarinaResponseParams {
            chat_id: opts.chat_id.clone(),
            answer: result.answer.clone(),
            answerer_id: BRAHMA_CARINA_ANSWERER_ID.to_string(),
            question: opts.question.clone(),
            participant_id: None,
            whisper: opts.whisper,
            asker_participant_id: opts.asker_participant_id.clone(),
        },
    )
    .await
    {
        Some(p) => p,
        None => {
            return CarinaResult::Err {
                error: CarinaError {
                    kind: CarinaErrorKind::LlmFailed,
                    detail: Some("failed to persist answer".to_string()),
                    character_name: Some("Brahma".to_string()),
                },
            }
        }
    };
    emit_carina_answer(deps.sink, &posted);

    CarinaResult::Ok {
        answer: result.answer,
        message_id: posted.id.clone(),
        message: posted,
        answerer_id: BRAHMA_CARINA_ANSWERER_ID.to_string(),
        answerer_name: "Brahma".to_string(),
    }
}

// ===========================================================================
// Profile resolution chain (v4 resolveCarinaProfile — NOT participant-scoped)
// ===========================================================================

/// v4 `resolveCarinaProfile`: answerer's default → instance default → first
/// profile whose provider supports native web search → `None`.
fn resolve_carina_profile(db: &Db, user_id: &str, answerer: &Value) -> Option<Value> {
    if let Some(default_id) = s(answerer, "defaultConnectionProfileId") {
        if let Ok(Some(by_char)) = db.read_main(|c| connection_profiles::find_by_id(c, &default_id))
        {
            return Some(by_char);
        }
    }
    let uid = user_id.to_string();
    if let Ok(Some(instance_default)) =
        db.read_main(move |c| connection_profiles::find_default(c, &uid))
    {
        return Some(instance_default);
    }
    let uid = user_id.to_string();
    let all = db
        .read_main(move |c| connection_profiles::find_by_user_id(c, &uid))
        .unwrap_or_default();
    let registry = Registry::built_in();
    all.into_iter().find(|p| {
        let provider = s(p, "provider").unwrap_or_default();
        registry.supports_capability(&provider, Capability::WebSearch)
    })
}

// ===========================================================================
// Reachability gates (v4 askerOpensCarinaLine / brahmaIsReachable)
// ===========================================================================

/// v4 `askerOpensCarinaLine`. The operator always opens the line; else the
/// user-controlled persona; else a `canBeCarina` character asker (read via the
/// overlay-free raw read — a broken asker vault must not sink the check).
fn asker_opens_carina_line(
    db: &Db,
    chat_id: &str,
    asker_participant_id: Option<&str>,
    operator_initiated: bool,
) -> bool {
    if operator_initiated {
        return true;
    }
    let Some(asker_pid) = asker_participant_id else {
        return false;
    };
    resolve_asker_flag(db, chat_id, asker_pid, "canBeCarina")
}

/// v4 `brahmaIsReachable`. Same gate as the ordinary line but keyed on
/// `systemTransparency` (Brahma runs at operator privilege).
fn brahma_is_reachable(
    db: &Db,
    chat_id: &str,
    asker_participant_id: Option<&str>,
    operator_initiated: bool,
) -> bool {
    if operator_initiated {
        return true;
    }
    let Some(asker_pid) = asker_participant_id else {
        return false;
    };
    resolve_asker_flag(db, chat_id, asker_pid, "systemTransparency")
}

/// Shared body of the two reachability gates: a user-controlled asker always
/// opens; else the asker character's raw `<flag>` column. `false` on any lookup
/// failure (the line stays gated / Brahma stays hidden).
fn resolve_asker_flag(db: &Db, chat_id: &str, asker_participant_id: &str, flag: &str) -> bool {
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(chat)) => chat,
        _ => return false,
    };
    let participant = chat
        .get("participants")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|p| s(p, "id").as_deref() == Some(asker_participant_id))
        });
    let Some(participant) = participant else {
        return false;
    };
    let Some(char_id) = s(participant, "characterId") else {
        return false;
    };
    if s(participant, "controlledBy").as_deref() == Some("user") {
        return true;
    }
    match db.read_main(|c| crate::db::characters_read::find_by_id_raw(c, &char_id)) {
        Ok(Some(asker)) => b(&asker, flag) == Some(true),
        _ => false,
    }
}

// ===========================================================================
// Context builders
// ===========================================================================

/// v4 `resolveDefaultScenario`: the `defaultScenarioId` scenario's content when
/// present + non-empty, else the first scenario's content, else `None`.
fn resolve_default_scenario(character: &Value) -> Option<String> {
    let scenarios = character.get("scenarios").and_then(Value::as_array)?;
    if scenarios.is_empty() {
        return None;
    }
    if let Some(default_id) = s(character, "defaultScenarioId") {
        if let Some(found) = scenarios
            .iter()
            .find(|sc| s(sc, "id").as_deref() == Some(default_id.as_str()))
        {
            if let Some(content) = s(found, "content").filter(|c| !c.is_empty()) {
                return Some(content);
            }
        }
    }
    s(&scenarios[0], "content").filter(|c| !c.is_empty())
}

/// v4 `loadAskerIdentity` → `buildPublicIdentityCard`. `None` when the asker can't
/// be resolved (no participant context / unknown participant / broken vault).
fn load_asker_identity(
    db: &Db,
    chat: &Value,
    asker_participant_id: Option<&str>,
) -> Option<String> {
    let asker_pid = asker_participant_id?;
    let participant = chat
        .get("participants")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find(|p| s(p, "id").as_deref() == Some(asker_pid)))?;
    let char_id = s(participant, "characterId")?;
    // v4 uses the OVERLAID read (`repos.characters.findById`) — the public card's
    // title/identity/description/pronouns/aliases are vault-managed fields, so the
    // overlay (main + mount) is required. `None`/error → no attribution.
    let asker = db
        .read_main(|main| {
            db.read_mount_index(|mount| {
                crate::db::characters_read::find_by_id(main, mount, &char_id)
            })
        })
        .ok()??;
    // `{{user}}` in a user-controlled asker's own text means themselves.
    let user_name = if s(participant, "controlledBy").as_deref() == Some("user") {
        s(&asker, "name")
    } else {
        None
    };
    let sys_char = to_carina_character(&asker);
    Some(crate::system_prompt::build_public_identity_card(
        &sys_char,
        user_name.as_deref(),
    ))
}

/// v4 `loadPriorCarinaExchanges`: scan `getMessages` for this answerer's prior
/// `systemSender:'carina'` / `systemKind:'carina-response'` messages and replay
/// them as alternating user(question)/assistant(answer) pairs. Empty on any error.
fn load_prior_carina_exchanges(db: &Db, chat_id: &str, answerer_id: &str) -> Vec<StreamMessage> {
    let cid = chat_id.to_string();
    let all = match db.read_main(move |c| crate::db::chats_messages_read::get_messages(c, &cid)) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut pairs = Vec::new();
    for m in &all {
        if s(m, "type").as_deref() != Some("message") {
            continue;
        }
        if s(m, "systemSender").as_deref() != Some("carina") {
            continue;
        }
        if s(m, "systemKind").as_deref() != Some("carina-response") {
            continue;
        }
        let Some(meta) = m.get("carinaMeta") else {
            continue;
        };
        if s(meta, "answererId").as_deref() != Some(answerer_id) {
            continue;
        }
        pairs.push(StreamMessage::user(s(meta, "question").unwrap_or_default()));
        pairs.push(StreamMessage::assistant(
            s(m, "content").unwrap_or_default(),
        ));
    }
    pairs
}

/// v4 `loadCarinaMemoryRecall`: recall the answerer's own relevant memories and
/// render them as a plain "you remember…" Commonplace-Book block. `None` when
/// there is nothing to recall (or the lookup fails).
async fn load_carina_memory_recall<EMB, STR, TR, TD, SNK, BRA>(
    deps: &CarinaQueryDeps<'_, EMB, STR, TR, TD, SNK, BRA>,
    character_id: &str,
    question: &str,
    user_id: &str,
    now_ms: f64,
) -> Option<String>
where
    EMB: EmbeddingProvider,
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    SNK: EventSink,
    BRA: RunBrahmaConsole,
{
    let trimmed = question.trim();
    if trimmed.is_empty() {
        return None;
    }
    let results = search_memories_semantic(
        deps.db,
        deps.embedding,
        character_id,
        trimmed,
        &SemanticSearchOptions {
            user_id: user_id.to_string(),
            embedding_profile_id: None,
            limit: Some(CARINA_MEMORY_LIMIT),
            min_score: None,
            min_importance: Some(CARINA_MEMORY_MIN_IMPORTANCE),
            source: None,
            about_character_id: None,
            apply_literal_phrase_boost: false,
            now_ms,
            ..Default::default()
        },
    )
    .await
    .ok()?;
    if results.is_empty() {
        return None;
    }
    let injector: Vec<crate::memory_injector::InjectorResult> =
        results.iter().map(injector_result_from_search).collect();
    let formatted = crate::memory_injector::format_memories_for_context(
        &injector,
        CARINA_MEMORY_TOKEN_BUDGET,
        now_ms,
    );
    if formatted.content.is_empty() || formatted.memories_used == 0 {
        return None;
    }
    Some(
        crate::services::commonplace_notifications::build_commonplace_llm_context(
            &crate::services::commonplace_notifications::CommonplaceParts {
                current_state: None,
                recap: None,
                relevant: Some(formatted.content),
                inter_char: None,
                knowledge: None,
                relevant_conversations: None,
                retrospective_recall: None,
            },
        ),
    )
}

// ===========================================================================
// Streaming helper
// ===========================================================================

struct StreamCtx<'a> {
    provider: &'a str,
    base_url: Option<&'a str>,
    model: &'a str,
    /// All three come off v4's `modelParams` bag (`streaming.service.ts:395-397`).
    temperature: Option<f64>,
    max_tokens: Option<i64>,
    top_p: Option<f64>,
    profile_parameters: Option<&'a Value>,
}

/// Accumulate one streamed LLM call into `(answer, rawResponse)` — v4's
/// `runStream`. Only the offered tool slate + native-web-search flag vary.
async fn run_stream<STR: StreamingCompletionProvider>(
    streaming: &STR,
    ctx: &StreamCtx<'_>,
    messages: &[StreamMessage],
    tools: &[Value],
    use_native_web_search: bool,
) -> (String, Option<Value>) {
    // v4: `tools.length > 0 ? tools : undefined`.
    let tools_value = if tools.is_empty() {
        None
    } else {
        Some(Value::Array(tools.to_vec()))
    };
    let params = StreamParams {
        messages: messages.to_vec(),
        model: ctx.model.to_string(),
        temperature: ctx.temperature,
        max_tokens: ctx.max_tokens,
        top_p: ctx.top_p,
        tools: tools_value,
        web_search_enabled: use_native_web_search,
        profile_parameters: ctx.profile_parameters.cloned(),
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
    };
    let mut rx = streaming
        .stream_message(ctx.provider, ctx.base_url, &params)
        .await;
    let mut answer = String::new();
    let mut raw: Option<Value> = None;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            Ok(c) => {
                answer.push_str(&c.content);
                if c.done {
                    raw = c.raw_response;
                }
            }
            Err(_) => break,
        }
    }
    (answer, raw)
}

// ===========================================================================
// Small helpers
// ===========================================================================

/// v4 `t.function?.name ?? t.name`.
fn tool_name(t: &Value) -> Option<String> {
    t.get("function")
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .or_else(|| t.get("name").and_then(Value::as_str))
        .map(str::to_string)
}

/// Build the [`crate::system_prompt::Character`] subset both the identity stack
/// and the public card read off a vault-overlaid (or raw) character row.
fn to_carina_character(c: &Value) -> crate::system_prompt::Character {
    let pronouns = c.get("pronouns").and_then(|p| {
        let subject = s(p, "subject")?;
        let object = s(p, "object")?;
        let possessive = s(p, "possessive")?;
        Some(crate::system_prompt::Pronouns {
            subject,
            object,
            possessive,
        })
    });
    let scenarios = c
        .get("scenarios")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|sc| crate::system_prompt::ScenarioEntry {
                    content: s(sc, "content").unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    let system_prompts = c
        .get("systemPrompts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|sp| crate::system_prompt::SystemPromptEntry {
                    id: s(sp, "id").unwrap_or_default(),
                    content: s(sp, "content").unwrap_or_default(),
                    is_default: b(sp, "isDefault").unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    crate::system_prompt::Character {
        name: s(c, "name").unwrap_or_default(),
        title: s(c, "title"),
        identity: s(c, "identity"),
        description: s(c, "description"),
        manifesto: s(c, "manifesto"),
        personality: s(c, "personality"),
        aliases: str_array(c, "aliases"),
        pronouns,
        scenarios,
        system_prompts,
        ..Default::default()
    }
}

/// Project a semantic-search result into the injector shape the memory formatter
/// reads (mirrors `build_context`'s private `injector_result_from_search`).
fn injector_result_from_search(
    r: &crate::services::memory_service::SemanticSearchResult,
) -> crate::memory_injector::InjectorResult {
    crate::memory_injector::InjectorResult {
        memory: injector_memory_from_json(&r.memory),
        score: r.score,
        effective_weight: Some(r.effective_weight),
        raw_weight: Some(r.raw_weight),
        recall_adjustment: None,
    }
}

pub(crate) fn injector_memory_from_json(v: &Value) -> crate::memory_injector::InjectorMemory {
    let ms = |key: &str| {
        v.get(key)
            .and_then(Value::as_str)
            .and_then(crate::clock::iso_to_ms)
            .map(|ms| ms as f64)
    };
    crate::memory_injector::InjectorMemory {
        id: s(v, "id").unwrap_or_default(),
        content: s(v, "content"),
        summary: s(v, "summary").unwrap_or_default(),
        importance: f(v, "importance").unwrap_or(0.0),
        keywords: str_array(v, "keywords"),
        about_character_id: s(v, "aboutCharacterId"),
        reinforced_importance: f(v, "reinforcedImportance"),
        created_at_ms: ms("createdAt").unwrap_or(0.0),
        last_reinforced_at_ms: ms("lastReinforcedAt"),
        last_accessed_at_ms: ms("lastAccessedAt"),
        reinforcement_count: f(v, "reinforcementCount").map(|n| n as u64),
        graph_degree: str_array(v, "relatedMemoryIds").len(),
        // Episodic spine (v4 8bf3cb5f): the declared kind + the event clock.
        kind_episodic: s(v, "kind").as_deref() == Some("episodic"),
        occurred_at_ms: crate::episodic::event_time_ms(v.get("occurredAt").and_then(Value::as_str)),
        narrative_time: s(v, "narrativeTime"),
    }
}

/// Emit the live `carinaAnswer` event (v4's `onPosted` → `encodeCarinaAnswerEvent`).
fn emit_carina_answer<SNK: EventSink>(sink: &SNK, posted: &PostedCarinaMessage) {
    sink.emit(ChatEvent::carina_answer(posted.message.clone()));
}

/// A swallowing [`EventSink`] for the tool loop (v4's no-op `StreamController`).
struct NoopSink;
impl EventSink for NoopSink {
    fn emit(&self, _event: ChatEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn brahma_name_is_case_and_whitespace_insensitive() {
        assert!(is_brahma_name("brahma"));
        assert!(is_brahma_name("  Brahma  "));
        assert!(is_brahma_name("BRAHMA"));
        assert!(!is_brahma_name("brahman"));
        assert!(!is_brahma_name("Alice"));
    }

    #[test]
    fn brahma_sentinel_is_a_valid_v4_uuid() {
        // Must parse and carry the v4 version nibble (not the nil UUID).
        assert!(uuid::Uuid::parse_str(BRAHMA_CARINA_ANSWERER_ID).is_ok());
        assert_ne!(BRAHMA_CARINA_ANSWERER_ID, uuid::Uuid::nil().to_string());
    }

    #[test]
    fn default_scenario_prefers_default_id_then_first_then_none() {
        // No scenarios → None.
        assert_eq!(resolve_default_scenario(&json!({})), None);
        // Default id match with content wins.
        let c = json!({
            "defaultScenarioId": "s2",
            "scenarios": [
                { "id": "s1", "content": "first" },
                { "id": "s2", "content": "chosen" },
            ],
        });
        assert_eq!(resolve_default_scenario(&c).as_deref(), Some("chosen"));
        // Default id points at empty content → fall through to first non-empty.
        let c = json!({
            "defaultScenarioId": "s2",
            "scenarios": [
                { "id": "s1", "content": "first" },
                { "id": "s2", "content": "" },
            ],
        });
        assert_eq!(resolve_default_scenario(&c).as_deref(), Some("first"));
        // No defaultScenarioId → first scenario's content.
        let c = json!({ "scenarios": [{ "id": "s1", "content": "only" }] });
        assert_eq!(resolve_default_scenario(&c).as_deref(), Some("only"));
        // First scenario empty content → None.
        let c = json!({ "scenarios": [{ "id": "s1", "content": "" }] });
        assert_eq!(resolve_default_scenario(&c), None);
    }

    #[test]
    fn to_carina_character_projects_vault_fields() {
        let c = json!({
            "name": "Alice",
            "title": "the rival",
            "identity": "a stranger's view",
            "description": "an acquaintance's view",
            "personality": "self-knowledge",
            "manifesto": "core",
            "aliases": ["Al", "Ali"],
            "pronouns": { "subject": "she", "object": "her", "possessive": "hers" },
            "scenarios": [{ "id": "s1", "content": "sc" }],
            "systemPrompts": [{ "id": "p1", "content": "sp", "isDefault": true }],
        });
        let ch = to_carina_character(&c);
        assert_eq!(ch.name, "Alice");
        assert_eq!(ch.title.as_deref(), Some("the rival"));
        assert_eq!(ch.aliases, vec!["Al".to_string(), "Ali".to_string()]);
        let pr = ch.pronouns.expect("pronouns");
        assert_eq!(
            (
                pr.subject.as_str(),
                pr.object.as_str(),
                pr.possessive.as_str()
            ),
            ("she", "her", "hers")
        );
        assert_eq!(ch.scenarios.len(), 1);
        assert_eq!(ch.system_prompts.len(), 1);
        assert!(ch.system_prompts[0].is_default);
    }

    #[test]
    fn tool_name_reads_function_then_bare() {
        assert_eq!(
            tool_name(&json!({ "function": { "name": "ask_carina" } })).as_deref(),
            Some("ask_carina")
        );
        assert_eq!(
            tool_name(&json!({ "name": "search" })).as_deref(),
            Some("search")
        );
        assert_eq!(tool_name(&json!({})), None);
    }
}
