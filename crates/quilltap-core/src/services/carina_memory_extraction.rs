//! The Carina memory-extraction job handler (W4.5) — v4
//! `lib/background-jobs/handlers/carina-memory-extraction.ts`.
//!
//! A Carina answer is posted as a `systemSender: 'carina'` message, which the
//! per-turn transcript builder deliberately skips — and the answerer is frequently
//! not even a participant in the chat. So the ordinary `MEMORY_EXTRACTION` path can
//! never see it. This handler is the dedicated route: it loads the posted carina
//! message, reconstructs a one-slice `TurnTranscript` (the question opens the turn,
//! the answer is the answerer's sole contribution), and runs the standard per-turn
//! extractor. With no user-controlled character in the synthetic transcript, the
//! OTHER pass finds no subjects and self-skips, so only SELF memories form.
//!
//! Ported as a service function; the job-RUNNER dispatch row is W4.8's. The
//! model boundaries are the same [`CompletionProvider`]/[`EmbeddingProvider`]/
//! [`CheapLlmTaskExecutor`] the processor consumes. The instance-settings
//! `getMemoryExtractionLimits` read and the pricing `estimateMessageCost` call are
//! injected (the limits as an input, the cost as the [`CarinaCostEstimator`] seam)
//! — the ported [`process_turn_for_memory`] is the substance.
//!
//! [`CompletionProvider`]: crate::model::completion::CompletionProvider
//! [`EmbeddingProvider`]: crate::model::embedding::EmbeddingProvider
//! [`CheapLlmTaskExecutor`]: crate::services::cheap_llm_exec::CheapLlmTaskExecutor

use std::collections::HashMap;
use std::future::Future;

use serde_json::{json, Value};

use crate::cheap_llm::DangerousContentSettings;
use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_read, connection_profiles};
use crate::memory_format::Pronouns;
use crate::memory_tasks::{TurnCharacterSlice, TurnTranscript};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::cost_events::{create_memory_extraction_event, TokenUsage};
use crate::services::dangerous_content::chat_override::is_chat_active_dangerous;
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::memory_processor::{
    process_turn_for_memory, CheapLlmSettings, MemoryExtractionLimits, TurnMemoryExtractionContext,
};

/// The `CARINA_MEMORY_EXTRACTION` job payload (v4 `CarinaMemoryExtractionPayload`).
#[derive(Debug, Clone)]
pub struct CarinaMemoryExtractionPayload {
    pub chat_id: String,
    pub carina_message_id: String,
    pub answerer_id: String,
    pub connection_profile_id: String,
}

/// The pricing seam v4's handler reaches via `estimateMessageCost` — the token
/// cost of the extraction pass, folded into the `MEMORY_EXTRACTION` system event.
/// Injected (the pricing subsystem is W4.7e). The default returns `None` (no cost
/// figure), still emitting the event with the usage totals.
pub trait CarinaCostEstimator {
    fn estimate(
        &self,
        provider: &str,
        model: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        user_id: &str,
    ) -> impl Future<Output = Option<f64>> + Send;
}

/// The default cost estimator — no cost figure (the pricing subsystem is W4.7e).
#[derive(Clone, Copy, Default)]
pub struct NoCarinaCost;

impl CarinaCostEstimator for NoCarinaCost {
    #[allow(clippy::manual_async_fn)]
    fn estimate(
        &self,
        _provider: &str,
        _model: &str,
        _prompt_tokens: i64,
        _completion_tokens: i64,
        _user_id: &str,
    ) -> impl Future<Output = Option<f64>> + Send {
        async { None }
    }
}

