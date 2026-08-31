//! The empty-response provider failover (Phase-3 Unit-3 wave 3) — v4
//! `provider-failover.service.ts` (`attemptEmptyResponseRecovery`,
//! `getEmptyResponseReason`, `restreamInto`).
//!
//! When the primary stream returns an empty assistant response, this service
//! tries to recover in two stages, mutating the shared [`StreamingState`] in
//! place (the caller reads the updated `full_response` / `effective_profile`
//! afterward):
//!
//!   1. **Same-provider retry** — if the content was NOT flagged dangerous, an
//!      empty response is likely transient; re-stream the same request once.
//!   2. **Uncensored failover** — if still empty AND Concierge Auto-Route is on
//!      with an uncensored text profile, resolve an uncensored provider and
//!      re-stream. On a non-empty result the state's `effective_profile` /
//!      `effective_api_key` switch to the uncensored provider (so the finalizer
//!      records the reroute).
//!
//! `get_empty_response_reason` returns the exact user-facing strings that
//! describe the outcome: the named moderation-refusal sentence first (bug 93,
//! v4 `a14a1811`), then the five inference sentences.
//!
//! ## The dangerous-content routing seam
//!
//! v4's `resolveProviderForDangerousContent` reads the `connections` repo and
//! decrypts an API key — host-side concerns (key decryption lands with the
//! Phase-4 transport layer). Rather than pull those unported subsystems into the
//! core, the uncensored resolution is injected as a [`DangerousContentRouter`]
//! seam: given the current profile + settings + user, it returns the route
//! decision. The differential drives a canned router that mirrors v4's real
//! resolution over the same corpus, so the failover *logic* (retry gating,
//! reroute-vs-same-profile guard, the state switch, the reason strings) is what
//! is verified here; the DB-reading resolution is verified where it is ported.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::model::stream::{
    StreamCacheUsage, StreamError, StreamParams, StreamUsage, StreamingCompletionProvider,
};

use super::chat_events::{ChatEvent, EventSink, StatusPayload};
use super::fallback_repos::FallbackChainRepos;
use super::primary_stream::{
    apply_reasoning_chunk, flush_reasoning_segment, log_chat_message_call, EffectiveProfile,
    StreamLogCtx, StreamingState,
};
use crate::llm_fallback::{
    build_fallback_chain, classify_fallback_trigger, record_attempt, FallbackAttempt,
    FallbackCandidateKind, FallbackContext, FallbackError, FallbackProfile, FallbackPurpose,
    FallbackTrigger,
};

/// v4 `DangerousContentSettings` subset the failover reads.
#[derive(Clone, Debug, PartialEq)]
pub struct DangerSettings {
    /// v4 `mode` — the failover only reroutes when this is `"AUTO_ROUTE"`.
    pub mode: String,
    /// v4 `uncensoredTextProfileId` — reroute is gated on this being set.
    pub uncensored_text_profile_id: Option<String>,
}

/// The route decision (v4 `DangerousProviderRouteResult` subset): whether a
/// reroute happened, and the effective profile + api key to use.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteResult {
    pub rerouted: bool,
    pub connection_profile: EffectiveProfile,
    pub api_key: String,
}

/// The dangerous-content routing seam — v4
/// `resolveProviderForDangerousContent`. Given the current profile / api key /
/// settings / user, resolve the uncensored provider (or return the original with
/// `rerouted: false`). Injected so the DB-reading + key-decryption resolution
/// stays host-side.
pub trait DangerousContentRouter {
    fn resolve(
        &self,
        original_profile: &EffectiveProfile,
        original_api_key: &str,
        settings: &DangerSettings,
        user_id: &str,
    ) -> impl std::future::Future<Output = RouteResult> + Send;
}

/// Options for `attempt_empty_response_recovery` (v4
/// `AttemptEmptyResponseRecoveryOptions`, minus controller/encoder → the
/// [`EventSink`], and the repos → the [`DangerousContentRouter`]).
pub struct AttemptEmptyResponseRecoveryOptions<'a> {
    /// The mutable streaming state (holds `effective_profile` / `effective_api_key`).
    pub state: &'a mut StreamingState,
    /// Number of tool result messages produced this turn (v4 `toolMessagesLength`).
    /// A non-empty tool loop means the "empty response" is expected — skip.
    pub tool_messages_length: usize,
    pub content_was_flagged_dangerous: bool,
    pub danger_settings: DangerSettings,
    /// The original (pre-failover) profile, for the "both empty" log (v4
    /// `connectionProfile`).
    pub connection_profile: EffectiveProfile,
    /// The stream params to re-issue (messages / model / temperature / …).
    pub params: StreamParams,
    pub user_id: String,
    pub chat_id: String,
    pub character_id: String,
    pub character_name: String,
    /// Capability flags for the chain (v4's `fallbackContext`), minus the three
    /// the recovery fills in itself (`userId`, `purpose`, `alreadyTried`).
    /// `None` keeps the pre-4.10 two-step behaviour, which is what a caller
    /// with no repository handle wants.
    pub fallback_context: Option<ChainCapabilities>,
}

/// The capability half of a [`FallbackContext`] — what the CALLER knows about
/// this turn. v4's `Omit<FallbackContext, 'userId' | 'purpose' | 'alreadyTried'>`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChainCapabilities {
    pub dangerous: bool,
    pub needs_vision: bool,
    pub needs_tools: bool,
}

/// The flags v4 `attemptEmptyResponseRecovery` returns — which retries ran.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmptyResponseRecoveryFlags {
    pub uncensored_retry_attempted: bool,
    pub same_provider_retry_attempted: bool,
    /// Whether the profile's fallback chain was walked after the two local
    /// retries came back empty (v4 `65f5021c8`).
    pub chain_fallback_attempted: bool,
    /// The failed attempts from that chain walk, for the log and the message.
    pub chain_attempts: Vec<FallbackAttempt>,
}

/// The pieces needed to write the v4 failover `CHAT_MESSAGE` `llm_logs` rows.
///
/// v4's `restreamInto` calls `streamMessage({ …, userId, messageId, chatId })`
/// (provider-failover.service.ts:266) — so each retry leg that reaches its
/// terminal chunk logs a `CHAT_MESSAGE` row through the SAME wrapper the primary
/// stream uses (streaming.service.ts:407). Since v4 `65f5021c8` the call site
/// passes `characterId` too, so those rows carry the character (before it, they
/// carried `characterId = NULL`). `userId`/`chatId` come
/// from [`AttemptEmptyResponseRecoveryOptions`]; `db` + `message_id` (v4's
/// `preGeneratedAssistantMessageId`) are supplied here.
///
/// The orchestrator spine does not yet thread this (its
/// [`AttemptEmptyResponseRecoveryOptions`] construction carries no `db` /
/// `preGeneratedAssistantMessageId`), so its failover call uses the no-logging
/// entry point [`attempt_empty_response_recovery`] — wiring the spine's failover
/// log is a spine-owner follow-up. The differential drives
/// [`attempt_empty_response_recovery_with_log`] to prove the row shape.
#[derive(Clone, Copy)]
pub struct FailoverLogCtx<'a> {
    pub db: &'a Db,
    /// v4 `preGeneratedAssistantMessageId` (the `messageId` on the log row).
    pub message_id: &'a str,
}

