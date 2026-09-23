//! The Brahma one-shot console (W4.5b) — v4
//! `lib/services/brahma-console/one-shot.service.ts` (`runBrahmaQuery`), the
//! isolated operator console the Carina engine invokes when the answerer is
//! Brahma. It runs a single ISOLATED query and returns the final answer text:
//!
//!  - **No chat history.** The conversation slate is exactly `[system,
//!    user(question)]` — loading the Salon transcript would leak every other
//!    participant's content into Brahma and break Carina's isolation contract.
//!  - **No persistence.** Tool calls execute (their own side effects stand), but
//!    the per-iteration assistant / TOOL messages are never written to the Salon,
//!    no tokens are tracked, and no done/error SSE events are emitted. The answer
//!    is accumulated in memory and returned.
//!  - **Operator surface.** Tools run with `operator_surface = true`, unlocking
//!    `run_sql` and all-store document access. The caller (`run_carina_query`)
//!    gates reachability to the operator, user-controlled personas, and
//!    `systemTransparency` characters BEFORE calling here.
//!
//! It closes the [`RunBrahmaConsole`](super::carina_query::RunBrahmaConsole) seam
//! W4.5 left injected. [`RealBrahmaConsole`] is the production impl of the frozen
//! trait; the Carina engine's `answer_as_brahma` maps `detail == "no-profile"` to
//! the no-profile error and anything else (or an internal failure) to `llm-failed`
//! — so `run_brahma_query` NEVER throws, returning every failure as a `detail`.
//!
//! **Since v4 `d1c06cd9d` the loop itself is shared** — `run_brahma_query` is
//! a thin wrapper over
//! [`run_one_shot_tool_loop`](crate::services::agent_loop::one_shot_loop::run_one_shot_tool_loop)
//! (label `Brahma one-shot`, the default `CHAT_MESSAGE` log type, a
//! [`NoopSink`](crate::services::agent_loop::one_shot_loop::NoopSink)
//! controller, no abort signal), exactly as v4's `runBrahmaQuery` became a
//! caller of `runOneShotToolLoop`. The duplicate-call signature moved to its v4
//! home, the streaming orchestrator.
//!
//! ## The seams
//!
//! Generic-consumed over the streaming provider / tool runner / tool detector (no
//! boxing; the future stays `Send` — the [`EmbeddingProvider`]/[`ToolRunner`]
//! precedent). The `search` tool's embedding rides the tool runner's own
//! `ErasedEmbeddingProvider`, so the console needs no separate embedding dep; the
//! api-key step reads `api_keys` directly (v4's UNSCOPED `findApiKeyById`), so no
//! resolver seam is needed (both differential sides read the same fixture; the
//! plaintext key value never reaches the diffed surface — the stream is canned).

pub mod orchestrator;
pub mod prompt_text;
pub mod turn_budget;

use std::future::Future;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{connection_profiles, DbError};
use crate::jsstr::js_trim;
use crate::model::stream::StreamingCompletionProvider;
use crate::provider_manifest::Registry;
use crate::services::agent_loop::one_shot_loop::{
    build_one_shot_tool_instructions, run_one_shot_tool_loop, NoopSink, OneShotLoopDeps,
    OneShotLoopResult, RunOneShotToolLoopOptions,
};
use crate::services::api_key_service::{self, ProfileApiKeyFailure, ProfileApiKeyResolution};
use crate::services::carina_query::{BrahmaConsoleResult, RunBrahmaConsole};
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::pseudo_tool::TextBlockEnabledToolOptions;
use crate::services::tool_build::{build_tools, BuildToolsExtras, BuildToolsInput, DocToolsMode};
use crate::services::tool_execution::{StatusContext, ToolExecutionContext};

use prompt_text::{BRAHMA_BASE_BRIEF, BRAHMA_SQL_PROMPT};

// ===========================================================================
// Small JSON accessors (private, mirrors carina_query's).
// ===========================================================================

