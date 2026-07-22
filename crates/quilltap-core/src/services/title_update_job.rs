//! The `TITLE_UPDATE` job handler (v4
//! `lib/background-jobs/handlers/title-update.ts`).
//!
//! Evaluates whether a chat has outgrown its title, renames it if so, and — for
//! non-help chats — kicks the story-background generation that a fresh title
//! implies. `context_summary` enqueues one of these at each title checkpoint
//! (`shouldCheckTitleAtInterchange`), so before P4.6ao every real instance's
//! title jobs died on the runner's "recognized but not yet available" loud
//! fallback — which ALSO meant the automatic background trigger never fired.
//!
//! ## The checkpoint cursor is the thing to get right
//!
//! Almost every arm still writes `lastRenameCheckInterchange = payload
//! .currentInterchange`, and the reason is the same each time: without it,
//! `shouldCheckTitleAtInterchange` keeps crossing the same checkpoint and
//! re-fires this job on EVERY following turn. So a manually-renamed chat, an
//! unavailable cheap LLM, a failed call, and a "no rename needed" verdict all
//! burn the current checkpoint deliberately. The next checkpoint (3 → 5 → 7 →
//! 10 …) gets its own chance.
//!
//! **The one exception:** an empty visible conversation returns WITHOUT
//! advancing the cursor (v4 `title-update.ts:125-127`). Carried as-is.
//!
//! ## Failures are throws, not swallows
//!
//! Unlike the cost service next door, a missing chat / connection profile /
//! chat-settings row THROWS — i.e. fails the job, so the runner's backoff
//! retries it. Only the cheap-LLM *call* failing is absorbed (cursor + return).
//!
//! ## Model boundary (tier-3 seam)
//!
//! [`CompletionProvider`], via the ported [`CheapLlmTaskExecutor`] — the two
//! consideration tasks live in [`crate::services::context_summary::tasks`]. The
//! cost figure behind the TITLE_GENERATION system event rides
//! [`MessageCostEstimator`] (the host wires the pricing cascade; the default is
//! [`NoMessageCost`]). `now_ms` is injected (the W4.9c frozen-clock precedent) so
//! the `updatedAt` this handler writes is diffable.

use serde_json::Value;

use crate::chat_predicates::is_help_like_chat_type;
use crate::cheap_llm::{
    get_cheap_llm_provider, resolve_uncensored_cheap_llm_selection, CheapLlmConfig,
    CheapLlmProfile, CheapLlmSelection, DangerousContentSettings,
};
use crate::clock::iso_from_unix_ms;
use crate::db::background_jobs::BackgroundJob;
use crate::db::chats::ChatUpdate;
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, connection_profiles};
use crate::model::completion::CompletionProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::context_summary::tasks::{
    consider_help_chat_title_update, consider_title_update, ChatMessage, TitleConsideration,
};
use crate::services::cost_estimation::{MessageCostEstimator, NoMessageCost};
use crate::services::dangerous_content::chat_override::is_chat_active_dangerous;
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::image_profile_resolution::queue_story_background_if_enabled;
use crate::services::job_runner::{JobFuture, JobHandler, JobOutcome};

/// The decoded `TITLE_UPDATE` payload (v4 `TitleUpdatePayload`).
#[derive(Clone, Debug, Default)]
pub struct TitleUpdatePayload {
    pub chat_id: String,
    pub connection_profile_id: String,
    pub current_interchange: f64,
}