/// v4 `attemptEmptyResponseRecovery` — the orchestrator entry point. Delegates to
/// [`attempt_empty_response_recovery_with_log`] with no `llm_logs` writer (the
/// spine does not yet thread the db + pre-generated message id into the failover).
pub async fn attempt_empty_response_recovery<P, S, R, FR>(
    provider: &P,
    sink: &S,
    router: &R,
    repos: Option<&FR>,
    opts: AttemptEmptyResponseRecoveryOptions<'_>,
) -> EmptyResponseRecoveryFlags
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: DangerousContentRouter,
    FR: FallbackChainRepos,
{
    attempt_empty_response_recovery_with_log(provider, sink, router, repos, opts, None).await
}

/// v4 `attemptEmptyResponseRecovery`, with the optional failover `CHAT_MESSAGE`
/// logging (v4's `restreamInto` logs per `streamMessage` call). Mutates `state` in
/// place.
pub async fn attempt_empty_response_recovery_with_log<P, S, R, FR>(
    provider: &P,
    sink: &S,
    router: &R,
    repos: Option<&FR>,
    opts: AttemptEmptyResponseRecoveryOptions<'_>,
    log: Option<FailoverLogCtx<'_>>,
) -> EmptyResponseRecoveryFlags
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: DangerousContentRouter,
    FR: FallbackChainRepos,
{
    let AttemptEmptyResponseRecoveryOptions {
        state,
        tool_messages_length,
        content_was_flagged_dangerous,
        danger_settings,
        connection_profile,
        params,
        user_id,
        chat_id,
        character_id,
        character_name,
        fallback_context,
    } = opts;

    // v4 gates the wrapper's log on `if (userId)`; `restreamInto` always passes a
    // (real) userId, and no `characterId`.
    //
    // LogContext: none. Threading a run-id context into the failover legs rides
    // the standing spine follow-up (W4.11b — the orchestrator does not wire
    // failover logging at all yet, so no autonomous caller reaches this today);
    // when that threading lands, `FailoverLogCtx` grows the context field.
    let none_ctx = crate::services::llm_logging::LogContext::none();
    let stream_log = log.filter(|_| !user_id.is_empty()).map(|l| StreamLogCtx {
        db: l.db,
        user_id: &user_id,
        chat_id: &chat_id,
        message_id: l.message_id,
        // ⚠ v4 `65f5021c8` ADDED `characterId: opts.character.id` to
        // `restreamInto`'s `streamMessage` call, alongside the optional `stop`.
        // Before that commit the call site passed none and every failover leg's
        // `CHAT_MESSAGE` row carried `characterId = NULL`; now they all carry
        // the character. Caught by the tier-3 `llm_logs` dump — the results and
        // the event traces both matched, and only the log rows disagreed.
        character_id: Some(&character_id),
        log_context: &none_ctx,
        started_at_ms: crate::clock::now_unix_ms(),
    });

    let mut flags = EmptyResponseRecoveryFlags::default();

    // Not empty (or a tool loop ran) → nothing to recover.
    if !crate::jsstr::js_trim(&state.full_response).is_empty() || tool_messages_length > 0 {
        return flags;
    }

    // --- Same-provider retry (only when not flagged dangerous) ---
    // Profiles this recovery has already spent. The chain walk at the bottom
    // reads it so a route that has already come back empty isn't asked twice.
    let mut tried_profile_ids: Vec<String> = Vec::new();

    if !content_was_flagged_dangerous {
        flags.same_provider_retry_attempted = true;
        if let Some(p) = state.effective_profile.as_ref() {
            tried_profile_ids.push(p.id.clone());
        }

        sink.emit(ChatEvent::status(StatusPayload {
            stage: "retrying".into(),
            message: "Empty response received — retrying...".into(),
            tool_name: None,
            character_name: Some(character_name.clone()),
            character_id: Some(character_id.clone()),
        }));

        // v4 logs the retry against `state.effectiveProfile` (same-provider).
        let same_profile = state
            .effective_profile
            .clone()
            .unwrap_or_else(|| connection_profile.clone());
        // Best-effort: a retry error is logged and swallowed (v4 catches it).
        let _ = restream_into(
            provider,
            state,
            sink,
            &same_profile,
            &params,
            // v4's empty-response callers pass no `stop`.
            None,
            &character_name,
            &character_id,
            stream_log.as_ref(),
        )
        .await;
    }

    // --- Uncensored failover ---
    if crate::jsstr::js_trim(&state.full_response).is_empty()
        && danger_settings.mode == "AUTO_ROUTE"
        && danger_settings.uncensored_text_profile_id.is_some()
    {
        flags.uncensored_retry_attempted = true;

        let original_profile = state
            .effective_profile
            .clone()
            .unwrap_or_else(|| connection_profile.clone());
        let route = router
            .resolve(
                &original_profile,
                &state.effective_api_key,
                &danger_settings,
                &user_id,
            )
            .await;

        // v4: if rerouted to the SAME profile id, do nothing (the empty `if`
        // block); else if rerouted, restream on the uncensored provider.
        if route.rerouted && route.connection_profile.id == original_profile.id {
            // No-op (v4's empty branch — nothing to gain re-hitting the same one).
        } else if route.rerouted {
            tried_profile_ids.push(route.connection_profile.id.clone());

            sink.emit(ChatEvent::status(StatusPayload {
                stage: "rerouting".into(),
                message: "Retrying with uncensored provider...".into(),
                tool_name: None,
                character_name: Some(character_name.clone()),
                character_id: Some(character_id.clone()),
            }));

            // v4's `streamMessage` takes its model from `connectionProfile.modelName`
            // (not `modelParams`), so a reroute switches the model too; v4 logs the
            // retry against the rerouted `connectionProfile`.
            let mut re_params = params.clone();
            re_params.model = route.connection_profile.model_name.clone();
            let _ = restream_into(
                provider,
                state,
                sink,
                &route.connection_profile,
                &re_params,
                None,
                &character_name,
                &character_id,
                stream_log.as_ref(),
            )
            .await;

            if !crate::jsstr::js_trim(&state.full_response).is_empty() {
                // The uncensored retry produced content — switch the effective
                // profile/key so the finalizer records the reroute.
                state.effective_profile = Some(route.connection_profile.clone());
                state.effective_api_key = route.api_key.clone();
            }
        }
    }

    // Third and last: the effective profile's own fallback chain.
    //
    // Deliberately last. An empty body is usually transient (the same-profile
    // retry above catches that), and when it isn't it is usually a refusal — a
    // *content* problem, which the uncensored reroute exists to answer. Only
    // once both have come back empty is it worth concluding the route itself is
    // no good and calling for the understudy.
    //
    // Note `state.effective_profile` may by now be the uncensored profile: it is
    // a connection profile like any other and carries its own understudy, whose
    // chain then runs with `dangerous: true` so tier picks stay cleared for the
    // content.
    if crate::jsstr::js_trim(&state.full_response).is_empty() {
        if let (Some(repos), Some(caps)) = (repos, fallback_context) {
            // v4 walks `state.effectiveProfile`, which IS the whole profile
            // object; v5's state carries the four-field `EffectiveProfile`, so
            // the row is re-read by id. A row that has gone (deleted mid-turn,
            // or an unreadable table) has no chain — say so rather than
            // silently answering the two-step flags.
            let effective_id = state
                .effective_profile
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_else(|| connection_profile.id.clone());
            match repos.find_by_id(&effective_id) {
                None => tracing::warn!(
                    target: "quilltap::failover",
                    chat_id = %chat_id,
                    profile_id = %effective_id,
                    "[Failover] No fallback chain: the effective profile's row is unavailable"
                ),
                Some(failed) => {
                    let chain_result = attempt_empty_response_chain_fallback(
                        provider,
                        sink,
                        WalkFallbackChainOptions {
                            state,
                            repos,
                            failed,
                            context: FallbackContext {
                                user_id: user_id.clone(),
                                purpose: FallbackPurpose::Chat,
                                // An uncensored reroute already happened, or the
                                // content was flagged: either way a stand-in must
                                // be cleared for this content.
                                dangerous: caps.dangerous
                                    || flags.uncensored_retry_attempted
                                    || content_was_flagged_dangerous,
                                needs_vision: caps.needs_vision,
                                needs_tools: caps.needs_tools,
                                already_tried: tried_profile_ids.clone(),
                            },
                            params: params.clone(),
                            chat_id: chat_id.clone(),
                            character_id: character_id.clone(),
                            character_name: character_name.clone(),
                        },
                        log,
                    )
                    .await;
                    flags.chain_fallback_attempted = true;
                    flags.chain_attempts = chain_result.attempts;
                }
            }
        }
    }

    flags
}

