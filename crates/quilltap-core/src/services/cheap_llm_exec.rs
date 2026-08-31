//! The shared cheap-LLM execution pipeline (v4
//! `lib/memory/cheap-llm-tasks/core-execution.ts`): temperature handling with
//! the session-level no-custom-temperature cache, the uncensored-provider
//! retry on empty responses, and the parse-and-wrap result shape. Generic over
//! [`CompletionProvider`] — the tier-3 seam sits at the provider call, exactly
//! where the v4 oracle mocks `createLLMProvider`'s returned provider.
//!
//! ## Deferred (tracked, out of scope for this unit)
//!
//!   - **API-key acquisition** (`getApiKeyForCheapLLMSelection`) — resolves
//!     before the provider call in v4 and throws when absent. Key management is
//!     host-side (the canned responder needs none; the real adapter arrives
//!     with the Phase-4 transport work), so the port's boundary starts at the
//!     provider call.
//!
//! ## `logLLMCall` (W4.7e3, wired)
//!
//! v4 fire-and-forgets a row into the llm-logs DB inside `sendToProvider` after
//! every successful provider call (error swallowed). That is now ported: the
//! executor carries an optional [`CheapLlmLogConfig`] (the `Db` handle + the
//! per-service `userId`/`chatId`/`messageId` + the ambient [`LogContext`]) —
//! `None` on the request/spine path until the spine owner wires it (the
//! `cheap_llm_selection: None` precedent). When present, each successful
//! provider call writes one `llm_logs` row via [`log_llm_call`], the log type
//! resolved from the per-call `task_type` through [`map_task_type_to_log_type`].
//! The writer never throws, so the port awaits (the watermark precedent) rather
//! than v4's fire-and-forget `.catch`; the DB effect is identical.
//!
//! **A FAILED provider call also writes a row** — a deliberate divergence from
//! v4, ruled 2026-07-23 (P4.13 unit 6): see `log_failed_call`.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::cheap_llm::{
    build_character_cache_key, profile_params, CheapLlmSelection, UncensoredFallbackOptions,
};
use crate::db::runtime::Db;
use crate::jsstr::js_trim;
use crate::model::completion::{
    CompletionError, CompletionMessage, CompletionParams, CompletionProvider, CompletionResponse,
    CompletionUsage,
};
use crate::services::activity_kinds::ActivityKind;
use crate::services::llm_logging::{
    log_llm_call, map_task_type_to_log_type, LogContext, LogLlmCallParams, LogRequest,
    LogRequestMessage, LogResponse, LogUsage,
};

/// Result of a cheap LLM task (v4 `CheapLLMTaskResult<T>`).
#[derive(Clone, Debug, PartialEq)]
pub struct CheapLlmTaskResult<T> {
    pub success: bool,
    pub result: Option<T>,
    pub error: Option<String>,
    pub usage: Option<CompletionUsage>,
}

impl<T> CheapLlmTaskResult<T> {
    /// v4's early-return shape: `{ success: true, result, usage: undefined }`.
    pub fn early(result: T) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            usage: None,
        }
    }
}

/// v4 `shouldAttemptUncensoredFallback`: whether an empty response should be
/// retried on the configured uncensored provider, and with which selection.
/// Only in `AUTO_ROUTE` mode with an uncensored text profile configured; a
/// dangerous-compatible current profile suppresses it unless the chat itself
/// is dangerous (uncensored→uncensored fallback allowed there).
fn should_attempt_uncensored_fallback(
    response_content: &str,
    current_selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
) -> Option<CheapLlmSelection> {
    if !js_trim(response_content).is_empty() {
        return None;
    }
    let options = uncensored_fallback?;
    let danger = options.danger_settings;

    if danger.mode != "AUTO_ROUTE" {
        return None;
    }
    let uncensored_id = danger
        .uncensored_text_profile_id
        .as_deref()
        .filter(|s| !s.is_empty())?;

    let current_profile = current_selection
        .connection_profile_id
        .as_deref()
        .and_then(|id| options.available_profiles.iter().find(|p| p.id == id));
    if current_profile.map(|p| p.is_dangerous_compatible) == Some(true)
        && options.is_dangerous_chat != Some(true)
    {
        return None;
    }

    let uncensored_profile = options
        .available_profiles
        .iter()
        .find(|p| p.id == uncensored_id)?;

    Some(CheapLlmSelection {
        provider: uncensored_profile.provider.clone(),
        model_name: uncensored_profile.model_name.clone(),
        base_url: uncensored_profile
            .base_url
            .clone()
            .filter(|s| !s.is_empty()),
        connection_profile_id: Some(uncensored_profile.id.clone()),
        // v4 hardcodes `isLocal: false` on this path.
        is_local: false,
        profile_parameters: profile_params(uncensored_profile),
    })
}

struct ProviderResponse {
    content: String,
    usage: Option<CompletionUsage>,
}

// ---------------------------------------------------------------------------
// The per-attempt deadline (v4 `74ec93b5`)
// ---------------------------------------------------------------------------

/// v4 `CHEAP_LLM_TASK_TIMEOUT_MS` — the wall-clock budget for a single
/// cheap-LLM attempt against a REMOTE provider.
///
/// Cheap tasks are small — a few hundred completion tokens — and several of
/// them (the memory recap in particular) are awaited *inline* on a
/// user-visible turn. Provider SDKs default to a 10-minute request timeout, so
/// a provider that accepts the connection and then never answers wedges the
/// whole turn behind it with no log output. This budget abandons the attempt
/// instead: the task fails soft, its caller drops the optional content, and the
/// turn moves on.
pub const CHEAP_LLM_TASK_TIMEOUT_MS: u64 = 45_000;

/// v4 `CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS` — the longer budget for local providers
/// (Ollama and friends), where a cold model load or a CPU-bound machine can
/// legitimately take far longer than a remote API would. A local endpoint that
/// is merely slow is still working.
pub const CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS: u64 = 180_000;

/// v4 `PROVIDER_BUDGET_HEADROOM_MS` — how far inside the caller's deadline the
/// provider's own budget sits. Module-private in v4 and here.
///
/// The provider should give up first so the failure arrives as an ordinary
/// provider error with the socket closed, rather than as our deadline firing
/// while an orphaned request runs on.
const PROVIDER_BUDGET_HEADROOM_MS: u64 = 5_000;

/// v4 `CHEAP_LLM_TASK_TIMEOUT_OVERRIDES_MS` (`8872d7efc`) — per-task deadline
/// overrides, keyed by the granular task type.
///
/// v4's why-comment, carried whole: the default budget suits a cheap task whose
/// prompt is a slice of a turn. Compression is not that shape: it carries the
/// whole conversation history, so it is structurally the largest prompt any
/// cheap task sends and it sits at the slow end of the distribution as a matter
/// of course, not as a stall. Measured over three days on Friday, compression
/// supplied 13 of the 34 calls that finished within five seconds of the old
/// 40 s provider budget — more than any other task type — and its mean (24.4 s)
/// ran roughly 2.5× the cheap-task mean. A ceiling that most of a task's
/// healthy distribution can reach is a ceiling set for the wrong task.
///
/// Kept well short of doubling on purpose. Compression is pre-computed off the
/// turn's critical path when a cached result is available, but falls back to a
/// synchronous inline call when it isn't — and there the operator waits out the
/// whole budget. This buys the real distribution room without letting one
/// uncached turn stall for minutes.
///
/// A slice rather than a map: v4's object literal is three keys, and the lookup
/// is on the hot-ish path of every cheap task (`TASK_TYPE_ACTIVITY` above takes
/// the same shape for the same reason).
const CHEAP_LLM_TASK_TIMEOUT_OVERRIDES_MS: &[(&str, u64)] = &[
    ("compress-conversation-history", 75_000),
    ("compress-system-prompt", 75_000),
    ("compress-memories", 75_000),
];

/// v4 `deadlineFor(selection, taskType?)` — the caller-side deadline for one
/// attempt.
///
/// **Order is load-bearing.** The local check comes FIRST, exactly as v4 writes
/// it: a local provider keeps its own (larger) budget regardless of task — a
/// cold model load dwarfs any per-task difference — so a per-task override can
/// never *shrink* the local budget. Reordering these two would silently cut a
/// local compression call from 180 s to 75 s.
///
/// v4's `OVERRIDES[taskType ?? ''] ?? CHEAP_LLM_TASK_TIMEOUT_MS` — an absent
/// task type looks up the empty string, which is never a key, so it falls
/// through to the default just as an unknown one does.
pub fn cheap_llm_deadline_for(selection: &CheapLlmSelection, task_type: Option<&str>) -> u64 {
    if selection.is_local {
        return CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS;
    }
    let key = task_type.unwrap_or("");
    CHEAP_LLM_TASK_TIMEOUT_OVERRIDES_MS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, ms)| *ms)
        .unwrap_or(CHEAP_LLM_TASK_TIMEOUT_MS)
}

/// v4 `providerBudgetFor(selection, taskType?)` — the hard per-request budget
/// handed to the provider ([`CompletionParams::request_timeout_ms`]), five
/// seconds inside whichever deadline applies to this attempt: 40 000 ms remote /
/// 70 000 ms remote compression / 175 000 ms local.
// The subtraction below must never underflow. Both base constants are v4
// literals today, and so is every override — but if any of them ever moves,
// make it a compile error rather than a wrap.
const _: () = assert!(CHEAP_LLM_TASK_TIMEOUT_MS > PROVIDER_BUDGET_HEADROOM_MS);
const _: () = assert!(CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS > PROVIDER_BUDGET_HEADROOM_MS);
const _: () = {
    let mut i = 0;
    while i < CHEAP_LLM_TASK_TIMEOUT_OVERRIDES_MS.len() {
        assert!(CHEAP_LLM_TASK_TIMEOUT_OVERRIDES_MS[i].1 > PROVIDER_BUDGET_HEADROOM_MS);
        i += 1;
    }
};

pub fn provider_budget_for(selection: &CheapLlmSelection, task_type: Option<&str>) -> i64 {
    (cheap_llm_deadline_for(selection, task_type) - PROVIDER_BUDGET_HEADROOM_MS) as i64
}

