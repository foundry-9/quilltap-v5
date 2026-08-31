//! The `CONTEXT_SUMMARY` job handler (v4
//! `lib/background-jobs/handlers/context-summary.ts`).
//!
//! Updates a chat's running context summary by delegating to the in-process
//! [`generate_context_summary_with_seams`] so the background path and the live
//! message-loop path share one source of truth — turn-based folds anchored on
//! `lastSummaryTurn`, never the title-check cursor
//! (`lastRenameCheckInterchange`).
//!
//! The job path is v4's REAL `generateContextSummary`, so it runs with
//! [`RealContextSummarySeams`] — the Librarian re-post, the vault mirror, the
//! relevant-conversations refresh, the cost events, AND the fold-time episode
//! pass all live (unlike the orchestrator's in-loop check, whose oracle mocks
//! the four cross-subsystem arms — see
//! [`FoldEpisodePassSeams`](super::context_summary::FoldEpisodePassSeams)).
//!
//! The scheduled danger scan enqueues these (`danger_scan.rs`, v4
//! `scheduled-danger-scan.ts`); before P4.6bj they died on the runner's loud
//! fallback.
//!
//! ## Failure shape (v4's exactly)
//!
//! A missing chat / connection profile / chat-settings row THROWS (`Err` → the
//! job FAILS, the runner's backoff retries). A summary that did not run
//! (`!success || !wasGenerated`) is a logged skip — and deliberately does NOT
//! chain. On success, the danger-classification chain enqueues at priority −2
//! when the user's resolved mode is not OFF (chain failure swallowed).

use serde_json::Value;

use crate::cheap_llm::CheapLlmProfile;
use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_read, connection_profiles};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::context_summary::{
    generate_context_summary_with_seams, CheapLlmSettings, GenerateSummaryOptions,
    RealContextSummarySeams, SummaryGenerationResult,
};
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::queue_service::enqueue_chat_danger_classification_with_priority;

/// The `CONTEXT_SUMMARY` job payload (v4 `ContextSummaryPayload`).
#[derive(Clone, Debug, Default)]
pub struct ContextSummaryPayload {
    pub chat_id: String,
    pub connection_profile_id: String,
    pub force_regenerate: bool,
}