/// v4 `getEmptyResponseReason` — the five exact user-facing strings, plus (v4
/// `a14a1811`, bug 93) the moderation first branch: a provider that NAMED its
/// refusal in the finish reason gets its own sentence ahead of every
/// inference-from-an-empty-body sentence below, with the uncensored-retry
/// suffix appended when that retry also came back empty.
///
/// `chain_attempts` (v4 `65f5021c8`) are the chain walk's failed attempts, when
/// one was walked. The understudies get named FIRST when there were any: "and
/// the stand-ins failed too" is the part that tells the user where to look, and
/// it would be lost inside the generic advice below. v4 slices off the FIRST
/// attempt — that one is the profile the sentence is already about.
pub fn get_empty_response_reason(
    uncensored_retry_attempted: bool,
    same_provider_retry_attempted: bool,
    content_was_flagged_dangerous: bool,
    chain_attempts: &[FallbackAttempt],
    finish_reason: Option<&str>,
    provider: Option<&str>,
    model_name: Option<&str>,
) -> String {
    let understudies = chain_attempts.iter().skip(1).collect::<Vec<_>>();
    let understudy_roll = if understudies.is_empty() {
        String::new()
    } else {
        format!(
            " The fallback chain was tried as well: {}.",
            understudies
                .iter()
                .map(|a| format!("{} ({})", a.profile_name, a.trigger))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    // A provider that named its refusal outright gets to say so. Everything
    // below this point is inference from an empty body; this is testimony, and
    // it changes the advice — "try resending" is wrong for a moderation stop
    // (bug 93). v4: `provider ?? 'The provider'`, `modelName ?? 'model'`.
    if let Some(refusal) = crate::moderation_finish_reason::describe_moderation_refusal(
        finish_reason,
        provider.unwrap_or("The provider"),
        model_name.unwrap_or("model"),
    ) {
        if uncensored_retry_attempted {
            return format!(
                "{refusal} An uncensored provider was tried as well and also returned \
                 empty.{understudy_roll}"
            );
        }
        return format!("{refusal}{understudy_roll}");
    }

    if uncensored_retry_attempted && same_provider_retry_attempted {
        return format!(
            "The AI model returned an empty response after retrying, and an uncensored provider \
             also returned empty. This may indicate the content was filtered by both \
             providers.{understudy_roll}"
        );
    }
    if uncensored_retry_attempted {
        return format!(
            "The AI model returned an empty response, and retrying with an uncensored provider \
             also returned empty. This may indicate the content was filtered by both \
             providers.{understudy_roll}"
        );
    }
    if content_was_flagged_dangerous {
        return format!(
            "The AI model returned an empty response, likely because the Concierge flagged this \
             content as dangerous and the provider refused to generate a response. Consider \
             enabling Auto-Route mode in the Concierge settings to automatically reroute \
             dangerous content to an uncensored provider.{understudy_roll}"
        );
    }
    if same_provider_retry_attempted {
        return format!(
            "The AI model returned an empty response twice. This may be a temporary issue with \
             the provider. Please try resending your message.{understudy_roll}"
        );
    }
    format!(
        "The AI model returned an empty response. This is a known issue with some providers. \
         Please try resending your message.{understudy_roll}"
    )
}

/// v4 `restreamInto` — re-stream a response into the mutable [`StreamingState`],
/// applying reasoning / content / done exactly as the primary loop does (the
/// `sending → streaming` flip, reasoning flush, content accumulation, terminal
/// capture). A mid-stream error surfaces as `Err`.
///
/// `profile` is the connection profile this retry streams against (same-provider =
/// `state.effectiveProfile`; uncensored = the rerouted profile); its provider +
/// model name feed the wire AND the terminal `CHAT_MESSAGE` `llm_logs` row when
/// `log` is set (v4's `restreamInto` passes `userId` → its wrapper logs). Like v4's
/// wrapper, the log uses the content / usage / cache / rawProviderUsage accumulated
/// by THIS call — not the whole `state.full_response`.
/// ## The two params v4's `restreamInto` does NOT forward
///
/// v4 builds its `streamMessage` call by hand (`provider-failover.service.ts`),
/// naming only `messages` / `connectionProfile` / `apiKey` / `modelParams` /
/// `tools` / `useNativeWebSearch` / `userId` / `messageId` / `chatId` — plus,
/// since `65f5021c8`, `characterId` and an OPTIONAL `stop`. Two keys it never
/// forwards, and both matter:
///
/// - **`previousResponseId`.** v4's own comment: *it is an OpenAI Responses-API
///   chaining token, and handing it to a different account — never mind a
///   different provider — is meaningless at best.* Cleared unconditionally
///   here, on every leg.
/// - **`stop`.** The empty-response callers pass none, so a same-provider retry
///   and an uncensored reroute go out with NO stop sequences even when the
///   primary sent them; the fallback CHAIN passes the primary's, so a
///   pseudo-tool profile's framing survives the swap.
///
/// v5 cloned the primary's whole `StreamParams` for the failover legs, so it
/// carried BOTH keys where v4 carries neither — a pre-existing divergence the
/// tier-3 corpus cannot see, because its canned key is
/// `provider|model|temperature|messages`. Fixed here with `stop` as an explicit
/// argument (`None` = v4's empty-response shape) and pinned by
/// `restream_clears_the_chaining_token_and_honours_the_stop_argument`.
#[allow(clippy::too_many_arguments)]
async fn restream_into<P, S>(
    provider: &P,
    state: &mut StreamingState,
    sink: &S,
    profile: &EffectiveProfile,
    params: &StreamParams,
    stop: Option<&[String]>,
    character_name: &str,
    character_id: &str,
    log: Option<&StreamLogCtx<'_>>,
) -> Result<(), StreamError>
where
    P: StreamingCompletionProvider,
    S: EventSink,
{
    // v4's hand-built call names neither key; see the doc above.
    let params = &{
        let mut p = params.clone();
        p.previous_response_id = None;
        p.stop = stop.map(<[String]>::to_vec).unwrap_or_default();
        p
    };
    // v4's wrapper accumulates the content + tracks the LAST non-null usage /
    // cacheUsage / rawProviderUsage across chunks for the terminal log row.
    let mut accumulated = String::new();
    let mut last_usage: Option<StreamUsage> = None;
    let mut last_cache_usage: Option<StreamCacheUsage> = None;
    let mut last_raw_provider_usage: Option<Value> = None;

    let mut rx = provider
        .stream_message(&profile.provider, profile.base_url.as_deref(), params)
        .await;
    while let Some(item) = rx.recv().await {
        let chunk = item?;
        apply_reasoning_chunk(state, &chunk, sink);
        if chunk.usage.is_some() {
            last_usage = chunk.usage;
        }
        if chunk.cache_usage.is_some() {
            last_cache_usage = chunk.cache_usage;
        }
        if chunk
            .raw_provider_usage
            .as_ref()
            .is_some_and(|v| v.is_object())
        {
            last_raw_provider_usage = chunk.raw_provider_usage.clone();
        }
        if !chunk.content.is_empty() {
            if !state.has_started_streaming {
                sink.emit(ChatEvent::status(StatusPayload {
                    stage: "streaming".into(),
                    message: format!("{character_name} is responding..."),
                    tool_name: None,
                    character_name: Some(character_name.to_string()),
                    character_id: Some(character_id.to_string()),
                }));
                state.has_started_streaming = true;
            }
            flush_reasoning_segment(state);
            accumulated.push_str(&chunk.content);
            state.full_response.push_str(&chunk.content);
            sink.emit(ChatEvent::content(chunk.content.clone()));
        }
        if chunk.done {
            state.usage = chunk.usage;
            state.cache_usage = chunk.cache_usage;
            // v4 `restreamInto`: `state.attachmentResults = chunk.attachmentResults
            // || null` — the RETRY's ledger replaces the failed attempt's, and a
            // retry that reports none CLEARS the stale one (found by the a14a1811
            // §3 review: bug 94 gave the ledger its first reader, which made the
            // missing overwrite user-visible as a warning the retry never produced).
            state.attachment_results = crate::services::primary_stream::attachment_results_to_value(
                &chunk.attachment_results,
            );
            state.raw_response = chunk.raw_response.clone();
            if let Some(ts) = chunk.thought_signature.as_ref() {
                if !ts.is_empty() {
                    state.thought_signature = Some(ts.clone());
                }
            }
            flush_reasoning_segment(state);

            // v4's wrapper logs on `chunk.done` (userId is passed by restreamInto).
            if let Some(log) = log {
                log_chat_message_call(
                    log,
                    profile,
                    params,
                    accumulated.clone(),
                    last_usage,
                    last_cache_usage,
                    last_raw_provider_usage.clone(),
                    chunk.raw_response.clone(),
                )
                .await;
            }
        }
    }
    Ok(())
}

// ============================================================================
// FALLBACK CHAINS (v4 `65f5021c8`)
// ============================================================================

/// Clear the streaming buffers so a re-stream *substitutes* a response instead
/// of continuing one.
///
/// [`restream_into`] appends, which is right when it is retrying a call that
/// produced nothing. A chain walk is different: the failed attempt may have
/// left reasoning in the buffers before it died, and the understudy's answer
/// must not be glued onto the corpse of the one before it.
///
/// Reasoning is display-only and the client replaces its buffer wholesale on
/// each cumulative update, so clearing it server-side and re-streaming lands
/// correctly on the client too.
fn reset_streaming_buffers_for_swap(state: &mut StreamingState) {
    state.full_response = String::new();
    state.usage = None;
    state.cache_usage = None;
    state.attachment_results = None;
    state.raw_response = None;
    state.thought_signature = None;
    // v4 assigns `state.reasoningContent = ''`, not `undefined` — so a reset
    // state is DISTINGUISHABLE from one that never carried reasoning. v5's
    // readers treat `Some("")` and `None` identically (`message_finalizer`
    // filters on `!s.is_empty()`), but the observable value is v4's.
    state.reasoning_content = Some(String::new());
    state.reasoning_segments = Vec::new();
    state.reasoning_flushed_len = 0;
}

/// Everything a chain walk needs beyond the engine's own inputs.
pub struct WalkFallbackChainOptions<'a, R: FallbackChainRepos> {
    pub state: &'a mut StreamingState,
    /// Reads only — the chain build plus each understudy's key resolution.
    pub repos: &'a R,
    /// The profile whose call just failed, as a full row.
    ///
    /// v4 walks `state.effectiveProfile`, which IS the whole
    /// `ConnectionProfile`; v5's `StreamingState` carries the four-field
    /// [`EffectiveProfile`], so the caller supplies the row it already holds
    /// (the orchestrator's `effective_profile_row`) or re-reads it by id. A
    /// caller that can produce neither has no chain to walk.
    pub failed: FallbackProfile,
    /// Everything a stand-in needs from this call.
    ///
    /// `context.already_tried` matters more than it looks: the empty-response
    /// path may have burned the uncensored profile before the chain is even
    /// reached, and a chain that re-offered it would spend a whole attempt
    /// re-learning what it already knows. The failing profile itself is added
    /// by the walk, so callers list only the *extra* ones.
    pub context: FallbackContext,
    /// The primary call's stream params. Each candidate's `model` replaces the
    /// primary's; `stop` is forwarded (see [`restream_into`]).
    pub params: StreamParams,
    pub chat_id: String,
    pub character_id: String,
    pub character_name: String,
}

/// The outcome of walking a chain to its end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FallbackChainResult {
    /// True when some understudy answered; `state` now holds their response.
    pub recovered: bool,
    /// Every failed attempt in order, starting with the profile that opened the
    /// chain. Empty when no chain was walked at all.
    pub attempts: Vec<FallbackAttempt>,
    /// Whether the chain offered an auto-picked tier candidate. Feeds the
    /// "no tier replacement qualified" half of the user-facing summary.
    pub tier_pick_was_offered: bool,
}