/// The message bytes of v4's `CheapLLMTimeoutError`
/// (`` `Cheap LLM task${taskType ? ` (${taskType})` : ''} exceeded its
/// ${timeoutMs}ms budget` ``). v4 distinguishes a fired deadline from failed
/// work *by type*; v5 has no thrown-value hierarchy to catch on here — the
/// deadline is handled where it fires and only its message escapes, which is
/// exactly what v4's outer `getErrorMessage(error)` reduces the error to before
/// it reaches [`CheapLlmTaskResult::error`].
///
/// NB the empty-string arm: JS `taskType ? …` is falsy for `''`, so a
/// zero-length task type takes the no-parentheses form.
pub fn cheap_llm_timeout_message(timeout_ms: u64, task_type: Option<&str>) -> String {
    match task_type.filter(|t| !t.is_empty()) {
        Some(t) => format!("Cheap LLM task ({t}) exceeded its {timeout_ms}ms budget"),
        None => format!("Cheap LLM task exceeded its {timeout_ms}ms budget"),
    }
}

/// v4's `effectiveMaxTokens` floor: cheap tasks never ask for fewer than 2048.
fn effective_max_tokens(max_tokens: Option<f64>) -> i64 {
    max_tokens.unwrap_or(2048.0).max(2048.0) as i64
}

/// v4's `profileKey` for the session-level no-custom-temperature cache.
fn profile_key_for(selection: &CheapLlmSelection) -> String {
    format!("{}:{}", selection.provider, selection.model_name)
}

/// The logging config attached to a [`CheapLlmTaskExecutor`] — v4's
/// per-service `userId`/`chatId`/`messageId` (constant across a service call)
/// plus the `Db` handle and the ambient [`LogContext`]. `task_type` varies per
/// call and is passed to [`CheapLlmTaskExecutor::execute`] instead. `None` on
/// the executor means no logging (the request/spine path until the spine owner
/// wires it).
#[derive(Clone)]
pub struct CheapLlmLogConfig {
    pub db: Db,
    pub user_id: String,
    pub chat_id: Option<String>,
    pub message_id: Option<String>,
    /// Autonomous-run context; [`LogContext::none`] on the request path.
    pub ctx: LogContext,
}

/// The cheap-LLM task executor. Holds v4's session-level
/// `profilesWithoutCustomTemp` cache — module-global process state in v4,
/// carried here as instance state (the host keeps one executor per process;
/// the differential keeps one per run, matching a fresh v4 module registry).
#[derive(Default)]
pub struct CheapLlmTaskExecutor {
    profiles_without_custom_temp: Mutex<HashSet<String>>,
    /// When present, every provider call writes an `llm_logs` row — success
    /// rows v4-faithful, failure rows per the ruled divergence (unit 6).
    log: Option<CheapLlmLogConfig>,
    /// When present, a failed task walks the route's fallback chain (v4
    /// `65f5021c8`). v4 reaches its ambient `getRepositories()`; v5 needs an
    /// explicit handle, and [`CheapLlmLogConfig`] already carries the `Db` + the
    /// user id the chain reads — so `with_logging` supplies BOTH and no call
    /// site changes.
    ///
    /// ⚠ `CheapLlmTaskExecutor::new()` therefore has no chain: the two
    /// production sites that use it (`tools::generate_image`'s prompt expansion
    /// and one `enclave::step` leg) keep the pre-4.10 behaviour, and so do the
    /// differentials. A NAMED gap, not an accident — closing it means giving
    /// those two a `Db`.
    fallback: Option<CheapFallbackHandle>,
}

/// The `Db` + user id a chain walk needs, carried beside the log config.
#[derive(Clone)]
struct CheapFallbackHandle {
    db: Db,
    user_id: String,
}