impl ContextSummaryPayload {
    pub fn decode(payload: &Value) -> Self {
        let s = |k: &str| {
            payload
                .get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default()
        };
        Self {
            chat_id: s("chatId"),
            connection_profile_id: s("connectionProfileId"),
            // v4 `payload.forceRegenerate ?? false`.
            force_regenerate: payload
                .get("forceRegenerate")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// v4 `handleContextSummary`. `Err(message)` is v4's `throw`.
pub async fn handle_context_summary<C, E>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    user_id: &str,
    payload: &ContextSummaryPayload,
) -> Result<Option<SummaryGenerationResult>, String>
where
    C: CompletionProvider + Sync,
    E: EmbeddingProvider + Sync,
{
    // ── The three throwing reads ──
    let cid = payload.chat_id.clone();
    let chat = db
        .read_main(move |c| chats_read::find_by_id(c, &cid))
        .map_err(|e| format!("{e:?}"))?
        .ok_or_else(|| format!("Chat not found: {}", payload.chat_id))?;

    let pid = payload.connection_profile_id.clone();
    let connection_profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .map_err(|e| format!("{e:?}"))?
        .ok_or_else(|| {
            format!(
                "Connection profile not found: {}",
                payload.connection_profile_id
            )
        })?;

    let uid = user_id.to_string();
    let chat_settings = db
        .read_main(move |c| chat_settings::find_by_user_id(c, &uid))
        .map_err(|e| format!("{e:?}"))?
        .ok_or_else(|| format!("Chat settings not found for user: {user_id}"))?;

    let uid = user_id.to_string();
    let available_profiles: Vec<CheapLlmProfile> = db
        .read_main(move |c| connection_profiles::find_by_user_id(c, &uid))
        .map_err(|e| format!("{e:?}"))?
        .iter()
        .map(crate::services::image_job_common::cheap_llm_profile_from_value)
        .collect();

    // v4's `generateContextSummary` resolves danger settings internally when
    // the chat is active-dangerous (`resolveDangerousContentSettings(
    // chatSettings, chat)`); v5 lifts that resolution to the caller (the
    // GenerateSummaryOptions seam) — resolve it here the same way.
    let global_danger = chat_settings
        .get("dangerousContentSettings")
        .and_then(|d| serde_json::from_value(d.clone()).ok());
    let resolved_danger = resolve_dangerous_content_settings(global_danger, Some(&chat)).settings;

    let options = GenerateSummaryOptions {
        user_id: user_id.to_string(),
        chat_id: payload.chat_id.clone(),
        connection_profile: crate::services::image_job_common::cheap_llm_profile_from_value(
            &connection_profile,
        ),
        cheap_llm_settings: cheap_settings_from(&chat_settings),
        available_profiles,
        force_regenerate: payload.force_regenerate,
        danger_settings: Some(crate::cheap_llm::DangerousContentSettings {
            mode: resolved_danger.mode.clone(),
            uncensored_text_profile_id: resolved_danger.uncensored_text_profile_id.clone(),
        }),
        registry_cheapest_for_current: None,
        // v4 reads `connectionProfile.maxContext ?? null` for the refresh list
        // size; the CheapLlmProfile subset drops it, so thread it separately.
        connection_max_context: connection_profile
            .get("maxContext")
            .and_then(Value::as_f64)
            .map(|f| f as i64),
    };

    let seams = RealContextSummarySeams {
        db,
        embedding,
        completion,
        executor,
    };
    let result =
        generate_context_summary_with_seams(db, completion, executor, &options, &seams).await;

    if !result.success || !result.was_generated {
        // v4's warn, byte-exact, which `a1d88aa3a` extended with
        // `timedOut: result.timedOut === true`. (v5 had no counterpart to this
        // line at all — a pre-existing gap the bug-107 field brought to light.
        // `jobId` has no source here: v5's core handler takes a payload, not
        // the job row.)
        tracing::warn!(
            chat_id = %payload.chat_id,
            error = result.error.as_deref().unwrap_or(""),
            timed_out = result.timed_out,
            "[ContextSummary] Summary update did not run"
        );
        // The summary was never produced — not produced and found wanting. Fail
        // the job so it is retried and, failing that, visible (v4 `a1d88aa3a`,
        // bug 107). `throw_if_lost_to_timeout` takes a `CheapLlmTaskResult`; the
        // two fields it reads are `success` and `timed_out`, which is what v4's
        // `Pick<CheapLLMTaskResult, 'success' | 'timedOut' | 'error'>` parameter
        // says too — this is that structural type, spelled out.
        if result.timed_out && !result.success {
            return Err(
                crate::services::cheap_llm_exec::cheap_llm_task_lost_message(
                    "update-context-summary",
                    result.error.as_deref(),
                ),
            );
        }
        return Ok(Some(result));
    }

    // Chain: enqueue danger classification after a successful summary update
    // (v4 resolves WITHOUT the chat here — the user's global settings only —
    // and swallows any chain failure).
    let global_danger = chat_settings
        .get("dangerousContentSettings")
        .and_then(|d| serde_json::from_value(d.clone()).ok());
    let chain_settings = resolve_dangerous_content_settings(global_danger, None).settings;
    if chain_settings.mode != "OFF" {
        let _ = enqueue_chat_danger_classification_with_priority(
            db,
            user_id,
            &payload.chat_id,
            &payload.connection_profile_id,
            -2.0,
        )
        .await;
    }

    Ok(Some(result))
}

/// The context-summary `CheapLlmSettings` off `chatSettings.cheapLLMSettings`
/// (v4 hands the settings object straight through; the summary path reads
/// exactly these four keys — the `defaultCheapProfileId` DOES reach this
/// path, unlike the memory processor's three-key subset).
fn cheap_settings_from(chat_settings: &Value) -> CheapLlmSettings {
    let cheap = chat_settings.get("cheapLLMSettings");
    let get_str = |k: &str| {
        cheap
            .and_then(|c| c.get(k))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    CheapLlmSettings {
        strategy: get_str("strategy").unwrap_or_else(|| "PROVIDER_CHEAPEST".to_string()),
        user_defined_profile_id: get_str("userDefinedProfileId"),
        default_cheap_profile_id: get_str("defaultCheapProfileId"),
        fallback_to_local: cheap
            .and_then(|c| c.get("fallbackToLocal"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
    }
}