pub(super) fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}
pub(super) fn b(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}

// ===========================================================================
// The two ported helpers (v4 orchestrator.service.ts — the SEPARATE Phase-4
// streaming console; only these two are imported by the one-shot engine).
// ===========================================================================

/// v4 `resolveBrahmaConnectionProfile(repos, userId, consoleConnectionProfileId)`.
/// The one-shot engine always passes `null` for the console profile, so this
/// collapses to the user's default profile (there is no per-chat console profile
/// when Brahma is consulted from a Salon). Returns `None` when no default resolves.
pub fn resolve_brahma_connection_profile(
    db: &Db,
    user_id: &str,
    console_connection_profile_id: Option<&str>,
) -> Option<Value> {
    if let Some(pinned_id) = console_connection_profile_id.filter(|s| !s.is_empty()) {
        let pid = pinned_id.to_string();
        if let Ok(Some(pinned)) = db.read_main(move |c| connection_profiles::find_by_id(c, &pid)) {
            // v4 requires the pinned profile to be owned by the user.
            if s(&pinned, "userId").as_deref() == Some(user_id) {
                return Some(pinned);
            }
        }
    }
    let uid = user_id.to_string();
    db.read_main(move |c| connection_profiles::find_default(c, &uid))
        .ok()
        .flatten()
}

// ===========================================================================
// The system prompt (v4 buildBrahmaSystemPrompt).
// ===========================================================================

/// v4 `buildBrahmaSystemPrompt`: `[base brief, (SQL section), (tool instructions)]
/// .join("\n\n").trim()`. The one-shot console always passes `include_sql_access =
/// true` and a non-empty `tool_instructions` (agent-mode instructions are always
/// appended), so both sections are present.
pub fn build_brahma_system_prompt(tool_instructions: &str, include_sql_access: bool) -> String {
    let mut parts: Vec<&str> = vec![BRAHMA_BASE_BRIEF];
    if include_sql_access {
        parts.push(BRAHMA_SQL_PROMPT);
    }
    if !tool_instructions.is_empty() {
        parts.push(tool_instructions);
    }
    js_trim(&parts.join("\n\n")).to_string()
}

// ===========================================================================
// The injected deps + the engine.
// ===========================================================================

/// The model boundaries + seams the console composes. Borrowed for the life of one
/// query.
pub struct BrahmaQueryDeps<'a, STR, TR, TD>
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
{
    pub db: &'a Db,
    /// The streaming model boundary (v4 `streamMessage`).
    pub streaming: &'a STR,
    /// The tool executor boundary (v4 `executeToolCallWithContext` + every
    /// handler). The Brahma slate has NO `ask_carina`, so no recursion.
    pub tool_runner: &'a TR,
    /// v4 `detectToolCallsInResponse` — reads provider-native tool calls off the
    /// raw response.
    pub tool_detector: &'a TD,
    /// Injected `checkModelSupportsTools(provider, model, userId)` for the resolved
    /// profile (registry-seam pattern; feeds `build_tools` + the tool-mode gate).
    pub model_supports_native_tools: bool,
}

use crate::services::tool_execution::ToolRunner;

fn fail(detail: impl Into<String>) -> BrahmaConsoleResult {
    BrahmaConsoleResult {
        ok: false,
        answer: String::new(),
        detail: Some(detail.into()),
    }
}