impl CheapLlmTaskExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// An executor that logs each successful provider call into `llm_logs` (v4's
    /// `sendToProvider` `logLLMCall`). The host / job runner / a logging
    /// differential constructs this; the request-path spine keeps [`new`].
    pub fn with_logging(log: CheapLlmLogConfig) -> Self {
        let fallback = Some(CheapFallbackHandle {
            db: log.db.clone(),
            user_id: log.user_id.clone(),
        });
        Self {
            profiles_without_custom_temp: Mutex::new(HashSet::new()),
            log: Some(log),
            fallback,
        }
    }

    /// v4 `logCall` (core-execution.ts:93): summarize the just-completed
    /// provider call and write one `llm_logs` row (no-op when logging is off).
    ///
    /// **The failure arms now write a row too — RULED 2026-07-23 (P4.13
    /// unit 6), a deliberate divergence from v4.** v4 calls `logCall` only on
    /// `sendToProvider`'s success arms (core-execution.ts:131 / :141 / :153);
    /// a throw logs nothing, so a provider outage leaves the LLM Inspector
    /// empty and reads as "the task never ran" — exactly how dogfood finding
    /// #23 stayed invisible through a whole dogfood pass (finding #26 repeated
    /// the lesson). The human ruling the P4.11 record requested came back YES:
    /// see [`Self::log_failed_call`]. The SUCCESS arms below stay byte-faithful
    /// to v4.
    /// `duration_ms` is the wall-clock of the provider call this row describes
    /// (v4 `0cde7fbc` made it a REQUIRED `logCall` argument). This shared path
    /// covers MEMORY_EXTRACTION, TITLE_GENERATION, SUMMARIZATION,
    /// CONTEXT_COMPRESSION, SCENE_STATE_TRACKING and IMAGE_PROMPT_CRAFTING — a
    /// large share of all `llm_logs` rows — so leaving it null used to hollow out
    /// every latency average the Almanack draws.
    ///
    /// The cheap path sets no `cacheUsage`/`rawProviderUsage`/
    /// `requestHashes`; the request carries `temperature` only when one was sent
    /// (v4 spreads `...(temperature !== undefined ? { temperature } : {})`, which
    /// `summarizeRequest` collapses to `temperature: null` when absent —
    /// reproduced by `None`).
    #[allow(clippy::too_many_arguments)]
    async fn log_call(
        &self,
        task_type: Option<&str>,
        selection: &CheapLlmSelection,
        messages: &[CompletionMessage],
        temperature: Option<f64>,
        effective_max_tokens: i64,
        character_id: Option<&str>,
        response: &CompletionResponse,
        duration_ms: f64,
    ) {
        let Some(cfg) = &self.log else {
            return;
        };
        let params = LogLlmCallParams {
            user_id: cfg.user_id.clone(),
            log_type: map_task_type_to_log_type(task_type),
            message_id: cfg.message_id.clone(),
            chat_id: cfg.chat_id.clone(),
            character_id: character_id.map(str::to_string),
            provider: selection.provider.clone(),
            model_name: selection.model_name.clone(),
            // v4 `0cde7fbc` core-execution.ts:191 — `selection.connectionProfileId
            // ?? null`.
            connection_profile_id: selection.connection_profile_id.clone(),
            image_profile_id: None,
            request: LogRequest {
                messages: messages
                    .iter()
                    .map(|m| LogRequestMessage {
                        role: m.role.as_str().to_string(),
                        content: m.content.clone(),
                        attachments: None,
                    })
                    .collect(),
                temperature,
                max_tokens: Some(effective_max_tokens),
                tools: None,
            },
            response: LogResponse {
                content: response.content.clone(),
                error: None,
                finish_reason: None,
                tool_calls: None,
            },
            usage: response.usage.map(|u| LogUsage {
                prompt_tokens: Some(u.prompt_tokens),
                completion_tokens: Some(u.completion_tokens),
                total_tokens: Some(u.total_tokens),
            }),
            cache_usage: None,
            raw_provider_usage: None,
            request_hashes: None,
            duration_ms: Some(duration_ms),
        };
        // Never throws (writer swallows); awaited for determinism (watermark
        // precedent) vs v4's fire-and-forget `.catch` — same DB effect.
        let _ = log_llm_call(&cfg.db, params, &cfg.ctx).await;
    }

    /// The failed-call error row — a DELIBERATE divergence from v4, RULED
    /// 2026-07-23 (P4.13 unit 6): v4 logs NOTHING when a cheap-LLM call fails,
    /// which made findings #23/#26 cost hours of invisible failure arms — and
    /// v5 has no console to fall back on. The row is a normal cheap-LLM row
    /// whose response is `{content: "", error: <text>, …}` with no usage —
    /// distinguishable from every success row (those always log `error: null`).
    /// The success arms above stay byte-faithful to v4.
    #[allow(clippy::too_many_arguments)]
    async fn log_failed_call(
        &self,
        task_type: Option<&str>,
        selection: &CheapLlmSelection,
        messages: &[CompletionMessage],
        temperature: Option<f64>,
        effective_max_tokens: i64,
        character_id: Option<&str>,
        error_text: &str,
    ) {
        // The event beside the DB error row (P4.18). Emitted BEFORE the config
        // guard so a failed cheap call is visible even when no llm-logs context
        // is attached — findings #23/#26 were exactly this arm failing in total
        // silence. v4's cheap path logs nothing here, but v5 has no console to
        // fall back on; the console IS the fallback now.
        tracing::error!(
            target: "quilltap::cheap_llm",
            task_type = task_type.unwrap_or("unknown"),
            provider = %selection.provider,
            model = %selection.model_name,
            character_id = character_id.unwrap_or(""),
            error = error_text,
            "Cheap-LLM call failed",
        );
        let Some(cfg) = &self.log else {
            return;
        };
        let params = LogLlmCallParams {
            user_id: cfg.user_id.clone(),
            log_type: map_task_type_to_log_type(task_type),
            message_id: cfg.message_id.clone(),
            chat_id: cfg.chat_id.clone(),
            character_id: character_id.map(str::to_string),
            provider: selection.provider.clone(),
            model_name: selection.model_name.clone(),
            // The failure row is v5's own (v4 logs nothing here), so it carries
            // the same attribution the success rows do.
            connection_profile_id: selection.connection_profile_id.clone(),
            image_profile_id: None,
            request: LogRequest {
                messages: messages
                    .iter()
                    .map(|m| LogRequestMessage {
                        role: m.role.as_str().to_string(),
                        content: m.content.clone(),
                        attachments: None,
                    })
                    .collect(),
                temperature,
                max_tokens: Some(effective_max_tokens),
                tools: None,
            },
            response: LogResponse {
                content: String::new(),
                error: Some(error_text.to_string()),
                finish_reason: None,
                tool_calls: None,
            },
            usage: None,
            cache_usage: None,
            raw_provider_usage: None,
            request_hashes: None,
            duration_ms: None,
        };
        let _ = log_llm_call(&cfg.db, params, &cfg.ctx).await;
    }

    /// v4 `sendToProvider` minus the host-side API-key step: build the params
    /// the cheap path sets (strict max-tokens floor of 2048, temperature 0.3
    /// unless the profile is known not to support one, the per-character cache
    /// key, the profile's provider extras) and call the boundary, retrying
    /// without a temperature when the provider rejects it.
    async fn send_to_provider<C: CompletionProvider>(
        &self,
        completion: &C,
        selection: &CheapLlmSelection,
        messages: &[CompletionMessage],
        max_tokens: Option<f64>,
        character_id: Option<&str>,
        task_type: Option<&str>,
    ) -> Result<ProviderResponse, CompletionError> {
        let profile_key = profile_key_for(selection);
        let effective_max_tokens = effective_max_tokens(max_tokens);

        let params = |temperature: Option<f64>| CompletionParams {
            messages: messages.to_vec(),
            model: selection.model_name.clone(),
            temperature,
            max_tokens: Some(effective_max_tokens),
            // v4's cheap-LLM `baseParams` names no `topP`.
            top_p: None,
            // Cheap tasks use strictMaxTokens so providers don't apply
            // reasoning-model minimums that add verbosity and latency.
            strict_max_tokens: true,
            cache_key: build_character_cache_key(character_id),
            profile_parameters: selection.profile_parameters.clone(),
            attachments: Vec::new(),
            // Give the provider a hard budget of its own, slightly inside the
            // caller's deadline, so a stalled request is aborted at the socket
            // rather than left running while we walk away from it.
            // `send_with_deadline` remains the backstop for providers that
            // ignore it. (v4 `baseParams.requestTimeoutMs`, stamped ONCE and
            // spread into all three arms — the closure is v5's spread.)
            request_timeout_ms: Some(provider_budget_for(selection, task_type)),
        };

        let known_no_temp = self
            .profiles_without_custom_temp
            .lock()
            .expect("temp cache lock")
            .contains(&profile_key);
        if known_no_temp {
            // v4 `0cde7fbc`: `const startedAt = Date.now()` around each of the
            // three send arms; the delta is `logCall`'s required third argument.
            let started_at = crate::clock::now_unix_ms();
            let response = match completion
                .send_message(
                    &selection.provider,
                    selection.base_url.as_deref(),
                    &params(None),
                )
                .await
            {
                Ok(r) => r,
                Err(error) => {
                    // The ruled error row (see `log_failed_call`).
                    self.log_failed_call(
                        task_type,
                        selection,
                        messages,
                        None,
                        effective_max_tokens,
                        character_id,
                        &error.message,
                    )
                    .await;
                    return Err(error);
                }
            };
            // v4 logs after every successful provider call (with the temperature
            // actually sent — none here).
            self.log_call(
                task_type,
                selection,
                messages,
                None,
                effective_max_tokens,
                character_id,
                &response,
                (crate::clock::now_unix_ms() - started_at) as f64,
            )
            .await;
            return Ok(ProviderResponse {
                content: response.content,
                usage: response.usage,
            });
        }

        // Try with lower temperature for more consistent outputs.
        let first_attempt_started_at = crate::clock::now_unix_ms();
        match completion
            .send_message(
                &selection.provider,
                selection.base_url.as_deref(),
                &params(Some(0.3)),
            )
            .await
        {
            Ok(response) => {
                self.log_call(
                    task_type,
                    selection,
                    messages,
                    Some(0.3),
                    effective_max_tokens,
                    character_id,
                    &response,
                    (crate::clock::now_unix_ms() - first_attempt_started_at) as f64,
                )
                .await;
                Ok(ProviderResponse {
                    content: response.content,
                    usage: response.usage,
                })
            }
            Err(error) => {
                // If temperature is unsupported, cache that and retry without.
                if error.message.contains("temperature")
                    || error.message.contains("does not support")
                {
                    self.profiles_without_custom_temp
                        .lock()
                        .expect("temp cache lock")
                        .insert(profile_key);
                    let retry_started_at = crate::clock::now_unix_ms();
                    let response = match completion
                        .send_message(
                            &selection.provider,
                            selection.base_url.as_deref(),
                            &params(None),
                        )
                        .await
                    {
                        Ok(r) => r,
                        Err(retry_error) => {
                            // The ruled error row (see `log_failed_call`).
                            self.log_failed_call(
                                task_type,
                                selection,
                                messages,
                                None,
                                effective_max_tokens,
                                character_id,
                                &retry_error.message,
                            )
                            .await;
                            return Err(retry_error);
                        }
                    };
                    self.log_call(
                        task_type,
                        selection,
                        messages,
                        None,
                        effective_max_tokens,
                        character_id,
                        &response,
                        (crate::clock::now_unix_ms() - retry_started_at) as f64,
                    )
                    .await;
                    Ok(ProviderResponse {
                        content: response.content,
                        usage: response.usage,
                    })
                } else {
                    // The ruled error row (see `log_failed_call`) — the
                    // temperature actually sent on the failing call was 0.3.
                    self.log_failed_call(
                        task_type,
                        selection,
                        messages,
                        Some(0.3),
                        effective_max_tokens,
                        character_id,
                        &error.message,
                    )
                    .await;
                    Err(error)
                }
            }
        }
    }

    /// The temperature the NEXT provider call on this selection will carry —
    /// `Some(0.3)` normally, `None` once the profile is cached as
    /// no-custom-temperature. A fired deadline never learns which arm of
    /// [`Self::send_to_provider`] was in flight, so this is what the
    /// abandonment's error row records: the temperature the attempt *started*
    /// with.
    fn pending_temperature(&self, selection: &CheapLlmSelection) -> Option<f64> {
        let known_no_temp = self
            .profiles_without_custom_temp
            .lock()
            .expect("temp cache lock")
            .contains(&profile_key_for(selection));
        if known_no_temp {
            None
        } else {
            Some(0.3)
        }
    }

    /// v4 `withDeadline` — bound one attempt by its deadline, and say so in the
    /// log when it fires. A stall used to be completely silent, which is what
    /// made the original incident so hard to see.
    ///
    /// **One structural divergence from v4, deliberate.** v4 does
    /// `void work.catch(() => {})` and walks away: the abandoned request is left
    /// to finish on its own because a JS promise cannot be cancelled, and only
    /// the *waiting* stops. Dropping a Rust future genuinely cancels it, so v5
    /// abandons AND cancels — strictly better (no orphaned socket, and no late
    /// `llm_logs` row from a call nobody is listening to), and unobservable in
    /// any differential: the abandoned v4 promise's only side effect is that
    /// late row, which no oracle case can reach because the canned providers
    /// never stall. The provider's own budget
    /// ([`provider_budget_for`]) fires five seconds earlier anyway, so a
    /// well-behaved provider closes the socket before either side gives up.
    #[allow(clippy::too_many_arguments)]
    async fn send_with_deadline<C: CompletionProvider>(
        &self,
        completion: &C,
        selection: &CheapLlmSelection,
        messages: &[CompletionMessage],
        max_tokens: Option<f64>,
        character_id: Option<&str>,
        task_type: Option<&str>,
    ) -> Result<ProviderResponse, CompletionError> {
        let timeout_ms = cheap_llm_deadline_for(selection, task_type);
        // `tokio::time::Instant`, not `std::time::Instant`: it reads the same
        // clock the deadline does, so a paused-clock test measures the virtual
        // elapsed time the log reports.
        let started_at = tokio::time::Instant::now();
        let pending_temperature = self.pending_temperature(selection);
        let work = self.send_to_provider(
            completion,
            selection,
            messages,
            max_tokens,
            character_id,
            task_type,
        );
        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), work).await {
            Ok(result) => result,
            Err(_) => {
                let message = cheap_llm_timeout_message(timeout_ms, task_type);
                // v4's abandonment warn, field for field (`chatId` comes from
                // the attached log config — v4 threads it as a parameter).
                tracing::warn!(
                    target: "quilltap::cheap_llm",
                    chat_id = self.log.as_ref().and_then(|c| c.chat_id.as_deref()).unwrap_or(""),
                    character_id = character_id.unwrap_or(""),
                    provider = %selection.provider,
                    model = %selection.model_name,
                    task_type = task_type.unwrap_or(""),
                    timeout_ms,
                    elapsed = started_at.elapsed().as_millis() as u64,
                    "[CheapLLM] Abandoned a stalled provider call",
                );
                // A timeout IS a failed call, so it takes the standing ruled
                // divergence's error row (see `log_failed_call`) — v4 writes no
                // row here either way, since it writes none for ANY unfinished
                // or failed cheap call.
                self.log_failed_call(
                    task_type,
                    selection,
                    messages,
                    pending_temperature,
                    effective_max_tokens(max_tokens),
                    character_id,
                    &message,
                )
                .await;
                Err(CompletionError::new(message))
            }
        }
    }

    /// v4 `executeCheapLLMTask`: send, maybe retry on the uncensored provider
    /// for an empty response, parse, and wrap — any error becomes
    /// `{ success: false, error }`.
    ///
    /// Registers the call with the activity registry for its whole duration, so
    /// a cheap-LLM task lights its chip whether it was queued as a job or run
    /// inline in a request. Re-entrant by kind: a task running inside a job of
    /// the same kind collapses into that job's count instead of doubling it. v4
    /// `core-execution.ts:366` (`664cfca84`). Every cheap-LLM task funnels
    /// through here, which is why [`TASK_TYPE_ACTIVITY`] is the one place that
    /// has to know which chip each task lights.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C: CompletionProvider, T>(
        &self,
        completion: &C,
        selection: &CheapLlmSelection,
        messages: Vec<CompletionMessage>,
        parse_response: impl Fn(&str) -> T,
        uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
        max_tokens: Option<f64>,
        character_id: Option<&str>,
        task_type: Option<&str>,
    ) -> CheapLlmTaskResult<T> {
        crate::services::activity_registry::track_activity(
            activity_kind_for_task(task_type),
            self.run(
                completion,
                selection,
                messages,
                parse_response,
                uncensored_fallback,
                max_tokens,
                character_id,
                task_type,
            ),
        )
        .await
    }

    /// v4 `runCheapLLMTask` — the unwrapped body.
    #[allow(clippy::too_many_arguments)]
    async fn run<C: CompletionProvider, T>(
        &self,
        completion: &C,
        selection: &CheapLlmSelection,
        messages: Vec<CompletionMessage>,
        parse_response: impl Fn(&str) -> T,
        uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
        max_tokens: Option<f64>,
        character_id: Option<&str>,
        task_type: Option<&str>,
    ) -> CheapLlmTaskResult<T> {
        // Each attempt gets its own budget rather than sharing one across the
        // task: the uncensored fallback below only fires after a *completed*
        // call came back empty, so it is a fresh attempt and deserves a fresh
        // deadline (v4's `deadlineFor` per call site).
        let attempt = async {
            let mut response = self
                .send_with_deadline(
                    completion,
                    selection,
                    &messages,
                    max_tokens,
                    character_id,
                    task_type,
                )
                .await?;

            if let Some(uncensored_selection) = should_attempt_uncensored_fallback(
                &response.content,
                selection,
                uncensored_fallback,
            ) {
                let retry = self
                    .send_with_deadline(
                        completion,
                        &uncensored_selection,
                        &messages,
                        max_tokens,
                        character_id,
                        task_type,
                    )
                    .await?;
                if js_trim(&retry.content).is_empty() {
                    return Err(CompletionError::new(format!(
                        "Empty response from both safe provider ({}/{}) and uncensored provider ({}/{})",
                        selection.provider,
                        selection.model_name,
                        uncensored_selection.provider,
                        uncensored_selection.model_name,
                    )));
                }
                response = retry;
            }

            Ok(response)
        };

        match attempt.await {
            Ok(response) => CheapLlmTaskResult {
                success: true,
                result: Some(parse_response(&response.content)),
                error: None,
                usage: response.usage,
            },
            // v4 `getErrorMessage(error)` — the thrown Error's message.
            Err(error) => {
                // v4 `65f5021c8`: walk the failed route's fallback chain,
                // re-issuing the task against each stand-in with a FRESH
                // deadline. A fresh budget per attempt, not a shared one: the
                // whole reason we are here is that the previous route spent its
                // budget without answering, and charging the understudy for
                // that would guarantee it fails too. Background work never
                // toasts — the job logs its attempt trail and moves on.
                if let Some(result) = self
                    .attempt_cheap_fallback_chain(
                        completion,
                        selection,
                        &messages,
                        &parse_response,
                        &error,
                        uncensored_fallback,
                        max_tokens,
                        character_id,
                        task_type,
                    )
                    .await
                {
                    return result;
                }
                // Say so in the server log (v4 `8872d7efc`). `send_with_deadline`
                // already reports the case where *our* deadline fires, but a
                // provider giving up on its own budget arrives here as an ordinary
                // provider error — and the plugin's own log line names the
                // provider without naming the task, so a timed-out extraction pass
                // was legible only in the debug logs stored on the message. One
                // line here ties the two together.
                //
                // v4 threads `chatId`/`characterId` as parameters; v5's `chat_id`
                // comes from the attached log config, the same source the
                // abandonment warn above reads.
                tracing::warn!(
                    target: "quilltap::cheap_llm",
                    task_type = task_type.unwrap_or(""),
                    chat_id = self.log.as_ref().and_then(|c| c.chat_id.as_deref()).unwrap_or(""),
                    character_id = character_id.unwrap_or(""),
                    provider = %selection.provider,
                    model = %selection.model_name,
                    error = %error.message,
                    "[CheapLLM] Task failed",
                );
                CheapLlmTaskResult {
                    success: false,
                    result: None,
                    error: Some(error.message),
                    usage: None,
                }
            }
        }
    }

    /// v4 `attemptCheapFallbackChain` (core-execution.ts, `65f5021c8`).
    ///
    /// Returns `None` when nothing was attempted or nothing worked, leaving the
    /// caller to fail exactly as it did before this feature existed.
    #[allow(clippy::too_many_arguments)]
    async fn attempt_cheap_fallback_chain<C: CompletionProvider, T>(
        &self,
        completion: &C,
        selection: &CheapLlmSelection,
        messages: &[CompletionMessage],
        parse_response: &impl Fn(&str) -> T,
        error: &CompletionError,
        uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
        max_tokens: Option<f64>,
        character_id: Option<&str>,
        task_type: Option<&str>,
    ) -> Option<CheapLlmTaskResult<T>> {
        let handle = self.fallback.as_ref()?;

        // v4 classifies the thrown error; a non-trigger (a token limit, a
        // tool-unsupported rejection, one of our own bugs) leaves the chain out
        // of it. v5's cheap path surfaces a `CompletionError` carrying the
        // message, which is what `FallbackError::message` reads — except for the
        // one class v4 tests by NAME: its own `CheapLLMTimeoutError`, which
        // `send_with_deadline` raises with v4's exact sentence.
        let is_deadline =
            error.message.contains("exceeded its") && error.message.contains("budget");
        let fe = if is_deadline {
            crate::llm_fallback::FallbackError::named("CheapLLMTimeoutError", &error.message)
        } else {
            crate::llm_fallback::FallbackError::message(&error.message)
        };
        let Some(trigger) = crate::llm_fallback::classify_fallback_trigger(fe) else {
            tracing::debug!(
                target: "quilltap::cheap_llm",
                task_type = task_type.unwrap_or(""),
                error = %error.message,
                "[CheapLLM] Failure is not fallback-eligible"
            );
            return None;
        };

        // v4: a stand-in for an uncensored route must itself be cleared for the
        // content, or the fallback hands it back to the moderation that refused.
        let dangerous = uncensored_fallback.is_some_and(|u| {
            u.is_dangerous_chat == Some(true)
                || selection
                    .connection_profile_id
                    .as_deref()
                    .is_some_and(|id| {
                        u.available_profiles
                            .iter()
                            .find(|p| p.id == id)
                            .is_some_and(|p| p.is_dangerous_compatible)
                    })
        });

        let stand_ins = super::cheap_llm_fallback::build_cheap_fallback_selections(
            &handle.db,
            super::cheap_llm_fallback::CheapFallbackRequest {
                selection,
                user_id: &handle.user_id,
                dangerous,
                already_tried: selection
                    .connection_profile_id
                    .clone()
                    .into_iter()
                    .collect(),
                task_type,
            },
        );
        if stand_ins.is_empty() {
            return None;
        }

        for stand_in in &stand_ins {
            tracing::info!(
                target: "quilltap::cheap_llm",
                task_type = task_type.unwrap_or(""),
                trigger = trigger.as_str(),
                failed_provider = %selection.provider,
                failed_model = %selection.model_name,
                stand_in_provider = %stand_in.provider,
                stand_in_model = %stand_in.model_name,
                stand_in_profile_id = stand_in.connection_profile_id.as_deref().unwrap_or(""),
                "[CheapLLM] Retrying task with a stand-in"
            );

            match self
                .send_with_deadline(
                    completion,
                    stand_in,
                    messages,
                    max_tokens,
                    character_id,
                    task_type,
                )
                .await
            {
                Ok(response) => {
                    if js_trim(&response.content).is_empty() {
                        tracing::warn!(
                            target: "quilltap::cheap_llm",
                            task_type = task_type.unwrap_or(""),
                            stand_in_provider = %stand_in.provider,
                            stand_in_model = %stand_in.model_name,
                            "[CheapLLM] Stand-in returned an empty response"
                        );
                        continue;
                    }
                    tracing::info!(
                        target: "quilltap::cheap_llm",
                        task_type = task_type.unwrap_or(""),
                        stand_in_provider = %stand_in.provider,
                        stand_in_model = %stand_in.model_name,
                        response_length = response.content.len(),
                        "[CheapLLM] Stand-in answered"
                    );
                    return Some(CheapLlmTaskResult {
                        success: true,
                        result: Some(parse_response(&response.content)),
                        error: None,
                        usage: response.usage,
                    });
                }
                Err(stand_in_error) => tracing::warn!(
                    target: "quilltap::cheap_llm",
                    task_type = task_type.unwrap_or(""),
                    stand_in_provider = %stand_in.provider,
                    stand_in_model = %stand_in.model_name,
                    error = %stand_in_error.message,
                    "[CheapLLM] Stand-in also failed"
                ),
            }
        }

        tracing::warn!(
            target: "quilltap::cheap_llm",
            task_type = task_type.unwrap_or(""),
            trigger = trigger.as_str(),
            failed_provider = %selection.provider,
            failed_model = %selection.model_name,
            stand_ins_tried = stand_ins.len(),
            "[CheapLLM] Fallback chain exhausted"
        );
        None
    }
}

