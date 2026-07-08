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
//! `get_empty_response_reason` returns the five exact user-facing strings that
//! describe the outcome.
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
use super::primary_stream::{
    apply_reasoning_chunk, flush_reasoning_segment, log_chat_message_call, EffectiveProfile,
    StreamLogCtx, StreamingState,
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
}

/// The flags v4 `attemptEmptyResponseRecovery` returns — which retries ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmptyResponseRecoveryFlags {
    pub uncensored_retry_attempted: bool,
    pub same_provider_retry_attempted: bool,
}

/// The pieces needed to write the v4 failover `CHAT_MESSAGE` `llm_logs` rows.
///
/// v4's `restreamInto` calls `streamMessage({ …, userId, messageId, chatId })`
/// (provider-failover.service.ts:266) — so each retry leg that reaches its
/// terminal chunk logs a `CHAT_MESSAGE` row through the SAME wrapper the primary
/// stream uses (streaming.service.ts:407). The `restreamInto` call site passes NO
/// `characterId`, so those rows carry `characterId = NULL`. `userId`/`chatId` come
/// from [`AttemptEmptyResponseRecoveryOptions`]; `db` + `message_id` (v4's
/// `preGeneratedAssistantMessageId`) are supplied here.
///
/// The orchestrator spine does not yet thread this (its
/// [`AttemptEmptyResponseRecoveryOptions`] construction carries no `db` /
/// `preGeneratedAssistantMessageId`), so its failover call uses the no-logging
/// entry point [`attempt_empty_response_recovery`] — wiring the spine's failover
/// log is a spine-owner follow-up. The differential drives
/// [`attempt_empty_response_recovery_with_log`] to prove the row shape.
pub struct FailoverLogCtx<'a> {
    pub db: &'a Db,
    /// v4 `preGeneratedAssistantMessageId` (the `messageId` on the log row).
    pub message_id: &'a str,
}

/// v4 `attemptEmptyResponseRecovery` — the orchestrator entry point. Delegates to
/// [`attempt_empty_response_recovery_with_log`] with no `llm_logs` writer (the
/// spine does not yet thread the db + pre-generated message id into the failover).
pub async fn attempt_empty_response_recovery<P, S, R>(
    provider: &P,
    sink: &S,
    router: &R,
    opts: AttemptEmptyResponseRecoveryOptions<'_>,
) -> EmptyResponseRecoveryFlags
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: DangerousContentRouter,
{
    attempt_empty_response_recovery_with_log(provider, sink, router, opts, None).await
}

/// v4 `attemptEmptyResponseRecovery`, with the optional failover `CHAT_MESSAGE`
/// logging (v4's `restreamInto` logs per `streamMessage` call). Mutates `state` in
/// place.
pub async fn attempt_empty_response_recovery_with_log<P, S, R>(
    provider: &P,
    sink: &S,
    router: &R,
    opts: AttemptEmptyResponseRecoveryOptions<'_>,
    log: Option<FailoverLogCtx<'_>>,
) -> EmptyResponseRecoveryFlags
where
    P: StreamingCompletionProvider,
    S: EventSink,
    R: DangerousContentRouter,
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
    } = opts;

    // v4 gates the wrapper's log on `if (userId)`; `restreamInto` always passes a
    // (real) userId, and no `characterId`.
    let stream_log = log.filter(|_| !user_id.is_empty()).map(|l| StreamLogCtx {
        db: l.db,
        user_id: &user_id,
        chat_id: &chat_id,
        message_id: l.message_id,
        character_id: None,
    });

    let mut flags = EmptyResponseRecoveryFlags::default();

    // Not empty (or a tool loop ran) → nothing to recover.
    if !crate::jsstr::js_trim(&state.full_response).is_empty() || tool_messages_length > 0 {
        return flags;
    }

    // --- Same-provider retry (only when not flagged dangerous) ---
    if !content_was_flagged_dangerous {
        flags.same_provider_retry_attempted = true;

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

    flags
}

