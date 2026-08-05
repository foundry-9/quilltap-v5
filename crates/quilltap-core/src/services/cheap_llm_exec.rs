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

/// v4 `deadlineFor(selection)` — the caller-side deadline for one attempt.
pub fn cheap_llm_deadline_for(selection: &CheapLlmSelection) -> u64 {
    if selection.is_local {
        CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS
    } else {
        CHEAP_LLM_TASK_TIMEOUT_MS
    }
}

/// v4 `providerBudgetFor(selection)` — the hard per-request budget handed to the
/// provider ([`CompletionParams::request_timeout_ms`]), five seconds inside the
/// caller's own deadline: 40 000 ms remote / 175 000 ms local.
// The subtraction below must never underflow: both are v4 literals today,
// but if either ever moves, make it a compile error rather than a wrap.
const _: () = assert!(CHEAP_LLM_TASK_TIMEOUT_MS > PROVIDER_BUDGET_HEADROOM_MS);
const _: () = assert!(CHEAP_LLM_TASK_TIMEOUT_LOCAL_MS > PROVIDER_BUDGET_HEADROOM_MS);

pub fn provider_budget_for(selection: &CheapLlmSelection) -> i64 {
    (cheap_llm_deadline_for(selection) - PROVIDER_BUDGET_HEADROOM_MS) as i64
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
}

impl CheapLlmTaskExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// An executor that logs each successful provider call into `llm_logs` (v4's
    /// `sendToProvider` `logLLMCall`). The host / job runner / a logging
    /// differential constructs this; the request-path spine keeps [`new`].
    pub fn with_logging(log: CheapLlmLogConfig) -> Self {
        Self {
            profiles_without_custom_temp: Mutex::new(HashSet::new()),
            log: Some(log),
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
            max_tokens: effective_max_tokens,
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
            request_timeout_ms: Some(provider_budget_for(selection)),
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
        let timeout_ms = cheap_llm_deadline_for(selection);
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
            Err(error) => CheapLlmTaskResult {
                success: false,
                result: None,
                error: Some(error.message),
                usage: None,
            },
        }
    }
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
    }

    /// The incident: a remote provider that accepts the request and never
    /// answers is abandoned at 45 s, and the task fails soft with v4's message.
    #[tokio::test(start_paused = true)]
    async fn a_remote_attempt_is_abandoned_at_its_45s_deadline() {
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

    /// v4 `providerBudgetFor`: the provider's own budget sits strictly inside
    /// the caller's deadline (40 s remote / 175 s local) and reaches
    /// `CompletionParams` on every arm.
    #[tokio::test]
    async fn the_provider_budget_sits_five_seconds_inside_the_deadline() {
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
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                Some("title-chat"),
            )
            .await;
        assert_eq!(local_rec.budgets(), vec![Some(175_000)]);

        // v4's own two assertions, restated: strictly inside our deadline (so
        // the provider aborts its socket first), and the local budget outranks
        // the remote *deadline*.
        assert!(
            provider_budget_for(&selection("DEEPSEEK", "m")) < CHEAP_LLM_TASK_TIMEOUT_MS as i64
        );
        assert!(provider_budget_for(&selection("DEEPSEEK", "m")) > 0);
        assert!(
            provider_budget_for(&local_selection("OLLAMA", "m")) > CHEAP_LLM_TASK_TIMEOUT_MS as i64
        );
    }

    /// The temperature-rejection retry re-issues inside the SAME attempt, so
    /// both provider calls carry the same budget (v4 spreads one `baseParams`).
    #[tokio::test]
    async fn every_arm_of_one_attempt_carries_the_same_budget() {
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
                       modelName TEXT, request TEXT, response TEXT, usage TEXT, \
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
        assert!(
            captured.contains("Abandoned a stalled provider call"),
            "a stall must never be silent again; captured:\n{captured}"
        );
        assert!(
            captured.contains("WARN quilltap::cheap_llm"),
            "the abandonment is a WARN on quilltap::cheap_llm; captured:\n{captured}"
        );
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
                captured.contains(field),
                "the abandonment warn is missing {field}; captured:\n{captured}"
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
                       modelName TEXT, request TEXT, response TEXT, usage TEXT, \
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
        )> = db
            .read_llm_logs(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT type, provider, modelName, chatId, userId, request, response, \
                     usage, durationMs FROM llm_logs",
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
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(out)
            })
            .unwrap();

        assert_eq!(rows.len(), 1, "exactly one llm_logs row written");
        let (typ, provider_col, model, chat, user, request, response, usage, duration) = &rows[0];
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
        // The cheap path sets no durationMs.
        assert_eq!(*duration, None);
    }

    /// The RULED failed-call error row (P4.13 unit 6, a deliberate divergence
    /// from v4 — see `log_failed_call`): a failing provider call writes ONE
    /// `llm_logs` row carrying the provider, model, mapped task type, and the
    /// error text, with empty content and no usage. No oracle differential is
    /// possible for a deliberate divergence — this test IS the pin.
    #[tokio::test]
    async fn failed_call_writes_the_ruled_error_row() {
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
                       modelName TEXT, request TEXT, response TEXT, usage TEXT, \
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