/// v4 `handleCarinaMemoryExtraction`. Loads the posted carina message, builds the
/// SELF-only synthetic transcript, runs [`process_turn_for_memory`], then persists
/// the debug logs onto the carina message and emits the token-tracking event.
///
/// `memory_extraction_limits` is v4's `getMemoryExtractionLimits()` (an
/// instance-settings read), injected here (the reader is not yet ported). Missing
/// profile / chat-settings mirror v4's `throw`; a missing chat / answerer / carina
/// message / empty answer is a logged skip (`Ok(())`).
#[allow(clippy::too_many_arguments)]
pub async fn handle_carina_memory_extraction<C, E, K>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    cost: &K,
    user_id: &str,
    payload: &CarinaMemoryExtractionPayload,
    memory_extraction_limits: Option<MemoryExtractionLimits>,
) -> Result<(), CarinaExtractionError>
where
    C: CompletionProvider,
    E: EmbeddingProvider,
    K: CarinaCostEstimator,
{
    // Connection profile + chat settings are hard errors (v4 throws).
    let pid = payload.connection_profile_id.clone();
    let connection_profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .map_err(|e| CarinaExtractionError(format!("{e:?}")))?
        .ok_or_else(|| {
            CarinaExtractionError(format!(
                "Connection profile not found: {}",
                payload.connection_profile_id
            ))
        })?;

    let uid = user_id.to_string();
    let chat_settings = db
        .read_main(move |c| chat_settings::find_by_user_id(c, &uid))
        .map_err(|e| CarinaExtractionError(format!("{e:?}")))?
        .ok_or_else(|| {
            CarinaExtractionError(format!("Chat settings not found for user: {user_id}"))
        })?;

    // Chat / answerer / carina message missing → logged skip (v4 returns).
    let cid = payload.chat_id.clone();
    let Some(chat) = db
        .read_main(move |c| chats_read::find_by_id(c, &cid))
        .map_err(|e| CarinaExtractionError(format!("{e:?}")))?
    else {
        return Ok(());
    };

    let aid = payload.answerer_id.clone();
    let Some(answerer) = db
        .read_main(|main| {
            db.read_mount_index(|mount| crate::db::characters_read::find_by_id(main, mount, &aid))
        })
        .map_err(|e| CarinaExtractionError(format!("{e:?}")))?
    else {
        return Ok(());
    };

    // Locate the posted carina message.
    let cid = payload.chat_id.clone();
    let all = db
        .read_main(move |c| crate::db::chats_messages_read::get_messages(c, &cid))
        .map_err(|e| CarinaExtractionError(format!("{e:?}")))?;
    let carina_message = all.iter().find(|m| {
        m.get("type").and_then(Value::as_str) == Some("message")
            && m.get("id").and_then(Value::as_str) == Some(payload.carina_message_id.as_str())
    });
    let Some(carina_message) = carina_message else {
        return Ok(());
    };

    let question = carina_message
        .get("carinaMeta")
        .and_then(|m| m.get("question"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let answer = carina_message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if answer.is_empty() {
        return Ok(());
    }
    let carina_message_id = carina_message
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // Synthetic one-slice transcript: the question opens the turn, the answer is
    // the answerer's only contribution. No user-controlled character, so the OTHER
    // pass finds no subjects and only SELF memories form.
    let answerer_id = answerer
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let answerer_name = answerer
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let answerer_pronouns = pronouns_from_value(&answerer);

    let transcript = TurnTranscript {
        turn_opener_message_id: None,
        user_message: if question.is_empty() {
            None
        } else {
            Some(question)
        },
        user_character_id: None,
        user_character_name: None,
        user_character_pronouns: None,
        character_slices: vec![TurnCharacterSlice {
            character_id: answerer_id.clone(),
            character_name: answerer_name,
            character_pronouns: answerer_pronouns,
            text: answer,
            contributing_message_ids: vec![carina_message_id.clone()],
            is_user_controlled: false,
        }],
        latest_assistant_message_id: Some(carina_message_id.clone()),
    };

    let mut participant_characters: HashMap<String, Value> = HashMap::new();
    participant_characters.insert(answerer_id.clone(), answerer.clone());

    let uid = user_id.to_string();
    let available_profiles = db
        .read_main(move |c| connection_profiles::find_by_user_id(c, &uid))
        .unwrap_or_default();

    // Global dangerous-content settings live on chat settings; resolve against the
    // chat (v4 `resolveDangerousContentSettings(chatSettings, chat).settings`), then
    // narrow to the slim `cheap_llm` shape the memory processor consumes.
    let global_danger = global_danger_from_settings(&chat_settings);
    let resolved_danger = resolve_dangerous_content_settings(global_danger, Some(&chat)).settings;
    let danger_settings = DangerousContentSettings {
        mode: resolved_danger.mode,
        uncensored_text_profile_id: resolved_danger.uncensored_text_profile_id,
    };

    // Anchor derived memories to the carina message's own timestamp (v4).
    let source_message_timestamp = carina_message
        .get("createdAt")
        .and_then(Value::as_str)
        .or_else(|| chat.get("createdAt").and_then(Value::as_str))
        .map(str::to_string);

    let ctx = TurnMemoryExtractionContext {
        transcript,
        participant_characters,
        chat_id: payload.chat_id.clone(),
        project_id: None,
        project_description: None,
        chat_context_summary: None,
        user_id: user_id.to_string(),
        connection_profile: crate::services::image_job_common::cheap_llm_profile_from_value(
            &connection_profile,
        ),
        cheap_llm_settings: cheap_settings_from(&chat_settings),
        available_profiles: Some(
            available_profiles
                .iter()
                .map(crate::services::image_job_common::cheap_llm_profile_from_value)
                .collect(),
        ),
        danger_settings: Some(danger_settings),
        is_dangerous_chat: is_chat_active_dangerous(Some(&chat)),
        memory_extraction_limits,
        source_message_timestamp,
        dry_run: false,
        // A Carina answerer in an autonomous room earns the same user-absence
        // provenance as any other extraction there.
        in_autonomous_room: chat.get("chatType").and_then(Value::as_str) == Some("autonomous"),
        registry_cheapest_for_current: None,
    };

    let result = process_turn_for_memory(db, completion, embedding, executor, &ctx).await;

    // Persist debug logs onto the carina message (best-effort).
    if !result.debug_logs.is_empty() {
        if let Some(source_id) = &result.source_message_id {
            let chat_id = payload.chat_id.clone();
            let source_id = source_id.clone();
            let updates = json!({ "debugMemoryLogs": result.debug_logs.clone() });
            let _ = db
                .write(move |writers| {
                    writers
                        .main()
                        .chat_messages()
                        .update_message(&chat_id, &source_id, &updates)
                })
                .await;
        }
    }

    // Token-tracking event mirroring the per-turn extractor (best-effort).
    if result.usage.prompt_tokens != 0 || result.usage.completion_tokens != 0 {
        let provider = connection_profile
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let model = connection_profile
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let estimated = cost
            .estimate(
                &provider,
                &model,
                result.usage.prompt_tokens,
                result.usage.completion_tokens,
                user_id,
            )
            .await;
        let _ = create_memory_extraction_event(
            db,
            &payload.chat_id,
            Some(TokenUsage {
                prompt_tokens: Some(result.usage.prompt_tokens as f64),
                completion_tokens: Some(result.usage.completion_tokens as f64),
                total_tokens: Some(result.usage.total_tokens as f64),
            }),
            Some(provider),
            Some(model),
            estimated,
        )
        .await;
    }

    Ok(())
}

/// A hard failure (v4's `throw`) — a missing connection profile or chat settings.
/// Skips (missing chat / answerer / message / empty answer) return `Ok(())`.
#[derive(Debug, Clone)]
pub struct CarinaExtractionError(pub String);

impl std::fmt::Display for CarinaExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for CarinaExtractionError {}

// ===========================================================================
// Small conversions
// ===========================================================================

fn pronouns_from_value(c: &Value) -> Option<Pronouns> {
    let p = c.get("pronouns")?;
    Some(Pronouns {
        subject: p.get("subject").and_then(Value::as_str)?.to_string(),
        object: p.get("object").and_then(Value::as_str)?.to_string(),
        possessive: p.get("possessive").and_then(Value::as_str)?.to_string(),
    })
}

/// Build the memory-processor `CheapLlmSettings` from `chatSettings.cheapLLMSettings`
/// (v4 maps exactly `strategy` / `userDefinedProfileId` / `fallbackToLocal`).
fn cheap_settings_from(chat_settings: &Value) -> CheapLlmSettings {
    let cheap = chat_settings.get("cheapLLMSettings");
    CheapLlmSettings {
        strategy: cheap
            .and_then(|c| c.get("strategy"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        user_defined_profile_id: cheap
            .and_then(|c| c.get("userDefinedProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        fallback_to_local: cheap
            .and_then(|c| c.get("fallbackToLocal"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// Extract the global `dangerousContentSettings` sub-object from chat settings for
/// [`resolve_dangerous_content_settings`]. `None` when absent or malformed (the
/// resolver then falls back to its default/off-duty branches per the chat).
fn global_danger_from_settings(
    chat_settings: &Value,
) -> Option<crate::db::chat_settings::DangerousContentSettings> {
    chat_settings
        .get("dangerousContentSettings")
        .and_then(|d| serde_json::from_value(d.clone()).ok())
}