/// Walk a profile's fallback chain, streaming the first answer that arrives into
/// `state`.
///
/// `opening_failure` is the attempt that sent us here — the primary's error, or
/// its empty response. It leads the attempt trail and seeds the loop guard, so
/// the chain never re-offers the profile that just failed.
///
/// On success `state.effective_profile` / `effective_api_key` are swapped to the
/// understudy: that pair is the seam every downstream stage reads, so
/// finalization, token accounting and the tool loop all attribute the message to
/// whoever actually wrote it. On exhaustion the buffers are left empty — a stray
/// fragment from a dead understudy is not this character's words.
async fn walk_fallback_chain<P, S, R>(
    provider: &P,
    sink: &S,
    opts: WalkFallbackChainOptions<'_, R>,
    opening_failure: FallbackAttempt,
    log: Option<FailoverLogCtx<'_>>,
) -> FallbackChainResult
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: FallbackChainRepos,
{
    let WalkFallbackChainOptions {
        state,
        repos,
        failed,
        context,
        params,
        chat_id,
        character_id,
        character_name,
    } = opts;

    let mut attempts = vec![opening_failure];

    // v4's `restreamInto` always passes `userId`, so its wrapper logs a
    // `CHAT_MESSAGE` row per attempt — with no `characterId` (the call site
    // passes none). Same construction as the empty-response legs above.
    let none_ctx = crate::services::llm_logging::LogContext::none();
    let stream_log = log
        .filter(|_| !context.user_id.is_empty())
        .map(|l| StreamLogCtx {
            db: l.db,
            user_id: &context.user_id,
            chat_id: &chat_id,
            message_id: l.message_id,
            // v4 `65f5021c8` passes `characterId` on every `restreamInto` call,
            // the chain legs included.
            character_id: Some(&character_id),
            log_context: &none_ctx,
            started_at_ms: crate::clock::now_unix_ms(),
        });

    let mut build_context = context.clone();
    build_context.already_tried.push(failed.id.clone());
    let chain = build_fallback_chain(&failed, repos, &build_context);

    let tier_pick_was_offered = chain
        .iter()
        .any(|c| c.kind == FallbackCandidateKind::TierPick);

    for candidate in &chain {
        let understudy = &candidate.profile;

        let api_key = match repos.resolve_api_key(understudy) {
            Ok(key) => key,
            Err(reason) => {
                tracing::warn!(
                    target: "quilltap::failover",
                    chat_id = %chat_id,
                    understudy_id = %understudy.id,
                    understudy_name = %understudy.name,
                    reason = reason.as_str(),
                    "[Failover] Understudy has no usable API key; moving on"
                );
                attempts.push(record_attempt(
                    understudy,
                    FallbackTrigger::Auth,
                    Some(reason.as_str()),
                ));
                continue;
            }
        };

        sink.emit(ChatEvent::status(StatusPayload {
            stage: "failing-over".into(),
            message: format!(
                "{} is standing in for {}...",
                understudy.name, character_name
            ),
            tool_name: None,
            character_name: Some(character_name.clone()),
            character_id: Some(character_id.clone()),
        }));

        reset_streaming_buffers_for_swap(state);

        let effective = EffectiveProfile {
            id: understudy.id.clone(),
            provider: understudy.provider.clone(),
            model_name: understudy.model_name.clone(),
            base_url: understudy.base_url.clone(),
        };
        // v4's `streamMessage` takes its model from `connectionProfile.modelName`,
        // so a swap changes the model too.
        let mut re_params = params.clone();
        re_params.model = understudy.model_name.clone();

        if let Err(understudy_error) = restream_into(
            provider,
            state,
            sink,
            &effective,
            &re_params,
            Some(&params.stop),
            &character_name,
            &character_id,
            stream_log.as_ref(),
        )
        .await
        {
            let understudy_trigger =
                classify_fallback_trigger(FallbackError::message(&understudy_error.message))
                    .unwrap_or(FallbackTrigger::ProviderError);
            attempts.push(record_attempt(
                understudy,
                understudy_trigger,
                Some(&understudy_error.message),
            ));
            tracing::warn!(
                target: "quilltap::failover",
                chat_id = %chat_id,
                understudy_id = %understudy.id,
                understudy_name = %understudy.name,
                provider = %understudy.provider,
                model = %understudy.model_name,
                kind = candidate.kind.as_str(),
                trigger = understudy_trigger.as_str(),
                error = %understudy_error.message,
                "[Failover] Understudy also failed"
            );
            continue;
        }

        if crate::jsstr::js_trim(&state.full_response).is_empty() {
            attempts.push(record_attempt(
                understudy,
                FallbackTrigger::EmptyResponse,
                Some("empty response"),
            ));
            tracing::warn!(
                target: "quilltap::failover",
                chat_id = %chat_id,
                understudy_id = %understudy.id,
                understudy_name = %understudy.name,
                kind = candidate.kind.as_str(),
                "[Failover] Understudy returned an empty response"
            );
            continue;
        }

        state.effective_profile = Some(effective);
        state.effective_api_key = api_key;

        tracing::info!(
            target: "quilltap::failover",
            chat_id = %chat_id,
            understudy_id = %understudy.id,
            understudy_name = %understudy.name,
            provider = %understudy.provider,
            model = %understudy.model_name,
            kind = candidate.kind.as_str(),
            response_length = state.full_response.len(),
            failed_attempts_before = attempts.len(),
            "[Failover] Understudy answered"
        );

        return FallbackChainResult {
            recovered: true,
            attempts,
            tier_pick_was_offered,
        };
    }

    tracing::error!(
        target: "quilltap::failover",
        chat_id = %chat_id,
        profile_id = %failed.id,
        purpose = %context.purpose,
        tier_pick_was_offered,
        attempts = ?attempts
            .iter()
            .map(|a| (a.profile_name.as_str(), a.provider.as_str(), a.trigger.as_str()))
            .collect::<Vec<_>>(),
        "[Failover] Fallback chain exhausted"
    );

    reset_streaming_buffers_for_swap(state);

    FallbackChainResult {
        recovered: false,
        attempts,
        tier_pick_was_offered,
    }
}

