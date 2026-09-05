//! The production [`HelpContextSummaryCheck`] — v4 `triggerContextSummaryCheck`
//! (`memory-trigger.service.ts:104-135`) as the help orchestrator's async tail
//! calls it: `repos.connections.findByUserId(userId)` for the available profiles,
//! then `checkAndGenerateSummaryIfNeeded(chatId, provider, modelName, userId,
//! connectionProfile, chatSettings.cheapLLMSettings, availableProfiles)`.
//!
//! This is the same composition the Courier paste-resolver runs
//! (`courier_transport::run_summary_check`, private there): the REAL stored
//! `cheapLLMSettings` + ALL the user's profiles so a configured cheap profile
//! wins (dogfood finding #27's fix), the danger settings resolved from the
//! global sub-object + the chat (a dangerous room does the uncensored swap), and
//! the fold's episode pass LIVE through `FoldEpisodePassSeams`. It lives in the
//! core rather than the host because `cheap_llm_profile_from_value` is
//! crate-private; the host supplies the three handles.
//!
//! ⚠ LIVE: when the interchange count hits a checkpoint this runs one cheap-LLM
//! fold (and the title enqueue). The tier-3 differential never reaches it — both
//! sides use a no-op there — so its behaviour is pinned by the context-summary
//! families it composes, not by a help-chat corpus.

use serde_json::Value;

use super::orchestrator::HelpContextSummaryCheck;
use crate::db::runtime::Db;
use crate::db::{connection_profiles, DbError};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;

/// The host's summary-check seam: the spine's completion + embedding providers
/// and a logging cheap executor built for the sending user/chat.
pub struct HelpSummaryCheck<'a, C, E>
where
    C: CompletionProvider + Sync,
    E: EmbeddingProvider + Sync,
{
    pub completion: &'a C,
    pub embedding: &'a E,
    pub executor: &'a CheapLlmTaskExecutor,
}

/// Parse the stored `cheapLLMSettings` sub-object into the summary service's
/// settings, field by field (v4 passes `chatSettings.cheapLLMSettings` straight
/// through; the stored object always carries the Zod-defaulted sub-keys).
fn cheap_llm_settings_from_stored(
    settings: &Value,
) -> crate::services::context_summary::CheapLlmSettings {
    let cheap = settings.get("cheapLLMSettings");
    crate::services::context_summary::CheapLlmSettings {
        strategy: cheap
            .and_then(|c| c.get("strategy"))
            .and_then(Value::as_str)
            .unwrap_or("PROVIDER_CHEAPEST")
            .to_string(),
        user_defined_profile_id: cheap
            .and_then(|c| c.get("userDefinedProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        default_cheap_profile_id: cheap
            .and_then(|c| c.get("defaultCheapProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        fallback_to_local: cheap
            .and_then(|c| c.get("fallbackToLocal"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

impl<C, E> HelpContextSummaryCheck for HelpSummaryCheck<'_, C, E>
where
    C: CompletionProvider + Sync,
    E: EmbeddingProvider + Sync,
{
    async fn check(
        &self,
        db: &Db,
        user_id: &str,
        chat_id: &str,
        connection_profile: &Value,
        chat_settings: &Value,
        chat: &Value,
    ) -> Result<(), DbError> {
        let cheap_profile =
            crate::services::orchestrator::cheap_llm_profile_from_value(connection_profile);
        let cheap_settings = cheap_llm_settings_from_stored(chat_settings);
        let uid = user_id.to_string();
        let available =
            db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))?;
        let available_profiles: Vec<_> = available
            .iter()
            .map(crate::services::orchestrator::cheap_llm_profile_from_value)
            .collect();
        let global_danger: Option<crate::db::chat_settings::DangerousContentSettings> =
            chat_settings
                .get("dangerousContentSettings")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
        let resolved =
            crate::services::dangerous_content::resolver::resolve_dangerous_content_settings(
                global_danger,
                Some(chat),
            );
        let danger = crate::cheap_llm::DangerousContentSettings {
            mode: resolved.settings.mode.clone(),
            uncensored_text_profile_id: resolved.settings.uncensored_text_profile_id.clone(),
        };
        let seams = crate::services::context_summary::FoldEpisodePassSeams {
            db,
            embedding: self.embedding,
            completion: self.completion,
            executor: self.executor,
        };
        crate::services::context_summary::check_and_generate_summary_if_needed_with_seams(
            db,
            self.completion,
            self.executor,
            chat_id,
            &cheap_profile,
            &cheap_settings,
            &available_profiles,
            user_id,
            Some(&danger),
            None,
            true,
            &seams,
        )
        .await?;
        Ok(())
    }
}