/// Which toolbar chip each cheap-LLM task lights (v4 `TASK_TYPE_ACTIVITY`,
/// `664cfca84:lib/memory/cheap-llm-tasks/core-execution.ts`).
///
/// Every cheap-LLM task funnels through [`CheapLlmTaskExecutor::execute`], so this
/// is the one place that has to know. A task type absent from the map falls
/// back to `summary` rather than going uncounted — v4's rule verbatim: "a chip
/// that is slightly generous is better than a chip that quietly lies".
///
/// Transcribed from v4's source; pinned by `activity_tables_equivalence`.
pub const TASK_TYPE_ACTIVITY: &[(&str, ActivityKind)] = &[
    // Image pipelines — prompt crafting and appearance work is part of the
    // image the user is waiting on, so it belongs inside the same span.
    ("craft-image-prompt", ActivityKind::Image),
    ("craft-story-background-prompt", ActivityKind::Image),
    ("derive-scene-context", ActivityKind::Image),
    ("resolve-character-appearances", ActivityKind::Image),
    ("sanitize-appearance", ActivityKind::Image),
    ("describe-attachment", ActivityKind::Image),
    ("outfit-selection", ActivityKind::Image),
    // the Commonplace Book
    ("memory-extraction-self", ActivityKind::Memory),
    ("memory-extraction-other", ActivityKind::Memory),
    ("batch-memory-extraction", ActivityKind::Memory),
    ("fold-episode-extraction", ActivityKind::Memory),
    ("memory-keyword-extraction", ActivityKind::Memory),
    ("memory-recap-summarization", ActivityKind::Memory),
    // Summarization and post-turn processing
    ("fold-chat-summary", ActivityKind::Summary),
    ("summarize-chat", ActivityKind::Summary),
    ("update-context-summary", ActivityKind::Summary),
    ("consider-title-update", ActivityKind::Summary),
    ("consider-help-chat-title-update", ActivityKind::Summary),
    ("scene-state-tracking", ActivityKind::Summary),
    ("compress-conversation-history", ActivityKind::Summary),
    ("compress-memories", ActivityKind::Summary),
    ("compress-system-prompt", ActivityKind::Summary),
];

