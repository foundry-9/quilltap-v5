//! The `CHAT_DANGER_CLASSIFICATION` job runner (v4
//! `lib/background-jobs/handlers/chat-danger-classification.ts`).
//!
//! Classifies a chat's content (context summary, else concatenated raw messages
//! truncated to 4000 chars) and persists the chat-level danger fields plus a
//! `DANGER_CLASSIFICATION` system event. Sticky once classified; bails on
//! moderation-exempt / off-duty / mode-OFF.
//!
//! The job-processor infrastructure (`ensureProcessorRunning` / the runner loop)
//! is the standing queue-service deferral — this ports the handler *function*,
//! driven directly. `getApiKeyForCheapLLMSelection` / `logLLMCall` /
//! `postConciergeDangerAnnouncement` are seams (see [`DangerAnnouncer`] +
//! [`super::gatekeeper`]).

use serde_json::Value;

use crate::chat_predicates::is_moderation_exempt_chat_type;
use crate::cheap_llm::{get_cheap_llm_provider, CheapLlmConfig, CheapLlmProfile};
use crate::clock::now_iso;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::db::{chat_settings, chats_messages_read, chats_read, connection_profiles};
use crate::jsnum::to_fixed;
use crate::model::completion::CompletionProvider;

use super::gatekeeper::{classify_content, DangerCategory, ModerationProvider};
use super::resolver::resolve_dangerous_content_settings;

/// The `CHAT_DANGER_CLASSIFICATION` job (v4 `BackgroundJob` +
/// `ChatDangerClassificationPayload`, the fields the handler reads).
#[derive(Clone, Debug)]
pub struct ChatDangerClassificationJob {
    pub id: String,
    pub user_id: String,
    pub chat_id: String,
    pub connection_profile_id: String,
}

/// Details for the Concierge danger announcement (v4
/// `postConciergeDangerAnnouncement`'s `details`).
#[derive(Clone, Debug, PartialEq)]
pub struct DangerAnnouncementDetails {
    pub score: f64,
    pub threshold: f64,
    pub categories: Vec<DangerCategory>,
    pub source: Option<String>,
    pub provider_name: Option<String>,
}

/// The danger announcement seam (v4 `postConciergeDangerAnnouncement`). Posts a
/// synthetic Concierge bubble when a chat newly flips dangerous. Default no-op
/// (the personified writer is a W4.6 deferral; the differential mocks it).
pub trait DangerAnnouncer {
    fn post_danger(&self, chat_id: &str, details: &DangerAnnouncementDetails);
}

/// A [`DangerAnnouncer`] that posts nothing — the faithful wiring until the
/// personified writer lands (W4.6).
pub struct NoDangerAnnouncer;
impl DangerAnnouncer for NoDangerAnnouncer {
    fn post_danger(&self, _chat_id: &str, _details: &DangerAnnouncementDetails) {}
}