/// Run an isolated Brahma Console query and return the final answer text (v4
/// `runBrahmaQuery`). NEVER errors out of the function — every failure is a
/// `BrahmaConsoleResult { ok: false, detail }`.
///
/// Since v4 `d1c06cd9d` the in-memory agent loop itself lives in
/// [`run_one_shot_tool_loop`] (shared with the Scenario Builder); this function
/// supplies the Brahma profile, slate, prompt and scope, and maps the loop's
/// result onto the unchanged [`BrahmaConsoleResult`].
pub async fn run_brahma_query<STR, TR, TD>(
    deps: &BrahmaQueryDeps<'_, STR, TR, TD>,
    user_id: &str,
    chat_id: &str,
    question: &str,
) -> BrahmaConsoleResult
where
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
{
    // 1. Profile (model): the user's default — no per-chat console profile from a
    //    Salon.
    let Some(profile) = resolve_brahma_connection_profile(deps.db, user_id, None) else {
        tracing::debug!(chatId = %chat_id, "No connection profile resolvable for Brahma query");
        return fail("no-profile");
    };
    let provider = s(&profile, "provider").unwrap_or_default();

    // 2. API key: v4's `resolveConnectionProfileApiKey` (bug 81) — required where
    //    required, forwarded where merely accepted, and loud on a dangling id
    //    even there. UNSCOPED `findApiKeyById`, as v4's is. The resolved plaintext
    //    is off the diffed surface (the stream is canned) and unused here, as it
    //    was before: the host streaming provider resolves keys internally.
    //    ⚠ v4's sentences here are LOWER-CASE where the orchestrator's are not;
    //    the difference is pre-existing and deliberate — ported verbatim.
    let key_id = s(&profile, "apiKeyId");
    let provider_for_key = provider.clone();
    let resolution = match deps.db.read_main(move |c| {
        Ok::<_, DbError>(api_key_service::resolve_connection_profile_api_key(
            c,
            &provider_for_key,
            key_id.as_deref(),
        ))
    }) {
        Ok(r) => r,
        Err(_) => ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::ApiKeyNotFound),
    };
    if let ProfileApiKeyResolution::Failed(reason) = resolution {
        // v4 `0506517d3` correction (d): the sentence comes from the shared
        // `describeProfileApiKeyFailure`, which is how the one-shot's lowercase
        // "no API key configured…" became the orchestrator's capitalised form.
        return fail(reason.describe());
    }

    // 3. Tools — identical to the standalone console: agent mode, doc
    //    read/write, the read-only run_sql tool, search-without-memories; NO
    //    ask_carina (recursion guard), NO workspace tools.
    let provider_supports_web_search = Registry::built_in()
        .supports_capability(&provider, crate::provider_manifest::Capability::WebSearch);
    let no_disabled: [String; 0] = [];
    let no_groups: [String; 0] = [];
    let built = match build_tools(
        deps.db,
        user_id,
        &BuildToolsInput {
            provider: &provider,
            use_native_web_search: b(&profile, "useNativeWebSearch").unwrap_or(false),
            allow_tool_use: b(&profile, "allowToolUse"),
            allow_web_search: b(&profile, "allowWebSearch").unwrap_or(false),
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
            // v4 `d1c06cd9d`: `'full'` for the old `true`.
            doc_tools_mode: DocToolsMode::Full,
            extras: BuildToolsExtras::default(),
            ask_carina_enabled: false,
            include_workspace_tools: false,
            exclude_memory_search: true,
            sql_access: true,
            model_supports_native_tools: deps.model_supports_native_tools,
            provider_supports_web_search,
            // The character-less Brahma Console never offers `run_custom`.
            custom_tool_context: None,
        },
    ) {
        Ok(built) => built,
        Err(e) => return fail(format!("{e:?}")),
    };

    // 4. Operator-set turn budget (Settings → Chat → Brahma Console); shared
    //    with the streaming orchestrator. The loop's stuck-loop guard is
    //    independent of it.
    let max_agent_turns = turn_budget::resolve_brahma_max_agent_turns(deps.db);
    let tool_instructions = build_one_shot_tool_instructions(
        &profile,
        &built,
        &TextBlockEnabledToolOptions {
            image_generation: false,
            search: true,
            web_search: b(&profile, "allowWebSearch").unwrap_or(false),
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
        max_agent_turns,
    );

    let system_prompt = build_brahma_system_prompt(&tool_instructions, true);

    // 5. Operator surface (character-less, all-stores). Tool side effects (SQL
    //    reads, doc writes) stand; the result MESSAGES are threaded in-memory
    //    only and never persisted to the Salon.
    let tool_context = ToolExecutionContext {
        chat_id: chat_id.to_string(),
        user_id: user_id.to_string(),
        operator_surface: true,
        pending_wardrobe_announcements: Arc::new(Mutex::new(std::collections::HashSet::new())),
        ..Default::default()
    };

    // 6. ISOLATION: the loop's slate is system + the single question only —
    //    never the Salon transcript. No controller: nothing is surfaced live.
    let result = run_one_shot_tool_loop(
        &OneShotLoopDeps {
            db: deps.db,
            streaming: deps.streaming,
            tool_runner: deps.tool_runner,
            tool_detector: deps.tool_detector,
        },
        RunOneShotToolLoopOptions {
            user_id,
            chat_id,
            connection_profile: &profile,
            system_prompt: &system_prompt,
            user_message: question,
            tools: &built,
            tool_context: &tool_context,
            max_agent_turns,
            controller: &NoopSink,
            signal: None,
            log_type: None,
            status_context: Some(StatusContext {
                character_name: "Brahma Console".to_string(),
                character_id: String::new(),
            }),
            on_reasoning: None,
            log_label: Some("Brahma one-shot"),
        },
    )
    .await;

    match result {
        Ok(OneShotLoopResult::Ok { answer, .. }) => BrahmaConsoleResult {
            ok: true,
            answer,
            detail: None,
        },
        Ok(OneShotLoopResult::Failed { detail }) => fail(detail),
        // v4's `for await` propagates a mid-stream throw out of `runBrahmaQuery`
        // to the caller (`answerAsBrahma`'s own try/catch, which never persists
        // and converts it to `{ok: false, error: {kind: 'llm-failed', ...}}`) —
        // mapped onto this engine's "never throws" idiom.
        Err(e) => fail(e.message),
    }
}