/// v4 `activityKindForTask`: the map, or `summary` for anything absent (a
/// missing `taskType` included — v4's `(taskType && MAP[taskType]) || 'summary'`).
pub fn activity_kind_for_task(task_type: Option<&str>) -> ActivityKind {
    task_type
        .and_then(|t| {
            TASK_TYPE_ACTIVITY
                .iter()
                .find(|(k, _)| *k == t)
                .map(|(_, kind)| *kind)
        })
        .unwrap_or(ActivityKind::Summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cheap_llm::{CheapLlmProfile, DangerousContentSettings};
    use crate::db::runtime::DbPaths;
    use crate::db::Writer;
    use crate::model::completion::CannedCompletionProvider;

    fn selection(provider: &str, model: &str) -> CheapLlmSelection {
        CheapLlmSelection {
            provider: provider.to_string(),
            model_name: model.to_string(),
            base_url: None,
            connection_profile_id: Some("cur".to_string()),
            is_local: false,
            profile_parameters: None,
        }
    }

    fn local_selection(provider: &str, model: &str) -> CheapLlmSelection {
        CheapLlmSelection {
            is_local: true,
            ..selection(provider, model)
        }
    }

    // === P4.D42 unit 2: the per-attempt deadline (v4 `74ec93b5`) ===
    //
    // A timeout is wall-clock behavior no NDJSON corpus can observe — the canned
    // providers never stall — so per the P4.15 falsifiability ruling these are
    // unit-tier pins, driven on a PAUSED tokio clock so they cost no real time.
    // They are the v5 counterpart of v4's own
    // `lib/memory/cheap-llm-tasks/__tests__/task-deadline.test.ts`.

    /// A provider that accepts every call and never answers — v4's
    /// `sendMessage.mockReturnValue(new Promise(() => {}))`.
    struct StallingProvider;

    impl CompletionProvider for StallingProvider {
        fn send_message(
            &self,
            _provider: &str,
            _base_url: Option<&str>,
            _params: &CompletionParams,
        ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send
        {
            std::future::pending()
        }
    }

    /// A provider that takes `delay_ms` and then answers — "slow" as distinct
    /// from "stalled".
    struct SlowProvider {
        delay_ms: u64,
        content: String,
    }

    impl CompletionProvider for SlowProvider {
        fn send_message(
            &self,
            _provider: &str,
            _base_url: Option<&str>,
            _params: &CompletionParams,
        ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send
        {
            let delay = std::time::Duration::from_millis(self.delay_ms);
            let content = self.content.clone();
            async move {
                tokio::time::sleep(delay).await;
                Ok(CompletionResponse {
                    content,
                    usage: None,
                    finish_reason: None,
                    attachment_results: None,
                })
            }
        }
    }

    /// Answers slowly (and emptily) for one provider so the uncensored fallback
    /// fires, then stalls forever on the fallback's provider. The two attempts'
    /// deadlines are only distinguishable if the first one CONSUMED time without
    /// firing — a shared task-level budget would fire earlier than a fresh one.
    struct SlowEmptyThenStall {
        first_provider: String,
        first_delay_ms: u64,
    }

    impl CompletionProvider for SlowEmptyThenStall {
        fn send_message(
            &self,
            provider: &str,
            _base_url: Option<&str>,
            _params: &CompletionParams,
        ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send
        {
            let first = provider == self.first_provider;
            let delay = std::time::Duration::from_millis(self.first_delay_ms);
            async move {
                if first {
                    tokio::time::sleep(delay).await;
                    return Ok(CompletionResponse {
                        content: String::new(),
                        usage: None,
                        finish_reason: None,
                        attachment_results: None,
                    });
                }
                std::future::pending().await
            }
        }
    }

    /// A `tracing` layer that captures `LEVEL target msg field=value …` for
    /// every event, so the abandonment warn can be asserted field by field —
    /// the fields ARE the deliverable here (the incident's whole cost was that
    /// a stall logged nothing at all).
    struct CaptureLayer(std::sync::Arc<Mutex<Vec<String>>>);

    struct FieldVisitor(String);

    impl tracing::field::Visit for FieldVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
        fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0.push_str(&format!(" {value:?}"));
            } else {
                self.0.push_str(&format!(" {}={value:?}", field.name()));
            }
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = event.metadata();
            let mut visitor = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.0);
        }
    }

    /// Records the [`CompletionParams`] of every call and answers at once.
    struct RecordingProvider {
        seen: Mutex<Vec<CompletionParams>>,
    }

    impl RecordingProvider {
        fn new() -> Self {
            Self {
                seen: Mutex::new(Vec::new()),
            }
        }
        fn budgets(&self) -> Vec<Option<i64>> {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .map(|p| p.request_timeout_ms)
                .collect()
        }
    }

    impl CompletionProvider for RecordingProvider {
        fn send_message(
            &self,
            _provider: &str,
            _base_url: Option<&str>,
            params: &CompletionParams,
        ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send
        {
            self.seen.lock().unwrap().push(params.clone());
            async move {
                Ok(CompletionResponse {
                    content: "ok".to_string(),
                    usage: None,
                    finish_reason: None,
                    attachment_results: None,
                })
            }
        }
    }

    /// The `CheapLLMTimeoutError` message bytes, both arms. JS `taskType ? …` is
    /// falsy for the empty string, so `Some("")` must take the bare form.
    #[test]
    fn the_timeout_message_matches_v4s_bytes() {
        assert_eq!(
            cheap_llm_timeout_message(45_000, Some("memory-recap-summarization")),
            "Cheap LLM task (memory-recap-summarization) exceeded its 45000ms budget"
        );
        assert_eq!(
            cheap_llm_timeout_message(180_000, None),
            "Cheap LLM task exceeded its 180000ms budget"
        );
        assert_eq!(
            cheap_llm_timeout_message(45_000, Some("")),
            "Cheap LLM task exceeded its 45000ms budget"
        );
        // P4.D127 (v4 `8872d7efc`): a compression timeout now reads 75000ms.
        assert_eq!(
            cheap_llm_timeout_message(
                cheap_llm_deadline_for(
                    &selection("DEEPSEEK", "m"),
                    Some("compress-conversation-history")
                ),
                Some("compress-conversation-history")
            ),
            "Cheap LLM task (compress-conversation-history) exceeded its 75000ms budget"
        );
    }

    // ── P4.D127 / v4 `8872d7efc` — the per-task deadline ────────────────────
    //
    // v4's four test groups, mirrored (`__tests__/unit/lib/memory/
    // cheap-llm-tasks/cheap-llm-deadlines.test.ts`). `deadline_for` is pure, so
    // no provider and no network are involved.

    const COMPRESSION_TASKS: &[&str] = &[
        "compress-conversation-history",
        "compress-system-prompt",
        "compress-memories",
    ];

    /// Compression's budget is genuinely LARGER than the shared default — the
    /// thing that made the old arrangement wrong.
    #[test]
    fn every_compression_task_gets_more_room_than_the_shared_default() {
        let remote = selection("DEEPSEEK", "m");
        for task in COMPRESSION_TASKS {
            assert!(
                cheap_llm_deadline_for(&remote, Some(task)) > CHEAP_LLM_TASK_TIMEOUT_MS,
                "{task} did not get more room than the default"
            );
        }
    }

    /// All three compression tasks share ONE budget (v4's `Set(...).size === 1`).
    #[test]
    fn all_three_compression_tasks_share_one_budget() {
        let remote = selection("DEEPSEEK", "m");
        let budgets: std::collections::HashSet<u64> = COMPRESSION_TASKS
            .iter()
            .map(|t| cheap_llm_deadline_for(&remote, Some(t)))
            .collect();
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets.into_iter().next(), Some(75_000));
    }

    /// The override must not read as a GLOBAL bump: v4 names six task types
    /// that shared the ceiling with compression and must not have moved.
    #[test]
    fn other_cheap_tasks_stay_on_the_shared_default() {
        let remote = selection("DEEPSEEK", "m");
        for task in [
            "memory-extraction-self",
            "memory-extraction-other",
            "scene-state-tracking",
            "answer-confirmation",
            "summarize-chat",
            "title-chat",
        ] {
            assert_eq!(
                cheap_llm_deadline_for(&remote, Some(task)),
                CHEAP_LLM_TASK_TIMEOUT_MS,
                "{task} should still be on the shared default"
            );
        }
    }

    /// An unknown or absent task type falls back to the default. (v4 looks up
    /// `taskType ?? ''`, and the empty string is never a key.)
    #[test]
    fn an_unknown_or_absent_task_type_falls_back_to_the_default() {
        let remote = selection("DEEPSEEK", "m");
        assert_eq!(
            cheap_llm_deadline_for(&remote, Some("not-a-real-task")),
            CHEAP_LLM_TASK_TIMEOUT_MS
        );
        assert_eq!(
            cheap_llm_deadline_for(&remote, None),
            CHEAP_LLM_TASK_TIMEOUT_MS
        );
        assert_eq!(
            cheap_llm_deadline_for(&remote, Some("")),
            CHEAP_LLM_TASK_TIMEOUT_MS
        );
    }

    /// The local exemption, compression included — the reason v4's local check
    /// comes FIRST. A cold model load dwarfs any per-task difference, and the
    /// local budget is already the larger of the two: a per-task override must
    /// never shrink it. (Reorder the two arms of `cheap_llm_deadline_for` and
    /// this is the test that reddens.)
    #[test]
    fn a_local_provider_keeps_its_own_budget_whatever_the_task() {
        let local = local_selection("OLLAMA", "qwen3");
        assert_eq!(
            cheap_llm_deadline_for(&local, None),
            CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS
        );
        for task in COMPRESSION_TASKS {
            assert_eq!(
                cheap_llm_deadline_for(&local, Some(task)),
                CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS,
                "a per-task override shrank the local budget for {task}"
            );
        }
    }

    // ── P4.D123 site 3: the executor is wrapped, and the KIND is COMPUTED ────
    //
    // Driven for real (rather than census-only) because the kind comes from
    // `activity_kind_for_task`: a mis-wired lookup would light the wrong chip
    // and nothing else in the workspace would notice. The provider stub reports
    // the ATTRIBUTION SET rather than the global counters, so the assertion is
    // immune to whatever else this test binary is running concurrently.

    struct AttributionProvider {
        seen: std::sync::Arc<Mutex<Option<Vec<crate::services::activity_kinds::ActivityKind>>>>,
    }

    impl CompletionProvider for AttributionProvider {
        fn send_message(
            &self,
            _provider: &str,
            _base_url: Option<&str>,
            _params: &CompletionParams,
        ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send
        {
            let seen = self.seen.clone();
            async move {
                *seen.lock().unwrap() =
                    Some(crate::services::activity_registry::attributed_kinds());
                Ok(CompletionResponse {
                    content: "ok".to_string(),
                    usage: None,
                    finish_reason: None,
                    attachment_results: None,
                })
            }
        }
    }

    async fn attribution_for_task(
        task_type: Option<&str>,
    ) -> Vec<crate::services::activity_kinds::ActivityKind> {
        let seen = std::sync::Arc::new(Mutex::new(None));
        let out = CheapLlmTaskExecutor::new()
            .execute(
                &AttributionProvider { seen: seen.clone() },
                &selection("DEEPSEEK", "m"),
                vec![CompletionMessage::user("hi")],
                |s| s.to_string(),
                None,
                None,
                None,
                task_type,
            )
            .await;
        assert!(out.success, "the stub provider answers");
        let seen = seen.lock().unwrap().take().expect("provider was called");
        seen
    }

    #[tokio::test]
    async fn a_cheap_llm_task_lights_the_chip_its_task_type_names() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        use crate::services::activity_kinds::ActivityKind;
        // An image-pipeline task lights "Img".
        assert_eq!(
            attribution_for_task(Some("craft-image-prompt")).await,
            vec![ActivityKind::Image]
        );
        // A Commonplace-Book task lights "Mem".
        assert_eq!(
            attribution_for_task(Some("fold-episode-extraction")).await,
            vec![ActivityKind::Memory]
        );
        // A summarization task lights "Sum".
        assert_eq!(
            attribution_for_task(Some("update-context-summary")).await,
            vec![ActivityKind::Summary]
        );
        // An unmapped task type — and a MISSING one — fall back to "Sum" (v4's
        // rule: a chip that is slightly generous beats one that quietly lies).
        assert_eq!(
            attribution_for_task(Some("some-future-task")).await,
            vec![ActivityKind::Summary]
        );
        assert_eq!(
            attribution_for_task(None).await,
            vec![ActivityKind::Summary]
        );
    }

    /// The incident: a remote provider that accepts the request and never
    /// answers is abandoned at 45 s, and the task fails soft with v4's message.
    #[tokio::test(start_paused = true)]
    async fn a_remote_attempt_is_abandoned_at_its_45s_deadline() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let exec = CheapLlmTaskExecutor::new();
        let sel = selection("DEEPSEEK", "deepseek-v4-flash");
        let started = tokio::time::Instant::now();

        let r = exec
            .execute(
                &StallingProvider,
                &sel,
                vec![CompletionMessage::user("hi")],
                |s| s.to_string(),
                None,
                None,
                None,
                Some("memory-recap-summarization"),
            )
            .await;

        assert!(!r.success);
        assert_eq!(
            r.error.as_deref(),
            Some("Cheap LLM task (memory-recap-summarization) exceeded its 45000ms budget")
        );
        let elapsed = started.elapsed();
        assert!(
            elapsed >= std::time::Duration::from_millis(CHEAP_LLM_TASK_TIMEOUT_MS)
                && elapsed < std::time::Duration::from_millis(CHEAP_LLM_TASK_TIMEOUT_MS + 1_000),
            "abandoned after {elapsed:?}, expected ~45s"
        );
    }

    /// Local providers get the longer budget — a cold model load is slow, not
    /// stalled. Same stalling provider, four times the patience.
    #[tokio::test(start_paused = true)]
    async fn a_local_attempt_is_abandoned_only_at_its_180s_deadline() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let exec = CheapLlmTaskExecutor::new();
        let sel = local_selection("OLLAMA", "qwen3");
        let started = tokio::time::Instant::now();

        let r = exec
            .execute(
                &StallingProvider,
                &sel,
                vec![CompletionMessage::user("hi")],
                |s| s.to_string(),
                None,
                None,
                None,
                Some("summarize-chat"),
            )
            .await;

        assert!(!r.success);
        assert_eq!(
            r.error.as_deref(),
            Some("Cheap LLM task (summarize-chat) exceeded its 180000ms budget")
        );
        let elapsed = started.elapsed();
        assert!(
            elapsed >= std::time::Duration::from_millis(CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS),
            "a local call gave up after {elapsed:?}, before its own budget"
        );
    }

    /// v4's "slow is not the same as stalled" pair: the SAME 100 s call is a
    /// success on a local provider and an abandonment on a remote one.
    #[tokio::test(start_paused = true)]
    async fn a_hundred_second_call_succeeds_locally_and_is_abandoned_remotely() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let slow = SlowProvider {
            delay_ms: 100_000,
            content: "slow but fine".to_string(),
        };
        let messages = vec![CompletionMessage::user("hi")];

        let local = CheapLlmTaskExecutor::new()
            .execute(
                &slow,
                &local_selection("OLLAMA", "qwen3"),
                messages.clone(),
                |s| s.to_string(),
                None,
                None,
                None,
                Some("summarize-chat"),
            )
            .await;
        assert!(local.success);
        assert_eq!(local.result.as_deref(), Some("slow but fine"));

        let remote = CheapLlmTaskExecutor::new()
            .execute(
                &slow,
                &selection("DEEPSEEK", "deepseek-v4-flash"),
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                Some("summarize-chat"),
            )
            .await;
        assert!(!remote.success);
        assert!(remote.error.as_deref().unwrap().contains("45000ms budget"));
    }

    /// P4.D127 (v4 `8872d7efc`) — the override on the REAL path, both ways. A
    /// 60 s remote compression call used to be abandoned at 45 s and now
    /// succeeds; a 100 s one is still abandoned, but at 75 s, and the message
    /// bytes say so. Pre-fix, the first half of this test was red.
    #[tokio::test(start_paused = true)]
    async fn a_remote_compression_call_lives_to_seventy_five_seconds() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let messages = vec![CompletionMessage::user("compress this")];

        let survives = CheapLlmTaskExecutor::new()
            .execute(
                &SlowProvider {
                    delay_ms: 60_000,
                    content: "compressed".to_string(),
                },
                &selection("DEEPSEEK", "deepseek-v4-flash"),
                messages.clone(),
                |s| s.to_string(),
                None,
                None,
                None,
                Some("compress-conversation-history"),
            )
            .await;
        assert!(
            survives.success,
            "a 60 s compression call must now finish: {:?}",
            survives.error
        );
        assert_eq!(survives.result.as_deref(), Some("compressed"));

        let abandoned = CheapLlmTaskExecutor::new()
            .execute(
                &SlowProvider {
                    delay_ms: 100_000,
                    content: "too slow".to_string(),
                },
                &selection("DEEPSEEK", "deepseek-v4-flash"),
                messages.clone(),
                |s| s.to_string(),
                None,
                None,
                None,
                Some("compress-memories"),
            )
            .await;
        assert!(!abandoned.success);
        assert!(
            abandoned
                .error
                .as_deref()
                .unwrap()
                .contains("75000ms budget"),
            "the abandonment message must name the compression budget: {:?}",
            abandoned.error
        );

        // …and the override is not a global bump on the real path either: the
        // same 60 s call on a non-compression task is still abandoned at 45 s.
        let still_bounded = CheapLlmTaskExecutor::new()
            .execute(
                &SlowProvider {
                    delay_ms: 60_000,
                    content: "irrelevant".to_string(),
                },
                &selection("DEEPSEEK", "deepseek-v4-flash"),
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                Some("summarize-chat"),
            )
            .await;
        assert!(!still_bounded.success);
        assert!(still_bounded
            .error
            .as_deref()
            .unwrap()
            .contains("45000ms budget"));
    }

    /// P4.D127 (v4 `8872d7efc`) — a failed cheap task narrates itself. Log-only:
    /// no differential can see a new `logger.warn`
    /// (`differential-blind-to-a-log-only-fix`), so the pin is a capturing
    /// tracing layer asserting the line AND its whole field set. v4's rationale:
    /// a provider giving up on its OWN budget used to arrive as an ordinary
    /// provider error, invisible to a server-log grep.
    #[tokio::test]
    async fn a_failed_cheap_task_warns_with_its_whole_field_set() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests.
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        use tracing_subscriber::layer::SubscriberExt;

        /// Fails the way a provider that gave up on its own budget does: an
        /// ordinary error, not our deadline firing.
        struct FailingProvider;
        impl CompletionProvider for FailingProvider {
            fn send_message(
                &self,
                _provider: &str,
                _base_url: Option<&str>,
                _params: &CompletionParams,
            ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send
            {
                // (Not an `async fn`: the trait's RPITIT signature is what the
                // sibling stubs above use, and clippy's `manual_async_fn` only
                // fires when the whole body is one bare async block.)
                let error = CompletionError::new("Request timed out after 40000ms");
                async move { Err(error) }
            }
        }

        let logs = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        let r = {
            let _guard = tracing::subscriber::set_default(subscriber);
            CheapLlmTaskExecutor::new()
                .execute(
                    &FailingProvider,
                    &selection("DEEPSEEK", "deepseek-chat"),
                    vec![CompletionMessage::user("extract")],
                    |s| s.to_string(),
                    None,
                    None,
                    Some("char-9"),
                    Some("memory-extraction-self"),
                )
                .await
        };
        assert!(!r.success);

        let captured = logs.lock().unwrap().join("\n");
        assert!(
            captured.contains("[CheapLLM] Task failed"),
            "a failed cheap task must never be silent; captured:\n{captured}"
        );
        // The exact target, not a prefix: `contains("quilltap::cheap_llm")` also
        // matches `quilltap::cheap_llm_anything`, which is how the mutation pass
        // first found this assertion too weak to fail.
        assert!(
            captured
                .lines()
                .any(|l| l.starts_with("WARN quilltap::cheap_llm ")),
            "the failure is a WARN on quilltap::cheap_llm; captured:\n{captured}"
        );
        for field in [
            "task_type=memory-extraction-self",
            "character_id=char-9",
            "provider=DEEPSEEK",
            "model=deepseek-chat",
            "error=Request timed out after 40000ms",
        ] {
            assert!(
                captured.contains(field),
                "the failure warn is missing {field}; captured:\n{captured}"
            );
        }
        // `chat_id` is present-but-empty on an executor with no log config —
        // v4's parameter is `undefined` there and its logger drops nothing.
        assert!(
            captured.contains("chat_id="),
            "the failure warn must carry chat_id; captured:\n{captured}"
        );
    }

    /// A SUCCESSFUL cheap task must not warn — the silence half of the pin.
    #[tokio::test]
    async fn a_successful_cheap_task_is_silent() {
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        use tracing_subscriber::layer::SubscriberExt;

        let logs = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        let r = {
            let _guard = tracing::subscriber::set_default(subscriber);
            CheapLlmTaskExecutor::new()
                .execute(
                    &RecordingProvider::new(),
                    &selection("DEEPSEEK", "deepseek-chat"),
                    vec![CompletionMessage::user("extract")],
                    |s| s.to_string(),
                    None,
                    None,
                    None,
                    Some("memory-extraction-self"),
                )
                .await
        };
        assert!(r.success);
        let captured = logs.lock().unwrap().join("\n");
        assert!(
            !captured.contains("[CheapLLM] Task failed"),
            "a successful task must not warn; captured:\n{captured}"
        );
    }

    /// v4 `providerBudgetFor`: the provider's own budget sits strictly inside
    /// the caller's deadline (40 s remote / 175 s local) and reaches
    /// `CompletionParams` on every arm.
    #[tokio::test]
    async fn the_provider_budget_sits_five_seconds_inside_the_deadline() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let messages = vec![CompletionMessage::user("hi")];

        let remote_rec = RecordingProvider::new();
        CheapLlmTaskExecutor::new()
            .execute(
                &remote_rec,
                &selection("DEEPSEEK", "deepseek-v4-flash"),
                messages.clone(),
                |s| s.to_string(),
                None,
                None,
                None,
                Some("title-chat"),
            )
            .await;
        assert_eq!(remote_rec.budgets(), vec![Some(40_000)]);

        let local_rec = RecordingProvider::new();
        CheapLlmTaskExecutor::new()
            .execute(
                &local_rec,
                &local_selection("OLLAMA", "qwen3"),
                messages.clone(),
                |s| s.to_string(),
                None,
                None,
                None,
                Some("title-chat"),
            )
            .await;
        assert_eq!(local_rec.budgets(), vec![Some(175_000)]);

        // P4.D127 (v4 `8872d7efc`): a REMOTE compression attempt hands the
        // provider 70 s — five seconds inside its own 75 s deadline — so the
        // per-task override reaches `CompletionParams`, not just `deadline_for`.
        let compress_rec = RecordingProvider::new();
        CheapLlmTaskExecutor::new()
            .execute(
                &compress_rec,
                &selection("DEEPSEEK", "deepseek-v4-flash"),
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                Some("compress-conversation-history"),
            )
            .await;
        assert_eq!(compress_rec.budgets(), vec![Some(70_000)]);

        // v4's own two assertions, restated: strictly inside our deadline (so
        // the provider aborts its socket first), and the local budget outranks
        // the remote *deadline*.
        assert!(
            provider_budget_for(&selection("DEEPSEEK", "m"), Some("title-chat"))
                < CHEAP_LLM_TASK_TIMEOUT_MS as i64
        );
        assert!(provider_budget_for(&selection("DEEPSEEK", "m"), Some("title-chat")) > 0);
        assert!(
            provider_budget_for(&local_selection("OLLAMA", "m"), Some("title-chat"))
                > CHEAP_LLM_TASK_TIMEOUT_MS as i64
        );
        // …and the local exemption reaches the provider budget too: a local
        // compression attempt keeps 175 s, not the override's 70 s.
        assert_eq!(
            provider_budget_for(&local_selection("OLLAMA", "m"), Some("compress-memories")),
            175_000
        );
    }

    /// The temperature-rejection retry re-issues inside the SAME attempt, so
    /// both provider calls carry the same budget (v4 spreads one `baseParams`).
    #[tokio::test]
    async fn every_arm_of_one_attempt_carries_the_same_budget() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let messages = vec![CompletionMessage::user("hi")];
        struct TempRejectThenRecord(RecordingProvider);
        impl CompletionProvider for TempRejectThenRecord {
            fn send_message(
                &self,
                provider: &str,
                base_url: Option<&str>,
                params: &CompletionParams,
            ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send
            {
                let reject = params.temperature.is_some();
                let inner = self.0.send_message(provider, base_url, params);
                async move {
                    if reject {
                        return Err(CompletionError::new("model does not support temperature"));
                    }
                    inner.await
                }
            }
        }
        let p = TempRejectThenRecord(RecordingProvider::new());
        let r = CheapLlmTaskExecutor::new()
            .execute(
                &p,
                &selection("OPENAI", "gpt-x"),
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                Some("title-chat"),
            )
            .await;
        assert!(r.success);
        assert_eq!(p.0.budgets(), vec![Some(40_000), Some(40_000)]);
    }

    /// Each attempt gets a FRESH budget. The safe provider burns 30 s and comes
    /// back empty, the uncensored fallback then stalls — with per-attempt
    /// budgets that fires at 30 + 45 = 75 s; a single task-level budget would
    /// have fired at 45 s.
    #[tokio::test(start_paused = true)]
    async fn each_attempt_gets_a_fresh_budget() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let messages = vec![CompletionMessage::user("spicy")];
        let provider = SlowEmptyThenStall {
            first_provider: "ANTHROPIC".to_string(),
            first_delay_ms: 30_000,
        };
        let danger = DangerousContentSettings {
            mode: "AUTO_ROUTE".to_string(),
            uncensored_text_profile_id: Some("u1".to_string()),
        };
        let profiles = vec![
            CheapLlmProfile {
                id: "cur".to_string(),
                provider: "ANTHROPIC".to_string(),
                model_name: "safe".to_string(),
                ..Default::default()
            },
            CheapLlmProfile {
                id: "u1".to_string(),
                provider: "OLLAMA".to_string(),
                model_name: "dolphin".to_string(),
                ..Default::default()
            },
        ];
        let options = UncensoredFallbackOptions {
            danger_settings: &danger,
            available_profiles: &profiles,
            is_dangerous_chat: None,
        };

        let started = tokio::time::Instant::now();
        let r = CheapLlmTaskExecutor::new()
            .execute(
                &provider,
                &selection("ANTHROPIC", "safe"),
                messages,
                |s| s.to_string(),
                Some(&options),
                None,
                None,
                Some("summarize-chat"),
            )
            .await;

        assert!(!r.success);
        // The fallback selection is `isLocal: false` in v4, so its fresh budget
        // is the remote 45 s — reported as such.
        assert!(r.error.as_deref().unwrap().contains("45000ms budget"));
        let elapsed = started.elapsed();
        assert!(
            elapsed >= std::time::Duration::from_millis(75_000),
            "the second attempt shared the first's budget (total {elapsed:?}, expected ~75s)"
        );
    }

    /// A fired deadline narrates itself: v4's abandonment WARN with its whole
    /// field set (the incident was invisible precisely because a stall logged
    /// nothing), plus — per the standing P4.13 ruled divergence — the failed-call
    /// error row carrying the timeout message. v4 writes no row for an
    /// unfinished call; v5 writes one for every failed cheap call, and a
    /// timeout is a failed call.
    #[tokio::test(start_paused = true)]
    async fn a_fired_deadline_warns_and_writes_the_ruled_error_row() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        use tracing_subscriber::layer::SubscriberExt;

        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.db");
        let ll_path = dir.path().join("llm-logs.db");
        drop(Writer::open_writable(&main_path, PEPPER).unwrap());
        {
            let w = Writer::open_writable(&ll_path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE llm_logs (\
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
                       chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
                       modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
                       request TEXT, response TEXT, usage TEXT, \
                       cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
                       durationMs REAL, createdAt TEXT, updatedAt TEXT);",
                )
                .unwrap();
        }
        let db = Db::open(
            DbPaths {
                main: main_path,
                mount_index: None,
                llm_logs: Some(ll_path),
            },
            PEPPER,
        )
        .unwrap();

        let exec = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: "user-1".to_string(),
            chat_id: Some("chat-7".to_string()),
            message_id: None,
            ctx: LogContext::none(),
        });

        let logs = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        let r = {
            let _guard = tracing::subscriber::set_default(subscriber);
            exec.execute(
                &StallingProvider,
                &selection("DEEPSEEK", "deepseek-chat"),
                vec![CompletionMessage::user("summarize")],
                |s| s.to_string(),
                None,
                None,
                Some("char-9"),
                Some("memory-recap-summarization"),
            )
            .await
        };
        assert!(!r.success);

        let captured = logs.lock().unwrap().join("\n");
        // ⚠ [P4.63] The level+target assert used to be
        // `captured.contains("WARN quilltap::cheap_llm")`, and the field asserts
        // ran against the whole capture. Both were looser than they read:
        // `contains` on a target is a PREFIX match (a sibling target such as
        // `quilltap::cheap_llm_exec` satisfies it — the P4.D127 finding), and
        // this very test drives THREE events on `quilltap::cheap_llm`
        // (the abandonment WARN, `Cheap-LLM call failed`, `[CheapLLM] Task
        // failed`), all of which carry `provider=`/`model=`/`character_id=`.
        // So the target is matched as a whole token — the trailing space is
        // what ends it — and every field is asserted on the abandonment's OWN
        // line.
        let abandonment = captured
            .lines()
            .find(|l| {
                l.starts_with("WARN quilltap::cheap_llm ")
                    && l.contains("Abandoned a stalled provider call")
            })
            .unwrap_or_else(|| {
                panic!(
                    "a stall must never be silent again: no line is a WARN on \
                     exactly `quilltap::cheap_llm` carrying the abandonment \
                     message; captured:\n{captured}"
                )
            });
        for field in [
            "chat_id=chat-7",
            "character_id=char-9",
            "provider=DEEPSEEK",
            "model=deepseek-chat",
            "task_type=memory-recap-summarization",
            "timeout_ms=45000",
            "elapsed=45000",
        ] {
            assert!(
                abandonment.contains(field),
                "the abandonment warn is missing {field}; line:\n{abandonment}"
            );
        }

        let rows: Vec<(String, String, String, String, Option<String>)> = db
            .read_llm_logs(|conn| {
                let mut stmt = conn
                    .prepare("SELECT type, provider, modelName, response, usage FROM llm_logs")?;
                let out = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(out)
            })
            .unwrap();
        assert_eq!(rows.len(), 1, "exactly one error row for the abandonment");
        let (typ, provider_col, model, response, usage) = &rows[0];
        assert_eq!(typ, "SUMMARIZATION");
        assert_eq!(provider_col, "DEEPSEEK");
        assert_eq!(model, "deepseek-chat");
        assert!(
            response.contains(
                "Cheap LLM task (memory-recap-summarization) exceeded its 45000ms budget"
            ),
            "response: {response}"
        );
        assert_eq!(usage.as_deref(), None, "an error row carries no usage");
    }

    #[tokio::test]
    async fn temperature_rejection_caches_and_retries() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let messages = vec![CompletionMessage::user("hi")];
        let provider = CannedCompletionProvider::new()
            .with_failure("OPENAI", "m", Some(0.3), &messages, "does not support temp")
            .with_response("OPENAI", "m", None, &messages, "ok", None);
        let exec = CheapLlmTaskExecutor::new();
        let sel = selection("OPENAI", "m");

        let r = exec
            .execute(
                &provider,
                &sel,
                messages.clone(),
                |s| s.to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(r.success);
        assert_eq!(r.result.as_deref(), Some("ok"));

        // Second call goes straight to the no-temperature form (cached) — the
        // 0.3 entry would fail if consulted again, and it isn't.
        let r2 = exec
            .execute(
                &provider,
                &sel,
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(r2.success);
    }

    #[tokio::test]
    async fn empty_response_falls_back_to_uncensored_provider() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        let messages = vec![CompletionMessage::user("spicy")];
        let provider = CannedCompletionProvider::new()
            .with_response("ANTHROPIC", "safe", Some(0.3), &messages, "", None)
            .with_response("OLLAMA", "dolphin", Some(0.3), &messages, "answer", None);
        let exec = CheapLlmTaskExecutor::new();
        let sel = selection("ANTHROPIC", "safe");

        let danger = DangerousContentSettings {
            mode: "AUTO_ROUTE".to_string(),
            uncensored_text_profile_id: Some("u1".to_string()),
        };
        let profiles = vec![
            CheapLlmProfile {
                id: "cur".to_string(),
                provider: "ANTHROPIC".to_string(),
                model_name: "safe".to_string(),
                ..Default::default()
            },
            CheapLlmProfile {
                id: "u1".to_string(),
                provider: "OLLAMA".to_string(),
                model_name: "dolphin".to_string(),
                ..Default::default()
            },
        ];
        let options = UncensoredFallbackOptions {
            danger_settings: &danger,
            available_profiles: &profiles,
            is_dangerous_chat: None,
        };

        let r = exec
            .execute(
                &provider,
                &sel,
                messages,
                |s| s.to_string(),
                Some(&options),
                None,
                None,
                None,
            )
            .await;
        assert!(r.success);
        assert_eq!(r.result.as_deref(), Some("answer"));
    }

    /// The writer's through-a-real-call-site proof, in process: a
    /// `with_logging` executor drives one cheap-LLM task through the real
    /// single-writer `Db` (main + llm-logs partitions) and lands exactly one
    /// `llm_logs` row with the v4-shaped summary — the `task_type` mapped to the
    /// log type, the sent temperature/maxTokens, the response content + usage,
    /// and no `durationMs` (the cheap path sets none). The byte-exact v4 diff is
    /// the differential-side proof (W4.7e3's per-oracle regens); this catches
    /// wiring regressions the compile-check can't.
    #[tokio::test]
    async fn logging_writes_one_row_through_the_real_writer() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        // The differential test pepper (32 bytes of "testpepper…", base64).
        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.db");
        let ll_path = dir.path().join("llm-logs.db");

        // The main file only needs to exist — `is_logging_enabled`'s
        // `chat_settings` read errors on the missing table and defaults to
        // enabled (v4's catch → DEFAULT_LOGGING_SETTINGS).
        drop(Writer::open_writable(&main_path, PEPPER).unwrap());
        // Materialize the `llm_logs` table (the TS oracle's `generateCreateTable`
        // in the tier-2 harness; here hand-rolled for the Rust-only proof).
        {
            let w = Writer::open_writable(&ll_path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE llm_logs (\
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
                       chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
                       modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
                       request TEXT, response TEXT, usage TEXT, \
                       cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
                       durationMs REAL, createdAt TEXT, updatedAt TEXT);",
                )
                .unwrap();
        }

        let db = Db::open(
            DbPaths {
                main: main_path,
                mount_index: None,
                llm_logs: Some(ll_path),
            },
            PEPPER,
        )
        .unwrap();

        let messages = vec![
            CompletionMessage::system("summarize this"),
            CompletionMessage::user("the transcript"),
        ];
        let provider = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "claude-haiku",
            Some(0.3),
            &messages,
            "a tidy summary",
            Some(CompletionUsage {
                prompt_tokens: 10,
                completion_tokens: 4,
                total_tokens: 14,
            }),
        );

        let exec = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: "user-1".to_string(),
            chat_id: Some("chat-1".to_string()),
            message_id: None,
            ctx: LogContext::none(),
        });
        let sel = selection("ANTHROPIC", "claude-haiku");

        let r = exec
            .execute(
                &provider,
                &sel,
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                // Maps to SUMMARIZATION via `map_task_type_to_log_type`.
                Some("summarize-chat"),
            )
            .await;
        assert!(r.success);

        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
            Option<f64>,
            Option<String>,
        )> = db
            .read_llm_logs(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT type, provider, modelName, chatId, userId, request, response, \
                     usage, durationMs, connectionProfileId FROM llm_logs",
                )?;
                let out = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                            row.get(9)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(out)
            })
            .unwrap();

        assert_eq!(rows.len(), 1, "exactly one llm_logs row written");
        let (
            typ,
            provider_col,
            model,
            chat,
            user,
            request,
            response,
            usage,
            duration,
            conn_profile,
        ) = &rows[0];
        // v4 `0cde7fbc`: `selection.connectionProfileId ?? null`.
        assert_eq!(conn_profile.as_deref(), Some("cur"));
        assert_eq!(typ, "SUMMARIZATION");
        assert_eq!(provider_col, "ANTHROPIC");
        assert_eq!(model, "claude-haiku");
        assert_eq!(chat.as_deref(), Some("chat-1"));
        assert_eq!(user, "user-1");
        // The sent temperature (0.3) + the strict-floor maxTokens (2048).
        assert!(
            request.contains("\"temperature\":0.3"),
            "request: {request}"
        );
        assert!(request.contains("\"maxTokens\":2048"), "request: {request}");
        assert!(response.contains("a tidy summary"), "response: {response}");
        // A SUCCESS row always logs `error: null` — the discriminator the
        // ruled failure row (below) is distinguishable by.
        assert!(response.contains("\"error\":null"), "response: {response}");
        assert!(
            usage
                .as_deref()
                .unwrap_or("")
                .contains("\"totalTokens\":14"),
            "usage: {usage:?}"
        );
        // v4 `0cde7fbc` made `durationMs` a REQUIRED argument of the shared
        // cheap path's `logCall`, so the row now always carries one. The value
        // is a real measured wall clock (0 against a canned provider on a fast
        // machine, 1+ on a slow one) — assert PRESENCE, never a number.
        assert!(duration.is_some(), "the cheap path now measures durationMs");
    }

    /// The RULED failed-call error row (P4.13 unit 6, a deliberate divergence
    /// from v4 — see `log_failed_call`): a failing provider call writes ONE
    /// `llm_logs` row carrying the provider, model, mapped task type, and the
    /// error text, with empty content and no usage. No oracle differential is
    /// possible for a deliberate divergence — this test IS the pin.
    #[tokio::test]
    async fn failed_call_writes_the_ruled_error_row() {
        // Drives the REAL wrapped path, so it opens real activity spans on the
        // process-global registry; serialize with the exact-count registry tests
        // (the drop also zeroes, so this test leaves no residue either).
        let _activity = crate::services::activity_registry::ActivityTestGuard::new();
        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.db");
        let ll_path = dir.path().join("llm-logs.db");
        drop(Writer::open_writable(&main_path, PEPPER).unwrap());
        {
            let w = Writer::open_writable(&ll_path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE llm_logs (\
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
                       chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
                       modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
                       request TEXT, response TEXT, usage TEXT, \
                       cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
                       durationMs REAL, createdAt TEXT, updatedAt TEXT);",
                )
                .unwrap();
        }
        let db = Db::open(
            DbPaths {
                main: main_path,
                mount_index: None,
                llm_logs: Some(ll_path),
            },
            PEPPER,
        )
        .unwrap();

        let messages = vec![CompletionMessage::user("extract memories")];
        // A provider that fails the 0.3-temperature attempt with a
        // NON-temperature error (no retry — the terminal fall-through arm).
        let provider = CannedCompletionProvider::new().with_failure(
            "DEEPSEEK",
            "deepseek-chat",
            Some(0.3),
            &messages,
            "402 Payment Required: Insufficient Balance",
        );

        let exec = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: "user-1".to_string(),
            chat_id: Some("chat-9".to_string()),
            message_id: None,
            ctx: LogContext::none(),
        });
        let sel = selection("DEEPSEEK", "deepseek-chat");

        let r = exec
            .execute(
                &provider,
                &sel,
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                Some("memory-extraction-self"),
            )
            .await;
        assert!(!r.success);

        let rows: Vec<(String, String, String, String, Option<String>)> = db
            .read_llm_logs(|conn| {
                let mut stmt = conn
                    .prepare("SELECT type, provider, modelName, response, usage FROM llm_logs")?;
                let out = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(out)
            })
            .unwrap();

        assert_eq!(rows.len(), 1, "exactly one error row written");
        let (typ, provider_col, model, response, usage) = &rows[0];
        assert_eq!(typ, "MEMORY_EXTRACTION");
        assert_eq!(provider_col, "DEEPSEEK");
        assert_eq!(model, "deepseek-chat");
        assert!(
            response.contains("\"error\":\"402 Payment Required: Insufficient Balance\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"content\":\"\""),
            "response: {response}"
        );
        assert_eq!(usage.as_deref(), None, "an error row carries no usage");
    }
}