/// Walk the effective profile's fallback chain after a HARD error.
///
/// Returns `recovered: false` with no attempts when the failure is not
/// fallback-eligible — a token-limit overrun, a tool-unsupported rejection, one
/// of our own validation bugs — so the caller rethrows exactly as it did before
/// this feature existed.
///
/// **Only runs before the first content chunk.** Once prose has reached the
/// user, a partial answer is worth more than a substituted one: the client has
/// already rendered the text, and `preserve_partial_on_error` will save it with
/// an OOC marker explaining the abrupt end. Nearly every hard error worth
/// failing over for — auth, rate limit, model-missing, connection refused —
/// arrives before a single token does. (`restream_into` APPENDS and the SSE
/// protocol has no reset, so substituting mid-stream would show the user a
/// truncated fragment with the understudy's answer glued on.)
pub async fn attempt_hard_error_failover<P, S, R>(
    provider: &P,
    sink: &S,
    opts: WalkFallbackChainOptions<'_, R>,
    error: &FallbackError<'_>,
    log: Option<FailoverLogCtx<'_>>,
) -> FallbackChainResult
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: FallbackChainRepos,
{
    let Some(trigger) = classify_fallback_trigger(*error) else {
        tracing::debug!(
            target: "quilltap::failover",
            chat_id = %opts.chat_id,
            profile_id = %opts.failed.id,
            error = %error.message,
            "[Failover] Error is not fallback-eligible; leaving it to the caller"
        );
        return FallbackChainResult::default();
    };

    if opts.state.has_started_streaming {
        tracing::info!(
            target: "quilltap::failover",
            chat_id = %opts.chat_id,
            profile_id = %opts.failed.id,
            trigger = trigger.as_str(),
            partial_length = opts.state.full_response.len(),
            "[Failover] Skipping chain: content already reached the user"
        );
        return FallbackChainResult::default();
    }

    tracing::warn!(
        target: "quilltap::failover",
        chat_id = %opts.chat_id,
        profile_id = %opts.failed.id,
        provider = %opts.failed.provider,
        model = %opts.failed.model_name,
        trigger = trigger.as_str(),
        purpose = %opts.context.purpose,
        error = %error.message,
        "[Failover] Primary call failed; walking the fallback chain"
    );

    let opening = record_attempt(&opts.failed, trigger, Some(error.message));
    walk_fallback_chain(provider, sink, opts, opening, log).await
}