/// v4 `getEmptyResponseReason` — the five exact user-facing strings.
pub fn get_empty_response_reason(
    uncensored_retry_attempted: bool,
    same_provider_retry_attempted: bool,
    content_was_flagged_dangerous: bool,
) -> String {
    if uncensored_retry_attempted && same_provider_retry_attempted {
        return "The AI model returned an empty response after retrying, and an uncensored provider \
                also returned empty. This may indicate the content was filtered by both providers."
            .into();
    }
    if uncensored_retry_attempted {
        return "The AI model returned an empty response, and retrying with an uncensored provider \
                also returned empty. This may indicate the content was filtered by both providers."
            .into();
    }
    if content_was_flagged_dangerous {
        return "The AI model returned an empty response, likely because the Concierge flagged this \
                content as dangerous and the provider refused to generate a response. Consider \
                enabling Auto-Route mode in the Concierge settings to automatically reroute \
                dangerous content to an uncensored provider."
            .into();
    }
    if same_provider_retry_attempted {
        return "The AI model returned an empty response twice. This may be a temporary issue with \
                the provider. Please try resending your message."
            .into();
    }
    "The AI model returned an empty response. This is a known issue with some providers. Please try \
     resending your message."
        .into()
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
#[allow(clippy::too_many_arguments)]
async fn restream_into<P, S>(
    provider: &P,
    state: &mut StreamingState,
    sink: &S,
    profile: &EffectiveProfile,
    params: &StreamParams,
    character_name: &str,
    character_id: &str,
    log: Option<&StreamLogCtx<'_>>,
) -> Result<(), StreamError>
where
    P: StreamingCompletionProvider,
    S: EventSink,
{
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
            messages: vec![crate::model::completion::CompletionMessage::user("hi")],
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
        assert!(get_empty_response_reason(true, true, false)
            .contains("after retrying, and an uncensored"));
        assert!(
            get_empty_response_reason(true, false, false).contains("retrying with an uncensored")
        );
        assert!(get_empty_response_reason(false, false, true).contains("Concierge flagged"));
        assert!(get_empty_response_reason(false, true, false).contains("empty response twice"));
        assert!(get_empty_response_reason(false, false, false)
            .contains("known issue with some providers"));
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
        let flags = attempt_empty_response_recovery(
            &provider,
            &sink,
            &NoRouter,
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
            },
        )
        .await;
        assert!(flags.same_provider_retry_attempted);
        assert!(!flags.uncensored_retry_attempted);
        assert_eq!(state.full_response, "recovered text");
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
        let flags = attempt_empty_response_recovery(
            &provider,
            &sink,
            &UncensoredRouter {
                profile: uncensored.clone(),
                key: "k2".into(),
            },
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
        let flags = attempt_empty_response_recovery(
            &provider,
            &sink,
            &NoRouter,
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
        let flags = attempt_empty_response_recovery(
            &provider,
            &sink,
            &NoRouter,
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
            },
        )
        .await;
        assert!(flags.same_provider_retry_attempted);
        assert!(state.full_response.is_empty());
    }

    /// The failover writes one `CHAT_MESSAGE` `llm_logs` row PER retry leg (v4's
    /// `restreamInto` logs per `streamMessage` call), against the leg's profile,
    /// with `characterId = NULL` (v4's `restreamInto` passes no `characterId`) and
    /// the supplied `messageId`. Drives the both-empty scenario so both legs run.
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
        let flags = attempt_empty_response_recovery_with_log(
            &provider,
            &sink,
            &UncensoredRouter {
                profile: uncensored,
                key: "k2".into(),
            },
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
            assert_eq!(r.2, None, "restreamInto passes no characterId → NULL");
            assert_eq!(r.3.as_deref(), Some("msg-1"));
            assert_eq!(r.4, "c");
        }
    }
}