/// A role+content-only [`ThreadedMessage`] — the one-shot loop's helper,
/// re-exported for the streaming orchestrator (which builds its slate the same
/// way).
pub(crate) use crate::services::agent_loop::one_shot_loop::plain_message;

// ===========================================================================
// The production impl of the frozen RunBrahmaConsole seam.
// ===========================================================================

/// The production Brahma console — holds the model boundaries + seams and
/// satisfies the frozen [`RunBrahmaConsole`] trait by running [`run_brahma_query`]
/// per call. Generic-consumed (no boxing); construct it at the Carina/spine
/// composition point and inject it where W4.5 injects `UnavailableBrahmaConsole`.
pub struct RealBrahmaConsole<STR, TR, TD> {
    db: Db,
    streaming: STR,
    tool_runner: TR,
    tool_detector: TD,
    model_supports_native_tools: bool,
}

impl<STR, TR, TD> RealBrahmaConsole<STR, TR, TD> {
    pub fn new(
        db: Db,
        streaming: STR,
        tool_runner: TR,
        tool_detector: TD,
        model_supports_native_tools: bool,
    ) -> Self {
        Self {
            db,
            streaming,
            tool_runner,
            tool_detector,
            model_supports_native_tools,
        }
    }
}

impl<STR, TR, TD> RunBrahmaConsole for RealBrahmaConsole<STR, TR, TD>
where
    STR: StreamingCompletionProvider + Sync,
    TR: ToolRunner + Sync,
    TD: ToolCallDetector + Sync,
{
    fn run(
        &self,
        user_id: &str,
        chat_id: &str,
        question: &str,
    ) -> impl Future<Output = BrahmaConsoleResult> + Send {
        let deps = BrahmaQueryDeps {
            db: &self.db,
            streaming: &self.streaming,
            tool_runner: &self.tool_runner,
            tool_detector: &self.tool_detector,
            model_supports_native_tools: self.model_supports_native_tools,
        };
        async move { run_brahma_query(&deps, user_id, chat_id, question).await }
    }
}

#[cfg(test)]
mod tests;