impl TitleUpdatePayload {
    fn decode(payload: &Value) -> Self {
        Self {
            chat_id: str_field(payload, "chatId"),
            connection_profile_id: str_field(payload, "connectionProfileId"),
            current_interchange: payload
                .get("currentInterchange")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        }
    }
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Build a [`CheapLlmProfile`] from a connection-profile row value.
fn cheap_llm_profile_from_value(v: &Value) -> CheapLlmProfile {
    CheapLlmProfile {
        id: str_field(v, "id"),
        provider: str_field(v, "provider"),
        model_name: str_field(v, "modelName"),
        base_url: v.get("baseUrl").and_then(Value::as_str).map(str::to_string),
        is_cheap: v.get("isCheap").and_then(Value::as_bool).unwrap_or(false),
        is_dangerous_compatible: v
            .get("isDangerousCompatible")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        parameters: v.get("parameters").cloned(),
        max_tokens: v.get("maxTokens").and_then(Value::as_f64),
        model_class: v
            .get("modelClass")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// v4's `toCheapLLMConfig` inline in the handler: the four keys it maps, with
/// `|| undefined` on the two ids (so an empty string reads as absent).
fn cheap_llm_config_from_settings(settings: &Value) -> CheapLlmConfig {
    let cls = settings.get("cheapLLMSettings");
    let get_str = |k: &str| {
        cls.and_then(|c| c.get(k))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    CheapLlmConfig {
        strategy: get_str("strategy").unwrap_or_else(|| "PROVIDER_CHEAPEST".to_string()),
        user_defined_profile_id: get_str("userDefinedProfileId"),
        default_cheap_profile_id: get_str("defaultCheapProfileId"),
        fallback_to_local: cls
            .and_then(|c| c.get("fallbackToLocal"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
    }
}

/// `repos.chats.update(chatId, { …, updatedAt })` through the single writer.
async fn write_chat(db: &Db, chat_id: &str, patch: ChatUpdate) {
    let cid = chat_id.to_string();
    let _ = db
        .write(move |writers| {
            writers.main().chats().update(&cid, &patch)?;
            Ok(())
        })
        .await;
}

/// `repos.chats.update(chatId, { lastRenameCheckInterchange, updatedAt })` — the
/// checkpoint-cursor advance every early return but one performs. See the module
/// docs for why.
async fn advance_cursor(db: &Db, chat_id: &str, current_interchange: f64, now_ms: i64) {
    write_chat(
        db,
        chat_id,
        ChatUpdate {
            last_rename_check_interchange: Some(current_interchange),
            updated_at: Some(iso_from_unix_ms(now_ms)),
            ..Default::default()
        },
    )
    .await;
}

/// v4 `handleTitleUpdate`. `Err(message)` is v4's `throw` (the runner records it
/// as `lastError` and the ported backoff decides retry-vs-dead).
pub async fn handle_title_update<C, E>(
    db: &Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    cost: &E,
    user_id: &str,
    payload: &TitleUpdatePayload,
    now_ms: i64,
) -> Result<(), String>
where
    C: CompletionProvider,
    E: MessageCostEstimator,
{
    // ── The three throwing reads ──
    let chat_id = payload.chat_id.clone();
    let chat = db
        .read_main(move |c| chats_read::find_by_id(c, &chat_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat not found: {}", payload.chat_id))?;

    // A manually-renamed chat is NEVER re-titled by the cheap LLM — but the
    // cursor still advances so this doesn't re-fire forever.
    if chat.get("isManuallyRenamed").and_then(Value::as_bool) == Some(true) {
        advance_cursor(db, &payload.chat_id, payload.current_interchange, now_ms).await;
        return Ok(());
    }

    let is_help_chat = is_help_like_chat_type(chat.get("chatType").and_then(Value::as_str));

    let profile_id = payload.connection_profile_id.clone();
    let connection_profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &profile_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "Connection profile not found: {}",
                payload.connection_profile_id
            )
        })?;

    let uid = user_id.to_string();
    let chat_settings = db
        .read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat settings not found for user: {user_id}"))?;

    let uid = user_id.to_string();
    let available_profiles: Vec<CheapLlmProfile> = db
        .read_main(move |c| connection_profiles::find_by_user_id(c, &uid))
        .map_err(|e| e.to_string())?
        .iter()
        .map(cheap_llm_profile_from_value)
        .collect();

    // ── The cheap-LLM selection ──
    // v4 has a `if (!cheapLLMSelection)` arm here that logs and advances the
    // cursor. It is DEAD given the throw above: `getCheapLLMProvider`'s
    // priority-5 fallback always yields the current profile, and we only reach
    // this line with a valid one. The port's `get_cheap_llm_provider` returns a
    // selection non-optionally for exactly that reason (the `context_summary`
    // precedent, `:478`), so there is no arm to carry — and no way to exercise
    // one, on either side.
    //
    // `ollama_available: false` + `registry_cheapest_for_current: None` follow
    // the `carina_memory_extraction` job precedent (`:271`); both feed only the
    // priority-4/5 fallbacks.
    let mut selection: CheapLlmSelection = get_cheap_llm_provider(
        &cheap_llm_profile_from_value(&connection_profile),
        &cheap_llm_config_from_settings(&chat_settings),
        &available_profiles,
        false,
        None,
    );

    // Dangerous chats route to the uncensored provider to dodge content
    // refusals; off-duty chats are explicitly opted out (inside the predicate).
    // v4 `resolveDangerousContentSettings(chatSettings, chat)`, then narrowed to
    // the slim `cheap_llm` shape the selection resolver consumes.
    let resolved_danger = resolve_dangerous_content_settings(
        global_danger_from_settings(&chat_settings),
        Some(&chat),
    )
    .settings;
    let danger_settings = DangerousContentSettings {
        mode: resolved_danger.mode,
        uncensored_text_profile_id: resolved_danger.uncensored_text_profile_id,
    };
    if is_chat_active_dangerous(Some(&chat)) {
        selection = resolve_uncensored_cheap_llm_selection(
            selection,
            true,
            Some(&danger_settings),
            &available_profiles,
        );
    }

    // ── The conversation window ──
    let cid = payload.chat_id.clone();
    let all_messages = db
        .read_main(move |c| chats_messages_read::get_messages(c, &cid))
        .map_err(|e| e.to_string())?;
    let raw: Vec<crate::chat_tasks::RawMessage> = all_messages
        .iter()
        .map(|m| crate::chat_tasks::RawMessage {
            type_: m.get("type").and_then(Value::as_str).map(str::to_string),
            role: m.get("role").and_then(Value::as_str).map(str::to_string),
            content: m.get("content").and_then(Value::as_str).map(str::to_string),
        })
        .collect();
    let visible = crate::chat_tasks::extract_visible_conversation(&raw);
    // `chatMessages.slice(-5)`.
    let start = visible.len().saturating_sub(5);
    let recent: Vec<ChatMessage> = visible[start..]
        .iter()
        .map(|m| ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            // The title tasks never render a date (v4's ChatMessage has no
            // createdAt on this path).
            created_at: None,
        })
        .collect();
    if recent.is_empty() {
        // NB: v4 returns here WITHOUT advancing the cursor — the one arm that
        // doesn't. Carried deliberately.
        return Ok(());
    }

    // `chat.contextSummary || chat.title` — JS-falsy, so an EMPTY summary falls
    // through to the title.
    let existing_context = chat
        .get("contextSummary")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| chat.get("title").and_then(Value::as_str))
        .map(str::to_string);
    let current_title = chat.get("title").and_then(Value::as_str).unwrap_or("");

    // ── The evaluation ──
    let task = if is_help_chat {
        consider_help_chat_title_update(
            executor,
            completion,
            current_title,
            &recent,
            existing_context.as_deref(),
            &selection,
        )
        .await
    } else {
        consider_title_update(
            executor,
            completion,
            current_title,
            &recent,
            existing_context.as_deref(),
            &selection,
        )
        .await
    };

    if !task.success {
        // A persistently-failing cheap LLM shouldn't re-fire every turn either.
        advance_cursor(db, &payload.chat_id, payload.current_interchange, now_ms).await;
        return Ok(());
    }

    // ── The spend trace (best-effort — v4 wraps this in its own try/catch) ──
    if let Some(usage) = task
        .usage
        .filter(|u| u.prompt_tokens > 0 || u.completion_tokens > 0)
    {
        let estimated = cost
            .estimate(
                &selection.provider,
                &selection.model_name,
                usage.prompt_tokens,
                usage.completion_tokens,
                user_id,
            )
            .await;
        use crate::services::cost_events as ce;
        ce::create_title_generation_event(
            db,
            &payload.chat_id,
            Some(ce::TokenUsage {
                prompt_tokens: Some(usage.prompt_tokens as f64),
                completion_tokens: Some(usage.completion_tokens as f64),
                total_tokens: Some(usage.total_tokens as f64),
            }),
            Some(selection.provider.clone()),
            Some(selection.model_name.clone()),
            estimated,
        )
        .await;
    }

    // ── The verdict ──
    let verdict: TitleConsideration = task.result.unwrap_or_default();
    let Some(new_title) = verdict
        .suggested_title
        .filter(|t| !t.is_empty())
        .filter(|_| verdict.needs_new_title)
    else {
        // No rename needed — but still burn the checkpoint.
        advance_cursor(db, &payload.chat_id, payload.current_interchange, now_ms).await;
        return Ok(());
    };

    write_chat(
        db,
        &payload.chat_id,
        ChatUpdate {
            title: Some(new_title.clone()),
            last_rename_check_interchange: Some(payload.current_interchange),
            updated_at: Some(iso_from_unix_ms(now_ms)),
            ..Default::default()
        },
    )
    .await;

    // Story backgrounds run for normal chats only — help chats are skipped here,
    // autonomous rooms inside the gate.
    if !is_help_chat {
        // Re-fetch: the helper must see the freshly written title (the `chat` we
        // loaded above still carries the old one).
        let cid = payload.chat_id.clone();
        if let Ok(Some(updated_chat)) = db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
            queue_story_background_if_enabled(
                db,
                user_id,
                &updated_chat,
                Some(&chat_settings),
                &new_title,
            )
            .await;
        }
    }

    Ok(())
}

/// The user's global dangerous-content settings off the chat-settings read (the
/// `carina_memory_extraction` precedent, `:381`).
fn global_danger_from_settings(
    chat_settings: &Value,
) -> Option<crate::db::chat_settings::DangerousContentSettings> {
    chat_settings
        .get("dangerousContentSettings")
        .and_then(|d| serde_json::from_value(d.clone()).ok())
}

/// The `TITLE_UPDATE` [`JobHandler`] — a payload decode around
/// [`handle_title_update`]. The host builds one per job so the executor's log
/// context carries the job's user/chat (the `StoryBackgroundJobHandler`
/// precedent).
pub struct TitleUpdateHandler<C, E = NoMessageCost> {
    pub completion: C,
    pub executor: CheapLlmTaskExecutor,
    pub cost: E,
    /// The wall clock at job time, injected so `updatedAt` is diffable (the
    /// `StoryBackgroundGenerationHandler` precedent).
    pub now_ms: i64,
}

impl<C, E> JobHandler for TitleUpdateHandler<C, E>
where
    C: CompletionProvider + Send + Sync,
    E: MessageCostEstimator + Send + Sync,
{
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let decoded = TitleUpdatePayload::decode(&payload);
            match handle_title_update(
                db,
                &self.completion,
                &self.executor,
                &self.cost,
                &job.user_id,
                &decoded,
                self.now_ms,
            )
            .await
            {
                Ok(()) => JobOutcome::Completed(None),
                Err(message) => JobOutcome::Failed(message),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn payload_decodes() {
        let p = TitleUpdatePayload::decode(&json!({
            "chatId": "c1", "connectionProfileId": "p1", "currentInterchange": 3
        }));
        assert_eq!(p.chat_id, "c1");
        assert_eq!(p.connection_profile_id, "p1");
        assert_eq!(p.current_interchange, 3.0);
    }

    #[test]
    fn config_maps_the_four_keys_with_empty_as_absent() {
        let c = cheap_llm_config_from_settings(&json!({
            "cheapLLMSettings": {
                "strategy": "USER_DEFINED",
                "userDefinedProfileId": "u1",
                "defaultCheapProfileId": "",
                "fallbackToLocal": false,
            }
        }));
        assert_eq!(c.strategy, "USER_DEFINED");
        assert_eq!(c.user_defined_profile_id.as_deref(), Some("u1"));
        // `|| undefined` — an empty string is absent, not "".
        assert_eq!(c.default_cheap_profile_id, None);
        assert!(!c.fallback_to_local);
    }

    #[test]
    fn config_defaults_when_the_bag_is_missing() {
        let c = cheap_llm_config_from_settings(&json!({}));
        assert_eq!(c.strategy, "PROVIDER_CHEAPEST");
        assert!(c.fallback_to_local);
    }
}