/// v4 `handleChatDangerClassification`. Drives the classification and persists
/// the result. `moderation` / `completion` are the model boundaries;
/// `announcer` is the Concierge post seam.
pub async fn handle_chat_danger_classification<M, C, An>(
    db: &Db,
    moderation: &M,
    completion: &C,
    announcer: &An,
    job: &ChatDangerClassificationJob,
) -> Result<(), DbError>
where
    M: ModerationProvider,
    C: CompletionProvider,
    An: DangerAnnouncer,
{
    let chat_id = job.chat_id.clone();
    let user_id = job.user_id.clone();

    // Get the chat metadata.
    let Some(chat) = db.read_main(|c| chats_read::find_by_id(c, &chat_id))? else {
        return Ok(()); // Chat not found → skip.
    };

    // Moderation-exempt chat types are never classified/flagged/announced.
    if is_moderation_exempt_chat_type(chat.get("chatType").and_then(Value::as_str)) {
        return Ok(());
    }
    // Off-duty: a job may already be in the queue from before the flip — bail.
    if chat.get("conciergeOverride").and_then(Value::as_str) == Some("OFF") {
        return Ok(());
    }
    // Sticky: if already classified dangerous, never re-check.
    if chat.get("isDangerousChat").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    // Sticky: classified safe with no new messages → skip.
    let message_count = chat
        .get("messageCount")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if chat.get("isDangerousChat").and_then(Value::as_bool) == Some(false) {
        if let Some(classified_at) = chat
            .get("dangerClassifiedAtMessageCount")
            .and_then(Value::as_f64)
        {
            if message_count <= classified_at {
                return Ok(());
            }
        }
    }

    // Determine classification input: prefer context summary, else raw messages.
    let classification_input = match chat
        .get("contextSummary")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(summary) => summary.to_string(),
        None => {
            let all = db.read_main(|c| chats_messages_read::get_messages(c, &chat_id))?;
            let events: Vec<&Value> = all
                .iter()
                .filter(|m| {
                    m.get("type").and_then(Value::as_str) == Some("message")
                        && m.get("role").and_then(Value::as_str) != Some("SYSTEM")
                        && m.get("role").and_then(Value::as_str) != Some("TOOL")
                        // `systemSender == null` — absent (net-read omits NULL) or
                        // explicit null.
                        && m.get("systemSender").is_none_or(Value::is_null)
                })
                .collect();
            if events.is_empty() {
                return Ok(());
            }
            concatenate_messages(&events)
        }
    };

    // User's chat settings for the danger mode + cheap-LLM config.
    let chat_settings = db.read_main(|c| chat_settings::find_by_user_id(c, &user_id))?;

    // Resolve danger settings (no per-chat arg here — v4 checks the chat fields
    // above directly) — bail if mode is OFF.
    let global_settings = chat_settings
        .as_ref()
        .and_then(|s| s.get("dangerousContentSettings"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let resolved = resolve_dangerous_content_settings(global_settings, None);
    let danger_settings = resolved.settings;
    if danger_settings.mode == "OFF" {
        return Ok(());
    }

    // Available profiles for cheap-LLM selection.
    let available_profiles = db.read_main(|c| connection_profiles::find_by_user_id(c, &user_id))?;

    // Connection profile, falling back to the first available if it was deleted.
    let connection_profile =
        db.read_main(|c| connection_profiles::find_by_id(c, &job.connection_profile_id))?;
    let connection_profile = match connection_profile {
        Some(p) => p,
        None => match available_profiles.first() {
            Some(p) => p.clone(),
            None => return Ok(()), // No available profiles → skip.
        },
    };

    // Cheap-LLM config + selection (ollamaAvailable defaults to false, as v4's
    // 3-arg `getCheapLLMProvider` call; no plugin registry → no cheapest hint).
    let config = cheap_llm_config_from_settings(chat_settings.as_ref());
    let profiles: Vec<CheapLlmProfile> = available_profiles
        .iter()
        .map(cheap_llm_profile_from_value)
        .collect();
    let current = cheap_llm_profile_from_value(&connection_profile);
    let selection = get_cheap_llm_provider(&current, &config, &profiles, false, None);

    // Classify.
    let result = classify_content(
        moderation,
        completion,
        &classification_input,
        &selection,
        &user_id,
        &danger_settings,
        Some(&chat_id),
    )
    .await;

    // Create a DANGER_CLASSIFICATION system event FIRST (it recounts messageCount
    // and bumps token aggregates), so the message count stored below includes it —
    // otherwise the +1 would trigger an infinite re-classification loop. v4 only
    // creates the event when the classifier reported usage (the LLM path).
    if let Some(usage) = result.usage {
        let event_id = uuid::Uuid::new_v4().to_string();
        let created_at = now_iso();
        let description = format!(
            "Chat-level danger classification: {} (score: {})",
            if result.is_dangerous {
                "dangerous"
            } else {
                "safe"
            },
            to_fixed(result.score, 2)
        );
        let provider = selection.provider.clone();
        let model_name = selection.model_name.clone();
        let ec_id = chat_id.clone();
        db.write(move |writers| {
            use crate::db::chats_messages::{ChatEventInput, SystemEventInput};
            let event = ChatEventInput::System(SystemEventInput {
                id: event_id,
                system_event_type: "DANGER_CLASSIFICATION".to_string(),
                description,
                prompt_tokens: Some(usage.prompt_tokens as f64),
                completion_tokens: Some(usage.completion_tokens as f64),
                total_tokens: Some(usage.total_tokens as f64),
                provider: Some(provider),
                model_name: Some(model_name),
                estimated_cost_usd: None,
                created_at,
            });
            writers.main().chat_messages().add_message(&ec_id, &event)?;
            // v4 `updateChatTokenAggregates`: no-op when both counts are 0.
            let pt = usage.prompt_tokens as f64;
            let ct = usage.completion_tokens as f64;
            if pt != 0.0 || ct != 0.0 {
                writers
                    .main()
                    .chat_tokens()
                    .increment_token_aggregates(&ec_id, pt, ct, None, None)?;
            }
            Ok(())
        })
        .await?;
    }

    // Re-read the chat to get the updated messageCount (after the system event).
    let final_message_count = db
        .read_main(|c| chats_read::find_by_id(c, &chat_id))?
        .and_then(|c| c.get("messageCount").and_then(Value::as_f64))
        .unwrap_or(message_count);

    // Persist the classification result. v4's `chats.update` does not mint
    // `updatedAt` (danger fields only), so this raw multi-column UPDATE — which
    // sets no `updatedAt` — is byte-identical (the standalone-write pattern).
    let now = now_iso();
    let categories_json = serde_json::to_string(
        &result
            .categories
            .iter()
            .map(|c| c.category.clone())
            .collect::<Vec<_>>(),
    )
    .expect("category list serializes infallibly");
    let is_dangerous = result.is_dangerous;
    let score = result.score;
    let up_chat_id = chat_id.clone();
    db.write(move |writers| {
        writers.main().connection().execute(
            "UPDATE chats SET \
               \"isDangerousChat\" = ?1, \
               \"dangerScore\" = ?2, \
               \"dangerCategories\" = ?3, \
               \"dangerClassifiedAt\" = ?4, \
               \"dangerClassifiedAtMessageCount\" = ?5 \
             WHERE id = ?6",
            rusqlite::params![
                is_dangerous,
                score,
                categories_json,
                now,
                final_message_count,
                up_chat_id
            ],
        )?;
        Ok(())
    })
    .await?;

    // Sticky-true: reaching here means a NEW transition to dangerous, so the
    // Concierge announces exactly once per chat.
    if result.is_dangerous {
        announcer.post_danger(
            &chat_id,
            &DangerAnnouncementDetails {
                score: result.score,
                threshold: danger_settings.threshold,
                categories: result.categories.clone(),
                source: result.source.clone(),
                provider_name: result.provider_name.clone(),
            },
        );
    }

    Ok(())
}

/// v4's raw-message concatenation: `"ROLE: content\n"` per event, truncated to
/// 4000 chars (the last line is sliced to fit). Operates on Unicode scalar
/// boundaries — for the ASCII corpus this equals JS's UTF-16 `.length` /
/// `.substring` (a tracked minor seam for non-ASCII truncation).
fn concatenate_messages(events: &[&Value]) -> String {
    const MAX_INPUT_LENGTH: usize = 4000;
    let mut concatenated = String::new();
    let mut len = 0usize; // char count of `concatenated`
    for msg in events {
        let role = msg
            .get("role")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_uppercase();
        let content = msg.get("content").and_then(Value::as_str).unwrap_or("");
        let line = format!("{role}: {content}\n");
        let line_len = line.chars().count();
        if len + line_len > MAX_INPUT_LENGTH {
            let take = MAX_INPUT_LENGTH - len;
            concatenated.extend(line.chars().take(take));
            break;
        }
        concatenated.push_str(&line);
        len += line_len;
    }
    concatenated
}

/// Build the cheap-LLM [`CheapLlmConfig`] from the chat-settings net read (v4's
/// `chatSettings?.cheapLLMSettings` with `|| default` / `?? true`).
fn cheap_llm_config_from_settings(settings: Option<&Value>) -> CheapLlmConfig {
    let cls = settings.and_then(|s| s.get("cheapLLMSettings"));
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

/// Build a [`CheapLlmProfile`] from a connection-profile net-read value.
fn cheap_llm_profile_from_value(v: &Value) -> CheapLlmProfile {
    CheapLlmProfile {
        id: v
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        provider: v
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model_name: v
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        base_url: v.get("baseUrl").and_then(Value::as_str).map(str::to_string),
        is_cheap: v.get("isCheap").and_then(Value::as_bool) == Some(true),
        is_dangerous_compatible: v.get("isDangerousCompatible").and_then(Value::as_bool)
            == Some(true),
        parameters: v.get("parameters").cloned(),
        max_tokens: v.get("maxTokens").and_then(Value::as_f64),
        model_class: v
            .get("modelClass")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}