/// Walk the effective profile's fallback chain after an *empty* response.
///
/// Runs last in the empty-response order — after the same-profile retry and, in
/// Auto-Route territory, after the uncensored reroute. Those two come first on
/// purpose: an empty body is usually transient, and when it isn't it is usually
/// a refusal, which is a content problem the uncensored profile exists to
/// answer. Only once both have come back empty is it worth concluding the route
/// itself is no good and calling for the understudy.
///
/// Note this runs against `state.effective_profile`, which by then may be the
/// *uncensored* profile rather than the one the chat started with — that profile
/// carries its own `fallbackProfileId`/`allowTierFallback`, and its chain runs
/// with `dangerous: true` so tier picks stay cleared for the content.
pub async fn attempt_empty_response_chain_fallback<P, S, R>(
    provider: &P,
    sink: &S,
    opts: WalkFallbackChainOptions<'_, R>,
    log: Option<FailoverLogCtx<'_>>,
) -> FallbackChainResult
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: FallbackChainRepos,
{
    tracing::warn!(
        target: "quilltap::failover",
        chat_id = %opts.chat_id,
        profile_id = %opts.failed.id,
        provider = %opts.failed.provider,
        model = %opts.failed.model_name,
        purpose = %opts.context.purpose,
        dangerous = opts.context.dangerous,
        "[Failover] Empty response survived local recovery; walking the fallback chain"
    );

    let opening = record_attempt(
        &opts.failed,
        FallbackTrigger::EmptyResponse,
        Some("empty response"),
    );
    walk_fallback_chain(provider, sink, opts, opening, log).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stream::{CannedStreamingProvider, StreamChunk};
    use crate::services::chat_events::RecordingSink;

    fn profile(id: &str, provider: &str) -> EffectiveProfile {
        EffectiveProfile {
            id: id.into(),
            provider: provider.into(),
            model_name: "m".into(),
            base_url: None,
        }
    }

    fn base_params() -> StreamParams {
        StreamParams {
            messages: vec![crate::model::stream::StreamMessage::user("hi")],
            model: "m".into(),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: None,
            tools: None,
            web_search_enabled: false,
            profile_parameters: None,
            cache_key: None,
            previous_response_id: None,
            stop: Vec::new(),
            request_timeout_ms: None,
        }
    }

    struct NoRouter;
    impl DangerousContentRouter for NoRouter {
        async fn resolve(
            &self,
            original_profile: &EffectiveProfile,
            original_api_key: &str,
            _settings: &DangerSettings,
            _user_id: &str,
        ) -> RouteResult {
            RouteResult {
                rerouted: false,
                connection_profile: original_profile.clone(),
                api_key: original_api_key.to_string(),
            }
        }
    }

    struct UncensoredRouter {
        profile: EffectiveProfile,
        key: String,
    }
    impl DangerousContentRouter for UncensoredRouter {
        async fn resolve(
            &self,
            _original_profile: &EffectiveProfile,
            _original_api_key: &str,
            _settings: &DangerSettings,
            _user_id: &str,
        ) -> RouteResult {
            RouteResult {
                rerouted: true,
                connection_profile: self.profile.clone(),
                api_key: self.key.clone(),
            }
        }
    }

    #[tokio::test]
    async fn empty_response_reason_strings() {
        assert!(
            get_empty_response_reason(true, true, false, &[], None, None, None)
                .contains("after retrying, and an uncensored")
        );
        assert!(
            get_empty_response_reason(true, false, false, &[], None, None, None)
                .contains("retrying with an uncensored")
        );
        assert!(
            get_empty_response_reason(false, false, true, &[], None, None, None)
                .contains("Concierge flagged")
        );
        assert!(
            get_empty_response_reason(false, true, false, &[], None, None, None)
                .contains("empty response twice")
        );
        assert!(
            get_empty_response_reason(false, false, false, &[], None, None, None)
                .contains("known issue with some providers")
        );
    }

    #[tokio::test]
    async fn moderation_refusal_beats_every_inference_sentence() {
        // Bug 93 (v4 `a14a1811`): a named refusal wins over each pre-existing
        // arm, and a NON-moderation finish reason leaves them all untouched.
        for (unc, same, flagged) in [
            (false, false, false),
            (false, true, false),
            (false, false, true),
            (true, false, false),
            (true, true, false),
        ] {
            let s = get_empty_response_reason(
                unc,
                same,
                flagged,
                &[],
                Some("sensitive"),
                Some("Z_AI"),
                Some("glm-5v-turbo"),
            );
            assert!(
                s.contains("refused this turn on content grounds"),
                "moderation branch must win for ({unc},{same},{flagged})"
            );
            assert_eq!(
                s.contains("An uncensored provider was tried as well and also returned empty."),
                unc,
                "the suffix rides exactly the uncensored-retry arm"
            );
        }
        // Defaults when the caller has no provider/model at hand.
        let s = get_empty_response_reason(false, false, false, &[], Some("SAFETY"), None, None);
        assert!(s.starts_with("The provider model refused this turn"));
        // An ordinary stop falls through to the old default.
        let s = get_empty_response_reason(
            false,
            false,
            false,
            &[],
            Some("stop"),
            Some("OPENAI"),
            Some("gpt-5"),
        );
        assert!(s.contains("known issue with some providers"));
    }

    #[tokio::test]
    async fn same_provider_retry_recovers_content() {
        let params = base_params();
        let provider = CannedStreamingProvider::new().with_content_stream(
            "OPENAI",
            "m",
            Some(0.7),
            &params.messages,
            &["recovered text"],
            None,
        );
        let sink = RecordingSink::new();
        let mut state = StreamingState {
            effective_profile: Some(profile("p1", "OPENAI")),
            effective_api_key: "k".into(),
            ..Default::default()
        };
        let flags = attempt_empty_response_recovery::<
            _,
            _,
            _,
            crate::services::fallback_repos::DbFallbackRepos,
        >(
            &provider,
            &sink,
            &NoRouter,
            None,
            AttemptEmptyResponseRecoveryOptions {
                state: &mut state,
                tool_messages_length: 0,
                content_was_flagged_dangerous: false,
                danger_settings: DangerSettings {
                    mode: "OFF".into(),
                    uncensored_text_profile_id: None,
                },
                connection_profile: profile("p1", "OPENAI"),
                params,
                user_id: "u".into(),
                chat_id: "c".into(),
                character_id: "ch".into(),
                character_name: "Friday".into(),
                fallback_context: None,
            },
        )
        .await;
        assert!(flags.same_provider_retry_attempted);
        assert!(!flags.uncensored_retry_attempted);
        assert_eq!(state.full_response, "recovered text");
    }

    /// The a14a1811 §3 review's catch: v4's `restreamInto` overwrites
    /// `state.attachmentResults` from the retry's done chunk (`|| null`), so a
    /// stale ledger from the failed first attempt can never outlive a recovery.
    /// Bug 94 gave the ledger its first reader (the Salon warning toast), which
    /// is what made the missing overwrite observable.
    #[tokio::test]
    async fn restream_overwrites_the_attachment_ledger() {
        let params = base_params();
        let provider = CannedStreamingProvider::new().with_content_stream(
            "OPENAI",
            "m",
            Some(0.7),
            &params.messages,
            &["recovered text"],
            None,
        );
        let sink = RecordingSink::new();
        let mut state = StreamingState {
            effective_profile: Some(profile("p1", "OPENAI")),
            effective_api_key: "k".into(),
            // The failed first attempt reported a dropped attachment.
            attachment_results: Some(serde_json::json!({
                "sent": [],
                "failed": [{ "id": "file-1", "error": "stale" }],
            })),
            ..Default::default()
        };
        attempt_empty_response_recovery::<_, _, _, crate::services::fallback_repos::DbFallbackRepos>(
            &provider,
            &sink,
            &NoRouter,
            None,
            AttemptEmptyResponseRecoveryOptions {
                state: &mut state,
                tool_messages_length: 0,
                content_was_flagged_dangerous: false,
                danger_settings: DangerSettings {
                    mode: "OFF".into(),
                    uncensored_text_profile_id: None,
                },
                connection_profile: profile("p1", "OPENAI"),
                params,
                user_id: "u".into(),
                chat_id: "c".into(),
                character_id: "ch".into(),
                character_name: "Friday".into(),
                fallback_context: None,
            },
        )
        .await;
        // The canned retry reports no ledger — the stale one must be CLEARED,
        // exactly v4's `chunk.attachmentResults || null`.
        assert_eq!(state.attachment_results, None);
    }

    #[tokio::test]
    async fn uncensored_failover_switches_profile_on_success() {
        // Same-provider retry is skipped (flagged dangerous); the uncensored
        // provider returns content, so the effective profile switches.
        let params = base_params();
        let uncensored = profile("p2", "UNCENSORED");
        let provider = CannedStreamingProvider::new().with_content_stream(
            "UNCENSORED",
            "m",
            Some(0.7),
            &params.messages,
            &["forbidden text"],
            None,
        );
        let sink = RecordingSink::new();
        let mut state = StreamingState {
            effective_profile: Some(profile("p1", "OPENAI")),
            effective_api_key: "k".into(),
            ..Default::default()
        };
        let flags = attempt_empty_response_recovery::<
            _,
            _,
            _,
            crate::services::fallback_repos::DbFallbackRepos,
        >(
            &provider,
            &sink,
            &UncensoredRouter {
                profile: uncensored.clone(),
                key: "k2".into(),
            },
            None,
            AttemptEmptyResponseRecoveryOptions {
                state: &mut state,
                tool_messages_length: 0,
                content_was_flagged_dangerous: true,
                danger_settings: DangerSettings {
                    mode: "AUTO_ROUTE".into(),
                    uncensored_text_profile_id: Some("p2".into()),
                },
                connection_profile: profile("p1", "OPENAI"),
                params,
                user_id: "u".into(),
                chat_id: "c".into(),
                character_id: "ch".into(),
                character_name: "Friday".into(),
                fallback_context: None,
            },
        )
        .await;
        assert!(
            !flags.same_provider_retry_attempted,
            "skipped: flagged dangerous"
        );
        assert!(flags.uncensored_retry_attempted);
        assert_eq!(state.full_response, "forbidden text");
        assert_eq!(state.effective_profile.as_ref().unwrap().id, "p2");
        assert_eq!(state.effective_api_key, "k2");
    }

    #[tokio::test]
    async fn non_empty_response_is_a_noop() {
        let params = base_params();
        let provider = CannedStreamingProvider::new();
        let sink = RecordingSink::new();
        let mut state = StreamingState {
            full_response: "already answered".into(),
            effective_profile: Some(profile("p1", "OPENAI")),
            ..Default::default()
        };
        let flags = attempt_empty_response_recovery::<
            _,
            _,
            _,
            crate::services::fallback_repos::DbFallbackRepos,
        >(
            &provider,
            &sink,
            &NoRouter,
            None,
            AttemptEmptyResponseRecoveryOptions {
                state: &mut state,
                tool_messages_length: 0,
                content_was_flagged_dangerous: false,
                danger_settings: DangerSettings {
                    mode: "OFF".into(),
                    uncensored_text_profile_id: None,
                },
                connection_profile: profile("p1", "OPENAI"),
                params,
                user_id: "u".into(),
                chat_id: "c".into(),
                character_id: "ch".into(),
                character_name: "Friday".into(),
                fallback_context: None,
            },
        )
        .await;
        assert_eq!(flags, EmptyResponseRecoveryFlags::default());
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn a_terminal_done_only_stream_leaves_response_empty() {
        // Retry that yields only a done chunk (no content) → still empty.
        let params = base_params();
        let provider = CannedStreamingProvider::new().with_stream(
            "OPENAI",
            "m",
            Some(0.7),
            &params.messages,
            vec![Ok(StreamChunk::done(None))],
        );
        let sink = RecordingSink::new();
        let mut state = StreamingState {
            effective_profile: Some(profile("p1", "OPENAI")),
            ..Default::default()
        };
        let flags = attempt_empty_response_recovery::<
            _,
            _,
            _,
            crate::services::fallback_repos::DbFallbackRepos,
        >(
            &provider,
            &sink,
            &NoRouter,
            None,
            AttemptEmptyResponseRecoveryOptions {
                state: &mut state,
                tool_messages_length: 0,
                content_was_flagged_dangerous: false,
                danger_settings: DangerSettings {
                    mode: "OFF".into(),
                    uncensored_text_profile_id: None,
                },
                connection_profile: profile("p1", "OPENAI"),
                params,
                user_id: "u".into(),
                chat_id: "c".into(),
                character_id: "ch".into(),
                character_name: "Friday".into(),
                fallback_context: None,
            },
        )
        .await;
        assert!(flags.same_provider_retry_attempted);
        assert!(state.full_response.is_empty());
    }

    /// **The two params `restreamInto` does NOT forward** (v4
    /// `provider-failover.service.ts` builds its `streamMessage` call by hand).
    ///
    /// v5 cloned the primary's whole `StreamParams` for the failover legs, so it
    /// carried BOTH `previousResponseId` and `stop` where v4 carries neither on
    /// the empty-response path — a pre-existing divergence the tier-3 corpus
    /// cannot see, because its canned key is
    /// `provider|model|temperature|messages`. This is the pin: a recording
    /// provider that keeps the params it was handed.
    #[tokio::test]
    async fn restream_clears_the_chaining_token_and_honours_the_stop_argument() {
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Recorder {
            seen: Arc<Mutex<Vec<StreamParams>>>,
        }
        impl StreamingCompletionProvider for Recorder {
            fn stream_message(
                &self,
                _provider: &str,
                _base_url: Option<&str>,
                params: &StreamParams,
            ) -> impl std::future::Future<
                Output = tokio::sync::mpsc::Receiver<crate::model::stream::StreamChunkResult>,
            > + Send {
                self.seen.lock().unwrap().push(params.clone());
                let (tx, rx) = tokio::sync::mpsc::channel(4);
                async move {
                    let _ = tx
                        .send(Ok(StreamChunk {
                            content: "answered".into(),
                            ..Default::default()
                        }))
                        .await;
                    let _ = tx
                        .send(Ok(StreamChunk {
                            done: true,
                            ..Default::default()
                        }))
                        .await;
                    rx
                }
            }
        }

        let seen = Arc::new(Mutex::new(Vec::new()));
        let provider = Recorder { seen: seen.clone() };
        let sink = RecordingSink::default();
        let mut state = StreamingState::default();

        let mut params = base_params();
        params.previous_response_id = Some("resp_abc".into());
        params.stop = vec!["</tool_call>".into()];

        // The empty-response shape: v4 passes NO stop.
        restream_into(
            &provider,
            &mut state,
            &sink,
            &profile("p1", "OPENAI"),
            &params,
            None,
            "Friday",
            "ch",
            None,
        )
        .await
        .unwrap();

        // The chain shape: v4 forwards the primary's sequences.
        restream_into(
            &provider,
            &mut state,
            &sink,
            &profile("p2", "OPENAI"),
            &params,
            Some(&params.stop.clone()),
            "Friday",
            "ch",
            None,
        )
        .await
        .unwrap();

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        for (i, p) in seen.iter().enumerate() {
            assert_eq!(
                p.previous_response_id, None,
                "leg {i}: an OpenAI Responses-API chaining token must never reach \
                 a different profile — v4 never forwards it"
            );
        }
        assert!(
            seen[0].stop.is_empty(),
            "the empty-response legs go out with NO stop sequences (v4 passes none)"
        );
        assert_eq!(
            seen[1].stop,
            vec!["</tool_call>".to_string()],
            "the chain legs carry the primary's stop sequences, so a pseudo-tool \
             profile's framing survives the swap"
        );
    }

    /// The failover writes one `CHAT_MESSAGE` `llm_logs` row PER retry leg (v4's
    /// `restreamInto` logs per `streamMessage` call), against the leg's profile,
    /// with the character id (v4 `65f5021c8` added `characterId` to
    /// `restreamInto`'s call; before it those rows carried NULL) and the
    /// supplied `messageId`. Drives the both-empty scenario so both legs run.
    #[tokio::test]
    async fn failover_logs_one_chat_message_row_per_retry_leg() {
        use crate::db::runtime::{Db, DbPaths};
        use crate::db::Writer;

        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.db");
        let ll_path = dir.path().join("llm-logs.db");
        // `is_logging_enabled`'s chat_settings read errors on the missing table and
        // defaults to enabled (v4's catch → DEFAULT_LOGGING_SETTINGS).
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

        let params = base_params();
        let uncensored = EffectiveProfile {
            id: "p2".into(),
            provider: "OPENROUTER".into(),
            model_name: "dolphin".into(),
            base_url: None,
        };
        // Same-provider (ANTHROPIC/m) → empty; uncensored (OPENROUTER/dolphin) →
        // empty. Both reach `done` → both log a CHAT_MESSAGE row.
        let provider = CannedStreamingProvider::new()
            .with_stream(
                "ANTHROPIC",
                "m",
                Some(0.7),
                &params.messages,
                vec![Ok(StreamChunk::done(None))],
            )
            .with_stream(
                "OPENROUTER",
                "dolphin",
                Some(0.7),
                &params.messages,
                vec![Ok(StreamChunk::done(None))],
            );
        let sink = RecordingSink::new();
        let mut state = StreamingState {
            effective_profile: Some(profile("p1", "ANTHROPIC")),
            effective_api_key: "k".into(),
            ..Default::default()
        };
        let flags = attempt_empty_response_recovery_with_log::<
            _,
            _,
            _,
            crate::services::fallback_repos::DbFallbackRepos,
        >(
            &provider,
            &sink,
            &UncensoredRouter {
                profile: uncensored,
                key: "k2".into(),
            },
            None,
            AttemptEmptyResponseRecoveryOptions {
                state: &mut state,
                tool_messages_length: 0,
                content_was_flagged_dangerous: false,
                danger_settings: DangerSettings {
                    mode: "AUTO_ROUTE".into(),
                    uncensored_text_profile_id: Some("p2".into()),
                },
                connection_profile: profile("p1", "ANTHROPIC"),
                params,
                user_id: "u".into(),
                chat_id: "c".into(),
                character_id: "ch".into(),
                character_name: "Friday".into(),
                fallback_context: None,
            },
            Some(FailoverLogCtx {
                db: &db,
                message_id: "msg-1",
            }),
        )
        .await;
        assert!(flags.same_provider_retry_attempted);
        assert!(flags.uncensored_retry_attempted);

        #[allow(clippy::type_complexity)]
        let rows: Vec<(String, String, Option<String>, Option<String>, String)> = db
            .read_llm_logs(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT type, provider, characterId, messageId, chatId \
                     FROM llm_logs ORDER BY provider",
                )?;
                let out = stmt
                    .query_map([], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(out)
            })
            .unwrap();

        assert_eq!(rows.len(), 2, "one CHAT_MESSAGE row per retry leg");
        // Ordered by provider: ANTHROPIC (same-provider), OPENROUTER (uncensored).
        assert_eq!(rows[0].0, "CHAT_MESSAGE");
        assert_eq!(rows[0].1, "ANTHROPIC");
        assert_eq!(rows[1].1, "OPENROUTER");
        for r in &rows {
            assert_eq!(r.0, "CHAT_MESSAGE");
            assert_eq!(
                r.2.as_deref(),
                Some("ch"),
                "v4 `65f5021c8` added `characterId` to restreamInto's call — before \
                 it these rows carried NULL"
            );
            assert_eq!(r.3.as_deref(), Some("msg-1"));
            assert_eq!(r.4, "c");
        }
    }
}
